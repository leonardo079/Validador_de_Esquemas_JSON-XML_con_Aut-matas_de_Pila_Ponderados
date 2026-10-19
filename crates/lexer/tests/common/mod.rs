//! Lectura del corpus de casos de `tests/cases` (formato en `tests/README.md`).
//!
//! Lo comparten varias pruebas y no todas usan todos los campos.
#![allow(dead_code)]

use std::path::PathBuf;

/// Un caso del corpus visto desde el lexer.
pub struct Case {
    pub path: PathBuf,
    /// Bytes del archivo del caso (siempre JSON).
    pub file: Vec<u8>,
    pub format: String,
    /// Payload, si el caso lo trae incrustado en `payload`.
    pub payload: Option<Vec<u8>>,
    /// Offset esperado si el caso declara un error léxico.
    pub lexical_offset: Option<u64>,
}

pub fn corpus() -> Vec<Case> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cases");
    let mut cases = Vec::new();
    for category in std::fs::read_dir(dir).unwrap() {
        for entry in std::fs::read_dir(category.unwrap().path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let file = std::fs::read(&path).unwrap();
            let v: serde_json::Value =
                serde_json::from_slice(&file).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let expected = &v["expected"];
            cases.push(Case {
                format: v["format"].as_str().unwrap_or_default().to_owned(),
                payload: v["payload"].as_str().map(|p| p.as_bytes().to_vec()),
                lexical_offset: (expected["errorKind"] == "lexical").then(|| {
                    expected["offset"]
                        .as_u64()
                        .expect("offset del error léxico")
                }),
                path,
                file,
            });
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    cases
}
