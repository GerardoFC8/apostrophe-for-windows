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

/// Formato de salida para EXPORTAR (M5). HTML primero; PDF (Typst)/DOCX/etc. post-MVP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Html,
}

impl ExportFormat {
    /// Valor de `--to` de pandoc.
    pub fn pandoc_to(self) -> &'static str {
        match self {
            ExportFormat::Html => "html5",
        }
    }

    /// Extension de archivo sugerida (sin punto).
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Html => "html",
        }
    }
}

/// Construye los args de pandoc para EXPORTAR a `output_path` en `format`:
/// `--from markdown --to <fmt> --standalone --mathjax --output <path>`. El input
/// (la ruta del temporal con el markdown) lo antepone el adaptador, igual que el preview.
pub fn export_args(format: ExportFormat, output_path: &str) -> Vec<String> {
    vec![
        "--from".to_string(),
        "markdown".to_string(),
        "--to".to_string(),
        format.pandoc_to().to_string(),
        "--standalone".to_string(),
        "--mathjax".to_string(),
        "--output".to_string(),
        output_path.to_string(),
    ]
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
}
