[Español](./README.md) | **English**

# Apostrophe for Windows

A native, minimal, distraction-free Markdown editor for Windows.
A native port of GNOME's [Apostrophe](https://gitlab.gnome.org/World/apostrophe), built
with **Tauri 2 + Rust** on a hexagonal architecture
(see [`docs/adr/0001-frontera-dominio-presentacion.md`](docs/adr/0001-frontera-dominio-presentacion.md)).

> The application interface is in Spanish. This document is the English translation of
> the project README.

## Features

- CodeMirror 6 editor with a custom Fluent title bar.
- Live preview via **pandoc** (bundled as a sidecar — no system dependencies).
- Open and save `.md` through native dialogs.
- Light, dark and sepia themes.
- Export to **PDF, HTML, ODT, DOCX, EPUB and LaTeX** (Ctrl+E) — pick the format from the
  dialog's "Save as type" dropdown. The bundled sidecars (pandoc and typst) handle
  everything: nothing else to install.
- Spanish spell checking (Hunspell via `spellbook`, pure Rust).

## Development requirements

- [Rust](https://rustup.rs/) (MSVC toolchain on Windows: `x86_64-pc-windows-msvc`).
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/) (this project uses **pnpm**,
  not npm).
- On Windows: [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
  (preinstalled on modern Windows 10/11).

## Getting started (dev)

The large binaries (pandoc ~150 MB, typst ~21 MB) and the Hunspell dictionaries **are not
committed**: they are gitignored and provisioned by scripts. Before your first
`dev`/`build`, run the provisioning scripts from PowerShell:

```powershell
pwsh scripts/fetch-pandoc.ps1        # downloads pandoc-x86_64-pc-windows-msvc.exe (sidecar)
pwsh scripts/fetch-typst.ps1         # downloads typst-x86_64-pc-windows-msvc.exe (PDF engine)
pwsh scripts/fetch-dictionaries.ps1  # downloads the Hunspell dictionaries (es, en)
```

> The pandoc pin must be **3.9 or newer**. The typst template in 3.7 emitted an empty font
> list that typst 0.15 rejects, which broke PDF export.

Then run dev with hot reload:

```powershell
pnpm install
pnpm tauri dev
```

## Release build (Windows installer)

> ⚠️ The **MSI installer must be built on Windows** (WiX only runs on Windows).
> The NSIS installer (`setup.exe`) is produced in the same step.

1. Make sure you have run the provisioning scripts (pandoc, typst and dictionaries).
2. Build the release bundle:

   ```powershell
   pnpm tauri build
   ```

3. The installers land in (the workspace keeps `target` at the root, not under
   `src-tauri/`):

   ```
   target/release/bundle/nsis/Apostrophe_<version>_x64-setup.exe   # NSIS
   target/release/bundle/msi/Apostrophe_<version>_x64_en-US.msi     # MSI (WiX)
   ```

### Installer configuration

Defined in `src-tauri/tauri.conf.json` (`bundle`):

- **`installMode: "currentUser"`** — installs per user, without asking for administrator
  rights (UAC). For per-machine distribution, change it to `"perMachine"`.
- **Installer languages**: Spanish (with a language selector).
- **`.md` / `.markdown` file association** — declared in the installer.

### "Windows protected your PC" — this is normal

The installer is **not signed**, and that is a deliberate choice: this is a free, personal
project, not a commercial one. A code-signing certificate costs roughly $200–400 a year
and does not make the app any safer or better — it only removes one click of friction for
whoever downloads it. Not worth it for a hobby.

During installation, Windows SmartScreen will show a blue screen: **"Windows protected
your PC — Unknown publisher"**. This is **expected and completely harmless**: it only
means Windows does not recognise the binary's publisher, not that the app is unsafe. To
continue:

1. Click **"More info"**.
2. Click **"Run anyway"**.

The app installs and runs normally. This is how thousands of open-source Windows
applications are distributed.

> If you ever wanted to remove the warning (for mass distribution, say), you would need an
> OV/EV certificate — see
> [code signing in Tauri](https://v2.tauri.app/distribute/sign/windows/).

## License

This project **derives** from the Apostrophe (GNOME) codebase, licensed
**GPL-3.0-or-later**, and is therefore distributed under the same license.
The Hunspell dictionaries (rla-es via `wooorm/dictionaries`) are included under
MPL 1.1 (see `src-tauri/dictionaries/es.license.txt`).
