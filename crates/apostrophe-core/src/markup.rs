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

static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    !\[(?P<text>.*?)\]
    \(
        (?P<url>.+?)
        (?:[ ]\"(?P<title>.+)\")?
    \)
    "#)
});

static LINK: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    \[(?P<text>.*?)\]
    \(
        (?P<url>.+?)
        (?:[ ]\"(?P<title>.+)\")?
    \)
    "#)
});

static LINK_ALT: LazyLock<Regex> = LazyLock::new(|| {
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

static MATH: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?xms)
    (?P<delim>[$]{1,2})
    (?P<text>
        [^`\\ ]{1,2}
        |
        [^` ].+?[^`\\ ])
    \k<delim>
    "#)
});

static FOOTNOTE_ID: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?x)
    (?P<text>[^\s]+)
    \[\^(?P<id>[^\s]+)\]
    "#)
});

static FOOTNOTE: LazyLock<Regex> = LazyLock::new(|| {
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
