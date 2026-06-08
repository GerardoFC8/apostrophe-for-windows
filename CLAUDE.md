# Apostrophe para Windows

Editor de Markdown nativo para Windows. Port de [Apostrophe](https://gitlab.gnome.org/World/apostrophe)
(GNOME) construido con **Tauri 2 + Rust** bajo arquitectura hexagonal
(ver `docs/adr/0001-frontera-dominio-presentacion.md`).

## Idioma — castellano neutro (OBLIGATORIO)

Todo en **castellano neutro/nativo**: respuestas del asistente, cadenas de la interfaz
(diálogos, mensajes, botones), comentarios del código, mensajes de commit y documentación.

Reglas:

- Usa la segunda persona con **"tú"** (tienes, quieres, puedes, guarda, abre, crea),
  **NUNCA voseo** (tenés, querés, podés, guardá, abrí, creá).
- **Sin regionalismos ni jerga.** Evita expresiones locales; prefiere las neutras,
  entendibles en cualquier país hispanohablante.
- Tono profesional, claro y directo.

Ejemplos (incorrecto → correcto):

| Voseo / regional | Castellano neutro |
| --- | --- |
| tenés cambios | tienes cambios |
| guardá con Ctrl+S | guarda con Ctrl+S |
| ¿querés cerrar? | ¿quieres cerrar? |
| ¿cerrar igual? | ¿cerrar de todos modos? |
| fijate / mirá | fíjate / mira |

Patrón para diálogos (estilo GNOME, sin repetir texto): **pregunta corta en el título +
explicación en el cuerpo**. Ej.: título `¿Cerrar sin guardar?`, cuerpo
`El documento tiene cambios que se perderán.` Motivo: `rfd` (la librería de diálogos)
usa el título tanto para el encabezado como para el título de la ventana, así que repetir
la misma frase en título y cuerpo la muestra dos o tres veces.

## Convenciones

- **Commits convencionales** (`feat`, `fix`, `docs`, `refactor`…). **NUNCA** agregar
  atribución de IA ni `Co-Authored-By`.
- **Arquitectura hexagonal** (ADR-0001): la lógica de dominio vive en
  `crates/apostrophe-core` (puro, sin Tauri); `src-tauri` es solo el adaptador (IPC,
  diálogos, procesos). El frontend (`src/`) solo PRESENTA.
- **Verificar antes de afirmar.** Las APIs de Tauri 2 y el comportamiento de las
  dependencias se confirman contra docs o el código fuente instalado, no de memoria.
- **Diálogos nativos**: siempre con el patrón `async` + `spawn_blocking` (evita el
  deadlock en el hilo principal de WebView2 — ver comentarios en `src-tauri/src/lib.rs`).

## Build y verificación

- Dominio (corre en WSL): `cargo test -p apostrophe-core`.
- Workspace completo: `cargo test`, `cargo clippy`.
- Frontend: `pnpm exec tsc --noEmit`.
- App completa (instalador, asociación de archivos, diálogos nativos de Windows):
  se valida en **Windows real** con `pnpm tauri build` / `pnpm tauri dev`. En WSL los
  diálogos usan el backend GTK y se ven distintos (es solo el entorno de desarrollo).
