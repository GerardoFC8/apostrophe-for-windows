// Fuentes bundleadas (offline) que clonan el look del GNOME actual (Adwaita):
// Inter (≈ Adwaita Sans) para el chrome, JetBrains Mono para el editor. Se
// empaquetan en el bundle via Vite — NO dependen de lo que tenga instalado Windows.
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { Transaction } from "@codemirror/state";
import { EditorView, drawSelection, highlightActiveLine, keymap } from "@codemirror/view";
import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { spellcheckExtension, requestSpellCheck } from "./spellcheck";

// M3: editor CodeMirror 6. M4: abrir/guardar .md. M5: temas light/dark/sepia +
// export a HTML. La logica esta en Rust; la WebView solo PRESENTA. La frontera IPC
// es gruesa y con debounce.
//
// ADR-0001 D1: el highlighting NO se hace con la gramatica Lezer de CodeMirror —
// vendra del nucleo Rust como spans (post-MVP). Por eso NO hay @codemirror/lang-markdown.
const DEBOUNCE_MS = 250;

// --- Contratos del IPC con el nucleo ---
interface ReadTime {
  hours: number;
  minutes: number;
  seconds: number;
}
interface Stats {
  characters: number;
  words: number;
  sentences: number;
  paragraphs: number;
  // snake_case: serde serializa con el nombre del campo de Rust (read_time).
  read_time: ReadTime;
}
interface OpenedDoc {
  path: string;
  file_name: string;
  content: string;
}
interface DocStatus {
  file_name: string | null;
  dirty: boolean;
}

const editorParent = document.querySelector<HTMLElement>("#editor")!;
const preview = document.querySelector<HTMLIFrameElement>("#preview")!;
const status = document.querySelector<HTMLElement>("#status")!;
const docName = document.querySelector<HTMLElement>("#doc-name")!;
const themeTrigger = document.querySelector<HTMLButtonElement>("#theme-trigger")!;
const themeTriggerIcon = document.querySelector<HTMLElement>("#theme-trigger-icon")!;
const themeTriggerLabel = document.querySelector<HTMLElement>("#theme-trigger-label")!;
const themeMenu = document.querySelector<HTMLElement>("#theme-menu")!;
const exportBtn = document.querySelector<HTMLButtonElement>("#export-btn")!;
const statWords = document.querySelector<HTMLElement>("#stat-words")!;
const statChars = document.querySelector<HTMLElement>("#stat-chars")!;
const statSentences = document.querySelector<HTMLElement>("#stat-sentences")!;
const statParagraphs = document.querySelector<HTMLElement>("#stat-paragraphs")!;
const statReadTime = document.querySelector<HTMLElement>("#stat-readtime")!;

// M8.4b: infobar de cambio externo.
const externalChangeBar = document.querySelector<HTMLElement>("#external-change-bar")!;
const reloadBtn = document.querySelector<HTMLButtonElement>("#reload-btn")!;
const keepBtn = document.querySelector<HTMLButtonElement>("#keep-btn")!;

// Ultimo HTML crudo de pandoc, para re-tematizar el preview al cambiar de tema sin
// volver a invocar pandoc.
let lastPreviewHtml = "";

// Chrome nativo (M3): titlebar custom. `decorations: false` (tauri.conf.json) saca
// la barra Win32; estos botones manejan la ventana via la window API de Tauri.
const appWindow = getCurrentWindow();
document.querySelector("#win-min")?.addEventListener("click", () => void appWindow.minimize());
document.querySelector("#win-max")?.addEventListener("click", () => void appWindow.toggleMaximize());
document.querySelector("#win-close")?.addEventListener("click", () => void appWindow.close());

// M8.1: guard anti-perdida al CERRAR la ventana. Cubre las 3 vías de cierre: botón X
// (que llama appWindow.close() -> dispara este evento), Alt+F4 y cerrar desde la barra
// de tareas. En Tauri v2 el handler puede ser async y llamar preventDefault() DESPUÉS
// del await (patrón oficial): si NO se previene, la ventana cierra al resolver el
// handler. confirm_close sincroniza el texto actual (los últimos tecleos, antes del
// debounce) y confirma si hay cambios sin guardar.
// `closing` colapsa disparos REENTRANTES: el diálogo nativo no es modal (el JS event
// loop sigue vivo durante el await), así que un segundo X / Alt+F4 mientras el diálogo
// está abierto re-dispararía el evento. Sin el flag, un "Cancelar" en el primer diálogo
// podría quedar anulado por un "Descartar" en uno apilado. Con el flag, solo el primer
// diálogo decide; los reentrantes se previenen. El flag se resetea SOLO al cancelar o
// fallar (la ventana sigue viva); en el camino de éxito queda puesto porque la ventana
// se destruye al resolver el handler.
let closing = false;
void appWindow.onCloseRequested(async (event) => {
  if (closing) {
    event.preventDefault(); // ya hay un diálogo de cierre decidiendo: no apilar otro
    return;
  }
  closing = true;
  try {
    const proceed = await invoke<boolean>("confirm_close", { text: getText() });
    if (!proceed) {
      event.preventDefault(); // el usuario canceló: la ventana NO se cierra
      closing = false; // permitir un futuro intento de cierre
    }
    // proceed === true: NO reseteamos `closing`; la ventana se destruye al resolver.
  } catch {
    // ante un error en el chequeo NO cerramos silenciosamente (podríamos perder datos):
    // prevenimos el cierre y lo hacemos VISIBLE para que el usuario guarde a mano.
    event.preventDefault();
    closing = false;
    flashStatus("no se pudo verificar los cambios — guarda con Ctrl+S antes de cerrar", 4000);
  }
});

// M4/M5: atajos de archivo a nivel WINDOW (no solo en CM6) — asi funcionan aunque el
// foco NO este en el editor. Shift+S se chequea ANTES que S sola.
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

// --- M5: temas (light / dark / sepia) ---
const THEMES = ["light", "dark", "sepia"] as const;
type Theme = (typeof THEMES)[number];
const THEME_LABEL: Record<Theme, string> = { light: "Claro", dark: "Oscuro", sepia: "Sepia" };

// Iconos SVG (stroke = currentColor, heredan el color del tema). Sol / luna / libro.
const ICON: Record<Theme, string> = {
  light:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"><circle cx="8" cy="8" r="3"/><path d="M8 1.2V2.8M8 13.2V14.8M1.2 8H2.8M13.2 8H14.8M3.4 3.4 4.5 4.5M11.5 11.5 12.6 12.6M12.6 3.4 11.5 4.5M4.5 11.5 3.4 12.6"/></svg>',
  dark:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"><path d="M13.4 9.3A5.6 5.6 0 0 1 6.7 2.6 5.6 5.6 0 1 0 13.4 9.3Z"/></svg>',
  sepia:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"><path d="M8 3.6C6.6 2.7 4.2 2.7 2.7 3.2V12c1.5-.5 3.9-.5 5.3.4M8 3.6c1.4-.9 3.8-.9 5.3-.4V12c-1.5-.5-3.9-.5-5.3.4M8 3.6V12.4"/></svg>',
};

function getInitialTheme(): Theme {
  const saved = localStorage.getItem("theme");
  if (saved === "light" || saved === "dark" || saved === "sepia") return saved;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

// CSS del preview derivado de las CSS vars del tema activo (una sola fuente de verdad).
function previewStyle(): string {
  const cs = getComputedStyle(document.documentElement);
  const v = (name: string) => cs.getPropertyValue(name).trim();
  return `<style>
    html,body{background:${v("--preview-bg")};color:${v("--preview-fg")};}
    body{font-family:"Inter Variable",system-ui,sans-serif;line-height:1.6;max-width:46em;margin:0 auto;padding:1.6rem 2rem;}
    a{color:${v("--accent")};}
    code,pre{background:${v("--preview-code-bg")};border-radius:6px;font-family:"JetBrains Mono Variable",ui-monospace,monospace;}
    code{padding:.1em .35em;}
    pre{padding:.7em .9em;overflow:auto;}
    pre code{background:none;padding:0;}
    blockquote{border-left:3px solid ${v("--border")};margin-left:0;padding-left:1em;opacity:.85;}
    h1,h2,h3{line-height:1.25;}
    img{max-width:100%;}
  </style>`;
}

// Inyecta el <style> del tema en el HTML standalone de pandoc (inline -> anda en el
// iframe srcdoc sandboxeado).
function withTheme(html: string): string {
  return html.includes("</head>")
    ? html.replace("</head>", `${previewStyle()}</head>`)
    : previewStyle() + html;
}

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem("theme", theme);
  // Reflejar en el trigger del selector y marcar el item activo del menu.
  themeTriggerIcon.innerHTML = ICON[theme];
  themeTriggerLabel.textContent = THEME_LABEL[theme];
  themeMenu.querySelectorAll<HTMLElement>(".theme-item").forEach((item) => {
    item.setAttribute("aria-checked", String(item.dataset.theme === theme));
  });
  // Re-tematizar el preview ya renderizado (editor/chrome se actualizan solos via vars).
  if (lastPreviewHtml) preview.srcdoc = withTheme(lastPreviewHtml);
}

// Construye el menu del selector: un item por tema (icono + label + check del activo).
function buildThemeMenu(): void {
  themeMenu.innerHTML = THEMES.map(
    (t) =>
      `<button class="theme-item" type="button" role="menuitemradio" data-theme="${t}" aria-checked="false">` +
      `<span class="theme-item-icon">${ICON[t]}</span><span>${THEME_LABEL[t]}</span>` +
      '<svg class="check" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 6.4 L5 8.9 L9.5 3.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>' +
      "</button>"
  ).join("");
  themeMenu.querySelectorAll<HTMLButtonElement>(".theme-item").forEach((item) => {
    item.addEventListener("click", () => {
      applyTheme(item.dataset.theme as Theme);
      closeThemeMenu();
    });
  });
}

function openThemeMenu(): void {
  themeMenu.hidden = false;
  themeTrigger.setAttribute("aria-expanded", "true");
}
function closeThemeMenu(): void {
  themeMenu.hidden = true;
  themeTrigger.setAttribute("aria-expanded", "false");
}

themeTrigger.addEventListener("click", (e) => {
  e.stopPropagation(); // que no llegue al cierre-por-click-afuera de document
  if (themeMenu.hidden) openThemeMenu();
  else closeThemeMenu();
});
document.addEventListener("click", () => closeThemeMenu()); // click afuera cierra
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeThemeMenu();
});

exportBtn.addEventListener("click", () => void exportDoc());

buildThemeMenu();

// Tema del editor (Adwaita-ish): hereda los colores via CSS vars (cambian con data-theme),
// tipografia JetBrains Mono. El acento es el azul Adwaita.
const editorTheme = EditorView.theme({
  "&": { height: "100%", color: "var(--editor-fg)" },
  ".cm-scroller": {
    fontFamily: '"JetBrains Mono Variable", "Cascadia Code", ui-monospace, monospace',
    fontSize: "14px",
    lineHeight: "1.6",
  },
  ".cm-content": { padding: "1rem", caretColor: "var(--accent)" },
  "&.cm-focused": { outline: "none" },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--accent)",
    borderLeftWidth: "2px",
  },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
    backgroundColor: "var(--selection)",
  },
  ".cm-activeLine": { backgroundColor: "var(--active-line)" },
});

let timer: number | undefined;

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

function formatReadTime(rt: ReadTime): string {
  const parts: string[] = [];
  if (rt.hours) parts.push(`${rt.hours} h`);
  if (rt.minutes) parts.push(`${rt.minutes} min`);
  parts.push(`${rt.seconds} s`);
  return `${parts.join(" ")} de lectura`;
}

// Escapa HTML para no inyectar markup al mostrar errores en el iframe del preview.
function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => (c === "&" ? "&amp;" : c === "<" ? "&lt;" : "&gt;"));
}

function getText(): string {
  return view.state.doc.toString();
}

// Reemplaza TODO el contenido del editor (al abrir un archivo). `addToHistory: false`
// para que Ctrl+Z tras abrir NO revierta al doc anterior (desincronizaria editor/store).
function setEditorText(text: string): void {
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
    annotations: Transaction.addToHistory.of(false),
  });
}

// Titlebar: nombre del archivo (o "Sin título") + indicador ● de cambios sin guardar.
function updateTitlebar(fileName: string | null, dirty: boolean): void {
  const name = fileName || "Sin título";
  docName.textContent = dirty ? `${name} ●` : name;
}

// Estado TRANSITORIO en la titlebar: muestra feedback de accion (guardado, exportado,
// error...) y se auto-limpia. En reposo la barra queda limpia (no un "listo" permanente),
// coherente con el ethos distraction-free de Apostrophe.
let statusTimer: number | undefined;
function flashStatus(msg: string, ms = 1800): void {
  status.textContent = msg;
  if (statusTimer !== undefined) clearTimeout(statusTimer);
  statusTimer = window.setTimeout(() => {
    status.textContent = "";
  }, ms);
}

async function render(): Promise<void> {
  // El preview que se actualiza YA es el feedback; no se ensucia la barra con
  // "convirtiendo…"/"listo". Solo se avisa si falla (ademas del error en el panel).
  try {
    const html = await invoke<string>("preview_markdown", { text: getText() });
    lastPreviewHtml = html;
    preview.srcdoc = withTheme(html);
  } catch (err) {
    flashStatus("error de preview", 4000);
    preview.srcdoc = `<pre style="color:#c00;font-family:monospace;padding:1rem">${escapeHtml(String(err))}</pre>`;
  }
}

async function updateStats(): Promise<void> {
  try {
    const s = await invoke<Stats>("count_stats", { text: getText() });
    statWords.textContent = plural(s.words, "palabra", "palabras");
    statChars.textContent = plural(s.characters, "carácter", "caracteres");
    statSentences.textContent = plural(s.sentences, "oración", "oraciones");
    statParagraphs.textContent = plural(s.paragraphs, "párrafo", "párrafos");
    statReadTime.textContent = formatReadTime(s.read_time);
  } catch (err) {
    statWords.textContent = `error: ${String(err)}`;
  }
}

// M4: sincroniza el contenido con el DocumentStore de Rust (calcula dirty) en el
// debounce. El indicador ● no es critico, por eso si falla se ignora en silencio.
async function syncDoc(): Promise<void> {
  try {
    const st = await invoke<DocStatus>("set_content", { text: getText() });
    updateTitlebar(st.file_name, st.dirty);
  } catch {
    /* no critico */
  }
}

// M8.1: documento nuevo (Ctrl+N). El Rust confirma si hay cambios sin guardar antes de
// descartar el documento actual.
async function newDoc(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("new_document", { text: getText() });
    if (!st) return; // cancelado (el usuario eligió NO descartar)
    setEditorText("");
    updateTitlebar(st.file_name, st.dirty);
    flashStatus("documento nuevo");
  } catch (err) {
    flashStatus(`error: ${String(err)}`, 4000);
  }
}

// M4: abrir un .md (dialogo nativo). El Rust confirma si hay cambios sin guardar.
async function openDoc(): Promise<void> {
  try {
    const opened = await invoke<OpenedDoc | null>("open_document");
    if (!opened) return; // cancelado (o se eligio NO descartar cambios)
    setEditorText(opened.content);
    updateTitlebar(opened.file_name, false);
    flashStatus("abierto");
  } catch (err) {
    flashStatus(`error al abrir: ${String(err)}`, 4000);
  }
}

// M4: guardar (escribe al path actual, o abre "guardar como" si es nuevo).
async function saveDoc(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("save_document", { text: getText() });
    if (!st) return; // cancelado
    updateTitlebar(st.file_name, st.dirty);
    flashStatus("guardado");
  } catch (err) {
    flashStatus(`error al guardar: ${String(err)}`, 4000);
  }
}

// M4: guardar como (siempre dialogo).
async function saveDocAs(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("save_document_as", { text: getText() });
    if (!st) return; // cancelado
    updateTitlebar(st.file_name, st.dirty);
    flashStatus("guardado");
  } catch (err) {
    flashStatus(`error al guardar: ${String(err)}`, 4000);
  }
}

// M5: exportar a HTML (dialogo "guardar como .html" + pandoc sidecar).
async function exportDoc(): Promise<void> {
  status.textContent = "exportando…"; // en-progreso (sin auto-limpiar hasta terminar)
  try {
    const out = await invoke<string | null>("export_document", { text: getText() });
    if (out) flashStatus("exportado");
    else status.textContent = ""; // cancelado
  } catch (err) {
    flashStatus(`error al exportar: ${String(err)}`, 4000);
  }
}

// M6: focus mode todavia no existe como feature; el hook queda cableado (WU6). Cuando
// exista, isFocusMode() devuelve true y el spellcheck se auto-desactiva (ADR-0001 D2).
const isFocusMode = (): boolean => false;

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
    flashStatus("El archivo cambió en disco — recargado", 4000);
  } else {
    // Con cambios locales sin guardar: mostrar el infobar (eleccion explicita del usuario).
    showExternalChangeBar();
  }
});

// Botones del infobar.
reloadBtn.addEventListener("click", () => {
  void reloadFromDisk().then(() => flashStatus("Archivo recargado desde disco", 3000));
});
keepBtn.addEventListener("click", () => {
  // El usuario elige mantener sus cambios; el proximo Ctrl+S sobrescribira el cambio externo.
  hideExternalChangeBar();
});

function scheduleUpdate(): void {
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(() => {
    void render();
    void updateStats();
    void syncDoc();
    void requestSpellCheck(view, getText(), isFocusMode());
  }, DEBOUNCE_MS);
}

// CodeMirror 6: editor de verdad (undo/redo, line-wrap, cierre de brackets) SIN
// gramatica Lezer (ADR-0001 D1). Los atajos de archivo van a nivel window (arriba).
// Arranca VACIO (distraction-free): sin texto de ejemplo. Si la app se abre via
// asociacion de archivo (.md), el contenido lo carga el arranque (ver mas abajo).
const view = new EditorView({
  doc: "",
  parent: editorParent,
  extensions: [
    history(),
    drawSelection(),
    highlightActiveLine(),
    EditorView.lineWrapping,
    closeBrackets(),
    keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap]),
    editorTheme,
    spellcheckExtension,
    EditorView.updateListener.of((update) => {
      if (update.docChanged) scheduleUpdate();
    }),
  ],
});

applyTheme(getInitialTheme()); // fija el tema antes del primer render (sin flash)
void render();
void updateStats();
// Spellcheck inicial (puede volver [] si el diccionario aun se esta cargando en
// background). Cuando el dict termina de cargar, Rust emite "spell-dict-ready" y
// re-chequeamos el doc actual — asi no hay que esperar al primer tecleo (review M6).
void requestSpellCheck(view, getText(), isFocusMode());
void listen("spell-dict-ready", () => {
  void requestSpellCheck(view, getText(), isFocusMode());
});

// M8.4a: contrato del snapshot restaurado (espejo de RestoredSnapshot de Rust).
interface RestoredSnapshot {
  content: string;
  original_file_name: string | null;
}

// --- Capa B + M8.4a: arranque ---
// Orden de precedencia: (1) take_pending_file (asociacion .md), (2) take_pending_restore
// (snapshot de crash). Si se abrio via asociacion de archivo, el snapshot NO se restaura.
void (async () => {
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
      flashStatus("Se restauró trabajo no guardado de la sesión anterior", 5000);
    }
  } catch {
    /* no critico: si falla, la app queda con el editor vacio */
  }
})();

// M8.4a: autosave por snapshots. Cada tick (cada 5 s, emitido por Rust) envia el texto
// actual al backend para que evalúe si debe escribir o borrar el snapshot. Silencioso:
// el autosave no es critico y no debe interrumpir al usuario con mensajes de error.
// Guard de cierre: si ya estamos cerrando, no re-escribir el snapshot que confirm_close
// va a borrar (evita la race entre el tick y el borrado de confirm_close).
void listen("autosave-tick", () => {
  if (closing) return;
  void invoke("write_snapshot", { text: getText() }).catch(() => {});
});

// App YA abierta + doble-click en otro .md: single-instance reenvia el path via este
// evento. open_path aplica el guard anti-perdida (confirma si hay cambios sin guardar).
void listen<string>("open-file-path", async (event) => {
  try {
    const opened = await invoke<OpenedDoc | null>("open_path", { path: event.payload });
    if (!opened) return; // cancelado por el guard de cambios sin guardar
    setEditorText(opened.content);
    updateTitlebar(opened.file_name, false);
    flashStatus("abierto");
  } catch (err) {
    flashStatus(`error al abrir: ${String(err)}`, 4000);
  }
});
