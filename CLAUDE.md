# Apostrophe para Windows

Editor de Markdown nativo para Windows. Port de [Apostrophe](https://gitlab.gnome.org/World/apostrophe)
(GNOME) construido con **Tauri 2 + Rust** bajo arquitectura hexagonal
(ver `docs/adr/0001-frontera-dominio-presentacion.md`).

## Idioma

### Interfaz — bilingüe vía catálogo (M15)

La app soporta **español e inglés**. Ninguna cadena visible se escribe en `index.html`
ni en los módulos: **todas** viven en el catálogo de `src/i18n.ts`.

- HTML estático: `data-i18n` (contenido), `data-i18n-title`, `data-i18n-aria-label`.
- TypeScript: `t("clave")` y `tp("clave", n)` para plurales.
- Diálogos nativos: la copia la arma el frontend y viaja por IPC (`ConfirmLabels`).
  **El backend no guarda copia** — sumar un idioma no debe tocar Rust.
- La interfaz `Messages` obliga a que todo idioma defina toda la clave: una traducción
  faltante es un error de compilación, no un hueco en producción.

Al escribir en español dentro del catálogo, sigue las reglas de castellano neutro de abajo.

### Todo lo demás — castellano neutro (OBLIGATORIO)

Respuestas del asistente, comentarios del código, mensajes de commit y documentación en
**castellano neutro/nativo**.

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
