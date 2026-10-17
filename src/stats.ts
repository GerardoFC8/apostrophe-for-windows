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
const statChars = document.querySelector<HTMLElement>("#stat-chars")!;
const statSentences = document.querySelector<HTMLElement>("#stat-sentences")!;
const statParagraphs = document.querySelector<HTMLElement>("#stat-paragraphs")!;
const statReadTime = document.querySelector<HTMLElement>("#stat-readtime")!;

// Las unidades (h/min/s) no se traducen: son simbolos y coinciden en ambos idiomas. Lo que
// si cambia es la frase que las envuelve, que sale del catalogo.
function formatReadTime(rt: ReadTime): string {
  const parts: string[] = [];
  if (rt.hours) parts.push(`${rt.hours} h`);
  if (rt.minutes) parts.push(`${rt.minutes} min`);
  parts.push(`${rt.seconds} s`);
  return t("stats.readtime", { time: parts.join(" ") });
}

export async function updateStats(): Promise<void> {
  try {
    const s = await invoke<Stats>("count_stats", { text: getText() });
    statWords.textContent = tp("stats.words", s.words);
    statChars.textContent = tp("stats.chars", s.characters);
    statSentences.textContent = tp("stats.sentences", s.sentences);
    statParagraphs.textContent = tp("stats.paragraphs", s.paragraphs);
    statReadTime.textContent = formatReadTime(s.read_time);
  } catch (err) {
    statWords.textContent = `${t("error.generic")}: ${String(err)}`;
  }
}
