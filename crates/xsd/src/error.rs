//! Errores del frontal XSD, con línea y columna.
//!
//! Los producen el lector XML ([`crate::Reader`], tarea 5.1) y el parser del
//! subconjunto XSD ([`crate::parse`], tarea 5.2); las tareas siguientes
//! (análisis) añaden sus clases a [`ErrorKind`] y sus códigos a
//! [`XsdError::code`]. Todo error del crate se construye con
//! [`XsdError::new`], que recorta los textos del usuario.

use core::fmt;

use lexer::{short, LexErrorKind, Pos, SHORT_MAX};

use crate::XS_NS;

/// Primer error encontrado en el documento de esquema. El lector se detiene
/// en él y lo repite si se le piden más tokens; el parser también se detiene
/// en el primero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XsdError {
    pub kind: ErrorKind,
    /// Posición del carácter o del nombre que provoca el error.
    pub at: Pos,
}

/// Los textos del usuario que llevan las variantes (nombres, valores) se
/// guardan **recortados** a `SHORT_MAX + 1` caracteres: es todo lo que
/// muestra [`lexer::short`] en el mensaje (los primeros `SHORT_MAX` y, si hay
/// más, `…`), así que el mensaje no cambia, y copiar el error (el lector lo
/// repite en cada llamada tras fallar) no copia un nombre o valor de varios
/// MiB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Error léxico del tokenizador XML de la Fase 1 (incluido
    /// `DoctypeForbidden`, D8).
    Lex(LexErrorKind),
    /// `</b>` cierra un elemento abierto con otro nombre.
    MismatchedEndTag { open: String, close: String },
    /// Etiqueta de cierre sin ningún elemento abierto.
    UnopenedEndTag(String),
    /// La entrada termina con un elemento sin cerrar.
    UnclosedElement(String),
    /// El documento no tiene elemento raíz.
    NoRootElement,
    /// Un segundo elemento después de cerrar la raíz.
    MultipleRootElements,
    /// Texto que no es de espacios (o una referencia) fuera del elemento raíz.
    TextOutsideRoot,
    /// Dos atributos con el mismo nombre (escrito o expandido) en una etiqueta.
    DuplicateAttribute(String),
    /// Declaración `<?xml …?>` que no es lo primero del documento.
    XmlDeclNotFirst,
    /// Instrucción de procesamiento con un nombre reservado (`XML`, `xMl`…).
    ReservedPiTarget(String),
    /// Declaración `<?xml …?>` mal formada; el texto dice qué falta o sobra.
    BadXmlDecl(&'static str),
    /// `encoding` distinto de UTF-8.
    UnsupportedEncoding(String),
    /// Nombre (de etiqueta, de atributo o en un valor) que no es un QName.
    InvalidQName(String),
    /// Prefijo sin declaración `xmlns:p` en el ámbito.
    UndeclaredPrefix(String),
    /// Declaración de espacio de nombres prohibida por Namespaces in XML 1.0
    /// (prefijos `xml`/`xmlns`, `xmlns:p=""`); el texto dice cuál.
    ReservedNamespace(&'static str),

    // --- Parser del subconjunto XSD (tarea 5.2, `docs/fase2/gramatica-xsd.md`).
    /// Un elemento pasa la profundidad máxima de anidamiento (D7). Lo detecta
    /// el lector en el `<` del elemento, sin leer más.
    TooDeep { max: u32 },
    /// Celda vacía de la tabla LL(1) (`docs/fase2/xsd.ll1`): el token no
    /// puede seguir aquí. `expected` lista lo que admitía la fila.
    Unexpected {
        expected: &'static str,
        found: Found,
    },
    /// Texto que no es de espacios en un contenido de solo elementos (todo
    /// elemento de XSD salvo `xs:documentation` y `xs:appinfo`).
    TextNotAllowed { parent: &'static str },
    /// Elemento fuera del espacio de nombres de XML Schema donde el esquema
    /// solo admite elementos de XSD.
    ForeignElement(String),
    /// Nombre del espacio de nombres de XML Schema que no es ningún elemento
    /// de XSD 1.0.
    UnknownElement(String),
    /// Construcción de XSD fuera del subconjunto (`00-alcance.md` §3.3, D6):
    /// su categoría y por qué no se soporta.
    UnsupportedElement {
        name: String,
        category: &'static str,
        reason: &'static str,
    },
    /// Atributo de XSD que el subconjunto no admite en esta construcción
    /// (`form`, `nillable`, `abstract`…; D6).
    UnsupportedAttribute {
        element: &'static str,
        name: String,
        category: &'static str,
        reason: &'static str,
    },
    /// Atributo que no está en la lista blanca de su construcción y que XSD
    /// tampoco define ahí.
    UnknownAttribute {
        element: &'static str,
        name: String,
        allowed: &'static [&'static str],
    },
    /// Falta un atributo obligatorio de la construcción.
    MissingAttribute {
        element: &'static str,
        name: &'static str,
    },
    /// Valor de atributo que no tiene la forma que la gramática necesita
    /// (`use`, `mixed`, `elementFormDefault`, `targetNamespace` vacío…).
    InvalidValue {
        attr: &'static str,
        expected: &'static str,
        found: String,
    },
    /// Valor que XSD admite pero el subconjunto no (`mixed="true"`,
    /// `use="prohibited"`; D6).
    UnsupportedValue {
        attr: &'static str,
        found: String,
        category: &'static str,
        reason: &'static str,
    },
}

/// Token encontrado en una celda vacía de la tabla LL(1), para el mensaje.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// `<nombre` (el nombre tal como está escrito).
    Tag(String),
    /// `</nombre>`.
    EndTag(String),
    /// `/>`.
    EmptyClose,
    /// Fin de la entrada.
    Eof,
}

/// Copia de `s` recortada a `SHORT_MAX + 1` caracteres. O(SHORT_MAX).
pub(crate) fn clip(s: &str) -> String {
    match s.char_indices().nth(SHORT_MAX + 1) {
        Some((cut, _)) => s[..cut].to_owned(),
        None => s.to_owned(),
    }
}

/// Recorta `s` en su sitio y libera lo que sobra.
fn clip_in_place(s: &mut String) {
    if let Some((cut, _)) = s.char_indices().nth(SHORT_MAX + 1) {
        s.truncate(cut);
        s.shrink_to_fit();
    }
}

impl ErrorKind {
    /// La misma clase con sus textos recortados (ver arriba).
    fn clipped(mut self) -> Self {
        match &mut self {
            ErrorKind::MismatchedEndTag { open, close } => {
                clip_in_place(open);
                clip_in_place(close);
            }
            ErrorKind::UnopenedEndTag(s)
            | ErrorKind::UnclosedElement(s)
            | ErrorKind::DuplicateAttribute(s)
            | ErrorKind::ReservedPiTarget(s)
            | ErrorKind::UnsupportedEncoding(s)
            | ErrorKind::InvalidQName(s)
            | ErrorKind::UndeclaredPrefix(s)
            | ErrorKind::ForeignElement(s)
            | ErrorKind::UnknownElement(s)
            | ErrorKind::Unexpected {
                found: Found::Tag(s) | Found::EndTag(s),
                ..
            }
            | ErrorKind::UnsupportedElement { name: s, .. }
            | ErrorKind::UnsupportedAttribute { name: s, .. }
            | ErrorKind::UnknownAttribute { name: s, .. }
            | ErrorKind::InvalidValue { found: s, .. }
            | ErrorKind::UnsupportedValue { found: s, .. } => clip_in_place(s),
            ErrorKind::Lex(_)
            | ErrorKind::NoRootElement
            | ErrorKind::MultipleRootElements
            | ErrorKind::TextOutsideRoot
            | ErrorKind::XmlDeclNotFirst
            | ErrorKind::BadXmlDecl(_)
            | ErrorKind::ReservedNamespace(_)
            | ErrorKind::TooDeep { .. }
            | ErrorKind::Unexpected { .. }
            | ErrorKind::TextNotAllowed { .. }
            | ErrorKind::MissingAttribute { .. } => {}
        }
        self
    }
}

impl XsdError {
    /// Único constructor dentro del crate: recorta los textos del usuario
    /// que lleve `kind` a `SHORT_MAX + 1` caracteres ([`ErrorKind`]). Quien
    /// lo llama pasa ya una copia recortada (`clip(&nombre)`), así que crear
    /// un error no copia un nombre o un valor entero de varios MiB; el
    /// recorte de aquí garantiza la cota aunque alguien olvide hacerlo.
    pub(crate) fn new(kind: ErrorKind, at: Pos) -> Self {
        Self {
            kind: kind.clipped(),
            at,
        }
    }

    pub fn line(&self) -> u32 {
        self.at.line
    }

    pub fn column(&self) -> u32 {
        self.at.column
    }

    /// Nombre corto de la clase de error, estable para las pruebas y la web.
    pub fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::Lex(_) => "lexical",
            ErrorKind::MismatchedEndTag { .. }
            | ErrorKind::UnopenedEndTag(_)
            | ErrorKind::UnclosedElement(_)
            | ErrorKind::NoRootElement
            | ErrorKind::MultipleRootElements
            | ErrorKind::TextOutsideRoot
            | ErrorKind::XmlDeclNotFirst
            | ErrorKind::ReservedPiTarget(_)
            | ErrorKind::BadXmlDecl(_)
            | ErrorKind::UnsupportedEncoding(_) => "syntax",
            ErrorKind::DuplicateAttribute(_) => "duplicate-attribute",
            ErrorKind::InvalidQName(_)
            | ErrorKind::UndeclaredPrefix(_)
            | ErrorKind::ReservedNamespace(_) => "namespace",
            ErrorKind::TooDeep { .. } => "limit",
            ErrorKind::Unexpected { .. }
            | ErrorKind::TextNotAllowed { .. }
            | ErrorKind::ForeignElement(_)
            | ErrorKind::UnknownElement(_)
            | ErrorKind::UnknownAttribute { .. }
            | ErrorKind::MissingAttribute { .. } => "syntax",
            ErrorKind::UnsupportedElement { .. }
            | ErrorKind::UnsupportedAttribute { .. }
            | ErrorKind::UnsupportedValue { .. } => "unsupported",
            ErrorKind::InvalidValue { .. } => "value",
        }
    }
}

impl fmt::Display for XsdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "línea {}, columna {}: ", self.at.line, self.at.column)?;
        match &self.kind {
            ErrorKind::Lex(k) => write!(f, "error léxico: {}", lex_message(*k)),
            ErrorKind::MismatchedEndTag { open, close } => write!(
                f,
                "la etiqueta de cierre </{}> no corresponde a <{}>",
                short(close),
                short(open)
            ),
            ErrorKind::UnopenedEndTag(n) => {
                write!(f, "etiqueta de cierre </{}> sin elemento abierto", short(n))
            }
            ErrorKind::UnclosedElement(n) => {
                write!(f, "la entrada termina con <{}> sin cerrar", short(n))
            }
            ErrorKind::NoRootElement => write!(f, "el documento no tiene elemento raíz"),
            ErrorKind::MultipleRootElements => {
                write!(f, "solo puede haber un elemento raíz")
            }
            ErrorKind::TextOutsideRoot => {
                write!(f, "fuera del elemento raíz solo puede haber espacios")
            }
            ErrorKind::DuplicateAttribute(n) => write!(f, "atributo repetido \"{}\"", short(n)),
            ErrorKind::XmlDeclNotFirst => {
                write!(
                    f,
                    "la declaración <?xml …?> solo puede ir al inicio del documento"
                )
            }
            ErrorKind::ReservedPiTarget(n) => write!(
                f,
                "\"{}\" es un nombre reservado de instrucción de procesamiento",
                short(n)
            ),
            ErrorKind::BadXmlDecl(m) => write!(f, "declaración <?xml …?> mal formada: {m}"),
            ErrorKind::UnsupportedEncoding(e) => write!(
                f,
                "codificación \"{}\" no soportada: solo se admite UTF-8",
                short(e)
            ),
            ErrorKind::InvalidQName(n) => write!(f, "\"{}\" no es un QName", short(n)),
            ErrorKind::UndeclaredPrefix(p) => {
                write!(f, "prefijo \"{}\" sin declarar", short(p))
            }
            ErrorKind::ReservedNamespace(m) => write!(f, "{m}"),
            ErrorKind::TooDeep { max } => write!(
                f,
                "el esquema supera la profundidad máxima de {max} elementos anidados"
            ),
            ErrorKind::Unexpected { expected, found } => {
                write!(f, "se esperaba {expected}, se encontró ")?;
                match found {
                    Found::Tag(n) => write!(f, "<{}>", short(n)),
                    Found::EndTag(n) => write!(f, "</{}>", short(n)),
                    Found::EmptyClose => write!(f, "`/>`"),
                    Found::Eof => write!(f, "el fin de la entrada"),
                }
            }
            ErrorKind::TextNotAllowed { parent } => write!(
                f,
                "texto en el contenido de {parent}, que solo admite elementos \
                 (y espacios entre ellos)"
            ),
            ErrorKind::ForeignElement(n) => write!(
                f,
                "el elemento <{}> no es del espacio de nombres de XML Schema ({XS_NS}); \
                 solo se admite dentro de xs:documentation o xs:appinfo",
                short(n)
            ),
            ErrorKind::UnknownElement(n) => {
                write!(f, "<{}> no es un elemento de XML Schema 1.0", short(n))
            }
            ErrorKind::UnsupportedElement {
                name,
                category,
                reason,
            } => write!(
                f,
                "elemento <{}> no soportado ({category}): {reason}",
                short(name)
            ),
            ErrorKind::UnsupportedAttribute {
                element,
                name,
                category,
                reason,
            } => write!(
                f,
                "atributo \"{}\" no soportado en {element} ({category}): {reason}",
                short(name)
            ),
            ErrorKind::UnknownAttribute {
                element,
                name,
                allowed,
            } => write!(
                f,
                "atributo \"{}\" no admitido en {element}; admite: {}",
                short(name),
                allowed.join(", ")
            ),
            ErrorKind::MissingAttribute { element, name } => {
                write!(f, "falta el atributo obligatorio \"{name}\" en {element}")
            }
            ErrorKind::InvalidValue {
                attr,
                expected,
                found,
            } => write!(
                f,
                "valor \"{}\" inválido para \"{attr}\": se esperaba {expected}",
                short(found)
            ),
            ErrorKind::UnsupportedValue {
                attr,
                found,
                category,
                reason,
            } => write!(
                f,
                "valor \"{}\" de \"{attr}\" no soportado ({category}): {reason}",
                short(found)
            ),
        }
    }
}

impl std::error::Error for XsdError {}

fn lex_message(k: LexErrorKind) -> &'static str {
    match k {
        LexErrorKind::UnexpectedByte => "byte inesperado",
        LexErrorKind::InvalidUtf8 => "UTF-8 inválido",
        LexErrorKind::UnexpectedEof => "la entrada termina en medio de una marca",
        LexErrorKind::DoctypeForbidden => {
            "<!DOCTYPE no está permitido (sin DTD ni entidades declaradas)"
        }
        LexErrorKind::InvalidReference => "referencia mal formada",
        LexErrorKind::UndefinedEntity => "entidad no definida (solo lt, gt, amp, quot y apos)",
        LexErrorKind::InvalidCharRef => "referencia de carácter fuera de la producción Char",
        LexErrorKind::InvalidXmlChar => "carácter no permitido en XML",
        LexErrorKind::InvalidPi => "instrucción de procesamiento sin nombre",
        // Propios de JSON o internos; no los produce el lexer XML con este
        // lector (su sumidero detiene el lexer con su propio error).
        LexErrorKind::InvalidLiteral
        | LexErrorKind::InvalidNumber
        | LexErrorKind::ControlCharInString
        | LexErrorKind::InvalidEscape
        | LexErrorKind::LoneSurrogate
        | LexErrorKind::Aborted => "entrada inválida",
    }
}
