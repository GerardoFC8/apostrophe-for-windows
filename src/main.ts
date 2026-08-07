// Fuentes bundleadas (offline) que clonan el look del GNOME actual (Adwaita):
// Inter (≈ Adwaita Sans) para el chrome, JetBrains Mono para el editor. Se
// empaquetan en el bundle via Vite — NO dependen de lo que tenga instalado Windows.
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
// M11: versión en build-time (sin IPC). tsconfig tiene resolveJsonModule, así que el
// import del package.json es type-safe y lo resuelve Vite al empaquetar.
import pkg from "../package.json";
import { EditorState, Transaction, Compartment } from "@codemirror/state";
import { EditorView, drawSelection, highlightActiveLine, keymap } from "@codemirror/view";
import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { search, searchKeymap, openSearchPanel } from "@codemirror/search";
import { spellcheckExtension, requestSpellCheck, clearSpellSpans } from "./spellcheck";
import { focusCompartment, enableFocusDim, disableFocusDim } from "./focus-mode";
import { markupExtension, requestMarkupHighlight } from "./markup-highlight";
import { getSetting, setSetting } from "./settings";

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
// Reglas del markdown renderizado, parametrizadas por la paleta. Las comparten el
// preview (tema activo) y la exportacion (siempre claro): mismas reglas, distinta
// paleta. Es lo que garantiza que el HTML exportado se vea como el preview.
function markdownStylesheet(v: (name: string) => string): string {
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

function previewStyle(): string {
  const cs = getComputedStyle(document.documentElement);
  return markdownStylesheet((name) => cs.getPropertyValue(name).trim());
}

// Hoja de estilos para EXPORTAR: siempre con la paleta CLARA, sin importar el tema
// activo. Un HTML o un PDF de fondo oscuro es incomodo de leer y de imprimir, y quien
// lo recibe no eligio tu tema. Se lee de un elemento sonda con data-theme="light"
// porque las vars del tema activo estan en <html> y se heredan a todo el arbol.
function exportStyle(): string {
  const probe = document.createElement("div");
  probe.dataset.theme = "light";
  probe.hidden = true;
  document.body.appendChild(probe);
  const cs = getComputedStyle(probe);
  const css = markdownStylesheet((name) => cs.getPropertyValue(name).trim());
  probe.remove();
  return css;
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

// Hereda en este mismo punto el cierre por Esc de los overlays/menú de M11: el cableado
// vive más abajo (closeTopmostChrome), pero el handler de captura de modos (que decide si
// Esc sale de fullscreen) lo integra vía escHasPriorityTarget para respetar la prioridad.

exportBtn.addEventListener("click", () => void exportDoc());

buildThemeMenu();

// --- M11: menú hamburguesa + overlays modales (atajos / acerca de) ---
// Frontend puro (ADR-0001): no toca el dominio ni agrega comandos. Reusa el patrón del
// theme-menu (toggle del hidden + aria-expanded, cierre por clic-afuera y Esc).
const menuTrigger = document.querySelector<HTMLButtonElement>("#menu-trigger")!;
const appMenu = document.querySelector<HTMLElement>("#app-menu")!;
const shortcutsOverlay = document.querySelector<HTMLElement>("#shortcuts-overlay")!;
const aboutOverlay = document.querySelector<HTMLElement>("#about-overlay")!;

function openAppMenu(): void {
  closeThemeMenu(); // solo un popover abierto a la vez
  appMenu.hidden = false;
  menuTrigger.setAttribute("aria-expanded", "true");
}
function closeAppMenu(): void {
  appMenu.hidden = true;
  menuTrigger.setAttribute("aria-expanded", "false");
}

menuTrigger.addEventListener("click", (e) => {
  e.stopPropagation(); // que no llegue al cierre-por-clic-afuera de document
  if (appMenu.hidden) openAppMenu();
  else closeAppMenu();
});
document.addEventListener("click", () => closeAppMenu()); // clic afuera cierra
appMenu.addEventListener("click", (e) => e.stopPropagation()); // clic dentro NO cierra

// Al abrir el theme-menu, cerrar el app-menu (espejo de la lógica inversa en openAppMenu).
themeTrigger.addEventListener("click", () => closeAppMenu());

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

function openShortcuts(): void {
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
function trapTab(e: KeyboardEvent): boolean {
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

// Items del menú.
document.querySelector<HTMLButtonElement>("#menu-shortcuts")?.addEventListener("click", openShortcuts);
document.querySelector<HTMLButtonElement>("#menu-about")?.addEventListener("click", openAbout);

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

// ¿Hay un overlay/menú abierto que Esc deba cerrar PRIMERO? Devuelve el cierre a aplicar,
// o null si no hay nada. El orden refleja la prioridad: overlay modal > popover del menú.
function closeTopmostChrome(): boolean {
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

// M12: exportar (dialogo nativo con un filtro por formato + pandoc sidecar). El formato
// lo decide la extension que elijas en el dialogo; el backend la traduce y avisa si no
// esta soportada.
async function exportDoc(): Promise<void> {
  status.textContent = "exportando…"; // en-progreso (sin auto-limpiar hasta terminar)
  try {
    const out = await invoke<string | null>("export_document", {
      text: getText(),
      css: exportStyle(),
    });
    if (out) flashStatus("exportado");
    else status.textContent = ""; // cancelado
  } catch (err) {
    flashStatus(`error al exportar: ${String(err)}`, 4000);
  }
}

// M9.1: estado del modo enfoque. isFocusMode() lo expone para el spellcheck (focus apaga
// las ondulitas — ADR-0001 D2). El atenuado del texto por parrafo vive en ./focus-mode; el
// toggle (toggleFocusMode) esta mas abajo, junto al resto de los modos de escritura.
let focusMode = false;
const isFocusMode = (): boolean => focusMode;

// M11.1: estado del corrector ortografico (toggle por F7), persistido en settings.json.
// Es la FUENTE DE VERDAD del gating: TODO punto que pida spellcheck (debounce, arranque,
// dict-ready, salida de enfoque) debe respetarlo, para que con el corrector apagado no
// queden ondulitas ni se dispare ningun pedido.
//
// Arranca en FALSE a proposito: el valor real se lee de forma async de la preferencia (ver
// el arranque del spellcheck, mas abajo). Mientras esa lectura vuela, un lector sincrono
// (el debounce, o el listener "spell-dict-ready") podria adelantarsele; con el valor inicial
// en false, en el peor caso el corrector arranca unos ms tarde (invisible, se corrige apenas
// resuelve la lectura), NUNCA pinta una ondulita que la preferencia querria apagada.
let spellcheckEnabled = false;

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
    // M11.1: solo pedir el corrector si esta activado. Si esta apagado no pedimos nada
    // (las ondulitas ya quedaron limpias al togglear); el resaltado es independiente.
    if (spellcheckEnabled) void requestSpellCheck(view, getText(), isFocusMode());
    void requestMarkupHighlight(view, getText()); // M10: resaltado (sigue activo en focus)
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

// --- M9.2: modo Hemingway (Ctrl+T): no puedes borrar lo que escribiste ---
// Paridad con el upstream (apostrophe/text_buffer.py: do_delete_range bloquea el borrado y
// do_insert_text bloquea escribir sobre una seleccion). Lo replicamos con un changeFilter
// que RECHAZA solo las transacciones del USUARIO que remueven texto existente: borrar/cortar
// ("delete"/"delete.cut"), teclear o pegar sobre una seleccion ("input.type"/"input.paste")
// y arrastrar-para-mover ("move.drop", que borra el origen). Lo programatico (abrir/recargar/
// nuevo, formato Markdown, auto-listas) NO lleva userEvent y pasa; undo/redo ("undo"/"redo")
// tampoco se bloquean (el modo intercepta el borrado del usuario, no la pila de historial);
// y Buscar/Reemplazar ("input.replace") se permite: es una edicion intencional, no un borrado.
// userEvents verificados contra @codemirror/{commands,view,search,state} instalados.
let hemingwayMode = false;
let lastHemingwayNotice = 0; // throttle del aviso (evita que se pegue con el autorepeat)
const hemingwayCompartment = new Compartment();
const hemingwayFilter = EditorState.changeFilter.of((tr) => {
  if (!tr.docChanged) return true; // mover el cursor / seleccionar no borra: permitido
  if (tr.isUserEvent("input.replace")) return true; // Reemplazar (M7) es edicion, no borrado
  // Solo se filtra el borrado/movimiento iniciado por el USUARIO.
  if (!(tr.isUserEvent("delete") || tr.isUserEvent("input") || tr.isUserEvent("move.drop"))) {
    return true; // programatico / undo / redo / arrastre-copia (input.drop solo inserta)
  }
  // ¿Remueve texto EXISTENTE? No contamos reemplazos no-op (lo quitado === lo insertado),
  // p.ej. el "skip-over" de un cierre de closeBrackets (teclear ")" sobre un ")" ya puesto).
  let removesText = false;
  tr.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
    if (toA > fromA && tr.startState.sliceDoc(fromA, toA) !== inserted.toString()) {
      removesText = true;
    }
  });
  if (!removesText) return true; // insercion pura o reemplazo no-op
  // Aviso (throttleado): el filtro decide; el feedback es secundario. No reseteamos el
  // timer en cada tecla del autorepeat para que el mensaje no se quede pegado.
  const now = performance.now();
  if (now - lastHemingwayNotice > 1200) {
    flashStatus("El texto no se puede borrar en modo Hemingway");
    lastHemingwayNotice = now;
  }
  // CM6 descarta los cambios (el texto se conserva); la seleccion del usuario colapsa a
  // cursor (changeFilter no puede reescribir la seleccion) — comportamiento aceptado.
  return false;
});

// CodeMirror 6: editor de verdad (undo/redo, line-wrap, cierre de brackets) SIN
// gramatica Lezer (ADR-0001 D1). Los atajos de archivo van a nivel window (arriba).
// Arranca VACIO (distraction-free): sin texto de ejemplo. Si la app se abre via
// asociacion de archivo (.md), el contenido lo carga el arranque (ver mas abajo).
const view = new EditorView({
  doc: "",
  parent: editorParent,
  extensions: [
    history(),
    // M9.2: filtro de Hemingway (arranca vacio; Ctrl+T lo activa).
    hemingwayCompartment.of([]),
    drawSelection(),
    highlightActiveLine(),
    // M9.1: atenuado por parrafo del modo enfoque (arranca vacio; Ctrl+D lo activa).
    focusCompartment.of([]),
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
    // M10: resaltado de Markdown (spans del nucleo Rust). Va tras el spellcheck; sus marks
    // pueden solaparse con las ondulitas y con el atenuado de focus sin conflicto.
    markupExtension,
    EditorView.updateListener.of((update) => {
      if (update.docChanged) scheduleUpdate();
    }),
  ],
});

applyTheme(getInitialTheme()); // fija el tema antes del primer render (sin flash)
void render();
void updateStats();
// M11.1: ARRANQUE SIN PARPADEO. Leemos la preferencia del corrector ANTES del primer
// pedido: si quedo apagado, no pedimos nada y el editor arranca sin ondulitas (sin el
// flash de pintarlas para luego limpiarlas). El spellcheck inicial puede volver [] si el
// diccionario aun se esta cargando en background; cuando termina, Rust emite
// "spell-dict-ready" y re-chequeamos el doc actual — asi no hay que esperar al primer
// tecleo (review M6).
void (async () => {
  spellcheckEnabled = await getSetting("spellcheck");
  if (spellcheckEnabled) void requestSpellCheck(view, getText(), isFocusMode());
})();
void requestMarkupHighlight(view, getText()); // M10: resaltado inicial (al abrir/restaurar)
void listen("spell-dict-ready", () => {
  // Solo re-chequear si el corrector esta activado: si esta apagado, que cargue el dict
  // tarde no debe repintar ondulitas.
  if (spellcheckEnabled) void requestSpellCheck(view, getText(), isFocusMode());
});

// --- M13: control de vista (solo editor / dividido / solo vista) ---
// Tres estados, paridad con el upstream Apostrophe: "editor" (solo el editor), "split"
// (editor + preview lado a lado, el default validado) y "preview" (solo la vista renderizada).
// El CSS de cada estado vive en styles.css (body.view-editor/view-split/view-preview); aqui
// solo se gestiona el estado y se sincroniza la clase del body + el resaltado de los botones.
type ViewMode = "editor" | "split" | "preview";
const VIEW_CLASS: Record<ViewMode, string> = {
  editor: "view-editor",
  split: "view-split",
  preview: "view-preview",
};
const VIEW_ORDER: ViewMode[] = ["editor", "split", "preview"];

let viewMode: ViewMode = "split";
// Recuerda la vista previa al entrar al modo enfoque, para restaurarla al salir (round-trip).
let viewBeforeFocus: ViewMode = "split";
const viewBtns = Array.from(document.querySelectorAll<HTMLButtonElement>("[data-view]"));

// Aplica un modo de vista: una sola fuente de verdad para la clase del body, el estado de los
// botones y el reenfoque del editor. Quita las tres clases y pone solo la del modo actual, asi
// nunca quedan dos activas a la vez.
function setViewMode(m: ViewMode): void {
  viewMode = m;
  document.body.classList.remove("view-editor", "view-split", "view-preview");
  document.body.classList.add(VIEW_CLASS[m]);
  for (const btn of viewBtns) {
    btn.setAttribute("aria-pressed", String(btn.dataset.view === m));
  }
  // Al volver a un modo donde el editor es visible (editor/split), re-enfocarlo: si veniamos de
  // "preview" el editor estaba oculto y habia perdido el foco. En "preview" no enfocamos nada
  // (el editor sigue vivo en el DOM, solo display:none — CM6 no se destruye).
  if (m !== "preview") view.focus();
}

// Cicla editor -> split -> preview -> editor (Ctrl+P, paridad con el upstream). El modo enfoque
// es DUEÑO de la vista (fuerza solo-editor): mientras este activo, ignoramos el ciclo.
function cycleViewMode(): void {
  if (focusMode) return;
  const next = VIEW_ORDER[(VIEW_ORDER.indexOf(viewMode) + 1) % VIEW_ORDER.length];
  setViewMode(next);
}

// Clic directo en los botones de la barra. Igual que el ciclo: el enfoque manda, asi que si esta
// activo el clic no hace nada (el CSS ademas los bloquea con pointer-events: none).
for (const btn of viewBtns) {
  btn.addEventListener("click", () => {
    if (focusMode) return;
    setViewMode(btn.dataset.view as ViewMode);
  });
}

// Estado inicial: split (el default validado). Fija la clase del body y el resaltado del boton.
setViewMode("split");

// --- M9: modos de escritura (enfoque / Hemingway / pantalla completa) ---

// M9.1: alterna el modo enfoque. Activa/desactiva el atenuado por parrafo (./focus-mode),
// togglea la clase body.focus-mode (CSS distraction-free: oculta el preview, centra el
// editor, atenua el chrome) y gestiona el spellcheck (al entrar lo apaga, al salir lo
// repinta — ADR-0001 D2).
function toggleFocusMode(): void {
  focusMode = !focusMode;
  document.body.classList.toggle("focus-mode", focusMode);
  if (focusMode) {
    // El enfoque fuerza solo-editor: recordamos la vista actual para restaurarla al salir (M13).
    viewBeforeFocus = viewMode;
    setViewMode("editor");
    enableFocusDim(view);
    clearSpellSpans(view); // entrar: apaga las ondulitas
  } else {
    disableFocusDim(view);
    setViewMode(viewBeforeFocus); // restaura la vista que habia antes del enfoque (round-trip M13)
    // Salir del enfoque: repinta el corrector SOLO si esta activado. Con el corrector
    // apagado, salir de enfoque NO debe hacer reaparecer ninguna ondulita (M11.1).
    if (spellcheckEnabled) void requestSpellCheck(view, getText(), false);
  }
  flashStatus(focusMode ? "Modo enfoque activado" : "Modo enfoque desactivado", 1500);
}

// M11.1: alterna el corrector ortografico (F7, paridad con el upstream). Persiste el nuevo
// estado en settings.json (fire-and-forget: persistir no es critico) y sincroniza las
// ondulitas: al activarlo re-chequea el doc actual (salvo en modo enfoque, donde el
// corrector se apaga por diseno — ADR-0001 D2); al desactivarlo limpia las ondulitas ya.
function toggleSpellcheck(): void {
  spellcheckEnabled = !spellcheckEnabled;
  void setSetting("spellcheck", spellcheckEnabled);
  if (spellcheckEnabled && !isFocusMode()) {
    void requestSpellCheck(view, getText(), false);
  } else {
    clearSpellSpans(view);
  }
  flashStatus(spellcheckEnabled ? "Corrector activado" : "Corrector desactivado", 1500);
}

// M9.2: alterna el modo Hemingway reconfigurando el compartment del changeFilter en
// caliente (sin recrear el EditorView).
function toggleHemingway(): void {
  hemingwayMode = !hemingwayMode;
  view.dispatch({
    effects: hemingwayCompartment.reconfigure(hemingwayMode ? hemingwayFilter : []),
  });
  flashStatus(
    hemingwayMode
      ? "Modo Hemingway activado — no puedes borrar texto"
      : "Modo Hemingway desactivado",
    1800,
  );
}

// M9.3: pantalla completa (F11). Tauri controla el fullscreen REAL del SO; nosotros
// ocultamos el chrome custom (titlebar + statusbar) con la clase body.is-fullscreen. UNA
// fuente de verdad: syncFullscreenChrome() lee el estado real y pinta el body, asi un toggle
// por F11 y una salida por otra via quedan coherentes (Tauri NO expone evento de fullscreen).
// Requiere core:window:allow-set-fullscreen + allow-is-fullscreen en capabilities/default.json;
// sin ellos setFullscreen falla en silencio (mismo gotcha que allow-destroy con el cierre).
async function syncFullscreenChrome(): Promise<void> {
  try {
    const fs = await appWindow.isFullscreen();
    document.body.classList.toggle("is-fullscreen", fs);
    if (!fs) document.body.classList.remove("reveal-chrome"); // al salir, limpia el hover
  } catch {
    /* no critico: si falla, el chrome simplemente no cambia */
  }
}

// Guard reentrante (mismo patron que closeInProgress en el cierre de ventana): isFullscreen
// es async, asi que dos cambios solapados (doble F11, autorepeat, o F11+Esc) leerian el MISMO
// estado viejo y se pisarian, dejando el fullscreen "pegado". applyFullscreen serializa TODOS
// los caminos (F11 togglea con target=undefined; Esc fuerza target=false) bajo el mismo guard.
let fullscreenInProgress = false;
async function applyFullscreen(target?: boolean): Promise<void> {
  if (fullscreenInProgress) return;
  fullscreenInProgress = true;
  try {
    const next = target ?? !(await appWindow.isFullscreen());
    await appWindow.setFullscreen(next);
  } catch {
    flashStatus("no se pudo cambiar a pantalla completa", 3000);
  } finally {
    fullscreenInProgress = false;
  }
  await syncFullscreenChrome();
}

// Entrar/salir de fullscreen SIEMPRE redimensiona la ventana; onResized cubre cualquier
// salida fuera de banda (Tauri no tiene un evento de fullscreen dedicado). Debounce: el
// resize emite en rafaga al arrastrar el borde; sin debounce serian decenas de IPC
// isFullscreen() por gesto. El cambio real (entrar/salir) lo aplica de inmediato
// applyFullscreen (F11/Esc); aqui solo cubrimos la salida fuera de banda, ~150ms de retardo basta.
let fsResizeTimer: number | undefined;
void appWindow.onResized(() => {
  if (fsResizeTimer !== undefined) clearTimeout(fsResizeTimer);
  fsResizeTimer = window.setTimeout(() => void syncFullscreenChrome(), 150);
});

// Reveal-on-hover del borde superior: en fullscreen, al acercar el mouse arriba reaparece la
// titlebar (paridad con el GtkRevealer del upstream) para mover/cerrar la ventana.
const fsHoverZone = document.querySelector<HTMLElement>("#fs-hover-zone");
fsHoverZone?.addEventListener("mouseenter", () => {
  if (document.body.classList.contains("is-fullscreen")) {
    document.body.classList.add("reveal-chrome");
  }
});
document.querySelector(".titlebar")?.addEventListener("mouseleave", () => {
  document.body.classList.remove("reveal-chrome");
});

// ¿Hay algo que Esc deba cerrar ANTES de salir de fullscreen? (paridad con el upstream, que
// no oculta el chrome mientras la busqueda o un aviso esten visibles).
function escHasPriorityTarget(): boolean {
  if (!themeMenu.hidden) return true; // el menu de tema tiene su propio Esc (mas abajo)
  if (!appMenu.hidden) return true; // M11: menu hamburguesa abierto
  if (!shortcutsOverlay.hidden || !aboutOverlay.hidden) return true; // M11: overlay modal abierto
  if (view.dom.querySelector(".cm-search")) return true; // panel de busqueda CM6 abierto
  if (document.querySelector(".spell-menu")) return true; // menu de sugerencias del corrector
  return false;
}

// Atajos de los MODOS DE ESCRITURA a nivel WINDOW en fase CAPTURA. Captura (no bubble como el
// handler de archivos de mas arriba) para ganarle a CodeMirror y a WebView2: los toggles
// andan con o sin foco en el editor y no chocan con ningun binding del editor. En Windows
// Ctrl+D/Ctrl+T no estan tomados por CM6 (sus bindings emacs son solo-Mac), pero capturamos
// igual por robustez y consistencia.
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
