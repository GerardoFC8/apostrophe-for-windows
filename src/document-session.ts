// Los documentos abiertos vistos desde el frontend: los flujos que hablan con el workspace
// de Rust (nueva pestaña, abrir, guardar, recargar, cerrar pestaña, cierre de ventana,
// autosave, cambio externo, arranque) y la titlebar. La coleccion de pestañas y cual esta
// activa viven en tabs.ts; aqui se decide QUE pedirle al backend y como reflejar la
// respuesta en esas pestañas.
//
// La logica (dirty, rutas, confirmaciones) vive en Rust; aqui solo se presenta y se cablea.
// Todos los comandos llevan `docId` explicito: el backend tambien tiene un "activo", pero
// el frontend no depende de que coincida con el suyo.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appWindow } from "./app-window";
import { getText, view } from "./editor";
import {
  activeDocId,
  activeTab,
  addTab,
  adoptViewAsTab,
  allTabs,
  getTab,
  isReusable,
  neighbourTabId,
  notifyTabsChanged,
  onTabsChanged,
  removeTab,
  resetTab,
  setTabText,
  showBlankEditor,
  showTab,
  tabDisplayName,
  tabText,
  updateTab,
  type DocId,
  type Tab,
} from "./tabs";
import { flashStatus } from "./status";
import { t, tp } from "./i18n";

// --- Contratos del IPC con el nucleo (snake_case: asi serializa serde los campos) ---
export interface OpenedDoc {
  doc_id: DocId;
  path: string;
  file_name: string;
  content: string;
}
export interface DocStatus {
  doc_id: DocId;
  file_name: string | null;
  dirty: boolean;
}
interface DocumentInfo {
  doc_id: DocId;
  path: string | null;
  name: string | null;
  dirty: boolean;
}
// M8.4a: contrato del snapshot restaurado (espejo de RestoredSnapshot de Rust).
export interface RestoredSnapshot {
  content: string;
  original_file_name: string | null;
  original_path?: string | null;
}
interface DiskChange {
  doc_id: DocId;
}

/// Error estable de "guardar como" sobre una ruta que ya tiene abierta otra pestaña.
const PATH_OPEN_IN_OTHER_DOCUMENT = "path-open-in-other-document";

const docName = document.querySelector<HTMLElement>("#doc-name")!;

// M8.4b: infobar de cambio externo (refleja a la pestaña ACTIVA).
const externalChangeBar = document.querySelector<HTMLElement>("#external-change-bar")!;
const reloadBtn = document.querySelector<HTMLButtonElement>("#reload-btn")!;
const keepBtn = document.querySelector<HTMLButtonElement>("#keep-btn")!;

// --- Etiquetas de los dialogos nativos ---
// Se arman aca y viajan por IPC: el backend no guarda copia (ADR-0001), asi sumar un idioma
// no toca Rust. Se resuelven en cada llamada para respetar el idioma activo en ese momento.
interface ConfirmLabels {
  title: string;
  message: string;
  confirm: string;
  cancel: string;
}

const openLabels = (): ConfirmLabels => ({
  title: t("confirm.openTitle"),
  message: t("confirm.openMessage"),
  confirm: t("confirm.openConfirm"),
  cancel: t("confirm.cancel"),
});

const newLabels = (): ConfirmLabels => ({
  title: t("confirm.newTitle"),
  message: t("confirm.openMessage"), // mismo aviso: se pierde el documento actual
  confirm: t("confirm.newConfirm"),
  cancel: t("confirm.cancel"),
});

const closeTabLabels = (name: string): ConfirmLabels => ({
  title: t("confirm.closeTitle"),
  message: t("confirm.closeTabMessage", { name }),
  confirm: t("confirm.closeConfirm"),
  cancel: t("confirm.cancel"),
});

/// Cierre de VENTANA: una sola confirmacion para todos los documentos modificados. Con una
/// sola pestaña se conserva la frase de siempre ("El documento…"); con varias se dice
/// cuantos se pierden, porque "el documento" ya no dice cual.
export function closeWindowLabels(dirtyCount: number, openCount: number): ConfirmLabels {
  return {
    title: t("confirm.closeTitle"),
    message:
      openCount <= 1
        ? t("confirm.closeMessage")
        : tp("confirm.closeWindowMessage", Math.max(dirtyCount, 1)),
    confirm: t("confirm.closeConfirm"),
    cancel: t("confirm.cancel"),
  };
}

// --- Titlebar: nombre de la pestaña activa + indicador ● de cambios sin guardar ---
function paintTitlebar(): void {
  const tab = activeTab();
  const name = tab ? tabDisplayName(tab) : t("doc.untitled");
  docName.textContent = tab?.dirty ? `${name} ●` : name;
}
onTabsChanged(paintTitlebar);

// Repinta la titlebar y la barra de pestañas (al cambiar de idioma: "Sin título" se traduce).
export function refreshTitlebar(): void {
  notifyTabsChanged();
}

function syncExternalBar(): void {
  externalChangeBar.hidden = !activeTab()?.externalChange;
}

// --- Enganche con el resto de la app ---
// Al cambiar el documento mostrado hay que re-renderizar preview, contadores, resaltado y
// corrector. Esos modulos no se importan aca (dependen de este): main.ts inyecta el refresco.
let onActiveDocChanged: () => void = () => {};

export function initDocumentSession(deps: { onActiveDocChanged: () => void }): void {
  onActiveDocChanged = deps.onActiveDocChanged;
}

// Las operaciones que cambian la coleccion (abrir, nueva, cerrar) se SERIALIZAN: dos
// aperturas simultaneas (p.ej. varios archivos reenviados por single-instance) verian la
// misma pestaña en blanco como reutilizable y una pisaria a la otra.
let queue: Promise<unknown> = Promise.resolve();
function serialized<T>(job: () => Promise<T>): Promise<T> {
  const run = queue.then(job, job);
  queue = run.catch(() => {});
  return run;
}

// Ninguna accion del usuario corre antes de que el arranque arme las pestañas.
let markReady: () => void = () => {};
const ready = new Promise<void>((resolve) => (markReady = resolve));

function applyStatus(st: DocStatus): void {
  updateTab(st.doc_id, { fileName: st.file_name, dirty: st.dirty });
}

// Rutas y nombres de todas las pestañas segun el backend (tras guardar-como cambia la ruta,
// y DocStatus solo trae el nombre). El dirty NO se toma de aca: el del backend puede ir
// atrasado respecto del debounce.
async function refreshPaths(): Promise<void> {
  try {
    const docs = await invoke<DocumentInfo[]>("list_documents");
    for (const info of docs) updateTab(info.doc_id, { path: info.path, fileName: info.name });
  } catch {
    /* no critico: el tooltip queda con la ruta anterior */
  }
}

// --- Sincronizacion de texto ---

// Envia el texto vivo de una pestaña al backend (calcula dirty). El texto se lee ANTES del
// await: llamarla justo antes de cambiar de pestaña captura lo ultimo que se escribio.
async function syncTab(tab: Tab): Promise<DocStatus | null> {
  const text = tabText(tab);
  try {
    const st = await invoke<DocStatus>("set_content", { text, docId: tab.id });
    applyStatus(st);
    return st;
  } catch {
    return null; // no critico (p.ej. la pestaña se cerro mientras volaba)
  }
}

// M4: sincroniza la pestaña activa en el debounce. El indicador ● no es critico.
export async function syncDoc(): Promise<void> {
  const tab = activeTab();
  if (tab) {
    await syncTab(tab);
    return;
  }
  // Antes del arranque no hay pestañas: el backend usa su documento activo.
  try {
    await invoke<DocStatus>("set_content", { text: getText() });
  } catch {
    /* no critico */
  }
}

// --- Activacion ---

function afterActivation(focusEditor: boolean): void {
  syncExternalBar();
  onActiveDocChanged();
  if (focusEditor) view.focus();
}

/// Muestra la pestaña `id`. `focusEditor = false` lo usa la navegacion por teclado de la
/// barra, que deja el foco en la pestaña.
export function activateTab(id: DocId, focusEditor = true): void {
  const previous = activeTab();
  if (!getTab(id)) return;
  if (previous && previous.id !== id) void syncTab(previous); // lee el texto ya, antes del swap
  showTab(id);
  void invoke("activate_document", { docId: id }).catch(() => {});
  afterActivation(focusEditor);
}

/// Ctrl+Tab / Ctrl+Shift+Tab.
export function cycleActiveTab(step: 1 | -1): void {
  const next = neighbourTabId(step);
  if (next !== null && next !== activeDocId()) activateTab(next);
}

// --- Nueva pestaña / abrir ---

/// Ctrl+T / Ctrl+N: documento nuevo en su propia pestaña (no descarta nada, no confirma).
export function newTab(): Promise<void> {
  return serialized(async () => {
    await ready;
    const current = activeTab();
    try {
      const st = await invoke<DocStatus | null>("new_document", {
        text: current ? tabText(current) : undefined,
        labels: newLabels(),
        docId: current?.id,
        newTab: true,
      });
      if (!st) return;
      addTab({ id: st.doc_id, fileName: st.file_name, dirty: st.dirty }, "");
      activateTab(st.doc_id);
      flashStatus(t("status.newDoc"));
    } catch (err) {
      flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
    }
  });
}

interface OpenTarget {
  docId?: DocId;
  newTab: boolean;
  /** Pestaña en blanco que se ocupa con el archivo, si se reutiliza una. */
  reused: DocId | null;
}

// Donde abrir: en la pestaña activa si esta en blanco (sin archivo, sin texto, sin cambios),
// o en una nueva. Antes de reutilizarla se sincroniza, para que el backend la vea limpia y
// no pida confirmar un descarte que no existe.
async function openTarget(): Promise<OpenTarget> {
  const tab = activeTab();
  if (tab && isReusable(tab)) {
    await syncTab(tab);
    if (isReusable(tab)) return { docId: tab.id, newTab: false, reused: tab.id };
  }
  return { newTab: true, reused: null };
}

// Refleja un documento recien abierto. Tres casos segun el doc_id que devolvio el backend:
// ya estaba abierto en otra pestaña (se activa, sin pisar su texto vivo), se cargo en la
// pestaña en blanco reutilizada, o es una pestaña nueva.
function applyOpened(opened: OpenedDoc, reused: DocId | null): void {
  const existing = getTab(opened.doc_id);
  if (existing && opened.doc_id !== reused) {
    activateTab(existing.id);
    flashStatus(t("status.alreadyOpen"));
    return;
  }
  if (existing) {
    updateTab(existing.id, {
      path: opened.path,
      fileName: opened.file_name,
      dirty: false,
      externalChange: false,
      restoredFrom: null,
    });
    resetTab(existing, opened.content);
    activateTab(existing.id);
  } else {
    addTab({ id: opened.doc_id, path: opened.path, fileName: opened.file_name }, opened.content);
    activateTab(opened.doc_id);
  }
  flashStatus(t("status.opened"));
}

// M4: abrir un .md con el dialogo nativo, en su propia pestaña.
export function openDoc(): Promise<void> {
  return serialized(async () => {
    await ready;
    try {
      const target = await openTarget();
      const opened = await invoke<OpenedDoc | null>("open_document", {
        labels: openLabels(),
        docId: target.docId,
        newTab: target.newTab,
      });
      if (!opened) return; // cancelado
      applyOpened(opened, target.reused);
    } catch (err) {
      flashStatus(`${t("error.open")}: ${String(err)}`, 4000);
    }
  });
}

// Abre una ruta concreta en su propia pestaña (o activa la que ya la tiene): recientes
// (M16), doble clic con la app abierta (single-instance) y archivos del arranque.
export function openPath(path: string): Promise<void> {
  return serialized(async () => {
    await ready;
    try {
      const target = await openTarget();
      const opened = await invoke<OpenedDoc | null>("open_path", {
        path,
        labels: openLabels(),
        docId: target.docId,
        newTab: target.newTab,
      });
      if (!opened) return;
      applyOpened(opened, target.reused);
    } catch (err) {
      flashStatus(`${t("error.open")}: ${String(err)}`, 4000);
    }
  });
}

// --- Guardar ---

function reportSaveError(err: unknown): void {
  // El archivo elegido ya lo tiene otra pestaña: dos documentos con la misma ruta se
  // pisarian al guardar. El backend no dice cual es (la ruta la eligio el dialogo), asi que
  // solo se explica.
  if (String(err) === PATH_OPEN_IN_OTHER_DOCUMENT) {
    flashStatus(t("status.pathOpenInOtherTab"), 6000);
    return;
  }
  flashStatus(`${t("error.save")}: ${String(err)}`, 4000);
}

async function save(command: "save_document" | "save_document_as"): Promise<void> {
  await ready;
  const tab = activeTab();
  const text = getText();
  try {
    const st = await invoke<DocStatus | null>(command, { text, docId: tab?.id });
    if (!st) return; // cancelado
    applyStatus(st);
    flashStatus(t("status.saved"));
    await refreshPaths(); // guardar sin ruta delega en guardar-como: la ruta pudo cambiar
  } catch (err) {
    reportSaveError(err);
  }
}

// M4: guardar (escribe al path actual, o abre "guardar como" si es nuevo).
export function saveDoc(): Promise<void> {
  return save("save_document");
}

// M4: guardar como (siempre dialogo).
export function saveDocAs(): Promise<void> {
  return save("save_document_as");
}

// --- Cerrar pestaña ---

// Pestañas con un dialogo de cierre abierto: un segundo Ctrl+W o clic no apila otro.
const closing = new Set<DocId>();

// Tras cerrar la ultima pestaña queda una en blanco: la app nunca se queda sin documento.
async function replaceLastTab(): Promise<void> {
  showBlankEditor();
  try {
    const st = await invoke<DocStatus | null>("new_document", { labels: newLabels(), newTab: true });
    if (st) adoptViewAsTab({ id: st.doc_id, fileName: st.file_name, dirty: st.dirty });
  } catch (err) {
    flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
  }
  afterActivation(true);
}

/// Cierra la pestaña `id` (la activa si no se indica). Si tiene cambios, el backend confirma.
export function closeTab(id?: DocId): Promise<void> {
  return serialized(async () => {
    await ready;
    const tab = id !== undefined ? getTab(id) : activeTab();
    if (!tab || closing.has(tab.id)) return;
    closing.add(tab.id);
    try {
      const closed = await invoke<boolean>("close_document", {
        docId: tab.id,
        labels: closeTabLabels(tabDisplayName(tab)),
        text: tabText(tab),
      });
      if (!closed) return; // el usuario eligio no descartar
      const { next, wasActive } = removeTab(tab.id);
      if (next === null) await replaceLastTab();
      else if (wasActive) activateTab(next);
    } catch (err) {
      flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
    } finally {
      closing.delete(tab.id);
    }
  });
}

// --- M8.1: cierre de la VENTANA ---
// Cubre las 3 vías de cierre: botón X (que llama appWindow.close() -> dispara este evento),
// Alt+F4 y cerrar desde la barra de tareas.
//
// SIEMPRE prevenimos el cierre y lo controlamos nosotros: tras confirmar, cerramos con
// appWindow.destroy() EXPLÍCITO (el destroy implícito de Tauri necesita el permiso
// core:window:allow-destroy; sin él la ventana quedaba trabada). destroy() NO re-dispara
// onCloseRequested, así que no hay loop.
//
// Con varias pestañas hay UNA sola confirmación para todas las modificadas; la copia dice
// cuántas son (dirty_documents). `closeInProgress` colapsa disparos REENTRANTES: el diálogo
// nativo no es modal para el event loop de JS, y un segundo X apilaría otro diálogo.
let closeInProgress = false;
export function initCloseGuard(): void {
  void appWindow.onCloseRequested(async (event) => {
    event.preventDefault(); // tomamos el control del cierre (evita el destroy implícito de Tauri)
    if (closeInProgress) return; // ya hay un diálogo decidiendo: no apilar otro
    closeInProgress = true;
    let proceed = false;
    try {
      const tab = activeTab();
      const text = getText();
      // El debounce puede ir 250 ms atrasado: sincronizar ANTES de contar los modificados,
      // para que teclear-y-cerrar no se cuente como "sin cambios".
      if (tab) await invoke<DocStatus>("set_content", { text, docId: tab.id });
      const dirty = await invoke<DocId[]>("dirty_documents");
      proceed = await invoke<boolean>("confirm_close", {
        text,
        labels: closeWindowLabels(dirty.length, allTabs().length),
        docId: tab?.id,
      });
    } catch {
      // ante un error en el chequeo NO cerramos silenciosamente (podríamos perder datos):
      // dejamos la ventana abierta y lo hacemos VISIBLE para que el usuario guarde a mano.
      flashStatus(t("status.dirtyCheckFailed"), 4000);
      closeInProgress = false;
      return;
    }
    if (proceed) {
      try {
        await appWindow.destroy(); // cierre forzado y explícito (no re-dispara onCloseRequested)
      } catch {
        // si el cierre fallara, no dejar la ventana trabada: permitir reintentar.
        closeInProgress = false;
        flashStatus(t("status.closeFailed"), 4000);
      }
    } else {
      closeInProgress = false; // el usuario canceló: permitir un futuro intento de cierre
    }
  });
}

// --- M8.4b: cambios externos ---

// Re-lee el archivo de una pestaña desde disco (activa o en segundo plano). Recargado =
// limpio. reload_from_disk deja ESE documento como activo del backend: si era una pestaña
// en segundo plano se le devuelve el activo real.
async function reloadTab(tab: Tab): Promise<void> {
  const opened = await invoke<OpenedDoc | null>("reload_from_disk", { docId: tab.id });
  if (opened && getTab(tab.id)) {
    setTabText(tab, opened.content);
    updateTab(tab.id, { fileName: opened.file_name, dirty: false, externalChange: false });
  } else {
    updateTab(tab.id, { externalChange: false });
  }
  const active = activeDocId();
  if (active !== undefined && active !== tab.id) {
    void invoke("activate_document", { docId: active }).catch(() => {});
  }
  syncExternalBar();
}

export function initExternalChangeWatcher(): void {
  // Ramifica por estado dirty (decision de producto M8.4b), por pestaña:
  // - limpia: se recarga (en silencio si esta en segundo plano);
  // - con cambios: infobar ahora si es la activa, o al activarla si no.
  // Decide sobre el dirty REAL del texto vivo (set_content), no sobre el ultimo conocido:
  // el debounce puede ir atrasado y una recarga pisaria tecleos recientes.
  void listen<DiskChange | null>("file-changed-on-disk", async (event) => {
    const id = event.payload?.doc_id;
    const tab = id !== undefined ? getTab(id) : activeTab();
    if (!tab) return;
    const st = await syncTab(tab);
    const dirty = st ? st.dirty : true; // ante la duda, NO recargar de forma destructiva
    if (!getTab(tab.id)) return; // se cerro mientras tanto
    if (!dirty) {
      try {
        await reloadTab(tab);
        if (tab.id === activeDocId()) flashStatus(t("status.changedOnDisk"), 4000);
      } catch {
        /* no critico: el archivo pudo desaparecer; el texto vivo sigue intacto */
      }
    } else {
      updateTab(tab.id, { externalChange: true });
      syncExternalBar();
    }
  });

  // Botones del infobar: actuan sobre la pestaña activa.
  reloadBtn.addEventListener("click", () => {
    const tab = activeTab();
    if (!tab) return;
    void reloadTab(tab)
      .then(() => flashStatus(t("status.reloaded"), 3000))
      .catch((err) => flashStatus(`${t("error.open")}: ${String(err)}`, 4000));
  });
  keepBtn.addEventListener("click", () => {
    // El usuario elige mantener sus cambios; el proximo Ctrl+S sobrescribira el cambio externo.
    const tab = activeTab();
    if (tab) updateTab(tab.id, { externalChange: false });
    syncExternalBar();
  });
}

// --- Capa B + M8.4a: arranque ---

// Restaura un snapshot huerfano en una pestaña: la activa si esta en blanco (la del
// arranque), o una nueva. Igual que con un solo documento: el contenido vuelve como
// documento SIN TITULO y modificado (hay que guardarlo), con el aviso visible desde ya.
async function restoreSnapshot(snapshot: RestoredSnapshot): Promise<DocId | null> {
  let tab = activeTab();
  if (!isReusable(tab)) {
    const st = await invoke<DocStatus | null>("new_document", { labels: newLabels(), newTab: true });
    if (!st) return null;
    tab = addTab({ id: st.doc_id, fileName: st.file_name }, "");
  }
  if (!tab) return null;
  resetTab(tab, snapshot.content);
  updateTab(tab.id, {
    dirty: true,
    restoredFrom: snapshot.original_path ?? snapshot.original_file_name ?? null,
  });
  await syncTab(tab); // el backend arranca vacio: asi tambien lo cuenta como modificado
  return tab.id;
}

// Arma las pestañas iniciales. Precedencia: (1) archivos de la linea de comandos /
// asociacion .md (take_pending_files, ya cargados en el workspace; el primero activo),
// (2) snapshots de un crash (take_pending_restores, uno por pestaña, el mas reciente
// activo). Si se abrio con archivos, los snapshots NO se restauran (igual que antes).
export async function restoreInitialDocument(): Promise<void> {
  try {
    const files = await invoke<OpenedDoc[]>("take_pending_files");
    const docs = await invoke<DocumentInfo[]>("list_documents");
    const contentOf = new Map(files.map((f) => [f.doc_id, f.content]));
    const first = files[0]?.doc_id ?? docs[0]?.doc_id;
    for (const info of docs) {
      const init = { id: info.doc_id, path: info.path, fileName: info.name, dirty: info.dirty };
      const text = contentOf.get(info.doc_id);
      if (info.doc_id === first) {
        const tab = adoptViewAsTab(init);
        if (text !== undefined) resetTab(tab, text);
      } else {
        addTab(init, text ?? "");
      }
    }
    if (first !== undefined) void invoke("activate_document", { docId: first }).catch(() => {});

    if (files.length === 0) {
      const restores = await invoke<RestoredSnapshot[]>("take_pending_restores");
      const restored: DocId[] = [];
      for (const snapshot of restores) {
        const id = await restoreSnapshot(snapshot);
        if (id !== null) restored.push(id);
      }
      if (restored.length > 0) {
        showTab(restored[0]); // el mas reciente
        void invoke("activate_document", { docId: restored[0] }).catch(() => {});
        flashStatus(tp("status.restored", restored.length), 5000);
      }
    }
  } catch {
    /* no critico: si falla, la app queda con el editor vacio */
  }
  // Pase lo que pase, la vista tiene que quedar registrada como pestaña.
  if (!activeTab()) {
    try {
      const st = await invoke<DocStatus>("set_content", { text: getText() });
      if (!getTab(st.doc_id)) adoptViewAsTab({ id: st.doc_id, fileName: st.file_name, dirty: st.dirty });
      else showTab(st.doc_id);
    } catch {
      /* sin backend no hay documento: las acciones seguiran fallando de forma visible */
    }
  }
  markReady();
  afterActivation(false);
}

// Listeners de Tauri que alimentan a los documentos una vez arrancada la app.
export function initDocumentEvents(): void {
  // M8.4a: autosave por snapshots. Cada tick (cada 5 s, emitido por Rust) envia el texto de
  // CADA pestaña para que el backend evalúe si debe escribir o borrar su snapshot. Silencioso:
  // el autosave no es critico. Guard de cierre: si ya estamos cerrando, no re-escribir los
  // snapshots que confirm_close va a borrar.
  void listen("autosave-tick", () => {
    if (closeInProgress) return;
    for (const tab of allTabs()) {
      void invoke("write_snapshot", { text: tabText(tab), docId: tab.id }).catch(() => {});
    }
  });

  // App YA abierta + doble-click en otro .md: single-instance reenvia cada ruta por este
  // evento. Se abre en su propia pestaña (o se activa la que ya la tiene).
  void listen<string>("open-file-path", (event) => void openPath(event.payload));
}
