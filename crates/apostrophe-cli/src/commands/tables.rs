//! `tables`: verifica (por defecto) o reescribe la alineacion de TODAS las tablas de un
//! documento, con `align_tables` del dominio.

use std::io::Read;

use apostrophe_core::table::{align_tables, SkipReason};
use serde::Serialize;

use crate::args::TablesArgs;
use crate::commands::Report;
use crate::error::CliError;
use crate::{input, Env};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Solo informa: nunca escribe.
    Check,
    /// Reescribe el archivo si alguna tabla cambia.
    Write,
}

#[derive(Debug, Serialize)]
pub struct TablesReport {
    /// Ruta absoluta del documento; `null` si vino por stdin.
    pub file: Option<String>,
    pub mode: Mode,
    /// Cada tabla alineable, con la linea (base 1) de su encabezado.
    pub tables: Vec<Table>,
    /// Cuantas tablas estaban desalineadas (en `write`, las que se reescribieron).
    pub changed: usize,
    /// Las tablas que se dejaron como estaban porque alinearlas podria estropearlas.
    pub skipped: Vec<Skipped>,
    #[serde(skip)]
    label: String,
}

#[derive(Debug, Serialize)]
pub struct Table {
    pub line: usize,
    pub aligned: bool,
}

#[derive(Debug, Serialize)]
pub struct Skipped {
    pub line: usize,
    /// Codigo estable: `escaped_pipe`, `extra_cells`, `mixed_indent` o `blockquote`.
    pub reason: &'static str,
    /// El motivo en palabras, para el aviso del modo texto.
    #[serde(skip)]
    why: &'static str,
}

pub fn execute(
    args: &TablesArgs,
    env: &Env,
    stdin: &mut dyn Read,
) -> Result<TablesReport, CliError> {
    let mode = if args.write { Mode::Write } else { Mode::Check };
    // Antes de leer: con stdin no hay archivo que reescribir.
    if mode == Mode::Write && input::is_stdin(&args.input) {
        return Err(CliError::usage(
            "--write needs a file: standard input cannot be rewritten",
        ));
    }
    let input = input::read(&args.input, &env.cwd, stdin)?;
    let result = align_tables(&input.text);
    let changed = result.tables.iter().filter(|t| !t.aligned).count();

    // Solo si algo cambio, y como guarda la GUI: escritura directa en UTF-8. Si el archivo
    // esta abierto en la GUI, su watcher lo nota y ofrece recargarlo.
    if let (Mode::Write, Some(path), true) = (mode, input.path.as_deref(), changed > 0) {
        std::fs::write(path, &result.text)
            .map_err(|e| CliError::io(format!("cannot write {}: {e}", input.label)))?;
    }

    Ok(TablesReport {
        file: input.file(),
        mode,
        tables: result
            .tables
            .iter()
            .map(|t| Table {
                line: t.line,
                aligned: t.aligned,
            })
            .collect(),
        changed,
        skipped: result
            .skipped
            .iter()
            .map(|s| {
                let (reason, why) = describe(s.reason);
                Skipped {
                    line: s.line,
                    reason,
                    why,
                }
            })
            .collect(),
        label: input.label,
    })
}

/// El motivo de una omision: su codigo estable y su explicacion.
fn describe(reason: SkipReason) -> (&'static str, &'static str) {
    match reason {
        SkipReason::EscapedPipe => ("escaped_pipe", "contains an escaped pipe"),
        SkipReason::ExtraCells => ("extra_cells", "a row has more cells than the header"),
        SkipReason::MixedIndent => ("mixed_indent", "its lines have different indentation"),
        SkipReason::Blockquote => ("blockquote", "inside a blockquote"),
    }
}

impl Report for TablesReport {
    fn text(&self) -> String {
        let what = match self.mode {
            Mode::Check => "table is not aligned",
            Mode::Write => "table realigned",
        };
        self.tables
            .iter()
            .filter(|t| !t.aligned)
            .map(|t| format!("{}:{}: {what}\n", self.label, t.line))
            .collect()
    }

    fn warnings(&self) -> Vec<String> {
        self.skipped
            .iter()
            .map(|s| format!("{}:{}: table skipped ({})", self.label, s.line, s.why))
            .collect()
    }

    fn has_findings(&self) -> bool {
        self.mode == Mode::Check && self.changed > 0
    }
}
