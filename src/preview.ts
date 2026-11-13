// Panel de vista previa: renderizado via pandoc (backend), hoja de estilos derivada del
// tema, ventana desacoplada (M18), tema de exportacion y modo de vista (M13).
import { invoke } from "@tauri-apps/api/core";
import { listen, emit } from "@tauri-apps/api/event";
import { view, getText } from "./editor";
import { activeDocId } from "./tabs";
import { flashStatus } from "./status";
import { getLocale, t } from "./i18n";
import { renderMermaid } from "./mermaid-diagrams";
import { FRAME_SCRIPT, frameMessage } from "./preview-frame";
import type { FrameMessage } from "./preview-frame";
import type { SyncPosition } from "./scroll-sync";

const preview = document.querySelector<HTMLIFrameElement>("#preview")!;

// Ultimo HTML crudo de pandoc, para re-tematizar el preview al cambiar de tema sin
// volver a invocar pandoc.
let lastPreviewHtml = "";
// Lo que se muestra: el HTML de pandoc con los diagramas Mermaid ya dibujados.
let lastShownHtml = "";

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
    pre{padding:.8em 1em;overflow:auto;border:1px solid ${v("--border")};border-radius:10px;}
    div.sourceCode{margin:1em 0;overflow:auto;background:${v("--preview-code-bg")};border:1px solid ${v("--border")};border-radius:10px;}
    div.sourceCode pre{margin:0;border:0;border-radius:0;background:none;overflow:visible;}
    pre code{background:none;padding:0;}
    blockquote{border-left:3px solid ${v("--border")};margin-left:0;padding-left:1em;opacity:.85;}
    h1,h2,h3{line-height:1.25;}
    img{max-width:100%;}
    .mermaid-diagram{margin:1em 0;text-align:center;}
    .mermaid-diagram svg{max-width:100%;height:auto;}
    .mermaid-error-message{margin-top:-.4em;color:#e01b24;font-size:.85em;}
  </style>${syntaxStylesheet(v)}`;
}

// Clases de token del resaltado de pandoc (skylighting), agrupadas por el color que usan.
const SYNTAX_GROUPS: [string, string[]][] = [
  ["--syntax-keyword", ["kw", "cf", "im"]],
  ["--syntax-type", ["dt", "bu", "wa"]],
  ["--syntax-function", ["fu", "at"]],
  ["--syntax-string", ["st", "ch", "vs", "ss", "sc", "sp"]],
  ["--syntax-number", ["dv", "bn", "fl", "cn", "pp"]],
  ["--syntax-comment", ["co", "do", "an", "cv", "in"]],
  ["--syntax-variable", ["va", "ex"]],
  ["--syntax-other", ["ot", "op"]],
  ["--syntax-error", ["al", "er"]],
];

// Reemplaza los colores del resaltado de pandoc cuando el tema define los suyos (solo el
// oscuro). Va después de la hoja de pandoc en el <head>, así que con la misma especificidad
// gana. Sin las variables (temas claros, exportación) no agrega nada.
function syntaxStylesheet(v: (name: string) => string): string {
  if (!v("--syntax-keyword")) return "";
  const rules = SYNTAX_GROUPS.map(
    ([variable, classes]) =>
      `${classes.map((c) => `code span.${c}`).join(",")}{color:${v(variable)};}`,
  );
  return `<style>${rules.join("")}code span.co,code span.do{font-style:italic;}</style>`;
}

// Barras de desplazamiento de la vista previa, iguales a las de la app (styles.css): el
// iframe es otro documento y no hereda esas reglas. `color-scheme` hace que lo que siga
// siendo nativo (controles, esquinas) use la variante clara u oscura del tema. Solo van en la
// vista previa: el HTML exportado no las lleva.
function scrollbarStyle(v: (name: string) => string): string {
  const dark = document.documentElement.dataset.theme === "dark";
  return `<style>
    :root{color-scheme:${dark ? "dark" : "light"};}
    ::-webkit-scrollbar{width:12px;height:12px;}
    ::-webkit-scrollbar-track,::-webkit-scrollbar-corner{background:transparent;}
    ::-webkit-scrollbar-thumb{background-color:${v("--scrollbar-thumb")};background-clip:content-box;border:3px solid transparent;border-radius:999px;min-height:40px;}
    ::-webkit-scrollbar-thumb:hover{background-color:${v("--scrollbar-thumb-hover")};}
    pre::-webkit-scrollbar{height:10px;}
  </style>`;
}

// El zoom del texto (zoom.ts) solo va en la vista previa: lo que se exporta sale siempre a
// tamaño normal.
function previewStyle(): string {
  const cs = getComputedStyle(document.documentElement);
  const zoom = cs.getPropertyValue("--text-zoom").trim() || "1";
  const v = (name: string) => cs.getPropertyValue(name).trim();
  return markdownStylesheet(v) + `<style>body{font-size:calc(100% * ${zoom});}</style>` + scrollbarStyle(v);
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

// Inyecta el <style> del tema y el script de enlaces en el HTML standalone de pandoc (inline
// -> anda en el iframe srcdoc sandboxeado).
function withTheme(html: string): string {
  const head = previewStyle() + FRAME_SCRIPT;
  return html.includes("</head>") ? html.replace("</head>", `${head}</head>`) : head + html;
}

const LINK_ERRORS: Record<string, () => string> = {
  "no-base-dir": () => t("status.linkNoBaseDir"),
  "not-found": () => t("status.linkNotFound"),
  unsupported: () => t("status.linkUnsupported"),
};

// Sigue un enlace del preview. A donde lleva lo decide el backend; aca solo se abre la
// pestaña cuando apunta a otro Markdown y se avisa si no se pudo.
async function followLink(href: string, openMarkdown: (path: string) => Promise<void>): Promise<void> {
  try {
    const path = await invoke<string | null>("open_preview_link", { href, docId: activeDocId() });
    if (path) await openMarkdown(path);
  } catch (err) {
    flashStatus((LINK_ERRORS[String(err)] ?? (() => t("status.linkFailed")))(), 4000);
  }
}

/// Lo que la vista previa necesita del resto de la app. Se inyecta desde el arranque para que
/// este modulo no dependa de document-session.ts ni de zoom.ts.
export interface PreviewFrameDeps {
  openMarkdown: (path: string) => Promise<void>;
  zoomStep: (step: 1 | -1 | 0) => void;
  zoomWheel: (deltaY: number) => void;
  /** La vista previa se recargó y espera la posición del scroll sincronizado. */
  onReady: () => void;
  /** La vista previa se desplazó (a mano o por la sincronización): su posición actual. */
  onScrolled: (pos: SyncPosition) => void;
}

function handleFrameMessage(msg: FrameMessage, deps: PreviewFrameDeps): void {
  if (msg.kind === "ready") deps.onReady();
  else if (msg.kind === "scrolled") deps.onScrolled(msg.pos);
  else if (msg.kind === "link") void followLink(msg.href, deps.openMarkdown);
  else if (msg.kind === "zoom") deps.zoomStep(msg.step);
  else deps.zoomWheel(msg.deltaY);
}

// Cablea lo que pide el iframe del panel (ver preview-frame.ts) y lo mismo cuando llega de la
// ventana desacoplada, que no resuelve nada por su cuenta: reenvia el mensaje y se atiende
// aca, con el documento activo como base de los enlaces.
export function initPreviewFrame(deps: PreviewFrameDeps): void {
  window.addEventListener("message", (e) => {
    const msg = frameMessage(e, preview);
    if (msg) handleFrameMessage(msg, deps);
  });
  // La ventana desacoplada atiende su propio "listo" y su propia posición (ver
  // preview-window.ts): su scroll es independiente del panel.
  void listen<FrameMessage>("preview-frame", (event) => {
    const kind = event.payload.kind;
    if (kind !== "ready" && kind !== "scrolled") handleFrameMessage(event.payload, deps);
  });
}

// La ventana desacoplada está (o pudo quedar) abierta. Mientras no, la posición no se emite:
// el scroll sincronizado manda una por cuadro y cada `emit` es un viaje por IPC.
let previewWindowOpen = false;

/// Manda la posición del scroll sincronizado a la vista previa y a la ventana desacoplada.
export function postSync(pos: SyncPosition): void {
  preview.contentWindow?.postMessage({ elisionSync: pos }, "*");
  if (previewWindowOpen) void emit("preview-sync", pos);
}

// Dibuja los diagramas Mermaid de `html` (con el tema activo) y lo muestra. `seq` descarta el
// resultado si mientras tanto se pidió otro render.
async function show(html: string, seq: number): Promise<void> {
  const dark = document.documentElement.dataset.theme === "dark";
  const shown = await renderMermaid(html, dark);
  if (seq !== renderSeq) return;
  lastShownHtml = shown;
  preview.srcdoc = withTheme(shown);
  pushToPreviewWindow(); // M18: si hay ventana desacoplada, se actualiza con el mismo HTML
}

// Re-tematizar el preview ya renderizado (editor/chrome se actualizan solos via vars).
// Lo llama applyTheme (theme.ts) al cambiar de tema, y el zoom. Los diagramas Mermaid se
// vuelven a dibujar con los colores del tema (los que no cambian salen de la caché).
export function refreshPreviewTheme(): void {
  if (lastPreviewHtml) void show(lastPreviewHtml, ++renderSeq);
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
    await show(html, seq);
  } catch (err) {
    if (seq !== renderSeq) return;
    flashStatus(t("status.previewFailed"), 4000);
    // Con la hoja del tema: en el oscuro, el error no debe aparecer en un recuadro blanco.
    preview.srcdoc = withTheme(
      `<pre style="color:#e01b24;background:none;border:0;white-space:pre-wrap">${escapeHtml(String(err))}</pre>`,
    );
  }
}

// --- M18: ventana de vista previa desacoplada ---

const previewWindowBtn = document.querySelector<HTMLButtonElement>("#preview-window-btn")!;

// Lo que recibe la ventana desacoplada. El color de fondo viaja aparte del HTML porque esa
// ventana lo pinta ella misma, con la opacidad que elija el usuario; el idioma, para las
// etiquetas de sus controles.
export interface PreviewPayload {
  html: string;
  background: string;
  locale: string;
}

// Manda el HTML actual a la ventana desacoplada. Se emite SIEMPRE, aunque no haya ventana:
// un evento sin destinatario no cuesta nada y evita tener que rastrear si esta abierta.
function pushToPreviewWindow(): void {
  if (!lastShownHtml) return;
  const payload: PreviewPayload = {
    html: withTheme(lastShownHtml),
    background: getComputedStyle(document.documentElement).getPropertyValue("--preview-bg").trim(),
    locale: getLocale(),
  };
  void emit("preview-html", payload);
}

async function togglePreviewWindow(): Promise<void> {
  try {
    const open = await invoke<boolean>("toggle_preview_window");
    previewWindowOpen = open;
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
  void listen("preview-window-ready", () => {
    previewWindowOpen = true;
    pushToPreviewWindow();
  });
  // Cerrada con su propia X: el botón deja de verse activo (lo avisa el backend).
  void listen("preview-window-closed", () => {
    previewWindowOpen = false;
    previewWindowBtn.setAttribute("aria-pressed", "false");
  });

  previewWindowBtn.addEventListener("click", () => void togglePreviewWindow());
  // Si la ventana quedo abierta de una sesion anterior (o de un recargar en dev), el boton
  // tiene que reflejarlo en vez de mentir.
  void invoke<boolean>("preview_window_open")
    .then((open) => {
      previewWindowOpen ||= open;
      previewWindowBtn.setAttribute("aria-pressed", String(open));
    })
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
