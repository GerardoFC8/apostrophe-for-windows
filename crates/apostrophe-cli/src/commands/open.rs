//! `open`: abre archivos en la GUI y termina enseguida, sin esperar a que se cierre.
//!
//! La GUI recibe las rutas en argv y abre cada una en su pestaña, en frío o a través de
//! single-instance si ya está abierta.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use apostrophe_sidecars::sidecar_path_in;
use serde::Serialize;

use crate::args::OpenArgs;
use crate::commands::Report;
use crate::error::{CliError, ErrorCode};
use crate::{input, Env};

/// Ejecutable de la GUI en la carpeta de instalacion (sin `.exe`). Es el nombre del
/// paquete de Tauri; cambiara con el renombre a Elision, y no puede ser `elision`: en
/// Windows chocaria con la CLI.
pub const GUI_BINARY: &str = "apostrophe";

#[derive(Debug, Serialize)]
pub struct OpenReport {
    /// Rutas absolutas, en el orden en que se pasaron a la GUI.
    pub files: Vec<String>,
}

pub fn execute(args: &OpenArgs, env: &Env) -> Result<OpenReport, CliError> {
    // Absolutas: la GUI corre en su carpeta, y si ya esta abierta resolveria las relativas
    // contra el directorio de la instancia viva.
    let files: Vec<PathBuf> = args
        .files
        .iter()
        .map(|file| input::absolutize(&env.cwd, file))
        .collect();
    if let Some((_, given)) = files
        .iter()
        .zip(&args.files)
        .find(|(path, _)| !path.is_file())
    {
        return Err(CliError::io(format!(
            "cannot open {}: no such file",
            given.display()
        )));
    }
    let exe = sidecar_path_in(&env.install_dir, GUI_BINARY);
    if !exe.is_file() {
        return Err(CliError::new(
            ErrorCode::SidecarMissing,
            format!("the editor was not found (expected at {})", exe.display()),
        ));
    }
    (env.launch)(&exe, &files)
        .map_err(|e| CliError::io(format!("could not start the editor: {e}")))?;
    Ok(OpenReport {
        files: files.iter().map(|f| input::display_path(f)).collect(),
    })
}

impl Report for OpenReport {
    fn text(&self) -> String {
        String::new()
    }
}

/// El lanzador real de [`Env::launch`]: arranca la GUI y vuelve sin esperarla.
pub fn launch_detached(exe: &Path, files: &[PathBuf]) -> io::Result<()> {
    // No se espera al hijo a proposito: la CLI termina enseguida y el sistema lo adopta.
    detached_command(exe, files).spawn().map(drop)
}

/// El comando que lanza la GUI desacoplada de quien llama.
///
/// - stdio nulo: si heredara los pipes del agente, este esperaria el EOF de stdout hasta
///   que se cerrara la GUI.
/// - En la carpeta de la GUI: un proceso vivo bloquea su directorio actual en Windows, y
///   no debe ser la carpeta del usuario.
/// - Sin consola y en su propio grupo: Ctrl+C en la terminal no cierra el editor.
fn detached_command(exe: &Path, files: &[PathBuf]) -> Command {
    let mut command = Command::new(exe);
    command
        .args(files)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = exe.parent().filter(|d| !d.as_os_str().is_empty()) {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` de la API de Win32 (`winbase.h`).
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn la_gui_se_lanza_con_los_archivos_desde_su_propia_carpeta() {
        let exe = Path::new("instalacion").join("apostrophe");
        let files = [PathBuf::from("/docs/a.md"), PathBuf::from("/docs/b.md")];
        let command = detached_command(&exe, &files);
        assert_eq!(Path::new(command.get_program()), exe);
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(args, [files[0].as_os_str(), files[1].as_os_str()]);
        // No en la carpeta del usuario: la GUI viva no debe bloquearla.
        assert_eq!(command.get_current_dir(), Some(Path::new("instalacion")));
    }

    #[cfg(unix)]
    #[test]
    fn lanzar_no_espera_al_proceso_y_falla_si_no_existe() {
        assert!(launch_detached(Path::new("/bin/sh"), &[]).is_ok());
        assert!(launch_detached(Path::new("/no/existe/apostrophe"), &[]).is_err());
    }
}
