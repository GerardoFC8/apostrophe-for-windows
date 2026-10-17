// "El documento actual" visto desde el frontend: nombre, estado dirty, titlebar y los
// flujos que hablan con el DocumentStore de Rust (nuevo, abrir, guardar, recargar, cierre,
// autosave, cambio externo). Es el UNICO modulo que conoce ese estado: los demas usan las
// funciones exportadas y nunca leen sus variables. Hoy hay un solo documento; las pestañas
// convertiran este estado en una coleccion sin que el resto tenga que enterarse.
//
// La logica (dirty, rutas, confirmaciones) vive en Rust; aqui solo se presenta y se cablea.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appWindow } from "./app-window";
import { getText, setEditorText } from "./editor";
import { flashStatus } from "./status";
import { t } from "./i18n";

// --- Contratos del IPC con el nucleo ---
export interface OpenedDoc {
  path: string;
  file_name: string;
  content: string;
}
export interface DocStatus {
  file_name: string | null;
  dirty: boolean;
}
// M8.4a: contrato del snapshot restaurado (espejo de RestoredSnapshot de Rust).
export interface RestoredSnapshot {
  content: string;
  original_file_name: string | null;
}

const docName = document.querySelector<HTMLElement>("#doc-name")!;

// M8.4b: infobar de cambio externo.
const externalChangeBar = document.querySelector<HTMLElement>("#external-change-bar")!;
const reloadBtn = document.querySelector<HTMLButtonElement>("#reload-btn")!;
const keepBtn = document.querySelector<HTMLButtonElement>("#keep-btn")!;

// Etiquetas de los dialogos nativos de confirmacion. Se arman aca y viajan por IPC:
// el backend no guarda copia (ADR-0001), asi sumar un idioma no toca Rust. Se resuelven
// en cada llamada, no una vez, para que respeten el idioma activo en ese momento.
const confirmLabels = (kind: "open" | "close" | "new") =>
  ({
    open: {
      title: t("confirm.openTitle"),
      message: t("confirm.openMessage"),
      confirm: t("confirm.openConfirm"),
    },
    close: {
      title: t("confirm.closeTitle"),
      message: t("confirm.closeMessage"),
      confirm: t("confirm.closeConfirm"),
    },
    new: {
      title: t("confirm.newTitle"),
      message: t("confirm.openMessage"), // mismo aviso: se pierde el documento actual
      confirm: t("confirm.newConfirm"),
    },
  })[kind] as { title: string; message: string; confirm: string } & { cancel?: string };

/// Igual que `confirmLabels` pero con el boton Cancelar ya incluido (lo que espera Rust).
const labelsFor = (kind: "open" | "close" | "new") => ({
  ...confirmLabels(kind),
  cancel: t("confirm.cancel"),
});

// Titlebar: nombre del archivo (o "Sin título") + indicador ● de cambios sin guardar.
// Se recuerda el ultimo estado para poder repintar la barra al cambiar de idioma sin
// volver a preguntarle al backend por el documento (el nombre "Sin título" es traducible).
let lastFileName: string | null = null;
let lastDirty = false;

function updateTitlebar(fileName: string | null, dirty: boolean): void {
  lastFileName = fileName;
  lastDirty = dirty;
  const name = fileName || t("doc.untitled");
  docName.textContent = dirty ? `${name} ●` : name;
}

// Repinta la titlebar con el ultimo estado conocido (al cambiar de idioma).
export function refreshTitlebar(): void {
  updateTitlebar(lastFileName, lastDirty);
}

// M4: sincroniza el contenido con el DocumentStore de Rust (calcula dirty) en el
// debounce. El indicador ● no es critico, por eso si falla se ignora en silencio.
export async function syncDoc(): Promise<void> {
  try {
    const st = await invoke<DocStatus>("set_content", { text: getText() });
    updateTitlebar(st.file_name, st.dirty);
  } catch {
    /* no critico */
  }
}

// M8.1: documento nuevo (Ctrl+N). El Rust confirma si hay cambios sin guardar antes de
// descartar el documento actual.
export async function newDoc(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("new_document", { text: getText(), labels: labelsFor("new") });
    if (!st) return; // cancelado (el usuario eligió NO descartar)
    setEditorText("");
    updateTitlebar(st.file_name, st.dirty);
    flashStatus(t("status.newDoc"));
  } catch (err) {
    flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
  }
}

// M4: abrir un .md (dialogo nativo). El Rust confirma si hay cambios sin guardar.
export async function openDoc(): Promise<void> {
  try {
    const opened = await invoke<OpenedDoc | null>("open_document", { labels: labelsFor("open") });
    if (!opened) return; // cancelado (o se eligio NO descartar cambios)
    setEditorText(opened.content);
    updateTitlebar(opened.file_name, false);
    flashStatus(t("status.opened"));
  } catch (err) {
    flashStatus(`${t("error.open")}: ${String(err)}`, 4000);
  }
}

// M4: guardar (escribe al path actual, o abre "guardar como" si es nuevo).
export async function saveDoc(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("save_document", { text: getText() });
    if (!st) return; // cancelado
    updateTitlebar(st.file_name, st.dirty);
    flashStatus(t("status.saved"));
  } catch (err) {
    flashStatus(`${t("error.save")}: ${String(err)}`, 4000);
  }
}

// M4: guardar como (siempre dialogo).
export async function saveDocAs(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("save_document_as", { text: getText() });
    if (!st) return; // cancelado
    updateTitlebar(st.file_name, st.dirty);
    flashStatus(t("status.saved"));
  } catch (err) {
    flashStatus(`${t("error.save")}: ${String(err)}`, 4000);
  }
}

// Abre una ruta concreta: la usan los recientes (M16) y el reenvio de single-instance.
// Reusa open_path, el mismo comando de la asociacion .md: trae incluido el guard que
// confirma antes de descartar cambios sin guardar.
export async function openPath(path: string): Promise<void> {
  try {
    const opened = await invoke<OpenedDoc | null>("open_path", {
      path,
      labels: labelsFor("open"),
    });
    if (!opened) return; // el usuario decidio no descartar sus cambios
    setEditorText(opened.content);
    updateTitlebar(opened.file_name, false);
    flashStatus(t("status.opened"));
  } catch (err) {
    flashStatus(`${t("error.open")}: ${String(err)}`, 4000);
  }
}

// M8.1: guard anti-perdida al CERRAR la ventana. Cubre las 3 vías de cierre: botón X
// (que llama appWindow.close() -> dispara este evento), Alt+F4 y cerrar desde la barra
// de tareas.
//
// SIEMPRE prevenimos el cierre y lo controlamos nosotros: tras confirmar, cerramos con
// appWindow.destroy() EXPLÍCITO. (Antes dependíamos del destroy IMPLÍCITO del wrapper de
// Tauri —cierra al no prevenir—, pero ese camino llama a window|destroy, que necesita el
// permiso core:window:allow-destroy; sin él la ventana quedaba trabada sin cerrar.) El
// destroy explícito hace el cierre determinista; destroy() NO re-dispara onCloseRequested,
// así que no hay loop.
//
// `closeInProgress` colapsa disparos REENTRANTES: el diálogo nativo no es modal (el JS
// event loop sigue vivo durante el await), así que un segundo X / Alt+F4 mientras el
// diálogo está abierto re-dispararía el evento; solo el primer diálogo decide.
let closeInProgress = false;
export function initCloseGuard(): void {
  void appWindow.onCloseRequested(async (event) => {
    event.preventDefault(); // tomamos el control del cierre (evita el destroy implícito de Tauri)
    if (closeInProgress) return; // ya hay un diálogo decidiendo: no apilar otro
    closeInProgress = true;
    let proceed = false;
    try {
      proceed = await invoke<boolean>("confirm_close", { text: getText(), labels: labelsFor("close") });
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

// --- M8.4b: watcher de cambios externos ---

function showExternalChangeBar(): void {
  externalChangeBar.hidden = false;
}

function hideExternalChangeBar(): void {
  externalChangeBar.hidden = true;
}

// Re-lee el archivo abierto desde disco y actualiza el editor. Si el backend devuelve
// un OpenedDoc, el store queda limpio (load_path_into_store hace doc.open).
async function reloadFromDisk(): Promise<void> {
  const opened = await invoke<OpenedDoc | null>("reload_from_disk");
  if (opened) {
    setEditorText(opened.content);
    updateTitlebar(opened.file_name, false); // recargado = limpio
  }
  hideExternalChangeBar();
}

export function initExternalChangeWatcher(): void {
  // Listener de cambio externo: ramifica por estado dirty (decision de producto M8.4b).
  // Decide sobre el dirty REAL del texto vivo: sincroniza getText() con el store via
  // set_content y usa ese dirty, para no recargar y pisar tecleos recientes cuando el
  // doc parece limpio pero aun no paso el debounce (ventana de hasta 250ms).
  void listen("file-changed-on-disk", async () => {
    let dirty: boolean;
    try {
      const st = await invoke<DocStatus>("set_content", { text: getText() });
      dirty = st.dirty;
    } catch {
      dirty = true; // ante la duda, NO recargar de forma destructiva (fail-safe)
    }
    if (!dirty) {
      // Sin cambios locales: recargar directo es seguro (el usuario no pierde nada).
      await reloadFromDisk();
      flashStatus(t("status.changedOnDisk"), 4000);
    } else {
      // Con cambios locales sin guardar: mostrar el infobar (eleccion explicita del usuario).
      showExternalChangeBar();
    }
  });

  // Botones del infobar.
  reloadBtn.addEventListener("click", () => {
    void reloadFromDisk().then(() => flashStatus(t("status.reloaded"), 3000));
  });
  keepBtn.addEventListener("click", () => {
    // El usuario elige mantener sus cambios; el proximo Ctrl+S sobrescribira el cambio externo.
    hideExternalChangeBar();
  });
}

// --- Capa B + M8.4a: arranque ---
// Orden de precedencia: (1) take_pending_file (asociacion .md), (2) take_pending_restore
// (snapshot de crash). Si se abrio via asociacion de archivo, el snapshot NO se restaura.
export async function restoreInitialDocument(): Promise<void> {
  try {
    const pending = await invoke<OpenedDoc | null>("take_pending_file");
    if (pending) {
      // Apertura via asociacion de archivo: el pending file gana; no restaurar snapshot.
      setEditorText(pending.content);
      updateTitlebar(pending.file_name, false);
      return;
    }

    // Sin pending file: verificar si hay un snapshot de una sesion anterior (crash).
    const restored = await invoke<RestoredSnapshot | null>("take_pending_restore");
    if (restored) {
      setEditorText(restored.content);
      // Marcar como modificado de inmediato (el store arranca vacio; el debounce lo
      // marcaria en el primer cambio, pero queremos el indicador visible desde ya).
      updateTitlebar(restored.original_file_name ?? null, true);
      flashStatus(t("status.restored"), 5000);
    }
  } catch {
    /* no critico: si falla, la app queda con el editor vacio */
  }
}

// Listeners de Tauri que alimentan al documento una vez arrancada la app.
export function initDocumentEvents(): void {
  // M8.4a: autosave por snapshots. Cada tick (cada 5 s, emitido por Rust) envia el texto
  // actual al backend para que evalúe si debe escribir o borrar el snapshot. Silencioso:
  // el autosave no es critico y no debe interrumpir al usuario con mensajes de error.
  // Guard de cierre: si ya estamos cerrando, no re-escribir el snapshot que confirm_close
  // va a borrar (evita la race entre el tick y el borrado de confirm_close).
  void listen("autosave-tick", () => {
    if (closeInProgress) return;
    void invoke("write_snapshot", { text: getText() }).catch(() => {});
  });

  // App YA abierta + doble-click en otro .md: single-instance reenvia el path via este
  // evento. open_path aplica el guard anti-perdida (confirma si hay cambios sin guardar).
  void listen<string>("open-file-path", (event) => void openPath(event.payload));
}
