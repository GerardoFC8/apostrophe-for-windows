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

// Ultimo HTML crudo de pandoc, para re-tematizar el preview al cambiar de tema sin
// volver a invocar pandoc.
let lastPreviewHtml = "";

// Chrome nativo (M3): titlebar custom. `decorations: false` (tauri.conf.json) saca
// la barra Win32; estos botones manejan la ventana via la window API de Tauri.
const appWindow = getCurrentWindow();
document.querySelector("#win-min")?.addEventListener("click", () => void appWindow.minimize());
document.querySelector("#win-max")?.addEventListener("click", () => void appWindow.toggleMaximize());
document.querySelector("#win-close")?.addEventListener("click", () => void appWindow.close());

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

async function render(): Promise<void> {
  status.textContent = "convirtiendo…";
  try {
    const html = await invoke<string>("preview_markdown", { text: getText() });
    lastPreviewHtml = html;
    preview.srcdoc = withTheme(html);
    status.textContent = "listo";
  } catch (err) {
    status.textContent = "error";
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

// M4: abrir un .md (dialogo nativo). El Rust confirma si hay cambios sin guardar.
async function openDoc(): Promise<void> {
  try {
    const opened = await invoke<OpenedDoc | null>("open_document");
    if (!opened) return; // cancelado (o se eligio NO descartar cambios)
    setEditorText(opened.content);
    updateTitlebar(opened.file_name, false);
    status.textContent = "abierto";
  } catch (err) {
    status.textContent = `error al abrir: ${String(err)}`;
  }
}

// M4: guardar (escribe al path actual, o abre "guardar como" si es nuevo).
async function saveDoc(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("save_document", { text: getText() });
    if (!st) return; // cancelado
    updateTitlebar(st.file_name, st.dirty);
    status.textContent = "guardado";
  } catch (err) {
    status.textContent = `error al guardar: ${String(err)}`;
  }
}

// M4: guardar como (siempre dialogo).
async function saveDocAs(): Promise<void> {
  try {
    const st = await invoke<DocStatus | null>("save_document_as", { text: getText() });
    if (!st) return; // cancelado
    updateTitlebar(st.file_name, st.dirty);
    status.textContent = "guardado";
  } catch (err) {
    status.textContent = `error al guardar: ${String(err)}`;
  }
}

// M5: exportar a HTML (dialogo "guardar como .html" + pandoc sidecar).
async function exportDoc(): Promise<void> {
  status.textContent = "exportando…";
  try {
    const out = await invoke<string | null>("export_document", { text: getText() });
    status.textContent = out ? "exportado" : "listo";
  } catch (err) {
    status.textContent = `error al exportar: ${String(err)}`;
  }
}

// M6: focus mode todavia no existe como feature; el hook queda cableado (WU6). Cuando
// exista, isFocusMode() devuelve true y el spellcheck se auto-desactiva (ADR-0001 D2).
const isFocusMode = (): boolean => false;

function scheduleUpdate(): void {
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(() => {
    void render();
    void updateStats();
    void syncDoc();
    void requestSpellCheck(view, getText(), isFocusMode());
  }, DEBOUNCE_MS);
}

const SAMPLE = [
  "# Apostrophe en Windows",
  "",
  "Editor **CodeMirror 6** nativo + pipeline **Rust → pandoc (sidecar) → WebView**.",
  "",
  "Atajos: `Ctrl+O` abrir · `Ctrl+S` guardar · `Ctrl+Shift+S` guardar como · `Ctrl+E` exportar.",
  "",
  "- ítem de lista",
  "- soporte de `código` inline",
  "",
  "> Cita en bloque para verificar estilos.",
  "",
  "Ecuación inline (MathJax): $E = mc^2$",
  "",
  "```rust",
  'fn main() { println!("hola, Apostrophe"); }',
  "```",
].join("\n");

// CodeMirror 6: editor de verdad (undo/redo, line-wrap, cierre de brackets) SIN
// gramatica Lezer (ADR-0001 D1). Los atajos de archivo van a nivel window (arriba).
const view = new EditorView({
  doc: SAMPLE,
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
