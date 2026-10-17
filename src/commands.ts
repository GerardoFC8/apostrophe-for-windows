// Comandos de la app que actuan sobre el texto del editor apoyandose en el nucleo Rust:
// tablas (M17), imagenes y exportacion (M12). No conocen al documento (nombre, dirty):
// solo leen y escriben el texto de la vista.
import { invoke } from "@tauri-apps/api/core";
import { view, getText } from "./editor";
import { exportTheme } from "./preview";
import { flashStatus, showStatus } from "./status";
import { t } from "./i18n";

const exportBtn = document.querySelector<HTMLButtonElement>("#export-btn")!;

// --- M17: tablas ---

// `rows` son filas de CUERPO: la grilla cuenta la primera fila como el encabezado, que en
// Markdown siempre existe. Elegir 3x2 da encabezado + 2 filas.
export async function insertTable(rows: number, cols: number): Promise<void> {
  try {
    const table = await invoke<string>("make_table", { rows: rows - 1, cols });
    const sel = view.state.selection.main;
    const line = view.state.doc.lineAt(sel.from);
    // Una tabla es un bloque: si el cursor está a mitad de un renglón con texto, se abre
    // uno nuevo en vez de partirlo por la mitad.
    const prefix = line.text.slice(0, sel.from - line.from).trim() === "" ? "" : "\n\n";
    const insert = `${prefix}${table}`;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert },
      // El cursor queda en la primera celda del encabezado, listo para escribir.
      selection: { anchor: sel.from + prefix.length + 2 },
      scrollIntoView: true,
    });
    view.focus();
  } catch (err) {
    flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
  }
}

// Elige una imagen con el selector nativo e inserta `![](ruta)` en el cursor.
//
// La ruta la arma Rust con el dominio: relativa al documento siempre que se pueda, para que
// el `.md` siga siendo portable. Aquí solo se coloca el texto y se deja el cursor entre los
// corchetes del alt, que es lo primero que conviene escribir.
export async function insertImage(): Promise<void> {
  try {
    const snippet = await invoke<string | null>("pick_image", {
      filterLabel: t("dialog.imageFilter"),
    });
    if (!snippet) return; // el usuario canceló el selector
    const sel = view.state.selection.main;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: snippet },
      // Entre `![` y `](`: el alt va vacío justamente para caer acá.
      selection: { anchor: sel.from + 2 },
      scrollIntoView: true,
    });
    view.focus();
  } catch (err) {
    flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
  }
}

// Realinea la tabla sobre la que está el cursor. El bloque se delimita por renglones en
// blanco; el dominio decide si eso es o no una tabla y devuelve null cuando no lo es, así
// que nunca se reescribe algo que no entendimos.
export async function alignTableAtCursor(): Promise<boolean> {
  const doc = view.state.doc;
  const cursor = doc.lineAt(view.state.selection.main.from);
  let first = cursor.number;
  let last = cursor.number;
  while (first > 1 && doc.line(first - 1).text.trim() !== "") first--;
  while (last < doc.lines && doc.line(last + 1).text.trim() !== "") last++;

  const from = doc.line(first).from;
  const to = doc.line(last).to;
  const block = doc.sliceString(from, to);
  try {
    const aligned = await invoke<string | null>("align_table", { text: block });
    if (!aligned) {
      flashStatus(t("status.notATable"));
      return true;
    }
    if (aligned !== block) {
      view.dispatch({ changes: { from, to, insert: aligned } });
      flashStatus(t("status.tableAligned"));
    }
  } catch (err) {
    flashStatus(`${t("error.generic")}: ${String(err)}`, 4000);
  }
  return true;
}

// M12: exportar (dialogo nativo con un filtro por formato + pandoc sidecar). El formato
// lo decide la extension que elijas en el dialogo; el backend la traduce y avisa si no
// esta soportada.
export async function exportDoc(): Promise<void> {
  showStatus(t("status.exporting")); // en-progreso (sin auto-limpiar hasta terminar)
  try {
    const { css, accent } = exportTheme();
    const out = await invoke<string | null>("export_document", {
      text: getText(),
      css,
      accent,
    });
    if (out) flashStatus(t("status.exported"));
    else showStatus(""); // cancelado
  } catch (err) {
    flashStatus(`${t("error.export")}: ${String(err)}`, 4000);
  }
}

export function initExportButton(): void {
  exportBtn.addEventListener("click", () => void exportDoc());
}
