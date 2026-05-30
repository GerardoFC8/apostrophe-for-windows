//! DocumentStore: modelo de ESTADO del documento en edicion (ADR-0001, muro de
//! carga del nucleo). PURO: modela `path` / `content` / `dirty` y sus transiciones.
//! El I/O de disco NO vive aca — el dominio modela el ESTADO; el adaptador
//! (src-tauri) hace el read/write con `std::fs`. Replica la idea de documento como
//! entidad de dominio (settings + autosave) del upstream.

use std::path::PathBuf;

/// Estado del documento abierto en el editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentStore {
    /// Ruta en disco. `None` = documento nuevo, todavia sin guardar.
    path: Option<PathBuf>,
    /// Contenido actual (lo ultimo sincronizado desde el editor o leido de disco).
    content: String,
    /// Hay cambios sin guardar respecto del ultimo open/save.
    dirty: bool,
}

impl DocumentStore {
    /// Documento nuevo vacio (sin path, sin cambios).
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga un documento desde disco: fija `path` + `content` y queda LIMPIO.
    pub fn open(&mut self, path: PathBuf, content: String) {
        self.path = Some(path);
        self.content = content;
        self.dirty = false;
    }

    /// Sincroniza el contenido desde el editor. Marca `dirty` SOLO si cambio
    /// respecto del contenido actual (escribir lo mismo no ensucia).
    pub fn set_content(&mut self, content: String) {
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

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
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
        assert_eq!(doc.path(), Some(&PathBuf::from("/tmp/a.md")), "el path se mantiene");
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
}
