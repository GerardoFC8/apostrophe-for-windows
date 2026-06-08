// Adaptador primario (IPC) de la arquitectura hexagonal (ADR-0001): cruza la
// frontera dominio<->presentacion. La logica vive en `apostrophe-core`; aca solo
// se orquestan los procesos externos (pandoc) y se serializa el resultado.
//
// M2: pandoc paso de ser el binario del SISTEMA a un SIDECAR bundleado.
// M4: abrir/guardar .md con DocumentStore (managed state) + dialogos nativos.
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use apostrophe_core::document::DocumentStore;
use apostrophe_core::spellcheck::{Misspelling, SpellChecker, SpellbookDictionary};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_window_state::StateFlags;

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

/// Guard anti-perdida: si el doc tiene cambios sin guardar, pide confirmacion (dialogo
/// nativo en spawn_blocking -> sin deadlock). Devuelve `true` si se puede proceder (no
/// habia cambios, o el usuario eligio descartar). Reusado por open_document, open_path,
/// confirm_close (M8.1) y new_document (M8.1) con su propio titulo, mensaje y label.
///
/// Patron GNOME: `title` = pregunta corta (encabezado), `message` = explicacion. NO
/// repetir texto entre ambos: rfd usa el title como encabezado Y como titulo de ventana
/// (en GTK lo muestra dos veces), asi que repetir la frase la duplica visualmente.
async fn confirm_discard_if_dirty(
    app: &tauri::AppHandle,
    doc: &Doc,
    title: &str,
    message: &str,
    confirm_label: &str,
) -> Result<bool, String> {
    // El guard del Mutex se suelta al terminar el statement: NO se cruza el .await.
    let dirty = doc.lock().unwrap().is_dirty();
    if !dirty {
        return Ok(true);
    }
    let app2 = app.clone();
    let title = title.to_string();
    let message = message.to_string();
    let confirm_label = confirm_label.to_string();
    let discard = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .message(message)
            .title(title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                confirm_label,
                "Cancelar".into(),
            ))
            .blocking_show()
    })
    .await
    .map_err(|e| format!("error en el dialogo: {e}"))?;
    Ok(discard)
}

/// Lee un .md de disco a un OpenedDoc y lo fija en el store. Unica fuente del flujo
/// "leer archivo -> estado del documento", reusada por el dialogo (open_document), el
/// arranque en frio (setup) y la apertura via asociacion (open_path).
fn load_path_into_store(path: PathBuf, doc: &Doc) -> Result<OpenedDoc, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path_str = path.to_string_lossy().into_owned();
    doc.lock().unwrap().open(path, content.clone());
    Ok(OpenedDoc {
        path: path_str,
        file_name,
        content,
    })
}

/// Abre un .md via DIALOGO nativo. Si el documento actual tiene cambios sin guardar,
/// CONFIRMA antes (anti-perdida, review M4). `async` + dialogos en spawn_blocking.
#[tauri::command]
async fn open_document(
    app: tauri::AppHandle,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<OpenedDoc>, String> {
    if !confirm_discard_if_dirty(
        &app,
        &doc,
        "¿Abrir otro archivo?",
        "El documento actual tiene cambios que se perderán.",
        "Descartar y abrir",
    )
    .await?
    {
        return Ok(None); // el usuario cancelo: NO se pierde nada
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
    load_path_into_store(path, &doc).map(Some)
}

/// Capa B: abre un .md por RUTA (no por dialogo). Lo usa la apertura via asociacion
/// de archivo cuando la app YA esta abierta: single-instance reenvia el path y el
/// frontend invoca este comando. Aplica el mismo guard anti-perdida que open_document.
/// Devuelve None si el usuario cancela.
#[tauri::command]
async fn open_path(
    app: tauri::AppHandle,
    path: String,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<OpenedDoc>, String> {
    if !confirm_discard_if_dirty(
        &app,
        &doc,
        "¿Abrir otro archivo?",
        "El documento actual tiene cambios que se perderán.",
        "Descartar y abrir",
    )
    .await?
    {
        return Ok(None);
    }
    load_path_into_store(PathBuf::from(path), &doc).map(Some)
}

/// M8.1: confirma el cierre de la ventana si hay cambios sin guardar. La invoca el
/// handler `onCloseRequested` del frontend. Sincroniza PRIMERO el texto actual del
/// editor: el debounce de `set_content` puede estar hasta 250ms atrasado, y sin esto
/// teclear-y-cerrar de inmediato perdería esos últimos tecleos sin avisar. Devuelve
/// `true` si se puede cerrar (no había cambios o el usuario eligió descartar).
#[tauri::command]
async fn confirm_close(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
) -> Result<bool, String> {
    doc.lock().unwrap().set_content(text);
    confirm_discard_if_dirty(
        &app,
        &doc,
        "¿Cerrar sin guardar?",
        "El documento tiene cambios que se perderán.",
        "Descartar y cerrar",
    )
    .await
}

/// M8.1: documento nuevo (Ctrl+N). Sincroniza el texto actual, confirma descarte si hay
/// cambios sin guardar y, si se procede, resetea el store a documento nuevo vacío.
/// Devuelve el nuevo estado para la titlebar, o `None` si el usuario canceló.
#[tauri::command]
async fn new_document(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
) -> Result<Option<DocStatus>, String> {
    doc.lock().unwrap().set_content(text);
    if !confirm_discard_if_dirty(
        &app,
        &doc,
        "¿Crear un documento nuevo?",
        "El documento actual tiene cambios que se perderán.",
        "Descartar y crear",
    )
    .await?
    {
        return Ok(None);
    }
    let mut store = doc.lock().unwrap();
    store.reset();
    Ok(Some(DocStatus {
        file_name: store.file_name(),
        dirty: store.is_dirty(),
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

// --- M6: spellcheck ---
// El diccionario es CARO de construir y BARATO de consultar: se carga UNA vez (en el
// setup, off-thread) y se cachea en managed state. Arc para clonar barato bajo un lock
// breve y soltar el lock antes del computo (no serializar otros comandos).

/// Diccionario cargado (`None` hasta que el setup termina de cargarlo en background).
pub struct SpellState(pub RwLock<Option<Arc<SpellbookDictionary>>>);

/// Chequeo ortografico. Contrato: spellcheck(text, focus_mode) -> [{word, from, to,
/// suggestions}] con char-offsets. focus_mode=true corto-circuita a [] (ADR-0001 D2).
/// El chequeo de doc completo es CPU-bound -> spawn_blocking.
#[tauri::command]
async fn spellcheck(
    text: String,
    focus_mode: bool,
    state: tauri::State<'_, SpellState>,
) -> Result<Vec<Misspelling>, String> {
    if focus_mode {
        return Ok(Vec::new()); // ni lock ni computo
    }
    // Clonar el Arc bajo lock BREVE; soltarlo antes del computo pesado.
    let dict = {
        let guard = state.0.read().unwrap();
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => return Ok(Vec::new()), // dict aun no cargado: spellcheck no critico
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        SpellChecker::check_text(dict.as_ref(), &text, false)
    })
    .await
    .map_err(|e| format!("error en el chequeo ortografico: {e}"))
}

/// Lee un archivo de diccionario. Bundle: via BaseDirectory::Resource. Dev: fallback
/// al dir del crate (donde fetch-dictionaries.ps1 deja los archivos), porque en
/// `tauri dev` el resolver de Resource puede no encontrar los resources.
fn read_dict_file(app: &tauri::AppHandle, name: &str) -> Result<String, String> {
    if let Ok(p) = app
        .path()
        .resolve(format!("dictionaries/{name}"), BaseDirectory::Resource)
    {
        if let Ok(s) = std::fs::read_to_string(&p) {
            return Ok(s);
        }
    }
    let dev_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/dictionaries/");
    std::fs::read_to_string(format!("{dev_dir}{name}")).map_err(|e| format!("{name}: {e}"))
}

/// Carga el diccionario es en background (off-thread, es caro) y lo guarda en
/// SpellState. Si falla (no provisionado), el spellcheck degrada graciosamente ([]).
fn load_dictionary(app: &tauri::AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let h = handle.clone();
        let loaded = tauri::async_runtime::spawn_blocking(move || -> Result<SpellbookDictionary, String> {
            let aff = read_dict_file(&h, "es.aff")?;
            let dic = read_dict_file(&h, "es.dic")?;
            SpellbookDictionary::load(&aff, &dic)
        })
        .await;
        match loaded {
            Ok(Ok(dict)) => {
                *handle.state::<SpellState>().0.write().unwrap() = Some(Arc::new(dict));
                // Avisar al front para re-chequear (el chequeo inicial pudo volver []
                // porque el dict aun no estaba cargado) — review M6.
                let _ = handle.emit("spell-dict-ready", ());
            }
            Ok(Err(e)) => eprintln!("spellcheck: no se pudo cargar el diccionario es: {e}"),
            Err(e) => eprintln!("spellcheck: error de join cargando el diccionario: {e}"),
        }
    });
}

// --- Capa B: asociacion de archivos (.md) ---
// En Windows el doble-click en un .md ejecuta `apostrophe.exe <ruta>`: el path llega
// como ARGUMENTO de CLI. `RunEvent::Opened` NO se dispara en Windows (solo macOS/iOS/
// Android), por eso se lee de argv. Dos caminos:
//  - app cerrada: setup() resuelve argv -> PendingFile; el frontend lo consume al cargar.
//  - app abierta: single-instance reenvia el argv -> evento "open-file-path" al frontend.

/// Extrae la ruta del archivo a abrir desde argv. Ignora argv[0] (el exe) y los flags
/// (empiezan con '-'); devuelve la primera ruta "normal", o None. No valida existencia:
/// el lector reporta un error legible si la ruta no se puede leer.
fn file_arg(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|a| !a.is_empty() && !a.starts_with('-'))
        .cloned()
}

/// Archivo pendiente del ARRANQUE EN FRIO (asociacion .md con la app cerrada): se
/// resuelve en setup() desde argv y el frontend lo consume (y limpia) al iniciar.
struct PendingFile(Mutex<Option<OpenedDoc>>);

/// Devuelve (y limpia) el archivo pendiente del arranque en frio, si lo hay. El frontend
/// lo llama al iniciar para cargar el .md con el que se abrio la app.
#[tauri::command]
fn take_pending_file(pending: tauri::State<'_, PendingFile>) -> Option<OpenedDoc> {
    pending.0.lock().unwrap().take()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance PRIMERO (requisito del plugin): si ya hay una instancia, la
        // nueva (lanzada por Windows al abrir un .md) reenvia su argv y se cierra.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Traer el foco a la ventana existente.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_focus();
            }
            // Apertura via asociacion con la app YA abierta: emitir el path al frontend,
            // que hace el open guardado (open_path). NO leemos el archivo aca: este
            // callback corre en el event-loop y leer/dialogar podria bloquear (deadlock M4).
            if let Some(path) = file_arg(&args) {
                let _ = app.emit("open-file-path", path);
            }
        }))
        // M8.2: recuerda la geometria de la ventana entre sesiones (restaura al crear la
        // ventana, guarda al salir). Excluye DECORATIONS (la titlebar es custom,
        // decorations:false -> restaurar el marco nativo daria doble barra) y VISIBLE (la
        // ventana siempre debe mostrarse al arrancar). Solo geometria real.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    StateFlags::SIZE
                        | StateFlags::POSITION
                        | StateFlags::MAXIMIZED
                        | StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(DocumentStore::new()))
        .manage(SpellState(RwLock::new(None)))
        .manage(PendingFile(Mutex::new(None)))
        .setup(|app| {
            load_dictionary(app.handle()); // carga el diccionario es en background
            // Capa B: arranque en frio via asociacion .md (app cerrada). Windows pasa el
            // path en argv; lo resolvemos a un OpenedDoc y lo dejamos pendiente para que
            // el frontend lo cargue al iniciar. Si falla (ruta invalida), se ignora y la
            // app abre vacia.
            let argv: Vec<String> = std::env::args().collect();
            if let Some(path) = file_arg(&argv) {
                let doc = app.state::<Doc>();
                match load_path_into_store(PathBuf::from(&path), &doc) {
                    Ok(opened) => *app.state::<PendingFile>().0.lock().unwrap() = Some(opened),
                    Err(e) => eprintln!("no se pudo abrir el archivo de inicio '{path}': {e}"),
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            preview_markdown,
            count_stats,
            set_content,
            open_document,
            open_path,
            confirm_close,
            new_document,
            save_document,
            save_document_as,
            export_document,
            spellcheck,
            take_pending_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::file_arg;

    #[test]
    fn file_arg_devuelve_la_ruta_tras_el_exe() {
        let args = vec!["apostrophe.exe".to_string(), "C:\\docs\\nota.md".to_string()];
        assert_eq!(file_arg(&args).as_deref(), Some("C:\\docs\\nota.md"));
    }

    #[test]
    fn file_arg_ignora_flags() {
        let args = vec![
            "apostrophe.exe".to_string(),
            "--algun-flag".to_string(),
            "nota.md".to_string(),
        ];
        assert_eq!(file_arg(&args).as_deref(), Some("nota.md"));
    }

    #[test]
    fn file_arg_none_cuando_solo_esta_el_exe() {
        let args = vec!["apostrophe.exe".to_string()];
        assert_eq!(file_arg(&args), None);
    }
}
