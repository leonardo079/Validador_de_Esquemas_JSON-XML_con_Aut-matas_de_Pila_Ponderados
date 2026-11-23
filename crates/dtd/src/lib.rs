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
mod check;
pub mod lexer;
mod lower;
pub mod parser;

pub use check::{analyze, Analysis, Flat, FlatKind, MAX_PARTICLES};
pub use lower::lower;
pub use parser::{parse, DtdError, ErrorKind, MAX_DEPTH};

/// Toda la tubería del frontal: texto de la DTD → IR verificada. Para la
/// CLI y el servidor (DTD.4) y las pruebas.
///
/// # Errors
///
/// El primer error léxico o sintáctico, o todos los semánticos; un fallo de
/// la bajada (que no debería ocurrir) llega como un único error interno.
pub fn compile(input: &[u8]) -> Result<ir::VerifiedIr, Compiled> {
    let dtd = parse(input).map_err(|e| Compiled::Schema(vec![e]))?;
    let analysis = analyze(&dtd).map_err(Compiled::Schema)?;
    lower(&analysis).map_err(Compiled::Internal)
}

/// Por qué no se pudo compilar una DTD.
#[derive(Debug)]
pub enum Compiled {
    /// Errores del esquema, ordenados por posición.
    Schema(Vec<DtdError>),
    /// La IR no cumplió sus invariantes (D15).
    Internal(ir::InternalError),
}
