// Internacionalizacion (M15). UNICA fuente de la copia de la interfaz: ninguna cadena
// visible debe quedar escrita en index.html ni en el resto de los modulos.
//
// Alcance deliberado: la app soporta español e ingles. No se usa una libreria de i18n
// porque el catalogo es chico y cerrado; lo que si se aprovecha es el tipado — `Messages`
// obliga a que TODO idioma defina TODA la clave, asi una traduccion faltante es un error
// de compilacion y no un hueco que aparece en produccion.
//
// La copia del backend tambien vive aca: los textos de los dialogos nativos se le pasan
// a Rust por IPC (ver `confirmLabels`), en linea con ADR-0001 — el frontend PRESENTA, el
// adaptador solo muestra lo que recibe.

export const LOCALES = ["es", "en"] as const;
export type Locale = (typeof LOCALES)[number];

/// Nombre de cada idioma EN SU PROPIO idioma (convencion de selectores de idioma: quien
/// no entiende el idioma actual igual reconoce el suyo en la lista).
export const LOCALE_LABEL: Record<Locale, string> = { es: "Español", en: "English" };

/// Entrada con formas singular/plural. El catalogo la usa solo donde hace falta.
interface Plural {
  one: string;
  other: string;
}

/// Contrato del catalogo. Agregar una clave aca rompe la compilacion hasta que TODOS los
/// idiomas la definan: es el mecanismo que evita traducciones a medias.
export interface Messages {
  // Barra de titulo
  "doc.untitled": string;
  "menu.open": string;
  "menu.shortcuts": string;
  "menu.about": string;
  "menu.language": string;
  "menu.recents": string;
  "menu.table": string;
  "menu.insertImage": string;
  /// Nombre del filtro del selector nativo de archivos. Viaja por IPC: el backend no
  /// guarda copia, para que sumar un idioma no obligue a tocar Rust.
  "dialog.imageFilter": string;
  "status.tableAligned": string;
  "status.notATable": string;
  "theme.open": string;
  "theme.light": string;
  "theme.dark": string;
  "theme.sepia": string;
  "view.group": string;
  "view.editor": string;
  "view.split": string;
  "view.preview": string;
  "view.previewFrame": string;
  "view.detach": string;
  "inline.link": string;
  "inline.image": string;
  /// Motivos por los que una imagen no se puede mostrar en el popover.
  "inline.imageRemote": string;
  "inline.imageNoBaseDir": string;
  "inline.imageNotFound": string;
  "inline.imageTooLarge": string;
  "inline.imageUnsupported": string;
  "inline.imageUnreadable": string;
  "inline.footnote": string;
  "inline.footnoteMissing": string;
  "inline.math": string;
  "export.action": string;
  "export.hint": string;
  "window.minimize": string;
  "window.maximize": string;
  "window.close": string;

  // Barra de pestañas
  "tabs.listLabel": string;
  "tabs.new": string;
  "tabs.newHint": string;
  /// Nombre accesible del boton × de una pestaña ({name} = nombre del documento).
  "tabs.close": string;
  "tabs.closeHint": string;
  /// Nombre accesible de una pestaña con cambios: el ● es solo visual.
  "tabs.unsavedLabel": string;
  /// Tooltip de una pestaña recuperada de un snapshot ({path} = archivo original).
  "tabs.restoredFrom": string;

  // Barra de cambio externo
  "external.message": string;
  "external.reload": string;
  "external.keep": string;

  // Barra de estado (contadores)
  "stats.words": Plural;
  "stats.chars": Plural;
  "stats.sentences": Plural;
  "stats.paragraphs": Plural;
  "stats.readtime": string;

  // Mensajes transitorios
  "status.saved": string;
  "status.opened": string;
  "status.newDoc": string;
  "status.exporting": string;
  "status.exported": string;
  "status.reloaded": string;
  "status.changedOnDisk": string;
  "status.restored": Plural;
  "status.alreadyOpen": string;
  "status.pathOpenInOtherTab": string;
  "status.hemingway": string;
  "status.focusOn": string;
  "status.focusOff": string;
  "status.spellcheckOn": string;
  "status.spellcheckOff": string;
  "status.hemingwayOn": string;
  "status.hemingwayOff": string;
  "status.fullscreenFailed": string;
  "status.closeFailed": string;
  "status.dirtyCheckFailed": string;
  "status.previewFailed": string;
  "error.open": string;
  "error.save": string;
  "error.export": string;
  "error.generic": string;

  // Dialogos nativos (se envian al backend por IPC)
  "confirm.openTitle": string;
  "confirm.openMessage": string;
  "confirm.openConfirm": string;
  "confirm.closeTitle": string;
  "confirm.closeMessage": string;
  /// Cierre de la ventana con varias pestañas: cuantos documentos pierden cambios.
  "confirm.closeWindowMessage": Plural;
  /// Cierre de UNA pestaña con cambios ({name} = nombre del documento).
  "confirm.closeTabMessage": string;
  "confirm.closeConfirm": string;
  "confirm.newTitle": string;
  "confirm.newConfirm": string;
  "confirm.cancel": string;

  // Ventana de atajos
  "shortcuts.title": string;
  "shortcuts.file": string;
  "shortcuts.editing": string;
  "shortcuts.view": string;
  "shortcuts.tabs": string;
  "shortcuts.writingModes": string;
  "shortcuts.spellcheck": string;
  "shortcuts.help": string;
  "shortcuts.open": string;
  "shortcuts.save": string;
  "shortcuts.saveAs": string;
  "shortcuts.newDoc": string;
  "shortcuts.export": string;
  "shortcuts.newTab": string;
  "shortcuts.closeTab": string;
  "shortcuts.nextTab": string;
  "shortcuts.prevTab": string;
  "shortcuts.bold": string;
  "shortcuts.italic": string;
  "shortcuts.strikethrough": string;
  "shortcuts.list": string;
  "shortcuts.alignTable": string;
  "shortcuts.heading": string;
  "shortcuts.find": string;
  "shortcuts.findReplace": string;
  "shortcuts.undo": string;
  "shortcuts.redo": string;
  "shortcuts.cycleView": string;
  "shortcuts.fullscreen": string;
  "shortcuts.focus": string;
  "shortcuts.hemingway": string;
  "shortcuts.toggleSpellcheck": string;
  "shortcuts.showShortcuts": string;

  // Acerca de
  "about.title": string;
  "about.name": string;
  "about.version": string;
  "about.description": string;
  "about.repo": string;
  "about.upstream": string;
  "about.credits": string;
  "common.close": string;
}

const ES: Messages = {
  "doc.untitled": "Sin título",
  "menu.open": "Menú",
  "menu.shortcuts": "Atajos de teclado",
  "menu.about": "Acerca de",
  "menu.language": "Idioma",
  "menu.recents": "Recientes",
  "menu.table": "Insertar tabla",
  "menu.insertImage": "Insertar imagen",
  "dialog.imageFilter": "Imágenes",
  "theme.open": "Tema",
  "theme.light": "Claro",
  "theme.dark": "Oscuro",
  "theme.sepia": "Sepia",
  "view.group": "Vista",
  "view.editor": "Solo editor",
  "view.split": "Dividido (editor y vista)",
  "view.preview": "Solo vista — Ctrl+P cicla",
  "view.previewFrame": "Vista previa",
  "view.detach": "Vista previa en otra ventana",
  "inline.link": "Enlace",
  "inline.image": "Imagen",
  "inline.imageRemote": "Las imágenes remotas no se descargan.",
  "inline.imageNoBaseDir": "Guarda el documento para poder usar rutas relativas.",
  "inline.imageNotFound": "No se encontró el archivo.",
  "inline.imageTooLarge": "La imagen supera los 5 MB.",
  "inline.imageUnsupported": "El archivo no es una imagen PNG, JPEG, GIF, WebP ni SVG.",
  "inline.imageUnreadable": "No se pudo leer el archivo.",
  "inline.footnote": "Nota al pie",
  "inline.footnoteMissing": "sin definir",
  "inline.math": "Fórmula",
  "export.action": "Exportar",
  "export.hint": "Exportar a PDF, HTML, ODT, DOCX, EPUB o LaTeX (Ctrl+E)",
  "window.minimize": "Minimizar",
  "window.maximize": "Maximizar",
  "window.close": "Cerrar",

  "tabs.listLabel": "Documentos abiertos",
  "tabs.new": "Nueva pestaña",
  "tabs.newHint": "Nueva pestaña (Ctrl+T)",
  "tabs.close": "Cerrar {name}",
  "tabs.closeHint": "Cerrar pestaña (Ctrl+W)",
  "tabs.unsavedLabel": "{name}, con cambios sin guardar",
  "tabs.restoredFrom": "Recuperado de {path}",

  "external.message": "El archivo cambió en disco.",
  "external.reload": "Recargar y perder mis cambios",
  "external.keep": "Mantener mis cambios",

  "stats.words": { one: "{n} palabra", other: "{n} palabras" },
  "stats.chars": { one: "{n} carácter", other: "{n} caracteres" },
  "stats.sentences": { one: "{n} oración", other: "{n} oraciones" },
  "stats.paragraphs": { one: "{n} párrafo", other: "{n} párrafos" },
  "stats.readtime": "{time} de lectura",

  "status.saved": "guardado",
  "status.opened": "abierto",
  "status.newDoc": "documento nuevo",
  "status.exporting": "exportando…",
  "status.exported": "exportado",
  "status.reloaded": "Archivo recargado desde disco",
  "status.changedOnDisk": "El archivo cambió en disco — recargado",
  "status.restored": {
    one: "Se restauró trabajo no guardado de la sesión anterior",
    other: "Se restauraron {n} documentos no guardados de la sesión anterior",
  },
  "status.alreadyOpen": "ese archivo ya estaba abierto",
  "status.pathOpenInOtherTab":
    "ese archivo ya está abierto en otra pestaña — ciérrala o elige otro nombre",
  "status.hemingway": "El texto no se puede borrar en modo Hemingway",
  "status.focusOn": "Modo enfoque activado",
  "status.focusOff": "Modo enfoque desactivado",
  "status.spellcheckOn": "Corrector activado",
  "status.spellcheckOff": "Corrector desactivado",
  "status.hemingwayOn": "Modo Hemingway activado — no puedes borrar texto",
  "status.hemingwayOff": "Modo Hemingway desactivado",
  "status.fullscreenFailed": "no se pudo cambiar a pantalla completa",
  "status.closeFailed": "no se pudo cerrar la ventana — reintenta",
  "status.dirtyCheckFailed":
    "no se pudo verificar los cambios — guarda con Ctrl+S antes de cerrar",
  "status.previewFailed": "error de preview",
  "status.tableAligned": "tabla alineada",
  "status.notATable": "el cursor no está sobre una tabla",
  "error.open": "error al abrir",
  "error.save": "error al guardar",
  "error.export": "error al exportar",
  "error.generic": "error",

  "confirm.openTitle": "¿Abrir otro archivo?",
  "confirm.openMessage": "El documento actual tiene cambios que se perderán.",
  "confirm.openConfirm": "Descartar y abrir",
  "confirm.closeTitle": "¿Cerrar sin guardar?",
  "confirm.closeMessage": "El documento tiene cambios que se perderán.",
  "confirm.closeWindowMessage": {
    one: "Un documento tiene cambios que se perderán.",
    other: "{n} documentos tienen cambios que se perderán.",
  },
  "confirm.closeTabMessage": "«{name}» tiene cambios que se perderán.",
  "confirm.closeConfirm": "Descartar y cerrar",
  "confirm.newTitle": "¿Crear un documento nuevo?",
  "confirm.newConfirm": "Descartar y crear",
  "confirm.cancel": "Cancelar",

  "shortcuts.title": "Atajos de teclado",
  "shortcuts.file": "Archivo",
  "shortcuts.editing": "Edición y formato",
  "shortcuts.view": "Vista",
  "shortcuts.tabs": "Pestañas",
  "shortcuts.writingModes": "Modos de escritura",
  "shortcuts.spellcheck": "Corrector",
  "shortcuts.help": "Ayuda",
  "shortcuts.open": "Abrir",
  "shortcuts.save": "Guardar",
  "shortcuts.saveAs": "Guardar como",
  "shortcuts.newDoc": "Nuevo documento",
  "shortcuts.export": "Exportar",
  "shortcuts.newTab": "Nueva pestaña",
  "shortcuts.closeTab": "Cerrar pestaña",
  "shortcuts.nextTab": "Pestaña siguiente",
  "shortcuts.prevTab": "Pestaña anterior",
  "shortcuts.bold": "Negrita",
  "shortcuts.italic": "Cursiva",
  "shortcuts.strikethrough": "Tachado",
  "shortcuts.list": "Lista",
  "shortcuts.alignTable": "Alinear la tabla bajo el cursor",
  "shortcuts.heading": "Encabezado (niveles 1 a 4)",
  "shortcuts.find": "Buscar",
  "shortcuts.findReplace": "Buscar y reemplazar",
  "shortcuts.undo": "Deshacer",
  "shortcuts.redo": "Rehacer",
  "shortcuts.cycleView": "Alternar vista (editor / dividido / solo vista)",
  "shortcuts.fullscreen": "Pantalla completa",
  "shortcuts.focus": "Enfoque",
  "shortcuts.hemingway": "Hemingway",
  "shortcuts.toggleSpellcheck": "Activar o desactivar el corrector",
  "shortcuts.showShortcuts": "Atajos de teclado",

  "about.title": "Acerca de",
  "about.name": "Apostrophe para Windows",
  "about.version": "Versión",
  "about.description": "Editor de Markdown nativo para Windows. Port de Apostrophe (GNOME).",
  "about.repo": "Repositorio",
  "about.upstream": "Proyecto original (GNOME)",
  "about.credits":
    "Distribuido bajo licencia GPL-3.0. Apostrophe es una marca de sus autores originales. " +
    "Las gracias al equipo de Apostrophe (GNOME) por el editor que inspira este port.",
  "common.close": "Cerrar",
};

const EN: Messages = {
  "doc.untitled": "Untitled",
  "menu.open": "Menu",
  "menu.shortcuts": "Keyboard shortcuts",
  "menu.about": "About",
  "menu.language": "Language",
  "menu.recents": "Recent",
  "menu.table": "Insert table",
  "menu.insertImage": "Insert image",
  "dialog.imageFilter": "Images",
  "theme.open": "Theme",
  "theme.light": "Light",
  "theme.dark": "Dark",
  "theme.sepia": "Sepia",
  "view.group": "View",
  "view.editor": "Editor only",
  "view.split": "Split (editor and preview)",
  "view.preview": "Preview only — Ctrl+P cycles",
  "view.previewFrame": "Preview",
  "view.detach": "Preview in a separate window",
  "inline.link": "Link",
  "inline.image": "Image",
  "inline.imageRemote": "Remote images are not downloaded.",
  "inline.imageNoBaseDir": "Save the document to use relative paths.",
  "inline.imageNotFound": "File not found.",
  "inline.imageTooLarge": "The image is larger than 5 MB.",
  "inline.imageUnsupported": "The file is not a PNG, JPEG, GIF, WebP or SVG image.",
  "inline.imageUnreadable": "The file could not be read.",
  "inline.footnote": "Footnote",
  "inline.footnoteMissing": "not defined",
  "inline.math": "Formula",
  "export.action": "Export",
  "export.hint": "Export to PDF, HTML, ODT, DOCX, EPUB or LaTeX (Ctrl+E)",
  "window.minimize": "Minimize",
  "window.maximize": "Maximize",
  "window.close": "Close",

  "tabs.listLabel": "Open documents",
  "tabs.new": "New tab",
  "tabs.newHint": "New tab (Ctrl+T)",
  "tabs.close": "Close {name}",
  "tabs.closeHint": "Close tab (Ctrl+W)",
  "tabs.unsavedLabel": "{name}, unsaved changes",
  "tabs.restoredFrom": "Recovered from {path}",

  "external.message": "The file changed on disk.",
  "external.reload": "Reload and discard my changes",
  "external.keep": "Keep my changes",

  "stats.words": { one: "{n} word", other: "{n} words" },
  "stats.chars": { one: "{n} character", other: "{n} characters" },
  "stats.sentences": { one: "{n} sentence", other: "{n} sentences" },
  "stats.paragraphs": { one: "{n} paragraph", other: "{n} paragraphs" },
  "stats.readtime": "{time} read",

  "status.saved": "saved",
  "status.opened": "opened",
  "status.newDoc": "new document",
  "status.exporting": "exporting…",
  "status.exported": "exported",
  "status.reloaded": "File reloaded from disk",
  "status.changedOnDisk": "The file changed on disk — reloaded",
  "status.restored": {
    one: "Unsaved work from the previous session was restored",
    other: "{n} unsaved documents from the previous session were restored",
  },
  "status.alreadyOpen": "that file was already open",
  "status.pathOpenInOtherTab":
    "that file is already open in another tab — close it or choose another name",
  "status.hemingway": "Text cannot be deleted in Hemingway mode",
  "status.focusOn": "Focus mode on",
  "status.focusOff": "Focus mode off",
  "status.spellcheckOn": "Spell checker on",
  "status.spellcheckOff": "Spell checker off",
  "status.hemingwayOn": "Hemingway mode on — you can't delete text",
  "status.hemingwayOff": "Hemingway mode off",
  "status.fullscreenFailed": "could not switch to full screen",
  "status.closeFailed": "could not close the window — try again",
  "status.dirtyCheckFailed":
    "could not check for changes — save with Ctrl+S before closing",
  "status.previewFailed": "preview error",
  "status.tableAligned": "table aligned",
  "status.notATable": "the cursor is not on a table",
  "error.open": "error opening the file",
  "error.save": "error saving",
  "error.export": "error exporting",
  "error.generic": "error",

  "confirm.openTitle": "Open another file?",
  "confirm.openMessage": "The current document has changes that will be lost.",
  "confirm.openConfirm": "Discard and open",
  "confirm.closeTitle": "Close without saving?",
  "confirm.closeMessage": "The document has changes that will be lost.",
  "confirm.closeWindowMessage": {
    one: "One document has changes that will be lost.",
    other: "{n} documents have changes that will be lost.",
  },
  "confirm.closeTabMessage": "“{name}” has changes that will be lost.",
  "confirm.closeConfirm": "Discard and close",
  "confirm.newTitle": "Create a new document?",
  "confirm.newConfirm": "Discard and create",
  "confirm.cancel": "Cancel",

  "shortcuts.title": "Keyboard shortcuts",
  "shortcuts.file": "File",
  "shortcuts.editing": "Editing and formatting",
  "shortcuts.view": "View",
  "shortcuts.tabs": "Tabs",
  "shortcuts.writingModes": "Writing modes",
  "shortcuts.spellcheck": "Spell checker",
  "shortcuts.help": "Help",
  "shortcuts.open": "Open",
  "shortcuts.save": "Save",
  "shortcuts.saveAs": "Save as",
  "shortcuts.newDoc": "New document",
  "shortcuts.export": "Export",
  "shortcuts.newTab": "New tab",
  "shortcuts.closeTab": "Close tab",
  "shortcuts.nextTab": "Next tab",
  "shortcuts.prevTab": "Previous tab",
  "shortcuts.bold": "Bold",
  "shortcuts.italic": "Italic",
  "shortcuts.strikethrough": "Strikethrough",
  "shortcuts.list": "List",
  "shortcuts.alignTable": "Align the table under the cursor",
  "shortcuts.heading": "Heading (levels 1 to 4)",
  "shortcuts.find": "Find",
  "shortcuts.findReplace": "Find and replace",
  "shortcuts.undo": "Undo",
  "shortcuts.redo": "Redo",
  "shortcuts.cycleView": "Cycle view (editor / split / preview only)",
  "shortcuts.fullscreen": "Full screen",
  "shortcuts.focus": "Focus",
  "shortcuts.hemingway": "Hemingway",
  "shortcuts.toggleSpellcheck": "Turn the spell checker on or off",
  "shortcuts.showShortcuts": "Keyboard shortcuts",

  "about.title": "About",
  "about.name": "Apostrophe for Windows",
  "about.version": "Version",
  "about.description": "Native Markdown editor for Windows. A port of Apostrophe (GNOME).",
  "about.repo": "Repository",
  "about.upstream": "Original project (GNOME)",
  "about.credits":
    "Distributed under the GPL-3.0 license. Apostrophe is a trademark of its original authors. " +
    "Thanks to the Apostrophe (GNOME) team for the editor that inspires this port.",
  "common.close": "Close",
};

const CATALOG: Record<Locale, Messages> = { es: ES, en: EN };

/// Claves cuyo valor es una cadena simple (excluye las de plural, que van por `tp`).
type TextKey = {
  [K in keyof Messages]: Messages[K] extends string ? K : never;
}[keyof Messages];

/// Claves con formas singular/plural.
type PluralKey = {
  [K in keyof Messages]: Messages[K] extends Plural ? K : never;
}[keyof Messages];

let current: Locale = "en";

/// Idioma que corresponde al sistema. Cualquier variante de español (es-PE, es-ES, ...)
/// cae en `es`; el resto en ingles, que es el default mas seguro para un publico global.
export function detectLocale(): Locale {
  return navigator.language.toLowerCase().startsWith("es") ? "es" : "en";
}

export function getLocale(): Locale {
  return current;
}

/// Fija el idioma activo. No re-renderiza por si mismo: quien llama decide cuando aplicar
/// (ver `applyTranslations` en language.ts), para hacer un unico repintado.
export function setLocale(locale: Locale): void {
  current = locale;
  document.documentElement.lang = locale;
}

/// Traduce una clave simple. `params` reemplaza marcadores `{nombre}`.
export function t(key: TextKey, params?: Record<string, string | number>): string {
  const raw = CATALOG[current][key] as string;
  return params ? interpolate(raw, params) : raw;
}

/// Traduce una clave con plural segun `n`. Ambos idiomas usan la regla simple
/// "1 = singular, resto = plural", que es correcta para es y en; si algun dia se suma un
/// idioma con reglas distintas (ruso, polaco), este es el unico punto a cambiar.
export function tp(key: PluralKey, n: number): string {
  const forms = CATALOG[current][key] as Plural;
  return interpolate(n === 1 ? forms.one : forms.other, { n });
}

function interpolate(raw: string, params: Record<string, string | number>): string {
  return raw.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}
