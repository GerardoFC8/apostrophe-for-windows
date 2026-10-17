// Barra de pestañas: presentacion pura de la coleccion de tabs.ts. Se repinta entera con
// cada cambio (son pocas pestañas y el DOM es chico) y delega TODA accion en
// document-session.ts (activar, cerrar, nueva).
//
// Accesibilidad (patron ARIA "tabs"): role=tablist/tab, aria-selected, tabindex itinerante
// (solo la activa entra en el orden de Tab) y flechas/Inicio/Fin para moverse, con
// activacion automatica. Supr cierra la pestaña enfocada. El boton × queda fuera del orden
// de tabulacion para no sumar una parada por pestaña (Ctrl+W y Supr cubren el teclado).
import { activeDocId, allTabs, onTabsChanged, tabDisplayName, type DocId, type Tab } from "./tabs";
import { activateTab, closeTab, newTab } from "./document-session";
import { t } from "./i18n";

const tabList = document.querySelector<HTMLElement>("#tab-list")!;
const newTabBtn = document.querySelector<HTMLButtonElement>("#new-tab-btn")!;
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
  return tabList.querySelector<HTMLButtonElement>(`.tab-btn[data-doc-id="${id}"]`);
}

function buildTab(tab: Tab, active: boolean): HTMLElement {
  const name = tabDisplayName(tab);

  const wrap = document.createElement("div");
  wrap.className = "tab";
  wrap.setAttribute("role", "presentation");
  wrap.classList.toggle("is-active", active);
  wrap.classList.toggle("is-dirty", tab.dirty);

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "tab-btn";
  btn.id = `tab-${tab.id}`;
  btn.dataset.docId = String(tab.id);
  btn.setAttribute("role", "tab");
  btn.setAttribute("aria-selected", String(active));
  btn.setAttribute("aria-controls", "editor");
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
  btn.append(label, dirty);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "tab-close";
  close.tabIndex = -1;
  close.setAttribute("aria-label", t("tabs.close", { name }));
  close.title = t("tabs.closeHint");
  close.innerHTML = CLOSE_ICON; // markup fijo, sin datos del documento

  btn.addEventListener("click", () => activateTab(tab.id));
  btn.addEventListener("keydown", (e) => onTabKey(e, tab.id));
  close.addEventListener("click", (e) => {
    e.stopPropagation();
    void closeTab(tab.id);
  });
  // Clic central cierra (convencion de navegadores y editores). En mousedown se evita el
  // autoscroll de Chromium; el cierre va en auxclick, que es el "clic" del boton central.
  wrap.addEventListener("mousedown", (e) => {
    if (e.button === 1) e.preventDefault();
  });
  wrap.addEventListener("auxclick", (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    void closeTab(tab.id);
  });

  wrap.append(btn, close);
  return wrap;
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
  if (active !== undefined) {
    editorPanel.setAttribute("aria-labelledby", `tab-${active}`);
    const activeBtn = tabButton(active);
    // Con muchas pestañas la barra scrollea: la activa siempre queda a la vista.
    activeBtn?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }
  if (focused !== undefined) tabButton(Number(focused))?.focus();
}

export function initTabBar(): void {
  onTabsChanged(render);
  newTabBtn.addEventListener("click", () => void newTab());
  // La rueda vertical desplaza la barra en horizontal (no hay scroll vertical que perder).
  tabList.addEventListener(
    "wheel",
    (e) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
      tabList.scrollLeft += e.deltaY;
      e.preventDefault();
    },
    { passive: false },
  );
  render();
}
