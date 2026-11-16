//! Pesos de M (D4, F4-U3): el semianillo tropical y el costo estático.
//!
//! Dos pesos distintos sobre las mismas transiciones:
//!
//! - w ∈ ℕ ∪ {∞} ([`Weight`], de `runtime-rs`): cuántas comprobaciones
//!   fallan (1 por `Constraint` violada por instancia; ∞ si el payload es
//!   malformado o pasa un límite). El peso de un recorrido es el producto
//!   ⊗ (la suma) de los de sus transiciones; ⊕ (el mínimo) solo aparece
//!   entre alternativas (F4.5).
//! - c: `RuleKind` → ℕ ([`cost`]): cuánto cuesta evaluar una comprobación.
//!   Ordena las de cada transición, baratas primero, para que el fallo
//!   temprano se detenga en la de menor costo.
//!
//! Teoría y tabla: `docs/fase4/automata.md` §5.

use ir::RuleKind;

pub use runtime_rs::Weight;

/// Costo estático de comprobar una regla (F4-B6): las que deciden con el
/// evento actual (tipo, `false`) son las más baratas; las de registros
/// (máscaras, contadores) les siguen; las que comparan valores, los
/// autómatas y las verificaciones diferidas van al final. Los empates se
/// resuelven por el orden de las reglas en el esquema.
pub const fn cost(kind: RuleKind) -> u32 {
    match kind {
        RuleKind::Type | RuleKind::Reject | RuleKind::Lexical => 1,
        RuleKind::Ref
        | RuleKind::Items
        | RuleKind::Properties
        | RuleKind::Attributes
        | RuleKind::Content
        | RuleKind::SimpleContent => 1,
        RuleKind::Required
        | RuleKind::MinProperties
        | RuleKind::MaxProperties
        | RuleKind::MinItems
        | RuleKind::MaxItems
        | RuleKind::MinLength
        | RuleKind::MaxLength => 2,
        RuleKind::DependentRequired => 3,
        RuleKind::Minimum
        | RuleKind::Maximum
        | RuleKind::ExclusiveMinimum
        | RuleKind::ExclusiveMaximum => 4,
        RuleKind::Const => 5,
        RuleKind::Enum => 6,
        RuleKind::MultipleOf => 8,
        RuleKind::Pattern => 10,
        RuleKind::UniqueItems => 12,
        RuleKind::AllOf
        | RuleKind::AnyOf
        | RuleKind::OneOf
        | RuleKind::Not
        | RuleKind::DependentSchemas => 16,
    }
}
