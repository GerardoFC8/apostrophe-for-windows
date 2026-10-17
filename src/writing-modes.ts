// --- M9: modos de escritura (enfoque / Hemingway / pantalla completa) + corrector (M11.1) ---
// Estado y toggles de los modos. El atenuado del enfoque vive en ./focus-mode y el filtro
// de Hemingway en ./hemingway; aqui solo se decide cuando activarlos.
import { listen } from "@tauri-apps/api/event";
import { appWindow } from "./app-window";
import { view, getText } from "./editor";
import { getViewMode, setViewMode, type ViewMode } from "./preview";
import { enableFocusDim, disableFocusDim } from "./focus-mode";
import { hemingwayCompartment, hemingwayFilter } from "./hemingway";
import { requestSpellCheck, clearSpellSpans } from "./spellcheck";
import { getSetting, setSetting } from "./settings";
import { flashStatus } from "./status";
import { t } from "./i18n";

// M9.1: estado del modo enfoque. isFocusMode() lo expone para el spellcheck (focus apaga
// las ondulitas — ADR-0001 D2). El atenuado del texto por parrafo vive en ./focus-mode; el
// toggle (toggleFocusMode) esta mas abajo, junto al resto de los modos de escritura.
let focusMode = false;
export const isFocusMode = (): boolean => focusMode;

// M11.1: estado del corrector ortografico (toggle por F7), persistido en settings.json.
// Es la FUENTE DE VERDAD del gating: TODO punto que pida spellcheck (debounce, arranque,
// dict-ready, salida de enfoque) debe respetarlo, para que con el corrector apagado no
// queden ondulitas ni se dispare ningun pedido.
//
// Arranca en FALSE a proposito: el valor real se lee de forma async de la preferencia (ver
// loadSpellcheckPreference, mas abajo). Mientras esa lectura vuela, un lector sincrono
// (el debounce, o el listener "spell-dict-ready") podria adelantarsele; con el valor inicial
// en false, en el peor caso el corrector arranca unos ms tarde (invisible, se corrige apenas
// resuelve la lectura), NUNCA pinta una ondulita que la preferencia querria apagada.
let spellcheckEnabled = false;

// Lectura del gating del corrector para quien pide spellcheck desde fuera (el debounce).
export const isSpellcheckEnabled = (): boolean => spellcheckEnabled;

// M11.1: ARRANQUE SIN PARPADEO. Leemos la preferencia del corrector ANTES del primer
// pedido: si quedo apagado, no pedimos nada y el editor arranca sin ondulitas (sin el
// flash de pintarlas para luego limpiarlas). El spellcheck inicial puede volver [] si el
// diccionario aun se esta cargando en background; cuando termina, Rust emite
// "spell-dict-ready" y re-chequeamos el doc actual — asi no hay que esperar al primer
// tecleo (review M6).
export function loadSpellcheckPreference(): void {
  void (async () => {
    spellcheckEnabled = await getSetting("spellcheck");
    if (spellcheckEnabled) void requestSpellCheck(view, getText(), isFocusMode());
  })();
}

export function initSpellDictListener(): void {
  void listen("spell-dict-ready", () => {
    // Solo re-chequear si el corrector esta activado: si esta apagado, que cargue el dict
    // tarde no debe repintar ondulitas.
    if (spellcheckEnabled) void requestSpellCheck(view, getText(), isFocusMode());
  });
}

// Recuerda la vista previa al entrar al modo enfoque, para restaurarla al salir (round-trip).
let viewBeforeFocus: ViewMode = "split";

// M9.1: alterna el modo enfoque. Activa/desactiva el atenuado por parrafo (./focus-mode),
// togglea la clase body.focus-mode (CSS distraction-free: oculta el preview, centra el
// editor, atenua el chrome) y gestiona el spellcheck (al entrar lo apaga, al salir lo
// repinta — ADR-0001 D2).
export function toggleFocusMode(): void {
  focusMode = !focusMode;
  document.body.classList.toggle("focus-mode", focusMode);
  if (focusMode) {
    // El enfoque fuerza solo-editor: recordamos la vista actual para restaurarla al salir (M13).
    viewBeforeFocus = getViewMode();
    setViewMode("editor");
    enableFocusDim(view);
    clearSpellSpans(view); // entrar: apaga las ondulitas
  } else {
    disableFocusDim(view);
    setViewMode(viewBeforeFocus); // restaura la vista que habia antes del enfoque (round-trip M13)
    // Salir del enfoque: repinta el corrector SOLO si esta activado. Con el corrector
    // apagado, salir de enfoque NO debe hacer reaparecer ninguna ondulita (M11.1).
    if (spellcheckEnabled) void requestSpellCheck(view, getText(), false);
  }
  flashStatus(focusMode ? "Modo enfoque activado" : "Modo enfoque desactivado", 1500);
}

// M11.1: alterna el corrector ortografico (F7, paridad con el upstream). Persiste el nuevo
// estado en settings.json (fire-and-forget: persistir no es critico) y sincroniza las
// ondulitas: al activarlo re-chequea el doc actual (salvo en modo enfoque, donde el
// corrector se apaga por diseno — ADR-0001 D2); al desactivarlo limpia las ondulitas ya.
export function toggleSpellcheck(): void {
  spellcheckEnabled = !spellcheckEnabled;
  void setSetting("spellcheck", spellcheckEnabled);
  if (spellcheckEnabled && !isFocusMode()) {
    void requestSpellCheck(view, getText(), false);
  } else {
    clearSpellSpans(view);
  }
  flashStatus(spellcheckEnabled ? "Corrector activado" : "Corrector desactivado", 1500);
}

let hemingwayMode = false;

// M9.2: alterna el modo Hemingway reconfigurando el compartment del changeFilter en
// caliente (sin recrear el EditorView).
export function toggleHemingway(): void {
  hemingwayMode = !hemingwayMode;
  view.dispatch({
    effects: hemingwayCompartment.reconfigure(hemingwayMode ? hemingwayFilter : []),
  });
  flashStatus(
    hemingwayMode
      ? "Modo Hemingway activado — no puedes borrar texto"
      : "Modo Hemingway desactivado",
    1800,
  );
}

// M9.3: pantalla completa (F11). Tauri controla el fullscreen REAL del SO; nosotros
// ocultamos el chrome custom (titlebar + statusbar) con la clase body.is-fullscreen. UNA
// fuente de verdad: syncFullscreenChrome() lee el estado real y pinta el body, asi un toggle
// por F11 y una salida por otra via quedan coherentes (Tauri NO expone evento de fullscreen).
// Requiere core:window:allow-set-fullscreen + allow-is-fullscreen en capabilities/default.json;
// sin ellos setFullscreen falla en silencio (mismo gotcha que allow-destroy con el cierre).
async function syncFullscreenChrome(): Promise<void> {
  try {
    const fs = await appWindow.isFullscreen();
    document.body.classList.toggle("is-fullscreen", fs);
    if (!fs) document.body.classList.remove("reveal-chrome"); // al salir, limpia el hover
  } catch {
    /* no critico: si falla, el chrome simplemente no cambia */
  }
}

// Guard reentrante (mismo patron que closeInProgress en el cierre de ventana): isFullscreen
// es async, asi que dos cambios solapados (doble F11, autorepeat, o F11+Esc) leerian el MISMO
// estado viejo y se pisarian, dejando el fullscreen "pegado". applyFullscreen serializa TODOS
// los caminos (F11 togglea con target=undefined; Esc fuerza target=false) bajo el mismo guard.
let fullscreenInProgress = false;
export async function applyFullscreen(target?: boolean): Promise<void> {
  if (fullscreenInProgress) return;
  fullscreenInProgress = true;
  try {
    const next = target ?? !(await appWindow.isFullscreen());
    await appWindow.setFullscreen(next);
  } catch {
    flashStatus(t("status.fullscreenFailed"), 3000);
  } finally {
    fullscreenInProgress = false;
  }
  await syncFullscreenChrome();
}

export function initFullscreen(): void {
  // Entrar/salir de fullscreen SIEMPRE redimensiona la ventana; onResized cubre cualquier
  // salida fuera de banda (Tauri no tiene un evento de fullscreen dedicado). Debounce: el
  // resize emite en rafaga al arrastrar el borde; sin debounce serian decenas de IPC
  // isFullscreen() por gesto. El cambio real (entrar/salir) lo aplica de inmediato
  // applyFullscreen (F11/Esc); aqui solo cubrimos la salida fuera de banda, ~150ms de retardo basta.
  let fsResizeTimer: number | undefined;
  void appWindow.onResized(() => {
    if (fsResizeTimer !== undefined) clearTimeout(fsResizeTimer);
    fsResizeTimer = window.setTimeout(() => void syncFullscreenChrome(), 150);
  });

  // Reveal-on-hover del borde superior: en fullscreen, al acercar el mouse arriba reaparece la
  // titlebar (paridad con el GtkRevealer del upstream) para mover/cerrar la ventana.
  const fsHoverZone = document.querySelector<HTMLElement>("#fs-hover-zone");
  fsHoverZone?.addEventListener("mouseenter", () => {
    if (document.body.classList.contains("is-fullscreen")) {
      document.body.classList.add("reveal-chrome");
    }
  });
  document.querySelector(".titlebar")?.addEventListener("mouseleave", () => {
    document.body.classList.remove("reveal-chrome");
  });
}
