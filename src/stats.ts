// Contadores de la barra inferior (palabras, caracteres, oraciones, parrafos, lectura).
// El calculo lo hace el nucleo Rust (count_stats); aqui solo se pinta.
import { invoke } from "@tauri-apps/api/core";
import { getText } from "./editor";
import { t, tp } from "./i18n";

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

const statWords = document.querySelector<HTMLElement>("#stat-words")!;

// Las unidades (h/min/s) no se traducen: son simbolos y coinciden en ambos idiomas. Lo que
// si cambia es la frase que las envuelve, que sale del catalogo.
function formatReadTime(rt: ReadTime): string {
  const parts: string[] = [];
  if (rt.hours) parts.push(`${rt.hours} h`);
  if (rt.minutes) parts.push(`${rt.minutes} min`);
  parts.push(`${rt.seconds} s`);
  return t("stats.readtime", { time: parts.join(" ") });
}

// Mismo descarte que el preview: al cambiar de pestaña, una respuesta atrasada de la
// anterior no debe pisar los contadores de la nueva.
let statsSeq = 0;

export async function updateStats(): Promise<void> {
  const seq = ++statsSeq;
  try {
    const s = await invoke<Stats>("count_stats", { text: getText() });
    if (seq !== statsSeq) return;
    // En el dock solo van las palabras; el resto queda en el tooltip.
    const words = tp("stats.words", s.words);
    const details = [
      tp("stats.chars", s.characters),
      tp("stats.sentences", s.sentences),
      tp("stats.paragraphs", s.paragraphs),
      formatReadTime(s.read_time),
    ].join(" · ");
    statWords.textContent = words;
    statWords.title = details;
    // El tooltip no se anuncia con el teclado: el nombre accesible lleva todo.
    statWords.setAttribute("aria-label", `${words} · ${details}`);
  } catch (err) {
    // En la píldora del dock cabe poco: el detalle del error va en el tooltip.
    statWords.textContent = t("error.generic");
    statWords.title = String(err);
    statWords.setAttribute("aria-label", `${t("error.generic")}: ${String(err)}`);
  }
}
