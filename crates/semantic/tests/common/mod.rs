//! Casos JSON de `tests/cases` con el **texto original** de su esquema
//! (`docs/fase3/pendientes.md` §18, 4.2-S3): el valor del campo `schema` se
//! recorta del archivo del caso por su rango, que da el parser del proyecto,
//! en lugar de reserializarlo con `serde_json` (que ordenaría las claves y
//! pasaría los números por `f64`). Así las pruebas ven el orden de las
//! palabras clave y los lexemas exactos del caso.
//!
//! Lo comparten varias pruebas y no todas usan todos los campos.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

/// Un caso JSON de `tests/cases`.
#[derive(Debug)]
pub struct Caso {
    pub path: PathBuf,
    /// La carpeta: `valid`, `non-dpda`…
    pub category: String,
    /// Texto exacto del esquema (el del campo `schema` o el de `schemaFile`).
    pub schema: Vec<u8>,
    /// Campo `strategy` de los casos `non-dpda`, si lo tiene.
    pub strategy: Option<String>,
}

fn ordenados(dir: &Path, f: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| f(p))
        .collect();
    v.sort();
    v
}

/// Texto exacto del valor del miembro `schema` del caso: el archivo del caso
/// se analiza con `schema_parser::parse` (sus campos son palabras clave
/// fuera del subconjunto, que el parser conserva con el rango de su valor).
pub fn texto_del_esquema(caso: &[u8]) -> Vec<u8> {
    let raiz = schema_parser::parse(caso).unwrap_or_else(|e| panic!("{e}"));
    let kw = raiz
        .keywords()
        .iter()
        .find(|k| k.name.value == "schema")
        .expect("el caso no tiene `schema`");
    let inicio = usize::try_from(kw.value_span.start.offset).unwrap();
    let fin = usize::try_from(kw.value_span.end).unwrap();
    caso[inicio..fin].to_vec()
}

/// Todos los casos JSON de `tests/cases`, por carpeta y nombre.
pub fn casos_json() -> Vec<Caso> {
    let mut out = Vec::new();
    for cat in ordenados(&Path::new(TESTS_DIR).join("cases"), |p| p.is_dir()) {
        let es_json = |p: &Path| p.extension().is_some_and(|e| e == "json");
        for path in ordenados(&cat, es_json) {
            let bytes = std::fs::read(&path).unwrap();
            let case: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            if case["format"] != "json" {
                continue;
            }
            let schema = match case["schemaFile"].as_str() {
                Some(f) => std::fs::read(path.parent().unwrap().join(f)).unwrap(),
                None => texto_del_esquema(&bytes),
            };
            out.push(Caso {
                category: case["category"].as_str().unwrap().to_owned(),
                strategy: case["strategy"].as_str().map(str::to_owned),
                schema,
                path,
            });
        }
    }
    out
}
