// Popover de vista previa inline (M19). Espejo de `inline_preview.py` del upstream: al
// posar el cursor sobre un enlace, una imagen, una nota al pie o una fórmula, se muestra su
// contenido sin salir del editor.
//
// La resolución de QUÉ hay bajo el cursor vive en el dominio
// (`apostrophe_core::inline::resolve_at`); aquí solo se decide cómo mostrarlo. Es la misma
// separación que usa el resaltado: si el popover reconociera markdown por su cuenta,
// terminaría discrepando del resto de la app.

import { hoverTooltip } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import { invoke } from "@tauri-apps/api/core";

/// Espejo de `InlineTarget` del dominio. `kind` es el discriminante que serializa serde.
type InlineTarget =
  | { kind: "link"; url: string; text: string }
  | { kind: "image"; url: string; alt: string }
  | { kind: "footnote"; id: string; body: string }
  | { kind: "math"; source: string };

/// Etiquetas del popover. Las inyecta main.ts desde el catálogo de i18n: este módulo no
/// importa i18n para no acoplar la extensión del editor al idioma (mismo criterio que
/// spellcheck.ts con sus sugerencias).
export interface InlineLabels {
  link: string;
  image: string;
  footnote: string;
  footnoteMissing: string;
  math: string;
  /// Color con el que se dibuja la fórmula. Sale de la paleta del tema activo: typst pinta
  /// en negro por defecto, que en tema oscuro sería una fórmula invisible.
  mathColor: string;
}

function row(label: string, value: string): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "inline-preview";
  const head = document.createElement("div");
  head.className = "inline-preview-label";
  head.textContent = label;
  const body = document.createElement("div");
  body.className = "inline-preview-value";
  body.textContent = value; // textContent, NUNCA innerHTML: el contenido viene del documento
  wrap.append(head, body);
  return wrap;
}

function render(target: InlineTarget, labels: InlineLabels): HTMLElement {
  switch (target.kind) {
    case "link":
      return row(labels.link, target.url);
    case "image":
      // Todavía se muestra la ruta y no la imagen. NO es una limitación de seguridad: el
      // mismo camino que usan las fórmulas sirve — Rust lee el archivo y devuelve un data
      // URI, sin darle acceso al disco al webview. Falta acotar tamaño máximo y formatos
      // aceptados, y eso es su propio slice.
      return row(labels.image, target.url);
    case "footnote":
      return row(
        labels.footnote,
        target.body || labels.footnoteMissing // sin definición: se avisa, no se calla
      );
    case "math":
      return mathRow(target.source, labels);
  }
}

/// Fórmula renderizada. Se pinta primero la fuente y se sustituye por el SVG cuando llega:
/// el render tarda ~64 ms (dos procesos) y un popover que aparece vacío y luego salta se ve
/// peor que uno que muestra algo desde el primer frame.
function mathRow(source: string, labels: InlineLabels): HTMLElement {
  const wrap = row(labels.math, source);
  const value = wrap.querySelector<HTMLElement>(".inline-preview-value")!;
  void invoke<string>("render_math", { source, color: labels.mathColor })
    .then((dataUri) => {
      // <img> y NO innerHTML: un SVG incrustado así no ejecuta scripts que pudiera traer.
      const img = document.createElement("img");
      img.className = "inline-preview-math";
      img.src = dataUri;
      img.alt = source;
      value.replaceChildren(img);
    })
    .catch(() => {
      /* si el render falla queda la fuente, que ya es informativa */
    });
  return wrap;
}

/// Extensión de CodeMirror que resuelve y muestra el popover.
///
/// Nota sobre offsets: CodeMirror cuenta en unidades UTF-16 y el dominio en caracteres
/// Unicode. Coinciden para todo el plano básico (acentos, eñes, CJK común) y solo divergen
/// con caracteres del plano astral, como los emoji. Es la MISMA convención que ya usan el
/// resaltado y el corrector; cambiarla aquí sola crearía una segunda verdad.
export function inlinePreviewExtension(getLabels: () => InlineLabels): Extension {
  return hoverTooltip(async (view, pos) => {
    let target: InlineTarget | null = null;
    try {
      target = await invoke<InlineTarget | null>("inline_target", {
        text: view.state.doc.toString(),
        offset: pos,
      });
    } catch {
      return null; // un popover que falla no debe interrumpir la escritura
    }
    if (!target) return null;
    const dom = render(target, getLabels());
    return { pos, above: true, create: () => ({ dom }) };
  });
}
