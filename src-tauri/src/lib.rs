// Adaptador primario (IPC) de la arquitectura hexagonal (ADR-0001): cruza la
// frontera dominio<->presentacion. La logica vive en `apostrophe-core`; aca solo
// se orquestan los procesos externos (pandoc) y se serializa el resultado.
//
// M2: pandoc paso de ser el binario del SISTEMA a un SIDECAR bundleado
// (externalBin). La app ya no depende de que el usuario tenga pandoc instalado.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use apostrophe_core::document::DocumentStore;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// Secuencia para nombrar temporales unicos por invocacion (evita choques entre
/// previews concurrentes que dispara el debounce del frontend).
static PREVIEW_SEQ: AtomicU64 = AtomicU64::new(0);

/// Convierte Markdown a HTML5 con pandoc via SIDECAR bundleado (M2). Escribimos el
/// markdown a un archivo temporal y le pasamos la RUTA a pandoc: evita tener que
/// cerrar el stdin del sidecar (el plugin no expone un EOF limpio) y es robusto en
/// documentos grandes. Los flags los arma el Converter port del dominio.
///
/// TODO(ADR-0001): theme (`--css`), base_path, lua-filter y highlight-style cuando
/// existan los assets de tema (M5).
#[tauri::command]
async fn preview_markdown(app: tauri::AppHandle, text: String) -> Result<String, String> {
    let seq = PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!(
        "apostrophe-preview-{}-{}.md",
        std::process::id(),
        seq
    ));
    std::fs::write(&tmp, text.as_bytes())
        .map_err(|e| format!("no se pudo escribir el temporal de preview: {e}"))?;

    // Args: <ruta-temporal> seguido de los flags puros del dominio (Converter port).
    let mut args = vec![tmp.to_string_lossy().into_owned()];
    args.extend(apostrophe_core::convert::preview_args(
        &apostrophe_core::convert::PreviewOptions::default(),
    ));

    let result = run_pandoc_sidecar(&app, args).await;
    let _ = std::fs::remove_file(&tmp); // limpieza best-effort, pase lo que pase
    result
}

/// Lanza el sidecar pandoc y junta su stdout. Aislado para mantener `preview_markdown`
/// legible y garantizar el borrado del temporal aunque pandoc falle.
async fn run_pandoc_sidecar(app: &tauri::AppHandle, args: Vec<String>) -> Result<String, String> {
    let sidecar = app
        .shell()
        .sidecar("pandoc")
        .map_err(|e| format!("no se encontro el sidecar pandoc: {e}"))?;
    let (mut rx, _child) = sidecar
        .args(args)
        .spawn()
        .map_err(|e| format!("no se pudo lanzar el sidecar pandoc: {e}"))?;

    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr = String::new();
    let mut code: Option<i32> = None;
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(chunk) => stdout.extend_from_slice(&chunk),
            CommandEvent::Stderr(chunk) => stderr.push_str(&String::from_utf8_lossy(&chunk)),
            CommandEvent::Error(err) => stderr.push_str(&err),
            CommandEvent::Terminated(payload) => code = payload.code,
            _ => {}
        }
    }

    if code != Some(0) {
        return Err(format!("pandoc devolvio error (codigo {code:?}): {stderr}"));
    }
    String::from_utf8(stdout).map_err(|e| format!("la salida de pandoc no es UTF-8: {e}"))
}

/// Adaptador primario (IPC): expone el contador de stats del dominio. La logica
/// vive en `apostrophe_core::stats`; este comando solo cruza la frontera hexagonal.
/// La frontera es gruesa y con debounce (ADR-0001): se manda el texto, vuelve Stats.
#[tauri::command]
fn count_stats(text: String) -> apostrophe_core::stats::Stats {
    apostrophe_core::stats::count(&text)
}

// --- M4: gestion de documento (abrir / guardar) ---
// El estado (path/content/dirty) vive en el DocumentStore del dominio como managed
// state (Mutex). El I/O de disco se hace aca con std::fs; los pickers nativos con
// tauri-plugin-dialog. La WebView solo dispara los comandos y refleja el estado.

type Doc = Mutex<DocumentStore>;

/// Documento recien abierto que cruza al frontend.
#[derive(serde::Serialize)]
struct OpenedDoc {
    path: String,
    file_name: String,
    content: String,
}

/// Estado para la titlebar: nombre de archivo + indicador de cambios sin guardar.
#[derive(serde::Serialize)]
struct DocStatus {
    file_name: Option<String>,
    dirty: bool,
}

/// Sincroniza el texto del editor con el store (se llama en el debounce) y devuelve
/// el estado para el indicador de "modificado".
#[tauri::command]
fn set_content(text: String, doc: tauri::State<'_, Doc>) -> DocStatus {
    let mut store = doc.lock().unwrap();
    store.set_content(text);
    DocStatus {
        file_name: store.file_name(),
        dirty: store.is_dirty(),
    }
}

/// Abre un .md con dialogo nativo + lectura de disco (std::fs). `None` si se cancela.
#[tauri::command]
fn open_document(
    app: tauri::AppHandle,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<OpenedDoc>, String> {
    let Some(file_path) = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md", "markdown", "txt"])
        .blocking_pick_file()
    else {
        return Ok(None); // el usuario cancelo el dialogo
    };
    let path = file_path
        .into_path()
        .map_err(|e| format!("ruta invalida: {e}"))?;
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;

    let mut store = doc.lock().unwrap();
    store.open(path, content.clone());
    Ok(Some(OpenedDoc {
        path: store
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        file_name: store.file_name().unwrap_or_default(),
        content,
    }))
}

/// Guarda: si el doc tiene path escribe ahi y marca limpio; si no, delega en
/// "guardar como". `None` si se cancela el dialogo de guardar como.
#[tauri::command]
fn save_document(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<DocStatus>, String> {
    let existing = doc.lock().unwrap().path().cloned();
    let Some(path) = existing else {
        return save_document_as(app, text, doc);
    };
    std::fs::write(&path, &text).map_err(|e| format!("no se pudo guardar: {e}"))?;
    let mut store = doc.lock().unwrap();
    store.set_content(text);
    store.mark_clean();
    Ok(Some(DocStatus {
        file_name: store.file_name(),
        dirty: false,
    }))
}

/// Guardar como: siempre dialogo nativo. Fija el nuevo path y marca limpio.
#[tauri::command]
fn save_document_as(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<DocStatus>, String> {
    let Some(file_path) = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .set_file_name("sin-titulo.md")
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = file_path
        .into_path()
        .map_err(|e| format!("ruta invalida: {e}"))?;
    std::fs::write(&path, &text).map_err(|e| format!("no se pudo guardar: {e}"))?;
    let mut store = doc.lock().unwrap();
    store.set_content(text);
    store.saved_as(path);
    Ok(Some(DocStatus {
        file_name: store.file_name(),
        dirty: false,
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(DocumentStore::new()))
        .invoke_handler(tauri::generate_handler![
            preview_markdown,
            count_stats,
            set_content,
            open_document,
            save_document,
            save_document_as
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
