//! Fase 5 — Generación de código.
//!
//! [`rust::emit`] (F5.1, D74) escribe un validador Rust autocontenido: las
//! tablas densas de M como `static` (D46 (1): sin saltos directos) delante del
//! mismo núcleo `no_std` de `runtime-rs` y del lexer de la Fase 1, copiados
//! ([`fuentes`]), en archivos planos que compilan con
//! `rustc --edition 2021 -O main.rs`, sin Cargo ni red (el sandbox de F7.3).
//! [`c::emit`] (F5.2, D75) escribe el mismo validador en C11: las tablas como
//! arreglos `static const` delante del núcleo C de `crates/runtime-c`, port
//! del de `runtime-rs`. Teoría y formato: `docs/fase5/codegen.md` y
//! `docs/fase5/codegen-c.md`.

pub mod c;
pub mod fuentes;
pub mod rust;

/// Un archivo emitido: nombre plano (`[a-z0-9_]+` con `.rs`, `.c` o `.h`) y
/// contenido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archivo {
    pub nombre: String,
    pub contenido: String,
}
