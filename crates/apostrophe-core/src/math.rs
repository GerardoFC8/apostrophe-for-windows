//! Documento typst para renderizar una formula suelta (M20). Espejo de `latex_to_PNG.py`
//! del upstream, que rasteriza las ecuaciones para mostrarlas en el popover inline.
//!
//! Divergencia: upstream necesita una instalacion de LaTeX. Aca la formula pasa por el
//! mismo par de binarios que ya se distribuyen — pandoc traduce el LaTeX del markdown a la
//! sintaxis de typst, y typst la dibuja. Cero dependencias nuevas.
//!
//! Logica PURA: arma el fuente. El spawn de los dos procesos vive en el adaptador.

/// Envuelve una expresion YA TRADUCIDA a sintaxis typst (por `pandoc --to typst`) en un
/// documento minimo, listo para `typst compile --format svg`.
///
/// Decisiones del preambulo, todas necesarias para que la formula se integre en el popover:
///
/// - `width/height: auto` — la pagina se ajusta a la formula. Sin esto saldria una A4 con
///   la ecuacion perdida en una esquina.
/// - `fill: none` — fondo TRANSPARENTE. Es lo que permite que el mismo SVG sirva en tema
///   claro y oscuro sin recortes blancos alrededor.
/// - `text(fill: ...)` — el color lo manda el frontend desde la paleta activa. typst pinta
///   en negro por defecto, que en tema oscuro seria una formula invisible sobre fondo
///   oscuro: el sintoma seria "el popover aparece vacio".
pub fn typst_document(typst_math: &str, text_color: &str) -> String {
    format!(
        "#set page(width: auto, height: auto, margin: 4pt, fill: none)\n\
         #set text(size: 13pt, fill: rgb(\"{}\"))\n\
         {}\n",
        normalize_color(text_color),
        typst_math
    )
}

/// typst quiere el hex SIN almohadilla dentro de `rgb("...")`. Se acepta con o sin ella para
/// que quien llama pueda pasar la variable CSS tal cual la lee del tema.
/// Un valor que no sea hex cae a negro: mejor una formula legible en claro que ninguna.
fn normalize_color(color: &str) -> String {
    let hex = color.trim().trim_start_matches('#');
    let valid = matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    if valid {
        hex.to_ascii_lowercase()
    } else {
        "000000".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_pagina_se_ajusta_a_la_formula_y_es_transparente() {
        let d = typst_document("$ x $", "#2e3436");
        assert!(d.contains("width: auto"), "sin esto saldria una A4: {d}");
        assert!(d.contains("height: auto"));
        assert!(
            d.contains("fill: none"),
            "el fondo debe ser transparente: {d}"
        );
    }

    #[test]
    fn el_color_del_texto_viene_del_tema() {
        // En tema oscuro, el negro por defecto de typst daria una formula invisible.
        let d = typst_document("$ x $", "#deddda");
        assert!(d.contains(r#"rgb("deddda")"#), "{d}");
    }

    #[test]
    fn el_color_se_acepta_con_o_sin_almohadilla() {
        // La var CSS del tema llega con `#`; typst la quiere sin ella.
        assert!(typst_document("$ x $", "#2E3436").contains(r#"rgb("2e3436")"#));
        assert!(typst_document("$ x $", "2e3436").contains(r#"rgb("2e3436")"#));
    }

    #[test]
    fn un_color_invalido_cae_a_negro_en_vez_de_romper_el_render() {
        // typst abortaria con un color mal formado y el popover quedaria vacio sin
        // explicacion. Negro es legible en claro, que es el caso por defecto.
        for malo in ["", "rojo", "#12345", "#zzzzzz", "rgb(1,2,3)"] {
            let d = typst_document("$ x $", malo);
            assert!(d.contains(r#"rgb("000000")"#), "con {malo:?}: {d}");
        }
    }

    #[test]
    fn se_aceptan_las_formas_hex_de_3_6_y_8_digitos() {
        assert!(typst_document("$ x $", "#abc").contains(r#"rgb("abc")"#));
        assert!(typst_document("$ x $", "#aabbcc").contains(r#"rgb("aabbcc")"#));
        assert!(typst_document("$ x $", "#aabbccdd").contains(r#"rgb("aabbccdd")"#));
    }

    #[test]
    fn la_formula_va_al_final_intacta() {
        // Lo que devuelve pandoc no se toca: cualquier retoque nuestro seria una segunda
        // traduccion compitiendo con la suya.
        let d = typst_document("$a / b + sqrt(x^2 + 1)$", "#000");
        assert!(d.trim_end().ends_with("$a / b + sqrt(x^2 + 1)$"), "{d}");
    }
}
