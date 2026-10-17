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
- `elision` CLI for AI agents and scripts (see [CLI for agents](#cli-for-agents-elision)).

## Installation

The easiest way is with [winget](https://learn.microsoft.com/windows/package-manager/winget/),
the package manager that ships with Windows 10 and 11. From PowerShell or the terminal:

```powershell
winget install GerardoFC8.ApostropheForWindows
```

No administrator rights needed: the app is installed for your user only.

To upgrade or uninstall:

```powershell
winget upgrade GerardoFC8.ApostropheForWindows
winget uninstall GerardoFC8.ApostropheForWindows
```

> The winget version may take a few days to catch up with the latest release. For the
> newest one, download `Apostrophe_<version>_x64-setup.exe` from
> [Releases](https://github.com/GerardoFC8/apostrophe-for-windows/releases) and run it
> (see ["Windows protected your PC"](#windows-protected-your-pc--this-is-normal)).

## CLI for agents (`elision`)

The installer also puts the **`elision`** command on the PATH: a CLI for AI agents and
scripts. It uses the same pandoc, typst and dictionary as the app, never asks for
confirmation, and its output is stable. The contract is the **exit codes and error codes**;
messages are in English and are only diagnostics.

> After installing, open a **new** terminal. Terminals that were already open (and the
> agents running in them) keep the old PATH and will not find `elision`.

```powershell
elision --help
elision stats notes.md --json
elision export notes.md -o notes.pdf
```

### Commands

| Command | What it does | Exits with 1 when… |
|---|---|---|
| `export <in> -o <out> [--to <fmt>]` | Converts to PDF, HTML, ODT, DOCX, EPUB or LaTeX. The format comes from the extension of `-o`, or from `--to`. Overwrites without asking. | — |
| `stats <in>` | Characters, words, sentences, paragraphs and reading time. | — |
| `tables <in> [--check \| --write]` | Checks (the default) or rewrites the alignment of every table. | there are misaligned tables (`--check` only) |
| `spell <in> [--dict-dir <dir>]` | Spanish spelling, with line and column (1-based). | there are misspellings |
| `open <files>...` | Opens the files in the editor without waiting for it to close. | — |

- **Standard input:** `<in>` can be `-` (except with `tables --write`). It is decoded like a
  file: UTF-8 or any other encoding the app can open. With `-`, `export` has no document
  folder and leaves relative images out.
- **`--json`**, before or after the subcommand: a single JSON line on stdout, ASCII only
  (everything else is written as `\uXXXX`), errors included. Without `--json`, stdout carries
  only data and stderr carries diagnostics.
- **Dictionary:** `--dict-dir`; otherwise the `ELISION_DICT_DIR` variable; otherwise the one
  that ships with the installation. The first one given is used without falling back to the
  next: if it fails, the result is `dictionary_missing`.
- Relative paths are resolved against the current folder, and the document's images against
  the document's folder: you can export from anywhere.

### Exit codes

| Exit | Meaning | `error.code` (JSON) |
|---|---|---|
| 0 | Success | — |
| 1 | Findings: misspellings or misaligned tables | — (`"ok": true`) |
| 2 | Invalid usage: arguments, unknown format | `usage` |
| 3 | pandoc, typst, the editor or the dictionary is missing | `sidecar_missing`, `dictionary_missing` |
| 4 | pandoc or typst failed to convert | `conversion_failed` |
| 5 | I/O error: missing file, output not writable | `io_error` |

An error with `--json`:

```json
{"ok":false,"error":{"code":"usage","message":"cannot tell the export format from x.xyz; use --to with one of: html, pdf, odt, docx, epub, latex"}}
```

### Examples for an agent

```powershell
# Spelling: read the JSON, not the text.
elision spell chapter.md --json
# {"ok":true,"file":"C:\\notes\\chapter.md","language":"es","misspellings":[{"line":7,"column":6,"word":"mundoo","suggestions":["mundo","mundos","mundo o","mundano"]}]}

# Tables: rewrite only when --check found something (exit code 1).
elision tables chapter.md --check --json
if ($LASTEXITCODE -eq 1) { elision tables chapter.md --write }

# Export from another folder; relative images are resolved against the document.
elision export docs\chapter.md -o C:\out\chapter.pdf --json
# {"ok":true,"output":"C:\\out\\chapter.pdf","format":"pdf"}
```

### Known limitations

- **Accents in PowerShell pipes.** Windows PowerShell 5.1 sends text to other programs using
  `$OutputEncoding`, which is ASCII by default: `Get-Content x.md | elision stats -` turns
  accented letters into `?`. PowerShell also decodes text output with the console code page.
  Pass the file path instead of `-`, read results with `--json` (ASCII only) and export with
  `-o`.
- **PATH precedence.** The installation folder also contains `pandoc.exe` and `typst.exe`,
  and it is added to the **end** of the PATH. With the NSIS installer (the one winget uses),
  any pandoc or typst already earlier on the PATH wins. With the MSI the folder goes to the
  system `Path`, which Windows puts before the user one: the bundled copies shadow the ones
  you installed for your user only. `elision` always uses its own.
- **`tables --write` is not atomic:** it rewrites the file in place, like the app's save. If
  the file is open in the app, the app shows its external-change notice.
- **Skipped tables:** `tables` reports, without fixing them, the tables it cannot align
  (escaped pipe `\|`, rows with extra cells, mixed indentation, or a table inside a quote).
  They are listed in the JSON `skipped` field (`escaped_pipe`, `extra_cells`, `mixed_indent`,
  `blockquote`) and as a warning on stderr; they do not change the exit code.
- **HTML without a theme:** HTML export uses pandoc's default style, not the app's theme.
- **`open` with several files:** it passes them all, but for now the editor opens only the
  first one (it warns on stderr).
- **Unsigned:** `elision.exe` is not signed either. Some antivirus products distrust new
  unsigned executables and may block or quarantine it; if that happens, add an exception for
  the installation folder.

During development, without installing: `cargo run -p apostrophe --bin elision -- --help`.
The architecture is described in
[`docs/adr/0002-cli-adaptador-primario.md`](docs/adr/0002-cli-adaptador-primario.md).

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
- **`elision` on the PATH** — the NSIS installer appends the installation folder to your
  user `Path` (`src-tauri/windows/hooks.nsh`, which runs `elision-path.ps1`); the MSI
  appends it to the system `Path` (`src-tauri/windows/cli-path.wxs`). Uninstalling removes
  only that entry, and reinstalling does not duplicate it. The script's tests run in CI; by
  hand:
  `powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri/windows/elision-path.tests.ps1`.

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
