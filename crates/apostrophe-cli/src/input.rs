//! Entrada de los comandos: un archivo o, con `-`, la entrada estandar. Ambos se
//! decodifican con `encoding::decode_bytes`, igual que la GUI al abrir un archivo.

use std::io::Read;
use std::path::{Path, PathBuf};

use apostrophe_core::encoding::decode_bytes;

use crate::error::CliError;

/// Nombre con el que se muestra la entrada estandar en el modo texto.
pub const STDIN_LABEL: &str = "<stdin>";

/// Un documento ya leido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// Como se muestra en el modo texto: la ruta tal como la escribio el usuario.
    pub label: String,
    /// Ruta absoluta; `None` si vino por stdin.
    pub path: Option<PathBuf>,
    pub text: String,
}

impl Input {
    /// Carpeta del documento, contra la que se resuelven sus imagenes. Por stdin no hay:
    /// se trata como un documento sin guardar de la GUI.
    pub fn dir(&self) -> Option<&Path> {
        self.path.as_deref().and_then(Path::parent)
    }

    /// La ruta absoluta para el JSON (`null` si vino por stdin).
    pub fn file(&self) -> Option<String> {
        self.path.as_deref().map(display_path)
    }
}

/// `true` si el argumento pide la entrada estandar.
pub fn is_stdin(arg: &Path) -> bool {
    arg == Path::new("-")
}

/// Lee `arg` (relativo a `cwd`) o, si es `-`, todo `stdin`.
pub fn read(arg: &Path, cwd: &Path, stdin: &mut dyn Read) -> Result<Input, CliError> {
    if is_stdin(arg) {
        let mut bytes = Vec::new();
        stdin
            .read_to_end(&mut bytes)
            .map_err(|e| CliError::io(format!("cannot read standard input: {e}")))?;
        return Ok(Input {
            label: STDIN_LABEL.to_string(),
            path: None,
            text: decode_bytes(&bytes).text,
        });
    }
    let label = arg.display().to_string();
    let path = absolutize(cwd, arg);
    let bytes =
        std::fs::read(&path).map_err(|e| CliError::io(format!("cannot read {label}: {e}")))?;
    Ok(Input {
        label,
        path: Some(path),
        text: decode_bytes(&bytes).text,
    })
}

/// `path` como ruta absoluta y normalizada: las relativas se resuelven contra `cwd`, el
/// directorio de trabajo de quien lanzo la CLI (pandoc corre en otro, asi que nada puede
/// quedar relativo). Es la UNICA via por la que pasan las rutas del usuario: la entrada,
/// `-o`, los archivos de `open` y la carpeta del diccionario.
///
/// La normaliza `std::path::absolute`, sin tocar el disco: quita los `.` y los separadores
/// repetidos, y en Windows (`GetFullPathNameW`) cambia cada `/` por `\` y resuelve los
/// `..`. En POSIX los `..` se conservan, porque un enlace simbolico los cambia. No se usa
/// `canonicalize`: exige que la ruta exista y en Windows agrega el prefijo `\\?\`.
pub fn absolutize(cwd: &Path, path: &Path) -> PathBuf {
    // `join` con una ruta absoluta la devuelve tal cual.
    let joined = cwd.join(path);
    // Solo falla con una ruta vacia o sintacticamente invalida: entonces, sin normalizar.
    std::path::absolute(&joined).unwrap_or(joined)
}

/// Una ruta como texto para el JSON (con reemplazo si no es Unicode valido).
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use crate::testutil::scratch;
    use std::io;

    #[test]
    fn lee_un_archivo_relativo_al_directorio_de_trabajo() {
        let cwd = scratch("input-rel");
        std::fs::write(cwd.join("doc.md"), "hola").unwrap();
        let input = read(Path::new("doc.md"), &cwd, &mut io::empty()).unwrap();
        assert_eq!(input.text, "hola");
        assert_eq!(input.label, "doc.md");
        assert_eq!(input.path, Some(cwd.join("doc.md")));
        assert_eq!(input.dir(), Some(cwd.as_path()));
    }

    #[test]
    fn un_guion_lee_la_entrada_estandar_y_no_tiene_carpeta() {
        let input = read(
            Path::new("-"),
            Path::new("no-se-usa"),
            &mut "uno dos".as_bytes(),
        )
        .unwrap();
        assert_eq!(input.text, "uno dos");
        assert_eq!(input.label, "<stdin>");
        assert_eq!(input.path, None);
        assert_eq!(input.dir(), None);
    }

    #[test]
    fn lo_que_no_es_utf8_se_decodifica_igual_en_archivo_y_en_stdin() {
        // "canción español ñ" en windows-1252.
        let bytes: &[u8] = b"canci\xf3n espa\xf1ol \xf1";
        let cwd = scratch("input-1252");
        std::fs::write(cwd.join("viejo.md"), bytes).unwrap();
        let file = read(Path::new("viejo.md"), &cwd, &mut io::empty()).unwrap();
        let stdin = read(Path::new("-"), &cwd, &mut &bytes[..]).unwrap();
        assert_eq!(file.text, "canción español ñ");
        assert_eq!(stdin.text, file.text);
    }

    #[test]
    fn una_ruta_inexistente_es_un_error_de_e_s() {
        let cwd = scratch("input-missing");
        let err = read(Path::new("no-existe.md"), &cwd, &mut io::empty()).unwrap_err();
        assert_eq!(err.code, ErrorCode::IoError);
        assert_eq!(err.exit_code(), 5);
        assert!(
            err.message.starts_with("cannot read no-existe.md: "),
            "{}",
            err.message
        );
    }

    #[test]
    fn las_rutas_relativas_se_resuelven_contra_el_directorio_de_trabajo() {
        let cwd = scratch("input-abs");
        assert_eq!(
            absolutize(&cwd, Path::new("a/b.md")),
            cwd.join("a").join("b.md")
        );
        let absolute = cwd.join("c.md");
        assert_eq!(absolutize(Path::new("otro"), &absolute), absolute);
    }

    /// La ruta como texto, para compararla tal cual: `Path` compara por componentes y da
    /// por iguales `a/./b` y `a/b` (y, en Windows, `a/b` y `a\b`).
    fn text(path: PathBuf) -> String {
        path.into_os_string().into_string().unwrap()
    }

    /// `base` seguida de `parts`, con el separador de la plataforma.
    fn native(base: &Path, parts: &[&str]) -> String {
        let mut path = base.display().to_string();
        for part in parts {
            path.push(std::path::MAIN_SEPARATOR);
            path.push_str(part);
        }
        path
    }

    #[test]
    fn las_rutas_se_normalizan_al_resolverlas() {
        let cwd = scratch("input-norm");
        // `.` y los separadores repetidos sobran en cualquier plataforma; en Windows,
        // ademas, cada `/` pasa a ser `\`.
        assert_eq!(
            text(absolutize(&cwd, Path::new("a/./b//c.md"))),
            native(&cwd, &["a", "b", "c.md"])
        );
        // Una ruta que ya es absoluta tambien se normaliza.
        let absolute = format!("{}/x/./y.md", cwd.display());
        assert_eq!(
            text(absolutize(Path::new("otro"), Path::new(&absolute))),
            native(&cwd, &["x", "y.md"])
        );
    }

    #[test]
    fn los_dos_puntos_siguen_las_reglas_de_cada_plataforma() {
        let cwd = scratch("input-dotdot");
        let resolved = text(absolutize(&cwd, Path::new("a/./b/../c.md")));
        if cfg!(windows) {
            // Windows los resuelve sin mirar el disco, como `GetFullPathNameW`.
            assert_eq!(resolved, native(&cwd, &["a", "c.md"]));
        } else {
            // POSIX los conserva: si `b` fuera un enlace simbolico, `b/..` no seria `a`.
            assert_eq!(resolved, native(&cwd, &["a", "b", "..", "c.md"]));
        }
    }

    #[cfg(windows)]
    #[test]
    fn en_windows_una_ruta_con_barras_queda_solo_con_barras_invertidas() {
        let cwd = scratch("input-slash");
        let resolved = text(absolutize(&cwd, Path::new("salida/a.pdf")));
        assert!(!resolved.contains('/'), "{resolved}");
        assert_eq!(resolved, native(&cwd, &["salida", "a.pdf"]));
    }
}
