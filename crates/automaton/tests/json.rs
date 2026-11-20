//! Constructor JSON de F4.1a y núcleo de `runtime-rs` (criterios de
//! `tasks/todo.md`, «Tarea F4.1a»): marco «cualquiera», `type`, `false`,
//! alias, `properties`/`additionalProperties`, `prefixItems`/`items`,
//! `required`/`dependentRequired`, claves repetidas, `Pending`, los dos
//! modos, la dorada contada a mano y D7.

use automaton::{
    build, validate, Automaton, BuildError, ClassId, ErrorKind, Mode, Report, StateId, Task, ERROR,
};
use ir::{RuleKind, VerifiedIr};
use runtime_rs::tables::{class, Op};
use runtime_rs::Weight;

/// Fases 2 y 3 y la bajada (en un hilo de 8 MiB: el parser es recursivo,
/// D7).
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

/// El reporte, igual con bloques de 1, 7 y 4096 bytes.
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

fn total(a: &Automaton, payload: &str) -> Report {
    correr(a, payload, Mode::Total)
}

fn violaciones(r: &Report) -> Option<u64> {
    r.violations.get()
}

const DORADA: &str = r#"{"type":"object","properties":{"a":{"type":"integer"}},"required":["a"],"additionalProperties":false}"#;

// ------------------------------------------------------ marco «cualquiera» ---

/// Criterio 1: con `true` se acepta con w = 0 todo JSON bien formado.
#[test]
fn true_acepta_json_bien_formado() {
    let a = compilar("true");
    for p in [
        "0",
        "-1.5e-3",
        "\"\"",
        "\"a\\u00f1\\ud83d\\ude00\"",
        "true",
        "false",
        "null",
        "[]",
        "{}",
        " [1, [2, {\"a\": [null, {}]}], \"x\"] ",
        "{\"a\": {\"b\": {\"c\": []}}, \"d\": -0}",
        "\u{feff}{}",
    ] {
        let r = total(&a, p);
        assert!(r.valid, "{p:?}: {r:?}");
        assert_eq!(r.violations, Weight::ZERO);
        assert_eq!(r.error_kind, None);
    }
}

/// Criterio 1: w = ∞ en el offset del evento que lo detecta, en los dos
/// modos y con cualquier bloque.
#[test]
fn true_rechaza_malformados_en_su_offset() {
    let a = compilar("true");
    for (p, offset) in [
        ("{\"a\" 1}", 5),
        ("[1,]", 3),
        ("{,}", 1),
        ("]", 0),
        ("1 2", 2),
        ("{\"a\":1}}", 7),
        ("", 0),
        ("[", 1),
        ("{\"a\":}", 5),
        ("[1 2]", 3),
    ] {
        for mode in [Mode::Total, Mode::EarlyFail] {
            let r = correr(&a, p, mode);
            assert!(!r.valid, "{p:?}");
            assert_eq!(r.violations, Weight::INFINITY, "{p:?}");
            assert_eq!(r.error_kind, Some(ErrorKind::Syntax), "{p:?}");
            let first = r.first.unwrap();
            assert_eq!(first.offset, offset, "{p:?}");
            assert_eq!(first.rule, None);
        }
    }
    // Un error léxico también es ∞, en la posición del lexer.
    let r = total(&a, "[01]");
    assert_eq!(r.error_kind, Some(ErrorKind::Lexical));
    assert_eq!(r.violations, Weight::INFINITY);
    assert_eq!(r.first.unwrap().offset, 2);
}

// ------------------------------------------------------------- dorada ---

/// Criterio 3: la dorada, contada a mano en `docs/fase4/automata.md` §8:
/// 24 estados, 16 clases y 77 transiciones (entradas de δ distintas del
/// error), por grupos.
#[test]
fn dorada_contada_a_mano() {
    let a = compilar(DORADA);
    let s = a.summary();
    assert_eq!((s.states, s.classes, s.transitions), (24, 16, 77));
    assert_eq!(s.frames, 3);
    // Un marco de máscara: 1 palabra de registros + 4 (F4.6: buena
    // formación y cabecera; F4.5b: hash canónico y clave del miembro).
    assert_eq!(s.max_frame_bytes, 40);
    // El trie de claves {a} no se reduce al minimizarlo (F4.3).
    assert_eq!((s.dfa_states, s.dfa_states_min), (3, 3));
    let mut por_grupo = vec![(0usize, 0usize); a.groups().len()];
    for q in 0..a.n_states() as u32 {
        let g = a.state(StateId(q)).group as usize;
        por_grupo[g].0 += 1;
        for c in 0..a.n_classes() as u16 {
            if a.delta(StateId(q), ClassId(c)) != ERROR {
                por_grupo[g].1 += 1;
            }
        }
    }
    let nombres: Vec<&str> = a.groups().iter().map(String::as_str).collect();
    assert_eq!(
        nombres,
        [
            "raíz (Z₀)",
            "cualquiera: objeto",
            "cualquiera: arreglo",
            "cualquiera: escalares",
            "objeto #",
            "número #/properties/a",
        ]
    );
    assert_eq!(
        por_grupo,
        [(2, 7), (6, 16), (3, 17), (2, 5), (10, 30), (1, 2)]
    );
    // Sus payloads: el entero requerido, sin claves de más.
    assert!(total(&a, r#"{"a": 1}"#).valid);
    assert!(total(&a, r#"{"a": 1.0}"#).valid);
    let r = total(&a, r#"{"a": 1.5}"#);
    assert_eq!(violaciones(&r), Some(1));
    assert_eq!(a.rule(r.first.unwrap().rule.unwrap()).1, RuleKind::Type);
    // Clave prohibida: 1 (el `false` de additionalProperties, en su valor);
    // y falta `a`: 1 más, en `}`.
    let r = total(&a, r#"{"b": 2}"#);
    assert_eq!(violaciones(&r), Some(2));
    let first = r.first.unwrap();
    assert_eq!(first.offset, 6);
    assert_eq!(
        a.rule(first.rule.unwrap()),
        (ir::NodeId::new(2), RuleKind::Reject)
    );
    assert_eq!(
        a.rule_location(first.rule.unwrap()),
        "#/additionalProperties"
    );
    // Tipo incorrecto: 1, y el resto de reglas sigue (falta `a`): 2.
    assert_eq!(violaciones(&total(&a, "[]")), Some(1));
    assert_eq!(violaciones(&total(&a, "[{}]")), Some(1));
    assert_eq!(violaciones(&total(&a, "{}")), Some(1));
}

// ------------------------------------------------------------ escalares ---

/// Criterio 4: entero por los registros del número, sin guardar el lexema.
#[test]
fn entero_por_los_registros_del_numero() {
    let a = compilar(r#"{"type": "integer"}"#);
    for p in [
        "1", "-0", "0.0", "1.0", "1e2", "10e-1", "100e-2", "1.50e1", "0e-5", "1E+2",
    ] {
        assert!(total(&a, p).valid, "{p}");
    }
    for p in ["1.5", "1e-1", "0.10", "100e-3", "-2.25e1"] {
        let r = total(&a, p);
        assert_eq!(violaciones(&r), Some(1), "{p}");
        assert_eq!(r.first.unwrap().offset, 0);
    }
    // Exponentes de 10⁵ cifras: sin pánico y con la respuesta exacta.
    let cifras = "7".repeat(100_000);
    assert!(total(&a, &format!("1.5e{cifras}")).valid);
    assert!(total(&a, &format!("1e+{cifras}")).valid);
    assert_eq!(violaciones(&total(&a, &format!("1e-{cifras}"))), Some(1));
    assert!(total(&a, &format!("0.0e-{cifras}")).valid);
    let ceros = format!("1e{}5", "0".repeat(100_000));
    assert!(total(&a, &ceros).valid);
    assert_eq!(violaciones(&total(&a, "\"1\"")), Some(1));
}

#[test]
fn type_con_varios_tipos() {
    let a = compilar(r#"{"type": ["string", "null", "number"]}"#);
    for p in ["\"x\"", "null", "1.5", "2"] {
        assert!(total(&a, p).valid, "{p}");
    }
    for p in ["true", "{}", "[]"] {
        assert_eq!(violaciones(&total(&a, p)), Some(1), "{p}");
    }
    let b = compilar(r#"{"type": "boolean"}"#);
    assert!(total(&b, "false").valid);
    assert_eq!(violaciones(&total(&b, "1")), Some(1));
}

/// `false` y `{"not": …}` aparte: `false` cuenta 1 en cualquier valor, y
/// el valor se recorre con el marco «cualquiera».
#[test]
fn false_cuenta_una_violacion() {
    let a = compilar("false");
    for p in ["1", "{\"a\": [1, 2]}", "[]", "\"x\""] {
        let r = total(&a, p);
        assert_eq!(violaciones(&r), Some(1), "{p}");
        assert_eq!(r.error_kind, Some(ErrorKind::Schema));
    }
    // Malformado sigue siendo ∞ aunque haya violaciones antes.
    assert_eq!(total(&a, "[1,]").violations, Weight::INFINITY);
}

// ------------------------------------------------------------- objetos ---

#[test]
fn properties_y_additional_properties() {
    let a = compilar(
        r#"{"properties": {"ab": {"type": "string"}, "a": {"type": "integer"}, "ñ😀": false},
            "additionalProperties": {"type": "boolean"}}"#,
    );
    assert!(total(&a, r#"{"ab": "x", "a": 1, "zz": true, "abc": false}"#).valid);
    // Escapes en la clave: "\u0061" es "a".
    assert!(total(&a, r#"{"\u0061": 2}"#).valid);
    assert_eq!(violaciones(&total(&a, r#"{"\u0061": "x"}"#)), Some(1));
    assert_eq!(violaciones(&total(&a, r#"{"ñ😀": 1}"#)), Some(1));
    assert_eq!(violaciones(&total(&a, r#"{"ñ": 1, "😀": true}"#)), Some(1));
    assert_eq!(
        violaciones(&total(&a, r#"{"ab": 1, "a": "x", "b": 1}"#)),
        Some(3)
    );
    // Solo `true`/`false` y anidados con el marco del hijo.
    assert!(total(&a, "[1, {\"ab\": 1}]").valid);
}

/// D2: una clave repetida, declarada o no (F4.6), es malformado (∞) en la
/// comilla de **cierre** de la segunda, donde se sabe que lo es (como la
/// línea base, D50 B14; columna incluida, también tras un escape).
#[test]
fn clave_declarada_repetida_es_infinito() {
    let a = compilar(r#"{"properties": {"a": true}, "required": ["b"]}"#);
    for (p, offset, col) in [
        (r#"{"a":1,"a":2}"#, 9, 10),
        (r#"{"b":1, "a":[], "b":{}}"#, 18, 19),
        (r#"{"a":1,"\u0061":1}"#, 14, 15),
        (r#"{"x":1,"x":2}"#, 9, 10),
        (r#"{"b":1, "zé":[], "zé":{}}"#, 22, 21),
        (r#"{"x":{"x":1, "x":2}}"#, 15, 16),
    ] {
        let r = total(&a, p);
        assert_eq!(r.violations, Weight::INFINITY, "{p}");
        assert_eq!(r.error_kind, Some(ErrorKind::Syntax));
        assert_eq!(r.first.unwrap().offset, offset, "{p}");
        assert_eq!(r.first.unwrap().column, col, "{p}");
    }
    // Una clave no declarada puede repetirse en objetos distintos, también
    // anidados y tras cerrar el hijo.
    assert!(!total(&a, r#"{"b":1, "x": {"x": 1}, "y": [{"x": 2}]}"#)
        .error_kind
        .is_some_and(|k| k != ErrorKind::Schema));
    // Repetidas en objetos distintos, no.
    assert!(total(&a, r#"{"b": {"a": 1, "c": {"a": 2}}}"#).valid);
}

#[test]
fn required_y_dependent_required() {
    let a =
        compilar(r#"{"required": ["x", "y"], "dependentRequired": {"x": ["z"], "w": ["x", "y"]}}"#);
    assert!(total(&a, r#"{"x": 1, "y": 2, "z": 3}"#).valid);
    // Faltan las dos: 1 violación de `required` (una por regla).
    let r = total(&a, "{}");
    assert_eq!(violaciones(&r), Some(1));
    assert_eq!(r.first.unwrap().offset, 1);
    // `x` sin `z`: 1 de `dependentRequired`.
    let r = total(&a, r#"{"x": 1, "y": 2}"#);
    assert_eq!(violaciones(&r), Some(1));
    assert_eq!(
        a.rule(r.first.unwrap().rule.unwrap()).1,
        RuleKind::DependentRequired
    );
    // `w` sin `x` ni `y` y `required` incumplido: 2.
    assert_eq!(violaciones(&total(&a, r#"{"w": 1}"#)), Some(2));
    // Máscaras de más de 64 bits.
    let nombres: Vec<String> = (0..130).map(|i| format!("\"k{i}\"")).collect();
    let b = compilar(&format!(r#"{{"required": [{}]}}"#, nombres.join(",")));
    let todos: Vec<String> = (0..130).map(|i| format!("\"k{i}\": {i}")).collect();
    assert!(total(&b, &format!("{{{}}}", todos.join(","))).valid);
    let menos: Vec<String> = (0..130)
        .filter(|&i| i != 100)
        .map(|i| format!("\"k{i}\": 0"))
        .collect();
    assert_eq!(
        violaciones(&total(&b, &format!("{{{}}}", menos.join(",")))),
        Some(1)
    );
}

/// Criterio 4: en fallo temprano se detiene en la primera transición con
/// peso > 0 e informa de la de menor costo, aunque en el esquema venga
/// después (`required`, c = 2, frente a `dependentRequired`, c = 3).
#[test]
fn fallo_temprano_informa_la_de_menor_costo() {
    let a = compilar(r#"{"dependentRequired": {"a": ["b"]}, "required": ["c"]}"#);
    let t = correr(&a, r#"{"a": 1}"#, Mode::Total);
    assert_eq!(violaciones(&t), Some(2));
    assert_eq!(a.rule(t.first.unwrap().rule.unwrap()).1, RuleKind::Required);
    let e = correr(&a, r#"{"a": 1}"#, Mode::EarlyFail);
    assert_eq!(violaciones(&e), Some(1));
    assert_eq!(a.rule(e.first.unwrap().rule.unwrap()).1, RuleKind::Required);
    // Se detiene: lo que sigue (otra violación, o un malformado) no cuenta.
    let b = compilar(r#"{"items": {"type": "string"}}"#);
    let e = correr(&b, "[1, 2, ]", Mode::EarlyFail);
    assert_eq!(violaciones(&e), Some(1));
    assert_eq!(e.first.unwrap().offset, 1);
    assert_eq!(violaciones(&correr(&b, "[1, 2]", Mode::Total)), Some(2));
}

// ------------------------------------------------------------ arreglos ---

#[test]
fn prefix_items_e_items() {
    let a = compilar(
        r#"{"prefixItems": [{"type": "string"}, {"type": "boolean"}], "items": {"type": "integer"}}"#,
    );
    assert!(total(&a, "[]").valid);
    assert!(total(&a, "[\"a\"]").valid);
    assert!(total(&a, "[\"a\", true, 1, 2, 3]").valid);
    assert_eq!(violaciones(&total(&a, "[1, 1, 1.5, \"x\"]")), Some(4));
    let b = compilar(r#"{"prefixItems": [{"type": "string"}], "items": false}"#);
    assert_eq!(violaciones(&total(&b, "[\"a\", 1, 2]")), Some(2));
    let c = compilar(r#"{"prefixItems": [{"type": "string"}]}"#);
    assert!(total(&c, "[\"a\", 1, {}]").valid);
}

// --------------------------------------------------------------- alias ---

#[test]
fn ref_sin_hermanas_es_alias() {
    let a = compilar(
        r##"{"$ref": "#/$defs/n", "$defs": {"n": {"$ref": "#/$defs/m"}, "m": {"type": "array", "items": {"$ref": "#/$defs/n"}}}}"##,
    );
    assert!(total(&a, "[[], [[[]]]]").valid);
    assert_eq!(violaciones(&total(&a, "[[1], [[{}]]]")), Some(2));
    // Un alias no crea estados: los de `m` y nada más.
    assert_eq!(a.groups().len(), 5);
}

// ----------------------------------------------------------- pendientes ---

#[test]
fn lo_no_implementado_es_pending_con_su_tarea() {
    // Lo de F4.1b ya compila.
    for src in [
        r#"{"minimum": 1}"#,
        r#"{"enum": [1, "a"]}"#,
        r#"{"allOf": [true]}"#,
        r##"{"$ref": "#/$defs/a", "type": "object", "$defs": {"a": true}}"##,
    ] {
        assert!(build(&bajar(src)).is_ok(), "{src}");
    }
    for (src, rule, task) in [
        (r#"{"enum": [[1]]}"#, RuleKind::Enum, Task::Literals),
        (r#"{"const": {"a": 1}}"#, RuleKind::Const, Task::Literals),
    ] {
        match build(&bajar(src)) {
            Err(BuildError::Pending {
                rule: r, task: t, ..
            }) => assert_eq!((r, t), (rule, task), "{src}"),
            otro => panic!("{src}: {otro:?}"),
        }
    }
    let e = build(&bajar(r#"{"properties": {"x": {"const": [1]}}}"#)).unwrap_err();
    assert_eq!(
        e.to_string(),
        "estrategia pendiente: const (F4.5b) en #/properties/x"
    );
    // Lo inalcanzable no cuenta.
    assert!(build(&bajar(
        r#"{"$defs": {"x": {"const": [1]}}, "type": "string"}"#
    ))
    .is_ok());
    // F4.5b: `uniqueItems` ya compila.
    assert!(build(&bajar(r#"{"uniqueItems": true}"#)).is_ok());
    // F4.4b: `pattern` ya compila en JSON.
    assert!(build(&bajar(r#"{"properties": {"x": {"pattern": "a"}}}"#)).is_ok());
}

/// Desde F4.1c un XSD compila, y desde F4.4b también su `pattern`.
#[test]
fn xsd_compila_con_su_pattern() {
    let ir_de = |src: &'static str| {
        std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                let s = xsd::parse(src.as_bytes()).unwrap();
                xsd::lower(&xsd::analyze(&s).unwrap()).unwrap()
            })
            .unwrap()
            .join()
            .unwrap()
    };
    let ir = ir_de(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
         <xs:element name='e' type='xs:int'/></xs:schema>",
    );
    assert!(build(&ir).is_ok());
    let ir = ir_de(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
         <xs:element name='e'><xs:simpleType><xs:restriction base='xs:string'>\
         <xs:pattern value='a+'/></xs:restriction></xs:simpleType></xs:element></xs:schema>",
    );
    // F4.4b: también su `xs:pattern` (anclado).
    let a = build(&ir).unwrap();
    assert!(validate(&a, b"<e>aaa</e>", Mode::Total, 7).valid);
    assert_eq!(
        validate(&a, b"<e>aab</e>", Mode::Total, 7).violations.get(),
        Some(1)
    );
}

// ----------------------------------------------------------- propiedades ---

/// Criterio 4: δ total y sin estados sobrantes. Cada (q, clase) tiene una
/// transición válida (el error por defecto) y cada estado se alcanza desde
/// q₀ por `next` o por el retorno de un `Push`/`Enter`.
#[test]
fn delta_total_y_estados_alcanzables() {
    for src in [
        "true",
        DORADA,
        r#"{"prefixItems": [true, {"type": "object", "required": ["q"]}], "dependentRequired": {"a": ["b"]}}"#,
        r##"{"$defs": {"n": {"type": "object", "properties": {"c": {"type": "array", "items": {"$ref": "#/$defs/n"}}}}}, "$ref": "#/$defs/n"}"##,
    ] {
        let a = compilar(src);
        let t = a.tables();
        assert_eq!(t.delta.len(), a.n_states() * a.n_classes());
        assert!(t.delta.iter().all(|&id| (id as usize) < t.trans.len()));
        assert_eq!(
            t.trans[ERROR.0 as usize].op,
            Op::Fail(runtime_rs::tables::Fault::Syntax)
        );
        let mut seen = vec![false; a.n_states()];
        let mut pila = vec![a.start().0];
        seen[a.start().0 as usize] = true;
        while let Some(q) = pila.pop() {
            for c in 0..a.n_classes() as u16 {
                let tr = a.transition(a.delta(StateId(q), ClassId(c)));
                let mut destinos = vec![];
                match tr.op {
                    Op::Move => destinos.push(tr.next),
                    Op::Push | Op::Enter => destinos.extend([tr.next, tr.ret]),
                    _ => {}
                }
                for d in destinos {
                    if !seen[d as usize] {
                        seen[d as usize] = true;
                        pila.push(d);
                    }
                }
            }
        }
        assert!(seen.iter().all(|&s| s), "{src}: estados inalcanzables");
        // Propiedad de pila visible: la operación de pila la fija la clase.
        for q in 0..a.n_states() as u32 {
            for c in 0..a.n_classes() as u16 {
                let op = a.transition(a.delta(StateId(q), ClassId(c))).op;
                let ok = match u32::from(c) {
                    class::BEGIN_OBJECT | class::BEGIN_ARRAY => {
                        matches!(op, Op::Push | Op::Fail(_))
                    }
                    class::END_OBJECT | class::END_ARRAY => matches!(op, Op::Pop | Op::Fail(_)),
                    _ => !matches!(op, Op::Push | Op::Pop),
                };
                assert!(ok, "{src}: q{q}, clase {c}: {op:?}");
            }
        }
    }
}

// ------------------------------------------------------------------ D7 ---

/// Criterio 5: una cadena de 10⁵ `$ref` y un esquema recursivo productivo
/// se construyen en un hilo de 256 KiB; el núcleo recorre un payload de
/// 10⁵ niveles sin recursión (también en un hilo de 256 KiB).
#[test]
fn d7_cadena_de_ref_y_payload_profundo() {
    let m = 100_000;
    let mut src = String::from(r##"{"$ref": "#/$defs/d0", "$defs": {"##);
    for i in 0..m {
        if i > 0 {
            src.push(',');
        }
        if i + 1 < m {
            src.push_str(&format!(r##""d{i}": {{"$ref": "#/$defs/d{}"}}"##, i + 1));
        } else {
            src.push_str(&format!(
                r##""d{i}": {{"type": "array", "items": {{"$ref": "#/$defs/d0"}}}}"##
            ));
        }
    }
    src.push_str("}}");
    let cadena = bajar(&src);
    let arbol = bajar(
        r##"{"$defs": {"node": {"type": "object", "properties": {"value": {"type": "integer"},
            "children": {"type": "array", "items": {"$ref": "#/$defs/node"}}},
            "required": ["value"], "additionalProperties": false}}, "$ref": "#/$defs/node"}"##,
    );
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || {
            let a = build(&cadena).unwrap();
            // Todo es alias del último: un solo arreglo propio.
            assert_eq!(a.groups().len(), 5);
            let n = 100_000;
            // F4.6: la profundidad la acota `Limits::max_depth` (256 por
            // defecto); aquí, 2·10⁵ + 1 para los dos niveles por nodo del
            // árbol.
            let mut o = automaton::Options::new(Mode::Total);
            o.limits.max_depth = 2 * n + 1;
            let validate = |a: &automaton::Automaton, p: &[u8], m: Mode, c: usize| {
                assert_eq!(m, Mode::Total);
                automaton::validate_with(a, p, &o, c)
            };
            let profundo = format!("{}{}", "[".repeat(n), "]".repeat(n));
            let r = validate(&a, profundo.as_bytes(), Mode::Total, 4096);
            assert!(r.valid, "{r:?}");
            let malo = format!("{}1{}", "[".repeat(n), "]".repeat(n));
            assert_eq!(
                validate(&a, malo.as_bytes(), Mode::Total, 7)
                    .violations
                    .get(),
                Some(1)
            );
            let b = build(&arbol).unwrap();
            let mut p = String::new();
            for _ in 0..n {
                p.push_str(r#"{"value": 1, "children": ["#);
            }
            p.push_str(r#"{"value": 2}"#);
            p.push_str(&"]}".repeat(n));
            assert!(validate(&b, p.as_bytes(), Mode::Total, 4096).valid);
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Sin sitio para otro marco en la memoria del llamador: error de límite
/// (∞), nunca un pánico ni una reserva.
#[test]
fn pila_llena_da_limit() {
    use runtime_rs::{Flow, JsonValidator, Options};
    let a = compilar(DORADA);
    // Dos marcos de «cualquiera» (`FRAME_OVERHEAD` = 4 palabras cada uno)
    // caben; el tercero no.
    let mut mem = vec![0u64; 8];
    let mut v = JsonValidator::new(a.tables(), &Options::new(Mode::Total), &mut mem);
    assert_eq!(v.feed(b"[[[]]]"), Flow::Stop);
    let r = v.finish();
    assert_eq!(r.error_kind, Some(ErrorKind::Limit));
    assert_eq!(r.violations, Weight::INFINITY);
    assert_eq!(r.first.unwrap().offset, 2);
    let mut mem = vec![0u64; 12];
    let mut v = JsonValidator::new(a.tables(), &Options::new(Mode::Total), &mut mem);
    assert_eq!(v.feed(b"[[[]]]"), Flow::Continue);
    // La raíz exige un objeto: 1 violación, sin límite.
    assert_eq!(v.finish().violations.get(), Some(1));
}
