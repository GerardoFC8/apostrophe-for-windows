//! Regex de markup de Apostrophe (port 1:1 de `apostrophe/markup_regex.py`, commit
//! upstream 75202362) y el PASO 0 del pipeline de stats: `strip` (limpieza de
//! markdown previa al conteo, = `MARKUP_REGEXP_REPLACE`/`REMOVE` de `do_count`).
//!
//! Fidelidad: los patrones se portan VERBATIM en modo verbose `(?x)` con sus flags
//! originales. Los casos de test se obtuvieron ejecutando los patrones REALES (via
//! `tools/oracle/strip_oracle.py`, que importa `markup_regex.py` tal cual).
//!
//! Motor: `fancy-regex` (backtracking) porque 8 de estos patrones usan lookbehind /
//! lookahead / backreferences que el crate `regex` no soporta. Es el mismo perfil
//! del modulo `regex` de Python del upstream.
//!
//! ALCANCE ACTUAL: solo `strip` (consumidor: stats). El otro consumidor —emitir
//! spans de decoracion para el highlighting de CodeMirror— se construira despues
//! reusando estos mismos patrones.

use std::sync::LazyLock;

use fancy_regex::Regex;

// Helper: compila en modo verbose verbatim. Si un patron no compila, el panic
// (en el primer uso) identifica cual por el mensaje de error.
fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| {
        panic!("patron de markup invalido: {e}\n--- patron ---\n{pattern}\n---")
    })
}

static BOLD_ITALIC: LazyLock<Regex> = LazyLock::new(|| {
    // fancy-regex exige named backrefs si hay grupos nombrados: los grupos de
    // delimitador (orig. 2-5) pasan a nombrados (bi_a..bi_d) y \5\4|\3\2 -> \k<...>.
    re(r#"(?x)
    (?<!\\)
    (?:(?P<bi_a>\*\*|__)(?P<bi_b>[*_])|(?P<bi_c>[*_])(?P<bi_d>\*\*|__))
    (?P<text>
        (?:
            [^\s_*\\]
        |
            [^\s*]
            .*?
            [^\\]
        )
    )
    (?:\k<bi_d>\k<bi_c>|\k<bi_b>\k<bi_a>)
    "#)
});

static ITALIC_ASTERISK: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?<![\\*])
    \*
    (?P<text>
        (?:
            [^\s*\\]
        |
            [^\s*]
            .*?
            [^\\*]
        )
    )
    \*
    (?!\*)
    "#)
});

static ITALIC_UNDERSCORE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?<![\\_\p{L}\p{N}])
    _
    (?P<text>
        (?:
            [^\s_\\]
        |
            [^\s_]
            .*?
            [^\\_]
        )
    )
    _
    (?![\p{L}\p{N}_])
    "#)
});

static BOLD: LazyLock<Regex> = LazyLock::new(|| {
    // \1 (backref al delimitador) -> grupo nombrado `delim` + \k<delim>.
    re(r#"(?x)
    (?<!\\)
    (?P<delim>\*\*|__)
    (?P<text>
        (?:
            [^\s_*\\]
        |
            [^\s*]
            .*?
            [^\\]
        )
    )
    \k<delim>
    "#)
});

static STRIKETHROUGH: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?<!\\)
    ~~
    (?P<text>
        (?:
            [^\s_*\\]
        |
            [^\s*]
            .*?
            [^\s\\]
        )
    )
    ~~
    "#)
});

static CODE: LazyLock<Regex> = LazyLock::new(|| {
    // Codigo inline. Port VERBATIM de markup_regex.CODE del upstream. NO esta en `strip`
    // (stats no lo usa), pero el highlight SI lo resalta. Named backref: (?P=ticks) -> \k<ticks>.
    re(r#"(?x)
    (?<!`)
    (?P<ticks>`+)
    (?!`)
    (?P<text>.+?)
    (?<!`)
    \k<ticks>
    (?!`)
    "#)
});

pub(crate) static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    !\[(?P<text>.*?)\]
    \(
        (?P<url>.+?)
        (?:[ ]\"(?P<title>.+)\")?
    \)
    "#)
});

pub(crate) static LINK: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    \[(?P<text>.*?)\]
    \(
        (?P<url>.+?)
        (?:[ ]\"(?P<title>.+)\")?
    \)
    "#)
});

pub(crate) static LINK_ALT: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    <(?P<text>(?P<url>((https?|ftp):[^'">\s]+)))>
    "#)
});

static LIST: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?:^|\n)
    (?P<content>
        (?P<indent>(?:\t|[ ]{2})*)
        (?P<symbol>(?:[\-*+]))
        [ ](?!\[[xX ]\])
        (?:\t|[ ]{2})*
        (?P<text>
            .+
        )?
    )
    "#)
});

static ORDERED_LIST: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?:^|\n)
    (?P<content>
        (?P<indent>(?>\t|[ ]{2})*)
        (?P<prefix>
            (?:
                (?P<number>\d+)
            |
                [a-z]+
            )
            (?P<delimiter>[.)])
        )
        [ \t]*
        (?P<text>
            .+
        )?
    )
    "#)
});

static BLOCK_QUOTE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?xm)
    ^[ ]{0,3}(?:>[ ]?)+(?P<text>.+)
    "#)
});

static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?xm)
    ^
    (?P<level>\#{1,6})[ ]
    (?P<text>[^\n]+)
    "#)
});

static HEADER_UNDER: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?xm)
    ^
    (?P<text>.+)[ \t]*
    \n
    [=-]+[ \t]*
    (?:\n)
    "#)
});

static CODE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?xms)
    ^[ ]{0,3}
    (?P<block>
        (?P<fence>[`~]{3})
        (?P<text>.+?)
        (?<![ ])[ ]{0,3}
        \k<fence>
    )
    (?:\s+?$|$)
    "#)
});

// TABLE no es verbose en el upstream (solo DOTALL): el patron es compacto.
static TABLE: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)^[\-+]{5,}\n(?P<text>.+?)\n[\-+]{5,}\n"));

pub(crate) static MATH: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?xms)
    (?P<delim>[$]{1,2})
    (?P<text>
        [^`\\ ]{1,2}
        |
        [^` ].+?[^`\\ ])
    \k<delim>
    "#)
});

pub(crate) static FOOTNOTE_ID: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?P<text>[^\s]+)
    \[\^(?P<id>[^\s]+)\]
    "#)
});

pub(crate) static FOOTNOTE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?xm)
    [ ]{0,3}
    \[\^(?P<id>[^\s]+)\]:
    [ ]{0,7}
    (?P<text>
        (?:
            [^\n]+
        |
            \n+(?=(?:\t|[ ]{4}))
        )+
    )
    (?:\n+|$)
    "#)
});

static HORIZONTAL_RULE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?:^|\n{2,})[ ]{0,3}
    (?P<symbols>
        (-[ ]{0,2}){3,} |
        (_[ ]{0,2}){3,} |
        (\*[ ]{0,2}){3,}
    )
    [ \t]*
    (?:\n{2,}|$)
    "#)
});

static FRONTMATTER: LazyLock<Regex> = LazyLock::new(|| {
    // Bloque YAML frontmatter al inicio del doc. Port VERBATIM de markup_regex.FRONTMATTER
    // (DOTALL -> (?s)). Se usa para descartar los matches setext que caen dentro del bloque
    // (el `---` de cierre del frontmatter parece un subrayado setext valido).
    re(r"(?s)^(?:---)\n(?P<text>.+?)\n(?:---|\.{3})")
});

/// Regex cuyo match se REEMPLAZA por su grupo `text`, en el orden EXACTO de
/// `MARKUP_REGEXP_REPLACE` (stats_counter.py). El orden importa y la substitucion
/// es secuencial (cada paso muta el texto para el siguiente).
static REPLACE: LazyLock<[&'static Regex; 18]> = LazyLock::new(|| {
    [
        &*BOLD_ITALIC,
        &*ITALIC_ASTERISK,
        &*ITALIC_UNDERSCORE,
        &*BOLD,
        &*STRIKETHROUGH,
        &*IMAGE,
        &*LINK,
        &*LINK_ALT,
        &*LIST,
        &*ORDERED_LIST,
        &*BLOCK_QUOTE,
        &*HEADER,
        &*HEADER_UNDER,
        &*CODE_BLOCK,
        &*TABLE,
        &*MATH,
        &*FOOTNOTE_ID,
        &*FOOTNOTE,
    ]
});

/// PASO 0 del conteo de stats: limpia la sintaxis markdown dejando el texto plano,
/// replicando `StatsCounter.do_count`: reemplaza cada match de `REPLACE` por su
/// grupo `text` (en orden) y luego elimina el `HORIZONTAL_RULE`.
pub fn strip(text: &str) -> String {
    let mut s = text.to_string();
    for re in REPLACE.iter() {
        s = re.replace_all(&s, "${text}").into_owned();
    }
    HORIZONTAL_RULE.replace_all(&s, "").into_owned()
}

// ===================== M10: spans de resaltado (highlighting) =====================
//
// Segundo consumidor de estas regex (ademas de `strip`): emitir SPANS de decoracion para
// el editor (ADR-0001 D1). Port de `MarkupHandler.parse()` del upstream — un SET DISTINTO
// al de `strip`: resalta enfasis/codigo/links/headers, pero NO listas/footnotes/tablas.

/// Tipo de token de resaltado. Serializa a kebab-case (la clase CSS es `.cm-md-<kind>`).
/// Solo lo que el editor del upstream realmente resalta (no el set de `strip`/stats).
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Bold,
    Italic,
    BoldItalic,
    Strikethrough,
    /// Codigo inline y math (el upstream usa el mismo tag para ambos).
    Code,
    CodeBlock,
    /// Marcadores de links/imagenes (`[`, `](url)`, `!`, `<>`), atenuados.
    Marker,
    Blockquote,
    Hr,
    /// El nivel (1..6) viaja en `Span.level`; el frontend escala el tamano segun el nivel.
    Heading,
}

/// Un span de resaltado. `from`/`to` en CHAR INDEX (espeja `Misspelling`), NUNCA byte.
/// Los spans PUEDEN SOLAPARSE (el upstream apila tags de Pango; el frontend los pinta con
/// `Decoration.set`, NO con `RangeSetBuilder`). `level` solo se usa para `Heading`.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub from: usize,
    pub to: usize,
    pub kind: SpanKind,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub level: Option<u8>,
}

impl Span {
    fn new(from: usize, to: usize, kind: SpanKind) -> Self {
        Span {
            from,
            to,
            kind,
            level: None,
        }
    }
    fn heading(from: usize, to: usize, level: u8) -> Self {
        Span {
            from,
            to,
            kind: SpanKind::Heading,
            level: Some(level),
        }
    }
}

/// Mapa byte->char: `map[byte] = char_index`, definido en CADA limite de char (incluido
/// `text.len()`). fancy-regex SIEMPRE devuelve offsets en limites de char, asi que solo se
/// consultan posiciones validas (las del interior de un char multibyte nunca se piden).
fn byte_to_char_map(text: &str) -> Vec<usize> {
    let mut map = vec![0usize; text.len() + 1];
    let mut ci = 0usize;
    for (bi, _) in text.char_indices() {
        map[bi] = ci;
        ci += 1;
    }
    map[text.len()] = ci;
    map
}

/// Emite los spans de resaltado de Markdown sobre el texto ORIGINAL (port de
/// `MarkupHandler.parse`). A diferencia de `strip` (que muta el texto para stats), corre
/// cada regex sobre el texto crudo y emite spans que PUEDEN SOLAPARSE. `from`/`to` en CHAR
/// INDEX: fancy-regex devuelve byte offsets que se convierten con un mapa byte->char.
pub fn parse_markup(text: &str) -> Vec<Span> {
    let b2c = byte_to_char_map(text);
    // Invariante: fancy-regex SIEMPRE devuelve offsets alineados a char, asi que `c` solo
    // recibe limites de char (los bytes interiores de un char multibyte valen 0 en el mapa
    // y nunca se consultan). El debug_assert lo blinda ante aritmetica de offsets futura.
    let c = |b: usize| {
        debug_assert!(
            text.is_char_boundary(b),
            "offset {b} no esta en un limite de char"
        );
        b2c[b]
    };
    let mut out: Vec<Span> = Vec::new();

    // 1) CODE_BLOCK primero: emite su span (grupo `block`) y alimenta el filtro (en BYTES).
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    for caps in CODE_BLOCK.captures_iter(text).flatten() {
        if let Some(m) = caps.name("block") {
            blocks.push((m.start(), m.end()));
            out.push(Span::new(c(m.start()), c(m.end()), SpanKind::CodeBlock));
        }
    }
    // Un match (salvo el propio code block) se descarta si su inicio O su fin cae
    // ESTRICTAMENTE dentro de un code block (paridad con `match_inside_code_blocks`).
    let inside = |s: usize, e: usize| {
        blocks
            .iter()
            .any(|&(bs, be)| (bs < s && s < be) || (bs < e && e < be))
    };

    // 2) Match COMPLETO (incluye los delimitadores), igual que el upstream.
    let full: [(&Regex, SpanKind); 8] = [
        (&ITALIC_ASTERISK, SpanKind::Italic),
        (&ITALIC_UNDERSCORE, SpanKind::Italic),
        (&BOLD, SpanKind::Bold),
        (&BOLD_ITALIC, SpanKind::BoldItalic),
        (&STRIKETHROUGH, SpanKind::Strikethrough),
        (&CODE, SpanKind::Code),
        (&MATH, SpanKind::Code), // el upstream usa el mismo tag para code inline y math
        (&BLOCK_QUOTE, SpanKind::Blockquote),
    ];
    for (re, kind) in full {
        for m in re.find_iter(text).flatten() {
            if inside(m.start(), m.end()) {
                continue;
            }
            out.push(Span::new(c(m.start()), c(m.end()), kind));
        }
    }

    // 3) LINK / IMAGE: solo los MARCADORES (el upstream taggea `[` y `](url)`, no el texto).
    for re in [&*LINK, &*IMAGE] {
        for caps in re.captures_iter(text).flatten() {
            let (Some(whole), Some(t)) = (caps.get(0), caps.name("text")) else {
                continue;
            };
            if inside(whole.start(), whole.end()) {
                continue;
            }
            out.push(Span::new(c(whole.start()), c(t.start()), SpanKind::Marker));
            out.push(Span::new(c(t.end()), c(whole.end()), SpanKind::Marker));
        }
    }
    // LINK_ALT (<url>): la URL desnuda como marcador atenuado (grupo text, sin los <>).
    for caps in LINK_ALT.captures_iter(text).flatten() {
        if let Some(t) = caps.name("text") {
            if !inside(t.start(), t.end()) {
                out.push(Span::new(c(t.start()), c(t.end()), SpanKind::Marker));
            }
        }
    }

    // 4) HORIZONTAL_RULE: el grupo `symbols`.
    for caps in HORIZONTAL_RULE.captures_iter(text).flatten() {
        if let Some(s) = caps.name("symbols") {
            if !inside(s.start(), s.end()) {
                out.push(Span::new(c(s.start()), c(s.end()), SpanKind::Hr));
            }
        }
    }

    // 5) Encabezados. ATX (`# T`): nivel = cantidad de '#'.
    for caps in HEADER.captures_iter(text).flatten() {
        let m = caps.get(0).unwrap();
        if inside(m.start(), m.end()) {
            continue;
        }
        let level = caps
            .name("level")
            .map_or(1, |l| (l.as_str().len().min(6)) as u8);
        out.push(Span::heading(c(m.start()), c(m.end()), level));
    }
    // Setext (`Titulo\n===`): === -> nivel 1, --- -> nivel 2. Se descartan los matches que
    // caen en el frontmatter YAML del inicio (su `---` de cierre parece un subrayado setext).
    // Paridad con el guard del upstream (text_view_markup_handler.py: match.start() <= fm.end()).
    let fm_end = FRONTMATTER.find(text).ok().flatten().map(|m| m.end());
    for caps in HEADER_UNDER.captures_iter(text).flatten() {
        let m = caps.get(0).unwrap();
        if inside(m.start(), m.end()) {
            continue;
        }
        if let Some(fe) = fm_end {
            if m.start() <= fe {
                continue; // dentro del frontmatter YAML
            }
        }
        let underline = text[m.start()..m.end()].split('\n').nth(1).unwrap_or("");
        let level = if underline.trim_start().starts_with('=') {
            1
        } else {
            2
        };
        out.push(Span::heading(c(m.start()), c(m.end()), level));
    }

    // CM6 exige las decoraciones ordenadas por posicion; los solapes se conservan.
    out.sort_by_key(|s| (s.from, s.to));
    out
}

#[cfg(test)]
mod tests {
    //! Casos canonicos de `tools/oracle/strip_oracle.py` (patrones reales de Apostrophe).
    use super::strip;

    #[test]
    fn strip_inline_emphasis() {
        assert_eq!(strip("**hello**"), "hello");
        assert_eq!(strip("*italic*"), "italic");
        assert_eq!(strip("_under_"), "under");
        assert_eq!(strip("***strong em***"), "strong em");
        assert_eq!(strip("~~tachado~~"), "tachado");
    }

    #[test]
    fn strip_keeps_inline_code() {
        // CODE no esta en MARKUP_REGEXP_REPLACE -> el codigo inline se conserva.
        assert_eq!(strip("`code`"), "`code`");
    }

    #[test]
    fn strip_headers() {
        assert_eq!(strip("# Title"), "Title");
        assert_eq!(strip("### Sub heading"), "Sub heading");
    }

    #[test]
    fn strip_links_and_images() {
        assert_eq!(strip("[click](http://x.com)"), "click");
        assert_eq!(strip("![alt](http://x.com/i.png)"), "alt");
        assert_eq!(strip("<https://example.com>"), "https://example.com");
    }

    #[test]
    fn strip_blocks_and_lists() {
        assert_eq!(strip("> una cita"), "una cita");
        assert_eq!(strip("- item de lista"), "item de lista");
        assert_eq!(strip("1. primero"), "primero");
    }

    #[test]
    fn strip_math_footnote_hr() {
        assert_eq!(strip("$E=mc^2$"), "E=mc^2");
        assert_eq!(strip("palabra[^1]"), "palabra");
        // El HORIZONTAL_RULE consume los \n\n que lo rodean.
        assert_eq!(strip("antes\n\n---\n\ndespues"), "antesdespues");
    }

    #[test]
    fn strip_in_sentence_context() {
        assert_eq!(
            strip("Esto es **muy** importante."),
            "Esto es muy importante."
        );
        assert_eq!(
            strip("Un *enfasis* y un **fuerte** y `code`."),
            "Un enfasis y un fuerte y `code`."
        );
    }

    #[test]
    fn strip_full_document() {
        let md = "# Titulo\n\nParrafo con **negrita**, *italica* y [un link](http://x.com).\n\n- item uno\n- item dos\n\n> cita final.";
        // LIST consume el \n inicial -> "item uno" e "item dos" quedan pegados.
        assert_eq!(
            strip(md),
            "Titulo\n\nParrafo con negrita, italica y un link.\nitem unoitem dos\n\ncita final."
        );
    }

    #[test]
    fn strip_idempotent_on_plain_and_escaped() {
        assert_eq!(
            strip("Texto plano sin nada de markdown."),
            "Texto plano sin nada de markdown."
        );
        // Asteriscos escapados NO son enfasis: el texto queda igual.
        assert_eq!(strip("no \\*italica\\* aca"), "no \\*italica\\* aca");
    }
}

#[cfg(test)]
mod highlight_tests {
    //! Tests del parser de spans (M10). Sin UI: texto -> Vec<Span> esperado. Offsets en
    //! CHAR index. El set de highlight NO es el de strip (no resalta listas/footnotes).
    use super::{parse_markup, Span, SpanKind};

    /// Helper: ¿hay un span exacto (from,to,kind) sin nivel?
    fn has(spans: &[Span], from: usize, to: usize, kind: SpanKind) -> bool {
        spans
            .iter()
            .any(|s| s.from == from && s.to == to && s.kind == kind && s.level.is_none())
    }

    #[test]
    fn bold_full_match() {
        assert_eq!(
            parse_markup("**foo**"),
            vec![Span::new(0, 7, SpanKind::Bold)]
        );
    }

    #[test]
    fn bold_in_context() {
        assert!(has(&parse_markup("a **b** c"), 2, 7, SpanKind::Bold));
    }

    #[test]
    fn italic_asterisk_and_underscore() {
        assert_eq!(
            parse_markup("*foo*"),
            vec![Span::new(0, 5, SpanKind::Italic)]
        );
        assert_eq!(
            parse_markup("_foo_"),
            vec![Span::new(0, 5, SpanKind::Italic)]
        );
    }

    #[test]
    fn bold_italic_stacks_overlapping() {
        // "***foo***" matchea BoldItalic (0..9) Y Bold (1..8): el dominio NO deduplica
        // (replica el apilado de tags de Pango). El frontend usa Decoration.set, que admite
        // solapes, NO RangeSetBuilder. Este test blinda esa decision.
        let s = parse_markup("***foo***");
        assert!(s
            .iter()
            .any(|x| x.kind == SpanKind::BoldItalic && x.from == 0 && x.to == 9));
        assert!(s.iter().any(|x| x.kind == SpanKind::Bold));
        assert!(s.len() >= 2, "esperaba spans solapados, hubo {}", s.len());
    }

    #[test]
    fn strikethrough() {
        assert_eq!(
            parse_markup("~~x~~"),
            vec![Span::new(0, 5, SpanKind::Strikethrough)]
        );
    }

    #[test]
    fn inline_code_single_and_double_ticks() {
        assert_eq!(parse_markup("`x`"), vec![Span::new(0, 3, SpanKind::Code)]);
        assert!(has(&parse_markup("``a`b``"), 0, 7, SpanKind::Code));
    }

    #[test]
    fn math_is_code_kind() {
        assert!(has(&parse_markup("$x$"), 0, 3, SpanKind::Code));
        assert!(has(&parse_markup("$$E=mc^2$$"), 0, 10, SpanKind::Code));
    }

    #[test]
    fn code_block_emits_span() {
        assert!(parse_markup("```\ncode\n```")
            .iter()
            .any(|x| x.kind == SpanKind::CodeBlock));
    }

    #[test]
    fn code_block_excludes_inner_markup() {
        // El ** dentro de un fence NO debe emitir Bold (paridad con match_inside_code_blocks).
        let s = parse_markup("```\n**no bold**\n```");
        assert!(s.iter().all(|x| x.kind != SpanKind::Bold));
        assert!(s.iter().any(|x| x.kind == SpanKind::CodeBlock));
    }

    #[test]
    fn link_tags_only_markers() {
        // "[txt](http://x)": marcadores en (0..1) y (4..15); el texto "txt" (1..4) sin span.
        let s = parse_markup("[txt](http://x)");
        assert!(has(&s, 0, 1, SpanKind::Marker));
        assert!(has(&s, 4, 15, SpanKind::Marker));
        assert!(
            !s.iter()
                .any(|x| x.kind == SpanKind::Marker && x.from <= 1 && x.to >= 4),
            "el texto del link no debe llevar marcador"
        );
    }

    #[test]
    fn image_tags_markers() {
        // "![alt](u)": marcadores en (0..2) y (5..9); el "alt" (2..5) sin span propio.
        let s = parse_markup("![alt](u)");
        assert!(has(&s, 0, 2, SpanKind::Marker));
        assert!(has(&s, 5, 9, SpanKind::Marker));
    }

    #[test]
    fn link_alt_marks_url() {
        // "<https://x.com>": la URL (sin <>) como marcador. URL en char 1..14.
        assert!(has(
            &parse_markup("<https://x.com>"),
            1,
            14,
            SpanKind::Marker
        ));
    }

    #[test]
    fn blockquote_full_line() {
        assert!(has(&parse_markup("> cita"), 0, 6, SpanKind::Blockquote));
    }

    #[test]
    fn horizontal_rule() {
        assert!(parse_markup("\n\n---\n\n")
            .iter()
            .any(|x| x.kind == SpanKind::Hr));
    }

    #[test]
    fn heading_levels() {
        assert_eq!(parse_markup("# T"), vec![Span::heading(0, 3, 1)]);
        assert_eq!(parse_markup("### T"), vec![Span::heading(0, 5, 3)]);
        assert_eq!(parse_markup("###### T"), vec![Span::heading(0, 8, 6)]);
    }

    #[test]
    fn heading_underline_setext_levels() {
        // === -> nivel 1; --- -> nivel 2 (rango exacto del match completo).
        assert_eq!(
            parse_markup("Titulo\n======\n"),
            vec![Span::heading(0, 14, 1)]
        );
        assert_eq!(
            parse_markup("Titulo\n------\n"),
            vec![Span::heading(0, 14, 2)]
        );
    }

    #[test]
    fn yaml_frontmatter_not_heading() {
        // El `---` de cierre del frontmatter YAML NO debe resaltarse como encabezado setext
        // (paridad con el guard de FRONTMATTER del upstream). El `# Real` posterior SI.
        let s = parse_markup("---\ntitle: x\nautor: y\n---\n\n# Real\n");
        assert!(
            !s.iter().any(|x| x.kind == SpanKind::Heading && x.from < 26),
            "el frontmatter no debe producir encabezados"
        );
        assert!(s
            .iter()
            .any(|x| x.kind == SpanKind::Heading && x.from >= 26));
    }

    #[test]
    fn char_offsets_not_byte_with_accents() {
        // "camión **fin**": 'ó' son 2 bytes; el Bold empieza en CHAR 7 (no byte 8).
        let s = parse_markup("camión **fin**");
        let b = s
            .iter()
            .find(|x| x.kind == SpanKind::Bold)
            .expect("falta el span Bold");
        assert_eq!((b.from, b.to), (7, 14), "offsets en CHAR index, no byte");
    }

    #[test]
    fn multibyte_content_offsets() {
        // "**camión**": 10 chars (** + camión[6] + **); 'ó' cuenta 1 char aunque sean 2 bytes.
        assert_eq!(
            parse_markup("**camión**"),
            vec![Span::new(0, 10, SpanKind::Bold)]
        );
    }

    #[test]
    fn plain_and_empty_no_spans() {
        assert_eq!(parse_markup("texto sin nada"), vec![]);
        assert_eq!(parse_markup(""), vec![]);
    }

    #[test]
    fn escaped_emphasis_not_emitted() {
        // "\*x\*" no es cursiva (lookbehind de la regex).
        assert!(parse_markup("\\*x\\*")
            .iter()
            .all(|x| x.kind != SpanKind::Italic));
    }

    #[test]
    fn lists_and_footnotes_not_highlighted() {
        // El set de highlight != el de strip: listas y footnotes NO se resaltan.
        assert_eq!(parse_markup("- item de lista"), vec![]);
        assert_eq!(parse_markup("texto[^1]"), vec![]);
    }

    #[test]
    fn spans_sorted_by_position() {
        let s = parse_markup("# T\n\n**b** y *i*");
        for w in s.windows(2) {
            assert!(
                (w[0].from, w[0].to) <= (w[1].from, w[1].to),
                "los spans deben salir ordenados por posicion"
            );
        }
    }
}
