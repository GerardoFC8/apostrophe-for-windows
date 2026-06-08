# Apostrophe para Windows

Editor de Markdown nativo, minimalista y libre de distracciones para Windows.
Port nativo del editor [Apostrophe](https://gitlab.gnome.org/World/apostrophe) de
GNOME, construido con **Tauri 2 + Rust** bajo una arquitectura hexagonal
(ver [`docs/adr/0001-frontera-dominio-presentacion.md`](docs/adr/0001-frontera-dominio-presentacion.md)).

## Características

- Editor CodeMirror 6 con titlebar Fluent custom.
- Vista previa en vivo vía **pandoc** (bundleado como sidecar, sin dependencias del sistema).
- Abrir / guardar `.md` con diálogos nativos.
- Temas claro / oscuro / sepia + exportación a HTML.
- Corrector ortográfico en español (Hunspell vía `spellbook`, puro-Rust).

## Requisitos de desarrollo

- [Rust](https://rustup.rs/) (toolchain MSVC en Windows: `x86_64-pc-windows-msvc`).
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/) (este proyecto usa **pnpm**, no npm).
- En Windows: [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
  (preinstalado en Windows 10/11 modernos).

## Puesta en marcha (dev)

Los binarios pesados (pandoc ~150 MB) y los diccionarios Hunspell **no se commitean**:
están gitignored y se provisionan con scripts. Antes del primer `dev`/`build`, ejecutá
los scripts de provisioning desde PowerShell:

```powershell
pwsh scripts/fetch-pandoc.ps1        # baja pandoc-x86_64-pc-windows-msvc.exe (sidecar)
pwsh scripts/fetch-dictionaries.ps1  # baja los diccionarios Hunspell (es, en)
```

Luego, dev con hot-reload:

```powershell
pnpm install
pnpm tauri dev
```

## Build de release (instalador Windows)

> ⚠️ El instalador **MSI debe construirse en Windows** (WiX solo corre en Windows).
> El instalador NSIS (`setup.exe`) también se genera en el mismo paso.

1. Asegurate de haber corrido los scripts de provisioning (pandoc + diccionarios).
2. Compilá el bundle de release:

   ```powershell
   pnpm tauri build
   ```

3. Los instaladores quedan en:

   ```
   src-tauri/target/release/bundle/nsis/Apostrophe_<versión>_x64-setup.exe   # NSIS
   src-tauri/target/release/bundle/msi/Apostrophe_<versión>_x64_en-US.msi     # MSI (WiX)
   ```

### Configuración del instalador

Definida en `src-tauri/tauri.conf.json` (`bundle`):

- **`installMode: "currentUser"`** — instala por usuario, sin pedir permisos de
  administrador (UAC). Para distribución por-máquina, cambiar a `"perMachine"`.
- **Idiomas del instalador**: español (con selector de idioma).
- **Asociación de archivos `.md` / `.markdown`** — declarada en el instalador. El
  comportamiento de "abrir con doble-click" se implementa en un slice aparte
  (requiere `tauri-plugin-single-instance` y se prueba en Windows).

### Firma de código (diferida)

El instalador **no está firmado** todavía. Sin un certificado de firma de código,
Windows SmartScreen puede mostrar una advertencia ("Windows protegió tu PC") al
instalar — es esperable y no bloquea la instalación (clic en "Más información" →
"Ejecutar de todas formas"). Para distribución pública, configurar
[firma de código](https://v2.tauri.app/distribute/sign/windows/) con un certificado OV/EV.

## Licencia

Este proyecto **deriva** del código de Apostrophe (GNOME), licenciado
**GPL-3.0-or-later**, por lo que se distribuye bajo la misma licencia.
Los diccionarios Hunspell (rla-es vía `wooorm/dictionaries`) se incluyen bajo
MPL 1.1 (ver `src-tauri/dictionaries/es.license.txt`).
