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

/// `path` como ruta absoluta: las relativas se resuelven contra `cwd`, el directorio de
/// trabajo de quien lanzo la CLI (pandoc corre en otro, asi que nada puede quedar
/// relativo). No toca el disco.
pub fn absolutize(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
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
}
