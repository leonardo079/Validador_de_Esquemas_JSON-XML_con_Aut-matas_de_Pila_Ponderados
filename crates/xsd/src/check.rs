//! Análisis del esquema XSD (tarea 5.3, `docs/fase3/xsd.md`): tabla de
//! símbolos, resolución de `type`/`ref`/`base` y restricciones de
//! componente que la IR exige, todas con su posición y antes de bajar
//! (D15: `Ir::verify` queda como defensa en profundidad).
//!
//! **Por etapas (B2).** Como `semantic::analyze`, se devuelven **todos** los
//! errores de la primera etapa que falla, en orden del documento:
//!
//! 1. *componentes y nombres*: forma de `name` (NCName) y de las
//!    ocurrencias, combinaciones de atributos (`name`/`ref`, `type`/tipo
//!    anónimo, `default`/`fixed`, `use="required"` con `default`), D14,
//!    nombres repetidos (globales e I10), referencias sin resolver (también
//!    la `base` de los `xs:restriction`), tipos fuera de §3.2, D27 y
//!    `base` con tipo anónimo a la vez (o ninguno);
//! 2. *tipos simples* (tarea 5.6, [`crate::simple`]): derivación circular,
//!    facetas (I4, I13, *valid restriction*), enumeraciones y
//!    `default`/`fixed` como valores del tipo. Necesitan las bases
//!    resueltas de la etapa 1;
//! 3. *modelos de contenido*: `minOccurs` ≤ `maxOccurs` (I11), *all-group
//!    limited* y *Element Declarations Consistent* (I12). Necesitan los
//!    tipos resueltos de la etapa 1.
//!
//! **Sin recursión (D7).** El AST es un árbol de profundidad ≤ 256, pero se
//! recorre con pilas explícitas: una de declaraciones pendientes (tipos
//! complejos y simples, que da el preorden de las declaraciones, B3) y,
//! dentro de cada tipo complejo, otra para aplanar su modelo de contenido en
//! una arena en preorden. Cada construcción del AST se visita una vez en el
//! recorrido y otra en la comprobación de su arena: O(n) en el tamaño del
//! AST, más O(1) esperado por consulta de las tablas hash (más el hash del
//! nombre local).

use std::collections::HashMap;

use ir::Occurs;
use lexer::{Pos, Span};

use crate::ast::{
    AnonymousType, Attribute, ComplexContent, ComplexType, Compositor, Element, Group, Particle,
    Schema, SimpleType, Spanned, TopLevel, Use,
};
use crate::error::{clip, ErrorKind, XsdError};
use crate::reader::is_ncname;
use crate::simple::{self, Own};
use crate::symbols::{key, Builtin, Key, Lookup, Symbols, TypeRef};
use crate::Name;

/// Un esquema que pasó el análisis: lo único que acepta [`crate::lower`].
/// Guarda referencias al AST, así que vive lo que él.
pub struct Analysis<'a> {
    pub(crate) schema: &'a Schema,
    /// Tipos complejos (con nombre y anónimos) en **preorden** de
    /// declaración.
    pub(crate) types: Vec<TypeInfo<'a>>,
    /// Tipos simples del usuario (con nombre y anónimos), en preorden de
    /// declaración; también los de los atributos globales, que no se bajan.
    pub(crate) simples: Vec<SimpleInfo<'a>>,
    /// Declaraciones que se bajan, en **preorden** (B3): la *i*-ésima es el
    /// nodo 1 + *i*.
    pub(crate) decls: Vec<Decl>,
    /// Elementos globales en el orden del documento.
    pub(crate) globals: Vec<Global<'a>>,
}

impl Analysis<'_> {
    /// Número de tipos complejos (con nombre y anónimos), sin los simples
    /// ni el documento ni los sintéticos.
    pub fn type_count(&self) -> usize {
        self.types.len()
    }
}

/// `Debug` con recuentos, no el AST (5.3-CR1-S3): el derivado imprimía cada
/// tipo con sus anónimos, O(n·d) y recursivo.
impl core::fmt::Debug for Analysis<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Analysis")
            .field("tipos_complejos", &self.types.len())
            .field("tipos_simples", &self.simples.len())
            .field("declaraciones", &self.decls.len())
            .field("elementos_globales", &self.globals.len())
            .finish()
    }
}

/// Una declaración que se baja a un nodo de la IR.
#[derive(Clone, Copy)]
pub(crate) enum Decl {
    /// Tipo complejo (posición en `Analysis::types`).
    Complex(usize),
    /// Envoltorio `Element` del tipo simple anónimo de un elemento
    /// (posición en `Analysis::simples`); le sigue el tipo.
    Wrapper(usize),
    /// Tipo simple (posición en `Analysis::simples`): un nodo `Text`.
    Simple(usize),
}

/// Un `xs:simpleType` del usuario.
pub(crate) struct SimpleInfo<'a> {
    pub(crate) st: &'a SimpleType,
    /// Su nodo `Text` (posición en `Analysis::decls`); `None` si no se baja
    /// (el tipo de un atributo global y sus bases anónimas).
    pub(crate) node: Option<usize>,
    pub(crate) origin: SimpleOrigin<'a>,
    /// Base resuelta en la etapa 1: un predefinido o otro tipo simple.
    pub(crate) base: TypeRef,
    /// Rango del uso de la base: el valor de `base` o el `xs:simpleType`
    /// anónimo (para el primer uso de un predefinido, B3).
    pub(crate) base_span: Span,
    /// La base anónima (`TypeRef::Simple`), si la tiene.
    pub(crate) anon_base: TypeRef,
    /// Facetas propias ya comprobadas (etapa 2), las que se bajan.
    pub(crate) own: Own<'a>,
}

/// De dónde cuelga el nodo de un tipo simple (`docs/fase3/ir.md` §5.2).
#[derive(Clone, Copy)]
pub(crate) enum SimpleOrigin<'a> {
    /// `xs:simpleType name="T"`: padre el documento, `Entry("simpleType", T)`.
    Named(&'a str),
    /// Tipo anónimo del elemento `name`: el envoltorio (nodo `wrapper`, padre
    /// `parent` o el documento, `Entry("element", name)`) y, colgando de él,
    /// el texto con `Keyword("simpleType")`.
    Element {
        wrapper: usize,
        parent: Option<usize>,
        name: &'a str,
    },
    /// Tipo anónimo del atributo `name` del tipo complejo de nodo `parent`:
    /// `Entry("attribute", name)`.
    Attribute { parent: usize, name: &'a str },
    /// Base anónima de la restricción del tipo de nodo `parent`:
    /// `Keyword("restriction")`.
    Restriction { parent: usize },
    /// No se baja.
    Unlowered,
}

/// Un elemento global: una partícula del documento.
pub(crate) struct Global<'a> {
    pub(crate) el: &'a Element,
    pub(crate) ty: TypeRef,
}

/// Un tipo complejo con su contenido aplanado y resuelto.
pub(crate) struct TypeInfo<'a> {
    pub(crate) ct: &'a ComplexType,
    /// Su nodo (posición en `Analysis::decls`).
    pub(crate) node: usize,
    /// Nodo de la declaración que la contiene (`None`: el documento).
    pub(crate) parent: Option<usize>,
    /// Paso desde el padre: `("complexType", nombre)` o, si es anónimo,
    /// `("element", nombre del elemento)`.
    pub(crate) step: (&'static str, &'a str),
    /// Modelo de contenido en preorden (vacío si no tiene).
    pub(crate) particles: Vec<Part<'a>>,
    /// Atributos (de `xs:complexType` o de `xs:extension`).
    pub(crate) attributes: Vec<Attr<'a>>,
    /// Tipo de `xs:simpleContent`/`xs:extension base` y el rango del uso.
    pub(crate) simple_base: Option<(TypeRef, Span)>,
    /// Tipo anónimo de cada atributo (`TypeRef::Invalid` si no tiene).
    anon_attrs: Vec<TypeRef>,
}

/// Partícula del modelo aplanado (el índice de los hijos es el de la
/// arena, como en `ir::ContentModel`).
pub(crate) struct Part<'a> {
    pub(crate) term: Term<'a>,
    pub(crate) occurs: Occurs,
    pub(crate) span: Span,
}

pub(crate) enum Term<'a> {
    Element {
        el: &'a Element,
        /// Nombre local de la partícula (el de la declaración global si es
        /// un `ref`).
        name: &'a str,
        ty: TypeRef,
    },
    Group {
        group: &'a Group,
        children: Vec<usize>,
    },
}

/// Un `xs:attribute` con su tipo resuelto.
pub(crate) struct Attr<'a> {
    pub(crate) a: &'a Attribute,
    pub(crate) ty: TypeRef,
}

/// Errores de cada etapa (B2).
#[derive(Default)]
pub(crate) struct Errors {
    /// Etapa 1: componentes y nombres.
    components: Vec<XsdError>,
    /// Etapa 2: tipos simples.
    pub(crate) simples: Vec<XsdError>,
    /// Etapa 3: modelos de contenido.
    models: Vec<XsdError>,
}

impl Errors {
    /// Un error de la etapa 1 (componentes y nombres).
    fn component(&mut self, kind: ErrorKind, at: Pos) {
        self.components.push(XsdError::new(kind, at));
    }

    /// Un error de la etapa 2 (tipos simples).
    pub(crate) fn simple(&mut self, kind: ErrorKind, at: Pos) {
        self.simples.push(XsdError::new(kind, at));
    }

    /// Un error de la etapa 3 (modelos de contenido).
    fn model(&mut self, kind: ErrorKind, at: Pos) {
        self.models.push(XsdError::new(kind, at));
    }
}

/// Declaración encontrada y aún sin número de preorden.
struct Pending<'a> {
    item: Item<'a>,
    slot: Slot,
}

enum Item<'a> {
    Complex {
        ct: &'a ComplexType,
        /// Nodo de la declaración que la contiene (`None`: el documento).
        parent: Option<usize>,
        step: (&'static str, &'a str),
    },
    Simple {
        st: &'a SimpleType,
        place: Place<'a>,
    },
}

/// Dónde está un `xs:simpleType` (da su `SimpleOrigin` al numerarlo).
enum Place<'a> {
    Named(&'a str),
    /// Tipo anónimo del elemento `name`, dentro del nodo `parent`.
    Element {
        parent: Option<usize>,
        name: &'a str,
    },
    /// Tipo anónimo del atributo `name` del tipo complejo de nodo `parent`.
    Attribute {
        parent: usize,
        name: &'a str,
    },
    /// Base anónima de una restricción; `None` si su tipo no se baja.
    Restriction {
        parent: Option<usize>,
    },
    /// Tipo de un atributo global: no se baja.
    Unlowered,
}

/// Dónde se anota el tipo al numerarlo.
enum Slot {
    /// Declaración global: posición en `Schema::items`.
    Named(usize),
    /// Tipo anónimo de un elemento global (posición en `globals`).
    Global(usize),
    /// Tipo anónimo de la partícula `part` del tipo `ty`.
    Particle { ty: usize, part: usize },
    /// Tipo anónimo del atributo `attr` del tipo `ty`.
    Attribute { ty: usize, attr: usize },
    /// Tipo anónimo de un atributo global (posición en `Schema::items`).
    GlobalAttribute(usize),
    /// Base anónima del tipo simple `simple`.
    Restriction(usize),
}

/// Lo que da el recorrido de [`discover`].
struct Found<'a> {
    types: Vec<TypeInfo<'a>>,
    simples: Vec<SimpleInfo<'a>>,
    decls: Vec<Decl>,
    globals: Vec<Global<'a>>,
    /// Tipo de cada declaración global de tipo, por su posición en
    /// `Schema::items`.
    named: Vec<Option<TypeRef>>,
    /// Tipo anónimo de cada atributo global, por su posición en
    /// `Schema::items`.
    global_attr_anon: Vec<TypeRef>,
}

const LOCAL_ELEMENT: &str = "xs:element local";
const GLOBAL_ELEMENT: &str = "xs:element global";
const REF_ELEMENT: &str = "xs:element ref";
const ATTRIBUTE: &str = "xs:attribute";
const RESTRICTION: &str = "xs:restriction";

/// Texto de un `name` opcional (vacío si falta; el parser lo exige en las
/// declaraciones globales).
fn text(name: &Option<Spanned<String>>) -> &str {
    name.as_ref().map_or("", |n| n.value.as_str())
}

/// Analiza el esquema: tabla de símbolos, resolución y restricciones de
/// componente. Devuelve el análisis o **todos** los errores de la primera
/// etapa que falla, en orden del documento (ver el módulo).
///
/// No recurre sobre el AST (D7), pero el AST ya existe: quien lo llama lo
/// tiene en un hilo con `stack_size` suficiente para su liberación
/// (`docs/fase2/pendientes.md` §5).
pub fn analyze(schema: &Schema) -> Result<Analysis<'_>, Vec<XsdError>> {
    let mut errors = Errors::default();
    let Found {
        mut types,
        mut simples,
        decls,
        mut globals,
        named,
        global_attr_anon,
    } = discover(schema);
    let symbols = symbols(schema, &named, &mut errors);

    // Tipos de los elementos globales (antes que las partículas: un `ref`
    // toma el tipo de su declaración global).
    for g in &mut globals {
        g.ty = element_type(g.el, g.ty, GLOBAL_ELEMENT, &symbols, &mut errors);
    }
    for t in &mut types {
        check_type(t, &globals, &symbols, &mut errors);
    }
    let mut global_attrs = Vec::new();
    for (i, item) in schema.items.iter().enumerate() {
        // 5.2-CR-S2: sin `ref` de atributo no se pueden usar, así que no se
        // bajan; se comprueban como cualquier declaración (XSD 1.0).
        if let TopLevel::Attribute(a) = item {
            let ty = attribute(a, global_attr_anon[i], &symbols, &mut errors);
            global_attrs.push(Attr { a, ty });
        }
    }
    for s in &mut simples {
        restriction_base(s, &symbols, &mut errors);
    }
    if errors.components.is_empty() {
        simple::check(&mut simples, &types, &global_attrs, &mut errors);
    }
    for t in &types {
        check_model(t, &mut errors);
    }

    for v in [
        &mut errors.components,
        &mut errors.simples,
        &mut errors.models,
    ] {
        v.sort_by_key(|e| e.at.offset);
    }
    for v in [errors.components, errors.simples, errors.models] {
        if !v.is_empty() {
            return Err(v);
        }
    }
    Ok(Analysis {
        schema,
        types,
        simples,
        decls,
        globals,
    })
}

/// La base de la restricción de un tipo simple: `base` o un `xs:simpleType`
/// hijo, exactamente uno (XSD 1.0, *src-simple-type* 2); un tipo complejo
/// no puede ser base de un tipo simple.
fn restriction_base(s: &mut SimpleInfo<'_>, symbols: &Symbols<'_>, errors: &mut Errors) {
    let r = &s.st.restriction;
    let conflict = |problem, at, errors: &mut Errors| {
        errors.component(
            ErrorKind::Conflict {
                construct: RESTRICTION,
                name: String::new(),
                problem,
            },
            at,
        );
    };
    s.base = match (&r.base, &r.simple_type) {
        (Some(_), Some(st)) => {
            conflict(
                "no puede tener base y un xs:simpleType hijo a la vez",
                st.span.start,
                errors,
            );
            TypeRef::Invalid
        }
        (None, None) => {
            conflict(
                "necesita base o un xs:simpleType hijo",
                r.span.start,
                errors,
            );
            TypeRef::Invalid
        }
        (None, Some(st)) => {
            s.base_span = st.span;
            s.anon_base
        }
        (Some(b), None) => {
            s.base_span = b.span;
            match symbols.lookup_type(&b.value) {
                Lookup::Found(TypeRef::Complex(_)) => {
                    errors.component(
                        ErrorKind::NotSimpleType {
                            name: clip(b.value.as_str()),
                            context: "la base de un xs:simpleType",
                        },
                        b.at(),
                    );
                    TypeRef::Invalid
                }
                Lookup::Found(t) => t,
                other => {
                    type_error(b, other, errors);
                    TypeRef::Invalid
                }
            }
        }
    };
}

/// Recorre el AST con una pila de declaraciones pendientes y da a cada una
/// su número de **preorden** (B3): las globales (`xs:complexType` y
/// `xs:simpleType` con nombre, tipos anónimos de los elementos globales) en
/// el orden del documento y, antes que los hermanos siguientes, las que
/// contiene cada una: los tipos anónimos de su modelo, después los de sus
/// atributos y, en un tipo simple, su base anónima. Aplana cada modelo al
/// llegar a su tipo. Los tipos de los atributos globales se recorren igual,
/// sin número: no se bajan.
fn discover(schema: &Schema) -> Found<'_> {
    let mut f = Found {
        types: Vec::new(),
        simples: Vec::new(),
        decls: Vec::new(),
        globals: Vec::new(),
        named: vec![None; schema.items.len()],
        global_attr_anon: vec![TypeRef::Invalid; schema.items.len()],
    };
    let mut stack: Vec<Pending<'_>> = Vec::new();
    for (i, item) in schema.items.iter().enumerate() {
        match item {
            TopLevel::ComplexType(ct) => stack.push(Pending {
                item: Item::Complex {
                    ct,
                    parent: None,
                    step: ("complexType", text(&ct.name)),
                },
                slot: Slot::Named(i),
            }),
            TopLevel::SimpleType(st) => stack.push(Pending {
                item: Item::Simple {
                    st,
                    place: Place::Named(text(&st.name)),
                },
                slot: Slot::Named(i),
            }),
            TopLevel::Element(el) => {
                let slot = Slot::Global(f.globals.len());
                match &el.anonymous {
                    // S-5, primera mitad (B3): padre el documento.
                    Some(AnonymousType::Complex(ct)) => stack.push(Pending {
                        item: Item::Complex {
                            ct,
                            parent: None,
                            step: ("element", text(&el.name)),
                        },
                        slot,
                    }),
                    Some(AnonymousType::Simple(st)) => stack.push(Pending {
                        item: Item::Simple {
                            st,
                            place: Place::Element {
                                parent: None,
                                name: text(&el.name),
                            },
                        },
                        slot,
                    }),
                    None => {}
                }
                f.globals.push(Global {
                    el,
                    ty: TypeRef::Invalid,
                });
            }
            TopLevel::Attribute(a) => {
                if let Some(st) = &a.simple_type {
                    stack.push(Pending {
                        item: Item::Simple {
                            st,
                            place: Place::Unlowered,
                        },
                        slot: Slot::GlobalAttribute(i),
                    });
                }
            }
        }
    }
    stack.reverse();
    let mut found = Vec::new();
    while let Some(p) = stack.pop() {
        let t = match p.item {
            Item::Complex { ct, parent, step } => {
                let idx = f.types.len();
                let node = f.decls.len();
                f.decls.push(Decl::Complex(idx));
                let (particles, attributes) = match &ct.content {
                    ComplexContent::Elements { model, attributes } => (
                        model
                            .as_ref()
                            .map_or_else(Vec::new, |g| flatten(idx, node, g, &mut found)),
                        attributes,
                    ),
                    ComplexContent::Simple(sc) => (Vec::new(), &sc.attributes),
                };
                for (k, a) in attributes.iter().enumerate() {
                    if let Some(st) = &a.simple_type {
                        found.push(Pending {
                            item: Item::Simple {
                                st,
                                place: Place::Attribute {
                                    parent: node,
                                    name: &a.name.value,
                                },
                            },
                            slot: Slot::Attribute { ty: idx, attr: k },
                        });
                    }
                }
                f.types.push(TypeInfo {
                    ct,
                    node,
                    parent,
                    step,
                    particles,
                    attributes: Vec::new(),
                    simple_base: None,
                    anon_attrs: vec![TypeRef::Invalid; attributes.len()],
                });
                TypeRef::Complex(idx)
            }
            Item::Simple { st, place } => {
                let idx = f.simples.len();
                let mut number = |d| {
                    f.decls.push(d);
                    f.decls.len() - 1
                };
                let (node, origin) = match place {
                    Place::Named(n) => (Some(number(Decl::Simple(idx))), SimpleOrigin::Named(n)),
                    Place::Element { parent, name } => {
                        let wrapper = number(Decl::Wrapper(idx));
                        (
                            Some(number(Decl::Simple(idx))),
                            SimpleOrigin::Element {
                                wrapper,
                                parent,
                                name,
                            },
                        )
                    }
                    Place::Attribute { parent, name } => (
                        Some(number(Decl::Simple(idx))),
                        SimpleOrigin::Attribute { parent, name },
                    ),
                    Place::Restriction {
                        parent: Some(parent),
                    } => (
                        Some(number(Decl::Simple(idx))),
                        SimpleOrigin::Restriction { parent },
                    ),
                    Place::Restriction { parent: None } | Place::Unlowered => {
                        (None, SimpleOrigin::Unlowered)
                    }
                };
                if let Some(child) = &st.restriction.simple_type {
                    found.push(Pending {
                        item: Item::Simple {
                            st: child,
                            place: Place::Restriction { parent: node },
                        },
                        slot: Slot::Restriction(idx),
                    });
                }
                f.simples.push(SimpleInfo {
                    st,
                    node,
                    origin,
                    base: TypeRef::Invalid,
                    base_span: st.restriction.span,
                    anon_base: TypeRef::Invalid,
                    own: Own::default(),
                });
                TypeRef::Simple(idx)
            }
        };
        match p.slot {
            Slot::Named(i) => f.named[i] = Some(t),
            Slot::Global(g) => f.globals[g].ty = t,
            Slot::Particle { ty, part } => {
                if let Term::Element { ty: slot, .. } = &mut f.types[ty].particles[part].term {
                    *slot = t;
                }
            }
            Slot::Attribute { ty, attr } => f.types[ty].anon_attrs[attr] = t,
            Slot::GlobalAttribute(i) => f.global_attr_anon[i] = t,
            Slot::Restriction(s) => f.simples[s].anon_base = t,
        }
        // Los hijos, en orden del documento: se apilan del revés.
        stack.extend(found.drain(..).rev());
    }
    f
}

/// Aplana el modelo `root` del tipo `ty` (nodo `node`) en una arena en
/// **preorden** con una pila explícita (D7): cada partícula se añade al
/// sacarla de la pila y se anota como hija de su grupo, que ya está en la
/// arena; los hijos se apilan del revés, así que salen en orden. Deja en
/// `found` los tipos anónimos (complejos y simples) de sus elementos, en
/// orden del documento.
fn flatten<'a>(
    ty: usize,
    node: usize,
    root: &'a Group,
    found: &mut Vec<Pending<'a>>,
) -> Vec<Part<'a>> {
    enum Item<'a> {
        Group(&'a Group),
        Element(&'a Element),
    }
    let mut parts: Vec<Part<'a>> = Vec::new();
    let mut stack = vec![(Item::Group(root), None::<usize>)];
    while let Some((item, parent)) = stack.pop() {
        let i = parts.len();
        if let Some(Part {
            term: Term::Group { children, .. },
            ..
        }) = parent.map(|p| &mut parts[p])
        {
            children.push(i);
        }
        match item {
            Item::Group(group) => {
                parts.push(Part {
                    term: Term::Group {
                        group,
                        children: Vec::with_capacity(group.particles.len()),
                    },
                    occurs: Occurs::ONCE,
                    span: group.span,
                });
                stack.extend(group.particles.iter().rev().map(|p| {
                    let item = match p {
                        Particle::Element(e) => Item::Element(e),
                        Particle::Group(g) => Item::Group(g),
                    };
                    (item, Some(i))
                }));
            }
            Item::Element(el) => {
                parts.push(Part {
                    term: Term::Element {
                        el,
                        name: text(&el.name),
                        ty: TypeRef::Invalid,
                    },
                    occurs: Occurs::ONCE,
                    span: el.span,
                });
                let slot = Slot::Particle { ty, part: i };
                match &el.anonymous {
                    Some(AnonymousType::Complex(ct)) => found.push(Pending {
                        item: self::Item::Complex {
                            ct,
                            parent: Some(node),
                            step: ("element", text(&el.name)),
                        },
                        slot,
                    }),
                    Some(AnonymousType::Simple(st)) => found.push(Pending {
                        item: self::Item::Simple {
                            st,
                            place: Place::Element {
                                parent: Some(node),
                                name: text(&el.name),
                            },
                        },
                        slot,
                    }),
                    None => {}
                }
            }
        }
    }
    parts
}

/// `name` como NCName (XSD 1.0: el valor de `name` es un `xs:NCName`).
fn ncname(name: &Spanned<String>, errors: &mut Errors) -> bool {
    let ok = is_ncname(&name.value);
    if !ok {
        errors.component(
            ErrorKind::InvalidValue {
                attr: "name",
                expected: "un NCName (nombre sin prefijo)",
                found: clip(&name.value),
            },
            name.at(),
        );
    }
    ok
}

/// Inserta una declaración global en su espacio de símbolos; si el nombre
/// ya está, error `duplicate-name` en el nombre de la segunda.
fn declare<'a, V>(
    map: &mut HashMap<Key<'a>, V>,
    k: Key<'a>,
    v: V,
    first: impl Fn(&V) -> Pos,
    space: &'static str,
    name: &Spanned<String>,
    errors: &mut Errors,
) {
    match map.get(&k) {
        Some(prev) => errors.component(
            ErrorKind::DuplicateName {
                space,
                name: clip(&name.value),
                first: first(prev),
            },
            name.at(),
        ),
        None => {
            map.insert(k, v);
        }
    }
}

/// Las tres tablas de símbolos globales, por `(targetNamespace, name)`.
fn symbols<'a>(schema: &'a Schema, named: &[Option<TypeRef>], errors: &mut Errors) -> Symbols<'a> {
    let tns = schema.target_namespace.as_ref().map(|t| &t.value);
    let mut s = Symbols::default();
    let mut g = 0;
    for (i, item) in schema.items.iter().enumerate() {
        match item {
            TopLevel::Element(el) => {
                if let Some(n) = &el.name {
                    if ncname(n, errors) {
                        let at = n.at();
                        declare(
                            &mut s.elements,
                            (tns, &n.value),
                            (g, at),
                            |p| p.1,
                            "elemento global",
                            n,
                            errors,
                        );
                    }
                }
                g += 1;
            }
            TopLevel::ComplexType(ct) => {
                if let Some(n) = &ct.name {
                    if ncname(n, errors) {
                        let t = named[i].unwrap_or(TypeRef::Invalid);
                        let at = n.at();
                        declare(
                            &mut s.types,
                            (tns, &n.value),
                            (t, at),
                            |p| p.1,
                            "tipo",
                            n,
                            errors,
                        );
                    }
                }
            }
            TopLevel::SimpleType(st) => {
                if let Some(n) = &st.name {
                    if ncname(n, errors) {
                        let at = n.at();
                        let v = (named[i].unwrap_or(TypeRef::Invalid), at);
                        declare(&mut s.types, (tns, &n.value), v, |p| p.1, "tipo", n, errors);
                    }
                }
            }
            TopLevel::Attribute(a) => {
                // El NCName del nombre lo comprueba `attribute`.
                if is_ncname(&a.name.value) {
                    let at = a.name.at();
                    declare(
                        &mut s.attributes,
                        (tns, &a.name.value),
                        at,
                        |p| *p,
                        "atributo global",
                        &a.name,
                        errors,
                    );
                }
            }
        }
    }
    s
}

/// Error de un `type`/`base` sin resolver o fuera de §3.2.
fn type_error(name: &Spanned<Name>, lookup: Lookup, errors: &mut Errors) {
    let kind = match lookup {
        Lookup::Unsupported(category, reason) => ErrorKind::Unsupported {
            what: "tipo",
            name: clip(name.value.as_str()),
            category,
            reason,
        },
        _ => ErrorKind::Unresolved {
            what: "tipo",
            name: clip(name.value.as_str()),
            ns: name.value.ns().map(clip),
        },
    };
    errors.component(kind, name.at());
}

/// Rango de un tipo anónimo.
fn anonymous_span(t: &AnonymousType) -> Span {
    match t {
        AnonymousType::Complex(ct) => ct.span,
        AnonymousType::Simple(st) => st.span,
    }
}

/// Tipo de un `xs:element` con nombre (global o local): `type`, tipo
/// anónimo (`anonymous`, ya numerado) o ninguno (D14).
fn element_type(
    el: &Element,
    anonymous: TypeRef,
    construct: &'static str,
    symbols: &Symbols<'_>,
    errors: &mut Errors,
) -> TypeRef {
    let name = text(&el.name);
    match (&el.ty, &el.anonymous) {
        (Some(_), Some(anon)) => {
            errors.component(
                ErrorKind::Conflict {
                    construct,
                    name: clip(name),
                    problem: "no puede tener type y un tipo anónimo a la vez",
                },
                anonymous_span(anon).start,
            );
            TypeRef::Invalid
        }
        (Some(ty), None) => match symbols.lookup_type(&ty.value) {
            Lookup::Found(t) => t,
            other => {
                type_error(ty, other, errors);
                TypeRef::Invalid
            }
        },
        (None, Some(_)) => anonymous,
        // D14: el tipo implícito sería `xs:anyType`.
        (None, None) => {
            errors.component(
                ErrorKind::Unsupported {
                    what: "xs:element",
                    name: clip(name),
                    category: "xs:anyType",
                    reason: "sin type, ref ni tipo anónimo, su tipo es xs:anyType, que admite \
                             contenido mixto, elementos no declarados y cualquier atributo \
                             (como xs:any, xs:anyAttribute y mixed=\"true\", 00-alcance.md \
                             §3.3; D14)",
                },
                el.span.start,
            );
            TypeRef::Invalid
        }
    }
}

/// Un `minOccurs` o `maxOccurs`: `xs:nonNegativeInteger` (con signo
/// opcional, `-` solo si vale cero) saturado en `u64::MAX` (`ir.md` §5.1), o
/// `unbounded` en `maxOccurs` (`None`). El valor ya viene con *collapse*.
/// `Err(())` si no tiene esa forma.
pub(crate) fn occurs_value(v: &str, unbounded: bool) -> Result<Option<u64>, ()> {
    if unbounded && v == "unbounded" {
        return Ok(None);
    }
    let (neg, digits) = match v.as_bytes().first() {
        Some(b'+') => (false, &v[1..]),
        Some(b'-') => (true, &v[1..]),
        _ => (false, v),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(());
    }
    if neg && digits.bytes().any(|b| b != b'0') {
        return Err(());
    }
    let n = digits.bytes().fold(0u64, |n, d| {
        n.saturating_mul(10).saturating_add(u64::from(d - b'0'))
    });
    Ok(Some(n))
}

/// Ocurrencias de una partícula; un valor mal formado es un error `value`
/// y cuenta como el valor por defecto (1).
fn occurs(
    min: &Option<Spanned<String>>,
    max: &Option<Spanned<String>>,
    errors: &mut Errors,
) -> Occurs {
    let mut o = Occurs::ONCE;
    if let Some(m) = min {
        match occurs_value(&m.value, false) {
            Ok(Some(n)) => o.min = n,
            _ => errors.component(
                ErrorKind::InvalidValue {
                    attr: "minOccurs",
                    expected: "un entero no negativo",
                    found: clip(&m.value),
                },
                m.at(),
            ),
        }
    }
    if let Some(m) = max {
        match occurs_value(&m.value, true) {
            Ok(n) => o.max = n,
            Err(()) => errors.component(
                ErrorKind::InvalidValue {
                    attr: "maxOccurs",
                    expected: "un entero no negativo o unbounded",
                    found: clip(&m.value),
                },
                m.at(),
            ),
        }
    }
    o
}

/// Restricciones de un `xs:attribute` (global o local) y su tipo: un tipo
/// simple predefinido o del usuario, su tipo anónimo (`anonymous`, ya
/// numerado) o `xs:anySimpleType` si no tiene tipo.
fn attribute(
    a: &Attribute,
    anonymous: TypeRef,
    symbols: &Symbols<'_>,
    errors: &mut Errors,
) -> TypeRef {
    ncname(&a.name, errors);
    let conflict = |problem, at, errors: &mut Errors| {
        errors.component(
            ErrorKind::Conflict {
                construct: ATTRIBUTE,
                name: clip(&a.name.value),
                problem,
            },
            at,
        )
    };
    // XSD 1.0, src-attribute 1 y 2 (I13).
    if let (Some(d), Some(f)) = (&a.default, &a.fixed) {
        let at = if d.span.start.offset > f.span.start.offset {
            d.at()
        } else {
            f.at()
        };
        conflict("no puede tener default y fixed a la vez", at, errors);
    }
    if let (Some(d), Some(u)) = (&a.default, &a.use_) {
        if u.value == Use::Required {
            conflict(
                "un atributo requerido (use=\"required\") no puede tener default",
                d.at(),
                errors,
            );
        }
    }
    match (&a.ty, &a.simple_type) {
        (Some(_), Some(st)) => {
            // src-attribute 4.
            conflict(
                "no puede tener type y un tipo anónimo a la vez",
                st.span.start,
                errors,
            );
            TypeRef::Invalid
        }
        (Some(ty), None) => match symbols.lookup_type(&ty.value) {
            Lookup::Found(TypeRef::Complex(_)) => {
                errors.component(
                    ErrorKind::NotSimpleType {
                        name: clip(ty.value.as_str()),
                        context: "el tipo de un atributo",
                    },
                    ty.at(),
                );
                TypeRef::Invalid
            }
            Lookup::Found(t) => t,
            other => {
                type_error(ty, other, errors);
                TypeRef::Invalid
            }
        },
        (None, Some(_)) => anonymous,
        (None, None) => TypeRef::Builtin(Builtin::AnySimpleType),
    }
}

/// Etapa 1 de un tipo complejo: atributos (I10), contenido simple (D27) y
/// las partículas de su modelo (ocurrencias, combinaciones de atributos,
/// resolución de `type` y `ref`).
fn check_type<'a>(
    t: &mut TypeInfo<'a>,
    globals: &[Global<'a>],
    symbols: &Symbols<'_>,
    errors: &mut Errors,
) {
    let attributes = match &t.ct.content {
        ComplexContent::Simple(sc) => {
            let base = match symbols.lookup_type(&sc.base.value) {
                Lookup::Found(TypeRef::Complex(_)) => {
                    // D27 (U7): derivar un tipo complejo de otro.
                    errors.component(
                        ErrorKind::Unsupported {
                            what: "tipo base",
                            name: clip(sc.base.value.as_str()),
                            category: "derivación de tipos complejos",
                            reason: "la base de xs:simpleContent es un tipo complejo; \
                                     extenderlo es derivar un tipo complejo de otro, como \
                                     xs:complexContent (D27): la base debe ser un tipo simple",
                        },
                        sc.base.at(),
                    );
                    TypeRef::Invalid
                }
                Lookup::Found(b) => b,
                other => {
                    type_error(&sc.base, other, errors);
                    TypeRef::Invalid
                }
            };
            t.simple_base = Some((base, sc.base.span));
            &sc.attributes
        }
        ComplexContent::Elements { attributes, .. } => attributes,
    };
    // I10: dos atributos con el mismo nombre en el tipo.
    let mut seen: HashMap<&str, Pos> = HashMap::with_capacity(attributes.len());
    for (k, a) in attributes.iter().enumerate() {
        if let Some(first) = seen.get(a.name.value.as_str()) {
            errors.component(
                ErrorKind::DuplicateName {
                    space: "atributo",
                    name: clip(&a.name.value),
                    first: *first,
                },
                a.name.at(),
            );
        } else {
            seen.insert(&a.name.value, a.name.at());
        }
        let ty = attribute(a, t.anon_attrs[k], symbols, errors);
        t.attributes.push(Attr { a, ty });
    }

    for p in &mut t.particles {
        match &mut p.term {
            Term::Group { group, .. } => {
                p.occurs = occurs(&group.min_occurs, &group.max_occurs, errors);
            }
            Term::Element { el, name, ty } => {
                p.occurs = occurs(&el.min_occurs, &el.max_occurs, errors);
                let (n, resolved) = particle_element(el, *ty, globals, symbols, errors);
                *name = n;
                *ty = resolved;
            }
        }
    }
}

/// Un `xs:element` local: `name` o `ref` (exactamente uno) y su tipo.
/// Devuelve el nombre de la partícula y su tipo.
fn particle_element<'a>(
    el: &'a Element,
    anonymous: TypeRef,
    globals: &[Global<'a>],
    symbols: &Symbols<'_>,
    errors: &mut Errors,
) -> (&'a str, TypeRef) {
    let Some(r) = &el.reference else {
        let Some(n) = &el.name else {
            errors.component(
                ErrorKind::Conflict {
                    construct: LOCAL_ELEMENT,
                    name: String::new(),
                    problem: "necesita name o ref",
                },
                el.span.start,
            );
            return ("", TypeRef::Invalid);
        };
        ncname(n, errors);
        let ty = element_type(el, anonymous, LOCAL_ELEMENT, symbols, errors);
        return (&n.value, ty);
    };
    // XSD 1.0, src-element 2: con `ref` no hay `name`, `type` ni tipo
    // anónimo.
    let conflict = |problem, at, errors: &mut Errors| {
        errors.component(
            ErrorKind::Conflict {
                construct: REF_ELEMENT,
                name: clip(r.value.as_str()),
                problem,
            },
            at,
        );
        ("", TypeRef::Invalid)
    };
    if el.name.is_some() {
        return conflict("no puede tener name y ref a la vez", r.at(), errors);
    }
    if let Some(ty) = &el.ty {
        return conflict(
            "no puede tener type: el tipo es el de la declaración global",
            ty.at(),
            errors,
        );
    }
    if let Some(anon) = &el.anonymous {
        return conflict(
            "no puede tener un tipo anónimo: el tipo es el de la declaración global",
            anonymous_span(anon).start,
            errors,
        );
    }
    match symbols.elements.get(&key(&r.value)) {
        Some(&(g, _)) => (text(&globals[g].el.name), globals[g].ty),
        None => {
            errors.component(
                ErrorKind::Unresolved {
                    what: "elemento global",
                    name: clip(r.value.as_str()),
                    ns: r.value.ns().map(clip),
                },
                r.at(),
            );
            ("", TypeRef::Invalid)
        }
    }
}

/// `a` > `b` para dos enteros no negativos ya validados por
/// [`occurs_value`] (signo opcional, ceros a la izquierda): por longitud sin
/// ceros a la izquierda y después lexicográficamente. O(|a| + |b|).
fn lexeme_gt(a: &str, b: &str) -> bool {
    fn digits(s: &str) -> &str {
        s.trim_start_matches(['+', '-']).trim_start_matches('0')
    }
    let (a, b) = (digits(a), digits(b));
    (a.len(), a) > (b.len(), b)
}

/// `minOccurs` y `maxOccurs` de una partícula, tal como se escribieron.
fn occurs_of<'p>(p: &Part<'p>) -> (&'p Option<Spanned<String>>, &'p Option<Spanned<String>>) {
    match p.term {
        Term::Element { el, .. } => (&el.min_occurs, &el.max_occurs),
        Term::Group { group, .. } => (&group.min_occurs, &group.max_occurs),
    }
}

/// Etapa 3 de un tipo: `minOccurs` ≤ `maxOccurs` (I11), *all-group limited*
/// y *Element Declarations Consistent* (I12). Un recorrido de la arena.
fn check_model(t: &TypeInfo<'_>, errors: &mut Errors) {
    // Partícula con el error y posición: el valor de `maxOccurs` si está
    // (es el que queda por debajo), si no el de `minOccurs`.
    let at_of =
        |v: &Option<Spanned<String>>, p: &Part<'_>| v.as_ref().map_or(p.span.start, |v| v.at());
    for p in &t.particles {
        let (min, max) = occurs_of(p);
        // 5.3-CR1-S2: si los dos saturan en `u64::MAX`, se comparan los
        // lexemas.
        let saturated = p.occurs.min == u64::MAX
            && p.occurs.max == Some(u64::MAX)
            && matches!((min, max), (Some(a), Some(b)) if lexeme_gt(&a.value, &b.value));
        if saturated || p.occurs.max.is_some_and(|max| p.occurs.min > max) {
            let at = if max.is_some() {
                at_of(max, p)
            } else {
                at_of(min, p)
            };
            errors.model(
                ErrorKind::ContentModel {
                    problem: "minOccurs es mayor que maxOccurs",
                },
                at,
            );
        }
    }
    // *all-group limited* (XSD 1.0 §3.8.6, cos-all-limited): la gramática
    // ya deja `xs:all` solo como modelo entero de un tipo y con hijos que
    // son elementos.
    if let Some(
        root @ Part {
            term: Term::Group { group, children },
            ..
        },
    ) = t.particles.first()
    {
        if group.compositor == Compositor::All {
            let (min, max) = occurs_of(root);
            if root.occurs.min > 1 {
                errors.model(
                    ErrorKind::ContentModel {
                        problem: "xs:all solo admite minOccurs 0 o 1",
                    },
                    at_of(min, root),
                );
            }
            if root.occurs.max != Some(1) {
                errors.model(
                    ErrorKind::ContentModel {
                        problem: "xs:all solo admite maxOccurs 1",
                    },
                    at_of(max, root),
                );
            }
            for &c in children {
                let p = &t.particles[c];
                let (min, max) = occurs_of(p);
                if p.occurs.min > 1 {
                    errors.model(
                        ErrorKind::ContentModel {
                            problem: "un elemento de xs:all solo admite minOccurs 0 o 1",
                        },
                        at_of(min, p),
                    );
                }
                if p.occurs.max.is_none_or(|m| m > 1) {
                    errors.model(
                        ErrorKind::ContentModel {
                            problem: "un elemento de xs:all solo admite maxOccurs 0 o 1",
                        },
                        at_of(max, p),
                    );
                }
            }
        }
    }
    // *Element Declarations Consistent*: mismo nombre, mismo tipo.
    let mut seen: HashMap<&str, (TypeRef, Pos)> = HashMap::new();
    for p in &t.particles {
        if let Term::Element { name, ty, .. } = &p.term {
            match seen.get(name) {
                Some((first_ty, first)) if first_ty != ty => errors.model(
                    ErrorKind::InconsistentElement {
                        name: clip(name),
                        first: *first,
                    },
                    p.span.start,
                ),
                Some(_) => {}
                None => {
                    seen.insert(name, (*ty, p.span.start));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{lexeme_gt, occurs_value};

    /// 5.3-CR1-S2: el orden de dos ocurrencias que saturan en `u64::MAX`.
    #[test]
    fn ocurrencias_saturadas_se_comparan_por_lexema() {
        assert!(lexeme_gt("18446744073709551617", "18446744073709551616"));
        assert!(lexeme_gt("+0100000000000000000000", "99999999999999999999"));
        assert!(!lexeme_gt("18446744073709551616", "018446744073709551616"));
        assert!(!lexeme_gt("18446744073709551616", "18446744073709551617"));
    }

    /// Forma léxica de `xs:nonNegativeInteger` (signo opcional, `-` solo con
    /// valor cero) y `unbounded` solo en `maxOccurs`; saturación en
    /// `u64::MAX` (`ir.md` §5.1).
    #[test]
    fn ocurrencias_como_entero_no_negativo_saturado() {
        for (v, unbounded, esperado) in [
            ("0", false, Ok(Some(0))),
            ("+7", false, Ok(Some(7))),
            ("-0", false, Ok(Some(0))),
            ("-000", true, Ok(Some(0))),
            ("007", false, Ok(Some(7))),
            ("18446744073709551615", false, Ok(Some(u64::MAX))),
            ("18446744073709551616", false, Ok(Some(u64::MAX))),
            ("9".repeat(100).as_str(), true, Ok(Some(u64::MAX))),
            ("unbounded", true, Ok(None)),
            ("unbounded", false, Err(())),
            ("-1", false, Err(())),
            ("", true, Err(())),
            ("+", false, Err(())),
            ("1.0", false, Err(())),
            ("1e2", false, Err(())),
            ("1 2", false, Err(())),
            ("٣", false, Err(())),
        ] {
            assert_eq!(occurs_value(v, unbounded), esperado, "{v:?}");
        }
    }
}
