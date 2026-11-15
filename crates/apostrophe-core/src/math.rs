//! Formulas LaTeX a MathML (ADR-0003). WebView2 (Chromium) dibuja MathML Core de forma
//! nativa: no hacen falta fuentes, scripts ni internet. Lo usan la vista previa y el
//! popover de formulas del editor.
//!
//! La conversion la hace `math-core`. Su parser es recursivo y NO limita la profundidad: una
//! formula muy anidada desborda la pila, y un desbordamiento no es un panico, cierra el
//! proceso. Medido con 1 MB de pila (la del hilo principal en Windows): en debug alcanzan
//! 10 fracciones anidadas, en release unas 300. Por eso hay dos defensas:
//!
//! 1. **Pila grande.** `to_mathml` convierte en un hilo propio con [`STACK_SIZE`] de pila
//!    reservada. En Windows es solo reserva virtual: no se usa memoria si no hace falta.
//!    `preview::render` hace lo mismo una vez por render y llama a [`convert`] adentro.
//! 2. **Limite de complejidad.** La profundidad de la recursion no supera la cantidad de
//!    elementos que pueden anidar (`{`, `\`, `^`, `_`, `(`, `[`). Con mas de [`MAX_TOKENS`]
//!    la formula no se convierte. Ademas, ninguna formula de mas de [`MAX_BYTES`] se
//!    convierte: cubre cualquier otra recursion de math-core que no se haya encontrado
//!    (los parentesis se encontraron asi, en una revision: 32 000 anidados desbordaban 64 MB
//!    en debug). Y no se aceptan macros (`\def`, `\newcommand`…): una macro recursiva
//!    multiplicaria la profundidad sin agregar elementos. En la vista previa no sirven de
//!    todos modos, porque cada formula se convierte por separado.
//!
//! Con 64 MB y 500 elementos, el peor caso medido en debug (fracciones con llaves, unos
//! 100 KB por nivel y 3 elementos por nivel) usa unos 17 MB.

use std::sync::OnceLock;

use math_core::{LatexToMathML, MathCoreConfig, MathDisplay};

/// Pila reservada para convertir. Ver el comentario del modulo.
pub const STACK_SIZE: usize = 64 << 20;

/// Maximo de elementos que pueden anidar (`{`, `\`, `^`, `_`, `(`, `[`) en una formula.
pub const MAX_TOKENS: usize = 500;

/// Largo maximo de una formula, en bytes. Una formula real cabe de sobra.
pub const MAX_BYTES: usize = 20_000;

/// Comandos que definen macros: no se aceptan (ver el comentario del modulo).
const MACRO_COMMANDS: [&str; 5] = [
    "def",
    "gdef",
    "newcommand",
    "renewcommand",
    "providecommand",
];

/// Por que una formula no se pudo convertir. El texto para la interfaz lo arma el
/// frontend: el dominio no guarda copia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathError {
    /// Demasiado compleja o con macros: no se intenta convertir.
    TooComplex,
    /// LaTeX invalido. Lleva el mensaje de `math-core`, en ingles.
    Invalid(String),
}

/// Convierte `latex` a un elemento `<math>`, en bloque si `display`. Corre en un hilo con
/// pila grande: se puede llamar desde cualquier hilo.
pub fn to_mathml(latex: &str, display: bool) -> Result<String, MathError> {
    with_big_stack(|| convert(latex, display))
}

/// Como [`to_mathml`], pero en el hilo actual. Solo para quien ya corre con [`STACK_SIZE`]
/// de pila, como `preview::render`.
pub(crate) fn convert(latex: &str, display: bool) -> Result<String, MathError> {
    if too_complex(latex) {
        return Err(MathError::TooComplex);
    }
    let mode = if display {
        MathDisplay::Block
    } else {
        MathDisplay::Inline
    };
    converter()
        .convert_with_local_state(latex, mode)
        .map(|result| result.mathml)
        .map_err(|e| MathError::Invalid(e.error_message()))
}

/// Corre `f` en un hilo con [`STACK_SIZE`] de pila y devuelve su resultado. Un panico de `f`
/// se propaga a quien llama, como si hubiera corrido en su hilo.
pub(crate) fn with_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("elision-math".into())
            .stack_size(STACK_SIZE)
            .spawn_scoped(scope, f)
            // Crear un hilo solo falla sin memoria virtual; ahi no hay nada mejor que hacer.
            .expect("no se pudo crear el hilo para convertir formulas")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

fn converter() -> &'static LatexToMathML {
    static CONVERTER: OnceLock<LatexToMathML> = OnceLock::new();
    CONVERTER.get_or_init(|| {
        LatexToMathML::new(MathCoreConfig::default(), Vec::new())
            .expect("sin macros propias, la configuracion por defecto es valida")
    })
}

/// `true` si la formula supera [`MAX_BYTES`] o [`MAX_TOKENS`], o define macros.
fn too_complex(latex: &str) -> bool {
    if latex.len() > MAX_BYTES {
        return true;
    }
    let tokens = latex
        .chars()
        .filter(|c| matches!(c, '{' | '\\' | '^' | '_' | '(' | '['))
        .count();
    tokens > MAX_TOKENS || defines_macro(latex)
}

fn defines_macro(latex: &str) -> bool {
    latex.split('\\').skip(1).any(|rest| {
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        MACRO_COMMANDS.contains(&name.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convierte_en_linea_y_en_bloque() {
        let inline = to_mathml("E = mc^2", false).unwrap();
        assert!(inline.starts_with("<math"), "{inline}");
        assert!(!inline.contains("display=\"block\""), "{inline}");
        assert!(inline.contains("<msup>"), "{inline}");

        let block = to_mathml(r"\frac{a}{b}", true).unwrap();
        assert!(block.contains("display=\"block\""), "{block}");
        assert!(block.contains("<mfrac>"), "{block}");
    }

    #[test]
    fn cubre_entornos_y_letras_especiales() {
        for latex in [
            r"\begin{cases} 1 & x > 0 \\ 0 & \text{si no} \end{cases}",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\begin{aligned} a &= b \\ c &= d \end{aligned}",
            r"\mathbb{R} \to \mathbf{v}",
            r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
        ] {
            assert!(to_mathml(latex, true).is_ok(), "{latex}");
        }
    }

    #[test]
    fn el_latex_invalido_es_un_error_sin_panico() {
        for latex in [r"\frac{a}", "x^", "{", r"\unknowncommand{x}"] {
            assert!(
                matches!(to_mathml(latex, false), Err(MathError::Invalid(_))),
                "{latex}"
            );
        }
    }

    #[test]
    fn el_texto_se_escapa() {
        let html = to_mathml(r"\text{<img src=x onerror=alert(1)>}", false).unwrap();
        assert!(!html.contains("<img"), "{html}");
    }

    #[test]
    fn una_formula_muy_anidada_no_cierra_la_app() {
        // Sin las defensas, 300 fracciones anidadas desbordan la pila y abortan el proceso
        // de pruebas entero (no es un panico que se pueda atrapar).
        let deep = format!("{}x{}", r"\frac{1}{".repeat(300), "}".repeat(300));
        assert_eq!(to_mathml(&deep, false), Err(MathError::TooComplex));
    }

    #[test]
    fn el_anidamiento_justo_bajo_el_limite_se_convierte() {
        // 166 fracciones = 498 elementos: el peor caso que sigue convirtiendose. Si la pila
        // no alcanzara, este test abortaria el proceso.
        let deep = format!("{}x{}", r"\frac{1}{".repeat(166), "}".repeat(166));
        assert!(to_mathml(&deep, false).is_ok());
        let chain = format!("{}x", r"\sqrt ".repeat(MAX_TOKENS));
        assert!(to_mathml(&chain, false).is_ok());
        let parens = format!("{}x{}", "(".repeat(MAX_TOKENS), ")".repeat(MAX_TOKENS));
        assert!(to_mathml(&parens, false).is_ok());
    }

    #[test]
    fn los_parentesis_y_corchetes_anidados_tambien_tienen_limite() {
        // Hallado en revision: 32 000 parentesis anidados desbordaban la pila en debug y
        // cerraban la app. Con el limite ni se intenta convertir.
        for (open, close) in [("(", ")"), ("[", "]")] {
            let deep = format!("{}x{}", open.repeat(100_000), close.repeat(100_000));
            assert_eq!(to_mathml(&deep, false), Err(MathError::TooComplex));
        }
    }

    #[test]
    fn una_formula_muy_larga_no_se_convierte() {
        let long = "x+".repeat(MAX_BYTES);
        assert_eq!(to_mathml(&long, false), Err(MathError::TooComplex));
        // Larga pero dentro del tope: se convierte.
        assert!(to_mathml(&"x+".repeat(5_000), false).is_ok());
    }

    #[test]
    fn las_macros_no_se_aceptan() {
        for latex in [
            r"\def\a{\a}\a",
            r"\newcommand{\b}{x}\b",
            r"\renewcommand{\c}{x}",
            r"\gdef\d{x}",
            r"\providecommand{\e}{x}",
        ] {
            assert_eq!(
                to_mathml(latex, false),
                Err(MathError::TooComplex),
                "{latex}"
            );
        }
        // Un comando que solo empieza igual no es una macro.
        assert!(to_mathml(r"\delta", false).is_ok());
    }
}
