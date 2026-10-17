// Panel de vista previa: renderizado via pandoc (backend), hoja de estilos derivada del
// tema, ventana desacoplada (M18), tema de exportacion y modo de vista (M13).
import { invoke } from "@tauri-apps/api/core";
import { listen, emit } from "@tauri-apps/api/event";
import { view, getText } from "./editor";
import { activeDocId } from "./tabs";
import { flashStatus } from "./status";
import { t } from "./i18n";

const preview = document.querySelector<HTMLIFrameElement>("#preview")!;

// Ultimo HTML crudo de pandoc, para re-tematizar el preview al cambiar de tema sin
// volver a invocar pandoc.
let lastPreviewHtml = "";

// CSS del preview derivado de las CSS vars del tema activo (una sola fuente de verdad).
// Reglas del markdown renderizado, parametrizadas por la paleta. Las comparten el
// preview (tema activo) y la exportacion (siempre claro): mismas reglas, distinta
// paleta. Es lo que garantiza que el HTML exportado se vea como el preview.
function markdownStylesheet(v: (name: string) => string): string {
  return `<style>
    html,body{background:${v("--preview-bg")};color:${v("--preview-fg")};}
    body{font-family:"Inter Variable",system-ui,sans-serif;line-height:1.6;max-width:46em;margin:0 auto;padding:1.6rem 2rem;}
    a{color:${v("--accent")};}
    code,pre{background:${v("--preview-code-bg")};border-radius:6px;font-family:"JetBrains Mono Variable",ui-monospace,monospace;}
    code{padding:.1em .35em;}
    pre{padding:.7em .9em;overflow:auto;}
    pre code{background:none;padding:0;}
    blockquote{border-left:3px solid ${v("--border")};margin-left:0;padding-left:1em;opacity:.85;}
    h1,h2,h3{line-height:1.25;}
    img{max-width:100%;}
  </style>`;
}

function previewStyle(): string {
  const cs = getComputedStyle(document.documentElement);
  return markdownStylesheet((name) => cs.getPropertyValue(name).trim());
}

// Tema para EXPORTAR: siempre la paleta CLARA, sin importar el tema activo. Un HTML o un
// PDF de fondo oscuro es incomodo de leer y de imprimir, y quien lo recibe no eligio tu
// tema. Se lee de un elemento sonda con data-theme="light" porque las vars del tema
// activo viven en <html> y se heredan a todo el arbol.
//
// Devuelve las dos piezas que necesita el backend: la hoja de estilos (para el <head> del
// HTML) y el acento suelto (para el color de enlaces del PDF, que no entiende CSS).
export function exportTheme(): { css: string; accent: string } {
  const probe = document.createElement("div");
  probe.dataset.theme = "light";
  probe.hidden = true;
  document.body.appendChild(probe);
  const cs = getComputedStyle(probe);
  const v = (name: string) => cs.getPropertyValue(name).trim();
  const theme = { css: markdownStylesheet(v), accent: v("--accent") };
  probe.remove();
  return theme;
}

// Inyecta el <style> del tema en el HTML standalone de pandoc (inline -> anda en el
// iframe srcdoc sandboxeado).
function withTheme(html: string): string {
  return html.includes("</head>")
    ? html.replace("</head>", `${previewStyle()}</head>`)
    : previewStyle() + html;
}

// Re-tematizar el preview ya renderizado (editor/chrome se actualizan solos via vars).
// Lo llama applyTheme (theme.ts) al cambiar de tema.
export function refreshPreviewTheme(): void {
  if (lastPreviewHtml) {
    preview.srcdoc = withTheme(lastPreviewHtml);
    pushToPreviewWindow(); // el HTML no cambio, pero si la hoja de estilos del tema
  }
}

// Escapa HTML para no inyectar markup al mostrar errores en el iframe del preview.
export function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => (c === "&" ? "&amp;" : c === "<" ? "&lt;" : "&gt;"));
}

// Numero del ultimo render pedido. Con pestañas, un render lento de la pestaña anterior
// puede llegar DESPUES que el de la nueva: solo se pinta la respuesta mas reciente.
let renderSeq = 0;

export async function render(): Promise<void> {
  // El preview que se actualiza YA es el feedback; no se ensucia la barra con
  // "convirtiendo…"/"listo". Solo se avisa si falla (ademas del error en el panel).
  const seq = ++renderSeq;
  try {
    // docId: pandoc resuelve las imagenes relativas contra la carpeta de ESTE documento.
    const html = await invoke<string>("preview_markdown", { text: getText(), docId: activeDocId() });
    if (seq !== renderSeq) return; // llego tarde: ya se pidio otro
    lastPreviewHtml = html;
    preview.srcdoc = withTheme(html);
    pushToPreviewWindow(); // M18: si hay ventana desacoplada, se actualiza con el mismo HTML
  } catch (err) {
    if (seq !== renderSeq) return;
    flashStatus(t("status.previewFailed"), 4000);
    preview.srcdoc = `<pre style="color:#c00;font-family:monospace;padding:1rem">${escapeHtml(String(err))}</pre>`;
  }
}

// --- M18: ventana de vista previa desacoplada ---

const previewWindowBtn = document.querySelector<HTMLButtonElement>("#preview-window-btn")!;

// Manda el HTML actual a la ventana desacoplada. Se emite SIEMPRE, aunque no haya ventana:
// un evento sin destinatario no cuesta nada y evita tener que rastrear si esta abierta.
function pushToPreviewWindow(): void {
  if (lastPreviewHtml) void emit("preview-html", withTheme(lastPreviewHtml));
}

async function togglePreviewWindow(): Promise<void> {
  try {
    const open = await invoke<boolean>("toggle_preview_window");
    previewWindowBtn.setAttribute("aria-pressed", String(open));
    if (open) pushToPreviewWindow();
  } catch (err) {
    flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
  }
}

// Cablea el boton y el handshake de la ventana desacoplada.
export function initPreviewWindow(): void {
  // La ventana recien abierta puede tardar en montar su listener, y la principal quiza lleve
  // rato sin renderizar: cuando avisa que esta lista, se le manda el HTML actual para que no
  // arranque en blanco.
  void listen("preview-window-ready", () => pushToPreviewWindow());

  previewWindowBtn.addEventListener("click", () => void togglePreviewWindow());
  // Si la ventana quedo abierta de una sesion anterior (o de un recargar en dev), el boton
  // tiene que reflejarlo en vez de mentir.
  void invoke<boolean>("preview_window_open")
    .then((open) => previewWindowBtn.setAttribute("aria-pressed", String(open)))
    .catch(() => {});
}

// --- M13: control de vista (solo editor / dividido / solo vista) ---
// Tres estados, paridad con el upstream Apostrophe: "editor" (solo el editor), "split"
// (editor + preview lado a lado, el default validado) y "preview" (solo la vista renderizada).
// El CSS de cada estado vive en styles.css (body.view-editor/view-split/view-preview); aqui
// solo se gestiona el estado y se sincroniza la clase del body + el resaltado de los botones.
export type ViewMode = "editor" | "split" | "preview";
const VIEW_CLASS: Record<ViewMode, string> = {
  editor: "view-editor",
  split: "view-split",
  preview: "view-preview",
};
const VIEW_ORDER: ViewMode[] = ["editor", "split", "preview"];

let viewMode: ViewMode = "split";
const viewBtns = Array.from(document.querySelectorAll<HTMLButtonElement>("[data-view]"));

// Aplica un modo de vista: una sola fuente de verdad para la clase del body, el estado de los
// botones y el reenfoque del editor. Quita las tres clases y pone solo la del modo actual, asi
// nunca quedan dos activas a la vez.
export function setViewMode(m: ViewMode): void {
  viewMode = m;
  document.body.classList.remove("view-editor", "view-split", "view-preview");
  document.body.classList.add(VIEW_CLASS[m]);
  for (const btn of viewBtns) {
    btn.setAttribute("aria-pressed", String(btn.dataset.view === m));
  }
  // Al volver a un modo donde el editor es visible (editor/split), re-enfocarlo: si veniamos de
  // "preview" el editor estaba oculto y habia perdido el foco. En "preview" no enfocamos nada
  // (el editor sigue vivo en el DOM, solo display:none — CM6 no se destruye).
  if (m !== "preview") view.focus();
}

// Cicla editor -> split -> preview -> editor (Ctrl+P, paridad con el upstream). El modo enfoque
// es DUEÑO de la vista (fuerza solo-editor): mientras este activo, ignoramos el ciclo.
export function cycleViewMode(): void {
  if (isLocked()) return;
  const next = VIEW_ORDER[(VIEW_ORDER.indexOf(viewMode) + 1) % VIEW_ORDER.length];
  setViewMode(next);
}

// Vista actual (la lee el modo enfoque para restaurarla al salir).
export function getViewMode(): ViewMode {
  return viewMode;
}

// Quien es dueño de la vista mientras esta bloqueada (el modo enfoque). Se inyecta desde
// el arranque para que este modulo no dependa de writing-modes.ts.
let isLocked: () => boolean = () => false;

export function initViewMode(locked: () => boolean): void {
  isLocked = locked;
  // Clic directo en los botones de la barra. Igual que el ciclo: el enfoque manda, asi que si esta
  // activo el clic no hace nada (el CSS ademas los bloquea con pointer-events: none).
  for (const btn of viewBtns) {
    btn.addEventListener("click", () => {
      if (isLocked()) return;
      setViewMode(btn.dataset.view as ViewMode);
    });
  }

  // Estado inicial: split (el default validado). Fija la clase del body y el resaltado del boton.
  setViewMode("split");
}
