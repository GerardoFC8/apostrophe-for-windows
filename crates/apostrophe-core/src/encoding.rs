//! Decodificacion de bytes de archivo a texto con deteccion de encoding.
//!
//! Expone `decode_bytes`: detecta el encoding (BOM, UTF-8 estricto, legacy via
//! chardetng) y devuelve el texto + el nombre del encoding detectado. NUNCA falla;
//! bytes irrecuperables se reemplazan con U+FFFD (best-effort).
//!
//! El campo `encoding_name` de `Decoded` prepara M8.4 (guardar preservando encoding)
//! sin reescribir esta firma.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_8};

/// Resultado de decodificar bytes de archivo: texto + nombre del encoding detectado
/// (p.ej. "UTF-8", "windows-1252", "UTF-16LE"). El encoding_name se devuelve aunque
/// M8.3 no lo persista todavia: prepara M8.4 (guardar preservando encoding) sin
/// reescribir esta firma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding_name: String,
}

/// Decodifica bytes de un archivo a texto detectando el encoding. NUNCA falla: ante
/// bytes irrecuperables degrada a U+FFFD (best-effort, igual que el upstream). Orden:
/// 1) BOM (UTF-8/UTF-16LE/UTF-16BE), 2) UTF-8 estricto (camino feliz),
/// 3) deteccion legacy con chardetng (windows-1252/latin-1, etc.).
pub fn decode_bytes(bytes: &[u8]) -> Decoded {
    if let Some((enc, bom_len)) = Encoding::for_bom(bytes) {
        let (text, _, _) = enc.decode(&bytes[bom_len..]);
        return Decoded {
            text: text.into_owned(),
            encoding_name: enc.name().to_string(),
        };
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Decoded {
            text: s.to_string(),
            encoding_name: UTF_8.name().to_string(),
        };
    }
    let mut det = EncodingDetector::new(Iso2022JpDetection::Deny);
    det.feed(bytes, true);
    let enc = det.guess(None, Utf8Detection::Allow);
    let (text, _, _) = enc.decode(bytes);
    Decoded {
        text: text.into_owned(),
        encoding_name: enc.name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_utf8_plano() {
        let bytes = b"# Hola mundo\n";
        let result = decode_bytes(bytes);
        assert_eq!(result.text, "# Hola mundo\n");
        assert_eq!(result.encoding_name, "UTF-8");
    }

    #[test]
    fn decode_utf8_con_acentos() {
        let input = "camión árbol ñandú";
        let bytes = input.as_bytes();
        let result = decode_bytes(bytes);
        assert_eq!(result.text, input);
        assert_eq!(result.encoding_name, "UTF-8");
    }

    #[test]
    fn decode_utf8_con_bom() {
        // BOM UTF-8: EF BB BF
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("áéíóú".as_bytes());
        let result = decode_bytes(&bytes);
        assert_eq!(result.text, "áéíóú");
        assert_eq!(result.encoding_name, "UTF-8");
    }

    #[test]
    fn decode_utf16le_con_bom() {
        // BOM UTF-16LE: FF FE, luego "áéíóú" como pares de bytes little-endian.
        // NOTA: encoding_rs::UTF_16LE.encode() NO produce bytes UTF-16LE — devuelve
        // UTF-8 (quirk de la spec WHATWG: UTF-16 es solo para decodificacion en el
        // encoder). Usamos str::encode_utf16() de la stdlib para construir los pares.
        let text = "áéíóú";
        let mut bytes = vec![0xFF, 0xFE]; // BOM LE
        for cu in text.encode_utf16() {
            bytes.push((cu & 0xFF) as u8);
            bytes.push((cu >> 8) as u8);
        }
        let result = decode_bytes(&bytes);
        assert_eq!(result.text, text);
        assert_eq!(result.encoding_name, "UTF-16LE");
    }

    #[test]
    fn decode_utf16be_con_bom() {
        // BOM UTF-16BE: FE FF, luego "hola" como pares de bytes big-endian.
        // Misma razon que en decode_utf16le_con_bom: str::encode_utf16() da los
        // code units; los invertimos para big-endian.
        let text = "hola";
        let mut bytes = vec![0xFE, 0xFF]; // BOM BE
        for cu in text.encode_utf16() {
            bytes.push((cu >> 8) as u8);
            bytes.push((cu & 0xFF) as u8);
        }
        let result = decode_bytes(&bytes);
        assert_eq!(result.text, text);
        assert_eq!(result.encoding_name, "UTF-16BE");
    }

    #[test]
    fn decode_windows1252_acentos() {
        // Construir el input con encoding_rs (NO bytes hardcodeados)
        let text = "canción español ñ";
        let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(text);
        let result = decode_bytes(&bytes);
        assert_eq!(result.text, text);
        // El encoding_name devuelto por chardetng puede variar; solo verificamos
        // que la deteccion no usó UTF-8 (porque los bytes no son UTF-8 valido)
        // y que el texto decodificado es correcto.
        assert_ne!(result.encoding_name, "UTF-8");
    }

    #[test]
    fn decode_latin1_comilla_no_rompe() {
        // 0xA1 es '¡' en Windows-1252 / Latin-1, invalido en UTF-8
        let bytes = &[0xA1u8, b'H', b'o', b'l', b'a'];
        // No debe entrar por el camino UTF-8 y no debe hacer panic
        let result = decode_bytes(bytes);
        assert_ne!(result.encoding_name, "UTF-8");
        assert!(!result.text.is_empty());
    }

    #[test]
    fn decode_bytes_vacios() {
        let result = decode_bytes(&[]);
        assert_eq!(result.text, "");
        assert_eq!(result.encoding_name, "UTF-8");
    }
}
