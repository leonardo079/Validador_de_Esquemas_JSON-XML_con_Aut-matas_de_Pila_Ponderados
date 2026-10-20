//! Corpus de esquemas en la Fase 3: los de `tests/schemas/parse-ok` y los de
//! `tests/cases` pasan la validación del meta-esquema y la resolución de
//! `$ref` (salvo las dos excepciones justificadas abajo); los de
//! `tests/schemas/semantic-error` parsean pero fallan con la clase, línea,
//! columna y número de errores declarados en
//! `tests/schemas/errores-semanticos.json`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schema_parser::parse;
use semantic::{analyze, SemanticError};

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    v.sort();
    v
}

/// Fase 3 hasta ahora: meta-esquema y tabla de símbolos con los `$ref`
/// resueltos.
fn check_file(path: &Path) -> Result<(), Vec<SemanticError>> {
    let bytes = std::fs::read(path).unwrap();
    let schema = parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    analyze(&schema).map(|_| ())
}

/// Esquemas de `parse-ok` que **no** pasan la Fase 3, a propósito. Ese
/// corpus es del parser y prueba formas que la gramática admite:
///
/// - `combinadores.json` usa `"oneOf": []` para cubrir la alternativa ε de
///   `schemas`; el meta-esquema exige `minItems: 1` en los arreglos de
///   esquemas.
/// - `fuera-del-subconjunto.json` existe para comprobar que el parser
///   conserva `if`, `then` y `x-extension` como `Kw::Unsupported`; la Fase 3
///   debe rechazarlas (D6).
///
/// (código, línea, columna) de un error.
type Esperado = (&'static str, u32, u32);

/// Cada entrada: archivo y errores esperados.
const EXCEPCIONES: [(&str, &[Esperado]); 2] = [
    ("combinadores.json", &[("meta-schema", 10, 12)]),
    (
        "fuera-del-subconjunto.json",
        &[
            ("unsupported", 3, 3),
            ("unsupported", 4, 3),
            ("unsupported", 5, 3),
        ],
    ),
];

#[test]
fn esquemas_correctos_pasan_el_meta_esquema() {
    let files = json_files(&Path::new(TESTS_DIR).join("schemas/parse-ok"));
    assert!(files.len() >= 10, "corpus demasiado pequeño");
    let mut excepciones_vistas = 0;
    for path in files {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let res = check_file(&path);
        match EXCEPCIONES.iter().find(|(f, _)| *f == file) {
            Some((_, esperados)) => {
                let errs = res.expect_err(&file);
                let got: Vec<Esperado> = errs
                    .iter()
                    .map(|e| (e.code(), e.line(), e.column()))
                    .collect();
                assert_eq!(&got, esperados, "{file}");
                excepciones_vistas += 1;
            }
            None => {
                if let Err(e) = res {
                    panic!("{file}: {}", e[0]);
                }
            }
        }
    }
    assert_eq!(excepciones_vistas, EXCEPCIONES.len());
}

/// Los esquemas JSON de los casos de `tests/cases` pasan la validación. Los
/// XSD son de otra tarea (D3).
#[test]
fn esquemas_de_los_casos_pasan_el_meta_esquema() {
    let mut n = 0;
    for cat in dirs(&Path::new(TESTS_DIR).join("cases")) {
        for path in json_files(&cat) {
            let case: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            if case["format"] != "json" {
                continue;
            }
            let text = if let Some(f) = case["schemaFile"].as_str() {
                std::fs::read(path.parent().unwrap().join(f)).unwrap()
            } else {
                serde_json::to_vec_pretty(&case["schema"]).unwrap()
            };
            let schema = parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            if let Err(e) = analyze(&schema) {
                panic!("{}: {}", path.display(), e[0]);
            }
            n += 1;
        }
    }
    assert!(n > 0, "no hay casos JSON en tests/cases");
}

fn dirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

#[test]
fn esquemas_erroneos_reportan_linea_y_columna() {
    let dir = Path::new(TESTS_DIR).join("schemas/semantic-error");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(Path::new(TESTS_DIR).join("schemas/errores-semanticos.json")).unwrap(),
    )
    .unwrap();
    let entries = manifest.as_array().unwrap();

    let en_disco: BTreeSet<String> = json_files(&dir)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    let declarados: BTreeSet<String> = entries
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        en_disco, declarados,
        "errores-semanticos.json y semantic-error/ no coinciden"
    );
    assert_eq!(declarados.len(), entries.len(), "entradas repetidas");

    for e in entries {
        let file = e["file"].as_str().unwrap();
        let path = dir.join(file);
        let len = std::fs::metadata(&path).unwrap().len();
        let errs = check_file(&path).expect_err(file);
        let first = &errs[0];
        let line = e["line"].as_u64().unwrap();
        let column = e["column"].as_u64().unwrap();
        let msg = first.to_string();
        assert_eq!(
            (
                first.code(),
                u64::from(first.line()),
                u64::from(first.column())
            ),
            (e["code"].as_str().unwrap(), line, column),
            "{file}: {msg}"
        );
        assert!(
            msg.starts_with(&format!("línea {line}, columna {column}: ")),
            "{file}: {msg}"
        );
        if let Some(sub) = e["contains"].as_str() {
            assert!(msg.contains(sub), "{file}: «{msg}» no contiene «{sub}»");
        }
        if let Some(n) = e["count"].as_u64() {
            assert_eq!(errs.len() as u64, n, "{file}: {errs:?}");
        }
        for err in &errs {
            assert!(err.at.offset < len, "{file}: offset fuera");
            assert!(
                !err.to_string().contains('\n'),
                "{file}: mensaje multilínea"
            );
        }
    }
}
