//! Fase 2 — Análisis sintáctico.
//!
//! Parser LL(1) del metalenguaje de esquemas (subconjunto de JSON Schema
//! 2020-12 y de XSD) que produce un AST común. El AST solo existe en tiempo
//! de compilación.
//!
//! El parser de JSON Schema ([`parse`]) es descendente recursivo, escrito a
//! mano sobre la gramática de `docs/fase2/esquema.ll1`, y lee los tokens del
//! tokenizador JSON de la Fase 1 ([`lexer::json`]). Comprueba la **forma** de
//! cada palabra clave (qué tipo de valor lleva); las restricciones que la
//! gramática no expresa (rangos, nombres de tipo, `$ref`, palabras clave
//! fuera del subconjunto) son de la Fase 3. Todo error lleva línea y columna.

pub mod ast;
pub mod error;
mod json_schema;
mod token;

pub use ast::{Class, Json, JsonKind, Keyword, Kw, Num, Schema, SchemaKind, Str, Value};
pub use error::{ErrorKind, ParseError};
pub use json_schema::{parse, parse_with_max_depth, DEFAULT_MAX_DEPTH};
pub use lexer::{Pos, Span};
