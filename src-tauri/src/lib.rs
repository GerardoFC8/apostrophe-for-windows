// Spike de preview (ADR-0001, D1): valida el pipeline Rust -> pandoc -> WebView.
// En el build de Windows, pandoc pasará a ser un sidecar (externalBin). Acá usamos el
// pandoc del sistema para de-riskear el PIPELINE en WSL. OJO: en WSL el render es
// WebKitGTK (vía WSLg), NO WebView2; el render real se re-valida en el build de Windows.
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;

/// Convierte Markdown a HTML5 con pandoc, replicando (un subset de) el pipeline de
/// preview de Apostrophe: `from=markdown`, `to=html5`, `--standalone --mathjax`.
///
/// TODO(ADR-0001): añadir `--css=<tema>`, `--metadata=base_path:<dir>`,
/// `--lua-filter=relative_to_absolute.lua` y `--highlight-style=breezedark` (modo oscuro)
/// cuando existan los assets de tema y el filtro Lua.
#[tauri::command]
fn preview_markdown(text: String) -> Result<String, String> {
    // Los args ya no estan hardcodeados: los construye el Converter port del dominio
    // (apostrophe_core::convert). El spawn sigue aca (adaptador). M2: el binario
    // pasara de `pandoc` del sistema a un sidecar bundleado.
    let args = apostrophe_core::convert::preview_args(&apostrophe_core::convert::PreviewOptions::default());
    let mut child = Command::new("pandoc")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo lanzar pandoc: {e} (¿instalado? `sudo apt install pandoc`)"))?;

    // Escribimos el stdin en un hilo aparte para evitar un deadlock en documentos grandes:
    // si pandoc llena el buffer de su stdout mientras nosotros seguimos escribiendo en su
    // stdin, ambos procesos se bloquean esperándose. Leer stdout (wait_with_output) en el
    // hilo principal mientras otro hilo alimenta stdin rompe ese ciclo.
    let mut stdin = child.stdin.take().ok_or("no se pudo abrir el stdin de pandoc")?;
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(text.as_bytes());
        // `stdin` se cierra al hacer drop al salir del hilo -> EOF para pandoc.
    });

    let output = child
        .wait_with_output()
        .map_err(|e| format!("error esperando a pandoc: {e}"))?;
    let _ = writer.join();

    if !output.status.success() {
        return Err(format!(
            "pandoc devolvió error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Adaptador primario (IPC): expone el contador de stats del dominio. La logica
/// vive en `apostrophe_core::stats`; este comando solo cruza la frontera hexagonal.
/// La frontera es gruesa y con debounce (ADR-0001): se manda el texto, vuelve Stats.
#[tauri::command]
fn count_stats(text: String) -> apostrophe_core::stats::Stats {
    apostrophe_core::stats::count(&text)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![preview_markdown, count_stats])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
