//! Errores del análisis sintáctico del esquema, con línea y columna.

use core::fmt;

use lexer::{LexErrorKind, Pos};

/// Primer error encontrado en el esquema. El parser se detiene en él.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ErrorKind,
    /// Posición del token (o byte) que provoca el error.
    pub at: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Error léxico del tokenizador JSON de la Fase 1.
    Lex(LexErrorKind),
    /// Celda vacía de la tabla LL(1): el token no puede seguir aquí.
    Unexpected {
        /// Qué admitía la gramática en este punto.
        expected: String,
        /// Descripción del token encontrado.
        found: String,
        /// Pista para formas conocidas fuera del subconjunto.
        hint: Option<&'static str>,
    },
    /// Clave repetida en un mismo objeto (palabra clave, entrada de un mapa
    /// o miembro de un valor JSON).
    DuplicateKey(String),
    /// Se superó la profundidad máxima de anidamiento de `{`/`[`.
    TooDeep { max: u32 },
}

impl ParseError {
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
            ErrorKind::Unexpected { .. } => "syntax",
            ErrorKind::DuplicateKey(_) => "duplicate-key",
            ErrorKind::TooDeep { .. } => "limit",
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "línea {}, columna {}: ", self.at.line, self.at.column)?;
        match &self.kind {
            ErrorKind::Lex(k) => write!(f, "error léxico: {}", lex_message(*k)),
            ErrorKind::Unexpected {
                expected,
                found,
                hint,
            } => {
                write!(f, "se esperaba {expected}, se encontró {found}")?;
                match hint {
                    Some(h) => write!(f, " ({h})"),
                    None => Ok(()),
                }
            }
            ErrorKind::DuplicateKey(k) => write!(f, "clave repetida \"{}\"", short(k)),
            ErrorKind::TooDeep { max } => {
                write!(
                    f,
                    "el esquema supera la profundidad máxima de {max} niveles"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Texto del usuario para un mensaje: con los caracteres de control escapados
/// (el mensaje ocupa siempre una línea) y recortado a 32 caracteres. Público
/// para que los errores de la Fase 3 sigan la misma regla.
pub fn short(s: &str) -> String {
    const MAX: usize = 32;
    let cut: String = s.chars().take(MAX).collect();
    let more = if s.chars().nth(MAX).is_some() {
        "…"
    } else {
        ""
    };
    format!("{}{more}", cut.escape_debug())
}

fn lex_message(k: LexErrorKind) -> &'static str {
    match k {
        LexErrorKind::UnexpectedByte => "byte inesperado",
        LexErrorKind::InvalidLiteral => "literal mal escrito (se admiten true, false y null)",
        LexErrorKind::InvalidNumber => "número mal formado",
        LexErrorKind::ControlCharInString => "carácter de control sin escapar en una cadena",
        LexErrorKind::InvalidEscape => "secuencia de escape inválida",
        LexErrorKind::LoneSurrogate => "sustituto UTF-16 sin pareja",
        LexErrorKind::InvalidUtf8 => "UTF-8 inválido",
        LexErrorKind::UnexpectedEof => "la entrada termina en medio de un token",
        // Los demás son propios de XML o internos; no los produce el lexer JSON.
        LexErrorKind::DoctypeForbidden
        | LexErrorKind::InvalidReference
        | LexErrorKind::UndefinedEntity
        | LexErrorKind::InvalidCharRef
        | LexErrorKind::InvalidXmlChar
        | LexErrorKind::InvalidPi
        | LexErrorKind::Aborted => "entrada inválida",
    }
}
