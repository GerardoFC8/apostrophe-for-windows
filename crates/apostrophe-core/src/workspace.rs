//! Workspace: el conjunto de documentos abiertos a la vez (pestañas). PURO: modela
//! qué documentos hay, en qué orden, cuál es el activo y cómo se identifican. Cada
//! documento sigue siendo un `DocumentStore`; el I/O de disco lo hace el adaptador.
//!
//! Identidad: cada documento recibe un `DocId` estable que NUNCA se reutiliza durante
//! la sesión (cerrar el 2 y abrir otro da el 3, no el 2). Así una respuesta tardía del
//! frontend que apunte a un documento ya cerrado no puede caer en otro por error.
//!
//! El `active` existe SOLO por compatibilidad: los comandos que no dicen a qué documento
//! apuntan operan sobre el activo, que es el último creado, abierto o activado.

use std::path::{Path, PathBuf};

use crate::document::DocumentStore;

/// Identificador estable de un documento abierto. No se reutiliza en la sesión.
pub type DocId = u64;

/// ¿Dos rutas señalan el mismo documento? Compara componente a componente ignorando
/// mayúsculas ASCII (Windows es case-insensitive) y es robusta a separadores (`/` vs
/// `\`). No toca el disco: el adaptador debe pasar rutas ya absolutas.
pub fn same_document_path(a: &Path, b: &Path) -> bool {
    let a: Vec<_> = a.components().collect();
    let b: Vec<_> = b.components().collect();
    a.len() == b.len()
        && a.iter()
            .zip(&b)
            .all(|(x, y)| x.as_os_str().eq_ignore_ascii_case(y.as_os_str()))
}

/// Resultado de abrir una ruta: el documento que la contiene y si ya estaba abierta
/// (en ese caso NO se tocó su contenido, para no pisar cambios sin guardar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opened {
    pub id: DocId,
    pub already_open: bool,
}

#[derive(Debug, Clone)]
struct Entry {
    id: DocId,
    store: DocumentStore,
}

/// Documentos abiertos, en el orden en que se abrieron.
#[derive(Debug, Clone)]
pub struct Workspace {
    docs: Vec<Entry>,
    next_id: DocId,
    active: Option<DocId>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// Workspace vacío. Los ids empiezan en 1.
    pub fn new() -> Self {
        Self {
            docs: Vec::new(),
            next_id: 1,
            active: None,
        }
    }

    fn push(&mut self, store: DocumentStore) -> DocId {
        let id = self.next_id;
        self.next_id += 1;
        self.docs.push(Entry { id, store });
        self.active = Some(id);
        id
    }

    /// Crea un documento nuevo sin título y lo deja activo.
    pub fn create_untitled(&mut self) -> DocId {
        self.push(DocumentStore::new())
    }

    /// Abre `path` como documento NUEVO. Si esa ruta ya está abierta, devuelve el
    /// documento existente sin tocar su contenido. En ambos casos queda activo.
    pub fn open_path(&mut self, path: PathBuf, content: String) -> Opened {
        if let Some(id) = self.find_by_path(&path) {
            self.active = Some(id);
            return Opened {
                id,
                already_open: true,
            };
        }
        let mut store = DocumentStore::new();
        store.open(path, content);
        Opened {
            id: self.push(store),
            already_open: false,
        }
    }

    /// Carga `path` DENTRO del documento `target`, reemplazando lo que tenía (el
    /// flujo de un solo documento: abrir sustituye al actual). Si la ruta ya está
    /// abierta en OTRO documento, activa ese y deja `target` intacto. Si es la ruta
    /// del propio `target`, la recarga. `None` si `target` no existe.
    pub fn open_path_into(
        &mut self,
        target: DocId,
        path: PathBuf,
        content: String,
    ) -> Option<Opened> {
        if let Some(other) = self.find_by_path(&path).filter(|&id| id != target) {
            self.active = Some(other);
            return Some(Opened {
                id: other,
                already_open: true,
            });
        }
        self.get_mut(target)?.open(path, content);
        self.active = Some(target);
        Some(Opened {
            id: target,
            already_open: false,
        })
    }

    pub fn get(&self, id: DocId) -> Option<&DocumentStore> {
        self.docs.iter().find(|e| e.id == id).map(|e| &e.store)
    }

    pub fn get_mut(&mut self, id: DocId) -> Option<&mut DocumentStore> {
        self.docs
            .iter_mut()
            .find(|e| e.id == id)
            .map(|e| &mut e.store)
    }

    pub fn contains(&self, id: DocId) -> bool {
        self.get(id).is_some()
    }

    /// Cierra un documento y lo devuelve. Si era el activo, el activo pasa al vecino
    /// siguiente (o al anterior si era el último), como al cerrar una pestaña.
    pub fn close(&mut self, id: DocId) -> Option<DocumentStore> {
        let index = self.docs.iter().position(|e| e.id == id)?;
        let entry = self.docs.remove(index);
        if self.active == Some(id) {
            self.active = self
                .docs
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|i| self.docs.get(i)))
                .map(|e| e.id);
        }
        Some(entry.store)
    }

    /// Ids en orden de apertura.
    pub fn ids(&self) -> Vec<DocId> {
        self.docs.iter().map(|e| e.id).collect()
    }

    /// Documentos en orden de apertura.
    pub fn iter(&self) -> impl Iterator<Item = (DocId, &DocumentStore)> {
        self.docs.iter().map(|e| (e.id, &e.store))
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Documento abierto en `path`, si lo hay (ver `same_document_path`).
    pub fn find_by_path(&self, path: &Path) -> Option<DocId> {
        self.docs
            .iter()
            .find(|e| e.store.path().is_some_and(|p| same_document_path(p, path)))
            .map(|e| e.id)
    }

    /// Ids de los documentos con cambios sin guardar, en orden de apertura.
    pub fn dirty_ids(&self) -> Vec<DocId> {
        self.docs
            .iter()
            .filter(|e| e.store.is_dirty())
            .map(|e| e.id)
            .collect()
    }

    pub fn active(&self) -> Option<DocId> {
        self.active
    }

    /// Activa un documento existente. `false` si no existe (el activo no cambia).
    pub fn activate(&mut self, id: DocId) -> bool {
        if self.contains(id) {
            self.active = Some(id);
            true
        } else {
            false
        }
    }

    /// Resuelve a qué documento apunta un comando. Con id explícito, ese (o `None` si
    /// ya no existe: NO se redirige a otro). Sin id, el activo; y si no queda ninguno,
    /// crea uno sin título, porque un frontend de un solo documento siempre espera
    /// tener uno.
    pub fn resolve_or_create(&mut self, id: Option<DocId>) -> Option<DocId> {
        match id {
            Some(id) => self.contains(id).then_some(id),
            None => Some(match self.active {
                Some(active) => active,
                None => self.create_untitled(),
            }),
        }
    }

    /// Como `resolve_or_create` pero sin crear: para lecturas que no deben mutar.
    pub fn resolve(&self, id: Option<DocId>) -> Option<DocId> {
        match id {
            Some(id) => self.contains(id).then_some(id),
            None => self.active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn nuevo_workspace_esta_vacio_y_sin_activo() {
        let ws = Workspace::new();
        assert!(ws.is_empty());
        assert_eq!(ws.len(), 0);
        assert_eq!(ws.active(), None);
        assert!(ws.ids().is_empty());
    }

    #[test]
    fn create_untitled_asigna_ids_crecientes_y_activa() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        let b = ws.create_untitled();
        assert_ne!(a, b);
        assert!(b > a);
        assert_eq!(ws.ids(), vec![a, b]);
        assert_eq!(ws.active(), Some(b));
        assert_eq!(ws.get(a).unwrap().path(), None);
        assert!(!ws.get(a).unwrap().is_dirty());
    }

    #[test]
    fn los_ids_no_se_reutilizan_tras_cerrar() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        let b = ws.create_untitled();
        ws.close(b);
        let c = ws.create_untitled();
        assert!(c > b, "c={c} no debe reutilizar b={b}");
        assert_eq!(ws.ids(), vec![a, c]);
    }

    #[test]
    fn open_path_crea_documento_limpio_y_lo_activa() {
        let mut ws = Workspace::new();
        let opened = ws.open_path(p("/docs/a.md"), "# A".into());
        assert!(!opened.already_open);
        let doc = ws.get(opened.id).unwrap();
        assert_eq!(doc.path(), Some(&p("/docs/a.md")));
        assert_eq!(doc.content(), "# A");
        assert!(!doc.is_dirty());
        assert_eq!(ws.active(), Some(opened.id));
    }

    #[test]
    fn open_path_repetido_devuelve_el_existente_sin_pisar_cambios() {
        let mut ws = Workspace::new();
        let a = ws.open_path(p("/docs/a.md"), "original".into()).id;
        ws.get_mut(a).unwrap().set_content("editado".into());
        let other = ws.create_untitled();
        assert_eq!(ws.active(), Some(other));

        let again = ws.open_path(p("/docs/a.md"), "de disco".into());
        assert_eq!(
            again,
            Opened {
                id: a,
                already_open: true
            }
        );
        assert_eq!(ws.len(), 2, "no duplica el documento");
        assert_eq!(ws.get(a).unwrap().content(), "editado", "no pisa cambios");
        assert!(ws.get(a).unwrap().is_dirty());
        assert_eq!(ws.active(), Some(a), "lo trae al frente");
    }

    #[test]
    fn open_path_reconoce_la_misma_ruta_con_otro_casing_y_separador() {
        let mut ws = Workspace::new();
        let a = ws.open_path(p("C:/Docs/Nota.md"), "x".into()).id;
        let again = ws.open_path(p("C:/docs/nota.MD"), "y".into());
        assert_eq!(again.id, a);
        assert!(again.already_open);
    }

    #[test]
    fn open_path_into_reemplaza_el_documento_destino() {
        let mut ws = Workspace::new();
        let t = ws.create_untitled();
        ws.get_mut(t).unwrap().set_content("borrador".into());
        let opened = ws.open_path_into(t, p("/docs/a.md"), "# A".into()).unwrap();
        assert_eq!(
            opened,
            Opened {
                id: t,
                already_open: false
            }
        );
        assert_eq!(ws.len(), 1, "un solo documento, como antes de las pestañas");
        let doc = ws.get(t).unwrap();
        assert_eq!(doc.path(), Some(&p("/docs/a.md")));
        assert_eq!(doc.content(), "# A");
        assert!(!doc.is_dirty());
    }

    #[test]
    fn open_path_into_con_su_propia_ruta_recarga() {
        let mut ws = Workspace::new();
        let a = ws.open_path(p("/docs/a.md"), "v1".into()).id;
        ws.get_mut(a).unwrap().set_content("local".into());
        let opened = ws.open_path_into(a, p("/docs/a.md"), "v2".into()).unwrap();
        assert_eq!(
            opened,
            Opened {
                id: a,
                already_open: false
            }
        );
        assert_eq!(ws.get(a).unwrap().content(), "v2");
        assert!(!ws.get(a).unwrap().is_dirty());
    }

    #[test]
    fn open_path_into_con_ruta_abierta_en_otro_activa_ese_y_no_toca_el_destino() {
        let mut ws = Workspace::new();
        let a = ws.open_path(p("/docs/a.md"), "A".into()).id;
        let t = ws.create_untitled();
        ws.get_mut(t).unwrap().set_content("borrador".into());
        let opened = ws.open_path_into(t, p("/docs/a.md"), "A".into()).unwrap();
        assert_eq!(
            opened,
            Opened {
                id: a,
                already_open: true
            }
        );
        assert_eq!(ws.get(t).unwrap().content(), "borrador");
        assert_eq!(ws.active(), Some(a));
    }

    #[test]
    fn open_path_into_destino_inexistente_devuelve_none() {
        let mut ws = Workspace::new();
        assert_eq!(ws.open_path_into(42, p("/docs/a.md"), "A".into()), None);
        assert!(ws.is_empty());
    }

    #[test]
    fn find_by_path_ignora_documentos_sin_ruta() {
        let mut ws = Workspace::new();
        ws.create_untitled();
        let a = ws.open_path(p("/docs/a.md"), "A".into()).id;
        assert_eq!(ws.find_by_path(&p("/docs/a.md")), Some(a));
        assert_eq!(ws.find_by_path(&p("/docs/b.md")), None);
    }

    #[test]
    fn dirty_ids_lista_solo_los_modificados_en_orden() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        let b = ws.create_untitled();
        let c = ws.create_untitled();
        ws.get_mut(c).unwrap().set_content("c".into());
        ws.get_mut(a).unwrap().set_content("a".into());
        assert_eq!(ws.dirty_ids(), vec![a, c]);
        ws.get_mut(a).unwrap().mark_clean();
        assert_eq!(ws.dirty_ids(), vec![c]);
        let _ = b;
    }

    #[test]
    fn activate_cambia_el_activo_solo_si_existe() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        let b = ws.create_untitled();
        assert!(ws.activate(a));
        assert_eq!(ws.active(), Some(a));
        assert!(!ws.activate(99));
        assert_eq!(ws.active(), Some(a));
        let _ = b;
    }

    #[test]
    fn close_devuelve_el_documento_y_lo_quita() {
        let mut ws = Workspace::new();
        let a = ws.open_path(p("/docs/a.md"), "A".into()).id;
        let closed = ws.close(a).unwrap();
        assert_eq!(closed.content(), "A");
        assert!(!ws.contains(a));
        assert_eq!(ws.close(a), None, "cerrar dos veces no hace nada");
    }

    #[test]
    fn close_del_activo_pasa_al_siguiente_o_al_anterior() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        let b = ws.create_untitled();
        let c = ws.create_untitled();
        ws.activate(b);
        ws.close(b);
        assert_eq!(ws.active(), Some(c), "el siguiente");
        ws.close(c);
        assert_eq!(ws.active(), Some(a), "el anterior si era el último");
        ws.close(a);
        assert_eq!(ws.active(), None, "sin documentos no hay activo");
    }

    #[test]
    fn close_de_otro_no_cambia_el_activo() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        let b = ws.create_untitled();
        ws.close(a);
        assert_eq!(ws.active(), Some(b));
    }

    #[test]
    fn resolve_or_create_sin_id_usa_el_activo() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        let _b = ws.create_untitled();
        ws.activate(a);
        assert_eq!(ws.resolve_or_create(None), Some(a));
        assert_eq!(ws.len(), 2);
    }

    #[test]
    fn resolve_or_create_sin_documentos_crea_uno() {
        let mut ws = Workspace::new();
        let id = ws.resolve_or_create(None).unwrap();
        assert_eq!(ws.ids(), vec![id]);
        assert_eq!(ws.active(), Some(id));
    }

    #[test]
    fn resolve_or_create_con_id_inexistente_no_redirige() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        assert_eq!(ws.resolve_or_create(Some(a)), Some(a));
        assert_eq!(ws.resolve_or_create(Some(a + 10)), None);
        assert_eq!(ws.len(), 1, "no crea nada");
    }

    #[test]
    fn resolve_no_crea() {
        let mut ws = Workspace::new();
        assert_eq!(ws.resolve(None), None);
        let a = ws.create_untitled();
        assert_eq!(ws.resolve(None), Some(a));
        assert_eq!(ws.resolve(Some(a)), Some(a));
        assert_eq!(ws.resolve(Some(a + 1)), None);
    }

    #[test]
    fn iter_recorre_en_orden_de_apertura() {
        let mut ws = Workspace::new();
        let a = ws.open_path(p("/docs/a.md"), "A".into()).id;
        let b = ws.create_untitled();
        let seen: Vec<_> = ws.iter().map(|(id, d)| (id, d.file_name())).collect();
        assert_eq!(seen, vec![(a, Some("a.md".to_string())), (b, None)]);
    }

    #[test]
    fn same_document_path_compara_sin_mayusculas_ni_separador() {
        assert!(same_document_path(&p("C:/Docs/A.md"), &p("c:/docs/a.md")));
        assert!(!same_document_path(&p("/docs/a.md"), &p("/docs/b.md")));
        assert!(!same_document_path(&p("/docs/a.md"), &p("/docs/sub/a.md")));
    }
}
