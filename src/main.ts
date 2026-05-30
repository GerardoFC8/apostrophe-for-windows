import { invoke } from "@tauri-apps/api/core";

// Spike (ADR-0001): editor (textarea) -> comandos Rust -> presentacion.
//   - preview_markdown (pandoc)        -> iframe.
//   - count_stats (nucleo de dominio)  -> barra de estado.
// La logica vive en Rust (dominio); la WebView solo PRESENTA. La frontera IPC es
// gruesa y con debounce: NO se invoca por tecla, sino tras una pausa de escritura.
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

const editor = document.querySelector<HTMLTextAreaElement>("#editor")!;
const preview = document.querySelector<HTMLIFrameElement>("#preview")!;
const status = document.querySelector<HTMLElement>("#status")!;
const statWords = document.querySelector<HTMLElement>("#stat-words")!;
const statChars = document.querySelector<HTMLElement>("#stat-chars")!;
const statSentences = document.querySelector<HTMLElement>("#stat-sentences")!;
const statParagraphs = document.querySelector<HTMLElement>("#stat-paragraphs")!;
const statReadTime = document.querySelector<HTMLElement>("#stat-readtime")!;

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

async function render(): Promise<void> {
  status.textContent = "convirtiendo…";
  try {
    const html = await invoke<string>("preview_markdown", { text: editor.value });
    preview.srcdoc = html;
    status.textContent = "listo";
  } catch (err) {
    status.textContent = "error";
    preview.srcdoc = `<pre style="color:#c00;font-family:monospace;padding:1rem">${String(err)}</pre>`;
  }
}

async function updateStats(): Promise<void> {
  try {
    const s = await invoke<Stats>("count_stats", { text: editor.value });
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

editor.addEventListener("input", scheduleUpdate);

editor.value = [
  "# Apostrophe en Windows",
  "",
  "Spike del pipeline **Rust → pandoc → WebView** + contador de stats nativo.",
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

void render();
void updateStats();
