//! Que hacer al hacer clic en un enlace del panel de vista previa.
//!
//! El preview es un `iframe` con `srcdoc`: si se deja navegar, un enlace relativo como
//! `notas.md` se resuelve contra la URL de la propia app y el panel termina mostrando la
//! interfaz de Elision en vez del documento. Por eso el clic se intercepta y aca se decide
//! adonde lleva cada `href`.
//!
//! Las anclas (`#nota`) no pasan por aca: se resuelven dentro del iframe, que es donde vive
//! el elemento al que apuntan.

use std::path::{Path, PathBuf};

use crate::image::{extension_of, has_scheme, is_absolute, is_windows_drive, percent_decode};

/// Adonde lleva un enlace del preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// `http:`, `https:` o `mailto:`: lo abre el navegador o el cliente de correo.
    Web(String),
    /// Otro documento Markdown: se abre en su propia pestaña.
    Markdown(PathBuf),
    /// Cualquier otro archivo, o una carpeta: se muestra en el Explorador.
    Local(PathBuf),
    /// Ruta relativa en un documento sin guardar: no hay carpeta contra la que resolverla.
    NoBaseDir,
    /// Ancla, enlace vacio o esquema que no se abre (`javascript:`, `file:`, `ms-settings:`…).
    Unsupported,
}

/// Esquemas que se entregan al sistema. Es una lista cerrada a proposito: el HTML del
/// preview puede traer scripts del propio Markdown, que pueden pedir abrir cualquier
/// `href` sin que haya clic. Un esquema registrado por otra app (`ms-settings:`,
/// `steam:`…) lanzaria esa app; estos tres solo abren el navegador o el correo.
const WEB_SCHEMES: [&str; 3] = ["http", "https", "mailto"];

/// Resuelve el `href` de un enlace, sabiendo en que carpeta vive el documento.
///
/// `doc_dir` es `None` mientras el documento no se haya guardado nunca. Igual que con las
/// imagenes, no se exige que el destino quede dentro de `doc_dir`: `../otro.md` es un uso
/// corriente.
pub fn resolve_link(href: &str, doc_dir: Option<&Path>) -> LinkTarget {
    // Markdown admite envolver el destino en angulos cuando tiene espacios: `[a](<mi nota.md>)`.
    let href = href.trim();
    let href = href.strip_prefix('<').unwrap_or(href);
    let href = href.strip_suffix('>').unwrap_or(href).trim();
    if href.is_empty() || href.starts_with('#') {
        return LinkTarget::Unsupported;
    }

    // Como en `resolve_image`: `C:\docs\a.md` tambien encaja en la forma "esquema:" de una
    // URL, asi que la unidad de Windows se prueba primero.
    if !is_windows_drive(href) && has_scheme(href) {
        let scheme = &href[..href.find(':').unwrap_or(0)];
        return if WEB_SCHEMES.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
            LinkTarget::Web(href.to_string())
        } else {
            LinkTarget::Unsupported
        };
    }

    // `otro.md#seccion` apunta al archivo; la seccion no se puede seguir desde otra pestaña.
    // Se corta ANTES de decodificar: un `#` que sea parte del nombre viene como `%23`.
    let path = href.split(['#', '?']).next().unwrap_or_default();
    let decoded = percent_decode(path);
    if decoded.is_empty() {
        return LinkTarget::Unsupported;
    }

    let full = if is_absolute(&decoded) {
        PathBuf::from(&decoded)
    } else {
        match doc_dir {
            Some(dir) => dir.join(&decoded),
            None => return LinkTarget::NoBaseDir,
        }
    };
    let is_markdown = extension_of(&decoded)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"));
    if is_markdown {
        LinkTarget::Markdown(full)
    } else {
        LinkTarget::Local(full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> Option<&'static Path> {
        Some(Path::new("/docs/notas"))
    }

    #[test]
    fn un_md_relativo_se_resuelve_contra_la_carpeta_del_documento() {
        assert_eq!(
            resolve_link("OP26243000-stock-negativo-acabados.md", dir()),
            LinkTarget::Markdown(PathBuf::from(
                "/docs/notas/OP26243000-stock-negativo-acabados.md"
            ))
        );
    }

    #[test]
    fn la_extension_markdown_tambien_cuenta_y_sin_distinguir_mayusculas() {
        assert!(matches!(
            resolve_link("a.markdown", dir()),
            LinkTarget::Markdown(_)
        ));
        assert!(matches!(
            resolve_link("LEEME.MD", dir()),
            LinkTarget::Markdown(_)
        ));
    }

    #[test]
    fn otros_archivos_y_carpetas_son_locales() {
        assert_eq!(
            resolve_link("consultas.sql", dir()),
            LinkTarget::Local(PathBuf::from("/docs/notas/consultas.sql"))
        );
        assert_eq!(
            resolve_link("OP26243000-correccion/", dir()),
            LinkTarget::Local(PathBuf::from("/docs/notas/OP26243000-correccion/"))
        );
    }

    #[test]
    fn las_rutas_absolutas_no_se_tocan() {
        assert_eq!(
            resolve_link("C:\\docs\\otro.md", dir()),
            LinkTarget::Markdown(PathBuf::from("C:\\docs\\otro.md"))
        );
        assert_eq!(
            resolve_link("/srv/a.txt", None),
            LinkTarget::Local(PathBuf::from("/srv/a.txt"))
        );
    }

    #[test]
    fn la_seccion_y_la_query_se_descartan_y_los_escapes_se_deshacen() {
        assert_eq!(
            resolve_link("mi%20nota.md#resumen", dir()),
            LinkTarget::Markdown(PathBuf::from("/docs/notas/mi nota.md"))
        );
        assert_eq!(
            resolve_link("tabla.csv?v=2", dir()),
            LinkTarget::Local(PathBuf::from("/docs/notas/tabla.csv"))
        );
        // Un `#` escapado es parte del nombre, no una seccion.
        assert_eq!(
            resolve_link("caso%231.md", dir()),
            LinkTarget::Markdown(PathBuf::from("/docs/notas/caso#1.md"))
        );
    }

    #[test]
    fn admite_el_destino_entre_angulos() {
        assert_eq!(
            resolve_link("<mi nota.md>", dir()),
            LinkTarget::Markdown(PathBuf::from("/docs/notas/mi nota.md"))
        );
    }

    #[test]
    fn web_y_correo_se_entregan_tal_cual() {
        assert_eq!(
            resolve_link("https://gitlab.gnome.org/World/apostrophe", dir()),
            LinkTarget::Web("https://gitlab.gnome.org/World/apostrophe".into())
        );
        assert_eq!(
            resolve_link("HTTP://ejemplo.com", None),
            LinkTarget::Web("HTTP://ejemplo.com".into())
        );
        assert_eq!(
            resolve_link("mailto:alguien@ejemplo.com", None),
            LinkTarget::Web("mailto:alguien@ejemplo.com".into())
        );
    }

    #[test]
    fn los_demas_esquemas_no_se_abren() {
        for href in [
            "javascript:alert(1)",
            "file:///C:/a.exe",
            "ms-settings:privacy",
            "data:text/html,x",
        ] {
            assert_eq!(resolve_link(href, dir()), LinkTarget::Unsupported, "{href}");
        }
    }

    #[test]
    fn anclas_y_enlaces_vacios_no_llevan_a_ningun_lado() {
        assert_eq!(resolve_link("#fn1", dir()), LinkTarget::Unsupported);
        assert_eq!(resolve_link("   ", dir()), LinkTarget::Unsupported);
        assert_eq!(resolve_link("?x=1", dir()), LinkTarget::Unsupported);
    }

    #[test]
    fn sin_guardar_una_ruta_relativa_no_tiene_base() {
        assert_eq!(resolve_link("otro.md", None), LinkTarget::NoBaseDir);
    }
}
