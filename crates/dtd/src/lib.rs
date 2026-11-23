//! Frontal DTD (D86; `docs/00-alcance.md` §3bis).
//!
//! Un subconjunto de DTD **externa**, dada como archivo de esquema aparte, que
//! baja a la misma IR que JSON Schema y XSD. Por fases del curso:
//!
//! - Fase 1, [`lexer`]: AFD de tabla con salida, en streaming, que rechaza
//!   como error léxico lo prohibido por seguridad (`<!ENTITY`, `<!DOCTYPE`,
//!   `<![`, `%nombre;`, entidades no predefinidas).
//! - Fase 2 (DTD.2): parser LL(1) (`docs/fase2/dtd.ll1`) y AST propio (D3).
//! - Fase 3 (DTD.3): tabla de símbolos, determinismo y bajada a la IR.
//!
//! No es `no_std`: la DTD solo se lee al compilar, nunca dentro del validador
//! generado (por eso el lexer vive aquí y no en `crates/lexer`).

pub mod lexer;
