//! Corpus de esquemas (criterio de la Fase 2): los de `tests/schemas/parse-ok`
//! y los de `tests/cases` parsean; los de `tests/schemas/parse-error` fallan
//! con la clase, línea y columna declaradas en `tests/schemas/errores.json`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schema_parser::{parse, Kw, SchemaKind, Value};

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

#[test]
fn esquemas_correctos_parsean() {
    let files = json_files(&Path::new(TESTS_DIR).join("schemas/parse-ok"));
    assert!(files.len() >= 10, "corpus demasiado pequeño");
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        if let Err(e) = parse(&bytes) {
            panic!("{}: {e}", path.display());
        }
    }
}

#[test]
fn esquemas_erroneos_reportan_linea_y_columna() {
    let dir = Path::new(TESTS_DIR).join("schemas/parse-error");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(Path::new(TESTS_DIR).join("schemas/errores.json")).unwrap(),
    )
    .unwrap();
    let entries = manifest.as_array().unwrap();

    // Cada archivo de la carpeta tiene su entrada, y viceversa.
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
        "errores.json y parse-error/ no coinciden"
    );
    assert_eq!(
        declarados.len(),
        entries.len(),
        "entradas repetidas en errores.json"
    );

    for e in entries {
        let file = e["file"].as_str().unwrap();
        let bytes = std::fs::read(dir.join(file)).unwrap();
        let err = match parse(&bytes) {
            Ok(_) => panic!("{file}: se esperaba un error"),
            Err(err) => err,
        };
        let line = e["line"].as_u64().unwrap();
        let column = e["column"].as_u64().unwrap();
        let msg = err.to_string();
        assert_eq!(
            (err.code(), u64::from(err.line()), u64::from(err.column())),
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
        assert!(err.at.offset <= bytes.len() as u64, "{file}: offset fuera");
    }
}

/// Los esquemas JSON de los casos de `tests/cases` (los que llegan como
/// objeto o como `schemaFile`) también parsean.
#[test]
fn esquemas_de_los_casos_parsean() {
    let mut n = 0;
    for cat in json_files_dirs(&Path::new(TESTS_DIR).join("cases")) {
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
            if let Err(e) = parse(&text) {
                panic!("{}: {e}", path.display());
            }
            n += 1;
        }
    }
    assert!(n > 0, "no hay casos JSON en tests/cases");
}

fn json_files_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

/// Las palabras clave fuera del subconjunto no se descartan: llegan al AST
/// como [`Kw::Unsupported`] con su nombre, para que la Fase 3 las rechace. Una
/// propiedad que se llama como una de ellas no es una palabra clave.
#[test]
fn fuera_del_subconjunto_se_conserva() {
    let bytes =
        std::fs::read(Path::new(TESTS_DIR).join("schemas/parse-ok/fuera-del-subconjunto.json"))
            .unwrap();
    let schema = parse(&bytes).unwrap();
    let unsupported: Vec<&str> = schema
        .keywords()
        .iter()
        .filter(|k| k.kw == Kw::Unsupported)
        .map(|k| k.name.value.as_str())
        .collect();
    assert_eq!(unsupported, ["if", "then", "x-extension"]);
    let if_kw = &schema.keywords()[1];
    assert_eq!(
        (if_kw.name.span.start.line, if_kw.name.span.start.column),
        (3, 3)
    );

    let Some(Value::SchemaMap(props)) = schema.get(Kw::Properties).map(|k| &k.value) else {
        panic!("properties no es un mapa de esquemas");
    };
    assert_eq!(props[0].0.value, "if");
    assert!(matches!(&props[0].1.kind, SchemaKind::Object(k) if k[0].kw == Kw::Type));
}
