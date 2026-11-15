// Scroll sincronizado: al desplazar el editor, la vista previa acompaña.
//
// La correspondencia se hace título por título (outline.ts): se calcula en qué sección está
// el renglón de arriba del editor y qué fracción de ella se recorrió, y la vista previa pone
// arriba ese mismo título, desplazado en la misma proporción. Los títulos son lo único que
// existe igual en los dos lados (pandoc no informa de qué renglón sale cada párrafo), y entre
// uno y otro la interpolación es suficiente para leer a la par.
import { view } from "./editor";
import { getHeadings, markSection, recentJump, sectionAt } from "./outline";

/// Posición a sincronizar. `index` es el título (-1 = antes del primero) y `fraction` cuánto
/// se avanzó hacia el siguiente; `end` pide ir al final.
export interface SyncPosition {
  index: number;
  fraction: number;
  end?: boolean;
  /** Solo en lo que informa la vista previa: la sección que se lee (para el esquema). */
  section?: number;
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
    markSection(last.end ? endSection() : sectionInView());
  });
}

// Qué sección se está leyendo: la del título que ya entró en el primer quinto de la vista
// (como mínimo 48 px, para que cuente un título puesto arriba por el esquema). Con solo el
// borde superior, un título visible pero no pegado arriba seguiría marcando la anterior. La
// vista previa usa la misma regla (preview-frame.ts).
function readingOffset(height: number): number {
  return Math.max(48, height * 0.2);
}

function sectionInView(): number {
  const dom = view.scrollDOM;
  const height = Math.max(0, dom.scrollTop - view.documentPadding.top + readingOffset(dom.clientHeight));
  const block = view.lineBlockAtHeight(height);
  return sectionAt(view.state.doc.lineAt(block.from).number - 1);
}

// Al final del documento las últimas secciones no llegan arriba: se marca la última, salvo
// que se haya llegado ahí con un clic en el esquema, que manda.
function endSection(): number {
  const jumped = recentJump();
  return jumped >= 0 ? jumped : getHeadings().length - 1;
}

// El esquema resalta la sección que se lee en la vista previa (`section`, con la misma regla
// que el editor).
function showSection(pos: SyncPosition): void {
  markSection(pos.end ? endSection() : (pos.section ?? pos.index));
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
/// se haya movido (por ejemplo, desplazándola a mano con la vista previa sola). También mueve
/// el resaltado del esquema, que con la vista previa sola es la única pista de dónde se está.
export function rememberPreviewPosition(pos: SyncPosition): void {
  last = pos;
  showSection(pos);
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
