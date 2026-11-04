//! Esquema del documento: sus titulos, en orden, con el renglon donde empieza cada uno.
//!
//! Lo usan el panel de esquema (saltar a una seccion) y el scroll sincronizado, que alinea
//! el editor con la vista previa titulo por titulo. Para lo segundo importa que la lista
//! coincida UNO A UNO con los `<h1>`…`<h6>` que genera pandoc: por eso se siguen sus reglas
//! del Markdown de pandoc, no las de CommonMark.
//!
//! - Titulos ATX (`## Texto`): de 1 a 6 `#` seguidos de un espacio, pegados al margen.
//!   Pandoc exige una linea en blanco antes (`blank_before_header`): un `#tema` en medio de
//!   un parrafo no es titulo. Con sangria solo cuentan dentro de una lista, y tambien
//!   cuentan justo despues de la marca de un elemento (`- # Titulo`).
//! - Una etiqueta HTML de bloque (`<div>`) corta el parrafo como una linea en blanco:
//!   pandoc sigue leyendo Markdown adentro.
//! - Titulos setext: un renglon de texto subrayado con `===` (nivel 1) o `---` (nivel 2).
//! - Se ignora lo que hay dentro de bloques de codigo cercados (```` ``` ```` o `~~~`) y el
//!   encabezado YAML del principio.
//! - Dentro de una cita (`> ### Titulo`) tambien hay titulos: pandoc los genera igual, asi
//!   que se miran a traves de los `>`.

/// Un titulo del documento.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Heading {
    /// Nivel, de 1 a 6.
    pub level: u8,
    /// Texto para mostrar, sin las marcas de Markdown del titulo.
    pub text: String,
    /// Renglon donde empieza el titulo, contando desde 0.
    pub line: usize,
}

/// Los titulos de `text`, en orden.
pub fn outline(text: &str) -> Vec<Heading> {
    let lines: Vec<&str> = text.lines().collect();
    let mut headings = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut i = front_matter_end(&lines);
    // El renglon anterior estaba en blanco (o es el principio del documento).
    let mut after_blank = true;
    // Se esta dentro de una lista: ahi los titulos pueden venir con sangria.
    let mut in_list = false;

    while i < lines.len() {
        let line = unquote(lines[i]);
        if let Some((ch, len)) = fence {
            if is_fence_close(line, ch, len) {
                fence = None;
            }
            after_blank = false;
            i += 1;
            continue;
        }
        if let Some(open) = fence_open(line) {
            fence = Some(open);
            after_blank = false;
            i += 1;
            continue;
        }
        if line.trim().is_empty() {
            after_blank = true;
            i += 1;
            continue;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        if let Some(item) = list_item(line) {
            in_list = true;
            if let Some((level, title)) = atx(item) {
                headings.push(Heading {
                    level,
                    text: clean(title),
                    line: i,
                });
            }
            after_blank = false;
            i += 1;
            continue;
        }
        if indent == 0 {
            in_list = false;
        }
        if is_html_tag_line(line) {
            after_blank = true; // pandoc sigue leyendo Markdown dentro del bloque
            i += 1;
            continue;
        }
        if after_blank && (indent == 0 || in_list) {
            if let Some((level, title)) = atx(line.trim_start()) {
                headings.push(Heading {
                    level,
                    text: clean(title),
                    line: i,
                });
                after_blank = false;
                i += 1;
                continue;
            }
            if let Some(level) = lines
                .get(i + 1)
                .and_then(|next| setext_level(unquote(next)))
            {
                if !line.starts_with("    ") && !line.starts_with('\t') {
                    headings.push(Heading {
                        level,
                        text: clean(line.trim()),
                        line: i,
                    });
                    after_blank = false;
                    i += 2;
                    continue;
                }
            }
        }
        after_blank = false;
        i += 1;
    }
    headings
}

/// El renglon sin las marcas de cita (`> > texto` → `texto`).
fn unquote(line: &str) -> &str {
    let mut rest = line;
    while let Some(inner) = strip_indent(rest).and_then(|t| t.strip_prefix('>')) {
        rest = inner.strip_prefix(' ').unwrap_or(inner);
    }
    rest
}

/// Primer renglon despues del encabezado YAML (`---` … `---` o `...`), o 0 si no hay.
fn front_matter_end(lines: &[&str]) -> usize {
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return 0;
    }
    lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| matches!(l.trim_end(), "---" | "..."))
        .map_or(0, |(i, _)| i + 1)
}

/// Apertura de un bloque cercado: el caracter y cuantos trae.
fn fence_open(line: &str) -> Option<(char, usize)> {
    let trimmed = strip_indent(line)?;
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|&c| c == ch).count();
    (len >= 3).then_some((ch, len))
}

fn is_fence_close(line: &str, ch: char, len: usize) -> bool {
    strip_indent(line).is_some_and(|t| {
        let run = t.chars().take_while(|&c| c == ch).count();
        run >= len && t[run * ch.len_utf8()..].trim().is_empty()
    })
}

/// El renglon sin hasta 3 espacios de sangria; `None` si tiene 4 o mas (codigo indentado).
fn strip_indent(line: &str) -> Option<&str> {
    let spaces = line.chars().take_while(|&c| c == ' ').count();
    (spaces <= 3 && !line[spaces..].starts_with('\t')).then(|| &line[spaces..])
}

/// Contenido de un elemento de lista (`- texto`, `1. texto`), sin su marca.
fn list_item(line: &str) -> Option<&str> {
    let t = strip_indent(line)?;
    let marker_len = if t.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        if !(1..=9).contains(&digits) || !t[digits..].starts_with(['.', ')']) {
            return None;
        }
        digits + 1
    };
    let rest = &t[marker_len..];
    rest.starts_with([' ', '\t']).then(|| rest.trim_start())
}

/// Un renglon que es solo una etiqueta HTML (`<div>`, `</div>`, `<details open>`).
fn is_html_tag_line(line: &str) -> bool {
    let t = line.trim();
    t.len() > 2
        && t.starts_with('<')
        && t.ends_with('>')
        && t[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/')
}

/// `## Texto ##` → (2, "Texto"). Pandoc pide un espacio despues de los `#`. Recibe el
/// renglon ya sin sangria.
fn atx(trimmed: &str) -> Option<(u8, &str)> {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    // Los `#` de cierre son opcionales y solo cuentan si van separados del texto.
    let mut title = rest.trim();
    let closing = title.trim_end_matches('#');
    if closing.is_empty() || closing.ends_with([' ', '\t']) {
        title = closing.trim_end();
    }
    Some((hashes as u8, title))
}

/// Nivel de un subrayado setext: `===` es 1 y `---` es 2.
fn setext_level(line: &str) -> Option<u8> {
    let t = strip_indent(line)?.trim_end();
    if !t.is_empty() && t.chars().all(|c| c == '=') {
        Some(1)
    } else if !t.is_empty() && t.chars().all(|c| c == '-') {
        Some(2)
    } else {
        None
    }
}

/// Texto del titulo para mostrar: sin los atributos de pandoc (`{#id .clase}`) ni las marcas
/// de enfasis y codigo, y con los enlaces reducidos a su texto.
fn clean(title: &str) -> String {
    let mut title = title.trim();
    if title.ends_with('}') {
        if let Some(open) = title.rfind('{') {
            title = title[..open].trim_end();
        }
    }
    let mut out = String::with_capacity(title.len());
    let mut chars = title.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '`' => {}
            // `[texto](url)` → `texto`.
            ']' if chars.peek() == Some(&'(') => {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
            }
            '[' => {}
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(level: u8, text: &str, line: usize) -> Heading {
        Heading {
            level,
            text: text.to_string(),
            line,
        }
    }

    #[test]
    fn titulos_atx_y_setext_con_su_renglon() {
        let doc = "# Uno\n\nTexto\n\nDos\n===\n\nTres\n---\n\n### Cuatro ###\n";
        assert_eq!(
            outline(doc),
            vec![
                h(1, "Uno", 0),
                h(1, "Dos", 4),
                h(2, "Tres", 7),
                h(3, "Cuatro", 10)
            ]
        );
    }

    #[test]
    fn el_codigo_cercado_no_tiene_titulos() {
        let doc = "# A\n\n```sql\n# no\n```\n\n~~~~\n## tampoco\n~~~\n~~~~\n\n## B\n";
        assert_eq!(outline(doc), vec![h(1, "A", 0), h(2, "B", 11)]);
    }

    #[test]
    fn sin_linea_en_blanco_antes_no_es_titulo() {
        // Pandoc (blank_before_header): un #tema dentro de un parrafo es texto.
        let doc = "Texto\n# no\n\n# si\n";
        assert_eq!(outline(doc), vec![h(1, "si", 3)]);
    }

    #[test]
    fn hace_falta_el_espacio_y_a_lo_sumo_seis_almohadillas() {
        let doc = "#hashtag\n\n####### siete\n\n###### seis\n";
        assert_eq!(outline(doc), vec![h(6, "seis", 4)]);
    }

    #[test]
    fn el_codigo_indentado_no_es_titulo() {
        let doc = "Parrafo\n\n    # codigo\n\n# Si\n";
        assert_eq!(outline(doc), vec![h(1, "Si", 4)]);
    }

    #[test]
    fn se_salta_el_encabezado_yaml() {
        let doc = "---\ntitle: X\n---\n\n# Real\n";
        assert_eq!(outline(doc), vec![h(1, "Real", 4)]);
    }

    #[test]
    fn el_texto_sale_sin_marcas_ni_atributos() {
        let doc = "## **Guía** de `SQL` y [enlace](http://x) {#guia .c}\n\n# 5.1 La \\*API\\*\n";
        assert_eq!(
            outline(doc),
            vec![h(2, "Guía de SQL y enlace", 0), h(1, "5.1 La *API*", 2)]
        );
    }

    #[test]
    fn una_regla_despues_de_un_blanco_no_convierte_nada_en_titulo() {
        let doc = "Texto\n\n---\n\nMas\n";
        assert_eq!(outline(doc), vec![]);
    }

    #[test]
    fn los_titulos_dentro_de_una_cita_cuentan() {
        let doc = "## A\n\n> Nota\n>\n> ### Dentro\n>\n> texto\n";
        assert_eq!(outline(doc), vec![h(2, "A", 0), h(3, "Dentro", 4)]);
    }

    #[test]
    fn listas_html_y_sangria_como_pandoc() {
        let doc = "- item\n- # en lista\n\n1. uno\n\n   ## dentro de item\n\nTexto\n\n  ### con sangria fuera de lista\n\n<div>\n# en div\n</div>\n";
        assert_eq!(
            outline(doc),
            vec![
                h(1, "en lista", 1),
                h(2, "dentro de item", 5),
                h(1, "en div", 12)
            ]
        );
    }

    #[test]
    fn coincide_con_crlf() {
        assert_eq!(
            outline("# A\r\n\r\n## B\r\n"),
            vec![h(1, "A", 0), h(2, "B", 2)]
        );
    }
}
