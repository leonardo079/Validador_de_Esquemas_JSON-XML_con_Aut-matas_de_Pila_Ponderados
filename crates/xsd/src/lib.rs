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
//! - el **análisis** ([`analyze`], tarea 5.3): tabla de símbolos por
//!   `(NsId, nombre local)`, resolución de `type`/`ref`/`base` y
//!   restricciones de componente, con su posición, por etapas;
//! - la **bajada** a la IR común ([`lower`], tarea 5.3), siempre verificada
//!   (D15) (`docs/fase3/xsd.md`).
//!
//! Faltan los tipos simples del usuario y las facetas (5.5, 5.6) y *Unique
//! Particle Attribution* (5.7); `schemac` lo usa desde la tarea 5.4. Todo el
//! crate es de tiempo de compilación: puede reservar memoria, pero no usa
//! recursión sin cota sobre la entrada (D7): el lector, el análisis y la
//! bajada usan pilas explícitas y la recursión del parser está acotada por
//! [`MAX_DEPTH_LIMIT`].
//!
//! Depende solo de `lexer` e `ir`; no de `semantic` ni de `schema-parser`
//! (D13, D21).

pub mod ast;
mod check;
pub mod error;
mod lower;
mod parser;
mod reader;
mod subset;
mod symbols;

pub use check::{analyze, Analysis};
pub use error::{ErrorKind, Found, XsdError};
pub use lexer::{Pos, Span};
pub use lower::lower;
pub use parser::{parse, parse_chunks, parse_with_max_depth, DEFAULT_MAX_DEPTH, MAX_DEPTH_LIMIT};
pub use reader::{Attr, Name, NsId, Reader, Tok, Token, XMLNS_NS, XML_NS, XS_NS};
