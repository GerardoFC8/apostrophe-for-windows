// Ventana de vista previa desacoplada (M18). Paridad con `preview_window.py` del upstream:
// la vista renderizada en su propia ventana, para escribir en un monitor y leer en el otro.
//
// Es DELIBERADAMENTE tonta respecto del documento: no habla con el dominio ni invoca
// comandos. La ventana principal calcula el HTML del preview (con su hoja de estilos) y lo
// emite junto con el color de fondo del tema y el idioma; aquí solo se vuelca en el iframe.
//
// Lo único propio de esta ventana son sus dos controles: "siempre visible" y la opacidad del
// fondo, para dejarla flotando encima de otra aplicación mientras se escribe.

import { listen, emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { linkFromMessage } from "./preview-link";
import type { PreviewPayload } from "./preview";
import { LOCALES, setLocale, t } from "./i18n";
import type { Locale } from "./i18n";
import { getSetting, setSetting } from "./settings";

const frame = document.querySelector<HTMLIFrameElement>("#frame")!;
const pin = document.querySelector<HTMLButtonElement>("#pin")!;
const opacity = document.querySelector<HTMLInputElement>("#opacity")!;

// El fondo lo pinta el body de esta página, con opacidad; el documento del iframe tiene que
// ir sin fondo para que se vea. Va al final del <head>, después de la hoja del tema.
const TRANSPARENT_DOCUMENT = "<style>html,body{background:transparent !important;}</style>";

let background = "#ffffff";
let alpha = 1;

function paintBackground(): void {
  const percent = Math.round(alpha * 100);
  document.body.style.background =
    percent >= 100 ? background : `color-mix(in srgb, ${background} ${percent}%, transparent)`;
}

function applyLabels(): void {
  pin.title = t("previewWindow.alwaysOnTop");
  pin.setAttribute("aria-label", pin.title);
  opacity.title = t("previewWindow.opacity");
  opacity.setAttribute("aria-label", opacity.title);
}

// La ventana principal emite esto en cada render (y una vez al abrirse esta ventana).
void listen<PreviewPayload>("preview-html", (event) => {
  const { html, locale } = event.payload;
  frame.srcdoc = html.includes("</head>")
    ? html.replace("</head>", `${TRANSPARENT_DOCUMENT}</head>`)
    : TRANSPARENT_DOCUMENT + html;
  background = event.payload.background || background;
  paintBackground();
  if ((LOCALES as readonly string[]).includes(locale)) {
    setLocale(locale as Locale);
    applyLabels();
  }
});

// --- Siempre visible ---

async function setPinned(on: boolean): Promise<void> {
  try {
    await getCurrentWindow().setAlwaysOnTop(on);
    pin.setAttribute("aria-pressed", String(on));
  } catch {
    /* sin permiso o sin ventana: el botón queda como estaba */
  }
}

pin.addEventListener("click", () => {
  const on = pin.getAttribute("aria-pressed") !== "true";
  void setPinned(on);
  void setSetting("previewAlwaysOnTop", on);
});

// --- Opacidad del fondo ---

opacity.addEventListener("input", () => {
  alpha = Number(opacity.value) / 100;
  paintBackground();
});
// Se guarda al soltar el control, no en cada paso del arrastre.
opacity.addEventListener("change", () => void setSetting("previewOpacity", alpha));

async function restore(): Promise<void> {
  alpha = Math.min(1, Math.max(0.2, await getSetting("previewOpacity")));
  opacity.value = String(Math.round(alpha * 100));
  paintBackground();
  if (await getSetting("previewAlwaysOnTop")) await setPinned(true);
}

applyLabels();
void restore();

// Al abrir, la ventana principal puede llevar rato sin renderizar: se le pide el HTML
// actual para no quedarse en blanco hasta la próxima tecla.
void emit("preview-window-ready");

// Los enlaces del HTML no se siguen aquí: el iframe pasa el href y se reenvía a la ventana
// principal, que conoce el documento activo y sus pestañas (ver initPreviewLinks).
window.addEventListener("message", (e) => {
  const href = linkFromMessage(e, frame);
  if (href !== null) void emit("preview-link", href);
});
