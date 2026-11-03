//! Frontal XSD (D21): de un documento de esquema XSD a la IR común.
//!
//! Hoy contiene la primera pieza, el **lector XML** ([`Reader`], tarea 5.1):
//! adapta los eventos del tokenizador XML de la Fase 1 ([`lexer::xml`]) a
//! tokens bajo demanda para un parser LL(1), comprueba la buena formación que
//! la gramática XML no expresa y resuelve los espacios de nombres. Las
//! tareas siguientes añaden la gramática y el AST del subconjunto XSD, el
//! análisis y la bajada a la IR. Todo el crate es de tiempo de compilación:
//! puede reservar memoria, pero no usa recursión sobre la entrada (D7).
//!
//! Depende solo de `lexer` (y de `ir` cuando llegue la bajada); no de
//! `semantic` ni de `schema-parser` (D13, D21). Teoría en
//! `docs/fase2/lector-xsd.md`.

pub mod error;
mod reader;

pub use error::{ErrorKind, XsdError};
pub use lexer::{Pos, Span};
pub use reader::{Attr, Name, NsId, Reader, Tok, Token, XMLNS_NS, XML_NS, XS_NS};
