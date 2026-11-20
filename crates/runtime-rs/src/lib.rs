//! Soporte del validador Rust generado: el núcleo `no_std` del ejecutor de
//! M (F4-U9, D51) y el formato de sus tablas.
//!
//! Sin `std` ni `alloc`: las tablas son *slices* prestados ([`Tables`]), la
//! pila de marcos es memoria del llamador y el peso vive en el semianillo
//! tropical ([`Weight`]). Lo construye y ejecuta `automaton` en las pruebas
//! (Fase 4); F4.6 añadió los límites ([`Limits`]), el reporte y la capa de
//! buena formación (`docs/fase4/runtime.md`), y la Fase 5 emite las tablas como `const` delante de este
//! mismo núcleo. Teoría: `docs/fase4/automata.md`.
#![no_std]

mod core;
pub mod hash;
mod json;
mod limits;
pub mod num;
pub mod tables;
mod weight;
mod xml;

pub use crate::core::{
    Core, ErrorAt, ErrorKind, Mode, Options, Report, Stop, FRAME_OVERHEAD, LEXEME_MAX,
};
pub use json::{Flow, JsonValidator};
pub use limits::Limits;
pub use tables::Tables;
pub use weight::Weight;
pub use xml::XmlValidator;
