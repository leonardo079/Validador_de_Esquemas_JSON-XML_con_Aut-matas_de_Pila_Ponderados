//! Semianillo tropical (ℕ ∪ {∞}, mín, +, ∞, 0) de D4.
//!
//! Un [`Weight`] es un `u64`: `u64::MAX` es ∞ y los finitos saturan en
//! [`Weight::MAX_FINITE`]. Saturar es el homomorfismo de truncamiento
//! ℕ ∪ {∞} → [0, M] ∪ {∞}, así que conserva las leyes del semianillo
//! (asociatividad, conmutatividad, distributividad, neutros y ∞ absorbente
//! para ⊗; `docs/fase4/automata.md` §5) y nunca confunde «muchas
//! violaciones» con un payload malformado.

/// Peso en ℕ ∪ {∞}: número de violaciones, o ∞ (malformado o límite).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Weight(u64);

impl Weight {
    /// 0: neutro de ⊗ (el «uno» del semianillo); peso de un recorrido conforme.
    pub const ZERO: Weight = Weight(0);
    /// 1: una violación.
    pub const ONE: Weight = Weight(1);
    /// ∞: neutro de ⊕ (el «cero» del semianillo) y absorbente de ⊗.
    pub const INFINITY: Weight = Weight(u64::MAX);
    /// Mayor peso finito; los finitos saturan aquí.
    pub const MAX_FINITE: u64 = u64::MAX - 1;

    /// Peso finito `n`, saturado en [`Weight::MAX_FINITE`].
    pub const fn finite(n: u64) -> Weight {
        if n > Self::MAX_FINITE {
            Weight(Self::MAX_FINITE)
        } else {
            Weight(n)
        }
    }

    /// El número de violaciones, o `None` si es ∞.
    pub const fn get(self) -> Option<u64> {
        if self.0 == u64::MAX {
            None
        } else {
            Some(self.0)
        }
    }

    pub const fn is_infinite(self) -> bool {
        self.0 == u64::MAX
    }

    /// ⊕ = mín (entre alternativas).
    pub const fn plus(self, other: Weight) -> Weight {
        if self.0 <= other.0 {
            self
        } else {
            other
        }
    }

    /// ⊗ = + (a lo largo de un recorrido); ∞ absorbe y los finitos saturan.
    pub const fn times(self, other: Weight) -> Weight {
        if self.0 == u64::MAX || other.0 == u64::MAX {
            Weight::INFINITY
        } else {
            Weight::finite(self.0.saturating_add(other.0))
        }
    }
}
