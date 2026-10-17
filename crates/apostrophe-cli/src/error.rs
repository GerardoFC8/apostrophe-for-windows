//! Errores de la CLI. El contrato para los agentes es el CODIGO (`usage`, `io_error`…) y
//! la salida del proceso; el mensaje es solo diagnostico, en ingles.
//!
//! Los mensajes se arman aca a partir de las variantes de los errores del adaptador, sin
//! reutilizar su `Display`: ese es la copia en espanol que muestra la GUI.

use apostrophe_sidecars::dictionary::DictionaryError;
use apostrophe_sidecars::export::ExportError;

/// Codigo estable de un error, con su codigo de salida. El 0 (exito) y el 1 (hallazgos:
/// errores ortograficos, tablas desalineadas) no son errores: no tienen codigo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Subcomando, opcion o valor invalido (incluye un formato de export desconocido).
    Usage,
    /// Falta un ejecutable de la instalacion (pandoc, typst o la GUI) o no arranca.
    SidecarMissing,
    /// No se pudo cargar el diccionario `es`.
    DictionaryMissing,
    /// Pandoc o typst corrieron y fallaron.
    ConversionFailed,
    /// No se pudo leer la entrada o escribir la salida.
    IoError,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::SidecarMissing => "sidecar_missing",
            Self::DictionaryMissing => "dictionary_missing",
            Self::ConversionFailed => "conversion_failed",
            Self::IoError => "io_error",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            Self::Usage => 2,
            Self::SidecarMissing | Self::DictionaryMissing => 3,
            Self::ConversionFailed => 4,
            Self::IoError => 5,
        }
    }
}

/// Un error que termina el comando: su codigo y un mensaje para diagnostico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
}

impl CliError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::IoError, message)
    }

    pub fn exit_code(&self) -> i32 {
        self.code.exit_code()
    }
}

impl From<ExportError> for CliError {
    fn from(err: ExportError) -> Self {
        match err {
            // Los temporales del export: el detalle viene del adaptador, tal cual.
            ExportError::Setup(detail) => {
                Self::io(format!("could not prepare the export files ({detail})"))
            }
            // El ejecutable existe (se comprueba antes de lanzar) pero no arranca: para
            // quien llama, el sidecar no esta disponible.
            ExportError::Launch { program, message } => Self::new(
                ErrorCode::SidecarMissing,
                format!("could not start {program}: {message}"),
            ),
            ExportError::Failed {
                program,
                code,
                stderr,
            } => {
                let status = match code {
                    Some(code) => format!("exit code {code}"),
                    None => "terminated by a signal".to_string(),
                };
                let message = match failure_line(&stderr) {
                    Some(line) => format!("{program} failed ({status}): {line}"),
                    None => format!("{program} failed ({status})"),
                };
                Self::new(ErrorCode::ConversionFailed, message)
            }
        }
    }
}

/// La linea del stderr de pandoc o typst que explica el fallo. El resto es ruido en un
/// mensaje de una linea: la traza de GHC que pandoc agrega a sus errores de E/S, o el
/// contexto de fuente y los avisos que typst escribe DESPUES de su `error: ...`.
///
/// Es la primera linea no vacia que no sea un aviso de pandoc (`[WARNING] ...`,
/// `[INFO] ...`), que los escribe ANTES del error. Si solo hay avisos, el primero.
fn failure_line(stderr: &str) -> Option<&str> {
    let lines = || {
        stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    };
    lines()
        .find(|line| !line.starts_with("[WARNING]") && !line.starts_with("[INFO]"))
        .or_else(|| lines().next())
}

impl From<DictionaryError> for CliError {
    fn from(err: DictionaryError) -> Self {
        let message = match err {
            DictionaryError::Missing { path, message } => format!(
                "cannot read the dictionary file {}: {message}",
                path.display()
            ),
            DictionaryError::Invalid(detail) => format!("the dictionary is not valid ({detail})"),
        };
        Self::new(ErrorCode::DictionaryMissing, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apostrophe_sidecars::dictionary::DictionaryError;
    use apostrophe_sidecars::export::ExportError;
    use std::path::PathBuf;

    #[test]
    fn cada_codigo_es_estable_y_tiene_su_salida() {
        // El contrato para los agentes es el codigo y la salida, no el mensaje.
        let table = [
            (ErrorCode::Usage, "usage", 2),
            (ErrorCode::SidecarMissing, "sidecar_missing", 3),
            (ErrorCode::DictionaryMissing, "dictionary_missing", 3),
            (ErrorCode::ConversionFailed, "conversion_failed", 4),
            (ErrorCode::IoError, "io_error", 5),
        ];
        for (code, name, exit) in table {
            assert_eq!(code.as_str(), name);
            assert_eq!(code.exit_code(), exit, "{name}");
            assert_eq!(CliError::new(code, "x").exit_code(), exit, "{name}");
        }
    }

    #[test]
    fn un_sidecar_que_no_arranca_falta() {
        let err = CliError::from(ExportError::Launch {
            program: "pandoc".into(),
            message: "permission denied".into(),
        });
        assert_eq!(err.code, ErrorCode::SidecarMissing);
        assert_eq!(err.message, "could not start pandoc: permission denied");
    }

    #[test]
    fn un_sidecar_que_falla_es_un_fallo_de_conversion_con_su_stderr() {
        let err = CliError::from(ExportError::Failed {
            program: "typst".into(),
            code: Some(1),
            stderr: "error: unknown font\n".into(),
        });
        assert_eq!(err.code, ErrorCode::ConversionFailed);
        assert_eq!(
            err.message,
            "typst failed (exit code 1): error: unknown font"
        );

        let killed = CliError::from(ExportError::Failed {
            program: "pandoc".into(),
            code: None,
            stderr: String::new(),
        });
        assert_eq!(killed.code, ErrorCode::ConversionFailed);
        assert_eq!(killed.message, "pandoc failed (terminated by a signal)");
    }

    /// Lo que deja en stderr el pandoc real cuando no puede escribir: la causa y despues
    /// la traza de GHC.
    const PANDOC_BACKTRACE: &str = "pandoc: a.html: withFile: permission denied (Permission denied)\n\
        HasCallStack backtrace:\n  \
        collectBacktraces, called at libraries/ghc-internal/src/GHC/Internal/Exception.hs:169:13 in ghc-internal:GHC.Internal.Exception\n  \
        throwIO, called at libraries/ghc-internal/src/GHC/Internal/IO/Exception.hs:315:19 in ghc-internal:GHC.Internal.IO.Exception\n\n\n";

    fn conversion_message(program: &str, stderr: &str) -> String {
        CliError::from(ExportError::Failed {
            program: program.into(),
            code: Some(1),
            stderr: stderr.into(),
        })
        .message
    }

    #[test]
    fn un_fallo_de_conversion_conserva_solo_la_primera_linea_util() {
        assert_eq!(
            conversion_message("pandoc", PANDOC_BACKTRACE),
            "pandoc failed (exit code 1): pandoc: a.html: withFile: permission denied (Permission denied)"
        );
        // typst: el error y despues su contexto de fuente y sus avisos.
        assert_eq!(
            conversion_message(
                "typst",
                "error: unknown variable: foo\n  \u{250c}\u{2500} documento.typ:3:1\n  \u{2502}\n3 \u{2502} #foo\n\nwarning: unknown font family: x\n"
            ),
            "typst failed (exit code 1): error: unknown variable: foo"
        );
    }

    #[test]
    fn los_avisos_previos_de_pandoc_no_son_la_causa_del_fallo() {
        // pandoc escribe sus avisos (`[WARNING]`, `[INFO]`) antes del error.
        assert_eq!(
            conversion_message(
                "pandoc",
                "\n[WARNING] Could not fetch resource x.png: replacing image with description\n\
                 [INFO] Loaded y.png\n\
                 Error parsing YAML metadata at \"documento.md\" (line 1, column 1):\n\
                 YAML parse exception at line 2, column 0,\n"
            ),
            "pandoc failed (exit code 1): Error parsing YAML metadata at \"documento.md\" (line 1, column 1):"
        );
        // Si solo hay avisos, el primero es mejor que nada.
        assert_eq!(
            conversion_message("pandoc", "  [WARNING] Duplicate identifier 'a'\n"),
            "pandoc failed (exit code 1): [WARNING] Duplicate identifier 'a'"
        );
    }

    #[test]
    fn un_temporal_que_no_se_pudo_preparar_es_un_error_de_e_s() {
        let err = CliError::from(ExportError::Setup("disco lleno".into()));
        assert_eq!(err.code, ErrorCode::IoError);
        assert_eq!(
            err.message,
            "could not prepare the export files (disco lleno)"
        );
    }

    #[test]
    fn los_mensajes_son_propios_y_no_el_display_en_espanol_de_la_gui() {
        let source = ExportError::Launch {
            program: "pandoc".into(),
            message: "x".into(),
        };
        let gui = source.to_string();
        assert_ne!(CliError::from(source).message, gui);
    }

    #[test]
    fn un_diccionario_ausente_o_roto_es_dictionary_missing() {
        let missing = CliError::from(DictionaryError::Missing {
            path: PathBuf::from("dicts").join("es.aff"),
            message: "not found".into(),
        });
        assert_eq!(missing.code, ErrorCode::DictionaryMissing);
        assert_eq!(
            missing.message,
            format!(
                "cannot read the dictionary file {}: not found",
                PathBuf::from("dicts").join("es.aff").display()
            )
        );

        let invalid = CliError::from(DictionaryError::Invalid("bad .dic".into()));
        assert_eq!(invalid.code, ErrorCode::DictionaryMissing);
        assert_eq!(invalid.message, "the dictionary is not valid (bad .dic)");
    }
}
