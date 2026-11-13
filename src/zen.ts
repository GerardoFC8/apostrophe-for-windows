// Modo zen: mientras escribes, el chrome se retira. El dock baja y se desvanece, el esquema
// se pliega y de la fila de pestañas solo queda tenue la del documento activo. Basta con
// mover el mouse para que todo vuelva.
//
// Es automático y distinto del modo enfoque (writing-modes.ts), que es una decisión explícita
// (Ctrl+D) y además cambia la vista. Aquí solo se pone o se quita la clase body.zen; lo que se
// oculta y cómo lo decide styles.css.
import { view } from "./editor";

// Distancia que tiene que recorrer el mouse para sacar el modo: el temblor de la mano o el
// roce del touchpad mientras se escribe no deben traer el chrome de vuelta.
const MOVE_THRESHOLD = 12;

// Atajos que se usan escribiendo y no deben sacar del modo (formato, deshacer, portapapeles,
// guardar). Cualquier otro Ctrl+… es una orden sobre la ventana (cambiar de vista o de
// pestaña, abrir el esquema) y necesita ver el chrome.
const WRITING_SHORTCUTS = new Set(["b", "i", "u", "z", "y", "c", "v", "x", "a", "s", "1", "2", "3", "4"]);

let active = false;
let travelled = 0;

function setZen(on: boolean): void {
  if (on === active) return;
  active = on;
  travelled = 0;
  document.body.classList.toggle("zen", on);
}

// Teclas que escriben o borran texto. Las de movimiento (flechas, Inicio…) no: quien se
// desplaza por el documento probablemente está leyendo.
function writes(e: KeyboardEvent): boolean {
  if (e.ctrlKey || e.metaKey || e.altKey) return false;
  return e.key.length === 1 || e.key === "Enter" || e.key === "Backspace" || e.key === "Delete" || e.key === "Tab";
}

export function initZen(): void {
  view.contentDOM.addEventListener("keydown", (e) => {
    if (writes(e)) setZen(true);
    else if ((e.ctrlKey || e.metaKey) && !WRITING_SHORTCUTS.has(e.key.toLowerCase())) setZen(false);
  });
  window.addEventListener(
    "mousemove",
    (e) => {
      if (!active) return;
      travelled += Math.abs(e.movementX) + Math.abs(e.movementY);
      if (travelled >= MOVE_THRESHOLD) setZen(false);
    },
    { passive: true },
  );
  // Fuera del editor (el foco en una pestaña, en el dock o en otra ventana) el modo no tiene
  // sentido: lo que se busca es justamente el chrome.
  view.contentDOM.addEventListener("blur", () => setZen(false));
  window.addEventListener("blur", () => setZen(false));
}
