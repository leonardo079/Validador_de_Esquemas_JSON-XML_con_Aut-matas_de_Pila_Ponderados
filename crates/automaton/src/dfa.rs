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

/// Construcción a mano de un AFD pequeño (los espacios léxicos de XSD y la
/// declaración `<?xml …?>`, F4.1c): estados con su salida y aristas por
/// clase; lo que falta va al estado muerto, que [`DfaBuilder::finish`]
/// añade al final.
#[derive(Debug)]
pub struct DfaBuilder<O> {
    n_classes: usize,
    trans: Vec<u32>,
    out: Vec<O>,
}

const SIN: u32 = u32::MAX;

impl<O: Copy + Default + PartialEq> DfaBuilder<O> {
    pub fn new(n_classes: usize) -> DfaBuilder<O> {
        DfaBuilder {
            n_classes,
            trans: Vec::new(),
            out: Vec::new(),
        }
    }

    /// Un estado nuevo con salida `o`.
    pub fn state(&mut self, o: O) -> u32 {
        self.out.push(o);
        self.trans.extend(std::iter::repeat_n(SIN, self.n_classes));
        (self.out.len() - 1) as u32
    }

    /// `s` va a `t` con cada clase de `classes`.
    pub fn on(&mut self, s: u32, classes: &[usize], t: u32) {
        for &a in classes {
            self.trans[s as usize * self.n_classes + a] = t;
        }
    }

    /// El AFD completo con inicial `start` y el muerto al final.
    pub fn finish(mut self, start: u32) -> Dfa<O> {
        let dead = self.state(O::default());
        for t in &mut self.trans {
            if *t == SIN {
                *t = dead;
            }
        }
        Dfa {
            n_classes: self.n_classes,
            start,
            trans: self.trans,
            out: self.out,
        }
    }
}

/// Transductor de *collapse* de XSD delante de `d` (F4.1c; F4.4b lo pone
/// también delante del AFD de un `pattern`): el AFD que acepta un texto t si
/// `d` acepta collapse(t), con la misma salida. collapse cambia `\t \n \r`
/// por espacios, junta los espacios seguidos en uno y quita los de los
/// extremos (XSD 1.0 Parte 2 §4.3.6). Estados: el inicial (aún sin
/// contenido: los espacios se descartan) y, por cada estado s de `d`, «s sin
/// espacio pendiente» y «s con un espacio pendiente» (los espacios tras el
/// contenido no se pasan a `d` hasta que llega otro carácter, y si el texto
/// acaba antes se descartan). `ws` son las clases de los cuatro espacios y
/// `space` la del espacio. Solo se construye lo alcanzable, con un único
/// muerto; tamaño ≤ 2·|d| + 1.
pub fn collapse<O: Copy + Default + PartialEq>(d: &Dfa<O>, ws: &[usize], space: usize) -> Dfa<O> {
    use std::collections::HashMap;
    let k = d.n_classes;
    let dead: Vec<bool> = (0..d.n_states() as u32).map(|s| d.is_dead(s)).collect();
    // (estado de d, modo): 0 = inicial, 1 = sin pendiente, 2 = pendiente.
    let mut ids: HashMap<(u32, u8), u32> = HashMap::new();
    let mut keys: Vec<(u32, u8)> = Vec::new();
    let mut trans: Vec<u32> = Vec::new();
    let mut intern = |key: (u32, u8), keys: &mut Vec<(u32, u8)>| -> u32 {
        let key = if dead[key.0 as usize] {
            (key.0, 1)
        } else {
            key
        };
        *ids.entry(key).or_insert_with(|| {
            keys.push(key);
            (keys.len() - 1) as u32
        })
    };
    intern((d.start, 0), &mut keys);
    let mut i = 0;
    while i < keys.len() {
        let (s, mode) = keys[i];
        for a in 0..k {
            let to = if ws.contains(&a) {
                match mode {
                    0 => (s, 0),
                    _ => (s, 2),
                }
            } else {
                match mode {
                    2 => (d.next(d.next(s, space), a), 1),
                    _ => (d.next(s, a), 1),
                }
            };
            let t = intern(to, &mut keys);
            trans.push(t);
        }
        i += 1;
    }
    let out = keys.iter().map(|&(s, _)| d.out[s as usize]).collect();
    Dfa {
        n_classes: k,
        start: 0,
        trans,
        out,
    }
}
