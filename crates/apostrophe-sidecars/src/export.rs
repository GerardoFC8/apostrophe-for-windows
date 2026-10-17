//! Orquestacion del export (M12): que procesos se lanzan, en que directorio y con que
//! archivos temporales. Los argumentos los arma el dominio (`apostrophe_core::convert`);
//! aca solo se ejecutan.
//!
//! No conoce Tauri, para que lo compartan la GUI y la CLI y para probarlo SIN `AppHandle`:
//! recibe una fabrica de `std::process::Command` por nombre de sidecar. La GUI le pasa la
//! del plugin shell (misma resolucion de ruta y, en Windows, sin ventana de consola); la
//! CLI y los tests, `std_command` sobre la carpeta de instalacion, que hace lo mismo.

use std::fmt;
use std::path::Path;
use std::process::Command;

use apostrophe_core::convert::{
    export_args, resource_path_defaults, typst_compile_args, ExportFormat, ExportOptions,
};

/// Por que fallo un export. Tipado para que la CLI lo traduzca a codigos de salida
/// estables; la GUI muestra `to_string()`, que conserva los mensajes de siempre.
#[derive(Debug)]
pub enum ExportError {
    /// No se pudo preparar el export: el directorio de trabajo, un temporal, o la fabrica
    /// no pudo dar el comando (el mensaje es el de la fabrica, sin cambios).
    Setup(String),
    /// El sistema no pudo lanzar el proceso (el binario no existe, no hay permisos…).
    Launch { program: String, message: String },
    /// El proceso corrio y termino con error: lo unico util es su stderr.
    Failed {
        program: String,
        code: Option<i32>,
        stderr: String,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(message) => f.write_str(message),
            Self::Launch { program, message } => {
                write!(f, "no se pudo lanzar el sidecar {program}: {message}")
            }
            Self::Failed {
                program,
                code,
                stderr,
            } => write!(f, "{program} devolvio error (codigo {code:?}): {stderr}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Lo que el usuario pidio exportar.
pub struct ExportJob<'a> {
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
pub fn export(
    sidecar: &dyn Fn(&str) -> Result<Command, String>,
    job: &ExportJob,
    work_dir: &Path,
) -> Result<(), ExportError> {
    std::fs::create_dir_all(work_dir).map_err(|e| {
        ExportError::Setup(format!(
            "no se pudo crear el directorio temporal de export: {e}"
        ))
    })?;
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
) -> Result<(), ExportError> {
    let path_str = |p: &Path| p.to_string_lossy().into_owned();

    // Solo sirve una carpeta que EXISTA. `Path::new("nota.md").parent()` es `Some("")`, y
    // la carpeta puede haber desaparecido (USB desconectado, carpeta renombrada): como
    // directorio actual, pandoc ni siquiera arrancaria y el error pareceria un sidecar
    // faltante. En esos casos se exporta como un documento sin guardar: sin sus imagenes.
    let doc_dir = job.doc_dir.filter(|d| d.is_dir());

    let input = work_dir.join("documento.md");
    std::fs::write(&input, job.markdown.as_bytes())
        .map_err(|e| setup(format!("no se pudo escribir el temporal de export: {e}")))?;

    // La hoja de estilos del preview (un `<style>` que arma el frontend) va a otro
    // temporal: pandoc solo la acepta como ARCHIVO via --include-in-header. Solo se
    // genera para HTML — es el unico formato que entiende CSS.
    let header = if job.format == ExportFormat::Html && !job.css.is_empty() {
        let path = work_dir.join("estilo.head.html");
        std::fs::write(&path, job.css.as_bytes())
            .map_err(|e| setup(format!("no se pudo escribir la hoja de estilos: {e}")))?;
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
                .map_err(|e| setup(format!("no se pudo escribir los defaults de pandoc: {e}")))?;
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

/// Atajo para los temporales que no se pudieron escribir.
fn setup(message: String) -> ExportError {
    ExportError::Setup(message)
}

/// Lanza un sidecar en `cwd` y espera a que termine. Falla con su stderr, que es lo unico
/// util cuando pandoc o typst rechazan una entrada.
fn run(
    sidecar: &dyn Fn(&str) -> Result<Command, String>,
    name: &str,
    args: Vec<String>,
    cwd: &Path,
) -> Result<(), ExportError> {
    let output = sidecar(name)
        .map_err(ExportError::Setup)?
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| ExportError::Launch {
            program: name.to_string(),
            message: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(ExportError::Failed {
            program: name.to_string(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Extremo a extremo con los sidecars REALES: es la unica forma de ver los fallos que
    //! nacen entre pandoc y typst, que los tests de argumentos del dominio no ven.
    //!
    //! El `tauri-build` de `src-tauri` copia `binaries/<nombre>-<target triple>` a
    //! `target/<perfil>/`, sin el triple; se buscan ahi con la MISMA regla que usan la app
    //! y la CLI (`install_dir`). Con `cargo test --workspace` ya estan copiados; con
    //! `-p apostrophe-sidecars`, solo si antes se compilo la app. Si no estan, el test se
    //! salta avisando en vez de fallar. En CI (`windows-latest`) los descargan
    //! `scripts/fetch-*.ps1` antes de `cargo test`.
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

    /// Fabrica de comandos de los tests: la misma de la CLI (`std_command` sobre la
    /// carpeta de instalacion), o `None` si faltan los sidecars (el test se salta).
    fn sidecars() -> Option<impl Fn(&str) -> Result<Command, String>> {
        let dir = crate::install_dir().ok()?;
        for name in ["pandoc", "typst"] {
            let path = crate::sidecar_path_in(&dir, name);
            if !path.exists() {
                eprintln!(
                    "SALTADO: falta el sidecar {name} en {}. Ejecuta scripts/fetch-{name}.ps1.",
                    path.display()
                );
                return None;
            }
        }
        Some(move |name: &str| Ok(crate::std_command(&dir, name)))
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
        crate::images::embed_html_images_in(&out, Some(&dir))
            .expect("no se incrustaron las imagenes");
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(
            html.contains("data:image/png;base64,"),
            "el HTML no incrusto la imagen"
        );
        assert!(!html.contains("src=\"gato.png\""), "quedo el src relativo");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn los_mensajes_de_error_son_los_que_ya_mostraba_la_gui() {
        // La GUI muestra `to_string()` tal cual: tipar el error no debe cambiarle la copia.
        let setup =
            ExportError::Setup("no se pudo crear el directorio temporal de export: x".into());
        assert_eq!(
            setup.to_string(),
            "no se pudo crear el directorio temporal de export: x"
        );
        let launch = ExportError::Launch {
            program: "pandoc".into(),
            message: "No such file or directory".into(),
        };
        assert_eq!(
            launch.to_string(),
            "no se pudo lanzar el sidecar pandoc: No such file or directory"
        );
        let failed = ExportError::Failed {
            program: "typst".into(),
            code: Some(1),
            stderr: "error: boom".into(),
        };
        assert_eq!(
            failed.to_string(),
            "typst devolvio error (codigo Some(1)): error: boom"
        );
        let killed = ExportError::Failed {
            program: "pandoc".into(),
            code: None,
            stderr: String::new(),
        };
        assert_eq!(killed.to_string(), "pandoc devolvio error (codigo None): ");
    }

    #[test]
    fn el_error_de_la_fabrica_llega_sin_cambios() {
        // La fabrica de la GUI ya arma su propio mensaje ("no se encontro el sidecar …").
        let base = scratch("factory");
        let out = base.join("salida.docx");
        let broken = |name: &str| -> Result<Command, String> {
            Err(format!("no se encontro el sidecar {name}: falta"))
        };
        let err = export(
            &broken,
            &job(ExportFormat::Docx, "Hola\n", &out, None),
            &base.join("t"),
        )
        .unwrap_err();
        assert!(
            matches!(&err, ExportError::Setup(m) if m == "no se encontro el sidecar pandoc: falta"),
            "{err:?}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn un_sidecar_que_no_arranca_es_un_error_de_lanzamiento() {
        let base = scratch("launch");
        let out = base.join("salida.html");
        let missing = base.join("no-existe");
        let factory = |_: &str| -> Result<Command, String> { Ok(Command::new(&missing)) };
        let err = export(
            &factory,
            &job(ExportFormat::Html, "Hola\n", &out, None),
            &base.join("t"),
        )
        .unwrap_err();
        assert!(
            matches!(&err, ExportError::Launch { program, .. } if program == "pandoc"),
            "{err:?}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn un_sidecar_que_falla_devuelve_su_codigo_y_su_stderr() {
        // `sh -c` recibe los argumentos de pandoc como `$0`, `$1`…, y los ignora.
        let base = scratch("failed");
        let out = base.join("salida.html");
        let factory = |_: &str| -> Result<Command, String> {
            let mut command = Command::new("sh");
            command.args(["-c", "echo 'entrada rota' >&2; exit 3"]);
            Ok(command)
        };
        let err = export(
            &factory,
            &job(ExportFormat::Html, "Hola\n", &out, None),
            &base.join("t"),
        )
        .unwrap_err();
        match err {
            ExportError::Failed {
                program,
                code,
                stderr,
            } => {
                assert_eq!(program, "pandoc");
                assert_eq!(code, Some(3));
                assert_eq!(stderr, "entrada rota\n");
            }
            other => panic!("se esperaba Failed: {other:?}"),
        }
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
