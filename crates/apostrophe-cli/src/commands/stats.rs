//! `stats`: las metricas del panel de estadisticas de la GUI (`Stats` y `ReadTime`).

use std::io::Read;

use apostrophe_core::stats::{count, Stats};
use serde::Serialize;

use crate::args::StatsArgs;
use crate::commands::Report;
use crate::error::CliError;
use crate::{input, Env};

#[derive(Debug, Serialize)]
pub struct StatsReport {
    /// Ruta absoluta del documento; `null` si vino por stdin.
    pub file: Option<String>,
    pub stats: Stats,
}

pub fn execute(args: &StatsArgs, env: &Env, stdin: &mut dyn Read) -> Result<StatsReport, CliError> {
    let input = input::read(&args.input, &env.cwd, stdin)?;
    Ok(StatsReport {
        file: input.file(),
        stats: count(&input.text),
    })
}

impl Report for StatsReport {
    fn text(&self) -> String {
        let s = &self.stats;
        let t = &s.read_time;
        format!(
            "characters: {}\nwords: {}\nsentences: {}\nparagraphs: {}\nread_time: {}:{:02}:{:02}\n",
            s.characters, s.words, s.sentences, s.paragraphs, t.hours, t.minutes, t.seconds
        )
    }
}
