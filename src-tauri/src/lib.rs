// Adaptador primario (IPC) de la arquitectura hexagonal (ADR-0001): cruza la
// frontera dominio<->presentacion. La logica vive en `apostrophe-core`; aca solo
// se orquestan los procesos externos (pandoc) y se serializa el resultado.
//
// M2: pandoc paso de ser el binario del SISTEMA a un SIDECAR bundleado.
// M4: abrir/guardar .md con DocumentStore (managed state) + dialogos nativos.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use apostrophe_core::document::DocumentStore;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
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
// tauri-plugin-dialog.
//
// CRITICO (review M4): los comandos que abren dialogos son `async` y corren el
// dialogo en `spawn_blocking`. Si fueran sincronos correrian INLINE en el main
// thread de WebView2, y `blocking_pick_file`/`blocking_save_file` bloquean ese
// thread mientras el dialogo se encola en el MISMO thread -> DEADLOCK (la app se
// congela). En async + spawn_blocking el bloqueo ocurre en un hilo de pool y el
// main thread queda libre para procesar el dialogo.

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
/// el estado para el indicador de "modificado". No abre dialogos -> puede ser sincrono.
#[tauri::command]
fn set_content(text: String, doc: tauri::State<'_, Doc>) -> DocStatus {
    let mut store = doc.lock().unwrap();
    store.set_content(text);
    DocStatus {
        file_name: store.file_name(),
        dirty: store.is_dirty(),
    }
}

/// Abre un .md. Si el documento actual tiene cambios sin guardar, CONFIRMA antes
/// (anti-perdida de datos, review M4). `async` + dialogos en spawn_blocking.
#[tauri::command]
async fn open_document(
    app: tauri::AppHandle,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<OpenedDoc>, String> {
    // Guarda anti-perdida: si hay cambios sin guardar, pedir confirmacion.
    if doc.lock().unwrap().is_dirty() {
        let app2 = app.clone();
        let discard = tauri::async_runtime::spawn_blocking(move || {
            app2.dialog()
                .message("Tenés cambios sin guardar que se van a perder. ¿Abrir otro archivo igual?")
                .title("Cambios sin guardar")
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Descartar y abrir".into(),
                    "Cancelar".into(),
                ))
                .blocking_show()
        })
        .await
        .map_err(|e| format!("error en el dialogo: {e}"))?;
        if !discard {
            return Ok(None); // el usuario cancelo: NO se pierde nada
        }
    }

    // Picker en un hilo de pool (NO en el main thread -> sin deadlock).
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .file()
            .add_filter("Markdown", &["md", "markdown", "txt"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("error en el dialogo: {e}"))?;

    let Some(file_path) = picked else {
        return Ok(None); // el usuario cancelo el picker
    };
    let path = file_path
        .into_path()
        .map_err(|e| format!("ruta invalida: {e}"))?;
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;

    // Datos para el frontend, tomados directo de lo ya leido (no re-derivar del store).
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path_str = path.to_string_lossy().into_owned();

    doc.lock().unwrap().open(path, content.clone());
    Ok(Some(OpenedDoc {
        path: path_str,
        file_name,
        content,
    }))
}

/// Guarda: si el doc tiene path escribe ahi; si no, delega en "guardar como".
/// `async` por la posible delegacion al dialogo de save_document_as.
#[tauri::command]
async fn save_document(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<DocStatus>, String> {
    let existing = doc.lock().unwrap().path().cloned();
    let Some(path) = existing else {
        return save_document_as(app, text, doc).await;
    };
    std::fs::write(&path, &text).map_err(|e| format!("no se pudo guardar: {e}"))?;
    // Actualizacion de estado atomica bajo un solo lock (set_content + mark_clean).
    let mut store = doc.lock().unwrap();
    store.set_content(text);
    store.mark_clean();
    Ok(Some(DocStatus {
        file_name: store.file_name(),
        dirty: false,
    }))
}

/// Guardar como: siempre dialogo nativo (en spawn_blocking). Fija el path y marca limpio.
#[tauri::command]
async fn save_document_as(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<DocStatus>, String> {
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .file()
            .add_filter("Markdown", &["md"])
            .set_file_name("sin-titulo.md")
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("error en el dialogo: {e}"))?;

    let Some(file_path) = picked else {
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

/// Exporta el markdown a un archivo HTML elegido por el usuario (dialogo nativo).
/// Reusa el sidecar pandoc via el Converter port (export_args). Devuelve la ruta de
/// salida, o `None` si se cancela. `async` + spawn_blocking (gotcha deadlock M4).
#[tauri::command]
async fn export_document(app: tauri::AppHandle, text: String) -> Result<Option<String>, String> {
    use apostrophe_core::convert::{export_args, ExportFormat};

    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .file()
            .add_filter("HTML", &[ExportFormat::Html.extension()])
            .set_file_name("documento.html")
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("error en el dialogo: {e}"))?;

    let Some(file_path) = picked else {
        return Ok(None);
    };
    let out_path = file_path
        .into_path()
        .map_err(|e| format!("ruta invalida: {e}"))?;
    let out_str = out_path.to_string_lossy().into_owned();

    // Markdown a un temporal; pandoc exporta con --output a la ruta elegida.
    let seq = PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!(
        "apostrophe-export-{}-{}.md",
        std::process::id(),
        seq
    ));
    std::fs::write(&tmp, text.as_bytes())
        .map_err(|e| format!("no se pudo escribir el temporal de export: {e}"))?;

    let mut args = vec![tmp.to_string_lossy().into_owned()];
    args.extend(export_args(ExportFormat::Html, &out_str));

    let result = run_pandoc_sidecar(&app, args).await;
    let _ = std::fs::remove_file(&tmp);
    result.map(|_| Some(out_str)) // pandoc escribe en --output; su stdout queda vacio
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
            save_document_as,
            export_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
