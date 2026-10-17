//! `spell`: ortografia con el diccionario `es`, la misma logica que el subrayado de la
//! GUI (`SpellChecker::check_text`, sin modo foco).

use std::io::Read;
use std::path::PathBuf;

use apostrophe_core::spellcheck::SpellChecker;
use apostrophe_sidecars::dictionary::load_es;
use serde::Serialize;

use crate::args::SpellArgs;
use crate::commands::Report;
use crate::error::CliError;
use crate::position::LineIndex;
use crate::{input, Env};

/// Idioma del unico diccionario que se distribuye.
const LANGUAGE: &str = "es";

#[derive(Debug, Serialize)]
pub struct SpellReport {
    /// Ruta absoluta del documento; `null` si vino por stdin.
    pub file: Option<String>,
    pub language: &'static str,
    pub misspellings: Vec<Misspelling>,
    #[serde(skip)]
    label: String,
}

#[derive(Debug, Serialize)]
pub struct Misspelling {
    /// Linea y columna base 1; la columna en caracteres (escalares Unicode).
    pub line: usize,
    pub column: usize,
    pub word: String,
    pub suggestions: Vec<String>,
}

pub fn execute(args: &SpellArgs, env: &Env, stdin: &mut dyn Read) -> Result<SpellReport, CliError> {
    // Primero la entrada: es barata, y cargar el diccionario no.
    let input = input::read(&args.input, &env.cwd, stdin)?;
    let dictionary = load_es(&dictionary_dir(args, env))?;
    let index = LineIndex::new(&input.text);
    let misspellings = SpellChecker::check_text(&dictionary, &input.text, false)
        .into_iter()
        .map(|m| {
            let (line, column) = index.position(m.from);
            Misspelling {
                line,
                column,
                word: m.word,
                suggestions: m.suggestions,
            }
        })
        .collect();
    Ok(SpellReport {
        file: input.file(),
        language: LANGUAGE,
        misspellings,
        label: input.label,
    })
}

/// Carpeta del diccionario: `--dict-dir`, si no `ELISION_DICT_DIR`, si no la de la
/// instalacion. La primera que se indique es la que se usa: si no sirve, es un error, no
/// se cae en silencio a la siguiente.
fn dictionary_dir(args: &SpellArgs, env: &Env) -> PathBuf {
    match args.dict_dir.as_deref().or(env.dict_dir.as_deref()) {
        Some(dir) => input::absolutize(&env.cwd, dir),
        None => env.install_dir.join("dictionaries"),
    }
}

impl Report for SpellReport {
    fn text(&self) -> String {
        self.misspellings
            .iter()
            .map(|m| {
                let mut line = format!("{}:{}:{}: {}", self.label, m.line, m.column, m.word);
                if !m.suggestions.is_empty() {
                    line.push_str(&format!(" (suggestions: {})", m.suggestions.join(", ")));
                }
                line.push('\n');
                line
            })
            .collect()
    }

    fn has_findings(&self) -> bool {
        !self.misspellings.is_empty()
    }
}
