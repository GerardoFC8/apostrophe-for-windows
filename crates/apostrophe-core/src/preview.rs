//! Vista previa: Markdown a HTML dentro del proceso, con comrak (ADR-0003).
//!
//! Reemplaza a pandoc en la vista previa; pandoc queda solo para exportar. comrak cubre
//! CommonMark, GFM y varias extensiones con opciones. Lo que el Markdown de pandoc hacia y
//! comrak no, y que vale la pena conservar, se agrega aqui con un formateador propio:
//!
//! - Encabezado del documento con `title`, `subtitle`, `author` y `date` del bloque YAML.
//! - Imagen sola en un parrafo como `<figure>`, con el texto alternativo como pie.
//! - Atributos de encabezado `{#id .clase}`: comrak los quita del texto pero no los usa.
//!   De paso, los encabezados salen sin el enlace ancla vacio que agrega comrak.
//! - Formulas en MathML (`crate::math`), tambien las de un bloque ```` ```math ````.
//! - Superindice y subindice con espacios (`x^2 y z^3`) salen literales, como en pandoc.
//! - Bloque de codigo con el lenguaje como clase (```` ```{.python} ````).
//!
//! Antes de parsear, [`prepare`] ajusta dos cosas del texto sin cambiar sus renglones (el
//! scroll sincronizado y el esquema dependen de ellos): el cierre `...` del YAML y los
//! bloques `$$` de varios renglones.
//!
//! Las construcciones que quedan fuera (tablas de cuadricula, listas con letras, bloques
//! de versos, spans con clase) estan en la tabla de limitaciones del ADR.

use std::borrow::Cow;
use std::fmt::{self, Write};

use comrak::html::{format_node_default, render_sourcepos, ChildRendering, Context};
use comrak::nodes::{AstNode, NodeValue};
use comrak::{create_formatter, parse_document, Arena, Options};

/// Documento HTML completo de la vista previa. El frontend inserta su hoja de estilos y su
/// script antes de `</head>`, igual que hacia con la salida `--standalone` de pandoc.
///
/// Corre en un hilo con pila grande: las formulas se convierten con un parser recursivo
/// (ver `crate::math`).
pub fn render(markdown: &str) -> String {
    crate::math::with_big_stack(|| render_here(markdown))
}

fn render_here(markdown: &str) -> String {
    let options = options();
    let arena = Arena::new();
    let root = parse_document(&arena, &prepare(markdown), &options);
    let mut body = String::new();
    // Escribir en un `String` no falla: `fmt::Error` solo puede venir del destino.
    PreviewFormatter::format_document(root, &options, &mut body)
        .expect("escribir HTML en un String no falla");
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\" />\n</head>\n<body>\n{body}</body>\n</html>\n"
    )
}

/// Extensiones de comrak. Ver la seccion "Configuracion de comrak" de ADR-0003. Las comparte
/// `crate::outline`: el esquema tiene que ver los mismos titulos que la vista previa.
pub(crate) fn options() -> Options<'static> {
    let mut o = Options::default();
    let e = &mut o.extension;
    e.strikethrough = true;
    e.table = true;
    e.autolink = true;
    e.tasklist = true;
    e.superscript = true;
    e.subscript = true;
    e.footnotes = true;
    e.inline_footnotes = true;
    e.description_lists = true;
    e.front_matter_delimiter = Some("---".into());
    e.alerts = true;
    e.math_dollars = true;
    e.highlight = true;
    e.block_directive = true;
    // Con prefijo vacio comrak calcula los `id` de los encabezados. Este formateador los
    // escribe por su cuenta (ver `Heading`), pero usa el mismo `Anchorizer`.
    e.header_id_prefix = Some(String::new());
    e.header_attributes = true;
    e.fenced_code_attributes = true;
    e.link_attributes = true;
    o.parse.smart = true;
    // El HTML crudo pasa, como con pandoc. La vista previa vive en un iframe con sandbox
    // sin `allow-same-origin`.
    o.render.r#unsafe = true;
    // `data-sourcepos` en cada bloque: base del scroll sincronizado por linea.
    o.render.sourcepos = true;
    // Clases en las listas de tareas, para quitarles las viñetas solo a ellas.
    o.render.tasklist_classes = true;
    o
}

create_formatter!(PreviewFormatter, {
    NodeValue::Heading(ref heading) => |context, node, entering| {
        if entering {
            let attrs = node.data().attrs.clone();
            let id = match attrs.as_ref().and_then(|a| a.id.clone()) {
                Some(id) => id,
                None => context.anchorizer.anchorize(&node.collect_text()),
            };
            context.cr()?;
            write!(context, "<h{} id=\"", heading.level)?;
            context.escape(&id)?;
            context.write_str("\"")?;
            if let Some(classes) = attrs.as_ref().map(|a| a.classes.join(" ")) {
                if !classes.is_empty() {
                    context.write_str(" class=\"")?;
                    context.escape(&classes)?;
                    context.write_str("\"")?;
                }
            }
            render_sourcepos(context, node)?;
            context.write_str(">")?;
        } else {
            write!(context, "</h{}>", heading.level)?;
            context.lf()?;
        }
    },
    NodeValue::Paragraph => |context, node, entering| {
        let Some(caption) = figure_caption(node) else {
            return format_node_default(context, node, entering);
        };
        if entering {
            context.cr()?;
            context.write_str("<figure")?;
            render_sourcepos(context, node)?;
            context.write_str(">\n")?;
        } else {
            context.write_str("\n<figcaption>")?;
            context.escape(&caption)?;
            context.write_str("</figcaption>\n</figure>")?;
            context.lf()?;
        }
    },
    NodeValue::Math(ref math) => |context, node, entering| {
        if entering {
            write_math(context, node, &math.literal, math.display_math)?;
        }
    },
    NodeValue::CodeBlock(ref block) => |context, node, entering| {
        let lang = block.info.split_whitespace().next().unwrap_or("");
        if lang == "math" {
            if entering {
                context.cr()?;
                write_math(context, node, &block.literal, true)?;
                context.lf()?;
            }
            return Ok(ChildRendering::HTML);
        }
        // ```{.python}: comrak deja el lenguaje en los atributos y el bloque sin clase.
        let class_lang = if lang.is_empty() {
            node.data().attrs.as_ref().and_then(|a| a.classes.first().cloned())
        } else {
            None
        };
        let Some(class_lang) = class_lang else {
            return format_node_default(context, node, entering);
        };
        if entering {
            context.cr()?;
            context.write_str("<pre")?;
            render_sourcepos(context, node)?;
            context.write_str("><code class=\"language-")?;
            context.escape(&class_lang)?;
            context.write_str("\">")?;
            context.escape(&block.literal)?;
            context.write_str("</code></pre>")?;
            context.lf()?;
        }
    },
    // Pandoc no acepta espacios dentro de `^…^` ni `~…~`: en prosa como "x^2 + y^2" las
    // marcas quedan como texto. Se respeta para no convertir medio parrafo en superindice.
    NodeValue::Superscript => |context, node, entering| {
        if !node.collect_text().contains(char::is_whitespace) {
            return format_node_default(context, node, entering);
        }
        context.write_str("^")?;
    },
    NodeValue::Subscript => |context, node, entering| {
        if !node.collect_text().contains(char::is_whitespace) {
            return format_node_default(context, node, entering);
        }
        context.write_str("~")?;
    },
    NodeValue::FrontMatter(ref yaml) => |context, entering| {
        if entering {
            let block = TitleBlock::from_yaml(yaml);
            if !block.is_empty() {
                context.cr()?;
                context.write_str("<header id=\"title-block-header\">\n")?;
                for (tag, class, value) in block.lines() {
                    write!(context, "<{tag} class=\"{class}\">")?;
                    context.escape(value)?;
                    writeln!(context, "</{tag}>")?;
                }
                context.write_str("</header>")?;
                context.lf()?;
            }
        }
    },
});

/// Formula como MathML dentro de un `<span class="math …">` con su `data-sourcepos`. Si no
/// se puede convertir, el LaTeX tal cual, marcado, para que se pueda corregir. El motivo va
/// como codigo; el texto para la interfaz es cosa del frontend.
fn write_math<'a>(
    context: &mut Context,
    node: &'a AstNode<'a>,
    latex: &str,
    display: bool,
) -> fmt::Result {
    let class = if display {
        "math display"
    } else {
        "math inline"
    };
    write!(context, "<span class=\"{class}\"")?;
    render_sourcepos(context, node)?;
    context.write_str(">")?;
    match crate::math::convert(latex, display) {
        Ok(mathml) => context.write_str(&mathml)?,
        Err(error) => {
            let reason = match error {
                crate::math::MathError::TooComplex => "too-complex",
                crate::math::MathError::Invalid(_) => "invalid",
            };
            write!(
                context,
                "<code class=\"math-error\" data-reason=\"{reason}\">"
            )?;
            context.escape(latex)?;
            context.write_str("</code>")?;
        }
    }
    context.write_str("</span>")
}

/// Ajustes previos al parseo, sin cambiar la cantidad ni el orden de los renglones. Lo usa
/// tambien `crate::outline`, para ver el mismo documento que la vista previa.
///
/// - Un YAML del principio cerrado con `...` (valido en pandoc) se cierra con `---`, que es
///   lo unico que comrak reconoce.
/// - Un bloque `$$` de varios renglones pasa a ```` ```math ````. En CommonMark un renglon
///   que empieza con `+`, `-` o `>` corta el parrafo, asi que `$$\na\n+ b\n$$` terminaba
///   partido en una lista. Pandoc no lo corta. Solo se toca un `$$` solo en su renglon, con
///   su cierre igual y sin renglones en blanco ni cercas de codigo en el medio.
pub(crate) fn prepare(markdown: &str) -> Cow<'_, str> {
    let mut lines: Vec<Cow<str>> = markdown.split_inclusive('\n').map(Cow::Borrowed).collect();
    let mut changed = false;

    let content = |line: &str| line.trim_end_matches(['\n', '\r']).to_string();
    let ending = |line: &str| line[line.trim_end_matches(['\n', '\r']).len()..].to_string();

    // YAML cerrado con `...`.
    if lines.first().is_some_and(|l| content(l) == "---") {
        for line in lines.iter_mut().skip(1) {
            match content(line).as_str() {
                "---" => break,
                "..." => {
                    let end = ending(line);
                    *line = Cow::Owned(format!("---{end}"));
                    changed = true;
                    break;
                }
                _ => {}
            }
        }
    }

    // Bloques `$$` de varios renglones.
    let mut fence: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let text = content(&lines[i]);
        let trimmed = text.trim_start();
        let indent = text.len() - trimmed.len();
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
            }
            i += 1;
            continue;
        }
        if indent <= 3 && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            fence = Some(trimmed[..3].to_string());
            i += 1;
            continue;
        }
        if indent <= 3 && trimmed.trim_end() == "$$" {
            let close = (i + 1..lines.len()).find(|&j| {
                let inner = content(&lines[j]);
                inner.trim() == "$$" || inner.trim().is_empty() || is_fence(&inner)
            });
            if let Some(j) = close.filter(|&j| j > i + 1 && content(&lines[j]).trim() == "$$") {
                let (open_end, close_end) = (ending(&lines[i]), ending(&lines[j]));
                lines[i] = Cow::Owned(format!("{}```math{open_end}", &text[..indent]));
                let close_text = content(&lines[j]);
                let close_indent = close_text.len() - close_text.trim_start().len();
                lines[j] = Cow::Owned(format!("{}```{close_end}", &close_text[..close_indent]));
                changed = true;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }

    if changed {
        Cow::Owned(lines.concat())
    } else {
        Cow::Borrowed(markdown)
    }
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    line.len() - trimmed.len() <= 3 && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
}

/// Pie de figura si `paragraph` es una imagen sola, con texto alternativo, en el nivel
/// superior del documento. Es la regla `implicit_figures` de pandoc; una imagen sin texto
/// alternativo, o rodeada de texto, sigue siendo una imagen en linea.
fn figure_caption<'a>(paragraph: &'a AstNode<'a>) -> Option<String> {
    let at_top = paragraph
        .parent()
        .is_some_and(|p| matches!(p.data().value, NodeValue::Document));
    let image = paragraph.first_child()?;
    if !at_top
        || image.next_sibling().is_some()
        || !matches!(image.data().value, NodeValue::Image(_))
    {
        return None;
    }
    let alt = alt_text(image);
    (!alt.trim().is_empty()).then_some(alt)
}

/// Texto alternativo de una imagen, el mismo que comrak pone en `alt`. `collect_text`
/// no sirve: descarta el HTML en linea, y el pie de figura quedaria distinto del `alt`.
fn alt_text<'a>(image: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for node in image.descendants().skip(1) {
        match &node.data().value {
            NodeValue::Text(t) => text.push_str(t),
            NodeValue::Code(code) => text.push_str(&code.literal),
            NodeValue::HtmlInline(html) => text.push_str(html),
            NodeValue::SoftBreak | NodeValue::LineBreak => text.push(' '),
            _ => {}
        }
    }
    text
}

/// Lo que el encabezado del documento toma del bloque YAML. Solo claves de primer nivel
/// con un texto, o una lista de textos en `author`: el resto del YAML no se muestra, igual
/// que con pandoc.
#[derive(Debug, Default, PartialEq)]
struct TitleBlock {
    title: Option<String>,
    subtitle: Option<String>,
    authors: Vec<String>,
    date: Option<String>,
}

impl TitleBlock {
    fn from_yaml(front_matter: &str) -> Self {
        let mut block = Self::default();
        let lines: Vec<&str> = front_matter.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            i += 1;
            let trimmed = line.trim();
            if line.starts_with(char::is_whitespace) || trimmed.starts_with('#') {
                continue; // valor anidado de otra clave, o comentario
            }
            let Some((key, raw)) = trimmed.split_once(':') else {
                continue;
            };
            // Los renglones con sangria que siguen a la clave: elementos de una lista
            // (`- Ana`) o el texto de un bloque `|` / `>`.
            let nested_start = i;
            while i < lines.len()
                && (lines[i].trim().is_empty() || lines[i].starts_with(char::is_whitespace))
            {
                i += 1;
            }
            let nested = &lines[nested_start..i];
            let value = scalar(raw);
            let text = if matches!(value, "|" | "|-" | ">" | ">-") {
                // Bloque literal o plegado: para un titulo, una sola linea.
                let joined = nested
                    .iter()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                non_empty(&joined)
            } else {
                non_empty(value)
            };
            match key.trim() {
                "title" => block.title = text,
                "subtitle" => block.subtitle = text,
                "date" => block.date = text,
                "author" => {
                    if let Some(list) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                        for item in list.split(',') {
                            push_text(&mut block.authors, item);
                        }
                    } else if value.is_empty() {
                        for item in nested.iter().filter_map(|l| l.trim().strip_prefix("- ")) {
                            push_text(&mut block.authors, item);
                        }
                    } else if let Some(author) = text {
                        block.authors.push(author);
                    }
                }
                _ => {}
            }
        }
        block
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Etiqueta, clase y texto de cada linea, en el orden y con las clases de pandoc: asi
    /// la hoja de estilos de la vista previa sigue sirviendo.
    fn lines(&self) -> Vec<(&'static str, &'static str, &str)> {
        let mut lines = Vec::new();
        if let Some(title) = &self.title {
            lines.push(("h1", "title", title.as_str()));
        }
        if let Some(subtitle) = &self.subtitle {
            lines.push(("p", "subtitle", subtitle.as_str()));
        }
        for author in &self.authors {
            lines.push(("p", "author", author.as_str()));
        }
        if let Some(date) = &self.date {
            lines.push(("p", "date", date.as_str()));
        }
        lines
    }
}

fn push_text(list: &mut Vec<String>, value: &str) {
    if let Some(text) = non_empty(scalar(value)) {
        list.push(text);
    }
}

/// Valor escalar de YAML: sin espacios alrededor, sin comillas y sin un comentario final
/// (` # …`, solo fuera de comillas).
fn scalar(raw: &str) -> &str {
    let value = raw.trim();
    if let Some(quote) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        // Hasta la comilla de cierre: lo que siga (un comentario) se descarta.
        return match value[1..].find(quote) {
            Some(end) => &value[1..1 + end],
            None => value,
        };
    }
    let value = match value.find(" #") {
        Some(at) => value[..at].trim_end(),
        None => value,
    };
    unquote(value)
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// Quita un par de comillas simples o dobles que rodeen el valor.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Solo el `<body>`, para que las aserciones no dependan de la plantilla.
    fn body(markdown: &str) -> String {
        let html = render(markdown);
        let start = html.find("<body>\n").unwrap() + "<body>\n".len();
        let end = html.find("</body>").unwrap();
        html[start..end].to_string()
    }

    #[test]
    fn es_un_documento_completo_con_head() {
        let html = render("Hola");
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(
            html.contains("<meta charset=\"utf-8\" />\n</head>"),
            "{html}"
        );
        assert!(
            html.contains("<p data-sourcepos=\"1:1-1:4\">Hola</p>"),
            "{html}"
        );
    }

    #[test]
    fn los_encabezados_llevan_id_con_acentos_y_sin_enlace_ancla() {
        // Las columnas de `data-sourcepos` cuentan bytes: la `é` ocupa dos.
        let html = body("# Encabezado con *énfasis*");
        assert_eq!(
            html,
            "<h1 id=\"encabezado-con-énfasis\" data-sourcepos=\"1:1-1:27\">Encabezado con <em data-sourcepos=\"1:18-1:27\">énfasis</em></h1>\n"
        );
    }

    #[test]
    fn los_id_repetidos_se_numeran() {
        let html = body("# Uno\n\n# Uno");
        assert!(html.contains("id=\"uno\""), "{html}");
        assert!(html.contains("id=\"uno-1\""), "{html}");
    }

    #[test]
    fn los_atributos_del_encabezado_dan_el_id_y_la_clase() {
        let html = body("## Título {#mi-id .clase .otra}");
        assert!(
            html.starts_with("<h2 id=\"mi-id\" class=\"clase otra\" data-sourcepos="),
            "{html}"
        );
        assert!(html.contains(">Título</h2>"), "{html}");
    }

    #[test]
    fn un_atributo_solo_de_clase_conserva_el_id_calculado() {
        let html = body("## Título {.clase}");
        assert!(
            html.starts_with("<h2 id=\"título\" class=\"clase\""),
            "{html}"
        );
    }

    #[test]
    fn el_id_del_atributo_se_escapa() {
        let html = body("# T {#a\"b}");
        assert!(!html.contains("id=\"a\"b\""), "{html}");
    }

    #[test]
    fn una_imagen_sola_es_una_figura_con_pie() {
        let html = body("![Pie de figura](imagen.png)");
        assert!(
            html.starts_with("<figure data-sourcepos=\"1:1-1:28\">\n<img"),
            "{html}"
        );
        assert!(html.contains("src=\"imagen.png\""), "{html}");
        assert!(
            html.ends_with("<figcaption>Pie de figura</figcaption>\n</figure>\n"),
            "{html}"
        );
        assert!(!html.contains("<p"), "{html}");
    }

    #[test]
    fn una_imagen_en_linea_no_es_figura() {
        let html = body("Texto ![alt](a.png) y más.");
        assert!(!html.contains("<figure"), "{html}");
        assert!(html.starts_with("<p"), "{html}");
    }

    #[test]
    fn una_imagen_sin_texto_alternativo_no_es_figura() {
        let html = body("![](a.png)");
        assert!(!html.contains("<figure"), "{html}");
    }

    #[test]
    fn una_imagen_sola_dentro_de_una_lista_no_es_figura() {
        let html = body("- ![alt](a.png)");
        assert!(!html.contains("<figure"), "{html}");
    }

    #[test]
    fn el_pie_de_figura_se_escapa() {
        let html = body("![a <b> & c](x.png)");
        assert!(
            html.contains("<figcaption>a &lt;b&gt; &amp; c</figcaption>"),
            "{html}"
        );
    }

    #[test]
    fn el_yaml_arma_el_encabezado_del_documento() {
        let html = body(
            "---\ntitle: Documento de prueba\nsubtitle: \"Con subtítulo\"\nauthor: Elision\ndate: '2026-11-15'\nlang: es\n---\n\nTexto",
        );
        assert!(
            html.starts_with(
                "<header id=\"title-block-header\">\n<h1 class=\"title\">Documento de prueba</h1>\n<p class=\"subtitle\">Con subtítulo</p>\n<p class=\"author\">Elision</p>\n<p class=\"date\">2026-11-15</p>\n</header>\n"
            ),
            "{html}"
        );
        assert!(!html.contains("lang"), "{html}");
    }

    #[test]
    fn varios_autores_en_lista() {
        let html = body("---\nauthor:\n  - Ana\n  - \"Luis\"\ntitle: T\n---\n");
        assert!(
            html.contains("<p class=\"author\">Ana</p>\n<p class=\"author\">Luis</p>"),
            "{html}"
        );
        assert!(html.contains("<h1 class=\"title\">T</h1>"), "{html}");
    }

    #[test]
    fn el_yaml_sin_titulo_ni_autor_no_muestra_nada() {
        let html = body("---\nlang: es\n---\n\nTexto");
        assert!(!html.contains("<header"), "{html}");
        assert!(!html.contains("lang"), "{html}");
    }

    #[test]
    fn el_titulo_del_yaml_se_escapa() {
        let html = body("---\ntitle: <script>x</script>\n---\n");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(!html.contains("<script>"), "{html}");
    }

    #[test]
    fn notas_al_pie_normales_y_en_linea() {
        let html = body("Uno[^1] y dos^[en línea].\n\n[^1]: Primera.");
        assert!(html.contains("class=\"footnotes\""), "{html}");
        assert!(html.contains("Primera."), "{html}");
        assert!(html.contains("en línea"), "{html}");
    }

    #[test]
    fn extensiones_activas() {
        let html = body(
            "~~tachado~~ H~2~O x^2^ ==marca== https://example.com\n\n- [x] hecha\n\nTérmino\n:   Definición\n\n::: aviso\nDentro\n:::\n\n> [!NOTE]\n> Alerta",
        );
        for expected in [
            "<del",
            "<sub",
            "<sup",
            "<mark",
            "href=\"https://example.com\"",
            "class=\"task-list-item-checkbox\" checked=\"\"",
            "<dl",
            "<div class=\"aviso\"",
            "markdown-alert-note",
        ] {
            assert!(html.contains(expected), "falta {expected} en {html}");
        }
    }

    #[test]
    fn las_listas_de_tareas_llevan_clases() {
        let html = body("- [ ] pendiente\n- [x] hecha");
        assert!(html.contains("<ul class=\"contains-task-list\""), "{html}");
        assert!(html.contains("<li class=\"task-list-item\""), "{html}");
    }

    #[test]
    fn tipografia_inteligente() {
        let html = body("\"comillas\" -- y --- y...");
        assert!(html.contains("“comillas” – y — y…"), "{html}");
    }

    #[test]
    fn las_formulas_salen_como_mathml() {
        let html = body("En línea $E = mc^2$.\n\n$$\nx^2\n$$");
        assert!(
            html.contains("<span class=\"math inline\" data-sourcepos="),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"math display\" data-sourcepos="),
            "{html}"
        );
        assert!(html.contains("<math>"), "{html}");
        assert!(html.contains("<math display=\"block\">"), "{html}");
        assert!(!html.contains("data-math-style"), "{html}");
    }

    #[test]
    fn una_formula_invalida_muestra_su_latex_marcado() {
        let html = body(r"Mal: $\frac{a}$ y $1 < 2$.");
        assert!(
            html.contains(r#"<code class="math-error" data-reason="invalid">\frac{a}</code>"#),
            "{html}"
        );
        // La que esta bien se convierte aunque la otra falle.
        assert!(html.contains("<math>"), "{html}");
    }

    #[test]
    fn una_formula_con_macros_no_se_convierte() {
        let html = body(r"$\def\a{\a}\a$");
        assert!(html.contains(r#"data-reason="too-complex""#), "{html}");
    }

    #[test]
    fn el_bloque_mermaid_conserva_su_lenguaje() {
        let html = body("```mermaid\ngraph TD\n  A --> B\n```");
        assert!(
            html.contains("<code class=\"language-mermaid\">graph TD\n  A --&gt; B\n</code>"),
            "{html}"
        );
    }

    #[test]
    fn un_bloque_de_formula_con_renglones_que_empiezan_con_signo_no_se_parte() {
        // En CommonMark `+ c` cortaria el parrafo y abriria una lista.
        let html = body("$$\na + b\n+ c\n- d\n$$\n\nDespués");
        assert!(!html.contains("<ul"), "{html}");
        assert!(html.contains("<span class=\"math display\""), "{html}");
        assert!(html.contains("<math display=\"block\">"), "{html}");
        assert!(html.contains("Después"), "{html}");
    }

    #[test]
    fn un_bloque_math_se_dibuja_como_formula() {
        let html = body("```math\n\\frac{a}{b}\n```");
        assert!(html.contains("<mfrac>"), "{html}");
        assert!(!html.contains("<pre"), "{html}");
    }

    #[test]
    fn prepare_no_cambia_los_renglones() {
        let doc = "---\ntitle: T\n...\n\n$$\na\n+ b\n$$\n\n```\n$$\nno\n$$\n```\n\n$$\n\nx\n$$\n";
        let prepared = prepare(doc);
        assert_eq!(prepared.lines().count(), doc.lines().count());
        let lines: Vec<&str> = prepared.lines().collect();
        assert_eq!(lines[2], "---", "cierre del YAML");
        assert_eq!(lines[4], "```math");
        assert_eq!(lines[7], "```");
        assert_eq!(lines[10], "$$", "dentro de un bloque de codigo no se toca");
        assert_eq!(
            lines[15], "$$",
            "con un renglon en blanco en el medio no se toca"
        );
    }

    #[test]
    fn prepare_conserva_los_crlf() {
        let doc = "$$\r\na\r\n$$\r\n";
        assert_eq!(prepare(doc), "```math\r\na\r\n```\r\n");
    }

    #[test]
    fn superindice_y_subindice_con_espacios_quedan_literales() {
        let html = body("x^2 y z^3 y ~tachado simple~ pero H~2~O y x^2^");
        assert!(html.contains("x^2 y z^3"), "{html}");
        assert!(html.contains("~tachado simple~"), "{html}");
        assert!(
            html.contains("<sub"),
            "H~2~O sigue siendo subindice: {html}"
        );
        assert!(
            html.contains("<sup"),
            "x^2^ sigue siendo superindice: {html}"
        );
    }

    #[test]
    fn el_lenguaje_como_clase_se_resalta() {
        let html = body("```{.python}\nprint(1 < 2)\n```");
        assert!(
            html.contains("<code class=\"language-python\">print(1 &lt; 2)\n</code>"),
            "{html}"
        );
    }

    #[test]
    fn yaml_cerrado_con_puntos_suspensivos() {
        let html = body("---\ntitle: T\n...\n\nTexto");
        assert!(html.contains("<h1 class=\"title\">T</h1>"), "{html}");
        assert!(!html.contains("<hr"), "{html}");
    }

    #[test]
    fn yaml_con_bloques_listas_y_comentarios() {
        let html = body(
            "---\ntitle: |\n  Un título\n  largo\nauthor: [Ana, \"Luis\"]\ndate: 2026-11-15 # comentario\nsubtitle: \"Con # adentro\" # y fuera\n---\n",
        );
        assert!(
            html.contains("<h1 class=\"title\">Un título largo</h1>"),
            "{html}"
        );
        assert!(
            html.contains("<p class=\"author\">Ana</p>\n<p class=\"author\">Luis</p>"),
            "{html}"
        );
        assert!(html.contains("<p class=\"date\">2026-11-15</p>"), "{html}");
        assert!(
            html.contains("<p class=\"subtitle\">Con # adentro</p>"),
            "{html}"
        );
    }

    #[test]
    fn el_html_crudo_pasa() {
        let html = body("<div class=\"crudo\">HTML <em>crudo</em></div>");
        assert!(
            html.contains("<div class=\"crudo\">HTML <em>crudo</em></div>"),
            "{html}"
        );
    }

    #[test]
    fn el_documento_de_prueba_del_adr_se_renderiza_entero() {
        // docs/adr/0003/casos.md: no debe entrar en panico y tiene que conservar el final.
        let casos = include_str!("../../../docs/adr/0003/casos.md");
        let html = render(casos);
        assert!(
            html.contains("<h1 class=\"title\">Documento de prueba</h1>"),
            "{html}"
        );
        assert!(html.contains("id=\"mi-id\""), "{html}");
        assert!(
            html.contains("<figcaption>Pie de figura</figcaption>"),
            "{html}"
        );
        assert!(html.contains("Fin.</p>"), "{html}");
    }
}
