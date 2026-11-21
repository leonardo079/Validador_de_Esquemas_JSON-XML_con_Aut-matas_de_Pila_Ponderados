//! Fase 4 — Autómata de pila determinista ponderado.
//!
//! M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) con pesos en el semianillo tropical
//! (ℕ ∪ {∞}, mín, +, ∞, 0) (D4), construido por composición sobre una
//! [`VerifiedIr`] ([`build`]) directamente como tablas densas
//! ([`Automaton`], F4-U4). Las ejecuta el núcleo `no_std` de `runtime-rs`
//! (F4-U9): [`validate`] es la envoltura `std` para pruebas.
//!
//! F4.1a compiló el esqueleto JSON: el marco «cualquiera» (la gramática de
//! JSON como DPDA), `type`, `false`, `$ref` sin hermanas (alias),
//! `properties`/`additionalProperties` con trie de claves, `prefixItems`/
//! `items`, `required` y `dependentRequired` con máscara y las claves
//! declaradas repetidas con peso ∞ (D2). F4.1b añade los contadores
//! (`min`/`maxProperties`, `min`/`maxItems`, `min`/`maxLength`), `enum`/
//! `const` de escalares, los rangos y `multipleOf` exactos sobre el lexema, y
//! los marcos por conjunto canónico de `NodeId` (`$ref` con hermanas y
//! `allOf` como producto con tope, [`BuildOptions`]). Lo demás se rechaza en
//! compilación con la tarea que lo cubrirá ([`BuildError::Pending`], F4-U5).
//!
//! Teoría: `docs/fase4/automata.md`; pendientes: `docs/fase4/pendientes.md`.

mod conj;
pub mod determinism;
pub mod dfa;
mod emit;
mod export;
mod glushkov;
mod json;
mod location;
pub mod minimize;
mod model;
pub mod pattern;
mod text;
pub mod weight;
mod xml;

use std::fmt;

use ir::{NodeId, RuleKind, VerifiedIr};
use runtime_rs::{Flow, JsonValidator, XmlValidator};

pub use conj::MAX_CLOSURE_WORK;
pub use json::{MAX_DELTA, MAX_NAMES, MAX_PRODUCT_STATES};
pub use location::{location_clipped, LOCATION_MAX};
pub use model::{
    Automaton, ClassId, FrameId, PayloadFormat, StateId, StateInfo, Summary, TransId, ERROR,
};
pub use runtime_rs::{ErrorAt, ErrorKind, Limits, Mode, Options, Pointer, Report, POINTER_MAX};

/// Tarea del plan que implementará una regla pendiente, en el orden en que
/// llegan (F4-U5 con las etiquetas de D51).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Task {
    /// F4.4b: `pattern`.
    Pattern,
    /// F4.5a: `anyOf`, `oneOf`, `not`, `dependentSchemas`.
    Combinators,
    /// F4.5b: `uniqueItems` y literales compuestos.
    Literals,
}

impl Task {
    /// Identificador de la tarea en el plan.
    pub const fn label(self) -> &'static str {
        match self {
            Task::Pattern => "F4.4b",
            Task::Combinators => "F4.5a",
            Task::Literals => "F4.5b",
        }
    }
}

/// Por qué no se construyó M (F4-B11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    /// Una regla alcanzable que esta tarea aún no compila (F4-U5). Si hay
    /// varias, la de la tarea más tardía: el esquema compilará cuando esa
    /// esté hecha.
    Pending {
        node: NodeId,
        rule: RuleKind,
        task: Task,
        /// Ubicación del nodo recortada a [`LOCATION_MAX`] bytes.
        location: String,
    },
    /// Un límite de `docs/00-alcance.md` §4 (F4-U6), comprobado antes de
    /// reservar.
    Limit {
        what: &'static str,
        limit: usize,
        found: usize,
        location: String,
    },
    /// Un producto (el subautómata de un conjunto de ≥ 2 componentes) pasa
    /// del tope de estados ([`BuildOptions::max_product_states`]; L-1,
    /// F4-U11), contado antes de reservar. `states` es el número alcanzado
    /// y `scalar` dice si el marco es escalar (cadena o número), en el que
    /// F4.5a podrá recurrir a la simulación paralela; en un marco con hijos
    /// (objeto o arreglo) es un error `limit` definitivo.
    ProductLimit {
        states: usize,
        limit: usize,
        scalar: bool,
        location: String,
    },
    /// Defensa en profundidad: la IR verificada no cumple lo que la
    /// construcción supone. Nunca un error del esquema del usuario.
    Internal(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Pending {
                rule,
                task,
                location,
                ..
            } => write!(
                f,
                "estrategia pendiente: {} ({}) en {location}",
                rule.name(),
                task.label()
            ),
            BuildError::Limit {
                what,
                limit,
                found,
                location,
            } => write!(
                f,
                "límite superado: {found} {what} (máximo {limit}) en {location}"
            ),
            BuildError::ProductLimit {
                states,
                limit,
                scalar,
                location,
            } => write!(
                f,
                "límite superado: producto de {states} estados (máximo {limit}) en el marco {} de {location}",
                if *scalar { "escalar" } else { "con hijos" }
            ),
            BuildError::Internal(m) => write!(f, "error interno del constructor del autómata: {m}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Opciones del constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildOptions {
    /// Tope de estados de cada producto (L-1; 4 096 por defecto,
    /// `docs/00-alcance.md` §4).
    pub max_product_states: usize,
    /// Presupuesto de entradas de las tablas por esquema (F4-U6, precisado
    /// en F4.1b: |Q| · |Σ| más comprobaciones, máscaras, dependencias y
    /// lexemas; 2²⁰ por defecto). Configurable a la baja para las pruebas.
    pub max_delta: usize,
}

impl Default for BuildOptions {
    fn default() -> BuildOptions {
        BuildOptions {
            max_product_states: MAX_PRODUCT_STATES,
            max_delta: MAX_DELTA,
        }
    }
}

/// Construye M para la IR (solo JSON hasta F4.1c) con las opciones por
/// defecto. Iterativo (D7): se puede llamar en un hilo de poca pila.
pub fn build(ir: &VerifiedIr) -> Result<Automaton, BuildError> {
    build_with(ir, &BuildOptions::default())
}

/// [`build`] con un tope de productos distinto.
pub fn build_with(ir: &VerifiedIr, opts: &BuildOptions) -> Result<Automaton, BuildError> {
    let a = match ir.format() {
        ir::Format::Json => json::build(ir, opts),
        ir::Format::Xml { .. } => xml::build(ir, opts),
    }?;
    // F4-B16: el determinismo de δ se verifica en cada construcción.
    determinism::check(&a).map_err(|e| {
        let lista: Vec<String> = e.iter().map(ToString::to_string).collect();
        BuildError::Internal(format!("M no es determinista: {}", lista.join("; ")))
    })?;
    Ok(a)
}

/// Valida `payload` con M partiéndolo en bloques de `chunk` bytes, con los
/// límites por defecto de `docs/00-alcance.md` §4 (F4-B24: pruebas,
/// herramientas, arnés de métricas y servidor).
pub fn validate(a: &Automaton, payload: &[u8], mode: Mode, chunk: usize) -> Report {
    validate_with(a, payload, &Options::new(mode), chunk)
}

/// [`validate`] con otros límites. La memoria del ejecutor sale de
/// `opts.limits` (`Tables::state_words`, F4.1a-CR-S3), no del payload; si
/// no se puede reservar (límites absurdos), el reporte es `limit` en el
/// offset 0 sin leer nada.
pub fn validate_with(a: &Automaton, payload: &[u8], opts: &Options, chunk: usize) -> Report {
    let words = a.state_words(&opts.limits);
    let mut mem = Vec::new();
    if mem.try_reserve_exact(words).is_err() {
        let mut sin = [0u64; 0];
        let mut l = *opts;
        l.limits.max_payload = 0;
        return feed_all(a, &[0], &l, &mut sin, chunk);
    }
    mem.resize(words, 0);
    feed_all(a, payload, opts, &mut mem, chunk)
}

fn feed_all(
    a: &Automaton,
    payload: &[u8],
    opts: &Options,
    mem: &mut [u64],
    chunk: usize,
) -> Report {
    match a.format() {
        PayloadFormat::Json => {
            let mut v = JsonValidator::new(a.tables(), opts, mem);
            for c in payload.chunks(chunk.max(1)) {
                if v.feed(c) == Flow::Stop {
                    break;
                }
            }
            v.finish()
        }
        PayloadFormat::Xml => {
            let mut v = XmlValidator::new(a.tables(), opts, mem);
            for c in payload.chunks(chunk.max(1)) {
                if v.feed(c) == Flow::Stop {
                    break;
                }
            }
            v.finish()
        }
    }
}
