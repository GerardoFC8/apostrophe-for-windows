// Panel de esquema: los títulos del documento en una columna a la izquierda del editor. Un
// clic lleva a la sección; la sección donde está el cursor se resalta. Se abre y cierra con
// el botón de la barra o Ctrl+Shift+O, y se recuerda entre sesiones.
//
// Los títulos los extrae el dominio (`outline`), con las mismas reglas que pandoc: así cada
// entrada corresponde a un título de la vista previa, que es lo que usa también el scroll
// sincronizado (scroll-sync.ts).
import { invoke } from "@tauri-apps/api/core";
import { EditorView } from "@codemirror/view";
import { view, getText } from "./editor";
import { getSetting, setSetting } from "./settings";
import { t } from "./i18n";

export interface Heading {
  level: number;
  text: string;
  /// Renglón donde empieza, desde 0.
  line: number;
}

const panel = document.querySelector<HTMLElement>("#outline")!;
const list = document.querySelector<HTMLElement>("#outline-list")!;
const toggleBtn = document.querySelector<HTMLButtonElement>("#outline-btn")!;

let headings: Heading[] = [];
let active = -1;
let onJump: (index: number) => void = () => {};
// Números de petición: con pestañas, una respuesta vieja puede llegar después de la nueva.
let seq = 0;

/// Títulos del documento mostrado (los usa también el scroll sincronizado).
export function getHeadings(): readonly Heading[] {
  return headings;
}

/// Índice del título cuya sección contiene el renglón `line` (desde 0), o -1 si está antes
/// del primero.
export function sectionAt(line: number): number {
  let index = -1;
  for (let i = 0; i < headings.length && headings[i].line <= line; i++) index = i;
  return index;
}

/// Vuelve a pedir los títulos al dominio y repinta el panel.
export async function updateOutline(): Promise<void> {
  const mine = ++seq;
  try {
    const next = await invoke<Heading[]>("outline", { text: getText() });
    if (mine !== seq) return;
    headings = next;
    renderList();
    markActive();
  } catch {
    /* no crítico: el panel se queda con lo último que tenía */
  }
}

function renderList(): void {
  list.replaceChildren();
  if (headings.length === 0) {
    const empty = document.createElement("p");
    empty.className = "outline-empty";
    empty.textContent = t("outline.empty");
    list.appendChild(empty);
    return;
  }
  // Sangría relativa al nivel más alto que haya: un documento sin `#` no arranca corrido.
  const top = Math.min(...headings.map((h) => h.level));
  headings.forEach((h, i) => {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "outline-item";
    item.style.paddingLeft = `${0.6 + (h.level - top) * 0.8}rem`;
    item.textContent = h.text || "…";
    item.title = h.text;
    item.dataset.index = String(i);
    item.addEventListener("click", () => jumpTo(i));
    list.appendChild(item);
  });
}

// Resalta la sección del cursor y la mantiene a la vista dentro del panel.
function markActive(): void {
  if (panel.hidden) return;
  const line = view.state.doc.lineAt(view.state.selection.main.head).number - 1;
  const index = sectionAt(line);
  if (index === active && list.querySelector(".outline-item.active")) return;
  active = index;
  list.querySelectorAll(".outline-item.active").forEach((el) => el.classList.remove("active"));
  const el = list.querySelector<HTMLElement>(`.outline-item[data-index="${index}"]`);
  if (el) {
    el.classList.add("active");
    el.scrollIntoView({ block: "nearest" });
  }
}

/// El cursor o la selección cambiaron: actualiza el resaltado.
export function onSelectionChanged(): void {
  markActive();
}

function jumpTo(index: number): void {
  const heading = headings[index];
  if (!heading) return;
  const line = Math.min(heading.line + 1, view.state.doc.lines);
  const pos = view.state.doc.line(line).from;
  view.dispatch({
    selection: { anchor: pos },
    effects: EditorView.scrollIntoView(pos, { y: "start", yMargin: 12 }),
  });
  view.focus();
  // Con la vista previa sola el editor no se ve: también se lleva la vista previa ahí.
  onJump(index);
}

function setOpen(open: boolean, persist: boolean): void {
  panel.hidden = !open;
  document.body.classList.toggle("outline-open", open);
  toggleBtn.setAttribute("aria-pressed", String(open));
  if (open) {
    renderList();
    active = -1;
    markActive();
  }
  if (persist) void setSetting("outlineOpen", open);
}

export function toggleOutline(): void {
  setOpen(panel.hidden, true);
}

// `jump` lleva la vista previa al título elegido (lo inyecta el arranque, para no depender de
// scroll-sync.ts).
export async function initOutline(jump: (index: number) => void): Promise<void> {
  onJump = jump;
  toggleBtn.addEventListener("click", toggleOutline);
  window.addEventListener(
    "keydown",
    (e) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && !e.altKey && e.key.toLowerCase() === "o") {
        e.preventDefault();
        e.stopImmediatePropagation();
        toggleOutline();
      }
    },
    true,
  );
  setOpen(await getSetting("outlineOpen"), false);
}
