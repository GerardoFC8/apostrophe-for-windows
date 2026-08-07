//! Lista de documentos recientes (M16). Logica PURA: ordenar, deduplicar y recortar.
//! SIN I/O: leer y escribir el JSON es cosa del adaptador (src-tauri), igual que en
//! `snapshot`. Espejo del popover de recientes del upstream (`open_popover.py`), que alli
//! se apoya en el GtkRecentManager del sistema; en Windows no hay equivalente util, asi
//! que la lista se mantiene y persiste en la propia app.

use std::path::{Path, PathBuf};

/// Cuantas rutas se recuerdan. El upstream muestra una lista corta en un popover; mas
/// alla de una decena deja de ser "reciente" y se vuelve un historial que nadie lee.
pub const MAX_RECENTS: usize = 10;

/// Rutas recientes, de la mas nueva a la mas vieja.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recents {
    paths: Vec<PathBuf>,
}

/// Clave de comparacion de rutas.
///
/// En Windows el sistema de archivos NO distingue mayusculas y acepta las dos barras, asi
/// que `C:\Docs\a.md`, `c:\docs\A.MD` y `C:/Docs/a.md` son EL MISMO archivo. Comparar las
/// cadenas tal cual dejaria entradas duplicadas apuntando al mismo documento. Se compara
/// por esta clave normalizada pero se CONSERVA la ruta original: es la que se le muestra
/// al usuario y la que se abre.
fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

impl Recents {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let mut recents = Self::default();
        // Se reinsertan de la mas vieja a la mas nueva para que `push` aplique la misma
        // deduplicacion y el mismo recorte que en uso normal: una lista cargada de un JSON
        // manipulado a mano no puede entrar en un estado que `push` nunca produciria.
        for path in paths.into_iter().rev() {
            recents.push(path);
        }
        recents
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Registra un documento como el mas reciente. Si ya estaba, SUBE al frente en vez de
    /// duplicarse. La lista se recorta a `MAX_RECENTS`.
    pub fn push(&mut self, path: PathBuf) {
        let incoming = key(&path);
        self.paths.retain(|p| key(p) != incoming);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENTS);
    }

    /// Saca una ruta de la lista. Se usa cuando el archivo ya no existe.
    pub fn remove(&mut self, path: &Path) {
        let target = key(path);
        self.paths.retain(|p| key(p) != target);
    }

    /// Descarta las rutas que ya no existen en disco. El predicado lo inyecta el adaptador
    /// (aca no se toca el sistema de archivos), asi el filtrado se testea sin disco.
    pub fn retain_existing(&mut self, exists: impl Fn(&Path) -> bool) {
        self.paths.retain(|p| exists(p));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn el_mas_reciente_queda_primero() {
        let mut r = Recents::default();
        r.push(p("a.md"));
        r.push(p("b.md"));
        assert_eq!(r.paths(), [p("b.md"), p("a.md")]);
    }

    #[test]
    fn reabrir_sube_al_frente_sin_duplicar() {
        let mut r = Recents::default();
        r.push(p("a.md"));
        r.push(p("b.md"));
        r.push(p("a.md"));
        assert_eq!(r.paths(), [p("a.md"), p("b.md")]);
    }

    #[test]
    fn las_rutas_de_windows_no_distinguen_mayusculas_ni_barras() {
        // El mismo archivo escrito de tres formas distintas debe ocupar UNA sola entrada.
        let mut r = Recents::default();
        r.push(p(r"C:\Docs\a.md"));
        r.push(p(r"c:\docs\A.MD"));
        r.push(p("C:/Docs/a.md"));
        assert_eq!(r.paths().len(), 1, "deberia haber una sola entrada");
    }

    #[test]
    fn se_conserva_la_ruta_original_no_la_normalizada() {
        // La clave normalizada es solo para comparar: lo que se guarda y se le muestra al
        // usuario es la ruta tal cual vino.
        let mut r = Recents::default();
        r.push(p(r"C:\Docs\Informe Final.md"));
        assert_eq!(r.paths()[0], p(r"C:\Docs\Informe Final.md"));
    }

    #[test]
    fn la_lista_se_recorta_al_maximo() {
        let mut r = Recents::default();
        for i in 0..MAX_RECENTS + 5 {
            r.push(p(&format!("doc{i}.md")));
        }
        assert_eq!(r.paths().len(), MAX_RECENTS);
        // Se conservan los mas nuevos, no los primeros que entraron.
        assert_eq!(r.paths()[0], p(&format!("doc{}.md", MAX_RECENTS + 4)));
    }

    #[test]
    fn new_normaliza_una_lista_venida_de_disco() {
        // Un JSON editado a mano podria traer duplicados o pasarse del maximo: `new` debe
        // dejar el mismo estado que produciria el uso normal.
        let sucia = vec![p("a.md"), p("A.MD"), p("b.md")];
        let r = Recents::new(sucia);
        assert_eq!(r.paths(), [p("a.md"), p("b.md")]);
    }

    #[test]
    fn new_conserva_el_orden_de_entrada() {
        let r = Recents::new(vec![p("a.md"), p("b.md"), p("c.md")]);
        assert_eq!(r.paths(), [p("a.md"), p("b.md"), p("c.md")]);
    }

    #[test]
    fn remove_saca_la_entrada_sin_distinguir_mayusculas() {
        let mut r = Recents::default();
        r.push(p(r"C:\Docs\a.md"));
        r.push(p("b.md"));
        r.remove(&p(r"c:\docs\A.MD"));
        assert_eq!(r.paths(), [p("b.md")]);
    }

    #[test]
    fn retain_existing_descarta_los_que_ya_no_estan() {
        let mut r = Recents::new(vec![p("vive.md"), p("borrado.md"), p("vive2.md")]);
        r.retain_existing(|path| path != Path::new("borrado.md"));
        assert_eq!(r.paths(), [p("vive.md"), p("vive2.md")]);
    }

    #[test]
    fn una_lista_vacia_se_reporta_vacia() {
        let mut r = Recents::default();
        assert!(r.is_empty());
        r.push(p("a.md"));
        assert!(!r.is_empty());
    }
}
