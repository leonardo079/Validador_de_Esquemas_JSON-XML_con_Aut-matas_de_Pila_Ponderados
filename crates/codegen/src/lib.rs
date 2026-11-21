//! Fase 5 — Generación de código.
//!
//! [`rust::emit`] (F5.1, D74) escribe un validador Rust autocontenido: las
//! tablas densas de M como `static` (D46 (1): sin saltos directos) delante del
//! mismo núcleo `no_std` de `runtime-rs` y del lexer de la Fase 1, copiados
//! ([`fuentes`]), en archivos planos que compilan con
//! `rustc --edition 2021 -O main.rs`, sin Cargo ni red (el sandbox de F7.3).
//! El emisor C11 es F5.2. Teoría y formato: `docs/fase5/codegen.md`.

pub mod fuentes;
pub mod rust;

/// Un archivo emitido: nombre plano (`[a-z0-9_]+.rs`) y contenido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archivo {
    pub nombre: String,
    pub contenido: String,
}
