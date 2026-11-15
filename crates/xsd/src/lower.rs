//! Bajada del esquema XSD analizado a la IR común (tarea 5.3,
//! `docs/fase3/xsd.md` §4; correspondencia de `docs/fase3/ir.md` §5.2).
//!
//! Entrada: un [`Analysis`], que solo sale de [`crate::analyze`] sin
//! errores. Salida: la IR **siempre verificada** ([`ir::VerifiedIr`]) o, si
//! no cumple sus invariantes, un único [`ir::InternalError`] (D15).
//!
//! **Orden de los `NodeId` (B3).** El documento (0); las declaraciones
//! (tipos complejos, tipos simples y envoltorios de los tipos simples
//! anónimos de los elementos) en el preorden de `Analysis::decls` (1 + su
//! posición); y después los nodos sintéticos, en el orden de su primer uso
//! al bajar los anteriores en orden de `NodeId` (en cada tipo complejo, las
//! partículas en preorden, después la base de `xs:simpleContent` y después
//! los atributos; en un tipo simple, su base): el envoltorio
//! `simpleContent/xs:T` (o `simpleContent/T` de un `xs:simpleType` con
//! nombre) de un tipo simple usado como tipo de elemento, seguido en su
//! primer uso del predefinido `builtin/T` al que apunta. Cada sintético se
//! crea una sola vez y solo si se usa.
//!
//! **Sin recursión (D7).** Bucles sobre las declaraciones y sobre sus arenas
//! ya aplanadas por el análisis. **Costo** O(n): un nodo por declaración,
//! una partícula por partícula, un atributo por atributo, una regla por
//! faceta (dos por `length`), a lo sumo 13 predefinidos, 12 envoltorios de
//! predefinidos y uno por tipo simple con nombre; cada uso de un tipo es
//! **una arista**, nunca una copia.

use ir::{
    Attribute, Constraint, ContentModel, Format, InternalError, Ir, Lexical, LitToken, Literal,
    Node, NodeId, Num, Occurs, Origin, Particle, Rule, Sort, Span, Step, Term as IrTerm,
    VerifiedIr,
};

use crate::ast::{ComplexContent, Compositor, Element};
use crate::check::{Analysis, Decl, Part, SimpleInfo, SimpleOrigin, Term, TypeInfo};
use crate::symbols::{Builtin, TypeRef};

/// `NodeId` imposible para un tipo que el análisis no resolvió (no ocurre
/// con un [`Analysis`] sin errores): `verify` lo rechaza (I2) y se presenta
/// como el error interno, sin pánico. Como en `semantic::lower`.
const DANGLING: NodeId = NodeId::new(usize::MAX);

/// Baja el esquema analizado a la IR y la verifica (D15).
///
/// # Errors
///
/// [`InternalError`] si la IR no cumple los invariantes de
/// `docs/fase3/ir.md` §4: un fallo de la bajada, no del esquema.
pub fn lower(analysis: &Analysis<'_>) -> Result<VerifiedIr, InternalError> {
    build(analysis)
        .verify()
        .map_err(|(_, errors)| InternalError::new(errors))
}

/// Nodo de la declaración `d` (posición en `Analysis::decls`).
fn decl_node(d: usize) -> NodeId {
    NodeId::new(1 + d)
}

/// Nodos sintéticos: predefinidos y envoltorios de contenido simple, creados
/// en su primer uso.
struct Synthetic<'s, 'a> {
    /// Primer id de los sintéticos: 1 + número de declaraciones.
    base: usize,
    simples: &'s [SimpleInfo<'a>],
    nodes: Vec<Node>,
    builtin: [Option<NodeId>; Builtin::ALL.len()],
    wrapper: [Option<NodeId>; Builtin::ALL.len()],
    /// Envoltorio `simpleContent/T` de cada `xs:simpleType` con nombre.
    user_wrapper: Vec<Option<NodeId>>,
    /// Nodo de cada tipo complejo.
    complex: Vec<NodeId>,
}

impl Synthetic<'_, '_> {
    fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        NodeId::new(self.base + self.nodes.len() - 1)
    }

    /// Nodo `Text` del predefinido `b` (`#/builtin/int`). `use_span`: el
    /// rango de su primer uso, que llevan su origen y sus reglas.
    fn builtin(&mut self, b: Builtin, use_span: Span) -> NodeId {
        if let Some(id) = self.builtin[b as usize] {
            return id;
        }
        let (lexical, min, max) = b.rules();
        let c = |rule| Constraint {
            rule,
            span: use_span,
        };
        let mut rules = vec![c(Rule::Lexical(lexical))];
        rules.extend(min.map(|n| c(Rule::Minimum(Num::new(n)))));
        rules.extend(max.map(|n| c(Rule::Maximum(Num::new(n)))));
        let id = self.push(Node {
            sort: Sort::Text,
            rules,
            origin: Origin {
                span: use_span,
                parent: Some(NodeId::ROOT),
                step: Step::Entry("builtin".to_owned(), b.local().to_owned()),
            },
        });
        self.builtin[b as usize] = Some(id);
        id
    }

    /// Nodo `Element` de contenido simple `b` sin atributos
    /// (`#/simpleContent/xs:string`), seguido de su predefinido si es el
    /// primer uso de los dos.
    fn wrapper(&mut self, b: Builtin, use_span: Span) -> NodeId {
        if let Some(id) = self.wrapper[b as usize] {
            return id;
        }
        let id = self.push(Node {
            sort: Sort::Element,
            rules: Vec::new(),
            origin: Origin {
                span: use_span,
                parent: Some(NodeId::ROOT),
                step: Step::Entry("simpleContent".to_owned(), format!("xs:{}", b.local())),
            },
        });
        self.wrapper[b as usize] = Some(id);
        let text = self.builtin(b, use_span);
        self.nodes[id.index() - self.base].rules.push(Constraint {
            rule: Rule::SimpleContent(text),
            span: use_span,
        });
        id
    }

    /// Nodo `Element` de contenido simple del `xs:simpleType` con nombre
    /// `s` (`#/simpleContent/Edad`, sin `xs:`), creado en su primer uso.
    fn user_wrapper(&mut self, s: usize, use_span: Span) -> NodeId {
        if let Some(id) = self.user_wrapper[s] {
            return id;
        }
        let info = &self.simples[s];
        let name = match info.origin {
            SimpleOrigin::Named(n) => n,
            _ => "",
        };
        let text = info.node.map_or(DANGLING, decl_node);
        let id = self.push(Node {
            sort: Sort::Element,
            rules: vec![Constraint {
                rule: Rule::SimpleContent(text),
                span: use_span,
            }],
            origin: Origin {
                span: use_span,
                parent: Some(NodeId::ROOT),
                step: Step::Entry("simpleContent".to_owned(), name.to_owned()),
            },
        });
        self.user_wrapper[s] = Some(id);
        id
    }

    /// Nodo de un tipo usado como tipo de elemento.
    fn element_type(&mut self, ty: TypeRef, use_span: Span) -> NodeId {
        match ty {
            TypeRef::Complex(i) => self.complex[i],
            TypeRef::Builtin(b) => self.wrapper(b, use_span),
            TypeRef::Simple(s) => match self.simples[s].origin {
                // Un tipo anónimo tiene un solo uso: su envoltorio es una
                // declaración (`ir.md` §5.2).
                SimpleOrigin::Element { wrapper, .. } => decl_node(wrapper),
                _ => self.user_wrapper(s, use_span),
            },
            TypeRef::Invalid => DANGLING,
        }
    }

    /// Nodo `Text` de un tipo simple (atributo, base de contenido simple o
    /// de una restricción).
    fn text_type(&mut self, ty: TypeRef, use_span: Span) -> NodeId {
        match ty {
            TypeRef::Builtin(b) => self.builtin(b, use_span),
            TypeRef::Simple(s) => self.simples[s].node.map_or(DANGLING, decl_node),
            TypeRef::Complex(_) | TypeRef::Invalid => DANGLING,
        }
    }
}

/// Rango del primer uso del tipo de un elemento: su `type`, o el elemento
/// entero si no lo tiene (`ref` o tipo anónimo).
fn type_use(el: &Element) -> Span {
    el.ty.as_ref().map_or(el.span, |t| t.span)
}

/// La IR sin verificar.
fn build(a: &Analysis<'_>) -> Ir {
    let schema = a.schema;
    let mut syn = Synthetic {
        base: 1 + a.decls.len(),
        simples: &a.simples,
        nodes: Vec::new(),
        builtin: [None; Builtin::ALL.len()],
        wrapper: [None; Builtin::ALL.len()],
        user_wrapper: vec![None; a.simples.len()],
        complex: a.types.iter().map(|t| decl_node(t.node)).collect(),
    };
    let mut nodes = Vec::with_capacity(1 + a.decls.len());

    // 0: el documento, exactamente uno de los elementos globales
    // (`ir.md` §5.2, convención del frontal).
    let mut particles = Vec::with_capacity(1 + a.globals.len());
    particles.push(Particle {
        term: IrTerm::Choice((1..=a.globals.len()).collect()),
        occurs: Occurs::ONCE,
        span: schema.span,
    });
    for g in &a.globals {
        let ty = syn.element_type(g.ty, type_use(g.el));
        particles.push(Particle {
            term: IrTerm::Element {
                name: g.el.name.as_ref().map_or("", |n| &n.value).to_owned(),
                ty,
            },
            occurs: Occurs::ONCE,
            span: g.el.span,
        });
    }
    nodes.push(Node {
        sort: Sort::Element,
        rules: vec![Constraint {
            rule: Rule::Content(ContentModel { particles }),
            span: schema.span,
        }],
        origin: Origin {
            span: schema.span,
            parent: None,
            step: Step::Root,
        },
    });

    let parent_of = |p: Option<usize>| Some(p.map_or(NodeId::ROOT, decl_node));
    for d in &a.decls {
        nodes.push(match *d {
            Decl::Complex(i) => {
                let t = &a.types[i];
                Node {
                    sort: Sort::Element,
                    rules: type_rules(t, &mut syn),
                    origin: Origin {
                        span: t.ct.span,
                        parent: parent_of(t.parent),
                        step: Step::Entry(t.step.0.to_owned(), t.step.1.to_owned()),
                    },
                }
            }
            Decl::Wrapper(s) => {
                let info = &a.simples[s];
                let (parent, name) = match info.origin {
                    SimpleOrigin::Element { parent, name, .. } => (parent, name),
                    _ => (None, ""),
                };
                let span = info.st.span;
                Node {
                    sort: Sort::Element,
                    rules: vec![Constraint {
                        rule: Rule::SimpleContent(info.node.map_or(DANGLING, decl_node)),
                        span,
                    }],
                    origin: Origin {
                        span,
                        parent: parent_of(parent),
                        step: Step::Entry("element".to_owned(), name.to_owned()),
                    },
                }
            }
            Decl::Simple(s) => simple_node(&a.simples[s], &mut syn),
        });
    }
    nodes.append(&mut syn.nodes);
    Ir {
        format: Format::Xml {
            target_namespace: schema
                .target_namespace
                .as_ref()
                .map(|t| t.value.as_str().to_owned()),
        },
        nodes,
    }
}

/// Nodo `Text` de un tipo simple (`ir.md` §5.2): `Lexical` (el primitivo),
/// `Ref` a la base si no es un primitivo, y sus facetas en un orden fijo
/// (`Enum`, cota inferior, cota superior, `MinLength`, `MaxLength`,
/// `Pattern`); `length` = n baja a `MinLength(n)` y `MaxLength(n)`, y varias
/// `xs:pattern` del paso a una sola, unidas con `|` (cada una entre
/// paréntesis).
fn simple_node(s: &SimpleInfo<'_>, syn: &mut Synthetic<'_, '_>) -> Node {
    let own = &s.own;
    let span = s.st.restriction.span;
    let mut rules = vec![Constraint {
        rule: Rule::Lexical(own.lexical),
        span,
    }];
    let base = match s.base {
        TypeRef::Builtin(b) if b.is_primitive() => None,
        t => Some(syn.text_type(t, s.base_span)),
    };
    rules.extend(base.map(|b| Constraint {
        rule: Rule::Ref(b),
        span: s.base_span,
    }));
    let mut push = |rule, span| rules.push(Constraint { rule, span });
    if let Some(span) = own.enum_span {
        let lit = |v: &String| {
            Literal(vec![if own.lexical == Lexical::String {
                LitToken::String(v.clone())
            } else {
                LitToken::Number(v.clone())
            }])
        };
        push(Rule::Enum(own.enums.iter().map(lit).collect()), span);
    }
    if let Some(b) = &own.lower {
        let n = b.num.clone();
        let rule = if b.exclusive {
            Rule::ExclusiveMinimum(n)
        } else {
            Rule::Minimum(n)
        };
        push(rule, b.span);
    }
    if let Some(b) = &own.upper {
        let n = b.num.clone();
        let rule = if b.exclusive {
            Rule::ExclusiveMaximum(n)
        } else {
            Rule::Maximum(n)
        };
        push(rule, b.span);
    }
    if let Some((n, span)) = own.length {
        push(Rule::MinLength(n), span);
        push(Rule::MaxLength(n), span);
    }
    if let Some((n, span)) = own.min_len {
        push(Rule::MinLength(n), span);
    }
    if let Some((n, span)) = own.max_len {
        push(Rule::MaxLength(n), span);
    }
    if let Some(span) = own.pattern_span {
        let source = match own.patterns.as_slice() {
            [one] => (*one).to_owned(),
            many => many
                .iter()
                .map(|p| format!("({p})"))
                .collect::<Vec<_>>()
                .join("|"),
        };
        push(Rule::Pattern(source), span);
    }
    let (parent, step) = match s.origin {
        SimpleOrigin::Named(n) => (
            NodeId::ROOT,
            Step::Entry("simpleType".to_owned(), n.to_owned()),
        ),
        SimpleOrigin::Element { wrapper, .. } => {
            (decl_node(wrapper), Step::Keyword("simpleType".to_owned()))
        }
        SimpleOrigin::Attribute { parent, name } => (
            decl_node(parent),
            Step::Entry("attribute".to_owned(), name.to_owned()),
        ),
        SimpleOrigin::Restriction { parent } => {
            (decl_node(parent), Step::Keyword("restriction".to_owned()))
        }
        // No ocurre: solo se bajan las declaraciones de `decls`.
        SimpleOrigin::Unlowered => (DANGLING, Step::Keyword("simpleType".to_owned())),
    };
    Node {
        sort: Sort::Text,
        rules,
        origin: Origin {
            span: s.st.span,
            parent: Some(parent),
            step,
        },
    }
}

/// Reglas de un tipo complejo: `Content` (o `SimpleContent`) y
/// `Attributes`, en ese orden.
fn type_rules(t: &TypeInfo<'_>, syn: &mut Synthetic<'_, '_>) -> Vec<Constraint> {
    let mut rules = Vec::new();
    if !empty_content(&t.particles) {
        let particles = t.particles.iter().map(|p| particle(p, syn)).collect();
        let span = match &t.ct.content {
            ComplexContent::Elements { model: Some(g), .. } => g.span,
            _ => t.ct.span,
        };
        rules.push(Constraint {
            rule: Rule::Content(ContentModel { particles }),
            span,
        });
    }
    if let ComplexContent::Simple(sc) = &t.ct.content {
        let (base, use_span) = t.simple_base.unwrap_or((TypeRef::Invalid, sc.base.span));
        rules.push(Constraint {
            rule: Rule::SimpleContent(syn.text_type(base, use_span)),
            span: sc.span,
        });
    }
    if let (Some(first), Some(last)) = (t.attributes.first(), t.attributes.last()) {
        let attrs = t
            .attributes
            .iter()
            .map(|at| {
                let a = at.a;
                let use_span = a.ty.as_ref().map_or(a.span, |t| t.span);
                Attribute {
                    name: a.name.value.clone(),
                    required: a
                        .use_
                        .as_ref()
                        .is_some_and(|u| u.value == crate::ast::Use::Required),
                    ty: syn.text_type(at.ty, use_span),
                    default: a.default.as_ref().map(|d| d.value.clone()),
                    fixed: a.fixed.as_ref().map(|f| f.value.clone()),
                    span: a.span,
                }
            })
            .collect();
        rules.push(Constraint {
            rule: Rule::Attributes(attrs),
            span: Span {
                start: first.a.span.start,
                end: last.a.span.end,
            },
        });
    }
    rules
}

/// Contenido vacío de XSD 1.0 Parte 1 §3.4.2, cláusulas 2.1.1-2.1.3 (`ir.md`
/// §5.2): sin modelo, o con `xs:sequence`/`xs:all` sin hijos, o con
/// `xs:choice` sin hijos y `minOccurs="0"`. Entonces no hay `Content`.
fn empty_content(particles: &[Part<'_>]) -> bool {
    match particles.first() {
        None => true,
        Some(Part {
            term: Term::Group { group, children },
            occurs,
            ..
        }) if children.is_empty() => match group.compositor {
            Compositor::Sequence | Compositor::All => true,
            Compositor::Choice => occurs.min == 0,
        },
        Some(_) => false,
    }
}

/// Una partícula de la arena del análisis en la de la IR (mismos índices).
fn particle(p: &Part<'_>, syn: &mut Synthetic<'_, '_>) -> Particle {
    let term = match &p.term {
        Term::Element { el, name, ty } => IrTerm::Element {
            name: (*name).to_owned(),
            ty: syn.element_type(*ty, type_use(el)),
        },
        Term::Group { group, children } => {
            let c = children.clone();
            match group.compositor {
                Compositor::Sequence => IrTerm::Sequence(c),
                Compositor::Choice => IrTerm::Choice(c),
                Compositor::All => IrTerm::All(c),
            }
        }
    };
    Particle {
        term,
        occurs: p.occurs,
        span: p.span,
    }
}
