//! Corpus de esquemas en la Fase 3: los de `tests/schemas/parse-ok` y los de
//! `tests/cases` pasan la validación del meta-esquema y la resolución de
//! `$ref` (salvo las dos excepciones justificadas abajo); los de
//! `tests/schemas/semantic-error` parsean pero fallan con la clase, línea,
//! columna y número de errores declarados en
//! `tests/schemas/errores-semanticos.json`. Y todo el corpus sale
//! clasificado (tarea 4.3, `docs/fase3/clasificacion.md`): cada caso
//! `non-dpda` tiene algún nodo (c) y los `valid` sin palabras clave (c),
//! ninguno.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ir::{classify, Class, NodeClass, StrategySet, VerifiedIr};
use schema_parser::parse;
use semantic::{analyze, lower, SemanticError};

mod common;

use common::TESTS_DIR;

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
    // Con el texto original del esquema, no reserializado (§18 de
    // `docs/fase3/pendientes.md`).
    let casos = common::casos_json();
    for caso in &casos {
        let path = caso.path.display();
        let schema = parse(&caso.schema).unwrap_or_else(|e| panic!("{path}: {e}"));
        if let Err(e) = analyze(&schema) {
            panic!("{path}: {}", e[0]);
        }
    }
    assert!(!casos.is_empty(), "no hay casos JSON en tests/cases");
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

// --- Clasificación (tarea 4.3) -----------------------------------------------

/// Palabras clave de clase (c) en `docs/00-alcance.md` §2.
const PALABRAS_C: [&str; 8] = [
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "pattern",
    "uniqueItems",
    "dependentRequired",
    "dependentSchemas",
];

/// Si algún subesquema del texto usa una palabra clave (c). Recorre el AST
/// del parser del proyecto con una pila explícita y solo mira las palabras
/// clave de los subesquemas: una propiedad que se llame `pattern` o un
/// literal de `const` no cuentan, y los números conservan su lexema (con
/// `serde_json`, `1e400` no se podría leer).
fn menciona_palabra_c(texto: &[u8]) -> bool {
    use schema_parser::{Schema, Value};
    let raiz = parse(texto).unwrap();
    let mut pila: Vec<&Schema> = vec![&raiz];
    while let Some(s) = pila.pop() {
        for k in s.keywords() {
            if PALABRAS_C.contains(&k.name.value.as_str()) {
                return true;
            }
            match &k.value {
                Value::Schema(s) => pila.push(s),
                Value::SchemaMap(m) => pila.extend(m.iter().map(|(_, s)| s)),
                Value::SchemaList(l) => pila.extend(l),
                _ => {}
            }
        }
    }
    false
}

fn ir_de(nombre: &str, texto: &[u8]) -> VerifiedIr {
    let schema = parse(texto).unwrap_or_else(|e| panic!("{nombre}: {e}"));
    let table = analyze(&schema).unwrap_or_else(|e| panic!("{nombre}: {}", e[0]));
    lower(&table).unwrap_or_else(|e| panic!("{nombre}: {:?}", e.errors()))
}

/// La clasificación cumple su definición en cada nodo: la clase es la
/// máxima, y las estrategias la unión, de sus reglas y de los destinos de
/// sus aristas sobre la misma instancia. Es la ecuación de punto fijo que
/// `classify` resuelve en postorden; aquí se comprueba nodo a nodo.
#[track_caller]
fn cumple_la_definicion(nombre: &str, ir: &VerifiedIr) -> Vec<NodeClass> {
    let c = classify(ir);
    assert_eq!(c.nodes().len(), ir.nodes().len(), "{nombre}");
    let mut destinos = Vec::new();
    for (i, node) in ir.nodes().iter().enumerate() {
        let mut clase = Class::A;
        let mut est = StrategySet::EMPTY;
        for r in &node.rules {
            let a = r.rule.assignment();
            clase = clase.max(a.class);
            est = est.with(a.strategy);
            if r.rule.kind().same_instance() {
                destinos.clear();
                r.rule.targets(&mut destinos);
                for t in &destinos {
                    clase = clase.max(c.node(*t).class);
                    est = est.union(c.node(*t).strategies);
                }
            }
        }
        assert_eq!(
            (c.nodes()[i].class, c.nodes()[i].strategies),
            (clase, est),
            "{nombre}: nodo {i}"
        );
    }
    c.nodes().to_vec()
}

/// Criterio 3 de la tarea 4.3: todo el corpus que pasa la Fase 3 sale
/// clasificado; cada caso de `tests/cases/non-dpda` tiene al menos un nodo
/// (c), con la estrategia de su campo `strategy` si lo declara; y los
/// `valid`, y los esquemas de `tests/schemas/parse-ok`, sin combinadores,
/// `pattern`, `uniqueItems` ni dependencias no tienen ninguno.
#[test]
fn corpus_clasificado() {
    let mut sin_c = 0;
    for path in json_files(&Path::new(TESTS_DIR).join("schemas/parse-ok")) {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        if EXCEPCIONES.iter().any(|(f, _)| *f == file) {
            continue;
        }
        let texto = std::fs::read(&path).unwrap();
        let clases = cumple_la_definicion(&file, &ir_de(&file, &texto));
        if !menciona_palabra_c(&texto) {
            sin_c += 1;
            let c = clases.iter().filter(|n| n.class == Class::C).count();
            assert_eq!(c, 0, "{file}: {c} nodos (c)");
        }
    }
    assert!(
        sin_c >= 5,
        "pocos esquemas de parse-ok sin palabras clave (c)"
    );
    let (mut non_dpda, mut valid) = (0, 0);
    for caso in common::casos_json() {
        let nombre = caso.path.display().to_string();
        let clases = cumple_la_definicion(&nombre, &ir_de(&nombre, &caso.schema));
        let c: Vec<&NodeClass> = clases.iter().filter(|n| n.class == Class::C).collect();
        match caso.category.as_str() {
            "non-dpda" => {
                non_dpda += 1;
                assert!(!c.is_empty(), "{nombre}: ningún nodo (c)");
                if let Some(e) = &caso.strategy {
                    assert!(
                        c.iter()
                            .any(|n| n.strategies.iter().any(|s| s.key() == e.as_str())),
                        "{nombre}: ningún nodo (c) con la estrategia {e}"
                    );
                }
            }
            "valid" if !menciona_palabra_c(&caso.schema) => {
                valid += 1;
                assert!(c.is_empty(), "{nombre}: {} nodos (c)", c.len());
            }
            _ => {}
        }
    }
    assert!(non_dpda >= 4, "faltan casos non-dpda");
    assert!(valid >= 1, "ningún caso valid sin palabras clave (c)");
}
