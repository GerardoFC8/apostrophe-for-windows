// Atajos de teclado a nivel WINDOW (los del editor viven en el keymap de editor.ts). Dos
// handlers: archivos (fase bubble) y modos de escritura / chrome (fase captura).
import { view } from "./editor";
import { newDoc, openDoc, saveDoc, saveDocAs } from "./document-session";
import { exportDoc } from "./commands";
import { closeTopmostChrome, isMenuChromeOpen, openShortcuts, trapTab } from "./menu";
import { isThemeMenuOpen } from "./theme";
import { cycleViewMode } from "./preview";
import { applyFullscreen, toggleFocusMode, toggleHemingway, toggleSpellcheck } from "./writing-modes";

// M4/M5: atajos de archivo a nivel WINDOW (no solo en CM6) — asi funcionan aunque el
// foco NO este en el editor. Shift+S se chequea ANTES que S sola.
export function initFileShortcuts(): void {
  window.addEventListener("keydown", (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (k === "o") {
      e.preventDefault();
      void openDoc();
    } else if (k === "s" && e.shiftKey) {
      e.preventDefault();
      void saveDocAs();
    } else if (k === "s") {
      e.preventDefault();
      void saveDoc();
    } else if (k === "e") {
      e.preventDefault();
      void exportDoc();
    } else if (k === "n") {
      e.preventDefault();
      void newDoc();
    }
  });
}

// ¿Hay algo que Esc deba cerrar ANTES de salir de fullscreen? (paridad con el upstream, que
// no oculta el chrome mientras la busqueda o un aviso esten visibles).
function escHasPriorityTarget(): boolean {
  if (isThemeMenuOpen()) return true; // el menu de tema tiene su propio Esc
  if (isMenuChromeOpen()) return true; // M11: menu hamburguesa u overlay modal abierto
  if (view.dom.querySelector(".cm-search")) return true; // panel de busqueda CM6 abierto
  if (document.querySelector(".spell-menu")) return true; // menu de sugerencias del corrector
  return false;
}

// Atajos de los MODOS DE ESCRITURA a nivel WINDOW en fase CAPTURA. Captura (no bubble como el
// handler de archivos de mas arriba) para ganarle a CodeMirror y a WebView2: los toggles
// andan con o sin foco en el editor y no chocan con ningun binding del editor. En Windows
// Ctrl+D/Ctrl+T no estan tomados por CM6 (sus bindings emacs son solo-Mac), pero capturamos
// igual por robustez y consistencia.
export function initModeShortcuts(): void {
  window.addEventListener(
    "keydown",
    (e) => {
      // M11: Ctrl+? abre la ventana de atajos. El "?" sale de Shift+/, así que e.key da el
      // carácter resultante ("?"). En CAPTURA para ganarle a cualquier binding del editor.
      if ((e.ctrlKey || e.metaKey) && e.key === "?") {
        e.preventDefault();
        e.stopImmediatePropagation();
        openShortcuts();
        return;
      }
      // M11: focus trap del overlay modal. Con un overlay abierto, Tab/Shift+Tab ciclan
      // dentro del panel (no escapan al chrome/editor de atrás), cumpliendo aria-modal=true.
      if (e.key === "Tab" && trapTab(e)) {
        e.stopImmediatePropagation();
        return;
      }
      // M11: Esc cierra PRIMERO el overlay/menú abierto (antes de que actúe el fullscreen).
      // Si hubo algo que cerrar, no seguimos: Esc no debe salir de fullscreen en el mismo
      // pulso (caso borde: overlay abierto Y en fullscreen → solo se cierra el overlay).
      if (e.key === "Escape" && closeTopmostChrome()) {
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }
      // F11: alterna pantalla completa. Ignora el autorepeat (mantener F11 presionado).
      if (e.key === "F11") {
        e.preventDefault();
        if (e.repeat) return;
        void applyFullscreen();
        return;
      }
      // F7: alterna el corrector ortografico (paridad con el upstream Apostrophe). Sin ctrl,
      // en CAPTURA igual que F11 para que funcione con o sin foco en el editor.
      if (e.key === "F7") {
        e.preventDefault();
        e.stopImmediatePropagation();
        toggleSpellcheck();
        return;
      }
      // Esc: sale de fullscreen SOLO si estamos en fullscreen y no hay otro objetivo de Esc
      // (menu de tema / panel de busqueda). Si no, no interceptamos: que actue quien corresponda.
      if (
        e.key === "Escape" &&
        document.body.classList.contains("is-fullscreen") &&
        !escHasPriorityTarget()
      ) {
        e.preventDefault();
        void applyFullscreen(false); // serializado por el mismo guard que F11
        return;
      }
      // Ctrl+D (enfoque) / Ctrl+T (Hemingway), sin Shift ni Alt.
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
        const k = e.key.toLowerCase();
        if (k === "d") {
          e.preventDefault();
          e.stopImmediatePropagation();
          toggleFocusMode();
        } else if (k === "t") {
          e.preventDefault();
          e.stopImmediatePropagation();
          toggleHemingway();
        } else if (k === "p") {
          // M13: Ctrl+P cicla la vista (editor -> dividido -> solo vista). El preventDefault es
          // CRITICO: en WebView2/Windows Ctrl+P abre el dialogo de IMPRESION del navegador; sin
          // esto el ciclo de vista quedaria tapado por el print preview.
          e.preventDefault();
          e.stopImmediatePropagation();
          cycleViewMode();
        }
      }
    },
    true, // fase CAPTURA
  );
}
