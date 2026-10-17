// --- M11: menú hamburguesa + overlays modales (atajos / acerca de) ---
// Frontend puro (ADR-0001): no toca el dominio ni agrega comandos. Reusa el patrón del
// theme-menu (toggle del hidden + aria-expanded, cierre por clic-afuera y Esc).
// Incluye lo que cuelga del menú: recientes (M16), selector de tablas (M17) e imagen.
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
// M11: versión en build-time (sin IPC). tsconfig tiene resolveJsonModule, así que el
// import del package.json es type-safe y lo resuelve Vite al empaquetar.
import pkg from "../package.json";
import { view } from "./editor";
import { openPath } from "./document-session";
import { insertTable, insertImage } from "./commands";
import { escapeHtml } from "./preview";
import { closeThemeMenu } from "./theme";

const menuTrigger = document.querySelector<HTMLButtonElement>("#menu-trigger")!;
const appMenu = document.querySelector<HTMLElement>("#app-menu")!;
const shortcutsOverlay = document.querySelector<HTMLElement>("#shortcuts-overlay")!;
const aboutOverlay = document.querySelector<HTMLElement>("#about-overlay")!;
const themeTrigger = document.querySelector<HTMLButtonElement>("#theme-trigger")!;

const TABLE_PICKER_SIZE = 5; // 5x5 alcanza para casi todo; mas grande se edita a mano

const tableGrid = document.querySelector<HTMLElement>("#table-grid")!;
const tableSize = document.querySelector<HTMLElement>("#table-size")!;

// Grilla de selección: al pasar el mouse se resaltan las celdas de arriba-izquierda hasta
// la actual y la etiqueta muestra el tamaño. Mismo gesto que el selector del upstream.
export function buildTablePicker(): void {
  const cells: string[] = [];
  for (let row = 1; row <= TABLE_PICKER_SIZE; row++) {
    for (let col = 1; col <= TABLE_PICKER_SIZE; col++) {
      cells.push(
        `<button class="table-cell" type="button" data-row="${row}" data-col="${col}" data-on="false" aria-label="${row} × ${col}"></button>`
      );
    }
  }
  tableGrid.innerHTML = cells.join("");

  const paint = (row: number, col: number) => {
    tableGrid.querySelectorAll<HTMLElement>(".table-cell").forEach((cell) => {
      const on = Number(cell.dataset.row) <= row && Number(cell.dataset.col) <= col;
      cell.dataset.on = String(on);
    });
    tableSize.textContent = `${row} × ${col}`;
  };

  tableGrid.querySelectorAll<HTMLButtonElement>(".table-cell").forEach((cell) => {
    const row = Number(cell.dataset.row);
    const col = Number(cell.dataset.col);
    cell.addEventListener("mouseenter", () => paint(row, col));
    cell.addEventListener("click", () => {
      closeAppMenu();
      void insertTable(row, col);
    });
  });
  // Al salir de la grilla se apaga todo: si no, queda un resaltado fantasma al reabrir.
  tableGrid.addEventListener("mouseleave", () => {
    tableGrid.querySelectorAll<HTMLElement>(".table-cell").forEach((c) => (c.dataset.on = "false"));
    tableSize.innerHTML = "&nbsp;";
  });
}

// --- M16: documentos recientes ---

interface RecentEntry {
  path: string;
  name: string;
}

const recentsGroup = document.querySelector<HTMLElement>("#recents-group")!;
const recentItems = document.querySelector<HTMLElement>("#recent-items")!;

// Se refresca cada vez que se ABRE el menu, no una sola vez al arrancar: la lista cambia
// al abrir y al guardar-como, y el backend ya descarta los archivos borrados. Si falla, el
// grupo queda oculto — unos recientes ausentes no son motivo para romper el menu.
async function refreshRecents(): Promise<void> {
  let entries: RecentEntry[] = [];
  try {
    entries = await invoke<RecentEntry[]>("recent_files");
  } catch {
    /* no critico */
  }
  recentsGroup.hidden = entries.length === 0;
  recentItems.innerHTML = entries
    .map(
      (entry) =>
        `<button class="menu-item recent-item" type="button" role="menuitem" data-path="${escapeHtml(entry.path)}" title="${escapeHtml(entry.path)}">` +
        `<span class="recent-name">${escapeHtml(entry.name)}</span></button>`
    )
    .join("");
  recentItems.querySelectorAll<HTMLButtonElement>(".recent-item").forEach((item) => {
    item.addEventListener("click", () => {
      closeAppMenu();
      void openPath(item.dataset.path!);
    });
  });
}

function openAppMenu(): void {
  closeThemeMenu(); // solo un popover abierto a la vez
  appMenu.hidden = false;
  menuTrigger.setAttribute("aria-expanded", "true");
  void refreshRecents(); // M16: la lista se rearma en cada apertura (ver refreshRecents)
}
export function closeAppMenu(): void {
  appMenu.hidden = true;
  menuTrigger.setAttribute("aria-expanded", "false");
}

// Overlays modales. openX cierra el app-menu y mueve el foco al botón × (accesible).
//
// Accesibilidad de modal (paridad con el patrón ARIA dialog): los paneles declaran
// aria-modal=true, así que cumplimos las dos promesas que implica:
//  1) al CERRAR, devolvemos el foco al elemento que abrió el overlay (lastFocused), para
//     no dejarlo caer al <body> y romper el orden de tabulación del usuario de teclado;
//  2) mientras está abierto, atrapamos Tab dentro del panel (trapTab) para que el foco no
//     escape a los controles del chrome/editor que quedan detrás del backdrop.
// El elemento a restaurar se captura del foco vivo al abrir; en el caso Ctrl+? el foco
// estaba en el editor, así que se restaura allí solo (view.focus() vía el fallback de abajo).
let lastFocused: HTMLElement | null = null;

// Captura el disparador a restaurar ANTES de cerrar el app-menu. Importa el orden: cerrar
// el menú lo oculta con [hidden] y el foco caería al <body> (que no es un objetivo útil);
// por eso leemos document.activeElement primero. Si el disparador es un item del menú (que
// quedará oculto), guardamos el trigger hamburguesa, que sigue visible y enfocable. Para
// Ctrl+? el foco está en el editor: se guarda el contenteditable de CM6 y se restaura allí.
function captureOverlayTrigger(): void {
  const active = document.activeElement as HTMLElement | null;
  lastFocused = active && active.closest(".app-menu") ? menuTrigger : active;
}

// Devuelve el foco al disparador guardado al cerrar el overlay. Si el disparador ya no
// existe, no es enfocable, o era el <body> (sin foco real), reenfoca el editor.
function restoreFocusAfterOverlay(): void {
  const target = lastFocused;
  lastFocused = null;
  if (target && target !== document.body && document.contains(target)) target.focus();
  else view.focus();
}

export function openShortcuts(): void {
  captureOverlayTrigger(); // a quién devolver el foco al cerrar (antes de cerrar el menú)
  closeAppMenu();
  aboutOverlay.hidden = true; // un overlay a la vez
  shortcutsOverlay.hidden = false;
  document.querySelector<HTMLButtonElement>("#shortcuts-close")?.focus();
}
function closeShortcuts(): void {
  shortcutsOverlay.hidden = true;
  restoreFocusAfterOverlay();
}
function openAbout(): void {
  captureOverlayTrigger(); // a quién devolver el foco al cerrar (antes de cerrar el menú)
  closeAppMenu();
  shortcutsOverlay.hidden = true; // un overlay a la vez
  aboutOverlay.hidden = false;
  document.querySelector<HTMLButtonElement>("#about-close")?.focus();
}
function closeAbout(): void {
  aboutOverlay.hidden = true;
  restoreFocusAfterOverlay();
}

// Focus trap del overlay activo: cicla el Tab/Shift+Tab entre los focusables del panel
// para que aria-modal=true sea cierto (el foco no se escapa al chrome/editor de atrás).
// Devuelve true si consumió el evento (había un overlay abierto y trató el Tab).
export function trapTab(e: KeyboardEvent): boolean {
  const overlay = !shortcutsOverlay.hidden
    ? shortcutsOverlay
    : !aboutOverlay.hidden
      ? aboutOverlay
      : null;
  if (!overlay) return false;
  const panel = overlay.querySelector<HTMLElement>(".overlay-panel");
  if (!panel) return false;
  const focusables = panel.querySelectorAll<HTMLElement>(
    'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])',
  );
  if (focusables.length === 0) {
    e.preventDefault(); // sin focusables, no dejes que el Tab salga del panel
    return true;
  }
  const first = focusables[0];
  const last = focusables[focusables.length - 1];
  const active = document.activeElement;
  if (e.shiftKey) {
    if (active === first || !panel.contains(active)) {
      e.preventDefault();
      last.focus();
    }
  } else {
    if (active === last || !panel.contains(active)) {
      e.preventDefault();
      first.focus();
    }
  }
  return true;
}

// ¿Hay un overlay/menú abierto que Esc deba cerrar PRIMERO? Devuelve el cierre a aplicar,
// o null si no hay nada. El orden refleja la prioridad: overlay modal > popover del menú.
export function closeTopmostChrome(): boolean {
  if (!shortcutsOverlay.hidden) {
    closeShortcuts();
    return true;
  }
  if (!aboutOverlay.hidden) {
    closeAbout();
    return true;
  }
  if (!appMenu.hidden) {
    closeAppMenu();
    return true;
  }
  return false;
}

// ¿Hay un popover del menú u overlay modal abierto? (lo consulta el Esc de pantalla completa).
export function isMenuChromeOpen(): boolean {
  if (!appMenu.hidden) return true; // M11: menu hamburguesa abierto
  if (!shortcutsOverlay.hidden || !aboutOverlay.hidden) return true; // M11: overlay modal abierto
  return false;
}

export function initMenu(): void {
  menuTrigger.addEventListener("click", (e) => {
    e.stopPropagation(); // que no llegue al cierre-por-clic-afuera de document
    if (appMenu.hidden) openAppMenu();
    else closeAppMenu();
  });
  document.addEventListener("click", () => closeAppMenu()); // clic afuera cierra
  appMenu.addEventListener("click", (e) => e.stopPropagation()); // clic dentro NO cierra

  // Al abrir el theme-menu, cerrar el app-menu (espejo de la lógica inversa en openAppMenu).
  themeTrigger.addEventListener("click", () => closeAppMenu());

  // Items del menú.
  document.querySelector<HTMLButtonElement>("#menu-shortcuts")?.addEventListener("click", openShortcuts);
  document.querySelector<HTMLButtonElement>("#menu-about")?.addEventListener("click", openAbout);
  document.querySelector<HTMLButtonElement>("#menu-insert-image")?.addEventListener("click", () => {
    closeAppMenu(); // el selector es una ventana nativa: el menú abierto detrás estorba
    void insertImage();
  });

  // Cierre de los overlays: botón × y clic en el BACKDROP (no en el panel). El backdrop y el
  // panel son HERMANOS dentro de .overlay (no el panel hijo del backdrop), así que un clic en
  // el panel nunca alcanza al backdrop: no hace falta stopPropagation para que el panel no cierre.
  document.querySelector<HTMLButtonElement>("#shortcuts-close")?.addEventListener("click", closeShortcuts);
  document.querySelector<HTMLButtonElement>("#about-close")?.addEventListener("click", closeAbout);
  shortcutsOverlay.querySelector(".overlay-backdrop")?.addEventListener("click", closeShortcuts);
  aboutOverlay.querySelector(".overlay-backdrop")?.addEventListener("click", closeAbout);

  // M11: versión en el Acerca de (build-time, sin IPC). pkg.version sale de package.json.
  const aboutVersionNum = document.querySelector<HTMLElement>("#about-version-num");
  if (aboutVersionNum) aboutVersionNum.textContent = pkg.version;

  // Enlaces externos: en WebView2 un <a target=_blank> NO abre el navegador del sistema; hay
  // que usar el plugin opener (openUrl). Interceptamos el clic y delegamos en el SO.
  document.querySelectorAll<HTMLAnchorElement>("[data-external-link]").forEach((a) => {
    a.addEventListener("click", (e) => {
      e.preventDefault();
      void openUrl(a.href);
    });
  });
}
