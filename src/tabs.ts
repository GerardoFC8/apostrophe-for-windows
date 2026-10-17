// Coleccion de pestañas del frontend: que documentos hay abiertos, cual se muestra y el
// estado de presentacion de cada uno (nombre, ruta, dirty, cambio externo pendiente).
//
// Es la UNICA fuente de "que pestaña esta activa" en el frontend. No habla con el backend:
// los flujos que si lo hacen (abrir, guardar, cerrar…) viven en document-session.ts y usan
// estas funciones. Asi este modulo solo depende del editor y no hay ciclos de imports.
//
// Cada pestaña guarda su propio EditorState mientras NO se muestra (`parked`); la activa
// vive en la EditorView. Cambiar de pestaña = estacionar la activa y montar la otra.
import {
  createDocState,
  getText,
  loadEditor,
  parkEditor,
  replaceStateText,
  setEditorText,
  type ParkedEditor,
} from "./editor";
import { activeAfterClose, canReuseTab, cycleTab } from "./tab-order";
import { t } from "./i18n";

/// Identificador estable del documento en el backend (espejo de `DocId`, un u64).
export type DocId = number;

export interface Tab {
  readonly id: DocId;
  /** Ruta completa del archivo, o null si nunca se guardo. */
  path: string | null;
  /** Nombre del archivo, o null para "Sin título" (lo traduce el catalogo). */
  fileName: string | null;
  dirty: boolean;
  /** El archivo cambio en disco con cambios locales: el infobar se muestra al activarla. */
  externalChange: boolean;
  /** Ruta (o nombre) del archivo del que se recupero un snapshot, para el tooltip. */
  restoredFrom: string | null;
  /** Estado estacionado mientras la pestaña no se muestra; null = es la que esta en la vista. */
  parked: ParkedEditor | null;
}

/// Datos con los que nace una pestaña.
export interface TabInit {
  id: DocId;
  path?: string | null;
  fileName?: string | null;
  dirty?: boolean;
  restoredFrom?: string | null;
}

/// Lo que se puede actualizar de una pestaña desde fuera (el estado del editor no).
export type TabPatch = Partial<Pick<Tab, "path" | "fileName" | "dirty" | "externalChange" | "restoredFrom">>;

const tabs: Tab[] = [];
let activeId: DocId | null = null;

// --- Suscripcion: la barra de pestañas y la titlebar se repintan con cada cambio ---
const listeners = new Set<() => void>();

export function onTabsChanged(listener: () => void): void {
  listeners.add(listener);
}

/// Repinta todo lo que depende de las pestañas (tambien lo usa el cambio de idioma).
export function notifyTabsChanged(): void {
  for (const listener of listeners) listener();
}

// --- Lectura ---

export function allTabs(): readonly Tab[] {
  return tabs;
}

export function getTab(id: DocId): Tab | undefined {
  return tabs.find((tab) => tab.id === id);
}

export function activeTab(): Tab | undefined {
  return activeId === null ? undefined : getTab(activeId);
}

/// Documento de la pestaña activa; undefined antes del arranque (los comandos sin docId
/// apuntan al activo del backend, que en ese momento es el mismo).
export function activeDocId(): DocId | undefined {
  return activeTab()?.id;
}

/// Texto vivo de una pestaña: el de la vista si es la activa, el estacionado si no.
export function tabText(tab: Tab): string {
  return tab.parked ? tab.parked.state.doc.toString() : getText();
}

/// Nombre visible: el del archivo o el "Sin título" del idioma activo.
export function tabDisplayName(tab: Tab): string {
  return tab.fileName || t("doc.untitled");
}

/// ¿Abrir un archivo puede ocupar esta pestaña en lugar de crear otra? (ver tab-order.ts).
export function isReusable(tab: Tab | undefined): boolean {
  return canReuseTab(
    tab && {
      path: tab.path,
      dirty: tab.dirty,
      textLength: tab.parked ? tab.parked.state.doc.length : getText().length,
    },
  );
}

/// Pestaña siguiente (1) o anterior (-1) a la activa, con vuelta.
export function neighbourTabId(step: 1 | -1): DocId | null {
  return cycleTab(
    tabs.map((tab) => tab.id),
    activeId,
    step,
  );
}

// --- Escritura ---

function makeTab(init: TabInit, parked: ParkedEditor | null): Tab {
  return {
    id: init.id,
    path: init.path ?? null,
    fileName: init.fileName ?? null,
    dirty: init.dirty ?? false,
    externalChange: false,
    restoredFrom: init.restoredFrom ?? null,
    parked,
  };
}

/// Agrega una pestaña en segundo plano (al final) con su propio estado de editor.
export function addTab(init: TabInit, text: string): Tab {
  const tab = makeTab(init, { state: createDocState(text), scroll: null });
  tabs.push(tab);
  notifyTabsChanged();
  return tab;
}

/// Registra como pestaña ACTIVA lo que ya esta en la vista (el documento con el que arranca
/// la app, o el en blanco que queda tras cerrar la ultima pestaña).
export function adoptViewAsTab(init: TabInit): Tab {
  const current = activeTab();
  if (current) current.parked = parkEditor();
  const tab = makeTab(init, null);
  tabs.push(tab);
  activeId = tab.id;
  notifyTabsChanged();
  return tab;
}

/// Muestra la pestaña `id` en la vista. Devuelve false si no existe.
export function showTab(id: DocId): boolean {
  const target = getTab(id);
  if (!target) return false;
  if (id === activeId) return true;
  const current = activeTab();
  if (current) current.parked = parkEditor();
  if (target.parked) loadEditor(target.parked);
  target.parked = null;
  activeId = id;
  notifyTabsChanged();
  return true;
}

/// Monta en la vista un documento en blanco que todavia no es pestaña (al cerrar la ultima,
/// mientras el backend crea la nueva). Deja la coleccion sin activa.
export function showBlankEditor(): void {
  const current = activeTab();
  if (current) current.parked = parkEditor();
  loadEditor({ state: createDocState(""), scroll: null });
  activeId = null;
  notifyTabsChanged();
}

/// Quita una pestaña. Si era la activa, la coleccion queda SIN activa y se devuelve la que
/// deberia ocupar su lugar (quien llama la muestra con showTab); si no, se devuelve la
/// activa de siempre. null = no queda ninguna.
export function removeTab(id: DocId): { next: DocId | null; wasActive: boolean } {
  const ids = tabs.map((tab) => tab.id);
  const index = ids.indexOf(id);
  if (index < 0) return { next: activeId, wasActive: false };
  const wasActive = id === activeId;
  const next = activeAfterClose(ids, id, activeId);
  tabs.splice(index, 1);
  if (wasActive) activeId = null;
  notifyTabsChanged();
  return { next, wasActive };
}

/// Actualiza los datos de presentacion. Solo avisa si algo cambio de verdad: el debounce
/// sincroniza cada 250 ms y no hace falta repintar la barra si el dirty sigue igual.
export function updateTab(id: DocId, patch: TabPatch): void {
  const tab = getTab(id);
  if (!tab) return;
  let changed = false;
  for (const key of Object.keys(patch) as (keyof TabPatch)[]) {
    if (patch[key] !== undefined && tab[key] !== patch[key]) {
      (tab as unknown as Record<string, unknown>)[key] = patch[key];
      changed = true;
    }
  }
  if (changed) notifyTabsChanged();
}

/// Reemplaza el texto conservando el historial (recargar desde disco: mismo criterio que
/// el editor de un solo documento). En segundo plano se descarta el scroll guardado, que
/// apuntaba al texto viejo.
export function setTabText(tab: Tab, text: string): void {
  if (!tab.parked) {
    setEditorText(text);
    return;
  }
  tab.parked = { state: replaceStateText(tab.parked.state, text), scroll: null };
}

/// Reemplaza el documento entero por uno NUEVO (historial vacio): abrir un archivo dentro
/// de una pestaña en blanco o restaurar un snapshot. Deshacer no debe volver a lo anterior.
export function resetTab(tab: Tab, text: string): void {
  const parked: ParkedEditor = { state: createDocState(text), scroll: null };
  if (tab.parked) tab.parked = parked;
  else loadEditor(parked);
}
