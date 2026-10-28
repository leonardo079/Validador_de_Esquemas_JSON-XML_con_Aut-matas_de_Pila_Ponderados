//! Exportación de una IR verificada a JSON (formato de `schemac ir`,
//! `docs/fase3/bajada.md` §5) y a DOT: salida exacta y determinista, JSON
//! bien formado para cualquier regla (también las de XSD) y texto del
//! usuario escapado.

use ir::{
    write_dot, write_json, Attribute, Constraint, ContentModel, Format, Ir, JsonType, Lexical,
    LitToken, Literal, Node, NodeId, Num, Occurs, Origin, Particle, Pos, Rule, Sort, Span, Step,
    Term, TypeSet, VerifiedIr,
};

fn sp(offset: u64, end: u64) -> Span {
    Span {
        start: Pos {
            offset,
            line: 1,
            column: offset as u32 + 1,
        },
        end,
    }
}

fn node(sort: Sort, parent: Option<usize>, step: Step, rules: Vec<Rule>) -> Node {
    Node {
        sort,
        rules: rules
            .into_iter()
            .map(|rule| Constraint {
                rule,
                span: sp(1, 2),
            })
            .collect(),
        origin: Origin {
            span: sp(0, 3),
            parent: parent.map(NodeId::new),
            step,
        },
    }
}

fn id(i: usize) -> NodeId {
    NodeId::new(i)
}

fn json_de(ir: &VerifiedIr) -> String {
    let mut s = String::new();
    write_json(ir, &mut s).unwrap();
    s
}

fn dot_de(ir: &VerifiedIr) -> String {
    let mut s = String::new();
    write_dot(ir, &mut s).unwrap();
    s
}

/// `{"properties": {"a\"b": {"$ref": "#"}}}`, a mano.
fn pequena() -> VerifiedIr {
    let mut ir = Ir::new(
        Format::Json,
        node(
            Sort::Json,
            None,
            Step::Root,
            vec![Rule::Properties {
                declared: vec![("a\"b".into(), id(1))],
                additional: None,
            }],
        ),
    );
    ir.push(node(
        Sort::Json,
        Some(0),
        Step::Entry("properties".into(), "a\"b".into()),
        vec![Rule::Ref(NodeId::ROOT)],
    ));
    ir.verify().unwrap()
}

#[test]
fn json_exacto() {
    let esperado = r#"{
  "irVersion": 2,
  "format": "json",
  "targetNamespace": null,
  "nodeCount": 2,
  "edgeCount": 2,
  "classCount": {"a": 2, "b": 0, "c": 0},
  "nodes": [
    {
      "id": 0,
      "sort": "json",
      "span": {"line": 1, "column": 1, "start": 0, "end": 3},
      "parent": null,
      "step": null,
      "class": "a",
      "strategies": ["transition"],
      "rules": [
        {"rule": "properties", "span": {"line": 1, "column": 2, "start": 1, "end": 2}, "class": "a", "strategy": "transition", "declared": [{"name": "a\"b", "target": 1}], "additional": null}
      ]
    },
    {
      "id": 1,
      "sort": "json",
      "span": {"line": 1, "column": 1, "start": 0, "end": 3},
      "parent": 0,
      "step": {"keyword": "properties", "entry": "a\"b"},
      "class": "a",
      "strategies": ["transition"],
      "rules": [
        {"rule": "$ref", "span": {"line": 1, "column": 2, "start": 1, "end": 2}, "class": "a", "strategy": "transition", "target": 0}
      ]
    }
  ]
}
"#;
    assert_eq!(json_de(&pequena()), esperado);
}

#[test]
fn dot_exacto() {
    let esperado = r#"digraph ir {
  rankdir=LR;
  label="IR (json): 2 nodos, 2 aristas; clases: 2 (a), 0 (b), 0 (c)\n(continuas: consumen entrada; discontinuas: misma instancia)";
  labelloc=t;
  node [shape=box, fontname="Helvetica"];
  edge [fontname="Helvetica", fontsize=10];
  n0 [label="0 (raíz)\nproperties\n(a) transición", peripheries=2];
  n1 [label="1 properties/a\\\"b\n$ref\n(a) transición"];
  n0 -> n1 [label="properties/a\\\"b"];
  n1 -> n0 [label="$ref", style=dashed];
}
"#;
    assert_eq!(dot_de(&pequena()), esperado);
}

/// Una regla de cada clase de JSON Schema y todas las de XSD: el JSON es
/// válido, determinista y tiene un objeto por nodo y por regla.
#[test]
fn json_valido_para_todas_las_reglas() {
    use LitToken as T;
    let lit = |t: Vec<LitToken>| Literal(t);
    let mut ir = Ir::new(
        Format::Json,
        node(
            Sort::Json,
            None,
            Step::Root,
            vec![
                Rule::Type(TypeSet::of(&[JsonType::String, JsonType::Null])),
                Rule::Enum(vec![
                    lit(vec![
                        T::Object(2),
                        T::Key("a".into()),
                        T::Array(0),
                        T::Key("b".into()),
                        T::Array(2),
                        T::Number("-1.5e3".into()),
                        T::Object(0),
                    ]),
                    lit(vec![T::String("\u{1}\n\\".into())]),
                ]),
                Rule::Const(lit(vec![T::Bool(true)])),
                Rule::Minimum(Num::new("1")),
                Rule::Maximum(Num::new("2")),
                Rule::ExclusiveMinimum(Num::new("0")),
                Rule::ExclusiveMaximum(Num::new("3")),
                Rule::MultipleOf(Num::new("0.5")),
                Rule::MinLength(0),
                Rule::MaxLength(u64::MAX),
                Rule::Pattern("^\"$".into()),
                Rule::Items {
                    prefix: vec![id(1)],
                    rest: None,
                },
                Rule::MinItems(1),
                Rule::MaxItems(2),
                Rule::UniqueItems,
                Rule::Properties {
                    declared: vec![],
                    additional: Some(id(1)),
                },
                Rule::Required(vec!["x".into()]),
                Rule::MinProperties(1),
                Rule::MaxProperties(2),
                Rule::DependentRequired(vec![("x".into(), vec!["y".into()])]),
                Rule::DependentSchemas(vec![("x".into(), id(1))]),
                Rule::AllOf(vec![id(1)]),
                Rule::AnyOf(vec![id(1), id(1)]),
                Rule::OneOf(vec![id(1)]),
                Rule::Not(id(1)),
            ],
        ),
    );
    ir.push(node(
        Sort::Json,
        Some(0),
        Step::Index("allOf".into(), 0),
        vec![Rule::Reject],
    ));
    let ir = ir.verify().unwrap();
    let texto = json_de(&ir);
    assert_eq!(texto, json_de(&ir), "determinista");
    let v: serde_json::Value = serde_json::from_str(&texto).expect(&texto);
    assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
    let reglas = v["nodes"][0]["rules"].as_array().unwrap();
    assert_eq!(reglas.len(), 25);
    assert_eq!(reglas[0]["types"], serde_json::json!(["null", "string"]));
    assert_eq!(
        reglas[1]["values"],
        serde_json::json!([
            [
                ["object", 2],
                ["key", "a"],
                ["array", 0],
                ["key", "b"],
                ["array", 2],
                ["number", "-1.5e3"],
                ["object", 0]
            ],
            [["string", "\u{1}\n\\"]]
        ])
    );
    assert_eq!(reglas[2]["value"], serde_json::json!([["bool", true]]));
    assert_eq!(reglas[9]["value"], serde_json::json!(u64::MAX));
    assert_eq!(reglas[3]["value"], "1");
    assert_eq!(v["nodes"][1]["step"]["index"], 0);
    assert_eq!(v["edgeCount"], 8);
    // El DOT también se genera y nombra cada arista.
    let dot = dot_de(&ir);
    assert_eq!(dot.matches(" -> ").count(), 8, "{dot}");
}

/// Una IR de XSD con atributos, contenido simple, modelo de contenido y
/// texto: JSON válido con todos sus campos.
#[test]
fn json_de_xsd() {
    let mut ir = Ir::new(
        Format::Xml {
            target_namespace: Some("urn:p".into()),
        },
        node(
            Sort::Element,
            None,
            Step::Root,
            vec![Rule::Content(ContentModel {
                particles: vec![
                    Particle {
                        term: Term::Choice(vec![1]),
                        occurs: Occurs::ONCE,
                        span: sp(0, 1),
                    },
                    Particle {
                        term: Term::Element {
                            name: "pedido".into(),
                            ty: id(1),
                        },
                        occurs: Occurs { min: 0, max: None },
                        span: sp(0, 1),
                    },
                ],
            })],
        ),
    );
    ir.push(node(
        Sort::Element,
        Some(0),
        Step::Entry("complexType".into(), "Pedido".into()),
        vec![
            Rule::Attributes(vec![Attribute {
                name: "moneda".into(),
                required: false,
                ty: id(2),
                default: Some("COP".into()),
                fixed: None,
                span: sp(0, 1),
            }]),
            Rule::SimpleContent(id(2)),
        ],
    ));
    ir.push(node(
        Sort::Text,
        Some(0),
        Step::Entry("builtin".into(), "string".into()),
        vec![Rule::Lexical(Lexical::String)],
    ));
    let ir = ir.verify().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_de(&ir)).unwrap();
    assert_eq!(v["format"], "xml");
    assert_eq!(v["targetNamespace"], "urn:p");
    let content = &v["nodes"][0]["rules"][0];
    assert_eq!(content["rule"], "content");
    assert_eq!(content["particles"][0]["term"], "choice");
    assert_eq!(content["particles"][0]["children"], serde_json::json!([1]));
    assert_eq!(content["particles"][1]["name"], "pedido");
    assert_eq!(content["particles"][1]["max"], serde_json::Value::Null);
    let attrs = &v["nodes"][1]["rules"][0];
    assert_eq!(attrs["attributes"][0]["default"], "COP");
    assert_eq!(attrs["attributes"][0]["type"], 2);
    assert_eq!(v["nodes"][2]["rules"][0]["lexical"], "string");
    assert_eq!(v["nodes"][2]["sort"], "text");
    let dot = dot_de(&ir);
    assert!(dot.contains("n0 -> n1 [label=\"<pedido>\"];"), "{dot}");
    assert!(dot.contains("n1 -> n2 [label=\"@moneda\"];"), "{dot}");
}

/// Los nombres largos o con controles salen recortados y escapados en el
/// DOT (con `short`), y el JSON los conserva enteros pero escapados.
#[test]
fn texto_del_usuario_escapado() {
    let largo = format!("x\n{}", "y".repeat(100));
    let mut ir = Ir::new(
        Format::Json,
        node(
            Sort::Json,
            None,
            Step::Root,
            vec![Rule::Properties {
                declared: vec![(largo.clone(), id(1))],
                additional: None,
            }],
        ),
    );
    ir.push(node(
        Sort::Json,
        Some(0),
        Step::Entry("properties".into(), largo.clone()),
        vec![],
    ));
    let ir = ir.verify().unwrap();
    let dot = dot_de(&ir);
    assert!(!dot.contains(&"y".repeat(40)), "{dot}");
    assert!(dot.contains("x\\\\n"), "{dot}");
    let v: serde_json::Value = serde_json::from_str(&json_de(&ir)).unwrap();
    assert_eq!(v["nodes"][1]["step"]["entry"], largo.as_str());
}

/// D7: la exportación no es recursiva. Un literal de 10⁵ arreglos anidados
/// y una cadena de 10⁵ `$ref` se exportan en un hilo de 256 KiB.
#[test]
fn exportar_sin_recursion() {
    const N: usize = 100_000;
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            let mut toks: Vec<LitToken> = (0..N).map(|_| LitToken::Array(1)).collect();
            toks.push(LitToken::Null);
            let mut ir = Ir::new(
                Format::Json,
                node(
                    Sort::Json,
                    None,
                    Step::Root,
                    vec![Rule::Const(Literal(toks)), Rule::Ref(id(1))],
                ),
            );
            for i in 1..N {
                let rules = if i + 1 < N {
                    vec![Rule::Ref(id(i + 1))]
                } else {
                    vec![]
                };
                ir.push(node(
                    Sort::Json,
                    Some(i - 1),
                    Step::Keyword("not".into()),
                    rules,
                ));
            }
            let ir = ir.verify().unwrap();
            let texto = json_de(&ir);
            let esperado = format!("{}[\"null\"]]", "[\"array\", 1], ".repeat(N));
            assert!(texto.contains(&esperado));
            assert_eq!(dot_de(&ir).matches(" -> ").count(), N - 1);
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Los literales de `enum`/`const` salen como su lista de tokens en
/// preorden, con los números como lexema en una cadena: un lector JSON
/// cualquiera (`serde_json`, JavaScript) los lee sin perder nada, también
/// `1e400` (fuera de `f64`) y `0.10000000000000000000001` (que `f64`
/// redondea), y sin recursión.
#[test]
fn literales_sin_perdida() {
    use LitToken as T;
    let num = |n: &str| T::Number(n.into());
    let ir = Ir::new(
        Format::Json,
        node(
            Sort::Json,
            None,
            Step::Root,
            vec![
                Rule::Enum(vec![
                    Literal(vec![num("1e400")]),
                    Literal(vec![num("0.10000000000000000000001")]),
                    Literal(vec![T::Null]),
                ]),
                Rule::Const(Literal(vec![
                    T::Object(1),
                    T::Key("a\"/~\n".into()),
                    T::Object(2),
                    T::Key("x".into()),
                    T::Array(2),
                    num("-0e-7"),
                    T::Bool(false),
                    T::Key("y".into()),
                    T::String("\u{0}".into()),
                ])),
            ],
        ),
    )
    .verify()
    .unwrap();
    let texto = json_de(&ir);
    let v: serde_json::Value = serde_json::from_str(&texto).expect(&texto);
    let reglas = &v["nodes"][0]["rules"];
    assert_eq!(
        reglas[0]["values"],
        serde_json::json!([
            [["number", "1e400"]],
            [["number", "0.10000000000000000000001"]],
            [["null"]]
        ])
    );
    assert_eq!(
        reglas[1]["value"],
        serde_json::json!([
            ["object", 1],
            ["key", "a\"/~\n"],
            ["object", 2],
            ["key", "x"],
            ["array", 2],
            ["number", "-0e-7"],
            ["bool", false],
            ["key", "y"],
            ["string", "\u{0}"]
        ])
    );
}

/// Versión 2 (tarea 4.3): cada nodo lleva su clase y sus estrategias, cada
/// regla las suyas, y la cabecera el recuento por clase; en DOT, la última
/// línea de cada nodo es su clase y los nodos (c) van con borde grueso.
/// `{"anyOf": [{"pattern": "^a"}, {"minLength": 2}]}`, a mano.
#[test]
fn clase_y_estrategia_en_la_salida() {
    let mut ir = Ir::new(
        Format::Json,
        node(
            Sort::Json,
            None,
            Step::Root,
            vec![Rule::AnyOf(vec![id(1), id(2)])],
        ),
    );
    ir.push(node(
        Sort::Json,
        Some(0),
        Step::Index("anyOf".into(), 0),
        vec![Rule::Pattern("^a".into())],
    ));
    ir.push(node(
        Sort::Json,
        Some(0),
        Step::Index("anyOf".into(), 1),
        vec![Rule::MinLength(2)],
    ));
    let ir = ir.verify().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json_de(&ir)).unwrap();
    assert_eq!(v["irVersion"], 2);
    assert_eq!(v["classCount"], serde_json::json!({"a": 0, "b": 1, "c": 2}));
    let raiz = &v["nodes"][0];
    assert_eq!(raiz["class"], "c");
    assert_eq!(
        raiz["strategies"],
        serde_json::json!(["register", "bounded-determinization", "dfa"])
    );
    assert_eq!(raiz["rules"][0]["class"], "c");
    assert_eq!(raiz["rules"][0]["strategy"], "bounded-determinization");
    assert_eq!(v["nodes"][1]["rules"][0]["strategy"], "dfa");
    assert_eq!(v["nodes"][2]["class"], "b");
    assert_eq!(v["nodes"][2]["strategies"], serde_json::json!(["register"]));

    let dot = dot_de(&ir);
    assert!(dot.contains("clases: 0 (a), 1 (b), 2 (c)"), "{dot}");
    assert!(
        dot.contains(
            "n0 [label=\"0 (raíz)\\nanyOf\\n(c) registro o contador, determinización acotada, \
             AFD\", peripheries=2, style=bold];"
        ),
        "{dot}"
    );
    assert!(
        dot.contains("n2 [label=\"2 anyOf/1\\nminLength\\n(b) registro o contador\"];"),
        "{dot}"
    );
}
