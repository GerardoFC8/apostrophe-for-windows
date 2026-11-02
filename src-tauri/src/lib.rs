// Adaptador primario (IPC) de la arquitectura hexagonal (ADR-0001): cruza la
// frontera dominio<->presentacion. La logica vive en `apostrophe-core`; aca solo
// se orquestan los procesos externos (pandoc) y se serializa el resultado.
//
// M2: pandoc paso de ser el binario del SISTEMA a un SIDECAR bundleado.
// M4: abrir/guardar .md con DocumentStore (managed state) + dialogos nativos.
// M8.4a: autosave por snapshots en app_local_data_dir()/snapshots.
// M8.4b: watcher de cambios externos (notify-debouncer-full).
//
// El export, la ruta de los sidecars, el incrustado de imagenes y la carga del
// diccionario viven en `apostrophe-sidecars`, sin Tauri, para compartirlos con la CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify_debouncer_full::{
    new_debouncer,
    notify::{EventKind, RecursiveMode},
    Debouncer, RecommendedCache,
};

use apostrophe_core::recents::Recents;
use apostrophe_core::spellcheck::{Misspelling, SpellChecker, SpellbookDictionary};
use apostrophe_core::workspace::{DocId, Workspace};
use apostrophe_sidecars::dictionary::{load_es, DictionaryError};
use apostrophe_sidecars::images::{embed_html_images_in, image_data_uri};
use apostrophe_sidecars::sidecar_path;
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
///
/// Antes de devolverlo, las imagenes LOCALES del HTML se incrustan como data URI. El preview
/// es un `iframe` con `srcdoc` en un sandbox: ahi un `src="C:/fotos/a.png"` se lee como URL
/// relativa y la imagen sale rota. Incrustarla la arregla sin darle al webview acceso al
/// disco, que es el mismo criterio de `render_math` y `render_image`.
#[tauri::command]
async fn preview_markdown(
    app: tauri::AppHandle,
    docs: tauri::State<'_, Docs>,
    text: String,
    doc_id: Option<DocId>,
) -> Result<String, String> {
    // El guard del Mutex no puede cruzar el `await`: se saca el directorio y se suelta.
    let doc_dir = target_doc_dir(&docs, doc_id);
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
    let html = result?;

    // Leer y codificar cada imagen es I/O bloqueante: va fuera del hilo de eventos. Lo que
    // no se pueda incrustar (remota, ausente, muy grande) conserva su `src` original.
    tauri::async_runtime::spawn_blocking(move || {
        apostrophe_core::image::rewrite_img_srcs(&html, |src| {
            image_data_uri(src, doc_dir.as_deref()).ok()
        })
    })
    .await
    .map_err(|e| format!("error al incrustar las imagenes del preview: {e}"))
}

/// Lanza el sidecar pandoc y junta su stdout. Aislado para mantener `preview_markdown`
/// legible y garantizar el borrado del temporal aunque pandoc falle.
async fn run_pandoc_sidecar(app: &tauri::AppHandle, args: Vec<String>) -> Result<String, String> {
    run_sidecar(app, "pandoc", args).await
}

/// Lanza un sidecar y junta su stdout. Falla con el stderr del proceso, que es lo unico
/// util cuando pandoc o typst rechazan una entrada.
async fn run_sidecar(
    app: &tauri::AppHandle,
    name: &str,
    args: Vec<String>,
) -> Result<String, String> {
    let sidecar = app
        .shell()
        .sidecar(name)
        .map_err(|e| format!("no se encontro el sidecar {name}: {e}"))?;
    let (mut rx, _child) = sidecar
        .args(args)
        .spawn()
        .map_err(|e| format!("no se pudo lanzar el sidecar {name}: {e}"))?;

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
        return Err(format!("{name} devolvio error (codigo {code:?}): {stderr}"));
    }
    String::from_utf8(stdout).map_err(|e| format!("la salida de {name} no es UTF-8: {e}"))
}

/// Adaptador primario (IPC): expone el contador de stats del dominio. La logica
/// vive en `apostrophe_core::stats`; este comando solo cruza la frontera hexagonal.
/// La frontera es gruesa y con debounce (ADR-0001): se manda el texto, vuelve Stats.
#[tauri::command]
fn count_stats(text: String) -> apostrophe_core::stats::Stats {
    apostrophe_core::stats::count(&text)
}

/// M17: tabla pipe vacia de `rows` x `cols`. Puro y barato: cruza la frontera igual que
/// `count_stats` para que la logica viva en un solo lado (ADR-0001). El dominio recorta
/// las dimensiones, asi que un valor absurdo no puede generar un documento gigante.
#[tauri::command]
fn make_table(rows: usize, cols: usize) -> String {
    apostrophe_core::table::empty_table(rows, cols)
}

/// M17: realinea las barras de una tabla pipe. Devuelve `None` si el texto NO es una tabla
/// reconocible — el frontend entonces no toca nada, que es mejor que estropear el documento.
#[tauri::command]
fn align_table(text: String) -> Option<String> {
    apostrophe_core::table::align_table(&text)
}

// --- M18: ventana de vista previa desacoplada ---
// Paridad con `preview_window.py` del upstream. La ventana la crea RUST y no el frontend:
// asi la superficie de la API que el JS puede tocar no crece (crear ventanas desde el
// webview exigiria abrirle el permiso `core:webview:allow-create-webview-window`).
//
// La ventana es tonta: solo muestra el HTML que le emite la principal. Ver preview.html.

/// Etiqueta de la ventana; tiene que coincidir con la lista `windows` de la capability.
const PREVIEW_WINDOW: &str = "preview";

/// Abre la ventana de vista previa, o la cierra si ya estaba abierta. Devuelve `true` si
/// quedo ABIERTA, para que el frontend refleje el estado en el boton.
#[tauri::command]
async fn toggle_preview_window(app: tauri::AppHandle) -> Result<bool, String> {
    if let Some(existing) = app.get_webview_window(PREVIEW_WINDOW) {
        existing
            .close()
            .map_err(|e| format!("no se pudo cerrar la ventana de vista previa: {e}"))?;
        return Ok(false);
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        PREVIEW_WINDOW,
        tauri::WebviewUrl::App("preview.html".into()),
    )
    .title("Apostrophe")
    .inner_size(700.0, 800.0)
    .build()
    .map_err(|e| format!("no se pudo abrir la ventana de vista previa: {e}"))?;
    Ok(true)
}

/// `true` si la ventana de vista previa esta abierta. El frontend lo consulta al arrancar
/// para dejar el boton en el estado correcto.
#[tauri::command]
fn preview_window_open(app: tauri::AppHandle) -> bool {
    app.get_webview_window(PREVIEW_WINDOW).is_some()
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

/// M19: que hay bajo el cursor (enlace, imagen, nota al pie o matematica), para el popover
/// de vista previa inline. Mismo perfil que `parse_markup`: los regex son de backtracking y
/// recorren el documento entero, asi que va off-thread para no bloquear el hilo de eventos.
#[tauri::command]
async fn inline_target(
    text: String,
    offset: usize,
) -> Result<Option<apostrophe_core::inline::InlineTarget>, String> {
    tauri::async_runtime::spawn_blocking(move || apostrophe_core::inline::resolve_at(&text, offset))
        .await
        .map_err(|e| format!("error al resolver el objetivo inline: {e}"))
}

/// M20: renderiza una formula a SVG y la devuelve como DATA URI, lista para un `<img>`.
///
/// El SVG viaja incrustado y no como archivo a proposito: asi el webview NUNCA necesita
/// leer del disco y no hay que habilitarle el protocolo de assets de Tauri, que le abriria
/// acceso al sistema de archivos. Un `<img>` con un SVG en data URI tampoco ejecuta scripts
/// que pudieran venir dentro del SVG.
///
/// Son DOS pasos porque el math del markdown es sintaxis LaTeX y typst tiene la suya:
/// pandoc traduce (`--to typst`) y typst dibuja. Medido en ~64 ms, aceptable para un hover.
#[tauri::command]
async fn render_math(
    app: tauri::AppHandle,
    source: String,
    color: String,
) -> Result<String, String> {
    use base64::Engine;

    let seq = PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let md = dir.join(format!("apostrophe-math-{}-{}.md", std::process::id(), seq));
    let typ = dir.join(format!(
        "apostrophe-math-{}-{}.typ",
        std::process::id(),
        seq
    ));
    let svg = dir.join(format!(
        "apostrophe-math-{}-{}.svg",
        std::process::id(),
        seq
    ));

    // Paso 1: LaTeX -> sintaxis typst, con el mismo pandoc que ya produce el preview.
    std::fs::write(&md, format!("${source}$"))
        .map_err(|e| format!("no se pudo escribir el temporal de la formula: {e}"))?;
    let translated = run_pandoc_sidecar(
        &app,
        vec![
            md.to_string_lossy().into_owned(),
            "--from".into(),
            "markdown".into(),
            "--to".into(),
            "typst".into(),
        ],
    )
    .await;
    let _ = std::fs::remove_file(&md);
    let translated = translated?;

    // Paso 2: dibujar. El documento (pagina al tamano de la formula, fondo transparente,
    // color del tema) lo arma el dominio.
    let document = apostrophe_core::math::typst_document(translated.trim(), &color);
    std::fs::write(&typ, document)
        .map_err(|e| format!("no se pudo escribir el documento typst: {e}"))?;

    let engine = sidecar_path("typst")?;
    if !engine.exists() {
        return Err("Falta el motor de fórmulas. Ejecuta scripts/fetch-typst.ps1.".into());
    }
    let result = run_sidecar(
        &app,
        "typst",
        vec![
            "compile".into(),
            typ.to_string_lossy().into_owned(),
            svg.to_string_lossy().into_owned(),
            "--format".into(),
            "svg".into(),
        ],
    )
    .await;
    let _ = std::fs::remove_file(&typ);
    result?;

    let bytes = std::fs::read(&svg).map_err(|e| format!("no se pudo leer el SVG: {e}"))?;
    let _ = std::fs::remove_file(&svg);
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// M19 (imagenes): lee la imagen de un `![alt](url)` y la devuelve como DATA URI, lista
/// para el `<img>` del popover.
///
/// Mismo criterio que `render_math`: la imagen viaja INCRUSTADA y no como archivo. Asi el
/// webview nunca necesita leer del disco y no hace falta habilitarle el protocolo de assets
/// de Tauri, que le daria acceso al sistema de archivos.
///
/// Las remotas NO se descargan: posar el cursor no debe generar trafico de red (ver
/// `apostrophe_core::image`).
///
/// El error es un CODIGO estable (`remote`, `not-found`, …), no una frase: la copia visible
/// vive en el catalogo de i18n del frontend, que es quien sabe en que idioma esta la app.
#[tauri::command]
async fn render_image(
    docs: tauri::State<'_, Docs>,
    url: String,
    doc_id: Option<DocId>,
) -> Result<String, String> {
    // El guard del Mutex no puede cruzar el `await`: se saca el directorio y se suelta.
    let doc_dir = target_doc_dir(&docs, doc_id);
    tauri::async_runtime::spawn_blocking(move || image_data_uri(&url, doc_dir.as_deref()))
        .await
        .map_err(|_| "unreadable".to_string())?
}

/// Elige una imagen con el selector nativo y devuelve el fragmento `![](ruta)` listo para
/// insertar. `None` si el usuario cancela.
///
/// La ruta sale RELATIVA al documento siempre que se pueda, porque es lo que mantiene el
/// `.md` portable. Quien lo decide es el dominio (`image_markdown`); aca solo se abre el
/// dialogo y se le pasa el directorio.
///
/// `filter_label` llega desde el frontend y no se escribe aca: la copia visible vive en el
/// catalogo de i18n, asi sumar un idioma no obliga a tocar Rust.
///
/// `async` + `spawn_blocking` como todos los dialogos: sincrono correria INLINE en el main
/// thread de WebView2, donde el picker se encola en el mismo hilo que lo bloquea -> deadlock.
#[tauri::command]
async fn pick_image(
    app: tauri::AppHandle,
    docs: tauri::State<'_, Docs>,
    filter_label: String,
    doc_id: Option<DocId>,
) -> Result<Option<String>, String> {
    // El guard del Mutex no puede cruzar el `await`: se saca el directorio y se suelta.
    let doc_dir = target_doc_dir(&docs, doc_id);

    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .file()
            .add_filter(&filter_label, &apostrophe_core::image::SUPPORTED_EXTENSIONS)
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("error en el dialogo: {e}"))?;

    let Some(file) = picked else {
        return Ok(None); // el usuario cancelo el picker
    };
    let path = file
        .into_path()
        .map_err(|e| format!("ruta invalida: {e}"))?;
    Ok(Some(apostrophe_core::image::image_markdown(
        &path,
        doc_dir.as_deref(),
    )))
}

// --- M4: gestion de documento (abrir / guardar) ---
// El estado vive en el `Workspace` del dominio como managed state (Mutex): varios
// `DocumentStore` (uno por pestaña) identificados por un `DocId` estable. El I/O de disco
// se hace aquí con std::fs; los pickers nativos con tauri-plugin-dialog.
//
// Compatibilidad: todo comando de documento acepta un `doc_id` OPCIONAL (`docId` en el
// IPC). Si no llega, opera sobre el documento ACTIVO, que es exactamente el documento
// unico de antes. Asi el frontend de un solo documento sigue funcionando sin cambios.
//
// El `DocId` se resuelve UNA vez al empezar el comando, antes de cualquier `.await`: si el
// usuario cambia de pestaña mientras hay un dialogo abierto, el comando sigue apuntando
// al documento con el que empezo.
//
// CRITICO (review M4): los comandos que abren dialogos son `async` y corren el
// dialogo en `spawn_blocking`. Si fueran sincronos correrian INLINE en el main
// thread de WebView2, y `blocking_pick_file`/`blocking_save_file` bloquean ese
// thread mientras el dialogo se encola en el MISMO thread -> DEADLOCK (la app se
// congela). En async + spawn_blocking el bloqueo ocurre en un hilo de pool y el
// main thread queda libre para procesar el dialogo. Y el lock del workspace NUNCA
// cruza un `.await`: se toma en bloques breves.

type Docs = Mutex<Workspace>;

/// Error estable cuando el `doc_id` pedido ya no existe (la pestaña se cerro). Es un
/// CODIGO, no una frase: la copia visible vive en el catalogo de i18n del frontend.
const UNKNOWN_DOCUMENT: &str = "unknown-document";

/// Error estable de "guardar como" sobre una ruta que ya tiene abierta OTRO documento.
/// Se rechaza: dos documentos con la misma ruta se pisarian al guardar y el watcher de
/// uno recargaria en silencio el contenido del otro. El frontend avisa que esta abierto
/// en otra pestaña.
const PATH_OPEN_IN_OTHER_DOCUMENT: &str = "path-open-in-other-document";

/// Comprueba, justo antes de escribir, que se puede guardar el documento `id` en `path`:
/// que el documento sigue abierto (pudo cerrarse mientras el dialogo estaba abierto) y
/// que ningun OTRO documento tiene esa ruta. Si falla, no se escribe nada.
fn check_save_target(ws: &Workspace, id: DocId, path: &Path) -> Result<(), String> {
    if !ws.contains(id) {
        return Err(UNKNOWN_DOCUMENT.to_string());
    }
    if ws.other_with_path(Some(id), path).is_some() {
        return Err(PATH_OPEN_IN_OTHER_DOCUMENT.to_string());
    }
    Ok(())
}

/// Workspace inicial: un documento nuevo vacio, el mismo punto de partida que tenia el
/// editor de un solo documento.
fn initial_workspace() -> Workspace {
    let mut ws = Workspace::new();
    ws.create_untitled();
    ws
}

/// Resuelve el documento destino de un comando: el `doc_id` pedido (error si ya no
/// existe) o el activo (creando uno si no queda ninguno).
fn target_doc(docs: &Docs, doc_id: Option<DocId>) -> Result<DocId, String> {
    docs.lock()
        .unwrap()
        .resolve_or_create(doc_id)
        .ok_or_else(|| UNKNOWN_DOCUMENT.to_string())
}

/// Carpeta del documento destino, contra la que se resuelven las rutas relativas de las
/// imagenes. `None` si es un documento sin guardar (o si ya no existe).
fn target_doc_dir(docs: &Docs, doc_id: Option<DocId>) -> Option<PathBuf> {
    let ws = docs.lock().unwrap();
    let id = ws.resolve(doc_id)?;
    ws.get(id)?
        .path()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

// --- M8.4b: watcher de cambios externos ---
// Un Debouncer POR documento con ruta, en managed state. Se re-arma al abrir/guardar un
// archivo y se suspende (drop) alrededor de nuestros propios guardados para evitar el
// auto-disparo. El handler solo traduce + emite: NUNCA toma locks del workspace ni abre
// dialogos (gotcha anti-deadlock M4).

type DocWatcher = Debouncer<notify_debouncer_full::notify::RecommendedWatcher, RecommendedCache>;

/// Watchers de los documentos abiertos, por `DocId`. Un documento sin ruta no tiene.
struct FileWatchers(Mutex<HashMap<DocId, DocWatcher>>);

/// Payload de `file-changed-on-disk`: a que documento le cambio el archivo
/// (`{ doc_id }`, en snake_case como `OpenedDoc` y `DocStatus`; solo los ARGUMENTOS de
/// los comandos van en camelCase). El frontend de un solo documento ignora el payload,
/// asi que agregarlo es compatible.
#[derive(Clone, serde::Serialize)]
struct DiskChange {
    doc_id: DocId,
}

/// Suspende el watcher de un documento (lo dropea -> deja de observar). Se llama ANTES de
/// un write propio para que notify no detecte nuestro guardado como cambio externo, y al
/// cerrar el documento. Soltar el Debouncer no bloquea: solo marca su hilo para que pare.
fn suppress_watcher(app: &tauri::AppHandle, id: DocId) {
    app.state::<FileWatchers>().0.lock().unwrap().remove(&id);
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

/// Arma (o re-arma) el watcher sobre el archivo del documento `id`. Sustituye el
/// Debouncer anterior de ese documento si lo habia (el drop del anterior detiene su
/// hilo). Degrada con gracia si notify falla (log + sin watcher, no crashea).
fn rewatch_doc(app: &tauri::AppHandle, docs: &Docs, id: DocId) {
    use apostrophe_core::watcher::{is_relevant_change, should_notify_frontend};

    // Leer el path bajo lock BREVE; soltarlo antes de crear el debouncer.
    let path = { docs.lock().unwrap().get(id).and_then(|d| d.path().cloned()) };
    let Some(path) = path else {
        // Documento nuevo sin path (o ya cerrado): no hay archivo que observar.
        suppress_watcher(app, id);
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
                    if is_relevant_change(&watched, event_path) && should_notify_frontend(kind) {
                        let _ = app_handle.emit("file-changed-on-disk", DiskChange { doc_id: id });
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
                suppress_watcher(app, id);
                return;
            }
            // Si el documento se cerro mientras se armaba, el watcher sobra: se suelta
            // aquí y no queda emitiendo eventos de un documento que ya no existe. El lock
            // del workspace se mantiene durante la insercion para que sea atomica frente a
            // close_document (que cierra bajo ese lock y luego suelta el watcher).
            //
            // Orden de locks: workspace -> watchers. Nadie toma el del workspace teniendo
            // el de watchers (suppress_watcher solo toma el de watchers), asi que no hay
            // deadlock.
            let ws = docs.lock().unwrap();
            if !ws.contains(id) {
                return;
            }
            app.state::<FileWatchers>()
                .0
                .lock()
                .unwrap()
                .insert(id, debouncer);
            drop(ws);
        }
        Err(e) => {
            eprintln!("watcher: no se pudo crear el debouncer: {e}");
            // Degradar con gracia.
        }
    }
}

/// Documento recien abierto que cruza al frontend. `doc_id` dice en que documento quedo:
/// si la ruta ya estaba abierta es el existente (y `content` es su contenido actual, que
/// puede tener cambios sin guardar).
#[derive(serde::Serialize, Clone)]
struct OpenedDoc {
    doc_id: DocId,
    path: String,
    file_name: String,
    content: String,
}

/// Estado para la titlebar: nombre de archivo + indicador de cambios sin guardar.
#[derive(serde::Serialize)]
struct DocStatus {
    doc_id: DocId,
    file_name: Option<String>,
    dirty: bool,
}

/// Estado del documento `id` para la titlebar, o el error estable si ya no existe.
fn doc_status(ws: &Workspace, id: DocId) -> Result<DocStatus, String> {
    let store = ws.get(id).ok_or_else(|| UNKNOWN_DOCUMENT.to_string())?;
    Ok(DocStatus {
        doc_id: id,
        file_name: store.file_name(),
        dirty: store.is_dirty(),
    })
}

/// Sincroniza el texto del editor con el store (se llama en el debounce) y devuelve
/// el estado para el indicador de "modificado". No abre dialogos -> puede ser sincrono.
#[tauri::command]
fn set_content(
    text: String,
    doc_id: Option<DocId>,
    docs: tauri::State<'_, Docs>,
) -> Result<DocStatus, String> {
    let mut ws = docs.lock().unwrap();
    let id = ws
        .resolve_or_create(doc_id)
        .ok_or_else(|| UNKNOWN_DOCUMENT.to_string())?;
    if let Some(store) = ws.get_mut(id) {
        store.set_content(text);
    }
    doc_status(&ws, id)
}

/// Sincroniza `text` (si llega) en el documento `id`. Lo usan los comandos que deciden
/// sobre el dirty real: el debounce de `set_content` puede ir hasta 250ms atrasado.
fn sync_text(docs: &Docs, id: DocId, text: Option<String>) {
    if let Some(text) = text {
        if let Some(store) = docs.lock().unwrap().get_mut(id) {
            store.set_content(text);
        }
    }
}

/// Muestra el dialogo de descarte (nativo, en spawn_blocking -> sin deadlock) y devuelve
/// `true` si el usuario eligio descartar.
///
/// Patron GNOME: `title` = pregunta corta (encabezado), `message` = explicacion. NO
/// repetir texto entre ambos: rfd usa el title como encabezado Y como titulo de ventana
/// (en GTK lo muestra dos veces), asi que repetir la frase la duplica visualmente.
async fn confirm_discard(app: &tauri::AppHandle, labels: ConfirmLabels) -> Result<bool, String> {
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .message(labels.message)
            .title(labels.title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                labels.confirm,
                labels.cancel,
            ))
            .blocking_show()
    })
    .await
    .map_err(|e| format!("error en el dialogo: {e}"))
}

/// Guard anti-perdida: si el documento `id` tiene cambios sin guardar, pide
/// confirmacion. Devuelve `true` si se puede proceder (no habia cambios, el documento ya
/// no existe, o el usuario eligio descartar). Reusado por open_document, open_path,
/// new_document y close_document con su propio titulo, mensaje y label.
async fn confirm_discard_if_dirty(
    app: &tauri::AppHandle,
    docs: &Docs,
    id: DocId,
    labels: ConfirmLabels,
) -> Result<bool, String> {
    // El guard del Mutex se suelta al terminar el statement: NO se cruza el .await.
    let dirty = docs.lock().unwrap().get(id).is_some_and(|d| d.is_dirty());
    if !dirty {
        return Ok(true);
    }
    confirm_discard(app, labels).await
}

/// Ruta ABSOLUTA del documento. Abrirlo desde una terminal (`apostrophe.exe nota.md`)
/// trae una relativa, cuya carpeta es `""`: no serviria para resolver sus imagenes ni como
/// directorio actual de pandoc al exportar. Se resuelve contra el directorio actual, sin
/// tocar el disco (`std::path::absolute` no sigue enlaces ni exige que exista). Si no se
/// puede, se conserva la original: es mejor un documento sin imagenes que no abrirlo.
fn absolute_doc_path(path: PathBuf) -> PathBuf {
    std::path::absolute(&path).unwrap_or(path)
}

/// OpenedDoc con el estado ACTUAL del documento `id` en el workspace.
fn opened_doc(ws: &Workspace, id: DocId) -> Result<OpenedDoc, String> {
    let store = ws.get(id).ok_or_else(|| UNKNOWN_DOCUMENT.to_string())?;
    let path = store.path().cloned().unwrap_or_default();
    Ok(OpenedDoc {
        doc_id: id,
        path: path.to_string_lossy().into_owned(),
        file_name: store.file_name().unwrap_or_default(),
        content: store.content().to_owned(),
    })
}

/// Lee un .md de disco y lo fija en el workspace. Unica fuente del flujo "leer archivo ->
/// estado del documento", reusada por el dialogo (open_document), el arranque en frio
/// (setup), la apertura via asociacion (open_path) y la recarga (reload_from_disk).
///
/// `into = Some(id)` carga el archivo DENTRO de ese documento (el flujo de un solo
/// documento: abrir reemplaza al actual); `None` lo abre como documento nuevo. En ambos
/// casos, si la ruta ya esta abierta en OTRO documento, se activa ese sin releer el disco
/// ni pisar sus cambios.
///
/// Devuelve el documento y si se cargo de disco (`true`): en ese caso hay que (re)armar
/// su watcher.
fn load_path(
    app: &tauri::AppHandle,
    path: PathBuf,
    docs: &Docs,
    into: Option<DocId>,
) -> Result<(OpenedDoc, bool), String> {
    let path = absolute_doc_path(path);
    {
        let mut ws = docs.lock().unwrap();
        if let Some(existing) = ws.find_by_path(&path).filter(|&id| Some(id) != into) {
            ws.activate(existing);
            return Ok((opened_doc(&ws, existing)?, false));
        }
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;
    let content = apostrophe_core::encoding::decode_bytes(&bytes).text;
    // M16: este es el UNICO punto por el que pasa "archivo -> estado del documento"
    // (dialogo, asociacion .md, arranque en frio y recarga), asi que registrar el reciente
    // aquí cubre todas las vias de apertura sin repetir la llamada en cada camino.
    remember_recent(app, &path);
    let mut ws = docs.lock().unwrap();
    // Si el destino se cerro mientras se leia el disco, se abre como documento nuevo.
    let opened = into
        .and_then(|target| ws.open_path_into(target, path.clone(), content.clone()))
        .unwrap_or_else(|| ws.open_path(path, content));
    Ok((opened_doc(&ws, opened.id)?, !opened.already_open))
}

// --- M16: documentos recientes ---
// La lista vive en app_local_data_dir/recents.json (local, NO roaming: son rutas de ESTA
// maquina, no tiene sentido sincronizarlas). La logica de orden, deduplicacion y recorte
// es del dominio (`apostrophe_core::recents`); aca solo esta el I/O.
//
// Los recientes NO son criticos: si el archivo falta, esta corrupto o el disco falla, se
// sigue con una lista vacia y el editor funciona igual. Ningun camino propaga el error.

/// Ruta del JSON de recientes, creando el directorio si hace falta.
fn recents_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("no se pudo resolver app_local_data_dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("no se pudo crear el dir de datos: {e}"))?;
    Ok(dir.join("recents.json"))
}

/// Lee la lista y descarta de una las rutas que ya no existen: un menu que ofrece abrir
/// archivos borrados es peor que no tener menu.
fn load_recents(app: &tauri::AppHandle) -> Recents {
    let Ok(path) = recents_file(app) else {
        return Recents::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Recents::default(); // todavia no hay archivo: primera ejecucion
    };
    let mut recents: Recents = serde_json::from_str(&raw).unwrap_or_default();
    recents.retain_existing(|p| p.exists());
    recents
}

fn save_recents(app: &tauri::AppHandle, recents: &Recents) {
    if let (Ok(path), Ok(json)) = (recents_file(app), serde_json::to_string(recents)) {
        let _ = std::fs::write(path, json); // best-effort
    }
}

/// Registra un documento como el mas reciente. Se llama desde el unico punto por el que
/// pasa "archivo -> estado del documento" y desde guardar-como (que estrena ruta).
fn remember_recent(app: &tauri::AppHandle, path: &Path) {
    let mut recents = load_recents(app);
    recents.push(path.to_path_buf());
    save_recents(app, &recents);
}

/// Entrada del menu de recientes: la ruta completa para abrir, el nombre para mostrar.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecentEntry {
    pub path: String,
    pub name: String,
}

/// Lista de recientes ya filtrada (sin archivos borrados), lista para pintar en el menu.
#[tauri::command]
fn recent_files(app: tauri::AppHandle) -> Vec<RecentEntry> {
    load_recents(&app)
        .paths()
        .iter()
        .map(|p| RecentEntry {
            path: p.to_string_lossy().into_owned(),
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned()),
        })
        .collect()
}

/// Textos del dialogo de confirmacion. Los arma el FRONTEND y viajan por IPC: toda la
/// copia visible vive en el catalogo de `src/i18n.ts` (ADR-0001 — el frontend presenta,
/// el adaptador solo muestra lo que recibe). Asi sumar un idioma no toca Rust.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConfirmLabels {
    pub title: String,
    pub message: String,
    pub confirm: String,
    pub cancel: String,
}

/// Abre un .md via DIALOGO nativo. `async` + dialogos en spawn_blocking.
///
/// Dos modos:
/// - por defecto (frontend de un solo documento): el archivo REEMPLAZA al documento
///   destino (`doc_id` o el activo), asi que si tiene cambios sin guardar se CONFIRMA
///   antes (anti-perdida, review M4).
/// - `new_tab = true`: el archivo se abre como documento nuevo (o se activa si ya estaba
///   abierto). No se pierde nada, asi que no hay confirmacion.
#[tauri::command]
async fn open_document(
    app: tauri::AppHandle,
    labels: ConfirmLabels,
    doc_id: Option<DocId>,
    new_tab: Option<bool>,
    docs: tauri::State<'_, Docs>,
) -> Result<Option<OpenedDoc>, String> {
    let into = if new_tab.unwrap_or(false) {
        None
    } else {
        Some(target_doc(&docs, doc_id)?)
    };
    if let Some(target) = into {
        if !confirm_discard_if_dirty(&app, &docs, target, labels).await? {
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
    let (opened, fresh) = load_path(&app, path, &docs, into)?;
    // M8.4b: armar el watcher sobre el archivo recien abierto.
    if fresh {
        rewatch_doc(&app, &docs, opened.doc_id);
    }
    Ok(Some(opened))
}

/// Capa B: abre un .md por RUTA (no por dialogo). Lo usa la apertura via asociacion
/// de archivo cuando la app YA esta abierta (single-instance reenvia el path y el
/// frontend invoca este comando) y el menu de recientes. Mismos modos que open_document.
/// Devuelve None si el usuario cancela.
#[tauri::command]
async fn open_path(
    app: tauri::AppHandle,
    path: String,
    labels: ConfirmLabels,
    doc_id: Option<DocId>,
    new_tab: Option<bool>,
    docs: tauri::State<'_, Docs>,
) -> Result<Option<OpenedDoc>, String> {
    let path = absolute_doc_path(PathBuf::from(path));
    let into = if new_tab.unwrap_or(false) {
        None
    } else {
        Some(target_doc(&docs, doc_id)?)
    };
    // Si la ruta ya esta abierta en OTRO documento basta con activarlo: el destino no se
    // toca, asi que no hay nada que confirmar.
    let already_elsewhere = {
        let ws = docs.lock().unwrap();
        ws.find_by_path(&path).is_some_and(|id| Some(id) != into)
    };
    if let (Some(target), false) = (into, already_elsewhere) {
        if !confirm_discard_if_dirty(&app, &docs, target, labels).await? {
            return Ok(None);
        }
    }
    let (opened, fresh) = load_path(&app, path, &docs, into)?;
    // M8.4b: armar el watcher sobre el archivo recien abierto por ruta.
    if fresh {
        rewatch_doc(&app, &docs, opened.doc_id);
    }
    Ok(Some(opened))
}

/// M8.1: confirma el cierre de la ventana si hay cambios sin guardar en CUALQUIER
/// documento. La invoca el handler `onCloseRequested` del frontend. Sincroniza PRIMERO
/// `text` en el documento destino (`doc_id` o el activo): el debounce de `set_content`
/// puede estar hasta 250ms atrasado, y sin esto teclear-y-cerrar de inmediato perderia
/// esos ultimos tecleos sin avisar.
///
/// Con varios documentos modificados se muestra UNA sola confirmacion para todos. La copia
/// (incluido el numero de documentos, si se quiere mostrar) la arma el frontend: puede
/// consultarlo antes con `dirty_documents`. Devuelve `true` si se puede cerrar.
///
/// M8.4a: si se procede con el cierre (limpio o descarte), borra los snapshots de la
/// sesion para que un arranque posterior no los restaure espuriamente.
#[tauri::command]
async fn confirm_close(
    app: tauri::AppHandle,
    text: Option<String>,
    labels: ConfirmLabels,
    doc_id: Option<DocId>,
    docs: tauri::State<'_, Docs>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<bool, String> {
    let target = docs.lock().unwrap().resolve(doc_id);
    if let Some(id) = target {
        sync_text(&docs, id, text);
    }
    let any_dirty = !docs.lock().unwrap().dirty_ids().is_empty();
    let proceed = if any_dirty {
        confirm_discard(&app, labels).await?
    } else {
        true
    };

    if proceed {
        // Cierre limpio o descartado: marcar la sesion como cerrando ANTES de borrar los
        // snapshots para anular cualquier write_snapshot en vuelo que cruce el .await.
        session.closing.store(true, Ordering::SeqCst);
        delete_session_snapshots(&app, &session).await;
    }
    Ok(proceed)
}

/// M8.1: documento nuevo (Ctrl+N). Sincroniza `text` (si llega) en el documento destino
/// (`doc_id` o el activo) y:
/// - por defecto: confirma descarte si hay cambios sin guardar y, si se procede, resetea
///   ese documento a uno nuevo vacio (M8.4b: y suelta su watcher, sin path no hay archivo
///   que observar).
/// - `new_tab = true`: crea un documento nuevo aparte y lo activa, sin confirmar nada.
///
/// Devuelve el estado del documento nuevo para la titlebar, o `None` si el usuario cancelo.
#[tauri::command]
async fn new_document(
    app: tauri::AppHandle,
    text: Option<String>,
    labels: ConfirmLabels,
    doc_id: Option<DocId>,
    new_tab: Option<bool>,
    docs: tauri::State<'_, Docs>,
) -> Result<Option<DocStatus>, String> {
    if new_tab.unwrap_or(false) {
        // Sin crear nada de mas: si no queda ningun documento, el unico nuevo es este.
        let current = docs.lock().unwrap().resolve(doc_id);
        if let Some(current) = current {
            sync_text(&docs, current, text);
        }
        let mut ws = docs.lock().unwrap();
        let id = ws.create_untitled();
        return doc_status(&ws, id).map(Some);
    }

    let target = target_doc(&docs, doc_id)?;
    sync_text(&docs, target, text);

    if !confirm_discard_if_dirty(&app, &docs, target, labels).await? {
        return Ok(None);
    }
    let status = {
        let mut ws = docs.lock().unwrap();
        let store = ws
            .get_mut(target)
            .ok_or_else(|| UNKNOWN_DOCUMENT.to_string())?;
        store.reset();
        ws.activate(target);
        doc_status(&ws, target)?
    }; // guard se suelta aqui

    // M8.4b: soltar el watcher (doc nuevo sin path: no hay archivo que observar).
    suppress_watcher(&app, target);
    Ok(Some(status))
}

/// M8.4b: re-lee desde disco el archivo del documento destino (`doc_id` o el activo),
/// con la deteccion de encoding M8.3, y devuelve el OpenedDoc para que el frontend
/// reemplace el contenido. Tras recargar, el documento queda LIMPIO. Si no tiene path
/// (documento nuevo), devuelve Ok(None). NO re-arma el watcher porque solo LEE (no cambia
/// el path ni escribe en disco).
#[tauri::command]
async fn reload_from_disk(
    app: tauri::AppHandle,
    doc_id: Option<DocId>,
    docs: tauri::State<'_, Docs>,
) -> Result<Option<OpenedDoc>, String> {
    let target = target_doc(&docs, doc_id)?;
    let path = {
        docs.lock()
            .unwrap()
            .get(target)
            .and_then(|d| d.path().cloned())
    };
    let Some(path) = path else { return Ok(None) };
    load_path(&app, path, &docs, Some(target)).map(|(opened, _)| Some(opened))
}

/// Guarda: si el doc tiene path escribe ahi; si no, delega en "guardar como".
/// `async` por la posible delegacion al dialogo de save_document_as.
/// M8.4b: suspende el watcher antes de escribir y lo re-arma despues para evitar
/// que el write propio se detecte como cambio externo.
#[tauri::command]
async fn save_document(
    app: tauri::AppHandle,
    text: String,
    doc_id: Option<DocId>,
    docs: tauri::State<'_, Docs>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<Option<DocStatus>, String> {
    let target = target_doc(&docs, doc_id)?;
    let existing = {
        docs.lock()
            .unwrap()
            .get(target)
            .and_then(|d| d.path().cloned())
    };
    let Some(path) = existing else {
        return save_document_as(app, text, Some(target), docs, session).await;
    };
    check_save_target(&docs.lock().unwrap(), target, &path)?;
    // M8.4b: suspender el watcher antes de escribir para no auto-disparar el evento.
    suppress_watcher(&app, target);
    // Capturar el error del write para re-armar el watcher SIEMPRE (exito o falla)
    // antes de propagar: si el write falla y no re-armamos, el watcher queda en None
    // el resto de la sesion.
    let write_result = std::fs::write(&path, &text).map_err(|e| format!("no se pudo guardar: {e}"));
    // M8.4b: re-armar el watcher sobre el mismo path siempre, incluso si fallo el write.
    rewatch_doc(&app, &docs, target);
    // Ahora si: propagar el error si lo hay.
    write_result?;
    // Lock BREVE (patron M4): set_content + mark_clean bajo un bloque acotado para que
    // el MutexGuard NO cruce el .await del spawn_blocking que viene despues.
    let status = {
        let mut ws = docs.lock().unwrap();
        if let Some(store) = ws.get_mut(target) {
            store.set_content(text);
            store.mark_clean();
        }
        doc_status(&ws, target)?
    }; // guard se suelta aqui

    // El .md ya esta a salvo: borrar su snapshot para no restaurar contenido ya guardado
    // si hay un crash en los proximos segundos (best-effort).
    delete_doc_snapshot(&app, &session, target).await;
    Ok(Some(status))
}

/// Guardar como: siempre dialogo nativo (en spawn_blocking). Fija el path y marca limpio.
/// M8.4b: suspende el watcher antes de escribir y lo re-arma sobre la NUEVA ruta despues.
#[tauri::command]
async fn save_document_as(
    app: tauri::AppHandle,
    text: String,
    doc_id: Option<DocId>,
    docs: tauri::State<'_, Docs>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<Option<DocStatus>, String> {
    let target = target_doc(&docs, doc_id)?;
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
    let path = absolute_doc_path(
        file_path
            .into_path()
            .map_err(|e| format!("ruta invalida: {e}"))?,
    );
    // Antes de escribir y de tocar recientes: si el documento se cerro durante el dialogo
    // o la ruta la tiene otra pestaña, no se escribe nada.
    check_save_target(&docs.lock().unwrap(), target, &path)?;
    // M8.4b: suspender el watcher antes de escribir (la ruta aun puede ser la antigua).
    suppress_watcher(&app, target);
    // Capturar el error del write para re-armar el watcher SIEMPRE (exito o falla)
    // antes de propagar: si el write falla y no re-armamos, el watcher queda en None
    // el resto de la sesion.
    let write_result = std::fs::write(&path, &text).map_err(|e| format!("no se pudo guardar: {e}"));
    if write_result.is_err() {
        // Camino de error: re-armar sobre la ruta anterior (saved_as no se llamo, el
        // documento aun tiene el path anterior).
        rewatch_doc(&app, &docs, target);
        return write_result.map(|_| None);
    }
    // Lock BREVE (patron M4): saved_as + set_content bajo un bloque acotado para que
    // el MutexGuard NO cruce el .await del spawn_blocking que viene despues.
    // M16: guardar-como estrena ruta, asi que el documento entra a recientes por ahi.
    // (Guardar sobre la ruta existente no hace falta: ya entro al abrirse.)
    remember_recent(&app, &path);
    let status = {
        let mut ws = docs.lock().unwrap();
        let store = ws
            .get_mut(target)
            .ok_or_else(|| UNKNOWN_DOCUMENT.to_string())?;
        store.set_content(text);
        store.saved_as(path);
        doc_status(&ws, target)?
    }; // guard se suelta aqui

    // El .md ya esta a salvo: borrar su snapshot (best-effort).
    delete_doc_snapshot(&app, &session, target).await;
    // M8.4b: re-armar el watcher sobre el NUEVO path (saved_as ya lo fijo en el store).
    rewatch_doc(&app, &docs, target);
    Ok(Some(status))
}

// --- Pestañas: comandos que solo usa el frontend con varios documentos ---

/// Activa un documento (la pestaña que el usuario eligio). Los comandos sin `doc_id`
/// apuntan al activo. `false` si el documento ya no existe.
#[tauri::command]
fn activate_document(doc_id: DocId, docs: tauri::State<'_, Docs>) -> bool {
    docs.lock().unwrap().activate(doc_id)
}

/// Cierra un documento (una pestaña). Sincroniza antes `text` si llega y, si hay cambios
/// sin guardar, confirma con `labels`. Al cerrar suelta su watcher y borra su snapshot.
/// Devuelve `true` si quedo cerrado (tambien si ya no existia) y `false` si el usuario
/// cancelo. Si era el activo, el activo pasa a la pestaña vecina; si era el ultimo, el
/// workspace queda vacio hasta que se abra o cree otro.
#[tauri::command]
async fn close_document(
    app: tauri::AppHandle,
    doc_id: DocId,
    labels: ConfirmLabels,
    text: Option<String>,
    docs: tauri::State<'_, Docs>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<bool, String> {
    sync_text(&docs, doc_id, text);
    if !confirm_discard_if_dirty(&app, &docs, doc_id, labels).await? {
        return Ok(false);
    }
    docs.lock().unwrap().close(doc_id);
    suppress_watcher(&app, doc_id);
    delete_doc_snapshot(&app, &session, doc_id).await;
    Ok(true)
}

/// Fila de `list_documents`. `name` es `None` en un documento sin guardar: el texto
/// "Sin titulo" lo pone el frontend desde su catalogo de i18n.
#[derive(serde::Serialize)]
struct DocumentInfo {
    doc_id: DocId,
    path: Option<String>,
    name: Option<String>,
    dirty: bool,
}

/// Documentos abiertos, en orden de apertura.
#[tauri::command]
fn list_documents(docs: tauri::State<'_, Docs>) -> Vec<DocumentInfo> {
    docs.lock()
        .unwrap()
        .iter()
        .map(|(id, d)| DocumentInfo {
            doc_id: id,
            path: d.path().map(|p| p.to_string_lossy().into_owned()),
            name: d.file_name(),
            dirty: d.is_dirty(),
        })
        .collect()
}

/// Ids de los documentos con cambios sin guardar, en orden de apertura. Sirve al
/// frontend para armar la copia del dialogo de cierre ("3 documentos sin guardar").
#[tauri::command]
fn dirty_documents(docs: tauri::State<'_, Docs>) -> Vec<DocId> {
    docs.lock().unwrap().dirty_ids()
}

/// Exporta el markdown al formato que el usuario elija en el dialogo nativo (M12).
/// Reusa los sidecars via el Converter port (ver `apostrophe_sidecars::export`). Devuelve
/// la ruta de salida, o `None` si se cancela. `async` + spawn_blocking (gotcha deadlock M4).
///
/// El dialogo ofrece un filtro por formato ("Guardar como tipo") pero devuelve SOLO la
/// ruta, sin decir cual quedo seleccionado: por eso el formato se deduce de la extension
/// del archivo resultante. En Windows el dialogo agrega la extension del filtro elegido,
/// asi que elegir "Documento de Word" produce `.docx` y de ahi sale el formato.
#[tauri::command]
async fn export_document(
    app: tauri::AppHandle,
    docs: tauri::State<'_, Docs>,
    text: String,
    css: String,
    accent: String,
    doc_id: Option<DocId>,
) -> Result<Option<String>, String> {
    use apostrophe_core::convert::ExportFormat;

    // El guard del Mutex no puede cruzar el `await`: se saca el directorio y se suelta.
    // Es contra el que se resuelven las rutas relativas de las imagenes.
    let doc_dir = target_doc_dir(&docs, doc_id);

    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app2.dialog().file();
        for format in ExportFormat::ALL {
            dialog = dialog.add_filter(format.display_name(), &[format.extension()]);
        }
        dialog
            .set_file_name(format!("documento.{}", ExportFormat::Html.extension()))
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

    let extension = out_path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(format) = ExportFormat::from_extension(&extension) else {
        // Pasa si escribes el nombre a mano con una extension que no soportamos
        // (p. ej. `.pdf`, que necesita un motor externo que no se distribuye).
        return Err(format!(
            "No se puede exportar a «{extension}». Formatos disponibles: {}.",
            ExportFormat::ALL.map(|f| f.extension()).join(", ")
        ));
    };
    let out_str = out_path.to_string_lossy().into_owned();

    // PDF lo compila typst: se comprueba el sidecar ANTES de lanzar nada, para fallar
    // con un mensaje util en vez de un error de proceso.
    if format.needs_pdf_engine() && !sidecar_path("typst")?.exists() {
        return Err(
            "Falta el motor de PDF. Ejecuta scripts/fetch-typst.ps1 y vuelve a compilar."
                .to_string(),
        );
    }

    // Los procesos y sus temporales los orquesta `apostrophe_sidecars::export` (probado de
    // extremo a extremo con los sidecars reales). Aca solo se le da la fabrica de comandos
    // del plugin shell: misma resolucion de ruta del sidecar y, en Windows, sin consola.
    // Lanzar procesos y esperarlos es bloqueante: va fuera del hilo de eventos.
    let seq = PREVIEW_SEQ.fetch_add(1, Ordering::Relaxed);
    let work_dir =
        std::env::temp_dir().join(format!("apostrophe-export-{}-{}", std::process::id(), seq));
    let app2 = app.clone();
    let job_out = out_path.clone();
    let job_dir = doc_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sidecar = |name: &str| {
            app2.shell()
                .sidecar(name)
                .map(std::process::Command::from)
                .map_err(|e| format!("no se encontro el sidecar {name}: {e}"))
        };
        let job = apostrophe_sidecars::export::ExportJob {
            format,
            markdown: &text,
            output: &job_out,
            doc_dir: job_dir.as_deref(),
            css: &css,
            accent: &accent,
        };
        // La GUI muestra el error tal cual: `to_string()` conserva los mensajes de siempre.
        apostrophe_sidecars::export::export(&sidecar, &job, &work_dir).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("error al exportar: {e}"))??;

    // El HTML se termina de armar aca. DOCX, ODT, EPUB y PDF ya incrustan la imagen dentro
    // del archivo, pero el HTML conserva el `src` relativo, que solo funciona si el export
    // cae justo al lado de la imagen — compartir el .html suelto lo rompe. (El LaTeX
    // tambien conserva la ruta, `\includegraphics{gato.png}`: es texto fuente y se compila
    // junto a sus imagenes, asi que se deja como esta.)
    if format == ExportFormat::Html {
        embed_html_images(&out_path, doc_dir).await?;
    }
    Ok(Some(out_str))
}

/// Reescribe el HTML ya exportado incrustando cada imagen local como data URI.
///
/// Es el MISMO camino que el preview (`rewrite_img_srcs` + `image_data_uri`), y no
/// `--embed-resources` de pandoc, a proposito: ese flag tambien se traga el bundle de
/// MathJax, lo que obliga a tener red en el momento de exportar y engorda el archivo de
/// unos pocos KB a mas de un mega. Aca solo viajan las imagenes; las formulas se siguen
/// resolviendo con el script de MathJax, igual que antes. Ver `html_nunca_incrusta_recursos`.
///
/// Lo que no se pueda incrustar —remota, ausente o por encima del tope— conserva su `src`
/// original: una imagen rota en el export es mejor que un export que falla entero.
///
/// Leer y codificar cada imagen es I/O bloqueante: va fuera del hilo de eventos.
async fn embed_html_images(path: &Path, doc_dir: Option<PathBuf>) -> Result<(), String> {
    let path = path.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || embed_html_images_in(&path, doc_dir.as_deref()))
        .await
        .map_err(|e| format!("error al incrustar las imagenes del export: {e}"))?
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

/// Carga el diccionario `es`. Bundle: la carpeta `dictionaries/` de los recursos
/// (BaseDirectory::Resource). Dev: fallback al dir del crate (donde fetch-dictionaries.ps1
/// deja los archivos), porque en `tauri dev` el resolver de Resource puede no encontrar
/// los resources. Solo se cae al fallback si FALTAN archivos: un diccionario roto en el
/// bundle se informa tal cual.
fn load_bundled_dictionary(app: &tauri::AppHandle) -> Result<SpellbookDictionary, DictionaryError> {
    let dev_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/dictionaries"));
    match app.path().resolve("dictionaries", BaseDirectory::Resource) {
        Ok(dir) => match load_es(&dir) {
            Err(DictionaryError::Missing { .. }) => load_es(dev_dir),
            loaded => loaded,
        },
        Err(_) => load_es(dev_dir),
    }
}

/// Carga el diccionario es en background (off-thread, es caro) y lo guarda en
/// SpellState. Si falla (no provisionado), el spellcheck degrada graciosamente ([]).
fn load_dictionary(app: &tauri::AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let h = handle.clone();
        let loaded =
            tauri::async_runtime::spawn_blocking(move || load_bundled_dictionary(&h)).await;
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
// por DOCUMENTO abierto, nombrado con el id de sesion (uuid v4 que se genera en run() y
// se mantiene toda la sesion) y el DocId. Si hay un snapshot huerfano al arrancar (crash), se restaura en el frontend.
// El .md real del usuario NUNCA se toca: el snapshot es un respaldo paralelo.

/// Id unico de esta sesion (uuid v4): junto con el DocId nombra el snapshot de cada
/// documento. Se genera una vez en run() y se reusa todo el tiempo.
/// `closing` es seteado por confirm_close justo antes de borrar el snapshot, para
/// anular cualquier write_snapshot en vuelo que ya haya pasado el guard del frontend.
struct SnapshotSession {
    id: String,
    closing: AtomicBool,
}

/// Snapshots huerfanos de una sesion anterior (crash) detectados al arrancar, para que
/// el frontend los restaure: uno por documento que tenia cambios sin guardar, del mas
/// reciente al mas antiguo. Espejo del patron PendingFiles.
struct PendingRestore(Mutex<Vec<RestoredSnapshot>>);

/// Contenido y metadatos de un snapshot huerfano listo para restaurar en el editor.
#[derive(serde::Serialize, Clone)]
struct RestoredSnapshot {
    content: String,
    /// Nombre del archivo original si lo habia (para el aviso), o None.
    original_file_name: Option<String>,
    /// Ruta completa del archivo original si la habia, o None (documento sin guardar).
    original_path: Option<String>,
}

/// Devuelve (y crea si no existe) el directorio de snapshots bajo app_local_data_dir.
fn snapshots_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("no se pudo resolver app_local_data_dir: {e}"))?
        .join("snapshots");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("no se pudo crear el dir de snapshots: {e}"))?;
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
    let meta = apostrophe_core::snapshot::SnapshotMeta {
        original_path,
        saved_at,
    };
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

/// Busca los snapshots huerfanos del dir (de una sesion anterior), del mas reciente al
/// mas antiguo, con el contenido y, si lo habia, el archivo original. Tolerante a
/// archivos corruptos: los omite sin propagarlos.
fn find_orphans(dir: &Path) -> Vec<RestoredSnapshot> {
    use apostrophe_core::snapshot::{
        meta_file_name, parse_snapshot_id, snapshot_file_name, sort_newest_first, SnapshotMeta,
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut found: Vec<(String, SnapshotMeta)> = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = parse_snapshot_id(&file_name) else {
            continue; // no es un .snapshot -> saltar
        };
        let Ok(meta_bytes) = std::fs::read(dir.join(meta_file_name(id))) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<SnapshotMeta>(&meta_bytes) else {
            continue; // metadata corrupta -> saltar
        };
        found.push((id.to_owned(), meta));
    }
    sort_newest_first(&mut found);

    found
        .into_iter()
        .filter_map(|(id, meta)| {
            let content = std::fs::read_to_string(dir.join(snapshot_file_name(&id))).ok()?;
            let original = meta.original_path.as_ref();
            Some(RestoredSnapshot {
                content,
                original_file_name: original
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned()),
                original_path: original.map(|p| p.to_string_lossy().into_owned()),
            })
        })
        .collect()
}

/// Borra el snapshot de UN documento de la sesion (best-effort, fuera del hilo de
/// eventos). Se llama al guardarlo y al cerrarlo.
async fn delete_doc_snapshot(app: &tauri::AppHandle, session: &SnapshotSession, id: DocId) {
    let Ok(dir) = snapshots_dir(app) else { return };
    let snap_id = apostrophe_core::snapshot::doc_snapshot_id(&session.id, id);
    let _ =
        tauri::async_runtime::spawn_blocking(move || delete_snapshot_files(&dir, &snap_id)).await;
}

/// Borra TODOS los snapshots de esta sesion (los de cada documento). Se llama al cerrar
/// la ventana: por prefijo de sesion y no por la lista de documentos, asi tambien caen
/// los de un documento cerrado cuyo write estaba en vuelo.
async fn delete_session_snapshots(app: &tauri::AppHandle, session: &SnapshotSession) {
    let Ok(dir) = snapshots_dir(app) else { return };
    let session_id = session.id.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if apostrophe_core::snapshot::is_session_file(&name, &session_id) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    })
    .await;
}

/// Escribe (o borra) el snapshot del documento destino (`doc_id` o el activo) segun la
/// politica de should_snapshot. Lo invoca el frontend en cada tick del timer de
/// autosave, una vez por documento.
#[tauri::command]
async fn write_snapshot(
    app: tauri::AppHandle,
    text: String,
    doc_id: Option<DocId>,
    docs: tauri::State<'_, Docs>,
    session: tauri::State<'_, SnapshotSession>,
) -> Result<(), String> {
    // Guard de cierre: si confirm_close ya seto la bandera, no re-escribir el snapshot
    // que fue (o sera) borrado por confirm_close (anula writes en vuelo).
    if session.closing.load(Ordering::SeqCst) {
        return Ok(());
    }
    // Lock BREVE: leer dirty + path, soltar ANTES de tocar disco (no cruzar el .await).
    let state = {
        let ws = docs.lock().unwrap();
        ws.resolve(doc_id)
            .and_then(|id| ws.get(id).map(|d| (id, d.is_dirty(), d.path().cloned())))
    };
    let Some((id, dirty, path)) = state else {
        return Ok(()); // el documento ya se cerro: no hay nada que respaldar
    };
    let snap_id = apostrophe_core::snapshot::doc_snapshot_id(&session.id, id);
    let dir = snapshots_dir(&app)?;

    if apostrophe_core::snapshot::should_snapshot(dirty, text.len()) {
        let job_dir = dir.clone();
        let job_id = snap_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            write_snapshot_files(&job_dir, &job_id, &text, path)
        })
        .await
        .map_err(|e| format!("error de autosave: {e}"))??;
        // Si el documento se cerro (o la ventana empezo a cerrarse) mientras se escribia,
        // su snapshot quedaria huerfano y se ofreceria restaurarlo en el proximo
        // arranque: se borra.
        if session.closing.load(Ordering::SeqCst) || !docs.lock().unwrap().contains(id) {
            let _ =
                tauri::async_runtime::spawn_blocking(move || delete_snapshot_files(&dir, &snap_id))
                    .await;
        }
        Ok(())
    } else {
        // Doc limpio o vacio: el .md real esta a salvo -> borrar su snapshot.
        let _ = tauri::async_runtime::spawn_blocking(move || {
            delete_snapshot_files(&dir, &snap_id);
        })
        .await;
        Ok(())
    }
}

/// Devuelve el snapshot huerfano MAS RECIENTE pendiente de restaurar, si lo hay, y
/// descarta los demas. Es el comportamiento del frontend de un solo documento, que solo
/// puede restaurar uno. Espejo de take_pending_file.
#[tauri::command]
fn take_pending_restore(pending: tauri::State<'_, PendingRestore>) -> Option<RestoredSnapshot> {
    std::mem::take(&mut *pending.0.lock().unwrap())
        .into_iter()
        .next()
}

/// Devuelve (y limpia) TODOS los snapshots huerfanos pendientes, del mas reciente al mas
/// antiguo: uno por pestaña a restaurar.
#[tauri::command]
fn take_pending_restores(pending: tauri::State<'_, PendingRestore>) -> Vec<RestoredSnapshot> {
    std::mem::take(&mut *pending.0.lock().unwrap())
}

/// Borra TODOS los archivos .snapshot y .meta.json del directorio (best-effort).
/// Se llama al arrancar, despues de cargar el huerfano en memoria, para evitar que
/// crashes multiples acumulen archivos indefinidamente.
fn purge_snapshots_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
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
// Android), por eso se lee de argv. Puede llegar MAS de una ruta (`apostrophe.exe a.md
// b.md` desde una terminal). Dos caminos:
//  - app cerrada: setup() resuelve argv -> PendingFiles; el frontend los consume al cargar.
//  - app abierta: single-instance reenvia el argv -> un evento "open-file-path" por ruta.

/// Extrae TODAS las rutas de archivo de argv, en orden. Ignora argv[0] (el exe) y los
/// flags (empiezan con '-'). No valida existencia: el lector reporta un error legible si
/// una ruta no se puede leer.
fn file_args(args: &[String]) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|a| !a.is_empty() && !a.starts_with('-'))
        .cloned()
        .collect()
}

/// Resuelve una ruta recibida por single-instance contra el directorio actual de la
/// instancia que la RECIBIO del sistema (la segunda), no contra el de esta: `apostrophe.exe
/// nota.md` lanzado desde otra carpeta trae una ruta relativa a esa carpeta. Si el plugin
/// no informa el directorio, se deja tal cual.
fn resolve_launch_path(cwd: &str, arg: &str) -> PathBuf {
    let path = PathBuf::from(arg);
    if path.is_absolute() || cwd.is_empty() {
        path
    } else {
        Path::new(cwd).join(path)
    }
}

/// Archivos pendientes del ARRANQUE EN FRIO (asociacion .md con la app cerrada), en el
/// orden de argv: se resuelven en setup() y el frontend los consume (y limpia) al iniciar.
struct PendingFiles(Mutex<Vec<OpenedDoc>>);

/// Devuelve el PRIMER archivo pendiente del arranque en frio, si lo hay, y descarta la
/// lista. Es lo que espera el frontend de un solo documento, que carga el .md con el que
/// se abrio la app (los demas siguen abiertos en el workspace).
#[tauri::command]
fn take_pending_file(pending: tauri::State<'_, PendingFiles>) -> Option<OpenedDoc> {
    std::mem::take(&mut *pending.0.lock().unwrap())
        .into_iter()
        .next()
}

/// Devuelve (y limpia) TODOS los archivos pendientes del arranque en frio, en orden: uno
/// por pestaña. El primero es el documento activo.
#[tauri::command]
fn take_pending_files(pending: tauri::State<'_, PendingFiles>) -> Vec<OpenedDoc> {
    std::mem::take(&mut *pending.0.lock().unwrap())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance PRIMERO (requisito del plugin): si ya hay una instancia, la
        // nueva (lanzada por Windows al abrir un .md) reenvia su argv y se cierra.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            // Traer el foco a la ventana existente.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_focus();
            }
            // Apertura via asociacion con la app YA abierta: emitir CADA path al frontend,
            // que hace el open guardado (open_path). NO leemos el archivo aca: este
            // callback corre en el event-loop y leer/dialogar podria bloquear (deadlock M4).
            // Las rutas relativas se resuelven contra el directorio de la segunda instancia.
            for path in file_args(&args) {
                let full = resolve_launch_path(&cwd, &path);
                let _ = app.emit("open-file-path", full.to_string_lossy().into_owned());
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
        // M11.1: preferencias persistentes (settings.json). El frontend lee/escribe via JS.
        .plugin(tauri_plugin_store::Builder::new().build())
        // Pestañas: el workspace arranca con un documento nuevo vacio, como antes.
        .manage(Mutex::new(initial_workspace()))
        .manage(SpellState(RwLock::new(None)))
        .manage(PendingFiles(Mutex::new(Vec::new())))
        // M8.4b: watchers de cambios externos, uno por documento con ruta.
        .manage(FileWatchers(Mutex::new(HashMap::new())))
        // M8.4a: session id generado en run() para que este disponible en setup() sin mover.
        .manage(SnapshotSession {
            id: uuid::Uuid::new_v4().to_string(),
            closing: AtomicBool::new(false),
        })
        .manage(PendingRestore(Mutex::new(Vec::new())))
        .setup(|app| {
            load_dictionary(app.handle()); // carga el diccionario es en background

            // M8.4a: restauracion de crash. Detectar los snapshots huerfanos de la sesion
            // anterior ANTES de procesar argv. Se conservan aunque lleguen archivos: el
            // frontend abre los archivos primero y lo recuperado en pestañas aparte.
            if let Ok(dir) = snapshots_dir(app.handle()) {
                *app.state::<PendingRestore>().0.lock().unwrap() = find_orphans(&dir);
                // Barrer TODOS los snapshots y metadatos del arranque anterior (incluyendo
                // el huerfano que acabamos de cargar en memoria, huerfanos de crashes
                // anteriores y archivos con meta corrupta). La app esta arrancando: es
                // seguro borrar todo; la sesion nueva generara su propio snapshot.
                purge_snapshots_dir(&dir);
            }

            // Capa B: arranque en frio via asociacion .md (app cerrada). Windows pasa las
            // rutas en argv; las cargamos en el workspace y las dejamos pendientes para que
            // el frontend las muestre al iniciar. La primera ocupa el documento vacio
            // inicial y las demas se abren aparte; la primera queda activa. Una ruta que
            // falla (invalida) se ignora. El restore no se descarta: convive con estos
            // archivos en pestañas propias.
            let argv: Vec<String> = std::env::args().collect();
            let docs = app.state::<Docs>();
            let initial = docs.lock().unwrap().active();
            let mut pending: Vec<OpenedDoc> = Vec::new();
            for path in file_args(&argv) {
                let into = if pending.is_empty() { initial } else { None };
                match load_path(app.handle(), PathBuf::from(&path), &docs, into) {
                    Ok((opened, fresh)) => {
                        // M8.4b: armar el watcher sobre el archivo del arranque en frio.
                        if fresh {
                            rewatch_doc(app.handle(), &docs, opened.doc_id);
                        }
                        // La misma ruta dos veces en argv es un solo documento.
                        if !pending.iter().any(|o| o.doc_id == opened.doc_id) {
                            pending.push(opened);
                        }
                    }
                    Err(e) => eprintln!("no se pudo abrir el archivo de inicio '{path}': {e}"),
                }
            }
            if let Some(first) = pending.first() {
                docs.lock().unwrap().activate(first.doc_id);
            }
            *app.state::<PendingFiles>().0.lock().unwrap() = pending;

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
            recent_files,
            make_table,
            toggle_preview_window,
            preview_window_open,
            align_table,
            inline_target,
            render_math,
            render_image,
            pick_image,
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
            take_pending_files,
            write_snapshot,
            take_pending_restore,
            take_pending_restores,
            reload_from_disk,
            activate_document,
            close_document,
            list_documents,
            dirty_documents
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        absolute_doc_path, check_save_target, file_args, find_orphans, resolve_launch_path,
    };
    use apostrophe_core::workspace::Workspace;

    #[test]
    fn guardar_como_sobre_la_ruta_de_otro_documento_se_rechaza() {
        let mut ws = Workspace::new();
        let a = ws.open_path("/docs/a.md".into(), "A".into()).id;
        let b = ws.create_untitled();
        assert_eq!(
            check_save_target(&ws, b, std::path::Path::new("/Docs/A.md")),
            Err("path-open-in-other-document".to_string())
        );
        let _ = a;
    }

    #[test]
    fn guardar_sobre_su_propia_ruta_o_una_libre_se_permite() {
        let mut ws = Workspace::new();
        let a = ws.open_path("/docs/a.md".into(), "A".into()).id;
        assert_eq!(
            check_save_target(&ws, a, std::path::Path::new("/docs/a.md")),
            Ok(())
        );
        assert_eq!(
            check_save_target(&ws, a, std::path::Path::new("/docs/b.md")),
            Ok(())
        );
    }

    #[test]
    fn guardar_un_documento_ya_cerrado_se_rechaza() {
        let mut ws = Workspace::new();
        let a = ws.create_untitled();
        ws.close(a);
        assert_eq!(
            check_save_target(&ws, a, std::path::Path::new("/docs/a.md")),
            Err("unknown-document".to_string())
        );
    }

    #[test]
    fn el_payload_de_cambio_en_disco_usa_doc_id_en_snake_case() {
        let json = serde_json::to_value(super::DiskChange { doc_id: 7 }).unwrap();
        assert_eq!(json, serde_json::json!({ "doc_id": 7 }));
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn file_args_devuelve_todas_las_rutas_en_orden() {
        let argv = args(&["apostrophe.exe", "a.md", "--flag", "", "C:\\docs\\b.md"]);
        assert_eq!(file_args(&argv), args(&["a.md", "C:\\docs\\b.md"]));
    }

    #[test]
    fn file_args_vacio_cuando_solo_esta_el_exe() {
        assert!(file_args(&args(&["apostrophe.exe"])).is_empty());
    }

    #[test]
    fn resolve_launch_path_resuelve_la_relativa_contra_el_cwd_de_la_segunda_instancia() {
        let cwd = std::env::temp_dir().join("otra-carpeta");
        let full = resolve_launch_path(&cwd.to_string_lossy(), "nota.md");
        assert_eq!(full, cwd.join("nota.md"));
    }

    #[test]
    fn resolve_launch_path_no_toca_la_absoluta() {
        let absolute = std::env::temp_dir().join("docs").join("nota.md");
        let cwd = std::env::temp_dir().join("otra-carpeta");
        let full = resolve_launch_path(&cwd.to_string_lossy(), &absolute.to_string_lossy());
        assert_eq!(full, absolute);
    }

    #[test]
    fn resolve_launch_path_sin_cwd_deja_la_ruta_igual() {
        assert_eq!(
            resolve_launch_path("", "nota.md"),
            std::path::PathBuf::from("nota.md")
        );
    }

    /// Deja en `dir` un snapshot con su metadata, como lo dejaria una sesion que crasheo.
    fn plant_snapshot(dir: &std::path::Path, id: &str, content: &str, saved_at: u64) {
        use apostrophe_core::snapshot::{meta_file_name, snapshot_file_name, SnapshotMeta};
        std::fs::write(dir.join(snapshot_file_name(id)), content).unwrap();
        let meta = SnapshotMeta {
            original_path: Some(dir.join(format!("{id}.md"))),
            saved_at,
        };
        std::fs::write(
            dir.join(meta_file_name(id)),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("apostrophe-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn find_orphans_devuelve_todos_del_mas_reciente_al_mas_antiguo() {
        let dir = scratch_dir("orphans");
        plant_snapshot(&dir, "s-1", "uno", 10);
        plant_snapshot(&dir, "s-2", "dos", 30);
        plant_snapshot(&dir, "s-3", "tres", 20);
        let orphans = find_orphans(&dir);
        let contents: Vec<_> = orphans.iter().map(|o| o.content.as_str()).collect();
        assert_eq!(contents, vec!["dos", "tres", "uno"]);
        assert_eq!(orphans[0].original_file_name.as_deref(), Some("s-2.md"));
        assert_eq!(
            orphans[0].original_path.as_deref(),
            Some(dir.join("s-2.md").to_string_lossy().as_ref())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_orphans_omite_los_que_no_tienen_metadata_valida() {
        let dir = scratch_dir("orphans-corrupt");
        plant_snapshot(&dir, "ok", "bien", 10);
        std::fs::write(dir.join("roto.snapshot"), "sin meta").unwrap();
        std::fs::write(dir.join("malo.snapshot"), "meta corrupta").unwrap();
        std::fs::write(dir.join("malo.meta.json"), "{no es json").unwrap();
        let orphans = find_orphans(&dir);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].content, "bien");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn la_ruta_relativa_del_documento_se_vuelve_absoluta() {
        // `apostrophe.exe nota.md` desde una terminal: sin esto la carpeta del documento
        // seria `""` y no serviria para resolver sus imagenes ni como directorio actual.
        let path = absolute_doc_path(std::path::PathBuf::from("nota.md"));
        assert!(path.is_absolute(), "{path:?}");
        assert_eq!(
            path.parent(),
            Some(std::env::current_dir().unwrap().as_path())
        );
        assert_eq!(path.file_name().unwrap(), "nota.md");
    }

    #[test]
    fn la_ruta_absoluta_del_documento_no_cambia() {
        let original = std::env::temp_dir().join("notas").join("nota.md");
        assert_eq!(absolute_doc_path(original.clone()), original);
    }

    #[test]
    fn file_args_devuelve_la_ruta_tras_el_exe() {
        let args = vec![
            "apostrophe.exe".to_string(),
            "C:\\docs\\nota.md".to_string(),
        ];
        assert_eq!(file_args(&args), vec!["C:\\docs\\nota.md".to_string()]);
    }

    #[test]
    fn file_args_ignora_flags() {
        let args = vec![
            "apostrophe.exe".to_string(),
            "--algun-flag".to_string(),
            "nota.md".to_string(),
        ];
        assert_eq!(file_args(&args), vec!["nota.md".to_string()]);
    }
}
