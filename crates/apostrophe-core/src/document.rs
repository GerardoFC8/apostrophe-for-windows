//! DocumentStore: modelo de ESTADO del documento en edicion (ADR-0001, muro de
//! carga del nucleo). PURO: modela `path` / `content` / `dirty` y sus transiciones.
//! El I/O de disco NO vive aca — el dominio modela el ESTADO; el adaptador
//! (src-tauri) hace el read/write con `std::fs`. Replica la idea de documento como
//! entidad de dominio (settings + autosave) del upstream.

use std::path::PathBuf;

/// Salto de linea con el que el documento se leyo y con el que se vuelve a escribir.
///
/// El editor (CodeMirror) trabaja siempre con `\n`: al cargar un texto convierte `\r\n` y
/// `\r` sueltos, y al devolverlo usa `\n`. Si el documento guardara el texto tal como vino
/// del disco, un archivo con saltos de Windows quedaria "modificado" en cuanto el editor lo
/// sincroniza, sin que nadie lo toque. Por eso el contenido se guarda normalizado y el salto
/// original se recuerda aparte, para restaurarlo al escribir.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineEnding {
    /// `\n`: el de los documentos nuevos y de los que ya lo usaban.
    #[default]
    Lf,
    /// `\r\n`: el habitual de los archivos creados en Windows.
    CrLf,
}

impl LineEnding {
    /// El salto que predomina en `text`. Sin saltos, o con empate, LF.
    pub fn detect(text: &str) -> Self {
        let crlf = text.matches("\r\n").count();
        let lf_only = text.matches('\n').count() - crlf;
        if crlf > lf_only {
            Self::CrLf
        } else {
            Self::Lf
        }
    }

    /// `text` listo para escribir a disco con este salto de linea.
    pub fn apply(self, text: &str) -> String {
        let text = normalize_line_endings(text);
        match self {
            Self::Lf => text,
            Self::CrLf => text.replace('\n', "\r\n"),
        }
    }
}

/// `text` con los saltos como los deja el editor: cada `\r\n` y cada `\r` suelto pasan a `\n`.
pub fn normalize_line_endings(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

/// Estado del documento abierto en el editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentStore {
    /// Ruta en disco. `None` = documento nuevo, todavia sin guardar.
    path: Option<PathBuf>,
    /// Contenido actual (lo ultimo sincronizado desde el editor o leido de disco), siempre
    /// con saltos `\n` (ver `LineEnding`).
    content: String,
    /// Salto de linea del archivo en disco, para escribirlo igual que como estaba.
    line_ending: LineEnding,
    /// Hay cambios sin guardar respecto del ultimo open/save.
    dirty: bool,
}

impl DocumentStore {
    /// Documento nuevo vacio (sin path, sin cambios).
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga un documento desde disco: fija `path` + `content` y queda LIMPIO. Recuerda el
    /// salto de linea del archivo y guarda el contenido normalizado a `\n`.
    pub fn open(&mut self, path: PathBuf, content: String) {
        self.path = Some(path);
        self.line_ending = LineEnding::detect(&content);
        self.content = normalize_line_endings(&content);
        self.dirty = false;
    }

    /// Sincroniza el contenido desde el editor. Marca `dirty` SOLO si cambio
    /// respecto del contenido actual (escribir lo mismo no ensucia). Un cambio que solo
    /// toca los saltos de linea no cuenta: el editor los normaliza por su cuenta.
    pub fn set_content(&mut self, content: String) {
        let content = normalize_line_endings(&content);
        if content != self.content {
            self.content = content;
            self.dirty = true;
        }
    }

    /// Marca el documento como guardado (limpio) manteniendo el `path` actual.
    /// Caso "Guardar" sobre un documento que ya tiene ruta.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Marca el documento como guardado en `path` (lo fija) y limpio. Caso
    /// "Guardar como" o primer guardado de un documento nuevo.
    pub fn saved_as(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.dirty = false;
    }

    /// Vuelve al estado de documento nuevo: sin path, vacío, limpio. Caso "Nuevo" (Ctrl+N).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Salto de linea con el que hay que escribir el documento (ver `LineEnding::apply`).
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Nombre de archivo (sin directorio) para mostrar en la titlebar, si hay path.
    pub fn file_name(&self) -> Option<String> {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty_and_clean() {
        let doc = DocumentStore::new();
        assert_eq!(doc.path(), None);
        assert_eq!(doc.content(), "");
        assert!(!doc.is_dirty());
        assert_eq!(doc.file_name(), None);
    }

    #[test]
    fn open_sets_path_content_and_is_clean() {
        let mut doc = DocumentStore::new();
        doc.open(PathBuf::from("/tmp/notas.md"), "# Hola".into());
        assert_eq!(doc.path(), Some(&PathBuf::from("/tmp/notas.md")));
        assert_eq!(doc.content(), "# Hola");
        assert!(!doc.is_dirty(), "abrir deja el doc limpio");
        assert_eq!(doc.file_name().as_deref(), Some("notas.md"));
    }

    #[test]
    fn set_content_marks_dirty_only_on_change() {
        let mut doc = DocumentStore::new();
        doc.open(PathBuf::from("/tmp/a.md"), "abc".into());
        doc.set_content("abc".into());
        assert!(!doc.is_dirty(), "contenido igual NO ensucia");
        doc.set_content("abcd".into());
        assert!(doc.is_dirty(), "contenido distinto ensucia");
    }

    #[test]
    fn set_content_on_new_doc_marks_dirty() {
        let mut doc = DocumentStore::new();
        doc.set_content("primera escritura".into());
        assert!(doc.is_dirty());
        assert_eq!(doc.content(), "primera escritura");
    }

    #[test]
    fn mark_clean_clears_dirty_keeping_path() {
        let mut doc = DocumentStore::new();
        doc.open(PathBuf::from("/tmp/a.md"), "abc".into());
        doc.set_content("xyz".into());
        assert!(doc.is_dirty());
        doc.mark_clean();
        assert!(!doc.is_dirty(), "guardar limpia el dirty");
        assert_eq!(
            doc.path(),
            Some(&PathBuf::from("/tmp/a.md")),
            "el path se mantiene"
        );
    }

    #[test]
    fn saved_as_sets_path_and_clears_dirty() {
        let mut doc = DocumentStore::new(); // doc nuevo, sin path
        doc.set_content("hola".into());
        assert!(doc.is_dirty());
        assert_eq!(doc.path(), None);
        doc.saved_as(PathBuf::from("/tmp/nuevo.md"));
        assert_eq!(doc.path(), Some(&PathBuf::from("/tmp/nuevo.md")));
        assert!(!doc.is_dirty(), "guardar como limpia el dirty");
        assert_eq!(doc.file_name().as_deref(), Some("nuevo.md"));
    }

    #[test]
    fn abrir_un_archivo_crlf_y_sincronizar_el_editor_no_lo_ensucia() {
        let mut doc = DocumentStore::new();
        doc.open(PathBuf::from("/tmp/w.md"), "# Hola\r\n\r\ntexto\r\n".into());
        assert_eq!(
            doc.content(),
            "# Hola\n\ntexto\n",
            "el contenido queda con \\n"
        );
        assert_eq!(doc.line_ending(), LineEnding::CrLf);
        // Lo que devuelve CodeMirror tras cargarlo: los mismos renglones con \n.
        doc.set_content("# Hola\n\ntexto\n".into());
        assert!(!doc.is_dirty(), "el editor solo normalizo los saltos");
        doc.set_content("# Hola\n\ntexto editado\n".into());
        assert!(doc.is_dirty(), "un cambio real si ensucia");
    }

    #[test]
    fn un_archivo_crlf_se_vuelve_a_escribir_con_crlf() {
        let mut doc = DocumentStore::new();
        doc.open(PathBuf::from("/tmp/w.md"), "a\r\nb\r\n".into());
        assert_eq!(doc.line_ending().apply("a\nb\nc\n"), "a\r\nb\r\nc\r\n");
    }

    #[test]
    fn un_archivo_lf_y_un_documento_nuevo_se_escriben_con_lf() {
        let mut doc = DocumentStore::new();
        assert_eq!(doc.line_ending(), LineEnding::Lf);
        doc.open(PathBuf::from("/tmp/u.md"), "a\nb\n".into());
        assert_eq!(doc.line_ending(), LineEnding::Lf);
        assert_eq!(doc.line_ending().apply("a\nb\n"), "a\nb\n");
    }

    #[test]
    fn detect_elige_el_salto_que_predomina() {
        assert_eq!(LineEnding::detect("sin saltos"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\r\nb\r\nc\n"), LineEnding::CrLf);
        assert_eq!(LineEnding::detect("a\nb\nc\r\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\r\nb\n"), LineEnding::Lf, "empate: LF");
    }

    #[test]
    fn normalizar_convierte_crlf_y_cr_sueltos() {
        assert_eq!(normalize_line_endings("a\r\nb\rc\nd"), "a\nb\nc\nd");
        // apply no duplica el \r de un texto que ya traia \r\n.
        assert_eq!(LineEnding::CrLf.apply("a\r\nb"), "a\r\nb");
    }

    #[test]
    fn reset_vuelve_a_documento_nuevo() {
        let mut doc = DocumentStore::new();
        doc.open(PathBuf::from("/tmp/a.md"), "contenido".into());
        doc.set_content("editado".into());
        assert!(doc.is_dirty());
        doc.reset();
        assert_eq!(doc.path(), None);
        assert_eq!(doc.line_ending(), LineEnding::Lf);
        assert_eq!(doc.content(), "");
        assert!(!doc.is_dirty());
        assert_eq!(doc.file_name(), None);
    }
}
