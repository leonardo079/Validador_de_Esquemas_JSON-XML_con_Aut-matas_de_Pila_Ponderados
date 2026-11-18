//! F4.1b (criterios de `tasks/todo.md`, «Tarea F4.1b»): contadores,
//! `enum`/`const` de escalares, rangos y `multipleOf` exactos, el lexema del
//! número y los límites heredados de F4.1a (F4.1a-CR-I4, CR-S1). Todo
//! payload se valida con bloques de 1, 7 y 4096 bytes y se exige el mismo
//! reporte.

use automaton::{build, validate, Automaton, BuildError, ErrorKind, Mode, Report, MAX_NAMES};
use ir::{RuleKind, VerifiedIr};
use runtime_rs::Weight;

fn bajar(src: &str) -> VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let schema = schema_parser::parse(src.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
            let table = semantic::analyze(&schema).unwrap_or_else(|e| panic!("{e:?}"));
            semantic::lower(&table).unwrap_or_else(|e| panic!("{e}"))
        })
        .unwrap()
        .join()
        .unwrap()
}

fn compilar(src: &str) -> Automaton {
    build(&bajar(src)).unwrap_or_else(|e| panic!("{e}"))
}

fn correr(a: &Automaton, payload: &str, mode: Mode) -> Report {
    let r = validate(a, payload.as_bytes(), mode, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate(a, payload.as_bytes(), mode, chunk),
            r,
            "{payload:?} con bloques de {chunk}"
        );
    }
    r
}

/// Violaciones en conformidad total (`None` = ∞).
fn v(a: &Automaton, payload: &str) -> Option<u64> {
    correr(a, payload, Mode::Total).violations.get()
}

/// La regla de la primera violación.
fn primera(a: &Automaton, payload: &str) -> RuleKind {
    let r = correr(a, payload, Mode::Total);
    a.rule(r.first.unwrap().rule.unwrap()).1
}

// --------------------------------------------------------- contadores ---

#[test]
fn min_y_max_properties() {
    let a = compilar(r#"{"minProperties": 1, "maxProperties": 2}"#);
    assert_eq!(v(&a, "{}"), Some(1));
    assert_eq!(v(&a, r#"{"a": 1}"#), Some(0));
    assert_eq!(v(&a, r#"{"a": 1, "b": {"x": 1, "y": 2, "z": 3}}"#), Some(0));
    assert_eq!(v(&a, r#"{"a": 1, "b": 2, "c": 3}"#), Some(1));
    // Solo cuenta en objetos.
    assert_eq!(v(&a, "[]"), Some(0));
    // La violación se detecta en la `}` que cierra el objeto.
    let r = correr(&a, r#"{"a": 1, "b": 2, "c": 3}"#, Mode::Total);
    assert_eq!(r.first.unwrap().offset, 23);
    assert_eq!(primera(&a, "{}"), RuleKind::MinProperties);
}

#[test]
fn min_y_max_items() {
    let a = compilar(r#"{"minItems": 2, "maxItems": 3, "items": {"maxItems": 0}}"#);
    assert_eq!(v(&a, "[]"), Some(1));
    assert_eq!(v(&a, "[[]]"), Some(1));
    assert_eq!(v(&a, "[[], []]"), Some(0));
    assert_eq!(v(&a, "[[], [], []]"), Some(0));
    assert_eq!(v(&a, "[[], [], [], []]"), Some(1));
    // Un hijo con elementos viola su `maxItems` (cada marco su contador).
    assert_eq!(v(&a, "[[1], [], [2, 3]]"), Some(2));
    assert_eq!(v(&a, "{}"), Some(0));
    // Con `prefixItems` el contador sigue contando tras saturar el índice.
    let b = compilar(r#"{"prefixItems": [{"type": "string"}], "maxItems": 2}"#);
    assert_eq!(v(&b, r#"["a", 1]"#), Some(0));
    assert_eq!(v(&b, r#"["a", 1, 2]"#), Some(1));
    assert_eq!(v(&b, r#"[1, 1, 2]"#), Some(2));
}

/// F4.1a-CR-S1: la máscara y el contador van separados en el marco
/// (`mask_words`): `required` y `dependentRequired` no leen el contador, ni
/// con máscaras de más de una palabra.
#[test]
fn mascara_separada_del_contador() {
    let a =
        compilar(r#"{"required": ["a"], "minProperties": 2, "dependentRequired": {"b": ["c"]}}"#);
    assert_eq!(v(&a, r#"{"a": 1}"#), Some(1));
    assert_eq!(primera(&a, r#"{"a": 1}"#), RuleKind::MinProperties);
    assert_eq!(v(&a, r#"{"x": 1, "y": 2}"#), Some(1));
    assert_eq!(primera(&a, r#"{"x": 1, "y": 2}"#), RuleKind::Required);
    assert_eq!(v(&a, r#"{"a": 1, "b": 2}"#), Some(1));
    assert_eq!(
        primera(&a, r#"{"a": 1, "b": 2}"#),
        RuleKind::DependentRequired
    );
    assert_eq!(v(&a, r#"{"a": 1, "b": 2, "c": 3}"#), Some(0));
    // 70 nombres: dos palabras de máscara y una de contador.
    let nombres: Vec<String> = (0..70).map(|i| format!("\"p{i}\"")).collect();
    let src = format!(
        r#"{{"required": [{}], "maxProperties": 70}}"#,
        nombres.join(",")
    );
    let b = compilar(&src);
    let lleno: Vec<String> = (0..70).map(|i| format!("\"p{i}\": {i}")).collect();
    assert_eq!(v(&b, &format!("{{{}}}", lleno.join(","))), Some(0));
    let falta: Vec<String> = (1..70).map(|i| format!("\"p{i}\": {i}")).collect();
    assert_eq!(v(&b, &format!("{{{}}}", falta.join(","))), Some(1));
    let mas = format!("{{{}, \"q\": 0}}", lleno.join(","));
    assert_eq!(v(&b, &mas), Some(1));
    assert_eq!(primera(&b, &mas), RuleKind::MaxProperties);
    let f = a.frame(automaton::FrameId(2));
    assert_eq!((f.words, f.mask_words), (2, 1));
}

/// Criterio 3: `minLength`/`maxLength` en puntos de código tras los
/// escapes: `"😀"` (un punto de código, 4 bytes en UTF-8, o el par de
/// sustitutos `"\ud83d\ude00"`) y `"ñ"` (o `"\u00f1"`) miden 1.
#[test]
fn longitudes_en_puntos_de_codigo() {
    let a = compilar(r#"{"minLength": 1, "maxLength": 1}"#);
    for p in [
        "\"😀\"",
        "\"ñ\"",
        "\"\\ud83d\\ude00\"",
        "\"\\u00f1\"",
        r#""\n""#,
        "\"a\"",
    ] {
        assert_eq!(v(&a, p), Some(0), "{p}");
    }
    assert_eq!(v(&a, "\"\""), Some(1));
    assert_eq!(v(&a, "\"😀😀\""), Some(1));
    assert_eq!(v(&a, "\"\\u00f1a\""), Some(1));
    assert_eq!(v(&a, "\"\\ud83d\\ude00\\ud83d\\ude00\""), Some(1));
    assert_eq!(v(&a, "1"), Some(0));
    // La violación de una cadena se detecta en su cierre, con el offset de
    // su comienzo (el `Span` de `StringEnd` cubre la cadena).
    let r = correr(&a, r#"  "ab""#, Mode::Total);
    assert_eq!(r.first.unwrap().offset, 2);
    // Las claves no cuentan para el valor.
    let b = compilar(r#"{"additionalProperties": {"maxLength": 2}}"#);
    assert_eq!(v(&b, r#"{"una clave larga": "ok"}"#), Some(0));
    assert_eq!(v(&b, r#"{"k": "largo"}"#), Some(1));
}

// ------------------------------------------------------- enum y const ---

/// Criterio 3: un `enum` de cadenas con prefijos compartidos y la cadena
/// vacía.
#[test]
fn enum_de_cadenas_con_prefijos_y_vacia() {
    let a = compilar(r#"{"enum": ["", "a", "ab", "abc", "b", "ñu", "😀"]}"#);
    for p in [
        "\"\"",
        "\"a\"",
        "\"ab\"",
        "\"abc\"",
        "\"b\"",
        "\"ñu\"",
        "\"\\u00f1u\"",
        "\"😀\"",
    ] {
        assert_eq!(v(&a, p), Some(0), "{p}");
    }
    for p in [
        "\"abcd\"", "\"ac\"", "\"c\"", "\"ñ\"", "\"A\"", "1", "null", "[]",
    ] {
        assert_eq!(v(&a, p), Some(1), "{p}");
        assert_eq!(primera(&a, p), RuleKind::Enum, "{p}");
    }
}

/// Criterio 3: `1`, `1.0` y `1e0` son el mismo valor en `enum` y `const`;
/// `enum` y `const` juntos; `true`/`false`/`null`.
#[test]
fn numeros_por_valor_y_enum_con_const() {
    let a = compilar(r#"{"enum": [1, "1", true, null, 2.5e-1]}"#);
    for p in [
        "1", "1.0", "1e0", "10e-1", "0.1e1", "\"1\"", "true", "null", "0.25", "25e-2",
    ] {
        assert_eq!(v(&a, p), Some(0), "{p}");
    }
    for p in [
        "2",
        "-1",
        "1.0000000000000000000001",
        "false",
        "\"1.0\"",
        "{}",
    ] {
        assert_eq!(v(&a, p), Some(1), "{p}");
    }
    let c = compilar(r#"{"const": 1.0}"#);
    for p in ["1", "1e0", "1.000"] {
        assert_eq!(v(&c, p), Some(0), "{p}");
    }
    assert_eq!(v(&c, "1.5"), Some(1));
    assert_eq!(primera(&c, "1.5"), RuleKind::Const);
    // `enum` y `const` en el mismo nodo: dos reglas, cada una su violación.
    let b = compilar(r#"{"enum": ["a", "b", 3], "const": "a"}"#);
    assert_eq!(v(&b, "\"a\""), Some(0));
    assert_eq!(v(&b, "\"b\""), Some(1));
    assert_eq!(primera(&b, "\"b\""), RuleKind::Const);
    assert_eq!(v(&b, "3"), Some(1));
    assert_eq!(v(&b, "\"c\""), Some(2));
    assert_eq!(v(&b, "true"), Some(2));
    // Valores repetidos por valor se funden.
    let d = compilar(r#"{"enum": [1, 1.0, 1e0, 2, false]}"#);
    assert_eq!(v(&d, "2.0"), Some(0));
    assert_eq!(v(&d, "false"), Some(0));
    assert_eq!(v(&d, "true"), Some(1));
}

/// D11: `enum: []` no acepta ningún valor (1 violación, como `false`).
#[test]
fn enum_vacio_no_acepta_nada() {
    let a = compilar(r#"{"enum": []}"#);
    for p in ["1", "\"\"", "true", "false", "null", "{}", "[]"] {
        assert_eq!(v(&a, p), Some(1), "{p}");
    }
}

// --------------------------------------------- rangos y multipleOf ---

#[test]
fn rangos_exactos_sobre_el_lexema() {
    let a = compilar(r#"{"minimum": -1.5, "exclusiveMaximum": 0.1}"#);
    for p in [
        "-1.5",
        "-15e-1",
        "0",
        "0.0999999999999999999999999999",
        "-1.4",
    ] {
        assert_eq!(v(&a, p), Some(0), "{p}");
    }
    for (p, n) in [
        ("0.1", 1),
        ("0.10", 1),
        ("1e-1", 1),
        ("0.10000000000000000001", 1),
        ("-1.5000000000000000001", 1),
        ("1e999999999999999999999999999999", 1),
        ("-1e999999999999999999999999999999", 1),
    ] {
        assert_eq!(v(&a, p), Some(n), "{p}");
    }
    let b = compilar(r#"{"exclusiveMinimum": 3, "maximum": 3e0}"#);
    assert_eq!(v(&b, "3"), Some(1));
    assert_eq!(primera(&b, "3"), RuleKind::ExclusiveMinimum);
    assert_eq!(v(&b, "3.0000001"), Some(1));
    assert_eq!(primera(&b, "3.0000001"), RuleKind::Maximum);
    // Solo se aplica a números.
    assert_eq!(v(&b, "\"x\""), Some(0));
    // El offset de la violación es el comienzo del número.
    let r = correr(&b, " 3", Mode::Total);
    assert_eq!(r.first.unwrap().offset, 1);
}

/// Criterio 2 por el autómata: `multipleOf` exacto, con `type: integer`
/// contado aparte.
#[test]
fn multiple_of_por_el_automata() {
    let a = compilar(r#"{"multipleOf": 0.01}"#);
    for p in ["0.1", "0.10", "1e-2", "-3.14", "0", "1e400"] {
        assert_eq!(v(&a, p), Some(0), "{p}");
    }
    for p in ["0.005", "1.001", "1e-400"] {
        assert_eq!(v(&a, p), Some(1), "{p}");
    }
    let b = compilar(r#"{"type": "integer", "multipleOf": 3}"#);
    assert_eq!(v(&b, "3e2"), Some(0));
    assert_eq!(v(&b, "1e2"), Some(1));
    assert_eq!(primera(&b, "1e2"), RuleKind::MultipleOf);
    assert_eq!(v(&b, "1.5"), Some(2));
    // En fallo temprano se informa la de menor costo (`type`, c = 1).
    let r = correr(&b, "1.5", Mode::EarlyFail);
    assert_eq!(r.violations.get(), Some(1));
    assert_eq!(b.rule(r.first.unwrap().rule.unwrap()).1, RuleKind::Type);
}

/// Criterio 2: un divisor por encima del tope de F4-U6 (parte coprima con 10
/// ≥ 2⁶⁰) da `limit` con la ubicación de la regla.
#[test]
fn divisor_por_encima_del_tope_da_limit() {
    let src = format!(
        r#"{{"properties": {{"x": {{"multipleOf": {}}}}}}}"#,
        (1u64 << 61) - 1
    );
    match build(&bajar(&src)) {
        Err(e @ BuildError::Limit { .. }) => {
            let BuildError::Limit {
                found, location, ..
            } = &e
            else {
                unreachable!()
            };
            assert_eq!(location, "#/properties/x");
            assert_eq!(*found, 61);
            assert!(e.to_string().contains("multipleOf"), "{e}");
        }
        otro => panic!("{otro:?}"),
    }
    assert!(build(&bajar(r#"{"multipleOf": 1152921504606846883}"#)).is_ok());
}

/// El lexema del número se guarda hasta 1 024 bytes (`00-alcance.md` §4): si
/// una comprobación lo necesita y es más largo, `limit` (∞) en el comienzo
/// del número; `integer` no lo necesita.
#[test]
fn lexema_largo_da_limit_solo_si_hace_falta() {
    let largo = format!("1e{}", "9".repeat(10_000));
    let a = compilar(r#"{"items": {"minimum": 0}}"#);
    let r = correr(&a, &format!("[0, {largo}]"), Mode::Total);
    assert_eq!(r.error_kind, Some(ErrorKind::Limit));
    assert_eq!(r.violations, Weight::INFINITY);
    assert_eq!(r.first.unwrap().offset, 4);
    let justo = format!("1{}", "0".repeat(1023));
    assert_eq!(v(&a, &format!("[{justo}]")), Some(0));
    let b = compilar(r#"{"type": "integer"}"#);
    assert_eq!(v(&b, &largo), Some(0));
    // `multipleOf` también lee el lexema: un exponente de 10⁴ cifras en el
    // payload da `limit` por diseño (la rutina, que no tiene tope, lo decide
    // exacto: `runtime-rs/tests/num.rs::tabla_de_multiple_of`).
    let m = compilar(r#"{"multipleOf": 0.01}"#);
    for p in [largo.clone(), format!("1e-{}", "9".repeat(10_000))] {
        let r = correr(&m, &format!(" {p}"), Mode::Total);
        assert_eq!(r.error_kind, Some(ErrorKind::Limit), "{}", &p[..8]);
        assert_eq!(r.first.unwrap().offset, 1);
    }
    // Un divisor del esquema con un exponente de 10⁴ cifras sí se decide
    // exacto en el autómata (las cotas del esquema no tienen tope).
    let d = compilar(&format!(r#"{{"multipleOf": 1e-{}}}"#, "9".repeat(10_000)));
    assert_eq!(v(&d, "0.1234567"), Some(0));
    let g = compilar(&format!(r#"{{"multipleOf": 1e{}}}"#, "9".repeat(10_000)));
    assert_eq!(v(&g, "1e100"), Some(1));
    assert_eq!(v(&g, "0"), Some(0));
}

// ------------------------------------------------------------- límites ---

/// F4.1a-CR-I4: 4 096 nombres por objeto compilan y 4 097 dan `limit` con
/// su ubicación.
#[test]
fn limite_de_nombres_por_objeto() {
    let src = |n: usize| {
        let props: Vec<String> = (0..n).map(|i| format!("\"k{i}\": true")).collect();
        format!(
            r#"{{"properties": {{"o": {{"properties": {{{}}}}}}}}}"#,
            props.join(",")
        )
    };
    let a = build(&bajar(&src(MAX_NAMES))).unwrap();
    assert_eq!(v(&a, r#"{"o": {"k4095": 1, "k0": 2}}"#), Some(0));
    match build(&bajar(&src(MAX_NAMES + 1))) {
        Err(BuildError::Limit {
            limit,
            found,
            location,
            ..
        }) => {
            assert_eq!((limit, found), (MAX_NAMES, MAX_NAMES + 1));
            assert_eq!(location, "#/properties/o");
        }
        otro => panic!("{otro:?}"),
    }
}

/// F4.1a-CR-I4: un esquema por encima de 2²⁰ entradas de δ da `limit` sin
/// reservar δ: la tabla densa tendría ~10¹¹ entradas (1 TB), así que si la
/// cota no se comprobara antes de reservar, la prueba abortaría al pedir la
/// memoria.
#[test]
fn presupuesto_de_delta_da_limit_sin_reservar() {
    // 4 000 nombres de 128 caracteres distintos, fuera del BMP (≈ 5·10⁵
    // clases y ≈ 5·10⁵ estados: ≈ 2,6·10¹¹ entradas, 1 TB).
    let mut props = Vec::new();
    for i in 0..4000u32 {
        let nombre: String = (0..128u32)
            .map(|j| char::from_u32(0x10000 + i * 128 + j).unwrap())
            .collect();
        props.push(format!("\"{nombre}{i}\": true"));
    }
    let src = format!(r#"{{"properties": {{{}}}}}"#, props.join(","));
    match build(&bajar(&src)) {
        Err(BuildError::Limit {
            limit, found, what, ..
        }) => {
            assert_eq!(limit, automaton::MAX_DELTA);
            assert!(found > 1 << 37, "{found}");
            assert!(what.contains("δ"), "{what}");
        }
        otro => panic!("{otro:?}"),
    }
}
