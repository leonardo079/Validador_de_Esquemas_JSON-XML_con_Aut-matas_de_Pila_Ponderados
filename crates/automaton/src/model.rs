//! M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) ya como tablas densas (F4-U4): lo que
//! construye [`crate::build`] y presta al núcleo de `runtime-rs`
//! ([`Automaton::tables`]). Definición formal: `docs/fase4/automata.md` §1.
//!
//! - Q: `0 .. n_states` ([`StateId`]); cada estado pertenece a un grupo (el
//!   subautómata de un nodo o de «cualquiera»), que es su clúster en el DOT.
//! - Σ: `0 .. n_classes` ([`ClassId`]): las clases fijas de eventos
//!   (`runtime_rs::tables::class`) y las de carácter del esquema.
//! - Γ: los tipos de marco ([`FrameId`]) con su retorno y sus registros; Z₀
//!   es la pila vacía.
//! - δ: `delta[q · |Σ| + a]` → [`TransId`], total (el error por defecto).
//! - q₀ = `start`, F = {`accept`} con la pila vacía; w en las
//!   comprobaciones de cada transición.

use ir::{NodeId, RuleKind};
use runtime_rs::tables::{
    CharRange, Check, CounterOp, Dep, DivisorRef, Frame, Interval, Lexeme, Rule, Trans,
};
use runtime_rs::Tables;

/// Estado de control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateId(pub u32);

/// Tipo de marco (símbolo de Γ sin su retorno ni sus registros).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameId(pub u32);

/// Índice de una transición en `trans`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransId(pub u32);

/// Clase de Σ.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(pub u16);

/// La transición de error de sintaxis, a la que apunta toda entrada de δ
/// que la gramática no admite.
pub const ERROR: TransId = TransId(0);

/// Metadatos de un estado para el DOT y las pruebas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateInfo {
    /// Grupo (clúster): índice en [`Automaton::groups`].
    pub group: u32,
    pub name: String,
}

/// Formato del payload que lee M: decide el lexer que va delante (Σ son
/// sus eventos, F4-U1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadFormat {
    Json,
    Xml,
}

impl PayloadFormat {
    /// Nombre en el resumen de `schemac automaton` (`format`).
    pub const fn name(self) -> &'static str {
        match self {
            PayloadFormat::Json => "json",
            PayloadFormat::Xml => "xml",
        }
    }
}

/// El autómata construido: tablas propias más metadatos.
#[derive(Clone, Debug)]
pub struct Automaton {
    pub(crate) format: PayloadFormat,
    pub(crate) n_classes: u32,
    pub(crate) start: u32,
    pub(crate) accept: u32,
    pub(crate) delta: Vec<u32>,
    pub(crate) trans: Vec<Trans>,
    pub(crate) checks: Vec<Check>,
    pub(crate) frames: Vec<Frame>,
    pub(crate) masks: Vec<u64>,
    pub(crate) deps: Vec<Dep>,
    pub(crate) nums: Vec<Lexeme>,
    pub(crate) lexemes: Vec<u8>,
    pub(crate) divisors: Vec<DivisorRef>,
    pub(crate) counter_ops: Vec<CounterOp>,
    pub(crate) intervals: Vec<Interval>,
    pub(crate) ascii: Vec<u32>,
    pub(crate) ranges: Vec<CharRange>,
    pub(crate) rules: Vec<Rule>,
    pub(crate) text: Vec<u8>,
    pub(crate) states: Vec<StateInfo>,
    pub(crate) groups: Vec<String>,
    pub(crate) class_names: Vec<String>,
    pub(crate) frame_names: Vec<String>,
    /// Subautómatas producto (conjuntos de ≥ 2 componentes).
    pub(crate) products: usize,
}

/// Tamaños de M (F4-U7: el resumen de `schemac automaton`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    /// |Q|.
    pub states: usize,
    /// Entradas de δ que no son el error: las transiciones de M.
    pub transitions: usize,
    /// Tipos de marco.
    pub frames: usize,
    /// |Σ|.
    pub classes: usize,
    /// Suma de los registros de todos los tipos de marco, en bytes.
    pub register_bytes: usize,
    /// El marco más grande en la pila (cabecera y registros), en bytes:
    /// la pila de profundidad d ocupa como mucho d veces esto.
    pub max_frame_bytes: usize,
    /// Subautómatas producto: los de un conjunto canónico de ≥ 2 componentes
    /// (`$ref` con hermanas, `allOf`), uno por subautómata de objeto,
    /// arreglo, número o cadena.
    pub products: usize,
    /// Reglas con estrategia de respaldo (verificación diferida, F4.5); 0
    /// hasta entonces. Se llamaba `pending` (F4.1a-CR-S5).
    pub deferred: usize,
}

impl Automaton {
    /// Las tablas prestadas, en el formato que lee el núcleo.
    pub fn tables(&self) -> Tables<'_> {
        Tables {
            n_classes: self.n_classes,
            start: self.start,
            accept: self.accept,
            delta: &self.delta,
            trans: &self.trans,
            checks: &self.checks,
            frames: &self.frames,
            masks: &self.masks,
            deps: &self.deps,
            nums: &self.nums,
            lexemes: &self.lexemes,
            divisors: &self.divisors,
            counter_ops: &self.counter_ops,
            intervals: &self.intervals,
            scalar_slots: 0,
            ascii: &self.ascii,
            ranges: &self.ranges,
            rules: &self.rules,
            text: &self.text,
        }
    }

    /// Formato del payload (el lexer que va delante).
    pub fn format(&self) -> PayloadFormat {
        self.format
    }

    pub fn n_states(&self) -> usize {
        self.states.len()
    }

    pub fn n_classes(&self) -> usize {
        self.n_classes as usize
    }

    pub fn start(&self) -> StateId {
        StateId(self.start)
    }

    pub fn accept(&self) -> StateId {
        StateId(self.accept)
    }

    /// δ(q, a).
    pub fn delta(&self, q: StateId, a: ClassId) -> TransId {
        TransId(self.delta[q.0 as usize * self.n_classes as usize + usize::from(a.0)])
    }

    pub fn transition(&self, t: TransId) -> &Trans {
        &self.trans[t.0 as usize]
    }

    pub fn checks_of(&self, t: TransId) -> &[Check] {
        let tr = self.transition(t);
        &self.checks[tr.checks as usize..(tr.checks + tr.n_checks) as usize]
    }

    pub fn state(&self, q: StateId) -> &StateInfo {
        &self.states[q.0 as usize]
    }

    pub fn groups(&self) -> &[String] {
        &self.groups
    }

    pub fn class_name(&self, a: ClassId) -> &str {
        &self.class_names[usize::from(a.0)]
    }

    pub fn frame(&self, f: FrameId) -> &Frame {
        &self.frames[f.0 as usize]
    }

    /// Nodo y clase de la regla `rule` de un reporte.
    pub fn rule(&self, rule: u32) -> (NodeId, RuleKind) {
        let r = &self.rules[rule as usize];
        (NodeId::new(r.node as usize), RuleKind::ALL[r.kind as usize])
    }

    /// Ubicación recortada de la regla `rule`.
    pub fn rule_location(&self, rule: u32) -> &str {
        std::str::from_utf8(self.tables().location(rule)).unwrap_or("")
    }

    /// Palabras de pila para `depth` marcos anidados.
    pub fn stack_words(&self, depth: usize) -> usize {
        let max = self
            .frames
            .iter()
            .map(|f| f.words as usize)
            .max()
            .unwrap_or(0);
        depth.saturating_mul(max + 1)
    }

    pub fn summary(&self) -> Summary {
        Summary {
            states: self.states.len(),
            transitions: self.delta.iter().filter(|&&t| t != ERROR.0).count(),
            frames: self.frames.len(),
            classes: self.n_classes as usize,
            register_bytes: self.frames.iter().map(|f| f.words as usize * 8).sum(),
            max_frame_bytes: self
                .frames
                .iter()
                .map(|f| (f.words as usize + 1) * 8)
                .max()
                .unwrap_or(0),
            products: self.products,
            deferred: 0,
        }
    }
}
