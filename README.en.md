[Español](./README.md) | **English**

<img src="src-tauri/icons/logo.svg" alt="Elision logo" width="96">

# Elision

**A simple, distraction-free Markdown editor for Windows.** You write on the left, see the
result on the right, and export to PDF, Word and other formats without installing anything
else.

Elision (formerly *Apostrophe for Windows*) is a native Windows version of GNOME's
[Apostrophe](https://gitlab.gnome.org/World/apostrophe) editor. It is free and open source.

[Installation](#installation) · [What you can do](#what-you-can-do) ·
[Shortcuts](#keyboard-shortcuts) · [Tabs](#tabs) ·
[Command line](#cli-for-agents-elision) ·
[Troubleshooting](#troubleshooting) · [Development](#development)

## Installation

Open PowerShell or the Windows terminal and run:

```powershell
winget install elision
```

That's it. No administrator rights needed: the app is installed for your user only.
[winget](https://learn.microsoft.com/windows/package-manager/winget/) is the package
manager that ships with Windows 10 and 11.

To upgrade or uninstall:

```powershell
winget upgrade elision
winget uninstall elision
```

**Prefer to download the installer?** Get `Elision_<version>_x64-setup.exe` from
[Releases](https://github.com/GerardoFC8/apostrophe-for-windows/releases) and run it.
The newest version is always there; the winget one may take a few days to catch up.
Windows will show the
["Windows protected your PC"](#windows-protected-your-pc) notice: this is normal.

**Requirements:** Windows 10 or later with the WebView2 Runtime, which is preinstalled on
modern Windows 10 and 11.

> **Did you have Apostrophe for Windows (0.3.0 or earlier)?** Uninstall it before installing
> Elision: because of the rename, Elision installs as a separate app and does not replace it.
> Your preferences and recent files are kept.
>
> ```powershell
> winget uninstall GerardoFC8.ApostropheForWindows
> winget install elision
> ```

`elision` is the package's short name. Its full identifier, which also works in all these
commands, is `GerardoFC8.ApostropheForWindows`: it is kept from the app's former name so that
existing users keep getting updates.

## What you can do

**Write**

- Several documents open at once, each in its own [tab](#tabs).
- Light, dark and sepia themes.
- **Text zoom** with `Ctrl +` / `Ctrl -` / `Ctrl 0` or `Ctrl` + mouse wheel.
- **Outline** in a side panel (`Ctrl+Shift+O`): click to jump to a section.
- **Spanish spell checking**, with a personal dictionary: right-click an underlined word →
  **Add to dictionary**.
- **Images:** paste them with `Ctrl+V` (saved in an `assets` folder next to the document) or
  drag them onto the window. Dragging a `.md` opens it in a tab.

**See the result**

- **Live preview** as you type.
- **Synced scrolling:** the preview follows the editor, heading by heading.
- **Mermaid diagrams:** a ` ```mermaid ` block is drawn as a diagram, with no internet
  connection.
- **Preview in a separate window**, with background opacity and an option to keep it always
  on top (the controls appear when the mouse is over the window).

**Export**

- To **PDF, HTML, ODT, DOCX, EPUB and LaTeX** with `Ctrl+E`. Pick the format from the
  dialog's "Save as type" dropdown.
- The conversion engines (pandoc and typst) are bundled: there is nothing else to install.

**Automate**

- The [`elision`](#cli-for-agents-elision) command lets you export, count words, check
  spelling and align tables from the terminal, built for scripts and AI agents.
  `elision view` shows a formatted document in the terminal.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+O` | Open a file |
| `Ctrl+T` or `Ctrl+N` | New tab with an untitled document |
| `Ctrl+W` | Close the active tab |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+E` | Export |
| `Ctrl+Shift+O` | Show or hide the outline |
| `Ctrl+Shift+H` | Hemingway mode |
| `Ctrl +` / `Ctrl -` / `Ctrl 0` | Zoom the text in, out and back to default |
| `Ctrl+V` | Paste an image from the clipboard |

> **Shortcut change:** Hemingway mode moved from `Ctrl+T` to **`Ctrl+Shift+H`**. `Ctrl+T`
> now opens a new tab, as in any tabbed program.

## Tabs

Each Markdown document opens in its own tab, and you can keep several open at once.

**Tab bar**

- **●** marks documents with unsaved changes.
- Hovering over a tab shows the file's full path.
- To close a tab: the **×**, a middle click, `Ctrl+W`, or `Delete` with the tab focused. If
  it has changes, you are asked to confirm.
- With many tabs, the bar scrolls horizontally (the mouse wheel works too).

**Opening files**

- `Ctrl+O`, recent files, double-clicking a `.md`, and `elision open a.md b.md` open each
  file in its own tab.
- If the file is already open, its tab is simply activated.
- The initial empty untitled tab is replaced by the file you open.

**What each tab keeps and what is shared**

| Per tab | Shared by all tabs |
|---|---|
| Undo history, cursor and scroll position | Writing modes: focus, Hemingway and view |

The preview (including the detached window) and the statistics follow the active tab.

**Closing and recovery**

- Closing the window with several unsaved documents asks **only once** and says how many
  have changes.
- After a crash, every document is recovered in its own tab, even when you launch the app by
  opening a file.
- "Save as" onto a file that is already open in another tab is refused with a notice: close
  that tab or choose another name.

## CLI for agents (`elision`)

The installer also puts the **`elision`** command on the PATH: a CLI for AI agents and
scripts. It uses the same pandoc, typst and dictionary as the app, never asks for
confirmation, and its output is stable.

```powershell
elision --help
elision stats notes.md --json
elision export notes.md -o notes.pdf
```

The contract is the **exit codes and error codes**; messages are in English and are only
diagnostics.

> Does the terminal say it does not recognize `elision` right after installing? See
> ["The `elision` command is not recognized"](#the-elision-command-is-not-recognized).

### Commands

| Command | What it does | Exits with 1 when… |
|---|---|---|
| `export <in> -o <out> [--to <fmt>]` | Converts to PDF, HTML, ODT, DOCX, EPUB or LaTeX. The format comes from the extension of `-o`, or from `--to`. Overwrites without asking. | — |
| `stats <in>` | Characters, words, sentences, paragraphs and reading time. | — |
| `tables <in> [--check \| --write]` | Checks (the default) or rewrites the alignment of every table. | there are misaligned tables (`--check` only) |
| `spell <in> [--dict-dir <dir>]` | Spanish spelling, with line and column (1-based). | there are misspellings |
| `open <files>...` | Opens each file in its own editor tab, without waiting for the editor to close. | — |
| `view <in> [--width <n>] [--no-color]` | Shows the document formatted in the terminal, wrapped to its width: headings, emphasis, lists, quotes, code and tables. Colors only when the output is a terminal (and `NO_COLOR` is not set). | — |

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
- **Unsigned:** `elision.exe` is not signed either; see
  ["Antivirus blocks `elision.exe`"](#antivirus-blocks-elisionexe).

During development, without installing: `cargo run -p apostrophe --bin elision -- --help`.
The architecture is described in
[`docs/adr/0002-cli-adaptador-primario.md`](docs/adr/0002-cli-adaptador-primario.md).

## Troubleshooting

### "Windows protected your PC"

When you run the downloaded installer, Windows SmartScreen shows a blue screen: **"Windows
protected your PC — Unknown publisher"**. This is **expected and completely harmless**: it
only means Windows does not recognise the binary's publisher, not that the app is unsafe.
To continue:

1. Click **"More info"**.
2. Click **"Run anyway"**.

The app installs and runs normally.

The installer is **not signed**, and that is a deliberate choice: this is a free, personal
project, not a commercial one. A code-signing certificate costs roughly $200–400 a year
and does not make the app any safer or better — it only removes one click of friction for
whoever downloads it. This is how thousands of open-source Windows applications are
distributed.

### The `elision` command is not recognized

After installing, **fully close** Windows Terminal, VS Code or whatever program you open
your terminal from, and start it again. A new tab is not enough: it inherits the PATH that
program had when it started, so neither the tab nor the agents running in it will find
`elision`.

If you would rather not close it, reload the PATH in the current tab:

```powershell
$env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')
```

### Antivirus blocks `elision.exe`

`elision.exe` is not signed either. Some antivirus products distrust new unsigned
executables and may block or quarantine it; if that happens, add an exception for the
installation folder.

## Development

Elision is built with **Tauri 2 + Rust** on a hexagonal architecture (see
[`docs/adr/0001-frontera-dominio-presentacion.md`](docs/adr/0001-frontera-dominio-presentacion.md)).
The editor is CodeMirror 6, the preview and export use **pandoc** and **typst** as bundled
sidecars, and the spell checker is Hunspell via `spellbook`, in pure Rust.

### Requirements

- [Rust](https://rustup.rs/) (MSVC toolchain on Windows: `x86_64-pc-windows-msvc`).
- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/) (this project uses **pnpm**,
  not npm).
- On Windows: [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
  (preinstalled on modern Windows 10/11).

### Getting started

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

### Release build (Windows installer)

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
   target/release/bundle/nsis/Elision_<version>_x64-setup.exe   # NSIS
   target/release/bundle/msi/Elision_<version>_x64_en-US.msi     # MSI (WiX)
   ```

#### Installer configuration

Defined in `src-tauri/tauri.conf.json` (`bundle`):

- **`installMode: "currentUser"`** — installs per user, without asking for administrator
  rights (UAC). For per-machine distribution, change it to `"perMachine"`.
- **Installer languages**: Spanish (with a language selector).
- **`.md` / `.markdown` file association** — declared in the installer. Double-clicking
  opens the file in a tab; if the app is already open, `tauri-plugin-single-instance`
  forwards the path to it instead of opening another window.
- **`elision` on the PATH** — the NSIS installer appends the installation folder to your
  user `Path` (`src-tauri/windows/hooks.nsh`, which runs `elision-path.ps1`); the MSI
  appends it to the system `Path` (`src-tauri/windows/cli-path.wxs`). Uninstalling removes
  only that entry, and reinstalling does not duplicate it. The script's tests run in CI; by
  hand:
  `powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri/windows/elision-path.tests.ps1`.

> If you ever wanted to remove the SmartScreen warning (for mass distribution, say), you
> would need an OV/EV certificate — see
> [code signing in Tauri](https://v2.tauri.app/distribute/sign/windows/).

## About the name

An *elision* is the omission of a sound or letter when two words are joined (*do not* →
*don't*, *le arbre* → *l'arbre*), and in writing it is marked with an **apostrophe**. In
other words, the mark the original Apostrophe is named after exists to signal an elision.
Hence the app's name; and the `elision` CLI is also the editor without its interface: what
is left once it is cut down.

## License

This project **derives** from the Apostrophe (GNOME) codebase, licensed
**GPL-3.0-or-later**, and is therefore distributed under the same license.
The Hunspell dictionaries (rla-es via `wooorm/dictionaries`) are included under
MPL 1.1 (see `src-tauri/dictionaries/es.license.txt`).
