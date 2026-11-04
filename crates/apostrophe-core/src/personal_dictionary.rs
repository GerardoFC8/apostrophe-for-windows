//! Diccionario personal: las palabras que el usuario marca como correctas ("Agregar al
//! diccionario") para que el corrector deje de subrayarlas. Nombres propios, siglas,
//! tecnicismos de su trabajo.
//!
//! Logica PURA: el formato del archivo, la comparacion y la combinacion con el diccionario
//! Hunspell. Leer y escribir el archivo es tarea del adaptador.

use std::collections::BTreeMap;

use crate::spellcheck::Dictionary;

/// Palabras aceptadas por el usuario.
///
/// La comparacion no distingue mayusculas: agregar "Elision" tambien acepta "ELISION" y
/// "elision". Es lo que se espera de una lista personal, y evita tener que agregar cada
/// palabra dos veces porque a veces abre una oracion.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PersonalDictionary {
    /// Clave en minusculas -> la palabra tal como se agrego (para escribir el archivo).
    words: BTreeMap<String, String>,
}

impl PersonalDictionary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lee el archivo: una palabra por linea. Se ignoran las lineas vacias y las que
    /// empiezan con `#`, para que el archivo se pueda editar a mano con comentarios.
    pub fn parse(text: &str) -> Self {
        let mut dict = Self::new();
        for line in text.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                dict.add(word);
            }
        }
        dict
    }

    /// Contenido del archivo: una palabra por linea, en orden alfabetico.
    pub fn to_file_text(&self) -> String {
        let mut out = String::new();
        for word in self.words.values() {
            out.push_str(word);
            out.push('\n');
        }
        out
    }

    /// Agrega una palabra. Devuelve `false` si ya estaba (o si esta vacia), para que quien
    /// llama sepa que no hace falta reescribir el archivo.
    pub fn add(&mut self, word: &str) -> bool {
        let word = word.trim();
        if word.is_empty() {
            return false;
        }
        let key = word.to_lowercase();
        if self.words.contains_key(&key) {
            return false;
        }
        self.words.insert(key, word.to_string());
        true
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains_key(&word.to_lowercase())
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

/// Un diccionario mas la lista personal: una palabra es correcta si la acepta cualquiera de
/// los dos. Las sugerencias salen solo del diccionario base.
pub struct WithPersonal<'a, D> {
    pub base: &'a D,
    pub personal: &'a PersonalDictionary,
}

impl<D: Dictionary> Dictionary for WithPersonal<'_, D> {
    fn check(&self, word: &str) -> bool {
        self.personal.contains(word) || self.base.check(word)
    }
    fn suggest(&self, word: &str, out: &mut Vec<String>) {
        self.base.suggest(word, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spellcheck::SpellChecker;

    struct OnlyHola;
    impl Dictionary for OnlyHola {
        fn check(&self, word: &str) -> bool {
            word == "hola"
        }
        fn suggest(&self, _: &str, out: &mut Vec<String>) {
            out.push("hola".into());
        }
    }

    #[test]
    fn parse_ignora_vacias_y_comentarios() {
        let dict = PersonalDictionary::parse("# mis palabras\nALMACA\n\n  stock  \r\npacking\n");
        assert_eq!(dict.len(), 3);
        assert!(dict.contains("almaca"));
        assert!(dict.contains("stock"));
    }

    #[test]
    fn no_distingue_mayusculas_y_conserva_la_forma_agregada() {
        let mut dict = PersonalDictionary::new();
        assert!(dict.add("Elision"));
        assert!(!dict.add("ELISION"), "ya estaba, con otra capitalizacion");
        assert!(dict.contains("elision"));
        assert!(dict.contains("ELISION"));
        assert_eq!(dict.to_file_text(), "Elision\n");
    }

    #[test]
    fn el_archivo_sale_ordenado_y_vuelve_a_leerse_igual() {
        let mut dict = PersonalDictionary::new();
        dict.add("stock");
        dict.add("ALMACA");
        dict.add("packing");
        let text = dict.to_file_text();
        assert_eq!(text, "ALMACA\npacking\nstock\n");
        assert_eq!(PersonalDictionary::parse(&text), dict);
    }

    #[test]
    fn una_palabra_vacia_no_se_agrega() {
        let mut dict = PersonalDictionary::new();
        assert!(!dict.add("   "));
        assert!(dict.is_empty());
    }

    #[test]
    fn el_corrector_deja_de_marcar_las_palabras_personales() {
        let mut personal = PersonalDictionary::new();
        personal.add("ALMACA");
        let dict = WithPersonal {
            base: &OnlyHola,
            personal: &personal,
        };
        let errores = SpellChecker::check_text(&dict, "hola Almaca zzz", false);
        assert_eq!(errores.len(), 1);
        assert_eq!(errores[0].word, "zzz");
        assert_eq!(errores[0].suggestions, vec!["hola".to_string()]);
    }
}
