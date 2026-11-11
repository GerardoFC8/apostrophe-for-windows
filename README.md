**Español** | [English](./README.en.md)

<img src="src-tauri/icons/logo.svg" alt="Logo de Elision" width="96">

# Elision

**Un editor de Markdown para Windows, sencillo y sin distracciones.** Escribes a la
izquierda, ves el resultado a la derecha y exportas a PDF, Word y otros formatos sin
instalar nada más.

Elision (antes *Apostrophe para Windows*) es una versión nativa para Windows del editor
[Apostrophe](https://gitlab.gnome.org/World/apostrophe) de GNOME. Es libre y gratuito.

[Instalación](#instalación) · [Qué puedes hacer](#qué-puedes-hacer) ·
[Atajos](#atajos-de-teclado) · [Pestañas](#pestañas) ·
[Línea de comandos](#cli-para-agentes-elision) ·
[Solución de problemas](#solución-de-problemas) · [Desarrollo](#desarrollo)

## Instalación

Abre PowerShell o la terminal de Windows y ejecuta:

```powershell
winget install elision
```

Eso es todo. No pide permisos de administrador: la app se instala solo para tu usuario.
[winget](https://learn.microsoft.com/windows/package-manager/winget/) es el gestor de
paquetes que ya viene en Windows 10 y 11.

Para actualizar o desinstalar:

```powershell
winget upgrade elision
winget uninstall elision
```

**¿Prefieres descargar el instalador?** Baja `Elision_<versión>_x64-setup.exe` desde
[Releases](https://github.com/GerardoFC8/apostrophe-for-windows/releases) y ejecútalo.
Ahí está siempre la versión más reciente; la de winget puede tardar unos días en
alcanzarla. Windows mostrará el aviso
["Windows protegió tu PC"](#windows-protegió-tu-pc): es normal.

**Requisitos:** Windows 10 o superior con WebView2 Runtime, que ya viene preinstalado en
las versiones modernas de Windows 10 y 11.

> **¿Tenías Apostrophe para Windows (0.3.0 o anterior)?** Desinstálalo antes de instalar
> Elision: por el cambio de nombre, Elision se instala como una aplicación aparte y no lo
> reemplaza. Tus preferencias y recientes se conservan.
>
> ```powershell
> winget uninstall GerardoFC8.ApostropheForWindows
> winget install elision
> ```

`elision` es el nombre corto del paquete. Su identificador completo, que también sirve en
todos estos comandos, es `GerardoFC8.ApostropheForWindows`: se conserva del nombre anterior
de la app para que quien ya la tenía siga recibiendo las actualizaciones.

## Qué puedes hacer

**Escribir**

- Varios documentos abiertos a la vez, cada uno en su [pestaña](#pestañas).
- Temas claro, oscuro y sepia.
- **Zoom del texto** con `Ctrl +` / `Ctrl -` / `Ctrl 0` o `Ctrl` + rueda del mouse.
- **Esquema de títulos** en un panel lateral (`Ctrl+Shift+O`): un clic te lleva a la sección.
- **Corrector ortográfico en español**, con diccionario personal: clic derecho sobre una
  palabra subrayada → **Agregar al diccionario**.
- **Imágenes:** pégalas con `Ctrl+V` (se guardan en la carpeta `assets` junto al documento)
  o arrástralas a la ventana. Arrastrar un `.md` lo abre en una pestaña.

**Ver el resultado**

- **Vista previa en vivo** mientras escribes.
- **Scroll sincronizado:** la vista previa acompaña al editor, título por título.
- **Diagramas Mermaid:** un bloque ` ```mermaid ` se dibuja como diagrama, sin conexión a
  internet.
- **Vista previa en una ventana aparte**, con opacidad del fondo y opción de dejarla
  siempre visible (los controles aparecen al pasar el mouse por la ventana).

**Exportar**

- A **PDF, HTML, ODT, DOCX, EPUB y LaTeX** con `Ctrl+E`. El formato se elige en el
  desplegable "Guardar como tipo" del diálogo.
- Los motores de conversión (pandoc y typst) vienen incluidos: no necesitas instalar nada
  más.

**Automatizar**

- El comando [`elision`](#cli-para-agentes-elision) permite exportar, contar palabras,
  revisar la ortografía y alinear tablas desde la terminal, pensado para scripts y agentes
  de IA. `elision view` muestra un documento con formato en la terminal.

## Atajos de teclado

| Atajo | Acción |
|---|---|
| `Ctrl+O` | Abrir un archivo |
| `Ctrl+T` o `Ctrl+N` | Pestaña nueva con un documento sin título |
| `Ctrl+W` | Cerrar la pestaña activa |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Pestaña siguiente / anterior |
| `Ctrl+E` | Exportar |
| `Ctrl+Shift+O` | Mostrar u ocultar el esquema de títulos |
| `Ctrl+Shift+H` | Modo Hemingway |
| `Ctrl +` / `Ctrl -` / `Ctrl 0` | Acercar, alejar y restablecer el zoom del texto |
| `Ctrl+V` | Pegar una imagen del portapapeles |

> **Cambio de atajo:** el modo Hemingway pasó de `Ctrl+T` a **`Ctrl+Shift+H`**. `Ctrl+T`
> ahora abre una pestaña nueva, como en cualquier programa con pestañas.

## Pestañas

Cada documento Markdown se abre en su propia pestaña, y puedes tener varios abiertos a la vez.

**Barra de pestañas**

- **●** marca los documentos con cambios sin guardar.
- Al pasar el mouse sobre una pestaña se ve la ruta completa del archivo.
- Para cerrar una pestaña: la **×**, el clic central, `Ctrl+W` o `Supr` con la pestaña
  enfocada. Si tiene cambios, se pide confirmación.
- Con muchas pestañas, la barra se desplaza en horizontal (también con la rueda del mouse).

**Abrir archivos**

- `Ctrl+O`, los recientes, el doble clic sobre un `.md` y `elision open a.md b.md` abren cada
  archivo en su propia pestaña.
- Si el archivo ya está abierto, solo se activa su pestaña.
- La pestaña inicial sin título y vacía se reemplaza por el archivo que abres.

**Qué guarda cada pestaña y qué es común**

| Por pestaña | Común a todas |
|---|---|
| Historial de deshacer, cursor y desplazamiento | Modos de escritura: enfoque, Hemingway y vista |

La vista previa (también la ventana desacoplada) y las estadísticas siguen a la pestaña activa.

**Cierre y recuperación**

- Al cerrar la ventana con varios documentos sin guardar, se pregunta **una sola vez** e
  indica cuántos tienen cambios.
- Tras un cierre inesperado, cada documento se recupera en su propia pestaña, también si
  abres la app con un archivo.
- "Guardar como" sobre un archivo que ya está abierto en otra pestaña se rechaza con un aviso:
  cierra esa pestaña o elige otro nombre.

## CLI para agentes (`elision`)

El instalador también deja en el PATH el comando **`elision`**: una línea de comandos para
agentes de IA y scripts. Usa los mismos pandoc, typst y diccionario que la app, nunca pide
confirmación y su salida es estable.

```powershell
elision --help
elision stats notas.md --json
elision export notas.md -o notas.pdf
```

El contrato son los **códigos de salida y de error**; los mensajes van en inglés y solo
sirven de diagnóstico.

> ¿La terminal dice que no reconoce `elision` justo después de instalar? Mira
> ["No se reconoce el comando `elision`"](#no-se-reconoce-el-comando-elision).

### Comandos

| Comando | Qué hace | Termina con 1 si… |
|---|---|---|
| `export <in> -o <out> [--to <fmt>]` | Convierte a PDF, HTML, ODT, DOCX, EPUB o LaTeX. El formato sale de la extensión de `-o`, o de `--to`. Sobrescribe sin preguntar. | — |
| `stats <in>` | Caracteres, palabras, oraciones, párrafos y tiempo de lectura. | — |
| `tables <in> [--check \| --write]` | Revisa (por defecto) o reescribe la alineación de todas las tablas. | hay tablas desalineadas (solo `--check`) |
| `spell <in> [--dict-dir <dir>]` | Ortografía en español, con línea y columna (base 1). | hay errores |
| `open <archivos>...` | Abre cada archivo en su propia pestaña del editor, sin esperar a que se cierre. | — |
| `view <in> [--width <n>] [--no-color]` | Muestra el documento con formato en la terminal, ajustado a su ancho: títulos, énfasis, listas, citas, código y tablas. Con colores solo si la salida es una terminal (y sin `NO_COLOR`). | — |

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
- **Sin firma:** `elision.exe` tampoco está firmado; ver
  ["El antivirus bloquea `elision.exe`"](#el-antivirus-bloquea-elisionexe).

En desarrollo, sin instalar: `cargo run -p apostrophe --bin elision -- --help`. La
arquitectura está en [`docs/adr/0002-cli-adaptador-primario.md`](docs/adr/0002-cli-adaptador-primario.md).

## Solución de problemas

### "Windows protegió tu PC"

Al ejecutar el instalador descargado, Windows SmartScreen muestra una pantalla azul:
**"Windows protegió tu PC — Editor desconocido"**. Es **esperado e inofensivo**: solo
significa que Windows no reconoce al editor del binario, no que la app sea insegura. Para
continuar:

1. Clic en **"Más información"**.
2. Clic en **"Ejecutar de todas formas"**.

La app se instala y funciona con normalidad.

El instalador **no está firmado**, y es una decisión deliberada: este es un proyecto libre
y personal, no comercial. Un certificado de firma de código cuesta ~$200-400/año y no hace
la app más segura ni mejor; solo le quita un clic de fricción a quien la descarga. Así se
distribuyen miles de aplicaciones de código abierto para Windows.

### No se reconoce el comando `elision`

Después de instalar, **cierra por completo** Windows Terminal, VS Code o el programa desde
el que abres la terminal, y vuelve a abrirlo. Una pestaña nueva no basta: hereda el PATH
que tenía ese programa al arrancar, así que ni ella ni los agentes que corran en ella
encuentran `elision`.

Si no quieres cerrarlo, recarga el PATH en la pestaña actual:

```powershell
$env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')
```

### El antivirus bloquea `elision.exe`

`elision.exe` tampoco está firmado. Algunos antivirus desconfían de ejecutables nuevos sin
firma y pueden bloquearlo o ponerlo en cuarentena; si pasa, agrega una excepción para la
carpeta de instalación.

## Desarrollo

Elision está construido con **Tauri 2 + Rust** bajo una arquitectura hexagonal (ver
[`docs/adr/0001-frontera-dominio-presentacion.md`](docs/adr/0001-frontera-dominio-presentacion.md)).
El editor es CodeMirror 6, la vista previa y la exportación usan **pandoc** y **typst**
como sidecars incluidos, y el corrector es Hunspell vía `spellbook`, en Rust puro.

### Requisitos

- [Rust](https://rustup.rs/) (toolchain MSVC en Windows: `x86_64-pc-windows-msvc`).
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/) (este proyecto usa **pnpm**, no npm).
- En Windows: [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
  (preinstalado en Windows 10/11 modernos).

### Puesta en marcha

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

### Build de release (instalador Windows)

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
   target/release/bundle/nsis/Elision_<versión>_x64-setup.exe   # NSIS
   target/release/bundle/msi/Elision_<versión>_x64_en-US.msi     # MSI (WiX)
   ```

#### Configuración del instalador

Definida en `src-tauri/tauri.conf.json` (`bundle`):

- **`installMode: "currentUser"`** — instala por usuario, sin pedir permisos de
  administrador (UAC). Para distribución por-máquina, cambiar a `"perMachine"`.
- **Idiomas del instalador**: español (con selector de idioma).
- **Asociación de archivos `.md` / `.markdown`** — declarada en el instalador. El doble
  clic abre el archivo en una pestaña; si la app ya está abierta, `tauri-plugin-single-instance`
  le reenvía la ruta en lugar de abrir otra ventana.
- **`elision` en el PATH** — el instalador NSIS agrega la carpeta de instalación al final
  del `Path` de tu usuario (`src-tauri/windows/hooks.nsh`, que ejecuta `elision-path.ps1`);
  el MSI, al final del `Path` del sistema (`src-tauri/windows/cli-path.wxs`). Al desinstalar
  se quita solo esa entrada, y reinstalar no la duplica. Las pruebas del script corren en la
  CI; a mano:
  `powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri/windows/elision-path.tests.ps1`.

> Para eliminar la advertencia de SmartScreen (por ejemplo, para una distribución masiva)
> haría falta un certificado OV/EV: ver
> [firma de código en Tauri](https://v2.tauri.app/distribute/sign/windows/).

## Sobre el nombre

Una *elisión* es la omisión de un sonido o una letra al unir dos palabras (*do not* →
*don't*, *le arbre* → *l'arbre*), y en la escritura se marca con un **apóstrofo**. Es
decir, el signo que da nombre al Apostrophe original existe para marcar una elisión. De ahí
el nombre de la app; y la CLI `elision` es además el editor sin su interfaz: lo que queda
al recortarlo.

## Licencia

Este proyecto **deriva** del código de Apostrophe (GNOME), licenciado
**GPL-3.0-or-later**, por lo que se distribuye bajo la misma licencia.
Los diccionarios Hunspell (rla-es vía `wooorm/dictionaries`) se incluyen bajo
MPL 1.1 (ver `src-tauri/dictionaries/es.license.txt`).
