//! AST propio de DTD (D3): lo que el parser LL(1) (`parser.rs`) reconoce del
//! subconjunto de `docs/00-alcance.md` §3bis. Lo que está fuera del
//! subconjunto (`ANY`, contenido mixto, tipos tokenizados, `NOTATION`) no
//! tiene representación: el parser lo rechaza antes.
//!
//! Cada nodo guarda su `Span` para que los errores de la Fase 3 (DTD.3) y la
//! bajada a la IR lleven la posición en el esquema.

use lexer::Span;

/// Un nombre con su posición: nombre de elemento, de atributo o valor de una
/// enumeración (un *nmtoken*).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub text: String,
    pub span: Span,
}

/// Un literal entre comillas, con las referencias ya resueltas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    pub value: String,
    pub span: Span,
}

/// La DTD: sus declaraciones en orden de aparición.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dtd {
    pub elements: Vec<ElementDecl>,
    pub attlists: Vec<AttlistDecl>,
}

/// `<!ELEMENT nombre contenido>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementDecl {
    pub name: Name,
    pub content: ContentSpec,
    /// La declaración completa, de `<` a `>`.
    pub span: Span,
}

/// Contenido de un elemento.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentSpec {
    /// `EMPTY`.
    Empty,
    /// `(#PCDATA)` o `(#PCDATA)*`: solo texto.
    Pcdata,
    /// Modelo de hijos: un grupo entre paréntesis con su sufijo.
    Children(Particle),
}

/// Sufijo de ocurrencia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occurs {
    /// Sin sufijo: exactamente una vez.
    One,
    /// `?`
    Opt,
    /// `*`
    Star,
    /// `+`
    Plus,
}

/// Partícula de un modelo de contenido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Particle {
    pub kind: ParticleKind,
    pub occurs: Occurs,
    /// El nombre, o el grupo de `(` a `)`, sin el sufijo.
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    /// Un elemento hijo.
    Name(Name),
    /// `(a, b, …)`; un grupo de un solo elemento, `(a)`, también es una secuencia.
    Seq(Vec<Particle>),
    /// `(a | b | …)`, con al menos dos opciones.
    Choice(Vec<Particle>),
}

/// `<!ATTLIST elemento definiciones>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttlistDecl {
    pub element: Name,
    pub defs: Vec<AttDef>,
    pub span: Span,
}

/// Definición de un atributo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttDef {
    pub name: Name,
    pub ty: AttType,
    pub default: DefaultDecl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttType {
    /// `CDATA`: texto libre.
    Cdata,
    /// `(v1|v2|…)`: enumeración de *nmtokens*.
    Enumeration(Vec<Name>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultDecl {
    /// `#REQUIRED`
    Required,
    /// `#IMPLIED`
    Implied,
    /// `#FIXED "v"`
    Fixed(Literal),
    /// `"v"`: valor por defecto (el validador no lo inserta).
    Default(Literal),
}
