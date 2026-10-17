//! CLI `elision`: segundo adaptador primario de la arquitectura hexagonal (ADR-0001),
//! junto a la GUI. Expone el export, las estadisticas, las tablas, la ortografia y la
//! apertura de archivos a scripts y agentes de IA, con salida estable y sin interaccion.
//!
//! Es una biblioteca: el ejecutable es un envoltorio minimo que arma el [`Env`] real y
//! llama a [`run`]. Asi todo se prueba en proceso, con buffers y un entorno falso.
//!
//! Contrato de salida: stdout lleva solo datos y stderr el diagnostico; con `--json`,
//! stdout lleva UN objeto JSON (ASCII puro), tambien para los errores. Codigos de salida:
//! 0 ok, 1 hallazgos, 2 uso, 3 falta sidecar o diccionario, 4 fallo de conversion, 5 E/S.
//! Los mensajes van en ingles; el contrato son los codigos, no la copia.

pub mod args;
pub mod commands;
pub mod error;
pub mod input;
pub mod output;
pub mod position;

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::args::{Cli, Command};
use crate::commands::Report;
use crate::error::CliError;

/// Nombre publico del programa: el que muestran la ayuda y los mensajes.
pub const PROGRAM: &str = "elision";

/// Variable de entorno con la carpeta del diccionario (`es.aff` y `es.dic`). La lee el
/// ejecutable, no `run`: aca llega ya resuelta en [`Env::dict_dir`].
pub const DICT_DIR_VAR: &str = "ELISION_DICT_DIR";

/// Los tres flujos del proceso.
pub struct Io<'a> {
    pub stdin: &'a mut dyn Read,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
}

/// Todo lo que la CLI toma del entorno. `run` no lee variables ni el directorio actual:
/// los inyecta el ejecutable, para que los tests en paralelo no compitan por estado global.
pub struct Env<'a> {
    /// Carpeta de instalacion: sidecars, `dictionaries/` y el ejecutable de la GUI.
    pub install_dir: PathBuf,
    /// Directorio de trabajo de quien lanzo la CLI: contra el se resuelven las rutas.
    pub cwd: PathBuf,
    /// Valor de [`DICT_DIR_VAR`], si esta definida.
    pub dict_dir: Option<PathBuf>,
    /// Lanza la GUI con estos archivos sin esperarla (en produccion,
    /// `commands::open::launch_detached`).
    pub launch: &'a dyn Fn(&Path, &[PathBuf]) -> io::Result<()>,
}

/// Ejecuta la CLI con `args` (el primero es el nombre del programa) y devuelve el codigo
/// de salida.
pub fn run<I, T>(args: I, env: &Env, io: &mut Io) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(err) => return parse_failure(&err, wants_json(&args), io),
    };
    let json = cli.json;
    match cli.command {
        Command::Export(args) => finish(commands::export::execute(&args, env, io.stdin), json, io),
        Command::Stats(args) => finish(commands::stats::execute(&args, env, io.stdin), json, io),
        Command::Tables(args) => finish(commands::tables::execute(&args, env, io.stdin), json, io),
        Command::Spell(args) => finish(commands::spell::execute(&args, env, io.stdin), json, io),
        Command::Open(args) => finish(commands::open::execute(&args, env), json, io),
    }
}

/// `true` si el usuario pidio `--json`. Se busca en los argumentos crudos porque cuando
/// clap falla no hay nada parseado, y su error tambien tiene que salir en JSON. Despues de
/// `--` todo es posicional.
fn wants_json(args: &[OsString]) -> bool {
    args.iter()
        .skip(1)
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "--json")
}

/// Un error de clap. `--help` y `--version` tambien llegan por aca: van a stdout y
/// terminan con 0.
fn parse_failure(err: &clap::Error, json: bool, io: &mut Io) -> i32 {
    let rendered = err.to_string();
    if !err.use_stderr() {
        return emit(io, &rendered, 0);
    }
    let error = CliError::usage(rendered.trim_end());
    if json {
        return emit(
            io,
            &format!("{}\n", output::error_json(&error)),
            error.exit_code(),
        );
    }
    // La copia de clap ya dice que fallo, el uso y donde pedir ayuda.
    let _ = io.stderr.write_all(rendered.as_bytes());
    error.exit_code()
}

/// Muestra el resultado de un comando y devuelve el codigo de salida.
fn finish<R: Report>(result: Result<R, CliError>, json: bool, io: &mut Io) -> i32 {
    let report = match result {
        Ok(report) => report,
        Err(err) => return fail(&err, json, io),
    };
    let code = i32::from(report.has_findings());
    if json {
        return emit(io, &format!("{}\n", output::success_json(&report)), code);
    }
    let code = emit(io, &report.text(), code);
    for warning in report.warnings() {
        let _ = io
            .stderr
            .write_all(output::warning_text(&warning).as_bytes());
    }
    code
}

/// Un error del comando: sin `--json`, solo a stderr (stdout queda vacio); con `--json`,
/// el sobre de error a stdout.
fn fail(err: &CliError, json: bool, io: &mut Io) -> i32 {
    if json {
        return emit(
            io,
            &format!("{}\n", output::error_json(err)),
            err.exit_code(),
        );
    }
    let _ = io.stderr.write_all(output::error_text(err).as_bytes());
    err.exit_code()
}

/// Escribe en stdout y devuelve `code`. Si quien lee cerro el pipe (`elision … | head`),
/// no hay a quien avisar: se ignora. Cualquier otro fallo de escritura es un error de E/S.
fn emit(io: &mut Io, text: &str, code: i32) -> i32 {
    if text.is_empty() {
        return code;
    }
    match io
        .stdout
        .write_all(text.as_bytes())
        .and_then(|()| io.stdout.flush())
    {
        Ok(()) => code,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => code,
        Err(e) => {
            let error = CliError::io(format!("cannot write to standard output: {e}"));
            let _ = io.stderr.write_all(output::error_text(&error).as_bytes());
            error.exit_code()
        }
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// Carpeta temporal propia del test, vacia.
    pub fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elision-cli-unit-{}-{}-{label}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
