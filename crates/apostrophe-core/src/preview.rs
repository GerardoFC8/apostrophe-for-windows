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
//!
//! Las construcciones que quedan fuera (tablas de cuadricula, listas con letras, bloques
//! de versos, spans con clase) estan en la tabla de limitaciones del ADR.

use std::fmt::Write;

use comrak::html::{format_node_default, render_sourcepos};
use comrak::nodes::{AstNode, NodeValue};
use comrak::{create_formatter, parse_document, Arena, Options};

/// Documento HTML completo de la vista previa. El frontend inserta su hoja de estilos y su
/// script antes de `</head>`, igual que hacia con la salida `--standalone` de pandoc.
pub fn render(markdown: &str) -> String {
    let options = options();
    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &options);
    let mut body = String::new();
    // Escribir en un `String` no falla: `fmt::Error` solo puede venir del destino.
    PreviewFormatter::format_document(root, &options, &mut body)
        .expect("escribir HTML en un String no falla");
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\" />\n</head>\n<body>\n{body}</body>\n</html>\n"
    )
}

/// Extensiones de comrak. Ver la seccion "Configuracion de comrak" de ADR-0003.
fn options() -> Options<'static> {
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
        let mut in_authors = false;
        for line in front_matter.lines() {
            let trimmed = line.trim();
            if trimmed == "---" || trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(item) = trimmed.strip_prefix("- ") {
                if in_authors {
                    push_text(&mut block.authors, item);
                }
                continue;
            }
            in_authors = false;
            if line.starts_with(char::is_whitespace) {
                continue; // valor anidado de otra clave
            }
            let Some((key, value)) = trimmed.split_once(':') else {
                continue;
            };
            let value = unquote(value.trim());
            match key.trim() {
                "title" => block.title = non_empty(value),
                "subtitle" => block.subtitle = non_empty(value),
                "date" => block.date = non_empty(value),
                "author" if value.is_empty() => in_authors = true,
                "author" => push_text(&mut block.authors, value),
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
    if let Some(text) = non_empty(unquote(value.trim())) {
        list.push(text);
    }
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
            "type=\"checkbox\" checked=\"\"",
            "<dl",
            "<div class=\"aviso\"",
            "markdown-alert-note",
        ] {
            assert!(html.contains(expected), "falta {expected} en {html}");
        }
    }

    #[test]
    fn tipografia_inteligente() {
        let html = body("\"comillas\" -- y --- y...");
        assert!(html.contains("“comillas” – y — y…"), "{html}");
    }

    #[test]
    fn las_formulas_quedan_marcadas_para_dibujarlas() {
        let html = body("En línea $E = mc^2$.\n\n$$\nx^2\n$$");
        assert!(html.contains("data-math-style=\"inline\""), "{html}");
        assert!(html.contains("E = mc^2"), "{html}");
        assert!(html.contains("data-math-style=\"display\""), "{html}");
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
