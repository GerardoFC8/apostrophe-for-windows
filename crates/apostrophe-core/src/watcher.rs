//! Politica del watcher de cambios en disco. PURO: no depende de notify. El adaptador
//! traduce los eventos de notify a este ChangeKind y consulta estas funciones.

use std::path::Path;

/// Tipo de cambio observado, independiente del enum de notify (mantiene notify fuera
/// del dominio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Modify,
    Remove,
    Rename,
    Other,
}

/// ¿El evento corresponde al archivo que tenemos abierto? El watcher observa el
/// DIRECTORIO (en Windows ReadDirectoryChangesW entrega eventos de hermanos), asi que
/// hay que filtrar por ruta. Compara componente a componente IGNORANDO MAYUSCULAS
/// (Windows es case-insensitive y notify puede entregar el nombre con el casing real de
/// NTFS, distinto del casing con que se abrio el archivo) y es robusta a separadores
/// (la iteracion por componentes ignora / vs \).
pub fn is_relevant_change(watched: &Path, event_path: &Path) -> bool {
    let w: Vec<_> = watched.components().collect();
    let e: Vec<_> = event_path.components().collect();
    w.len() == e.len()
        && w.iter()
            .zip(&e)
            .all(|(a, b)| a.as_os_str().eq_ignore_ascii_case(b.as_os_str()))
}

/// ¿Este tipo de cambio amerita avisar al usuario? Modify/Remove/Rename si; eventos de
/// metadata/acceso (Other) no.
pub fn should_notify_frontend(kind: ChangeKind) -> bool {
    matches!(kind, ChangeKind::Modify | ChangeKind::Remove | ChangeKind::Rename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn is_relevant_change_misma_ruta_devuelve_true() {
        let ruta = PathBuf::from("C:/docs/nota.md");
        assert!(is_relevant_change(&ruta, &ruta));
    }

    #[test]
    fn is_relevant_change_rutas_distintas_devuelve_false() {
        let watched = PathBuf::from("C:/docs/nota.md");
        let event_path = PathBuf::from("C:/docs/otro.md");
        assert!(!is_relevant_change(&watched, &event_path));
    }

    #[test]
    fn is_relevant_change_diferente_casing_devuelve_true() {
        // Windows es case-insensitive: notify puede entregar el casing real de NTFS,
        // distinto del casing con que se abrio el archivo.
        let watched = PathBuf::from("/a/Readme.md");
        let event_path = PathBuf::from("/a/readme.md");
        assert!(is_relevant_change(&watched, &event_path));
    }

    #[test]
    fn should_notify_frontend_modify_es_true() {
        assert!(should_notify_frontend(ChangeKind::Modify));
    }

    #[test]
    fn should_notify_frontend_remove_es_true() {
        assert!(should_notify_frontend(ChangeKind::Remove));
    }

    #[test]
    fn should_notify_frontend_rename_es_true() {
        assert!(should_notify_frontend(ChangeKind::Rename));
    }

    #[test]
    fn should_notify_frontend_other_es_false() {
        assert!(!should_notify_frontend(ChangeKind::Other));
    }
}
