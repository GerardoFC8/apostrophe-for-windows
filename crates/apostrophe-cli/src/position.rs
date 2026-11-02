//! Posiciones legibles para un editor: el dominio da offsets en CARACTERES (escalares
//! Unicode) y un agente necesita linea y columna.

/// Indice de los inicios de linea de un texto, para traducir un offset en caracteres a
/// `(linea, columna)`, ambas base 1.
///
/// Solo `\n` corta lineas: en CRLF el `\r` queda al final de la linea anterior (un solo
/// salto) y un `\r` suelto no es salto. La columna se cuenta en caracteres, como los
/// offsets del dominio: difiere de UTF-16 solo fuera del BMP (un emoji es una columna).
pub struct LineIndex {
    /// Offset en caracteres del primer caracter de cada linea; el primero es 0.
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.chars()
                .enumerate()
                .filter(|(_, ch)| *ch == '\n')
                .map(|(i, _)| i + 1),
        );
        Self { starts }
    }

    /// `(linea, columna)` base 1 del caracter numero `offset`. Busqueda binaria: un texto
    /// con muchos errores no recorre el documento una vez por cada uno.
    pub fn position(&self, offset: usize) -> (usize, usize) {
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        (line + 1, offset - self.starts[line] + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_linea_y_la_columna_empiezan_en_uno() {
        let index = LineIndex::new("hola\nmundoo");
        assert_eq!(index.position(0), (1, 1));
        assert_eq!(index.position(2), (1, 3));
        // `mundoo` empieza en el caracter 5: primera columna de la segunda linea.
        assert_eq!(index.position(5), (2, 1));
        assert_eq!(index.position(8), (2, 4));
    }

    #[test]
    fn crlf_es_un_solo_salto_y_un_retorno_suelto_no_es_salto() {
        let crlf = LineIndex::new("a\r\nbc\r\nd");
        assert_eq!(crlf.position(3), (2, 1));
        assert_eq!(crlf.position(7), (3, 1));
        let lone_cr = LineIndex::new("a\rb");
        assert_eq!(lone_cr.position(2), (1, 3));
    }

    #[test]
    fn las_columnas_se_cuentan_en_caracteres_no_en_bytes_ni_en_utf16() {
        // "ñ" ocupa 2 bytes y "😀" 4 bytes (o 2 unidades UTF-16): cada uno es UNA columna,
        // igual que en los offsets del dominio.
        let index = LineIndex::new("ñ😀 x\n😀y");
        assert_eq!(index.position(3), (1, 4));
        assert_eq!(index.position(6), (2, 2));
    }

    #[test]
    fn muchas_lineas_se_ubican_bien() {
        let text = "x\n".repeat(1000);
        let index = LineIndex::new(&text);
        assert_eq!(index.position(0), (1, 1));
        assert_eq!(index.position(1998), (1000, 1));
        assert_eq!(index.position(1999), (1000, 2));
    }
}
