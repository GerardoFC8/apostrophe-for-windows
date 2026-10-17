//! Orquestacion del export (M12): que procesos se lanzan, en que directorio y con que
//! archivos temporales. Los argumentos los arma el dominio (`apostrophe_core::convert`);
//! aca solo se ejecutan.
//!
//! Vive aparte de `export_document` para poder probarlo SIN `AppHandle`: recibe una
//! fabrica de `std::process::Command` por nombre de sidecar. La app le pasa la del plugin
//! shell (misma resolucion de ruta y, en Windows, sin ventana de consola); los tests le
//! pasan los binarios que `tauri-build` copia junto al ejecutable de pruebas.

use std::path::Path;
use std::process::Command;

use apostrophe_core::convert::{
    export_args, resource_path_defaults, typst_compile_args, ExportFormat, ExportOptions,
};

/// Lo que el usuario pidio exportar.
pub(crate) struct ExportJob<'a> {
    pub format: ExportFormat,
    pub markdown: &'a str,
    pub output: &'a Path,
    /// Carpeta del documento; `None` si nunca se guardo.
    pub doc_dir: Option<&'a Path>,
    /// Hoja de estilos del preview (un `<style>`). Solo la usa HTML.
    pub css: &'a str,
    /// Color de acento del tema. Solo lo usa PDF.
    pub accent: &'a str,
}

/// Exporta `job` usando `work_dir` como directorio de trabajo PROPIO: lo crea, deja ahi
/// todos los temporales y lo borra al terminar, falle o no.
pub(crate) fn export(
    sidecar: &dyn Fn(&str) -> Result<Command, String>,
    job: &ExportJob,
    work_dir: &Path,
) -> Result<(), String> {
    std::fs::create_dir_all(work_dir)
        .map_err(|e| format!("no se pudo crear el directorio temporal de export: {e}"))?;
    // El borrado va en un `Drop` y no despues de la llamada: asi tambien ocurre si algo
    // entra en panico a mitad del export.
    let _cleanup = RemoveOnDrop(work_dir);
    export_in(sidecar, job, work_dir)
}

/// Borra el directorio al salir de alcance (best-effort: si falla, no hay a quien avisar).
struct RemoveOnDrop<'a>(&'a Path);

impl Drop for RemoveOnDrop<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
    }
}

fn export_in(
    sidecar: &dyn Fn(&str) -> Result<Command, String>,
    job: &ExportJob,
    work_dir: &Path,
) -> Result<(), String> {
    let path_str = |p: &Path| p.to_string_lossy().into_owned();

    // Solo sirve una carpeta que EXISTA. `Path::new("nota.md").parent()` es `Some("")`, y
    // la carpeta puede haber desaparecido (USB desconectado, carpeta renombrada): como
    // directorio actual, pandoc ni siquiera arrancaria y el error pareceria un sidecar
    // faltante. En esos casos se exporta como un documento sin guardar: sin sus imagenes.
    let doc_dir = job.doc_dir.filter(|d| d.is_dir());

    let input = work_dir.join("documento.md");
    std::fs::write(&input, job.markdown.as_bytes())
        .map_err(|e| format!("no se pudo escribir el temporal de export: {e}"))?;

    // La hoja de estilos del preview (un `<style>` que arma el frontend) va a otro
    // temporal: pandoc solo la acepta como ARCHIVO via --include-in-header. Solo se
    // genera para HTML — es el unico formato que entiende CSS.
    let header = if job.format == ExportFormat::Html && !job.css.is_empty() {
        let path = work_dir.join("estilo.head.html");
        std::fs::write(&path, job.css.as_bytes())
            .map_err(|e| format!("no se pudo escribir la hoja de estilos: {e}"))?;
        Some(path_str(&path))
    } else {
        None
    };

    // Solo el PDF: la carpeta del documento viaja en un archivo de defaults, porque
    // pandoc no puede correr desde ella (ver `PDF_MEDIA_DIR`).
    let defaults = match doc_dir {
        Some(dir) if job.format == ExportFormat::Pdf => {
            let path = work_dir.join("recursos.yaml");
            std::fs::write(&path, resource_path_defaults(&path_str(dir)))
                .map_err(|e| format!("no se pudo escribir los defaults de pandoc: {e}"))?;
            Some(path_str(&path))
        }
        _ => None,
    };

    let opts = ExportOptions {
        header_include: header.as_deref(),
        link_color: (!job.accent.is_empty()).then_some(job.accent),
        resource_defaults: defaults.as_deref(),
    };

    if job.format != ExportFormat::Pdf {
        // Desde la carpeta del documento, `![](gato.png)` resuelve solo: pandoc busca los
        // recursos en su directorio actual. Sin carpeta (documento nunca guardado) corre
        // en el de trabajo, donde no hay nada que encontrar: la imagen se cambia por su
        // texto alternativo en vez de resolverse contra un directorio cualquiera.
        //
        // Ojo: esto vale para los formatos que EMPAQUETAN la imagen (DOCX, ODT, EPUB). El
        // LaTeX conserva `\includegraphics{gato.png}` tal cual, y el HTML conserva el
        // `src` relativo hasta que el adaptador lo incrusta (`embed_html_images`).
        let mut args = vec![path_str(&input)];
        args.extend(export_args(job.format, &path_str(job.output), &opts));
        return run(sidecar, "pandoc", args, doc_dir.unwrap_or(work_dir));
    }

    // PDF, paso 1: pandoc escribe el fuente typst y extrae las imagenes, todo dentro del
    // directorio de trabajo. Tiene que correr DESDE ahi: `--extract-media` es relativo.
    let source = work_dir.join("documento.typ");
    let mut args = vec![path_str(&input)];
    args.extend(export_args(job.format, &path_str(&source), &opts));
    run(sidecar, "pandoc", args, work_dir)?;

    // Paso 2: typst compila con la raiz en el directorio de trabajo, que es donde viven
    // el fuente y sus imagenes. Lo lanzamos nosotros y no pandoc (`--pdf-engine`) porque
    // pandoc no le pasa `--root`.
    let args = typst_compile_args(
        &path_str(&source),
        &path_str(job.output),
        &path_str(work_dir),
    );
    run(sidecar, "typst", args, work_dir)
}

/// Lanza un sidecar en `cwd` y espera a que termine. Falla con su stderr, que es lo unico
/// util cuando pandoc o typst rechazan una entrada.
fn run(
    sidecar: &dyn Fn(&str) -> Result<Command, String>,
    name: &str,
    args: Vec<String>,
    cwd: &Path,
) -> Result<(), String> {
    let output = sidecar(name)?
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("no se pudo lanzar el sidecar {name}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{name} devolvio error (codigo {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Extremo a extremo con los sidecars REALES: es la unica forma de ver los fallos que
    //! nacen entre pandoc y typst, que los tests de argumentos del dominio no ven.
    //!
    //! `tauri-build` copia `binaries/<nombre>-<target triple>` junto al ejecutable de
    //! pruebas, sin el triple; se buscan ahi con la MISMA regla que usa la app
    //! (`sidecar_path`). Si no estan, el test se salta avisando en vez de fallar. En CI
    //! (`windows-latest`) los descargan `scripts/fetch-*.ps1` antes de `cargo test`.
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// PNG valido de 4x4 px, rojo.
    const PNG: [u8; 73] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x08, 0x02, 0x00, 0x00, 0x00, 0x26,
        0x93, 0x09, 0x29, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0x00, 0x47, 0x0c, 0xc4, 0x71, 0x00, 0xae, 0x93, 0x0f, 0xf1, 0xd0, 0x5f, 0x23,
        0x9e, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// Fabrica de comandos de los tests: los sidecars junto al ejecutable de pruebas, o
    /// `None` si faltan (el test se salta).
    fn sidecars() -> Option<impl Fn(&str) -> Result<Command, String>> {
        for name in ["pandoc", "typst"] {
            let path = crate::sidecar_path(name).ok()?;
            if !path.exists() {
                eprintln!(
                    "SALTADO: falta el sidecar {name} en {}. Ejecuta scripts/fetch-{name}.ps1.",
                    path.display()
                );
                return None;
            }
        }
        Some(|name: &str| crate::sidecar_path(name).map(Command::new))
    }

    /// Directorio temporal propio del test, vacio.
    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "apostrophe-export-test-{}-{}-{label}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Carpeta de documento con el separador de rutas de busqueda de pandoc en el nombre
    /// (`;` en Windows, `:` en el resto), un acento y un espacio: si alguna capa partiera
    /// o reinterpretara la ruta, la imagen no se encontraria.
    fn doc_dir_in(base: &Path) -> PathBuf {
        let name = if cfg!(windows) {
            "notas; año 1"
        } else {
            "notas: año 1"
        };
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("gato.png"), PNG).unwrap();
        dir
    }

    fn job<'a>(
        format: ExportFormat,
        md: &'a str,
        out: &'a Path,
        dir: Option<&'a Path>,
    ) -> ExportJob<'a> {
        ExportJob {
            format,
            markdown: md,
            output: out,
            doc_dir: dir,
            css: "",
            accent: "#3584e4",
        }
    }

    /// `true` si el PDF dibuja al menos una imagen (un XObject de tipo imagen).
    fn pdf_has_image(pdf: &[u8]) -> bool {
        let text = String::from_utf8_lossy(pdf);
        text.contains("/Subtype/Image") || text.contains("/Subtype /Image")
    }

    fn assert_pdf_with_image(out: &Path) {
        let pdf = std::fs::read(out).expect("no se genero el PDF");
        assert!(pdf.starts_with(b"%PDF"), "la salida no es un PDF");
        assert!(pdf_has_image(&pdf), "el PDF se genero SIN la imagen");
    }

    #[test]
    fn pdf_con_imagen_relativa_se_genera_y_la_incluye() {
        let Some(sidecar) = sidecars() else { return };
        let base = scratch("pdf-rel");
        let dir = doc_dir_in(&base);
        let out = base.join("salida.pdf");
        export(
            &sidecar,
            &job(
                ExportFormat::Pdf,
                "Hola\n\n![un gato](gato.png)\n",
                &out,
                Some(&dir),
            ),
            &base.join("trabajo"),
        )
        .expect("el export a PDF fallo");
        assert_pdf_with_image(&out);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn pdf_con_imagen_absoluta_se_genera_y_la_incluye() {
        let Some(sidecar) = sidecars() else { return };
        let base = scratch("pdf-abs");
        let dir = doc_dir_in(&base);
        // La ruta absoluta queda FUERA de la raiz de typst (y en Windows, quiza en otra
        // unidad). Entre `<>` porque la carpeta tiene espacios; con `/` porque en markdown
        // la barra invertida puede escapar el caracter que la sigue.
        let abs = dir.join("gato.png").to_string_lossy().replace('\\', "/");
        let md = format!("![un gato](<{abs}>)\n");
        let out = base.join("salida.pdf");
        export(
            &sidecar,
            &job(ExportFormat::Pdf, &md, &out, None),
            &base.join("trabajo"),
        )
        .expect("el export a PDF fallo");
        assert_pdf_with_image(&out);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn pdf_sin_documento_guardado_se_genera_sin_la_imagen() {
        // Sin carpeta no hay contra que resolver `gato.png`: pandoc la cambia por su texto
        // alternativo y el PDF sale igual. Lo que no puede pasar es que el export falle.
        let Some(sidecar) = sidecars() else { return };
        let base = scratch("pdf-untitled");
        let out = base.join("salida.pdf");
        export(
            &sidecar,
            &job(ExportFormat::Pdf, "![un gato](gato.png)\n", &out, None),
            &base.join("trabajo"),
        )
        .expect("el export a PDF fallo");
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn docx_con_imagen_relativa_la_incluye() {
        let Some(sidecar) = sidecars() else { return };
        let base = scratch("docx-rel");
        let dir = doc_dir_in(&base);
        let out = base.join("salida.docx");
        export(
            &sidecar,
            &job(
                ExportFormat::Docx,
                "![un gato](gato.png)\n",
                &out,
                Some(&dir),
            ),
            &base.join("trabajo"),
        )
        .expect("el export a DOCX fallo");
        // El zip guarda los nombres de sus entradas sin comprimir: si la imagen viajo,
        // hay una entrada bajo `word/media/`.
        let docx = std::fs::read(&out).unwrap();
        assert!(
            docx.windows(b"word/media/".len())
                .any(|w| w == b"word/media/"),
            "el DOCX se genero SIN la imagen"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn carpeta_vacia_o_inexistente_no_rompe_el_export() {
        // Regresion: `Path::new("nota.md").parent()` es `Some("")`, y una carpeta que ya
        // no existe (USB desconectado, carpeta renombrada) tampoco sirve como directorio
        // actual. Lanzar pandoc ahi fallaba con "No such file or directory", que parece un
        // sidecar faltante. Antes de este cambio esos documentos se exportaban (sin
        // imagenes), y asi tiene que seguir siendo.
        let Some(sidecar) = sidecars() else { return };
        let base = scratch("bad-dir");
        let missing = base.join("ya-no-existe");
        for dir in [Path::new(""), missing.as_path()] {
            for format in [ExportFormat::Docx, ExportFormat::Html, ExportFormat::Pdf] {
                let out = base.join(format!("salida.{}", format.extension()));
                export(
                    &sidecar,
                    &job(format, "![un gato](gato.png)\n", &out, Some(dir)),
                    &base.join("trabajo"),
                )
                .unwrap_or_else(|e| panic!("{format:?} con carpeta {dir:?}: {e}"));
                assert!(
                    out.exists(),
                    "{format:?} con carpeta {dir:?} no genero salida"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn el_directorio_de_trabajo_se_borra_aunque_algo_entre_en_panico() {
        // No necesita sidecars: la fabrica misma entra en panico en medio del export.
        let base = scratch("panic");
        let work = base.join("trabajo");
        let out = base.join("salida.docx");
        let boom = |_: &str| -> Result<Command, String> { panic!("fabrica rota") };
        let result = std::panic::catch_unwind(|| {
            let _ = export(&boom, &job(ExportFormat::Docx, "Hola\n", &out, None), &work);
        });
        assert!(result.is_err(), "el panico deberia propagarse");
        assert!(!work.exists(), "quedaron temporales en {}", work.display());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn html_incrusta_la_imagen_relativa() {
        // El HTML no empaqueta la imagen: la incrusta despues el adaptador, con el mismo
        // camino que el preview. Aca se prueba el par completo: pandoc + incrustado.
        let Some(sidecar) = sidecars() else { return };
        let base = scratch("html-rel");
        let dir = doc_dir_in(&base);
        let out = base.join("salida.html");
        export(
            &sidecar,
            &job(
                ExportFormat::Html,
                "![un gato](gato.png)\n",
                &out,
                Some(&dir),
            ),
            &base.join("trabajo"),
        )
        .expect("el export a HTML fallo");
        crate::embed_html_images_in(&out, Some(&dir)).expect("no se incrustaron las imagenes");
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(
            html.contains("data:image/png;base64,"),
            "el HTML no incrusto la imagen"
        );
        assert!(!html.contains("src=\"gato.png\""), "quedo el src relativo");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn el_directorio_de_trabajo_se_borra_al_terminar() {
        let Some(sidecar) = sidecars() else { return };
        let base = scratch("cleanup");
        let work = base.join("trabajo");
        let out = base.join("salida.pdf");
        export(
            &sidecar,
            &job(ExportFormat::Pdf, "Hola\n", &out, None),
            &work,
        )
        .unwrap();
        assert!(!work.exists(), "quedaron temporales en {}", work.display());
        let _ = std::fs::remove_dir_all(&base);
    }
}
