//! Superficie de la CLI (clap derive): lo que muestra `--help` es lo que leen los agentes.

use std::path::PathBuf;

use apostrophe_core::convert::ExportFormat;
use clap::{Args, Parser, Subcommand, ValueEnum};

const AFTER_HELP: &str = "\
Exit codes: 0 ok, 1 findings (misspellings, misaligned tables), 2 usage, 3 missing sidecar \
or dictionary, 4 conversion failed, 5 I/O error.
With --json, stdout carries a single ASCII-only JSON object, errors included: \
{\"ok\":false,\"error\":{\"code\":...,\"message\":...}}.";

/// `bin_name` fijo: la ayuda dice `elision` aunque el ejecutable se llame distinto
/// (`elision.exe`, o el binario de pruebas).
#[derive(Debug, Parser)]
#[command(
    name = "elision",
    bin_name = "elision",
    version,
    about = "Markdown tools for scripts and AI agents: export, statistics, tables, spelling \
             and opening files in the editor.",
    after_help = AFTER_HELP
)]
pub struct Cli {
    /// Print a single ASCII-only JSON object on stdout (errors too)
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert a document with the bundled pandoc and typst
    Export(ExportArgs),
    /// Count characters, words, sentences and paragraphs, and estimate the reading time
    Stats(StatsArgs),
    /// Check (default) or rewrite the alignment of every Markdown table
    Tables(TablesArgs),
    /// Check the spelling with the bundled Spanish dictionary
    Spell(SpellArgs),
    /// Open files in the editor without waiting for it to close
    Open(OpenArgs),
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Markdown file to convert, or - to read standard input
    #[arg(value_name = "FILE")]
    pub input: PathBuf,

    /// Output file, overwritten without asking; its extension picks the format
    #[arg(short, long, value_name = "OUT")]
    pub output: PathBuf,

    /// Output format, when the extension of --output does not say it
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub to: Option<Format>,
}

#[derive(Debug, Args)]
pub struct StatsArgs {
    /// Markdown file, or - to read standard input
    #[arg(value_name = "FILE")]
    pub input: PathBuf,
}

#[derive(Debug, Args)]
pub struct TablesArgs {
    /// Markdown file, or - to read standard input (only without --write)
    #[arg(value_name = "FILE")]
    pub input: PathBuf,

    /// Report misaligned tables without writing anything (the default); exit 1 if any
    #[arg(long, conflicts_with = "write")]
    pub check: bool,

    /// Rewrite the file with every table aligned (UTF-8, line endings kept)
    #[arg(long)]
    pub write: bool,
}

#[derive(Debug, Args)]
pub struct SpellArgs {
    /// Markdown file, or - to read standard input
    #[arg(value_name = "FILE")]
    pub input: PathBuf,

    /// Folder with es.aff and es.dic; overrides ELISION_DICT_DIR and the bundled dictionary
    #[arg(long, value_name = "DIR")]
    pub dict_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct OpenArgs {
    /// Files to open, each in its own tab
    #[arg(required = true, value_name = "FILE")]
    pub files: Vec<PathBuf>,
}

/// Formatos de `--to`. Son los de la GUI; LaTeX se pide por su nombre, no por `.tex`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Html,
    Pdf,
    Odt,
    Docx,
    Epub,
    Latex,
}

impl Format {
    pub const ALL: [Format; 6] = [
        Format::Html,
        Format::Pdf,
        Format::Odt,
        Format::Docx,
        Format::Epub,
        Format::Latex,
    ];

    /// Nombre estable, el mismo de `--to` y del campo `format` del JSON.
    pub fn name(self) -> &'static str {
        match self {
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Odt => "odt",
            Format::Docx => "docx",
            Format::Epub => "epub",
            Format::Latex => "latex",
        }
    }

    pub fn export_format(self) -> ExportFormat {
        match self {
            Format::Html => ExportFormat::Html,
            Format::Pdf => ExportFormat::Pdf,
            Format::Odt => ExportFormat::Odt,
            Format::Docx => ExportFormat::Docx,
            Format::Epub => ExportFormat::Epub,
            Format::Latex => ExportFormat::Latex,
        }
    }

    /// El formato que corresponde a una extension (la misma regla que el dialogo de la GUI).
    pub fn from_extension(ext: &str) -> Option<Self> {
        let format = ExportFormat::from_extension(ext)?;
        Self::ALL.into_iter().find(|f| f.export_format() == format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn la_definicion_de_clap_es_coherente() {
        Cli::command().debug_assert();
    }

    #[test]
    fn la_extension_decide_el_formato_y_latex_es_tex() {
        assert_eq!(Format::from_extension("PDF"), Some(Format::Pdf));
        assert_eq!(Format::from_extension("tex"), Some(Format::Latex));
        assert_eq!(Format::from_extension("xyz"), None);
        for format in Format::ALL {
            assert_eq!(
                Format::from_extension(format.export_format().extension()),
                Some(format)
            );
        }
    }
}
