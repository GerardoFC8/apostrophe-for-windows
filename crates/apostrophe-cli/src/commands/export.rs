//! `export`: el mismo export de la GUI (`apostrophe_sidecars::export`), con los pandoc y
//! typst de la instalacion. Las imagenes locales se resuelven igual: contra la carpeta del
//! documento, que por stdin no existe (como un documento sin guardar).

use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use apostrophe_core::convert::ExportFormat;
use apostrophe_sidecars::export::{export, ExportJob};
use apostrophe_sidecars::images::embed_html_images_in;
use apostrophe_sidecars::{sidecar_path_in, std_command};
use serde::Serialize;

use crate::args::{ExportArgs, Format};
use crate::commands::Report;
use crate::error::{CliError, ErrorCode};
use crate::{input, Env};

/// Color de los enlaces del PDF: el acento del tema claro (`--accent` en `styles.css`).
/// La CLI no tiene tema: usa el de la GUI por defecto.
const ACCENT: &str = "#3584e4";

/// Distingue los directorios de trabajo de varios export en el mismo proceso (los tests).
static EXPORT_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize)]
pub struct ExportReport {
    /// Ruta absoluta del archivo generado.
    pub output: String,
    pub format: &'static str,
}

pub fn execute(
    args: &ExportArgs,
    env: &Env,
    stdin: &mut dyn Read,
) -> Result<ExportReport, CliError> {
    let format = resolve_format(args)?;
    let input = input::read(&args.input, &env.cwd, stdin)?;
    // Absoluta: pandoc corre en la carpeta del documento o en una temporal, no en `cwd`.
    let output = input::absolutize(&env.cwd, &args.output);
    check_output(&output)?;
    let kind = format.export_format();
    check_sidecars(&env.install_dir, kind)?;

    let job = ExportJob {
        format: kind,
        markdown: &input.text,
        output: &output,
        doc_dir: input.dir(),
        css: "",
        accent: ACCENT,
    };
    let sidecar = |name: &str| Ok(std_command(&env.install_dir, name));
    export(&sidecar, &job, &work_dir())?;

    // Como en la GUI: el HTML conserva el `src` relativo de sus imagenes hasta que se
    // incrustan; los demas formatos ya las empaquetan.
    if kind == ExportFormat::Html {
        embed_html_images_in(&output, input.dir()).map_err(|detail| {
            CliError::io(format!(
                "cannot embed the images in {} ({detail})",
                output.display()
            ))
        })?;
    }

    Ok(ExportReport {
        output: input::display_path(&output),
        format: format.name(),
    })
}

/// `--to`, o el formato que dice la extension de `-o`.
fn resolve_format(args: &ExportArgs) -> Result<Format, CliError> {
    if let Some(format) = args.to {
        return Ok(format);
    }
    let extension = args
        .output
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    Format::from_extension(&extension).ok_or_else(|| {
        let names: Vec<&str> = Format::ALL.iter().map(|f| f.name()).collect();
        CliError::usage(format!(
            "cannot tell the export format from {}; use --to with one of: {}",
            args.output.display(),
            names.join(", ")
        ))
    })
}

/// La salida tiene que poder escribirse. Si no (su carpeta no existe, el archivo es de solo
/// lectura, la carpeta no da permiso o, en Windows, otro programa tiene el archivo
/// abierto), pandoc o typst fallarian y pareceria un fallo de conversion, cuando es un
/// problema de E/S.
fn check_output(output: &Path) -> Result<(), CliError> {
    let cannot_write = |reason: &dyn std::fmt::Display| {
        CliError::io(format!("cannot write {}: {reason}", output.display()))
    };
    if output.is_dir() {
        return Err(cannot_write(&"it is a folder"));
    }
    match output.parent() {
        Some(dir) if dir.is_dir() => {}
        _ => return Err(cannot_write(&"the folder does not exist")),
    }
    probe_writable(output).map_err(|e| cannot_write(&e))
}

/// Prueba de verdad que `output` se puede escribir, sin cambiar nada: los bits de permiso
/// no bastan (en Windows, el atributo de solo lectura de una carpeta no impide crear
/// archivos, y las ACL no se ven en ellos). Si el archivo existe, se abre para escritura
/// SIN truncarlo; si no, se crea y se borra.
fn probe_writable(output: &Path) -> io::Result<()> {
    match OpenOptions::new().write(true).open(output) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            // Si no se pudiera borrar, queda un archivo vacio que el export sobrescribe.
            let _ = std::fs::remove_file(output);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Se comprueba ANTES de lanzar nada que los sidecars del formato existan, para fallar
/// con `sidecar_missing` y la ruta esperada en vez de un error de proceso.
fn check_sidecars(install_dir: &Path, format: ExportFormat) -> Result<(), CliError> {
    let typst = format.needs_pdf_engine().then_some("typst");
    for name in std::iter::once("pandoc").chain(typst) {
        let path = sidecar_path_in(install_dir, name);
        if !path.is_file() {
            return Err(CliError::new(
                ErrorCode::SidecarMissing,
                format!("{name} not found (expected at {})", path.display()),
            ));
        }
    }
    Ok(())
}

/// Directorio de trabajo propio del export: lo crea y lo borra `export`.
fn work_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "elision-export-{}-{}",
        std::process::id(),
        EXPORT_SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

impl Report for ExportReport {
    fn text(&self) -> String {
        format!("{}\n", self.output)
    }
}
