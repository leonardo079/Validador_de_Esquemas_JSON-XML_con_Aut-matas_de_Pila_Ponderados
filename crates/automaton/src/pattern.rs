//! `pattern` → AFD minimizado (F4.4b; `docs/fase4/pattern.md` §6, F4-B18).
//!
//! El patrón se vuelve a analizar con `ir::regex` (F4-U15; un fallo aquí es
//! `Internal`: la Fase 3 ya lo aceptó) y su autómata de Glushkov (sin ε) se
//! determiniza por subconjuntos sobre una **partición local** de los puntos
//! de código: los intervalos con la misma pertenencia a los conjuntos de
//! las posiciones forman una clase. El anclaje va por alternativa de primer
//! nivel: sin `^` se reinyecta `first` en cada paso; sin `$`, al alcanzar
//! `last` se pasa a un estado «encontrado» absorbente. Los estados se
//! cuentan **antes** de crearlos: más del tope (4 096, F4-U16) es un error
//! con el número alcanzado. El resultado se minimiza con Hopcroft (F4.3).

use std::collections::{BTreeMap, HashMap};

use ir::regex::{bit, contains, CharSet, Dialect, Regex};

use crate::dfa::Dfa;
use crate::minimize::hopcroft;

/// Una partición de [0, 0x10FFFF]: intervalos ordenados y disjuntos con su
/// clase (1..); lo que no cubren es la clase 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Partition {
    pub(crate) runs: Vec<(u32, u32, u32)>,
    pub(crate) n_classes: usize,
}

impl Partition {
    /// La partición más gruesa en la que cada conjunto de `sets` y cada
    /// punto de código de `singles` es unión de clases (los singletons, cada
    /// uno en su clase). Las clases se numeran por el primer punto de código
    /// que tocan; los intervalos sin nada son la clase 0.
    pub(crate) fn refine(singles: &[u32], sets: &[&CharSet]) -> Partition {
        // Eventos: (punto, +conjunto / −conjunto); los singletons son un
        // conjunto propio cada uno.
        let mut bounds: BTreeMap<u32, Vec<(bool, u64)>> = BTreeMap::new();
        let mut add = |lo: u32, hi: u32, id: u64| {
            bounds.entry(lo).or_default().push((true, id));
            if hi < 0x10FFFF {
                bounds.entry(hi + 1).or_default().push((false, id));
            }
        };
        for (i, s) in sets.iter().enumerate() {
            for &(lo, hi) in s.iter() {
                add(lo, hi, i as u64);
            }
        }
        for &c in singles {
            add(c, c, (1u64 << 40) | u64::from(c));
        }
        let mut active: BTreeMap<u64, u32> = BTreeMap::new();
        let mut ids: HashMap<Vec<u64>, u32> = HashMap::new();
        let mut runs: Vec<(u32, u32, u32)> = Vec::new();
        let puntos: Vec<u32> = bounds.keys().copied().collect();
        for (k, &p) in puntos.iter().enumerate() {
            for &(abre, id) in &bounds[&p] {
                let e = active.entry(id).or_insert(0);
                if abre {
                    *e += 1;
                } else {
                    *e -= 1;
                    if *e == 0 {
                        active.remove(&id);
                    }
                }
            }
            if active.is_empty() {
                continue;
            }
            let hi = puntos.get(k + 1).map_or(0x10FFFF, |&n| n - 1);
            let sig: Vec<u64> = active.keys().copied().collect();
            let next = ids.len() as u32 + 1;
            let c = *ids.entry(sig).or_insert(next);
            match runs.last_mut() {
                Some(r) if r.2 == c && r.1 + 1 == p => r.1 = hi,
                _ => runs.push((p, hi, c)),
            }
        }
        Partition {
            n_classes: ids.len() + 1,
            runs,
        }
    }

    /// La clase de `c`.
    pub(crate) fn class(&self, c: u32) -> u32 {
        let i = self.runs.partition_point(|r| r.1 < c);
        match self.runs.get(i) {
            Some(&(lo, _, k)) if lo <= c => k,
            _ => 0,
        }
    }

    /// Un punto de código de cada clase (la 0, uno fuera de todo intervalo
    /// si lo hay).
    pub(crate) fn reps(&self) -> Vec<u32> {
        let mut r = vec![u32::MAX; self.n_classes];
        for &(lo, _, k) in &self.runs {
            if r[k as usize] == u32::MAX {
                r[k as usize] = lo;
            }
        }
        // Un punto de código de la clase 0: el primero de un hueco que no
        // sea un sustituto (los sustitutos nunca son caracteres). Si no hay
        // ninguno, la clase 0 está vacía y su representante no importa.
        r[0] = 0;
        let mut next = 0u32;
        let ends = self
            .runs
            .iter()
            .map(|&(lo, hi, _)| (lo, hi))
            .chain([(0x11_0000, 0x11_0000)]);
        for (lo, hi) in ends {
            if lo > next {
                let (a, b) = (next, lo - 1);
                let cand = if (0xD800..=0xDFFF).contains(&a) {
                    0xE000
                } else {
                    a
                };
                if cand <= b && cand <= 0x10_FFFF {
                    r[0] = cand;
                    break;
                }
            }
            next = hi.saturating_add(1);
        }
        r
    }
}

/// El AFD de un patrón sobre su partición local.
#[derive(Clone, Debug)]
pub(crate) struct PatternDfa {
    pub(crate) part: Partition,
    pub(crate) dfa: Dfa<bool>,
    /// Los conjuntos de las posiciones (para refinar la partición global).
    pub(crate) sets: Vec<CharSet>,
    /// Trabajo de construirlo, en palabras: las filas de follow (m · w), la
    /// determinización (lo que cuenta `MAX_PATTERN_WORK`) y las claves de los
    /// estados (estados · w). Los constructores lo suman en
    /// `MAX_SCHEMA_PATTERN_WORK` para todo el esquema (E3B-2, E3B-3).
    pub(crate) work: usize,
}

/// Por qué no se construyó el AFD.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PatternError {
    /// El patrón no pasa `ir::regex` (la Fase 3 lo habría rechazado).
    Invalid(String),
    /// Más de `cap` estados (el número alcanzado al cortar).
    Limit(usize),
}

/// Determiniza y minimiza `src` en el dialecto `d` con como mucho `cap`
/// estados (contados antes de crearlos).
pub(crate) fn compile(src: &str, d: Dialect, cap: usize) -> Result<PatternDfa, PatternError> {
    let re = ir::regex::parse(src, d).map_err(|e| PatternError::Invalid(e.to_string()))?;
    build(&re, cap)
}

/// Palabras de trabajo como mucho al determinizar un patrón (filas de follow
/// combinadas y máscaras de clase): con 4 096 posiciones densas y 4 096
/// estados el trabajo sería ≈ 10⁹ palabras, así que se corta antes con
/// `limit` (como `MAX_CLOSURE_WORK` de los cierres).
pub(crate) const MAX_PATTERN_WORK: usize = 1 << 26;

/// Trabajo de todos los `pattern` de un esquema juntos (E3B-2, E3B-3): sin él,
/// cada `pattern` respeta `MAX_PATTERN_WORK` pero un esquema de pocos KB con
/// cientos de patrones distintos tarda minutos. 2²³ palabras son unos
/// 1-2 s en release en el peor caso medido (`^a{0,4000}$`, ≈ 1,3 · 10⁶ cada
/// uno) y decenas de miles de patrones de los habituales.
pub(crate) const MAX_SCHEMA_PATTERN_WORK: usize = 1 << 23;

fn build(re: &Regex, cap: usize) -> Result<PatternDfa, PatternError> {
    let gs = re.glushkov();
    // Posiciones de todas las alternativas en una numeración común.
    let m: usize = gs.iter().map(|g| g.sets.len()).sum();
    let w = m.div_ceil(64).max(1);
    let mut sets: Vec<CharSet> = Vec::with_capacity(m);
    let mut follow = vec![0u64; m * w];
    let mut first_any = vec![0u64; w]; // sin `^`: se reinyecta siempre
    let mut first_start = vec![0u64; w]; // con `^`: solo en el primer paso
    let mut last_free = vec![0u64; w]; // sin `$`: al alcanzarlo, encontrado
    let mut last_end = vec![0u64; w]; // con `$`: acepta al final
    let mut found0 = false;
    let mut empty_ok = false;
    let set = |b: &mut [u64], p: usize| b[p / 64] |= 1 << (p % 64);
    for g in &gs {
        let off = sets.len();
        let gm = g.sets.len();
        sets.extend(g.sets.iter().cloned());
        for p in 0..gm {
            let fp = g.follow_of(p);
            for q in (0..gm).filter(|&q| bit(fp, q)) {
                set(&mut follow[(off + p) * w..(off + p + 1) * w], off + q);
            }
            let (f, l) = (bit(&g.first, p), bit(&g.last, p));
            if f {
                set(
                    if g.start {
                        &mut first_start
                    } else {
                        &mut first_any
                    },
                    off + p,
                );
            }
            if l {
                set(if g.end { &mut last_end } else { &mut last_free }, off + p);
            }
        }
        if g.nullable && (!g.end || !g.start) {
            found0 = true;
        }
        if g.nullable && g.end && g.start {
            empty_ok = true;
        }
    }
    let refs: Vec<&CharSet> = sets.iter().collect();
    let part = Partition::refine(&[], &refs);
    let k = part.n_classes;
    let reps = part.reps();
    // Máscara de cada clase: las posiciones cuyo conjunto la contiene.
    let mut on = vec![0u64; k * w];
    for (a, &c) in reps.iter().enumerate() {
        for (p, s) in sets.iter().enumerate() {
            if contains(s, c) {
                set(&mut on[a * w..(a + 1) * w], p);
            }
        }
    }
    let hits = |a: &[u64], b: &[u64]| a.iter().zip(b).any(|(x, y)| x & y != 0);
    let mut work: usize = 0;
    // Estado = (posiciones, ¿al principio?, ¿encontrado?).
    type Key = (Vec<u64>, bool, bool);
    let mut ids: HashMap<Key, u32> = HashMap::new();
    let mut keys: Vec<Key> = Vec::new();
    let mut trans: Vec<u32> = Vec::new();
    let start: Key = (vec![0u64; w], true, found0);
    ids.insert(start.clone(), 0);
    keys.push(start);
    let found_key: Key = (vec![0u64; w], false, true);
    let mut i = 0;
    while i < keys.len() {
        let (ps, at_start, found) = keys[i].clone();
        // Lo que puede seguir, sin mirar la clase: una vez por estado.
        let mut u = first_any.clone();
        if at_start {
            for (x, y) in u.iter_mut().zip(&first_start) {
                *x |= y;
            }
        }
        if !found {
            for p in (0..m).filter(|&p| bit(&ps, p)) {
                for (x, y) in u.iter_mut().zip(&follow[p * w..(p + 1) * w]) {
                    *x |= y;
                }
                work = work.saturating_add(w);
            }
        }
        for a in 0..k {
            work = work.saturating_add(w);
            if work > MAX_PATTERN_WORK {
                return Err(PatternError::Limit(keys.len()));
            }
            let key: Key = if found {
                found_key.clone()
            } else {
                let next: Vec<u64> = u
                    .iter()
                    .zip(&on[a * w..(a + 1) * w])
                    .map(|(x, y)| x & y)
                    .collect();
                if hits(&next, &last_free) {
                    found_key.clone()
                } else {
                    (next, false, false)
                }
            };
            let id = match ids.get(&key) {
                Some(&id) => id,
                None => {
                    if keys.len() >= cap {
                        return Err(PatternError::Limit(keys.len() + 1));
                    }
                    let id = keys.len() as u32;
                    ids.insert(key.clone(), id);
                    keys.push(key);
                    id
                }
            };
            trans.push(id);
        }
        i += 1;
    }
    let out: Vec<bool> = keys
        .iter()
        .map(|(ps, at_start, found)| *found || hits(ps, &last_end) || (*at_start && empty_ok))
        .collect();
    let work = work
        .saturating_add(m.saturating_mul(w))
        .saturating_add(keys.len().saturating_mul(w));
    let raw = Dfa {
        n_classes: k,
        start: 0,
        trans,
        out,
    };
    Ok(PatternDfa {
        part,
        dfa: hopcroft(&raw),
        sets,
        work,
    })
}

impl PatternDfa {
    /// Si el AFD acepta `s` (pruebas: lo compara con `Regex::matches`).
    pub fn accepts(&self, s: &str) -> bool {
        let w: Vec<usize> = s
            .chars()
            .map(|c| self.part.class(u32::from(c)) as usize)
            .collect();
        self.dfa.run(&w)
    }
}

/// Para las pruebas de integración (`tests/pattern.rs`): estados del AFD
/// mínimo de `src`, o `None` si pasa de `cap`.
pub fn states(src: &str, d: Dialect, cap: usize) -> Option<usize> {
    compile(src, d, cap).ok().map(|p| p.dfa.n_states())
}

/// Para las pruebas: si el AFD de `src` acepta `s`.
pub fn dfa_accepts(src: &str, d: Dialect, s: &str) -> bool {
    compile(src, d, usize::MAX).is_ok_and(|p| p.accepts(s))
}
