//! Spellcheck (ADR-0001 D2, M6): reemplaza libspelling (no existe en Windows).
//!
//! Arquitectura hexagonal: este modulo define el PUERTO (`Dictionary` trait) y la
//! LOGICA PURA (`SpellChecker`: tokenizado, skip de markup, construccion de spans).
//! El motor real (spellbook) entra como ADAPTADOR detras del feature `spellbook-backend`.
//! Asi el dominio depende del trait y la logica se testea con un `FakeDictionary` en
//! memoria, sin parsear .aff/.dic ni linkear nada pesado (TDD instantaneo).
//!
//! GOTCHA CENTRAL (verificado en el codigo, no asumido): NO se puede reusar
//! `markup::strip` para limpiar antes de chequear — `strip` COLAPSA posiciones
//! (`**hola**` -> `hola`), asi que los offsets del texto stripeado NO mapearian al
//! editor. En cambio, se tokeniza el texto CRUDO y se SALTEAN posicionalmente los
//! tokens dentro de markup (codigo, math, URLs de links). Asi `from`/`to` caen 1:1
//! sobre la palabra que ve CodeMirror.
//!
//! OFFSETS: `from`/`to` son CHAR INDEX (code point), NO byte offset. Para todo el BMP
//! (espanol completo) char index === code unit UTF-16, asi que CM6 los consume directo.
//! Byte offsets romperian con cualquier acento ('camion' tiene 'o' multibyte en UTF-8).

/// Un error ortografico. `from`/`to` en CHAR INDEX sobre el texto crudo del editor.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misspelling {
    /// Palabra original (para mostrar en el menu).
    pub word: String,
    /// Char index inclusive.
    pub from: usize,
    /// Char index exclusive.
    pub to: usize,
    /// Sugerencias (<= 5), eager.
    pub suggestions: Vec<String>,
}

/// PUERTO: el nucleo depende de ESTE trait, no de spellbook (ADR-0001). El adaptador
/// `spellbook-backend` provee la impl concreta; los tests usan un `FakeDictionary`.
pub trait Dictionary {
    /// `true` = la palabra es correcta.
    fn check(&self, word: &str) -> bool;
    /// Llena `out` con sugerencias (espejo del out-param de spellbook).
    fn suggest(&self, word: &str, out: &mut Vec<String>);
}

/// Maximo de sugerencias por palabra (truncado).
pub const MAX_SUGGESTIONS: usize = 5;

/// Logica pura de spellcheck: generica sobre `Dictionary` -> testeable sin spellbook.
pub struct SpellChecker;

impl SpellChecker {
    /// Chequea `text` (crudo del editor). Si `focus_mode` es `true` devuelve `[]`
    /// (regla del upstream, ADR-0001 D2). Si no, retorna las palabras mal escritas
    /// con char-offsets y hasta [`MAX_SUGGESTIONS`] sugerencias cada una, en orden
    /// ascendente de `from`.
    pub fn check_text<D: Dictionary>(dict: &D, text: &str, focus_mode: bool) -> Vec<Misspelling> {
        if focus_mode {
            return Vec::new();
        }
        let skip = skip_ranges(text);
        let mut out = Vec::new();
        for (from, to, word) in tokenize(text) {
            if overlaps_any(from, to, &skip) {
                continue; // dentro de codigo / math / URL: no se chequea
            }
            if dict.check(&word) {
                continue;
            }
            let mut suggestions = Vec::new();
            dict.suggest(&word, &mut suggestions);
            suggestions.truncate(MAX_SUGGESTIONS);
            out.push(Misspelling {
                word,
                from,
                to,
                suggestions,
            });
        }
        out
    }
}

/// Una palabra = corrida maximal de chars alfabeticos (cubre acentos del espanol;
/// excluye digitos y puntuacion). Emite CHAR offsets (no byte).
fn tokenize(text: &str) -> Vec<(usize, usize, String)> {
    let mut words = Vec::new();
    let mut start = 0usize;
    let mut cur = String::new();
    let mut ci = 0usize; // char index
    for c in text.chars() {
        if is_word_char(c) {
            if cur.is_empty() {
                start = ci;
            }
            cur.push(c);
        } else if !cur.is_empty() {
            words.push((start, ci, std::mem::take(&mut cur)));
        }
        ci += 1;
    }
    if !cur.is_empty() {
        words.push((start, ci, cur));
    }
    words
}

/// Un char es parte de una palabra si es alfabetico O una MARCA COMBINANTE (acentos
/// DESCOMPUESTOS NFD, p.ej. U+0301 que puede llegar al pegar texto). Sin esto, un acento
/// NFD partiria la palabra en dos y se marcarian falsos errores (review M6). No cambia
/// el conteo de chars -> los offsets siguen 1:1 con CodeMirror (a diferencia de
/// normalizar a NFC, que SI cambiaria la cantidad de chars y desincronizaria).
fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || is_combining_mark(c)
}

fn is_combining_mark(c: char) -> bool {
    matches!(
        c as u32,
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F
    )
}

/// Rangos (char index) que NO se deben chequear: codigo (inline y fence, corridas de
/// backticks), math (`$`/`$$`), y URLs de links/imagenes (el `(...)` de `](...)`).
fn skip_ranges(text: &str) -> Vec<(usize, usize)> {
    let cs: Vec<char> = text.chars().collect();
    let n = cs.len();
    let mut ranges = Vec::new();
    let mut i = 0usize;
    while i < n {
        match cs[i] {
            '`' => {
                let start = i;
                let mut run = 0usize;
                while i < n && cs[i] == '`' {
                    run += 1;
                    i += 1;
                }
                if let Some(close) = find_closing(&cs, i, '`', run) {
                    ranges.push((start, close)); // incluye los fences
                    i = close;
                }
                // sin cierre: `i` ya paso la corrida de apertura; backtick literal
            }
            '$' => {
                let start = i;
                let mut run = 0usize;
                while i < n && cs[i] == '$' {
                    run += 1;
                    i += 1;
                }
                // Math solo si el contenido NO empieza ni termina con espacio (estilo
                // CommonMark-math): asi "$5 y vale $7" (precios) NO se confunde con math
                // y no se silencian errores entre dos signos `$` (review M6).
                let opens = i < n && !cs[i].is_whitespace();
                if opens {
                    if let Some(close) = find_math_close(&cs, i, run) {
                        ranges.push((start, close));
                        i = close;
                    }
                }
            }
            ']' if i + 1 < n && cs[i + 1] == '(' => {
                let start = i + 1; // el '('
                let mut depth = 0i32;
                let mut j = i + 1;
                let mut close = None;
                while j < n {
                    match cs[j] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                close = Some(j + 1);
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if let Some(cl) = close {
                    ranges.push((start, cl));
                    i = cl;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    ranges
}

/// Busca desde `from` una corrida de >= `min` veces `delim`. Devuelve el indice
/// (exclusivo) del final de la corrida de cierre, o `None` si no hay.
fn find_closing(cs: &[char], from: usize, delim: char, min: usize) -> Option<usize> {
    let n = cs.len();
    let mut j = from;
    while j < n {
        if cs[j] == delim {
            let mut k = j;
            let mut r = 0usize;
            while k < n && cs[k] == delim {
                r += 1;
                k += 1;
            }
            if r >= min {
                return Some(k);
            }
            j = k;
        } else {
            j += 1;
        }
    }
    None
}

/// Como `find_closing` pero para math: el char antes de la corrida de cierre NO puede
/// ser espacio (estilo CommonMark-math), asi el `$` de un precio no cierra el de otro.
fn find_math_close(cs: &[char], from: usize, min: usize) -> Option<usize> {
    let n = cs.len();
    let mut j = from;
    while j < n {
        if cs[j] == '$' {
            let mut k = j;
            let mut r = 0usize;
            while k < n && cs[k] == '$' {
                r += 1;
                k += 1;
            }
            if r >= min && j > from && !cs[j - 1].is_whitespace() {
                return Some(k);
            }
            j = k;
        } else {
            j += 1;
        }
    }
    None
}

fn overlaps_any(from: usize, to: usize, skip: &[(usize, usize)]) -> bool {
    skip.iter().any(|&(sf, st)| from < st && sf < to)
}

// === Adaptador secundario: spellbook (detras del feature `spellbook-backend`) ===
// El nucleo NO depende de spellbook salvo bajo este feature; el adaptador src-tauri
// lo activa. spellbook es puro-Rust (hashbrown + foldhash, sin C) -> build MSVC limpio.

/// Diccionario Hunspell real (spellbook). Implementa el puerto [`Dictionary`].
#[cfg(feature = "spellbook-backend")]
pub struct SpellbookDictionary(spellbook::Dictionary);

#[cfg(feature = "spellbook-backend")]
impl SpellbookDictionary {
    /// Construye el diccionario desde el contenido de un par `.aff`/`.dic`. Es CARO
    /// (parsea cientos de miles de palabras) -> cargar UNA vez y cachear.
    pub fn load(aff: &str, dic: &str) -> Result<Self, String> {
        spellbook::Dictionary::new(aff, dic)
            .map(SpellbookDictionary)
            .map_err(|e| format!("no se pudo cargar el diccionario: {e}"))
    }
}

#[cfg(feature = "spellbook-backend")]
impl Dictionary for SpellbookDictionary {
    fn check(&self, word: &str) -> bool {
        self.0.check(word)
    }
    fn suggest(&self, word: &str, out: &mut Vec<String>) {
        self.0.suggest(word, out);
        out.truncate(MAX_SUGGESTIONS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// Diccionario falso en memoria: TDD de spans/offsets/focus_mode sin spellbook.
    struct FakeDict {
        known: HashSet<String>,
        sugg: HashMap<String, Vec<String>>,
    }
    impl FakeDict {
        fn new(known: &[&str]) -> Self {
            Self {
                known: known.iter().map(|s| s.to_string()).collect(),
                sugg: HashMap::new(),
            }
        }
        fn with_sugg(mut self, word: &str, s: &[&str]) -> Self {
            self.sugg
                .insert(word.to_string(), s.iter().map(|x| x.to_string()).collect());
            self
        }
    }
    impl Dictionary for FakeDict {
        fn check(&self, word: &str) -> bool {
            self.known.contains(word)
        }
        fn suggest(&self, word: &str, out: &mut Vec<String>) {
            if let Some(s) = self.sugg.get(word) {
                out.extend(s.iter().cloned());
            }
        }
    }

    #[test]
    fn focus_mode_returns_empty() {
        let dict = FakeDict::new(&[]);
        assert_eq!(
            SpellChecker::check_text(&dict, "palabra malescrita", true),
            vec![]
        );
    }

    #[test]
    fn correct_words_have_no_misspellings() {
        let dict = FakeDict::new(&["hola", "mundo"]);
        assert_eq!(SpellChecker::check_text(&dict, "hola mundo", false), vec![]);
    }

    #[test]
    fn flags_misspelled_word_with_char_offsets() {
        let dict = FakeDict::new(&["hola"]);
        let r = SpellChecker::check_text(&dict, "hola zzz", false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].word, "zzz");
        assert_eq!((r[0].from, r[0].to), (5, 8));
    }

    #[test]
    fn char_offsets_not_byte_offsets_with_accents() {
        // "camión zzz": 'ó' es 2 bytes. char offset de "zzz" = 7 (byte offset = 8).
        let dict = FakeDict::new(&["camión"]);
        let r = SpellChecker::check_text(&dict, "camión zzz", false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].word, "zzz");
        assert_eq!(
            (r[0].from, r[0].to),
            (7, 10),
            "CHAR index, NO byte (que seria 8..11)"
        );
    }

    #[test]
    fn skips_inline_code() {
        let dict = FakeDict::new(&["usa"]); // "fn" no esta en el dict, pero esta en codigo
        assert_eq!(SpellChecker::check_text(&dict, "usa `fn`", false), vec![]);
    }

    #[test]
    fn skips_fenced_code_block() {
        let dict = FakeDict::new(&["codigo"]);
        let text = "codigo\n```\nzzz qqq\n```";
        assert_eq!(SpellChecker::check_text(&dict, text, false), vec![]);
    }

    #[test]
    fn skips_link_url_but_checks_text() {
        // "[ok](http://zzz)": "ok" se chequea (correcta), la URL no.
        let dict = FakeDict::new(&["ok"]);
        assert_eq!(
            SpellChecker::check_text(&dict, "[ok](http://zzz)", false),
            vec![]
        );
    }

    #[test]
    fn checks_link_text_flags_bad_word() {
        let dict = FakeDict::new(&[]); // "zz" desconocida en el TEXTO del link
        let r = SpellChecker::check_text(&dict, "[zz](http://x)", false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].word, "zz");
        assert_eq!((r[0].from, r[0].to), (1, 3));
    }

    #[test]
    fn skips_math() {
        let dict = FakeDict::new(&[]);
        assert_eq!(SpellChecker::check_text(&dict, "$E=mc^2$", false), vec![]);
    }

    #[test]
    fn lone_dollar_does_not_swallow_rest() {
        // Un '$' suelto (precio) NO debe apagar el chequeo del resto.
        let dict = FakeDict::new(&["cuesta", "pesos"]);
        let r = SpellChecker::check_text(&dict, "cuesta $5 zzz pesos", false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].word, "zzz");
    }

    #[test]
    fn two_price_dollars_do_not_swallow_between() {
        // "$5 y vale $7": el 2do '$' NO cierra el 1ro (espacio antes) -> no es math,
        // y las palabras entre los precios SI se chequean (review M6).
        let dict = FakeDict::new(&["cuesta", "y", "vale", "pesos"]);
        let r = SpellChecker::check_text(&dict, "cuesta $5 y vale $7 zzz pesos", false);
        assert_eq!(
            r.len(),
            1,
            "solo 'zzz' (lo de entre precios no se silencia)"
        );
        assert_eq!(r[0].word, "zzz");
    }

    #[test]
    fn nfd_accent_keeps_word_whole() {
        // "camión" DESCOMPUESTO (NFD): c a m i o U+0301 n = 7 chars. La marca combinante
        // no debe partir la palabra (review M6).
        let nfd = "camio\u{0301}n";
        let dict = FakeDict::new(&[]);
        let r = SpellChecker::check_text(&dict, nfd, false);
        assert_eq!(r.len(), 1, "UNA palabra, no fragmentada en 'camio' + 'n'");
        assert_eq!(r[0].word, nfd);
        assert_eq!((r[0].from, r[0].to), (0, 7));
    }

    #[test]
    fn suggestions_truncated_to_five() {
        let dict = FakeDict::new(&[]).with_sugg("zzz", &["a", "b", "c", "d", "e", "f", "g"]);
        let r = SpellChecker::check_text(&dict, "zzz", false);
        assert_eq!(r[0].suggestions, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn multiple_errors_ordered_ascending() {
        let dict = FakeDict::new(&["hola"]);
        let r = SpellChecker::check_text(&dict, "xxx hola yyy", false);
        assert_eq!(r.len(), 2);
        assert!(r[0].from < r[1].from);
        assert_eq!((r[0].word.as_str(), r[1].word.as_str()), ("xxx", "yyy"));
    }

    #[cfg(feature = "spellbook-backend")]
    #[test]
    fn integration_real_es_dictionary() {
        // Carga el es_ES real (si esta provisionado) y verifica el motor end-to-end.
        let aff_p = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../src-tauri/dictionaries/es.aff"
        );
        let dic_p = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../src-tauri/dictionaries/es.dic"
        );
        let (Ok(aff), Ok(dic)) = (
            std::fs::read_to_string(aff_p),
            std::fs::read_to_string(dic_p),
        ) else {
            // La CI define ELISION_REQUIRE_SIDECARS porque ahi siempre lo provisiona: un
            // salto silencioso esconderia una regresion.
            if std::env::var_os("ELISION_REQUIRE_SIDECARS").is_some() {
                panic!(
                    "diccionario es no provisionado; ELISION_REQUIRE_SIDECARS exige que exista."
                );
            }
            eprintln!(
                "SALTADO: diccionario es no provisionado. Ejecuta scripts/fetch-dictionaries.ps1."
            );
            return;
        };
        let dict = SpellbookDictionary::load(&aff, &dic).expect("cargar es dict");
        let r = SpellChecker::check_text(&dict, "casa kasa", false);
        assert_eq!(r.len(), 1, "'casa' correcta, 'kasa' incorrecta");
        assert_eq!(r[0].word, "kasa");
        assert!(
            !r[0].suggestions.is_empty(),
            "spellbook deberia sugerir algo para 'kasa'"
        );
    }
}
