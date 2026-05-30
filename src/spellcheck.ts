// Spellcheck en CodeMirror 6 (M6). Las ondulitas y las sugerencias se PINTAN/MUESTRAN
// aca, pero los spans (que palabra esta mal, donde, y que sugerir) los calcula el
// NUCLEO RUST (apostrophe_core::spellcheck) y llegan por IPC. El front NO re-parsea.
//
// OFFSETS: Rust emite `from`/`to` en CHAR INDEX (code point). Para el BMP (espanol
// completo) eso === code unit UTF-16 === posicion CM6, asi que se usan directo. (Solo
// divergeria con emojis astral, fuera del MVP — ver nota de offsets del backend.)

import { Decoration, EditorView } from "@codemirror/view";
import type { DecorationSet } from "@codemirror/view";
import { RangeSetBuilder, StateEffect, StateField } from "@codemirror/state";
import type { Extension } from "@codemirror/state";
import { invoke } from "@tauri-apps/api/core";

export interface Misspelling {
  word: string;
  from: number;
  to: number;
  suggestions: string[];
}

// Efecto para reemplazar el set de errores con los recien llegados del IPC.
const setSpellSpans = StateEffect.define<Misspelling[]>();

const spellMark = Decoration.mark({ class: "cm-spell-error" });

function buildDecorations(spans: Misspelling[]): DecorationSet {
  // RangeSetBuilder exige orden ascendente por `from` y rangos validos.
  const sorted = spans.filter((s) => s.from < s.to).sort((a, b) => a.from - b.from);
  const builder = new RangeSetBuilder<Decoration>();
  for (const s of sorted) builder.add(s.from, s.to, spellMark);
  return builder.finish();
}

// Fuente de verdad: el StateField guarda los Misspelling MAPEADOS a traves de las
// ediciones (asi las ondulitas no "saltan" hasta que llega el proximo IPC). Las
// decoraciones se derivan de el; el menu contextual tambien lo lee (las sugerencias
// viajan en el span, sin un segundo viaje IPC).
const spellField = StateField.define<Misspelling[]>({
  create() {
    return [];
  },
  update(spans, tr) {
    let next = spans;
    if (tr.docChanged) {
      next = spans
        .map((s) => ({
          ...s,
          from: tr.changes.mapPos(s.from),
          to: tr.changes.mapPos(s.to, 1),
        }))
        .filter((s) => s.from < s.to);
    }
    for (const e of tr.effects) {
      if (e.is(setSpellSpans)) next = e.value;
    }
    return next;
  },
  provide: (f) => EditorView.decorations.from(f, (spans) => buildDecorations(spans)),
});

// --- Menu contextual de sugerencias (WebView2 no deja controlar el menu nativo por
// eventos del documento, asi que es un div HTML custom). ---
let activeMenu: HTMLElement | null = null;

function closeSpellMenu(): void {
  if (activeMenu) {
    activeMenu.remove();
    activeMenu = null;
    document.removeEventListener("mousedown", onDocMouseDown, true);
    document.removeEventListener("keydown", onMenuKey, true);
  }
}

function onDocMouseDown(e: MouseEvent): void {
  if (activeMenu && !activeMenu.contains(e.target as Node)) closeSpellMenu();
}
function onMenuKey(e: KeyboardEvent): void {
  if (e.key === "Escape") closeSpellMenu();
}

function spanAt(view: EditorView, pos: number): Misspelling | undefined {
  return view.state.field(spellField).find((s) => pos >= s.from && pos <= s.to);
}

function showSpellMenu(view: EditorView, span: Misspelling, x: number, y: number): void {
  closeSpellMenu();
  const menu = document.createElement("div");
  menu.className = "spell-menu";
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
  for (const suggestion of span.suggestions) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "spell-item";
    item.textContent = suggestion;
    // mousedown + preventDefault: no perder el foco/seleccion del editor antes del dispatch.
    item.addEventListener("mousedown", (e) => {
      e.preventDefault();
      view.dispatch({ changes: { from: span.from, to: span.to, insert: suggestion } });
      closeSpellMenu();
      view.focus();
    });
    menu.appendChild(item);
  }
  document.body.appendChild(menu);
  activeMenu = menu;
  // En captura y diferido para no atrapar el mismo gesto que abrio el menu.
  setTimeout(() => {
    document.addEventListener("mousedown", onDocMouseDown, true);
    document.addEventListener("keydown", onMenuKey, true);
  });
}

const spellContextMenu = EditorView.domEventHandlers({
  contextmenu(event, view) {
    const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
    if (pos == null) return false;
    const span = spanAt(view, pos);
    if (!span || span.suggestions.length === 0) return false; // deja pasar el menu nativo
    event.preventDefault();
    showSpellMenu(view, span, event.clientX, event.clientY);
    return true;
  },
});

/// Extension a montar en el EditorView.
export const spellcheckExtension: Extension = [spellField, spellContextMenu];

/// Pide el chequeo al backend y aplica los spans. `focusMode` lo corto-circuita en Rust.
/// Spellcheck NO es critico: si falla (dict no cargado aun) se ignora en silencio.
export async function requestSpellCheck(
  view: EditorView,
  text: string,
  focusMode: boolean,
): Promise<void> {
  try {
    const spans = await invoke<Misspelling[]>("spellcheck", { text, focusMode });
    view.dispatch({ effects: setSpellSpans.of(spans) });
  } catch {
    /* no critico */
  }
}

/// Limpia las ondulitas (p.ej. al entrar a focus mode).
export function clearSpellSpans(view: EditorView): void {
  view.dispatch({ effects: setSpellSpans.of([]) });
}
