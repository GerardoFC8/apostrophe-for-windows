//! Resolucion de la imagen que hay bajo el cursor, para incrustarla en el popover de vista
//! previa inline (M19). Complementa a `inline::resolve_at`, que solo dice QUE hay ahi.
//!
//! Logica PURA: decide de donde saldria la imagen y si el contenido es realmente una, pero
//! no toca disco. Leer el archivo y armar el data URI es tarea del adaptador.
//!
//! Decision de alcance: las imagenes REMOTAS no se descargan. Posar el cursor no debe
//! disparar una peticion de red — un documento ajeno con una imagen remota se convertiria en
//! un pixel de rastreo que delata cuando y desde donde se lo lee.

use std::path::{Path, PathBuf};

/// Tope de tamano para una imagen del popover. Por encima se muestra la ruta y ya: el hover
/// tiene que sentirse instantaneo, y codificar decenas de megas en base64 lo congelaria.
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

/// Cuanto se mira del contenido para reconocer un SVG. Basta para saltar un BOM, una
/// declaracion XML y algun comentario antes de la etiqueta raiz.
const SVG_SNIFF_BYTES: usize = 1024;

/// Formatos que el popover sabe incrustar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
}

impl ImageFormat {
    /// Tipo MIME para el data URI.
    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Svg => "image/svg+xml",
        }
    }

    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(ImageFormat::Png),
            "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
            "gif" => Some(ImageFormat::Gif),
            "webp" => Some(ImageFormat::Webp),
            "svg" => Some(ImageFormat::Svg),
            _ => None,
        }
    }
}

/// De donde saldria la imagen de un `![alt](url)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// Archivo del disco, con la ruta ya resuelta y el formato que promete la extension.
    Local { path: PathBuf, format: ImageFormat },
    /// La URL tiene esquema (`http:`, `https:`, `file:`…). No se descarga; el popover
    /// se queda mostrando la direccion.
    Remote,
    /// Ruta relativa en un documento sin guardar: no hay directorio contra el que resolverla.
    NoBaseDir,
    /// Sin extension o con una que no es de imagen.
    Unsupported,
}

/// Que hacer con la `url` de una imagen, sabiendo en que directorio vive el documento.
///
/// `doc_dir` es `None` mientras el documento no se haya guardado nunca.
///
/// No se restringe a que la ruta quede DENTRO de `doc_dir`: `../assets/logo.png` es un uso
/// legitimo y corriente. Lo que acota el riesgo es que solo se aceptan extensiones de imagen
/// y que despues el contenido tiene que coincidir con ellas (ver `sniff_format`).
pub fn resolve_image(url: &str, doc_dir: Option<&Path>) -> ImageSource {
    // Markdown admite envolver la ruta en angulos cuando tiene espacios: `![a](<mi foto.png>)`.
    let url = url.trim();
    let url = url.strip_prefix('<').unwrap_or(url);
    let url = url.strip_suffix('>').unwrap_or(url).trim();
    if url.is_empty() {
        return ImageSource::Unsupported;
    }

    // El orden importa: `C:\fotos\a.png` tambien encaja en la forma "esquema:" de una URL.
    // Si se probara el esquema primero, toda ruta de Windows se reportaria como remota.
    if !is_windows_drive(url) && has_scheme(url) {
        return ImageSource::Remote;
    }

    let decoded = percent_decode(url);
    let Some(format) = extension_of(&decoded).and_then(ImageFormat::from_extension) else {
        return ImageSource::Unsupported;
    };

    if is_absolute(&decoded) {
        return ImageSource::Local { path: PathBuf::from(decoded), format };
    }
    match doc_dir {
        Some(dir) => ImageSource::Local { path: dir.join(decoded), format },
        None => ImageSource::NoBaseDir,
    }
}

/// Formato que declara el CONTENIDO, por su firma. `None` si no es ninguno de los aceptados.
///
/// Existe para no confiar en la extension: un archivo llamado `.png` puede ser cualquier
/// cosa, y lo que se incrusta va directo a un `<img>` del webview.
pub fn sniff_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(ImageFormat::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageFormat::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(ImageFormat::Gif);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(ImageFormat::Webp);
    }
    // El SVG es texto: no tiene firma fija, puede venir con BOM, declaracion XML o
    // comentarios antes de la raiz. Se busca la etiqueta en el arranque del archivo.
    let head = &bytes[..bytes.len().min(SVG_SNIFF_BYTES)];
    if String::from_utf8_lossy(head).to_ascii_lowercase().contains("<svg") {
        return Some(ImageFormat::Svg);
    }
    None
}

/// Reemplaza el `src` de cada `<img>` del HTML por lo que devuelva `resolve`.
///
/// El preview es un `iframe` con `srcdoc` en un sandbox: ahi dentro un `src="C:/fotos/a.png"`
/// se interpreta como una URL relativa, no como un archivo, y la imagen sale rota. Se
/// resuelve incrustando el contenido, el mismo camino que el popover — asi el webview sigue
/// sin poder leer del disco.
///
/// `resolve` recibe el `src` original y devuelve el reemplazo, o `None` para dejarlo como
/// esta (imagen remota, archivo ausente, demasiado grande). Que sea un callback mantiene el
/// I/O afuera: aca solo vive el recorrido del HTML, que es lo que conviene tener con pruebas.
pub fn rewrite_img_srcs(html: &str, mut resolve: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find("<img") {
        out.push_str(&rest[..start]);
        rest = &rest[start..];

        // Sin cierre el HTML esta truncado: se copia lo que queda y se corta, en vez de
        // arriesgar un reemplazo sobre una etiqueta a medias.
        let Some(close) = rest.find('>') else {
            break;
        };
        let (tag, tail) = rest.split_at(close + 1);
        rest = tail;

        match find_src_value(tag) {
            Some((from, to)) => match resolve(&tag[from..to]) {
                Some(replacement) => {
                    out.push_str(&tag[..from]);
                    out.push_str(&replacement);
                    out.push_str(&tag[to..]);
                }
                None => out.push_str(tag),
            },
            None => out.push_str(tag),
        }
    }

    out.push_str(rest);
    out
}

/// Rango del VALOR del atributo `src` dentro de una etiqueta, sin las comillas.
///
/// Se exige un espacio delante para no confundirlo con `data-src=` ni con un `src=` que
/// aparezca dentro de otro atributo.
fn find_src_value(tag: &str) -> Option<(usize, usize)> {
    let bytes = tag.as_bytes();
    let mut from = 0;
    while let Some(rel) = tag[from..].find("src=") {
        let at = from + rel;
        let quote = bytes.get(at + 4).copied();
        let preceded_by_space = at > 0 && bytes[at - 1].is_ascii_whitespace();
        if preceded_by_space && (quote == Some(b'"') || quote == Some(b'\'')) {
            let start = at + 5;
            let end = start + tag[start..].find(quote? as char)?;
            return Some((start, end));
        }
        from = at + 4;
    }
    None
}

/// `true` si la ruta es absoluta en POSIX o en Windows.
///
/// No se usa `Path::is_absolute` porque depende de la plataforma que COMPILA: en WSL, donde
/// corren las pruebas, `C:\fotos\a.png` seria "relativa" y se resolveria contra el documento.
/// La app es de Windows; la respuesta tiene que ser la misma en los dos lados.
fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || path.starts_with('\\') || is_windows_drive(path)
}

/// Forma `X:/` o `X:\` del comienzo de una ruta de Windows.
fn is_windows_drive(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

/// `true` si la cadena arranca con un esquema de URL (`http:`, `https:`, `data:`…).
fn has_scheme(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    let scheme = &url[..colon];
    !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Deshace los `%XX` de la ruta. Markdown escapa asi los espacios (`mi%20foto.png`), y sin
/// deshacerlo se buscaria un archivo que no existe.
///
/// Un `%` suelto o mal formado se deja tal cual: en Windows es un caracter valido de nombre.
fn percent_decode(url: &str) -> String {
    let bytes = url.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| url.to_string())
}

/// Extension del ultimo componente de la ruta, sin el punto.
fn extension_of(path: &str) -> Option<&str> {
    let name = path.rsplit(['/', '\\']).next()?;
    let dot = name.rfind('.')?;
    if dot == 0 {
        return None; // `.gitignore` es el nombre, no una extension
    }
    Some(&name[dot + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        PathBuf::from("/home/ana/notas")
    }

    #[test]
    fn una_ruta_relativa_se_resuelve_contra_el_directorio_del_documento() {
        assert_eq!(
            resolve_image("gato.png", Some(&dir())),
            ImageSource::Local {
                path: PathBuf::from("/home/ana/notas/gato.png"),
                format: ImageFormat::Png,
            }
        );
    }

    #[test]
    fn sin_documento_guardado_una_ruta_relativa_no_tiene_contra_que_resolverse() {
        // Se distingue de `Unsupported` a proposito: la imagen puede existir perfectamente,
        // lo que falta es haber guardado el documento.
        assert_eq!(resolve_image("gato.png", None), ImageSource::NoBaseDir);
    }

    #[test]
    fn una_ruta_absoluta_se_usa_tal_cual_aunque_haya_documento() {
        assert_eq!(
            resolve_image("/var/img/a.jpg", Some(&dir())),
            ImageSource::Local {
                path: PathBuf::from("/var/img/a.jpg"),
                format: ImageFormat::Jpeg,
            }
        );
    }

    #[test]
    fn una_ruta_absoluta_de_windows_se_reconoce_corra_donde_corra() {
        // Con `Path::is_absolute` esta prueba fallaria en WSL: alli `C:\...` es relativa y la
        // imagen se buscaria dentro del directorio del documento.
        assert_eq!(
            resolve_image(r"C:\fotos\a.png", Some(&dir())),
            ImageSource::Local {
                path: PathBuf::from(r"C:\fotos\a.png"),
                format: ImageFormat::Png,
            }
        );
    }

    #[test]
    fn una_url_remota_no_se_descarga() {
        for url in [
            "https://example.com/a.png",
            "http://example.com/a.png",
            "data:image/png;base64,AAAA",
        ] {
            assert_eq!(resolve_image(url, Some(&dir())), ImageSource::Remote, "con {url}");
        }
    }

    #[test]
    fn una_extension_que_no_es_de_imagen_no_se_previsualiza() {
        for url in ["informe.pdf", "notas.md", "sin-extension", ".gitignore"] {
            assert_eq!(resolve_image(url, Some(&dir())), ImageSource::Unsupported, "con {url}");
        }
    }

    #[test]
    fn la_extension_no_distingue_mayusculas() {
        let ImageSource::Local { format, .. } = resolve_image("FOTO.PNG", Some(&dir())) else {
            panic!("deberia ser local");
        };
        assert_eq!(format, ImageFormat::Png);
    }

    #[test]
    fn los_espacios_escapados_de_la_url_se_deshacen() {
        // Markdown escribe `mi%20gato.png`; en disco el archivo se llama con el espacio.
        assert_eq!(
            resolve_image("mi%20gato.png", Some(&dir())),
            ImageSource::Local {
                path: PathBuf::from("/home/ana/notas/mi gato.png"),
                format: ImageFormat::Png,
            }
        );
    }

    #[test]
    fn un_porcentaje_mal_formado_se_deja_como_esta() {
        // En Windows `%` es un caracter valido de nombre de archivo.
        assert_eq!(
            resolve_image("100%exito.png", Some(&dir())),
            ImageSource::Local {
                path: PathBuf::from("/home/ana/notas/100%exito.png"),
                format: ImageFormat::Png,
            }
        );
    }

    #[test]
    fn se_aceptan_los_angulos_con_que_markdown_envuelve_rutas_con_espacios() {
        assert_eq!(
            resolve_image("<mi gato.png>", Some(&dir())),
            ImageSource::Local {
                path: PathBuf::from("/home/ana/notas/mi gato.png"),
                format: ImageFormat::Png,
            }
        );
    }

    #[test]
    fn una_url_vacia_no_rompe() {
        assert_eq!(resolve_image("", Some(&dir())), ImageSource::Unsupported);
        assert_eq!(resolve_image("   ", Some(&dir())), ImageSource::Unsupported);
    }

    #[test]
    fn cada_formato_declara_su_mime() {
        assert_eq!(ImageFormat::Png.mime(), "image/png");
        assert_eq!(ImageFormat::Jpeg.mime(), "image/jpeg");
        assert_eq!(ImageFormat::Gif.mime(), "image/gif");
        assert_eq!(ImageFormat::Webp.mime(), "image/webp");
        assert_eq!(ImageFormat::Svg.mime(), "image/svg+xml");
    }

    #[test]
    fn el_contenido_se_reconoce_por_su_firma() {
        assert_eq!(sniff_format(b"\x89PNG\r\n\x1a\nresto"), Some(ImageFormat::Png));
        assert_eq!(sniff_format(b"\xFF\xD8\xFF\xE0resto"), Some(ImageFormat::Jpeg));
        assert_eq!(sniff_format(b"GIF89a resto"), Some(ImageFormat::Gif));
        assert_eq!(sniff_format(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some(ImageFormat::Webp));
        assert_eq!(sniff_format(b"<svg xmlns=\"...\"></svg>"), Some(ImageFormat::Svg));
    }

    #[test]
    fn un_svg_se_reconoce_con_declaracion_xml_delante() {
        let svg = b"\xEF\xBB\xBF<?xml version=\"1.0\"?>\n<!-- hecho a mano -->\n<SVG></SVG>";
        assert_eq!(sniff_format(svg), Some(ImageFormat::Svg));
    }

    #[test]
    fn un_archivo_que_no_es_imagen_no_pasa_la_firma() {
        // El caso que importa: algo llamado `foto.png` que en realidad es otra cosa. La
        // extension sola no alcanza para decidir que se incrusta en el webview.
        assert_eq!(sniff_format(b"MZ\x90\x00ejecutable"), None);
        assert_eq!(sniff_format(b"solo texto plano"), None);
        assert_eq!(sniff_format(b""), None);
    }

    #[test]
    fn una_firma_valida_pero_truncada_no_se_confunde() {
        assert_eq!(sniff_format(b"RIFF\x00\x00\x00\x00"), None); // RIFF sin el marcador WEBP
        assert_eq!(sniff_format(b"\x89PNG"), None);
    }

    /// Resolutor de mentira para las pruebas del reescritor: incrusta todo lo que no sea
    /// remoto, para que se vea que el HTML se rearma bien.
    fn fake(src: &str) -> Option<String> {
        if src.starts_with("http") {
            None
        } else {
            Some(format!("data:image/png;base64,AAAA({src})"))
        }
    }

    #[test]
    fn el_src_de_una_imagen_se_reemplaza_conservando_el_resto_de_la_etiqueta() {
        let html = r#"<p>antes</p><img src="a.png" alt="gato" /><p>despues</p>"#;
        assert_eq!(
            rewrite_img_srcs(html, fake),
            r#"<p>antes</p><img src="data:image/png;base64,AAAA(a.png)" alt="gato" /><p>despues</p>"#
        );
    }

    #[test]
    fn pandoc_parte_la_etiqueta_en_varias_lineas_y_se_reescribe_igual() {
        // Es la forma REAL de la salida de pandoc: atributos separados por saltos de linea.
        let html = "<figure>\n<img\nsrc=\"C:/fotos/a.png\"\nalt=\"cap\" />\n</figure>";
        let out = rewrite_img_srcs(html, fake);
        assert!(out.contains("src=\"data:image/png;base64,AAAA(C:/fotos/a.png)\""), "{out}");
        assert!(out.contains("alt=\"cap\""), "el resto de la etiqueta se conserva: {out}");
    }

    #[test]
    fn lo_que_el_resolutor_rechaza_queda_intacto() {
        // Una imagen remota tiene que seguir apuntando a su URL: el iframe la cargara solo.
        let html = r#"<img src="https://example.com/a.png" alt="x" />"#;
        assert_eq!(rewrite_img_srcs(html, fake), html);
    }

    #[test]
    fn se_reescriben_todas_las_imagenes_del_documento() {
        let html = r#"<img src="a.png" /><p>medio</p><img src="b.png" />"#;
        let out = rewrite_img_srcs(html, fake);
        assert!(out.contains("AAAA(a.png)"), "{out}");
        assert!(out.contains("AAAA(b.png)"), "{out}");
        assert!(out.contains("<p>medio</p>"), "no se pierde lo que hay en el medio: {out}");
    }

    #[test]
    fn no_se_confunde_con_otros_atributos_ni_otras_etiquetas() {
        // `data-src` termina en `src=`; sin el guard del espacio delante, se reescribiria.
        let html = r#"<img data-src="a.png" alt="x" /><video src="v.mp4"></video>"#;
        assert_eq!(rewrite_img_srcs(html, fake), html);
    }

    #[test]
    fn se_aceptan_las_comillas_simples() {
        let html = "<img src='a.png' />";
        assert_eq!(rewrite_img_srcs(html, fake), "<img src='data:image/png;base64,AAAA(a.png)' />");
    }

    #[test]
    fn un_html_sin_imagenes_vuelve_igual() {
        let html = "<h1>Titulo</h1><p>Texto con &lt;img&gt; escapado.</p>";
        assert_eq!(rewrite_img_srcs(html, fake), html);
    }

    #[test]
    fn una_etiqueta_truncada_no_pierde_texto_ni_entra_en_bucle() {
        // Si pandoc devolviera algo cortado, el preview tiene que mostrar lo que haya.
        let html = "<p>ok</p><img src=\"a.png\"";
        assert_eq!(rewrite_img_srcs(html, fake), html);
    }

    #[test]
    fn una_imagen_sin_src_se_deja_como_esta() {
        let html = r#"<img alt="sin fuente" />"#;
        assert_eq!(rewrite_img_srcs(html, fake), html);
    }
}
