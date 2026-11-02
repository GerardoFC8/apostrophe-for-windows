# ADR-0002 — La CLI como segundo adaptador primario

- **Estado:** Aceptado
- **Fecha:** 2026-10-17
- **Relación:** amplía [ADR-0001](0001-frontera-dominio-presentacion.md) (arquitectura hexagonal).

---

## Decisión

- La CLI **`elision`** es un **segundo adaptador primario**, al mismo nivel que Tauri. La
  usan agentes de IA y scripts para exportar, contar, alinear tablas, revisar la ortografía
  y abrir archivos en el editor, sin la GUI.
- Lo que la GUI y la CLI comparten para hablar con el mundo exterior (pandoc, typst,
  diccionarios, rutas de instalación, imágenes) vive en **`crates/apostrophe-sidecars`**: un
  **adaptador secundario compartido**, sin Tauri.
- El dominio (`crates/apostrophe-core`) sigue puro. Gana `align_tables`, que alinea todas las
  tablas de un documento.

```
apostrophe.exe ─> src-tauri (IPC, diálogos) ──┐
                                              ├─> apostrophe-sidecars ─> pandoc, typst, dictionaries/
elision.exe ───> apostrophe-cli (argv, JSON) ─┘
                 └─ ambos ─> apostrophe-core (stats, tablas, ortografía)
```

## Contexto

- Los agentes trabajan desde una terminal. Antes de este cambio todo pasaba por el IPC de
  Tauri, y la lógica de export (`export.rs`, imágenes, carga del diccionario) vivía en
  `src-tauri`: no se podía usar sin arrastrar Tauri y WebView2.
- La GUI se compila con el subsistema `windows`: no tiene consola, `cmd` no espera a que
  termine y su salida no llega a la terminal. La CLI necesita su propio ejecutable de consola.

## Piezas

| Pieza | Rol | Depende de |
|---|---|---|
| `crates/apostrophe-core` | Dominio puro | — |
| `crates/apostrophe-sidecars` | Adaptador secundario: `install_dir`, `sidecar_path_in`, `std_command` (con `CREATE_NO_WINDOW`), export con pandoc y typst (`ExportError`), imágenes, `dictionary::load_es` | core |
| `crates/apostrophe-cli` | Adaptador primario: argumentos (clap), salida en texto o JSON, códigos de salida. Entrada única: `run(args, env, io) -> i32` | core, sidecars |
| `src-tauri` | Adaptador primario de la GUI | core, sidecars |
| `src-tauri/src/bin/elision.rs` | Shim de consola: arma `Env` con el entorno real y llama a `run` | cli |

## Reglas de la frontera

1. **`apostrophe-cli` no depende de Tauri.** Se prueba en proceso: `run` con buffers y un
   `Env` falso (carpeta de instalación, cwd, diccionario, lanzador de la GUI).
2. **`run` no lee el estado global.** Ni variables de entorno ni la cwd: el shim las inyecta
   en `Env`. Así los tests corren en paralelo sin `set_var`.
3. **El contrato son los códigos, no los mensajes.** Códigos de salida 0–5 y códigos de error
   estables (`usage`, `sidecar_missing`, `dictionary_missing`, `conversion_failed`,
   `io_error`). Los mensajes van en inglés y la CLI los arma desde las variantes de
   `ExportError` y `DictionaryError`; no reutiliza su `Display` en español, que es de la GUI.
   El catálogo `src/i18n.ts` rige solo la GUI.
4. **JSON solo ASCII.** Todo carácter no ASCII sale como `\uXXXX`: la salida sobrevive a
   cualquier página de códigos de la consola.
5. **Recursos desde la propia carpeta.** La CLI busca pandoc, typst, `dictionaries/` y la GUI
   junto a su ejecutable (`install_dir`), nunca en la cwd ni en el PATH. Las rutas del
   usuario se resuelven contra la cwd y se normalizan antes de usarlas.
6. **Un adaptador más no toca el dominio.** Un futuro servidor MCP sería un tercer adaptador
   primario sobre los mismos `commands::*`.

El shim vive en `src-tauri/src/bin/` porque tauri-cli solo empaqueta los ejecutables de ese
paquete; `default-run = "apostrophe"` mantiene a la GUI como ejecutable principal. Así el
instalador deja `elision.exe` junto a la GUI, los sidecars y `dictionaries/`.

## Instalación y PATH

| Instalador | Dónde agrega la carpeta | Cómo |
|---|---|---|
| NSIS (por usuario; el de winget) | Al final del `Path` del usuario (HKCU) | `src-tauri/windows/hooks.nsh` ejecuta `elision-path.ps1` con Windows PowerShell |
| MSI (por máquina) | Al final del `Path` del sistema (HKLM) | `src-tauri/windows/cli-path.wxs`: `Environment Part="last" System="yes"` |

- Ambos quitan solo su entrada al desinstalar y no la duplican al reinstalar o actualizar.
- NSIS no edita el valor por su cuenta (sus cadenas se truncan a 1024 caracteres): lo hace
  el script, que conserva el tipo `REG_EXPAND_SZ` y las variables sin expandir. Se prueba
  en la CI con `elision-path.tests.ps1` sobre una clave temporal.
- **Precedencia (limitación aceptada).** La carpeta también contiene `pandoc.exe` y
  `typst.exe`. Al agregarse al final, con NSIS gana cualquier pandoc o typst que ya esté
  antes en el PATH. Con el MSI pasa lo contrario para lo instalado por usuario: Windows pone
  el `Path` del sistema antes que el del usuario, así que los incluidos tapan a esas copias.
  `elision` siempre usa los suyos. Una carpeta `bin/` aparte, solo con `elision.exe`,
  evitaría el problema en el futuro.

## Consecuencias

**Positivas**
- Los agentes usan el mismo motor que la GUI: mismos argumentos de pandoc, mismas reglas
  para las imágenes y el mismo diccionario.
- `apostrophe-sidecars` y `apostrophe-cli` se prueban sin Tauri.

**Negativas / a vigilar**
- Dos ejecutables comparten la carpeta. La GUI no puede llamarse `elision.exe` (Windows no
  distingue mayúsculas): su nombre se decide en el cambio de renombre.
- El instalador modifica el PATH: hay más superficie que limpiar al desinstalar.
- `cargo test --workspace` también compila la GUI: los e2e usan `CARGO_BIN_EXE_elision`.
- `tables --write` no es atómico, igual que el guardado de la GUI.

## Alternativas descartadas

- **La CLI dentro de la GUI (`apostrophe.exe --cli`):** el subsistema `windows` no tiene
  consola. Con trucos como `AttachConsole`, `cmd` ya volvió al prompt y la salida aparece
  mezclada con él.
- **El shim en otro paquete:** tauri-cli no lo empaquetaría.
- **Dejar el export en `src-tauri`:** la CLI arrastraría Tauri y no se podría probar en
  proceso.
- **Parser de argumentos propio:** sin ayuda por subcomando ni sugerencias, que es lo que
  leen los agentes. clap cuesta unos 0,5 MB.
- **`[Environment]::SetEnvironmentVariable` o `EnvVarUpdate` de NSIS para el PATH:** el
  primero expande `%USERPROFILE%` y guarda el valor como `REG_SZ`; el segundo trunca a 1024
  caracteres.
