//! `schemac automaton [--dot] <esquema>` (F4-U7, F4.1a): el resumen de
//! tamaños en JSON (dorado en `tests/automata`), M en DOT (el `.dot`
//! versionado en `docs/fase4`) y los códigos de salida (0; 1 con los
//! errores del esquema o una regla pendiente con su tarea; 2 si no puede
//! leer o los argumentos son inválidos).

use std::path::{Path, PathBuf};
use std::process::Command;

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

fn schemac(args: &[&std::ffi::OsStr]) -> (i32, String, String) {
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

fn temporal(nombre: &str, contenido: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("schemac-aut-{nombre}-{}", std::process::id()));
    std::fs::write(&p, contenido).unwrap();
    p
}

/// La dorada contada a mano (`docs/fase4/automata.md` §8): el resumen es
/// determinista y es el versionado en `tests/automata`.
#[test]
fn resumen_de_la_dorada() {
    let p = ruta("schemas/parse-ok/objeto-cerrado.json");
    let (code, out, err) = schemac(&["automaton".as_ref(), p.as_os_str()]);
    assert_eq!(code, 0, "{err}");
    assert!(err.is_empty(), "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).expect(&out);
    assert_eq!(v["automatonVersion"], 1);
    assert_eq!(v["states"], 24);
    assert_eq!(v["transitions"], 77);
    assert_eq!(v["classes"], 16);
    let (_, otra, _) = schemac(&["automaton".as_ref(), p.as_os_str()]);
    assert_eq!(out, otra);
    let dorada = std::fs::read_to_string(ruta("automata/objeto-cerrado.json")).unwrap();
    assert_eq!(out, dorada, "ejecutar `make doradas`");
}

/// `--dot` antes o después de la ruta; es el `.dot` versionado.
#[test]
fn dot_de_la_dorada() {
    let p = ruta("schemas/parse-ok/objeto-cerrado.json");
    let (code, out, err) = schemac(&["automaton".as_ref(), "--dot".as_ref(), p.as_os_str()]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("digraph automata {\n"), "{out}");
    let (_, despues, _) = schemac(&["automaton".as_ref(), p.as_os_str(), "--dot".as_ref()]);
    assert_eq!(out, despues);
    let versionado = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/fase4/automata-objeto-cerrado.dot"),
    )
    .unwrap();
    assert_eq!(out, versionado, "ejecutar `make docs`");
}

/// F4-U5: una regla pendiente es un error del esquema (1) con la tarea y la
/// ubicación; también el `pattern` de un XSD (F4.4b) y los errores de
/// `check`.
#[test]
fn pendiente_y_errores_dan_1() {
    let p = temporal(
        "literal.json",
        r#"{"properties": {"x": {"const": {"a": 1}}}}"#,
    );
    let (code, out, err) = schemac(&["automaton".as_ref(), p.as_os_str()]);
    let _ = std::fs::remove_file(&p);
    assert_eq!(code, 1);
    assert!(out.is_empty());
    assert!(
        err.ends_with("estrategia pendiente: const (F4.5b) en #/properties/x\n"),
        "{err}"
    );
    let x = temporal(
        "e.xsd",
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'><xs:element name='e'><xs:simpleType>\
         <xs:restriction base='xs:string'><xs:pattern value='a+'/></xs:restriction></xs:simpleType></xs:element></xs:schema>",
    );
    // Desde F4.4b, el `xs:pattern` de un XSD compila.
    let (code, _, err) = schemac(&["automaton".as_ref(), x.as_os_str()]);
    let _ = std::fs::remove_file(&x);
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = schemac(&[
        "automaton".as_ref(),
        ruta("schemas/parse-error/coma-final.json").as_os_str(),
    ]);
    assert_eq!(code, 1, "{err}");
}

#[test]
fn argumentos_invalidos_o_sin_archivo_dan_2() {
    let p = ruta("schemas/parse-ok/objeto-cerrado.json");
    for args in [
        vec!["automaton".as_ref()],
        vec!["automaton".as_ref(), "--json".as_ref(), p.as_os_str()],
        vec!["automaton".as_ref(), "--dot".as_ref(), "--dot".as_ref()],
    ] {
        let (code, _, err) = schemac(&args);
        assert_eq!(code, 2, "{args:?}: {err}");
    }
    let (code, _, _) = schemac(&["automaton".as_ref(), ruta("no-existe.json").as_os_str()]);
    assert_eq!(code, 2);
}
