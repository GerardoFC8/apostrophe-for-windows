// Filtro del modo Hemingway (M9.2). Mismo patron que focus-mode.ts: el modulo expone un
// Compartment que el editor monta vacio y el toggle (writing-modes.ts) reconfigura en
// caliente, sin recrear el EditorView.
import { EditorState, Compartment } from "@codemirror/state";
import { flashStatus } from "./status";
import { t } from "./i18n";

// --- M9.2: modo Hemingway (Ctrl+T): no puedes borrar lo que escribiste ---
// Paridad con el upstream (apostrophe/text_buffer.py: do_delete_range bloquea el borrado y
// do_insert_text bloquea escribir sobre una seleccion). Lo replicamos con un changeFilter
// que RECHAZA solo las transacciones del USUARIO que remueven texto existente: borrar/cortar
// ("delete"/"delete.cut"), teclear o pegar sobre una seleccion ("input.type"/"input.paste")
// y arrastrar-para-mover ("move.drop", que borra el origen). Lo programatico (abrir/recargar/
// nuevo, formato Markdown, auto-listas) NO lleva userEvent y pasa; undo/redo ("undo"/"redo")
// tampoco se bloquean (el modo intercepta el borrado del usuario, no la pila de historial);
// y Buscar/Reemplazar ("input.replace") se permite: es una edicion intencional, no un borrado.
// userEvents verificados contra @codemirror/{commands,view,search,state} instalados.
let lastHemingwayNotice = 0; // throttle del aviso (evita que se pegue con el autorepeat)
export const hemingwayCompartment = new Compartment();
export const hemingwayFilter = EditorState.changeFilter.of((tr) => {
  if (!tr.docChanged) return true; // mover el cursor / seleccionar no borra: permitido
  if (tr.isUserEvent("input.replace")) return true; // Reemplazar (M7) es edicion, no borrado
  // Solo se filtra el borrado/movimiento iniciado por el USUARIO.
  if (!(tr.isUserEvent("delete") || tr.isUserEvent("input") || tr.isUserEvent("move.drop"))) {
    return true; // programatico / undo / redo / arrastre-copia (input.drop solo inserta)
  }
  // ¿Remueve texto EXISTENTE? No contamos reemplazos no-op (lo quitado === lo insertado),
  // p.ej. el "skip-over" de un cierre de closeBrackets (teclear ")" sobre un ")" ya puesto).
  let removesText = false;
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    if (toA > fromA && tr.startState.sliceDoc(fromA, toA) !== inserted.toString()) {
      removesText = true;
    }
  });
  if (!removesText) return true; // insercion pura o reemplazo no-op
  // Aviso (throttleado): el filtro decide; el feedback es secundario. No reseteamos el
  // timer en cada tecla del autorepeat para que el mensaje no se quede pegado.
  const now = performance.now();
  if (now - lastHemingwayNotice > 1200) {
    flashStatus(t("status.hemingway"));
    lastHemingwayNotice = now;
  }
  // CM6 descarta los cambios (el texto se conserva); la seleccion del usuario colapsa a
  // cursor (changeFilter no puede reescribir la seleccion) — comportamiento aceptado.
  return false;
});
