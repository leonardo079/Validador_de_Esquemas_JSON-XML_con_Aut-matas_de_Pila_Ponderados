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
//! - la detección de ciclos no productivos ([`check_cycles`]): ciclos de
//!   `$ref` y aplicadores sobre la misma instancia; teoría en
//!   `docs/fase3/ciclos.md`.
//!
//! - la bajada a la IR común ([`lower`]): un nodo por subesquema, `$ref`
//!   como arista, la IR siempre verificada; teoría en
//!   `docs/fase3/bajada.md`.
//!
//! [`analyze`] encadena las tres primeras; [`lower`] parte de su resultado.

mod cycles;
pub mod error;
mod lower;
mod meta;
mod number;
mod pointer;
mod symbols;
mod unsupported;

pub use cycles::{application, check_cycles, Application, FRAGMENT_BYTES};
pub use error::{Category, ErrorKind, RefProblem, SemanticError};
pub use lower::lower;
pub use meta::{check_meta_schema, DIALECT, TYPE_NAMES};
pub use symbols::{Node, Ref, SchemaId, Step, SymbolTable};

use schema_parser::Schema;

/// Análisis semántico hecho hasta ahora, por etapas: meta-esquema; si pasa,
/// tabla de símbolos con los `$ref` resueltos; si pasa, ciclos no
/// productivos. Cada etapa necesita la anterior completa (un `$ref` remoto o
/// una palabra clave fuera del subconjunto ya tienen su error, y repetirlos
/// como referencias sin resolver confundiría; sin todos los `$ref` resueltos
/// el grafo de ciclos estaría incompleto), así que se devuelven los errores
/// de la primera etapa que falla.
pub fn analyze(schema: &Schema) -> Result<SymbolTable<'_>, Vec<SemanticError>> {
    check_meta_schema(schema)?;
    let table = SymbolTable::build(schema)?;
    check_cycles(&table)?;
    Ok(table)
}
