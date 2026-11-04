// Zoom del texto: Ctrl + / Ctrl - / Ctrl 0 y Ctrl + rueda del mouse. Amplía la letra del
// editor y de la vista previa a la vez (no la interfaz: barras, pestañas y menús quedan
// igual), y se recuerda entre sesiones.
//
// WebView2 trae su propio zoom de página con esas mismas teclas, pero Tauri lo deja apagado
// (`zoomHotkeysEnabled` es false por defecto), así que estos atajos no compiten con él.
import { getSetting, setSetting } from "./settings";
import { flashStatus } from "./status";
import { t } from "./i18n";

const MIN = 0.6;
const MAX = 2.5;
const STEP = 0.1;

let zoom = 1;
let onZoomChanged: () => void = () => {};

// Una sola fuente de verdad: la variable CSS que lee el tema del editor. La vista previa la
// lee en cada render (ver preview.ts), por eso además se avisa para repintarla.
function apply(next: number, announce: boolean): void {
  zoom = Math.round(Math.min(MAX, Math.max(MIN, next)) * 10) / 10;
  document.documentElement.style.setProperty("--text-zoom", String(zoom));
  onZoomChanged();
  if (announce) {
    flashStatus(t("status.zoom", { percent: Math.round(zoom * 100) }));
    void setSetting("zoom", zoom);
  }
}

export function getZoom(): number {
  return zoom;
}

function zoomBy(steps: number): void {
  apply(zoom + steps * STEP, true);
}

// Rueda: un "click" del mouse trae deltaY de ~100; un touchpad manda muchos eventos chicos.
// Se acumula y se da un paso cada 50, para que los dos se sientan parecidos.
let wheelAccum = 0;

function onWheel(e: WheelEvent): void {
  if (!e.ctrlKey) return;
  e.preventDefault();
  wheelAccum += e.deltaY;
  while (Math.abs(wheelAccum) >= 50) {
    const dir = wheelAccum < 0 ? 1 : -1; // rueda hacia arriba (deltaY negativo) = acercar
    zoomBy(dir);
    wheelAccum += dir * 50;
  }
}

function onKey(e: KeyboardEvent): void {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
  // "+" sale con Shift en muchos teclados (o en el teclado numérico); "=" es la misma tecla
  // sin Shift en el teclado de EE. UU. Se aceptan las dos.
  if (e.key === "+" || e.key === "=") zoomBy(1);
  else if (e.key === "-") zoomBy(-1);
  else if (e.key === "0") apply(1, true);
  else return;
  e.preventDefault();
  e.stopImmediatePropagation();
}

// `refresh` repinta la vista previa con el tamaño nuevo. Se inyecta desde el arranque para
// que este módulo no dependa de preview.ts.
export async function initZoom(refresh: () => void): Promise<void> {
  onZoomChanged = refresh;
  window.addEventListener("keydown", onKey, true);
  // passive: false, o el navegador ignora preventDefault y además hace scroll.
  window.addEventListener("wheel", onWheel, { passive: false });
  apply(await getSetting("zoom"), false);
}
