// Fuentes bundleadas (offline) que clonan el look del GNOME actual (Adwaita):
// Inter (≈ Adwaita Sans) para el chrome, JetBrains Mono para el editor. Se
// empaquetan en el bundle via Vite — NO dependen de lo que tenga instalado Windows.
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { EditorState, Transaction } from "@codemirror/state";
import { EditorView, drawSelection, highlightActiveLine, keymap } from "@codemirror/view";
import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { search, searchKeymap, openSearchPanel } from "@codemirror/search";
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
void appWindow.onCloseRequested(async (event) => {
  event.preventDefault(); // tomamos el control del cierre (evita el destroy implícito de Tauri)
  if (closeInProgress) return; // ya hay un diálogo decidiendo: no apilar otro
  closeInProgress = true;
  let proceed = false;
  try {
    proceed = await invoke<boolean>("confirm_close", { text: getText() });
  } catch {
    // ante un error en el chequeo NO cerramos silenciosamente (podríamos perder datos):
    // dejamos la ventana abierta y lo hacemos VISIBLE para que el usuario guarde a mano.
    flashStatus("no se pudo verificar los cambios — guarda con Ctrl+S antes de cerrar", 4000);
    closeInProgress = false;
    return;
  }
  if (proceed) {
    try {
      await appWindow.destroy(); // cierre forzado y explícito (no re-dispara onCloseRequested)
    } catch {
      // si el cierre fallara, no dejar la ventana trabada: permitir reintentar.
      closeInProgress = false;
      flashStatus("no se pudo cerrar la ventana — reintenta", 4000);
    }
  } else {
    closeInProgress = false; // el usuario canceló: permitir un futuro intento de cierre
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

// --- M7: traducciones del panel de busqueda de @codemirror/search (castellano neutro) ---
// EditorState.phrases.of({}) registra un mapa de traduccion; state.phrase() lo consulta.
// Cadenas verificadas contra el codigo fuente de @codemirror/search 6.7.0.
const phrasesES = EditorState.phrases.of({
  "Find": "Buscar",
  "Replace": "Reemplazar",
  "next": "siguiente",
  "previous": "anterior",
  "all": "todo",
  "match case": "distinguir mayúsculas",
  "by word": "palabra completa",
  "regexp": "expresión regular",
  "replace": "reemplazar",
  "replace all": "reemplazar todo",
  "close": "cerrar",
  "current match": "coincidencia actual",
  "replaced $ matches": "$ coincidencias reemplazadas",
  "replaced match on line $": "coincidencia reemplazada en la línea $",
  "on line": "en la línea",
  "Go to line": "Ir a la línea",
  "go": "ir",
});

// --- M7: comandos de formato Markdown (CodeMirror commands) ---
//
// Fuente de atajos: /tmp/apostrophe-upstream/data/ui/Shortcuts.ui y
// text_view_format_inserter.py del upstream. Paridad con el original GNOME:
//   Negrita    → Ctrl+B  — marcador **  (igual al upstream)
//   Cursiva    → Ctrl+I  — marcador _   (underscore, igual al upstream)
//   Tachado    → Ctrl+Shift+D — marcador ~~ (upstream usa <Shift><Primary>D)
//   Lista      → Ctrl+U  — prefijo "- " (igual al upstream)
//   Encabezado → Ctrl+1..Ctrl+4 — fija nivel (upstream: "Header <Primary>1 4")
//   Buscar     → Ctrl+F  (searchKeymap, igual al upstream "win.find")
//   B+Reemplazar → Ctrl+H — binding adicional (upstream: "win.find_replace")

/** Envuelve/desenvuelve la seleccion con un marcador inline (negrita, cursiva, tachado).
 *  Si no hay seleccion, inserta los dos marcadores y deja el cursor en el medio. */
function toggleInlineFormat(view: EditorView, marker: string): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const markerLen = marker.length;

  if (sel.empty) {
    // Sin seleccion: inserta el marcador doble y posiciona el cursor en el medio.
    const insert = marker + marker;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert },
      selection: { anchor: sel.from + markerLen },
    });
    return true;
  }

  const selectedText = state.sliceDoc(sel.from, sel.to);

  // Comprueba si la seleccion YA esta envuelta (busca tambien fuera de la seleccion).
  const extFrom = Math.max(0, sel.from - markerLen);
  const extTo   = Math.min(state.doc.length, sel.to + markerLen);
  const extText = state.sliceDoc(extFrom, extTo);

  const outerWrapped =
    extText.startsWith(marker) && extText.endsWith(marker) && extFrom < sel.from;
  const innerWrapped =
    selectedText.startsWith(marker) && selectedText.endsWith(marker) &&
    selectedText.length >= markerLen * 2;

  if (outerWrapped) {
    // Quita los marcadores que estan FUERA de la seleccion.
    const inner = extText.slice(markerLen, extText.length - markerLen);
    view.dispatch({
      changes: { from: extFrom, to: extTo, insert: inner },
      selection: { anchor: extFrom, head: extFrom + inner.length },
    });
    return true;
  }

  if (innerWrapped) {
    // Quita los marcadores que estan DENTRO de la seleccion.
    const inner = selectedText.slice(markerLen, selectedText.length - markerLen);
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: inner },
      selection: { anchor: sel.from, head: sel.from + inner.length },
    });
    return true;
  }

  // Envuelve la seleccion con el marcador.
  const wrapped = marker + selectedText + marker;
  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert: wrapped },
    selection: { anchor: sel.from + markerLen, head: sel.from + markerLen + selectedText.length },
  });
  return true;
}

/** Fija o quita el nivel de encabezado en la(s) linea(s) de la seleccion.
 *  Si la linea ya tiene ese nivel, lo quita (toggle). Sigue la semantica del upstream:
 *  al repetir el mismo nivel se elimina el encabezado. */
function setHeadingLevel(view: EditorView, level: number): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const startLine = state.doc.lineAt(sel.from);
  const endLine   = state.doc.lineAt(sel.to);

  const changes: { from: number; to: number; insert: string }[] = [];

  for (let ln = startLine.number; ln <= endLine.number; ln++) {
    const line = state.doc.line(ln);
    const text  = line.text;
    const match = text.match(/^(#{1,6}) /);

    if (match) {
      const currentLevel = match[1].length;
      if (currentLevel === level) {
        // Mismo nivel → eliminar encabezado (toggle off).
        changes.push({ from: line.from, to: line.from + currentLevel + 1, insert: "" });
      } else {
        // Cambiar a nuevo nivel.
        changes.push({
          from: line.from,
          to: line.from + currentLevel + 1,
          insert: "#".repeat(level) + " ",
        });
      }
    } else {
      // Sin encabezado → agregar.
      changes.push({ from: line.from, to: line.from, insert: "#".repeat(level) + " " });
    }
  }

  view.dispatch({ changes });
  return true;
}

/** Agrega/quita el prefijo de lista desordenada ("- ") en la(s) linea(s) seleccionadas.
 *  Sigue la semantica del upstream: si la linea ya tiene prefijo de lista, lo quita. */
function toggleList(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const startLine = state.doc.lineAt(sel.from);
  const endLine   = state.doc.lineAt(sel.to);

  const changes: { from: number; to: number; insert: string }[] = [];

  for (let ln = startLine.number; ln <= endLine.number; ln++) {
    const line = state.doc.line(ln);
    const text  = line.text;
    // Patron de lista desordenada: "- ", "* " o "+ " con posible sangria.
    const listMatch = text.match(/^(\s*)([-*+] )/);
    if (listMatch) {
      // Quitar el prefijo "- " (los 2 chars del marcador).
      const prefixStart = line.from + listMatch[1].length;
      changes.push({ from: prefixStart, to: prefixStart + 2, insert: "" });
    } else {
      // Agregar "- " al inicio de la linea (sin sangria, como el upstream).
      changes.push({ from: line.from, to: line.from, insert: "- " });
    }
  }

  view.dispatch({ changes });
  return true;
}

/** Auto-continuacion de listas al pulsar Enter.
 *  - Si la linea actual es un item de lista CON contenido → inserta nueva linea + prefijo.
 *  - Si el item esta VACIO (solo el prefijo) → elimina el prefijo y termina la lista.
 *  - Si no es lista → devuelve false para que actue el Enter por defecto.
 *  Soporta listas desordenadas (- / * / +) y ordenadas (N. / N)).
 *  Semantica verificada contra text_view_markup_handler.py del upstream. */
function handleListEnter(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  if (!sel.empty) return false; // con seleccion, deja actuar al Enter por defecto

  const line = state.doc.lineAt(sel.from);
  const text  = line.text;

  // Lista desordenada: "  - texto", "  * texto", "  + texto"
  const unorderedMatch = text.match(/^(\s*)([-*+]) (.*)/);
  // Lista ordenada: "  1. texto", "  2) texto"
  const orderedMatch   = text.match(/^(\s*)(\d+)([.)]) (.*)/);

  if (unorderedMatch) {
    const indent  = unorderedMatch[1];
    const symbol  = unorderedMatch[2];
    const content = unorderedMatch[3];
    const prefixLen = indent.length + 2; // simbolo (1) + espacio (1)

    if (content.trim() === "") {
      // Item vacio: terminar la lista SOLO si el cursor esta despues del prefijo
      // completo. Si esta dentro del prefijo (pos 0/1), dejar el Enter por defecto
      // para no producir un dispatch vacio ni borrar a medias el prefijo.
      if (sel.from < line.from + prefixLen) return false;
      // Eliminar el prefijo (termina la lista).
      view.dispatch({
        changes: { from: line.from, to: sel.from, insert: "" },
        scrollIntoView: true,
      });
      return true;
    }
    // Item con contenido: nueva linea + mismo prefijo.
    view.dispatch({
      changes: { from: sel.from, to: sel.from, insert: `\n${indent}${symbol} ` },
      selection: { anchor: sel.from + indent.length + 3 },
      scrollIntoView: true,
    });
    return true;
  }

  if (orderedMatch) {
    const indent    = orderedMatch[1];
    const number    = parseInt(orderedMatch[2], 10);
    const delimiter = orderedMatch[3];
    const content   = orderedMatch[4];
    const prefixLen = indent.length + orderedMatch[2].length + 2; // numero + delim (1) + espacio (1)

    if (content.trim() === "") {
      // Item vacio: terminar la lista SOLO si el cursor esta despues del prefijo
      // completo (ver nota en la rama desordenada).
      if (sel.from < line.from + prefixLen) return false;
      // Eliminar el prefijo (termina la lista).
      view.dispatch({
        changes: { from: line.from, to: sel.from, insert: "" },
        scrollIntoView: true,
      });
      return true;
    }
    // Item con contenido: nueva linea + numero incrementado.
    const nextPrefix = `${indent}${number + 1}${delimiter} `;
    view.dispatch({
      changes: { from: sel.from, to: sel.from, insert: `\n${nextPrefix}` },
      selection: { anchor: sel.from + nextPrefix.length + 1 },
      scrollIntoView: true,
    });
    return true;
  }

  return false; // no es linea de lista: Enter por defecto
}

// Keymap de formato Markdown (alta prioridad: va ANTES de defaultKeymap).
// Ctrl+F y Ctrl+H los maneja searchKeymap; aqui solo van los de formato.
const formatKeymap = keymap.of([
  // Negrita: Ctrl+B — marcador ** (upstream: <Primary>b)
  { key: "Mod-b", run: (v) => toggleInlineFormat(v, "**"), preventDefault: true },
  // Cursiva: Ctrl+I — marcador _ (underscore, como el upstream)
  { key: "Mod-i", run: (v) => toggleInlineFormat(v, "_"), preventDefault: true },
  // Tachado: Ctrl+Shift+D — marcador ~~ (upstream: <Shift><Primary>D)
  { key: "Mod-Shift-d", run: (v) => toggleInlineFormat(v, "~~"), preventDefault: true },
  // Lista desordenada: Ctrl+U — prefijo "- " (upstream: <Primary>u)
  { key: "Mod-u", run: toggleList, preventDefault: true },
  // Encabezados Ctrl+1..Ctrl+4 (upstream: "<Primary>1 4" = cicla niveles 1-4)
  { key: "Mod-1", run: (v) => setHeadingLevel(v, 1), preventDefault: true },
  { key: "Mod-2", run: (v) => setHeadingLevel(v, 2), preventDefault: true },
  { key: "Mod-3", run: (v) => setHeadingLevel(v, 3), preventDefault: true },
  { key: "Mod-4", run: (v) => setHeadingLevel(v, 4), preventDefault: true },
  // Auto-continuacion de listas al Enter (alta prioridad sobre defaultKeymap).
  { key: "Enter", run: handleListEnter },
  // Ctrl+H: abrir buscar+reemplazar (searchKeymap solo incluye Ctrl+F; H lo agregamos aqui).
  { key: "Mod-h", run: openSearchPanel, preventDefault: true },
]);

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
    // Traducciones del panel de busqueda al castellano.
    phrasesES,
    // Panel de busqueda y reemplazo (arriba del editor, top:true).
    search({ top: true }),
    // Keymaps en orden de prioridad: busqueda > formato MD > brackets > default > historial.
    // searchKeymap primero: Ctrl+F y Ctrl+G tienen prioridad sobre cualquier otro.
    keymap.of([...searchKeymap]),
    // Comandos de formato Markdown (Ctrl+B/I, Ctrl+Shift+D, Ctrl+U, Ctrl+1-4, Enter).
    // Va antes de defaultKeymap para que el Enter de listas tenga prioridad.
    formatKeymap,
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
  if (closeInProgress) return;
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
