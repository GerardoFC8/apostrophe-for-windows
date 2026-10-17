// El editor CodeMirror 6: una sola EditorView, sus extensiones y el acceso al texto.
// No conoce al documento (nombre, ruta, dirty) ni al backend: avisa de los cambios por el
// callback `onDocChanged` y el resto de la app decide que hacer con ellos.
//
// ADR-0001 D1: el highlighting NO se hace con la gramatica Lezer de CodeMirror —
// vendra del nucleo Rust como spans (post-MVP). Por eso NO hay @codemirror/lang-markdown.
import { EditorState, Transaction } from "@codemirror/state";
import { EditorView, drawSelection, highlightActiveLine, keymap } from "@codemirror/view";
import { history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { search, searchKeymap, openSearchPanel } from "@codemirror/search";
import { spellcheckExtension } from "./spellcheck";
import { focusCompartment } from "./focus-mode";
import { markupExtension } from "./markup-highlight";
import { inlinePreviewExtension } from "./inline-preview";
import { hemingwayCompartment } from "./hemingway";
import { toggleInlineFormat, setHeadingLevel, toggleList, handleListEnter } from "./markdown-format";
import { t } from "./i18n";

const editorParent = document.querySelector<HTMLElement>("#editor")!;

// Tema del editor (Adwaita-ish): hereda los colores via CSS vars (cambian con data-theme),
// tipografia JetBrains Mono. El acento es el azul Adwaita.
const editorTheme = EditorView.theme({
  "&": { height: "100%", color: "var(--editor-fg)" },
  ".cm-scroller": {
    fontFamily: '"JetBrains Mono Variable", "Cascadia Code", ui-monospace, monospace',
    fontSize: "14px",
    lineHeight: "1.6",
  },
  ".cm-content": { padding: "1rem", caretColor: "var(--accent)" },
  "&.cm-focused": { outline: "none" },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--accent)",
    borderLeftWidth: "2px",
  },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
    backgroundColor: "var(--selection)",
  },
  ".cm-activeLine": { backgroundColor: "var(--active-line)" },
});

// --- M7: traducciones del panel de busqueda de @codemirror/search (castellano neutro) ---
// EditorState.phrases.of({}) registra un mapa de traduccion; state.phrase() lo consulta.
// Cadenas verificadas contra el codigo fuente de @codemirror/search 6.7.0.
const phrasesES = EditorState.phrases.of({
  "Find": "Buscar",
  "Replace": "Reemplazar",
  "next": "siguiente",
  "previous": "anterior",
  "all": "todo",
  "match case": "distinguir mayúsculas",
  "by word": "palabra completa",
  "regexp": "expresión regular",
  "replace": "reemplazar",
  "replace all": "reemplazar todo",
  "close": "cerrar",
  "current match": "coincidencia actual",
  "replaced $ matches": "$ coincidencias reemplazadas",
  "replaced match on line $": "coincidencia reemplazada en la línea $",
  "on line": "en la línea",
  "Go to line": "Ir a la línea",
  "go": "ir",
});

// Lo que el editor necesita del resto de la app. Se inyecta (en vez de importarlo) para
// que editor.ts no dependa de los modulos que a su vez dependen de el.
export interface EditorDeps {
  /** Se llama en cada transaccion que cambia el documento (el debounce lo decide quien llama). */
  onDocChanged: () => void;
  /** M17: realinea la tabla bajo el cursor (Ctrl+Shift+T). */
  onAlignTable: () => Promise<unknown>;
}

// Keymap de formato Markdown (alta prioridad: va ANTES de defaultKeymap).
// Ctrl+F y Ctrl+H los maneja searchKeymap; aqui solo van los de formato.
function formatKeymap(onAlignTable: EditorDeps["onAlignTable"]) {
  return keymap.of([
    // Negrita: Ctrl+B — marcador ** (upstream: <Primary>b)
    { key: "Mod-b", run: (v) => toggleInlineFormat(v, "**"), preventDefault: true },
    // Cursiva: Ctrl+I — marcador _ (underscore, como el upstream)
    { key: "Mod-i", run: (v) => toggleInlineFormat(v, "_"), preventDefault: true },
    // Tachado: Ctrl+Shift+D — marcador ~~ (upstream: <Shift><Primary>D)
    { key: "Mod-Shift-d", run: (v) => toggleInlineFormat(v, "~~"), preventDefault: true },
    // M17: realinear la tabla bajo el cursor — Ctrl+Shift+T (Ctrl+T ya es Hemingway).
    { key: "Mod-Shift-t", run: () => { void onAlignTable(); return true; }, preventDefault: true },
    // Lista desordenada: Ctrl+U — prefijo "- " (upstream: <Primary>u)
    { key: "Mod-u", run: toggleList, preventDefault: true },
    // Encabezados Ctrl+1..Ctrl+4 (upstream: "<Primary>1 4" = cicla niveles 1-4)
    { key: "Mod-1", run: (v) => setHeadingLevel(v, 1), preventDefault: true },
    { key: "Mod-2", run: (v) => setHeadingLevel(v, 2), preventDefault: true },
    { key: "Mod-3", run: (v) => setHeadingLevel(v, 3), preventDefault: true },
    { key: "Mod-4", run: (v) => setHeadingLevel(v, 4), preventDefault: true },
    // Auto-continuacion de listas al Enter (alta prioridad sobre defaultKeymap).
    { key: "Enter", run: handleListEnter },
    // Ctrl+H: abrir buscar+reemplazar (searchKeymap solo incluye Ctrl+F; H lo agregamos aqui).
    { key: "Mod-h", run: openSearchPanel, preventDefault: true },
  ]);
}

// La vista se crea en createEditor (la llama el arranque, en el mismo punto donde antes se
// construia), no al evaluar el modulo. Los demas modulos la leen por este binding vivo.
export let view: EditorView;

// CodeMirror 6: editor de verdad (undo/redo, line-wrap, cierre de brackets) SIN
// gramatica Lezer (ADR-0001 D1). Los atajos de archivo van a nivel window (shortcuts.ts).
// Arranca VACIO (distraction-free): sin texto de ejemplo. Si la app se abre via
// asociacion de archivo (.md), el contenido lo carga el arranque (restoreInitialDocument).
export function createEditor(deps: EditorDeps): EditorView {
  view = new EditorView({
    doc: "",
    parent: editorParent,
    extensions: [
      history(),
      // M9.2: filtro de Hemingway (arranca vacio; Ctrl+T lo activa).
      hemingwayCompartment.of([]),
      drawSelection(),
      highlightActiveLine(),
      // M9.1: atenuado por parrafo del modo enfoque (arranca vacio; Ctrl+D lo activa).
      focusCompartment.of([]),
      EditorView.lineWrapping,
      closeBrackets(),
      // Traducciones del panel de busqueda al castellano.
      phrasesES,
      // Panel de busqueda y reemplazo (arriba del editor, top:true).
      search({ top: true }),
      // Keymaps en orden de prioridad: busqueda > formato MD > brackets > default > historial.
      // searchKeymap primero: Ctrl+F y Ctrl+G tienen prioridad sobre cualquier otro.
      keymap.of([...searchKeymap]),
      // Comandos de formato Markdown (Ctrl+B/I, Ctrl+Shift+D, Ctrl+U, Ctrl+1-4, Enter).
      // Va antes de defaultKeymap para que el Enter de listas tenga prioridad.
      formatKeymap(deps.onAlignTable),
      keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap]),
      editorTheme,
      spellcheckExtension,
      // M10: resaltado de Markdown (spans del nucleo Rust). Va tras el spellcheck; sus marks
      // pueden solaparse con las ondulitas y con el atenuado de focus sin conflicto.
      markupExtension,
      // M19: popover de vista previa inline. Las etiquetas se resuelven en CADA popover (no
      // una vez al construir) para que sigan al idioma activo sin recrear el editor.
      inlinePreviewExtension(() => ({
        link: t("inline.link"),
        image: t("inline.image"),
        footnote: t("inline.footnote"),
        footnoteMissing: t("inline.footnoteMissing"),
        math: t("inline.math"),
        // Se lee en cada popover, no una vez: así sigue al tema activo sin recrear el editor.
        mathColor: getComputedStyle(document.documentElement)
          .getPropertyValue("--editor-fg")
          .trim(),
        imageProblem: {
          remote: t("inline.imageRemote"),
          "no-base-dir": t("inline.imageNoBaseDir"),
          "not-found": t("inline.imageNotFound"),
          "too-large": t("inline.imageTooLarge"),
          unsupported: t("inline.imageUnsupported"),
          unreadable: t("inline.imageUnreadable"),
        },
      })),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) deps.onDocChanged();
      }),
    ],
  });
  return view;
}

export function getText(): string {
  return view.state.doc.toString();
}

// Reemplaza TODO el contenido del editor (al abrir un archivo). `addToHistory: false`
// para que Ctrl+Z tras abrir NO revierta al doc anterior (desincronizaria editor/store).
export function setEditorText(text: string): void {
  view.dispatch({
    changes: { from: 0, to: view.state.doc.length, insert: text },
    annotations: Transaction.addToHistory.of(false),
  });
}
