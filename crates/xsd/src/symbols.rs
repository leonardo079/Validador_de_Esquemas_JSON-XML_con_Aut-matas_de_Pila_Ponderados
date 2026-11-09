//! Tabla de símbolos del esquema XSD y tipos predefinidos (tarea 5.3,
//! `docs/fase3/xsd.md` §2).
//!
//! XSD tiene un espacio de símbolos por clase de componente global: los
//! elementos, los tipos (complejos y simples **juntos**: un `type="T"` no
//! dice cuál busca) y los atributos. Cada tabla se indexa por el nombre
//! expandido como **(identidad del URI, nombre local)**, es decir
//! `(Name::ns_id(), Name::local())` en las referencias y
//! `(targetNamespace, name)` en las declaraciones, con el `NsId` que internó
//! la 5.2 para el `targetNamespace`. Nunca por el texto del URI: con un URI
//! de varios MiB y muchas referencias, hashearlo en cada consulta sería
//! O(referencias · |URI|) (`docs/fase2/pendientes.md` §11). `NsId` hashea la
//! dirección de su URI internado, así que las tablas usan el *hasher*
//! aleatorio de `std` (`RandomState`), nunca uno determinista (5.1-SEC3-I1).

use std::collections::HashMap;

use ir::Lexical;
use lexer::Pos;

use crate::{Name, NsId, XS_NS};

/// Nombre expandido de una declaración global o de una referencia.
pub(crate) type Key<'a> = (Option<&'a NsId>, &'a str);

/// La clave de un nombre resuelto por el lector.
pub(crate) fn key(name: &Name) -> Key<'_> {
    (name.ns_id(), name.local())
}

/// Tipos predefinidos de `00-alcance.md` §3.2 y `xs:anySimpleType`, el tipo
/// implícito de un `xs:attribute` sin tipo (`docs/fase3/ir.md` §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Builtin {
    String,
    Boolean,
    Decimal,
    Integer,
    Long,
    Int,
    Short,
    Byte,
    NonNegativeInteger,
    PositiveInteger,
    Double,
    Float,
    AnySimpleType,
}

impl Builtin {
    pub(crate) const ALL: [Builtin; 13] = [
        Builtin::String,
        Builtin::Boolean,
        Builtin::Decimal,
        Builtin::Integer,
        Builtin::Long,
        Builtin::Int,
        Builtin::Short,
        Builtin::Byte,
        Builtin::NonNegativeInteger,
        Builtin::PositiveInteger,
        Builtin::Double,
        Builtin::Float,
        Builtin::AnySimpleType,
    ];

    /// Nombre local en el espacio de nombres de XML Schema.
    pub(crate) fn local(self) -> &'static str {
        match self {
            Builtin::String => "string",
            Builtin::Boolean => "boolean",
            Builtin::Decimal => "decimal",
            Builtin::Integer => "integer",
            Builtin::Long => "long",
            Builtin::Int => "int",
            Builtin::Short => "short",
            Builtin::Byte => "byte",
            Builtin::NonNegativeInteger => "nonNegativeInteger",
            Builtin::PositiveInteger => "positiveInteger",
            Builtin::Double => "double",
            Builtin::Float => "float",
            Builtin::AnySimpleType => "anySimpleType",
        }
    }

    /// Los doce de §3.2 por su nombre local; `anySimpleType` no se puede
    /// nombrar (fuera de §3.2), solo es el tipo de un atributo sin tipo.
    fn by_name(local: &str) -> Option<Builtin> {
        Builtin::ALL[..12]
            .iter()
            .copied()
            .find(|b| b.local() == local)
    }

    /// Espacio léxico y rango de valores (`docs/fase3/ir.md` §5.2): los
    /// enteros acotados son `Integer` con `Minimum`/`Maximum`.
    pub(crate) fn rules(self) -> (Lexical, Option<&'static str>, Option<&'static str>) {
        match self {
            Builtin::String | Builtin::AnySimpleType => (Lexical::String, None, None),
            Builtin::Boolean => (Lexical::Boolean, None, None),
            Builtin::Decimal => (Lexical::Decimal, None, None),
            Builtin::Integer => (Lexical::Integer, None, None),
            Builtin::Long => (
                Lexical::Integer,
                Some("-9223372036854775808"),
                Some("9223372036854775807"),
            ),
            Builtin::Int => (Lexical::Integer, Some("-2147483648"), Some("2147483647")),
            Builtin::Short => (Lexical::Integer, Some("-32768"), Some("32767")),
            Builtin::Byte => (Lexical::Integer, Some("-128"), Some("127")),
            Builtin::NonNegativeInteger => (Lexical::Integer, Some("0"), None),
            Builtin::PositiveInteger => (Lexical::Integer, Some("1"), None),
            Builtin::Double => (Lexical::Double, None, None),
            Builtin::Float => (Lexical::Float, None, None),
        }
    }
}

/// Tipo al que lleva un `type`, un `base` o la declaración de un elemento.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeRef {
    /// Tipo complejo, por su posición en el preorden de declaraciones de
    /// tipo (`Analysis::types`).
    Complex(usize),
    Builtin(Builtin),
    /// Un `xs:simpleType` del usuario: pendiente de la tarea 5.6, así que
    /// el análisis ya tiene un error y la bajada nunca lo ve.
    UserSimple,
    /// El error ya está informado (sin resolver, D14…): no se compara ni se
    /// baja.
    Invalid,
}

const DATE_TIME: [&str; 9] = [
    "dateTime",
    "date",
    "time",
    "duration",
    "gYear",
    "gYearMonth",
    "gMonth",
    "gMonthDay",
    "gDay",
];

/// Categoría y motivo de un nombre de tipo del espacio de XML Schema que no
/// es de `00-alcance.md` §3.2 (D6). `None`: no es un tipo de XSD 1.0.
pub(crate) fn unsupported_builtin(local: &str) -> Option<(&'static str, &'static str)> {
    const OTHERS: [&str; 30] = [
        "anySimpleType",
        "normalizedString",
        "token",
        "language",
        "Name",
        "NCName",
        "ID",
        "IDREF",
        "IDREFS",
        "ENTITY",
        "ENTITIES",
        "NMTOKEN",
        "NMTOKENS",
        "QName",
        "NOTATION",
        "anyURI",
        "base64Binary",
        "hexBinary",
        "nonPositiveInteger",
        "negativeInteger",
        "unsignedLong",
        "unsignedInt",
        "unsignedShort",
        "unsignedByte",
        // XSD 1.1, para dar un mensaje claro.
        "dateTimeStamp",
        "dayTimeDuration",
        "yearMonthDuration",
        "anyAtomicType",
        "error",
        "untypedAtomic",
    ];
    if local == "anyType" {
        Some((
            "xs:anyType",
            "admite contenido mixto, elementos no declarados y cualquier atributo \
             (como xs:any, xs:anyAttribute y mixed=\"true\", 00-alcance.md §3.3; D14)",
        ))
    } else if DATE_TIME.contains(&local) {
        Some((
            "tipo de fecha y hora",
            "fuera del subconjunto (00-alcance.md §3.3)",
        ))
    } else if OTHERS.contains(&local) {
        Some((
            "tipo predefinido fuera del subconjunto",
            "se admiten string, boolean, decimal, integer, long, int, short, byte, \
             nonNegativeInteger, positiveInteger, double y float (00-alcance.md §3.2)",
        ))
    } else {
        None
    }
}

/// Resultado de buscar un nombre de tipo.
pub(crate) enum Lookup {
    Found(TypeRef),
    /// Del espacio de XML Schema, pero fuera de §3.2: categoría y motivo.
    Unsupported(&'static str, &'static str),
    NotFound,
}

/// Los tres espacios de símbolos globales, cada uno con la posición del
/// nombre de su declaración (para el error de un nombre repetido). Los
/// elementos guardan su posición en `Analysis::globals`; los tipos, su
/// `TypeRef`.
#[derive(Default)]
pub(crate) struct Symbols<'a> {
    pub(crate) elements: HashMap<Key<'a>, (usize, Pos)>,
    pub(crate) types: HashMap<Key<'a>, (TypeRef, Pos)>,
    pub(crate) attributes: HashMap<Key<'a>, Pos>,
}

impl Symbols<'_> {
    /// Un nombre de tipo: primero los del esquema y después los
    /// predefinidos (`(XS_NS, nombre)`). Comparar el URI con [`XS_NS`]
    /// cuesta O(|XS_NS|): `==` de `str` compara antes las longitudes.
    /// O(|nombre local|) más el hash del `NsId`, que es O(1).
    pub(crate) fn lookup_type(&self, name: &Name) -> Lookup {
        if let Some((t, _)) = self.types.get(&key(name)) {
            return Lookup::Found(*t);
        }
        if name.ns() != Some(XS_NS) {
            return Lookup::NotFound;
        }
        match Builtin::by_name(name.local()) {
            Some(b) => Lookup::Found(TypeRef::Builtin(b)),
            None => match unsupported_builtin(name.local()) {
                Some((cat, why)) => Lookup::Unsupported(cat, why),
                None => Lookup::NotFound,
            },
        }
    }
}
