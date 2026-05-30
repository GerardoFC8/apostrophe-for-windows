//! Contador de estadisticas de texto: chars / words (con CJK) / sentences /
//! paragraphs / read_time. Port fiel de `apostrophe/stats_counter.py` (upstream
//! GNOME GitLab, rama main, commit 75202362).
//!
//! Fidelidad: los valores esperados de los tests se OBTUVIERON ejecutando el
//! algoritmo original (modulo `regex` de Python), no se calcularon a ojo. Ver
//! `tools/oracle/stats_oracle.py` (el oraculo) y `docs/upstream-reference/`.
//!
//! Estrategia de port (ver brief del ground truth):
//! - WORDS, SENTENCES: crate `regex` (no usan lookaround, tiempo lineal).
//! - CHARACTERS, PARAGRAPHS: codigo imperativo (usan lookahead `(?!\s)` /
//!   `(?=\n{2,}|\Z)` que el crate `regex` no soporta).
//! - READ_TIME: aritmetica f64 truncada (200 wpm).
//!
//! ALCANCE: este modulo cuenta sobre TEXTO PLANO. El PASO 0 del pipeline original
//! (stripping de markdown previo al conteo) depende de `markup_regex.py` y se
//! incorpora junto al modulo de markup/highlighting. La limpieza es idempotente
//! sobre texto sin sintaxis markdown, por lo que el conteo puro es testeable solo.

use std::sync::LazyLock;

use regex::Regex;

/// WORDS (literal del upstream): un code point en [U+3040..U+FFFF] = 1 palabra
/// (CJK/Hiragana/Katakana/Hangul/fullwidth), o secuencias `\w` con a lo sumo un
/// `\S` intermedio por iteracion. El crate `regex` es Unicode por defecto (= re.UNICODE).
static WORDS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\u{3040}-\u{FFFF}]|(?:\w+\S?\w*)+").unwrap());

/// SENTENCES (literal del upstream): un char no-`\n` seguido de 1+ terminadores
/// (incluye `\n` y variantes Unicode de fin de oracion).
static SENTENCES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[^\n][.。।෴۔።?՞;⸮؟？፧꘏⳺⳻⁇﹖⁈⁉‽!﹗！՜߹႟᥄\n]+").unwrap());

/// Tiempo estimado de lectura, descompuesto como en el original (tupla h/m/s).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadTime {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
}

/// Estadisticas de un texto. El orden y los campos replican la 5-tupla que
/// devuelve `do_count` en el upstream: (characters, words, sentences,
/// paragraphs, read_time). El docstring original dice 4 campos pero esta
/// desactualizado: son 5.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub characters: usize,
    pub words: usize,
    pub sentences: usize,
    pub paragraphs: usize,
    pub read_time: ReadTime,
}

/// Cuenta las estadisticas de `text` (asumido texto plano, sin markdown).
pub fn count(text: &str) -> Stats {
    let words = count_words(text);
    Stats {
        characters: count_characters(text),
        words,
        sentences: count_sentences(text),
        paragraphs: count_paragraphs(text),
        read_time: read_time(words),
    }
}

/// CHARACTERS = `[^\s]|(?:[^\S\n](?!\s))`. Imperativo (el original usa el lookahead
/// `(?!\s)` que el crate `regex` no soporta). Cuenta: cada char no-whitespace, MAS
/// cada whitespace-que-no-es-`\n` que NO va seguido de otro whitespace (espacios
/// "internos" sueltos; los multiples colapsan y `\n` nunca cuenta).
fn count_characters(text: &str) -> usize {
    let mut count = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !c.is_whitespace() {
            count += 1; // alternativa 1: [^\s]
        } else if c != '\n' {
            // alternativa 2: [^\S\n] con lookahead (?!\s). El lookahead se cumple
            // tambien al final del texto (no hay char siguiente). Una sola pasada,
            // sin materializar el texto (importa en documentos grandes, ver ADR-0001).
            let next_is_ws = chars.peek().is_some_and(|n| n.is_whitespace());
            if !next_is_ws {
                count += 1;
            }
        }
    }
    count
}

fn count_words(text: &str) -> usize {
    WORDS_RE.find_iter(text).count()
}

fn count_sentences(text: &str) -> usize {
    SENTENCES_RE.find_iter(text).count()
}

/// PARAGRAPHS = `(?:[^\n]+\n?)+(?=\n{2,}|\Z)`. Imperativo (el original usa el
/// lookahead `(?=\n{2,}|\Z)`). Un parrafo = bloque con algun char no-`\n`,
/// separado por >=2 `\n` consecutivos o por el fin del texto. Un `\n` simple
/// interno NO separa.
fn count_paragraphs(text: &str) -> usize {
    let mut count = 0;
    let mut has_content = false;
    let mut newline_run = 0;
    for c in text.chars() {
        if c == '\n' {
            newline_run += 1;
            if newline_run >= 2 && has_content {
                count += 1;
                has_content = false;
            }
        } else {
            newline_run = 0;
            has_content = true;
        }
    }
    if has_content {
        count += 1;
    }
    count
}

/// Tiempo de lectura a 200 palabras/minuto, truncando (no redondea). Replica
/// `divmod(word_count/200*60, 60)` del upstream con aritmetica `f64` identica.
pub fn read_time(words: usize) -> ReadTime {
    let total_seconds = words as f64 / 200.0 * 60.0;
    ReadTime {
        hours: (total_seconds / 3600.0).trunc() as u32,
        minutes: ((total_seconds % 3600.0) / 60.0).trunc() as u32,
        seconds: (total_seconds % 60.0).trunc() as u32,
    }
}

#[cfg(test)]
mod tests {
    //! Casos canonicos generados por `tools/oracle/stats_oracle.py` ejecutando
    //! los 4 regex reales de Apostrophe con el modulo `regex` 2026.5.9.
    use super::*;

    fn rt(hours: u32, minutes: u32, seconds: u32) -> ReadTime {
        ReadTime {
            hours,
            minutes,
            seconds,
        }
    }

    #[test]
    fn characters_matches_oracle() {
        assert_eq!(count("").characters, 0, "vacio");
        assert_eq!(
            count(" ").characters,
            1,
            "un espacio (cuenta: precede a EOF)"
        );
        assert_eq!(count("\n\n\n").characters, 0, "solo newlines");
        assert_eq!(count("hello").characters, 5, "palabra simple");
        assert_eq!(
            count("hello world").characters,
            11,
            "espacio interno cuenta"
        );
        assert_eq!(count("a b").characters, 3);
        assert_eq!(count("a  b").characters, 3, "espacios multiples colapsan");
        assert_eq!(count("a\tb").characters, 3, "tab interno cuenta");
        assert_eq!(
            count("hello ").characters,
            6,
            "espacio final cuenta (precede a EOF)"
        );
        assert_eq!(count(" hello").characters, 6, "espacio inicial cuenta");
        assert_eq!(count("Hello. World.").characters, 13);
        assert_eq!(
            count("a\u{3000}b").characters,
            3,
            "espacio ideografico U+3000 es ws"
        );
        assert_eq!(count("a\u{a0}b").characters, 3, "NBSP U+00A0 es ws");
        assert_eq!(count("a\r\nb").characters, 2, "CR seguido de LF no cuenta");
        assert_eq!(
            count("a\rb").characters,
            3,
            "CR solo cuenta como ws interno"
        );
        assert_eq!(
            count("text\n\n\n").characters,
            4,
            "trailing newlines no cuentan"
        );
    }

    #[test]
    fn words_matches_oracle() {
        assert_eq!(count("").words, 0);
        assert_eq!(count(" ").words, 0);
        assert_eq!(count("hello").words, 1);
        assert_eq!(count("hello world").words, 2);
        assert_eq!(count("日本").words, 2, "CJK: cada code point = 1 palabra");
        assert_eq!(count("私は学生です").words, 6);
        assert_eq!(count("한국어").words, 3, "Hangul");
        assert_eq!(count("あいう").words, 3, "Hiragana");
        assert_eq!(
            count("。").words,
            0,
            "puntuacion CJK U+3002 esta debajo del rango"
        );
        assert_eq!(count("あ。い").words, 2, "el punto CJK no es palabra");
        assert_eq!(count("a日").words, 1, "asimetria: latino+CJK se unen");
        assert_eq!(count("日a").words, 2, "asimetria: CJK separa, luego latino");
        assert_eq!(
            count("rock'n'roll").words,
            2,
            "un solo \\S intermedio por iteracion"
        );
        assert_eq!(count("a-b-c").words, 2);
        assert_eq!(count("don't").words, 1);
        assert_eq!(count("well-known").words, 1);
        assert_eq!(count("camión").words, 1, "acento es \\w Unicode");
        assert_eq!(count("naïve").words, 1);
        assert_eq!(count("123").words, 1);
        assert_eq!(count("under_score").words, 1, "_ es \\w");
        assert_eq!(count("hello 😀 world").words, 2, "emoji no es palabra");
        assert_eq!(count("😀").words, 0);
        assert_eq!(
            count("\u{303F}").words,
            0,
            "limite inferior: U+303F NO cuenta"
        );
        assert_eq!(
            count("\u{3040}").words,
            1,
            "limite inferior: U+3040 SI cuenta"
        );
        assert_eq!(count("\u{FFFF}").words, 1, "limite superior: U+FFFF cuenta");
        assert_eq!(
            count("\u{20000}").words,
            1,
            "astral: cuenta via \\w, no via rango"
        );
    }

    #[test]
    fn sentences_matches_oracle() {
        assert_eq!(count("").sentences, 0);
        assert_eq!(count("hello").sentences, 0, "sin terminador");
        assert_eq!(count("Hello. World.").sentences, 2);
        assert_eq!(
            count("line one\nline two").sentences,
            1,
            "\\n es terminador"
        );
        assert_eq!(count("A\n\nB\n\nC").sentences, 2);
        assert_eq!(count("Para one.\n\nPara two.").sentences, 2);
        assert_eq!(count(".").sentences, 0, "exige prefijo [^\\n]");
        assert_eq!(count("..").sentences, 1);
        assert_eq!(count("...").sentences, 1);
        assert_eq!(count("!!!").sentences, 1);
        assert_eq!(count("あ。い").sentences, 1, "punto CJK es terminador");
        assert_eq!(count("text\n\n\n").sentences, 1);
        assert_eq!(count("a\r\nb").sentences, 1, "\\n cierra oracion");
        assert_eq!(count("a\rb").sentences, 0);
    }

    #[test]
    fn paragraphs_matches_oracle() {
        assert_eq!(count("").paragraphs, 0);
        assert_eq!(count(" ").paragraphs, 1, "un espacio es contenido");
        assert_eq!(
            count("\n\n\n").paragraphs,
            0,
            "solo newlines, sin contenido"
        );
        assert_eq!(count("hello").paragraphs, 1);
        assert_eq!(
            count("A\n\nB\n\nC").paragraphs,
            3,
            "separados por linea en blanco"
        );
        assert_eq!(count("Para one.\n\nPara two.").paragraphs, 2);
        assert_eq!(
            count("line one\nline two").paragraphs,
            1,
            "\\n simple no separa parrafo"
        );
        assert_eq!(count(".").paragraphs, 1);
        assert_eq!(
            count("text\n\n\n").paragraphs,
            1,
            "trailing newlines no abren parrafo"
        );
        assert_eq!(count("single line no newline").paragraphs, 1);
    }

    #[test]
    fn read_time_matches_oracle() {
        assert_eq!(read_time(0), rt(0, 0, 0));
        assert_eq!(read_time(1), rt(0, 0, 0), "trunca: 0.3s -> 0");
        assert_eq!(read_time(50), rt(0, 0, 15));
        assert_eq!(read_time(100), rt(0, 0, 30));
        assert_eq!(read_time(150), rt(0, 0, 45));
        assert_eq!(
            read_time(199),
            rt(0, 0, 59),
            "trunca 59.7 -> 59, NO redondea a 60"
        );
        assert_eq!(read_time(200), rt(0, 1, 0));
        assert_eq!(read_time(201), rt(0, 1, 0));
        assert_eq!(read_time(400), rt(0, 2, 0));
        assert_eq!(read_time(600), rt(0, 3, 0));
        assert_eq!(read_time(12000), rt(1, 0, 0));
        assert_eq!(read_time(25000), rt(2, 5, 0));
    }

    #[test]
    fn full_stats_integration() {
        // "Para one.\n\nPara two." -> 18 chars, 4 words, 2 sentences, 2 paragraphs.
        let s = count("Para one.\n\nPara two.");
        assert_eq!(
            s,
            Stats {
                characters: 18,
                words: 4,
                sentences: 2,
                paragraphs: 2,
                read_time: rt(0, 0, 1), // 4 palabras -> 4/200*60 = 1.2s -> trunc 1s
            }
        );
    }
}
