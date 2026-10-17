// Comandos de formato Markdown sobre CodeMirror 6 (M7). Puros: reciben la vista, leen su
// estado y despachan cambios; no tocan el backend ni el DOM de la app. El keymap que los
// enlaza a teclas vive en editor.ts.
import type { EditorView } from "@codemirror/view";

// --- M7: comandos de formato Markdown (CodeMirror commands) ---
//
// Fuente de atajos: /tmp/apostrophe-upstream/data/ui/Shortcuts.ui y
// text_view_format_inserter.py del upstream. Paridad con el original GNOME:
//   Negrita    → Ctrl+B  — marcador **  (igual al upstream)
//   Cursiva    → Ctrl+I  — marcador _   (underscore, igual al upstream)
//   Tachado    → Ctrl+Shift+D — marcador ~~ (upstream usa <Shift><Primary>D)
//   Lista      → Ctrl+U  — prefijo "- " (igual al upstream)
//   Encabezado → Ctrl+1..Ctrl+4 — fija nivel (upstream: "Header <Primary>1 4")
//   Buscar     → Ctrl+F  (searchKeymap, igual al upstream "win.find")
//   B+Reemplazar → Ctrl+H — binding adicional (upstream: "win.find_replace")

/** Envuelve/desenvuelve la seleccion con un marcador inline (negrita, cursiva, tachado).
 *  Si no hay seleccion, inserta los dos marcadores y deja el cursor en el medio. */
export function toggleInlineFormat(view: EditorView, marker: string): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const markerLen = marker.length;

  if (sel.empty) {
    // Sin seleccion: inserta el marcador doble y posiciona el cursor en el medio.
    const insert = marker + marker;
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert },
      selection: { anchor: sel.from + markerLen },
    });
    return true;
  }

  const selectedText = state.sliceDoc(sel.from, sel.to);

  // Comprueba si la seleccion YA esta envuelta (busca tambien fuera de la seleccion).
  const extFrom = Math.max(0, sel.from - markerLen);
  const extTo   = Math.min(state.doc.length, sel.to + markerLen);
  const extText = state.sliceDoc(extFrom, extTo);

  const outerWrapped =
    extText.startsWith(marker) && extText.endsWith(marker) && extFrom < sel.from;
  const innerWrapped =
    selectedText.startsWith(marker) && selectedText.endsWith(marker) &&
    selectedText.length >= markerLen * 2;

  if (outerWrapped) {
    // Quita los marcadores que estan FUERA de la seleccion.
    const inner = extText.slice(markerLen, extText.length - markerLen);
    view.dispatch({
      changes: { from: extFrom, to: extTo, insert: inner },
      selection: { anchor: extFrom, head: extFrom + inner.length },
    });
    return true;
  }

  if (innerWrapped) {
    // Quita los marcadores que estan DENTRO de la seleccion.
    const inner = selectedText.slice(markerLen, selectedText.length - markerLen);
    view.dispatch({
      changes: { from: sel.from, to: sel.to, insert: inner },
      selection: { anchor: sel.from, head: sel.from + inner.length },
    });
    return true;
  }

  // Envuelve la seleccion con el marcador.
  const wrapped = marker + selectedText + marker;
  view.dispatch({
    changes: { from: sel.from, to: sel.to, insert: wrapped },
    selection: { anchor: sel.from + markerLen, head: sel.from + markerLen + selectedText.length },
  });
  return true;
}

/** Fija o quita el nivel de encabezado en la(s) linea(s) de la seleccion.
 *  Si la linea ya tiene ese nivel, lo quita (toggle). Sigue la semantica del upstream:
 *  al repetir el mismo nivel se elimina el encabezado. */
export function setHeadingLevel(view: EditorView, level: number): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const startLine = state.doc.lineAt(sel.from);
  const endLine   = state.doc.lineAt(sel.to);

  const changes: { from: number; to: number; insert: string }[] = [];

  for (let ln = startLine.number; ln <= endLine.number; ln++) {
    const line = state.doc.line(ln);
    const text  = line.text;
    const match = text.match(/^(#{1,6}) /);

    if (match) {
      const currentLevel = match[1].length;
      if (currentLevel === level) {
        // Mismo nivel → eliminar encabezado (toggle off).
        changes.push({ from: line.from, to: line.from + currentLevel + 1, insert: "" });
      } else {
        // Cambiar a nuevo nivel.
        changes.push({
          from: line.from,
          to: line.from + currentLevel + 1,
          insert: "#".repeat(level) + " ",
        });
      }
    } else {
      // Sin encabezado → agregar.
      changes.push({ from: line.from, to: line.from, insert: "#".repeat(level) + " " });
    }
  }

  view.dispatch({ changes });
  return true;
}

/** Agrega/quita el prefijo de lista desordenada ("- ") en la(s) linea(s) seleccionadas.
 *  Sigue la semantica del upstream: si la linea ya tiene prefijo de lista, lo quita. */
export function toggleList(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  const startLine = state.doc.lineAt(sel.from);
  const endLine   = state.doc.lineAt(sel.to);

  const changes: { from: number; to: number; insert: string }[] = [];

  for (let ln = startLine.number; ln <= endLine.number; ln++) {
    const line = state.doc.line(ln);
    const text  = line.text;
    // Patron de lista desordenada: "- ", "* " o "+ " con posible sangria.
    const listMatch = text.match(/^(\s*)([-*+] )/);
    if (listMatch) {
      // Quitar el prefijo "- " (los 2 chars del marcador).
      const prefixStart = line.from + listMatch[1].length;
      changes.push({ from: prefixStart, to: prefixStart + 2, insert: "" });
    } else {
      // Agregar "- " al inicio de la linea (sin sangria, como el upstream).
      changes.push({ from: line.from, to: line.from, insert: "- " });
    }
  }

  view.dispatch({ changes });
  return true;
}

/** Auto-continuacion de listas al pulsar Enter.
 *  - Si la linea actual es un item de lista CON contenido → inserta nueva linea + prefijo.
 *  - Si el item esta VACIO (solo el prefijo) → elimina el prefijo y termina la lista.
 *  - Si no es lista → devuelve false para que actue el Enter por defecto.
 *  Soporta listas desordenadas (- / * / +) y ordenadas (N. / N)).
 *  Semantica verificada contra text_view_markup_handler.py del upstream. */
export function handleListEnter(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  if (!sel.empty) return false; // con seleccion, deja actuar al Enter por defecto

  const line = state.doc.lineAt(sel.from);
  const text  = line.text;

  // Lista desordenada: "  - texto", "  * texto", "  + texto"
  const unorderedMatch = text.match(/^(\s*)([-*+]) (.*)/);
  // Lista ordenada: "  1. texto", "  2) texto"
  const orderedMatch   = text.match(/^(\s*)(\d+)([.)]) (.*)/);

  if (unorderedMatch) {
    const indent  = unorderedMatch[1];
    const symbol  = unorderedMatch[2];
    const content = unorderedMatch[3];
    const prefixLen = indent.length + 2; // simbolo (1) + espacio (1)

    if (content.trim() === "") {
      // Item vacio: terminar la lista SOLO si el cursor esta despues del prefijo
      // completo. Si esta dentro del prefijo (pos 0/1), dejar el Enter por defecto
      // para no producir un dispatch vacio ni borrar a medias el prefijo.
      if (sel.from < line.from + prefixLen) return false;
      // Eliminar el prefijo (termina la lista).
      view.dispatch({
        changes: { from: line.from, to: sel.from, insert: "" },
        scrollIntoView: true,
      });
      return true;
    }
    // Item con contenido: nueva linea + mismo prefijo.
    view.dispatch({
      changes: { from: sel.from, to: sel.from, insert: `\n${indent}${symbol} ` },
      selection: { anchor: sel.from + indent.length + 3 },
      scrollIntoView: true,
    });
    return true;
  }

  if (orderedMatch) {
    const indent    = orderedMatch[1];
    const number    = parseInt(orderedMatch[2], 10);
    const delimiter = orderedMatch[3];
    const content   = orderedMatch[4];
    const prefixLen = indent.length + orderedMatch[2].length + 2; // numero + delim (1) + espacio (1)

    if (content.trim() === "") {
      // Item vacio: terminar la lista SOLO si el cursor esta despues del prefijo
      // completo (ver nota en la rama desordenada).
      if (sel.from < line.from + prefixLen) return false;
      // Eliminar el prefijo (termina la lista).
      view.dispatch({
        changes: { from: line.from, to: sel.from, insert: "" },
        scrollIntoView: true,
      });
      return true;
    }
    // Item con contenido: nueva linea + numero incrementado.
    const nextPrefix = `${indent}${number + 1}${delimiter} `;
    view.dispatch({
      changes: { from: sel.from, to: sel.from, insert: `\n${nextPrefix}` },
      selection: { anchor: sel.from + nextPrefix.length + 1 },
      scrollIntoView: true,
    });
    return true;
  }

  return false; // no es linea de lista: Enter por defecto
}
