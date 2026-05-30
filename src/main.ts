// Fuentes bundleadas (offline) que clonan el look del GNOME actual (Adwaita):
// Inter (≈ Adwaita Sans) para el chrome, JetBrains Mono para el editor. Se
// empaquetan en el bundle via Vite — NO dependen de lo que tenga instalado Windows.
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Transaction } from "@codemirror/state";
import { EditorView, drawSelection, highlightActiveLine, keymap } from "@codemirror/view";
import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";

// M3: editor CodeMirror 6 (target del ADR-0001). M4: abrir/guardar .md con dialogos
// nativos; el estado (path/dirty) vive en el DocumentStore de Rust. La logica esta en
// Rust; la WebView solo PRESENTA. La frontera IPC es gruesa y con debounce.
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
// M4: documento abierto / estado del documento (titlebar).
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
const statWords = document.querySelector<HTMLElement>("#stat-words")!;
const statChars = document.querySelector<HTMLElement>("#stat-chars")!;
const statSentences = document.querySelector<HTMLElement>("#stat-sentences")!;
const statParagraphs = document.querySelector<HTMLElement>("#stat-paragraphs")!;
const statReadTime = document.querySelector<HTMLElement>("#stat-readtime")!;

// Chrome nativo (M3): titlebar custom. `decorations: false` (tauri.conf.json) saca
// la barra Win32; estos botones manejan la ventana via la window API de Tauri.
const appWindow = getCurrentWindow();
document.querySelector("#win-min")?.addEventListener("click", () => void appWindow.minimize());
document.querySelector("#win-max")?.addEventListener("click", () => void appWindow.toggleMaximize());
document.querySelector("#win-close")?.addEventListener("click", () => void appWindow.close());

// M4: atajos de archivo a nivel WINDOW (no solo en CM6) — asi funcionan aunque el
// foco NO este en el editor (titlebar, etc.). Shift+S se chequea ANTES que S sola.
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
  }
});

// Tema del editor (Adwaita-ish): hereda los colores de la pagina via CSS vars
// (light/dark en styles.css), tipografia JetBrains Mono. El acento es el azul Adwaita.
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
// para que el cambio NO sea deshacible: Ctrl+Z tras abrir NO debe revertir al doc
// anterior (eso desincronizaria editor y store -> riesgo de guardar lo viejo).
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
    preview.srcdoc = html;
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

function scheduleUpdate(): void {
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(() => {
    void render();
    void updateStats();
    void syncDoc();
  }, DEBOUNCE_MS);
}

const SAMPLE = [
  "# Apostrophe en Windows",
  "",
  "Editor **CodeMirror 6** nativo + pipeline **Rust → pandoc (sidecar) → WebView**.",
  "",
  "Atajos: `Ctrl+O` abrir · `Ctrl+S` guardar · `Ctrl+Shift+S` guardar como.",
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
// gramatica Lezer (ADR-0001 D1). Los atajos de archivo van a nivel window (arriba),
// no en el keymap de CM6, para que anden sin foco en el editor y sin doble disparo.
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
    EditorView.updateListener.of((update) => {
      if (update.docChanged) scheduleUpdate();
    }),
  ],
});

void render();
void updateStats();
