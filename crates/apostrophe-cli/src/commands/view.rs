//! `view`: muestra un documento Markdown en la terminal, con formato y ajustado al ancho
//! (ver `render`). Con colores solo si la salida es una terminal.

use std::io::Read;

use serde::Serialize;

use crate::args::ViewArgs;
use crate::commands::Report;
use crate::error::CliError;
use crate::render::{render, DEFAULT_WIDTH};
use crate::{input, Env};

#[derive(Debug, Serialize)]
pub struct ViewReport {
    /// Ruta absoluta del documento; `null` si vino por stdin.
    pub file: Option<String>,
    /// El documento dibujado en texto plano, sin secuencias de color.
    pub text: String,
    /// Lo que se escribe en la terminal: igual que `text`, con color si corresponde.
    #[serde(skip)]
    pub rendered: String,
}

pub fn execute(args: &ViewArgs, env: &Env, stdin: &mut dyn Read) -> Result<ViewReport, CliError> {
    let input = input::read(&args.input, &env.cwd, stdin)?;
    let width = args.width.or(env.terminal.width).unwrap_or(DEFAULT_WIDTH);
    let color = env.terminal.color && !args.no_color;
    let text = render(&input.text, width, false);
    let rendered = if color {
        render(&input.text, width, true)
    } else {
        text.clone()
    };
    Ok(ViewReport {
        file: input.file(),
        text,
        rendered,
    })
}

impl Report for ViewReport {
    fn text(&self) -> String {
        self.rendered.clone()
    }
}
