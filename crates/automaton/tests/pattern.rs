//! F4.4b: `pattern` → AFD minimizado dentro de M (`docs/fase4/pattern.md`
//! §6): el AFD coincide con el oráculo `ir::regex::Regex::matches`, sus
//! tamaños son los de la teoría y M lo aplica a las cadenas JSON.

use automaton::pattern::{dfa_accepts, states};
use automaton::{build, validate, Automaton, ErrorKind, Mode, Report};
use ir::regex::{parse, Dialect};
use proptest::prelude::*;

const CAP: usize = 4096;

/// Patrones del corpus, de F4.4a y de frontera (anclas, nulos, astrales,
/// clases negadas, `{m,n}`).
const PATRONES: &[(Dialect, &str)] = &[
    (Dialect::Ecma, "^[a-z]+[0-9]*$"),
    (Dialect::Ecma, r"^[A-Z]{3}-\d{4}$"),
    (Dialect::Ecma, "^(?:a|b)+$"),
    (Dialect::Ecma, r"[^\n]"),
    (Dialect::Ecma, "^a|b$"),
    (Dialect::Ecma, "a"),
    (Dialect::Ecma, "^a"),
    (Dialect::Ecma, "a$"),
    (Dialect::Ecma, ""),
    (Dialect::Ecma, "^$"),
    (Dialect::Ecma, "^.$"),
    (Dialect::Ecma, "(ab|a)*b"),
    (Dialect::Ecma, "^(a+)+$"),
    (Dialect::Ecma, "a{2,3}b?"),
    (Dialect::Ecma, "^😀+é$"),
    (Dialect::Ecma, r"\s\S"),
    (Dialect::Ecma, "^[^ab]*$"),
    (Dialect::Xsd, "[A-Z]{3}"),
    (Dialect::Xsd, "(a|b)*a(a|b){3}"),
    (Dialect::Xsd, "a?b*"),
    (Dialect::Xsd, "^a$"),
    (Dialect::Xsd, ".*a.*"),
    (Dialect::Xsd, "(ab){2,}"),
];

fn cadena() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just('a'),
            Just('b'),
            Just('A'),
            Just('Z'),
            Just('0'),
            Just('-'),
            Just(' '),
            Just('\n'),
            Just('é'),
            Just('😀'),
        ],
        0..10,
    )
    .prop_map(|v| v.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// El AFD minimizado y la simulación de posiciones coinciden.
    #[test]
    fn afd_y_oraculo_coinciden(i in 0..PATRONES.len(), s in cadena()) {
        let (d, p) = PATRONES[i];
        let oraculo = parse(p, d).unwrap().matches(&s);
        prop_assert_eq!(dfa_accepts(p, d, &s), oraculo, "{:?} {} sobre {:?}", d, p, s);
    }
}

#[test]
fn hay_veinte_patrones() {
    assert!(PATRONES.len() >= 20);
}

/// `^(a{p})*$`: p estados del contador módulo p más el muerto.
#[test]
fn contador_modulo_p() {
    for p in [2usize, 3, 5, 7, 11] {
        let src = format!("^(a{{{p}}})*$");
        assert_eq!(states(&src, Dialect::Ecma, CAP), Some(p + 1), "{src}");
    }
}

/// `(a|b)*a(a|b){k}` necesita 2^(k+1) estados (recordar las k + 1 últimas
/// letras): con k = 12 pasa el tope antes de crear el estado 4 097; con
/// k = 8, compila.
#[test]
fn explosion_de_subconjuntos() {
    assert_eq!(states("(a|b)*a(a|b){12}", Dialect::Xsd, CAP), None);
    let n = states("(a|b)*a(a|b){8}", Dialect::Xsd, CAP).unwrap();
    // Anclado (XSD): las 2⁹ ventanas de 9 letras y el muerto.
    assert_eq!(n, (1 << 9) + 1);
    // En M, el tope da `limit` en la ubicación del `pattern`.
    let e = build(&json_ir(r#"{"pattern": "^(a|b)*a(a|b){12}$"}"#)).unwrap_err();
    assert!(e.to_string().contains("AFD de `pattern`"), "{e}");
}

/// `^(a+)+$` (el ReDoS clásico) queda en 3 estados, y en M 10⁵ `a` y un
/// `!` se leen con un paso por carácter: 1 violación, sin retroceso.
#[test]
fn redos_clasico_es_lineal() {
    assert!(states("^(a+)+$", Dialect::Ecma, CAP).unwrap() <= 3);
    let a = json(r#"{"pattern": "^(a+)+$"}"#);
    let p = format!("\"{}!\"", "a".repeat(100_000));
    let r = validate(&a, p.as_bytes(), Mode::Total, 4096);
    assert_eq!(r.violations.get(), Some(1));
    let p = format!("\"{}\"", "a".repeat(100_000));
    assert!(validate(&a, p.as_bytes(), Mode::Total, 4096).valid);
}

fn json_ir(src: &str) -> ir::VerifiedIr {
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

fn json(src: &str) -> Automaton {
    build(&json_ir(src)).unwrap()
}

/// El mismo reporte en bloques de 1, 7 y 4096 bytes.
fn correr(a: &Automaton, p: &str) -> Report {
    let r = validate(a, p.as_bytes(), Mode::Total, 4096);
    for c in [1, 7] {
        assert_eq!(validate(a, p.as_bytes(), Mode::Total, c), r, "{p}");
    }
    r
}

fn violaciones(a: &Automaton, p: &str) -> Option<u64> {
    correr(a, p).violations.get()
}

#[test]
fn json_busca_sin_anclar_y_solo_en_cadenas() {
    let a = json(r#"{"pattern": "a"}"#);
    assert_eq!(violaciones(&a, r#""xay""#), Some(0));
    assert_eq!(violaciones(&a, r#""xy""#), Some(1));
    // `pattern` sobre un número (o cualquier no cadena) no aplica.
    assert_eq!(violaciones(&a, "12"), Some(0));
    assert_eq!(violaciones(&a, "[1, {}]"), Some(0));
    let b = json(r#"{"pattern": "^a"}"#);
    assert_eq!(violaciones(&b, r#""ba""#), Some(1));
    assert_eq!(violaciones(&b, r#""ab""#), Some(0));
    // Los escapes del JSON se aplican antes: `a` es `a`.
    assert_eq!(violaciones(&b, r#""ab""#), Some(0));
}

/// Varias reglas sobre la misma cadena: `pattern` con `enum`, longitudes y
/// otro `pattern` (por `allOf`), cada una con su violación.
#[test]
fn pattern_con_otras_reglas_de_cadena() {
    let a = json(
        r#"{"allOf": [{"pattern": "^[a-z]+$"}, {"pattern": "x"}],
            "enum": ["abc", "xyz", "X"], "maxLength": 3}"#,
    );
    assert_eq!(violaciones(&a, r#""xyz""#), Some(0));
    // No es minúscula ni tiene x.
    assert_eq!(violaciones(&a, r#""abc""#), Some(1));
    assert_eq!(violaciones(&a, r#""X""#), Some(2));
    // Fuera del enum, más largo y con mayúsculas.
    assert_eq!(violaciones(&a, r#""ABCD""#), Some(4));
    let r = correr(&a, r#""ABCD""#);
    assert_eq!(r.error_kind, Some(ErrorKind::Schema));
}

/// Un `pattern` con clases que no son caracteres sueltos (rangos, `\d`,
/// `.`, negadas) junto a las claves de un objeto: la partición global las
/// respeta (F4-B3).
#[test]
fn clases_con_rangos_y_claves() {
    let a = json(
        r#"{"properties": {"código": {"pattern": "^[A-Z]{2}\\d{2}$"},
            "nota": {"pattern": "^[^ó]*$"}}}"#,
    );
    assert_eq!(
        violaciones(&a, r#"{"código": "AB12", "nota": "bien"}"#),
        Some(0)
    );
    assert_eq!(
        violaciones(&a, r#"{"código": "Ab12", "nota": "acción"}"#),
        Some(2)
    );
    assert_eq!(violaciones(&a, r#"{"código": "ÁB12"}"#), Some(1));
}

fn xsd(cuerpo: &str) -> Automaton {
    let src =
        format!("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>{cuerpo}</xs:schema>");
    let ir = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = xsd::parse(src.as_bytes()).unwrap();
            xsd::lower(&xsd::analyze(&s).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    build(&ir).unwrap()
}

/// XSD: anclado; sobre `xs:string` el patrón ve el texto tal cual, y sobre
/// un tipo con *collapse* (`xs:int`), el valor normalizado.
#[test]
fn xsd_anclado_y_con_collapse() {
    let s = xsd(
        "<xs:element name='c'><xs:simpleType><xs:restriction base='xs:string'>\
         <xs:pattern value='[A-Z]{3}'/></xs:restriction></xs:simpleType></xs:element>",
    );
    assert_eq!(violaciones(&s, "<c>ABC</c>"), Some(0));
    assert_eq!(violaciones(&s, "<c>ABCD</c>"), Some(1));
    assert_eq!(violaciones(&s, "<c> ABC</c>"), Some(1));
    let n = xsd(
        "<xs:element name='n'><xs:simpleType><xs:restriction base='xs:int'>\
         <xs:pattern value='[0-9]{3}'/></xs:restriction></xs:simpleType></xs:element>",
    );
    assert_eq!(violaciones(&n, "<n> 123 </n>"), Some(0));
    assert_eq!(violaciones(&n, "<n>1234</n>"), Some(1));
    // Fuera del espacio léxico: 1 violación, sin evaluar el `pattern`.
    assert_eq!(violaciones(&n, "<n>abc</n>"), Some(1));
    // Dos pasos de restricción: las dos `pattern` deben cumplirse; las de
    // un mismo paso, una de ellas.
    let d = xsd("<xs:simpleType name='B'><xs:restriction base='xs:string'>\
         <xs:pattern value='a.*'/><xs:pattern value='b.*'/></xs:restriction></xs:simpleType>\
         <xs:element name='d'><xs:simpleType><xs:restriction base='B'>\
         <xs:pattern value='.*z'/></xs:restriction></xs:simpleType></xs:element>");
    assert_eq!(violaciones(&d, "<d>abz</d>"), Some(0));
    assert_eq!(violaciones(&d, "<d>bz</d>"), Some(0));
    assert_eq!(violaciones(&d, "<d>cz</d>"), Some(1));
    assert_eq!(violaciones(&d, "<d>ab</d>"), Some(1));
    assert_eq!(violaciones(&d, "<d>c</d>"), Some(2));
    // En un atributo.
    let at = xsd("<xs:element name='e'><xs:complexType>\
         <xs:attribute name='k'><xs:simpleType><xs:restriction base='xs:string'>\
         <xs:pattern value='\\d+'/></xs:restriction></xs:simpleType></xs:attribute>\
         </xs:complexType></xs:element>");
    assert_eq!(violaciones(&at, "<e k='42'/>"), Some(0));
    assert_eq!(violaciones(&at, "<e k='4x'/>"), Some(1));
}

/// La clase «otro» cuando el primer hueco de la partición son los
/// sustitutos: `^[^😀]$` rechaza `"😀"` (informe de F4.4b, 7.1).
#[test]
fn clase_otro_con_el_hueco_de_los_sustitutos() {
    assert!(!dfa_accepts("^[^😀]$", Dialect::Ecma, "😀"));
    assert!(dfa_accepts("^[^😀]$", Dialect::Ecma, "a"));
    let a = json(r#"{"pattern": "^[^😀]$"}"#);
    assert_eq!(violaciones(&a, "\"😀\""), Some(1));
    assert_eq!(violaciones(&a, "\"\u{E000}\""), Some(0));
}

/// El presupuesto cuenta las clases que añaden los `pattern` (7.2): muchas
/// clases sueltas en un patrón con el tope bajado dan `limit`, no un M que
/// pasa del presupuesto.
#[test]
fn presupuesto_con_las_clases_de_los_pattern() {
    let alfabeto: Vec<String> = ('a'..='z').map(|c| c.to_string()).collect();
    let src = format!(r#"{{"pattern": "^({})+$"}}"#, alfabeto.join("|"));
    let ir = json_ir(&src);
    let a = build(&ir).unwrap();
    assert!(a.n_classes() >= 14 + 2);
    let poco = automaton::BuildOptions {
        max_delta: 50,
        ..automaton::BuildOptions::default()
    };
    assert!(automaton::build_with(&ir, &poco).is_err());
}
