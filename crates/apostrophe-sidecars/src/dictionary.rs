//! Diccionario Hunspell `es` (M6): el par `es.aff` / `es.dic` que el instalador deja en
//! `dictionaries/`. La GUI y la CLI lo cargan con la MISMA funcion; cada una decide en que
//! carpetas buscarlo.

use std::fmt;
use std::path::{Path, PathBuf};

use apostrophe_core::spellcheck::SpellbookDictionary;

/// Por que no se pudo cargar el diccionario. Se distingue la falta de archivos de un
/// diccionario roto: lo primero se arregla instalando o indicando otra carpeta, lo
/// segundo no.
#[derive(Debug)]
pub enum DictionaryError {
    /// No se pudo leer uno de los dos archivos (no existe, sin permisos…).
    Missing { path: PathBuf, message: String },
    /// Los archivos estan, pero no son UTF-8 o spellbook no los acepta.
    Invalid(String),
}

impl fmt::Display for DictionaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { path, message } => {
                let name = path.file_name().unwrap_or(path.as_os_str());
                write!(f, "{}: {message}", name.to_string_lossy())
            }
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DictionaryError {}

/// Carga `es.aff` y `es.dic` de `dir`. Es CARO (parsea cientos de miles de palabras): la
/// GUI lo hace una vez, fuera del hilo de eventos, y lo cachea.
pub fn load_es(dir: &Path) -> Result<SpellbookDictionary, DictionaryError> {
    let aff = read(&dir.join("es.aff"))?;
    let dic = read(&dir.join("es.dic"))?;
    SpellbookDictionary::load(&aff, &dic).map_err(DictionaryError::Invalid)
}

fn read(path: &Path) -> Result<String, DictionaryError> {
    std::fs::read_to_string(path).map_err(|e| {
        let missing = DictionaryError::Missing {
            path: path.to_path_buf(),
            message: e.to_string(),
        };
        // `read_to_string` reporta un archivo que no es UTF-8 como `InvalidData`: existe,
        // asi que no "falta"; el mensaje sigue diciendo de que archivo se trata.
        if e.kind() == std::io::ErrorKind::InvalidData {
            DictionaryError::Invalid(missing.to_string())
        } else {
            missing
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use apostrophe_core::spellcheck::Dictionary;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// Carpeta propia del test con el `es.aff` y el `es.dic` indicados (`None` = no se crea).
    fn dict_dir(label: &str, aff: Option<&str>, dic: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "apostrophe-dict-test-{}-{}-{label}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(aff) = aff {
            std::fs::write(dir.join("es.aff"), aff).unwrap();
        }
        if let Some(dic) = dic {
            std::fs::write(dir.join("es.dic"), dic).unwrap();
        }
        dir
    }

    #[test]
    fn carga_el_par_es_de_la_carpeta() {
        let dir = dict_dir("ok", Some("SET UTF-8\n"), Some("2\nhola\nmundo\n"));
        let dict = load_es(&dir).expect("el diccionario deberia cargar");
        assert!(dict.check("hola"));
        assert!(dict.check("mundo"));
        assert!(!dict.check("mundoo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sin_es_aff_falta_el_diccionario() {
        let dir = dict_dir("sin-aff", None, Some("1\nhola\n"));
        match load_es(&dir) {
            Err(DictionaryError::Missing { path, .. }) => assert_eq!(path, dir.join("es.aff")),
            Err(other) => panic!("se esperaba Missing: {other}"),
            Ok(_) => panic!("no deberia cargar sin es.aff"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sin_es_dic_falta_el_diccionario() {
        let dir = dict_dir("sin-dic", Some("SET UTF-8\n"), None);
        match load_es(&dir) {
            Err(DictionaryError::Missing { path, .. }) => assert_eq!(path, dir.join("es.dic")),
            Err(other) => panic!("se esperaba Missing: {other}"),
            Ok(_) => panic!("no deberia cargar sin es.dic"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_dic_mal_formado_es_un_diccionario_invalido() {
        // La primera linea de un `.dic` es la cantidad de palabras.
        let dir = dict_dir(
            "invalido",
            Some("SET UTF-8\n"),
            Some("no-es-un-numero\nhola\n"),
        );
        match load_es(&dir) {
            Err(DictionaryError::Invalid(message)) => {
                assert!(
                    message.contains("no se pudo cargar el diccionario"),
                    "{message}"
                )
            }
            Err(other) => panic!("se esperaba Invalid: {other}"),
            Ok(_) => panic!("no deberia cargar un .dic mal formado"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_archivo_que_no_es_utf8_es_un_diccionario_invalido() {
        // El archivo esta: indicar otra carpeta no lo arregla, asi que no "falta".
        let dir = dict_dir("latin1", Some("SET UTF-8\n"), Some("1\nhola\n"));
        std::fs::write(dir.join("es.aff"), b"SET ISO8859-1\nTRY \xf1\n").unwrap();
        match load_es(&dir) {
            Err(DictionaryError::Invalid(message)) => {
                assert!(message.starts_with("es.aff: "), "{message}")
            }
            Err(other) => panic!("se esperaba Invalid: {other}"),
            Ok(_) => panic!("no deberia cargar un .aff que no es UTF-8"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn el_mensaje_dice_que_archivo_falta() {
        let dir = dict_dir("mensaje", None, None);
        let err = load_es(&dir)
            .err()
            .expect("no deberia cargar una carpeta vacia");
        let shown = err.to_string();
        assert!(shown.starts_with("es.aff: "), "{shown}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
