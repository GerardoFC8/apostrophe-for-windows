//! Logica de snapshots de autosave (nombres de archivo + politica). SIN I/O: el
//! adaptador (src-tauri) hace la escritura/lectura real. Modela la idea de respaldo
//! periodico del documento del upstream.

use std::path::PathBuf;

use crate::workspace::DocId;

/// Metadata que acompana a cada snapshot: ruta original del doc (si la habia) y
/// momento de guardado (epoch secs) para elegir el mas reciente al restaurar.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotMeta {
    pub original_path: Option<PathBuf>,
    pub saved_at: u64,
}

/// Politica: solo respaldar si hay cambios sin guardar Y el contenido no esta vacio
/// (un doc vacio no tiene nada que perder; espejo del `if not text` del upstream).
pub fn should_snapshot(dirty: bool, content_len: usize) -> bool {
    dirty && content_len > 0
}

/// Nombre del archivo de snapshot para un id dado.
pub fn snapshot_file_name(id: &str) -> String {
    format!("{id}.snapshot")
}

/// Nombre del archivo de metadata para un id dado.
pub fn meta_file_name(id: &str) -> String {
    format!("{id}.meta.json")
}

/// Extrae el id de un nombre de archivo de snapshot (".snapshot"); None si no lo es
/// (asi al enumerar el dir se ignoran .meta.json y archivos ajenos).
pub fn parse_snapshot_id(file_name: &str) -> Option<&str> {
    file_name.strip_suffix(".snapshot")
}

/// Id del snapshot de UN documento de la sesión: con varias pestañas cada documento
/// tiene su propio respaldo, así que el nombre combina la sesión y el `DocId`.
pub fn doc_snapshot_id(session: &str, doc: DocId) -> String {
    format!("{session}-{doc}")
}

/// ¿El archivo es un snapshot (o su metadata) de esta sesión? Sirve para borrar de una
/// vez todos los respaldos de la sesión al cerrar la ventana.
pub fn is_session_file(file_name: &str, session: &str) -> bool {
    let Some(rest) = file_name
        .strip_prefix(session)
        .and_then(|r| r.strip_prefix('-'))
    else {
        return false;
    };
    let id = rest
        .strip_suffix(".snapshot")
        .or_else(|| rest.strip_suffix(".meta.json"));
    id.is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// Ordena los snapshots huérfanos del más reciente al más antiguo: el primero es el que
/// restaura un frontend de un solo documento.
pub fn sort_newest_first<T>(items: &mut [(T, SnapshotMeta)]) {
    items.sort_by_key(|(_, meta)| std::cmp::Reverse(meta.saved_at));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_snapshot_dirty_y_con_contenido() {
        assert!(should_snapshot(true, 10));
    }

    #[test]
    fn should_snapshot_no_dirty() {
        assert!(!should_snapshot(false, 10));
    }

    #[test]
    fn should_snapshot_vacio() {
        assert!(!should_snapshot(true, 0));
    }

    #[test]
    fn snapshot_file_name_formato_correcto() {
        assert_eq!(snapshot_file_name("abc"), "abc.snapshot");
    }

    #[test]
    fn meta_file_name_formato_correcto() {
        assert_eq!(meta_file_name("abc"), "abc.meta.json");
    }

    #[test]
    fn parse_snapshot_id_detecta_snapshot() {
        assert_eq!(parse_snapshot_id("abc.snapshot"), Some("abc"));
    }

    #[test]
    fn parse_snapshot_id_ignora_meta_json() {
        assert_eq!(parse_snapshot_id("abc.meta.json"), None);
    }

    #[test]
    fn doc_snapshot_id_combina_sesion_y_documento() {
        assert_eq!(doc_snapshot_id("abc", 3), "abc-3");
        assert_ne!(doc_snapshot_id("abc", 3), doc_snapshot_id("abc", 4));
    }

    #[test]
    fn is_session_file_reconoce_snapshot_y_meta_de_la_sesion() {
        let id = doc_snapshot_id("abc", 3);
        assert!(is_session_file(&snapshot_file_name(&id), "abc"));
        assert!(is_session_file(&meta_file_name(&id), "abc"));
    }

    #[test]
    fn is_session_file_ignora_otras_sesiones_y_archivos_ajenos() {
        let otra = doc_snapshot_id("xyz", 3);
        assert!(!is_session_file(&snapshot_file_name(&otra), "abc"));
        assert!(!is_session_file("abc-3.md", "abc"));
        assert!(!is_session_file("abcd-3.snapshot", "abc"));
    }

    #[test]
    fn sort_newest_first_ordena_por_fecha_descendente() {
        let meta = |saved_at| SnapshotMeta {
            original_path: None,
            saved_at,
        };
        let mut items = vec![("a", meta(10)), ("b", meta(30)), ("c", meta(20))];
        sort_newest_first(&mut items);
        let ids: Vec<_> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn parse_snapshot_id_ignora_md() {
        assert_eq!(parse_snapshot_id("notas.md"), None);
    }
}
