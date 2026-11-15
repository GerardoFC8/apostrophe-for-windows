//! Esquema del documento: sus titulos, en orden, con el renglon donde empieza cada uno.
//!
//! Lo usan el panel de esquema (saltar a una seccion) y el scroll sincronizado, que alinea
//! el editor con la vista previa titulo por titulo. Para lo segundo importa que la lista
//! coincida UNO A UNO con los `<h1>`…`<h6>` de la vista previa. Por eso no hay reglas
//! propias: se lee el mismo arbol de comrak, con las mismas opciones, que usa
//! `crate::preview` (ADR-0003). Un titulo dentro de una cita o de una lista cuenta; uno
//! dentro de un bloque HTML o de codigo, no, igual que en la vista previa.

use comrak::nodes::NodeValue;
use comrak::{parse_document, Arena};

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
    let options = crate::preview::options();
    let arena = Arena::new();
    let root = parse_document(&arena, &crate::preview::prepare(text), &options);
    root.descendants()
        .filter_map(|node| {
            let data = node.data();
            let NodeValue::Heading(heading) = &data.value else {
                return None;
            };
            Some(Heading {
                level: heading.level,
                text: node.collect_text().trim().to_string(),
                // `sourcepos` cuenta desde 1.
                line: data.sourcepos.start.line.saturating_sub(1),
            })
        })
        .collect()
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
    fn un_titulo_puede_cortar_un_parrafo() {
        // CommonMark: no hace falta una linea en blanco antes, a diferencia de pandoc.
        let doc = "Texto\n# si\n# Uno\n## Dos\n";
        assert_eq!(
            outline(doc),
            vec![h(1, "si", 1), h(1, "Uno", 2), h(2, "Dos", 3)]
        );
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
    fn los_titulos_dentro_de_una_cita_o_una_lista_cuentan() {
        let doc = "## A\n\n> Nota\n>\n> ### Dentro\n\n- # en lista\n";
        assert_eq!(
            outline(doc),
            vec![h(2, "A", 0), h(3, "Dentro", 4), h(1, "en lista", 6)]
        );
    }

    #[test]
    fn dentro_de_un_bloque_html_no_hay_titulos() {
        let doc = "<div>\n# en div\n</div>\n\n# Fuera\n";
        assert_eq!(outline(doc), vec![h(1, "Fuera", 4)]);
    }

    #[test]
    fn coincide_con_crlf() {
        assert_eq!(
            outline("# A\r\n\r\n## B\r\n"),
            vec![h(1, "A", 0), h(2, "B", 2)]
        );
    }

    #[test]
    fn coincide_uno_a_uno_con_los_titulos_de_la_vista_previa() {
        // El scroll sincronizado empareja por indice: un titulo de mas o de menos desplaza
        // todas las secciones siguientes. Casos donde CommonMark y pandoc difieren.
        for doc in [
            "# Uno\n## Dos\n### Tres",
            "Párrafo\n# Uno\n\n# Dos",
            "- item\n# Uno",
            "> cita\n# Uno",
            "```\ncodigo\n```\n# si",
            "Uno\n===\nDos\n---",
            "<div>\n# Dentro\n</div>\n\n# Fuera",
            "::: nota\n# Dentro\n:::\n\n# Fuera",
            "---\ntitle: T\n---\n\n# A\n\n> ## B\n\n- ### C",
        ] {
            let html = crate::preview::render(doc);
            let body = &html[html.find("<body>").unwrap()..];
            // El titulo del YAML no es parte del esquema (el frame tambien lo salta).
            let body = match body.find("</header>") {
                Some(end) => &body[end..],
                None => body,
            };
            let in_preview = (1..=6)
                .map(|n| body.matches(&format!("<h{n} ")).count())
                .sum::<usize>();
            assert_eq!(outline(doc).len(), in_preview, "{doc:?}\n{html}");
        }
    }
}
