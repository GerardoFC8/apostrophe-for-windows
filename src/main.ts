// Fuentes bundleadas (offline) que clonan el look del GNOME actual (Adwaita):
// Inter (≈ Adwaita Sans) para el chrome, JetBrains Mono para el editor. Se
// empaquetan en el bundle via Vite — NO dependen de lo que tenga instalado Windows.
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";

import { appWindow } from "./app-window";
import { createEditor, view, getText } from "./editor";
import {
  initCloseGuard,
  initExternalChangeWatcher,
  initDocumentEvents,
  restoreInitialDocument,
  syncDoc,
} from "./document-session";
import { initPreviewWindow, initViewMode, render } from "./preview";
import { updateStats } from "./stats";
import { applyTheme, buildThemeMenu, getInitialTheme, initThemeMenu } from "./theme";
import { alignTableAtCursor, initExportButton } from "./commands";
import { initMenu } from "./menu";
import { initLanguage } from "./language";
import {
  initFullscreen,
  initSpellDictListener,
  isFocusMode,
  isSpellcheckEnabled,
  loadSpellcheckPreference,
} from "./writing-modes";
import { initFileShortcuts, initModeShortcuts } from "./shortcuts";
import { requestSpellCheck } from "./spellcheck";
import { requestMarkupHighlight } from "./markup-highlight";
import { detectLocale, setLocale } from "./i18n";

// M3: editor CodeMirror 6. M4: abrir/guardar .md. M5: temas light/dark/sepia +
// export a HTML. La logica esta en Rust; la WebView solo PRESENTA. La frontera IPC
// es gruesa y con debounce.
//
// Este archivo es solo el ARRANQUE: cablea los modulos en el mismo orden en que antes se
// ejecutaba todo desde aqui (registro de listeners, primer render, restauracion). Cada
// modulo documenta su responsabilidad; el estado del documento actual vive en
// document-session.ts.
const DEBOUNCE_MS = 250;

let timer: number | undefined;

function scheduleUpdate(): void {
  if (timer !== undefined) clearTimeout(timer);
  timer = window.setTimeout(() => {
    void render();
    void updateStats();
    void syncDoc();
    // M11.1: solo pedir el corrector si esta activado. Si esta apagado no pedimos nada
    // (las ondulitas ya quedaron limpias al togglear); el resaltado es independiente.
    if (isSpellcheckEnabled()) void requestSpellCheck(view, getText(), isFocusMode());
    void requestMarkupHighlight(view, getText()); // M10: resaltado (sigue activo en focus)
  }, DEBOUNCE_MS);
}

// Chrome nativo (M3): titlebar custom. `decorations: false` (tauri.conf.json) saca
// la barra Win32; estos botones manejan la ventana via la window API de Tauri.
document.querySelector("#win-min")?.addEventListener("click", () => void appWindow.minimize());
document.querySelector("#win-max")?.addEventListener("click", () => void appWindow.toggleMaximize());
document.querySelector("#win-close")?.addEventListener("click", () => void appWindow.close());

initCloseGuard(); // M8.1: guard anti-perdida al cerrar la ventana
initFileShortcuts(); // M4/M5: Ctrl+O/S/Shift+S/E/N
initPreviewWindow(); // M18: ventana de vista previa desacoplada
initThemeMenu(); // M5: selector de tema
initExportButton(); // M12

// M15: el idioma se fija ANTES de construir cualquier menu, porque las etiquetas salen del
// catalogo. Arranca con el del sistema (sincrono, sin parpadeo); la preferencia guardada se
// aplica mas abajo, en el arranque asincrono, y solo repinta si el usuario eligio otro.
setLocale(detectLocale());

buildThemeMenu();

initMenu(); // M11: menú hamburguesa + overlays (atajos / acerca de)
initExternalChangeWatcher(); // M8.4b: infobar de cambio externo

createEditor({ onDocChanged: scheduleUpdate, onAlignTable: alignTableAtCursor });

applyTheme(getInitialTheme()); // fija el tema antes del primer render (sin flash)
// M15: vuelca el catalogo sobre el DOM estatico, arma el selector de idioma y aplica la
// preferencia guardada (ver language.ts).
initLanguage();
void render();
void updateStats();
loadSpellcheckPreference(); // M11.1: arranque sin parpadeo (ver writing-modes.ts)
void requestMarkupHighlight(view, getText()); // M10: resaltado inicial (al abrir/restaurar)
initSpellDictListener();

initViewMode(isFocusMode); // M13: el modo enfoque es dueño de la vista mientras esta activo
initFullscreen(); // M9.3
initModeShortcuts(); // M9/M11/M13: atajos en fase captura

// Capa B + M8.4a: arranque del documento (asociacion .md o snapshot de crash).
void restoreInitialDocument();
initDocumentEvents(); // M8.4a autosave + reenvio de single-instance
