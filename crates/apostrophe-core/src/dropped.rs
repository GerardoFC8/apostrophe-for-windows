//! Que hacer con los archivos que se sueltan sobre la ventana.
//!
//! Los documentos Markdown se abren, cada uno en su pestaña; las imagenes se insertan en el
//! documento como `![](ruta)`, con la misma ruta relativa que el selector de "Insertar
//! imagen". Lo demas se ignora: soltar un `.zip` por error no debe hacer nada.

use std::path::{Path, PathBuf};

use crate::image::{extension_of, image_markdown, SUPPORTED_EXTENSIONS};

/// Lo que hay que hacer con una tanda de archivos soltados.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct DropPlan {
    /// Documentos Markdown a abrir, en el orden en que llegaron.
    pub open: Vec<PathBuf>,
    /// Un `![](ruta)` por imagen, listos para insertar.
    pub images: Vec<String>,
    /// Cuantos archivos no eran ni Markdown ni imagen.
    pub ignored: usize,
}

/// Clasifica los archivos soltados. `doc_dir` es la carpeta del documento activo, contra la
/// que se escriben relativas las rutas de las imagenes; `None` si no esta guardado.
pub fn plan_drop(paths: &[PathBuf], doc_dir: Option<&Path>) -> DropPlan {
    let mut plan = DropPlan::default();
    for path in paths {
        let name = path.to_string_lossy();
        let ext = extension_of(&name).map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("md" | "markdown") => plan.open.push(path.clone()),
            Some(e) if SUPPORTED_EXTENSIONS.contains(&e) => {
                plan.images.push(image_markdown(path, doc_dir));
            }
            _ => plan.ignored += 1,
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn separa_documentos_imagenes_y_el_resto() {
        let plan = plan_drop(
            &paths(&[
                "/n/a.md",
                "/n/fotos/gato.PNG",
                "/n/datos.zip",
                "/n/b.markdown",
                "/n/sin-extension",
            ]),
            Some(Path::new("/n")),
        );
        assert_eq!(plan.open, paths(&["/n/a.md", "/n/b.markdown"]));
        assert_eq!(plan.images, vec!["![](fotos/gato.PNG)".to_string()]);
        assert_eq!(plan.ignored, 2);
    }

    #[test]
    fn sin_documento_guardado_la_imagen_va_con_ruta_absoluta() {
        let plan = plan_drop(&paths(&["C:\\fotos\\mi gato.jpg"]), None);
        assert_eq!(plan.images, vec!["![](<C:/fotos/mi gato.jpg>)".to_string()]);
    }

    #[test]
    fn nada_soltado_no_hace_nada() {
        assert_eq!(plan_drop(&[], None), DropPlan::default());
    }
}
