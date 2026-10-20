//! Fase 3 — Análisis semántico del esquema.
//!
//! Validación contra el meta-esquema, tabla de símbolos de `$defs`/`$anchor`,
//! resolución de `$ref`, detección de ciclos no productivos y clasificación
//! de palabras clave en las categorías (a), (b) y (c).
//!
//! Hecho hasta ahora:
//!
//! - la validación contra el meta-esquema 2020-12 restringido al subconjunto
//!   ([`check_meta_schema`]), que también rechaza las palabras clave fuera
//!   del subconjunto con su categoría y su motivo (decisión D6); teoría en
//!   `docs/fase3/meta-esquema.md`;
//! - la tabla de símbolos (subesquemas con id estable y puntero JSON,
//!   `$defs`, `$anchor`) y la resolución de `$ref` ([`SymbolTable`]); teoría
//!   en `docs/fase3/referencias.md`.
//!
//! [`analyze`] encadena las dos.

pub mod error;
mod meta;
mod number;
mod pointer;
mod symbols;
mod unsupported;

pub use error::{Category, ErrorKind, RefProblem, SemanticError};
pub use meta::{check_meta_schema, DIALECT, TYPE_NAMES};
pub use symbols::{Node, Ref, SchemaId, Step, SymbolTable};

use schema_parser::Schema;

/// Análisis semántico hecho hasta ahora: meta-esquema y, si pasa, tabla de
/// símbolos con los `$ref` resueltos. Si el meta-esquema falla no se
/// resuelve nada: un `$ref` remoto o una palabra clave fuera del subconjunto
/// ya tienen su propio error, y repetirlos como referencias sin resolver solo
/// confundiría.
pub fn analyze(schema: &Schema) -> Result<SymbolTable<'_>, Vec<SemanticError>> {
    check_meta_schema(schema)?;
    SymbolTable::build(schema)
}
