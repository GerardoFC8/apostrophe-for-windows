//! Converter port (fundacion, ADR-0001): construccion PURA de los argumentos de
//! pandoc para la conversion markdown->HTML del preview. El SPAWN del proceso vive
//! en el adaptador (src-tauri); aca solo se arma el vector de args como dato
//! testeable, para empezar a reemplazar el spike hardcodeado de `lib.rs`.
//!
//! ALCANCE ACTUAL (M2): solo el preview (markdown->html5). El export a otros
//! formatos (HTML a disco, PDF/Typst) reusa este builder en M5 — por eso el orden
//! y los flags se modelan como dato, no como string fijo.

/// Opciones del preview markdown->HTML5 (subset del pipeline de Apostrophe).
/// El default replica EXACTAMENTE el spike actual (`src-tauri/lib.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewOptions {
    /// Render de matematica con MathJax (`--mathjax`).
    pub mathjax: bool,
    /// Documento HTML completo con `<head>`/`<body>` (`--standalone`).
    pub standalone: bool,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        // Replica el spike: `--from markdown --to html5 --standalone --mathjax`.
        Self {
            mathjax: true,
            standalone: true,
        }
    }
}

/// Construye los argumentos de pandoc para un preview markdown->HTML5, en el orden
/// del spike: `--from markdown --to html5 [--standalone] [--mathjax]`. Los flags
/// opcionales se omiten si su opcion esta en `false`.
pub fn preview_args(opts: &PreviewOptions) -> Vec<String> {
    let mut args = vec![
        "--from".to_string(),
        "markdown".to_string(),
        "--to".to_string(),
        "html5".to_string(),
    ];
    if opts.standalone {
        args.push("--standalone".to_string());
    }
    if opts.mathjax {
        args.push("--mathjax".to_string());
    }
    args
}

/// Formato de salida para EXPORTAR (M12). Cubre los formatos que el sidecar pandoc
/// resuelve SOLO, sin binarios extra ni recursos de red:
///
/// - `Html`, `Latex`, `Epub`: texto/empaquetado propio de pandoc.
/// - `Odt`, `Docx`: binarios ofimaticos que pandoc genera con sus plantillas internas.
///
/// Deliberadamente FUERA (cada uno necesita algo que hoy no se distribuye):
///
/// - **PDF**: pandoc delega en un motor externo (`--pdf-engine`). Ni `typst` ni
///   `xelatex` vienen con el sidecar, asi que exportar PDF hoy falla con
///   "typst not found". Requiere bundlear typst como segundo sidecar (~21 MB).
/// - **reveal.js / DZSlides**: pandoc emite HTML que referencia `unpkg.com`, asi que
///   las diapositivas no abren sin internet. Upstream lo evita bundleando reveal.js.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Html,
    Odt,
    Docx,
    Epub,
    Latex,
}

impl ExportFormat {
    /// Todos los formatos soportados, en el orden en que se ofrecen al usuario
    /// (HTML primero: es el default historico del boton Exportar).
    pub const ALL: [ExportFormat; 5] = [
        ExportFormat::Html,
        ExportFormat::Odt,
        ExportFormat::Docx,
        ExportFormat::Epub,
        ExportFormat::Latex,
    ];

    /// Valor de `--to` de pandoc.
    pub fn pandoc_to(self) -> &'static str {
        match self {
            ExportFormat::Html => "html5",
            ExportFormat::Odt => "odt",
            ExportFormat::Docx => "docx",
            ExportFormat::Epub => "epub",
            ExportFormat::Latex => "latex",
        }
    }

    /// Extension de archivo sugerida (sin punto).
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Html => "html",
            ExportFormat::Odt => "odt",
            ExportFormat::Docx => "docx",
            ExportFormat::Epub => "epub",
            ExportFormat::Latex => "tex",
        }
    }

    /// Nombre para el filtro del dialogo nativo ("Guardar como tipo").
    pub fn display_name(self) -> &'static str {
        match self {
            ExportFormat::Html => "HTML",
            ExportFormat::Odt => "Documento de LibreOffice",
            ExportFormat::Docx => "Documento de Word",
            ExportFormat::Epub => "Libro EPUB",
            ExportFormat::Latex => "Documento LaTeX",
        }
    }

    /// Deduce el formato desde la extension del archivo que el usuario eligio.
    ///
    /// El dialogo nativo de Windows devuelve SOLO la ruta, no el filtro seleccionado,
    /// asi que la extension es la unica senal disponible. Se compara sin distinguir
    /// mayusculas y tolera el punto inicial (`"PDF"`, `".Docx"` y `"docx"` entran igual).
    /// Devuelve `None` si la extension no corresponde a un formato soportado — el
    /// adaptador traduce eso a un mensaje de error para el usuario.
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        Self::ALL.into_iter().find(|f| f.extension() == ext)
    }

    /// Flags propios del formato, entre `--to` y `--output`.
    ///
    /// - HTML: `--standalone --mathjax` — documento completo y matematica via MathJax.
    ///   NO se agrega `--embed-resources`: pandoc intentaria incrustar MathJax desde una
    ///   ruta local del sistema (la de Debian, que upstream si tiene) y la exportacion
    ///   fallaria en Windows con el sidecar. La matematica se resuelve por CDN.
    /// - LaTeX / EPUB: `--standalone` para que la salida sea un documento completo y no
    ///   un fragmento.
    /// - ODT / DOCX: `--variable=papersize:a4` (igual que upstream). Son binarios que
    ///   pandoc arma con su plantilla interna: `--standalone` no aplica.
    fn format_args(self) -> &'static [&'static str] {
        match self {
            ExportFormat::Html => &["--standalone", "--mathjax"],
            ExportFormat::Latex | ExportFormat::Epub => &["--standalone"],
            ExportFormat::Odt | ExportFormat::Docx => &["--variable=papersize:a4"],
        }
    }
}

/// Construye los args de pandoc para EXPORTAR a `output_path` en `format`:
/// `--from markdown --to <fmt> <flags del formato> --output <path>`. El input
/// (la ruta del temporal con el markdown) lo antepone el adaptador, igual que el preview.
pub fn export_args(format: ExportFormat, output_path: &str) -> Vec<String> {
    let mut args = vec![
        "--from".to_string(),
        "markdown".to_string(),
        "--to".to_string(),
        format.pandoc_to().to_string(),
    ];
    args.extend(format.format_args().iter().map(|s| s.to_string()));
    args.push("--output".to_string());
    args.push(output_path.to_string());
    args
}

#[cfg(test)]
mod tests {
    //! Oraculo: los args hardcodeados del spike `src-tauri/lib.rs::preview_markdown`
    //! (`["--from","markdown","--to","html5","--standalone","--mathjax"]`). El port
    //! NO debe cambiar el comportamiento observable del preview.
    use super::*;

    #[test]
    fn preview_args_default_matches_spike() {
        assert_eq!(
            preview_args(&PreviewOptions::default()),
            ["--from", "markdown", "--to", "html5", "--standalone", "--mathjax"]
        );
    }

    #[test]
    fn preview_args_without_mathjax_omits_flag() {
        let opts = PreviewOptions {
            mathjax: false,
            ..Default::default()
        };
        assert_eq!(
            preview_args(&opts),
            ["--from", "markdown", "--to", "html5", "--standalone"]
        );
    }

    #[test]
    fn preview_args_without_standalone_omits_flag() {
        let opts = PreviewOptions {
            standalone: false,
            mathjax: false,
        };
        assert_eq!(preview_args(&opts), ["--from", "markdown", "--to", "html5"]);
    }

    #[test]
    fn preview_args_standalone_without_mathjax() {
        let opts = PreviewOptions {
            standalone: true,
            mathjax: false,
        };
        assert_eq!(
            preview_args(&opts),
            ["--from", "markdown", "--to", "html5", "--standalone"]
        );
    }

    #[test]
    fn export_args_html_tiene_output_y_standalone() {
        let args = export_args(ExportFormat::Html, "/tmp/out.html");
        assert_eq!(
            args,
            [
                "--from", "markdown", "--to", "html5", "--standalone", "--mathjax", "--output",
                "/tmp/out.html"
            ]
        );
    }

    #[test]
    fn export_format_metadata() {
        assert_eq!(ExportFormat::Html.pandoc_to(), "html5");
        assert_eq!(ExportFormat::Html.extension(), "html");
    }

    // --- M12: exportacion multi-formato ---

    #[test]
    fn export_args_odt_usa_papersize_y_no_standalone() {
        // ODT/DOCX son binarios armados con la plantilla interna de pandoc: --standalone
        // no aplica, y --mathjax tampoco (no hay HTML donde cargar el script).
        assert_eq!(
            export_args(ExportFormat::Odt, "/tmp/out.odt"),
            [
                "--from",
                "markdown",
                "--to",
                "odt",
                "--variable=papersize:a4",
                "--output",
                "/tmp/out.odt"
            ]
        );
    }

    #[test]
    fn export_args_docx_usa_papersize_y_no_standalone() {
        assert_eq!(
            export_args(ExportFormat::Docx, "/tmp/out.docx"),
            [
                "--from",
                "markdown",
                "--to",
                "docx",
                "--variable=papersize:a4",
                "--output",
                "/tmp/out.docx"
            ]
        );
    }

    #[test]
    fn export_args_latex_y_epub_son_standalone_sin_mathjax() {
        assert_eq!(
            export_args(ExportFormat::Latex, "/tmp/out.tex"),
            ["--from", "markdown", "--to", "latex", "--standalone", "--output", "/tmp/out.tex"]
        );
        assert_eq!(
            export_args(ExportFormat::Epub, "/tmp/out.epub"),
            ["--from", "markdown", "--to", "epub", "--standalone", "--output", "/tmp/out.epub"]
        );
    }

    #[test]
    fn html_nunca_incrusta_recursos() {
        // Regresion: --embed-resources junto a --mathjax hace que pandoc busque MathJax
        // en una ruta local del sistema y la exportacion falle en Windows. Ver format_args.
        let args = export_args(ExportFormat::Html, "/tmp/out.html");
        assert!(!args.iter().any(|a| a.contains("embed-resources")));
    }

    #[test]
    fn from_extension_reconoce_todos_los_formatos() {
        for f in ExportFormat::ALL {
            assert_eq!(ExportFormat::from_extension(f.extension()), Some(f));
        }
    }

    #[test]
    fn from_extension_tolera_mayusculas_y_punto() {
        assert_eq!(ExportFormat::from_extension(".DOCX"), Some(ExportFormat::Docx));
        assert_eq!(ExportFormat::from_extension("Tex"), Some(ExportFormat::Latex));
        assert_eq!(ExportFormat::from_extension("HTML"), Some(ExportFormat::Html));
    }

    #[test]
    fn from_extension_rechaza_formatos_no_soportados() {
        // PDF necesita un motor externo (typst/xelatex) que hoy no se distribuye.
        assert_eq!(ExportFormat::from_extension("pdf"), None);
        assert_eq!(ExportFormat::from_extension("md"), None);
        assert_eq!(ExportFormat::from_extension(""), None);
    }

    #[test]
    fn extensiones_y_nombres_no_se_repiten() {
        // from_extension resuelve por extension: si dos formatos la compartieran, uno
        // quedaria inalcanzable desde el dialogo.
        let mut exts: Vec<_> = ExportFormat::ALL.iter().map(|f| f.extension()).collect();
        exts.sort_unstable();
        let total = exts.len();
        exts.dedup();
        assert_eq!(exts.len(), total, "hay extensiones duplicadas entre formatos");
    }
}
