//! La CLI `elision` de punta a punta: el ejecutable real (`CARGO_BIN_EXE_elision`) con
//! stdin, stdout y stderr de verdad, variables de entorno y su propio directorio de
//! trabajo. Lo que se prueba aqui es el ejecutable; la logica ya la prueban en proceso
//! los tests de `apostrophe-cli`.
//!
//! El ejecutable queda en `target/<perfil>/`, donde `tauri-build` copia pandoc, typst y
//! `dictionaries/`: la misma disposicion que deja el instalador. Los tests que necesitan
//! esos recursos se saltan avisando si faltan.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use apostrophe_cli::DICT_DIR_VAR;
use serde_json::Value;

const ELISION: &str = env!("CARGO_BIN_EXE_elision");

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Un PNG de 4x4 valido.
const PNG: [u8; 73] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x08, 0x02, 0x00, 0x00, 0x00, 0x26, 0x93, 0x09,
    0x29, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x47, 0x0c, 0xc4, 0x71, 0x00, 0xae, 0x93, 0x0f, 0xf1, 0xd0, 0x5f, 0x23, 0x9e, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Lo que dejo una ejecucion del proceso.
struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Outcome {
    /// stdout como el UNICO objeto JSON de la salida, que ademas es ASCII puro.
    fn json(&self) -> Value {
        assert!(
            self.stdout.is_ascii(),
            "stdout no es ASCII: {}",
            self.stdout
        );
        assert_eq!(
            self.stdout.matches('\n').count(),
            1,
            "se esperaba una sola linea: {}",
            self.stdout
        );
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| panic!("{e}: {}", self.stdout))
    }
}

/// Carpeta temporal propia del test (el directorio de trabajo del proceso), que se borra
/// al terminar.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "elision-e2e-{}-{}-{label}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// Escribe `content` en `rel` (separado con `/`) y devuelve la ruta absoluta, con el
    /// separador de la plataforma, como la informa la CLI.
    fn file(&self, rel: &str, content: impl AsRef<[u8]>) -> PathBuf {
        let path = rel
            .split('/')
            .fold(self.0.clone(), |path, part| path.join(part));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Ejecuta `elision` en esta carpeta, sin la variable del diccionario de quien corre
    /// los tests (salvo que el test la defina).
    fn run(&self, args: &[&str]) -> Outcome {
        self.run_with(args, b"", &[])
    }

    fn run_with(&self, args: &[&str], stdin: &[u8], vars: &[(&str, &Path)]) -> Outcome {
        let mut command = Command::new(ELISION);
        command
            .args(args)
            .current_dir(&self.0)
            .env_remove(DICT_DIR_VAR)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in vars {
            command.env(name, value);
        }
        let mut child = command.spawn().expect("no se pudo lanzar elision");
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        let output = child.wait_with_output().unwrap();
        Outcome {
            code: output.status.code().expect("elision termino por una senal"),
            stdout: String::from_utf8(output.stdout).expect("stdout no es UTF-8"),
            stderr: String::from_utf8(output.stderr).expect("stderr no es UTF-8"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// La carpeta del ejecutable: donde la CLI busca sus sidecars y su diccionario.
fn install_dir() -> PathBuf {
    Path::new(ELISION).parent().unwrap().to_path_buf()
}

/// `true` si pandoc y typst estan junto al ejecutable.
fn has_sidecars() -> bool {
    ["pandoc", "typst"].iter().all(|name| {
        let path = apostrophe_sidecars::sidecar_path_in(&install_dir(), name);
        let found = path.is_file();
        if !found {
            eprintln!(
                "SALTADO: falta el sidecar {name} en {}. Ejecuta scripts/fetch-{name}.ps1.",
                path.display()
            );
        }
        found
    })
}

/// `true` si el diccionario `es` esta junto al ejecutable.
fn has_dictionary() -> bool {
    let path = install_dir().join("dictionaries").join("es.aff");
    let found = path.is_file();
    if !found {
        eprintln!(
            "SALTADO: falta {}. Ejecuta scripts/fetch-dictionaries.ps1.",
            path.display()
        );
    }
    found
}

#[test]
fn la_ayuda_y_la_version_van_a_stdout_y_terminan_con_0() {
    let dir = Scratch::new("help");
    let help = dir.run(&["--help"]);
    assert_eq!(help.code, 0, "{}", help.stderr);
    assert_eq!(help.stderr, "");
    assert!(help.stdout.contains("Usage: elision"), "{}", help.stdout);
    for command in ["export", "stats", "tables", "spell", "open"] {
        assert!(help.stdout.contains(command), "falta {command}");
    }

    let export = dir.run(&["export", "--help"]);
    assert_eq!(export.code, 0);
    assert!(
        export.stdout.contains("Usage: elision export"),
        "{}",
        export.stdout
    );

    let version = dir.run(&["--version"]);
    assert_eq!(version.code, 0);
    assert!(version.stdout.starts_with("elision "), "{}", version.stdout);
}

#[test]
fn un_uso_invalido_termina_con_2_y_el_diagnostico_va_a_stderr() {
    let dir = Scratch::new("usage");
    dir.file("doc.md", "Hola");
    let text = dir.run(&["export", "doc.md"]);
    assert_eq!(text.code, 2);
    assert_eq!(text.stdout, "");
    assert!(text.stderr.contains("Usage: elision"), "{}", text.stderr);

    // Con `--json` es al reves: el sobre de error a stdout y stderr vacio.
    let json = dir.run(&["--json", "export", "doc.md"]);
    assert_eq!(json.code, 2);
    assert_eq!(json.stderr, "");
    assert_eq!(json.json()["error"]["code"], "usage");
}

#[test]
fn stats_en_json_es_ascii_y_resuelve_la_ruta_contra_el_directorio_actual() {
    let dir = Scratch::new("stats");
    let text = "Él escribió una canción 😀.";
    let doc = dir.file("notas/canción.md", text);
    let out = dir.run(&["stats", "notas/canción.md", "--json"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stderr, "");
    let json = out.json();
    assert_eq!(json["ok"], true);
    // Al parsear se recupera el texto original, escapado como `\uXXXX` en los bytes.
    assert_eq!(json["file"], doc.to_string_lossy().as_ref());
    assert!(out.stdout.contains("canci\\u00f3n.md"), "{}", out.stdout);
    assert_eq!(
        json["stats"],
        serde_json::to_value(apostrophe_core::stats::count(text)).unwrap()
    );

    // Y por la entrada estandar real.
    let stdin = dir.run_with(&["stats", "-", "--json"], "uno dos".as_bytes(), &[]);
    assert_eq!(stdin.code, 0);
    assert_eq!(stdin.json()["stats"]["words"], 2);
}

#[test]
fn los_codigos_de_salida_llegan_al_proceso() {
    let dir = Scratch::new("codes");
    dir.file("tabla.md", "| a | bb |\n|---|---|\n| ccc | d |\n");
    // Hallazgos: 1, con los datos en stdout.
    let check = dir.run(&["tables", "tabla.md"]);
    assert_eq!(check.code, 1);
    assert_eq!(check.stdout, "tabla.md:1: table is not aligned\n");
    assert_eq!(check.stderr, "");

    // E/S: 5, sin nada en stdout.
    let missing = dir.run(&["stats", "no-existe.md"]);
    assert_eq!(missing.code, 5);
    assert_eq!(missing.stdout, "");
    assert!(
        missing
            .stderr
            .starts_with("elision: error[io_error]: cannot read no-existe.md: "),
        "{}",
        missing.stderr
    );

    // `open` con un archivo inexistente falla ANTES de lanzar la GUI.
    let open = dir.run(&["open", "no-existe.md", "--json"]);
    assert_eq!(open.code, 5);
    assert_eq!(open.json()["error"]["code"], "io_error");
}

#[test]
fn export_pdf_con_imagen_relativa_desde_otra_carpeta() {
    if !has_sidecars() {
        return;
    }
    // El directorio actual no es ni la carpeta del documento ni la del ejecutable: la
    // imagen se resuelve contra `docs/` y pandoc y typst se buscan junto a `elision`.
    let dir = Scratch::new("export-pdf");
    dir.file("docs/a.md", "Hola\n\n![x](img/f.png)\n");
    dir.file("docs/img/f.png", PNG);
    std::fs::create_dir_all(dir.0.join("salida")).unwrap();
    let out = dir.run(&["export", "docs/a.md", "-o", "salida/a.pdf", "--json"]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.stderr, "");
    let json = out.json();
    let pdf_path = dir.0.join("salida").join("a.pdf");
    assert_eq!(json["output"], pdf_path.to_string_lossy().as_ref());
    assert_eq!(json["format"], "pdf");
    let pdf = std::fs::read(&pdf_path).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("/Subtype/Image") || text.contains("/Subtype /Image"),
        "el PDF salio sin la imagen"
    );
}

#[test]
fn spell_usa_el_diccionario_de_la_instalacion() {
    if !has_dictionary() {
        return;
    }
    let dir = Scratch::new("spell");
    let out = dir.run_with(&["spell", "-", "--json"], b"hola\nmundoo", &[]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(out.stderr, "");
    let json = out.json();
    let found = &json["misspellings"][0];
    assert_eq!(found["word"], "mundoo");
    assert_eq!(
        (found["line"].as_u64(), found["column"].as_u64()),
        (Some(2), Some(1))
    );

    let ok = dir.run_with(&["spell", "-"], b"hola mundo", &[]);
    assert_eq!(ok.code, 0, "{}", ok.stderr);
    assert_eq!(ok.stdout, "");
}

#[test]
fn la_variable_del_diccionario_llega_a_la_cli_y_la_bandera_le_gana() {
    let dir = Scratch::new("dict-var");
    let empty = dir.0.join("vacia");
    std::fs::create_dir_all(&empty).unwrap();
    // La variable indica una carpeta sin diccionario: se usa (sin caer a la instalacion)
    // y falla con 3.
    let var = dir.run_with(
        &["spell", "-", "--json"],
        b"hola",
        &[(DICT_DIR_VAR, empty.as_path())],
    );
    assert_eq!(var.code, 3, "{}", var.stdout);
    assert_eq!(var.json()["error"]["code"], "dictionary_missing");

    // `--dict-dir` le gana a la variable.
    dir.file("propio/es.aff", "SET UTF-8\n");
    dir.file("propio/es.dic", "2\nhola\nmundo\n");
    let flag = dir.run_with(
        &["spell", "-", "--dict-dir", "propio"],
        b"hola mundo",
        &[(DICT_DIR_VAR, empty.as_path())],
    );
    assert_eq!(flag.code, 0, "{}", flag.stderr);
}
