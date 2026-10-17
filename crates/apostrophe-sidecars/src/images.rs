//! Imagenes locales incrustadas como data URI: el popover de imagen y el preview de la
//! GUI, y el HTML exportado (GUI y CLI) recorren el MISMO camino.

use std::path::Path;

/// Lee el archivo, lo valida y lo codifica. Es I/O bloqueante: la GUI lo llama desde
/// `spawn_blocking`.
///
/// El formato con el que se incrusta lo decide el CONTENIDO, no la extension. La extension
/// solo filtra que archivos vale la pena abrir; mandarla a decidir el MIME rechazaria una
/// imagen perfectamente valida por tener el nombre cambiado — un `.png` que en realidad es
/// WebP es de lo mas comun en material descargado, y los navegadores lo muestran sin
/// chistar porque miran los bytes. La garantia que importa se mantiene: solo se incrusta lo
/// que `sniff_format` reconoce como una imagen soportada.
///
/// El error es un CODIGO estable (`remote`, `not-found`, …), no una frase: la copia visible
/// vive en el catalogo de i18n del frontend.
pub fn image_data_uri(url: &str, doc_dir: Option<&Path>) -> Result<String, String> {
    use apostrophe_core::image::{resolve_image, sniff_format, ImageSource, MAX_IMAGE_BYTES};
    use base64::Engine;

    let path = match resolve_image(url, doc_dir) {
        ImageSource::Local { path, .. } => path,
        ImageSource::Remote => return Err("remote".into()),
        ImageSource::NoBaseDir => return Err("no-base-dir".into()),
        ImageSource::Unsupported => return Err("unsupported".into()),
    };

    // El tamano se mira ANTES de leer: un archivo de cientos de megas no debe llegar a
    // memoria solo para descubrir despues que no cabia en el popover.
    let size = std::fs::metadata(&path).map_err(|_| "not-found")?.len();
    if size > MAX_IMAGE_BYTES {
        return Err("too-large".into());
    }

    let bytes = std::fs::read(&path).map_err(|_| "unreadable")?;
    let format = sniff_format(&bytes).ok_or("unsupported")?;

    Ok(format!(
        "data:{};base64,{}",
        format.mime(),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Reescribe el HTML ya exportado incrustando cada imagen local como data URI.
///
/// Es el MISMO camino que el preview (`rewrite_img_srcs` + `image_data_uri`), y no
/// `--embed-resources` de pandoc, a proposito: ese flag tambien se traga el bundle de
/// MathJax, lo que obliga a tener red en el momento de exportar y engorda el archivo de
/// unos pocos KB a mas de un mega. Aqui solo viajan las imagenes; las formulas se siguen
/// resolviendo con el script de MathJax. Ver `html_nunca_incrusta_recursos` en el dominio.
///
/// Lo que no se pueda incrustar —remota, ausente o por encima del tope— conserva su `src`
/// original: una imagen rota en el export es mejor que un export que falla entero.
///
/// Sincrono y sin runtime de Tauri: la GUI lo envuelve en `spawn_blocking`.
pub fn embed_html_images_in(path: &Path, doc_dir: Option<&Path>) -> Result<(), String> {
    let html = std::fs::read_to_string(path)
        .map_err(|e| format!("no se pudo releer el HTML exportado: {e}"))?;
    let embedded =
        apostrophe_core::image::rewrite_img_srcs(&html, |src| image_data_uri(src, doc_dir).ok());
    std::fs::write(path, embedded)
        .map_err(|e| format!("no se pudo escribir el HTML exportado: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Basta la firma: el formato lo decide el contenido (`sniff_format`), no un PNG valido.
    const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// Carpeta de documento propia del test, con `gato.png` adentro.
    fn doc_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "apostrophe-images-test-{}-{}-{label}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("gato.png"), PNG_SIGNATURE).unwrap();
        dir
    }

    #[test]
    fn una_imagen_relativa_se_vuelve_data_uri() {
        let dir = doc_dir("rel");
        let uri = image_data_uri("gato.png", Some(&dir)).unwrap();
        assert_eq!(uri, "data:image/png;base64,iVBORw0KGgoAAA==");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn los_errores_son_codigos_estables() {
        // Son codigos y no frases: la copia visible la pone el catalogo de i18n.
        let dir = doc_dir("errores");
        assert_eq!(
            image_data_uri("https://x.org/a.png", Some(&dir)).unwrap_err(),
            "remote"
        );
        assert_eq!(image_data_uri("gato.png", None).unwrap_err(), "no-base-dir");
        assert_eq!(
            image_data_uri("notas.txt", Some(&dir)).unwrap_err(),
            "unsupported"
        );
        assert_eq!(
            image_data_uri("perro.png", Some(&dir)).unwrap_err(),
            "not-found"
        );
        std::fs::write(dir.join("falso.png"), b"no soy una imagen").unwrap();
        assert_eq!(
            image_data_uri("falso.png", Some(&dir)).unwrap_err(),
            "unsupported"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn el_html_exportado_incrusta_lo_local_y_conserva_lo_demas() {
        let dir = doc_dir("html");
        let html = dir.join("salida.html");
        std::fs::write(
            &html,
            "<p><img src=\"gato.png\" /><img src=\"https://x.org/a.png\" /></p>",
        )
        .unwrap();
        embed_html_images_in(&html, Some(&dir)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&html).unwrap(),
            "<p><img src=\"data:image/png;base64,iVBORw0KGgoAAA==\" />\
             <img src=\"https://x.org/a.png\" /></p>"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_html_que_no_existe_falla_con_mensaje() {
        let dir = doc_dir("sin-html");
        let err = embed_html_images_in(&dir.join("no-existe.html"), Some(&dir)).unwrap_err();
        assert!(
            err.starts_with("no se pudo releer el HTML exportado: "),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
