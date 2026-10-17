//! Lo que la CLI escribe: el JSON de `--json` (un objeto por linea, solo ASCII) y las
//! lineas de error y aviso del modo texto.

use std::io;

use serde::Serialize;
use serde_json::ser::Formatter;

use crate::error::CliError;
use crate::PROGRAM;

/// Formateador compacto de serde_json que escapa TODO caracter no ASCII como `\uXXXX`
/// (fuera del BMP, como par sustituto).
///
/// Asi stdout es valido en cualquier pagina de codigos: PowerShell 5.1 decodifica la
/// salida de un proceso con la de la consola (no UTF-8), y un acento sin escapar llegaria
/// roto a `ConvertFrom-Json`. Con escapes, el texto se recupera exacto al parsear.
pub struct AsciiFormatter;

impl Formatter for AsciiFormatter {
    fn write_string_fragment<W>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        // Los tramos ASCII se copian enteros; solo se escapa caracter a caracter lo demas.
        let mut start = 0;
        for (i, ch) in fragment.char_indices() {
            if ch.is_ascii() {
                continue;
            }
            writer.write_all(&fragment.as_bytes()[start..i])?;
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                write!(writer, "\\u{unit:04x}")?;
            }
            start = i + ch.len_utf8();
        }
        writer.write_all(&fragment.as_bytes()[start..])
    }
}

/// `value` como JSON compacto en una linea (sin salto final), solo ASCII.
pub fn to_ascii_json<T: Serialize + ?Sized>(value: &T) -> String {
    let mut out = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, AsciiFormatter);
    // Los reportes son structs de datos sin mapas con claves raras: no pueden fallar.
    value
        .serialize(&mut serializer)
        .expect("los reportes de la CLI siempre se serializan");
    // El formateador solo emite ASCII.
    String::from_utf8(out).expect("el JSON de la CLI es ASCII")
}

/// Sobre de exito: `{"ok":true, ...campos del reporte}`.
#[derive(Serialize)]
struct Success<'a, T: Serialize> {
    ok: bool,
    #[serde(flatten)]
    report: &'a T,
}

/// Sobre de error: `{"ok":false,"error":{"code","message"}}`.
#[derive(Serialize)]
struct Failure<'a> {
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

pub fn success_json<T: Serialize>(report: &T) -> String {
    to_ascii_json(&Success { ok: true, report })
}

pub fn error_json(err: &CliError) -> String {
    to_ascii_json(&Failure {
        ok: false,
        error: ErrorBody {
            code: err.code.as_str(),
            message: &err.message,
        },
    })
}

/// Linea de error del modo texto, para stderr: `elision: error[code]: mensaje`.
pub fn error_text(err: &CliError) -> String {
    format!("{PROGRAM}: error[{}]: {}\n", err.code.as_str(), err.message)
}

/// Linea de aviso del modo texto, para stderr.
pub fn warning_text(message: &str) -> String {
    format!("{PROGRAM}: warning: {message}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{CliError, ErrorCode};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample<'a> {
        file: &'a str,
        count: usize,
    }

    #[test]
    fn el_json_es_ascii_puro_y_se_recupera_el_texto_original() {
        let text = "canción 😀";
        let json = to_ascii_json(&Sample {
            file: text,
            count: 1,
        });
        assert!(json.is_ascii(), "{json}");
        // Fuera del BMP va como par sustituto, igual que en UTF-16.
        assert_eq!(json, r#"{"file":"canci\u00f3n \ud83d\ude00","count":1}"#);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["file"], text);
    }

    #[test]
    fn las_comillas_y_los_controles_se_siguen_escapando() {
        let json = to_ascii_json(&Sample {
            file: "a \"b\"\\\n\tñ",
            count: 0,
        });
        assert_eq!(json, r#"{"file":"a \"b\"\\\n\t\u00f1","count":0}"#);
    }

    #[test]
    fn el_exito_lleva_ok_true_antes_del_reporte() {
        let json = success_json(&Sample {
            file: "a.md",
            count: 2,
        });
        assert_eq!(json, r#"{"ok":true,"file":"a.md","count":2}"#);
    }

    #[test]
    fn el_error_lleva_codigo_y_mensaje() {
        let err = CliError::new(ErrorCode::IoError, "cannot read año.md");
        assert_eq!(
            error_json(&err),
            r#"{"ok":false,"error":{"code":"io_error","message":"cannot read a\u00f1o.md"}}"#
        );
    }

    #[test]
    fn en_texto_el_error_y_el_aviso_llevan_el_nombre_del_programa() {
        let err = CliError::usage("unknown format");
        assert_eq!(error_text(&err), "elision: error[usage]: unknown format\n");
        assert_eq!(
            warning_text("table left untouched"),
            "elision: warning: table left untouched\n"
        );
    }
}
