#!/usr/bin/env python3
"""Oraculo de stats: ejecuta los 4 regex REALES de Apostrophe (stats_counter.py,
repo World/apostrophe rama main, commit 75202362) para generar la tabla canonica
input -> (characters, words, sentences, paragraphs) que alimenta los tests Rust.

Por que existe: los valores esperados de los tests del nucleo Rust NO se calculan
a ojo; se OBTIENEN ejecutando el algoritmo original. Esto convierte los tests en
una replica VERIFICADA del comportamiento de Apostrophe, no en una aproximacion.

CLAVE: usa el modulo `regex` de PyPI (import regex as re), igual que el upstream.
El `re` stdlib NO reproduce identicamente los rangos Unicode/CJK.

Alcance: replica los PASOS 1-5 del pipeline (CHARACTERS/WORDS/SENTENCES/PARAGRAPHS
+ read_time). NO incluye el PASO 0 (stripping de markdown), que depende de
markup_regex.py y se portara/testeara junto al modulo de highlighting. Por eso los
casos de aca son TEXTO PLANO (la limpieza de markup es idempotente sobre texto sin
sintaxis markdown).
"""

import regex as re  # PyPI `regex`, NO stdlib `re` — igual que Apostrophe

# --- Los 4 regex LITERALES de apostrophe/stats_counter.py (commit 75202362) ---
CHARACTERS = re.compile(r"[^\s]|(?:[^\S\n](?!\s))")
WORDS = re.compile(r"[぀-￿]|(?:\w+\S?\w*)+", re.UNICODE)
SENTENCES = re.compile(r"[^\n][.。।෴۔።?՞;⸮؟？፧꘏⳺⳻⁇﹖⁈⁉‽!﹗！՜߹႟᥄\n]+")
PARAGRAPHS = re.compile(r"(?:[^\n]+\n?)+(?=\n{2,}|\Z)")


def read_time(word_count):
    """Replica literal de stats_counter.py lineas 101-103 (200 wpm, trunca con int)."""
    read_m, read_s = divmod(word_count / 200 * 60, 60)
    read_h, read_m = divmod(read_m, 60)
    return (int(read_h), int(read_m), int(read_s))


def count(text):
    """Pasos 1-4 sobre texto plano (sin stripping de markup). Devuelve la 4-tupla
    de conteos; read_time se prueba aparte porque depende solo de word_count."""
    characters = len(re.findall(CHARACTERS, text))
    words = len(re.findall(WORDS, text))
    sentences = len(re.findall(SENTENCES, text))
    paragraphs = len(re.findall(PARAGRAPHS, text))
    return characters, words, sentences, paragraphs


# Casos de conteo: cada entrada es (etiqueta, texto). El oraculo imprime el
# resultado real para transcribir como assert en Rust.
COUNT_CASES = [
    ("vacio", ""),
    ("un_espacio", " "),
    ("solo_newlines", "\n\n\n"),
    ("palabra_simple", "hello"),
    ("dos_palabras", "hello world"),
    ("a_b", "a b"),
    ("a_doble_espacio_b", "a  b"),
    ("a_tab_b", "a\tb"),
    ("trailing_space", "hello "),
    ("leading_space", " hello"),
    ("dos_oraciones", "Hello. World."),
    ("newline_separa_oracion", "line one\nline two"),
    ("tres_parrafos", "A\n\nB\n\nC"),
    ("dos_parrafos_con_punto", "Para one.\n\nPara two."),
    ("punto_solo", "."),
    ("dos_puntos", ".."),
    ("tres_puntos", "..."),
    ("tres_exclamaciones", "!!!"),
    ("cjk_nihon", "日本"),
    ("cjk_frase", "私は学生です"),
    ("hangul", "한국어"),
    ("hiragana", "あいう"),
    ("punto_cjk_solo", "。"),
    ("cjk_con_punto", "あ。い"),
    ("latino_pegado_a_cjk", "a日"),
    ("cjk_pegado_a_latino", "日a"),
    ("rock_n_roll", "rock'n'roll"),
    ("a_b_c_guiones", "a-b-c"),
    ("dont", "don't"),
    ("well_known", "well-known"),
    ("acento_camion", "camión"),
    ("acento_naive", "naïve"),
    ("numero", "123"),
    ("under_score", "under_score"),
    ("emoji_entre_palabras", "hello 😀 world"),
    ("emoji_solo", "😀"),
    ("espacio_ideografico", "a　b"),
    ("nbsp", "a b"),
    ("crlf", "a\r\nb"),
    ("cr_solo", "a\rb"),
    ("limite_inferior_303F", "〿"),
    ("limite_inferior_3040", "぀"),
    ("limite_superior_FFFF", "￿"),
    ("astral_20000", "\U00020000"),
    ("parrafo_una_linea", "single line no newline"),
    ("parrafo_trailing_newlines", "text\n\n\n"),
]

READ_TIME_CASES = [0, 1, 50, 100, 150, 199, 200, 201, 400, 600, 12000, 25000]


def py_str_literal(s):
    """Representa el string como literal escapado, para transcribir a Rust sin
    sorpresas con chars no imprimibles."""
    return repr(s)


if __name__ == "__main__":
    print(f"# regex module version: {re.__version__}")
    print("# ===== COUNT CASES: (characters, words, sentences, paragraphs) =====")
    for label, text in COUNT_CASES:
        c, w, s, p = count(text)
        print(f"{label:32} {py_str_literal(text):28} -> chars={c:<3} words={w:<3} sentences={s:<3} paragraphs={p:<3}")
    print("# ===== READ_TIME CASES: words -> (h, m, s) =====")
    for n in READ_TIME_CASES:
        print(f"read_time({n:6}) -> {read_time(n)}")
