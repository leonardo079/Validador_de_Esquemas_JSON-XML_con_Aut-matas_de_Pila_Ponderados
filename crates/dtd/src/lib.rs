//! Frontal DTD (D86; `docs/00-alcance.md` §3bis).
//!
//! Un subconjunto de DTD **externa**, dada como archivo de esquema aparte, que
//! baja a la misma IR que JSON Schema y XSD. Por fases del curso:
//!
//! - Fase 1, [`lexer`]: AFD de tabla con salida, en streaming, que rechaza
//!   como error léxico lo prohibido por seguridad (`<!ENTITY`, `<!DOCTYPE`,
//!   `<![`, `%nombre;`, entidades no predefinidas).
//! - Fase 2, [`parser`] y [`ast`]: parser LL(1) iterativo (`docs/fase2/dtd.ll1`)
//!   al AST propio (D3), con los rechazos por alcance (§3bis.2).
//! - Fase 3 (DTD.3): tabla de símbolos, determinismo y bajada a la IR.
//!
//! No es `no_std`: la DTD solo se lee al compilar, nunca dentro del validador
//! generado (por eso el lexer vive aquí y no en `crates/lexer`).

pub mod ast;
pub mod lexer;
pub mod parser;

pub use parser::{parse, ErrorKind, ParseError, MAX_DEPTH};
