//! Un modulo por subcomando. Cada uno expone `execute(args, env, …) -> Result<Report,
//! CliError>`: hace el trabajo y devuelve DATOS, sin escribir nada. `run` decide como
//! mostrarlos (texto o JSON) y con que codigo terminar; un futuro servidor MCP puede
//! reutilizar los mismos `execute` y serializar los mismos reportes.

use serde::Serialize;

/// El resultado de un comando que termino bien. Su forma serializada, con `"ok":true`
/// delante, es lo que imprime `--json`.
pub trait Report: Serialize {
    /// Lo que va a stdout sin `--json` (vacio = nada).
    fn text(&self) -> String;

    /// Avisos para stderr sin `--json`; con `--json` ya viajan dentro del objeto.
    fn warnings(&self) -> Vec<String> {
        Vec::new()
    }

    /// `true` si hubo hallazgos (errores ortograficos, tablas desalineadas): exit 1.
    fn has_findings(&self) -> bool {
        false
    }
}
pub mod export;
pub mod open;
pub mod spell;
pub mod stats;
pub mod tables;
