//! Forma de la IR (tarea 4.1): IR construidas a mano de un esquema JSON
//! recursivo y de un XSD con `sequence`, `choice`, atributo requerido y
//! `maxOccurs="unbounded"` pasan [`Ir::verify`]; cada invariante I1–I14 de
//! `docs/fase3/ir.md` §4 tiene al menos un caso que lo rompe y falla con su
//! código; y el verificador no es recursivo (D7) ni la IR crece al compartir
//! referencias.

use ir::{
    Attribute, Constraint, ContentModel, Format, InternalError, Ir, JsonType, Lexical, LitToken,
    Literal, Node, NodeId, Num, Occurs, Origin, Particle, Pos, Rule, Sort, Span, Step, Term,
    TypeSet, VerifiedIr, VerifyError,
};

// --- Ayudas para construir IR a mano -------------------------------------

fn sp() -> Span {
    Span {
        start: Pos::START,
        end: 0,
    }
}

fn id(i: usize) -> NodeId {
    NodeId::new(i)
}

fn kw(k: &str) -> Step {
    Step::Keyword(k.to_owned())
}

fn entry(k: &str, e: &str) -> Step {
    Step::Entry(k.to_owned(), e.to_owned())
}

fn node(sort: Sort, parent: Option<usize>, step: Step, rules: Vec<Rule>) -> Node {
    Node {
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

fn root(sort: Sort, rules: Vec<Rule>) -> Node {
    node(sort, None, Step::Root, rules)
}

fn json(rules: Vec<Rule>) -> Ir {
    Ir::new(Format::Json, root(Sort::Json, rules))
}

/// Un nodo JSON hijo de la raíz.
fn hijo(ir: &mut Ir, rules: Vec<Rule>) -> NodeId {
    let n = ir.nodes.len();
    ir.push(node(
        Sort::Json,
        Some(0),
        Step::Index("allOf".into(), n),
        rules,
    ))
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn lit(tokens: Vec<LitToken>) -> Literal {
    Literal(tokens)
}

fn s(v: &str) -> LitToken {
    LitToken::String(v.to_owned())
}

fn key(v: &str) -> LitToken {
    LitToken::Key(v.to_owned())
}

fn el(name: &str, ty: usize, occurs: Occurs) -> Particle {
    Particle {
        term: Term::Element {
            name: name.to_owned(),
            ty: id(ty),
        },
        occurs,
        span: sp(),
    }
}

fn group(term: Term, occurs: Occurs) -> Particle {
    Particle {
        term,
        occurs,
        span: sp(),
    }
}

fn content(particles: Vec<Particle>) -> Rule {
    Rule::Content(ContentModel { particles })
}

fn attr(name: &str, required: bool, ty: usize) -> Attribute {
    Attribute {
        name: name.to_owned(),
        required,
        ty: id(ty),
        default: None,
        fixed: None,
        span: sp(),
    }
}

/// [`Ir::verify`] consume la IR (D15); las pruebas la verifican sobre una
/// copia y comprueban además que no la cambia: si pasa, la `VerifiedIr` es
/// la misma IR; si no, devuelve la IR intacta junto a los errores.
#[track_caller]
fn verificar(ir: &Ir) -> Result<VerifiedIr, Vec<VerifyError>> {
    match ir.clone().verify() {
        Ok(v) => {
            assert_eq!(v.ir(), ir, "verify cambió la IR");
            Ok(v)
        }
        Err((devuelta, e)) => {
            assert_eq!(&devuelta, ir, "verify no devolvió la IR intacta");
            Err(e)
        }
    }
}

/// Códigos de los errores del verificador, en orden.
fn codes(ir: &Ir) -> Vec<&'static str> {
    match verificar(ir) {
        Ok(_) => Vec::new(),
        Err(e) => e.iter().map(|e| e.code()).collect(),
    }
}

/// La IR rompe exactamente un invariante, con el código `code`.
#[track_caller]
fn falla(ir: &Ir, code: &str) {
    assert_eq!(codes(ir), [code], "{:?}", verificar(ir));
}

#[track_caller]
fn pasa(ir: &Ir) {
    if let Err(e) = verificar(ir) {
        panic!("{}", e[0]);
    }
}

// --- IR de ejemplo ---------------------------------------------------------

/// IR a mano de un árbol recursivo en JSON Schema: la raíz se referencia a
/// sí misma por `properties` (`hijo`) y por `properties` + `items` (`hijos`).
///
/// ```json
/// { "type": "object",
///   "properties": {
///     "nombre": { "type": "string", "minLength": 1 },
///     "hijo":   { "$ref": "#" },
///     "hijos":  { "type": "array", "items": { "$ref": "#" } } },
///   "required": ["nombre"],
///   "additionalProperties": false }
/// ```
fn arbol_json() -> Ir {
    let mut ir = json(vec![
        Rule::Type(TypeSet::of(&[JsonType::Object])),
        Rule::Properties {
            declared: vec![
                ("nombre".into(), id(1)),
                ("hijo".into(), id(2)),
                ("hijos".into(), id(3)),
            ],
            additional: Some(id(5)),
        },
        Rule::Required(names(&["nombre"])),
    ]);
    ir.push(node(
        Sort::Json,
        Some(0),
        entry("properties", "nombre"),
        vec![
            Rule::Type(TypeSet::of(&[JsonType::String])),
            Rule::MinLength(1),
        ],
    ));
    ir.push(node(
        Sort::Json,
        Some(0),
        entry("properties", "hijo"),
        vec![Rule::Ref(NodeId::ROOT)],
    ));
    ir.push(node(
        Sort::Json,
        Some(0),
        entry("properties", "hijos"),
        vec![
            Rule::Type(TypeSet::of(&[JsonType::Array])),
            Rule::Items {
                prefix: vec![],
                rest: Some(id(4)),
            },
        ],
    ));
    ir.push(node(
        Sort::Json,
        Some(3),
        kw("items"),
        vec![Rule::Ref(NodeId::ROOT)],
    ));
    ir.push(node(
        Sort::Json,
        Some(0),
        kw("additionalProperties"),
        vec![Rule::Reject],
    ));
    ir
}

#[test]
fn json_recursivo_por_properties_pasa() {
    let ir = arbol_json();
    pasa(&ir);
    // Un nodo por subesquema; la recursión son aristas, no copias.
    assert_eq!(ir.nodes.len(), 6);
    assert_eq!(ir.edge_count(), 7);
    assert_eq!(ir.location(id(4)), "#/properties/hijos/items");
    assert_eq!(ir.location(NodeId::ROOT), "#");
}

/// IR a mano de un XSD con `sequence`, `choice`, atributos requeridos,
/// `maxOccurs="unbounded"`, `simpleContent` con extensión y un tipo con
/// nombre recursivo por el modelo de contenido:
///
/// ```xml
/// <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
///   <xs:element name="pedido" type="Pedido"/>
///   <xs:complexType name="Pedido">
///     <xs:sequence>
///       <xs:element name="cliente" type="xs:string"/>
///       <xs:choice>
///         <xs:element name="tarjeta" type="xs:string"/>
///         <xs:element name="efectivo" type="xs:decimal"/>
///       </xs:choice>
///       <xs:element name="item" maxOccurs="unbounded">
///         <xs:complexType>
///           <xs:simpleContent>
///             <xs:extension base="xs:decimal">
///               <xs:attribute name="sku" type="xs:string" use="required"/>
///               <xs:attribute name="moneda" type="xs:string" default="COP"/>
///             </xs:extension>
///           </xs:simpleContent>
///         </xs:complexType>
///       </xs:element>
///       <xs:element name="subpedido" type="Pedido" minOccurs="0"/>
///     </xs:sequence>
///     <xs:attribute name="id" type="xs:int" use="required"/>
///   </xs:complexType>
/// </xs:schema>
/// ```
fn pedido_xsd() -> Ir {
    let mut ir = Ir::new(
        Format::Xml {
            target_namespace: None,
        },
        // El documento: exactamente uno de los elementos globales.
        root(
            Sort::Element,
            vec![content(vec![
                group(Term::Choice(vec![1]), Occurs::ONCE),
                el("pedido", 1, Occurs::ONCE),
            ])],
        ),
    );
    // 1: complexType Pedido.
    let mut moneda = attr("moneda", false, 6);
    moneda.default = Some("COP".into());
    ir.push(node(
        Sort::Element,
        Some(0),
        entry("complexType", "Pedido"),
        vec![
            content(vec![
                group(Term::Sequence(vec![1, 2, 5, 6]), Occurs::ONCE),
                el("cliente", 3, Occurs::ONCE),
                group(Term::Choice(vec![3, 4]), Occurs::ONCE),
                el("tarjeta", 3, Occurs::ONCE),
                el("efectivo", 4, Occurs::ONCE),
                el("item", 5, Occurs { min: 1, max: None }),
                el(
                    "subpedido",
                    1,
                    Occurs {
                        min: 0,
                        max: Some(1),
                    },
                ),
            ]),
            Rule::Attributes(vec![attr("id", true, 2)]),
        ],
    ));
    // 2: xs:int = entero en [-2³¹, 2³¹ − 1].
    ir.push(node(
        Sort::Text,
        Some(0),
        entry("builtin", "int"),
        vec![
            Rule::Lexical(Lexical::Integer),
            Rule::Minimum(Num::new("-2147483648")),
            Rule::Maximum(Num::new("2147483647")),
        ],
    ));
    // 3: elemento de contenido xs:string sin atributos (compartido por
    // `cliente` y `tarjeta`; su origen nombra el tipo simple, no un uso).
    ir.push(node(
        Sort::Element,
        Some(0),
        entry("simpleContent", "xs:string"),
        vec![Rule::SimpleContent(id(6))],
    ));
    // 4: elemento de contenido xs:decimal.
    ir.push(node(
        Sort::Element,
        Some(0),
        entry("simpleContent", "xs:decimal"),
        vec![Rule::SimpleContent(id(7))],
    ));
    // 5: tipo anónimo de `item`: simpleContent + extension con atributos.
    ir.push(node(
        Sort::Element,
        Some(1),
        entry("element", "item"),
        vec![
            Rule::SimpleContent(id(7)),
            Rule::Attributes(vec![attr("sku", true, 6), moneda]),
        ],
    ));
    // 6, 7: tipos predefinidos.
    ir.push(node(
        Sort::Text,
        Some(0),
        entry("builtin", "string"),
        vec![Rule::Lexical(Lexical::String)],
    ));
    ir.push(node(
        Sort::Text,
        Some(0),
        entry("builtin", "decimal"),
        vec![Rule::Lexical(Lexical::Decimal)],
    ));
    ir
}

#[test]
fn xsd_con_sequence_choice_atributo_y_unbounded_pasa() {
    let ir = pedido_xsd();
    pasa(&ir);
    assert_eq!(ir.location(id(5)), "#/complexType/Pedido/element/item");
    // Nodos sintéticos (`ir.md` §5.2): predefinidos y envoltorios de
    // contenido simple cuelgan del documento.
    assert_eq!(ir.location(id(2)), "#/builtin/int");
    assert_eq!(ir.location(id(3)), "#/simpleContent/xs:string");
}

/// Varios invariantes rotos a la vez: se informan todos, en orden de nodo.
#[test]
fn informa_todos_los_errores_en_orden() {
    let mut ir = json(vec![Rule::AllOf(vec![]), Rule::Ref(id(9))]);
    hijo(&mut ir, vec![Rule::Minimum(Num::new("01"))]);
    let e = verificar(&ir).unwrap_err();
    let got: Vec<_> = e.iter().map(|e| (e.node.index(), e.code())).collect();
    assert_eq!(got, [(0, "empty"), (0, "dangling"), (1, "number")]);
}

#[test]
fn mensaje_claro() {
    let ir = json(vec![Rule::Ref(id(9))]);
    let e = verificar(&ir).unwrap_err();
    assert_eq!(
        e[0].to_string(),
        "IR inválida: nodo 0 (línea 1, columna 1): \"$ref\" apunta al nodo 9, que no existe"
    );
}

// --- I1: origen ------------------------------------------------------------

#[test]
fn i1_ir_vacia() {
    let ir = Ir {
        format: Format::Json,
        nodes: vec![],
    };
    falla(&ir, "origin");
}

#[test]
fn i1_raiz_con_padre_o_con_paso() {
    let mut ir = json(vec![]);
    ir.nodes[0].origin.parent = Some(id(0));
    falla(&ir, "origin");
    let mut ir = json(vec![]);
    ir.nodes[0].origin.step = kw("items");
    falla(&ir, "origin");
}

#[test]
fn i1_padre_que_no_es_anterior() {
    for parent in [None, Some(1), Some(2)] {
        let mut ir = json(vec![]);
        ir.push(node(Sort::Json, parent, kw("not"), vec![]));
        falla(&ir, "origin");
    }
    let mut ir = json(vec![]);
    ir.push(node(Sort::Json, Some(0), Step::Root, vec![]));
    falla(&ir, "origin");
}

// --- I2: ids en rango ------------------------------------------------------

#[test]
fn i2_arista_colgante() {
    for rule in [
        Rule::Not(id(1)),
        Rule::Items {
            prefix: vec![id(0), id(7)],
            rest: None,
        },
        Rule::Properties {
            declared: vec![],
            additional: Some(id(3)),
        },
        Rule::DependentSchemas(vec![("a".into(), id(1))]),
    ] {
        falla(&json(vec![rule]), "dangling");
    }
    let mut ir = pedido_xsd();
    ir.nodes[1].rules[1].rule = Rule::Attributes(vec![attr("id", true, 40)]);
    falla(&ir, "dangling");
}

// --- I3: reglas admitidas por el tipo del nodo -----------------------------

#[test]
fn i3_regla_de_otro_tipo_de_nodo() {
    falla(&json(vec![Rule::Lexical(Lexical::String)]), "rule-sort");
    let mut ir = pedido_xsd();
    ir.nodes[3].rules.push(Constraint {
        rule: Rule::Type(TypeSet::of(&[JsonType::String])),
        span: sp(),
    });
    falla(&ir, "rule-sort");
    let mut ir = pedido_xsd();
    ir.nodes[6].rules.push(Constraint {
        rule: Rule::MultipleOf(Num::new("2")),
        span: sp(),
    });
    falla(&ir, "rule-sort");
}

// --- I4: a lo sumo una regla de cada clase ---------------------------------

#[test]
fn i4_regla_repetida() {
    falla(
        &json(vec![Rule::MinLength(1), Rule::MinLength(2)]),
        "duplicate-rule",
    );
    let mut ir = pedido_xsd();
    ir.nodes[3].rules.push(Constraint {
        rule: content(vec![el("x", 3, Occurs::ONCE)]),
        span: sp(),
    });
    falla(&ir, "duplicate-rule");
}

// --- I5: tipo del destino de cada arista -----------------------------------

#[test]
fn i5_destino_de_otro_tipo() {
    // Atributo cuyo tipo es un elemento.
    let mut ir = pedido_xsd();
    ir.nodes[1].rules[1].rule = Rule::Attributes(vec![attr("id", true, 3)]);
    falla(&ir, "target-sort");
    // Partícula cuyo tipo es un texto.
    let mut ir = pedido_xsd();
    ir.nodes[0].rules[0].rule = content(vec![el("pedido", 6, Occurs::ONCE)]);
    falla(&ir, "target-sort");
}

// --- I6: formato -----------------------------------------------------------

#[test]
fn i6_formato() {
    // Nodo XML en una IR de JSON.
    let mut ir = json(vec![]);
    ir.push(node(
        Sort::Text,
        Some(0),
        kw("x"),
        vec![Rule::Lexical(Lexical::String)],
    ));
    falla(&ir, "format");
    // Raíz XML que no es el documento.
    let ir = Ir::new(
        Format::Xml {
            target_namespace: None,
        },
        root(Sort::Text, vec![Rule::Lexical(Lexical::String)]),
    );
    falla(&ir, "format");
    // Documento sin modelo de contenido o con atributos.
    let mut ir = pedido_xsd();
    ir.nodes[0].rules.clear();
    falla(&ir, "format");
    let mut ir = pedido_xsd();
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::Attributes(vec![]),
        span: sp(),
    });
    falla(&ir, "format");
}

// --- I7: listas no vacías --------------------------------------------------

#[test]
fn i7_listas_vacias() {
    for rule in [
        Rule::AllOf(vec![]),
        Rule::AnyOf(vec![]),
        Rule::OneOf(vec![]),
        Rule::Type(TypeSet::of(&[])),
        Rule::Items {
            prefix: vec![],
            rest: None,
        },
    ] {
        falla(&json(vec![rule]), "empty");
    }
    // `enum: []` sí se admite (D11), igual que `properties: {}`.
    pasa(&json(vec![Rule::Enum(vec![])]));
    pasa(&json(vec![Rule::Properties {
        declared: vec![],
        additional: None,
    }]));
}

// --- I8: números -----------------------------------------------------------

#[test]
fn i8_lexemas_de_rfc_8259() {
    for ok in ["0", "-0", "12", "-1.50", "1e400", "1E-7", "2.5e+3"] {
        pasa(&json(vec![Rule::Minimum(Num::new(ok))]));
    }
    for bad in [
        "", "01", "1.", ".5", "+1", "-", "1e", "1e+", "0x1", "1 ", "NaN",
    ] {
        falla(&json(vec![Rule::Maximum(Num::new(bad))]), "number");
    }
}

#[test]
fn i8_multiple_of_positivo() {
    pasa(&json(vec![Rule::MultipleOf(Num::new("0.01"))]));
    for bad in ["0", "-0", "0.000e5", "-2"] {
        falla(&json(vec![Rule::MultipleOf(Num::new(bad))]), "number");
    }
}

// --- I9: literales ---------------------------------------------------------

#[test]
fn i9_literales_bien_formados() {
    let bueno = lit(vec![
        LitToken::Object(2),
        key("a"),
        LitToken::Array(2),
        LitToken::Null,
        LitToken::Number("1.0".into()),
        key("b"),
        LitToken::Object(0),
    ]);
    pasa(&json(vec![Rule::Enum(vec![bueno, lit(vec![s("x")])])]));
    for (malo, que) in [
        (vec![], "vacío"),
        (vec![LitToken::Null, LitToken::Null], "sobra"),
        (vec![LitToken::Array(2), LitToken::Null], "falta"),
        (vec![key("a")], "clave fuera de objeto"),
        (vec![LitToken::Object(1), LitToken::Null], "valor sin clave"),
        (
            vec![
                LitToken::Object(2),
                key("b"),
                LitToken::Null,
                key("a"),
                LitToken::Null,
            ],
            "desordenadas",
        ),
        (
            vec![
                LitToken::Object(2),
                key("a"),
                LitToken::Null,
                key("a"),
                LitToken::Null,
            ],
            "repetidas",
        ),
        (vec![LitToken::Number("1.".into())], "número"),
    ] {
        let ir = json(vec![Rule::Enum(vec![lit(vec![s("ok")]), lit(malo)])]);
        let e = verificar(&ir).expect_err(que);
        assert_eq!(e.len(), 1, "{que}: {e:?}");
        assert_eq!(e[0].code(), "literal", "{que}");
        assert!(
            matches!(e[0].kind, ir::VerifyErrorKind::Literal { index: 1, .. }),
            "{que}"
        );
    }
}

// --- I10: nombres únicos ---------------------------------------------------

#[test]
fn i10_nombres_repetidos() {
    let mut ir = json(vec![]);
    let a = hijo(&mut ir, vec![]);
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::Properties {
            declared: vec![("a".into(), a), ("a".into(), a)],
            additional: None,
        },
        span: sp(),
    });
    falla(&ir, "duplicate-name");
    for rule in [
        Rule::Required(names(&["a", "b", "a"])),
        Rule::DependentRequired(vec![("a".into(), names(&["b"])), ("a".into(), vec![])]),
        Rule::DependentRequired(vec![("a".into(), names(&["b", "b"]))]),
    ] {
        falla(&json(vec![rule]), "duplicate-name");
    }
    let mut ir = json(vec![]);
    let a = hijo(&mut ir, vec![]);
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::DependentSchemas(vec![("a".into(), a), ("a".into(), a)]),
        span: sp(),
    });
    falla(&ir, "duplicate-name");
    let mut ir = pedido_xsd();
    ir.nodes[5].rules[1].rule = Rule::Attributes(vec![attr("sku", true, 6), attr("sku", false, 6)]);
    falla(&ir, "duplicate-name");
}

// --- I11: modelo de contenido en preorden ----------------------------------

fn con_modelo(particles: Vec<Particle>) -> Ir {
    let mut ir = pedido_xsd();
    ir.nodes[1].rules[0].rule = content(particles);
    ir
}

#[test]
fn i11_arbol_de_particulas() {
    let seq = |hijos: Vec<usize>| group(Term::Sequence(hijos), Occurs::ONCE);
    // Bien: grupos anidados en preorden.
    pasa(&con_modelo(vec![
        seq(vec![1, 4]),
        group(Term::Choice(vec![2, 3]), Occurs { min: 0, max: None }),
        el("a", 3, Occurs::ONCE),
        el("b", 3, Occurs::ONCE),
        el("c", 3, Occurs::ONCE),
    ]));
    for (malo, que) in [
        (vec![], "vacío"),
        (vec![seq(vec![0])], "hijo no posterior"),
        (
            vec![
                seq(vec![1]),
                el("a", 3, Occurs::ONCE),
                el("b", 3, Occurs::ONCE),
            ],
            "huérfano",
        ),
        (
            vec![seq(vec![1, 1]), el("a", 3, Occurs::ONCE)],
            "hijo dos veces",
        ),
        (vec![seq(vec![2])], "fuera de rango"),
    ] {
        assert_eq!(codes(&con_modelo(malo)), ["content-model"], "{que}");
    }
}

#[test]
fn i11_min_occurs_mayor_que_max_occurs() {
    falla(
        &con_modelo(vec![el(
            "a",
            3,
            Occurs {
                min: 3,
                max: Some(2),
            },
        )]),
        "content-model",
    );
    // maxOccurs="0" es válido en XSD (el elemento no puede aparecer).
    pasa(&con_modelo(vec![el(
        "a",
        3,
        Occurs {
            min: 0,
            max: Some(0),
        },
    )]));
}

// --- I12: xs:all y declaraciones consistentes ------------------------------

#[test]
fn i12_all() {
    let opt = Occurs {
        min: 0,
        max: Some(1),
    };
    pasa(&con_modelo(vec![
        group(Term::All(vec![1, 2]), Occurs::ONCE),
        el("a", 3, Occurs::ONCE),
        el("b", 3, opt),
    ]));
    // No es la raíz del modelo.
    falla(
        &con_modelo(vec![
            group(Term::Sequence(vec![1]), Occurs::ONCE),
            group(Term::All(vec![2]), Occurs::ONCE),
            el("a", 3, Occurs::ONCE),
        ]),
        "all",
    );
    // maxOccurs del grupo distinto de 1.
    falla(
        &con_modelo(vec![
            group(Term::All(vec![1]), Occurs { min: 1, max: None }),
            el("a", 3, Occurs::ONCE),
        ]),
        "all",
    );
    // Hijo con maxOccurs > 1, e hijo que es un grupo.
    falla(
        &con_modelo(vec![
            group(Term::All(vec![1]), Occurs::ONCE),
            el(
                "a",
                3,
                Occurs {
                    min: 1,
                    max: Some(2),
                },
            ),
        ]),
        "all",
    );
    falla(
        &con_modelo(vec![
            group(Term::All(vec![1]), Occurs::ONCE),
            group(Term::Choice(vec![2]), Occurs::ONCE),
            el("a", 3, Occurs::ONCE),
        ]),
        "all",
    );
}

#[test]
fn i12_declaraciones_consistentes() {
    falla(
        &con_modelo(vec![
            group(Term::Sequence(vec![1, 2]), Occurs::ONCE),
            el("a", 3, Occurs::ONCE),
            el("a", 4, Occurs::ONCE),
        ]),
        "element-consistency",
    );
    // El mismo nombre con el mismo tipo sí.
    pasa(&con_modelo(vec![
        group(Term::Sequence(vec![1, 2]), Occurs::ONCE),
        el("a", 3, Occurs::ONCE),
        el("a", 3, Occurs::ONCE),
    ]));
}

// --- I13: textos y atributos -----------------------------------------------

#[test]
fn i13_texto_con_un_espacio_lexico() {
    let mut ir = pedido_xsd();
    ir.nodes[6].rules.clear();
    falla(&ir, "text");
    let mut ir = pedido_xsd();
    ir.nodes[6].rules.push(Constraint {
        rule: Rule::Lexical(Lexical::Decimal),
        span: sp(),
    });
    falla(&ir, "duplicate-rule");
}

#[test]
fn i13_facetas_segun_el_espacio_lexico() {
    let mut ir = pedido_xsd();
    ir.nodes[7].rules.push(Constraint {
        rule: Rule::MinLength(2),
        span: sp(),
    });
    falla(&ir, "text");
    let mut ir = pedido_xsd();
    ir.nodes[6].rules.push(Constraint {
        rule: Rule::Minimum(Num::new("1")),
        span: sp(),
    });
    falla(&ir, "text");
    // `pattern` y `enum` valen en cualquiera.
    let mut ir = pedido_xsd();
    ir.nodes[7].rules.push(Constraint {
        rule: Rule::Pattern("[0-9]+".into()),
        span: sp(),
    });
    pasa(&ir);
}

#[test]
fn i13_enumeracion_del_tipo_del_texto() {
    let con_enum = |n: usize, valores: Vec<Literal>| {
        let mut ir = pedido_xsd();
        ir.nodes[n].rules.push(Constraint {
            rule: Rule::Enum(valores),
            span: sp(),
        });
        ir
    };
    pasa(&con_enum(6, vec![lit(vec![s("a")]), lit(vec![s("b")])]));
    pasa(&con_enum(
        7,
        vec![lit(vec![LitToken::Number("1.5".into())])],
    ));
    falla(
        &con_enum(6, vec![lit(vec![LitToken::Number("1".into())])]),
        "text",
    );
    falla(&con_enum(7, vec![lit(vec![s("1.5")])]), "text");
    falla(&con_enum(6, vec![lit(vec![LitToken::Array(0)])]), "text");
}

#[test]
fn i13_restriccion_conserva_el_espacio_lexico() {
    // T8 restringe xs:int (nodo 2) con `maxInclusive`: mismo espacio léxico.
    let mut ir = pedido_xsd();
    let t = ir.push(node(
        Sort::Text,
        Some(0),
        entry("simpleType", "Pequeno"),
        vec![
            Rule::Lexical(Lexical::Integer),
            Rule::Ref(id(2)),
            Rule::Maximum(Num::new("9")),
        ],
    ));
    ir.nodes[1].rules[1].rule = Rule::Attributes(vec![attr("id", true, t.index())]);
    pasa(&ir);
    // Restringir xs:int con espacio léxico de cadena (sin facetas de rango,
    // que darían su propio error).
    ir.nodes[t.index()].rules.truncate(2);
    ir.nodes[t.index()].rules[0].rule = Rule::Lexical(Lexical::String);
    falla(&ir, "text");
}

#[test]
fn i13_atributos_default_y_fixed() {
    let mut a = attr("moneda", false, 6);
    a.default = Some("COP".into());
    a.fixed = Some("COP".into());
    let mut ir = pedido_xsd();
    ir.nodes[5].rules[1].rule = Rule::Attributes(vec![attr("sku", true, 6), a]);
    falla(&ir, "attribute");
    let mut a = attr("sku", true, 6);
    a.default = Some("x".into());
    let mut ir = pedido_xsd();
    ir.nodes[5].rules[1].rule = Rule::Attributes(vec![a]);
    falla(&ir, "attribute");
    // Requerido con `fixed` sí es válido.
    let mut a = attr("sku", true, 6);
    a.fixed = Some("x".into());
    let mut ir = pedido_xsd();
    ir.nodes[5].rules[1].rule = Rule::Attributes(vec![a]);
    pasa(&ir);
}

// --- I14: sin ciclos sobre la misma instancia ------------------------------

#[test]
fn i14_ciclos_que_no_consumen() {
    // {"$ref": "#"}
    falla(&json(vec![Rule::Ref(NodeId::ROOT)]), "cycle");
    // A —allOf→ B —not→ A
    let mut ir = json(vec![]);
    let b = hijo(&mut ir, vec![Rule::Not(NodeId::ROOT)]);
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::AllOf(vec![b]),
        span: sp(),
    });
    falla(&ir, "cycle");
    // Por dependentSchemas.
    let mut ir = json(vec![]);
    let b = hijo(&mut ir, vec![Rule::AnyOf(vec![NodeId::ROOT])]);
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::DependentSchemas(vec![("k".into(), b)]),
        span: sp(),
    });
    falla(&ir, "cycle");
    // Restricción circular de tipos simples en XSD.
    let mut ir = pedido_xsd();
    ir.nodes[6].rules.push(Constraint {
        rule: Rule::Ref(id(6)),
        span: sp(),
    });
    falla(&ir, "cycle");
}

#[test]
fn i14_recursion_productiva_y_diamantes_pasan() {
    // A —properties→ B —$ref→ A consume entrada: válido.
    pasa(&arbol_json());
    // Diamante sin ciclo: A —allOf→ {B, C}, B —$ref→ D, C —$ref→ D.
    let mut ir = json(vec![]);
    let d = hijo(&mut ir, vec![]);
    let b = hijo(&mut ir, vec![Rule::Ref(d)]);
    let c = hijo(&mut ir, vec![Rule::Ref(d)]);
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::AllOf(vec![b, c]),
        span: sp(),
    });
    pasa(&ir);
}

// --- D7 y tamaño -----------------------------------------------------------

/// Corre `f` en un hilo de 256 KiB: un recorrido recursivo de 10⁵ niveles
/// necesitaría varios MiB.
fn en_pila_pequena(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

const N: usize = 100_000;

#[test]
fn d7_cadena_larga_de_ref() {
    en_pila_pequena(|| {
        // 0 —$ref→ 1 —$ref→ … —$ref→ N−1, cada uno hijo del anterior.
        let mut ir = json(vec![Rule::Ref(id(1))]);
        for i in 1..N {
            let rules = if i + 1 < N {
                vec![Rule::Ref(id(i + 1))]
            } else {
                vec![]
            };
            ir.push(node(Sort::Json, Some(i - 1), kw("not"), rules));
        }
        pasa(&ir);
        assert_eq!(ir.location(id(N - 1)).len(), 1 + 4 * (N - 1));
        // Cerrar el ciclo al final da un error.
        ir.nodes[N - 1].rules.push(Constraint {
            rule: Rule::Ref(NodeId::ROOT),
            span: sp(),
        });
        falla(&ir, "cycle");
        drop(ir);
    });
}

#[test]
fn d7_modelo_de_contenido_y_literal_profundos() {
    en_pila_pequena(|| {
        let mut ps: Vec<Particle> = (0..N - 1)
            .map(|i| group(Term::Sequence(vec![i + 1]), Occurs::ONCE))
            .collect();
        ps.push(el("hoja", 3, Occurs::ONCE));
        pasa(&con_modelo(ps));

        let mut toks: Vec<LitToken> = (0..N).map(|_| LitToken::Array(1)).collect();
        toks.push(LitToken::Null);
        pasa(&json(vec![Rule::Enum(vec![Literal(toks)])]));
    });
}

/// m definiciones, cada una con `allOf` a la siguiente dos veces: la IR
/// tiene m + 1 nodos y 2m − 1 aristas, no 2^m copias.
#[test]
fn referencias_compartidas_tamano_lineal() {
    let m = 10_000;
    let mut ir = json(vec![Rule::Ref(id(1))]);
    for i in 1..=m {
        let rules = if i < m {
            vec![Rule::AllOf(vec![id(i + 1), id(i + 1)])]
        } else {
            vec![Rule::Type(TypeSet::of(&[JsonType::Integer]))]
        };
        ir.push(node(
            Sort::Json,
            Some(0),
            entry("$defs", &format!("d{i}")),
            rules,
        ));
    }
    pasa(&ir);
    assert_eq!(ir.nodes.len(), m + 1);
    assert_eq!(ir.edge_count(), 1 + 2 * (m - 1));
}

#[test]
fn ubicacion_escapa_segmentos() {
    let mut ir = json(vec![]);
    ir.push(node(
        Sort::Json,
        Some(0),
        entry("properties", "a/b~c"),
        vec![],
    ));
    assert_eq!(ir.location(id(1)), "#/properties/a~1b~0c");
    // `Step::Index` añade la palabra clave y el índice.
    ir.push(node(
        Sort::Json,
        Some(1),
        Step::Index("allOf".into(), 0),
        vec![],
    ));
    assert_eq!(ir.location(id(2)), "#/properties/a~1b~0c/allOf/0");
}

// --- Ronda de corrección 1 -----------------------------------------------

/// `{"enum": [1, 2], "const": 1}` es válido para el meta-esquema: `const` es
/// su propia regla y convive con `enum` en el mismo nodo (`ir.md` §5.1).
#[test]
fn enum_y_const_en_el_mismo_nodo() {
    let ir = json(vec![
        Rule::Enum(vec![
            lit(vec![LitToken::Number("1".into())]),
            lit(vec![LitToken::Number("2".into())]),
        ]),
        Rule::Const(lit(vec![LitToken::Number("1".into())])),
    ]);
    pasa(&ir);
    falla(
        &json(vec![Rule::Const(lit(vec![LitToken::Number("1.".into())]))]),
        "literal",
    );
    falla(
        &json(vec![
            Rule::Const(lit(vec![LitToken::Null])),
            Rule::Const(lit(vec![LitToken::Null])),
        ]),
        "duplicate-rule",
    );
    // XSD no tiene `const`: en un texto es de otro tipo de nodo.
    let mut ir = pedido_xsd();
    ir.nodes[6].rules.push(Constraint {
        rule: Rule::Const(lit(vec![s("a")])),
        span: sp(),
    });
    falla(&ir, "rule-sort");
}

/// I11 exige preorden de verdad: el mismo árbol en orden de anchura (cada
/// hijo posterior a su grupo y con un solo padre) no lo es.
#[test]
fn i11_orden_de_anchura_no_es_preorden() {
    let seq = |hijos: Vec<usize>| group(Term::Sequence(hijos), Occurs::ONCE);
    // seq(seq(a), b) en anchura: 0 seq[1,2], 1 seq[3], 2 b, 3 a.
    falla(
        &con_modelo(vec![
            seq(vec![1, 2]),
            seq(vec![3]),
            el("b", 3, Occurs::ONCE),
            el("a", 3, Occurs::ONCE),
        ]),
        "content-model",
    );
    // El mismo árbol en preorden: 0 seq[1,3], 1 seq[2], 2 a, 3 b.
    pasa(&con_modelo(vec![
        seq(vec![1, 3]),
        seq(vec![2]),
        el("a", 3, Occurs::ONCE),
        el("b", 3, Occurs::ONCE),
    ]));
}

#[test]
fn i6_nodo_json_en_ir_de_xml_y_contenido_simple_en_el_documento() {
    let mut ir = pedido_xsd();
    ir.push(node(Sort::Json, Some(0), kw("x"), vec![]));
    falla(&ir, "format");
    let mut ir = pedido_xsd();
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::SimpleContent(id(6)),
        span: sp(),
    });
    // También rompe I4 (`Content` y `SimpleContent` a la vez).
    assert_eq!(codes(&ir), ["format", "duplicate-rule"]);
}

/// I4 en los dos órdenes: `SimpleContent` después de `Content` también.
#[test]
fn i4_simple_content_despues_de_content() {
    let mut ir = pedido_xsd();
    ir.nodes[1].rules.push(Constraint {
        rule: Rule::SimpleContent(id(6)),
        span: sp(),
    });
    falla(&ir, "duplicate-rule");
}

/// I2 e I5 en las aristas de `SimpleContent`.
#[test]
fn i2_i5_simple_content() {
    let mut ir = pedido_xsd();
    ir.nodes[3].rules[0].rule = Rule::SimpleContent(id(99));
    falla(&ir, "dangling");
    let mut ir = pedido_xsd();
    ir.nodes[3].rules[0].rule = Rule::SimpleContent(id(5));
    falla(&ir, "target-sort");
}

#[test]
fn i8_lexemas_de_las_cotas_exclusivas() {
    falla(
        &json(vec![Rule::ExclusiveMinimum(Num::new("1."))]),
        "number",
    );
    falla(
        &json(vec![Rule::ExclusiveMaximum(Num::new("+1"))]),
        "number",
    );
}

#[test]
fn i12_all_con_max_occurs_2_o_hijo_unbounded() {
    falla(
        &con_modelo(vec![
            group(
                Term::All(vec![1]),
                Occurs {
                    min: 1,
                    max: Some(2),
                },
            ),
            el("a", 3, Occurs::ONCE),
        ]),
        "all",
    );
    falla(
        &con_modelo(vec![
            group(Term::All(vec![1]), Occurs::ONCE),
            el("a", 3, Occurs { min: 0, max: None }),
        ]),
        "all",
    );
}

/// I13: cada faceta de longitud y de rango en un texto de otro espacio
/// léxico.
#[test]
fn i13_cada_faceta_en_texto_equivocado() {
    let con = |n: usize, rule: Rule| {
        let mut ir = pedido_xsd();
        ir.nodes[n].rules.push(Constraint { rule, span: sp() });
        ir
    };
    falla(&con(7, Rule::MaxLength(2)), "text");
    falla(&con(6, Rule::Maximum(Num::new("1"))), "text");
    falla(&con(6, Rule::ExclusiveMinimum(Num::new("1"))), "text");
    falla(&con(6, Rule::ExclusiveMaximum(Num::new("1"))), "text");
    // Un booleano en una enumeración de cadena o numérica.
    falla(
        &con(6, Rule::Enum(vec![lit(vec![LitToken::Bool(true)])])),
        "text",
    );
    falla(
        &con(7, Rule::Enum(vec![lit(vec![LitToken::Bool(true)])])),
        "text",
    );
    // … ni en una de xs:boolean: en XSD 1.0 `boolean` solo admite las
    // facetas `pattern` y `whiteSpace`, no `enumeration`.
    let mut ir = pedido_xsd();
    let b = ir.push(node(
        Sort::Text,
        Some(0),
        entry("builtin", "boolean"),
        vec![
            Rule::Lexical(Lexical::Boolean),
            Rule::Enum(vec![lit(vec![LitToken::Bool(true)])]),
        ],
    ));
    ir.nodes[3].rules[0].rule = Rule::SimpleContent(b);
    falla(&ir, "text");
    // El mensaje dice por qué, también con un literal de cadena.
    ir.nodes[b.index()].rules[1].rule = Rule::Enum(vec![lit(vec![s("true")])]);
    let e = verificar(&ir).unwrap_err();
    assert!(
        e[0].to_string()
            .contains("xs:boolean no admite xs:enumeration"),
        "{}",
        e[0]
    );
    // `pattern` en xs:boolean sí vale.
    ir.nodes[b.index()].rules[1].rule = Rule::Pattern("true|false".into());
    pasa(&ir);
}

/// I13: una enumeración de un tipo entero es entera (`1.0` y `1e2` lo son,
/// como en 2020-12; `1.5` y `1e-1` no).
#[test]
fn i13_enumeracion_entera() {
    let con_enum = |lexemas: &[&str]| {
        let mut ir = pedido_xsd();
        ir.nodes[2].rules.push(Constraint {
            rule: Rule::Enum(
                lexemas
                    .iter()
                    .map(|l| lit(vec![LitToken::Number((*l).into())]))
                    .collect(),
            ),
            span: sp(),
        });
        ir
    };
    pasa(&con_enum(&[
        "1", "-0", "1.0", "1e2", "15", "150e-1", "0.0e-9", "1e400",
    ]));
    for malo in ["1.5", "1e-1", "15e-1", "0.5e0", "1e-400"] {
        falla(&con_enum(&[malo]), "text");
    }
}

/// I6: un elemento no puede tener como tipo el documento.
#[test]
fn i6_elemento_cuyo_tipo_es_el_documento() {
    let mut ir = pedido_xsd();
    ir.nodes[1].rules[0].rule = content(vec![el("raiz", 0, Occurs::ONCE)]);
    falla(&ir, "format");
}

/// Los errores salen en orden de nodo aunque el de ciclo se detecte al
/// final: nodo 1 con ciclo, nodo 2 con otro error.
#[test]
fn errores_de_ciclo_en_orden_de_nodo() {
    let mut ir = json(vec![]);
    let a = hijo(&mut ir, vec![]);
    hijo(&mut ir, vec![Rule::MinLength(1), Rule::MinLength(2)]);
    ir.nodes[a.index()].rules.push(Constraint {
        rule: Rule::Ref(a),
        span: sp(),
    });
    let got: Vec<_> = verificar(&ir)
        .unwrap_err()
        .iter()
        .map(|e| (e.node.index(), e.code()))
        .collect();
    assert_eq!(got, [(1, "cycle"), (2, "duplicate-rule")]);
}

/// I13 con una base inválida (la regla `pattern` repetida) de muchas reglas y muchas
/// restricciones que la referencian: el resultado no depende de recorrer
/// la base en cada `Ref` (la versión anterior era cuadrática); sin medir
/// tiempo (D1), se comprueba el resultado con n grande.
#[test]
fn i13_base_con_muchas_reglas() {
    let n = 20_000;
    let mut ir = pedido_xsd();
    let mut rules = vec![Rule::Lexical(Lexical::String)];
    rules.extend((0..n).map(|_| Rule::Pattern("a".into())));
    let base = ir.push(node(Sort::Text, Some(0), kw("base"), rules));
    for _ in 0..n {
        ir.push(node(
            Sort::Text,
            Some(0),
            kw("r"),
            vec![Rule::Lexical(Lexical::Decimal), Rule::Ref(base)],
        ));
    }
    let e = verificar(&ir).unwrap_err();
    // Una regla `pattern` repetida n − 1 veces en la base, y n restricciones
    // que cambian el espacio léxico.
    assert_eq!(
        e.iter().filter(|e| e.code() == "duplicate-rule").count(),
        n - 1
    );
    assert_eq!(e.iter().filter(|e| e.code() == "text").count(), n);
}

// --- Ronda de corrección 2 -----------------------------------------------

/// Preorden con un grupo de profundidad 3 seguido de un hermano: el segundo
/// hijo de la raíz empieza donde acaba el subárbol entero del primero (5),
/// no donde acaban sus hijos directos.
#[test]
fn i11_preorden_con_grupo_profundo_y_hermano() {
    let seq = |hijos: Vec<usize>| group(Term::Sequence(hijos), Occurs::ONCE);
    // seq[seq[seq[a, b]], c]
    pasa(&con_modelo(vec![
        seq(vec![1, 5]),
        seq(vec![2]),
        seq(vec![3, 4]),
        el("a", 3, Occurs::ONCE),
        el("b", 3, Occurs::ONCE),
        el("c", 3, Occurs::ONCE),
    ]));
    // El mismo árbol con `c` antes que el subárbol profundo: 0 seq[1,2],
    // 1 c, 2 seq[3], 3 seq[4,5], 4 a, 5 b es preorden; con los índices del
    // primero pero `c` en la posición 2 no lo es.
    falla(
        &con_modelo(vec![
            seq(vec![1, 5]),
            seq(vec![3]),
            el("c", 3, Occurs::ONCE),
            seq(vec![4, 2]),
            el("a", 3, Occurs::ONCE),
            el("b", 3, Occurs::ONCE),
        ]),
        "content-model",
    );
    falla(
        &con_modelo(vec![
            seq(vec![1, 2]),
            seq(vec![3]),
            el("c", 3, Occurs::ONCE),
            seq(vec![4, 5]),
            el("a", 3, Occurs::ONCE),
            el("b", 3, Occurs::ONCE),
        ]),
        "content-model",
    );
}

/// Las facetas de rango valen en `xs:float` y `xs:double`.
#[test]
fn i13_rango_en_float_y_double() {
    for lx in [Lexical::Float, Lexical::Double] {
        let mut ir = pedido_xsd();
        let t = ir.push(node(
            Sort::Text,
            Some(0),
            entry("builtin", "real"),
            vec![
                Rule::Lexical(lx),
                Rule::Minimum(Num::new("-1.5e3")),
                Rule::ExclusiveMaximum(Num::new("1e38")),
                Rule::Enum(vec![lit(vec![LitToken::Number("0.5".into())])]),
            ],
        ));
        ir.nodes[3].rules[0].rule = Rule::SimpleContent(t);
        pasa(&ir);
    }
}

/// I10 en todas las listas de `dependentRequired`, no solo en la primera.
#[test]
fn i10_repetido_en_la_segunda_lista_de_dependent_required() {
    falla(
        &json(vec![Rule::DependentRequired(vec![
            ("a".into(), names(&["x"])),
            ("b".into(), names(&["y", "y"])),
        ])]),
        "duplicate-name",
    );
}

// --- Ronda de corrección 3 -----------------------------------------------

/// I2 e I14 en las aristas de `oneOf`.
#[test]
fn one_of_colgante_y_con_ciclo() {
    falla(&json(vec![Rule::OneOf(vec![id(5)])]), "dangling");
    let mut ir = json(vec![]);
    let b = hijo(&mut ir, vec![Rule::OneOf(vec![NodeId::ROOT])]);
    ir.nodes[0].rules.push(Constraint {
        rule: Rule::OneOf(vec![b]),
        span: sp(),
    });
    falla(&ir, "cycle");
}

/// I3 entera: `RuleKind::ALL × Sort` contra la tabla de `docs/fase3/ir.md`
/// §4 («Reglas admitidas por tipo de nodo»), escrita aquí por nombre.
#[test]
fn i3_tabla_completa_de_reglas_por_tipo_de_nodo() {
    let element = ["xs:attribute", "modelo de contenido", "xs:simpleContent"];
    let text = [
        "tipo predefinido",
        "$ref",
        "enum",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "minLength",
        "maxLength",
        "pattern",
    ];
    for k in ir::RuleKind::ALL {
        let n = k.name();
        let json = !element.contains(&n) && n != "tipo predefinido";
        assert_eq!(k.allowed_in(Sort::Json), json, "{n} en Json");
        assert_eq!(
            k.allowed_in(Sort::Element),
            element.contains(&n),
            "{n} en Element"
        );
        assert_eq!(k.allowed_in(Sort::Text), text.contains(&n), "{n} en Text");
    }
    // Y el verificador lo aplica: `$ref` en un elemento es `rule-sort`.
    let mut ir = pedido_xsd();
    ir.nodes[3].rules.push(Constraint {
        rule: Rule::Ref(id(5)),
        span: sp(),
    });
    falla(&ir, "rule-sort");
}

/// Dentro de un nodo, los errores siguen el orden de las reglas, también
/// los de I13: `MinLength` (texto) antes que `Minimum` (número).
#[test]
fn errores_de_texto_en_orden_de_regla() {
    let mut ir = pedido_xsd();
    ir.nodes[2].rules = vec![
        Constraint {
            rule: Rule::Lexical(Lexical::Integer),
            span: sp(),
        },
        Constraint {
            rule: Rule::MinLength(1),
            span: sp(),
        },
        Constraint {
            rule: Rule::Minimum(Num::new("01")),
            span: sp(),
        },
    ];
    assert_eq!(codes(&ir), ["text", "number"]);
}

// --- Ronda de corrección 4 -----------------------------------------------

/// I6 en una raíz de XML de texto que además tiene modelo de contenido: el
/// error de formato no se pierde aunque la regla tampoco sea admitida.
#[test]
fn i6_raiz_de_texto_con_modelo_de_contenido() {
    let ir = Ir::new(
        Format::Xml {
            target_namespace: None,
        },
        root(
            Sort::Text,
            vec![
                Rule::Lexical(Lexical::String),
                content(vec![el("r", 0, Occurs::ONCE)]),
            ],
        ),
    );
    assert_eq!(codes(&ir), ["format", "rule-sort"]);
}

/// Códigos de `pedido_xsd` con una `xs:enumeration` de un solo número en
/// xs:int (nodo 2, espacio léxico entero).
fn enum_entero(lexema: &str) -> Vec<&'static str> {
    let mut ir = pedido_xsd();
    ir.nodes[2].rules.push(Constraint {
        rule: Rule::Enum(vec![lit(vec![LitToken::Number(lexema.into())])]),
        span: sp(),
    });
    codes(&ir)
}

/// I13: casos límite de «es entero»: signo en la mantisa (`-0e-5`),
/// exponentes que no caben en `i64` y mantisas más largas que cualquier
/// entero de máquina.
#[test]
fn i13_enumeracion_entera_casos_limite() {
    for l in [
        "0",
        "-0",
        "-0e-5",
        "0.0e-99",
        "1.0",
        "1e2",
        "10e-1",
        "100e-2",
        "0.5e1",
        "1.5e9223372036854775808",
        "10000000000000000000000000000000e-31",
    ] {
        assert!(enum_entero(l).is_empty(), "{l}: {:?}", enum_entero(l));
    }
    for l in [
        "-1.5",
        "1.25e1",
        "0.1e-9223372036854775809",
        "123456789012345678901234567890e-29",
    ] {
        assert_eq!(enum_entero(l), ["text"], "{l}");
    }
}

/// I13: el exponente se reconoce también con `E` mayúscula.
#[test]
fn i13_enumeracion_entera_con_e_mayuscula() {
    assert_eq!(enum_entero("15E-1"), ["text"]);
    assert!(enum_entero("15E1").is_empty());
}

/// I13: un exponente con `+` explícito (`1e+2` = 100) no es negativo.
#[test]
fn i13_enumeracion_entera_con_exponente_positivo() {
    for l in ["1e+2", "1E+2", "15e+0"] {
        assert!(enum_entero(l).is_empty(), "{l}: {:?}", enum_entero(l));
    }
    assert_eq!(enum_entero("15e-1"), ["text"]);
}

/// I9 e I13: un lexema que no es de RFC 8259 en una enumeración entera da
/// `literal` y `text`, sin pánico al leer su exponente.
#[test]
fn i13_enumeracion_entera_con_lexema_invalido() {
    assert_eq!(enum_entero("1e."), ["literal", "text"]);
    assert_eq!(enum_entero("1x"), ["literal", "text"]);
}

/// I8: `multipleOf` cero con `E` mayúscula en el exponente.
#[test]
fn i8_multiple_of_cero_con_e_mayuscula() {
    falla(&json(vec![Rule::MultipleOf(Num::new("0E5"))]), "number");
}

/// I11: con el árbol de partículas roto no se siguen comprobando
/// ocurrencias ni `xs:all` (indexarían fuera de rango); un hijo con índice
/// igual al número de partículas también está fuera de rango.
#[test]
fn i11_arbol_roto_no_sigue() {
    assert_eq!(
        codes(&con_modelo(vec![
            group(Term::All(vec![1, 7]), Occurs::ONCE),
            el("a", 3, Occurs::ONCE),
        ])),
        ["content-model"]
    );
    assert_eq!(
        codes(&con_modelo(vec![
            group(Term::Sequence(vec![1, 2]), Occurs::ONCE),
            el("a", 3, Occurs::ONCE),
        ])),
        ["content-model"]
    );
}

/// I11 e I12: un `xs:all` con minOccurs 2 y maxOccurs 1 rompe los dos.
#[test]
fn i12_all_con_min_occurs_2() {
    assert_eq!(
        codes(&con_modelo(vec![
            group(
                Term::All(vec![1]),
                Occurs {
                    min: 2,
                    max: Some(1),
                },
            ),
            el("a", 3, Occurs::ONCE),
        ])),
        ["content-model", "all"]
    );
}

/// Las reglas no admitidas por el tipo del nodo solo dan `rule-sort`: su
/// arista no cuenta para I13 (espacio léxico de la base) ni para I14.
#[test]
fn reglas_no_admitidas_no_cuentan_para_i13_ni_i14() {
    // Texto que restringe un elemento con `Lexical` (que no admite).
    let mut ir = pedido_xsd();
    let e = ir.push(node(
        Sort::Element,
        Some(0),
        kw("e"),
        vec![Rule::Lexical(Lexical::Decimal)],
    ));
    ir.push(node(
        Sort::Text,
        Some(0),
        kw("t"),
        vec![Rule::Lexical(Lexical::String), Rule::Ref(e)],
    ));
    assert_eq!(codes(&ir), ["rule-sort", "target-sort"]);
    // `$ref` de un elemento a sí mismo.
    let mut ir = pedido_xsd();
    ir.nodes[3].rules.push(Constraint {
        rule: Rule::Ref(id(3)),
        span: sp(),
    });
    falla(&ir, "rule-sort");
}

/// `location` corta en el primer padre que no es anterior, como promete su
/// documentación, también en la raíz.
#[test]
fn ubicacion_de_ir_invalida_termina() {
    let mut ir = json(vec![]);
    ir.nodes[0].origin.parent = Some(NodeId::ROOT);
    assert_eq!(ir.location(NodeId::ROOT), "#");
    ir.push(node(Sort::Json, Some(1), kw("a"), vec![]));
    ir.push(node(Sort::Json, Some(5), kw("b"), vec![]));
    assert_eq!(ir.location(id(1)), "#/a");
    assert_eq!(ir.location(id(2)), "#/b");
    assert_eq!(ir.location(id(9)), "#");
}

/// I14 con un ciclo que no pasa por el nodo donde empieza el recorrido:
/// 0 —allOf→ 1 —$ref→ 2 —$ref→ 1.
#[test]
fn i14_ciclo_que_no_pasa_por_el_inicio_del_recorrido() {
    let mut ir = json(vec![Rule::AllOf(vec![id(1)])]);
    hijo(&mut ir, vec![Rule::Ref(id(2))]);
    hijo(&mut ir, vec![Rule::Ref(id(1))]);
    let got: Vec<_> = verificar(&ir)
        .unwrap_err()
        .iter()
        .map(|e| (e.node.index(), e.code()))
        .collect();
    assert_eq!(got, [(2, "cycle")]);
}

// --- Revisión, ronda 1 ---------------------------------------------------

/// Un `<xs:choice/>` sin hijos y con `minOccurs` ≥ 1 es una partícula de
/// lenguaje vacío (XSD 1.0 Parte 1 §3.4.2, cláusula 2.2): baja a `Content`
/// con `Choice([])`, que ningún contenido cumple, ni siquiera el vacío. No es
/// el contenido vacío (un nodo sin `Content`), y la IR lo admite.
#[test]
fn i11_choice_vacio_con_min_occurs_1_pasa() {
    for min in [1, 3] {
        pasa(&con_modelo(vec![group(
            Term::Choice(vec![]),
            Occurs { min, max: Some(3) },
        )]));
    }
    // También anidado en una secuencia.
    pasa(&con_modelo(vec![
        group(Term::Sequence(vec![1, 2]), Occurs::ONCE),
        el("a", 3, Occurs::ONCE),
        group(Term::Choice(vec![]), Occurs::ONCE),
    ]));
}

/// Cada variante de `VerifyError` se muestra con el prefijo de posición y
/// su motivo.
#[test]
fn mensajes_de_cada_clase_de_error() {
    let con = |n: usize, rule: Rule| {
        let mut ir = pedido_xsd();
        ir.nodes[n].rules.push(Constraint { rule, span: sp() });
        ir
    };
    let mut moneda = attr("moneda", false, 6);
    moneda.default = Some("COP".into());
    moneda.fixed = Some("COP".into());
    let mut atributo = pedido_xsd();
    atributo.nodes[5].rules[1].rule = Rule::Attributes(vec![moneda]);
    let mut destino = pedido_xsd();
    destino.nodes[1].rules[1].rule = Rule::Attributes(vec![attr("id", true, 3)]);
    let mut formato = json(vec![]);
    formato.push(node(
        Sort::Text,
        Some(0),
        kw("x"),
        vec![Rule::Lexical(Lexical::String)],
    ));
    let casos: Vec<(Ir, &str, &str)> = vec![
        (
            Ir {
                format: Format::Json,
                nodes: vec![],
            },
            "origin",
            "origen: la IR no tiene nodos",
        ),
        (
            json(vec![Rule::Ref(id(9))]),
            "dangling",
            "\"$ref\" apunta al nodo 9, que no existe",
        ),
        (
            json(vec![Rule::Lexical(Lexical::String)]),
            "rule-sort",
            "\"tipo predefinido\" no se aplica a un nodo de tipo valor JSON",
        ),
        (
            json(vec![Rule::MinLength(1), Rule::MinLength(2)]),
            "duplicate-rule",
            "\"minLength\" repetida en el mismo nodo",
        ),
        (
            destino,
            "target-sort",
            "\"xs:attribute\" apunta al nodo 3, de tipo elemento XML, pero exige un nodo \
             de tipo texto XML",
        ),
        (formato, "format", "formato: "),
        (
            json(vec![Rule::AllOf(vec![])]),
            "empty",
            "\"allOf\" necesita al menos un elemento",
        ),
        (
            json(vec![Rule::Minimum(Num::new("01"))]),
            "number",
            "\"minimum\": número \"01\" ",
        ),
        (
            json(vec![Rule::Enum(vec![lit(vec![s("a")]), lit(vec![])])]),
            "literal",
            "literal 1 de \"enum\": ",
        ),
        (
            json(vec![Rule::Required(names(&["a", "a"]))]),
            "duplicate-name",
            "\"required\": nombre \"a\" repetido",
        ),
        (con_modelo(vec![]), "content-model", "modelo de contenido: "),
        (
            con_modelo(vec![
                group(Term::All(vec![1]), Occurs { min: 1, max: None }),
                el("a", 3, Occurs::ONCE),
            ]),
            "all",
            "xs:all: ",
        ),
        (
            con_modelo(vec![
                group(Term::Sequence(vec![1, 2]), Occurs::ONCE),
                el("a", 3, Occurs::ONCE),
                el("a", 4, Occurs::ONCE),
            ]),
            "element-consistency",
            "el elemento \"a\" aparece con dos tipos distintos",
        ),
        (con(7, Rule::MinLength(2)), "text", "texto: "),
        (atributo, "attribute", "atributo \"moneda\": "),
        (
            json(vec![Rule::Ref(NodeId::ROOT)]),
            "cycle",
            "ciclo de aristas sobre la misma instancia: \"$ref\" vuelve al nodo 0",
        ),
    ];
    let mut vistos = Vec::new();
    for (ir, code, motivo) in &casos {
        let e = verificar(ir).unwrap_err();
        assert_eq!(e[0].code(), *code, "{:?}", e);
        let texto = e[0].to_string();
        let prefijo = format!(
            "IR inválida: nodo {} (línea 1, columna 1): ",
            e[0].node.index()
        );
        assert!(texto.starts_with(&prefijo), "{texto}");
        assert!(texto[prefijo.len()..].starts_with(motivo), "{texto}");
        assert!(!texto.contains('\n'), "{texto}");
        vistos.push(*code);
    }
    // Una por variante de `VerifyErrorKind`, sin repetir.
    vistos.sort_unstable();
    vistos.dedup();
    assert_eq!(vistos.len(), 16);
}

// --- Tarea 4.2: `VerifiedIr` (D15) y mensajes (SA-L1) ----------------------

/// `verify` consume la IR: si pasa, la `VerifiedIr` da acceso de solo
/// lectura a la misma IR (nodos, ubicación, aristas) y `into_ir` la devuelve
/// para modificarla, que obliga a verificarla otra vez; si no, devuelve la
/// IR intacta junto a todos los errores.
#[test]
fn verified_ir_de_solo_lectura() {
    let original = arbol_json();
    let v = original.clone().verify().expect("árbol válido");
    assert_eq!(v.ir(), &original);
    assert_eq!(v.format(), &Format::Json);
    assert_eq!(v.nodes().len(), 6);
    assert_eq!(v.node(id(3)).origin.step, entry("properties", "hijos"));
    assert_eq!(v.location(id(4)), "#/properties/hijos/items");
    assert_eq!(v.edge_count(), 7);
    assert_eq!(v.into_ir(), original);

    let mala = json(vec![Rule::Ref(id(9))]);
    let (devuelta, e) = mala.clone().verify().unwrap_err();
    assert_eq!(devuelta, mala);
    assert_eq!(e.len(), 1);
}

/// SA-L1: los nombres y lexemas del usuario salen en los mensajes de
/// `VerifyError` escapados y recortados con el mismo `short` que la Fase 3
/// (32 caracteres y `…`): el mensaje es de una línea y acotado.
#[test]
fn mensajes_escapan_y_recortan_texto_del_usuario() {
    let largo = format!("a\nb\"{}", "x".repeat(100));
    let corto = format!("a\\nb\\\"{}…", "x".repeat(28));
    let casos = [
        json(vec![Rule::Minimum(Num::new(&largo))]),
        json(vec![Rule::Required(vec![largo.clone(), largo.clone()])]),
    ];
    for ir in &casos {
        let e = verificar(ir).unwrap_err();
        let texto = e[0].to_string();
        assert!(texto.contains(&format!("\"{corto}\"")), "{texto}");
        assert!(!texto.contains('\n'), "{texto}");
        assert!(texto.len() < 200, "{texto}");
    }
    // También en los de XSD: el nombre de un elemento inconsistente.
    let ir = con_modelo(vec![
        group(Term::Sequence(vec![1, 2]), Occurs::ONCE),
        el(&largo, 3, Occurs::ONCE),
        el(&largo, 4, Occurs::ONCE),
    ]);
    let texto = verificar(&ir)
        .unwrap_err()
        .iter()
        .find(|e| e.code() == "element-consistency")
        .expect("inconsistente")
        .to_string();
    assert!(texto.contains(&format!("\"{corto}\"")), "{texto}");
    assert!(!texto.contains('\n'), "{texto}");
}

/// D15: los errores de `verify` se presentan como **un** error interno de
/// texto fijo; el detalle para el registro lleva el recuento y solo los N
/// primeros, así que está acotado aunque haya muchos errores.
#[test]
fn error_interno_unico_y_acotado() {
    let mut ir = json(vec![]);
    for _ in 0..1000 {
        hijo(&mut ir, vec![Rule::Ref(id(usize::MAX))]);
    }
    let (_, errores) = ir.verify().unwrap_err();
    assert_eq!(errores.len(), 1000);
    let interno = InternalError::new(errores);
    assert_eq!(
        interno.to_string(),
        "error interno del compilador: la IR no cumple sus invariantes"
    );
    assert_eq!(interno.errors().len(), 1000);
    let detalle: Vec<String> = interno.detail(5).collect();
    assert_eq!(detalle.len(), 6, "{detalle:?}");
    assert_eq!(
        detalle[0],
        "1000 invariantes rotos; se muestran los 5 primeros"
    );
    assert!(
        detalle[1].starts_with("IR inválida: nodo 1 "),
        "{detalle:?}"
    );
    // Con menos errores que el tope se muestran todos.
    let (_, pocos) = json(vec![Rule::Ref(id(9))]).verify().unwrap_err();
    let detalle: Vec<String> = InternalError::new(pocos).detail(5).collect();
    assert_eq!(detalle.len(), 2, "{detalle:?}");
    assert_eq!(detalle[0], "1 invariante roto");
}
