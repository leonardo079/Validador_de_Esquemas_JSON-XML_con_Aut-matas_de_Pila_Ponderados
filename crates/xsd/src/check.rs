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
//!    nombres repetidos (globales e I10), referencias sin resolver, tipos
//!    fuera de §3.2, D27 y `xs:simpleType` (pendiente de la 5.6);
//! 2. *modelos de contenido*: `minOccurs` ≤ `maxOccurs` (I11), *all-group
//!    limited* y *Element Declarations Consistent* (I12). Necesitan los
//!    tipos resueltos de la etapa 1.
//!
//! **Sin recursión (D7).** El AST es un árbol de profundidad ≤ 256, pero se
//! recorre con pilas explícitas: una de tipos complejos pendientes (que da
//! el preorden de las declaraciones, B3) y, dentro de cada tipo, otra para
//! aplanar su modelo de contenido en una arena en preorden. Cada
//! construcción del AST se visita una vez en el recorrido y otra en la
//! comprobación de su arena: O(n) en el tamaño del AST, más O(1) esperado
//! por consulta de las tablas hash (más el hash del nombre local).

use std::collections::HashMap;

use ir::Occurs;
use lexer::{Pos, Span};

use crate::ast::{
    AnonymousType, Attribute, ComplexContent, ComplexType, Compositor, Element, Group, Particle,
    Schema, Spanned, TopLevel, Use,
};
use crate::error::{clip, ErrorKind, XsdError};
use crate::reader::is_ncname;
use crate::symbols::{key, Builtin, Key, Lookup, Symbols, TypeRef};
use crate::Name;

/// Un esquema que pasó el análisis: lo único que acepta [`crate::lower`].
/// Guarda referencias al AST, así que vive lo que él.
#[derive(Debug)]
pub struct Analysis<'a> {
    pub(crate) schema: &'a Schema,
    /// Tipos complejos (con nombre y anónimos) en **preorden** de
    /// declaración: su posición + 1 es su `NodeId` (B3).
    pub(crate) types: Vec<TypeInfo<'a>>,
    /// Elementos globales en el orden del documento.
    pub(crate) globals: Vec<Global<'a>>,
}

impl Analysis<'_> {
    /// Número de tipos complejos (con nombre y anónimos): los nodos de
    /// declaración de la IR, sin el documento ni los sintéticos.
    pub fn type_count(&self) -> usize {
        self.types.len()
    }
}

/// Un elemento global: una partícula del documento.
#[derive(Debug)]
pub(crate) struct Global<'a> {
    pub(crate) el: &'a Element,
    pub(crate) ty: TypeRef,
}

/// Un tipo complejo con su contenido aplanado y resuelto.
#[derive(Debug)]
pub(crate) struct TypeInfo<'a> {
    pub(crate) ct: &'a ComplexType,
    /// Tipo que contiene la declaración (`None`: el documento).
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
}

/// Partícula del modelo aplanado (el índice de los hijos es el de la
/// arena, como en `ir::ContentModel`).
#[derive(Debug)]
pub(crate) struct Part<'a> {
    pub(crate) term: Term<'a>,
    pub(crate) occurs: Occurs,
    pub(crate) span: Span,
}

#[derive(Debug)]
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
#[derive(Debug)]
pub(crate) struct Attr<'a> {
    pub(crate) a: &'a Attribute,
    pub(crate) ty: TypeRef,
}

/// Errores de cada etapa.
#[derive(Default)]
struct Errors {
    /// Etapa 1: componentes y nombres.
    components: Vec<XsdError>,
    /// Etapa 2: modelos de contenido.
    models: Vec<XsdError>,
}

impl Errors {
    fn push(&mut self, kind: ErrorKind, at: Pos) {
        self.components.push(XsdError::new(kind, at));
    }

    fn model(&mut self, kind: ErrorKind, at: Pos) {
        self.models.push(XsdError::new(kind, at));
    }
}

/// Tipo complejo encontrado y aún sin número de preorden.
struct Pending<'a> {
    ct: &'a ComplexType,
    parent: Option<usize>,
    step: (&'static str, &'a str),
    slot: Slot,
}

/// Dónde se anota el número del tipo al dárselo.
enum Slot {
    /// `xs:complexType` global: posición en `Schema::items`.
    Named(usize),
    /// Tipo anónimo de un elemento global (posición en `globals`).
    Global(usize),
    /// Tipo anónimo de la partícula `part` del tipo `ty`.
    Particle { ty: usize, part: usize },
}

const LOCAL_ELEMENT: &str = "xs:element local";
const GLOBAL_ELEMENT: &str = "xs:element global";
const REF_ELEMENT: &str = "xs:element ref";
const ATTRIBUTE: &str = "xs:attribute";
const SIMPLE_TYPE: &str = "xs:simpleType";
const SIMPLE_TYPE_TASK: &str = "tarea 5.6: tipos simples del usuario y facetas";

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
    let (mut types, mut globals, named) = discover(schema);
    let symbols = symbols(schema, &named, &mut errors);

    // Tipos de los elementos globales (antes que las partículas: un `ref`
    // toma el tipo de su declaración global).
    for g in &mut globals {
        g.ty = element_type(g.el, g.ty, GLOBAL_ELEMENT, &symbols, &mut errors);
    }
    for t in &mut types {
        check_type(t, &globals, &symbols, &mut errors);
    }
    for item in &schema.items {
        match item {
            // 5.2-CR-S2: sin `ref` de atributo no se pueden usar, así que no
            // se bajan; se comprueban como cualquier declaración (XSD 1.0).
            TopLevel::Attribute(a) => {
                attribute(a, &symbols, &mut errors);
            }
            TopLevel::SimpleType(st) => errors.push(
                ErrorKind::Pending {
                    construct: SIMPLE_TYPE,
                    task: SIMPLE_TYPE_TASK,
                },
                st.span.start,
            ),
            TopLevel::Element(_) | TopLevel::ComplexType(_) => {}
        }
    }
    for t in &types {
        check_model(t, &mut errors);
    }

    for v in [&mut errors.components, &mut errors.models] {
        v.sort_by_key(|e| e.at.offset);
    }
    if !errors.components.is_empty() {
        return Err(errors.components);
    }
    if !errors.models.is_empty() {
        return Err(errors.models);
    }
    Ok(Analysis {
        schema,
        types,
        globals,
    })
}

/// Recorre el AST con una pila de tipos pendientes y da a cada tipo complejo
/// su número de **preorden** de declaración: un `xs:complexType` global o
/// el tipo anónimo de un elemento global, en el orden del documento, y antes
/// de los hermanos siguientes, los tipos anónimos de su propio modelo
/// (B3). Aplana cada modelo al llegar a su tipo. Devuelve los tipos, los
/// elementos globales y el número de cada `xs:complexType` global por su
/// posición en `Schema::items`.
fn discover(schema: &Schema) -> (Vec<TypeInfo<'_>>, Vec<Global<'_>>, Vec<Option<usize>>) {
    let mut globals = Vec::new();
    let mut named = vec![None; schema.items.len()];
    let mut stack: Vec<Pending<'_>> = Vec::new();
    for (i, item) in schema.items.iter().enumerate() {
        match item {
            TopLevel::ComplexType(ct) => stack.push(Pending {
                ct,
                parent: None,
                step: ("complexType", text(&ct.name)),
                slot: Slot::Named(i),
            }),
            TopLevel::Element(el) => {
                if let Some(AnonymousType::Complex(ct)) = &el.anonymous {
                    // S-5, primera mitad (B3): padre el documento.
                    stack.push(Pending {
                        ct,
                        parent: None,
                        step: ("element", text(&el.name)),
                        slot: Slot::Global(globals.len()),
                    });
                }
                globals.push(Global {
                    el,
                    ty: TypeRef::Invalid,
                });
            }
            TopLevel::SimpleType(_) | TopLevel::Attribute(_) => {}
        }
    }
    stack.reverse();
    let mut types: Vec<TypeInfo<'_>> = Vec::new();
    let mut found = Vec::new();
    while let Some(p) = stack.pop() {
        let idx = types.len();
        match p.slot {
            Slot::Named(i) => named[i] = Some(idx),
            Slot::Global(g) => globals[g].ty = TypeRef::Complex(idx),
            Slot::Particle { ty, part } => {
                if let Term::Element { ty: t, .. } = &mut types[ty].particles[part].term {
                    *t = TypeRef::Complex(idx);
                }
            }
        }
        let particles = match &p.ct.content {
            ComplexContent::Elements { model: Some(g), .. } => flatten(idx, g, &mut found),
            _ => Vec::new(),
        };
        types.push(TypeInfo {
            ct: p.ct,
            parent: p.parent,
            step: p.step,
            particles,
            attributes: Vec::new(),
            simple_base: None,
        });
        // Los hijos, en orden del documento: se apilan del revés.
        stack.extend(found.drain(..).rev());
    }
    (types, globals, named)
}

/// Aplana el modelo `root` del tipo `ty` en una arena en **preorden** con
/// una pila explícita (D7): cada partícula se añade al sacarla de la pila y
/// se anota como hija de su grupo, que ya está en la arena; los hijos se
/// apilan del revés, así que salen en orden. Deja en `found` los tipos
/// anónimos complejos de sus elementos, en orden del documento.
fn flatten<'a>(ty: usize, root: &'a Group, found: &mut Vec<Pending<'a>>) -> Vec<Part<'a>> {
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
                if let Some(AnonymousType::Complex(ct)) = &el.anonymous {
                    found.push(Pending {
                        ct,
                        parent: Some(ty),
                        step: ("element", text(&el.name)),
                        slot: Slot::Particle { ty, part: i },
                    });
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
        errors.push(
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
        Some(prev) => errors.push(
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
fn symbols<'a>(schema: &'a Schema, named: &[Option<usize>], errors: &mut Errors) -> Symbols<'a> {
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
                        let t = named[i].map_or(TypeRef::Invalid, TypeRef::Complex);
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
                        let v = (TypeRef::UserSimple, at);
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
    errors.push(kind, name.at());
}

/// Rango de un tipo anónimo.
fn anonymous_span(t: &AnonymousType) -> Span {
    match t {
        AnonymousType::Complex(ct) => ct.span,
        AnonymousType::Simple(st) => st.span,
    }
}

/// Tipo de un `xs:element` con nombre (global o local): `type`, tipo
/// anónimo (`anonymous`, ya numerado si es complejo) o ninguno (D14).
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
            errors.push(
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
        (None, Some(AnonymousType::Complex(_))) => anonymous,
        (None, Some(AnonymousType::Simple(st))) => {
            errors.push(
                ErrorKind::Pending {
                    construct: SIMPLE_TYPE,
                    task: SIMPLE_TYPE_TASK,
                },
                st.span.start,
            );
            TypeRef::Invalid
        }
        // D14: el tipo implícito sería `xs:anyType`.
        (None, None) => {
            errors.push(
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
fn occurs_value(v: &str, unbounded: bool) -> Result<Option<u64>, ()> {
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
            _ => errors.push(
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
            Err(()) => errors.push(
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
/// simple predefinido, `xs:anySimpleType` si no tiene tipo, o un
/// `xs:simpleType` (pendiente de la 5.6).
fn attribute(a: &Attribute, symbols: &Symbols<'_>, errors: &mut Errors) -> TypeRef {
    ncname(&a.name, errors);
    let conflict = |problem, at, errors: &mut Errors| {
        errors.push(
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
                errors.push(
                    ErrorKind::NotSimpleType {
                        name: clip(ty.value.as_str()),
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
        (None, Some(st)) => {
            errors.push(
                ErrorKind::Pending {
                    construct: SIMPLE_TYPE,
                    task: SIMPLE_TYPE_TASK,
                },
                st.span.start,
            );
            TypeRef::Invalid
        }
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
                    errors.push(
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
    for a in attributes {
        if let Some(first) = seen.get(a.name.value.as_str()) {
            errors.push(
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
        let ty = attribute(a, symbols, errors);
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
            errors.push(
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
        errors.push(
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
            errors.push(
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

/// `minOccurs` y `maxOccurs` de una partícula, tal como se escribieron.
fn occurs_of<'p>(p: &Part<'p>) -> (&'p Option<Spanned<String>>, &'p Option<Spanned<String>>) {
    match p.term {
        Term::Element { el, .. } => (&el.min_occurs, &el.max_occurs),
        Term::Group { group, .. } => (&group.min_occurs, &group.max_occurs),
    }
}

/// Etapa 2 de un tipo: `minOccurs` ≤ `maxOccurs` (I11), *all-group limited*
/// y *Element Declarations Consistent* (I12). Un recorrido de la arena.
fn check_model(t: &TypeInfo<'_>, errors: &mut Errors) {
    // Partícula con el error y posición: el valor de `maxOccurs` si está
    // (es el que queda por debajo), si no el de `minOccurs`.
    let at_of =
        |v: &Option<Spanned<String>>, p: &Part<'_>| v.as_ref().map_or(p.span.start, |v| v.at());
    for p in &t.particles {
        if p.occurs.max.is_some_and(|max| p.occurs.min > max) {
            let (min, max) = occurs_of(p);
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
    use super::occurs_value;

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
