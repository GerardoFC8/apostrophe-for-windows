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

/// Etiquetas del popover. Las inyecta editor.ts desde el catálogo de i18n: este módulo no
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
  /// Por qué no se pudo mostrar una imagen, por código. Rust manda el código y editor.ts lo
  /// traduce: la ruta sola dice DÓNDE mirar, no QUÉ está mal.
  imageProblem: Record<ImageProblem, string>;
}

/// Códigos que devuelve `render_image` cuando no puede incrustar la imagen.
export type ImageProblem =
  | "remote"
  | "no-base-dir"
  | "not-found"
  | "too-large"
  | "unsupported"
  | "unreadable";

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
      return imageRow(target.url, target.alt, labels);
    case "footnote":
      return row(
        labels.footnote,
        target.body || labels.footnoteMissing // sin definición: se avisa, no se calla
      );
    case "math":
      return mathRow(target.source, labels);
  }
}

/// Imagen incrustada. Se pinta primero la ruta y se sustituye por la imagen cuando llega,
/// igual que las fórmulas: leer y codificar un archivo lleva su tiempo y un popover que
/// aparece vacío se ve peor que uno que muestra algo desde el primer frame.
///
/// Si el render falla, debajo de la ruta se explica POR QUÉ: remota a propósito, documento
/// sin guardar, archivo ausente, demasiado grande o no reconocible como imagen. La ruta sola
/// dice dónde mirar pero no qué está mal, y sin el motivo la app parece rota sin explicación.
function imageRow(url: string, alt: string, labels: InlineLabels): HTMLElement {
  const wrap = row(labels.image, url);
  const value = wrap.querySelector<HTMLElement>(".inline-preview-value")!;
  void invoke<string>("render_image", { url })
    .then((dataUri) => {
      // <img> con data URI y NO innerHTML: un SVG incrustado así no ejecuta los scripts que
      // pudiera traer dentro. Mismo criterio que la fórmula.
      const img = document.createElement("img");
      img.className = "inline-preview-image";
      img.src = dataUri;
      img.alt = alt || url;
      value.replaceChildren(img);
    })
    .catch((err) => {
      const reason = labels.imageProblem[String(err) as ImageProblem];
      if (!reason) return; // código desconocido: mejor callar que mostrar una clave suelta
      const note = document.createElement("div");
      note.className = "inline-preview-note";
      note.textContent = reason;
      wrap.append(note);
    });
  return wrap;
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
