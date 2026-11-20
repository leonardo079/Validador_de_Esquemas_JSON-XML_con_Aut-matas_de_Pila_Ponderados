//! F4.1b, conjunción por E₌ (criterio 4 de «Tarea F4.1b»; F4-U2, L-1, S-3
//! de `docs/fase3/pendientes.md` §5): marcos por conjunto canónico de
//! `NodeId`, `$ref` con hermanas y `allOf` como producto, atribución de cada
//! comprobación a su componente, tamaño lineal con referencias compartidas y
//! el tope de estados por producto contado antes de reservar.

use automaton::{build, build_with, validate, Automaton, BuildError, BuildOptions, Mode, Report};
use ir::{RuleKind, VerifiedIr};

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

fn correr(a: &Automaton, payload: &str) -> Report {
    let r = validate(a, payload.as_bytes(), Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate(a, payload.as_bytes(), Mode::Total, chunk),
            r,
            "{payload:?} con bloques de {chunk}"
        );
    }
    r
}

fn v(a: &Automaton, payload: &str) -> Option<u64> {
    correr(a, payload).violations.get()
}

/// (regla, ubicación) de la primera violación.
fn primera(a: &Automaton, payload: &str) -> (RuleKind, String) {
    let r = correr(a, payload).first.unwrap().rule.unwrap();
    (a.rule(r).1, a.rule_location(r).to_owned())
}

/// Criterio 4: el ejemplo de `fase3/ir.md` §6.2. `additionalProperties`
/// solo ve las `properties` de su objeto-esquema, así que `a` es adicional
/// para la raíz aunque la rama lo declare: 1 violación, de `false`.
#[test]
fn all_of_con_additional_properties_false() {
    let a = compilar(
        r#"{"allOf": [{"properties": {"a": {"type": "integer"}}}],
            "additionalProperties": false}"#,
    );
    assert_eq!(v(&a, r#"{"a": 1}"#), Some(1));
    assert_eq!(
        primera(&a, r#"{"a": 1}"#),
        (RuleKind::Reject, "#/additionalProperties".to_owned())
    );
    // La rama también comprueba lo suyo: `false` y `type`, 2.
    assert_eq!(v(&a, r#"{"a": "x"}"#), Some(2));
    assert_eq!(v(&a, "{}"), Some(0));
    assert_eq!(a.summary().products, 1);
}

/// Criterio 4: `$ref` con hermanas exige las dos cosas.
#[test]
fn ref_con_hermanas_exige_las_dos() {
    let a =
        compilar(r##"{"$ref": "#/$defs/a", "minimum": 3, "$defs": {"a": {"type": "integer"}}}"##);
    assert_eq!(v(&a, "5"), Some(0));
    assert_eq!(v(&a, "3.0"), Some(0));
    assert_eq!(v(&a, "2"), Some(1));
    assert_eq!(primera(&a, "2"), (RuleKind::Minimum, "#".to_owned()));
    assert_eq!(v(&a, "4.5"), Some(1));
    assert_eq!(primera(&a, "4.5"), (RuleKind::Type, "#/$defs/a".to_owned()));
    assert_eq!(v(&a, "2.5"), Some(2));
    assert_eq!(v(&a, "\"x\""), Some(1));
}

/// Cada comprobación se atribuye a su componente (su nodo y su regla), y
/// cada `Constraint` cuenta su propia violación.
#[test]
fn el_producto_atribuye_cada_comprobacion() {
    let a = compilar(
        r#"{"allOf": [{"minimum": 1}, {"maximum": 5}, {"maxLength": 2, "minLength": 1}]}"#,
    );
    assert_eq!(
        primera(&a, "9"),
        (RuleKind::Maximum, "#/allOf/1".to_owned())
    );
    assert_eq!(
        primera(&a, "0"),
        (RuleKind::Minimum, "#/allOf/0".to_owned())
    );
    assert_eq!(
        primera(&a, "\"abc\""),
        (RuleKind::MaxLength, "#/allOf/2".to_owned())
    );
    assert_eq!(v(&a, "\"\""), Some(1));
    assert_eq!(v(&a, "3"), Some(0));
}

/// S-3: los componentes son los `NodeId` distintos, no los caminos: una
/// rama repetida cuenta una vez.
#[test]
fn rama_repetida_cuenta_una_vez() {
    let a = compilar(
        r##"{"allOf": [{"$ref": "#/$defs/x"}, {"$ref": "#/$defs/x"}, {"allOf": [{"$ref": "#/$defs/x"}]}],
            "$defs": {"x": {"minimum": 1}}}"##,
    );
    assert_eq!(v(&a, "0"), Some(1));
    // Un solo componente: no es un producto.
    assert_eq!(a.summary().products, 0);
}

/// Producto de objetos: el trie de la unión de los nombres; cada nombre
/// lleva al hijo que es la unión de los cierres de cada componente; las
/// máscaras de `required` de cada componente sobre los bits de la unión.
#[test]
fn producto_de_objetos_y_de_sus_hijos() {
    let a = compilar(
        r#"{"allOf": [
              {"properties": {"a": {"minimum": 1}, "b": {"type": "string"}}},
              {"properties": {"a": {"maximum": 5}}, "required": ["a"], "maxProperties": 2}]}"#,
    );
    assert_eq!(v(&a, r#"{"a": 3}"#), Some(0));
    assert_eq!(v(&a, r#"{"a": 9}"#), Some(1));
    assert_eq!(v(&a, r#"{"a": 0}"#), Some(1));
    assert_eq!(v(&a, "{}"), Some(1));
    assert_eq!(v(&a, r#"{"a": 3, "b": 1}"#), Some(1));
    assert_eq!(v(&a, r#"{"a": 3, "b": "x", "c": null}"#), Some(1));
    assert_eq!(
        primera(&a, r#"{"a": 3, "b": "x", "c": null}"#),
        (RuleKind::MaxProperties, "#/allOf/1".to_owned())
    );
    // La clave declarada repetida sigue siendo ∞ (D2).
    assert_eq!(v(&a, r#"{"a": 3, "a": 3}"#), None);
}

#[test]
fn producto_de_arreglos() {
    let a = compilar(
        r#"{"allOf": [{"prefixItems": [{"type": "string"}]},
                      {"items": {"type": "string"}, "maxItems": 2}]}"#,
    );
    assert_eq!(v(&a, r#"["a", "b"]"#), Some(0));
    assert_eq!(v(&a, r#"["a", 1]"#), Some(1));
    // El elemento 0 lo leen las dos ramas: dos `type` distintos.
    assert_eq!(v(&a, "[1]"), Some(2));
    assert_eq!(v(&a, r#"["a", "b", "c"]"#), Some(1));
}

/// Recursión a través de un producto: los conjuntos se descubren una vez y
/// el hijo de un antepasado ya existe cuando se apila.
#[test]
fn recursion_a_traves_del_producto() {
    let a = compilar(
        r##"{"$defs": {"n": {"type": "object", "properties": {"c": {"$ref": "#/$defs/n"}}}},
            "allOf": [{"$ref": "#/$defs/n"}], "required": ["c"]}"##,
    );
    assert_eq!(v(&a, r#"{"c": {"c": {}}}"#), Some(0));
    assert_eq!(v(&a, r#"{"c": {"c": 1}}"#), Some(1));
    assert_eq!(v(&a, "{}"), Some(1));
}

/// Criterio 4: la cadena de diamantes de `referencias_compartidas_tamano_
/// lineal` (`crates/ir/tests/forma.rs`, `fase3/ir.md` §8): m = 10⁴
/// definiciones, cada una con `allOf` a la siguiente dos veces (2^(m − 1)
/// caminos). Se compila con un número lineal de marcos y de comprobaciones
/// (se comprueba el resultado, no el tiempo, D1).
#[test]
fn referencias_compartidas_tamano_lineal() {
    let m = 10_000;
    for con_reglas in [false, true] {
        let mut src = String::from(r##"{"$ref": "#/$defs/d1", "$defs": {"##);
        for i in 1..=m {
            if i > 1 {
                src.push(',');
            }
            if i < m {
                let extra = if con_reglas {
                    format!(r#", "maximum": {}"#, 1_000_000 + i)
                } else {
                    String::new()
                };
                src.push_str(&format!(
                    r##""d{i}": {{"allOf": [{{"$ref": "#/$defs/d{0}"}}, {{"$ref": "#/$defs/d{0}"}}]{extra}}}"##,
                    i + 1
                ));
            } else {
                src.push_str(&format!(r#""d{i}": {{"type": "integer", "minimum": 3}}"#));
            }
        }
        src.push_str("}}");
        let a = compilar(&src);
        let s = a.summary();
        // Los marcos (Γ) de «cualquiera» y un solo número propio.
        assert_eq!(s.frames, 2, "{con_reglas}");
        assert!(s.states <= 13 + 1, "{s:?}");
        let checks = a.tables().checks.len();
        if con_reglas {
            // Un componente por definición: m comprobaciones al cerrar el
            // número (más las de `type` de la entrada), no 2^m.
            assert!(checks <= 2 * m, "{checks}");
            assert_eq!(s.products, 1);
            assert_eq!(v(&a, "1000005"), Some(4));
        } else {
            assert!(checks <= 16, "{checks}");
            assert_eq!(s.products, 0);
        }
        assert_eq!(v(&a, "5"), Some(0));
        assert_eq!(v(&a, "2"), Some(1));
        assert_eq!(v(&a, "2.5"), Some(2));
    }
}

/// Criterio 4: con el tope bajado a 8, un producto de marcos de objeto
/// mayor da `limit` antes de reservar, con el número de estados alcanzado y
/// que el marco no es escalar; con el tope por defecto compila. El tamaño se
/// cuenta en el análisis, con el trie de la unión contado sin construirlo.
#[test]
fn tope_de_estados_por_producto() {
    let objetos = bajar(r#"{"allOf": [{"properties": {"a": true}}, {"properties": {"b": true}}]}"#);
    let ocho = BuildOptions {
        max_product_states: 8,
        ..BuildOptions::default()
    };
    match build_with(&objetos, &ocho) {
        Err(e @ BuildError::ProductLimit { .. }) => {
            let BuildError::ProductLimit {
                states,
                limit,
                scalar,
                ref location,
            } = e
            else {
                unreachable!()
            };
            // primero, siguiente, trie de {a, b} (4), 2 por nombre, 2 de la
            // clave desconocida y «tras valor».
            assert_eq!((states, limit, scalar), (13, 8, false));
            assert_eq!(location, "#/allOf/0 ∧ #/allOf/1");
            assert!(e.to_string().contains("con hijos"), "{e}");
        }
        otro => panic!("{otro:?}"),
    }
    let a = build(&objetos).unwrap();
    assert_eq!(a.summary().products, 1);
    assert_eq!(v(&a, r#"{"a": 1, "b": 2}"#), Some(0));
    // Un producto escalar (cadena) mayor que el tope: `scalar` = true.
    let cadena = bajar(r#"{"allOf": [{"enum": ["abcdefgh"]}, {"maxLength": 3}]}"#);
    match build_with(&cadena, &ocho) {
        Err(BuildError::ProductLimit { states, scalar, .. }) => {
            // Desde F4.4b el producto de la cadena se construye contando
            // sus estados al crearlos: el 9.º pasa el tope de 8.
            assert_eq!((states, scalar), (9, true));
        }
        otro => panic!("{otro:?}"),
    }
    // Un objeto solo (sin producto) no tiene tope.
    let solo = bajar(r#"{"properties": {"a": true, "b": true, "c": true}}"#);
    assert!(build_with(&solo, &ocho).is_ok());
}
