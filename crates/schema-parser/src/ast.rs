//! AST del esquema JSON (subconjunto de JSON Schema 2020-12).
//!
//! Cada nodo guarda su [`Span`] para que las fases siguientes reporten
//! errores con línea y columna. El AST conserva el orden de aparición de las
//! palabras clave y de las entradas de los mapas; no hay claves duplicadas
//! porque el parser las rechaza.

pub use lexer::{Pos, Span};

/// Un esquema: `true`, `false` o un objeto de palabras clave.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    pub kind: SchemaKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SchemaKind {
    Bool(bool),
    Object(Vec<Keyword>),
}

/// Una palabra clave de un objeto-esquema: `"nombre": valor`.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyword {
    /// Nombre tal como aparece (con su posición).
    pub name: Str,
    /// Palabra clave reconocida, o [`Kw::Unsupported`].
    pub kw: Kw,
    pub value: Value,
    /// Rango del valor.
    pub value_span: Span,
}

/// Valor de una palabra clave. La forma la fija la clase de la palabra
/// clave ([`Class`]), que es la producción de `keyword` en la gramática.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `K_STRING : STRING`, y `type` con un solo nombre.
    Str(Str),
    /// `K_NUMBER : NUMBER`.
    Num(Num),
    /// `K_BOOL : boolean`.
    Bool(bool),
    /// `K_SCHEMA : schema`.
    Schema(Box<Schema>),
    /// `K_MAP : schema_map`, en orden de aparición.
    SchemaMap(Vec<(Str, Schema)>),
    /// `K_LIST : [ schemas ]`.
    SchemaList(Vec<Schema>),
    /// `required` y `type` con arreglo.
    Strings(Vec<Str>),
    /// `dependentRequired`: propiedad → propiedades que exige.
    StringMap(Vec<(Str, Vec<Str>)>),
    /// `K_VALUE : value` y `K_OTHER : value`.
    Json(Json),
    /// `K_VALUES : array`.
    JsonList(Vec<Json>),
}

/// Cadena decodificada (sin comillas ni escapes) con su posición.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Str {
    pub value: String,
    pub span: Span,
}

/// Número como lexema decimal exacto (no se convierte a `f64`; ver
/// `docs/00-alcance.md` §1.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Num {
    pub lexeme: String,
    pub span: Span,
}

/// Valor JSON arbitrario (para `const`, `enum`, `default`, `examples` y las
/// palabras clave fuera del subconjunto).
#[derive(Clone, Debug, PartialEq)]
pub struct Json {
    pub kind: JsonKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JsonKind {
    Null,
    Bool(bool),
    /// Lexema decimal exacto.
    Number(String),
    String(String),
    Array(Vec<Json>),
    /// Pares en orden de aparición, sin claves repetidas.
    Object(Vec<(Str, Json)>),
}

/// Clase de una palabra clave: el terminal `K_*` de la gramática
/// (`docs/fase2/esquema.ll1`). Decide la forma de su valor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// `K_STRING : STRING`
    String,
    /// `K_NUMBER : NUMBER`
    Number,
    /// `K_BOOL : boolean`
    Bool,
    /// `K_SCHEMA : schema`
    Schema,
    /// `K_MAP : schema_map`
    SchemaMap,
    /// `K_LIST : [ schemas ]`
    SchemaList,
    /// `K_VALUE : value`
    Value,
    /// `K_VALUES : array`
    Values,
    /// `K_TYPE : type_value`
    Type,
    /// `K_REQUIRED : string_array`
    Required,
    /// `K_DEPREQ : dep_map`
    DependentRequired,
    /// `K_OTHER : value`
    Other,
}

/// Palabras clave reconocidas por la gramática.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kw {
    // Núcleo y anotaciones (§2.1 de docs/00-alcance.md).
    DollarSchema,
    Id,
    Anchor,
    Ref,
    Comment,
    Defs,
    Definitions,
    Title,
    Description,
    Default,
    Examples,
    Deprecated,
    ReadOnly,
    WriteOnly,
    // Cualquier tipo.
    Type,
    Enum,
    Const,
    // Números.
    Minimum,
    Maximum,
    ExclusiveMinimum,
    ExclusiveMaximum,
    MultipleOf,
    // Cadenas.
    MinLength,
    MaxLength,
    Pattern,
    Format,
    // Arreglos.
    Items,
    PrefixItems,
    MinItems,
    MaxItems,
    UniqueItems,
    // Objetos.
    Properties,
    Required,
    AdditionalProperties,
    MinProperties,
    MaxProperties,
    DependentRequired,
    DependentSchemas,
    // Combinadores.
    AllOf,
    AnyOf,
    OneOf,
    Not,
    /// Cualquier otra clave: fuera del subconjunto. El parser conserva su
    /// nombre y su valor; la Fase 3 la rechaza (nunca se ignora).
    Unsupported,
}

/// Tabla única nombre → palabra clave → clase. Es la partición del terminal
/// `STRING` en las clases `K_*` cuando aparece como clave de un objeto-esquema.
const KEYWORDS: [(&str, Kw, Class); 42] = [
    ("$schema", Kw::DollarSchema, Class::String),
    ("$id", Kw::Id, Class::String),
    ("$anchor", Kw::Anchor, Class::String),
    ("$ref", Kw::Ref, Class::String),
    ("$comment", Kw::Comment, Class::String),
    ("$defs", Kw::Defs, Class::SchemaMap),
    ("definitions", Kw::Definitions, Class::SchemaMap),
    ("title", Kw::Title, Class::String),
    ("description", Kw::Description, Class::String),
    ("default", Kw::Default, Class::Value),
    ("examples", Kw::Examples, Class::Values),
    ("deprecated", Kw::Deprecated, Class::Bool),
    ("readOnly", Kw::ReadOnly, Class::Bool),
    ("writeOnly", Kw::WriteOnly, Class::Bool),
    ("type", Kw::Type, Class::Type),
    ("enum", Kw::Enum, Class::Values),
    ("const", Kw::Const, Class::Value),
    ("minimum", Kw::Minimum, Class::Number),
    ("maximum", Kw::Maximum, Class::Number),
    ("exclusiveMinimum", Kw::ExclusiveMinimum, Class::Number),
    ("exclusiveMaximum", Kw::ExclusiveMaximum, Class::Number),
    ("multipleOf", Kw::MultipleOf, Class::Number),
    ("minLength", Kw::MinLength, Class::Number),
    ("maxLength", Kw::MaxLength, Class::Number),
    ("pattern", Kw::Pattern, Class::String),
    ("format", Kw::Format, Class::String),
    ("items", Kw::Items, Class::Schema),
    ("prefixItems", Kw::PrefixItems, Class::SchemaList),
    ("minItems", Kw::MinItems, Class::Number),
    ("maxItems", Kw::MaxItems, Class::Number),
    ("uniqueItems", Kw::UniqueItems, Class::Bool),
    ("properties", Kw::Properties, Class::SchemaMap),
    ("required", Kw::Required, Class::Required),
    (
        "additionalProperties",
        Kw::AdditionalProperties,
        Class::Schema,
    ),
    ("minProperties", Kw::MinProperties, Class::Number),
    ("maxProperties", Kw::MaxProperties, Class::Number),
    (
        "dependentRequired",
        Kw::DependentRequired,
        Class::DependentRequired,
    ),
    ("dependentSchemas", Kw::DependentSchemas, Class::SchemaMap),
    ("allOf", Kw::AllOf, Class::SchemaList),
    ("anyOf", Kw::AnyOf, Class::SchemaList),
    ("oneOf", Kw::OneOf, Class::SchemaList),
    ("not", Kw::Not, Class::Schema),
];

impl Kw {
    /// Clasifica el nombre de una clave de un objeto-esquema.
    pub fn from_name(name: &str) -> Kw {
        KEYWORDS
            .iter()
            .find(|(n, _, _)| *n == name)
            .map_or(Kw::Unsupported, |&(_, kw, _)| kw)
    }

    /// Nombre de la palabra clave; `None` para [`Kw::Unsupported`].
    pub fn name(self) -> Option<&'static str> {
        KEYWORDS
            .iter()
            .find(|(_, kw, _)| *kw == self)
            .map(|&(n, _, _)| n)
    }

    /// Nombres de todas las palabras clave reconocidas, en el orden de la
    /// tabla. La Fase 3 los usa para sugerir la palabra clave más parecida a
    /// una desconocida.
    pub fn names() -> impl Iterator<Item = &'static str> {
        KEYWORDS.iter().map(|&(n, _, _)| n)
    }

    /// Terminal `K_*` de la gramática.
    pub fn class(self) -> Class {
        KEYWORDS
            .iter()
            .find(|(_, kw, _)| *kw == self)
            .map_or(Class::Other, |&(_, _, c)| c)
    }
}

impl Schema {
    /// Palabras clave del esquema (vacío para `true`/`false`).
    pub fn keywords(&self) -> &[Keyword] {
        match &self.kind {
            SchemaKind::Bool(_) => &[],
            SchemaKind::Object(kws) => kws,
        }
    }

    /// Primera palabra clave `kw` del esquema.
    pub fn get(&self, kw: Kw) -> Option<&Keyword> {
        self.keywords().iter().find(|k| k.kw == kw)
    }
}
