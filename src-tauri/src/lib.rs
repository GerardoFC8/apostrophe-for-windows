// Adaptador primario (IPC) de la arquitectura hexagonal (ADR-0001): cruza la
// frontera dominio<->presentacion. La logica vive en `apostrophe-core`; aca solo
// se orquestan los procesos externos (pandoc) y se serializa el resultado.
//
// M2: pandoc paso de ser el binario del SISTEMA a un SIDECAR bundleado.
// M4: abrir/guardar .md con DocumentStore (managed state) + dialogos nativos.
// M8.4a: autosave por snapshots en app_local_data_dir()/snapshots.
// M8.4b: watcher de cambios externos (notify-debouncer-full).
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify_debouncer_full::{
    new_debouncer,
    notify::{EventKind, RecursiveMode},
    Debouncer, RecommendedCache,
};

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

/// M10: parser de resaltado de Markdown (ADR-0001 D1). CPU-bound (regex con backtracking
/// sobre el documento completo) -> `spawn_blocking` para no bloquear el hilo de eventos,
/// igual que `spellcheck`. Sin estado ni `focus_mode`: el highlight sigue activo en modo
/// enfoque (el upstream mantiene los tags de markup; solo el corrector se apaga en foco).
#[tauri::command]
async fn parse_markup(text: String) -> Result<Vec<apostrophe_core::markup::Span>, String> {
    tauri::async_runtime::spawn_blocking(move || apostrophe_core::markup::parse_markup(&text))
        .await
        .map_err(|e| format!("error al parsear el markup: {e}"))
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

// --- M8.4b: watcher de cambios externos ---
// El Debouncer vive en managed state y se re-arma al abrir/guardar un archivo.
// Se suspende (drop) alrededor de nuestros propios guardados para evitar el
// auto-disparo. El handler solo traduce + emite: NUNCA toma locks del Doc ni
// abre dialogos (gotcha anti-deadlock M4).

/// Watcher del archivo abierto. Se re-arma al abrir/guardar-como (cambia el path) y
/// se suspende+re-arma alrededor de nuestros propios guardados (evita el auto-disparo).
struct FileWatcher(Mutex<Option<Debouncer<notify_debouncer_full::notify::RecommendedWatcher, RecommendedCache>>>);

/// Suspende el watcher (lo dropea -> deja de observar). Se llama ANTES de un write
/// propio para que notify no detecte nuestro guardado como cambio externo.
fn suppress_watcher(state: &FileWatcher) {
    *state.0.lock().unwrap() = None;
}

/// Traduce notify EventKind -> apostrophe_core::watcher::ChangeKind (dominio puro).
/// Solo Modify(Data) y Modify(Name) son notificables; Modify(Metadata/Any/Other)
/// son cambios de atributos/acceso sin cambio de contenido -> Other (no notificar).
fn to_change_kind(kind: &EventKind) -> apostrophe_core::watcher::ChangeKind {
    use apostrophe_core::watcher::ChangeKind;
    use notify_debouncer_full::notify::event::ModifyKind;
    match kind {
        EventKind::Modify(ModifyKind::Data(_)) => ChangeKind::Modify,
        EventKind::Modify(ModifyKind::Name(_)) => ChangeKind::Rename,
        EventKind::Modify(_) => ChangeKind::Other, // metadata/any/other: no notificar
        EventKind::Remove(_) => ChangeKind::Remove,
        EventKind::Create(_) => ChangeKind::Modify, // crear en ruta vigilada = reemplazo
        _ => ChangeKind::Other,
    }
}

/// Arma (o re-arma) el watcher sobre el archivo actualmente abierto en el store.
/// Sustituye el Debouncer anterior si lo habia (el drop del anterior detiene su hilo).
/// Degrada con gracia si notify falla (log + sin watcher, no crashea).
fn rewatch_doc(app: &tauri::AppHandle, doc: &Doc) {
    use apostrophe_core::watcher::{is_relevant_change, should_notify_frontend};

    // Leer el path bajo lock BREVE; soltarlo antes de crear el debouncer.
    let path = {
        let guard = doc.lock().unwrap();
        guard.path().cloned()
    };
    let Some(path) = path else {
        // Documento nuevo sin path: no hay archivo que observar.
        let fw = app.state::<FileWatcher>();
        suppress_watcher(&fw);
        return;
    };

    let app_handle = app.clone();
    let watched = path.clone();

    let result = new_debouncer(
        Duration::from_millis(500),
        None,
        move |result: notify_debouncer_full::DebounceEventResult| {
            let events = match result {
                Ok(evs) => evs,
                Err(errs) => {
                    for e in errs {
                        eprintln!("watcher: error de notify: {e}");
                    }
                    return;
                }
            };
            for debounced in events {
                let kind = to_change_kind(&debounced.event.kind);
                for event_path in &debounced.event.paths {
                    if is_relevant_change(&watched, event_path)
                        && should_notify_frontend(kind)
                    {
                        let _ = app_handle.emit("file-changed-on-disk", ());
                        return; // un evento por batch es suficiente
                    }
                }
            }
        },
    );

    match result {
        Ok(mut debouncer) => {
            // Observar el archivo directamente (NonRecursive sobre su ruta).
            if let Err(e) = debouncer.watch(&path, RecursiveMode::NonRecursive) {
                eprintln!("watcher: no se pudo observar '{}': {e}", path.display());
                // Degradar con gracia: sin watcher pero sin crash.
                let fw = app.state::<FileWatcher>();
                suppress_watcher(&fw);
                return;
            }
            let fw = app.state::<FileWatcher>();
            *fw.0.lock().unwrap() = Some(debouncer);
        }
        Err(e) => {
            eprintln!("watcher: no se pudo crear el debouncer: {e}");
            // Degradar con gracia.
        }
    }
}

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
    let bytes =
        std::fs::read(&path).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;
    let content = apostrophe_core::encoding::decode_bytes(&bytes).text;
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
    let result = load_path_into_store(path, &doc).map(Some);
    // M8.4b: armar el watcher sobre el archivo recien abierto.
    if result.is_ok() {
        rewatch_doc(&app, &doc);
    }
    result
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
    let result = load_path_into_store(PathBuf::from(path), &doc).map(Some);
    // M8.4b: armar el watcher sobre el archivo recien abierto por ruta.
    if result.is_ok() {
        rewatch_doc(&app, &doc);
    }
    result
}

/// M8.1: confirma el cierre de la ventana si hay cambios sin guardar. La invoca el
/// handler `onCloseRequested` del frontend. Sincroniza PRIMERO el texto actual del
/// editor: el debounce de `set_content` puede estar hasta 250ms atrasado, y sin esto
/// teclear-y-cerrar de inmediato perderia esos ultimos tecleos sin avisar. Devuelve
/// `true` si se puede cerrar (no habia cambios o el usuario eligio descartar).
///
/// M8.4a: si se procede con el cierre (limpio o descarte), borra el snapshot de sesion
/// para que un arranque posterior no lo restaure espuriamente.
#[tauri::command]
async fn confirm_close(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<bool, String> {
    doc.lock().unwrap().set_content(text);
    let proceed = confirm_discard_if_dirty(
        &app,
        &doc,
        "¿Cerrar sin guardar?",
        "El documento tiene cambios que se perderán.",
        "Descartar y cerrar",
    )
    .await?;

    if proceed {
        // Cierre limpio o descartado: marcar la sesion como cerrando ANTES de borrar el
        // snapshot para anular cualquier write_snapshot en vuelo que cruce el .await.
        session.closing.store(true, Ordering::SeqCst);
        let id = session.id.clone();
        if let Ok(dir) = snapshots_dir(&app) {
            let _ = tauri::async_runtime::spawn_blocking(move || {
                delete_snapshot_files(&dir, &id);
            })
            .await;
        }
    }
    Ok(proceed)
}

/// M8.1: documento nuevo (Ctrl+N). Sincroniza el texto actual, confirma descarte si hay
/// cambios sin guardar y, si se procede, resetea el store a documento nuevo vacío.
/// M8.4b: tras el reset, suelta el watcher (doc nuevo = sin path = sin watcher).
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
    let status = {
        let mut store = doc.lock().unwrap();
        store.reset();
        DocStatus {
            file_name: store.file_name(),
            dirty: store.is_dirty(),
        }
    }; // guard se suelta aqui
    // M8.4b: soltar el watcher (doc nuevo sin path: no hay archivo que observar).
    suppress_watcher(&app.state::<FileWatcher>());
    Ok(Some(status))
}

/// M8.4b: re-lee el archivo abierto desde disco (usando la deteccion de encoding M8.3)
/// y devuelve el OpenedDoc para que el frontend reemplace el contenido. Tras recargar,
/// el store queda LIMPIO (load_path_into_store invoca doc.open, que limpia dirty).
/// Si no hay path (documento nuevo), devuelve Ok(None). NO re-arma el watcher porque
/// solo LEE (no cambia el path ni escribe en disco).
#[tauri::command]
async fn reload_from_disk(doc: tauri::State<'_, Doc>) -> Result<Option<OpenedDoc>, String> {
    let path = { doc.lock().unwrap().path().cloned() };
    let Some(path) = path else { return Ok(None) };
    load_path_into_store(path, &doc).map(Some)
}

/// Guarda: si el doc tiene path escribe ahi; si no, delega en "guardar como".
/// `async` por la posible delegacion al dialogo de save_document_as.
/// M8.4b: suspende el watcher antes de escribir y lo re-arma despues para evitar
/// que el write propio se detecte como cambio externo.
#[tauri::command]
async fn save_document(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<Option<DocStatus>, String> {
    let existing = doc.lock().unwrap().path().cloned();
    let Some(path) = existing else {
        return save_document_as(app, text, doc, session).await;
    };
    // M8.4b: suspender el watcher antes de escribir para no auto-disparar el evento.
    suppress_watcher(&app.state::<FileWatcher>());
    // Capturar el error del write para re-armar el watcher SIEMPRE (exito o falla)
    // antes de propagar: si el write falla y no re-armamos, el watcher queda en None
    // el resto de la sesion.
    let write_result = std::fs::write(&path, &text).map_err(|e| format!("no se pudo guardar: {e}"));
    // M8.4b: re-armar el watcher sobre el mismo path siempre, incluso si fallo el write.
    rewatch_doc(&app, &doc);
    // Ahora si: propagar el error si lo hay.
    write_result?;
    // Lock BREVE (patron M4): set_content + mark_clean bajo un bloque acotado para que
    // el MutexGuard NO cruce el .await del spawn_blocking que viene despues.
    let status = {
        let mut store = doc.lock().unwrap();
        store.set_content(text);
        store.mark_clean();
        DocStatus {
            file_name: store.file_name(),
            dirty: false,
        }
    }; // guard se suelta aqui
    // El .md ya esta a salvo: borrar el snapshot de sesion sincrónicamente para no
    // restaurar contenido ya guardado si hay un crash en los proximos segundos (best-effort).
    if let Ok(dir) = snapshots_dir(&app) {
        let id = session.id.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            delete_snapshot_files(&dir, &id);
        })
        .await;
    }
    Ok(Some(status))
}

/// Guardar como: siempre dialogo nativo (en spawn_blocking). Fija el path y marca limpio.
/// M8.4b: suspende el watcher antes de escribir y lo re-arma sobre la NUEVA ruta despues.
#[tauri::command]
async fn save_document_as(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
    session: tauri::State<'_, SnapshotSession>,
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
    // M8.4b: suspender el watcher antes de escribir (la ruta aun puede ser la antigua).
    suppress_watcher(&app.state::<FileWatcher>());
    // Capturar el error del write para re-armar el watcher SIEMPRE (exito o falla)
    // antes de propagar: si el write falla y no re-armamos, el watcher queda en None
    // el resto de la sesion.
    let write_result = std::fs::write(&path, &text).map_err(|e| format!("no se pudo guardar: {e}"));
    if write_result.is_err() {
        // Camino de error: re-armar sobre la ruta anterior (saved_as no se llamo, el
        // store aun tiene el path anterior).
        rewatch_doc(&app, &doc);
        return write_result.map(|_| None);
    }
    // Lock BREVE (patron M4): saved_as + set_content bajo un bloque acotado para que
    // el MutexGuard NO cruce el .await del spawn_blocking que viene despues.
    let status = {
        let mut store = doc.lock().unwrap();
        store.set_content(text);
        store.saved_as(path);
        DocStatus {
            file_name: store.file_name(),
            dirty: false,
        }
    }; // guard se suelta aqui
    // El .md ya esta a salvo: borrar el snapshot de sesion sincrónicamente (best-effort).
    if let Ok(dir) = snapshots_dir(&app) {
        let id = session.id.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            delete_snapshot_files(&dir, &id);
        })
        .await;
    }
    // M8.4b: re-armar el watcher sobre el NUEVO path (saved_as ya lo fijo en el store).
    rewatch_doc(&app, &doc);
    Ok(Some(status))
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

// --- M8.4a: autosave por snapshots ---
// El snapshot se escribe en app_local_data_dir()/snapshots/ (no roaming). Un snapshot
// por sesion (el session id es un uuid v4 que se genera en run() y se mantiene toda la
// sesion). Si hay un snapshot huerfano al arrancar (crash), se restaura en el frontend.
// El .md real del usuario NUNCA se toca: el snapshot es un respaldo paralelo.

/// Id unico de esta sesion (uuid v4): nombra el snapshot de autosave. Se genera una
/// vez en run() y se reusa todo el tiempo (un snapshot por sesion, se sobreescribe).
/// `closing` es seteado por confirm_close justo antes de borrar el snapshot, para
/// anular cualquier write_snapshot en vuelo que ya haya pasado el guard del frontend.
struct SnapshotSession {
    id: String,
    closing: AtomicBool,
}

/// Snapshot huerfano de una sesion anterior (crash) detectado al arrancar, para que el
/// frontend lo restaure. Espejo del patron PendingFile.
struct PendingRestore(Mutex<Option<RestoredSnapshot>>);

/// Contenido y metadatos de un snapshot huerfano listo para restaurar en el editor.
#[derive(serde::Serialize, Clone)]
struct RestoredSnapshot {
    content: String,
    /// Nombre del archivo original si lo habia (para el aviso), o None.
    original_file_name: Option<String>,
}

/// Devuelve (y crea si no existe) el directorio de snapshots bajo app_local_data_dir.
fn snapshots_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("no se pudo resolver app_local_data_dir: {e}"))?
        .join("snapshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("no se pudo crear el dir de snapshots: {e}"))?;
    Ok(dir)
}

/// Escribe el archivo de contenido (.snapshot) y el de metadata (.meta.json) para un id.
fn write_snapshot_files(
    dir: &Path,
    id: &str,
    content: &str,
    original_path: Option<PathBuf>,
) -> Result<(), String> {
    let snap_path = dir.join(apostrophe_core::snapshot::snapshot_file_name(id));
    std::fs::write(&snap_path, content.as_bytes())
        .map_err(|e| format!("no se pudo escribir el snapshot: {e}"))?;

    let saved_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let meta = apostrophe_core::snapshot::SnapshotMeta { original_path, saved_at };
    let meta_json =
        serde_json::to_string(&meta).map_err(|e| format!("no se pudo serializar metadata: {e}"))?;
    let meta_path = dir.join(apostrophe_core::snapshot::meta_file_name(id));
    std::fs::write(&meta_path, meta_json.as_bytes())
        .map_err(|e| format!("no se pudo escribir la metadata del snapshot: {e}"))?;
    Ok(())
}

/// Borra el snapshot y la metadata de un id (best-effort: ignora si no existen).
fn delete_snapshot_files(dir: &Path, id: &str) {
    let _ = std::fs::remove_file(dir.join(apostrophe_core::snapshot::snapshot_file_name(id)));
    let _ = std::fs::remove_file(dir.join(apostrophe_core::snapshot::meta_file_name(id)));
}

/// Busca el snapshot mas reciente en el dir (huerfano de una sesion anterior). Devuelve
/// el contenido y, opcionalmente, el nombre del archivo original. Tolerante a archivos
/// corruptos: los saltea sin propagarlos.
fn find_latest_orphan(dir: &Path) -> Option<(String, RestoredSnapshot)> {
    let entries = std::fs::read_dir(dir).ok()?;

    let mut best: Option<(u64, String, apostrophe_core::snapshot::SnapshotMeta)> = None;

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = apostrophe_core::snapshot::parse_snapshot_id(&file_name) else {
            continue; // no es un .snapshot -> saltar
        };
        let meta_path = dir.join(apostrophe_core::snapshot::meta_file_name(id));
        let Ok(meta_bytes) = std::fs::read(&meta_path) else { continue };
        let Ok(meta) = serde_json::from_slice::<apostrophe_core::snapshot::SnapshotMeta>(&meta_bytes)
        else {
            continue; // metadata corrupta -> saltar
        };
        if best.as_ref().is_none_or(|(t, _, _)| meta.saved_at > *t) {
            best = Some((meta.saved_at, id.to_owned(), meta));
        }
    }

    let (_, id, meta) = best?;
    let snap_path = dir.join(apostrophe_core::snapshot::snapshot_file_name(&id));
    let content = std::fs::read_to_string(&snap_path).ok()?;

    let original_file_name = meta
        .original_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());

    Some((id, RestoredSnapshot { content, original_file_name }))
}

/// Escribe (o borra) el snapshot de la sesion segun la politica de should_snapshot.
/// Lo invoca el frontend en cada tick del timer de autosave.
#[tauri::command]
async fn write_snapshot(
    app: tauri::AppHandle,
    text: String,
    doc: tauri::State<'_, Doc>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<(), String> {
    // Guard de cierre: si confirm_close ya seto la bandera, no re-escribir el snapshot
    // que fue (o sera) borrado por confirm_close (anula writes en vuelo).
    if session.closing.load(Ordering::SeqCst) {
        return Ok(());
    }
    // Lock BREVE: leer dirty + path, soltar ANTES de tocar disco (no cruzar el .await).
    let (dirty, path) = {
        let s = doc.lock().unwrap();
        (s.is_dirty(), s.path().cloned())
    };
    let id = session.id.clone();
    let dir = snapshots_dir(&app)?;

    if apostrophe_core::snapshot::should_snapshot(dirty, text.len()) {
        tauri::async_runtime::spawn_blocking(move || write_snapshot_files(&dir, &id, &text, path))
            .await
            .map_err(|e| format!("error de autosave: {e}"))?
    } else {
        // Doc limpio o vacio: el .md real esta a salvo -> borrar el snapshot de sesion.
        let _ = tauri::async_runtime::spawn_blocking(move || {
            delete_snapshot_files(&dir, &id);
        })
        .await;
        Ok(())
    }
}

/// Devuelve (y limpia) el snapshot huerfano pendiente de restaurar, si lo hay.
/// Espejo de take_pending_file.
#[tauri::command]
fn take_pending_restore(pending: tauri::State<'_, PendingRestore>) -> Option<RestoredSnapshot> {
    pending.0.lock().unwrap().take()
}

/// Borra TODOS los archivos .snapshot y .meta.json del directorio (best-effort).
/// Se llama al arrancar, despues de cargar el huerfano en memoria, para evitar que
/// crashes multiples acumulen archivos indefinidamente.
fn purge_snapshots_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".snapshot") || name.ends_with(".meta.json") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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
        // M8.4b: watcher de cambios externos (None hasta que se abre un archivo).
        .manage(FileWatcher(Mutex::new(None)))
        // M8.4a: session id generado en run() para que este disponible en setup() sin mover.
        .manage(SnapshotSession {
            id: uuid::Uuid::new_v4().to_string(),
            closing: AtomicBool::new(false),
        })
        .manage(PendingRestore(Mutex::new(None)))
        .setup(|app| {
            load_dictionary(app.handle()); // carga el diccionario es en background

            // M8.4a: restauracion de crash. Detectar snapshot huerfano de sesion anterior
            // ANTES de procesar argv (el pending file tiene precedencia).
            if let Ok(dir) = snapshots_dir(app.handle()) {
                if let Some((_orphan_id, restored)) = find_latest_orphan(&dir) {
                    *app.state::<PendingRestore>().0.lock().unwrap() = Some(restored);
                }
                // Barrer TODOS los snapshots y metadatos del arranque anterior (incluyendo
                // el huerfano que acabamos de cargar en memoria, huerfanos de crashes
                // anteriores y archivos con meta corrupta). La app esta arrancando: es
                // seguro borrar todo; la sesion nueva generara su propio snapshot.
                purge_snapshots_dir(&dir);
            }

            // Capa B: arranque en frio via asociacion .md (app cerrada). Windows pasa el
            // path en argv; lo resolvemos a un OpenedDoc y lo dejamos pendiente para que
            // el frontend lo cargue al iniciar. Si falla (ruta invalida), se ignora y la
            // app abre vacia. El pending file tiene PRECEDENCIA sobre el restore.
            let argv: Vec<String> = std::env::args().collect();
            if let Some(path) = file_arg(&argv) {
                let doc = app.state::<Doc>();
                match load_path_into_store(PathBuf::from(&path), &doc) {
                    Ok(opened) => {
                        *app.state::<PendingFile>().0.lock().unwrap() = Some(opened);
                        // M8.4b: armar el watcher sobre el archivo del arranque en frio.
                        rewatch_doc(app.handle(), &doc);
                    }
                    Err(e) => eprintln!("no se pudo abrir el archivo de inicio '{path}': {e}"),
                }
            }

            // M8.4a: timer de autosave. Emite "autosave-tick" cada 5 s para que el frontend
            // invoque write_snapshot. Usa tauri::async_runtime::spawn (runtime de Tauri/tokio)
            // y tokio::time::sleep para no bloquear ningun hilo.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let _ = handle.emit("autosave-tick", ());
                }
            });

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
            parse_markup,
            take_pending_file,
            write_snapshot,
            take_pending_restore,
            reload_from_disk
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
