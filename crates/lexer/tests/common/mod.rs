//! Lectura del corpus de casos de `tests/cases` (formato en `tests/README.md`)
//! y comprobación de un payload en bloques.
//!
//! Lo comparten varias pruebas y no todas usan todos los campos.
#![allow(dead_code)]

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use lexer::LexError;

/// Payload de un caso.
pub enum Payload {
    /// Bytes concretos (de `payload`, `payloadBase64` o `payloadFile`).
    Bytes(Vec<u8>),
    /// Se produce con un generador (`generator`); el nombre del generador.
    Generator(String),
}

/// Un caso del corpus visto desde el lexer.
pub struct Case {
    pub path: PathBuf,
    /// Bytes del archivo del caso (siempre JSON).
    pub file: Vec<u8>,
    pub format: String,
    pub payload: Payload,
    /// Offset esperado si el caso declara un error léxico.
    pub lexical_offset: Option<u64>,
}

impl Case {
    pub fn name(&self) -> String {
        self.path.display().to_string()
    }
}

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

pub fn corpus() -> Vec<Case> {
    let mut cases = Vec::new();
    for category in std::fs::read_dir(format!("{TESTS_DIR}/cases")).unwrap() {
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
                payload: payload(&v, &path),
                lexical_offset: (expected["errorKind"] == "lexical").then(|| {
                    expected["offset"].as_u64().unwrap_or_else(|| {
                        panic!("{}: falta el offset del error léxico", path.display())
                    })
                }),
                path,
                file,
            });
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    cases
}

/// Lee el payload. Un caso debe traer exactamente una de las cuatro formas;
/// si no, o si no se puede leer, la prueba falla en lugar de saltarse el caso.
fn payload(v: &serde_json::Value, path: &Path) -> Payload {
    const FORMAS: [&str; 4] = ["payload", "payloadBase64", "payloadFile", "generator"];
    let presentes: Vec<&str> = FORMAS.into_iter().filter(|k| !v[*k].is_null()).collect();
    let [forma] = presentes[..] else {
        panic!(
            "{}: debe tener exactamente uno de {FORMAS:?}, tiene {presentes:?}",
            path.display()
        );
    };
    let texto = |k: &str| {
        v[k].as_str()
            .unwrap_or_else(|| panic!("{}: `{k}` debe ser una cadena", path.display()))
    };
    match forma {
        "payload" => Payload::Bytes(texto("payload").as_bytes().to_vec()),
        "payloadBase64" => Payload::Bytes(
            STANDARD
                .decode(texto("payloadBase64"))
                .unwrap_or_else(|e| panic!("{}: payloadBase64 inválido: {e}", path.display())),
        ),
        "payloadFile" => {
            // Solo archivos versionados junto al caso. Los payloads grandes no
            // se guardan (tests/large/ está fuera de git): se describen con
            // `generator`, y así la CI nunca depende de un archivo ausente.
            let rel = Path::new(texto("payloadFile"));
            let dentro = rel
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)));
            assert!(
                dentro,
                "{}: payloadFile debe ser una ruta relativa dentro de la carpeta del caso \
                 (sin `..` ni rutas absolutas); para payloads grandes use `generator`",
                path.display()
            );
            let file = path.parent().unwrap().join(rel);
            Payload::Bytes(std::fs::read(&file).unwrap_or_else(|e| {
                panic!(
                    "{}: no se pudo leer {}: {e}",
                    path.display(),
                    file.display()
                )
            }))
        }
        _ => Payload::Generator(
            v["generator"]["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: generator.name debe ser una cadena", path.display()))
                .to_owned(),
        ),
    }
}

/// Tokeniza `bytes` entero y en bloques de 1, 7 y 4096 bytes con `lex`: los
/// cuatro resultados deben ser idénticos. Si el caso declara un error léxico,
/// debe ocurrir en el offset declarado; si no, no debe haber error léxico.
pub fn por_bloques<T: PartialEq + Debug>(
    bytes: &[u8],
    lexical_offset: Option<u64>,
    what: &str,
    lex: impl Fn(&[u8], usize) -> (Vec<T>, Option<LexError>),
) {
    let whole = lex(bytes, usize::MAX);
    assert_eq!(
        whole.1.map(|e| e.at.offset),
        lexical_offset,
        "{what}: {:?}",
        whole.1
    );
    for size in [1, 7, 4096] {
        assert_eq!(lex(bytes, size), whole, "{what}, bloques de {size}");
    }
}

/// Comprueba el payload de cada caso del formato dado. Los casos con
/// generador se informan y se omiten hasta que existan los generadores.
pub fn corpus_en_bloques<T: PartialEq + Debug>(
    format: &str,
    lex: impl Fn(&[u8], usize) -> (Vec<T>, Option<LexError>) + Copy,
) {
    let cases: Vec<Case> = corpus()
        .into_iter()
        .filter(|c| c.format == format)
        .collect();
    assert!(
        cases.iter().any(|c| matches!(c.payload, Payload::Bytes(_))),
        "el corpus no tiene payloads {format}"
    );
    for case in &cases {
        match &case.payload {
            Payload::Bytes(b) => por_bloques(b, case.lexical_offset, &case.name(), lex),
            Payload::Generator(g) => eprintln!(
                "omitido {}: lo produce el generador `{g}`, aún sin implementar",
                case.name()
            ),
        }
    }
}
