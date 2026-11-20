//! F4.5a: combinadores con salidas por rama y puertas al cerrar la
//! instancia (`docs/fase4/automata.md` §18): `anyOf`, `oneOf` y `not`
//! cuentan 1 violación; `allOf` y `dependentSchemas` suman las de dentro
//! (F4-U13); un límite o un malformado (∞) nunca se invierte.

use automaton::{build, build_with, validate, validate_with, Automaton, BuildError, ErrorKind};
use automaton::{Limits, Mode, Options, Report};
use proptest::prelude::*;
use runtime_rs::Weight;

fn ir(src: &str) -> ir::VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = schema_parser::parse(src.as_bytes()).unwrap();
            semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

fn m(src: &str) -> Automaton {
    build(&ir(src)).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// Total, en bloques de 1, 7 y 4096, con el mismo reporte.
fn r(a: &Automaton, p: &str) -> Report {
    let x = validate(a, p.as_bytes(), Mode::Total, 4096);
    for c in [1, 7] {
        assert_eq!(validate(a, p.as_bytes(), Mode::Total, c), x, "{p}");
    }
    x
}

fn v(a: &Automaton, p: &str) -> Option<u64> {
    r(a, p).violations.get()
}

#[test]
fn any_of_cuenta_uno_y_all_of_suma() {
    // Sobre un número solo falla `type` en cada rama (las longitudes no
    // aplican); sobre "ab", la rama 1 falla 1 vez y la 2, 2.
    let r1 = r#"{"type": "string", "minLength": 5}"#;
    let r2 = r#"{"type": "string", "minLength": 5, "maxLength": 0}"#;
    let any = m(&format!(r#"{{"anyOf": [{r1}, {r2}]}}"#));
    let all = m(&format!(r#"{{"allOf": [{r1}, {r2}]}}"#));
    assert_eq!(v(&any, "1"), Some(1));
    assert_eq!(v(&all, "1"), Some(2));
    assert_eq!(v(&any, r#""ab""#), Some(1));
    assert_eq!(v(&all, r#""ab""#), Some(1 + 2));
    assert_eq!(v(&any, r#""abcdef""#), Some(0));
    // Con 2 y 3 violaciones por rama: anyOf da 1, allOf 5.
    let a2 = r#"{"type": "array", "minItems": 3, "maxItems": 0}"#;
    let a3 = r#"{"type": "array", "minItems": 4, "maxItems": 0, "items": {"type": "string"}}"#;
    let any = m(&format!(r#"{{"anyOf": [{a2}, {a3}]}}"#));
    let all = m(&format!(r#"{{"allOf": [{a2}, {a3}]}}"#));
    assert_eq!(v(&any, "[1]"), Some(1));
    assert_eq!(v(&all, "[1]"), Some(5));
}

#[test]
fn not_invierte_y_el_limite_no() {
    let a = m(r#"{"not": {"type": "string"}}"#);
    assert_eq!(v(&a, "1"), Some(0));
    assert_eq!(v(&a, r#""x""#), Some(1));
    assert_eq!(v(&a, "{}"), Some(0));
    assert_eq!(v(&a, "[[]]"), Some(0));
    // Una cadena que pasa el límite: `limit` (∞), no invertido.
    let o = Options {
        mode: Mode::Total,
        limits: Limits {
            max_string: 3,
            ..Limits::default()
        },
    };
    let x = validate_with(&a, br#""abcd""#, &o, 7);
    assert_eq!(x.error_kind, Some(ErrorKind::Limit));
    assert_eq!(x.violations, Weight::INFINITY);
    // Ni un malformado.
    let x = r(&a, r#"{"a": 1,}"#);
    assert_eq!(x.error_kind, Some(ErrorKind::Syntax));
    // `not` de `not`.
    let nn = m(r#"{"not": {"not": {"type": "integer"}}}"#);
    assert_eq!(v(&nn, "1"), Some(0));
    assert_eq!(v(&nn, "1.5"), Some(1));
}

#[test]
fn one_of_exactamente_una() {
    let a = m(r#"{"oneOf": [{"type": "integer"}, {"minimum": 0}]}"#);
    assert_eq!(v(&a, "-1"), Some(0));
    assert_eq!(v(&a, "1.5"), Some(0));
    assert_eq!(v(&a, "2"), Some(1));
    assert_eq!(v(&a, "-1.5"), Some(1));
    // Dos ramas iguales que aceptan: 1.
    let b = m(r#"{"oneOf": [{"type": "string"}, {"type": "string"}]}"#);
    assert_eq!(v(&b, r#""x""#), Some(1));
    assert_eq!(v(&b, "1"), Some(1));
}

#[test]
fn dependent_schemas_suma_si_salta() {
    let a = m(r#"{"dependentSchemas": {"a": {"required": ["b"]}}}"#);
    assert_eq!(v(&a, r#"{"b": 1}"#), Some(0));
    assert_eq!(v(&a, r#"{"a": 1}"#), Some(1));
    assert_eq!(v(&a, r#"{"b": 1, "a": 1}"#), Some(0));
    assert_eq!(v(&a, "1"), Some(0));
    // Suma las de dentro.
    let b = m(r#"{"dependentSchemas": {"a": {"required": ["b"], "maxProperties": 0}}}"#);
    assert_eq!(v(&b, r#"{"a": 1}"#), Some(2));
}

/// Un `anyOf` anidado bajo `properties` de una rama cuenta en esa rama: la
/// otra rama acepta, así que no hay violación; si las dos fallan, 1.
#[test]
fn anidados_cuentan_en_su_rama() {
    let a = m(r#"{"anyOf": [
        {"properties": {"x": {"anyOf": [{"type": "string"}, {"type": "boolean"}]}}},
        {"required": ["y"]}]}"#);
    assert_eq!(v(&a, r#"{"x": "s"}"#), Some(0));
    assert_eq!(v(&a, r#"{"x": 1, "y": 0}"#), Some(0));
    assert_eq!(v(&a, r#"{"x": 1}"#), Some(1));
    // Hijos de hijos y arreglos dentro de una rama.
    let b = m(r#"{"not": {"items": {"properties": {"k": {"type": "string"}}}}}"#);
    assert_eq!(v(&b, r#"[{"k": "s"}]"#), Some(1));
    assert_eq!(v(&b, r#"[{"k": "s"}, {"k": 1}]"#), Some(0));
    // Combinadores en una clave conocida de un objeto de la raíz.
    let c = m(r#"{"properties": {"p": {"oneOf": [{"type": "object"}, {"required": ["q"]}]}}}"#);
    assert_eq!(v(&c, r#"{"p": {"q": 1}}"#), Some(1));
    assert_eq!(v(&c, r#"{"p": {}}"#), Some(0));
    // `required` no aplica a un número: la segunda rama acepta, la primera
    // no; exactamente una.
    assert_eq!(v(&c, r#"{"p": 1}"#), Some(0));
}

/// 15 `pattern` primos en `allOf`: el producto pasa el tope de 4 096
/// estados y se corta contándolos (recorte 3 del plan: `limit` con el
/// tamaño, sin simulación paralela).
#[test]
fn quince_primos_dan_limit() {
    let primos = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47];
    let ramas: Vec<String> = primos
        .iter()
        .map(|p| format!(r#"{{"pattern": "^(a{{{p}}})*$"}}"#))
        .collect();
    let src = format!(r#"{{"allOf": [{}]}}"#, ramas.join(","));
    match build(&ir(&src)) {
        Err(BuildError::ProductLimit {
            states,
            limit,
            scalar,
            ..
        }) => {
            assert_eq!(limit, 4096);
            assert_eq!(states, 4097);
            assert!(scalar);
        }
        otro => panic!("{otro:?}"),
    }
    // Con 3 primos (30 estados) compila: "" → 0, "aa" → 2, "a"×30 → 0.
    let a = m(
        r#"{"allOf": [{"pattern": "^(a{2})*$"}, {"pattern": "^(a{3})*$"}, {"pattern": "^(a{5})*$"}]}"#,
    );
    assert_eq!(v(&a, r#""""#), Some(0));
    assert_eq!(v(&a, r#""aa""#), Some(2));
    assert_eq!(v(&a, &format!("\"{}\"", "a".repeat(30))), Some(0));
}

#[test]
fn fallo_temprano_solo_se_detiene_en_lo_global() {
    // anyOf: la primera rama falla, la segunda acepta: válido, sin parar.
    let a = m(r#"{"anyOf": [{"type": "string"}, {"type": "integer"}]}"#);
    let e = validate(&a, b"1", Mode::EarlyFail, 1);
    assert!(e.valid, "{e:?}");
    // allOf: se detiene en la primera violación (la de menor costo).
    let b = m(r#"{"allOf": [{"maxLength": 0}, {"type": "string"}]}"#);
    let e = validate(&b, br#"["xx"]"#, Mode::EarlyFail, 1);
    assert_eq!(e.violations.get(), Some(1));
    assert_eq!(e.first.unwrap().offset, 0);
}

/// Una cadena de 10⁴ diamantes con `anyOf`: cada nodo una sola salida por
/// conjunto, así que compila con un número de marcos que no crece con los
/// caminos (2^(10⁴)).
#[test]
fn cadena_de_diamantes_lineal() {
    let n = 10_000;
    let mut defs = Vec::new();
    for i in 0..n {
        defs.push(format!(
            r##""d{i}": {{"anyOf": [{{"$ref": "#/$defs/d{j}"}}, {{"$ref": "#/$defs/d{j}"}}]}}"##,
            j = i + 1
        ));
    }
    defs.push(format!(r#""d{n}": {{"type": "integer"}}"#));
    let src = format!(
        r##"{{"$defs": {{{}}}, "$ref": "#/$defs/d0"}}"##,
        defs.join(",")
    );
    let a = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || build(&ir(&src)).map(|a| a.summary()))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
    assert!(a.frames < 16, "{a:?}");
}

/// Con el tope bajado, un producto de objetos en ramas da `limit`.
/// S-3 entre conjuntos: el mismo combinador alcanzado por dos partes de un
/// hijo cuenta una vez (informe de F4.5a, punto 2).
#[test]
fn combinador_compartido_cuenta_una_vez() {
    let a = m(
        r##"{"$defs": {"A": {"anyOf": [{"type": "string"}, {"minimum": 10}]}},
        "properties": {"p": {"$ref": "#/$defs/A"}},
        "allOf": [{"properties": {"p": {"allOf": [{"$ref": "#/$defs/A"}, {"type": "integer"}]}}}]}"##,
    );
    assert_eq!(v(&a, r#"{"p": 1.5}"#), Some(2));
    assert_eq!(v(&a, r#"{"p": 12}"#), Some(0));
}

#[test]
fn tope_en_ramas_de_objetos() {
    let src = r#"{"anyOf": [{"properties": {"a": true}}, {"properties": {"b": true}}]}"#;
    let ocho = automaton::BuildOptions {
        max_product_states: 8,
        ..automaton::BuildOptions::default()
    };
    assert!(matches!(
        build_with(&ir(src), &ocho),
        Err(BuildError::ProductLimit { scalar: false, .. })
    ));
}

/// Esquemas de F4.1a-b para el proptest.
const ESQUEMAS: &[&str] = &[
    r#"{"type": "object", "required": ["a"]}"#,
    r#"{"type": "array", "items": {"type": "integer"}, "maxItems": 2}"#,
    r#"{"type": "string", "minLength": 2, "enum": ["ab", "abc", "x"]}"#,
    r#"{"properties": {"a": {"minimum": 0}}, "additionalProperties": false}"#,
    r#"{"type": ["integer", "null"], "multipleOf": 3}"#,
];

fn valor() -> impl Strategy<Value = String> {
    let hoja = prop_oneof![
        Just("null".to_owned()),
        Just("true".to_owned()),
        (-4i32..10).prop_map(|n| n.to_string()),
        Just("1.5".to_owned()),
        "[abcx]{0,3}".prop_map(|s| format!("\"{s}\"")),
    ];
    hoja.prop_recursive(3, 16, 3, |d| {
        prop_oneof![
            prop::collection::vec(d.clone(), 0..3).prop_map(|v| format!("[{}]", v.join(","))),
            prop::collection::btree_map("[ab]", d, 0..3).prop_map(|m| {
                let v: Vec<String> = m.into_iter().map(|(k, x)| format!("\"{k}\":{x}")).collect();
                format!("{{{}}}", v.join(","))
            }),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// `anyOf: [S]` y `not: {not: S}` dan el mismo veredicto que S.
    #[test]
    fn any_of_y_doble_not_como_s(i in 0..ESQUEMAS.len(), p in valor()) {
        let s = ESQUEMAS[i];
        let base = m(s);
        let any = m(&format!(r#"{{"anyOf": [{s}]}}"#));
        let nn = m(&format!(r#"{{"not": {{"not": {s}}}}}"#));
        let vb = validate(&base, p.as_bytes(), Mode::Total, 7).valid;
        prop_assert_eq!(validate(&any, p.as_bytes(), Mode::Total, 7).valid, vb, "{} {}", s, p);
        prop_assert_eq!(validate(&nn, p.as_bytes(), Mode::Total, 1).valid, vb, "{} {}", s, p);
    }
}
