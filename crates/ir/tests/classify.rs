//! Clasificación (a)/(b)/(c) y estrategia (tarea 4.3,
//! `docs/fase3/clasificacion.md`): la tabla única del código coincide con
//! las columnas «Cat.» de `docs/00-alcance.md` §2 y §3 (con un `match`
//! exhaustivo sobre `RuleKind`) y con la tabla escrita en
//! `clasificacion.md`; un modelo de contenido sube a (b) solo con `xs:all` o
//! con una partícula que necesita contador; la clase de un nodo es la máxima
//! de sus reglas y de los nodos sobre la misma instancia, sin pasar por las
//! aristas que consumen entrada; y el cálculo no es recursivo (D7) ni
//! expande los caminos (D16).

use std::path::Path;

use ir::{
    classify, write_classification_table, Class, Constraint, ContentModel, Format, Ir, NodeId,
    Occurs, Origin, Particle, Pos, Rule, RuleKind, Sort, Span, Step, Strategy, StrategySet, Term,
    VerifiedIr,
};

const RAIZ: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

// --- Ayudas ----------------------------------------------------------------

fn sp() -> Span {
    Span {
        start: Pos::START,
        end: 0,
    }
}

fn id(i: usize) -> NodeId {
    NodeId::new(i)
}

fn node(sort: Sort, parent: Option<usize>, step: Step, rules: Vec<Rule>) -> ir::Node {
    ir::Node {
        sort,
        rules: rules
            .into_iter()
            .map(|rule| Constraint { rule, span: sp() })
            .collect(),
        origin: Origin {
            span: sp(),
            parent: parent.map(NodeId::new),
            step,
        },
    }
}

/// IR JSON: la raíz con `rules` y un hijo de la raíz por cada elemento de
/// `hijos` (nodos 1, 2…).
fn json(rules: Vec<Rule>, hijos: Vec<Vec<Rule>>) -> VerifiedIr {
    let mut ir = Ir::new(Format::Json, node(Sort::Json, None, Step::Root, rules));
    for (i, r) in hijos.into_iter().enumerate() {
        ir.push(node(
            Sort::Json,
            Some(0),
            Step::Entry("$defs".into(), (i + 1).to_string()),
            r,
        ));
    }
    ir.verify().unwrap_or_else(|(_, e)| panic!("{}", e[0]))
}

fn conjunto(v: &[Strategy]) -> StrategySet {
    v.iter().fold(StrategySet::EMPTY, |s, &e| s.with(e))
}

/// (clase, estrategias) de cada nodo.
fn clases(ir: &VerifiedIr) -> Vec<(Class, StrategySet)> {
    let c = classify(ir);
    (0..ir.nodes().len())
        .map(|i| {
            let n = c.node(id(i));
            (n.class, n.strategies)
        })
        .collect()
}

// --- La tabla contra 00-alcance.md -----------------------------------------

/// Filas de las tablas de `docs/00-alcance.md` §2 y §3: primera celda →
/// última celda (la columna «Cat.»).
fn filas_del_alcance() -> Vec<(String, String)> {
    let texto = std::fs::read_to_string(Path::new(RAIZ).join("docs/00-alcance.md")).unwrap();
    texto
        .lines()
        .filter(|l| l.starts_with('|') && !l.starts_with("|---") && !l.starts_with("| ---"))
        .filter_map(|l| {
            let celdas: Vec<&str> = l.trim_matches('|').split(" | ").map(str::trim).collect();
            (celdas.len() >= 3).then(|| {
                (
                    celdas[0].to_owned(),
                    celdas[celdas.len() - 1]
                        .trim_end_matches('|')
                        .trim()
                        .to_owned(),
                )
            })
        })
        .collect()
}

/// La `n`-ésima clase `(a)`/`(b)`/`(c)` que aparece en la celda «Cat.» de la
/// fila `fila` del alcance.
#[track_caller]
fn clase_del_alcance(filas: &[(String, String)], fila: &str, n: usize) -> Class {
    let (_, cat) = filas
        .iter()
        .find(|(f, _)| f == fila)
        .unwrap_or_else(|| panic!("00-alcance.md no tiene la fila «{fila}»"));
    let clases: Vec<Class> = cat
        .match_indices('(')
        .filter_map(|(i, _)| match cat.get(i..i + 3) {
            Some("(a)") => Some(Class::A),
            Some("(b)") => Some(Class::B),
            Some("(c)") => Some(Class::C),
            _ => None,
        })
        .collect();
    *clases
        .get(n)
        .unwrap_or_else(|| panic!("«{fila}»: la celda «{cat}» no tiene la clase {n}"))
}

/// (fila de `00-alcance.md`, índice de la clase dentro de su celda «Cat.»).
type Filas = &'static [(&'static str, usize)];

/// Filas del alcance de las que sale cada `RuleKind`. La primera lista da la
/// clase de la tabla; la segunda, la clase a la que sube la regla (solo
/// `Content`). El `match` es exhaustivo: una `RuleKind` nueva no compila sin
/// su fila.
fn filas_de(kind: RuleKind) -> (Filas, Filas) {
    use RuleKind as K;
    // `xs:simpleType` + `xs:restriction`: «(a); facetas de longitud (b)».
    const RESTRICCION: &str = "`xs:simpleType` + `xs:restriction`";
    match kind {
        K::Reject => (&[("Esquemas booleanos `true` / `false`", 0)], &[]),
        K::Ref => (&[("`$ref`", 0), (RESTRICCION, 0)], &[]),
        K::Type => (&[("`type`", 0)], &[]),
        K::Enum => (&[("`enum`", 0), (RESTRICCION, 0)], &[]),
        K::Const => (&[("`const`", 0)], &[]),
        K::Minimum | K::Maximum => (&[("`minimum`, `maximum`", 0), (RESTRICCION, 0)], &[]),
        K::ExclusiveMinimum | K::ExclusiveMaximum => (
            &[
                ("`exclusiveMinimum`, `exclusiveMaximum`", 0),
                (RESTRICCION, 0),
            ],
            &[],
        ),
        K::MultipleOf => (&[("`multipleOf`", 0)], &[]),
        K::MinLength | K::MaxLength => (&[("`minLength`, `maxLength`", 0), (RESTRICCION, 1)], &[]),
        K::Pattern => (&[("`pattern`", 0), ("Faceta `pattern`", 0)], &[]),
        K::Items => (&[("`items`", 0), ("`prefixItems`", 0)], &[]),
        K::MinItems | K::MaxItems => (&[("`minItems`, `maxItems`", 0)], &[]),
        K::UniqueItems => (&[("`uniqueItems`", 0)], &[]),
        K::Properties => (&[("`properties`", 0), ("`additionalProperties`", 0)], &[]),
        K::Required => (&[("`required`", 0)], &[]),
        K::MinProperties | K::MaxProperties => (&[("`minProperties`, `maxProperties`", 0)], &[]),
        K::DependentRequired => (&[("`dependentRequired`", 0)], &[]),
        K::DependentSchemas => (&[("`dependentSchemas`", 0)], &[]),
        K::AllOf => (&[("`allOf`", 0)], &[]),
        K::AnyOf => (&[("`anyOf`", 0)], &[]),
        K::OneOf => (&[("`oneOf`", 0)], &[]),
        K::Not => (&[("`not`", 0)], &[]),
        K::Attributes => (&[("`xs:attribute`", 0)], &[]),
        K::Content => (
            &[
                ("`xs:element`", 0),
                ("`xs:complexType`", 0),
                ("`xs:sequence`", 0),
                ("`xs:choice`", 0),
            ],
            &[("`xs:all`", 0), ("`minOccurs`, `maxOccurs`", 0)],
        ),
        K::SimpleContent => (&[("`xs:simpleContent` + `xs:extension`", 0)], &[]),
        K::Lexical => (&[(RESTRICCION, 0)], &[]),
    }
}

/// Criterio 1: la clase de cada una de las 31 `RuleKind` es la de su fila
/// (o filas) en las columnas «Cat.» de `00-alcance.md` §2 y §3, incluidas
/// las de XSD; y solo `Content` sube de clase, a la de `xs:all` y
/// `minOccurs`/`maxOccurs`.
#[test]
fn tabla_coincide_con_el_alcance() {
    let filas = filas_del_alcance();
    for kind in RuleKind::ALL {
        let (base, sube) = filas_de(kind);
        let (asignada, subida) = kind.classification();
        for &(fila, n) in base {
            assert_eq!(
                asignada.class,
                clase_del_alcance(&filas, fila, n),
                "{kind:?} frente a «{fila}»"
            );
        }
        assert_eq!(subida.is_some(), !sube.is_empty(), "{kind:?}");
        for &(fila, n) in sube {
            assert_eq!(
                subida.unwrap().class,
                clase_del_alcance(&filas, fila, n),
                "{kind:?} (subida) frente a «{fila}»"
            );
        }
    }
}

/// Las estrategias de la tabla son las que dice el texto del alcance (§2 y
/// §3) y D5: (a) siempre transición y (b) registro; en (c), AFD para
/// `pattern`, verificación diferida para `uniqueItems` y `dependentSchemas`,
/// máscara de bits (registro) para `dependentRequired` (D5), producto acotado
/// para `allOf` y `not` (`ir.md` §6.1) y determinización acotada para
/// `anyOf`/`oneOf`.
#[test]
fn estrategias_de_la_tabla() {
    use RuleKind as K;
    for kind in RuleKind::ALL {
        let (a, subida) = kind.classification();
        let esperada = match kind {
            K::Pattern => Strategy::Dfa,
            K::UniqueItems | K::DependentSchemas => Strategy::Deferred,
            K::DependentRequired => Strategy::Register,
            K::AllOf | K::Not => Strategy::BoundedProduct,
            K::AnyOf | K::OneOf => Strategy::BoundedDeterminization,
            _ if a.class == Class::A => Strategy::Transition,
            _ => Strategy::Register,
        };
        assert_eq!(a.strategy, esperada, "{kind:?}");
        if let Some(s) = subida {
            assert_eq!((s.class, s.strategy), (Class::B, Strategy::Register));
        }
    }
}

/// Criterio 1: la tabla escrita en `docs/fase3/clasificacion.md`, entre sus
/// marcas, es exactamente la que genera el código.
#[test]
fn tabla_escrita_es_la_del_codigo() {
    let mut generada = String::new();
    write_classification_table(&mut generada).unwrap();
    let doc = std::fs::read_to_string(Path::new(RAIZ).join("docs/fase3/clasificacion.md")).unwrap();
    const INICIO: &str = "<!-- tabla-clasificacion:inicio (generada; no editar) -->\n";
    const FIN: &str = "<!-- tabla-clasificacion:fin -->";
    let i = doc.find(INICIO).expect("falta la marca de inicio") + INICIO.len();
    let f = doc[i..].find(FIN).expect("falta la marca de fin") + i;
    assert_eq!(
        &doc[i..f],
        generada,
        "la tabla de clasificacion.md no es la del código; la generada es:\n{generada}"
    );
}

// --- Modelo de contenido -----------------------------------------------------

/// IR de XSD: el documento admite el elemento `r` (nodo 1), cuyo contenido
/// es `modelo` sobre elementos de contenido vacío (nodo 2).
fn xsd(modelo: Vec<Particle>) -> VerifiedIr {
    let el = |name: &str, ty: usize, occurs: Occurs| Particle {
        term: Term::Element {
            name: name.to_owned(),
            ty: id(ty),
        },
        occurs,
        span: sp(),
    };
    let doc = node(
        Sort::Element,
        None,
        Step::Root,
        vec![Rule::Content(ContentModel {
            particles: vec![
                Particle {
                    term: Term::Choice(vec![1]),
                    occurs: Occurs::ONCE,
                    span: sp(),
                },
                el("r", 1, Occurs::ONCE),
            ],
        })],
    );
    let mut ir = Ir::new(
        Format::Xml {
            target_namespace: None,
        },
        doc,
    );
    let mut particles = modelo;
    for p in &mut particles {
        if let Term::Element { ty, .. } = &mut p.term {
            *ty = id(2);
        }
    }
    ir.push(node(
        Sort::Element,
        Some(0),
        Step::Entry("element".into(), "r".into()),
        vec![Rule::Content(ContentModel { particles })],
    ));
    ir.push(node(
        Sort::Element,
        Some(1),
        Step::Entry("element".into(), "h".into()),
        vec![],
    ));
    ir.verify().unwrap_or_else(|(_, e)| panic!("{}", e[0]))
}

fn p(term: Term, min: u64, max: Option<u64>) -> Particle {
    Particle {
        term,
        occurs: Occurs { min, max },
        span: sp(),
    }
}

fn e(name: &str) -> Term {
    Term::Element {
        name: name.to_owned(),
        ty: id(2),
    }
}

/// Un `Content` es (a) con solo `sequence`/`choice` y ocurrencias que el
/// autómata de Glushkov expresa con estados (`?`, `*`, `+`, una vez,
/// `maxOccurs="0"`), y (b) con `xs:all` o con una partícula que necesita
/// contador (`minOccurs` ≥ 2 o `maxOccurs` acotado > 1; D16, `ir.md` §7).
#[test]
fn contenido_sube_a_b_solo_con_all_o_contador() {
    let a = xsd(vec![
        p(Term::Sequence(vec![1, 2, 3, 4, 5]), 1, Some(1)),
        p(e("a"), 0, Some(1)),
        p(e("b"), 0, None),
        p(e("c"), 1, None),
        p(e("d"), 0, Some(0)),
        p(e("f"), 1, Some(1)),
    ]);
    let b_all = xsd(vec![
        p(Term::All(vec![1, 2]), 1, Some(1)),
        p(e("a"), 0, Some(1)),
        p(e("b"), 1, Some(1)),
    ]);
    let b_max = xsd(vec![
        p(Term::Sequence(vec![1]), 1, Some(1)),
        p(e("a"), 0, Some(5)),
    ]);
    let b_min = xsd(vec![
        p(Term::Sequence(vec![1]), 1, Some(1)),
        p(e("a"), 2, None),
    ]);
    // Bordes: `maxOccurs="2"` ya necesita contador (`max > 1`), y
    // `minOccurs="1" maxOccurs="1"` no.
    let b_max_2 = xsd(vec![
        p(Term::Sequence(vec![1]), 1, Some(1)),
        p(e("a"), 0, Some(2)),
    ]);
    let b_exacto_2 = xsd(vec![
        p(Term::Sequence(vec![1]), 1, Some(1)),
        p(e("a"), 2, Some(2)),
    ]);
    let a_uno = xsd(vec![
        p(Term::Sequence(vec![1]), 1, Some(1)),
        p(e("a"), 1, Some(1)),
    ]);
    let b_grupo = xsd(vec![
        p(Term::Choice(vec![1, 2]), 0, Some(3)),
        p(e("a"), 1, Some(1)),
        p(e("b"), 1, Some(1)),
    ]);
    let r = |ir: &VerifiedIr| ir.node(id(1)).rules[0].rule.assignment();
    for ir in [&a, &a_uno] {
        assert_eq!(
            (r(ir).class, r(ir).strategy),
            (Class::A, Strategy::Transition)
        );
    }
    for (ir, que) in [
        (&b_max_2, "minOccurs=0 maxOccurs=2"),
        (&b_exacto_2, "minOccurs=2 maxOccurs=2"),
        (&b_all, "all"),
        (&b_max, "maxOccurs=5"),
        (&b_min, "minOccurs=2"),
        (&b_grupo, "grupo con maxOccurs=3"),
    ] {
        assert_eq!(
            (r(ir).class, r(ir).strategy),
            (Class::B, Strategy::Register),
            "{que}"
        );
        // El modelo consume entrada: no sube la clase del documento.
        assert_eq!(clases(ir)[0].0, Class::A, "{que}");
        assert_eq!(clases(ir)[1].0, Class::B, "{que}");
    }
}

// --- La clase de un nodo ----------------------------------------------------

/// Sin reglas (`true`), un nodo es (a) sin estrategias; con varias, la
/// máxima de sus reglas y la unión de sus estrategias.
#[test]
fn nodo_toma_la_maxima_de_sus_reglas() {
    let ir = json(
        vec![
            Rule::Type(ir::TypeSet::of(&[ir::JsonType::String])),
            Rule::MinLength(1),
            Rule::Pattern("^a".into()),
        ],
        vec![vec![], vec![Rule::Reject]],
    );
    use Strategy as S;
    assert_eq!(
        clases(&ir),
        [
            (Class::C, conjunto(&[S::Transition, S::Register, S::Dfa])),
            (Class::A, StrategySet::EMPTY),
            (Class::A, conjunto(&[S::Transition])),
        ]
    );
}

/// La clase sube por las aristas de E₌ (`$ref`, combinadores, `not`,
/// `dependentSchemas`) y no por las que consumen entrada (`properties`,
/// `items`): el hijo de una propiedad es otra instancia.
#[test]
fn sube_por_misma_instancia_y_no_por_aristas_que_consumen() {
    use Strategy as S;
    let patron = vec![Rule::Pattern("x".into())];
    let por_ref = json(vec![Rule::Ref(id(1))], vec![patron.clone()]);
    // `$ref` es (a) y aporta su transición (llamada ε o expansión).
    assert_eq!(
        clases(&por_ref)[0],
        (Class::C, conjunto(&[S::Transition, S::Dfa]))
    );
    let por_propiedad = json(
        vec![Rule::Properties {
            declared: vec![("a".into(), id(1))],
            additional: None,
        }],
        vec![patron.clone()],
    );
    assert_eq!(
        clases(&por_propiedad),
        [
            (Class::A, conjunto(&[S::Transition])),
            (Class::C, conjunto(&[S::Dfa]))
        ]
    );
    let por_items = json(
        vec![Rule::Items {
            prefix: vec![],
            rest: Some(id(1)),
        }],
        vec![patron.clone()],
    );
    assert_eq!(clases(&por_items)[0].0, Class::A);
    let por_dependent = json(
        vec![Rule::DependentSchemas(vec![("a".into(), id(1))])],
        vec![vec![Rule::MinProperties(2)]],
    );
    assert_eq!(
        clases(&por_dependent)[0],
        (Class::C, conjunto(&[S::Register, S::Deferred]))
    );
    // (b) también sube: `$ref` a un nodo con `required`.
    let b = json(
        vec![Rule::Ref(id(1))],
        vec![vec![Rule::Required(vec!["a".into()])]],
    );
    assert_eq!(
        clases(&b)[0],
        (Class::B, conjunto(&[S::Transition, S::Register]))
    );
}

/// Criterio 2: `not` y `anyOf` sobre (a) y sobre (c). El nodo del
/// combinador es (c) siempre (la regla es (c)); sobre (c) suma además las
/// estrategias de su destino (`not` sigue la del subesquema e invierte,
/// `ir.md` §6.1). Los destinos conservan su clase.
#[test]
fn not_y_any_of_sobre_a_y_sobre_c() {
    use Strategy as S;
    let tipo = || Rule::Type(ir::TypeSet::of(&[ir::JsonType::String]));
    let not_a = json(vec![Rule::Not(id(1))], vec![vec![tipo()]]);
    assert_eq!(
        clases(&not_a),
        [
            (Class::C, conjunto(&[S::Transition, S::BoundedProduct])),
            (Class::A, conjunto(&[S::Transition])),
        ]
    );
    let not_c = json(vec![Rule::Not(id(1))], vec![vec![Rule::UniqueItems]]);
    assert_eq!(
        clases(&not_c),
        [
            (Class::C, conjunto(&[S::BoundedProduct, S::Deferred])),
            (Class::C, conjunto(&[S::Deferred])),
        ]
    );
    let any_a = json(
        vec![Rule::AnyOf(vec![id(1), id(2)])],
        vec![vec![tipo()], vec![]],
    );
    assert_eq!(
        clases(&any_a),
        [
            (
                Class::C,
                conjunto(&[S::Transition, S::BoundedDeterminization])
            ),
            (Class::A, conjunto(&[S::Transition])),
            (Class::A, StrategySet::EMPTY),
        ]
    );
    let any_c = json(
        vec![Rule::AnyOf(vec![id(1), id(2)])],
        vec![vec![tipo()], vec![Rule::Pattern("^a".into())]],
    );
    assert_eq!(
        clases(&any_c)[0],
        (
            Class::C,
            conjunto(&[S::Transition, S::BoundedDeterminization, S::Dfa])
        )
    );
}

/// `not(not(type))`: los dos niveles son (c) con producto acotado (cada
/// `not` es su propia regla (c)) y la hoja sigue (a).
#[test]
fn not_anidado() {
    use Strategy as S;
    let mut ir = Ir::new(
        Format::Json,
        node(Sort::Json, None, Step::Root, vec![Rule::Not(id(1))]),
    );
    ir.push(node(
        Sort::Json,
        Some(0),
        Step::Keyword("not".into()),
        vec![Rule::Not(id(2))],
    ));
    ir.push(node(
        Sort::Json,
        Some(1),
        Step::Keyword("not".into()),
        vec![Rule::Type(ir::TypeSet::of(&[ir::JsonType::String]))],
    ));
    let ir = ir.verify().unwrap_or_else(|(_, e)| panic!("{}", e[0]));
    let c = conjunto(&[S::Transition, S::BoundedProduct]);
    assert_eq!(
        clases(&ir),
        [
            (Class::C, c),
            (Class::C, c),
            (Class::A, conjunto(&[S::Transition]))
        ]
    );
}

/// La recursión productiva es un ciclo del grafo que pasa por una arista que
/// consume entrada; E₌ no tiene ciclos (I14), así que el cálculo termina y
/// la clase de un nodo no depende de sus descendientes por `properties`.
#[test]
fn recursion_productiva_termina() {
    // 0 —properties/a→ 1 —$ref→ 0; 0 tiene `required`.
    let ir = json(
        vec![
            Rule::Properties {
                declared: vec![("a".into(), id(1))],
                additional: None,
            },
            Rule::Required(vec!["a".into()]),
        ],
        vec![vec![Rule::Ref(NodeId::ROOT)]],
    );
    let c = clases(&ir);
    assert_eq!(c[0].0, Class::B);
    assert_eq!(c[1].0, Class::B, "el $ref hereda la clase de la raíz");
}

// --- D7 y D16 ----------------------------------------------------------------

const N: usize = 100_000;

/// D7: una cadena de 10⁵ `$ref` hasta un `pattern`, en un hilo de 256 KiB:
/// todos los nodos son (c) con AFD.
#[test]
fn d7_cadena_larga_de_ref() {
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            let mut ir = Ir::new(
                Format::Json,
                node(Sort::Json, None, Step::Root, vec![Rule::Ref(id(1))]),
            );
            for i in 1..N {
                let rules = if i + 1 < N {
                    vec![Rule::Ref(id(i + 1))]
                } else {
                    vec![Rule::Pattern("a".into())]
                };
                ir.push(node(
                    Sort::Json,
                    Some(i - 1),
                    Step::Keyword("not".into()),
                    rules,
                ));
            }
            let ir = ir.verify().unwrap_or_else(|(_, e)| panic!("{}", e[0]));
            let c = classify(&ir);
            assert_eq!(c.count(Class::C), N);
            assert_eq!(
                c.node(NodeId::ROOT).strategies,
                conjunto(&[Strategy::Transition, Strategy::Dfa])
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

/// D16: m nodos, cada uno con `allOf` al siguiente dos veces (2^(m − 1)
/// caminos): se comparte por `NodeId` y el resultado sale en una pasada
/// (se comprueba el resultado, no el tiempo, D1).
#[test]
fn d16_diamantes_sin_expandir() {
    let m = 10_000;
    let mut ir = Ir::new(
        Format::Json,
        node(
            Sort::Json,
            None,
            Step::Root,
            vec![Rule::AllOf(vec![id(1), id(1)])],
        ),
    );
    for i in 1..m {
        let rules = if i + 1 < m {
            vec![Rule::AllOf(vec![id(i + 1), id(i + 1)])]
        } else {
            vec![Rule::MaxItems(3)]
        };
        ir.push(node(
            Sort::Json,
            Some(i - 1),
            Step::Index("allOf".into(), 0),
            rules,
        ));
    }
    let ir = ir.verify().unwrap_or_else(|(_, e)| panic!("{}", e[0]));
    let c = classify(&ir);
    assert_eq!(c.count(Class::C), m - 1);
    assert_eq!(c.count(Class::B), 1);
    assert_eq!(
        c.node(NodeId::ROOT).strategies,
        conjunto(&[Strategy::Register, Strategy::BoundedProduct])
    );
}

/// Las claves estables de la exportación y los nombres de la documentación.
#[test]
fn claves_y_nombres() {
    let claves: Vec<&str> = Strategy::ALL.iter().map(|s| s.key()).collect();
    assert_eq!(
        claves,
        [
            "transition",
            "register",
            "product",
            "bounded-determinization",
            "dfa",
            "deferred"
        ]
    );
    assert_eq!(
        [Class::A, Class::B, Class::C].map(Class::key),
        ["a", "b", "c"]
    );
    assert!(Class::A < Class::B && Class::B < Class::C);
    let todas = conjunto(&Strategy::ALL);
    assert_eq!(todas.iter().collect::<Vec<_>>(), Strategy::ALL);
    assert!(StrategySet::EMPTY.is_empty());
}

/// D16 por otras aristas de E₌: m nodos alternando `$ref` + `allOf` al
/// siguiente (dos caminos) y `dependentSchemas` con dos entradas al
/// siguiente (dos caminos), hasta un `pattern`: 2^(m − 1) caminos, una
/// pasada; todos los nodos son (c) y la raíz reúne las cuatro estrategias.
#[test]
fn d16_diamantes_por_ref_y_dependent_schemas() {
    let m = 10_000;
    let reglas = |i: usize| -> Vec<Rule> {
        if i + 1 == m {
            vec![Rule::Pattern("a".into())]
        } else if i.is_multiple_of(2) {
            vec![Rule::Ref(id(i + 1)), Rule::AllOf(vec![id(i + 1)])]
        } else {
            vec![Rule::DependentSchemas(vec![
                ("a".into(), id(i + 1)),
                ("b".into(), id(i + 1)),
            ])]
        }
    };
    let mut ir = Ir::new(Format::Json, node(Sort::Json, None, Step::Root, reglas(0)));
    for i in 1..m {
        ir.push(node(
            Sort::Json,
            Some(i - 1),
            Step::Index("allOf".into(), 0),
            reglas(i),
        ));
    }
    let ir = ir.verify().unwrap_or_else(|(_, e)| panic!("{}", e[0]));
    let c = classify(&ir);
    assert_eq!(c.count(Class::C), m);
    assert_eq!(
        c.node(NodeId::ROOT).strategies,
        conjunto(&[
            Strategy::Transition,
            Strategy::BoundedProduct,
            Strategy::Dfa,
            Strategy::Deferred
        ])
    );
    // Cada nodo solo reúne lo que tiene debajo: el penúltimo (`$ref` +
    // `allOf` al `pattern`) no ve el `dependentSchemas` de arriba.
    assert_eq!(
        c.node(id(m - 2)).strategies,
        conjunto(&[
            Strategy::Transition,
            Strategy::BoundedProduct,
            Strategy::Dfa
        ])
    );
}
