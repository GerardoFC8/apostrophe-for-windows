import { invoke } from "@tauri-apps/api/core";

// Spike (ADR-0001): editor (textarea) -> comando Rust `preview_markdown` (pandoc) -> iframe.
// La conversión vive en Rust (dominio); la WebView solo PRESENTA. La frontera IPC es
// gruesa y con debounce: NO se invoca por tecla, sino tras una pausa de escritura.
// El editor real (CodeMirror 6) y el resaltado vienen DESPUÉS de validar este pipeline.
const DEBOUNCE_MS = 250;

const editor = document.querySelector<HTMLTextAreaElement>("#editor")!;
const preview = document.querySelector<HTMLIFrameElement>("#preview")!;
const status = document.querySelector<HTMLElement>("#status")!;

let timer: number | undefined;

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

editor.addEventListener("input", () => {
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(render, DEBOUNCE_MS);
});

editor.value = [
  "# Apostrophe en Windows",
  "",
  "Spike del pipeline **Rust → pandoc → WebView**.",
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

render();
