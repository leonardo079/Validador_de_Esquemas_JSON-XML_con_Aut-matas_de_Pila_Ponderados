//! El corpus de `tests/cases` con M (criterios 1 y 2 de F4.1a y 1-2 y 6 de
//! F4.6): con el esquema `true` acepta todo payload JSON bien formado y
//! rechaza con ∞ los malformados en su offset; todo caso cuyo esquema
//! compila (JSON y XML) da el `valid`, `errorKind`, `violations` y `offset` de
//! `expected`, con el mismo reporte en bloques de 1, 7 y 4096 bytes y el
//! mismo veredicto en fallo temprano (o, ante un error de peso ∞, una
//! violación anterior: F6.3-P1); en JSON, el veredicto coincide con la
//! línea base de F6.7a (D48); y los `non-dpda` que aún no compilan dan
//! `Pending` con la tarea de F4.4/F4.5.

use automaton::{build, validate, Automaton, BuildError, ErrorKind, Mode, Report};
use baseline::corpus::{self, Case, ErrorKind as Esperado, Format, Payload};
use baseline::{run_case, with_stack, Kind};
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
        if c.expected.error_kind == Some(Esperado::Limit) {
            // Los límites que dependen del esquema (el lexema) no saltan con
            // `true`; la profundidad, sí.
            assert!(
                r.valid || r.error_kind == Some(ErrorKind::Limit),
                "{}: {r:?}",
                c.id
            );
        } else if malformado(c) {
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
            // F4.4b y F4.5a: `pattern` y los combinadores ya compilan.
            Ok(_) => continue,
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
    // F4.5b: ya no queda ningún `non-dpda` pendiente.
    assert_eq!(n, 0);
}

/// El IR de un caso del corpus, JSON o XML (hilo de 8 MiB, D7).
fn ir_del_caso(c: &Case) -> VerifiedIr {
    let texto = c.schema_text.clone();
    let xml = c.format == Format::Xml;
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            if xml {
                let s = xsd::parse(&texto).unwrap_or_else(|e| panic!("{e}"));
                xsd::lower(&xsd::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"))).unwrap()
            } else {
                let s = schema_parser::parse(&texto).unwrap_or_else(|e| panic!("{e}"));
                let t = semantic::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"));
                semantic::lower(&t).unwrap_or_else(|e| panic!("{e}"))
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn clase(k: Esperado) -> ErrorKind {
    match k {
        Esperado::Lexical => ErrorKind::Lexical,
        Esperado::Syntax => ErrorKind::Syntax,
        Esperado::Schema => ErrorKind::Schema,
        Esperado::Limit => ErrorKind::Limit,
    }
}

/// Criterio 1 de F4.6 (y 5 de F4.1b, criterio de F4.1c): todo caso del
/// corpus con bytes cuyo esquema compila, JSON o XML, da el `valid`,
/// `errorKind`, `violations` y `offset` de `expected`, con un reporte
/// idéntico en bloques de 1, 7 y 4096 bytes (`correr`). Criterio 6: el
/// fallo temprano da el mismo veredicto (y, sin error de peso ∞, se
/// detiene en la primera violación con peso 1).
#[test]
fn todo_el_corpus_da_lo_esperado() {
    let mut n = 0;
    for c in corpus::casos().unwrap() {
        let Payload::Bytes(payload) = &c.payload else {
            continue;
        };
        let a = match build(&ir_del_caso(&c)) {
            Ok(a) => a,
            Err(BuildError::Pending { .. }) if c.category == "non-dpda" => continue,
            Err(e) => panic!("{}: {e}", c.id),
        };
        let r = correr(&a, payload);
        assert_eq!(r.valid, c.expected.valid, "{}: {r:?}", c.id);
        assert_eq!(r.error_kind, c.expected.error_kind.map(clase), "{}", c.id);
        match c.expected.violations {
            Some(v) => assert_eq!(r.violations.get(), Some(v), "{}", c.id),
            None if !c.expected.valid => {
                assert_eq!(r.violations, Weight::INFINITY, "{}", c.id)
            }
            None => {}
        }
        if let Some(off) = c.expected.offset {
            assert_eq!(r.first.map(|f| f.offset), Some(off), "{}", c.id);
        }
        // F6.1: el puntero del primer error.
        if let Some(p) = &c.expected.pointer {
            assert_eq!(r.pointer.as_str(), p, "{}", c.id);
            assert!(!r.pointer.truncated(), "{}", c.id);
        }
        let e = validate(&a, payload, Mode::EarlyFail, 7);
        assert_eq!(e.valid, r.valid, "{}: fallo temprano {e:?}", c.id);
        if e.error_kind != r.error_kind {
            // F6.3-P1 (opción a, `docs/fase6/pendientes.md` §28): con un error
            // de peso ∞, el fallo temprano se detiene antes si hay una
            // violación delante de él.
            assert_eq!(e.error_kind, Some(ErrorKind::Schema), "{}", c.id);
            assert_eq!(e.violations, Weight::ONE, "{}", c.id);
            assert!(
                e.first.unwrap().offset <= r.first.unwrap().offset,
                "{}",
                c.id
            );
        } else if r.error_kind == Some(ErrorKind::Schema) {
            assert_eq!(e.violations, Weight::ONE, "{}", c.id);
            assert_eq!(e.first, r.first, "{}", c.id);
        } else {
            assert_eq!(e, r, "{}", c.id);
        }
        n += 1;
    }
    assert!(n >= 17, "{n}");
}

/// Casos en que el DPDA y la línea base divergen a propósito, con su motivo
/// (los mismos que `DIVERGENCIAS` de `crates/baseline/tests/corpus_json.rs`).
const DIVERGENCIAS: &[(&str, &str)] = &[(
    "extreme-big-number-01",
    "lexema de más de 1 024 bytes: `limit` en el DPDA, `Unsupported` en serde_json",
)];

/// Criterio 2 de F4.6: en JSON, el veredicto de M (válido, malformado,
/// violación o límite) coincide con el de la línea base DOM de F6.7a
/// (`serde_json` + `jsonschema`, D48) en cada caso que compila, salvo las
/// divergencias declaradas; y el recuento, salvo las de F4-U3/U13 (hoy
/// ninguna en el corpus).
#[test]
fn json_coincide_con_la_linea_base() {
    let casos: Vec<(Case, Vec<u8>)> = casos_json();
    let mut n = 0;
    for (c, payload) in &casos {
        let a = match build(&ir_del_caso(c)) {
            Ok(a) => a,
            Err(BuildError::Pending { .. }) => continue,
            Err(e) => panic!("{}: {e}", c.id),
        };
        let r = correr(&a, payload);
        let o = with_stack(|s| run_case(c, s)).unwrap_or_else(|e| panic!("{}: {e}", c.id));
        assert_eq!(r.valid, o.valid, "{}: {r:?} / {o:?}", c.id);
        if DIVERGENCIAS.iter().any(|(id, _)| *id == c.id) {
            assert_ne!(o.kind, Some(Kind::Limit), "{}: ya no diverge", c.id);
            continue;
        }
        let esperado = match r.error_kind {
            None => None,
            Some(ErrorKind::Lexical | ErrorKind::Syntax) => Some(Kind::Malformed),
            Some(ErrorKind::Schema) => Some(Kind::Schema),
            Some(ErrorKind::Limit) => Some(Kind::Limit),
            Some(ErrorKind::Internal) => panic!("{}: interno", c.id),
        };
        assert_eq!(o.kind, esperado, "{}: {r:?} / {o:?}", c.id);
        if let Some(v) = r.violations.get() {
            assert_eq!(v, o.violation_count() as u64, "{}", c.id);
        }
        // F6.1: el puntero del primer error es el de una de las violaciones
        // de la línea base (`instance_path` de `jsonschema`) o un
        // descendiente suyo: `jsonschema` atribuye `additionalProperties:
        // false` al objeto, y M lo detecta en el valor del miembro
        // (`docs/fase6/puntero.md` §3).
        if r.error_kind == Some(ErrorKind::Schema) {
            let p = r.pointer.as_str();
            let bajo = |q: &str| p == q || p.strip_prefix(q).is_some_and(|r| r.starts_with('/'));
            assert!(
                o.violations.iter().any(|v| bajo(&v.pointer)),
                "{}: {p:?} no está en {:?}",
                c.id,
                o.violations.iter().map(|v| &v.pointer).collect::<Vec<_>>()
            );
        }
        n += 1;
    }
    assert!(n >= 10, "{n}");
}
