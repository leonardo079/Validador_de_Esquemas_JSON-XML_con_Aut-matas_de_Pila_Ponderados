//! F4.5b: `uniqueItems` por hash canónico en *streaming* (verificación
//! diferida, `docs/fase4/automata.md` §19): números por valor, cadenas por
//! punto de código, arreglos en orden, objetos sin orden; 1 violación en el
//! `]` si hay dos iguales; más de 4 096 elementos guardados → `limit`.

use automaton::{build, validate, validate_with, Automaton, ErrorKind, Limits, Mode, Options};
use proptest::prelude::*;

fn m(src: &str) -> Automaton {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = schema_parser::parse(src.as_bytes()).unwrap();
            build(&semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

fn v(a: &Automaton, p: &str) -> Option<u64> {
    let r = validate(a, p.as_bytes(), Mode::Total, 4096);
    for c in [1, 7] {
        assert_eq!(validate(a, p.as_bytes(), Mode::Total, c), r, "{p}");
    }
    r.violations.get()
}

const U: &str = r#"{"uniqueItems": true}"#;

#[test]
fn iguales_por_valor() {
    let a = m(U);
    assert_eq!(v(&a, "[1, 2, 3]"), Some(0));
    assert_eq!(v(&a, "[1, 2, 1.0]"), Some(1));
    assert_eq!(v(&a, "[10, 1e1, 100e-1]"), Some(1));
    assert_eq!(v(&a, "[0, -0, 0.0]"), Some(1));
    assert_eq!(v(&a, "[-1, 1]"), Some(0));
    assert_eq!(v(&a, r#"["a", "a"]"#), Some(1));
    assert_eq!(v(&a, r#"["a", "a"]"#), Some(1));
    assert_eq!(v(&a, r#"[1, "1"]"#), Some(0));
    assert_eq!(v(&a, "[true, 1]"), Some(0));
    assert_eq!(v(&a, "[true, true]"), Some(1));
    assert_eq!(v(&a, "[null, false, 0, \"\", [], {}]"), Some(0));
    assert_eq!(v(&a, "[[1, 2], [2, 1]]"), Some(0));
    assert_eq!(v(&a, "[[1, 2], [1, 2.0]]"), Some(1));
    assert_eq!(v(&a, r#"[{"a": 1, "b": 2}, {"b": 2, "a": 1}]"#), Some(1));
    assert_eq!(v(&a, r#"[{"a": 1}, {"a": 2}]"#), Some(0));
    assert_eq!(
        v(
            &a,
            r#"[{"a": [1, {"b": null}]}, {"a": [1, {"b": null}], "c": 1}]"#
        ),
        Some(0)
    );
    // Separación de dominio (informe de F4.5b, 7.0): `false` y una cadena
    // cuyos bytes coinciden con su etiqueta no chocan.
    let laf = format!(r#"["laf{}", false]"#, "\\u0000".repeat(12));
    assert_eq!(v(&a, &laf), Some(0));
    // El cero no lee su exponente: `0e…` enorme es 0; un exponente enorme
    // con mantisa ≠ 0 da `limit` (§19).
    assert_eq!(v(&a, "[0e99999999999999999999, 0]"), Some(1));
    let r = validate(&a, b"[1e99999999999999999999]", Mode::Total, 7);
    assert_eq!(r.error_kind, Some(ErrorKind::Limit));
    // Fuera de un arreglo no aplica.
    assert_eq!(v(&a, r#"{"a": 1}"#), Some(0));
}

/// La violación va en el `]` del arreglo, atribuida a `uniqueItems`.
#[test]
fn en_el_cierre() {
    let a = m(U);
    let r = validate(&a, b"[1, 2, 1.0]", Mode::Total, 4096);
    assert_eq!(r.error_kind, Some(ErrorKind::Schema));
    assert_eq!(r.first.unwrap().offset, 10);
    let (_, kind) = a.rule(r.first.unwrap().rule.unwrap());
    assert_eq!(kind, ir::RuleKind::UniqueItems);
}

/// Arreglos anidados con `uniqueItems` comparten la reserva y cada uno
/// comprueba solo su región; los hashes del hijo no se mezclan con los del
/// padre.
#[test]
fn anidados_comparten_la_reserva() {
    let a = m(r#"{"uniqueItems": true, "items": {"uniqueItems": true}}"#);
    assert_eq!(v(&a, "[[1, 2], [3, 4]]"), Some(0));
    assert_eq!(v(&a, "[[1, 1], [3, 4]]"), Some(1));
    assert_eq!(v(&a, "[[1, 2], [1, 2]]"), Some(1));
    assert_eq!(v(&a, "[[1, 1], [1, 1]]"), Some(3));
    // Bajo `properties` de un objeto dentro del arreglo.
    let b = m(r#"{"uniqueItems": true, "items": {"properties": {"l": {"uniqueItems": true}}}}"#);
    assert_eq!(v(&b, r#"[{"l": [1, 1]}, {"l": [1]}]"#), Some(1));
}

/// 4 096 elementos únicos caben; el 4 097.º da `limit` en su cierre.
#[test]
fn tope_de_elementos() {
    let a = m(U);
    let lista = |n: usize| {
        let v: Vec<String> = (0..n).map(|i| i.to_string()).collect();
        format!("[{}]", v.join(","))
    };
    assert_eq!(v(&a, &lista(4096)), Some(0));
    let p = lista(4097);
    let r = validate(&a, p.as_bytes(), Mode::Total, 4096);
    assert_eq!(r.error_kind, Some(ErrorKind::Limit), "{r:?}");
    assert_eq!(r.first.unwrap().offset, p.rfind(',').unwrap() as u64 + 1);
    // Con otro tope.
    let o = Options {
        mode: Mode::Total,
        limits: Limits {
            max_unique: 2,
            ..Limits::default()
        },
    };
    assert!(validate_with(&a, b"[1, 2]", &o, 7).valid);
    let r = validate_with(&a, b"[1, 2, 3]", &o, 7);
    assert_eq!(r.error_kind, Some(ErrorKind::Limit));
    assert_eq!(r.first.unwrap().offset, 7);
}

/// Con `not`, el «repetido» se invierte como cualquier violación.
#[test]
fn bajo_not() {
    let a = m(r#"{"not": {"uniqueItems": true}}"#);
    assert_eq!(v(&a, "[1, 1]"), Some(0));
    assert_eq!(v(&a, "[1, 2]"), Some(1));
}

/// Valor JSON pequeño como texto, con su forma normalizada para el oráculo.
fn valor() -> impl Strategy<Value = String> {
    let hoja = prop_oneof![
        Just("null".to_owned()),
        Just("true".to_owned()),
        Just("false".to_owned()),
        prop_oneof![
            Just("1"),
            Just("1.0"),
            Just("10e-1"),
            Just("2"),
            Just("-0"),
            Just("0")
        ]
        .prop_map(str::to_owned),
        "[ab]{0,2}".prop_map(|s| format!("\"{s}\"")),
    ];
    hoja.prop_recursive(2, 8, 3, |d| {
        prop_oneof![
            prop::collection::vec(d.clone(), 0..3).prop_map(|v| format!("[{}]", v.join(","))),
            prop::collection::btree_map("[ab]", d, 0..3).prop_map(|m| {
                let mut v: Vec<String> =
                    m.into_iter().map(|(k, x)| format!("\"{k}\":{x}")).collect();
                v.reverse();
                format!("{{{}}}", v.join(","))
            }),
        ]
    })
}

/// Forma canónica para el oráculo: números por valor (f64 basta con estos
/// literales), objetos con claves ordenadas.
fn canon(x: &serde_json::Value) -> String {
    use serde_json::Value;
    match x {
        Value::Number(n) => format!("n{}", n.as_f64().unwrap() + 0.0),
        Value::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
        Value::Object(o) => {
            let mut v: Vec<String> = o
                .iter()
                .map(|(k, x)| format!("{k:?}:{}", canon(x)))
                .collect();
            v.sort();
            format!("{{{}}}", v.join(","))
        }
        otro => otro.to_string(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// `uniqueItems` coincide con un oráculo `std` (valores normalizados).
    #[test]
    fn coincide_con_el_oraculo(xs in prop::collection::vec(valor(), 0..5)) {
        let a = m(U);
        let p = format!("[{}]", xs.join(","));
        let parsed: Vec<serde_json::Value> = xs.iter().map(|x| serde_json::from_str(x).unwrap()).collect();
        let mut c: Vec<String> = parsed.iter().map(canon).collect();
        c.sort();
        let repetido = c.windows(2).any(|w| w[0] == w[1]);
        let r = validate(&a, p.as_bytes(), Mode::Total, 7);
        prop_assert_eq!(r.violations.get(), Some(u64::from(repetido)), "{}", p);
    }
}
