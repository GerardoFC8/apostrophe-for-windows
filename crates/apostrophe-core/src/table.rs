//! Tablas Markdown (M17): insertar una vacia y realinear una existente. Logica PURA,
//! sin I/O ni dependencias de UI.
//!
//! Espejo de `table_input.py` (el selector de NxM) y `fix_table.py` (el realineado) del
//! upstream, con UNA divergencia deliberada: aquel genera tablas *grid*
//! (`+---+---+`) y aca se generan tablas *pipe* (`| a | b |`).
//!
//! Motivo: ambas las entiende pandoc, pero las pipe son el estandar de facto del
//! ecosistema (GitHub, VS Code, casi cualquier renderizador). Una tabla escrita en este
//! editor deberia verse bien tambien al pegar el archivo en otro lado, no solo al pasar
//! por nuestro preview. Las grid tables se romperian fuera de pandoc.

/// Tope del selector de tamano. Mas alla de esto una tabla se edita mejor a mano que
/// desde una grilla, y evita que un valor absurdo genere un documento gigante.
pub const MAX_DIMENSION: usize = 10;

/// Construye una tabla pipe VACIA con `cols` columnas y `rows` filas de cuerpo (la fila
/// de encabezado va aparte y siempre existe: sin ella no es una tabla valida en Markdown).
///
/// Las celdas van vacias a proposito: poner textos de ejemplo obligaria al dominio a
/// conocer el idioma de la interfaz, y la copia vive en el catalogo del frontend.
///
/// Ambas dimensiones se recortan a `MAX_DIMENSION` y a un minimo de 1 columna: la funcion
/// SIEMPRE devuelve una tabla valida, sin importar lo que le pidan.
pub fn empty_table(rows: usize, cols: usize) -> String {
    let cols = cols.clamp(1, MAX_DIMENSION);
    let rows = rows.min(MAX_DIMENSION);

    // La forma tiene que ser la MISMA que produce `align_table`, o realinear una tabla
    // recien insertada la movería. El ancho minimo de columna alli es 3, y cada celda va
    // rodeada de un espacio: de ahi `|     |` y `| --- |`.
    let empty_row = format!("|{}\n", "     |".repeat(cols));
    let separator = format!("|{}\n", " --- |".repeat(cols));

    let mut table = String::new();
    table.push_str(&empty_row); // encabezado
    table.push_str(&separator);
    for _ in 0..rows {
        table.push_str(&empty_row);
    }
    table
}

/// Celdas de una fila pipe: parte por `|` y descarta el vacio inicial y final que dejan
/// las barras de los extremos. Tolera tablas escritas SIN barras en los bordes.
fn cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('|')
        .unwrap_or(trimmed)
        .strip_suffix('|')
        .unwrap_or_else(|| trimmed.strip_prefix('|').unwrap_or(trimmed));
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

/// `true` si la celda es un separador (`---`, `:--`, `--:`, `:-:`), con al menos un guion.
fn is_separator_cell(cell: &str) -> bool {
    let body = cell.trim_start_matches(':').trim_end_matches(':');
    !body.is_empty() && body.chars().all(|c| c == '-')
}

/// Alineacion declarada por la celda separadora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// Los dos puntos tal como los escribio el autor. Se guardan por separado de `Align`
/// porque `:---` y `---` alinean IGUAL (izquierda es el default) pero no son lo mismo en
/// el archivo: si el autor puso el marcador explicito, realinear no debe borrarselo.
#[derive(Debug, Clone, Copy)]
struct Colons {
    start: bool,
    end: bool,
}

fn colons_of(cell: &str) -> Colons {
    Colons {
        start: cell.starts_with(':'),
        end: cell.ends_with(':'),
    }
}

fn align_of(colons: Colons) -> Align {
    match (colons.start, colons.end) {
        (true, true) => Align::Center,
        (false, true) => Align::Right,
        _ => Align::Left,
    }
}

/// Reconstruye la celda separadora conservando los dos puntos originales, con `width` de
/// ancho total (los dos puntos se descuentan para que mida lo mismo que las demas celdas).
fn separator_cell(colons: Colons, width: usize) -> String {
    let dashes = width
        .saturating_sub(colons.start as usize)
        .saturating_sub(colons.end as usize)
        .max(1);
    format!(
        "{}{}{}",
        if colons.start { ":" } else { "" },
        "-".repeat(dashes),
        if colons.end { ":" } else { "" }
    )
}

fn pad(text: &str, width: usize, align: Align) -> String {
    let missing = width.saturating_sub(text.chars().count());
    match align {
        Align::Left => format!("{text}{}", " ".repeat(missing)),
        Align::Right => format!("{}{text}", " ".repeat(missing)),
        Align::Center => {
            let left = missing / 2;
            format!("{}{text}{}", " ".repeat(left), " ".repeat(missing - left))
        }
    }
}

/// Realinea una tabla pipe para que las barras queden en columna.
///
/// Devuelve `None` si el texto NO es una tabla pipe reconocible (hacen falta al menos una
/// fila de encabezado y una separadora). Devolver `None` en vez de un texto a medias deja
/// que quien llama no toque nada: es preferible no hacer nada a estropear el documento.
///
/// El ancho de columna se mide en CARACTERES, no en bytes: un encabezado con acentos o
/// eñes desalinearia la tabla si se contaran bytes.
pub fn align_table(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return None;
    }

    let header = cells(lines[0]);
    let separator = cells(lines[1]);
    // La segunda fila TIENE que ser la separadora y tener tantas celdas como el encabezado:
    // es lo que distingue una tabla de dos renglones que casualmente llevan barras.
    if separator.len() != header.len() || !separator.iter().all(|c| is_separator_cell(c)) {
        return None;
    }

    let colons: Vec<Colons> = separator.iter().map(|c| colons_of(c)).collect();
    let aligns: Vec<Align> = colons.iter().map(|c| align_of(*c)).collect();
    let body: Vec<Vec<String>> = lines[2..].iter().map(|l| cells(l)).collect();

    // Ancho por columna: la celda mas larga de toda la tabla. El minimo de 3 mantiene el
    // separador legible (`---`) aunque todas las celdas esten vacias.
    let widths: Vec<usize> = (0..header.len())
        .map(|i| {
            let longest = std::iter::once(&header)
                .chain(body.iter())
                .filter_map(|row| row.get(i))
                .map(|c| c.chars().count())
                .max()
                .unwrap_or(0);
            longest.max(3)
        })
        .collect();

    let render = |row: &[String]| -> String {
        let rendered: Vec<String> = (0..header.len())
            .map(|i| {
                let cell = row.get(i).map(String::as_str).unwrap_or("");
                format!(" {} ", pad(cell, widths[i], aligns[i]))
            })
            .collect();
        format!("|{}|", rendered.join("|"))
    };

    let mut out = String::new();
    out.push_str(&render(&header));
    out.push('\n');
    let sep: Vec<String> = (0..header.len())
        .map(|i| format!(" {} ", pad(&separator_cell(colons[i], widths[i]), widths[i], Align::Left)))
        .collect();
    out.push_str(&format!("|{}|", sep.join("|")));
    for row in &body {
        out.push('\n');
        out.push_str(&render(row));
    }
    Some(out)
}

/// Una tabla que encontro [`align_tables`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSpan {
    /// Linea del encabezado en el documento original, base 1 (como la muestra un editor).
    pub line: usize,
    /// `true` si ya estaba alineada: realinearla no cambio ni un byte.
    pub aligned: bool,
}

/// Por que [`align_tables`] dejo una tabla como estaba.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Lleva una barra escapada (`\|`): partir por las barras cambiaria sus celdas.
    EscapedPipe,
    /// Una fila tiene mas celdas que el encabezado: alinearla las perderia.
    ExtraCells,
    /// Sus lineas no tienen la misma sangria: elegir una seria decidir por el autor.
    MixedIndent,
    /// Esta dentro de una cita (`>`): habria que conservar los marcadores de cada linea.
    Blockquote,
}

/// Una tabla que [`align_tables`] encontro pero no toco, porque alinearla podria
/// estropear el documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedTable {
    /// Linea del encabezado, base 1.
    pub line: usize,
    pub reason: SkipReason,
}

/// Resultado de [`align_tables`]: el documento con todas sus tablas alineadas, una
/// entrada por tabla alineada y otra por tabla omitida, en orden de aparicion.
///
/// Las omitidas se informan aparte para que quien verifica no las de por buenas en
/// silencio: no estan alineadas, pero `align_tables` tampoco puede arreglarlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedTables {
    pub text: String,
    pub tables: Vec<TableSpan>,
    pub skipped: Vec<SkippedTable>,
}

/// Alinea TODAS las tablas pipe de un documento y deja intacto el resto del texto.
///
/// Las tablas dentro de bloques de codigo (cercas ``` o ~~~, o sangria de 4 espacios o un
/// tab) no se tocan: ahi el texto es literal. Cada tabla conserva su sangria y los saltos de
/// linea de cada renglon (LF o CRLF), y la ultima linea sin salto sigue sin salto.
///
/// Es idempotente y, si ninguna tabla cambia, `text` es identico a `doc` byte a byte: quien
/// llama puede comparar para decidir si hace falta reescribir el archivo.
///
/// Las tablas que no puede alinear sin estropearlas (ver [`SkipReason`]) quedan intactas y
/// se informan en `skipped`, no en `tables`.
pub fn align_tables(doc: &str) -> AlignedTables {
    let lines: Vec<Line> = doc.split_inclusive('\n').map(Line::new).collect();
    let mut out = AlignedTables {
        text: String::with_capacity(doc.len()),
        tables: Vec::new(),
        skipped: Vec::new(),
    };
    let mut fence: Option<Fence> = None;

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if let Some(open) = fence {
            if closes_fence(open, line.content) {
                fence = None;
            }
        } else if let Some(open) = opening_fence(line.content) {
            fence = Some(open);
        } else if is_table_candidate(line.content) {
            // Un bloque es una racha de lineas con barras: una tabla no cruza lineas vacias.
            let end = (i..lines.len())
                .find(|&j| !is_table_candidate(lines[j].content))
                .unwrap_or(lines.len());
            align_block(&lines[i..end], i + 1, &mut out);
            i = end;
            continue;
        }
        line.push_to(&mut out.text);
        i += 1;
    }

    out
}

/// Una linea del documento separada de su salto (`"\n"`, `"\r\n"` o nada al final).
struct Line<'a> {
    content: &'a str,
    ending: &'a str,
}

impl<'a> Line<'a> {
    fn new(raw: &'a str) -> Self {
        let cut = if raw.ends_with("\r\n") {
            raw.len() - 2
        } else if raw.ends_with('\n') {
            raw.len() - 1
        } else {
            raw.len()
        };
        let (content, ending) = raw.split_at(cut);
        Line { content, ending }
    }

    fn push_to(&self, out: &mut String) {
        out.push_str(self.content);
        out.push_str(self.ending);
    }
}

/// Cerca de un bloque de codigo: el caracter (` o ~) y cuantos lleva.
#[derive(Debug, Clone, Copy)]
struct Fence {
    marker: char,
    len: usize,
}

/// La cerca que abre esta linea, si abre una, y lo que la sigue (el info string).
fn fence_at(content: &str) -> Option<(Fence, &str)> {
    let rest = &content[table_indent(content)?.len()..];
    let marker = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let len = rest.chars().take_while(|c| *c == marker).count();
    // `marker` es ASCII: `len` caracteres son `len` bytes.
    (len >= 3).then(|| (Fence { marker, len }, &rest[len..]))
}

fn opening_fence(content: &str) -> Option<Fence> {
    fence_at(content).map(|(fence, _)| fence)
}

/// Cierra la cerca `open` una linea con el MISMO caracter, al menos tantos como la que
/// abrio y nada mas: con una de cuatro, una de tres de adentro es contenido.
fn closes_fence(open: Fence, content: &str) -> bool {
    fence_at(content).is_some_and(|(fence, rest)| {
        fence.marker == open.marker && fence.len >= open.len && rest.trim().is_empty()
    })
}

/// Sangria de una linea: hasta 3 espacios. `None` si es codigo indentado (4 espacios o mas,
/// o un tab), donde una tabla no es tabla sino texto literal.
fn table_indent(content: &str) -> Option<&str> {
    let spaces = content.len() - content.trim_start_matches(' ').len();
    if spaces >= 4 || content[spaces..].starts_with('\t') {
        return None;
    }
    Some(&content[..spaces])
}

/// Linea que puede ser parte de una tabla: tiene barras, no es codigo indentado y no abre
/// una cerca (```` ```md|x ```` lleva una barra, pero lo que sigue es codigo).
fn is_table_candidate(content: &str) -> bool {
    content.contains('|')
        && !content.trim().is_empty()
        && table_indent(content).is_some()
        && opening_fence(content).is_none()
}

/// `true` si la linea es una fila separadora (`|---|:-:|`): todas sus celdas lo son.
fn is_separator_row(content: &str) -> bool {
    cells(content).iter().all(|c| is_separator_cell(c))
}

/// Alinea un bloque de lineas con barras que empieza en la linea `first_line` (base 1).
///
/// El encabezado es la linea que precede al PRIMER separador, como en GFM: sin una linea
/// vacia de por medio, una linea de prosa con barras queda en el mismo bloque y no debe
/// esconder la tabla. Lo que va antes del encabezado es prosa y se copia tal cual, igual
/// que el bloque entero si no es una tabla o no se puede alinear sin inventar nada (en
/// ese caso, se anota como omitida).
fn align_block(block: &[Line], first_line: usize, out: &mut AlignedTables) {
    let copy = |lines: &[Line], text: &mut String| lines.iter().for_each(|l| l.push_to(text));
    let Some(header) = header_index(block, |content| Some(content)) else {
        // Sin separador visible puede ser una tabla dentro de una cita: no se toca, pero
        // se avisa.
        if let Some(header) = header_index(block, unquoted) {
            out.skipped.push(SkippedTable {
                line: first_line + header,
                reason: SkipReason::Blockquote,
            });
        }
        copy(block, &mut out.text);
        return;
    };
    let (prose, table) = block.split_at(header);
    copy(prose, &mut out.text);
    let line = first_line + header;
    match realign(table) {
        Ok(Some(rebuilt)) => {
            let mut original = String::new();
            copy(table, &mut original);
            out.tables.push(TableSpan {
                line,
                aligned: rebuilt == original,
            });
            out.text.push_str(&rebuilt);
        }
        Ok(None) => copy(table, &mut out.text),
        Err(reason) => {
            out.skipped.push(SkippedTable { line, reason });
            copy(table, &mut out.text);
        }
    }
}

/// Indice del encabezado: la linea anterior al primer separador del bloque, mirando el
/// contenido que deja `view` (`None` = la linea no cuenta).
fn header_index(block: &[Line], view: impl Fn(&str) -> Option<&str>) -> Option<usize> {
    (1..block.len())
        .find(|&j| view(block[j].content).is_some_and(is_separator_row))
        .filter(|&j| view(block[j - 1].content).is_some())
        .map(|j| j - 1)
}

/// Contenido de una linea de cita sin sus marcadores (`>`, `> >`…); `None` si no es cita.
fn unquoted(content: &str) -> Option<&str> {
    let rest = content.trim_start().strip_prefix('>')?;
    Some(rest.trim_start_matches(['>', ' ']))
}

/// La tabla alineada, con la sangria comun y el salto de cada linea originales.
///
/// `Ok(None)` si no es una tabla (el separador no tiene tantas celdas como el encabezado)
/// y `Err` si lo es pero alinearla podria estropearla.
fn realign(table: &[Line]) -> Result<Option<String>, SkipReason> {
    // La barra escapada va primero: con ella, contar celdas partiendo por todas las barras
    // ya no dice nada, ni siquiera si es una tabla.
    if table.iter().any(|l| l.content.contains("\\|")) {
        return Err(SkipReason::EscapedPipe);
    }
    let columns = cells(table[0].content).len();
    if cells(table[1].content).len() != columns {
        return Ok(None);
    }
    // `is_table_candidate` ya garantizo que ninguna linea es codigo indentado.
    let indent = table_indent(table[0].content).unwrap_or_default();
    if table
        .iter()
        .any(|l| table_indent(l.content) != Some(indent))
    {
        return Err(SkipReason::MixedIndent);
    }
    // `align_table` dibuja solo las columnas del encabezado: perderia las de mas.
    if table.iter().any(|l| cells(l.content).len() > columns) {
        return Err(SkipReason::ExtraCells);
    }
    let body: Vec<&str> = table.iter().map(|l| &l.content[indent.len()..]).collect();
    let Some(aligned) = align_table(&body.join("\n")) else {
        return Ok(None);
    };
    // `align_table` devuelve una linea por cada linea no vacia; si no, mejor no tocar nada.
    if aligned.lines().count() != table.len() {
        return Ok(None);
    }
    let mut out = String::new();
    for (line, new) in table.iter().zip(aligned.lines()) {
        out.push_str(indent);
        out.push_str(new);
        out.push_str(line.ending);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_tabla_vacia_tiene_encabezado_separador_y_cuerpo() {
        let t = empty_table(2, 3);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines.len(), 4, "encabezado + separador + 2 filas");
        assert_eq!(lines[0], "|     |     |     |");
        // Misma forma canonica que produce `align_table` (ver el test de ida y vuelta).
        assert_eq!(lines[1], "| --- | --- | --- |");
        assert_eq!(lines[2], "|     |     |     |");
    }

    #[test]
    fn una_tabla_sin_filas_de_cuerpo_sigue_siendo_valida() {
        // Sin fila separadora no seria una tabla en Markdown: el encabezado y el separador
        // no son opcionales aunque el usuario pida cero filas.
        let t = empty_table(0, 2);
        assert_eq!(t.lines().count(), 2);
    }

    #[test]
    fn las_dimensiones_se_recortan_al_maximo() {
        let t = empty_table(99, 99);
        assert_eq!(t.lines().count(), MAX_DIMENSION + 2);
        assert_eq!(t.lines().next().unwrap().matches('|').count(), MAX_DIMENSION + 1);
    }

    #[test]
    fn cero_columnas_no_produce_una_tabla_rota() {
        // Pedir cero columnas es un error de quien llama; la funcion no puede devolver algo
        // que Markdown no sepa leer.
        let t = empty_table(1, 0);
        assert_eq!(t.lines().next().unwrap(), "|     |");
    }

    #[test]
    fn la_tabla_generada_se_realinea_a_si_misma_sin_cambios() {
        // Propiedad util: lo que genera `empty_table` ya esta alineado, asi que pasarlo por
        // `align_table` no deberia moverlo (mas alla del salto final).
        let generada = empty_table(2, 3);
        let alineada = align_table(&generada).expect("deberia reconocerse como tabla");
        assert_eq!(alineada.trim_end(), generada.trim_end());
    }

    #[test]
    fn alinea_las_barras_de_una_tabla_desprolija() {
        let sucia = "| nombre | edad |\n|---|---|\n| Ana | 30 |\n| Maximiliano | 7 |";
        let esperada = "\
| nombre      | edad |
| ----------- | ---- |
| Ana         | 30   |
| Maximiliano | 7    |";
        assert_eq!(align_table(sucia).unwrap(), esperada);
    }

    #[test]
    fn el_ancho_se_mide_en_caracteres_no_en_bytes() {
        // "añoñ" son 4 caracteres pero 6 bytes: contando bytes, la columna quedaria
        // desalineada respecto de una de 4 letras ASCII.
        let t = align_table("| añoñ |\n|---|\n| abcd |").unwrap();
        let anchos: Vec<usize> = t.lines().map(|l| l.chars().count()).collect();
        assert!(
            anchos.windows(2).all(|w| w[0] == w[1]),
            "todas las filas deberian medir lo mismo: {anchos:?}"
        );
    }

    #[test]
    fn se_respeta_la_alineacion_declarada() {
        let t = align_table("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |").unwrap();
        let sep = t.lines().nth(1).unwrap();
        assert!(sep.contains(":--"), "izquierda: {sep}");
        assert!(sep.contains(":-:") || sep.contains(": "), "centro: {sep}");
        assert!(sep.contains("--:"), "derecha: {sep}");
    }

    #[test]
    fn una_fila_con_celdas_de_menos_se_completa_con_vacias() {
        // Tabla escrita a mano a la que le falta una celda: se rellena en vez de descuadrar.
        let t = align_table("| a | b |\n|---|---|\n| 1 |").unwrap();
        let ultima = t.lines().last().unwrap();
        assert_eq!(ultima.matches('|').count(), 3, "deberia tener dos celdas: {ultima}");
    }

    #[test]
    fn tolera_tablas_sin_barras_en_los_bordes() {
        let t = align_table("a | b\n--- | ---\n1 | 2");
        assert!(t.is_some(), "una tabla sin barras externas sigue siendo una tabla");
        assert!(t.unwrap().starts_with('|'), "la salida SI las normaliza");
    }

    #[test]
    fn el_texto_que_no_es_tabla_se_deja_intacto() {
        // Devolver None deja que quien llama no toque nada: mejor no hacer nada que
        // estropear el documento.
        assert!(align_table("solo un parrafo").is_none());
        assert!(align_table("| parece | tabla |\npero no hay separador").is_none());
        assert!(align_table("").is_none());
    }

    #[test]
    fn un_separador_con_distinta_cantidad_de_celdas_no_es_tabla() {
        // Si el separador no coincide con el encabezado, el documento esta mal formado:
        // realinear a ciegas inventaria columnas.
        assert!(align_table("| a | b | c |\n|---|---|\n| 1 | 2 | 3 |").is_none());
    }

    #[test]
    fn realinear_es_idempotente() {
        // Correr el comando dos veces no debe seguir moviendo el texto.
        let una = align_table("| a | bbbb |\n|---|---|\n| 1 | 2 |").unwrap();
        let dos = align_table(&una).unwrap();
        assert_eq!(una, dos);
    }

    // --- align_tables: todas las tablas de un documento ---

    /// Atajo para comparar los reportes: `(linea, ya_alineada)`.
    fn spans(result: &AlignedTables) -> Vec<(usize, bool)> {
        result.tables.iter().map(|t| (t.line, t.aligned)).collect()
    }

    #[test]
    fn alinea_dos_tablas_sin_tocar_el_texto_entre_ellas() {
        let doc = "# Notas\n\n| a | bb |\n|---|---|\n| ccc | d |\n\n\
                   Texto | con barra en medio.\n\n| x |\n|:-:|\n| yyyyy |\n";
        let result = align_tables(doc);
        assert_eq!(
            result.text,
            "# Notas\n\n| a   | bb  |\n| --- | --- |\n| ccc | d   |\n\n\
             Texto | con barra en medio.\n\n|   x   |\n| :---: |\n| yyyyy |\n"
        );
        // Lineas base 1 del encabezado de cada tabla.
        assert_eq!(spans(&result), [(3, false), (9, false)]);
    }

    #[test]
    fn alinear_un_documento_dos_veces_no_lo_mueve() {
        let primera = align_tables("| a | bb |\n|---|---|\n| ccc | d |\n\n| x |\n|---|\n");
        let segunda = align_tables(&primera.text);
        assert_eq!(segunda.text, primera.text);
        assert_eq!(spans(&primera), [(1, false), (5, false)]);
        assert_eq!(spans(&segunda), [(1, true), (5, true)]);
    }

    #[test]
    fn sin_cambios_el_texto_es_identico_byte_a_byte() {
        // Espacios al final, saltos mezclados y un final sin salto: si ninguna tabla cambia,
        // el resultado es EXACTAMENTE la entrada (lo que permite no reescribir el archivo).
        let doc = "Título  \r\n\n| a   | bb  |\n| --- | --- |\r\n\n  sangría  \nsin salto";
        let result = align_tables(doc);
        assert_eq!(result.text, doc);
        assert_eq!(spans(&result), [(3, true)]);
    }

    #[test]
    fn un_documento_sin_tablas_no_reporta_ninguna() {
        let doc = "Solo texto.\n\nUna | barra suelta no hace una tabla.\n";
        let result = align_tables(doc);
        assert_eq!(result.text, doc);
        assert!(result.tables.is_empty());
        assert_eq!(align_tables("").text, "");
    }

    #[test]
    fn las_tablas_dentro_de_bloques_de_codigo_no_se_tocan() {
        // La cerca de cuatro acentos no se cierra con la de tres que lleva adentro; la tabla
        // de despues de las cercas SI se alinea (prueba que se cerraron bien).
        let doc = "```\n| a | b |\n|---|---|\n```\n\n~~~md\n| c | d |\n|---|---|\n~~~\n\n\
                   ````\n```\n| e | f |\n|---|---|\n```\n````\n\n| g | h |\n|---|---|\n";
        let result = align_tables(doc);
        let expected = doc.replace("| g | h |\n|---|---|\n", "| g   | h   |\n| --- | --- |\n");
        assert_eq!(result.text, expected);
        assert_eq!(spans(&result), [(18, false)]);
    }

    #[test]
    fn conserva_los_saltos_crlf() {
        let doc = "Intro\r\n\r\n| a | bb |\r\n|---|---|\r\n| ccc | d |\r\n";
        let result = align_tables(doc);
        assert_eq!(
            result.text,
            "Intro\r\n\r\n| a   | bb  |\r\n| --- | --- |\r\n| ccc | d   |\r\n"
        );
        assert_eq!(
            result.text.matches('\n').count(),
            result.text.matches("\r\n").count(),
            "no deberia quedar ningun salto LF suelto"
        );
    }

    #[test]
    fn la_ultima_linea_sin_salto_sigue_sin_salto() {
        let result = align_tables("Texto\n\n| a | bb |\n|---|---|");
        assert_eq!(result.text, "Texto\n\n| a   | bb  |\n| --- | --- |");
        assert_eq!(spans(&result), [(3, false)]);
    }

    #[test]
    fn una_tabla_con_sangria_de_dos_espacios_se_alinea_y_conserva_la_sangria() {
        let result = align_tables("  | a | bb |\n  |---|---|\n  | ccc | d |\n");
        assert_eq!(
            result.text,
            "  | a   | bb  |\n  | --- | --- |\n  | ccc | d   |\n"
        );
        assert_eq!(spans(&result), [(1, false)]);
    }

    #[test]
    fn cuatro_espacios_o_un_tab_son_codigo_y_no_se_tocan() {
        for doc in [
            "    | a | bb |\n    |---|---|\n",
            "\t| a | bb |\n\t|---|---|\n",
        ] {
            let result = align_tables(doc);
            assert_eq!(result.text, doc);
            assert!(result.tables.is_empty(), "{doc:?}");
        }
    }

    #[test]
    fn una_sangria_que_no_es_comun_deja_la_tabla_como_esta() {
        // Realinearla obligaria a elegir una sangria por el autor: mejor no tocarla.
        let doc = " | a | bb |\n|---|---|\n";
        let result = align_tables(doc);
        assert_eq!(result.text, doc);
        assert!(result.tables.is_empty());
    }

    #[test]
    fn una_linea_de_prosa_con_barra_justo_encima_no_esconde_la_tabla() {
        // Sin linea vacia de por medio, la prosa queda en el mismo bloque de lineas con
        // barras. El encabezado es la linea que precede al separador (como en GFM): lo de
        // arriba es prosa y se deja intacto.
        let doc = "Nota: a | b es un ejemplo.\n| a | bb |\n|---|---|\n| ccc | d |\n";
        let result = align_tables(doc);
        assert_eq!(
            result.text,
            "Nota: a | b es un ejemplo.\n| a   | bb  |\n| --- | --- |\n| ccc | d   |\n"
        );
        assert_eq!(spans(&result), [(2, false)]);
    }

    #[test]
    fn varias_lineas_de_prosa_con_barras_encima_tampoco() {
        let doc = "uno | dos\ntres | cuatro | cinco\n| a | bb |\n|---|---|\n";
        let result = align_tables(doc);
        assert_eq!(
            result.text,
            "uno | dos\ntres | cuatro | cinco\n| a   | bb  |\n| --- | --- |\n"
        );
        assert_eq!(spans(&result), [(3, false)]);
    }

    #[test]
    fn una_cerca_con_barra_en_el_info_string_corta_la_tabla() {
        // La linea de la cerca tiene una barra, pero abre un bloque de codigo: lo de adentro
        // es literal aunque parezca la continuacion de la tabla de arriba.
        let doc = "| a | bb |\n|---|---|\n```md|x\n| c | d |\n|---|---|\n```\n";
        let result = align_tables(doc);
        assert_eq!(
            result.text,
            "| a   | bb  |\n| --- | --- |\n```md|x\n| c | d |\n|---|---|\n```\n"
        );
        assert_eq!(spans(&result), [(1, false)]);
    }

    #[test]
    fn una_tabla_que_perderia_contenido_se_deja_como_esta() {
        // `align_table` parte por TODAS las barras y dibuja solo las columnas del
        // encabezado: una barra escapada se partiria en dos celdas y una fila con celdas de
        // mas las perderia. En vez de estropear el documento, la tabla no se toca.
        for doc in [
            "| a | b |\n|---|---|\n| x \\| y | z |\n",
            "| a | b |\n|---|---|\n| uno \\| dos |\n",
            "| a | b |\n|---|---|\n| 1 | 2 | 3 |\n",
        ] {
            let result = align_tables(doc);
            assert_eq!(result.text, doc);
            assert!(result.tables.is_empty(), "{doc:?}");
        }
    }

    #[test]
    fn un_separador_que_no_coincide_con_el_encabezado_no_es_tabla() {
        let doc = "| a | b | c |\n|---|---|\n";
        let result = align_tables(doc);
        assert_eq!(result.text, doc);
        assert!(result.tables.is_empty());
        assert!(
            result.skipped.is_empty(),
            "no es una tabla: no se omite nada"
        );
    }

    // --- tablas omitidas: se dejan como estan, pero se avisa ---

    /// Atajo para comparar las omitidas: `(linea, motivo)`.
    fn skipped(result: &AlignedTables) -> Vec<(usize, SkipReason)> {
        result.skipped.iter().map(|s| (s.line, s.reason)).collect()
    }

    #[test]
    fn una_tabla_que_no_se_puede_alinear_se_reporta_como_omitida() {
        // Quien verifica (`tables --check`) no puede darlas por alineadas en silencio: un
        // agente creeria que el documento esta bien, o reintentaria un `--write` que no
        // las arregla. Se dejan intactas y se informan aparte, con su motivo.
        for (doc, reason) in [
            (
                "| a | b |\n|---|---|\n| x \\| y | z |\n",
                SkipReason::EscapedPipe,
            ),
            (
                "| a | b |\n|---|---|\n| 1 | 2 | 3 |\n",
                SkipReason::ExtraCells,
            ),
            (" | a | bb |\n|---|---|\n", SkipReason::MixedIndent),
            (
                "> | a | bb |\n> |---|---|\n> | c | d |\n",
                SkipReason::Blockquote,
            ),
        ] {
            let result = align_tables(doc);
            assert_eq!(result.text, doc);
            assert!(result.tables.is_empty(), "{doc:?}");
            assert_eq!(skipped(&result), [(1, reason)], "{doc:?}");
        }
    }

    #[test]
    fn una_barra_escapada_en_el_encabezado_tambien_omite_la_tabla() {
        // Sin el escape, el encabezado tendria tantas celdas como el separador: es una
        // tabla, y contar las barras a ciegas la haria pasar por texto.
        let doc = "| a \\| b | c |\n|---|---|\n";
        let result = align_tables(doc);
        assert_eq!(result.text, doc);
        assert_eq!(skipped(&result), [(1, SkipReason::EscapedPipe)]);
    }

    #[test]
    fn las_omitidas_llevan_la_linea_de_su_encabezado_y_no_tapan_a_las_demas() {
        let doc = "Intro\n\n| a | b |\n|---|---|\n| 1 | 2 | 3 |\n\nprosa | barra\n\
                   > | c | d |\n> |---|---|\n\n| e | ff |\n|---|---|\n";
        let result = align_tables(doc);
        assert_eq!(
            result.text,
            doc.replace("| e | ff |\n|---|---|\n", "| e   | ff  |\n| --- | --- |\n")
        );
        assert_eq!(spans(&result), [(11, false)]);
        assert_eq!(
            skipped(&result),
            [(3, SkipReason::ExtraCells), (8, SkipReason::Blockquote)]
        );
    }

    #[test]
    fn una_cita_con_barras_que_no_es_tabla_no_se_reporta() {
        let doc = "> uno | dos\n> tres | cuatro\n";
        let result = align_tables(doc);
        assert_eq!(result.text, doc);
        assert!(result.skipped.is_empty());
    }
}
