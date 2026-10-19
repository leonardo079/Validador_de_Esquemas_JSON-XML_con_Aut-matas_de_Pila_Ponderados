//! Tokens del esquema sobre el tokenizador JSON de la Fase 1.
//!
//! El lexer empuja eventos a un [`Sink`]; el parser descendente necesita
//! *pedir* el siguiente token. [`Tokens`] adapta uno al otro: alimenta el lexer
//! byte a byte hasta que el [`Assembler`] completa al menos un token. Un byte
//! produce a lo sumo dos tokens (el fin de un número se detecta con el byte
//! siguiente, que puede ser otro token), así que la cola nunca crece más.

use std::collections::VecDeque;
use std::convert::Infallible;

use lexer::json::{Event, JsonLexer};
use lexer::{Error, Pos, Sink, Span};

use crate::error::{short, ErrorKind, ParseError};

/// Terminal de la gramática. `Str` y `Num` llevan el texto ya decodificado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tok {
    LBrace,
    RBrace,
    LBrack,
    RBrack,
    Colon,
    Comma,
    True,
    False,
    Null,
    Str(String),
    Num(String),
    /// Fin de la entrada (`$`).
    Eof,
}

impl Tok {
    /// Descripción para los mensajes de error.
    pub(crate) fn describe(&self) -> String {
        match self {
            Tok::LBrace => "`{`".into(),
            Tok::RBrace => "`}`".into(),
            Tok::LBrack => "`[`".into(),
            Tok::RBrack => "`]`".into(),
            Tok::Colon => "`:`".into(),
            Tok::Comma => "`,`".into(),
            Tok::True => "`true`".into(),
            Tok::False => "`false`".into(),
            Tok::Null => "`null`".into(),
            Tok::Str(s) => format!("la cadena \"{}\"", short(s)),
            Tok::Num(n) => format!("el número {}", short(n)),
            Tok::Eof => "el fin de la entrada".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Arma tokens completos con los eventos del lexer.
#[derive(Debug, Default)]
struct Assembler {
    queue: VecDeque<Token>,
    /// Texto de la cadena o el número en curso.
    text: String,
}

impl Sink<Event> for Assembler {
    type Error = Infallible;

    fn event(&mut self, ev: Event, span: Span) -> Result<(), Infallible> {
        let tok = match ev {
            Event::BeginObject => Tok::LBrace,
            Event::EndObject => Tok::RBrace,
            Event::BeginArray => Tok::LBrack,
            Event::EndArray => Tok::RBrack,
            Event::Colon => Tok::Colon,
            Event::Comma => Tok::Comma,
            Event::True => Tok::True,
            Event::False => Tok::False,
            Event::Null => Tok::Null,
            Event::StringBegin | Event::NumberBegin => {
                self.text.clear();
                return Ok(());
            }
            Event::StringChar(c) => {
                self.text.push(c);
                return Ok(());
            }
            Event::Number(_, b) => {
                self.text.push(char::from(b));
                return Ok(());
            }
            Event::StringEnd => Tok::Str(std::mem::take(&mut self.text)),
            Event::NumberEnd => Tok::Num(std::mem::take(&mut self.text)),
        };
        self.queue.push_back(Token { tok, span });
        Ok(())
    }
}

/// Flujo de tokens bajo demanda sobre una entrada en memoria.
#[derive(Debug)]
pub(crate) struct Tokens<'a> {
    input: &'a [u8],
    /// Índice del próximo byte a entregar al lexer.
    next: usize,
    lexer: JsonLexer,
    sink: Assembler,
    finished: bool,
    /// Error léxico ocurrido mientras quedaban tokens en la cola: se entrega
    /// después de ellos, para que un error sintáctico anterior en la entrada
    /// se reporte primero.
    pending: Option<ParseError>,
}

impl<'a> Tokens<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            next: 0,
            lexer: JsonLexer::new(),
            sink: Assembler::default(),
            finished: false,
            pending: None,
        }
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        loop {
            if let Some(t) = self.sink.queue.pop_front() {
                return Ok(t);
            }
            if let Some(e) = self.pending.take() {
                // El lexer quedó detenido; el error se repite si se insiste.
                self.pending = Some(e.clone());
                return Err(e);
            }
            let res = if let Some(&b) = self.input.get(self.next) {
                self.next += 1;
                self.lexer.feed(&[b], &mut self.sink)
            } else if !self.finished {
                self.finished = true;
                self.lexer.finish(&mut self.sink)
            } else {
                let at: Pos = self.lexer.position();
                return Ok(Token {
                    tok: Tok::Eof,
                    span: Span {
                        start: at,
                        end: at.offset,
                    },
                });
            };
            if let Err(e) = res {
                let e = match e {
                    Error::Lex(e) => ParseError {
                        kind: ErrorKind::Lex(e.kind),
                        at: e.at,
                    },
                    Error::Sink(never) => match never {},
                };
                self.pending = Some(e);
            }
        }
    }
}
