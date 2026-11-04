//! Frontal XSD (D21): de un documento de esquema XSD a la IR común.
//!
//! Piezas hechas:
//!
//! - el **lector XML** ([`Reader`], tarea 5.1): adapta los eventos del
//!   tokenizador XML de la Fase 1 ([`lexer::xml`]) a tokens bajo demanda,
//!   comprueba la buena formación que la gramática XML no expresa y resuelve
//!   los espacios de nombres (`docs/fase2/lector-xsd.md`);
//! - el **parser LL(1)** del subconjunto XSD ([`parse`], tarea 5.2): lee esos
//!   tokens con el `STAG` de XSD refinado por construcción, aplica la lista
//!   blanca de atributos, rechaza lo que está fuera del subconjunto con su
//!   categoría y su motivo (D6) y construye el [`ast`] propio de XSD (D3)
//!   (`docs/fase2/gramatica-xsd.md`).
//!
//! Las tareas siguientes añaden el análisis (tabla de símbolos,
//! restricciones) y la bajada a la IR. Todo el crate es de tiempo de
//! compilación: puede reservar memoria, pero no usa recursión sin cota sobre
//! la entrada (D7): el lector usa pilas explícitas y la recursión del parser
//! está acotada por [`MAX_DEPTH_LIMIT`].
//!
//! Depende solo de `lexer` (y de `ir` cuando llegue la bajada); no de
//! `semantic` ni de `schema-parser` (D13, D21).

pub mod ast;
pub mod error;
mod parser;
mod reader;
mod subset;

pub use error::{ErrorKind, Found, XsdError};
pub use lexer::{Pos, Span};
pub use parser::{parse, parse_chunks, parse_with_max_depth, DEFAULT_MAX_DEPTH, MAX_DEPTH_LIMIT};
pub use reader::{Attr, Name, NsId, Reader, Tok, Token, XMLNS_NS, XML_NS, XS_NS};
