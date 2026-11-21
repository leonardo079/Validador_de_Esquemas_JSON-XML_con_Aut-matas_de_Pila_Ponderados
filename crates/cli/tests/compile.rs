//! `schemac compile <esquema> --lang <rust|c> --out <dir>` (F5.1, D74): escribe
//! el validador Rust (F5.1) o C (F5.2) autocontenido (lo que compila y
//! ejecuta lo prueban `crates/codegen/tests/`); códigos 1 (esquema), 2
//! (argumentos, E/S) y la salida reproducible.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

fn schemac(args: &[&OsStr]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_schemac"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn ruta(rel: &str) -> PathBuf {
    Path::new(TESTS_DIR).join(rel)
}

fn salida(nombre: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("compile-{nombre}"));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn compilar(esquema: &Path, out: &Path) -> (i32, String, String) {
    schemac(&[
        "compile".as_ref(),
        esquema.as_os_str(),
        "--lang".as_ref(),
        "rust".as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
    ])
}

fn leer(d: &Path, nombre: &str) -> String {
    std::fs::read_to_string(d.join(nombre)).unwrap()
}

#[test]
fn json_escribe_el_validador() {
    let p = ruta("schemas/parse-ok/objeto-cerrado.json");
    let d = salida("json");
    let (code, out, err) = compilar(&p, &d);
    assert_eq!(code, 0, "{err}");
    assert!(err.is_empty(), "{err}");
    assert!(out.contains("validador Rust en"), "{out}");
    for f in [
        "main.rs",
        "validador.rs",
        "tablas.rs",
        "lexer.rs",
        "runtime_rs_core.rs",
    ] {
        assert!(d.join(f).is_file(), "{f}");
    }
    assert!(leer(&d, "validador.rs").contains("pub const FORMATO: &str = \"json\";"));
    // Reproducible, y las opciones en otro orden dan lo mismo.
    let d2 = salida("json-2");
    let (code, _, err) = schemac(&[
        "compile".as_ref(),
        "--out".as_ref(),
        d2.as_os_str(),
        "--lang".as_ref(),
        "rust".as_ref(),
        p.as_os_str(),
    ]);
    assert_eq!(code, 0, "{err}");
    for f in std::fs::read_dir(&d).unwrap() {
        let n = f.unwrap().file_name();
        let n = n.to_str().unwrap();
        assert_eq!(leer(&d, n), leer(&d2, n), "{n}");
    }
}

#[test]
fn xsd_escribe_el_validador() {
    let p = ruta("schemas/xsd/parse-ok/pedido.xsd");
    let d = salida("xsd");
    let (code, _, err) = compilar(&p, &d);
    assert_eq!(code, 0, "{err}");
    assert!(leer(&d, "validador.rs").contains("pub const FORMATO: &str = \"xml\";"));
    assert!(leer(&d, "validador.rs").contains("XmlValidator as Nucleo"));
}

/// F5.2: `--lang c` escribe el validador C (lo que compila y ejecuta lo
/// prueban `crates/codegen/tests/c.rs` y `diferencial.rs`).
#[test]
fn lang_c_escribe_el_validador() {
    let p = ruta("schemas/xsd/parse-ok/pedido.xsd");
    let d = salida("c");
    let (code, out, err) = schemac(&[
        "compile".as_ref(),
        p.as_os_str(),
        "--lang".as_ref(),
        "c".as_ref(),
        "--out".as_ref(),
        d.as_os_str(),
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("validador C en"), "{out}");
    for f in [
        "main.c",
        "validador.h",
        "validador.c",
        "tablas.c",
        "sv_lexico.h",
        "sv_lexico.c",
        "sv_nucleo.h",
        "sv_nucleo.c",
    ] {
        assert!(d.join(f).is_file(), "{f}");
    }
    assert!(leer(&d, "validador.h").contains("#define SV_FORMATO \"xml\""));
    assert!(leer(&d, "validador.h")
        .contains("int sv_feed(sv_state_t* s, const uint8_t* chunk, size_t len);"));
}

#[test]
fn argumentos_invalidos_dan_2() {
    let p = ruta("schemas/parse-ok/objeto-cerrado.json");
    let d = salida("args");
    let p = p.as_os_str();
    let o = d.as_os_str();
    for args in [
        vec!["compile".as_ref()],
        vec!["compile".as_ref(), p, "--out".as_ref(), o],
        vec!["compile".as_ref(), p, "--lang".as_ref(), "rust".as_ref()],
        vec![
            "compile".as_ref(),
            "--lang".as_ref(),
            "rust".as_ref(),
            "--out".as_ref(),
            o,
        ],
        vec![
            "compile".as_ref(),
            p,
            "--lang".as_ref(),
            "go".as_ref(),
            "--out".as_ref(),
            o,
        ],
        vec![
            "compile".as_ref(),
            p,
            "--lang".as_ref(),
            "rust".as_ref(),
            "--out".as_ref(),
        ],
        vec![
            "compile".as_ref(),
            p,
            "--lang".as_ref(),
            "rust".as_ref(),
            "--out".as_ref(),
            o,
            "--x".as_ref(),
        ],
        vec![
            "compile".as_ref(),
            p,
            p,
            "--lang".as_ref(),
            "rust".as_ref(),
            "--out".as_ref(),
            o,
        ],
        vec![
            "compile".as_ref(),
            p,
            "--lang".as_ref(),
            "rust".as_ref(),
            "--lang".as_ref(),
            "rust".as_ref(),
            "--out".as_ref(),
            o,
        ],
    ] {
        let (code, out, err) = schemac(&args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(out.is_empty(), "{args:?}");
        assert!(err.contains("uso: schemac"), "{args:?}: {err}");
    }
    assert!(!d.exists());
}

#[test]
fn esquema_con_errores_da_1_y_no_escribe() {
    let p = std::env::temp_dir().join(format!("schemac-compile-malo-{}.json", std::process::id()));
    std::fs::write(&p, r#"{"type": "objeto"}"#).unwrap();
    let d = salida("malo");
    let (code, out, err) = compilar(&p, &d);
    let _ = std::fs::remove_file(&p);
    assert_eq!(code, 1, "{err}");
    assert!(out.is_empty());
    assert!(!err.is_empty());
    assert!(!d.exists());
}

#[test]
fn salida_que_no_es_directorio_da_2() {
    let p = ruta("schemas/parse-ok/objeto-cerrado.json");
    let f = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compile-archivo");
    std::fs::write(&f, "no soy un directorio").unwrap();
    let (code, out, err) = compilar(&p, &f);
    assert_eq!(code, 2, "{err}");
    assert!(out.is_empty());
    assert!(err.contains("compile-archivo"), "{err}");
}

/// F6.2: `--limit nombre=valor` llega a lo emitido (Rust y C); un límite
/// desconocido, repetido, sin `=` o no entero da 2, y unos que piden
/// demasiada memoria dan 1 sin escribir nada.
#[test]
fn limit_cambia_los_limites_emitidos() {
    let p = ruta("schemas/parse-ok/objeto-cerrado.json");
    let base = |d: &Path, lang: &str| -> Vec<std::ffi::OsString> {
        vec![
            "compile".into(),
            p.clone().into_os_string(),
            "--lang".into(),
            lang.into(),
            "--out".into(),
            d.as_os_str().to_owned(),
        ]
    };
    let correr = |args: &[std::ffi::OsString]| {
        let a: Vec<&OsStr> = args.iter().map(|x| x.as_os_str()).collect();
        schemac(&a)
    };
    let d = salida("limit-rust");
    let mut args = base(&d, "rust");
    for l in ["maxDepth=8", "maxLexeme=20", "maxTimeMs=1500"] {
        args.extend(["--limit".into(), l.into()]);
    }
    let (code, _, err) = correr(&args);
    assert_eq!(code, 0, "{err}");
    let v = leer(&d, "validador.rs");
    assert!(v.contains("pub const SV_MAX_DEPTH: usize = 8;"), "{v}");
    assert!(v.contains("max_lexeme: 20,"), "{v}");
    assert!(leer(&d, "main.rs").contains("const TIEMPO_MS: Option<u64> = Some(1500);"));
    let d = salida("limit-c");
    let mut args = base(&d, "c");
    args.extend(["--limit".into(), "maxString=99".into()]);
    let (code, _, err) = correr(&args);
    assert_eq!(code, 0, "{err}");
    assert!(leer(&d, "validador.c").contains("lim->max_string = 99ULL;"));
    assert!(leer(&d, "main.c").contains("#define TIEMPO_MS -1LL"));
    for (malo, msg) in [
        ("maxProfundidad=3", "desconocido"),
        ("maxDepth", "falta"),
        ("maxDepth=-1", "entero"),
        ("maxDepth=x", "entero"),
        ("maxDepth=+5", "entero"),
        ("maxDepth=", "entero"),
    ] {
        let d = salida("limit-malo");
        let mut args = base(&d, "rust");
        args.extend(["--limit".into(), malo.into()]);
        let (code, _, err) = correr(&args);
        assert_eq!(code, 2, "{malo}: {err}");
        assert!(err.contains(msg), "{malo}: {err}");
        assert!(!d.exists(), "{malo}");
    }
    let d = salida("limit-repetido");
    let mut args = base(&d, "rust");
    args.extend([
        "--limit".into(),
        "maxDepth=3".into(),
        "--limit".into(),
        "maxDepth=4".into(),
    ]);
    let (code, _, err) = correr(&args);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("repetido"), "{err}");
    let mut args = base(&d, "rust");
    args.push("--limit".into());
    assert_eq!(correr(&args).0, 2);
    let d = salida("limit-grande");
    let mut args = base(&d, "c");
    args.extend(["--limit".into(), "keyPool=999999999".into()]);
    let (code, _, err) = correr(&args);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("palabras de memoria"), "{err}");
    assert!(!d.exists());
}
