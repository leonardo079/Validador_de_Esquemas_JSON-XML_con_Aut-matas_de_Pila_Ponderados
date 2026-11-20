//! Propiedad del presupuesto de F4-U6 (D53 (5), §22.5 de
//! `docs/fase4/pendientes.md`; ronda 3 de F4.1c): lo que cuenta el análisis
//! antes de reservar es una **cota superior** de las entradas que de verdad
//! se emiten (δ, comprobaciones, máscaras, dependencias, lexemas, intervalos
//! y operaciones de contador). La construcción es monótona en `max_delta`
//! (cada comprobación es «cuenta > `max_delta` → `limit`»), así que basta
//! con ver que **no** compila con un presupuesto de una entrada menos que lo
//! emitido: si compilara, la cuenta se habría quedado corta (como el trie
//! del documento de I5).

use automaton::{build, build_with, Automaton, BuildError, BuildOptions};
use ir::VerifiedIr;

/// Entradas de las tablas que cubre el presupuesto. Los lexemas cuentan como
/// entradas (`nums`, D53 (5)), no en bytes: los umbrales e intervalos IEEE
/// son lexemas de ≈ 770 B que acota el presupuesto de pasos
/// (`docs/fase4/pendientes.md` §54).
pub fn entradas(a: &Automaton) -> usize {
    let t = a.tables();
    t.delta.len()
        + t.checks.len()
        + t.masks.len()
        + t.deps.len()
        + t.nums.len()
        + t.intervals.len()
        + t.counter_ops.len()
}

/// Si `ir` compila, el presupuesto contado cubre lo emitido. Devuelve las
/// entradas (0 si no compila).
pub fn cubre(ir: &VerifiedIr) -> usize {
    let Ok(a) = build(ir) else {
        return 0;
    };
    let n = entradas(&a);
    let menos = BuildOptions {
        max_delta: n - 1,
        ..BuildOptions::default()
    };
    match build_with(ir, &menos) {
        Err(BuildError::Limit { .. }) => n,
        otro => panic!(
            "el presupuesto no cubre las {n} entradas emitidas: con {} da {:?}",
            n - 1,
            otro.map(|a| a.summary())
        ),
    }
}
