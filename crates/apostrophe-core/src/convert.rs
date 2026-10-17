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
/// - `Pdf`: pandoc NO sabe generar PDF por su cuenta. Se usa typst, que se distribuye
///   como segundo sidecar (~21 MB, un unico binario). La alternativa, xelatex, exigiria
///   TeXLive (gigabytes). Upstream tomo la misma decision. Son DOS pasos: pandoc escribe
///   el fuente typst (`export_args`) y typst lo compila (`typst_compile_args`).
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

    /// Valor de `--to` de pandoc. Para `Pdf` es `typst`: pandoc solo escribe el fuente,
    /// el PDF lo compila typst en un segundo paso.
    pub fn pandoc_to(self) -> &'static str {
        match self {
            ExportFormat::Html => "html5",
            ExportFormat::Pdf => "typst",
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
    /// El adaptador lo consulta para comprobar que el sidecar existe ANTES de lanzar
    /// nada y fallar con un mensaje claro si no esta.
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
    /// - PDF ademas: `--standalone` (el fuente typst lleva la plantilla, que es donde
    ///   actuan las variables de tipografia) y `--extract-media=media`, RELATIVO. Ver
    ///   `PDF_MEDIA_DIR` para por que no sirve `--pdf-engine` ni una ruta absoluta.
    ///   Si hay carpeta de documento, `--defaults=<archivo>`: ver `resource_path_defaults`.
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
                if let Some(defaults) = opts.resource_defaults {
                    args.push(format!("--defaults={defaults}"));
                }
                args.push("--standalone".to_string());
                args.push(format!("--extract-media={PDF_MEDIA_DIR}"));
                args.push("--variable=papersize:a4".to_string());
                args.extend(owned(PDF_TYPOGRAPHY));
                if let Some(color) = opts.link_color {
                    // typst quiere el hex SIN `#`. La plantilla de pandoc genera
                    // `rgb(content-to-string(linkcolor))`, y `[#3584e4]` en typst es
                    // CODIGO: evalua 3584e4 como notacion cientifica (35 840 000), el
                    // color sale invalido y el texto del enlace se vuelve invisible.
                    args.push(format!("--variable=linkcolor:{}", color.trim_start_matches('#')));
                }
                args
            }
        }
    }
}

/// Tipografia del PDF, alineada con el preview (`markdownStylesheet` en `main.ts`).
///
/// El preview pide `system-ui`, que en Windows resuelve a **Segoe UI**, y `ui-monospace`,
/// que resuelve a **Consolas**: nombrarlas explicitamente no aproxima el aspecto del
/// preview, usa la misma fuente. `linestretch` replica el `line-height:1.6` del preview.
///
/// Si una fuente no existe (por ejemplo al desarrollar en WSL), typst solo emite un
/// warning y cae a su fuente por defecto — el PDF se genera igual.
const PDF_TYPOGRAPHY: &[&str] = &[
    "--variable=mainfont:Segoe UI",
    "--variable=codefont:Consolas",
    "--variable=fontsize:11pt",
    "--variable=linestretch:1.6",
    "--variable=margin.x:2.5cm",
    "--variable=margin.y:2.5cm",
];

/// Carpeta donde pandoc extrae las imagenes del PDF, RELATIVA a su directorio de trabajo.
///
/// Es relativa a proposito. pandoc escribe en el fuente typst la ruta TAL CUAL la recibe:
/// con una absoluta saldria `image("/tmp/trabajo/media/gato.png")`, y typst lee una ruta
/// que empieza con `/` contra la raiz de SU proyecto (`--root`), no contra la del disco
/// — en Windows ademas ignora la unidad. La imagen no se encontraria y el PDF fallaria
/// entero. Es exactamente lo que pasaba con `--pdf-engine`: pandoc extraia a su propio
/// `/tmp/media-…` con ruta absoluta.
///
/// El contrato con el adaptador: pandoc corre CON el directorio de trabajo del export
/// como directorio actual y escribe ahi el `.typ`, y typst compila con `--root` en ese
/// mismo directorio. Asi el fuente dice `image("media/gato.png")` y todo vive bajo la raiz.
pub const PDF_MEDIA_DIR: &str = "media";

/// Contenido de un archivo de defaults de pandoc que agrega `dir` a sus rutas de busqueda
/// de recursos. Es como el PDF encuentra las imagenes relativas del documento.
///
/// El PDF no puede resolverlas lanzando pandoc desde la carpeta del documento, como el
/// resto de los formatos, porque su directorio actual tiene que ser el de trabajo (ver
/// `PDF_MEDIA_DIR`). Y `--resource-path=<dir>` no sirve: pandoc parte el valor por el
/// separador de rutas de busqueda (`;` en Windows, `:` en el resto), asi que una carpeta
/// "notas; 2026" se volveria dos rutas inexistentes. En el archivo de defaults la ruta va
/// como un ELEMENTO de lista YAML y no se parte.
///
/// Va entre comillas simples, donde la unica secuencia especial es `''`: la barra
/// invertida de Windows queda literal. Limite conocido: pandoc interpola `${VAR}` en las
/// rutas de los defaults, asi que una carpeta llamada literalmente `${HOME}` no resolveria.
pub fn resource_path_defaults(dir: &str) -> String {
    format!("resource-path:\n- '{}'\n", dir.replace('\'', "''"))
}

/// Paso 2 del PDF: `compile <fuente.typ> <salida.pdf> --root <raiz>`. `root` es el
/// directorio de trabajo donde pandoc dejo el fuente y sus imagenes (ver `PDF_MEDIA_DIR`):
/// typst se niega a leer archivos fuera de su raiz.
pub fn typst_compile_args(source: &str, output_pdf: &str, root: &str) -> Vec<String> {
    vec![
        "compile".to_string(),
        source.to_string(),
        output_pdf.to_string(),
        "--root".to_string(),
        root.to_string(),
    ]
}

/// Entradas OPCIONALES del export que dependen del entorno (rutas y colores que solo el
/// adaptador conoce). Cada una la consume un unico formato; el resto las ignora.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportOptions<'a> {
    /// Ruta a un fragmento HTML (un `<style>`) que se inyecta en el `<head>`.
    /// Solo la usa `Html`. Es como viaja la hoja de estilos del preview al archivo.
    pub header_include: Option<&'a str>,
    /// Color de los enlaces en hex, con o sin `#`. Solo lo usa `Pdf`; en HTML el color
    /// ya viaja dentro de la hoja de estilos. Sale de la misma var `--accent` del tema,
    /// para no tener dos fuentes de verdad del color de acento.
    pub link_color: Option<&'a str>,
    /// Ruta a un archivo de defaults de pandoc con la carpeta del documento (ver
    /// `resource_path_defaults`). Solo la usa `Pdf`; `None` si el documento nunca se
    /// guardo, porque no hay carpeta contra la cual resolver sus imagenes.
    pub resource_defaults: Option<&'a str>,
}

/// Construye los args de pandoc para EXPORTAR a `output_path` en `format`:
/// `--from markdown --to <fmt> <flags del formato> --output <path>`. El input (la ruta del
/// temporal con el markdown) lo antepone el adaptador, igual que el preview.
///
/// Para `Pdf`, `output_path` es el fuente `.typ` intermedio, no el PDF: ver
/// `typst_compile_args`.
///
/// Las imagenes RELATIVAS del documento no se resuelven con flags: el adaptador lanza
/// pandoc con la carpeta del documento como directorio actual (salvo el PDF, que usa
/// `resource_defaults`). Asi no hace falta `--resource-path`, que parte rutas con `;`.
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
        // Regresion: `--embed-resources` no se limita a las imagenes, tambien absorbe el
        // bundle de MathJax que enlaza `--mathjax`. Eso obliga a tener red en el momento de
        // exportar y engorda el archivo — medido con el pandoc del sidecar (3.9.0.2), una
        // sola formula lleva el HTML de 3,9 KB a 1,3 MB.
        //
        // Las imagenes del HTML se incrustan en el adaptador con el mismo camino que el
        // preview (`rewrite_img_srcs`), que toca solo los `<img>`. Ver `embed_html_images`.
        let args = export_args(ExportFormat::Html, "/tmp/out.html", &ExportOptions::default());
        assert!(!args.iter().any(|a| a.contains("embed-resources")));
    }

    #[test]
    fn ningun_formato_usa_resource_path() {
        // `--resource-path` parte su valor por el separador de rutas de busqueda (`;` en
        // Windows, `:` en el resto): una carpeta "notas; 2026" se volveria dos rutas que
        // no existen y la imagen se perderia. Las imagenes relativas se resuelven lanzando
        // pandoc DESDE la carpeta del documento (lo decide el adaptador) y, en el PDF, con
        // un archivo de defaults, que recibe la ruta como un elemento de lista.
        for format in ExportFormat::ALL {
            let opts = ExportOptions {
                resource_defaults: Some("/tmp/trabajo/recursos.yaml"),
                ..Default::default()
            };
            let args = export_args(format, "/tmp/out", &opts);
            assert!(!args.iter().any(|a| a.starts_with("--resource-path")), "{format:?}");
        }
    }

    #[test]
    fn los_defaults_de_recursos_listan_la_carpeta_sin_partirla() {
        assert_eq!(
            resource_path_defaults(r"C:\PROYECTOS DEMO\notas; 2026"),
            "resource-path:\n- 'C:\\PROYECTOS DEMO\\notas; 2026'\n"
        );
    }

    #[test]
    fn los_defaults_de_recursos_escapan_la_comilla_simple() {
        // En YAML entre comillas simples la unica secuencia especial es `''`. La barra
        // invertida de Windows queda literal, que es justo lo que hace falta.
        assert_eq!(
            resource_path_defaults("/home/ana/it's"),
            "resource-path:\n- '/home/ana/it''s'\n"
        );
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

    // --- M13: PDF via sidecar typst, en DOS pasos ---

    #[test]
    fn el_pdf_lo_prepara_pandoc_como_fuente_typst() {
        // Paso 1: pandoc NO produce el PDF. Escribe el fuente typst completo (con la
        // plantilla, por eso `--standalone`) y extrae las imagenes junto a el.
        let opts = ExportOptions::default();
        let args = export_args(ExportFormat::Pdf, "/tmp/trabajo/documento.typ", &opts);
        assert_eq!(&args[..4], ["--from", "markdown", "--to", "typst"]);
        assert!(args.contains(&"--standalone".to_string()));
        assert_eq!(args[args.len() - 2..], ["--output", "/tmp/trabajo/documento.typ"]);
    }

    #[test]
    fn el_pdf_extrae_las_imagenes_a_una_ruta_relativa() {
        // Regresion: con `--pdf-engine` pandoc copiaba la imagen a `/tmp/media-…` y
        // escribia `image("/tmp/media-…/gato.png")`. typst lee una ruta que empieza con
        // `/` contra la RAIZ del proyecto, no del disco, y el PDF fallaba entero. Con la
        // ruta relativa, el fuente dice `image("media/gato.png")` y typst la encuentra
        // junto al `.typ`, en cualquier sistema y en cualquier unidad.
        let args = export_args(ExportFormat::Pdf, "/tmp/out.typ", &ExportOptions::default());
        assert!(args.contains(&format!("--extract-media={PDF_MEDIA_DIR}")));
        assert!(!PDF_MEDIA_DIR.starts_with('/') && !PDF_MEDIA_DIR.contains(':'));
    }

    #[test]
    fn el_pdf_ya_no_delega_en_un_motor() {
        let args = export_args(ExportFormat::Pdf, "/tmp/out.typ", &ExportOptions::default());
        assert!(!args.iter().any(|a| a.starts_with("--pdf-engine")));
        assert!(!args.iter().any(|a| a == "pdf"));
    }

    #[test]
    fn el_pdf_lee_la_carpeta_del_documento_desde_los_defaults() {
        let args = export_args(
            ExportFormat::Pdf,
            "/tmp/out.typ",
            &ExportOptions {
                resource_defaults: Some(r"C:\Temp\trabajo\recursos.yaml"),
                ..Default::default()
            },
        );
        assert!(args.contains(&r"--defaults=C:\Temp\trabajo\recursos.yaml".to_string()));
    }

    #[test]
    fn sin_documento_guardado_el_pdf_no_lleva_defaults() {
        let args = export_args(ExportFormat::Pdf, "/tmp/out.typ", &ExportOptions::default());
        assert!(!args.iter().any(|a| a.starts_with("--defaults")));
    }

    #[test]
    fn los_defaults_de_recursos_no_contaminan_otros_formatos() {
        // Fuera del PDF, pandoc corre desde la carpeta del documento y no los necesita.
        for format in ExportFormat::ALL.into_iter().filter(|f| *f != ExportFormat::Pdf) {
            let args = export_args(
                format,
                "/tmp/out",
                &ExportOptions {
                    resource_defaults: Some("/tmp/recursos.yaml"),
                    ..Default::default()
                },
            );
            assert!(!args.iter().any(|a| a.starts_with("--defaults")), "{format:?}");
        }
    }

    #[test]
    fn la_extraccion_de_imagenes_no_contamina_otros_formatos() {
        // DOCX, ODT y EPUB ya meten la imagen en su propio paquete; en HTML y LaTeX
        // `--extract-media` reescribiria el `src` a una carpeta temporal que se borra.
        for format in ExportFormat::ALL.into_iter().filter(|f| *f != ExportFormat::Pdf) {
            let args = export_args(format, "/tmp/out", &ExportOptions::default());
            assert!(!args.iter().any(|a| a.starts_with("--extract-media")), "{format:?}");
        }
    }

    #[test]
    fn typst_compila_con_la_raiz_en_el_directorio_de_trabajo() {
        // Paso 2: typst solo lee archivos DENTRO de `--root`. Las imagenes extraidas viven
        // ahi, asi que con esa raiz las encuentra aunque el documento este en otra unidad.
        assert_eq!(
            typst_compile_args(
                r"C:\Temp\trabajo\documento.typ",
                r"D:\PROYECTOS DEMO\salida.pdf",
                r"C:\Temp\trabajo"
            ),
            [
                "compile",
                r"C:\Temp\trabajo\documento.typ",
                r"D:\PROYECTOS DEMO\salida.pdf",
                "--root",
                r"C:\Temp\trabajo"
            ]
        );
    }

    #[test]
    fn pdf_lleva_la_tipografia_del_preview() {
        let args = export_args(ExportFormat::Pdf, "/tmp/out.typ", &ExportOptions::default());
        // system-ui y ui-monospace del preview resuelven a estas dos en Windows.
        assert!(args.contains(&"--variable=mainfont:Segoe UI".to_string()));
        assert!(args.contains(&"--variable=codefont:Consolas".to_string()));
        // line-height:1.6 del preview.
        assert!(args.contains(&"--variable=linestretch:1.6".to_string()));
        assert!(args.contains(&"--variable=papersize:a4".to_string()));
        assert_eq!(args.last().unwrap(), "/tmp/out.typ");
    }

    #[test]
    fn la_tipografia_del_pdf_no_contamina_otros_formatos() {
        for format in ExportFormat::ALL.into_iter().filter(|f| *f != ExportFormat::Pdf) {
            let args = export_args(format, "/tmp/out", &ExportOptions::default());
            assert!(
                !args.iter().any(|a| a.contains("mainfont") || a.contains("linestretch")),
                "{format:?} no deberia recibir tipografia de PDF"
            );
        }
    }

    #[test]
    fn el_color_de_enlace_del_pdf_va_sin_almohadilla() {
        // Regresion de un bug que solo se ve renderizando: la plantilla typst de pandoc
        // hace `rgb(content-to-string(linkcolor))`, y `[#3584e4]` es CODIGO en typst —
        // evalua 3584e4 como notacion cientifica. El color queda invalido y el texto del
        // enlace se vuelve INVISIBLE en el PDF. Sin `#`, se lee como texto y funciona.
        for entrada in ["#3584e4", "3584e4"] {
            let args = export_args(
                ExportFormat::Pdf,
                "/tmp/out.typ",
                &ExportOptions {
                    link_color: Some(entrada),
                    ..Default::default()
                },
            );
            assert!(
                args.contains(&"--variable=linkcolor:3584e4".to_string()),
                "entrada {entrada:?} deberia normalizarse sin almohadilla"
            );
            assert!(!args.iter().any(|a| a.contains('#')));
        }
    }

    #[test]
    fn el_color_de_enlace_no_contamina_otros_formatos() {
        // En HTML el color ya viaja dentro de la hoja de estilos.
        for format in ExportFormat::ALL.into_iter().filter(|f| *f != ExportFormat::Pdf) {
            let args = export_args(
                format,
                "/tmp/out",
                &ExportOptions {
                    link_color: Some("#3584e4"),
                    ..Default::default()
                },
            );
            assert!(!args.iter().any(|a| a.contains("linkcolor")), "{format:?}");
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
