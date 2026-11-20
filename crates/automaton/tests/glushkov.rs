//! F4.1c: el autómata de Glushkov con contadores de los modelos de
//! contenido (`docs/fase4/automata.md` §12), `xs:all` (§13), la defensa en
//! profundidad ante una IR que viola la UPA, el presupuesto de transiciones
//! y D7 (10⁴ secuencias anidadas, 10⁵ niveles de payload).

use automaton::{build, build_with, validate, Automaton, BuildError, BuildOptions, Mode, Report};
use ir::{
    Constraint, ContentModel, Format, Ir, Node, NodeId, Occurs, Origin, Particle, Pos, Rule, Sort,
    Span, Step, Term, VerifiedIr,
};

#[path = "comun/presupuesto.rs"]
mod presupuesto;

fn xsd(src: &str) -> VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = xsd::parse(src.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
            let a = xsd::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"));
            xsd::lower(&a).unwrap_or_else(|e| panic!("{e}"))
        })
        .unwrap()
        .join()
        .unwrap()
}

/// El elemento global `r` con el modelo `modelo` (sobre elementos `a`, `b`,
/// `c` de contenido vacío).
fn modelo(modelo: &str) -> Automaton {
    let src = format!(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
         <xs:complexType name='V'/>\
         <xs:element name='r'><xs:complexType>{modelo}</xs:complexType></xs:element>\
         </xs:schema>"
    );
    compilar(&xsd(&src))
}

/// Compila y comprueba que el presupuesto de F4-U6 cubre lo emitido
/// (`comun/presupuesto.rs`, ronda 3).
fn compilar(ir: &VerifiedIr) -> Automaton {
    presupuesto::cubre(ir);
    build(ir).unwrap_or_else(|e| panic!("{e}"))
}

fn correr(a: &Automaton, payload: &str) -> Report {
    let r = validate(a, payload.as_bytes(), Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate(a, payload.as_bytes(), Mode::Total, chunk),
            r,
            "{payload}"
        );
    }
    r
}

/// Violaciones de `<r>` con los hijos `hijos` (`"aab"` = `<a/><a/><b/>`).
fn hijos(a: &Automaton, hijos: &str) -> Option<u64> {
    let mut p = String::from("<r>");
    for c in hijos.chars() {
        p.push_str(&format!("<{c}/>"));
    }
    p.push_str("</r>");
    correr(a, &p).violations.get()
}

fn el(name: &str, occurs: &str) -> String {
    format!("<xs:element name='{name}' type='V' {occurs}/>")
}

/// Criterio de F4.1c: `(a?){2,3}` acepta 0 a 3 `a` (el cuerpo es anulable:
/// salir sin el mínimo) y 4 dan 1 violación.
#[test]
fn opcional_con_contador() {
    let a = modelo(&format!(
        "<xs:sequence minOccurs='2' maxOccurs='3'>{}</xs:sequence>",
        el("a", "minOccurs='0'")
    ));
    for (h, v) in [
        ("", 0),
        ("a", 0),
        ("aa", 0),
        ("aaa", 0),
        ("aaaa", 1),
        ("aaaaa", 2),
    ] {
        assert_eq!(hijos(&a, h), Some(v), "{h:?}");
    }
    let r = correr(&a, "<r><a/><a/><a/><a/></r>");
    // En el `<` de la cuarta `a` (el `/>` que termina su nombre).
    assert_eq!(r.first.unwrap().offset, 15);
}

/// `(a{1,2}, b){2,2}` exige sus mínimos: dos vueltas, cada una con 1 o 2
/// `a` y una `b` (dos contadores en el mismo marco, con reinicio).
#[test]
fn dos_contadores_con_reinicio() {
    let a = modelo(&format!(
        "<xs:sequence minOccurs='2' maxOccurs='2'>{}{}</xs:sequence>",
        el("a", "maxOccurs='2'"),
        el("b", "")
    ));
    for (h, v) in [
        ("abab", 0),
        ("aabaab", 0),
        ("abaab", 0),
        ("ab", 1),   // falta la segunda vuelta
        ("aaab", 2), // la tercera `a`; y falta la segunda vuelta
        ("ababab", 1),
        ("aabaaab", 1), // la `a` de más en la segunda vuelta (el contador se reinició)
    ] {
        assert_eq!(hijos(&a, h), Some(v), "{h:?}");
    }
    // `ba`: `b` inesperada al principio (se salta) y el modelo incompleto.
    assert_eq!(hijos(&a, "b"), Some(2));
}

#[test]
fn elemento_con_minimo() {
    let a = modelo(&format!(
        "<xs:sequence>{}{}</xs:sequence>",
        el("a", "minOccurs='2' maxOccurs='unbounded'"),
        el("b", "minOccurs='0'")
    ));
    for (h, v) in [
        ("aa", 0),
        ("aaaaa", 0),
        ("aab", 0),
        ("a", 1),
        ("ab", 1),
        ("", 1),
    ] {
        assert_eq!(hijos(&a, h), Some(v), "{h:?}");
    }
}

#[test]
fn eleccion_y_repeticion() {
    let a = modelo(&format!(
        "<xs:choice maxOccurs='unbounded'>{}{}</xs:choice>",
        el("a", ""),
        el("b", "")
    ));
    for (h, v) in [("a", 0), ("abba", 0), ("", 1), ("c", 2), ("acb", 1)] {
        assert_eq!(hijos(&a, h), Some(v), "{h:?}");
    }
}

/// Criterio de F4.1c: `xs:all` acepta cualquier orden; el requerido que
/// falta y el repetido dan violación.
#[test]
fn all_en_cualquier_orden() {
    let a = modelo(&format!(
        "<xs:all>{}{}{}</xs:all>",
        el("a", ""),
        el("b", "minOccurs='0'"),
        el("c", "")
    ));
    for (h, v) in [
        ("abc", 0),
        ("cba", 0),
        ("ca", 0),
        ("ab", 1),   // falta c
        ("acca", 2), // c y a repetidos
        ("", 1),
        ("abcd", 1),
    ] {
        assert_eq!(hijos(&a, h), Some(v), "{h:?}");
    }
    let opcional = modelo(&format!(
        "<xs:all minOccurs='0'>{}{}</xs:all>",
        el("a", ""),
        el("b", "")
    ));
    assert_eq!(hijos(&opcional, ""), Some(0));
    assert_eq!(hijos(&opcional, "b"), Some(1));
    assert_eq!(hijos(&opcional, "ba"), Some(0));
}

/// Un elemento recursivo: el marco del hijo es el del padre.
#[test]
fn recursion_por_tipo() {
    let src = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
        <xs:complexType name='T'><xs:sequence>\
        <xs:element name='n' type='T' minOccurs='0' maxOccurs='2'/>\
        </xs:sequence><xs:attribute name='v' type='xs:int'/></xs:complexType>\
        <xs:element name='n' type='T'/></xs:schema>";
    let a = compilar(&xsd(src));
    let r = correr(&a, "<n v='1'><n><n v='x'/><n/></n><n/></n>");
    assert_eq!(r.violations.get(), Some(1));
    assert_eq!(correr(&a, "<n><n/><n/><n/></n>").violations.get(), Some(1));
}

// ------------------------------------------------- IR hecha a mano ---

const SPAN: Span = Span {
    start: Pos::START,
    end: 0,
};

fn nodo(sort: Sort, rules: Vec<Rule>, parent: Option<usize>) -> Node {
    Node {
        sort,
        rules: rules
            .into_iter()
            .map(|rule| Constraint { rule, span: SPAN })
            .collect(),
        origin: Origin {
            span: SPAN,
            parent: parent.map(NodeId::new),
            step: if parent.is_some() {
                Step::Keyword("x".to_owned())
            } else {
                Step::Root
            },
        },
    }
}

fn particula(term: Term, occurs: Occurs) -> Particle {
    Particle {
        term,
        occurs,
        span: SPAN,
    }
}

/// Un documento con el elemento `r` (nodo 1) de modelo `particles` sobre
/// elementos de tipo vacío (nodo 2).
fn ir_a_mano(particles: Vec<Particle>) -> VerifiedIr {
    let doc = ContentModel {
        particles: vec![
            particula(Term::Choice(vec![1]), Occurs::ONCE),
            particula(
                Term::Element {
                    name: "r".to_owned(),
                    ty: NodeId::new(1),
                },
                Occurs::ONCE,
            ),
        ],
    };
    let mut ir = Ir::new(
        Format::Xml {
            target_namespace: None,
        },
        nodo(Sort::Element, vec![Rule::Content(doc)], None),
    );
    ir.push(nodo(
        Sort::Element,
        vec![Rule::Content(ContentModel { particles })],
        Some(0),
    ));
    ir.push(nodo(Sort::Element, vec![], Some(0)));
    ir.verify().map_err(|(_, e)| e).unwrap()
}

fn hoja(name: &str, occurs: Occurs) -> Particle {
    particula(
        Term::Element {
            name: name.to_owned(),
            ty: NodeId::new(2),
        },
        occurs,
    )
}

/// Criterio de F4.1c: una IR que viola la UPA (`a?, a`) da `Internal`, sin
/// pánico; y una vuelta ambigua con contador (`(a{2,2})+`), también.
#[test]
fn ir_que_viola_la_upa_da_internal() {
    let opt = Occurs {
        min: 0,
        max: Some(1),
    };
    let ir = ir_a_mano(vec![
        particula(Term::Sequence(vec![1, 2]), Occurs::ONCE),
        hoja("a", opt),
        hoja("a", Occurs::ONCE),
    ]);
    assert!(
        matches!(build(&ir), Err(BuildError::Internal(_))),
        "{:?}",
        build(&ir).err()
    );
    let ir = ir_a_mano(vec![
        particula(Term::Sequence(vec![1]), Occurs { min: 1, max: None }),
        hoja(
            "a",
            Occurs {
                min: 2,
                max: Some(2),
            },
        ),
    ]);
    assert!(
        matches!(build(&ir), Err(BuildError::Internal(_))),
        "{:?}",
        build(&ir).err()
    );
}

/// Criterio de F4.1c (F4.1b-CR2-S3): con el presupuesto bajado,
/// `(e₁|…|e₄₀₉₅)*` da `limit` (la cuenta de F4-U6: entradas de δ más
/// comprobaciones) antes de construir sus 4 096 tries; con el presupuesto
/// por defecto también, porque su Glushkov es cuadrático.
#[test]
fn eleccion_ancha_da_limit() {
    let hojas: String = (1..=4095).map(|i| el(&format!("e{i}"), "")).collect();
    let src = format!(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
         <xs:complexType name='V'/>\
         <xs:element name='r'><xs:complexType><xs:choice minOccurs='0' maxOccurs='unbounded'>{hojas}</xs:choice></xs:complexType></xs:element>\
         </xs:schema>"
    );
    let ir = xsd(&src);
    let bajo = BuildOptions {
        max_delta: 1 << 16,
        ..BuildOptions::default()
    };
    for opts in [bajo, BuildOptions::default()] {
        match build_with(&ir, &opts) {
            Err(BuildError::Limit { limit, found, .. }) => {
                assert_eq!(limit, opts.max_delta);
                assert!(found > limit);
            }
            otro => panic!("{:?}", otro.map(|a| a.summary())),
        }
    }
    // Con 40 nombres compila y valida.
    let hojas: String = (1..=40).map(|i| el(&format!("e{i}"), "")).collect();
    let a = modelo(&format!(
        "<xs:choice minOccurs='0' maxOccurs='unbounded'>{hojas}</xs:choice>"
    ));
    assert_eq!(
        correr(&a, "<r><e40/><e1/><e7/></r>").violations.get(),
        Some(0)
    );
}

/// Criterio de F4.1c (D7): un modelo de 10⁴ secuencias anidadas (IR hecha a
/// mano: el frontal limita el anidamiento a 256 y las partículas a 4 096) se
/// construye en un hilo de 256 KiB; un payload de 10⁵ niveles se recorre.
#[test]
fn d7_secuencias_anidadas_y_payload_profundo() {
    let n = 10_000;
    let mut ps: Vec<Particle> = (0..n)
        .map(|i| particula(Term::Sequence(vec![i + 1]), Occurs::ONCE))
        .collect();
    ps.push(hoja(
        "a",
        Occurs {
            min: 0,
            max: Some(1),
        },
    ));
    let ir = ir_a_mano(ps);
    let a = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || build(&ir).map(|a| a.summary()))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
    assert!(a.states > 0);

    let src = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
        <xs:complexType name='T'><xs:sequence>\
        <xs:element name='a' type='T' minOccurs='0'/>\
        </xs:sequence></xs:complexType>\
        <xs:element name='a' type='T'/></xs:schema>";
    let a = compilar(&xsd(src));
    let d = 100_000;
    let payload = format!("{}{}", "<a>".repeat(d), "</a>".repeat(d));
    let r = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || validate(&a, payload.as_bytes(), Mode::Total, 4096))
        .unwrap()
        .join()
        .unwrap();
    assert!(r.valid, "{r:?}");
}
