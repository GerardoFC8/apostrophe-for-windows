#!/usr/bin/env python3
"""Oraculo de stripping de markdown: ejecuta el PASO 0 REAL de Apostrophe
(`do_count` en stats_counter.py) usando los regex REALES de markup_regex.py.

Maxima fidelidad: importa `markup_regex.py` del upstream TAL CUAL (es un modulo
puro: solo `import regex as re` + constantes), sin transcribir patrones. Y reusa
el `count()` del oraculo de stats para emitir los conteos POST-stripping (lo que
debe devolver stats::count una vez integrado el PASO 0).

Pipeline replicado (literal de StatsCounter.do_count):
    for rx in MARKUP_REGEXP_REPLACE: text = re.sub(rx, r"\\g<text>", text)
    for rx in MARKUP_REGEXP_REMOVE:  text = re.sub(rx, "", text)
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))  # tools/oracle -> tools -> repo
sys.path.insert(0, os.path.join(REPO, "docs", "upstream-reference"))
sys.path.insert(0, HERE)

import regex as re  # noqa: E402  (PyPI regex, igual que el upstream)
import markup_regex as mk  # noqa: E402  (patrones REALES del upstream)
import stats_oracle  # noqa: E402  (reusa count() y los 4 regex de stats)

# Orden EXACTO de stats_counter.py. El orden importa: BOLD_ITALIC antes que
# BOLD/ITALIC para no romper ***x***; la substitucion es secuencial.
MARKUP_REGEXP_REPLACE = (
    mk.BOLD_ITALIC, mk.ITALIC_ASTERISK, mk.ITALIC_UNDERSCORE, mk.BOLD,
    mk.STRIKETHROUGH, mk.IMAGE, mk.LINK, mk.LINK_ALT, mk.LIST, mk.ORDERED_LIST,
    mk.BLOCK_QUOTE, mk.HEADER, mk.HEADER_UNDER, mk.CODE_BLOCK, mk.TABLE,
    mk.MATH, mk.FOOTNOTE_ID, mk.FOOTNOTE,
)
MARKUP_REGEXP_REMOVE = (mk.HORIZONTAL_RULE,)


def strip(text):
    """PASO 0 literal: limpia el markup antes de contar."""
    for rx in MARKUP_REGEXP_REPLACE:
        text = re.sub(rx, r"\g<text>", text)
    for rx in MARKUP_REGEXP_REMOVE:
        text = re.sub(rx, "", text)
    return text


# Casos markdown -> texto plano. El oraculo emite el stripped real y, ademas,
# los conteos POST-stripping (lo que stats::count debe dar con el PASO 0).
CASES = [
    ("bold", "**hello**"),
    ("italic_asterisk", "*italic*"),
    ("italic_underscore", "_under_"),
    ("bold_italic", "***strong em***"),
    ("strikethrough", "~~tachado~~"),
    ("inline_code_no_strip", "`code`"),
    ("header_1", "# Title"),
    ("header_3", "### Sub heading"),
    ("link", "[click](http://x.com)"),
    ("image", "![alt](http://x.com/i.png)"),
    ("link_alt", "<https://example.com>"),
    ("blockquote", "> una cita"),
    ("unordered_list", "- item de lista"),
    ("ordered_list", "1. primero"),
    ("math_inline", "$E=mc^2$"),
    ("footnote_ref", "palabra[^1]"),
    ("horizontal_rule", "antes\n\n---\n\ndespues"),
    ("bold_in_sentence", "Esto es **muy** importante."),
    ("mixed_inline", "Un *enfasis* y un **fuerte** y `code`."),
    (
        "doc_completo",
        "# Titulo\n\nParrafo con **negrita**, *italica* y [un link](http://x.com).\n\n"
        "- item uno\n- item dos\n\n> cita final.",
    ),
    ("plano_idempotente", "Texto plano sin nada de markdown."),
    ("escaped_asterisk", "no \\*italica\\* aca"),
]


def py(s):
    return repr(s)


if __name__ == "__main__":
    print(f"# regex module version: {re.__version__}")
    print("# ===== STRIP: markdown -> texto plano =====")
    for label, md in CASES:
        print(f"{label:24} {py(md)}\n{'':24} -> {py(strip(md))}")
    print("# ===== COUNT POST-STRIP: lo que stats::count debe devolver con PASO 0 =====")
    for label, md in CASES:
        stripped = strip(md)
        c, w, s, p = stats_oracle.count(stripped)
        print(f"{label:24} -> chars={c:<3} words={w:<3} sentences={s:<3} paragraphs={p:<3}")
