// Resaltado de Markdown (M10): pinta los SPANS de decoracion que calcula el NUCLEO RUST
// (apostrophe_core::markup::parse_markup) y llegan por IPC. El front NO re-parsea (ADR-0001
// D1: sin gramatica Lezer). Mismo patron que spellcheck.ts (un StateField que mapea los
// spans a traves de las ediciones y deriva las decoraciones), con UNA diferencia CLAVE: los
// spans de markup PUEDEN SOLAPARSE (negrita dentro de un encabezado, code dentro de texto,
// etc.), asi que se usa Decoration.set(ranges, true) — que admite solapes — y NO
// RangeSetBuilder (que los prohibe, como hacen el spellcheck y el focus mode).
//
// OFFSETS: from/to en CHAR INDEX (code point) === code unit UTF-16 para el BMP (espanol
// completo), igual contrato que el spellcheck. El backend ya los emite en char index.

import { Decoration, EditorView } from "@codemirror/view";
import type { DecorationSet } from "@codemirror/view";
import { MapMode, StateEffect, StateField } from "@codemirror/state";
import type { Extension, Range } from "@codemirror/state";
import { invoke } from "@tauri-apps/api/core";

// Espejo de apostrophe_core::markup::Span (serde). `level` solo viene en kind "heading".
export interface Span {
  from: number;
  to: number;
  kind: string;
  level?: number;
}

// Efecto para reemplazar el set de spans con los recien llegados del IPC.
const setMarkupSpans = StateEffect.define<Span[]>();

// Una Decoration.mark por clase CSS (.cm-md-<kind>). El estilo concreto vive en styles.css
// (deriva de las CSS vars del tema, igual que .cm-spell-error y .cm-dim).
const MARKS: Record<string, Decoration> = {
  bold: Decoration.mark({ class: "cm-md-bold" }),
  italic: Decoration.mark({ class: "cm-md-italic" }),
  "bold-italic": Decoration.mark({ class: "cm-md-bold-italic" }),
  strikethrough: Decoration.mark({ class: "cm-md-strikethrough" }),
  code: Decoration.mark({ class: "cm-md-code" }),
  "code-block": Decoration.mark({ class: "cm-md-code-block" }),
  marker: Decoration.mark({ class: "cm-md-marker" }),
  blockquote: Decoration.mark({ class: "cm-md-blockquote" }),
  hr: Decoration.mark({ class: "cm-md-hr" }),
};

// Encabezados: una mark por nivel (1..6) para escalar el tamano via CSS (cm-md-h1..h6).
const HEADING_MARKS: Record<number, Decoration> = {
  1: Decoration.mark({ class: "cm-md-heading cm-md-h1" }),
  2: Decoration.mark({ class: "cm-md-heading cm-md-h2" }),
  3: Decoration.mark({ class: "cm-md-heading cm-md-h3" }),
  4: Decoration.mark({ class: "cm-md-heading cm-md-h4" }),
  5: Decoration.mark({ class: "cm-md-heading cm-md-h5" }),
  6: Decoration.mark({ class: "cm-md-heading cm-md-h6" }),
};

function markFor(span: Span): Decoration | undefined {
  if (span.kind === "heading") {
    const lvl = span.level && span.level >= 1 && span.level <= 6 ? span.level : 1;
    return HEADING_MARKS[lvl];
  }
  return MARKS[span.kind];
}

// CLAVE: Decoration.set(ranges, true) ADMITE SOLAPES (a diferencia de RangeSetBuilder, que
// usan spellcheck/focus). Los spans vienen ordenados desde Rust, pero tras mapPos pueden
// quedar levemente desordenados; sort=true deja que CM6 los reordene.
function buildDecorations(spans: Span[]): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  for (const s of spans) {
    if (s.from >= s.to) continue; // descarta rangos vacios/invertidos (defensivo)
    const mark = markFor(s);
    if (mark) ranges.push(mark.range(s.from, s.to));
  }
  return ranges.length ? Decoration.set(ranges, true) : Decoration.none;
}

// StateField: guarda los spans MAPEADOS a traves de las ediciones (asi el resaltado no
// "salta" hasta que llega el proximo IPC) y deriva el DecorationSet. Mismo molde que
// spellField, salvo que aqui se permiten solapes.
const markupField = StateField.define<Span[]>({
  create() {
    return [];
  },
  update(spans, tr) {
    let next = spans;
    if (tr.docChanged) {
      // TrackDel: si una edicion BORRO el rango de un span, mapPos devuelve null y lo
      // descartamos. Sin esto, un reemplazo de TODO el documento (abrir otro archivo,
      // recargar de disco, restaurar un snapshot) colapsaria los spans viejos sobre el doc
      // nuevo y lo pintaria entero como decoracion fantasma hasta que llega el resaltado
      // fresco (~250ms). Mapeo defensivo y filtro from<to.
      const mapped: Span[] = [];
      for (const s of spans) {
        const from = tr.changes.mapPos(s.from, -1, MapMode.TrackDel);
        const to = tr.changes.mapPos(s.to, 1, MapMode.TrackDel);
        if (from !== null && to !== null && from < to) {
          mapped.push({ ...s, from, to });
        }
      }
      next = mapped;
    }
    for (const e of tr.effects) {
      if (e.is(setMarkupSpans)) next = e.value;
    }
    return next;
  },
  provide: (f) => EditorView.decorations.from(f, (spans) => buildDecorations(spans)),
});

/// Extension a montar en el EditorView.
export const markupExtension: Extension = [markupField];

/// Pide el resaltado al backend y aplica los spans. NO critico: si falla se ignora en
/// silencio. Si el doc cambio mientras volaba el IPC, descarta el resultado (offsets viejos)
/// — el debounce ya programo uno fresco. Identico al descarte de requestSpellCheck.
export async function requestMarkupHighlight(view: EditorView, text: string): Promise<void> {
  const sentDoc = view.state.doc;
  try {
    const spans = await invoke<Span[]>("parse_markup", { text });
    if (view.state.doc === sentDoc) {
      view.dispatch({ effects: setMarkupSpans.of(spans) });
    }
  } catch {
    /* no critico */
  }
}
