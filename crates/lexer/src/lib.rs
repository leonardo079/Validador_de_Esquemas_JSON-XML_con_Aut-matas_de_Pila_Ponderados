//! Fase 1 — Análisis léxico.
//!
//! Tokenizador en streaming construido como AFD explícito (tabla
//! `estado × clase_de_byte → estado`) para JSON y XML, con validación UTF-8.
//! Es `no_std` porque se incrusta en el validador generado (T5.6).
#![no_std]
