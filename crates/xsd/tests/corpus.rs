//! Corpus XSD clasificado y doradas (tarea 5.4): el equivalente XSD de
//! `corpus_clasificado` (`crates/semantic/tests/corpus.rs`;
//! `docs/fase3/pendientes.md` §22) y de `ir_doradas`
//! (`crates/semantic/tests/lower.rs`).
//!
//! Todo esquema XSD de `parse-ok` pasa el análisis (desde la 5.6 también los
//! de `xs:simpleType`) y sale clasificado según la definición de
//! `docs/fase3/clasificacion.md` §4; la única regla (c) del subconjunto XSD
//! es la faceta `pattern` (`00-alcance.md` §3.1), así que un nodo es (c)
//! solo por un `Pattern` propio o por el `Ref` a una base que lo tenga. Los
//! dos casos XSD de `tests/cases` pasan con el **texto original** de su campo
//! `schema`, y la IR de cada XSD con dorada en `tests/ir/xsd` es la dorada.

use std::path::{Path, PathBuf};

use ir::{classify, Class, NodeClass, RuleKind, StrategySet, VerifiedIr};
use xsd::{analyze, lower, parse};

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    v.sort();
    v
}

/// IR verificada del texto. Un error del usuario (de la Fase 2 o del
/// análisis) o un error interno hace fallar la prueba con sus mensajes
/// (5.4-CR1-S1: ningún XSD de `parse-ok` se omite en silencio).
fn ir_de(nombre: &str, texto: &[u8]) -> VerifiedIr {
    let schema = parse(texto).unwrap_or_else(|e| panic!("{nombre}: {e}"));
    let analysis = analyze(&schema).unwrap_or_else(|errs| {
        panic!(
            "{nombre}: no pasa el análisis:\n{}",
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    match lower(&analysis) {
        Ok(ir) => ir,
        Err(e) => panic!(
            "{nombre}: error interno de la bajada:\n{}",
            e.detail(8).collect::<Vec<_>>().join("\n")
        ),
    }
}

/// La única regla (c) que puede tener un XSD es `Pattern`: cualquier otro
/// nodo (c) lo es por la definición (un `Ref` a un nodo (c)), que comprueba
/// `cumple_la_definicion`.
#[track_caller]
fn c_solo_por_pattern(nombre: &str, ir: &VerifiedIr) {
    for (i, node) in ir.nodes().iter().enumerate() {
        for r in &node.rules {
            if r.rule.assignment().class == Class::C {
                assert_eq!(r.rule.kind(), RuleKind::Pattern, "{nombre}: nodo {i}");
            }
        }
    }
}

/// Un caso XSD de `tests/cases` (`format: "xml"`).
struct Caso {
    path: PathBuf,
    id: String,
    /// El texto del esquema tal como está en el caso.
    schema: Vec<u8>,
}

/// Los casos XSD de `tests/cases`. El campo `schema` de un caso XML es una
/// **cadena** JSON cuyo contenido es el documento XSD: decodificarla (los
/// escapes `\"`, `\n` de RFC 8259) da sus bytes exactos, sin reordenar ni
/// reescribir nada, a diferencia de un esquema JSON leído como valor
/// (`docs/fase3/pendientes.md` §18). Es el «texto original» del criterio.
fn casos_xml() -> Vec<Caso> {
    let mut v = Vec::new();
    let mut cats: Vec<PathBuf> = std::fs::read_dir(Path::new(TESTS_DIR).join("cases"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    cats.sort();
    for cat in cats {
        for path in files(&cat, "json") {
            let case: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            if case["format"] != "xml" {
                continue;
            }
            let schema = case["schema"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: `schema` no es una cadena", path.display()))
                .as_bytes()
                .to_vec();
            // Los casos XML con una DTD como esquema (D86) son del frontal DTD.
            if dtd::formato(&schema) == dtd::Formato::Dtd {
                continue;
            }
            v.push(Caso {
                id: case["_id"].as_str().unwrap().to_owned(),
                schema,
                path,
            });
        }
    }
    v
}

/// La clasificación cumple su definición en cada nodo: la clase es la
/// máxima, y las estrategias la unión, de sus reglas y de los destinos de
/// sus aristas sobre la misma instancia (en XSD no hay ninguna: I14). Igual
/// que la de `crates/semantic/tests/corpus.rs`.
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

/// Criterio de las tareas 5.4 y 5.6 (`pendientes.md` §22, §35): **todos**
/// los XSD de `tests/schemas/xsd/parse-ok` (también los tres con
/// `xs:simpleType` que hasta la 5.6 daban «pendiente»: `anotaciones`,
/// `atributos-ajenos` y `tipos-simples`) y los dos de `tests/cases` pasan el
/// análisis y salen clasificados según la definición, con nodos (c) solo
/// por `pattern`. El pedido (`valid-xml-pedido-01`) tiene exactamente un nodo (b),
/// el tipo anónimo de `pedido`, por su `Attributes` (`id` requerido); los dos
/// del árbol recursivo, el tipo `Nodo`, por lo mismo; y los
/// demás (`malformed-xml-billion-laughs-01` y los de F4.6), ninguno. Los casos usan el
/// texto de su campo `schema`, que es el de la copia en `parse-ok`.
#[test]
fn corpus_clasificado() {
    let mut clasificados = Vec::new();
    let mut con_c = Vec::new();
    for path in files(&Path::new(TESTS_DIR).join("schemas/xsd/parse-ok"), "xsd") {
        let nombre = path.display().to_string();
        let ir = ir_de(&nombre, &std::fs::read(&path).unwrap());
        let clases = cumple_la_definicion(&nombre, &ir);
        c_solo_por_pattern(&nombre, &ir);
        let corto = path.file_stem().unwrap().to_string_lossy().into_owned();
        if clases.iter().any(|n| n.class == Class::C) {
            con_c.push(corto.clone());
        }
        clasificados.push(corto);
    }
    for n in ["anotaciones", "atributos-ajenos", "tipos-simples"] {
        assert!(clasificados.iter().any(|c| c == n), "falta {n}");
    }
    assert!(clasificados.len() >= 12, "{clasificados:?}");
    // Los dos con `xs:pattern` tienen nodos (c), y ninguno más.
    assert_eq!(con_c, ["anotaciones", "tipos-simples"]);

    let casos = casos_xml();
    let ids: Vec<&str> = casos.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "invalid-mut-xml-arbol-01",
            "invalid-mut-xml-pedido-01",
            "invalid-xml-arbol-recursivo-01",
            "invalid-xml-missing-child-01",
            "invalid-xml-range-01",
            "malformed-xml-billion-laughs-01",
            "malformed-xml-mut-arbol-01",
            "malformed-xml-mut-pedido-01",
            "malformed-xml-unclosed-01",
            "valid-gen-xml-arbol-01",
            "valid-gen-xml-pedido-01",
            "valid-xml-arbol-recursivo-01",
            "valid-xml-pedido-01"
        ],
        "casos XSD de tests/cases"
    );
    for caso in &casos {
        let nombre = caso.path.display().to_string();
        // El texto del caso es, byte a byte, la copia de `parse-ok`.
        let copia = Path::new(TESTS_DIR)
            .join("schemas/xsd/parse-ok")
            .join(format!("{}.xsd", caso.id));
        assert_eq!(
            caso.schema,
            std::fs::read(&copia).unwrap(),
            "{nombre}: distinto de {}",
            copia.display()
        );
        let ir = ir_de(&nombre, &caso.schema);
        let clases = cumple_la_definicion(&nombre, &ir);
        assert!(
            clases.iter().all(|n| n.class != Class::C),
            "{nombre}: hay nodos (c)"
        );
        let b: Vec<usize> = (0..clases.len())
            .filter(|&i| clases[i].class == Class::B)
            .collect();
        // El nodo (b) de los casos con un atributo requerido: el tipo
        // anónimo de `pedido` y el tipo recursivo `Nodo` del árbol.
        // Los generados por F5.4-P1 (`*-xml-pedido-01`, `*-xml-arbol-01`)
        // usan el mismo XSD que su origen.
        let id = caso.id.as_str();
        let con_b = if id.contains("pedido") {
            Some("#/element/pedido")
        } else if id.contains("arbol") {
            Some("#/complexType/Nodo")
        } else {
            None
        };
        match con_b {
            Some(ubicacion) => {
                assert_eq!(b, [1], "{nombre}: nodos (b)");
                assert_eq!(ir.location(ir::NodeId::new(1)), ubicacion);
                // Lo sube a (b) su `Attributes`, y solo él.
                let reglas_b: Vec<RuleKind> = ir.nodes()[1]
                    .rules
                    .iter()
                    .filter(|r| r.rule.assignment().class == Class::B)
                    .map(|r| r.rule.kind())
                    .collect();
                assert_eq!(reglas_b, [RuleKind::Attributes], "{nombre}");
            }
            None => assert!(b.is_empty(), "{nombre}: nodos (b) {b:?}"),
        }
    }
}

/// Las IR doradas XSD de `tests/ir/xsd` (`make doradas`): para cada
/// `tests/ir/xsd/<n>.json`, la salida de `write_json` de
/// `tests/schemas/xsd/parse-ok/<n>.xsd` es la dorada, no cambia entre dos
/// bajadas y la lee `serde_json` con `format: "xml"`. Están en un
/// subdirectorio para que `ir_doradas` de `semantic` (todos los `.json` de
/// `tests/ir`, de `tests/schemas/parse-ok`) no las vea.
#[test]
fn ir_doradas_xsd() {
    let doradas = files(&Path::new(TESTS_DIR).join("ir/xsd"), "json");
    assert!(doradas.len() >= 2, "hacen falta al menos 2 doradas XSD");
    for dorada in doradas {
        let nombre = dorada.file_stem().unwrap().to_string_lossy().into_owned();
        let fuente = Path::new(TESTS_DIR)
            .join("schemas/xsd/parse-ok")
            .join(format!("{nombre}.xsd"));
        let texto = std::fs::read(&fuente).unwrap_or_else(|e| panic!("{nombre}: {e}"));
        let salida = |t: &[u8]| {
            let ir = ir_de(&nombre, t);
            let mut s = String::new();
            ir::write_json(&ir, &mut s).unwrap();
            s
        };
        let (a, b) = (salida(&texto), salida(&texto));
        assert_eq!(a, b, "{nombre}: no determinista");
        let v: serde_json::Value =
            serde_json::from_str(&a).unwrap_or_else(|e| panic!("{nombre}: JSON ilegible: {e}"));
        assert_eq!(v["format"], "xml", "{nombre}");
        let esperado = std::fs::read_to_string(&dorada).unwrap();
        assert!(
            a == esperado,
            "{nombre}: la IR cambió; revisar el cambio y regenerar con `make doradas`"
        );
    }
}
