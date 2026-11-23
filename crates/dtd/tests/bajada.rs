//! DTD.3: bajada a la IR (`crates/dtd/src/lower.rs`).
//!
//! - la forma de la IR (orden de los `NodeId`, reglas y orígenes);
//! - las IR doradas de `tests/ir/dtd` (JSON v2 de `ir::write_json`);
//! - de extremo a extremo: la IR de la DTD construye el autómata de pila
//!   (`automaton::build`) y valida documentos XML como manda la DTD.

use std::path::{Path, PathBuf};

use automaton::Mode;
use dtd::{compile, Compiled};
use ir::{NodeId, Rule, Sort, Step, Term, VerifiedIr};

const TESTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

fn fuente(nombre: &str) -> Vec<u8> {
    let p = Path::new(TESTS).join(format!("schemas/dtd/parse-ok/{nombre}.dtd"));
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn ir(fuente: &[u8]) -> VerifiedIr {
    match compile(fuente) {
        Ok(ir) => ir,
        Err(Compiled::Schema(es)) => panic!(
            "{}",
            es.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Err(Compiled::Internal(e)) => panic!("interno: {e:?}"),
    }
}

#[test]
fn forma_de_la_ir_del_pedido() {
    let v = ir(&fuente("pedido"));
    let ir = v.ir();
    assert!(matches!(
        ir.format,
        ir::Format::Xml {
            target_namespace: None
        }
    ));
    // 0 documento, 1-6 los elementos en orden, 7 builtin/string, 8 la enumeración de `estado`.
    assert_eq!(ir.nodes.len(), 9);
    let paso = |i: usize| ir.nodes[i].origin.step.clone();
    assert_eq!(paso(0), Step::Root);
    assert_eq!(paso(1), Step::Entry("element".into(), "pedido".into()));
    assert_eq!(paso(6), Step::Entry("element".into(), "nota".into()));
    assert_eq!(paso(7), Step::Entry("builtin".into(), "string".into()));
    assert_eq!(paso(8), Step::Entry("attribute".into(), "estado".into()));
    assert_eq!(ir.nodes[8].origin.parent, Some(NodeId::new(1)));

    // El documento: una elección entre los 6 elementos declarados.
    let Rule::Content(doc) = &ir.nodes[0].rules[0].rule else {
        panic!()
    };
    assert_eq!(doc.particles[0].term, Term::Choice((1..=6).collect()));
    assert!(
        matches!(&doc.particles[1].term, Term::Element { name, ty } if name == "pedido" && *ty == NodeId::new(1))
    );

    // pedido: Content y Attributes; efectivo (4): vacío; cliente (2): texto.
    let reglas = |i: usize| {
        ir.nodes[i]
            .rules
            .iter()
            .map(|c| &c.rule)
            .collect::<Vec<_>>()
    };
    assert!(matches!(
        reglas(1)[..],
        [Rule::Content(_), Rule::Attributes(_)]
    ));
    assert!(reglas(4).is_empty());
    assert!(matches!(reglas(2)[..], [Rule::SimpleContent(t)] if *t == NodeId::new(7)));
    let Rule::Attributes(attrs) = reglas(1)[1] else {
        panic!()
    };
    assert_eq!(attrs.len(), 2);
    assert!(attrs[0].required && attrs[0].ty == NodeId::new(7));
    assert_eq!(attrs[1].default.as_deref(), Some("nuevo"));
    assert_eq!(attrs[1].ty, NodeId::new(8));
    assert_eq!(ir.nodes[8].sort, Sort::Text);
    let Rule::Attributes(item) = reglas(5)[1] else {
        panic!()
    };
    assert_eq!(item[1].fixed.as_deref(), Some("COP"));

    // El modelo de pedido en preorden: Seq(cliente, Choice(tarjeta, efectivo), item+, nota?).
    let Rule::Content(m) = reglas(1)[0] else {
        panic!()
    };
    assert_eq!(m.particles[0].term, Term::Sequence(vec![1, 2, 5, 6]));
    assert_eq!(m.particles[2].term, Term::Choice(vec![3, 4]));
    assert_eq!(m.particles[5].occurs, ir::Occurs { min: 1, max: None });
    assert_eq!(
        m.particles[6].occurs,
        ir::Occurs {
            min: 0,
            max: Some(1)
        }
    );
}

// --------------------------------------------------------------- doradas ---

fn doradas() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(Path::new(TESTS).join("schemas/dtd/parse-ok"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "dtd"))
        .collect();
    v.sort();
    v
}

/// `tests/ir/dtd/<n>.json` es la IR (JSON v2) de `tests/schemas/dtd/parse-ok/<n>.dtd`.
/// Para regenerarlas tras un cambio de la bajada revisado a mano:
/// `DTD_DORADAS=escribir cargo test -p dtd --test bajada`.
#[test]
fn ir_doradas_dtd() {
    let escribir = std::env::var("DTD_DORADAS").is_ok_and(|v| v == "escribir");
    let fuentes = doradas();
    assert!(fuentes.len() >= 2, "hacen falta al menos 2 DTD con dorada");
    for f in fuentes {
        let nombre = f.file_stem().unwrap().to_string_lossy().into_owned();
        let salida = || {
            let mut s = String::new();
            ir::write_json(&ir(&std::fs::read(&f).unwrap()), &mut s).unwrap();
            s
        };
        let a = salida();
        assert_eq!(a, salida(), "{nombre}: no determinista");
        let v: serde_json::Value = serde_json::from_str(&a).unwrap();
        assert_eq!(v["format"], "xml", "{nombre}");
        let dorada = Path::new(TESTS).join(format!("ir/dtd/{nombre}.json"));
        if escribir {
            std::fs::write(&dorada, &a).unwrap();
        }
        let esperado = std::fs::read_to_string(&dorada)
            .unwrap_or_else(|e| panic!("{}: {e}", dorada.display()));
        assert_eq!(a, esperado, "{nombre}: la IR cambió respecto de la dorada");
    }
}

// --------------------------------------------------- de extremo a extremo ---

fn valida(dtd: &str, casos: &[(&str, bool)]) {
    let a = automaton::build(&ir(&fuente(dtd))).unwrap_or_else(|e| panic!("{dtd}: {e:?}"));
    for (xml, esperado) in casos {
        for bloque in [1, 7, 4096] {
            let r = automaton::validate(&a, xml.as_bytes(), Mode::Total, bloque);
            assert_eq!(r.valid, *esperado, "{dtd}, bloque {bloque}: {xml}");
        }
    }
}

#[test]
fn el_automata_del_pedido_valida_como_la_dtd() {
    valida(
        "pedido",
        &[
            (
                r#"<pedido id="7"><cliente>Ana</cliente><efectivo/><item sku="x">2</item></pedido>"#,
                true,
            ),
            (
                r#"<pedido id="7" estado="pagado"><cliente>Ana</cliente><tarjeta>4242</tarjeta><item sku="x" moneda="COP">1</item><item sku="y">2</item><nota>urgente</nota></pedido>"#,
                true,
            ),
            // Cualquier elemento declarado puede ser la raíz.
            ("<cliente>Ana</cliente>", true),
            // Falta el atributo requerido `id`.
            (
                r#"<pedido><cliente>Ana</cliente><efectivo/><item sku="x">2</item></pedido>"#,
                false,
            ),
            // `estado` fuera de su enumeración.
            (
                r#"<pedido id="7" estado="otro"><cliente>A</cliente><efectivo/><item sku="x">2</item></pedido>"#,
                false,
            ),
            // `moneda` distinta del valor #FIXED.
            (
                r#"<pedido id="7"><cliente>A</cliente><efectivo/><item sku="x" moneda="USD">2</item></pedido>"#,
                false,
            ),
            // Orden equivocado, falta `item+`, y las dos opciones a la vez.
            (
                r#"<pedido id="7"><efectivo/><cliente>A</cliente><item sku="x">2</item></pedido>"#,
                false,
            ),
            (
                r#"<pedido id="7"><cliente>A</cliente><efectivo/></pedido>"#,
                false,
            ),
            (
                r#"<pedido id="7"><cliente>A</cliente><tarjeta>1</tarjeta><efectivo/><item sku="x">2</item></pedido>"#,
                false,
            ),
            // `efectivo` es EMPTY; atributo no declarado; elemento no declarado.
            ("<efectivo>texto</efectivo>", false),
            (r#"<cliente vip="si">Ana</cliente>"#, false),
            ("<factura/>", false),
        ],
    );
}

#[test]
fn el_automata_de_la_biblioteca_admite_la_recursion() {
    valida(
        "biblioteca",
        &[
            ("<biblioteca/>", true),
            (
                r#"<biblioteca><libro isbn="1" idioma="es"><titulo>T</titulo><autor>A</autor><autor>B</autor><seccion nivel="2"><titulo>S</titulo><parrafo>p</parrafo><seccion><titulo>S2</titulo></seccion></seccion><anexo/></libro></biblioteca>"#,
                true,
            ),
            // Sin `isbn`; `idioma` fuera de la enumeración; sección sin título.
            (
                "<biblioteca><libro><titulo>T</titulo><autor>A</autor></libro></biblioteca>",
                false,
            ),
            (
                r#"<libro isbn="1" idioma="de"><titulo>T</titulo><autor>A</autor></libro>"#,
                false,
            ),
            (r#"<seccion><parrafo>p</parrafo></seccion>"#, false),
        ],
    );
}
