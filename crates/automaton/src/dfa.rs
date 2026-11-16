//! AFD completos con salidas (F4-B14): las partes finitas de M (tries de
//! claves; en F4.1c los espacios léxicos y en F4.4b los AFD de `pattern`).
//! F4.3 los minimiza con Hopcroft sobre esta misma forma.
//!
//! `trans` va por filas: `trans[s * n_classes + a]` es el destino de `s`
//! con la clase `a`. Es **completo** (toda fila tiene `n_classes`
//! entradas) y tiene un **estado muerto explícito**: un estado sin salida
//! (la de [`Default`]) cuyas transiciones vuelven a él mismo
//! ([`Dfa::is_dead`]).

/// AFD completo sobre `n_classes` clases, con una salida por estado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dfa<O> {
    pub n_classes: usize,
    pub start: u32,
    /// Por filas: `n_states × n_classes`.
    pub trans: Vec<u32>,
    /// Salida de cada estado (`O::default()` = no acepta).
    pub out: Vec<O>,
}

impl<O: Copy + Default + PartialEq> Dfa<O> {
    pub fn n_states(&self) -> usize {
        self.out.len()
    }

    /// Destino de `s` con la clase `a`.
    pub fn next(&self, s: u32, a: usize) -> u32 {
        self.trans[s as usize * self.n_classes + a]
    }

    /// Estado sin salida que solo vuelve a sí mismo.
    pub fn is_dead(&self, s: u32) -> bool {
        self.out[s as usize] == O::default() && (0..self.n_classes).all(|a| self.next(s, a) == s)
    }

    /// Salida tras leer `word` (clases) desde el inicial.
    pub fn run(&self, word: &[usize]) -> O {
        let s = word.iter().fold(self.start, |s, &a| self.next(s, a));
        self.out[s as usize]
    }
}

impl Dfa<Option<u32>> {
    /// Trie de `words` (cada palabra, una lista de clases; ninguna se
    /// repite): un estado por prefijo, más el muerto, que es el último.
    /// La salida del estado de la palabra `i` es `Some(i)`. Iterativo,
    /// O(Σ |palabra| · n_classes).
    pub fn trie(n_classes: usize, words: &[Vec<usize>]) -> Dfa<Option<u32>> {
        const SIN: u32 = u32::MAX;
        let mut trans: Vec<u32> = vec![SIN; n_classes];
        let mut out: Vec<Option<u32>> = vec![None];
        for (i, w) in words.iter().enumerate() {
            let mut s = 0usize;
            for &a in w {
                let t = trans[s * n_classes + a];
                s = if t == SIN {
                    let nuevo = out.len();
                    out.push(None);
                    trans.extend(std::iter::repeat_n(SIN, n_classes));
                    trans[s * n_classes + a] = nuevo as u32;
                    nuevo
                } else {
                    t as usize
                };
            }
            out[s] = Some(i as u32);
        }
        let dead = out.len() as u32;
        out.push(None);
        trans.extend(std::iter::repeat_n(dead, n_classes));
        for t in &mut trans {
            if *t == SIN {
                *t = dead;
            }
        }
        Dfa {
            n_classes,
            start: 0,
            trans,
            out,
        }
    }
}
