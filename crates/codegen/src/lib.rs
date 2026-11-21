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

use automaton::{Automaton, Limits};

/// Palabras `u64` de memoria del núcleo que puede pedir un validador
/// emitido (`STATE_WORDS`, 128 MiB): con límites que pidan más,
/// [`Opciones::comprobar`] falla (F6.2).
pub const MAX_STATE_WORDS: usize = 16 << 20;

/// Con qué límites se emite un validador (F6.2, `docs/fase6/limites.md`):
/// los del núcleo y un tiempo máximo que vigila `main` entre bloques
/// (`None`: sin tiempo, como la biblioteca, `docs/00-alcance.md` §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opciones {
    pub limites: Limits,
    pub tiempo_ms: Option<u64>,
}

impl Default for Opciones {
    fn default() -> Opciones {
        Opciones {
            limites: Limits::DEFAULT,
            tiempo_ms: None,
        }
    }
}

impl Opciones {
    /// Las palabras de memoria del validador de `a` con estos límites, o
    /// el motivo por el que no se puede emitir: más de
    /// [`MAX_STATE_WORDS`] o una profundidad 0.
    pub fn comprobar(&self, a: &Automaton) -> Result<usize, String> {
        if self.limites.max_depth == 0 {
            return Err("maxDepth debe ser al menos 1".into());
        }
        let w = a.state_words(&self.limites);
        if w > MAX_STATE_WORDS {
            return Err(format!(
                "los límites piden {w} palabras de memoria; el máximo es {MAX_STATE_WORDS} \
                 (bajar maxDepth, keyPool o maxUnique)"
            ));
        }
        Ok(w)
    }
}

/// Nombres de los límites en `schemac compile --limit nombre=valor` (y en el
/// campo `limits` del corpus que se propone, `docs/fase6/limites.md` §3).
pub const NOMBRES_DE_LIMITES: [&str; 9] = [
    "maxDepth",
    "maxString",
    "maxLexeme",
    "maxKeys",
    "keyPool",
    "maxUnique",
    "maxXmlRefs",
    "maxPayload",
    "maxTimeMs",
];

impl Opciones {
    /// Fija el límite `nombre` (uno de [`NOMBRES_DE_LIMITES`]) a `valor`.
    pub fn fijar(&mut self, nombre: &str, valor: u64) -> Result<(), String> {
        let tam = || usize::try_from(valor).map_err(|_| format!("{nombre}: {valor} no cabe"));
        let l = &mut self.limites;
        match nombre {
            "maxDepth" => l.max_depth = tam()?,
            "maxString" => l.max_string = valor,
            "maxLexeme" => l.max_lexeme = tam()?,
            "maxKeys" => l.max_keys = tam()?,
            "keyPool" => l.key_pool = tam()?,
            "maxUnique" => l.max_unique = tam()?,
            "maxXmlRefs" => l.max_xml_refs = valor,
            "maxPayload" => l.max_payload = valor,
            "maxTimeMs" => self.tiempo_ms = Some(valor),
            _ => {
                return Err(format!(
                    "límite desconocido {nombre:?} (son: {})",
                    NOMBRES_DE_LIMITES.join(", ")
                ))
            }
        }
        Ok(())
    }
}

/// Un archivo emitido: nombre plano (`[a-z0-9_]+` con `.rs`, `.c` o `.h`) y
/// contenido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Archivo {
    pub nombre: String,
    pub contenido: String,
}
