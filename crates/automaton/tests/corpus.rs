//! El corpus de `tests/cases` con M (criterios 1 y 2 de F4.1a): con el
//! esquema `true` acepta todo payload JSON bien formado y rechaza con ∞ los
//! malformados en su offset; `valid-recursive-tree-01` da 0,
//! `invalid-missing-required-01` da 1 de `required` en el offset 64, y los
//! `non-dpda` dan `Pending` con la tarea de F4.4/F4.5. Bloques de 1, 7 y
//! 4096 bytes.

use automaton::{build, validate, Automaton, BuildError, ErrorKind, Mode, Report};
use baseline::corpus::{self, Case, ErrorKind as Esperado, Format, Payload};
use ir::{RuleKind, VerifiedIr};
use runtime_rs::Weight;

fn casos_json() -> Vec<(Case, Vec<u8>)> {
    corpus::casos()
        .unwrap()
        .into_iter()
        .filter(|c| c.format == Format::Json)
        .filter_map(|c| match &c.payload {
            Payload::Bytes(b) => {
                let b = b.clone();
                Some((c, b))
            }
            Payload::Generator { .. } => None,
        })
        .collect()
}

/// La IR del texto exacto del esquema del caso (hilo de 8 MiB, D7).
fn ir_de(texto: &[u8]) -> VerifiedIr {
    let texto = texto.to_vec();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = schema_parser::parse(&texto).unwrap_or_else(|e| panic!("{e}"));
            let t = semantic::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"));
            semantic::lower(&t).unwrap_or_else(|e| panic!("{e}"))
        })
        .unwrap()
        .join()
        .unwrap()
}

fn compilar(texto: &[u8]) -> Result<Automaton, BuildError> {
    build(&ir_de(texto))
}

fn correr(a: &Automaton, payload: &[u8]) -> Report {
    let r = validate(a, payload, Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate(a, payload, Mode::Total, chunk),
            r,
            "bloques de {chunk}"
        );
    }
    r
}

fn malformado(c: &Case) -> bool {
    matches!(
        c.expected.error_kind,
        Some(Esperado::Lexical | Esperado::Syntax)
    )
}

#[test]
fn esquema_true_sobre_todo_el_corpus() {
    let a = compilar(b"true").unwrap();
    let casos = casos_json();
    assert!(casos.len() >= 7, "{}", casos.len());
    for (c, payload) in &casos {
        let r = correr(&a, payload);
        if malformado(c) {
            assert_eq!(r.violations, Weight::INFINITY, "{}", c.id);
            let kind = match c.expected.error_kind {
                Some(Esperado::Lexical) => ErrorKind::Lexical,
                _ => ErrorKind::Syntax,
            };
            assert_eq!(r.error_kind, Some(kind), "{}", c.id);
            if let Some(off) = c.expected.offset {
                assert_eq!(r.first.unwrap().offset, off, "{}", c.id);
            }
        } else {
            assert!(r.valid, "{}: {r:?}", c.id);
            assert_eq!(r.violations, Weight::ZERO, "{}", c.id);
        }
    }
}

fn caso(id: &str) -> (Case, Vec<u8>) {
    casos_json()
        .into_iter()
        .find(|(c, _)| c.id == id)
        .unwrap_or_else(|| panic!("falta {id}"))
}

#[test]
fn arbol_recursivo_da_cero() {
    let (c, payload) = caso("valid-recursive-tree-01");
    let a = compilar(&c.schema_text).unwrap();
    let r = correr(&a, &payload);
    assert!(r.valid, "{r:?}");
    assert_eq!(r.violations.get(), Some(0));
}

#[test]
fn falta_required_da_una_violacion_en_64() {
    let (c, payload) = caso("invalid-missing-required-01");
    let ir = ir_de(&c.schema_text);
    let a = build(&ir).unwrap();
    let r = correr(&a, &payload);
    assert!(!r.valid);
    assert_eq!(r.violations.get(), Some(1));
    assert_eq!(r.error_kind, Some(ErrorKind::Schema));
    let first = r.first.unwrap();
    assert_eq!(Some(first.offset), c.expected.offset);
    assert_eq!(first.offset, 64);
    let (node, kind) = a.rule(first.rule.unwrap());
    assert_eq!(kind, RuleKind::Required);
    assert_eq!(ir.location(node), "#/properties/users/items");
    assert_eq!(
        a.rule_location(first.rule.unwrap()),
        "#/properties/users/items"
    );
    // En fallo temprano, lo mismo.
    let e = validate(&a, &payload, Mode::EarlyFail, 7);
    assert_eq!(e.violations.get(), Some(1));
    assert_eq!(e.first, r.first);
}

/// Los `non-dpda` dan `Pending` con su tarea (F4.4b o F4.5a/b), nunca
/// aceptación.
#[test]
fn non_dpda_da_pending() {
    let mut n = 0;
    for (c, _) in casos_json()
        .iter()
        .filter(|(c, _)| c.category == "non-dpda")
    {
        match compilar(&c.schema_text) {
            Err(BuildError::Pending { task, .. }) => {
                let t = task.label();
                assert!(
                    t.starts_with("F4.4") || t.starts_with("F4.5"),
                    "{}: {t}",
                    c.id
                );
            }
            otro => panic!("{}: {otro:?}", c.id),
        }
        n += 1;
    }
    assert_eq!(n, 4);
}

/// Criterio 5 de F4.1b: todos los casos JSON del corpus sin `Pending` dan el
/// `valid`, `violations` y `offset` de `expected`.
#[test]
fn casos_que_compilan_dan_lo_esperado() {
    let mut n = 0;
    for (c, payload) in casos_json() {
        let Ok(a) = compilar(&c.schema_text) else {
            continue;
        };
        let r = correr(&a, &payload);
        assert_eq!(r.valid, c.expected.valid, "{}", c.id);
        if let Some(v) = c.expected.violations {
            assert_eq!(r.violations.get(), Some(v), "{}", c.id);
        }
        if let Some(off) = c.expected.offset {
            assert_eq!(r.first.map(|f| f.offset), Some(off), "{}", c.id);
        }
        n += 1;
    }
    assert!(n >= 4, "{n}");
}

/// F4.1c: los casos XML del corpus con bytes (`valid-xml-pedido-01` da 0,
/// `malformed-xml-billion-laughs-01` da `lexical` en su offset) dan lo de
/// `expected`, en bloques de 1, 7 y 4096 bytes.
#[test]
fn casos_xml_dan_lo_esperado() {
    let mut n = 0;
    for c in corpus::casos().unwrap() {
        if c.format != Format::Xml {
            continue;
        }
        let Payload::Bytes(payload) = &c.payload else {
            continue;
        };
        let texto = c.schema_text.clone();
        let ir = std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                let s = xsd::parse(&texto).unwrap_or_else(|e| panic!("{e}"));
                xsd::lower(&xsd::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"))).unwrap()
            })
            .unwrap()
            .join()
            .unwrap();
        let a = build(&ir).unwrap_or_else(|e| panic!("{}: {e}", c.id));
        let r = correr(&a, payload);
        assert_eq!(r.valid, c.expected.valid, "{}: {r:?}", c.id);
        if let Some(v) = c.expected.violations {
            assert_eq!(r.violations.get(), Some(v), "{}", c.id);
        }
        if let Some(off) = c.expected.offset {
            assert_eq!(r.first.map(|f| f.offset), Some(off), "{}", c.id);
        }
        if let Some(Esperado::Lexical) = c.expected.error_kind {
            assert_eq!(r.error_kind, Some(ErrorKind::Lexical), "{}", c.id);
        }
        n += 1;
    }
    assert!(n >= 2, "{n}");
}
