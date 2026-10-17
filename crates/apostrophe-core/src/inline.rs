//! Resolucion de lo que hay bajo el cursor, para el popover de vista previa inline (M19).
//! Espejo de `inline_preview.py` del upstream, que muestra el destino de un enlace, la
//! imagen, el cuerpo de una nota al pie o la ecuacion sin salir del editor.
//!
//! Logica PURA: recibe el texto y una posicion, devuelve QUE hay ahi. No renderiza nada ni
//! toca disco — el frontend decide como mostrarlo.
//!
//! Reusa los regex de `markup`, que ya son los portados 1:1 del upstream: si el resaltado y
//! el popover reconocieran markdown por caminos distintos, tarde o temprano discreparian.

use crate::markup::{FOOTNOTE, FOOTNOTE_ID, IMAGE, LINK, LINK_ALT, MATH};

/// Lo que el cursor tiene debajo, si es algo previsualizable.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "kebab-case"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineTarget {
    /// `[texto](url)` o `<url>`.
    Link { url: String, text: String },
    /// `![alt](url)`.
    Image { url: String, alt: String },
    /// Referencia `[^id]`, ya resuelta contra su definicion `[^id]: cuerpo`.
    /// `body` va vacio si la definicion no existe — el frontend avisa en vez de mentir.
    Footnote { id: String, body: String },
    /// `$...$` o `$$...$$`, sin los delimitadores.
    Math { source: String },
}

/// Convierte un indice de CARACTER a indice de BYTE. Los offsets cruzan la frontera en
/// caracteres (igual que `Span` y `Misspelling`) porque es lo que entiende el editor, pero
/// los regex trabajan en bytes: con acentos o eñes antes del cursor, confundirlos apuntaria
/// a la posicion equivocada.
fn byte_offset(text: &str, char_offset: usize) -> Option<usize> {
    if char_offset == text.chars().count() {
        return Some(text.len()); // el final del texto es una posicion valida
    }
    text.char_indices().nth(char_offset).map(|(i, _)| i)
}

/// Primera captura cuyo rango CONTIENE `at` (en bytes). Devuelve los grupos pedidos.
fn find_at(
    re: &fancy_regex::Regex,
    text: &str,
    at: usize,
    groups: [&str; 2],
) -> Option<(String, String)> {
    for caps in re.captures_iter(text).flatten() {
        let whole = caps.get(0)?;
        // `at` puede caer justo al final del match (cursor pegado al cierre): se acepta,
        // porque el usuario percibe el cursor "sobre" el enlace tambien ahi.
        if at < whole.start() || at > whole.end() {
            continue;
        }
        let a = caps.name(groups[0]).map(|m| m.as_str().to_string())?;
        let b = caps.name(groups[1]).map(|m| m.as_str().to_string())?;
        return Some((a, b));
    }
    None
}

/// Que hay en `char_offset`, si es previsualizable.
///
/// El ORDEN de las pruebas importa: `![alt](url)` contiene un `[alt](url)` valido, asi que
/// la imagen se prueba antes que el enlace. Al reves, toda imagen se reportaria como enlace.
pub fn resolve_at(text: &str, char_offset: usize) -> Option<InlineTarget> {
    let at = byte_offset(text, char_offset)?;

    if let Some((url, alt)) = find_at(&IMAGE, text, at, ["url", "text"]) {
        return Some(InlineTarget::Image { url, alt });
    }
    if let Some((url, label)) = find_at(&LINK, text, at, ["url", "text"]) {
        return Some(InlineTarget::Link { url, text: label });
    }
    if let Some((url, label)) = find_at(&LINK_ALT, text, at, ["url", "text"]) {
        return Some(InlineTarget::Link { url, text: label });
    }
    if let Some((id, _)) = find_at(&FOOTNOTE_ID, text, at, ["id", "text"]) {
        let body = footnote_body(text, &id).unwrap_or_default();
        return Some(InlineTarget::Footnote { id, body });
    }
    if let Some((source, _)) = find_at(&MATH, text, at, ["text", "delim"]) {
        return Some(InlineTarget::Math { source });
    }
    None
}

/// Cuerpo de la definicion `[^id]: ...` correspondiente. `None` si no esta definida.
fn footnote_body(text: &str, id: &str) -> Option<String> {
    for caps in FOOTNOTE.captures_iter(text).flatten() {
        if caps.name("id").map(|m| m.as_str()) == Some(id) {
            return caps.name("text").map(|m| m.as_str().trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Posicion (en caracteres) de la primera aparicion de `needle`, mas `plus`.
    fn at(text: &str, needle: &str, plus: usize) -> usize {
        let byte = text
            .find(needle)
            .expect("el needle deberia estar en el texto");
        text[..byte].chars().count() + plus
    }

    #[test]
    fn reconoce_un_enlace_y_devuelve_su_destino() {
        let t = "ver [la doc](https://example.com) para mas.";
        assert_eq!(
            resolve_at(t, at(t, "la doc", 2)),
            Some(InlineTarget::Link {
                url: "https://example.com".into(),
                text: "la doc".into()
            })
        );
    }

    #[test]
    fn una_imagen_no_se_confunde_con_un_enlace() {
        // `![alt](url)` contiene un `[alt](url)` valido: si el orden de las pruebas se
        // invirtiera, toda imagen se reportaria como enlace.
        let t = "mira ![un gato](gato.png) aca";
        assert_eq!(
            resolve_at(t, at(t, "un gato", 1)),
            Some(InlineTarget::Image {
                url: "gato.png".into(),
                alt: "un gato".into()
            })
        );
    }

    #[test]
    fn reconoce_la_forma_con_angulos() {
        let t = "escribi a <https://example.com> hoy";
        assert_eq!(
            resolve_at(t, at(t, "https", 3)),
            Some(InlineTarget::Link {
                url: "https://example.com".into(),
                text: "https://example.com".into()
            })
        );
    }

    #[test]
    fn una_nota_al_pie_trae_el_cuerpo_de_su_definicion() {
        let t = "algo[^1] mas\n\n[^1]: el cuerpo de la nota\n";
        assert_eq!(
            resolve_at(t, at(t, "[^1]", 2)),
            Some(InlineTarget::Footnote {
                id: "1".into(),
                body: "el cuerpo de la nota".into()
            })
        );
    }

    #[test]
    fn una_nota_sin_definicion_devuelve_cuerpo_vacio() {
        // Se reporta igual para que el frontend pueda avisar "sin definir" en vez de que el
        // popover no aparezca y parezca que la app no responde.
        let t = "algo[^falta] mas";
        assert_eq!(
            resolve_at(t, at(t, "[^falta]", 2)),
            Some(InlineTarget::Footnote {
                id: "falta".into(),
                body: String::new()
            })
        );
    }

    #[test]
    fn reconoce_matematica_y_quita_los_delimitadores() {
        let t = "la formula $E = mc^2$ es famosa";
        assert_eq!(
            resolve_at(t, at(t, "E = mc", 1)),
            Some(InlineTarget::Math {
                source: "E = mc^2".into()
            })
        );
    }

    #[test]
    fn los_offsets_son_de_caracteres_y_no_de_bytes() {
        // "Añoñado" tiene mas bytes que caracteres: si se confundieran, el cursor caeria
        // fuera del enlace y no se reconoceria nada.
        let t = "Añoñado ñandú [destino](https://example.com) fin";
        let pos = at(t, "destino", 2);
        assert!(
            pos < t.find("destino").unwrap(),
            "el indice de char va antes que el de byte"
        );
        assert_eq!(
            resolve_at(t, pos),
            Some(InlineTarget::Link {
                url: "https://example.com".into(),
                text: "destino".into()
            })
        );
    }

    #[test]
    fn el_texto_plano_no_devuelve_nada() {
        let t = "una linea sin nada previsualizable";
        assert_eq!(resolve_at(t, 5), None);
    }

    #[test]
    fn fuera_del_enlace_no_devuelve_nada() {
        let t = "[destino](https://example.com) y despues texto suelto";
        assert_eq!(resolve_at(t, at(t, "suelto", 2)), None);
    }

    #[test]
    fn un_offset_pasado_del_final_no_rompe() {
        let t = "corto";
        assert_eq!(resolve_at(t, 999), None);
        assert_eq!(resolve_at(t, t.chars().count()), None); // el final es valido, pero vacio
    }

    #[test]
    fn con_varios_enlaces_devuelve_el_del_cursor() {
        let t = "[uno](a.md) y [dos](b.md)";
        assert_eq!(
            resolve_at(t, at(t, "dos", 1)),
            Some(InlineTarget::Link {
                url: "b.md".into(),
                text: "dos".into()
            })
        );
    }
}
