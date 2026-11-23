//! DTD.5: el corpus de esquemas DTD de `tests/schemas/dtd`.
//!
//! - todo `parse-ok` compila a una IR verificada;
//! - todo `parse-error` lo rechazan las Fases 1-2, y todo `semantic-error`
//!   la Fase 3, con el código, la línea y la columna del primer error de la
//!   tabla de abajo: ningún archivo queda sin su expectativa.

use std::collections::BTreeMap;
use std::path::Path;

use dtd::{analyze, compile, parse, DtdError};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/schemas/dtd");

fn archivos(sub: &str) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(Path::new(DIR).join(sub))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "dtd"))
        .map(|p| {
            (
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect()
}

fn primero(e: &DtdError) -> (&'static str, u32, u32) {
    (e.code(), e.line(), e.column())
}

#[test]
fn parse_ok_compila() {
    let ok = archivos("parse-ok");
    assert!(ok.len() >= 2);
    for (n, t) in ok {
        assert!(compile(&t).is_ok(), "{n}");
    }
}

#[test]
fn parse_error_con_su_codigo_y_posicion() {
    let esperado: BTreeMap<&str, (&str, u32, u32)> = [
        ("any", ("unsupported", 1, 13)),
        ("billion-laughs", ("lexical", 2, 1)),
        ("condicional", ("lexical", 1, 1)),
        ("doctype", ("lexical", 2, 1)),
        ("entidad", ("lexical", 2, 1)),
        ("id", ("unsupported", 2, 19)),
        ("mixto", ("unsupported", 1, 13)),
        ("notation", ("unsupported", 1, 1)),
        ("parametro", ("lexical", 1, 13)),
        // El `(` número 257.
        ("profundidad", ("limit", 1, 12 + 257)),
    ]
    .into();
    let archivos = archivos("parse-error");
    assert_eq!(
        archivos.keys().map(String::as_str).collect::<Vec<_>>(),
        esperado.keys().copied().collect::<Vec<_>>()
    );
    for (n, t) in archivos {
        let e = parse(&t).expect_err(&n);
        assert_eq!(primero(&e), esperado[n.as_str()], "{n}: {e}");
    }
}

#[test]
fn semantic_error_con_su_codigo_y_posicion() {
    let esperado: BTreeMap<&str, (&str, u32, u32)> = [
        ("no-determinista", ("content-model", 1, 24)),
        ("sin-declarar", ("unresolved", 1, 14)),
        ("sin-elementos", ("empty-schema", 1, 1)),
        ("varios", ("content-model", 1, 35)),
    ]
    .into();
    let archivos = archivos("semantic-error");
    assert_eq!(
        archivos.keys().map(String::as_str).collect::<Vec<_>>(),
        esperado.keys().copied().collect::<Vec<_>>()
    );
    for (n, t) in archivos {
        let d = parse(&t).unwrap_or_else(|e| panic!("{n}: {e}"));
        let es = analyze(&d).expect_err(&n);
        assert_eq!(primero(&es[0]), esperado[n.as_str()], "{n}: {}", es[0]);
    }
}
