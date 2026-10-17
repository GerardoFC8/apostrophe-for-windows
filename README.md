**Español** | [English](./README.en.md)

# Apostrophe para Windows

Editor de Markdown nativo, minimalista y libre de distracciones para Windows.
Port nativo del editor [Apostrophe](https://gitlab.gnome.org/World/apostrophe) de
GNOME, construido con **Tauri 2 + Rust** bajo una arquitectura hexagonal
(ver [`docs/adr/0001-frontera-dominio-presentacion.md`](docs/adr/0001-frontera-dominio-presentacion.md)).

## Características

- Editor CodeMirror 6 con titlebar Fluent custom.
- Vista previa en vivo vía **pandoc** (bundleado como sidecar, sin dependencias del sistema).
- Abrir / guardar `.md` con diálogos nativos.
- Temas claro / oscuro / sepia.
- Exportación a **PDF, HTML, ODT, DOCX, EPUB y LaTeX** (Ctrl+E) — el formato se elige en
  el desplegable "Guardar como tipo" del diálogo. Lo resuelven los sidecars bundleados
  (pandoc y typst): no necesitas instalar nada más.
- Corrector ortográfico en español (Hunspell vía `spellbook`, puro-Rust).
- CLI `elision` para agentes de IA y scripts (ver [CLI para agentes](#cli-para-agentes-elision)).

## Instalación

La forma más sencilla es con [winget](https://learn.microsoft.com/windows/package-manager/winget/),
el gestor de paquetes que ya viene en Windows 10 y 11. Desde PowerShell o la terminal:

```powershell
winget install GerardoFC8.ApostropheForWindows
```

No pide permisos de administrador: la app se instala solo para tu usuario.

Para actualizar o desinstalar:

```powershell
winget upgrade GerardoFC8.ApostropheForWindows
winget uninstall GerardoFC8.ApostropheForWindows
```

> La versión de winget puede tardar unos días en alcanzar a la última release. Si
> quieres la más reciente, descarga `Apostrophe_<versión>_x64-setup.exe` desde
> [Releases](https://github.com/GerardoFC8/apostrophe-for-windows/releases) y ejecútalo
> (ver ["Windows protegió tu PC"](#windows-protegió-tu-pc--es-normal)).

## CLI para agentes (`elision`)

El instalador también deja en el PATH el comando **`elision`**: una CLI para agentes de IA
y scripts. Usa los mismos pandoc, typst y diccionario que la app, nunca pide confirmación y
su salida es estable. El contrato son los **códigos de salida y de error**; los mensajes van
en inglés y solo sirven de diagnóstico.

> Después de instalar, **cierra por completo** Windows Terminal, VS Code o el programa desde
> el que abres la terminal, y vuelve a abrirlo. Una pestaña nueva no basta: hereda el PATH
> que tenía ese programa al arrancar, así que ni ella ni los agentes que corran en ella
> encuentran `elision`. Si no quieres cerrarlo, recarga el PATH en la pestaña actual:
>
> ```powershell
> $env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')
> ```

```powershell
elision --help
elision stats notas.md --json
elision export notas.md -o notas.pdf
```

### Comandos

| Comando | Qué hace | Termina con 1 si… |
|---|---|---|
| `export <in> -o <out> [--to <fmt>]` | Convierte a PDF, HTML, ODT, DOCX, EPUB o LaTeX. El formato sale de la extensión de `-o`, o de `--to`. Sobrescribe sin preguntar. | — |
| `stats <in>` | Caracteres, palabras, oraciones, párrafos y tiempo de lectura. | — |
| `tables <in> [--check \| --write]` | Revisa (por defecto) o reescribe la alineación de todas las tablas. | hay tablas desalineadas (solo `--check`) |
| `spell <in> [--dict-dir <dir>]` | Ortografía en español, con línea y columna (base 1). | hay errores |
| `open <archivos>...` | Abre los archivos en el editor sin esperar a que se cierre. | — |

- **Entrada estándar:** `<in>` puede ser `-` (salvo con `tables --write`). Se decodifica igual
  que un archivo: UTF-8 u otra codificación que la app sepa abrir. Con `-`, `export` no tiene
  carpeta de documento y omite las imágenes relativas.
- **`--json`**, antes o después del subcomando: una sola línea JSON en stdout, solo ASCII
  (lo demás sale como `\uXXXX`), también para los errores. Sin `--json`, stdout lleva solo
  datos y stderr los diagnósticos.
- **Diccionario:** `--dict-dir`; si no, la variable `ELISION_DICT_DIR`; si no, el que trae la
  instalación. La primera opción indicada se usa sin caer a la siguiente: si falla, el
  resultado es `dictionary_missing`.
- Las rutas relativas se resuelven contra la carpeta actual, y las imágenes del documento,
  contra la carpeta del documento: puedes exportar desde cualquier lugar.

### Códigos de salida

| Salida | Significado | `error.code` (JSON) |
|---|---|---|
| 0 | Éxito | — |
| 1 | Hallazgos: errores de ortografía o tablas desalineadas | — (`"ok": true`) |
| 2 | Uso inválido: argumentos, formato desconocido | `usage` |
| 3 | Falta pandoc, typst, el editor o el diccionario | `sidecar_missing`, `dictionary_missing` |
| 4 | Pandoc o typst fallaron al convertir | `conversion_failed` |
| 5 | Error de E/S: archivo inexistente, salida no escribible | `io_error` |

Un error con `--json`:

```json
{"ok":false,"error":{"code":"usage","message":"cannot tell the export format from x.xyz; use --to with one of: html, pdf, odt, docx, epub, latex"}}
```

### Ejemplos para un agente

```powershell
# Ortografía: lee el JSON, no el texto.
elision spell capitulo.md --json
# {"ok":true,"file":"C:\\notas\\capitulo.md","language":"es","misspellings":[{"line":7,"column":6,"word":"mundoo","suggestions":["mundo","mundos","mundo o","mundano"]}]}

# Tablas: reescribe solo si --check encontró algo (código 1).
elision tables capitulo.md --check --json
if ($LASTEXITCODE -eq 1) { elision tables capitulo.md --write }

# Export desde otra carpeta; las imágenes relativas se resuelven contra el documento.
elision export docs\capitulo.md -o C:\salida\capitulo.pdf --json
# {"ok":true,"output":"C:\\salida\\capitulo.pdf","format":"pdf"}
```

### Límites conocidos

- **Acentos en tuberías de PowerShell.** Windows PowerShell 5.1 envía texto a otros
  programas con `$OutputEncoding`, que por defecto es ASCII: `Get-Content x.md | elision stats -`
  convierte los acentos en `?`. Además, PowerShell decodifica la salida de texto con la
  página de códigos de la consola. Pasa la ruta del archivo en lugar de `-`, lee los
  resultados con `--json` (solo ASCII) y exporta con `-o`.
- **Precedencia en el PATH.** La carpeta de instalación también contiene `pandoc.exe` y
  `typst.exe`, y se agrega **al final** del PATH. Con el instalador NSIS (el de winget) gana
  cualquier pandoc o typst que ya esté antes en el PATH. Con el MSI la carpeta va al `Path`
  del sistema, que Windows antepone al del usuario: los incluidos tapan a los que hayas
  instalado solo para tu usuario. `elision` siempre usa los suyos.
- **`tables --write` no es atómico:** reescribe el archivo en su lugar, igual que el guardado
  de la app. Si el archivo está abierto en la app, esta muestra el aviso de cambio externo.
- **Tablas omitidas:** `tables` informa, sin arreglarlas, las tablas que no sabe alinear
  (barra escapada `\|`, filas con celdas de más, sangría mixta o tabla dentro de una cita).
  Salen en `skipped` del JSON (`escaped_pipe`, `extra_cells`, `mixed_indent`, `blockquote`)
  y como aviso en stderr; no cambian el código de salida.
- **HTML sin tema:** el export a HTML usa el estilo por defecto de pandoc, no el tema de la
  app.
- **`open` con varios archivos:** los pasa todos, pero por ahora el editor abre solo el
  primero (lo avisa en stderr).
- **Sin firma:** `elision.exe` tampoco está firmado. Algunos antivirus desconfían de
  ejecutables nuevos sin firma y pueden bloquearlo o ponerlo en cuarentena; si pasa, agrega
  una excepción para la carpeta de instalación.

En desarrollo, sin instalar: `cargo run -p apostrophe --bin elision -- --help`. La
arquitectura está en [`docs/adr/0002-cli-adaptador-primario.md`](docs/adr/0002-cli-adaptador-primario.md).

## Requisitos de desarrollo

- [Rust](https://rustup.rs/) (toolchain MSVC en Windows: `x86_64-pc-windows-msvc`).
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/) (este proyecto usa **pnpm**, no npm).
- En Windows: [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
  (preinstalado en Windows 10/11 modernos).

## Puesta en marcha (dev)

Los binarios pesados (pandoc ~150 MB, typst ~21 MB) y los diccionarios Hunspell **no se
commitean**: están gitignored y se provisionan con scripts. Antes del primer
`dev`/`build`, ejecuta los scripts de provisioning desde PowerShell:

```powershell
pwsh scripts/fetch-pandoc.ps1        # baja pandoc-x86_64-pc-windows-msvc.exe (sidecar)
pwsh scripts/fetch-typst.ps1         # baja typst-x86_64-pc-windows-msvc.exe (motor de PDF)
pwsh scripts/fetch-dictionaries.ps1  # baja los diccionarios Hunspell (es, en)
```

> El pin de pandoc debe ser **3.9 o superior**. El template typst de 3.7 emitía una lista
> de fuentes vacía que typst 0.15 rechaza, y la exportación a PDF fallaba.

Luego, dev con hot-reload:

```powershell
pnpm install
pnpm tauri dev
```

## Build de release (instalador Windows)

> ⚠️ El instalador **MSI debe construirse en Windows** (WiX solo corre en Windows).
> El instalador NSIS (`setup.exe`) también se genera en el mismo paso.

1. Asegúrate de haber corrido los scripts de provisioning (pandoc, typst y diccionarios).
2. Compila el bundle de release:

   ```powershell
   pnpm tauri build
   ```

3. Los instaladores quedan en (el workspace tiene el `target` en la raíz, no en
   `src-tauri/`):

   ```
   target/release/bundle/nsis/Apostrophe_<versión>_x64-setup.exe   # NSIS
   target/release/bundle/msi/Apostrophe_<versión>_x64_en-US.msi     # MSI (WiX)
   ```

### Configuración del instalador

Definida en `src-tauri/tauri.conf.json` (`bundle`):

- **`installMode: "currentUser"`** — instala por usuario, sin pedir permisos de
  administrador (UAC). Para distribución por-máquina, cambiar a `"perMachine"`.
- **Idiomas del instalador**: español (con selector de idioma).
- **Asociación de archivos `.md` / `.markdown`** — declarada en el instalador. El
  comportamiento de "abrir con doble-click" se implementa en un slice aparte
  (requiere `tauri-plugin-single-instance` y se prueba en Windows).
- **`elision` en el PATH** — el instalador NSIS agrega la carpeta de instalación al final
  del `Path` de tu usuario (`src-tauri/windows/hooks.nsh`, que ejecuta `elision-path.ps1`);
  el MSI, al final del `Path` del sistema (`src-tauri/windows/cli-path.wxs`). Al desinstalar
  se quita solo esa entrada, y reinstalar no la duplica. Las pruebas del script corren en la
  CI; a mano:
  `powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri/windows/elision-path.tests.ps1`.

### "Windows protegió tu PC" — es normal

El instalador **no está firmado**, y es una decisión deliberada: este es un
proyecto libre y personal, no comercial. Un certificado de firma de código cuesta
~$200-400/año y no hace la app más segura ni mejor — solo le quita un clic de
fricción a quien la descarga. No vale la pena para un hobby.

Al instalar, Windows SmartScreen mostrará una pantalla azul: **"Windows protegió
tu PC — Editor desconocido"**. Esto es **esperado y completamente inofensivo**: solo
significa que Windows no reconoce al editor del binario, no que la app sea insegura.
Para continuar:

1. Clic en **"Más información"**.
2. Clic en **"Ejecutar de todas formas"**.

La app instala y funciona con normalidad. Así se distribuyen miles de aplicaciones
open-source para Windows.

> Si en el futuro quisieras eliminar la advertencia (p. ej. para distribución
> masiva), haría falta un certificado OV/EV — ver
> [firma de código en Tauri](https://v2.tauri.app/distribute/sign/windows/).

## Licencia

Este proyecto **deriva** del código de Apostrophe (GNOME), licenciado
**GPL-3.0-or-later**, por lo que se distribuye bajo la misma licencia.
Los diccionarios Hunspell (rla-es vía `wooorm/dictionaries`) se incluyen bajo
MPL 1.1 (ver `src-tauri/dictionaries/es.license.txt`).
