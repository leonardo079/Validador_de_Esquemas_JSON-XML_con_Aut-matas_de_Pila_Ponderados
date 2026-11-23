//! Bajada de la DTD analizada a la IR común (DTD.3; `docs/fase3/dtd.md` §4),
//! con la misma forma que la de XSD (`docs/fase3/ir.md` §5.2). Sin campos
//! nuevos en la IR (D86).
//!
//! **Orden de los `NodeId`.** El documento (0); un nodo `Element` por
//! `<!ELEMENT>`, en orden de declaración (1 + su posición); y después los
//! sintéticos, en el orden de su primer uso al bajar los anteriores: el
//! predefinido `builtin/string` (de `#PCDATA` y de `CDATA`) y un nodo `Text`
//! por atributo enumerado.
//!
//! Sin recursión (D7): bucles sobre las declaraciones y sobre las arenas ya
//! aplanadas por el análisis. Costo O(n).

use ir::{
    Attribute, Constraint, ContentModel, Format, InternalError, Ir, Lexical, LitToken, Literal,
    Node, NodeId, Occurs as IrOccurs, Origin, Particle, Rule, Sort, Span, Step, Term, VerifiedIr,
};

use crate::ast::{AttType, ContentSpec, DefaultDecl, Occurs};
use crate::check::{Analysis, FlatKind};

/// Baja la DTD analizada a la IR y la verifica (D15).
///
/// # Errors
///
/// [`InternalError`] si la IR no cumple sus invariantes: un fallo de la
/// bajada, no del esquema.
pub fn lower(a: &Analysis<'_>) -> Result<VerifiedIr, InternalError> {
    build(a)
        .verify()
        .map_err(|(_, errors)| InternalError::new(errors))
}

fn element_node(i: usize) -> NodeId {
    NodeId::new(1 + i)
}

fn occurs(o: Occurs) -> IrOccurs {
    let (min, max) = match o {
        Occurs::One => (1, Some(1)),
        Occurs::Opt => (0, Some(1)),
        Occurs::Star => (0, None),
        Occurs::Plus => (1, None),
    };
    IrOccurs { min, max }
}

/// Nodos sintéticos, creados en su primer uso.
struct Synthetic {
    base: usize,
    nodes: Vec<Node>,
    string: Option<NodeId>,
}

impl Synthetic {
    fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        NodeId::new(self.base + self.nodes.len() - 1)
    }

    /// `#/builtin/string`: `CDATA` y `#PCDATA` son texto sin restricciones.
    fn string(&mut self, use_span: Span) -> NodeId {
        if let Some(id) = self.string {
            return id;
        }
        let id = self.push(Node {
            sort: Sort::Text,
            rules: vec![Constraint {
                rule: Rule::Lexical(Lexical::String),
                span: use_span,
            }],
            origin: Origin {
                span: use_span,
                parent: Some(NodeId::ROOT),
                step: Step::Entry("builtin".to_owned(), "string".to_owned()),
            },
        });
        self.string = Some(id);
        id
    }
}

/// Rango que cubre todas las declaraciones de elementos.
fn rango_documento(a: &Analysis<'_>) -> Span {
    let es = &a.dtd.elements;
    match (es.first(), es.iter().map(|e| e.span.end).max()) {
        (Some(p), Some(end)) => Span {
            start: p.span.start,
            end,
        },
        _ => Span {
            start: lexer::Pos::START,
            end: 0,
        },
    }
}

fn build(a: &Analysis<'_>) -> Ir {
    let dtd = a.dtd;
    let mut syn = Synthetic {
        base: 1 + dtd.elements.len(),
        nodes: Vec::new(),
        string: None,
    };
    let doc = rango_documento(a);

    // 0: el documento, exactamente uno de los elementos declarados (la raíz
    // puede ser cualquiera: §3bis, como los globales de XSD).
    let mut particles = Vec::with_capacity(1 + dtd.elements.len());
    particles.push(Particle {
        term: Term::Choice((1..=dtd.elements.len()).collect()),
        occurs: IrOccurs::ONCE,
        span: doc,
    });
    for (i, e) in dtd.elements.iter().enumerate() {
        particles.push(Particle {
            term: Term::Element {
                name: e.name.text.clone(),
                ty: element_node(i),
            },
            occurs: IrOccurs::ONCE,
            span: e.span,
        });
    }
    let mut nodes = vec![Node {
        sort: Sort::Element,
        rules: vec![Constraint {
            rule: Rule::Content(ContentModel { particles }),
            span: doc,
        }],
        origin: Origin {
            span: doc,
            parent: None,
            step: Step::Root,
        },
    }];

    for (i, e) in dtd.elements.iter().enumerate() {
        let mut rules = Vec::new();
        match &e.content {
            // Contenido vacío: sin `Content` ni `SimpleContent`.
            ContentSpec::Empty => {}
            ContentSpec::Pcdata => rules.push(Constraint {
                rule: Rule::SimpleContent(syn.string(e.span)),
                span: e.span,
            }),
            ContentSpec::Children(raiz) => {
                let particles = a.models[i]
                    .iter()
                    .map(|p| Particle {
                        term: match &p.kind {
                            FlatKind::Name(n) => Term::Element {
                                name: n.text.clone(),
                                ty: element_node(a.index[n.text.as_str()]),
                            },
                            FlatKind::Seq(hs) => Term::Sequence(hs.clone()),
                            FlatKind::Choice(hs) => Term::Choice(hs.clone()),
                        },
                        occurs: occurs(p.occurs),
                        span: p.span,
                    })
                    .collect();
                rules.push(Constraint {
                    rule: Rule::Content(ContentModel { particles }),
                    span: raiz.span,
                });
            }
        }
        let defs = &a.attributes[i];
        if let (Some(first), Some(last)) = (defs.first(), defs.last()) {
            let attrs = defs
                .iter()
                .map(|d| {
                    let ty = match &d.ty {
                        AttType::Cdata => syn.string(d.name.span),
                        AttType::Enumeration(vs) => syn.push(Node {
                            sort: Sort::Text,
                            rules: vec![
                                Constraint {
                                    rule: Rule::Lexical(Lexical::String),
                                    span: d.name.span,
                                },
                                Constraint {
                                    rule: Rule::Enum(
                                        vs.iter()
                                            .map(|v| {
                                                Literal(vec![LitToken::String(v.text.clone())])
                                            })
                                            .collect(),
                                    ),
                                    span: d.name.span,
                                },
                            ],
                            origin: Origin {
                                span: d.name.span,
                                parent: Some(element_node(i)),
                                step: Step::Entry("attribute".to_owned(), d.name.text.clone()),
                            },
                        }),
                    };
                    let (default, fixed) = match &d.default {
                        DefaultDecl::Default(l) => (Some(l.value.clone()), None),
                        DefaultDecl::Fixed(l) => (None, Some(l.value.clone())),
                        DefaultDecl::Required | DefaultDecl::Implied => (None, None),
                    };
                    Attribute {
                        name: d.name.text.clone(),
                        required: d.default == DefaultDecl::Required,
                        ty,
                        default,
                        fixed,
                        span: d.name.span,
                    }
                })
                .collect();
            rules.push(Constraint {
                rule: Rule::Attributes(attrs),
                span: Span {
                    start: first.name.span.start,
                    end: last.name.span.end,
                },
            });
        }
        nodes.push(Node {
            sort: Sort::Element,
            rules,
            origin: Origin {
                span: e.span,
                parent: Some(NodeId::ROOT),
                step: Step::Entry("element".to_owned(), e.name.text.clone()),
            },
        });
    }
    nodes.append(&mut syn.nodes);
    Ir {
        format: Format::Xml {
            target_namespace: None,
        },
        nodes,
    }
}
