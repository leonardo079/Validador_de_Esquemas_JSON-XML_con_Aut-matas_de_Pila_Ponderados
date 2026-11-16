//! Criterio 4 de F4.1a: leyes del semianillo tropical (ℕ ∪ {∞}, mín, +, ∞,
//! 0) por proptest, también cerca de la saturación, y la tabla de costo.

use automaton::weight::{cost, Weight};
use ir::RuleKind;
use proptest::prelude::*;

/// Pesos: pequeños, cerca del tope finito y ∞.
fn peso() -> impl Strategy<Value = Weight> {
    prop_oneof![
        (0u64..1000).prop_map(Weight::finite),
        (0u64..1000).prop_map(|d| Weight::finite(Weight::MAX_FINITE - d)),
        Just(Weight::INFINITY),
        Just(Weight::ZERO),
    ]
}

proptest! {
    #[test]
    fn suma_y_producto_asociativos(a in peso(), b in peso(), c in peso()) {
        prop_assert_eq!(a.plus(b).plus(c), a.plus(b.plus(c)));
        prop_assert_eq!(a.times(b).times(c), a.times(b.times(c)));
    }

    #[test]
    fn suma_y_producto_conmutativos(a in peso(), b in peso()) {
        prop_assert_eq!(a.plus(b), b.plus(a));
        prop_assert_eq!(a.times(b), b.times(a));
    }

    #[test]
    fn producto_distribuye_sobre_la_suma(a in peso(), b in peso(), c in peso()) {
        prop_assert_eq!(a.times(b.plus(c)), a.times(b).plus(a.times(c)));
        prop_assert_eq!(b.plus(c).times(a), b.times(a).plus(c.times(a)));
    }

    #[test]
    fn neutros_y_absorbente(a in peso()) {
        // ∞ es el cero (neutro de ⊕, absorbente de ⊗); 0 es el uno.
        prop_assert_eq!(a.plus(Weight::INFINITY), a);
        prop_assert_eq!(a.times(Weight::INFINITY), Weight::INFINITY);
        prop_assert_eq!(a.times(Weight::ZERO), a);
        // ⊕ idempotente (mín).
        prop_assert_eq!(a.plus(a), a);
    }

    /// Lejos de la saturación, ⊗ es la suma exacta de ℕ.
    #[test]
    fn producto_es_la_suma_de_naturales(x in 0u64..1 << 40, y in 0u64..1 << 40) {
        prop_assert_eq!(Weight::finite(x).times(Weight::finite(y)).get(), Some(x + y));
        prop_assert_eq!(Weight::finite(x).plus(Weight::finite(y)).get(), Some(x.min(y)));
    }
}

/// Los finitos saturan sin llegar a ∞: muchas violaciones no son un
/// malformado.
#[test]
fn saturacion_finita() {
    let max = Weight::finite(Weight::MAX_FINITE);
    assert_eq!(max.times(Weight::ONE), max);
    assert!(!max.times(max).is_infinite());
    assert_eq!(Weight::finite(u64::MAX), max);
    assert_eq!(Weight::INFINITY.get(), None);
}

/// El costo está definido para las 31 reglas (el `match` es exhaustivo) y
/// ordena como dice `docs/fase4/automata.md` §5: `type` y `false` primero,
/// los registros después, y los combinadores al final.
#[test]
fn costo_por_regla() {
    for k in RuleKind::ALL {
        assert!(cost(k) >= 1, "{k:?}");
    }
    assert!(cost(RuleKind::Type) < cost(RuleKind::Required));
    assert!(cost(RuleKind::Required) < cost(RuleKind::DependentRequired));
    assert!(cost(RuleKind::Type) < cost(RuleKind::Enum));
    assert!(cost(RuleKind::Enum) < cost(RuleKind::Pattern));
    assert!(cost(RuleKind::Pattern) < cost(RuleKind::AnyOf));
    assert_eq!(cost(RuleKind::Reject), cost(RuleKind::Type));
}
