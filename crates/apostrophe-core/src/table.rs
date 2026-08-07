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
}
