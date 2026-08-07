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

/// Formato de salida para EXPORTAR (M12).
///
/// - `Html`, `Latex`, `Epub`: texto/empaquetado propio de pandoc.
/// - `Odt`, `Docx`: binarios ofimaticos que pandoc genera con sus plantillas internas.
/// - `Pdf`: pandoc NO sabe generar PDF por su cuenta; delega en un motor externo. Se usa
///   typst, que se distribuye como segundo sidecar (~21 MB, un unico binario). La
///   alternativa, xelatex, exigiria TeXLive (gigabytes). Upstream tomo la misma decision.
///
/// Deliberadamente FUERA: **reveal.js / DZSlides**. Pandoc emite HTML que referencia
/// `unpkg.com`, asi que las diapositivas no abren sin internet. Upstream lo evita
/// bundleando reveal.js; mientras no se bundlee, no se ofrecen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Html,
    Pdf,
    Odt,
    Docx,
    Epub,
    Latex,
}

impl ExportFormat {
    /// Todos los formatos soportados, en el orden en que se ofrecen al usuario.
    /// HTML va primero a proposito: es el filtro preseleccionado del dialogo y el
    /// default historico del boton Exportar. Cambiarlo cambiaria el comportamiento
    /// de quien solo pulsa Ctrl+E y guarda.
    pub const ALL: [ExportFormat; 6] = [
        ExportFormat::Html,
        ExportFormat::Pdf,
        ExportFormat::Odt,
        ExportFormat::Docx,
        ExportFormat::Epub,
        ExportFormat::Latex,
    ];

    /// Valor de `--to` de pandoc.
    pub fn pandoc_to(self) -> &'static str {
        match self {
            ExportFormat::Html => "html5",
            ExportFormat::Pdf => "pdf",
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
            ExportFormat::Pdf => "pdf",
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
            ExportFormat::Pdf => "Documento PDF",
            ExportFormat::Odt => "Documento de LibreOffice",
            ExportFormat::Docx => "Documento de Word",
            ExportFormat::Epub => "Libro EPUB",
            ExportFormat::Latex => "Documento LaTeX",
        }
    }

    /// `true` si el formato necesita el sidecar typst para producirse.
    /// El adaptador lo consulta para resolver la ruta del motor ANTES de lanzar pandoc
    /// y fallar con un mensaje claro si el sidecar no esta.
    pub fn needs_pdf_engine(self) -> bool {
        matches!(self, ExportFormat::Pdf)
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
    /// - ODT / DOCX / PDF: `--variable=papersize:a4` (igual que upstream). Los dos
    ///   primeros son binarios que pandoc arma con su plantilla interna, asi que
    ///   `--standalone` no aplica.
    /// - PDF ademas: `--pdf-engine=<ruta>` con la ruta ABSOLUTA del sidecar typst. No
    ///   alcanza con `--pdf-engine=typst`: pandoc buscaria `typst` en el PATH y el
    ///   sidecar vive junto al ejecutable de la app, que no esta en el PATH.
    /// - HTML ademas: `--include-in-header=<ruta>` con la hoja de estilos del preview.
    ///   Se usa `--include-in-header` y NO `--css` (lo que hace upstream) porque `--css`
    ///   agrega un `<link>` a un archivo externo: el HTML dejaria de ser portable. Este
    ///   flag inyecta el `<style>` INLINE y el archivo se puede compartir suelto.
    fn format_args(self, opts: &ExportOptions) -> Vec<String> {
        let owned = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        match self {
            ExportFormat::Html => {
                let mut args = owned(&["--standalone", "--mathjax"]);
                if let Some(header) = opts.header_include {
                    args.push(format!("--include-in-header={header}"));
                }
                args
            }
            ExportFormat::Latex | ExportFormat::Epub => owned(&["--standalone"]),
            ExportFormat::Odt | ExportFormat::Docx => owned(&["--variable=papersize:a4"]),
            ExportFormat::Pdf => {
                let mut args = Vec::new();
                // Si falta la ruta se omite el flag: pandoc caera de vuelta al PATH y
                // fallara con "typst not found". El adaptador resuelve la ruta antes de
                // llegar aca, asi que en la app esta rama no deberia darse.
                if let Some(engine) = opts.pdf_engine {
                    args.push(format!("--pdf-engine={engine}"));
                }
                args.push("--variable=papersize:a4".to_string());
                args
            }
        }
    }
}

/// Entradas OPCIONALES del export que dependen del entorno (rutas que solo el adaptador
/// conoce). Cada una la consume un unico formato; el resto las ignora.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportOptions<'a> {
    /// Ruta absoluta al sidecar typst. Solo la usa `Pdf`.
    pub pdf_engine: Option<&'a str>,
    /// Ruta a un fragmento HTML (un `<style>`) que se inyecta en el `<head>`.
    /// Solo la usa `Html`. Es como viaja la hoja de estilos del preview al archivo.
    pub header_include: Option<&'a str>,
}

/// Construye los args de pandoc para EXPORTAR a `output_path` en `format`:
/// `--from markdown --to <fmt> <flags del formato> --output <path>`. El input
/// (la ruta del temporal con el markdown) lo antepone el adaptador, igual que el preview.
pub fn export_args(format: ExportFormat, output_path: &str, opts: &ExportOptions) -> Vec<String> {
    let mut args = vec![
        "--from".to_string(),
        "markdown".to_string(),
        "--to".to_string(),
        format.pandoc_to().to_string(),
    ];
    args.extend(format.format_args(opts));
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
        let args = export_args(ExportFormat::Html, "/tmp/out.html", &ExportOptions::default());
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
            export_args(ExportFormat::Odt, "/tmp/out.odt", &ExportOptions::default()),
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
            export_args(ExportFormat::Docx, "/tmp/out.docx", &ExportOptions::default()),
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
            export_args(ExportFormat::Latex, "/tmp/out.tex", &ExportOptions::default()),
            ["--from", "markdown", "--to", "latex", "--standalone", "--output", "/tmp/out.tex"]
        );
        assert_eq!(
            export_args(ExportFormat::Epub, "/tmp/out.epub", &ExportOptions::default()),
            ["--from", "markdown", "--to", "epub", "--standalone", "--output", "/tmp/out.epub"]
        );
    }

    #[test]
    fn html_nunca_incrusta_recursos() {
        // Regresion: --embed-resources junto a --mathjax hace que pandoc busque MathJax
        // en una ruta local del sistema y la exportacion falle en Windows. Ver format_args.
        let args = export_args(ExportFormat::Html, "/tmp/out.html", &ExportOptions::default());
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
        assert_eq!(ExportFormat::from_extension("md"), None);
        assert_eq!(ExportFormat::from_extension("docm"), None);
        assert_eq!(ExportFormat::from_extension(""), None);
    }

    // --- M13: PDF via sidecar typst ---

    #[test]
    fn export_args_pdf_usa_la_ruta_absoluta_del_motor() {
        // `--pdf-engine=typst` a secas NO sirve: pandoc lo buscaria en el PATH y el
        // sidecar vive junto al ejecutable de la app. Tiene que ir la ruta completa.
        assert_eq!(
            export_args(ExportFormat::Pdf, "/tmp/out.pdf", &ExportOptions { pdf_engine: Some("/opt/app/typst"), ..Default::default() }),
            [
                "--from",
                "markdown",
                "--to",
                "pdf",
                "--pdf-engine=/opt/app/typst",
                "--variable=papersize:a4",
                "--output",
                "/tmp/out.pdf"
            ]
        );
    }

    #[test]
    fn export_args_pdf_admite_rutas_con_espacios() {
        // El clon de Windows vive bajo "PROYECTOS DEMO": la ruta llega con espacios y
        // debe viajar como UN solo argumento, sin comillas ni escapes agregados.
        let args = export_args(
            ExportFormat::Pdf,
            "/tmp/out.pdf",
            &ExportOptions {
                pdf_engine: Some(r"C:\PROYECTOS DEMO\typst.exe"),
                ..Default::default()
            },
        );
        assert!(args.contains(&r"--pdf-engine=C:\PROYECTOS DEMO\typst.exe".to_string()));
    }

    #[test]
    fn export_args_pdf_sin_motor_omite_el_flag() {
        let args = export_args(ExportFormat::Pdf, "/tmp/out.pdf", &ExportOptions::default());
        assert!(!args.iter().any(|a| a.starts_with("--pdf-engine")));
        assert!(args.contains(&"--variable=papersize:a4".to_string()));
    }

    #[test]
    fn el_motor_pdf_no_contamina_otros_formatos() {
        // Pasar la ruta con un formato que no es PDF no debe agregar el flag.
        for format in ExportFormat::ALL.into_iter().filter(|f| *f != ExportFormat::Pdf) {
            let args = export_args(
                format,
                "/tmp/out",
                &ExportOptions {
                    pdf_engine: Some("/opt/app/typst"),
                    ..Default::default()
                },
            );
            assert!(
                !args.iter().any(|a| a.starts_with("--pdf-engine")),
                "{format:?} no deberia recibir --pdf-engine"
            );
        }
    }

    #[test]
    fn solo_pdf_necesita_motor_externo() {
        for format in ExportFormat::ALL {
            assert_eq!(
                format.needs_pdf_engine(),
                format == ExportFormat::Pdf,
                "{format:?}"
            );
        }
    }

    // --- M14: la hoja de estilos del preview viaja al HTML exportado ---

    #[test]
    fn html_inyecta_la_hoja_de_estilos_en_el_head() {
        let args = export_args(
            ExportFormat::Html,
            "/tmp/out.html",
            &ExportOptions {
                header_include: Some("/tmp/estilo.html"),
                ..Default::default()
            },
        );
        assert_eq!(
            args,
            [
                "--from",
                "markdown",
                "--to",
                "html5",
                "--standalone",
                "--mathjax",
                "--include-in-header=/tmp/estilo.html",
                "--output",
                "/tmp/out.html"
            ]
        );
    }

    #[test]
    fn html_nunca_enlaza_una_hoja_externa() {
        // Regresion: `--css` (lo que usa upstream) agrega un <link> a un archivo suelto y
        // el HTML deja de ser portable. Tiene que ir inline via --include-in-header.
        let args = export_args(
            ExportFormat::Html,
            "/tmp/out.html",
            &ExportOptions {
                header_include: Some("/tmp/estilo.html"),
                ..Default::default()
            },
        );
        assert!(!args.iter().any(|a| a.starts_with("--css")));
    }

    #[test]
    fn html_sin_hoja_de_estilos_omite_el_flag() {
        let args = export_args(ExportFormat::Html, "/tmp/out.html", &ExportOptions::default());
        assert!(!args.iter().any(|a| a.starts_with("--include-in-header")));
    }

    #[test]
    fn la_hoja_de_estilos_no_contamina_otros_formatos() {
        // Solo HTML entiende un <style>: inyectarlo en un DOCX o un PDF no tiene sentido.
        for format in ExportFormat::ALL.into_iter().filter(|f| *f != ExportFormat::Html) {
            let args = export_args(
                format,
                "/tmp/out",
                &ExportOptions {
                    header_include: Some("/tmp/estilo.html"),
                    ..Default::default()
                },
            );
            assert!(
                !args.iter().any(|a| a.starts_with("--include-in-header")),
                "{format:?} no deberia recibir --include-in-header"
            );
        }
    }

    #[test]
    fn pdf_esta_en_la_lista_y_html_sigue_primero() {
        // El primer formato es el filtro preseleccionado del dialogo: si HTML dejara de
        // ir primero, cambiaria en silencio lo que obtiene quien solo pulsa Ctrl+E.
        assert_eq!(ExportFormat::ALL[0], ExportFormat::Html);
        assert!(ExportFormat::ALL.contains(&ExportFormat::Pdf));
        assert_eq!(ExportFormat::from_extension("pdf"), Some(ExportFormat::Pdf));
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
