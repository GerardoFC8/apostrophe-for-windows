# ADR-0003 — Vista previa con comrak en el proceso, pandoc solo para exportar

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Relación:** aplica [ADR-0001](0001-frontera-dominio-presentacion.md) (el cómputo va en
  Rust) y no cambia [ADR-0002](0002-cli-adaptador-primario.md) (el export de la CLI sigue
  con pandoc).

---

## Decisión

- La vista previa convierte Markdown a HTML **dentro del proceso**, con la biblioteca
  [comrak](https://crates.io/crates/comrak) (CommonMark + GFM, en Rust), en
  `apostrophe-core`. Deja de lanzar pandoc en cada render.
- **Pandoc queda solo para exportar** (HTML, DOCX, ODT, EPUB, LaTeX y el paso previo del
  PDF). El export de la GUI y el de la CLI no cambian.
- Las fórmulas de la vista previa se dibujan como **MathML**, sin internet. Hoy dependen
  de MathJax desde un CDN.
- El popover de fórmulas deja de usar pandoc y typst, y usa el mismo MathML.
- Que pandoc se descargue bajo demanda, en vez de venir en el instalador, es el paso
  siguiente y **no se decide aquí**: tiene su propio problema de seguridad (de dónde se
  baja, cómo se verifica) y merece su propio ADR.

## Contexto

Hoy cada render de la vista previa (`preview_markdown` en `src-tauri/src/lib.rs`):

1. escribe el documento en un archivo temporal,
2. lanza `pandoc.exe` (231 MB) con `--from markdown --to html5 --standalone --mathjax`,
3. lee su salida e incrusta las imágenes locales como data URI.

Es la causa de fondo del retraso con documentos largos, y pandoc es más de dos tercios del
instalador.

### Medición (2026-11-15, este equipo, 12 núcleos lógicos)

Pandoc 3.9.0.2 como lo lanza la app hoy; comrak 0.59.0 en un banco de prueba compilado en
release. Pandoc: mediana de 15 ejecuciones, contando el arranque del proceso. Comrak:
promedio de 200 renders.

| Documento | Tamaño | pandoc | comrak |
|---|---|---|---|
| Una línea (`hola`) | 5 B | 111 ms | — |
| `README.md` | 17 KB | 175 ms | 0,7 ms |
| Casos difíciles (abajo) | 1,5 KB | 564 ms | 0,1 ms |
| Documento largo (README y docs repetidos) | 110 KB | 510 ms | 5,4 ms |

- Pandoc tarda **más de 100 ms solo en arrancar**; comrak es entre 100 y 250 veces más
  rápido en estos documentos.
- No se midió el costo del lado del WebView (recrear el iframe, decodificar imágenes), que
  no cambia con esta decisión.
- comrak suma unos **0,75 MB** al ejecutable (banco de prueba: 865 KB frente a 120 KB de
  un programa vacío con el mismo perfil).

### Comparación de salida

Documento de prueba ([`0003/casos.md`](0003/casos.md)) con unas 30 construcciones: las
comunes y las propias del Markdown de pandoc. comrak con las extensiones de la sección
"Configuración".

**Iguales o equivalentes:** encabezados (mismos `id`, con acentos), énfasis, tachado,
subíndice y superíndice, enlaces, notas al pie (también las en línea `^[…]`), listas de
tareas, listas numeradas, listas de definición, citas, tablas con barras (con
alineación), divs con `:::` y una clase simple (`::: aviso`), HTML crudo, salto de línea
con `\`, regla horizontal y comillas y guiones tipográficos.

**Mejor con comrak:**
- `==resaltado==` sale como `<mark>`; pandoc lo deja literal.
- Los enlaces sueltos (`https://…`) se vuelven enlaces.
- Las alertas de GitHub (`> [!NOTE]`) se dibujan como alerta.
- Cada bloque lleva `data-sourcepos` (línea y columna del original). Permite un scroll
  sincronizado exacto, en vez de por títulos.
- Nunca falla por un sidecar ausente: el error "no se pudo lanzar el sidecar pandoc"
  desaparece de la vista previa.

**Distinto o ausente con comrak (lo que se pierde):**

| Construcción | pandoc | comrak | Plan |
|---|---|---|---|
| Metadatos YAML (`title`, `author`) | Encabezado con título y autor | Se omiten | **Se agrega**: el dominio arma el encabezado |
| Imagen sola en un párrafo | `<figure>` con el texto alternativo como pie | `<img>` en un párrafo | **Se agrega**: se recorre el árbol de comrak |
| Atributos de encabezado `{#id .clase}` | Usa el `id` y la clase | Los quita del texto pero ignora el `id` | **Se agrega**, si se puede, con el árbol de comrak; si no, limitación |
| Fórmulas `$…$` y `$$…$$` | MathJax desde CDN | Marca el texto con `data-math-style` | **Se agrega**: MathML |
| Resaltado de código | Sí (skylighting) | No | **Se agrega** en el frontend (ver abajo) |
| Diagramas Mermaid | `<pre class="mermaid">` | `<code class="language-mermaid">` | Cambia el patrón en `mermaid-diagrams.ts` |
| Tablas de cuadrícula y simples | Tabla | Texto (y los `---` se vuelven rayas) | **Limitación aceptada** |
| Listas con letras o romanos (`a.`, `i.`) y de ejemplos (`(@)`) | Lista | Párrafo | **Limitación aceptada** |
| Bloques de versos (`\| …`) | Conserva saltos y sangría | Párrafo | **Limitación aceptada** |
| Spans con clase (`[texto]{.clase}`) | `<span class>` | Texto literal | **Limitación aceptada** |
| Divs con atributos (`::: {.aviso #x}`) | `<div class id>` | La clase es el texto entre llaves, escapado | **Limitación aceptada** |
| Encabezado sin numerar (`# T {-}`) | Quita el `{-}` | Lo deja en el texto | **Limitación aceptada** |
| LaTeX crudo (`\textbf{…}`) | Lo descarta en HTML | Lo muestra literal | **Limitación aceptada** |

### Ajustes tras la revisión de la implementación

Una revisión independiente del cambio encontró diferencias que no salían en la comparación
de arriba. Se corrigieron así:

- **Esquema y scroll sincronizado:** `outline.rs` seguía las reglas de pandoc (por ejemplo,
  exigir una línea en blanco antes de un título) y dejó de coincidir con los títulos de la
  vista previa. Ahora el esquema sale del mismo árbol de comrak, con las mismas opciones.
  Un test compara las dos listas.
- **`$$` de varios renglones:** en CommonMark, un renglón que empieza con `+`, `-` o `>`
  corta el párrafo, y la fórmula terminaba partida en una lista. Antes de parsear, un bloque
  `$$` solo en su renglón pasa a ```` ```math ```` sin cambiar la cantidad de renglones.
  Los bloques ```` ```math ```` que escriba el usuario también se dibujan como fórmula.
- **Superíndice y subíndice con espacios** (`x^2 y z^3`): salen literales, como en pandoc.
- **YAML:** se acepta el cierre con `...`, los bloques `title: |` y `>`, las listas
  `author: [Ana, Luis]` y los comentarios al final del renglón.
- **```` ```{.python} ````:** el lenguaje se toma de la clase.
- **Fórmulas que cierran la app:** los paréntesis y corchetes anidados también desbordaban
  la pila de math-core. Ahora cuentan para el límite de complejidad, y además hay un tope
  de 20 000 bytes por fórmula.

Las limitaciones aceptadas siguen funcionando **al exportar**, porque el export sigue con
pandoc. Solo se ven distinto en la vista previa.

## Configuración de comrak

Extensiones activas: `strikethrough`, `table`, `autolink`, `tasklist`, `superscript`,
`subscript`, `footnotes`, `inline_footnotes`, `description_lists`,
`front_matter_delimiter = "---"`, `alerts`, `math_dollars`, `highlight`, `block_directive`,
`header_id_prefix = ""` (los `id` de los encabezados, sin prefijo) y, con la feature `attributes`, `header_attributes`,
`fenced_code_attributes` y `link_attributes`. Además `parse.smart`, `render.unsafe` (el
HTML crudo pasa, como con pandoc) y `render.sourcepos`.

`default-features = false`: sin la CLI de comrak ni syntect.

- **No** se usa `figure_with_caption`: mete `<figure>` dentro de `<p>` (HTML inválido),
  también en imágenes en línea.
- `render.unsafe` no abre un agujero nuevo: hoy pandoc también deja pasar el HTML crudo, y
  la vista previa vive en un iframe con sandbox sin `allow-same-origin`.

## Piezas

| Pieza | Cambio |
|---|---|
| `apostrophe-core` | Nuevo `preview::render(md) -> String`: comrak más los agregados (encabezado YAML, figuras, atributos, MathML). Puro y con tests. `outline` lee el mismo árbol. `math::to_mathml` convierte con math-core en un hilo con 64 MB de pila y rechaza fórmulas demasiado complejas o con macros. |
| `src-tauri` | `preview_markdown` llama al dominio en `spawn_blocking`; ya no hay temporal ni sidecar. Sigue incrustando las imágenes. |
| `src/` | Plantilla mínima (lo que daba `--standalone`), CSS de notas al pie y tareas, patrón de Mermaid, resaltado de código. |
| `convert.rs` | `PreviewOptions` y `preview_args` se retiran; `export_args` no cambia. |
| `render_math` | MathML en vez de pandoc y typst. typst sigue para el PDF. |

### Fórmulas: MathML

Chromium, y por lo tanto WebView2, dibuja MathML de forma nativa: no hace falta cargar
fuentes ni scripts en el iframe. La conversión de LaTeX a MathML se elige al implementar,
con una prueba de cobertura sobre fórmulas reales:

1. **Preferida:** un crate de Rust en el dominio (ADR-0001: ante la duda, Rust).
2. **Alternativa:** KaTeX con salida `mathml` en la ventana principal, cargado solo si hay
   fórmulas, igual que Mermaid hoy.

**Elegido: `math-core` 0.9** (MIT). En un banco de 25 fórmulas (fracciones, integrales,
matrices, `cases`, `aligned`, `\mathbb`, `\text`…) convirtió las 25 bien. Le siguieron
`pulldown-latex` (convierte todas, con defectos visibles en acentos y operadores, y necesita
su hoja de estilos), `latex2mathml` (22/25, abandonado desde 2020) y `tex2math` (LGPL, casi
sin uso). Suma unos 290 KB y tarda unos 1,5 µs por fórmula. No limita la profundidad de su
parser recursivo: ver las defensas en `crates/apostrophe-core/src/math.rs`.

### Resaltado de código: en el frontend

comrak con syntect suma **2,2 MB** más al ejecutable (banco de prueba: 3,1 MB). Se resalta
en la ventana principal con una biblioteca de JavaScript cargada solo si hay bloques de
código, el mismo patrón que Mermaid. Colorear es presentación; el costo cae solo en
documentos con código.

## Consecuencias

**Positivas**
- La vista previa pasa de cientos de milisegundos a pocos milisegundos por render, sin
  procesos ni archivos temporales.
- Las fórmulas se ven sin internet.
- Prepara el instalador de unos 10 MB: con este cambio, pandoc solo hace falta al
  exportar.
- `data-sourcepos` abre la puerta a un scroll sincronizado por línea y, más adelante, a
  actualizar solo los bloques que cambian en vez de recrear el iframe.

**Negativas / a vigilar**
- **La vista previa y el export pueden diferir** en las construcciones de la tabla de
  limitaciones. Es el costo principal. Hay que documentarlo en la ayuda.
- Un documento escrito para Apostrophe (GNOME), que usa pandoc en la vista previa, puede
  verse distinto si usa esas construcciones.
- Hay más código propio: los agregados al HTML de comrak (encabezado YAML, figuras,
  atributos) se mantienen aquí.
- Una dependencia más en el dominio. comrak es puro (sin E/S), así que no rompe la
  frontera de ADR-0001.

## Alternativas descartadas

- **Mantener pandoc, pero como servidor persistente** (`pandoc-server`): se ahorra el
  arranque (unos 110 ms), pero el documento largo sigue en unos 400 ms de conversión, y el
  instalador no baja.
- **pulldown-cmark:** más chico, pero su HTML no anota la posición de cada bloque en el
  original (habría que armarlo a mano con sus offsets) y no se confirmó que cubra los divs
  con `:::`. No se probó en el banco: comrak ya cubre lo necesario con opciones.
- **markdown-it en el WebView:** contradice ADR-0001 (el cómputo va en Rust) y tampoco
  cubre las construcciones de pandoc.
- **Detectar sintaxis exclusiva de pandoc y usar pandoc solo en esos documentos:** dos
  motores con resultados distintos según el contenido, y el retraso vuelve justo en los
  documentos más complejos.
- **syntect en Rust para el resaltado:** +2,2 MB por algo que solo colorea.
