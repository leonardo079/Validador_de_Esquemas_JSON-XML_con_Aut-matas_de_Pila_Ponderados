//! Verificación del determinismo de M (F4.3, F4-B16;
//! `docs/fase4/automata.md` §17). [`check`] corre al final de cada
//! [`crate::build`] en O(|δ| + |tablas|) y comprueba sobre las tablas ya
//! construidas lo que la definición de M (§1) exige y el núcleo supone:
//!
//! 1. δ es una **función total**: `|δ| = |Q|·|Σ|`, cada entrada es una
//!    transición que existe y cada índice de una transición (destino,
//!    retorno, tipo de marco, comprobaciones, operaciones de contador, regla,
//!    lexema, intervalo, máscara) cae dentro de su tabla.
//! 2. Las clases de carácter son una **partición** de [0, 0x10FFFF] sin los
//!    sustitutos: 128 entradas ASCII y rangos ordenados, disjuntos, por
//!    encima de 0x7F, sin tocar D800-DFFF; lo que no cubren es la clase 0.
//! 3. **Pila visible**: solo las clases de llamada apilan (`{`, `[` en JSON;
//!    `@` y `>` de una etiqueta de apertura en XML), solo las de retorno
//!    desapilan (`}`, `]`; `/>` y `>` de una etiqueta de cierre), y solo las
//!    de apertura y cierre de un escalar lo abren y lo cierran.
//! 4. **Sin ε**: no hay transiciones vacías porque la representación no las
//!    tiene (cada entrada de δ consume un evento); no hay nada que comprobar.
//! 5. **Un destino y una actualización por par**: δ es una tabla, así que
//!    cada (q, a) tiene una sola transición; el Glushkov y el producto ya lo
//!    comprueban al construir (`Internal`, D57).
//!
//! Un fallo es un error del constructor (`BuildError::Internal`), nunca del
//! esquema del usuario.

use std::fmt;

use runtime_rs::tables::{class, xclass, Op, Test};

use crate::model::{Automaton, PayloadFormat};

/// Una incoherencia de las tablas de M.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetError {
    /// δ no tiene |Q|·|Σ| entradas, o Σ no tiene las clases fijas.
    Shape(String),
    /// Un índice fuera de su tabla (en `δ(q, a)` o en la transición `t`).
    OutOfRange { what: &'static str, at: String },
    /// Las clases de carácter no son una partición.
    Classes(String),
    /// Una operación de pila con una clase que no la admite.
    NotVisible {
        state: u32,
        class: u32,
        op: &'static str,
    },
}

impl fmt::Display for DetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DetError::Shape(m) => write!(f, "forma de δ: {m}"),
            DetError::OutOfRange { what, at } => write!(f, "{what} fuera de rango en {at}"),
            DetError::Classes(m) => write!(f, "clases de carácter: {m}"),
            DetError::NotVisible { state, class, op } => {
                write!(f, "pila no visible: {op} en δ({state}, {class})")
            }
        }
    }
}

/// Como mucho tantos errores se informan.
const MAX_ERRORS: usize = 16;

/// Comprueba 1-3 sobre `a` (4 y 5 valen por construcción; ver el módulo).
pub fn check(a: &Automaton) -> Result<(), Vec<DetError>> {
    let mut e = Vec::new();
    shape(a, &mut e);
    if e.is_empty() {
        transitions(a, &mut e);
        classes(a, &mut e);
        visible(a, &mut e);
    }
    e.truncate(MAX_ERRORS);
    if e.is_empty() {
        Ok(())
    } else {
        Err(e)
    }
}

fn shape(a: &Automaton, e: &mut Vec<DetError>) {
    let n = a.states.len();
    let k = a.n_classes as usize;
    if k <= class::CHAR as usize {
        e.push(DetError::Shape(format!("|Σ| = {k} sin clases de carácter")));
    }
    if Some(a.delta.len()) != n.checked_mul(k) {
        e.push(DetError::Shape(format!(
            "{} entradas para {n} estados y {k} clases",
            a.delta.len()
        )));
    }
    for (what, q) in [("q₀", a.start), ("F", a.accept)] {
        if q as usize >= n {
            e.push(DetError::OutOfRange {
                what,
                at: format!("estado {q}"),
            });
        }
    }
}

fn transitions(a: &Automaton, e: &mut Vec<DetError>) {
    let n = a.states.len() as u64;
    let k = a.n_classes as usize;
    for (i, &t) in a.delta.iter().enumerate() {
        if t as usize >= a.trans.len() {
            e.push(DetError::OutOfRange {
                what: "transición",
                at: format!("δ({}, {})", i / k, i % k),
            });
            if e.len() >= MAX_ERRORS {
                return;
            }
        }
    }
    let rango = |ini: u32, len: u32, total: usize| u64::from(ini) + u64::from(len) <= total as u64;
    for (i, t) in a.trans.iter().enumerate() {
        let at = || format!("la transición {i}");
        let mut mal = |what: &'static str| {
            e.push(DetError::OutOfRange { what, at: at() });
        };
        match t.op {
            Op::Move => {
                if u64::from(t.next) >= n {
                    mal("destino");
                }
            }
            Op::Push | Op::Enter => {
                if u64::from(t.next) >= n {
                    mal("destino");
                }
                if u64::from(t.ret) >= n {
                    mal("retorno");
                }
                if t.op == Op::Push && t.frame as usize >= a.frames.len() {
                    mal("tipo de marco");
                }
            }
            Op::Pop | Op::Leave | Op::Fail(_) => {}
        }
        if !rango(t.checks, t.n_checks, a.checks.len()) {
            mal("comprobaciones");
        }
        if !rango(t.counts, t.n_counts, a.counter_ops.len()) {
            mal("operaciones de contador");
        }
        if e.len() >= MAX_ERRORS {
            return;
        }
    }
    for (i, c) in a.checks.iter().enumerate() {
        let mal = |what| DetError::OutOfRange {
            what,
            at: format!("la comprobación {i}"),
        };
        if c.rule as usize >= a.rules.len() {
            e.push(mal("regla"));
        }
        let ok = match c.test {
            Test::NumMin | Test::NumExMin | Test::NumMax | Test::NumExMax => {
                (c.arg as usize) < a.nums.len()
            }
            Test::NumEnum => rango(c.arg, c.len, a.nums.len()),
            Test::MultipleOf => (c.arg as usize) < a.divisors.len(),
            Test::NumIn => rango(c.arg, c.len, a.intervals.len()),
            Test::Dependent => rango(c.arg, c.len, a.deps.len()),
            Test::Mask => (c.arg as usize) <= a.masks.len(),
            _ => true,
        };
        if !ok {
            e.push(mal("argumento"));
        }
        if e.len() >= MAX_ERRORS {
            return;
        }
    }
    for (i, f) in a.frames.iter().enumerate() {
        if f.mask_words > f.words {
            e.push(DetError::OutOfRange {
                what: "máscara",
                at: format!("el marco {i}"),
            });
        }
    }
    for (i, l) in a.nums.iter().enumerate() {
        if !rango(l.off, l.len, a.lexemes.len()) {
            e.push(DetError::OutOfRange {
                what: "lexema",
                at: format!("el número {i}"),
            });
        }
    }
}

fn classes(a: &Automaton, e: &mut Vec<DetError>) {
    let chars = a.n_classes - class::CHAR;
    if a.ascii.len() != 128 {
        e.push(DetError::Classes(format!(
            "{} entradas ASCII",
            a.ascii.len()
        )));
    }
    if let Some(c) = a.ascii.iter().find(|&&c| c >= chars) {
        e.push(DetError::Classes(format!("clase ASCII {c} ≥ {chars}")));
    }
    let mut prev: Option<u32> = None;
    for r in &a.ranges {
        let mal = if r.lo > r.hi {
            Some("vacío")
        } else if r.lo < 0x80 {
            Some("solapa ASCII")
        } else if r.hi > 0x10FFFF {
            Some("pasa de U+10FFFF")
        } else if r.lo <= 0xDFFF && r.hi >= 0xD800 {
            Some("toca los sustitutos")
        } else if prev.is_some_and(|p| r.lo <= p) {
            Some("desordenado o solapado")
        } else if r.class >= chars {
            Some("clase fuera de rango")
        } else {
            None
        };
        if let Some(m) = mal {
            e.push(DetError::Classes(format!(
                "rango {:X}-{:X}: {m}",
                r.lo, r.hi
            )));
        }
        prev = Some(r.hi);
    }
}

/// Las clases de llamada, retorno, apertura y cierre de un escalar.
fn alphabet(f: PayloadFormat) -> [&'static [u32]; 4] {
    match f {
        PayloadFormat::Json => [
            &[class::BEGIN_OBJECT, class::BEGIN_ARRAY],
            &[class::END_OBJECT, class::END_ARRAY],
            &[class::STRING_BEGIN, class::NUMBER_BEGIN],
            &[class::STRING_END, class::NUMBER_END],
        ],
        PayloadFormat::Xml => [
            &[xclass::ATTR, xclass::TAG_CLOSE],
            &[xclass::EMPTY_CLOSE, xclass::END_CLOSE],
            &[xclass::VALUE_BEGIN, xclass::PI_OPEN],
            &[xclass::VALUE_END, xclass::PI_CLOSE],
        ],
    }
}

fn visible(a: &Automaton, e: &mut Vec<DetError>) {
    let [call, ret, open, close] = alphabet(a.format);
    let k = a.n_classes as usize;
    for (i, &t) in a.delta.iter().enumerate() {
        let c = (i % k) as u32;
        // Una entrada fuera de rango ya la informó `transitions`.
        let Some(tr) = a.trans.get(t as usize) else {
            continue;
        };
        let (op, permitidas): (&'static str, &[u32]) = match tr.op {
            Op::Push => ("apila", call),
            Op::Pop => ("desapila", ret),
            Op::Enter => ("abre un escalar", open),
            Op::Leave => ("cierra un escalar", close),
            Op::Move | Op::Fail(_) => continue,
        };
        if !permitidas.contains(&c) {
            e.push(DetError::NotVisible {
                state: (i / k) as u32,
                class: c,
                op,
            });
            if e.len() >= MAX_ERRORS {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{check, DetError};
    use crate::model::Automaton;
    use runtime_rs::tables::{class, CharRange, Op};

    fn m() -> Automaton {
        let src = r#"{"properties": {"ñ": {"type": "string", "enum": ["é", "ü"]}}}"#;
        let s = schema_parser::parse(src.as_bytes()).unwrap();
        crate::build(&semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn acepta_el_m_construido() {
        assert_eq!(check(&m()), Ok(()));
    }

    #[test]
    fn transicion_fuera_de_rango() {
        let mut a = m();
        a.delta[3] = a.trans.len() as u32;
        let e = check(&a).unwrap_err();
        assert!(
            matches!(
                e[0],
                DetError::OutOfRange {
                    what: "transición",
                    ..
                }
            ),
            "{e:?}"
        );
        let mut a = m();
        a.trans[1].next = a.states.len() as u32;
        a.trans[1].op = Op::Move;
        let e = check(&a).unwrap_err();
        assert!(
            matches!(
                e[0],
                DetError::OutOfRange {
                    what: "destino",
                    ..
                }
            ),
            "{e:?}"
        );
    }

    #[test]
    fn rangos_que_se_solapan_y_hueco_en_la_particion() {
        let mut a = m();
        assert!(a.ranges.len() >= 2, "{:?}", a.ranges);
        a.ranges[1].lo = a.ranges[0].hi;
        let e = check(&a).unwrap_err();
        assert!(
            matches!(&e[0], DetError::Classes(m) if m.contains("solapado")),
            "{e:?}"
        );
        // Un hueco: el ASCII sin su última entrada (U+007F sin clase).
        let mut a = m();
        a.ascii.pop();
        assert!(matches!(&check(&a).unwrap_err()[0], DetError::Classes(_)));
        // Un rango sobre los sustitutos.
        let mut a = m();
        a.ranges.push(CharRange {
            lo: 0xD800,
            hi: 0xD801,
            class: 0,
        });
        assert!(check(&a).is_err());
    }

    #[test]
    fn apilar_con_una_clase_de_retorno() {
        let mut a = m();
        let k = a.n_classes as usize;
        let push = a.delta[a.start as usize * k + class::BEGIN_OBJECT as usize];
        assert_eq!(a.trans[push as usize].op, Op::Push);
        a.delta[a.start as usize * k + class::END_ARRAY as usize] = push;
        let e = check(&a).unwrap_err();
        assert_eq!(
            e[0],
            DetError::NotVisible {
                state: a.start,
                class: class::END_ARRAY,
                op: "apila"
            }
        );
    }

    #[test]
    fn forma_de_delta() {
        let mut a = m();
        a.delta.pop();
        assert!(matches!(check(&a).unwrap_err()[0], DetError::Shape(_)));
    }
}
