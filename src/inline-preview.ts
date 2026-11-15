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

function render(target: InlineTarget, labels: InlineLabels, docId: number | undefined): HTMLElement {
  switch (target.kind) {
    case "link":
      return row(labels.link, target.url);
    case "image":
      return imageRow(target.url, target.alt, labels, docId);
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
///
/// `docId` es el documento de la pestaña activa: las rutas relativas se resuelven contra
/// SU carpeta, no contra la del documento que el backend tenga como activo.
function imageRow(
  url: string,
  alt: string,
  labels: InlineLabels,
  docId: number | undefined,
): HTMLElement {
  const wrap = row(labels.image, url);
  const value = wrap.querySelector<HTMLElement>(".inline-preview-value")!;
  void invoke<string>("render_image", { url, docId })
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

/// Fórmula renderizada como MathML (ADR-0003), que WebView2 dibuja de forma nativa y toma el
/// color del texto del tema. Se pinta primero la fuente y se sustituye cuando llega: un
/// popover que aparece vacío y luego salta se ve peor que uno que muestra algo desde el
/// primer frame.
function mathRow(source: string, labels: InlineLabels): HTMLElement {
  const wrap = row(labels.math, source);
  const value = wrap.querySelector<HTMLElement>(".inline-preview-value")!;
  void invoke<string>("render_math", { source })
    .then((mathml) => {
      const math = safeMathml(mathml);
      if (!math) return;
      math.classList.add("inline-preview-math");
      value.replaceChildren(math);
    })
    .catch(() => {
      /* si la fórmula no es válida queda la fuente, que ya es informativa */
    });
  return wrap;
}

const MATHML_NS = "http://www.w3.org/1998/Math/MathML";

/// El elemento `<math>` de `mathml`, o `null` si trae algo que no sea MathML puro. El popover
/// vive en la ventana principal, que tiene acceso a la API de Tauri: aunque el MathML lo arma
/// el backend con el texto escapado, aquí no entra nada que pueda ejecutar código (elementos
/// de otro espacio de nombres, `on…`, enlaces).
function safeMathml(mathml: string): Element | null {
  const doc = new DOMParser().parseFromString(mathml, "text/html");
  const math = doc.body.firstElementChild;
  if (!math || math.namespaceURI !== MATHML_NS || math.localName !== "math") return null;
  for (const el of [math, ...math.querySelectorAll("*")]) {
    if (el.namespaceURI !== MATHML_NS) return null;
    for (const attr of el.attributes) {
      const name = attr.name.toLowerCase();
      if (name.startsWith("on") || name.includes("href") || name === "src") return null;
    }
  }
  return document.importNode(math, true);
}

/// Extensión de CodeMirror que resuelve y muestra el popover.
///
/// Nota sobre offsets: CodeMirror cuenta en unidades UTF-16 y el dominio en caracteres
/// Unicode. Coinciden para todo el plano básico (acentos, eñes, CJK común) y solo divergen
/// con caracteres del plano astral, como los emoji. Es la MISMA convención que ya usan el
/// resaltado y el corrector; cambiarla aquí sola crearía una segunda verdad.
export function inlinePreviewExtension(
  getLabels: () => InlineLabels,
  getDocId: () => number | undefined,
): Extension {
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
    const dom = render(target, getLabels(), getDocId());
    return { pos, above: true, create: () => ({ dom }) };
  });
}
