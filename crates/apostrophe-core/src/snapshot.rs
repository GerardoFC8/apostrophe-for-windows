//! Logica de snapshots de autosave (nombres de archivo + politica). SIN I/O: el
//! adaptador (src-tauri) hace la escritura/lectura real. Modela la idea de respaldo
//! periodico del documento del upstream.

use std::path::PathBuf;

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
    fn parse_snapshot_id_ignora_md() {
        assert_eq!(parse_snapshot_id("notas.md"), None);
    }
}
