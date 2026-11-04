// Scroll sincronizado: al desplazar el editor, la vista previa acompaña.
//
// La correspondencia se hace título por título (outline.ts): se calcula en qué sección está
// el renglón de arriba del editor y qué fracción de ella se recorrió, y la vista previa pone
// arriba ese mismo título, desplazado en la misma proporción. Los títulos son lo único que
// existe igual en los dos lados (pandoc no informa de qué renglón sale cada párrafo), y entre
// uno y otro la interpolación es suficiente para leer a la par.
import { view } from "./editor";
import { getHeadings, sectionAt } from "./outline";

/// Posición a sincronizar. `index` es el título (-1 = antes del primero) y `fraction` cuánto
/// se avanzó hacia el siguiente; `end` pide ir al final.
export interface SyncPosition {
  index: number;
  fraction: number;
  end?: boolean;
}

let last: SyncPosition = { index: -1, fraction: 0 };
let send: (pos: SyncPosition) => void = () => {};
let pending = false;

function compute(): SyncPosition {
  const dom = view.scrollDOM;
  if (dom.scrollTop > 0 && dom.scrollTop + dom.clientHeight >= dom.scrollHeight - 2) {
    return { index: -1, fraction: 1, end: true };
  }
  const doc = view.state.doc;
  const height = Math.max(0, dom.scrollTop - view.documentPadding.top);
  const block = view.lineBlockAtHeight(height);
  const within = block.height > 0 ? (height - block.top) / block.height : 0;
  const line = doc.lineAt(block.from).number - 1 + Math.min(1, Math.max(0, within));

  const headings = getHeadings();
  const index = sectionAt(Math.floor(line));
  const start = index < 0 ? 0 : headings[index].line;
  const end = index + 1 < headings.length ? headings[index + 1].line : doc.lines;
  const fraction = end > start ? Math.min(1, Math.max(0, (line - start) / (end - start))) : 0;
  return { index, fraction };
}

function onScroll(): void {
  if (pending) return;
  pending = true;
  requestAnimationFrame(() => {
    pending = false;
    last = compute();
    send(last);
  });
}

// El editor se ve (vista dividida o solo editor). Con la vista previa sola está oculto y su
// posición no dice nada.
function editorVisible(): boolean {
  return view.scrollDOM.clientHeight > 0;
}

/// La vista previa se recargó (cada render la vuelve a crear): se le reenvía la posición
/// para que no salte al principio. Con el editor a la vista se recalcula desde él, porque
/// los títulos pudieron cambiar desde la última vez (se agregó o borró uno).
export function resendSync(): void {
  if (editorVisible()) last = compute();
  send(last);
}

/// La vista previa informa dónde quedó: es lo que se restaura si se recarga sin que el editor
/// se haya movido (por ejemplo, desplazándola a mano con la vista previa sola).
export function rememberPreviewPosition(pos: SyncPosition): void {
  last = pos;
}

/// Lleva la vista previa al título `index` (el esquema, cuando solo se ve la vista previa).
export function syncToHeading(index: number): void {
  last = { index, fraction: 0 };
  send(last);
}

// `post` entrega la posición a la vista previa (y a la ventana desacoplada). Se inyecta
// desde el arranque para que este módulo no dependa de preview.ts.
export function initScrollSync(post: (pos: SyncPosition) => void): void {
  send = post;
  view.scrollDOM.addEventListener("scroll", onScroll, { passive: true });
}
