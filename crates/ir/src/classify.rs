//! Clasificación (a)/(b)/(c) y estrategia de implementación de cada regla y
//! de cada nodo de una IR verificada (tarea 4.3, `docs/fase3/clasificacion.md`).
//!
//! La clase y la estrategia **no se guardan en el nodo** (D13): la de una
//! regla sale de una sola tabla indexada por [`RuleKind`]
//! ([`RuleKind::classification`]), tomada de las columnas «Cat.» de
//! `docs/00-alcance.md` §2 y §3; la de un nodo se deriva con [`classify`]:
//! la máxima de sus reglas y de los nodos que aplica a la **misma
//! instancia** (aristas de E₌), y la unión de sus estrategias.
//!
//! [`classify`] es iterativo (D7) y O(n + m) en nodos y aristas de E₌:
//! recorre E₌ en postorden con una pila explícita y comparte el resultado
//! de cada nodo por `NodeId`, sin expandir caminos (D16). E₌ es acíclico en
//! una [`VerifiedIr`] (I14), así que el postorden existe.

use core::fmt::{self, Write};

use crate::{ContentModel, NodeId, Rule, RuleKind, Term, VerifiedIr};

/// Clase de reconocibilidad (`docs/00-alcance.md`, columna «Cat.»). El orden
/// es el de la derivación: (a) < (b) < (c), y la clase de un nodo es la
/// máxima.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// (a) Directa en el DPDA: transiciones o guardas sobre un token.
    A,
    /// (b) Con registros o contadores acotados en el marco de pila.
    B,
    /// (c) No determinista o no libre de contexto: producto acotado,
    /// determinización acotada, AFD o verificación diferida.
    C,
}

impl Class {
    /// Clave estable en la exportación: `a`, `b`, `c`.
    pub const fn key(self) -> &'static str {
        match self {
            Class::A => "a",
            Class::B => "b",
            Class::C => "c",
        }
    }

    /// Como en la documentación: `(a)`, `(b)`, `(c)`.
    pub const fn label(self) -> &'static str {
        match self {
            Class::A => "(a)",
            Class::B => "(b)",
            Class::C => "(c)",
        }
    }
}

/// Estrategia de implementación de una regla en la Fase 4
/// (`docs/fase3/ir.md` §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Strategy {
    /// Transiciones del autómata (trie, comparador, autómata de Glushkov…).
    Transition,
    /// Registro o contador de tamaño fijo en el marco de pila.
    Register,
    /// Producto acotado de los subautómatas sobre la misma instancia.
    BoundedProduct,
    /// Determinización acotada (4 096 estados, `00-alcance.md` §4); al
    /// pasar del tope, verificación diferida.
    BoundedDeterminization,
    /// AFD sin *backtracking* (`pattern`).
    Dfa,
    /// Verificación diferida al cerrar la instancia (hash acotado,
    /// `dependentSchemas`).
    Deferred,
}

impl Strategy {
    /// Todas, en el orden de la declaración (el de [`StrategySet::iter`]).
    pub const ALL: [Strategy; 6] = [
        Strategy::Transition,
        Strategy::Register,
        Strategy::BoundedProduct,
        Strategy::BoundedDeterminization,
        Strategy::Dfa,
        Strategy::Deferred,
    ];

    /// Clave estable en la exportación. `product`,
    /// `bounded-determinization` y `deferred` son los valores del campo
    /// `strategy` de los casos `non-dpda` (`tests/README.md`).
    pub const fn key(self) -> &'static str {
        match self {
            Strategy::Transition => "transition",
            Strategy::Register => "register",
            Strategy::BoundedProduct => "product",
            Strategy::BoundedDeterminization => "bounded-determinization",
            Strategy::Dfa => "dfa",
            Strategy::Deferred => "deferred",
        }
    }

    /// Nombre en la documentación y en el DOT.
    pub const fn name(self) -> &'static str {
        match self {
            Strategy::Transition => "transición",
            Strategy::Register => "registro o contador",
            Strategy::BoundedProduct => "producto acotado",
            Strategy::BoundedDeterminization => "determinización acotada",
            Strategy::Dfa => "AFD",
            Strategy::Deferred => "verificación diferida",
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Conjunto de estrategias, como máscara de 6 bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StrategySet(u8);

impl StrategySet {
    pub const EMPTY: StrategySet = StrategySet(0);

    /// El conjunto con `s` añadida.
    #[must_use]
    pub const fn with(self, s: Strategy) -> StrategySet {
        StrategySet(self.0 | s.bit())
    }

    #[must_use]
    pub const fn union(self, o: StrategySet) -> StrategySet {
        StrategySet(self.0 | o.0)
    }

    pub const fn contains(self, s: Strategy) -> bool {
        self.0 & s.bit() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Las estrategias del conjunto, en el orden de [`Strategy::ALL`].
    pub fn iter(self) -> impl Iterator<Item = Strategy> {
        Strategy::ALL.into_iter().filter(move |s| self.contains(*s))
    }
}

/// Clase y estrategia de una regla.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Assignment {
    pub class: Class,
    pub strategy: Strategy,
}

const fn asg(class: Class, strategy: Strategy) -> Assignment {
    Assignment { class, strategy }
}

impl RuleKind {
    /// **La tabla** de clasificación (`docs/fase3/clasificacion.md` §3):
    /// la clase y la estrategia de cada clase de regla y, si la regla puede
    /// subir según sus datos, aquella a la que sube. Solo `Content` sube: a
    /// (b) con `xs:all` o una partícula con contador ([`Rule::assignment`]).
    /// Las clases son las de las columnas «Cat.» de `docs/00-alcance.md` §2
    /// y §3; la prueba `tabla_coincide_con_el_alcance` lo comprueba fila a
    /// fila.
    pub const fn classification(self) -> (Assignment, Option<Assignment>) {
        use Class::{A, B, C};
        use RuleKind as K;
        use Strategy as S;
        let a = asg(A, S::Transition);
        let b = asg(B, S::Register);
        match self {
            K::Reject
            | K::Ref
            | K::Type
            | K::Enum
            | K::Const
            | K::Minimum
            | K::Maximum
            | K::ExclusiveMinimum
            | K::ExclusiveMaximum
            | K::MultipleOf
            | K::Items
            | K::Properties
            | K::SimpleContent
            | K::Lexical => (a, None),
            K::Content => (a, Some(b)),
            K::MinLength
            | K::MaxLength
            | K::MinItems
            | K::MaxItems
            | K::Required
            | K::MinProperties
            | K::MaxProperties
            | K::Attributes => (b, None),
            K::Pattern => (asg(C, S::Dfa), None),
            K::UniqueItems | K::DependentSchemas => (asg(C, S::Deferred), None),
            // (c) en el alcance; se comprueba con la máscara de bits al
            // cerrar el objeto (D5).
            K::DependentRequired => (asg(C, S::Register), None),
            K::AllOf | K::Not => (asg(C, S::BoundedProduct), None),
            K::AnyOf | K::OneOf => (asg(C, S::BoundedDeterminization), None),
        }
    }

    /// Clase y estrategia de la tabla, sin subir.
    pub const fn assignment(self) -> Assignment {
        self.classification().0
    }
}

impl Rule {
    /// Clase y estrategia de esta regla: la de su [`RuleKind`], o aquella a
    /// la que sube si sus datos lo piden (un modelo de contenido con
    /// `xs:all` o con una partícula que necesita contador).
    pub fn assignment(&self) -> Assignment {
        let (base, raised) = self.kind().classification();
        match (self, raised) {
            (Rule::Content(m), Some(r)) if needs_registers(m) => r,
            _ => base,
        }
    }
}

/// Un modelo de contenido necesita registros si tiene `xs:all` (máscara de
/// bits) o una partícula con `minOccurs` ≥ 2 o `maxOccurs` acotado > 1
/// (contador). `?`, `*`, `+`, una vez y `maxOccurs="0"` los expresa el
/// autómata de Glushkov con estados (D16, `docs/fase3/ir.md` §7).
fn needs_registers(m: &ContentModel) -> bool {
    m.particles.iter().any(|p| {
        matches!(p.term, Term::All(_))
            || p.occurs.min >= 2
            || matches!(p.occurs.max, Some(max) if max > 1)
    })
}

/// Clase y estrategias de un nodo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeClass {
    /// La máxima de sus reglas y de los nodos sobre la misma instancia; (a)
    /// si no hay ninguna regla.
    pub class: Class,
    /// Las de sus reglas y las de los nodos sobre la misma instancia; vacío
    /// si no hay reglas (el esquema `true`: el DPDA solo recorre la
    /// instancia).
    pub strategies: StrategySet,
}

impl NodeClass {
    const NONE: NodeClass = NodeClass {
        class: Class::A,
        strategies: StrategySet::EMPTY,
    };

    fn join(self, o: NodeClass) -> NodeClass {
        NodeClass {
            class: if o.class > self.class {
                o.class
            } else {
                self.class
            },
            strategies: self.strategies.union(o.strategies),
        }
    }
}

/// La clasificación de todos los nodos de una IR, indexada por `NodeId`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classification(Vec<NodeClass>);

impl Classification {
    /// La del nodo `id`.
    ///
    /// # Panics
    ///
    /// Si `id` no es de la IR clasificada.
    pub fn node(&self, id: NodeId) -> NodeClass {
        self.0[id.index()]
    }

    /// Todas, en orden de `NodeId`.
    pub fn nodes(&self) -> &[NodeClass] {
        &self.0
    }

    /// Número de nodos de la clase `c`.
    pub fn count(&self, c: Class) -> usize {
        self.0.iter().filter(|n| n.class == c).count()
    }
}

/// Clasifica cada nodo de la IR: la clase máxima y la unión de las
/// estrategias de sus reglas y de los nodos alcanzables por aristas de E₌
/// (`$ref`, `allOf`, `anyOf`, `oneOf`, `not`, `dependentSchemas`), que se
/// aplican a la misma instancia. Las aristas que consumen entrada
/// (`properties`, `items`, modelo de contenido, atributos, contenido simple)
/// no suben la clase: su destino valida otra instancia, en otro marco.
///
/// Postorden iterativo sobre E₌ (D7), O(n + m): cada nodo se cierra una vez
/// y su resultado se reutiliza en todos sus predecesores (D16). E₌ es
/// acíclico en una [`VerifiedIr`] (I14); si no lo fuera, una arista a un
/// nodo aún abierto se ignora, así que el recorrido termina igualmente.
pub fn classify(ir: &VerifiedIr) -> Classification {
    let nodes = ir.nodes();
    let n = nodes.len();
    // Clase propia de cada nodo y aristas de E₌ en formato CSR.
    let mut own = Vec::with_capacity(n);
    let mut start = Vec::with_capacity(n + 1);
    let mut succ: Vec<usize> = Vec::new();
    let mut buf = Vec::new();
    for node in nodes {
        start.push(succ.len());
        let mut c = NodeClass::NONE;
        for r in &node.rules {
            let a = r.rule.assignment();
            c = c.join(NodeClass {
                class: a.class,
                strategies: StrategySet::EMPTY.with(a.strategy),
            });
            if r.rule.kind().same_instance() {
                buf.clear();
                r.rule.targets(&mut buf);
                succ.extend(buf.iter().map(|t| t.index()));
            }
        }
        own.push(c);
    }
    start.push(succ.len());

    // 0: sin visitar; 1: en la pila; 2: cerrado (`own` ya es el resultado).
    let mut state = vec![0u8; n];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for s in 0..n {
        if state[s] != 0 {
            continue;
        }
        state[s] = 1;
        stack.push((s, start[s]));
        while let Some(&mut (v, ref mut k)) = stack.last_mut() {
            if *k < start[v + 1] {
                let w = succ[*k];
                *k += 1;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, start[w]));
                    }
                    2 => own[v] = own[v].join(own[w]),
                    // Ciclo de E₌: imposible en una IR verificada (I14).
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
                if let Some(&(p, _)) = stack.last() {
                    own[p] = own[p].join(own[v]);
                }
            }
        }
    }
    Classification(own)
}

/// Escribe la tabla de [`RuleKind::classification`] en Markdown, una fila
/// por [`RuleKind`] en el orden de [`RuleKind::ALL`]. Es el bloque generado
/// de `docs/fase3/clasificacion.md` §3 (la prueba
/// `tabla_escrita_es_la_del_codigo` los compara).
pub fn write_classification_table(w: &mut impl Write) -> fmt::Result {
    writeln!(w, "| `RuleKind` | Regla | Clase | Estrategia |")?;
    writeln!(w, "|---|---|---|---|")?;
    for kind in RuleKind::ALL {
        let (a, raised) = kind.classification();
        write!(w, "| `{kind:?}` | `{}` | {}", kind.key(), a.class.label())?;
        if let Some(r) = raised {
            write!(
                w,
                "; {} con `xs:all` o una partícula con contador",
                r.class.label()
            )?;
        }
        write!(w, " | {} (`{}`)", a.strategy.name(), a.strategy.key())?;
        if let Some(r) = raised {
            write!(w, "; {} (`{}`)", r.strategy.name(), r.strategy.key())?;
        }
        writeln!(w, " |")?;
    }
    Ok(())
}
