//! Fase 1 — Análisis léxico.
//!
//! Tokenizador en streaming construido como AFD explícito (tabla
//! `estado × clase_de_byte → estado`) para JSON y XML, con validación UTF-8.
//! Es `no_std` porque se incrusta en el validador generado (T5.6).
//!
//! El lexer no copia el contenido de los tokens: entrega **eventos** a un
//! [`Sink`] a medida que reconoce cada byte. Las cadenas llegan carácter a
//! carácter ya decodificadas (escapes y UTF-8), de modo que el autómata de la
//! Fase 4 puede aplicarles tries, `pattern` o `minLength` sin guardarlas en un
//! búfer.
#![no_std]

pub mod dot;
pub mod json;
mod table;
pub mod utf8;
pub mod xml;

/// Posición de un byte en la entrada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    /// Desplazamiento en bytes desde el inicio (base 0).
    pub offset: u64,
    /// Línea (base 1).
    pub line: u32,
    /// Columna en puntos de código (base 1).
    pub column: u32,
}

impl Pos {
    pub const START: Pos = Pos {
        offset: 0,
        line: 1,
        column: 1,
    };

    /// Avanza la posición sobre el byte `b`.
    #[inline]
    pub fn advance(&mut self, b: u8) {
        self.offset += 1;
        if b == b'\n' {
            self.line += 1;
            self.column = 1;
        } else if b & 0xC0 != 0x80 {
            // Los bytes de continuación UTF-8 no inician un punto de código.
            self.column += 1;
        }
    }
}

/// Rango de bytes `[start, end)` de un evento.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    /// Desplazamiento exclusivo del final.
    pub end: u64,
}

/// Error léxico con su posición.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub at: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexErrorKind {
    /// Byte que no puede iniciar ningún token.
    UnexpectedByte,
    /// `true`, `false` o `null` incompleto o mal escrito.
    InvalidLiteral,
    /// Número que no cumple RFC 8259 (p. ej. `01`, `1.`, `-`, `1e`).
    InvalidNumber,
    /// Carácter de control U+0000–U+001F sin escapar dentro de una cadena.
    ControlCharInString,
    /// Secuencia de escape inválida (`\x`, `\u12G4`).
    InvalidEscape,
    /// Sustituto UTF-16 sin pareja (`\uD800` solo, `\uDC00` solo).
    LoneSurrogate,
    /// Secuencia UTF-8 inválida (sobrelarga, sustituto, > U+10FFFF, truncada).
    InvalidUtf8,
    /// La entrada terminó en medio de un token.
    UnexpectedEof,
    /// XML: `<!DOCTYPE` (DTD prohibida; mitigación de XXE y *billion laughs*).
    DoctypeForbidden,
    /// XML: referencia mal formada (`&;`, `&#;`, `&#x;`, `& `).
    InvalidReference,
    /// XML: entidad que no es una de las 5 predefinidas (`&foo;`).
    UndefinedEntity,
    /// XML: referencia de carácter fuera de la producción `Char` (`&#0;`).
    InvalidCharRef,
    /// XML: carácter prohibido por la producción `Char` (controles, U+FFFE, U+FFFF).
    InvalidXmlChar,
}

/// Error devuelto por el lexer: léxico, o propagado desde el [`Sink`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<E> {
    Lex(LexError),
    Sink(E),
}

/// Consumidor de los eventos del lexer.
///
/// Devolver `Err` detiene el análisis (fallo temprano del validador).
pub trait Sink<Ev> {
    type Error;
    fn event(&mut self, ev: Ev, span: Span) -> Result<(), Self::Error>;
}
