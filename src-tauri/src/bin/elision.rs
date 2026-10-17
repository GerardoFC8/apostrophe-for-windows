//! Ejecutable de la CLI `elision` (adaptador primario, ADR-0001). Solo arma el entorno
//! real y llama a `apostrophe_cli::run`: toda la logica vive en ese crate, sin Tauri.
//!
//! Es un programa de CONSOLA: no lleva `windows_subsystem`. El de `main.rs` es un atributo
//! de crate y cada archivo de `src/bin` es un crate aparte, asi que solo afecta a la GUI.
//! Con el subsistema de consola, la CLI usa la terminal de quien la lanza y este espera a
//! que termine para leer su codigo de salida.
//!
//! Vive en src-tauri (y no en `crates/apostrophe-cli`) porque tauri-cli solo empaqueta
//! los ejecutables de este paquete: asi el instalador deja `elision.exe` junto a la GUI,
//! los sidecars y `dictionaries/`, que es donde los busca.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use apostrophe_cli::commands::open::launch_detached;
use apostrophe_cli::error::CliError;
use apostrophe_cli::{run, startup_failure, Env, Io, DICT_DIR_VAR};

fn main() {
    let args: Vec<OsString> = std::env::args_os().collect();
    // Los bloqueos de los flujos se sueltan al cerrar el bloque, antes de `exit`.
    let code = {
        let mut stdin = io::stdin().lock();
        let mut stdout = io::stdout().lock();
        let mut stderr = io::stderr().lock();
        let mut io = Io {
            stdin: &mut stdin,
            stdout: &mut stdout,
            stderr: &mut stderr,
        };
        match environment() {
            Ok(env) => run(args, &env, &mut io),
            Err(error) => startup_failure(&args, &error, &mut io),
        }
    };
    std::process::exit(code);
}

/// El entorno real del proceso: lo unico que la CLI toma del sistema.
fn environment() -> Result<Env<'static>, CliError> {
    // El detalle de `install_dir` esta en castellano (es de la GUI); la CLI habla ingles.
    let install_dir = apostrophe_sidecars::install_dir()
        .map_err(|_| CliError::io("cannot locate the folder of the elision executable"))?;
    let cwd = std::env::current_dir()
        .map_err(|e| CliError::io(format!("cannot read the current folder: {e}")))?;
    Ok(Env {
        install_dir,
        cwd,
        dict_dir: std::env::var_os(DICT_DIR_VAR).map(PathBuf::from),
        launch: &launch_detached,
    })
}
