// Modo zen: mientras escribes, el chrome se retira. El dock baja y se desvanece, el esquema
// se desvanece y de la fila de pestañas solo queda tenue la del documento activo. Basta con
// mover el mouse para que todo vuelva.
//
// Es automático y distinto del modo enfoque (writing-modes.ts), que es una decisión explícita
// (Ctrl+D) y además cambia la vista. Aquí solo se pone o se quita la clase body.zen; lo que se
// oculta y cómo lo decide styles.css.
import { EditorView } from "@codemirror/view";
import { view } from "./editor";

// Distancia que tiene que recorrer el mouse, en un solo gesto, para sacar el modo: el temblor
// de la mano o el roce del touchpad mientras se escribe no deben traer el chrome de vuelta.
const MOVE_THRESHOLD = 12;
// Pausa que separa un gesto del siguiente: los movimientos sueltos no se acumulan.
const GESTURE_GAP_MS = 400;

// Atajos que se usan escribiendo y no deben sacar del modo (formato, deshacer, portapapeles,
// guardar). Cualquier otro Ctrl+… es una orden sobre la ventana (cambiar de vista o de
// pestaña, abrir el esquema) y necesita ver el chrome.
const WRITING_SHORTCUTS = new Set(["b", "i", "u", "z", "y", "c", "v", "x", "a", "s", "1", "2", "3", "4"]);

// Las modificadoras solas no dicen nada todavía: Ctrl llega como keydown propio (con
// ctrlKey) antes de la B de Ctrl+B.
const MODIFIERS = new Set(["Control", "Shift", "Alt", "AltGraph", "Meta", "OS", "CapsLock"]);

let active = false;
let travelled = 0;
let lastMove = 0;

function setZen(on: boolean): void {
  if (on === active) return;
  active = on;
  travelled = 0;
  document.body.classList.toggle("zen", on);
  // Al volver, el dock recupera su espacio y el editor se achica: la línea que se estaba
  // escribiendo podría quedar debajo del borde. Se la mantiene a la vista.
  if (!on && view.scrollDOM.clientHeight > 0 && view.hasFocus) {
    requestAnimationFrame(() =>
      view.dispatch({
        effects: EditorView.scrollIntoView(view.state.selection.main.head, { y: "nearest", yMargin: 24 }),
      }),
    );
  }
}

// AltGr: en Windows llega como Ctrl+Alt. Con él se escriben [ ] { } @ \ | en los teclados
// en español, así que es escribir, no un atajo.
function isAltGr(e: KeyboardEvent): boolean {
  return e.getModifierState("AltGraph") || (e.ctrlKey && e.altKey);
}

// Teclas que escriben o borran texto. Las de movimiento (flechas, Inicio…) no: quien se
// desplaza por el documento probablemente está leyendo.
function writes(e: KeyboardEvent): boolean {
  if (isAltGr(e)) return e.key.length === 1;
  if (e.ctrlKey || e.metaKey || e.altKey) return false;
  return e.key.length === 1 || e.key === "Enter" || e.key === "Backspace" || e.key === "Delete" || e.key === "Tab";
}

function onKey(e: KeyboardEvent): void {
  if (!view || MODIFIERS.has(e.key)) return;
  if (writes(e)) {
    if (view.contentDOM.contains(e.target as Node)) setZen(true);
  } else if ((e.ctrlKey || e.metaKey) && !isAltGr(e) && !WRITING_SHORTCUTS.has(e.key.toLowerCase())) {
    setZen(false);
  }
}

// Captura en `window`, registrada al cargar el módulo (antes que shortcuts.ts y los demás
// atajos globales): esos cortan la propagación de lo que atienden (Ctrl+P, Ctrl+Tab…), y un
// oyente posterior nunca vería esas teclas.
window.addEventListener("keydown", onKey, true);

export function initZen(): void {
  // En modo zen la vista previa no recibe el mouse (styles.css): así el movimiento sobre ella
  // también llega aquí, porque el iframe no reenvía sus mousemove a la ventana.
  window.addEventListener(
    "mousemove",
    (e) => {
      if (!active) return;
      if (e.timeStamp - lastMove > GESTURE_GAP_MS) travelled = 0;
      lastMove = e.timeStamp;
      travelled += Math.abs(e.movementX) + Math.abs(e.movementY);
      if (travelled >= MOVE_THRESHOLD) setZen(false);
    },
    { passive: true },
  );
  // La rueda es leer, no escribir. Sobre la vista previa además es la única forma de que la
  // rueda vuelva a llegarle (en zen no recibe el mouse).
  window.addEventListener("wheel", () => setZen(false), { passive: true });
  // Fuera del editor (el foco en una pestaña, en el dock o en otra ventana) el modo no tiene
  // sentido: lo que se busca es justamente el chrome.
  view.contentDOM.addEventListener("blur", () => setZen(false));
  window.addEventListener("blur", () => setZen(false));
}
