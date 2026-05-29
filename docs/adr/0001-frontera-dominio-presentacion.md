# ADR-0001 — Frontera Dominio / Presentación

- **Estado:** Aceptado
- **Fecha:** 2026-05-29
- **Contexto del proyecto:** clon nativo de [Apostrophe](https://gitlab.gnome.org/World/apostrophe) para Windows, construido con **Tauri 2** (núcleo Rust + WebView2).

---

## Contexto y fuerzas

Apostrophe es un editor Markdown *distraction-free* y **ligero**. El clon es **Windows-first**, con estas prioridades, en orden:

1. **Look/feel nativo de Windows** (faux-native aceptable: Fluent en CSS + chrome nativo vía `tauri-controls` + diálogos nativos del SO; no se exigen controles Win32 reales).
2. **Rendimiento, optimización y escalabilidad** (especialmente en documentos grandes).
3. **Ligereza** coherente con el ethos del producto.

**Riesgo central identificado:** el grano de Tauri empuja la lógica hacia TypeScript dentro de la WebView. Si cedemos a ese grano, perdemos la única ventaja real de Tauri sobre Electron y quedamos con *"un Electron más chico"*. La ventaja de rendimiento de Tauri **solo se materializa si el cómputo CPU-bound vive en el núcleo Rust nativo y multihilo.**

**Hechos verificados del upstream que condicionan el diseño:**

- El resaltado NO es una gramática: es decoración por **~22 regex** (`markup_regex.py`) sobre un buffer plano, parseada **fuera del hilo** (`multiprocessing.Pipe`) y aplicada vía `GLib.idle_add`. → Hay latencia async en el original y es aceptable.
- La conversión Markdown→HTML (preview y export) la hace **Pandoc** (binario externo) en el upstream; el motor PDF por defecto es **Typst**. → Empate técnico entre Tauri y Electron; no depende del backend.
- El spellcheck usa **libspelling**, que **no existe en Windows** → hay que reimplementarlo.
- El render del editor es un motor Chromium-clase tanto en Tauri (WebView2) como en Electron → empate de render.

---

## Decisión

Adoptamos **arquitectura hexagonal (puertos y adaptadores)**:

- El **dominio vive en Rust** como núcleo **puro**: no conoce Tauri, ni la WebView, ni CodeMirror.
- **Tauri** es el **adaptador primario** (comandos + eventos).
- La **WebView (CodeMirror 6 + HTML/CSS)** es **solo presentación**.
- **Pandoc / Typst / hunspell** son **adaptadores secundarios** (procesos externos / FFI).

> Regla de oro arquitectónica: si una pieza es lógica de negocio o cómputo, va en Rust. Si es cómo se ve o cómo se interactúa, va en la WebView. Ante la duda, **va en Rust** (es donde está la ventaja).

---

## Qué va en el NÚCLEO Rust (muro de carga)

| Origen en Apostrophe | Componente del núcleo | Por qué aquí |
|---|---|---|
| `markup_regex.py` + `MarkupHandler` | **Parser de markup** (~22 regex) → emite *spans* de decoración | CPU-bound, off-thread, escala con documentos grandes |
| `stats_counter.py` | **Contador de stats** (chars / words con CJK / sentences / paragraphs / `read_time = words/200*60`) | Cómputo puro; primer objetivo de TDD |
| `helpers.pandoc_convert` + `export_dialog.py` | **Orquestador de conversión** (build de args, spawn de Pandoc/Typst, manejo de errores) | Lógica de negocio, no UI |
| `preview_security.py` | **Política de seguridad de preview** (scan de tags → ASK / RESTRICTED) | Regla de dominio pura |
| `settings.py` + autosave | **DocumentStore** (modelo de documento, persistencia, snapshots) | I/O nativo y rápido |
| *(nuevo — reemplaza libspelling)* | **Motor de spellcheck** compatible Hunspell | `libspelling` no existe en Windows; nativo y rápido |

## Qué va en la PRESENTACIÓN / WebView (tabique movible)

- Editor **CodeMirror 6** + `ViewPlugin`/Decoration que **consume** los spans del núcleo (NO re-parsea con la gramática Lezer — eso divergiría del comportamiento original).
- Render del **preview** (el HTML de Pandoc en un `div`/`iframe`), con **scroll-sync in-page**.
- **CSS de temas** (light / dark / sepia) clonando el design language Adwaita.
- **Modos de layout** del preview, **popovers inline**, **diálogos**, **focus-mode** (centrado typewriter + gris por oración).
- **Chrome nativo** vía `tauri-controls`.

---

## Puertos (interfaces del dominio)

`MarkupParser` · `StatsCounter` · `DocumentStore` · `Converter` (Pandoc/Typst) · `SpellChecker` · `PreviewSecurityPolicy`.

Cada puerto tiene su adaptador concreto. El núcleo depende de los **traits**, no de las implementaciones.

---

## La frontera IPC (instalaciones que cruzan el muro)

- **Comandos** (request/response): `open_document`, `save_document`, `export_document`, `parse_markup(text) → spans`, `count_stats(text) → Stats`, `convert_preview(text, theme) → html`, `spellcheck(range) → suggestions`.
- **Eventos** (streaming Rust → WebView): resultados de parsing/preview que llegan async (equivalente al `GLib.idle_add` del original).
- **Regla de oro:** la frontera es **gruesa y con debounce**, NUNCA un viaje por tecla. Se manda texto/rango, vuelven spans/stats/html. Minimizar serialización es parte explícita del diseño de rendimiento.

---

## Decisiones de diseño resueltas

### D1 — Estrategia de highlighting

**Decisión:** el parsing del resaltado vive en el **núcleo Rust** y cruza el IPC **con debounce (~150–300 ms)**; los spans vuelven a CodeMirror 6 por **evento async**, que aplica las decoraciones. Esto replica el modelo off-thread + `idle_add` del upstream y concentra la ganancia de escalabilidad en docs grandes en el lado nativo.

**Descartado por ahora (optimización diferida):** un "highlight optimista híbrido" (resaltado rápido aproximado en CM6 + corrección autoritativa desde Rust). **No se construye el día 1.** Solo se implementará si el *profiling* demuestra lag perceptible por tecla. No se pre-optimiza sin medición.

### D2 — Spellcheck en el MVP

**Decisión:** el corrector entra **desde el MVP**, en el núcleo Rust, con un motor **compatible Hunspell** (candidatos: crates puro-Rust como `spellbook` / `zspell`, o binding FFI `hunspell-rs`). Requiere **bundlear los diccionarios Hunspell** (`.aff`/`.dic`) y reproducir la regla del upstream: **el spellcheck se auto-desactiva en focus mode**. Las ondulitas se dibujan como decoraciones de CodeMirror 6 (no se delega al spellcheck de Chromium, que no aplica a CM6 por no ser `contenteditable`).

---

## Consecuencias

**Positivas**
- Núcleo nativo rápido y **testeable** (lógica pura, sin UI).
- Presentación delgada y **reemplazable**.
- Binario chico, coherente con el ethos ligero del producto.
- La ventaja real de Tauri sobre Electron **se materializa** (no quedamos en "Electron más chico").

**Negativas / a vigilar**
- Hay que **pelear contra el grano** de Tauri (disciplina arquitectónica continua).
- **Costo de serialización** en el IPC → mitigado con debounce y batching.
- Si en el futuro se va **multi-OS**, el webview cambia por plataforma (WebView2 / WebKitGTK / WKWebView) → riesgo de diferencias de render.
- **Spellcheck es el gap más caro** y entra en el MVP → impacta el alcance inicial (ver D2).

---

## Alternativas descartadas

- **Todo en TypeScript sobre Tauri:** desperdicia el backend nativo → "Electron más chico". Rechazado.
- **Electron:** footprint (~80–150 MB+) y contradicción con el ethos ligero; solo convendría si se priorizara paridad de render multi-OS o time-to-market. Rechazado para este target Windows-only.
- **GTK4 nativo (portar el código real):** feel no-nativo en Windows (GNOME HIG, titlebar CSD roto) **y** su preview depende de WebKitGTK, que **no tiene port a Windows**. Rechazado.
- **Qt/PySide6:** mejor feel nativo real, pero reescritura total del editor contra `QTextDocument` y bundlea ~130 MB de Chromium (QtWebEngine). Rechazado.

---

## Referencias

- Arquitectura verificada del upstream: ver memoria `apostrophe-windows/upstream-architecture`.
- Análisis de stack completo: ver memoria `apostrophe-windows/stack-decision`.
