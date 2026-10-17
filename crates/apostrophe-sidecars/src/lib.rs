//! Adaptador secundario de Apostrophe (arquitectura hexagonal, ADR-0001): los procesos
//! externos (pandoc, typst) y los recursos que viajan junto al ejecutable (sidecars y
//! `dictionaries/`).
//!
//! Vive aparte de `src-tauri` para que lo compartan los DOS adaptadores primarios: la GUI
//! (Tauri) y la CLI. Por eso no depende de Tauri: la CLI no debe arrastrar WebView2 ni
//! WebKitGTK, y estas piezas se prueban en proceso sin levantar la app.

pub mod dictionary;
pub mod export;
pub mod images;

use std::path::{Path, PathBuf};
use std::process::Command;

/// Carpeta de instalacion: la del ejecutable que esta corriendo, donde el instalador deja
/// los sidecars, `dictionaries/` y los dos ejecutables (la GUI `apostrophe` y la CLI
/// `elision`).
///
/// La ruta del ejecutable se resuelve con `canonicalize`, igual que `STARTING_BINARY` de
/// tauri-utils (lo que usa tauri-plugin-shell para ubicar los sidecars): asi la GUI y la
/// CLI encuentran los MISMOS binarios aunque se las lance por un enlace simbolico. Y no
/// depende del directorio de trabajo: una CLI lanzada desde `C:\Users\x` sigue
/// encontrando sus recursos.
pub fn install_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .and_then(|exe| exe.canonicalize())
        .map_err(|e| format!("no se pudo ubicar el ejecutable: {e}"))?;
    install_dir_of(&exe).ok_or_else(|| "el ejecutable no tiene directorio padre".to_string())
}

/// Regla pura detras de [`install_dir`]: la carpeta que contiene `exe`.
///
/// Bajo `cargo test` el binario de pruebas queda en `target/<perfil>/deps/`, pero
/// `tauri-build` copia los sidecars a `target/<perfil>/`: en ese caso se sube un nivel. Es
/// la misma regla de `relative_command_path` en tauri-plugin-shell.
pub fn install_dir_of(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent().filter(|d| !d.as_os_str().is_empty())?;
    let base = if dir.ends_with("deps") {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    };
    Some(base.to_path_buf())
}

/// Ruta de un sidecar dentro de `dir`: el nombre pelado (Tauri quita el target triple al
/// empaquetar) y `.exe` en Windows. El `.exe` se AGREGA en vez de usar `with_extension`,
/// que borraria cualquier punto del nombre.
pub fn sidecar_path_in(dir: &Path, name: &str) -> PathBuf {
    let mut path = dir.join(name);
    if cfg!(windows) {
        path.as_mut_os_string().push(".exe");
    }
    path
}

/// Ruta ABSOLUTA de un sidecar junto al ejecutable actual, con la MISMA regla que aplica
/// tauri-plugin-shell. Se replica porque el plugin solo expone un `Command`, no la ruta, y
/// hace falta saber si el archivo EXISTE antes de lanzar nada: el export a PDF y
/// `render_math` fallan con un mensaje util si falta typst.
pub fn sidecar_path(name: &str) -> Result<PathBuf, String> {
    Ok(sidecar_path_in(&install_dir()?, name))
}

/// `Command` para lanzar un sidecar de `dir` sin el plugin shell (la CLI no tiene Tauri).
///
/// En Windows se lanza con `CREATE_NO_WINDOW`, como hace el plugin: pandoc y typst son
/// programas de consola, y si quien los lanza no tiene una (un agente que corre la CLI
/// sin terminal), cada uno abriria su propia ventana negra.
pub fn std_command(dir: &Path, name: &str) -> Command {
    #[allow(unused_mut)] // solo se modifica en Windows
    let mut command = Command::new(sidecar_path_in(dir, name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `CREATE_NO_WINDOW` de la API de Win32 (`winbase.h`).
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nombre del binario tal como queda en disco: `.exe` solo en Windows.
    fn exe_name(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    #[test]
    fn el_sidecar_vive_en_la_carpeta_indicada() {
        let dir = Path::new("instalacion");
        assert_eq!(sidecar_path_in(dir, "pandoc"), dir.join(exe_name("pandoc")));
        assert_eq!(sidecar_path_in(dir, "typst"), dir.join(exe_name("typst")));
    }

    #[test]
    fn el_nombre_del_sidecar_conserva_sus_puntos() {
        // Igual que tauri-plugin-shell: se AGREGA `.exe`, no se reemplaza la "extension".
        let dir = Path::new("instalacion");
        assert_eq!(
            sidecar_path_in(dir, "pandoc.v3"),
            dir.join(exe_name("pandoc.v3"))
        );
    }

    #[test]
    fn la_carpeta_de_instalacion_es_la_del_ejecutable() {
        let exe = PathBuf::from("instalacion").join(exe_name("apostrophe-cli"));
        assert_eq!(install_dir_of(&exe), Some(PathBuf::from("instalacion")));
    }

    #[test]
    fn bajo_cargo_test_se_sube_un_nivel_desde_deps() {
        // `cargo test` deja el binario de pruebas en `target/<perfil>/deps/`, pero
        // `tauri-build` copia los sidecars a `target/<perfil>/`.
        let exe = Path::new("target")
            .join("debug")
            .join("deps")
            .join("pruebas-123");
        assert_eq!(
            install_dir_of(&exe),
            Some(Path::new("target").join("debug"))
        );
    }

    #[test]
    fn un_ejecutable_sin_carpeta_no_tiene_instalacion() {
        assert_eq!(install_dir_of(Path::new("")), None);
    }

    #[test]
    fn la_instalacion_real_es_absoluta_y_no_es_deps() {
        // El binario de este test vive en `target/<perfil>/deps/`: la regla tiene que
        // devolver la carpeta de arriba, ya resuelta a ruta absoluta.
        let dir = install_dir().expect("deberia ubicarse el ejecutable de pruebas");
        assert!(dir.is_absolute(), "{}", dir.display());
        assert!(!dir.ends_with("deps"), "{}", dir.display());
        assert_eq!(
            sidecar_path("pandoc").unwrap(),
            sidecar_path_in(&dir, "pandoc")
        );
    }

    #[test]
    fn el_comando_lanza_el_sidecar_de_la_carpeta() {
        let dir = Path::new("instalacion");
        let command = std_command(dir, "typst");
        assert_eq!(
            Path::new(command.get_program()),
            sidecar_path_in(dir, "typst")
        );
    }
}
