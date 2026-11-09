//! Tablas del subconjunto XSD (`docs/00-alcance.md` §3): la clase de
//! terminal de cada elemento del espacio de nombres de XML Schema, la lista
//! blanca de atributos de cada construcción y la categoría y el motivo de lo
//! que está fuera (§3.3, D6).
//!
//! Igual que las clases `K_*` de `esquema.ll1`, una clase `S_*` no es un
//! token nuevo del lector: es un `STAG` del espacio de nombres de XSD
//! clasificado por su nombre local, y solo en las posiciones de la gramática
//! que lo esperan (dentro de `xs:documentation`/`xs:appinfo` todo `STAG` es
//! un `STAG` cualquiera). `docs/fase2/gramatica-xsd.md` §1.

use crate::ast::FacetKind;
use crate::{Name, XS_NS};

/// Terminal refinado `S_*` de un `STAG` de XSD (`docs/fase2/xsd.ll1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Xs {
    Schema,
    Element,
    ComplexType,
    Sequence,
    Choice,
    All,
    Attribute,
    SimpleContent,
    Extension,
    SimpleType,
    Restriction,
    Facet(FacetKind),
    Annotation,
    Documentation,
    Appinfo,
}

/// Clase de un `STAG` en posición de construcción.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Class {
    Xs(Xs),
    /// De XSD pero fuera del subconjunto: categoría y motivo.
    Unsupported(&'static str, &'static str),
    /// En el espacio de nombres de XSD sin ser un elemento de XSD 1.0.
    Unknown,
    /// De otro espacio de nombres (o de ninguno).
    Foreign,
}

const COMPOSITION: &str = "composición de esquemas";
const COMPOSITION_WHY: &str = "el esquema se compila desde un solo documento, sin leer ni \
                               resolver otros";
const WILDCARD: &str = "comodín";
const WILDCARD_WHY: &str = "admite elementos o atributos no declarados; el autómata de cada \
                            elemento necesita el conjunto de nombres hijos cerrado";
const GROUP: &str = "grupo con nombre";
const GROUP_WHY: &str = "modelos o atributos reutilizables por referencia; se escribe el \
                         contenido en cada tipo";
const DERIVATION: &str = "derivación de tipos complejos";
const DERIVATION_WHY: &str = "extender o restringir un tipo complejo a partir de otro exige \
                              fusionar modelos de contenido y atributos; solo se admite \
                              xs:simpleContent con xs:extension de un tipo simple";
const VARIETY: &str = "lista o unión de tipos simples";
const VARIETY_WHY: &str = "solo se admiten tipos simples atómicos definidos por \
                           xs:restriction";
const IDENTITY: &str = "clase (c), no libre de contexto";
const IDENTITY_WHY: &str = "las restricciones de identidad comparan valores de partes \
                            arbitrarias del documento (unicidad y referencias cruzadas), que \
                            ningún autómata de pila con memoria acotada puede recordar";
const FACET: &str = "faceta fuera del subconjunto";
const FACET_WHY: &str = "las facetas admitidas son enumeration, pattern, las de rango \
                         (min/maxInclusive, min/maxExclusive) y las de longitud";
const XSD11: &str = "XML Schema 1.1";
const XSD11_WHY: &str = "el subconjunto es de XML Schema 1.0";

/// Elementos de XSD 1.0 (y los de 1.1 más comunes) fuera del subconjunto
/// (`00-alcance.md` §3.3).
const UNSUPPORTED_ELEMENTS: [(&str, &str, &str); 23] = [
    ("import", COMPOSITION, COMPOSITION_WHY),
    ("include", COMPOSITION, COMPOSITION_WHY),
    ("redefine", COMPOSITION, COMPOSITION_WHY),
    (
        "notation",
        "notación",
        "declara formatos de datos externos para atributos NOTATION, sin efecto en la \
         validación del subconjunto",
    ),
    ("any", WILDCARD, WILDCARD_WHY),
    ("anyAttribute", WILDCARD, WILDCARD_WHY),
    ("group", GROUP, GROUP_WHY),
    ("attributeGroup", GROUP, GROUP_WHY),
    ("complexContent", DERIVATION, DERIVATION_WHY),
    ("list", VARIETY, VARIETY_WHY),
    ("union", VARIETY, VARIETY_WHY),
    ("key", IDENTITY, IDENTITY_WHY),
    ("keyref", IDENTITY, IDENTITY_WHY),
    ("unique", IDENTITY, IDENTITY_WHY),
    ("selector", IDENTITY, IDENTITY_WHY),
    ("field", IDENTITY, IDENTITY_WHY),
    ("totalDigits", FACET, FACET_WHY),
    ("fractionDigits", FACET, FACET_WHY),
    ("whiteSpace", FACET, FACET_WHY),
    ("override", XSD11, XSD11_WHY),
    ("assert", XSD11, XSD11_WHY),
    ("alternative", XSD11, XSD11_WHY),
    ("openContent", XSD11, XSD11_WHY),
];

/// Clase de un `STAG` por su nombre expandido. O(|nombre local|) más la
/// comparación del URI con la constante [`XS_NS`].
pub(crate) fn classify(name: &Name) -> Class {
    if name.ns() != Some(XS_NS) {
        return Class::Foreign;
    }
    let local = name.local();
    let xs = match local {
        "schema" => Xs::Schema,
        "element" => Xs::Element,
        "complexType" => Xs::ComplexType,
        "sequence" => Xs::Sequence,
        "choice" => Xs::Choice,
        "all" => Xs::All,
        "attribute" => Xs::Attribute,
        "simpleContent" => Xs::SimpleContent,
        "extension" => Xs::Extension,
        "simpleType" => Xs::SimpleType,
        "restriction" => Xs::Restriction,
        "annotation" => Xs::Annotation,
        "documentation" => Xs::Documentation,
        "appinfo" => Xs::Appinfo,
        _ => {
            if let Some(k) = FacetKind::ALL.iter().find(|k| k.local() == local) {
                Xs::Facet(*k)
            } else if let Some(&(_, cat, why)) =
                UNSUPPORTED_ELEMENTS.iter().find(|(n, _, _)| *n == local)
            {
                return Class::Unsupported(cat, why);
            } else {
                return Class::Unknown;
            }
        }
    };
    Class::Xs(xs)
}

/// Categoría y motivo de `xs:restriction` dentro de `xs:simpleContent`:
/// restringir un tipo complejo de contenido simple es derivar un tipo
/// complejo de otro (D27).
pub(crate) const SIMPLE_CONTENT_RESTRICTION: (&str, &str) = (DERIVATION, DERIVATION_WHY);

/// Contexto de una construcción: decide su lista blanca de atributos, los
/// obligatorios y el nombre que muestran los mensajes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ctx {
    Schema,
    GlobalElement,
    LocalElement,
    GlobalComplexType,
    LocalComplexType,
    Sequence,
    Choice,
    All,
    GlobalAttribute,
    LocalAttribute,
    SimpleContent,
    Extension,
    GlobalSimpleType,
    LocalSimpleType,
    Restriction,
    Facet(FacetKind),
    Annotation,
    Documentation,
    Appinfo,
}

impl Ctx {
    /// Nombre de la construcción en los mensajes (con el prefijo fijo `xs:`,
    /// sea cual sea el del documento).
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Ctx::Schema => "xs:schema",
            Ctx::GlobalElement => "xs:element global",
            Ctx::LocalElement => "xs:element local",
            Ctx::GlobalComplexType => "xs:complexType global",
            Ctx::LocalComplexType => "xs:complexType anónimo",
            Ctx::Sequence => "xs:sequence",
            Ctx::Choice => "xs:choice",
            Ctx::All => "xs:all",
            Ctx::GlobalAttribute => "xs:attribute global",
            Ctx::LocalAttribute => "xs:attribute local",
            Ctx::SimpleContent => "xs:simpleContent",
            Ctx::Extension => "xs:extension",
            Ctx::GlobalSimpleType => "xs:simpleType global",
            Ctx::LocalSimpleType => "xs:simpleType anónimo",
            Ctx::Restriction => "xs:restriction",
            Ctx::Facet(k) => match k {
                FacetKind::Enumeration => "xs:enumeration",
                FacetKind::MinInclusive => "xs:minInclusive",
                FacetKind::MaxInclusive => "xs:maxInclusive",
                FacetKind::MinExclusive => "xs:minExclusive",
                FacetKind::MaxExclusive => "xs:maxExclusive",
                FacetKind::Length => "xs:length",
                FacetKind::MinLength => "xs:minLength",
                FacetKind::MaxLength => "xs:maxLength",
                FacetKind::Pattern => "xs:pattern",
            },
            Ctx::Annotation => "xs:annotation",
            Ctx::Documentation => "xs:documentation",
            Ctx::Appinfo => "xs:appinfo",
        }
    }

    /// Lista blanca de atributos sin espacio de nombres (`00-alcance.md`
    /// §3.1; `id` en todas, D23). Las declaraciones `xmlns` no cuentan, y
    /// los atributos de otro espacio de nombres que el de XSD se aceptan y
    /// se ignoran en todas las construcciones (D34). `attributeFormDefault`,
    /// `form`, `nillable` y `abstract` solo se admiten con el valor que no
    /// cambia la validación ([`default_only`], D39).
    pub(crate) fn allowed(self) -> &'static [&'static str] {
        match self {
            Ctx::Schema => &[
                "targetNamespace",
                "elementFormDefault",
                "attributeFormDefault",
                "id",
            ],
            Ctx::GlobalElement => &["name", "type", "nillable", "abstract", "id"],
            Ctx::LocalElement => &[
                "name",
                "type",
                "ref",
                "minOccurs",
                "maxOccurs",
                "form",
                "nillable",
                "id",
            ],
            Ctx::GlobalComplexType => &["name", "mixed", "abstract", "id"],
            Ctx::LocalComplexType => &["mixed", "id"],
            Ctx::Sequence | Ctx::Choice | Ctx::All => &["minOccurs", "maxOccurs", "id"],
            Ctx::GlobalAttribute => &["name", "type", "default", "fixed", "id"],
            Ctx::LocalAttribute => &["name", "type", "use", "default", "fixed", "form", "id"],
            Ctx::SimpleContent | Ctx::Annotation => &["id"],
            Ctx::Extension | Ctx::Restriction => &["base", "id"],
            Ctx::GlobalSimpleType => &["name", "id"],
            Ctx::LocalSimpleType => &["id"],
            Ctx::Facet(_) => &["value", "id"],
            Ctx::Documentation | Ctx::Appinfo => &["source"],
        }
    }

    /// Atributos obligatorios en XSD 1.0 que la construcción no puede omitir
    /// en el subconjunto. Los que dependen de otro (`name` o `ref` en un
    /// `xs:element` local; `base` o un `xs:simpleType` en `xs:restriction`)
    /// son de la 5.3.
    pub(crate) fn required(self) -> &'static [&'static str] {
        match self {
            Ctx::GlobalElement
            | Ctx::GlobalComplexType
            | Ctx::GlobalAttribute
            | Ctx::GlobalSimpleType
            // Sin `ref` de atributo (fuera del subconjunto), `name` es
            // obligatorio también en los locales.
            | Ctx::LocalAttribute => &["name"],
            Ctx::Extension => &["base"],
            Ctx::Facet(_) => &["value"],
            _ => &[],
        }
    }
}

const FORM: &str = "calificación de nombres";
const FORM_WHY: &str = "los elementos se comparan por nombre local en un único espacio de \
                        nombres (00-alcance.md §1.2)";
const DERIVATION_CONTROL: &str = "control de derivación";
const DERIVATION_CONTROL_WHY: &str = "limita qué tipos o sustituciones se pueden derivar; sin \
                                      derivación de tipos complejos ni sustitución no tiene \
                                      efecto";
const SUBSTITUTION: &str = "grupos de sustitución y tipos abstractos";
const SUBSTITUTION_WHY: &str = "permiten que otro elemento o tipo ocupe el lugar del \
                                declarado, lo que abre el conjunto de nombres hijos";
const NIL: &str = "valores nulos";
const NIL_WHY: &str = "xsi:nil en un documento instancia permitiría un elemento vacío aunque \
                       su tipo exija contenido, y el validador no interpreta los atributos \
                       xsi de los documentos";
const VALUE_CONSTRAINT: &str = "valor por defecto o fijo de un elemento";
const VALUE_CONSTRAINT_WHY: &str = "solo se admiten default y fixed en xs:attribute";

/// D39: atributos de la lista blanca que solo se admiten con el valor que no
/// cambia la validación (el valor por defecto de XSD, o en `form` el que ya
/// rige), y que se ignoran; cualquier otro valor válido es `unsupported`
/// con esta categoría y este motivo.
pub(crate) fn default_only(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "attributeFormDefault" | "form" => Some((FORM, FORM_WHY)),
        "nillable" => Some((NIL, NIL_WHY)),
        "abstract" => Some((SUBSTITUTION, SUBSTITUTION_WHY)),
        _ => None,
    }
}

/// Atributos que XSD define en la construcción `ctx` pero el subconjunto no
/// admite: categoría y motivo (D6). `None`: no es un atributo de XSD en ese
/// contexto (error `syntax`).
pub(crate) fn unsupported_attribute(ctx: Ctx, local: &str) -> Option<(&'static str, &'static str)> {
    let r = match (ctx, local) {
        (Ctx::GlobalElement, "substitutionGroup") => (SUBSTITUTION, SUBSTITUTION_WHY),
        (Ctx::GlobalElement | Ctx::LocalElement, "default" | "fixed") => {
            (VALUE_CONSTRAINT, VALUE_CONSTRAINT_WHY)
        }
        (Ctx::GlobalElement | Ctx::LocalElement | Ctx::GlobalComplexType, "block")
        | (Ctx::GlobalElement | Ctx::GlobalComplexType | Ctx::GlobalSimpleType, "final")
        | (Ctx::Schema, "blockDefault" | "finalDefault") => {
            (DERIVATION_CONTROL, DERIVATION_CONTROL_WHY)
        }
        // XSD define `fixed` en las facetas de rango y de longitud;
        // `enumeration` y `pattern` son `xs:noFixedFacet` (sin `fixed`): ahí es
        // un atributo desconocido (`syntax`).
        (
            Ctx::Facet(
                FacetKind::MinInclusive
                | FacetKind::MaxInclusive
                | FacetKind::MinExclusive
                | FacetKind::MaxExclusive
                | FacetKind::Length
                | FacetKind::MinLength
                | FacetKind::MaxLength,
            ),
            "fixed",
        ) => (
            DERIVATION_CONTROL,
            "fixed en una faceta solo impide que otros tipos la cambien al derivar",
        ),
        (Ctx::Schema, "version") => (
            "metadato",
            "solo se aceptan e ignoran id y xs:annotation (D23)",
        ),
        (Ctx::LocalAttribute, "ref") => (
            "referencia a un atributo global",
            "xs:attribute admite name, type, use, default y fixed (00-alcance.md §3.1); los \
             atributos se declaran en cada tipo",
        ),
        _ => return None,
    };
    Some(r)
}
