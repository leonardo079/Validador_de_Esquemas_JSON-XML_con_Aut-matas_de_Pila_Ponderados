//! AST del subconjunto XSD (D3): propio de XSD, no el de JSON Schema.
//!
//! Lo construye el parser LL(1) de la tarea 5.2 ([`crate::parse`]) y lo
//! consume la tarea 5.3 (tabla de símbolos, restricciones y bajada a la IR).
//! Solo existe en tiempo de compilación. Cada construcción lleva el [`Span`]
//! de su elemento entero (del `<` al `>` de su cierre, o al `/>`), y cada
//! valor de atributo el de su valor con las comillas ([`Spanned`]).
//!
//! Lo que el AST guarda de cada atributo (`docs/fase2/gramatica-xsd.md` §5):
//!
//! - `type`, `base` y `ref`: el [`Name`] que resolvió el lector con el
//!   ámbito de su etiqueta (`Attr::as_qname`, una sola llamada por atributo).
//!   No se busca: la tabla de símbolos es de la 5.3, con
//!   `(Name::ns_id(), Name::local())` como clave.
//! - `targetNamespace`: el [`NsId`] del URI recortado (*collapse*) e internado
//!   en el lector, comparable con el de los nombres resueltos.
//! - `name`, `minOccurs`, `maxOccurs` y las facetas numéricas o de longitud:
//!   el lexema con *collapse* (sin espacios en los extremos y con cada
//!   secuencia interior reducida a uno). Su forma léxica la comprueban las
//!   tareas 5.3, 5.5 y 5.6.
//! - `default`, `fixed`, `xs:enumeration` y `xs:pattern`: el valor tal como lo
//!   entrega el lector (normalización de XML 1.0); el *whiteSpace* depende del
//!   tipo y lo aplican las tareas 5.5 y 5.6.
//! - `id`, `xs:annotation` y su contenido no se guardan (D23), ni los
//!   atributos de otro espacio de nombres que el de XSD (D34).
//!
//! La profundidad del AST está acotada por la del documento (como mucho
//! `MAX_DEPTH_LIMIT` = 256 elementos anidados, D7), así que su liberación
//! recursiva también lo está; se mide junto con el parser
//! (`gramatica-xsd.md` §8).

use lexer::{Pos, Span};

use crate::{Name, NsId};

/// Valor de un atributo con el rango de su valor, comillas incluidas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Posición del primer carácter del valor (el que sigue a la comilla):
    /// la de los errores sobre el valor, como en `Attr::as_qname`.
    pub fn at(&self) -> Pos {
        let mut at = self.span.start;
        at.advance(b'"');
        at
    }
}

/// `elementFormDefault`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormDefault {
    Qualified,
    Unqualified,
}

/// `xs:schema`.
///
/// **Recursión.** Los `Clone`, `Debug` y `PartialEq` derivados y el `Drop`
/// del AST son recursivos, acotados por la profundidad del documento (256,
/// D7). Todo consumidor recursivo (5.3, 5.4) y cualquier `clone()`, `{:?}` o
/// `==` de un `Schema` debe correr en un hilo con `stack_size` de al menos
/// 2 MiB, como el parser (`docs/fase2/pendientes.md` §5).
///
/// **Igualdad.** `==` compara los `Name` por contenido pero los [`NsId`] por
/// identidad (la del lector que los internó): solo tiene sentido entre
/// valores de un mismo análisis. Para comparar dos análisis, comparar su
/// forma de depuración (`{:?}` muestra los URI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    /// `targetNamespace` recortado e internado en el lector (`None` si no
    /// está). Su `NsId` es igual al de los nombres resueltos con ese URI.
    pub target_namespace: Option<Spanned<NsId>>,
    pub element_form_default: Option<Spanned<FormDefault>>,
    /// Declaraciones globales en el orden del documento.
    pub items: Vec<TopLevel>,
    pub span: Span,
}

/// Declaración global (hija de `xs:schema`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopLevel {
    Element(Element),
    ComplexType(ComplexType),
    SimpleType(SimpleType),
    Attribute(Attribute),
}

/// `xs:element`, global (`name`, `type`) o local (además `ref`, `minOccurs`,
/// `maxOccurs`). Las combinaciones válidas (`name` o `ref`, `type` o tipo
/// anónimo, D14) las comprueba la 5.3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub name: Option<Spanned<String>>,
    pub ty: Option<Spanned<Name>>,
    pub reference: Option<Spanned<Name>>,
    pub min_occurs: Option<Spanned<String>>,
    pub max_occurs: Option<Spanned<String>>,
    pub anonymous: Option<AnonymousType>,
    pub span: Span,
}

/// Tipo anónimo de un `xs:element`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnonymousType {
    Complex(Box<ComplexType>),
    Simple(Box<SimpleType>),
}

/// `xs:complexType`, con nombre (global) o anónimo. `mixed="true"` no llega
/// aquí: es un error `unsupported` del parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplexType {
    pub name: Option<Spanned<String>>,
    pub content: ComplexContent,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComplexContent {
    /// `xs:simpleContent` + `xs:extension`.
    Simple(SimpleContent),
    /// Modelo de contenido (o ninguno: contenido vacío) y atributos.
    Elements {
        model: Option<Group>,
        attributes: Vec<Attribute>,
    },
}

/// `xs:simpleContent` con su `xs:extension base="S"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleContent {
    pub base: Spanned<Name>,
    pub attributes: Vec<Attribute>,
    /// El `xs:simpleContent` entero.
    pub span: Span,
    /// El `xs:extension` entero.
    pub extension_span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compositor {
    Sequence,
    Choice,
    All,
}

/// `xs:sequence`, `xs:choice` o `xs:all` con sus ocurrencias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub compositor: Compositor,
    pub min_occurs: Option<Spanned<String>>,
    pub max_occurs: Option<Spanned<String>>,
    /// En `xs:all`, solo elementos (lo garantiza la gramática).
    pub particles: Vec<Particle>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Particle {
    Element(Element),
    Group(Group),
}

/// `use` de un `xs:attribute` local (`prohibited` es `unsupported`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Required,
    Optional,
}

/// `xs:attribute`, global o local (`use` solo en los locales).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub name: Spanned<String>,
    pub ty: Option<Spanned<Name>>,
    pub use_: Option<Spanned<Use>>,
    pub default: Option<Spanned<String>>,
    pub fixed: Option<Spanned<String>>,
    pub simple_type: Option<Box<SimpleType>>,
    pub span: Span,
}

/// `xs:simpleType`, con nombre (global) o anónimo; siempre una restricción
/// (`xs:list` y `xs:union` están fuera del subconjunto).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleType {
    pub name: Option<Spanned<String>>,
    pub restriction: Restriction,
    pub span: Span,
}

/// `xs:restriction` de un tipo simple: `base` o un `xs:simpleType` anónimo
/// (que esté exactamente uno de los dos lo comprueba la 5.3), y facetas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restriction {
    pub base: Option<Spanned<Name>>,
    pub simple_type: Option<Box<SimpleType>>,
    pub facets: Vec<Facet>,
    pub span: Span,
}

/// Facetas del subconjunto (`00-alcance.md` §3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacetKind {
    Enumeration,
    MinInclusive,
    MaxInclusive,
    MinExclusive,
    MaxExclusive,
    Length,
    MinLength,
    MaxLength,
    Pattern,
}

impl FacetKind {
    pub const ALL: [FacetKind; 9] = [
        FacetKind::Enumeration,
        FacetKind::MinInclusive,
        FacetKind::MaxInclusive,
        FacetKind::MinExclusive,
        FacetKind::MaxExclusive,
        FacetKind::Length,
        FacetKind::MinLength,
        FacetKind::MaxLength,
        FacetKind::Pattern,
    ];

    /// Nombre local del elemento (`minInclusive`).
    pub fn local(self) -> &'static str {
        match self {
            FacetKind::Enumeration => "enumeration",
            FacetKind::MinInclusive => "minInclusive",
            FacetKind::MaxInclusive => "maxInclusive",
            FacetKind::MinExclusive => "minExclusive",
            FacetKind::MaxExclusive => "maxExclusive",
            FacetKind::Length => "length",
            FacetKind::MinLength => "minLength",
            FacetKind::MaxLength => "maxLength",
            FacetKind::Pattern => "pattern",
        }
    }

    /// `xs:enumeration` y `xs:pattern` guardan el valor sin *collapse*: su
    /// tratamiento de los espacios depende del tipo (o no tiene ninguno).
    pub fn keeps_whitespace(self) -> bool {
        matches!(self, FacetKind::Enumeration | FacetKind::Pattern)
    }
}

/// `xs:<faceta> value="…"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facet {
    pub kind: FacetKind,
    pub value: Spanned<String>,
    pub span: Span,
}
