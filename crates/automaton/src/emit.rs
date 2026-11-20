//! Utilidades comunes de la construcción de las tablas de M (F4.1b-CR-S6):
//! las comparten el constructor de JSON Schema ([`crate::json`]) y el de XSD
//! ([`crate::xml`]), para que las dos construcciones escriban las tablas con
//! las mismas reglas (memoria por transición, por regla y por lexema;
//! índices de 32 bits comprobados; el presupuesto de F4-U6, §6.4 y §6.6 de
//! `docs/fase4/automata.md`).
//!
//! [`Emitter`] guarda el autómata en construcción y las memorias que evitan
//! repetir una transición, una regla, una ubicación o un lexema; las
//! funciones libres son las que no necesitan el autómata (tamaño de un trie
//! contado sin construirlo, nombres para el DOT y los errores).

use std::collections::{BTreeSet, HashMap};

use ir::{NodeId, VerifiedIr};
use runtime_rs::tables::{
    CharRange, Check, CounterOp, Frame, Lexeme, Op, Rule as RuleRef, Test, Trans, NO_BIT,
    OUT_IDENTITY,
};

use crate::dfa::Dfa;
use crate::location::{location_clipped, LOCATION_MAX};
use crate::model::{Automaton, StateInfo};
use crate::weight::cost;
use crate::BuildError;

/// Un trie incrustado: el estado de M de cada estado del trie, y el trie.
pub(crate) type TrieStates = (Vec<u32>, Dfa<Option<u32>>);

/// Una comprobación de contador o longitud: (nodo, índice de la regla,
/// prueba, cota).
pub(crate) type Bound = (NodeId, usize, Test, u64);

/// Entradas del presupuesto de F4-U6 (precisado en F4.1b): las de δ
/// (|Q| · |Σ|) más las de las tablas que crecen con los productos.
pub(crate) fn budget(upper: usize, n_classes: usize, extra: usize) -> usize {
    upper.saturating_mul(n_classes).saturating_add(extra)
}

/// El presupuesto de F4-U6 agotado, con la ubicación del conjunto que lo
/// agotó (o la raíz, en la comprobación final).
pub(crate) fn delta_limit(
    upper: usize,
    n_classes: usize,
    extra: usize,
    limit: usize,
    location: String,
) -> BuildError {
    BuildError::Limit {
        what: "entradas de las tablas como mucho (cota de estados × clases de δ, más comprobaciones, máscaras y lexemas)",
        limit,
        found: budget(upper, n_classes, extra),
        location,
    }
}

/// `n` como índice `u32` de una tabla del análisis, o `limit`.
pub(crate) fn table_index(n: usize) -> Result<u32, BuildError> {
    u32::try_from(n).map_err(|_| BuildError::Limit {
        what: "entradas de una tabla de M (índices de 32 bits)",
        limit: u32::MAX as usize,
        found: n,
        location: "#".to_owned(),
    })
}

/// Estados del trie de `words` (uno por prefijo distinto, más el muerto),
/// contados sin construirlo: con las palabras ordenadas, cada una añade sus
/// caracteres menos el prefijo común con la anterior. El prefijo común se
/// mide por bytes y se recorta a la frontera de carácter (los bytes iguales
/// antes de ella son los mismos caracteres en las dos palabras).
pub(crate) fn trie_size(words: &[&str]) -> usize {
    #[cfg(test)]
    CARACTERES.with(|c| c.set(c.get() + words.iter().map(|w| w.len() as u64).sum::<u64>()));
    let mut v: Vec<&str> = words.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut n: usize = 2;
    for (i, w) in v.iter().enumerate() {
        let lcp = if i == 0 {
            0
        } else {
            let mut l = w
                .bytes()
                .zip(v[i - 1].bytes())
                .take_while(|(a, b)| a == b)
                .count();
            while !w.is_char_boundary(l) {
                l -= 1;
            }
            l
        };
        n = n.saturating_add(w[lcp..].chars().count());
    }
    n
}

#[cfg(test)]
std::thread_local! {
    /// Bytes de nombres que ha recorrido [`trie_size`] en este hilo (solo
    /// pruebas: ronda extra de F4.1c, I6/F4E-M1).
    pub(crate) static CARACTERES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Nombre de un conjunto para el DOT y los errores: las ubicaciones de sus
/// componentes (hasta tres) unidas con «∧».
pub(crate) fn label(ir: &VerifiedIr, comps: &[NodeId]) -> String {
    let mut parts: Vec<String> = comps
        .iter()
        .take(3)
        .map(|&id| location_clipped(ir, id, 64))
        .collect();
    if comps.len() > 3 {
        parts.push("…".to_owned());
    }
    if parts.is_empty() {
        "#".to_owned()
    } else {
        parts.join(" ∧ ")
    }
}

/// `s` recortado a 32 caracteres para los nombres de estados.
pub(crate) fn clip(s: &str) -> String {
    let mut out: String = s.chars().take(32).collect();
    if s.chars().nth(32).is_some() {
        out.push('…');
    }
    out
}

/// Prefijo que lee cada estado del trie (para el nombre del estado), de la
/// primera palabra que pasa por él y recortado a 32 caracteres: O(Σ |w|).
fn prefix_names(trie: &Dfa<Option<u32>>, names: &[&str], words: &[Vec<usize>]) -> Vec<String> {
    const MAX: usize = 32;
    let mut out: Vec<Option<String>> = vec![None; trie.n_states()];
    out[trie.start as usize] = Some(String::new());
    for (name, w) in names.iter().zip(words) {
        let mut q = trie.start;
        for (i, &a) in w.iter().enumerate() {
            q = trie.next(q, a);
            if out[q as usize].is_none() {
                let mut p: String = name.chars().take((i + 1).min(MAX)).collect();
                if i + 1 > MAX {
                    p.push('…');
                }
                out[q as usize] = Some(p);
            }
        }
    }
    out.into_iter().map(Option::unwrap_or_default).collect()
}

/// El autómata en construcción y las memorias comunes.
pub(crate) struct Emitter<'a> {
    pub(crate) ir: &'a VerifiedIr,
    /// |Σ|.
    pub(crate) n_classes: u32,
    /// Punto de código → clase de carácter (1..; 0 es «otro»).
    pub(crate) char_class: HashMap<char, u32>,
    pub(crate) a: Automaton,
    trans_ids: HashMap<Trans, u32>,
    rule_ids: HashMap<(NodeId, usize), u32>,
    loc_ids: HashMap<NodeId, (u32, u32)>,
    /// Lexema de cada cota o divisor en las tablas: una vez por regla.
    lexeme_ids: HashMap<(NodeId, usize), u32>,
    /// Listas de operaciones de contador ya guardadas.
    counter_ids: HashMap<Vec<CounterOp>, (u32, u32)>,
    /// El primer error al rellenar (un índice que no cabe en `u32`, un nombre
    /// fuera de la unión): [`Emitter::finish`] lo devuelve.
    pub(crate) error: Option<BuildError>,
}

impl<'a> Emitter<'a> {
    /// Tablas vacías con las clases fijas `fixed` (sus nombres en el DOT) y
    /// una clase de carácter por punto de código de `chars` (F4-B3); los
    /// demás, la clase «otro». `upper` es la cota de |Q| para reservar δ.
    /// La transición 0 es el error de sintaxis (`model::ERROR`).
    pub(crate) fn new(
        ir: &'a VerifiedIr,
        chars: &BTreeSet<char>,
        fixed: &[&str],
        upper: usize,
        products: usize,
    ) -> Emitter<'a> {
        let n_classes = fixed.len() + 1 + chars.len();
        let char_class: HashMap<char, u32> = chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u32 + 1))
            .collect();
        let mut class_names: Vec<String> = fixed.iter().map(|s| (*s).to_owned()).collect();
        class_names.push("otro".to_owned());
        class_names.extend(chars.iter().map(|c| format!("'{c}'")));
        let mut ascii = vec![0u32; 128];
        let mut ranges = Vec::new();
        for (&c, &k) in &char_class {
            if (c as u32) < 128 {
                ascii[c as usize] = k;
            } else {
                ranges.push(CharRange {
                    lo: c as u32,
                    hi: c as u32,
                    class: k,
                });
            }
        }
        ranges.sort_by_key(|r| r.lo);
        let a = Automaton {
            format: crate::model::PayloadFormat::Json,
            n_classes: n_classes as u32,
            start: 0,
            accept: 0,
            delta: Vec::with_capacity(upper.saturating_mul(n_classes)),
            trans: Vec::new(),
            checks: Vec::new(),
            frames: Vec::new(),
            masks: Vec::new(),
            deps: Vec::new(),
            nums: Vec::new(),
            lexemes: Vec::new(),
            divisors: Vec::new(),
            counter_ops: Vec::new(),
            intervals: Vec::new(),
            ascii,
            ranges,
            rules: Vec::new(),
            text: Vec::new(),
            states: Vec::new(),
            groups: Vec::new(),
            class_names,
            frame_names: Vec::new(),
            products,
        };
        let mut e = Emitter {
            ir,
            n_classes: n_classes as u32,
            char_class,
            a,
            trans_ids: HashMap::new(),
            rule_ids: HashMap::new(),
            loc_ids: HashMap::new(),
            lexeme_ids: HashMap::new(),
            counter_ids: HashMap::new(),
            error: None,
        };
        e.simple(Op::Fail(runtime_rs::tables::Fault::Syntax), 0);
        e
    }

    // ----------------------------------------------------- reservas ---

    /// `n` como índice `u32` de las tablas; si no cabe, guarda un `limit`
    /// (que devuelve `finish`) en vez de truncar: un índice truncado
    /// señalaría otra entrada válida y daría un veredicto incorrecto.
    pub(crate) fn idx(&mut self, n: usize) -> u32 {
        match u32::try_from(n) {
            Ok(v) => v,
            Err(_) => {
                if self.error.is_none() {
                    self.error = Some(BuildError::Limit {
                        what: "entradas de una tabla de M (índices de 32 bits)",
                        limit: u32::MAX as usize,
                        found: n,
                        location: "#".to_owned(),
                    });
                }
                u32::MAX
            }
        }
    }

    /// El último índice de una tabla de `len` entradas.
    pub(crate) fn last(&mut self, len: usize) -> u32 {
        self.idx(len.saturating_sub(1))
    }

    pub(crate) fn internal(&mut self, m: String) {
        if self.error.is_none() {
            self.error = Some(BuildError::Internal(m));
        }
    }

    pub(crate) fn group(&mut self, name: String) -> u32 {
        self.a.groups.push(name);
        self.last(self.a.groups.len())
    }

    pub(crate) fn state(&mut self, group: u32, name: impl Into<String>) -> u32 {
        self.a.states.push(StateInfo {
            group,
            name: name.into(),
        });
        self.a
            .delta
            .extend(std::iter::repeat_n(0, self.n_classes as usize));
        self.last(self.a.states.len())
    }

    pub(crate) fn frame(&mut self, words: u32, mask_words: u32, name: String) -> u32 {
        self.a.frames.push(Frame {
            words,
            mask_words,
            slots: 0,
            gates: 0,
            n_gates: 0,
        });
        self.a.frame_names.push(name);
        self.last(self.a.frames.len())
    }

    pub(crate) fn classes_of(&self, w: &str) -> Vec<usize> {
        w.chars()
            .map(|c| self.char_class.get(&c).copied().unwrap_or(0) as usize)
            .collect()
    }

    /// Clase de carácter de `c` (sin sumar `class::CHAR`).
    pub(crate) fn class_of(&self, c: char) -> usize {
        self.char_class.get(&c).copied().unwrap_or(0) as usize
    }

    /// Clases de carácter (sin la base fija).
    pub(crate) fn n_chars(&self) -> usize {
        self.n_classes as usize - runtime_rs::tables::class::CHAR as usize
    }

    /// Un trie como grupo de estados: (estado de cada estado del trie, trie).
    pub(crate) fn trie_states(
        &mut self,
        g: u32,
        words: &[&str],
        pre: &str,
        dead: &str,
    ) -> TrieStates {
        let ws: Vec<Vec<usize>> = words.iter().map(|w| self.classes_of(w)).collect();
        let trie = Dfa::trie(self.n_chars(), &ws);
        let prefixes = prefix_names(&trie, words, &ws);
        let mut states = Vec::with_capacity(trie.n_states());
        for s in 0..trie.n_states() as u32 {
            let name = if trie.is_dead(s) {
                dead.to_owned()
            } else {
                format!("{pre}«{}»", prefixes[s as usize])
            };
            states.push(self.state(g, name));
        }
        (states, trie)
    }

    // --------------------------------------------------- rellenado ---

    /// δ(q, a) = t.
    pub(crate) fn set(&mut self, q: u32, a: u32, t: u32) {
        self.a.delta[q as usize * self.n_classes as usize + a as usize] = t;
    }

    pub(crate) fn trans(&mut self, t: Trans) -> u32 {
        if let Some(&id) = self.trans_ids.get(&t) {
            return id;
        }
        self.a.trans.push(t);
        let id = self.last(self.a.trans.len());
        self.trans_ids.insert(t, id);
        id
    }

    /// Una transición sin registros con `list` como comprobaciones.
    pub(crate) fn with_checks(&mut self, op: Op, next: u32, list: Vec<Check>) -> u32 {
        let (checks, n_checks) = self.checks(list);
        self.trans(Trans {
            checks,
            n_checks,
            ..plain(op, next)
        })
    }

    pub(crate) fn simple(&mut self, op: Op, next: u32) -> u32 {
        self.with_checks(op, next, Vec::new())
    }

    pub(crate) fn mv(&mut self, next: u32) -> u32 {
        self.simple(Op::Move, next)
    }

    /// Guarda una lista de operaciones de contador (una vez cada lista
    /// distinta) y devuelve (inicio, número).
    pub(crate) fn counter_ops(&mut self, ops: &[CounterOp]) -> (u32, u32) {
        if ops.is_empty() {
            return (0, 0);
        }
        if let Some(&r) = self.counter_ids.get(ops) {
            return r;
        }
        let ini = self.idx(self.a.counter_ops.len());
        let n = self.idx(ops.len());
        self.a.counter_ops.extend_from_slice(ops);
        self.counter_ids.insert(ops.to_vec(), (ini, n));
        (ini, n)
    }

    /// Índice de la regla `i` del nodo en la tabla de reglas: la
    /// comprobación se **atribuye a su componente** (su nodo y su regla).
    /// `i` = `usize::MAX` con `kind` es una regla implícita del nodo (en
    /// XML, el contenido vacío o la ausencia de atributos de un elemento
    /// sin esas reglas).
    pub(crate) fn rule_of(&mut self, node: NodeId, i: usize, kind: ir::RuleKind) -> u32 {
        let key = if i == usize::MAX {
            (node, usize::MAX - kind as usize)
        } else {
            (node, i)
        };
        if let Some(&r) = self.rule_ids.get(&key) {
            return r;
        }
        let (loc, loc_len) = match self.loc_ids.get(&node) {
            Some(&l) => l,
            None => {
                let s = location_clipped(self.ir, node, LOCATION_MAX);
                let l = (self.idx(self.a.text.len()), self.idx(s.len()));
                self.a.text.extend_from_slice(s.as_bytes());
                self.loc_ids.insert(node, l);
                l
            }
        };
        let node_ix = self.idx(node.index());
        self.a.rules.push(RuleRef {
            node: node_ix,
            kind: kind as u32,
            loc,
            loc_len,
        });
        let r = self.last(self.a.rules.len());
        self.rule_ids.insert(key, r);
        r
    }

    pub(crate) fn check(
        &mut self,
        node: NodeId,
        i: usize,
        test: Test,
        arg: u32,
        len: u32,
    ) -> Check {
        let kind = self.ir.node(node).rules[i].rule.kind();
        self.check_of(node, i, kind, test, arg, len)
    }

    /// [`Emitter::check`] de una regla dada por su clase (también las
    /// implícitas, `i` = `usize::MAX`).
    pub(crate) fn check_of(
        &mut self,
        node: NodeId,
        i: usize,
        kind: ir::RuleKind,
        test: Test,
        arg: u32,
        len: u32,
    ) -> Check {
        Check {
            test,
            rule: self.rule_of(node, i, kind),
            cost: cost(kind),
            slot: 0,
            counter: 0,
            arg,
            len,
        }
    }

    pub(crate) fn bound(&mut self, (id, i, test, n): Bound) -> Check {
        let (arg, len) = Check::split(n);
        self.check(id, i, test, arg, len)
    }

    /// Guarda las comprobaciones ordenadas por costo (estable: a igualdad,
    /// el orden de las reglas) y devuelve (inicio, número).
    pub(crate) fn checks(&mut self, mut list: Vec<Check>) -> (u32, u32) {
        if list.is_empty() {
            return (0, 0);
        }
        list.sort_by_key(|c| c.cost);
        let ini = self.idx(self.a.checks.len());
        let n = self.idx(list.len());
        self.a.checks.extend(list);
        (ini, n)
    }

    /// El lexema de la regla `i` de `id` (una cota o un divisor), guardado
    /// una sola vez aunque lo lean muchos conjuntos.
    pub(crate) fn rule_lexeme(&mut self, id: NodeId, i: usize, l: &str) -> u32 {
        if let Some(&k) = self.lexeme_ids.get(&(id, i)) {
            return k;
        }
        let k = self.lexeme(l);
        self.lexeme_ids.insert((id, i), k);
        k
    }

    /// Guarda un lexema del esquema y devuelve su índice en `nums`.
    pub(crate) fn lexeme(&mut self, l: &str) -> u32 {
        let off = self.idx(self.a.lexemes.len());
        let len = self.idx(l.len());
        self.a.nums.push(Lexeme { off, len });
        self.a.lexemes.extend_from_slice(l.as_bytes());
        self.last(self.a.nums.len())
    }

    pub(crate) fn finish(self) -> Result<Automaton, BuildError> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(self.a),
        }
    }
}

/// Una transición sin registros ni comprobaciones.
pub(crate) const fn plain(op: Op, next: u32) -> Trans {
    Trans {
        op,
        next,
        ret: 0,
        frame: 0,
        bit: NO_BIT,
        mark: NO_BIT,
        counts: 0,
        n_counts: 0,
        checks: 0,
        n_checks: 0,
        outs: OUT_IDENTITY,
    }
}
