// Archivos soltados sobre la ventana. Los documentos Markdown se abren en su pestaña y las
// imágenes se insertan donde se soltaron, con la misma ruta relativa que "Insertar imagen".
// Qué es cada archivo lo decide el backend (`plan_drop`); aquí solo se coloca el resultado.
//
// Tauri entrega el soltado como evento propio (con las rutas reales del disco): mientras
// `dragDropEnabled` siga en su valor por defecto, el webview no recibe el `drop` de HTML5.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { PhysicalPosition } from "@tauri-apps/api/dpi";
import { view } from "./editor";
import { activeDocId } from "./tabs";
import { flashStatus } from "./status";
import { t } from "./i18n";

interface DropPlan {
  open: string[];
  images: string[];
  ignored: number;
}

// Posición del documento bajo el punto donde se soltó, o el cursor si fue fuera del texto
// (la barra de pestañas, la vista previa).
function dropOffset(position: PhysicalPosition): number {
  const { x, y } = position.toLogical(window.devicePixelRatio);
  return view.posAtCoords({ x, y }) ?? view.state.selection.main.head;
}

function insertImages(snippets: string[], at: number): void {
  const insert = snippets.join("\n\n");
  view.dispatch({
    changes: { from: at, insert },
    // Con una sola imagen el cursor cae en el alt, como al insertarla desde el menú.
    selection: { anchor: snippets.length === 1 ? at + 2 : at + insert.length },
    scrollIntoView: true,
  });
  view.focus();
}

// `openMarkdown` se inyecta desde el arranque para no depender de document-session.ts.
export function initFileDrop(openMarkdown: (path: string) => Promise<void>): void {
  void getCurrentWebview().onDragDropEvent(async (event) => {
    if (event.payload.type !== "drop") return;
    const { paths, position } = event.payload;
    try {
      const plan = await invoke<DropPlan>("plan_drop", { paths, docId: activeDocId() });
      // Primero las imágenes: van al documento activo, y abrir un .md cambia de pestaña.
      if (plan.images.length > 0) insertImages(plan.images, dropOffset(position));
      for (const path of plan.open) await openMarkdown(path);
      if (plan.ignored > 0) flashStatus(t("status.dropIgnored"), 4000);
    } catch (err) {
      flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
    }
  });
}
