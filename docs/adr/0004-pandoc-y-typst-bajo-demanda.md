# ADR-0004 — Pandoc y typst bajo demanda

- **Estado:** Pospuesto
- **Fecha:** 2026-11-15
- **Relación:** sigue a [ADR-0003](0003-vista-previa-con-comrak.md) (la vista previa ya no usa
  pandoc). Si se retoma, modificaría la regla 5 de
  [ADR-0002](0002-cli-adaptador-primario.md) (dónde busca la CLI sus recursos).

---

## Por qué se pospone

Pandoc y typst **siguen incluidos en los dos instaladores**. El análisis de abajo queda como
propuesta lista para cuando haga falta.

- **El problema de fondo ya se resolvió por otro lado.** Pandoc pesaba sobre todo en la vista
  previa, que lo lanzaba en cada render. Desde ADR-0003 solo se usa al exportar, donde un
  segundo de espera no se nota.
- **45 MB es un tamaño normal** para una app de escritorio, y se descarga una vez.
- **El costo no compensa hoy:** descarga, verificación, avance, cancelación, diálogo, un
  subcomando de la CLI y dos configuraciones de instalador. Todo eso es código para mantener y
  probar, y el primer export pasaría a depender de internet, a cambio de 39 MB.

**Lo que sí se hizo:** los scripts de provisión (`scripts/fetch-pandoc.ps1` y
`scripts/fetch-typst.ps1`) ahora verifican el SHA-256 del zip oficial antes de extraerlo. Antes
no verificaban nada, y esos binarios terminan dentro del instalador que se publica.

**Cuándo retomarlo:**
- Si el tamaño del instalador genera quejas.
- Si se publica una versión portable o en Scoop, donde el tamaño pesa más.
- Si pandoc o typst crecen mucho en versiones futuras.

## Propuesta (no aplicada)

- El instalador NSIS, que es el que usa winget, **deja de incluir pandoc y typst**. Pasa de
  45,2 MB a 6,3 MB.
- La primera vez que se exporta a un formato que los necesita, la app **pregunta** si
  descargarlos y muestra el tamaño. Nunca descarga sin que el usuario lo pida.
- Se descargan de la **release oficial en GitHub**, en una versión fijada, y se comprueban con
  un **SHA-256 fijado en el código** antes de usarlos.
- Se guardan en `%LOCALAPPDATA%\Elision\tools\<herramienta>-<versión>\`: por usuario y sin
  permisos de administrador.
- La CLI **nunca descarga por su cuenta**: hay un subcomando explícito, `elision tools`.
- El MSI sigue incluyendo todo, para instalaciones sin internet.

## Contexto

Desde ADR-0003, la vista previa y el popover de fórmulas no usan pandoc ni typst. Solo hacen
falta para exportar:

| Formato | pandoc | typst |
|---|---|---|
| HTML, DOCX, ODT, EPUB, LaTeX | sí | no |
| PDF | sí (genera el fuente typst) | sí (lo compila) |

Quien solo escribe y lee no los usa nunca, pero hoy los descarga e instala igual.

### Medición (2026-11-15)

Instalador NSIS regenerado con el script que produce Tauri, quitando cada binario:

| Contenido | Instalador |
|---|---|
| Todo (como hoy) | 45,2 MB |
| Sin pandoc | 21,6 MB |
| Sin pandoc ni typst | **6,3 MB** |

Releases oficiales de las versiones fijadas hoy (`scripts/fetch-*.ps1`):

| Herramienta | Archivo | Descarga | Instalado | SHA-256 en GitHub | Firma del `.exe` |
|---|---|---|---|---|---|
| pandoc 3.9.0.2 | `pandoc-3.9.0.2-windows-x86_64.zip` | 39,3 MB | 231 MB | `c97542f2…5f272d3a` | Válida, SignPath Foundation |
| typst 0.15.1 | `typst-x86_64-pc-windows-msvc.zip` | 21,4 MB | 52 MB | `19ce3551…8913f6736` | Sin firma |

GitHub publica el SHA-256 de cada archivo de release en su API (`digest`). Los scripts de
provisión no verificaban ningún hash. Eso se corrigió aparte, sin esperar a esta decisión
(ver "Por qué se pospone").

## Piezas

### Resolución: dónde se busca cada herramienta

En este orden:

1. **Junto al ejecutable**, como hoy: el MSI, el desarrollo local y la CI siguen igual.
2. **En la carpeta de herramientas**, con la versión exacta fijada:
   `%LOCALAPPDATA%\Elision\tools\pandoc-3.9.0.2\pandoc.exe`.
3. Si no está en ninguna, **falta**.

Nunca se busca en el `PATH`: un pandoc anterior a 3.9 rompe el export a PDF (ver
`fetch-typst.ps1`), y la CLI tiene que usar siempre el mismo motor que la GUI (ADR-0002).

La resolución vive en `apostrophe-sidecars`, junto a `install_dir`. La GUI y la CLI ya reciben
una "fábrica de comandos" para exportar, así que el cambio queda acotado: la fábrica pasa a
preguntar la ruta a esta resolución.

### Descarga e instalación

En `apostrophe-sidecars`, con `ureq` (TLS nativo de Windows), `zip` (solo deflate) y
`sha2`. Suma unos **0,85 MB** al ejecutable (medido: 967 KB contra 120 KB de un programa
vacío con el mismo perfil).

1. Se descarga el `.zip` a un temporal, informando el avance.
2. Se calcula su SHA-256 y se compara con la constante del código. Si no coincide, se borra y
   se informa un error. El hash **no se descarga**: viene compilado en Elision, así que un
   servidor comprometido no puede cambiarlo.
3. Del zip se extrae **solo** el ejecutable esperado (`pandoc.exe`, `typst.exe`). Se busca por
   nombre: ninguna ruta del zip se usa para escribir en disco, así que no hay "zip slip".
4. Se extrae a `…\pandoc-3.9.0.2.partial\` y recién después se renombra a `pandoc-3.9.0.2\`.
   Una descarga cortada nunca queda como herramienta instalada.
5. Se borran las carpetas de versiones anteriores de la misma herramienta.

Cuando una versión nueva de Elision cambie el número de versión fijado, la herramienta se
descarga de nuevo en el siguiente export. La anterior se borra al instalar la nueva.

### GUI

- Al exportar a un formato que necesita una herramienta que falta, aparece un diálogo
  (patrón de CLAUDE.md: pregunta corta en el título y explicación en el cuerpo), por ejemplo:
  título "¿Descargar pandoc?", cuerpo "Para exportar a DOCX hace falta pandoc (39 MB). Se
  descarga una sola vez." Para PDF se piden los dos juntos (61 MB).
- Mientras descarga se muestra el avance y se puede cancelar.
- **Sin internet** o con el error de descarga, el mismo diálogo ofrece "Instalar desde un
  archivo…": se elige el `.zip` oficial bajado en otra máquina y pasa por la misma
  verificación del hash.
- Como en el resto de la app, la copia la arma el frontend (`src/i18n.ts`). El backend
  devuelve códigos estables: `tool_missing`, `download_failed`, `checksum_mismatch`, `cancelled`.

### CLI

- `elision export` con la herramienta ausente sigue saliendo con **código 3,
  `sidecar_missing`**: el contrato con los agentes no cambia. Lo único nuevo es que el mensaje
  sugiere `elision tools install`.
- Subcomando nuevo **`elision tools`**:
  - `elision tools` (o `tools status`) lista cada herramienta, su versión y si está, y dónde.
    Admite `--json`.
  - `elision tools install [pandoc|typst|all]` descarga y verifica. Un error de red es
    `io_error` (salida 5). Un hash distinto también sale con 5, pero con el código de error
    nuevo `checksum_mismatch`: agregar un código no rompe a quien ya los usa, cambiar uno sí.
- La CLI nunca descarga sin `tools install`: un agente o un script en CI no debe generar
  tráfico de red ni tardar un minuto sin pedirlo.
- La regla 5 de ADR-0002 ("recursos desde la propia carpeta") pasa a ser "desde la propia
  carpeta **o** la carpeta de herramientas del usuario, nunca el `PATH`".

### Instaladores

| Instalador | pandoc y typst | Para quién |
|---|---|---|
| NSIS (winget, por usuario) | Bajo demanda | La mayoría: 6,3 MB |
| MSI (por máquina) | Incluidos | Empresas, aulas, equipos sin internet |

- Tauri toma `bundle.externalBin` de `tauri.conf.json` para todos los formatos. Por eso la CI
  compila el NSIS con una configuración sin `externalBin` (`tauri build --config`) y el MSI con
  la actual.
- Al actualizar desde una versión anterior, el instalador NSIS borra el `pandoc.exe` y el
  `typst.exe` que dejó la instalación anterior (`windows/hooks.nsh`). Si no, quedarían
  versiones viejas que la resolución encontraría primero.
- Al desinstalar, el NSIS borra `%LOCALAPPDATA%\Elision\tools` del usuario. El MSI, que es
  por máquina, no puede borrar los datos de cada usuario: es una limitación aceptada, y con
  el MSI esa carpeta normalmente no existe porque las herramientas vienen incluidas.

### Provisión para desarrollo y CI

`fetch-pandoc.ps1` y `fetch-typst.ps1` ya verifican el SHA-256 (parámetro `-Sha256`). Si se
retoma esta propuesta, los hashes pasarían a un solo archivo de datos que lean los scripts y
que `apostrophe-sidecars` incluya al compilar (`include_str!`), para que no se desincronicen.

## Consecuencias

**Positivas**
- El instalador de winget baja de 45,2 MB a 6,3 MB, y lo instalado de unos 305 MB a unos
  20 MB, para todos los que no exportan.
- La primera instalación y cada actualización se bajan en segundos.
- Los binarios descargados en el equipo del usuario se verifican con un hash fijado.
- Con un zip portable liviano, publicar en Scoop pasa a ser razonable.

**Negativas / a vigilar**
- **El primer export necesita internet** (o el zip a mano) y tarda lo que tarde bajar 39 a
  61 MB.
- Hay más código: descarga, verificación, avance, cancelación, diálogo y subcomando de la CLI,
  y todo hay que probarlo.
- **Privacidad:** la descarga le muestra a GitHub la IP del usuario. Solo pasa cuando el
  usuario lo pide, y el diálogo dice de dónde se descarga.
- Dos instaladores con contenido distinto: la CI arma dos configuraciones.
- Si GitHub borra una release fijada, la descarga falla. Las releases de pandoc y typst se
  conservan desde hace años, y el MSI y el "Instalar desde un archivo…" no dependen de eso.

## Alternativas descartadas

- **Usar el pandoc del `PATH`** (o declarar pandoc como dependencia en el manifiesto de
  winget): no se controla la versión. Un pandoc anterior a 3.9 rompe el PDF, y la CLI dejaría
  de usar el mismo motor que la GUI.
- **Descargar con `curl.exe` y `tar.exe`**, que vienen con Windows 10 y 11: no suman nada al
  ejecutable, pero informar el avance, cancelar y distinguir errores dependería de leer la
  salida de otros programas. Además, un `tar` o un `curl` anterior en el `PATH` del usuario
  taparía al del sistema.
- **Bajar el hash junto con el archivo** (o leerlo de la API de GitHub en el momento): quien
  pudiera cambiar el archivo podría cambiar el hash. Fijado en el código, no.
- **Mantener un solo instalador completo:** es lo de hoy, 45 MB para todos.
- **Comprimir mejor:** el NSIS ya usa LZMA sólido; no hay margen que se acerque a esto.

## Preguntas abiertas

1. **¿El MSI sigue completo?** La propuesta es que sí. La alternativa es que también sea
   liviano y ofrecer aparte un zip "completo" para equipos sin internet.
2. **¿Exportar HTML con comrak?** El HTML exportado podría salir del mismo motor que la vista
   previa, con su hoja de estilos, y no necesitar pandoc. Pero cambia el HTML que reciben
   quienes exportan hoy. Merece su propia decisión.
3. **¿Verificar además la firma de pandoc?** El ejecutable de pandoc viene firmado por SignPath
   Foundation. Comprobar la firma suma una defensa si algún día el hash fijado estuviera mal
   copiado, pero typst no viene firmado, así que el hash sigue siendo la verificación principal.
