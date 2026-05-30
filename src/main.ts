// Fuentes bundleadas (offline) que clonan el look del GNOME actual (Adwaita):
// Inter (≈ Adwaita Sans) para el chrome, JetBrains Mono para el editor. Se
// empaquetan en el bundle via Vite — NO dependen de lo que tenga instalado Windows.
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";

import { invoke } from "@tauri-apps/api/core";
import { EditorView, keymap, drawSelection, highlightActiveLine } from "@codemirror/view";
import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";

// M3: el editor pasa de <textarea> a CodeMirror 6 (target del ADR-0001). La logica
// sigue en Rust (dominio); la WebView solo PRESENTA. La frontera IPC es gruesa y con
// debounce: NO se invoca por tecla, sino tras una pausa de escritura.
//
// ADR-0001 D1: el highlighting (negritas/italicas dentro del editor) NO se hace con
// la gramatica Lezer de CodeMirror — vendra del nucleo Rust como spans (post-MVP).
// Por eso aca NO se incluye @codemirror/lang-markdown ni syntaxHighlighting.
const DEBOUNCE_MS = 250;

// --- Contrato del IPC con el nucleo (apostrophe_core::stats) ---
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

const editorParent = document.querySelector<HTMLElement>("#editor")!;
const preview = document.querySelector<HTMLIFrameElement>("#preview")!;
const status = document.querySelector<HTMLElement>("#status")!;
const statWords = document.querySelector<HTMLElement>("#stat-words")!;
const statChars = document.querySelector<HTMLElement>("#stat-chars")!;
const statSentences = document.querySelector<HTMLElement>("#stat-sentences")!;
const statParagraphs = document.querySelector<HTMLElement>("#stat-paragraphs")!;
const statReadTime = document.querySelector<HTMLElement>("#stat-readtime")!;

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

function getText(): string {
  return view.state.doc.toString();
}

async function render(): Promise<void> {
  status.textContent = "convirtiendo…";
  try {
    const html = await invoke<string>("preview_markdown", { text: getText() });
    preview.srcdoc = html;
    status.textContent = "listo";
  } catch (err) {
    status.textContent = "error";
    preview.srcdoc = `<pre style="color:#c00;font-family:monospace;padding:1rem">${String(err)}</pre>`;
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

function scheduleUpdate(): void {
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(() => {
    void render();
    void updateStats();
  }, DEBOUNCE_MS);
}

const SAMPLE = [
  "# Apostrophe en Windows",
  "",
  "Editor **CodeMirror 6** nativo + pipeline **Rust → pandoc (sidecar) → WebView**.",
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
// gramatica Lezer (ADR-0001 D1). El updateListener dispara el mismo debounce.
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
