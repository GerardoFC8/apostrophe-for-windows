// Barra de pestañas: presentacion pura de la coleccion de tabs.ts. Se repinta entera con
// cada cambio (son pocas pestañas y el DOM es pequeño) y delega TODA accion en
// document-session.ts (activar, cerrar, nueva).
//
// Accesibilidad (patron ARIA "tabs"): el tablist contiene SOLO pestañas (role=tab, con
// aria-selected y tabindex itinerante: solo la activa entra en el orden de Tab) y se
// recorre con flechas/Inicio/Fin, con activacion automatica. La × es un adorno para el
// mouse dentro de la propia pestaña (aria-hidden, sin foco); el camino de teclado para
// cerrar es Supr sobre la pestaña o Ctrl+W, y se anuncia con aria-keyshortcuts.
//
// Desborde: las pestañas se encogen hasta su ancho mínimo (CSS). Si aun así no entran, las
// que sobran se ocultan y quedan en el menú del botón +N; la activa siempre queda a la
// vista (toma el lugar de la última visible). Nada se desplaza: la barra es también la
// zona para arrastrar la ventana.
import { activeDocId, allTabs, onTabsChanged, tabDisplayName, type DocId, type Tab } from "./tabs";
import { activateTab, closeTab, newTab } from "./document-session";
import { t, tp } from "./i18n";

const tabList = document.querySelector<HTMLElement>("#tab-list")!;
const newTabBtn = document.querySelector<HTMLButtonElement>("#new-tab-btn")!;
const tabBar = document.querySelector<HTMLElement>("#tab-bar")!;
const overflowBtn = document.querySelector<HTMLButtonElement>("#tab-overflow")!;
const overflowMenu = document.querySelector<HTMLElement>("#tab-overflow-menu")!;
const editorPanel = document.querySelector<HTMLElement>("#editor")!;

const CLOSE_ICON =
  '<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 1.5 L8.5 8.5 M8.5 1.5 L1.5 8.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>';

// Tooltip: la ruta completa; sin archivo, de donde se recupero (si fue un snapshot) o el
// nombre visible.
function tabTooltip(tab: Tab): string {
  if (tab.path) return tab.path;
  if (tab.restoredFrom) return t("tabs.restoredFrom", { path: tab.restoredFrom });
  return tabDisplayName(tab);
}

function tabButton(id: DocId): HTMLButtonElement | null {
  return tabList.querySelector<HTMLButtonElement>(`.tab[data-doc-id="${id}"]`);
}

function buildTab(tab: Tab, active: boolean): HTMLElement {
  const name = tabDisplayName(tab);

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "tab";
  btn.classList.toggle("is-active", active);
  btn.classList.toggle("is-dirty", tab.dirty);
  btn.id = `tab-${tab.id}`;
  btn.dataset.docId = String(tab.id);
  btn.setAttribute("role", "tab");
  btn.setAttribute("aria-selected", String(active));
  btn.setAttribute("aria-controls", "editor");
  btn.setAttribute("aria-keyshortcuts", "Delete Control+W");
  btn.tabIndex = active ? 0 : -1;
  btn.title = tabTooltip(tab);
  // El ● es visual: el nombre accesible ya dice si hay cambios sin guardar.
  btn.setAttribute("aria-label", tab.dirty ? t("tabs.unsavedLabel", { name }) : name);

  const label = document.createElement("span");
  label.className = "tab-name";
  label.textContent = name; // textContent: el nombre viene del sistema de archivos
  const dirty = document.createElement("span");
  dirty.className = "tab-dirty";
  dirty.setAttribute("aria-hidden", "true");
  dirty.textContent = "●";
  // × solo para el mouse: fuera del arbol de accesibilidad y sin foco (ver cabecera).
  const close = document.createElement("span");
  close.className = "tab-close";
  close.setAttribute("aria-hidden", "true");
  close.title = t("tabs.closeHint");
  close.innerHTML = CLOSE_ICON; // markup fijo, sin datos del documento
  btn.append(label, dirty, close);

  btn.addEventListener("click", (e) => {
    if ((e.target as Element).closest(".tab-close")) void closeTab(tab.id);
    else activateTab(tab.id);
  });
  btn.addEventListener("keydown", (e) => onTabKey(e, tab.id));
  // Clic central cierra (convencion de navegadores y editores). En mousedown se evita el
  // autoscroll de Chromium; el cierre va en auxclick, que es el "clic" del boton central.
  btn.addEventListener("mousedown", (e) => {
    if (e.button === 1) e.preventDefault();
  });
  btn.addEventListener("auxclick", (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    void closeTab(tab.id);
  });
  return btn;
}

// Navegacion por teclado dentro del tablist (el foco se queda en las pestañas).
function onTabKey(e: KeyboardEvent, id: DocId): void {
  const ids = allTabs().map((tab) => tab.id);
  const index = ids.indexOf(id);
  let target: DocId | undefined;
  if (e.key === "ArrowRight") target = ids[(index + 1) % ids.length];
  else if (e.key === "ArrowLeft") target = ids[(index - 1 + ids.length) % ids.length];
  else if (e.key === "Home") target = ids[0];
  else if (e.key === "End") target = ids[ids.length - 1];
  else if (e.key === "Delete") {
    e.preventDefault();
    void closeTab(id);
    return;
  } else return;
  e.preventDefault();
  if (target === undefined) return;
  activateTab(target, false);
  tabButton(target)?.focus();
}

function render(): void {
  // Si el foco estaba en una pestaña, se devuelve a la misma tras reconstruir el DOM.
  const focused = (document.activeElement as HTMLElement | null)?.dataset?.docId;
  const active = activeDocId();
  tabList.replaceChildren(...allTabs().map((tab) => buildTab(tab, tab.id === active)));
  if (active !== undefined) editorPanel.setAttribute("aria-labelledby", `tab-${active}`);
  closeOverflowMenu();
  layoutOverflow();
  if (focused !== undefined) tabButton(Number(focused))?.focus();
}

// --- Desborde: botón +N y su menú ---

// Pestañas que no entran (en el orden de la barra). Las lista el menú del +N.
let overflowIds: DocId[] = [];

function layoutOverflow(): void {
  const buttons = Array.from(tabList.children) as HTMLElement[];
  for (const el of buttons) el.hidden = false;
  overflowBtn.hidden = true;
  overflowIds = [];
  if (buttons.length === 0 || tabList.scrollWidth <= tabList.clientWidth + 1) return;

  // El botón se muestra antes de medir: su ancho sale del espacio de las pestañas.
  overflowBtn.hidden = false;
  overflowBtn.textContent = `+${buttons.length}`;
  const minWidth = parseFloat(getComputedStyle(buttons[0]).minWidth) || 96;
  const gap = parseFloat(getComputedStyle(tabList).columnGap) || 0;
  const fit = Math.max(1, Math.floor((tabList.clientWidth + gap) / (minWidth + gap)));

  const ids = allTabs().map((tab) => tab.id);
  const shown = new Set(ids.slice(0, fit));
  const active = activeDocId();
  if (active !== undefined && !shown.has(active)) {
    shown.delete(ids[fit - 1]);
    shown.add(active);
  }
  overflowIds = ids.filter((id) => !shown.has(id));
  for (const el of buttons) el.hidden = !shown.has(Number(el.dataset.docId));

  overflowBtn.textContent = `+${overflowIds.length}`;
  const label = tp("tabs.more", overflowIds.length);
  overflowBtn.title = label;
  overflowBtn.setAttribute("aria-label", label);
}

function openOverflowMenu(): void {
  const tabs = allTabs();
  overflowMenu.replaceChildren(
    ...overflowIds.map((id) => {
      const tab = tabs.find((x) => x.id === id)!;
      const item = document.createElement("button");
      item.type = "button";
      item.className = "menu-item tab-overflow-item";
      item.setAttribute("role", "menuitem");
      item.title = tabTooltip(tab);
      const name = document.createElement("span");
      name.className = "recent-name";
      name.textContent = tabDisplayName(tab); // textContent: viene del sistema de archivos
      item.append(name);
      if (tab.dirty) {
        const dot = document.createElement("span");
        dot.className = "tab-overflow-dirty";
        dot.setAttribute("aria-hidden", "true");
        item.append(dot);
        item.setAttribute("aria-label", t("tabs.unsavedLabel", { name: tabDisplayName(tab) }));
      }
      item.addEventListener("click", () => {
        closeOverflowMenu();
        activateTab(id);
      });
      return item;
    }),
  );
  overflowMenu.hidden = false;
  overflowBtn.setAttribute("aria-expanded", "true");
  overflowMenu.querySelector<HTMLElement>(".menu-item")?.focus();
}

function closeOverflowMenu(): void {
  if (overflowMenu.hidden) return;
  overflowMenu.hidden = true;
  overflowBtn.setAttribute("aria-expanded", "false");
}

export function initTabBar(): void {
  onTabsChanged(render);
  newTabBtn.addEventListener("click", () => void newTab());
  overflowBtn.addEventListener("click", () => {
    if (overflowMenu.hidden) openOverflowMenu();
    else closeOverflowMenu();
  });
  document.addEventListener("pointerdown", (e) => {
    if (!(e.target as Element).closest(".tab-overflow-select")) closeOverflowMenu();
  });
  overflowMenu.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closeOverflowMenu();
      overflowBtn.focus();
    }
  });
  // Al cambiar el ancho de la ventana cambia cuántas entran. Ocultar pestañas no cambia el
  // tamaño de la barra (ocupa el espacio libre), así que no hay bucle.
  new ResizeObserver(() => layoutOverflow()).observe(tabBar);
  render();
}
