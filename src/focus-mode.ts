// Modo enfoque (M9.1): atenua el texto fuera del PARRAFO del cursor. Presentacion pura
// (mismo patron que spellcheck.ts: un StateField provee las decoraciones; la WebView solo
// PRESENTA). El upstream GNOME atenua por ORACION (apostrophe/text_view_markup_handler.py,
// tag 'unfocused_text' con foreground='gray'); aca la granularidad es PARRAFO por decision
// de producto: mas calmo, no "salta" al mover el cursor dentro del mismo bloque. Usamos
// opacidad CSS (.cm-dim) en vez de gris fijo para que funcione en los tres temas.

import { Decoration, EditorView } from "@codemirror/view";
import type { DecorationSet } from "@codemirror/view";
import { Compartment, RangeSetBuilder, StateField } from "@codemirror/state";
import type { EditorState, Extension } from "@codemirror/state";

const dimMark = Decoration.mark({ class: "cm-dim" });

// Limites del PARRAFO actual: el bloque de lineas contiguas NO vacias alrededor del
// cursor. Si el cursor esta sobre una linea en blanco (entre parrafos) no hay parrafo que
// enfocar -> rango enfocado vacio (se atenua todo el texto). Devuelve offsets absolutos.
// Tope de escaneo: en un documento "patologico" sin lineas en blanco (p.ej. una oracion por
// linea en cientos de lineas) los bucles recorrerian medio doc en CADA tecla/movimiento. El
// tope acota el costo; mas alla de ~MAX lineas el atenuado deja de expandirse (imperceptible:
// ese texto esta fuera de pantalla de todos modos).
const MAX_SCAN_LINES = 400;

function paragraphBounds(state: EditorState): { from: number; to: number } {
  const head = state.selection.main.head;
  const cur = state.doc.lineAt(head);
  if (cur.text.trim() === "") return { from: cur.from, to: cur.from };

  let startLine = cur;
  let steps = 0;
  while (startLine.number > 1 && steps++ < MAX_SCAN_LINES) {
    const prev = state.doc.line(startLine.number - 1);
    if (prev.text.trim() === "") break;
    startLine = prev;
  }
  let endLine = cur;
  steps = 0;
  while (endLine.number < state.doc.lines && steps++ < MAX_SCAN_LINES) {
    const next = state.doc.line(endLine.number + 1);
    if (next.text.trim() === "") break;
    endLine = next;
  }
  return { from: startLine.from, to: endLine.to };
}

// Construye el set de decoraciones: atenua [0..from) y [to..len). RangeSetBuilder exige
// orden ascendente y sin solapes (mismo contrato que buildDecorations en spellcheck.ts).
// Casos borde sin crash: doc vacio o parrafo que abarca todo -> ningun rango -> set vacio.
function buildDim(state: EditorState): DecorationSet {
  const { from, to } = paragraphBounds(state);
  const len = state.doc.length;
  const builder = new RangeSetBuilder<Decoration>();
  if (from > 0) builder.add(0, from, dimMark); // antes del parrafo
  if (to < len) builder.add(to, len, dimMark); // despues del parrafo
  return builder.finish();
}

// StateField que recomputa el atenuado en cada transaccion que cambie el documento O la
// seleccion (movimiento del cursor) — igual que el upstream, que reacciona tanto a editar
// como a mover el cursor. Recomputa desde el estado ACTUAL post-transaccion, asi que NO
// necesita mapPos (a diferencia del spellField, que sigue spans a traves de las ediciones).
const focusDimField = StateField.define<DecorationSet>({
  create: (state) => buildDim(state),
  update(deco, tr) {
    if (tr.docChanged || tr.selection) return buildDim(tr.state);
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const focusDimExtension: Extension = [focusDimField];

// Compartment para activar/desactivar el atenuado en caliente, sin recrear el EditorView
// (mismo mecanismo que usa hemingway.ts para su filtro).
export const focusCompartment = new Compartment();

/** Activa el atenuado por parrafo (reconfigura el compartment con la extension). */
export function enableFocusDim(view: EditorView): void {
  view.dispatch({ effects: focusCompartment.reconfigure(focusDimExtension) });
}

/** Desactiva el atenuado (compartment vacio). */
export function disableFocusDim(view: EditorView): void {
  view.dispatch({ effects: focusCompartment.reconfigure([]) });
}
