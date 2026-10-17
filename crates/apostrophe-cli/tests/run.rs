//! La CLI en proceso: `run` con buffers en lugar de stdin/stdout/stderr y un `Env` falso
//! (carpeta de instalacion temporal, lanzador de la GUI que solo anota). Cada test arma
//! su propia carpeta: nada depende del directorio actual ni de variables de entorno.
//!
//! Los tests de export con pandoc y typst REALES los buscan en `target/<perfil>/`, donde
//! los copia `tauri-build` (igual que los tests de `apostrophe-sidecars`), y se saltan
//! avisando si no estan.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use apostrophe_cli::{run, Env, Io};
use serde_json::Value;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Lo que dejo una ejecucion.
struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Outcome {
    /// stdout parseado como el UNICO objeto JSON de la salida, que ademas es ASCII puro.
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

/// Una llamada al lanzador de la GUI: el ejecutable y los archivos.
type Launch = (PathBuf, Vec<PathBuf>);

/// Entorno de un test: carpeta de instalacion y directorio de trabajo propios.
struct Fixture {
    base: PathBuf,
    install: PathBuf,
    cwd: PathBuf,
    dict_dir: Option<PathBuf>,
    launches: RefCell<Vec<Launch>>,
    launch_error: Option<io::ErrorKind>,
}

fn fixture(label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "elision-cli-test-{}-{}-{label}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&base);
    let install = base.join("install");
    let cwd = base.join("work");
    std::fs::create_dir_all(&install).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    Fixture {
        base,
        install,
        cwd,
        dict_dir: None,
        launches: RefCell::new(Vec::new()),
        launch_error: None,
    }
}

impl Fixture {
    fn run(&self, args: &[&str]) -> Outcome {
        self.run_with(args, b"", &mut Vec::new())
    }

    fn run_stdin(&self, args: &[&str], stdin: &[u8]) -> Outcome {
        self.run_with(args, stdin, &mut Vec::new())
    }

    fn run_with(&self, args: &[&str], stdin: &[u8], stdout: &mut dyn io::Write) -> Outcome {
        let launch = |exe: &Path, files: &[PathBuf]| -> io::Result<()> {
            if let Some(kind) = self.launch_error {
                return Err(io::Error::new(kind, "launcher failed"));
            }
            self.launches
                .borrow_mut()
                .push((exe.to_path_buf(), files.to_vec()));
            Ok(())
        };
        let env = Env {
            install_dir: self.install.clone(),
            cwd: self.cwd.clone(),
            dict_dir: self.dict_dir.clone(),
            launch: &launch,
        };
        let mut stdin = stdin;
        let mut captured = Vec::new();
        let mut stderr = Vec::new();
        let code = {
            let mut tee = Tee {
                inner: stdout,
                copy: &mut captured,
            };
            let mut io = Io {
                stdin: &mut stdin,
                stdout: &mut tee,
                stderr: &mut stderr,
            };
            run(
                std::iter::once("elision").chain(args.iter().copied()),
                &env,
                &mut io,
            )
        };
        Outcome {
            code,
            stdout: String::from_utf8(captured).expect("stdout no es UTF-8"),
            stderr: String::from_utf8(stderr).expect("stderr no es UTF-8"),
        }
    }

    /// Escribe `content` en `rel` (separado con `/`), relativo al directorio de trabajo, y
    /// devuelve la ruta con el separador de la plataforma, como la informa la CLI.
    fn file(&self, rel: &str, content: impl AsRef<[u8]>) -> PathBuf {
        let path = rel
            .split('/')
            .fold(self.cwd.clone(), |path, part| path.join(part));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Escritor que copia lo que recibe a `copy` despues de escribirlo en `inner` (si `inner`
/// falla, no se copia nada).
struct Tee<'a> {
    inner: &'a mut dyn io::Write,
    copy: &'a mut Vec<u8>,
}

impl io::Write for Tee<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.copy.extend_from_slice(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

// --- ayuda y uso (3.6) ---

#[test]
fn la_ayuda_global_lista_los_comandos_en_stdout() {
    let fx = fixture("help");
    let out = fx.run(&["--help"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("Usage: elision"), "{}", out.stdout);
    for command in ["export", "stats", "tables", "spell", "open"] {
        assert!(
            out.stdout.contains(command),
            "falta {command}: {}",
            out.stdout
        );
    }
    assert_eq!(out.stderr, "");
}

#[test]
fn cada_subcomando_tiene_su_ayuda() {
    let fx = fixture("help-sub");
    let out = fx.run(&["export", "--help"]);
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.contains("Usage: elision export"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("--output"), "{}", out.stdout);
    assert!(out.stdout.contains("--to"), "{}", out.stdout);
}

#[test]
fn la_version_va_a_stdout() {
    let fx = fixture("version");
    let out = fx.run(&["--version"]);
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        format!("elision {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn un_uso_invalido_termina_con_2_y_describe_el_uso_en_stderr() {
    let fx = fixture("usage");
    for args in [
        &["bogus"][..],
        &["stats", "--nope", "doc.md"],
        &["export", "doc.md"],
        &["open"],
        &["tables", "doc.md", "--check", "--write"],
        &[],
    ] {
        let out = fx.run(args);
        assert_eq!(out.code, 2, "{args:?}: {}", out.stderr);
        assert_eq!(out.stdout, "", "{args:?}");
        assert!(out.stderr.contains("Usage:"), "{args:?}: {}", out.stderr);
    }
}

#[test]
fn con_json_los_errores_de_uso_tambien_salen_en_json() {
    let fx = fixture("usage-json");
    for args in [&["--json", "bogus"][..], &["export", "doc.md", "--json"]] {
        let out = fx.run(args);
        assert_eq!(out.code, 2, "{args:?}");
        let json = out.json();
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "usage");
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Usage:"));
        assert_eq!(out.stderr, "");
    }
}

#[test]
fn un_json_despues_de_doble_guion_no_cuenta() {
    // Tras `--` todo es posicional: `--json` seria el nombre de un archivo.
    let fx = fixture("usage-dashdash");
    let out = fx.run(&["bogus", "--", "--json"]);
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
}

// --- stats (3.7) ---

/// Las metricas que calcula el dominio, como las serializa la CLI.
fn core_stats(text: &str) -> Value {
    serde_json::to_value(apostrophe_core::stats::count(text)).unwrap()
}

#[test]
fn stats_en_json_informa_las_metricas_del_dominio() {
    let fx = fixture("stats-json");
    let text = "palabra ".repeat(120);
    let doc = fx.file("doc.md", &text);
    let out = fx.run(&["stats", "doc.md", "--json"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let json = out.json();
    assert_eq!(json["ok"], true);
    assert_eq!(json["file"], doc.to_string_lossy().as_ref());
    assert_eq!(json["stats"]["words"], 120);
    assert_eq!(json["stats"], core_stats(&text));
    let read_time = apostrophe_core::stats::read_time(120);
    assert_eq!(json["stats"]["read_time"]["seconds"], read_time.seconds);
}

#[test]
fn stats_lee_la_entrada_estandar_con_guion() {
    let fx = fixture("stats-stdin");
    let out = fx.run_stdin(&["--json", "stats", "-"], b"uno dos");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let json = out.json();
    assert_eq!(json["file"], Value::Null);
    assert_eq!(json["stats"]["words"], 2);
}

#[test]
fn stats_en_texto_es_una_metrica_por_linea() {
    let fx = fixture("stats-text");
    let text = "Uno dos tres.\n\nCuatro.";
    let stats = apostrophe_core::stats::count(text);
    let out = fx.run_stdin(&["stats", "-"], text.as_bytes());
    assert_eq!(out.code, 0);
    assert_eq!(
        out.stdout,
        format!(
            "characters: {}\nwords: 4\nsentences: {}\nparagraphs: {}\nread_time: {}:{:02}:{:02}\n",
            stats.characters,
            stats.sentences,
            stats.paragraphs,
            stats.read_time.hours,
            stats.read_time.minutes,
            stats.read_time.seconds
        )
    );
    assert_eq!(out.stderr, "");
}

// --- tables (3.8) ---

const MISALIGNED: &str = "# T\n\n| a | bb |\n|---|---|\n| ccc | d |\n";
const ALIGNED: &str = "# T\n\n| a   | bb  |\n| --- | --- |\n| ccc | d   |\n";

#[test]
fn tables_sin_bandera_verifica_y_no_escribe() {
    let fx = fixture("tables-check");
    let doc = fx.file("doc.md", MISALIGNED);
    for args in [&["tables", "doc.md"][..], &["tables", "doc.md", "--check"]] {
        let out = fx.run(args);
        assert_eq!(out.code, 1, "{args:?}: {}", out.stderr);
        assert_eq!(out.stdout, "doc.md:3: table is not aligned\n");
        assert_eq!(std::fs::read_to_string(&doc).unwrap(), MISALIGNED);
    }
}

#[test]
fn tables_check_en_json_indica_las_lineas_de_cada_tabla() {
    let fx = fixture("tables-json");
    let doc = fx.file("doc.md", format!("{ALIGNED}\n{MISALIGNED}"));
    let out = fx.run(&["tables", "doc.md", "--json"]);
    assert_eq!(out.code, 1);
    let json = out.json();
    assert_eq!(json["ok"], true);
    assert_eq!(json["file"], doc.to_string_lossy().as_ref());
    assert_eq!(json["mode"], "check");
    assert_eq!(
        json["tables"],
        serde_json::json!([{"line": 3, "aligned": true}, {"line": 9, "aligned": false}])
    );
    assert_eq!(json["changed"], 1);
    assert_eq!(json["skipped"], serde_json::json!([]));
}

#[test]
fn tables_check_con_todo_alineado_termina_con_0() {
    let fx = fixture("tables-ok");
    fx.file("doc.md", ALIGNED);
    let out = fx.run(&["tables", "doc.md"]);
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "");
    let stdin = fx.run_stdin(&["tables", "-", "--json"], MISALIGNED.as_bytes());
    assert_eq!(stdin.code, 1);
    assert_eq!(stdin.json()["file"], Value::Null);
}

#[test]
fn tables_write_alinea_conserva_crlf_y_es_idempotente() {
    let fx = fixture("tables-write");
    let doc = fx.file("doc.md", MISALIGNED.replace('\n', "\r\n"));
    let out = fx.run(&["tables", "doc.md", "--write"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "doc.md:3: table realigned\n");
    let written = std::fs::read(&doc).unwrap();
    assert_eq!(written, ALIGNED.replace('\n', "\r\n").into_bytes());

    let again = fx.run(&["tables", "doc.md", "--write", "--json"]);
    assert_eq!(again.code, 0);
    assert_eq!(again.json()["mode"], "write");
    assert_eq!(again.json()["changed"], 0);
    assert_eq!(std::fs::read(&doc).unwrap(), written);
}

#[test]
fn tables_write_deja_en_utf8_un_archivo_que_no_lo_era() {
    let fx = fixture("tables-1252");
    // "año" en windows-1252.
    let doc = fx.file("doc.md", b"| a\xf1o | b |\n|---|---|\n| c | d |\n");
    let out = fx.run(&["tables", "doc.md", "--write"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        std::fs::read_to_string(&doc).unwrap(),
        "| año | b   |\n| --- | --- |\n| c   | d   |\n"
    );
}

#[test]
fn tables_write_no_acepta_la_entrada_estandar() {
    let fx = fixture("tables-write-stdin");
    let out = fx.run_stdin(&["tables", "-", "--write"], MISALIGNED.as_bytes());
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with("elision: error[usage]: "),
        "{}",
        out.stderr
    );
    let json = fx.run_stdin(&["tables", "-", "--write", "--json"], MISALIGNED.as_bytes());
    assert_eq!(json.code, 2);
    assert_eq!(json.json()["error"]["code"], "usage");
}

#[test]
fn las_tablas_omitidas_no_fallan_el_check_pero_se_avisan() {
    // `--write` no puede arreglarlas: si `--check` fallara, un agente reintentaria para
    // siempre. Se avisan aparte (stderr en texto, `skipped` en JSON) y no cambian el codigo.
    let fx = fixture("tables-skipped");
    let skipped = "| a | b |\n|---|---|\n| x \\| y | z |\n";
    let doc = fx.file("doc.md", skipped);
    let out = fx.run(&["tables", "doc.md"]);
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "elision: warning: doc.md:1: table skipped (contains an escaped pipe)\n"
    );

    let json = fx.run(&["tables", "doc.md", "--json"]);
    assert_eq!(json.code, 0);
    assert_eq!(json.stderr, "");
    assert_eq!(json.json()["tables"], serde_json::json!([]));
    assert_eq!(
        json.json()["skipped"],
        serde_json::json!([{"line": 1, "reason": "escaped_pipe"}])
    );

    let write = fx.run(&["tables", "doc.md", "--write"]);
    assert_eq!(write.code, 0);
    assert!(write.stderr.contains("table skipped"), "{}", write.stderr);
    assert_eq!(std::fs::read_to_string(&doc).unwrap(), skipped);
}

#[test]
fn cada_motivo_de_omision_tiene_su_codigo() {
    let fx = fixture("tables-reasons");
    let doc = "| a | b |\n|---|---|\n| 1 | 2 | 3 |\n\n | a | bb |\n|---|---|\n\n\
               > | a | b |\n> |---|---|\n\n| e | ff |\n|---|---|\n";
    let out = fx.run_stdin(&["tables", "-", "--json"], doc.as_bytes());
    // La ultima tabla esta desalineada: las omitidas no tapan ese hallazgo.
    assert_eq!(out.code, 1);
    assert_eq!(
        out.json()["skipped"],
        serde_json::json!([
            {"line": 1, "reason": "extra_cells"},
            {"line": 5, "reason": "mixed_indent"},
            {"line": 8, "reason": "blockquote"},
        ])
    );
    assert_eq!(
        out.json()["tables"],
        serde_json::json!([{"line": 11, "aligned": false}])
    );
}

// --- spell (3.9) ---

/// Un diccionario `es` minimo con estas palabras en `dir`.
fn dictionary(dir: &Path, words: &[&str]) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("es.aff"), "SET UTF-8\n").unwrap();
    std::fs::write(
        dir.join("es.dic"),
        format!("{}\n{}\n", words.len(), words.join("\n")),
    )
    .unwrap();
}

/// Las palabras mal escritas que informo `spell --json`.
fn misspelled(out: &Outcome) -> Vec<String> {
    out.json()["misspellings"]
        .as_array()
        .expect("misspellings deberia ser una lista")
        .iter()
        .map(|m| m["word"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn spell_informa_cada_error_con_linea_y_columna() {
    let mut fx = fixture("spell-pos");
    let dict = fx.base.join("dict");
    dictionary(&dict, &["hola", "mundo"]);
    fx.dict_dir = Some(dict.clone());
    let out = fx.run_stdin(&["spell", "-", "--json"], b"hola\nmundoo");
    assert_eq!(out.code, 1, "{}", out.stderr);
    let json = out.json();
    assert_eq!(json["ok"], true);
    assert_eq!(json["file"], Value::Null);
    assert_eq!(json["language"], "es");
    let found = &json["misspellings"];
    assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    assert_eq!(found[0]["word"], "mundoo");
    assert_eq!(found[0]["line"], 2);
    assert_eq!(found[0]["column"], 1);
    // Las sugerencias son las del dominio con el mismo diccionario.
    let loaded = apostrophe_sidecars::dictionary::load_es(&dict).unwrap();
    let expected = apostrophe_core::spellcheck::SpellChecker::check_text(&loaded, "mundoo", false);
    assert_eq!(
        found[0]["suggestions"],
        serde_json::json!(expected[0].suggestions)
    );
}

#[test]
fn spell_en_texto_es_una_linea_por_error_y_sin_errores_termina_con_0() {
    let mut fx = fixture("spell-text");
    let dict = fx.base.join("dict");
    dictionary(&dict, &["hola", "mundo"]);
    fx.dict_dir = Some(dict);
    fx.file("mal.md", "hola\n  mundoo hola\n");
    let out = fx.run(&["spell", "mal.md"]);
    assert_eq!(out.code, 1);
    assert!(
        out.stdout.starts_with("mal.md:2:3: mundoo"),
        "{}",
        out.stdout
    );
    assert_eq!(out.stdout.lines().count(), 1, "{}", out.stdout);

    fx.file("bien.md", "hola mundo\n");
    let ok = fx.run(&["spell", "bien.md"]);
    assert_eq!(ok.code, 0, "{}", ok.stderr);
    assert_eq!(ok.stdout, "");
}

#[test]
fn spell_en_json_es_ascii_y_conserva_el_texto() {
    let mut fx = fixture("spell-ascii");
    let dict = fx.base.join("dict");
    dictionary(&dict, &["mundo"]);
    fx.dict_dir = Some(dict);
    let doc = fx.file("canción 😀.md", "canción mundo");
    let out = fx.run(&["spell", "canción 😀.md", "--json"]);
    assert_eq!(out.code, 1, "{}", out.stderr);
    // `json()` comprueba que stdout es ASCII puro; al parsear vuelve el texto original.
    assert_eq!(misspelled(&out), ["canción"]);
    assert_eq!(out.json()["file"], doc.to_string_lossy().as_ref());
}

#[test]
fn el_diccionario_se_busca_en_la_bandera_la_variable_y_la_instalacion() {
    let mut fx = fixture("spell-priority");
    dictionary(&fx.install.join("dictionaries"), &["uno"]);
    let text: &[u8] = b"uno dos tres";
    let bundled = fx.run_stdin(&["spell", "-", "--json"], text);
    assert_eq!(misspelled(&bundled), ["dos", "tres"]);

    let from_env = fx.base.join("env-dict");
    dictionary(&from_env, &["dos"]);
    fx.dict_dir = Some(from_env);
    let env = fx.run_stdin(&["spell", "-", "--json"], text);
    assert_eq!(misspelled(&env), ["uno", "tres"]);

    // `--dict-dir` gana, y es relativa al directorio de trabajo.
    dictionary(&fx.cwd.join("dicts"), &["tres"]);
    let flag = fx.run_stdin(&["spell", "-", "--dict-dir", "dicts", "--json"], text);
    assert_eq!(misspelled(&flag), ["uno", "dos"]);
}

#[test]
fn sin_diccionario_termina_con_3() {
    let fx = fixture("spell-missing");
    let out = fx.run_stdin(&["spell", "-"], b"hola");
    assert_eq!(out.code, 3);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr
            .starts_with("elision: error[dictionary_missing]: "),
        "{}",
        out.stderr
    );

    // Una carpeta indicada que no sirve no cae en silencio a la de la instalacion.
    dictionary(&fx.install.join("dictionaries"), &["hola"]);
    let json = fx.run_stdin(&["spell", "-", "--dict-dir", "vacia", "--json"], b"hola");
    assert_eq!(json.code, 3);
    assert_eq!(json.json()["error"]["code"], "dictionary_missing");
}

// --- export (3.10) ---

/// PNG valido de 4x4 px, rojo.
const PNG: [u8; 73] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x08, 0x02, 0x00, 0x00, 0x00, 0x26, 0x93, 0x09,
    0x29, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x47, 0x0c, 0xc4, 0x71, 0x00, 0xae, 0x93, 0x0f, 0xf1, 0xd0, 0x5f, 0x23, 0x9e, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Un sidecar falso: un script de shell que ignora sus argumentos.
#[cfg(unix)]
fn fake_sidecar(fx: &Fixture, name: &str, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = fx.install.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// La carpeta con pandoc y typst REALES (`target/<perfil>/`), o `None` si faltan.
fn real_sidecars() -> Option<PathBuf> {
    let dir = apostrophe_sidecars::install_dir().ok()?;
    for name in ["pandoc", "typst"] {
        let path = apostrophe_sidecars::sidecar_path_in(&dir, name);
        if !path.exists() {
            skip(&format!(
                "falta el sidecar {name} en {}. Ejecuta scripts/fetch-{name}.ps1.",
                path.display()
            ));
            return None;
        }
    }
    Some(dir)
}

/// Avisa que el test se salta porque falta un sidecar. Con `ELISION_REQUIRE_SIDECARS`
/// definida (la CI la define, porque ahi siempre los provisiona) falla en su lugar: un
/// salto silencioso escondería una regresión.
fn skip(reason: &str) {
    if std::env::var_os("ELISION_REQUIRE_SIDECARS").is_some() {
        panic!("{reason} ELISION_REQUIRE_SIDECARS exige que exista.");
    }
    eprintln!("SALTADO: {reason}");
}

#[cfg(unix)]
#[test]
fn export_deduce_el_formato_de_la_extension_de_la_salida() {
    let fx = fixture("export-ext");
    fake_sidecar(&fx, "pandoc", "exit 0");
    fx.file("doc.md", "Hola");
    let out = fx.run(&["export", "doc.md", "-o", "out.docx", "--json"]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let json = out.json();
    assert_eq!(json["ok"], true);
    assert_eq!(json["format"], "docx");
    assert_eq!(
        json["output"],
        fx.cwd.join("out.docx").to_string_lossy().as_ref()
    );

    // En texto, stdout lleva solo la ruta de salida.
    let text = fx.run(&["export", "doc.md", "-o", "out.odt"]);
    assert_eq!(text.code, 0);
    assert_eq!(
        text.stdout,
        format!("{}\n", fx.cwd.join("out.odt").display())
    );
}

#[cfg(unix)]
#[test]
fn export_con_to_ignora_la_extension() {
    let fx = fixture("export-to");
    fake_sidecar(&fx, "pandoc", "exit 0");
    fx.file("doc.md", "Hola");
    let out = fx.run(&[
        "export",
        "doc.md",
        "-o",
        "notas.txt",
        "--to",
        "latex",
        "--json",
    ]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.json()["format"], "latex");
}

#[test]
fn export_con_un_formato_desconocido_es_un_error_de_uso() {
    let fx = fixture("export-unknown");
    fx.file("doc.md", "Hola");
    let out = fx.run(&["export", "doc.md", "-o", "out.xyz"]);
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with("elision: error[usage]: "),
        "{}",
        out.stderr
    );
    let bad_to = fx.run(&["export", "doc.md", "-o", "out.html", "--to", "xyz"]);
    assert_eq!(bad_to.code, 2);
}

#[test]
fn export_sin_sidecar_termina_con_3() {
    let fx = fixture("export-no-sidecar");
    fx.file("a.md", "Hola");
    let out = fx.run(&["export", "a.md", "-o", "a.pdf", "--json"]);
    assert_eq!(out.code, 3);
    assert_eq!(out.json()["error"]["code"], "sidecar_missing");
}

#[cfg(unix)]
#[test]
fn el_pdf_tambien_necesita_typst() {
    let fx = fixture("export-no-typst");
    fake_sidecar(&fx, "pandoc", "exit 0");
    fx.file("a.md", "Hola");
    let out = fx.run(&["export", "a.md", "-o", "a.pdf", "--json"]);
    assert_eq!(out.code, 3);
    let message = out.json()["error"]["message"].as_str().unwrap().to_string();
    assert!(message.contains("typst"), "{message}");
}

#[cfg(unix)]
#[test]
fn un_sidecar_que_falla_termina_con_4_y_su_stderr_va_a_stderr() {
    let fx = fixture("export-failed");
    fake_sidecar(&fx, "pandoc", "echo 'bad input' >&2; exit 1");
    fx.file("a.md", "Hola");
    let out = fx.run(&["export", "a.md", "-o", "a.html"]);
    assert_eq!(out.code, 4);
    assert_eq!(out.stdout, "");
    assert_eq!(
        out.stderr,
        "elision: error[conversion_failed]: pandoc failed (exit code 1): bad input\n"
    );
    let json = fx.run(&["export", "a.md", "-o", "a.html", "--json"]);
    assert_eq!(json.code, 4);
    assert_eq!(json.json()["error"]["code"], "conversion_failed");
}

#[test]
fn una_salida_en_una_carpeta_inexistente_termina_con_5() {
    let fx = fixture("export-unwritable");
    fx.file("a.md", "Hola");
    let out = fx.run(&["export", "a.md", "-o", "no-existe/a.html", "--json"]);
    assert_eq!(out.code, 5);
    assert_eq!(out.json()["error"]["code"], "io_error");
}

/// `true` si este proceso puede abrir `path` para escritura a pesar de sus permisos (por
/// ejemplo, si corre como root). Entonces los tests de permisos no prueban nada.
fn ignores_permissions(path: &Path) -> bool {
    let writable = if path.is_dir() {
        let probe = path.join(".probe");
        let ok = std::fs::write(&probe, "").is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    } else {
        std::fs::OpenOptions::new().write(true).open(path).is_ok()
    };
    if writable {
        eprintln!(
            "SALTADO: este usuario puede escribir en {} sin permiso.",
            path.display()
        );
    }
    writable
}

#[test]
fn una_salida_de_solo_lectura_termina_con_5_sin_tocarla() {
    // Se detecta ANTES de lanzar nada: aquí ni siquiera hay pandoc instalado, y el error
    // no es `sidecar_missing` sino de E/S.
    let fx = fixture("export-readonly");
    fx.file("a.md", "Hola");
    let out_path = fx.file("a.html", "viejo");
    let original = std::fs::metadata(&out_path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&out_path, readonly).unwrap();
    if ignores_permissions(&out_path) {
        return;
    }
    let out = fx.run(&["export", "a.md", "-o", "a.html", "--json"]);
    std::fs::set_permissions(&out_path, original).unwrap();
    assert_eq!(out.code, 5, "{}", out.stdout);
    let json = out.json();
    assert_eq!(json["error"]["code"], "io_error");
    let message = json["error"]["message"].as_str().unwrap();
    assert!(
        message.starts_with(&format!("cannot write {}: ", out_path.display())),
        "{message}"
    );
    assert_eq!(std::fs::read_to_string(&out_path).unwrap(), "viejo");
}

#[cfg(unix)]
#[test]
fn una_carpeta_sin_permiso_de_escritura_termina_con_5_sin_lanzar_pandoc() {
    use std::os::unix::fs::PermissionsExt;
    let fx = fixture("export-readonly-dir");
    // Si llegara a correr, fallaria como el pandoc real.
    fake_sidecar(
        &fx,
        "pandoc",
        "echo 'pandoc: a.html: withFile: permission denied' >&2; exit 1",
    );
    fx.file("a.md", "Hola");
    let dir = fx.cwd.join("solo-lectura");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    if ignores_permissions(&dir) {
        return;
    }
    let out = fx.run(&["export", "a.md", "-o", "solo-lectura/a.html"]);
    let left = std::fs::read_dir(&dir).unwrap().count();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(out.code, 5, "{}", out.stderr);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.starts_with(&format!(
            "elision: error[io_error]: cannot write {}: ",
            dir.join("a.html").display()
        )),
        "{}",
        out.stderr
    );
    assert_eq!(left, 0, "quedo algo en la carpeta");
}

#[cfg(unix)]
#[test]
fn un_fallo_de_conversion_muestra_una_sola_linea_y_no_deja_la_salida() {
    let fx = fixture("export-backtrace");
    // Como el pandoc real: la causa y despues la traza de GHC.
    fake_sidecar(
        &fx,
        "pandoc",
        "printf 'pandoc: bad input\\nHasCallStack backtrace:\\n  collectBacktraces, called at x.hs:1:1\\n\\n' >&2; exit 1",
    );
    fx.file("a.md", "Hola");
    let out = fx.run(&["export", "a.md", "-o", "a.html"]);
    assert_eq!(out.code, 4);
    assert_eq!(
        out.stderr,
        "elision: error[conversion_failed]: pandoc failed (exit code 1): pandoc: bad input\n"
    );
    // Comprobar que la salida se podia escribir no deja un archivo vacio.
    assert!(!fx.cwd.join("a.html").exists());
}

#[test]
fn export_html_incrusta_la_imagen_relativa_desde_otra_carpeta() {
    let Some(install) = real_sidecars() else {
        return;
    };
    let mut fx = fixture("export-html");
    fx.install = install;
    fx.file("docs/a.md", "![x](img/f.png)\n");
    fx.file("docs/img/f.png", PNG);
    // Ya existe: se sobrescribe sin preguntar.
    let out_path = fx.file("salida/a.html", "viejo");
    let out = fx.run(&["export", "docs/a.md", "-o", "salida/a.html"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let html = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        html.contains("data:image/png;base64,"),
        "no se incrusto la imagen"
    );
    assert!(!html.contains("viejo"));
}

#[test]
fn export_por_stdin_no_tiene_carpeta_y_sale_sin_imagenes() {
    let Some(install) = real_sidecars() else {
        return;
    };
    let mut fx = fixture("export-stdin");
    fx.install = install;
    // La imagen existe junto al directorio de trabajo, pero un documento sin carpeta no
    // la resuelve contra nada (como un documento sin guardar de la GUI).
    fx.file("img/f.png", PNG);
    let out = fx.run_stdin(&["export", "-", "-o", "out.html"], b"![x](img/f.png)\n");
    assert_eq!(out.code, 0, "{}", out.stderr);
    let html = std::fs::read_to_string(fx.cwd.join("out.html")).unwrap();
    assert!(!html.contains("data:image/png;base64,"));
}

#[test]
fn export_pdf_con_imagen_relativa() {
    let Some(install) = real_sidecars() else {
        return;
    };
    let mut fx = fixture("export-pdf");
    fx.install = install;
    fx.file("docs/a.md", "Hola\n\n![x](img/f.png)\n");
    fx.file("docs/img/f.png", PNG);
    let out = fx.run(&["export", "docs/a.md", "-o", "a.pdf", "--json"]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(out.json()["format"], "pdf");
    let pdf = std::fs::read(fx.cwd.join("a.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("/Subtype/Image") || text.contains("/Subtype /Image"),
        "el PDF salio sin la imagen"
    );
}

// --- open (3.11) ---

/// Un ejecutable de GUI falso en la instalacion (el lanzador del test no lo ejecuta).
fn fake_gui(fx: &Fixture) -> PathBuf {
    let exe = apostrophe_sidecars::sidecar_path_in(&fx.install, "apostrophe");
    std::fs::write(&exe, "").unwrap();
    exe
}

#[test]
fn open_pasa_todas_las_rutas_absolutas_a_la_gui_y_no_la_espera() {
    let fx = fixture("open");
    let exe = fake_gui(&fx);
    let a = fx.file("a.md", "a");
    let b = fx.file("sub/b.md", "b");
    let out = fx.run(&["open", "a.md", "sub/b.md"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "");
    assert_eq!(*fx.launches.borrow(), [(exe, vec![a, b])]);
    // Con pestañas la GUI abre todos los archivos: no hay nada que avisar.
    assert_eq!(out.stderr, "");
}

#[test]
fn open_en_json_lista_los_archivos() {
    let fx = fixture("open-json");
    fake_gui(&fx);
    let a = fx.file("a.md", "a");
    let out = fx.run(&["open", "a.md", "--json"]);
    assert_eq!(out.code, 0);
    assert_eq!(out.stderr, "");
    assert_eq!(
        out.json(),
        serde_json::json!({"ok": true, "files": [a.to_string_lossy()]})
    );
}

#[test]
fn open_con_un_archivo_inexistente_termina_con_5_sin_lanzar_nada() {
    let fx = fixture("open-missing");
    fake_gui(&fx);
    fx.file("a.md", "a");
    let out = fx.run(&["open", "a.md", "no-existe.md", "--json"]);
    assert_eq!(out.code, 5);
    assert_eq!(out.json()["error"]["code"], "io_error");
    assert!(fx.launches.borrow().is_empty());
}

#[test]
fn open_sin_la_gui_instalada_termina_con_3() {
    let fx = fixture("open-no-gui");
    fx.file("a.md", "a");
    let out = fx.run(&["open", "a.md", "--json"]);
    assert_eq!(out.code, 3);
    assert_eq!(out.json()["error"]["code"], "sidecar_missing");
    assert!(fx.launches.borrow().is_empty());
}

#[test]
fn si_la_gui_no_arranca_termina_con_5() {
    let mut fx = fixture("open-launch-error");
    fake_gui(&fx);
    fx.file("a.md", "a");
    fx.launch_error = Some(io::ErrorKind::PermissionDenied);
    let out = fx.run(&["open", "a.md"]);
    assert_eq!(out.code, 5);
    assert!(
        out.stderr.starts_with("elision: error[io_error]: "),
        "{}",
        out.stderr
    );
}

// --- flujos (3.12) ---

/// stdout que siempre falla con `kind` (un pipe cerrado, un disco lleno…).
struct Failing(io::ErrorKind);

impl io::Write for Failing {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(self.0))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn una_entrada_ilegible_no_escribe_en_stdout_sin_json() {
    let fx = fixture("unreadable");
    for args in [
        &["stats", "no-existe.md"][..],
        &["tables", "no-existe.md"],
        &["spell", "no-existe.md"],
        &["export", "no-existe.md", "-o", "x.html"],
    ] {
        let out = fx.run(args);
        assert_eq!(out.code, 5, "{args:?}");
        assert_eq!(out.stdout, "", "{args:?}");
        assert!(
            out.stderr
                .starts_with("elision: error[io_error]: cannot read no-existe.md: "),
            "{args:?}: {}",
            out.stderr
        );
    }
}

#[test]
fn con_json_el_error_va_a_stdout_y_stderr_queda_vacio() {
    let fx = fixture("unreadable-json");
    let out = fx.run(&["stats", "no-existe.md", "--json"]);
    assert_eq!(out.code, 5);
    assert_eq!(out.stderr, "");
    let json = out.json();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "io_error");
}

#[test]
fn un_pipe_cerrado_no_cambia_el_codigo_ni_ensucia_stderr() {
    // `elision stats doc.md | head -0`: quien lee se fue; no hay a quien avisar.
    let fx = fixture("broken-pipe");
    let mut stdout = Failing(io::ErrorKind::BrokenPipe);
    let ok = fx.run_with(&["stats", "-"], b"uno dos", &mut stdout);
    assert_eq!(ok.code, 0);
    assert_eq!(ok.stderr, "");
    let findings = fx.run_with(
        &["tables", "-", "--json"],
        MISALIGNED.as_bytes(),
        &mut stdout,
    );
    assert_eq!(findings.code, 1);
    assert_eq!(findings.stderr, "");
}

#[test]
fn otro_fallo_al_escribir_stdout_es_un_error_de_e_s() {
    let fx = fixture("stdout-error");
    let mut stdout = Failing(io::ErrorKind::Other);
    let out = fx.run_with(&["stats", "-"], b"uno dos", &mut stdout);
    assert_eq!(out.code, 5);
    assert!(
        out.stderr
            .starts_with("elision: error[io_error]: cannot write to standard output: "),
        "{}",
        out.stderr
    );
}

// --- arranque del ejecutable (4.2) ---

/// Lo que deja `startup_failure`, el camino del ejecutable cuando no puede armar su `Env`.
fn startup(args: &[&str], error: &apostrophe_cli::error::CliError) -> Outcome {
    let args: Vec<std::ffi::OsString> = std::iter::once("elision")
        .chain(args.iter().copied())
        .map(Into::into)
        .collect();
    let mut stdin: &[u8] = b"";
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = apostrophe_cli::startup_failure(
        &args,
        error,
        &mut Io {
            stdin: &mut stdin,
            stdout: &mut stdout,
            stderr: &mut stderr,
        },
    );
    Outcome {
        code,
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

#[test]
fn un_fallo_al_arrancar_sigue_el_contrato_de_errores() {
    let error = apostrophe_cli::error::CliError::io("cannot read the current folder");
    let text = startup(&["stats", "doc.md"], &error);
    assert_eq!(text.code, 5);
    assert_eq!(text.stdout, "");
    assert_eq!(
        text.stderr,
        "elision: error[io_error]: cannot read the current folder\n"
    );

    // Con `--json` en cualquier lugar (antes de `--`), el sobre de error a stdout.
    let json = startup(&["--json", "stats", "doc.md"], &error);
    assert_eq!(json.code, 5);
    assert_eq!(json.stderr, "");
    assert_eq!(json.json()["error"]["code"], "io_error");
    let after = startup(&["stats", "--", "--json"], &error);
    assert_eq!(after.stdout, "");
}

// --- rutas normalizadas ---

#[test]
fn las_rutas_que_informa_la_cli_salen_normalizadas() {
    // `Path` compara por componentes y daria por iguales `notas/./doc.md` y `notas/doc.md`:
    // aqui se compara el texto, que es lo que lee un agente.
    let mut fx = fixture("normalized");
    let doc = fx.file("notas/doc.md", "uno dos");
    let expected = doc.to_string_lossy().into_owned();

    let stats = fx.run(&["stats", "notas/./doc.md", "--json"]);
    assert_eq!(stats.json()["file"], expected.as_str());

    let export = fx.run(&[
        "export",
        "notas/doc.md",
        "-o",
        "no-existe/./a.html",
        "--json",
    ]);
    assert_eq!(export.code, 5);
    let message = export.json()["error"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    let output = fx.cwd.join("no-existe").join("a.html");
    assert!(
        message.starts_with(&format!("cannot write {}: ", output.display())),
        "{message}"
    );

    fake_gui(&fx);
    let open = fx.run(&["open", "notas/./doc.md", "--json"]);
    assert_eq!(open.code, 0, "{}", open.stdout);
    assert_eq!(open.json()["files"], serde_json::json!([expected]));
    let launched = fx.launches.borrow()[0].1[0].to_string_lossy().into_owned();
    assert_eq!(launched, expected);

    // `--dict-dir` y la variable del diccionario, igual.
    let dict = fx.cwd.join("dicts").join("es").join("es.aff");
    for (flag, var) in [(true, false), (false, true)] {
        fx.dict_dir = var.then(|| PathBuf::from("dicts/./es"));
        let mut args = vec!["spell", "notas/doc.md", "--json"];
        if flag {
            args.extend(["--dict-dir", "dicts/./es"]);
        }
        let spell = fx.run(&args);
        assert_eq!(spell.code, 3);
        let message = spell.json()["error"]["message"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            message.starts_with(&format!(
                "cannot read the dictionary file {}: ",
                dict.display()
            )),
            "{message}"
        );
    }
}
