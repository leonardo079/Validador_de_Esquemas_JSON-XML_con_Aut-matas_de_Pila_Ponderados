//! DTD.3: análisis semántico (`crates/dtd/src/check.rs`).
//!
//! - cada regla de la Fase 3 con su clase y su byte;
//! - el determinismo (Glushkov, XML 1.0 apéndice E) en casos escritos a mano
//!   y en una **diferencial** contra la *Unique Particle Attribution* del
//!   frontal XSD (`xsd::analyze`) sobre los mismos modelos escritos en XSD:
//!   sin contadores, las dos reglas son la misma.

use dtd::{analyze, parse, ErrorKind, MAX_PARTICLES};
use proptest::prelude::*;

/// Errores semánticos (clase y byte), o vacío si la DTD es válida.
fn errores(s: &str) -> Vec<(ErrorKind, u64)> {
    let d = parse(s.as_bytes()).unwrap_or_else(|e| panic!("{s:?}: {e}"));
    match analyze(&d) {
        Ok(_) => Vec::new(),
        Err(es) => es.into_iter().map(|e| (e.kind, e.at.offset)).collect(),
    }
}

fn valida(s: &str) {
    assert_eq!(errores(s), Vec::new(), "{s:?}");
}

const HIJOS: &str = "<!ELEMENT a EMPTY><!ELEMENT b EMPTY><!ELEMENT c EMPTY><!ELEMENT d EMPTY>";

/// La DTD con `r` de modelo `m` y los hijos `a`-`d` vacíos.
fn modelo(m: &str) -> String {
    format!("<!ELEMENT r {m}>{HIJOS}")
}

#[test]
fn una_dtd_sin_elementos_no_sirve() {
    assert_eq!(errores(""), vec![(ErrorKind::NoElements, 0)]);
    assert_eq!(
        errores("<?xml version='1.0'?><!-- nada -->"),
        vec![(ErrorKind::NoElements, 0)]
    );
}

#[test]
fn nombres_repetidos() {
    use ErrorKind::Duplicate;
    assert_eq!(
        errores("<!ELEMENT a EMPTY><!ELEMENT a (#PCDATA)>"),
        vec![(Duplicate, 28)]
    );
    // El mismo atributo en dos <!ATTLIST> del mismo elemento.
    assert_eq!(
        errores("<!ELEMENT a EMPTY><!ATTLIST a x CDATA #IMPLIED><!ATTLIST a x CDATA #REQUIRED>"),
        vec![(Duplicate, 59)]
    );
    assert_eq!(
        errores("<!ELEMENT a EMPTY><!ATTLIST a x (u|v|u) #IMPLIED>"),
        vec![(Duplicate, 37)]
    );
    // Atributos homónimos de elementos distintos sí valen.
    valida("<!ELEMENT a EMPTY><!ELEMENT b EMPTY><!ATTLIST a x CDATA #IMPLIED><!ATTLIST b x CDATA #IMPLIED>");
    let e = analyze(&parse(b"<!ELEMENT a EMPTY><!ELEMENT a EMPTY>").unwrap()).unwrap_err();
    assert!(e[0].message.contains("1:11"), "{}", e[0]);
}

#[test]
fn nombres_sin_declarar() {
    use ErrorKind::Unresolved;
    assert_eq!(
        errores("<!ELEMENT a (b, c)><!ELEMENT b EMPTY>"),
        vec![(Unresolved, 16)]
    );
    assert_eq!(
        errores("<!ELEMENT a EMPTY><!ATTLIST z x CDATA #IMPLIED>"),
        vec![(Unresolved, 28)]
    );
}

#[test]
fn valores_por_defecto_en_su_enumeracion() {
    use ErrorKind::InvalidValue;
    assert_eq!(
        errores("<!ELEMENT a EMPTY><!ATTLIST a x (u|v) 'w'>"),
        vec![(InvalidValue, 38)]
    );
    assert_eq!(
        errores("<!ELEMENT a EMPTY><!ATTLIST a x (u|v) #FIXED 'w'>"),
        vec![(InvalidValue, 45)]
    );
    // Sin normalizar espacios (§3bis.4, desviación 3).
    assert_eq!(
        errores("<!ELEMENT a EMPTY><!ATTLIST a x (u|v) ' u'>"),
        vec![(InvalidValue, 38)]
    );
    valida("<!ELEMENT a EMPTY><!ATTLIST a x (u|v) 'v' y CDATA #FIXED 'cualquiera'>");
}

#[test]
fn se_informan_todos_los_errores_en_orden() {
    let es = errores("<!ELEMENT a (z)><!ELEMENT a EMPTY><!ATTLIST q x CDATA #IMPLIED>");
    assert_eq!(
        es,
        vec![
            (ErrorKind::Unresolved, 13),
            (ErrorKind::Duplicate, 26),
            (ErrorKind::Unresolved, 44)
        ]
    );
}

#[test]
fn modelos_deterministas() {
    for m in [
        "(a)",
        "(a, b)",
        "(a | b)",
        "(a?, b)",
        "(a | b)*",
        "(a, b)+",
        "((a, b) | (c, d))",
        "(a*, b)",
        "(a, (b | c)*, d?)",
        "((a | b), c)*",
        "(a?, b?, c?)",
        "((a, b)*, c)",
        "(a, a)",
        "(a+)",
        "((a)*)",
    ] {
        valida(&modelo(m));
    }
}

#[test]
fn modelos_no_deterministas_en_la_segunda_particula() {
    use ErrorKind::NonDeterministic;
    // (modelo, byte de la segunda partícula en conflicto dentro del modelo)
    for (m, k) in [
        ("(a | a)", 5),
        ("(a?, a)", 5),
        ("(a*, a)", 5),
        ("(a, b?, b)", 8),
        ("((a, b) | (a, c))", 11),
        ("((a, b)*, a?)", 10),
        ("((a | b)*, b)", 11),
        ("(a+, a)", 5),
        ("((a?, b?), b)", 11),
    ] {
        let s = modelo(m);
        assert_eq!(errores(&s), vec![(NonDeterministic, 12 + k)], "{m}");
    }
    let e = analyze(&parse(modelo("(a | a)").as_bytes()).unwrap()).unwrap_err();
    assert!(
        e[0].message.contains("«a» en 1:14 y «a» en 1:18"),
        "{}",
        e[0]
    );
}

#[test]
fn tope_de_particulas() {
    // Raíz + 4 095 nombres = 4 096 partículas: vale.
    let lista = |n: usize| vec!["a"; n].join(",");
    valida(&modelo(&format!("({})", lista(MAX_PARTICLES - 1))));
    // Una más es `Limit` en la partícula 4 097 (el nombre número 4 096).
    let s = modelo(&format!("({})", lista(MAX_PARTICLES)));
    let byte = 12 + 1 + 2 * (MAX_PARTICLES as u64 - 1);
    assert_eq!(errores(&s), vec![(ErrorKind::Limit, byte)]);
}

// ------------------------------------------------- diferencial con XSD ---

#[derive(Clone, Debug)]
enum M {
    N(char, &'static str),
    Seq(Vec<M>, &'static str),
    Alt(Vec<M>, &'static str),
}

fn ocurrencia() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec!["", "", "?", "*", "+"])
}

fn modelos() -> impl Strategy<Value = M> {
    let hoja =
        (prop::sample::select(vec!['a', 'b', 'c']), ocurrencia()).prop_map(|(c, o)| M::N(c, o));
    let interior = hoja.prop_recursive(3, 16, 3, |m| {
        prop_oneof![
            (prop::collection::vec(m.clone(), 1..4), ocurrencia())
                .prop_map(|(xs, o)| M::Seq(xs, o)),
            (prop::collection::vec(m, 2..4), ocurrencia()).prop_map(|(xs, o)| M::Alt(xs, o)),
        ]
    });
    // La raíz es siempre un grupo.
    (
        prop::collection::vec(interior, 1..4),
        prop::bool::ANY,
        ocurrencia(),
    )
        .prop_map(|(xs, alt, o)| {
            if alt && xs.len() > 1 {
                M::Alt(xs, o)
            } else {
                M::Seq(xs, o)
            }
        })
}

fn a_dtd(m: &M) -> String {
    match m {
        M::N(c, o) => format!("{c}{o}"),
        M::Seq(xs, o) | M::Alt(xs, o) => {
            let sep = if matches!(m, M::Seq(..)) { ", " } else { " | " };
            format!(
                "({}){o}",
                xs.iter().map(a_dtd).collect::<Vec<_>>().join(sep)
            )
        }
    }
}

fn veces(o: &str) -> &'static str {
    match o {
        "?" => r#" minOccurs="0""#,
        "*" => r#" minOccurs="0" maxOccurs="unbounded""#,
        "+" => r#" maxOccurs="unbounded""#,
        _ => "",
    }
}

fn a_xsd(m: &M) -> String {
    match m {
        M::N(c, o) => format!(r#"<xs:element ref="{c}"{}/>"#, veces(o)),
        M::Seq(xs, o) | M::Alt(xs, o) => {
            let g = if matches!(m, M::Seq(..)) {
                "sequence"
            } else {
                "choice"
            };
            format!(
                "<xs:{g}{}>{}</xs:{g}>",
                veces(o),
                xs.iter().map(a_xsd).collect::<String>()
            )
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(3000))]

    /// Sin contadores, el determinismo de XML 1.0 (apéndice E) y la UPA de
    /// XSD son la misma regla: los dos frontales tienen que coincidir.
    #[test]
    fn el_determinismo_coincide_con_la_upa_de_xsd(m in modelos()) {
        let dtd = format!("<!ELEMENT r {}><!ELEMENT a EMPTY><!ELEMENT b EMPTY><!ELEMENT c EMPTY>", a_dtd(&m));
        let d = parse(dtd.as_bytes()).unwrap();
        let no_det_dtd = matches!(analyze(&d), Err(es) if es.iter().any(|e| e.kind == ErrorKind::NonDeterministic));

        let xsd = format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
                 <xs:element name="r"><xs:complexType>{}</xs:complexType></xs:element>
                 <xs:element name="a"><xs:complexType/></xs:element>
                 <xs:element name="b"><xs:complexType/></xs:element>
                 <xs:element name="c"><xs:complexType/></xs:element>
               </xs:schema>"#,
            a_xsd(&m)
        );
        let s = xsd::parse(xsd.as_bytes()).unwrap();
        let no_det_xsd = matches!(xsd::analyze(&s), Err(es) if es.iter().any(|e| e.code() == "content-model"));
        prop_assert_eq!(no_det_dtd, no_det_xsd, "DTD {} / XSD {}", dtd, xsd);
    }
}
