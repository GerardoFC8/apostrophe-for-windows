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
