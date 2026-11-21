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
    // Los ids virtuales de F4.5a (componentes con salida) no tienen
    // ubicación propia: se nombra el conjunto por sus componentes reales.
    let n = ir.nodes().len();
    let comps: Vec<NodeId> = comps.iter().copied().filter(|id| id.index() < n).collect();
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
    /// Partición de los puntos de código en clases de carácter (1..; 0 es
    /// «otro»): cada carácter de `chars` en su clase y los conjuntos de los
    /// `pattern` como unión de clases (F4-B3, F4.4b).
    pub(crate) part: crate::pattern::Partition,
    /// Un punto de código de cada clase de carácter.
    reps: Vec<u32>,
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
    /// Ids virtuales de los componentes con salida (F4.5a): el id
    /// `ir.nodes().len() + i` es (nodo real, salida) = `virt[i]`.
    virt: Vec<(NodeId, u32)>,
}

impl<'a> Emitter<'a> {
    /// Tablas vacías con las clases fijas `fixed` (sus nombres en el DOT) y
    /// las clases de carácter de la partición de `chars` (cada uno en la
    /// suya, F4-B3) y de los conjuntos `sets` de los `pattern` (F4.4b); lo
    /// demás es la clase «otro». Sin `sets`, la clase i + 1 es el i-ésimo
    /// carácter de `chars`, como antes de F4.4b. `upper` es la cota de |Q|
    /// para reservar δ. La transición 0 es el error de sintaxis
    /// (`model::ERROR`).
    pub(crate) fn new(
        ir: &'a VerifiedIr,
        chars: &BTreeSet<char>,
        sets: &[ir::regex::CharSet],
        fixed: &[&str],
        upper: usize,
        products: usize,
    ) -> Emitter<'a> {
        let singles: Vec<u32> = chars.iter().map(|&c| u32::from(c)).collect();
        let refs: Vec<&ir::regex::CharSet> = sets.iter().collect();
        let part = crate::pattern::Partition::refine(&singles, &refs);
        let reps = part.reps();
        let n_classes = fixed.len() + part.n_classes;
        let mut class_names: Vec<String> = fixed.iter().map(|s| (*s).to_owned()).collect();
        class_names.push("otro".to_owned());
        let mut primero: Vec<Option<(u32, u32)>> = vec![None; part.n_classes];
        let mut tramos = vec![0usize; part.n_classes];
        for &(lo, hi, k) in &part.runs {
            primero[k as usize].get_or_insert((lo, hi));
            tramos[k as usize] += 1;
        }
        for k in 1..part.n_classes {
            let nombre = match primero[k] {
                Some((lo, hi)) if lo == hi && tramos[k] == 1 => {
                    format!("'{}'", char::from_u32(lo).unwrap_or('?'))
                }
                Some((lo, hi)) => format!(
                    "[{}-{}{}]",
                    char::from_u32(lo).unwrap_or('?').escape_debug(),
                    char::from_u32(hi).unwrap_or('?').escape_debug(),
                    if tramos[k] > 1 { "…" } else { "" }
                ),
                None => format!("#{k}"),
            };
            class_names.push(nombre);
        }
        let mut ascii = vec![0u32; 128];
        let mut ranges = Vec::new();
        for &(lo, hi, k) in &part.runs {
            for c in lo..=hi.min(127) {
                ascii[c as usize] = k;
            }
            if hi >= 128 {
                ranges.push(CharRange {
                    lo: lo.max(128),
                    hi,
                    class: k,
                });
            }
        }
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
            gates: Vec::new(),
            gate_inputs: Vec::new(),
            outmaps: Vec::new(),
            out_targets: Vec::new(),
            scalar_slots: 0,
            ascii,
            ranges,
            rules: Vec::new(),
            text: Vec::new(),
            states: Vec::new(),
            groups: Vec::new(),
            class_names,
            frame_names: Vec::new(),
            products,
            dfa_states: 0,
            dfa_states_min: 0,
        };
        let mut e = Emitter {
            ir,
            n_classes: n_classes as u32,
            part,
            reps,
            a,
            trans_ids: HashMap::new(),
            rule_ids: HashMap::new(),
            loc_ids: HashMap::new(),
            lexeme_ids: HashMap::new(),
            counter_ids: HashMap::new(),
            error: None,
            virt: Vec::new(),
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
            unique: false,
        });
        self.a.frame_names.push(name);
        self.last(self.a.frames.len())
    }

    pub(crate) fn classes_of(&self, w: &str) -> Vec<usize> {
        w.chars()
            .map(|c| self.part.class(u32::from(c)) as usize)
            .collect()
    }

    /// Clase de carácter de `c` (sin sumar `class::CHAR`).
    pub(crate) fn class_of(&self, c: char) -> usize {
        self.part.class(u32::from(c)) as usize
    }

    /// Un punto de código de la clase de carácter `a` (sin la base fija).
    pub(crate) fn class_rep(&self, a: usize) -> u32 {
        self.reps.get(a).copied().unwrap_or(0)
    }

    /// Clases de carácter (sin la base fija).
    pub(crate) fn n_chars(&self) -> usize {
        self.n_classes as usize - runtime_rs::tables::class::CHAR as usize
    }

    /// `d` minimizado con Hopcroft (F4.3), con los estados de antes y de
    /// después sumados al resumen.
    pub(crate) fn minimized<O: Copy + Eq + std::hash::Hash + Default>(
        &mut self,
        d: Dfa<O>,
    ) -> Dfa<O> {
        let m = crate::minimize::hopcroft(&d);
        self.a.dfa_states += d.n_states();
        self.a.dfa_states_min += m.n_states();
        m
    }

    /// Un trie (minimizado) como grupo de estados: (estado de cada estado del
    /// trie, trie).
    pub(crate) fn trie_states(
        &mut self,
        g: u32,
        words: &[&str],
        pre: &str,
        dead: &str,
    ) -> TrieStates {
        let ws: Vec<Vec<usize>> = words.iter().map(|w| self.classes_of(w)).collect();
        let trie = self.minimized(Dfa::trie(self.n_chars(), &ws));
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
        let (real, slot) = self.resolve(node);
        let kind = self.ir.node(real).rules[i].rule.kind();
        let mut c = self.check_of(real, i, kind, test, arg, len);
        c.slot = slot;
        c
    }

    /// Los ids virtuales del análisis (F4.5a).
    pub(crate) fn set_virtuals(&mut self, v: &[(NodeId, u32)]) {
        self.virt = v.to_vec();
    }

    /// (nodo real, salida) de un componente.
    pub(crate) fn resolve(&self, id: NodeId) -> (NodeId, u32) {
        match id.index().checked_sub(self.ir.nodes().len()) {
            Some(i) => self.virt.get(i).copied().unwrap_or((id, 0)),
            None => (id, 0),
        }
    }

    /// Un tipo de marco con salidas y puertas (F4.5a): `words` cuenta los
    /// registros sin las salidas.
    pub(crate) fn frame_with(
        &mut self,
        words: u32,
        mask_words: u32,
        slots: u32,
        gates: (u32, u32),
        name: String,
    ) -> u32 {
        self.a.frames.push(Frame {
            words: words + slots,
            mask_words,
            slots,
            gates: gates.0,
            n_gates: gates.1,
            unique: false,
        });
        self.a.frame_names.push(name);
        self.a.scalar_slots = self.a.scalar_slots.max(slots);
        self.last(self.a.frames.len())
    }

    /// Las puertas de un conjunto en las tablas, en su orden: `bits` da el
    /// bit de máscara de cada clave de `dependentSchemas` (solo en el marco
    /// de objeto; fuera, la puerta nunca salta).
    pub(crate) fn gates(
        &mut self,
        list: &[crate::conj::GateSpec],
        bits: Option<&HashMap<&str, u32>>,
    ) -> (u32, u32) {
        let ini = self.idx(self.a.gates.len());
        for g in list {
            let kind = self.ir.node(g.node).rules[g.rule].rule.kind();
            let rule = self.rule_of(g.node, g.rule, kind);
            let inputs = self.idx(self.a.gate_inputs.len());
            self.a.gate_inputs.extend_from_slice(&g.inputs);
            let bit = match (&g.key, bits) {
                (Some(k), Some(b)) => b.get(k.as_str()).copied().unwrap_or(NO_BIT),
                _ => NO_BIT,
            };
            let n_inputs = self.idx(g.inputs.len());
            self.a.gates.push(runtime_rs::tables::Gate {
                kind: g.kind,
                rule,
                target: g.target,
                inputs,
                n_inputs,
                bit,
            });
        }
        (ini, self.idx(list.len()))
    }

    /// Los mapas de salidas del análisis, con sus ids.
    pub(crate) fn outmaps(&mut self, maps: &[Vec<u32>]) {
        for m in maps {
            let off = self.idx(self.a.out_targets.len());
            self.a.out_targets.extend_from_slice(m);
            let len = self.idx(m.len());
            self.a.outmaps.push(runtime_rs::tables::OutMap { off, len });
        }
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

    /// Guarda un lexema numérico del esquema (cota, divisor, `enum`, `fixed`
    /// o umbral IEEE) en su forma canónica ([`canonical`]) y devuelve su
    /// índice en `nums`.
    pub(crate) fn lexeme(&mut self, l: &str) -> u32 {
        // Los cortos se guardan tal cual (ya cuestan O(64) y el DOT los
        // enseña como se escribieron); los largos, en forma canónica.
        let l = if l.len() <= CANON_SHORT {
            l.to_owned()
        } else {
            canonical(l)
        };
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

/// Lexemas de hasta estos bytes se guardan como se escribieron.
const CANON_SHORT: usize = 64;

/// Cifras significativas que conserva [`canonical`]: más que las que puede
/// tener el lexema de un número del payload (como mucho `LEXEME_MAX` bytes).
const CANON_DIGITS: usize = 1025;
const _: () = assert!(CANON_DIGITS > runtime_rs::LEXEME_MAX);

/// Cifras del exponente por encima de las cuales [`canonical`] lo satura:
/// el de un número del payload, con su desplazamiento, es menor que
/// 10^(`LEXEME_MAX` + 1).
const CANON_EXP_DIGITS: usize = 1100;
const _: () = assert!(CANON_EXP_DIGITS > runtime_rs::LEXEME_MAX + 2);

/// La forma canónica corta de un lexema numérico (§61 de
/// `docs/fase4/pendientes.md`, E3A-2): `[-]0.De±E`, con D las cifras
/// significativas, sin ceros a los lados, y E el exponente exacto, o `0`.
///
/// Las rutinas del núcleo (`num::compare`, `num::is_multiple`) cuestan
/// O(|x| + |cota|) por número del payload. Con una cota escrita con 10⁶
/// cifras, ceros o exponente, cada número costaba O(10⁶). Con la forma
/// canónica cuestan O(`LEXEME_MAX`). Las comparaciones con cualquier número
/// del payload, que tiene como mucho `LEXEME_MAX` cifras significativas y un
/// exponente de menos de `LEXEME_MAX` cifras, no cambian:
///
/// - **D de más de [`CANON_DIGITS`] cifras** se recorta a esas cifras
///   seguidas de un `1`. Ningún x llega a la cifra [`CANON_DIGITS`], así que
///   x y la cota se ordenan por sus primeras cifras, y si x coincide en
///   todas las suyas, la cota es mayor en magnitud con D (no termina en
///   cero) y con el recorte (el `1` final). La igualdad es imposible con las
///   dos.
/// - **Un exponente de más de [`CANON_EXP_DIGITS`] cifras** se satura en
///   ±10^[`CANON_EXP_DIGITS`]: el signo de E_x − E_cota no cambia.
///
/// Las listas de `enum` y los intervalos se ordenan por el valor original
/// antes de reescribirlos. `canonical` es monótona en sentido débil (dos
/// entradas largas pueden quedar iguales), y eso basta a la búsqueda binaria
/// del núcleo, que no exige orden estricto: esas entradas nunca son iguales a
/// un número del payload.
///
/// El lexema ya lo validaron `ir` o `xsd`; aun así, un byte que no es cifra
/// se lee como 9, sin pánico (como `num::dig`).
///
/// Un divisor de `multipleOf` tiene como mucho 38 cifras significativas
/// (`Divisor::of`), así que nunca se recorta. Su exponente saturado tampoco
/// cambia el resultado: k = E_x − E_d sigue siendo negativo, o mayor que 64.
pub(crate) fn canonical(l: &str) -> String {
    let b = l.as_bytes();
    let (neg, rest) = match b.split_first() {
        Some((b'-', r)) => (true, r),
        Some((b'+', r)) => (false, r),
        _ => (false, b),
    };
    let (mant, exp) = match rest.iter().position(|&c| c == b'e' || c == b'E') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, &rest[rest.len()..]),
    };
    let (int, frac) = match mant.iter().position(|&c| c == b'.') {
        Some(i) => (&mant[..i], &mant[i + 1..]),
        None => (mant, &mant[mant.len()..]),
    };
    let (exp_neg, exp) = match exp.split_first() {
        Some((b'-', r)) => (true, r),
        Some((b'+', r)) => (false, r),
        _ => (false, exp),
    };
    let digits: Vec<u8> = int.iter().chain(frac).copied().collect();
    let lead = digits.iter().take_while(|&&c| c == b'0').count();
    if lead == digits.len() {
        return "0".to_owned();
    }
    let trail = digits.iter().rev().take_while(|&&c| c == b'0').count();
    let d = &digits[lead..digits.len() - trail];
    // Valor = 0.D · 10^(exp + |int| − lead).
    let off = int.len() as i128 - lead as i128;
    let exp = &exp[exp.iter().position(|&c| c != b'0').unwrap_or(exp.len())..];
    let (e_neg, e_mag) = if exp.len() <= 30 {
        let v = exp.iter().fold(0i128, |v, &c| {
            v * 10 + i128::from(c.wrapping_sub(b'0').min(9))
        });
        let e = if exp_neg { -v } else { v } + off;
        (e < 0, e.unsigned_abs().to_string())
    } else if exp.len() > CANON_EXP_DIGITS {
        let mut m = String::from("1");
        m.extend(core::iter::repeat_n('0', CANON_EXP_DIGITS));
        (exp_neg, m)
    } else {
        // |exp| ≥ 10³⁰ > |off|: el signo es el de exp, y la magnitud es
        // |exp| ± |off| (suma si tienen el mismo signo).
        (
            exp_neg,
            add_small(exp, exp_neg == (off < 0), off.unsigned_abs()),
        )
    };
    let mut s = String::with_capacity(d.len().min(CANON_DIGITS + 1) + e_mag.len() + 5);
    if neg {
        s.push('-');
    }
    s.push_str("0.");
    if d.len() > CANON_DIGITS {
        s.push_str(core::str::from_utf8(&d[..CANON_DIGITS]).unwrap_or("0"));
        s.push('1');
    } else {
        s.push_str(core::str::from_utf8(d).unwrap_or("0"));
    }
    s.push('e');
    if e_neg && e_mag != "0" {
        s.push('-');
    }
    s.push_str(&e_mag);
    s
}

/// |m| + δ (`suma`) o |m| − δ, con m cifras decimales sin ceros a la
/// izquierda y m > δ. Devuelve las cifras sin ceros a la izquierda.
fn add_small(m: &[u8], suma: bool, delta: u128) -> String {
    let mut v: Vec<u8> = m
        .iter()
        .rev()
        .map(|&c| c.wrapping_sub(b'0').min(9))
        .collect();
    let mut carry = delta;
    let mut i = 0;
    while carry > 0 {
        if i == v.len() {
            v.push(0);
        }
        let d = u128::from(v[i]);
        let c = carry % 10;
        carry /= 10;
        if suma {
            let t = d + c;
            v[i] = (t % 10) as u8;
            carry += t / 10;
        } else if d >= c {
            v[i] = (d - c) as u8;
        } else {
            v[i] = (d + 10 - c) as u8;
            carry += 1;
        }
        i += 1;
    }
    while v.len() > 1 && v.last() == Some(&0) {
        v.pop();
    }
    v.iter().rev().map(|&d| char::from(b'0' + d)).collect()
}

#[cfg(test)]
mod canonica {
    use super::{canonical, CANON_DIGITS, CANON_EXP_DIGITS};
    use core::cmp::Ordering;
    use proptest::prelude::*;
    use runtime_rs::num::{compare, is_multiple, Divisor};

    #[test]
    fn forma_canonica() {
        for (l, c) in [
            ("0", "0"),
            ("-0.000", "0"),
            ("1", "0.1e1"),
            ("-12.5", "-0.125e2"),
            ("0.00120", "0.12e-2"),
            ("100e-5", "0.1e-2"),
            ("1E+3", "0.1e4"),
            ("+7", "0.7e1"),
        ] {
            assert_eq!(canonical(l), c, "{l}");
        }
        // Exponente largo: exacto hasta CANON_EXP_DIGITS cifras, saturado
        // por encima; y la suma con el desplazamiento por encima de 10³⁰.
        let e40 = format!("1{}", "0".repeat(40));
        assert_eq!(
            canonical(&format!("12e{e40}")),
            format!("0.12e{}", "1".to_owned() + &"0".repeat(38) + "02")
        );
        let largo = format!("1e-{}", "9".repeat(CANON_EXP_DIGITS + 5));
        assert_eq!(
            canonical(&largo),
            format!("0.1e-1{}", "0".repeat(CANON_EXP_DIGITS))
        );
        // Mantisa larga: recorte con el 1 testigo.
        let m = format!("0.{}1{}", "0".repeat(1_000_000), "5".repeat(2000));
        let c = canonical(&m);
        assert!(c.len() < CANON_DIGITS + 20, "{}", c.len());
        assert!(
            c.ends_with(&format!("{}1e-1000000", "5".repeat(CANON_DIGITS - 1))),
            "{}",
            &c[c.len() - 30..]
        );
    }

    /// Un número como los del payload: como mucho 1 024 bytes.
    fn corto() -> impl Strategy<Value = String> {
        (
            any::<bool>(),
            "[0-9]{1,20}",
            proptest::option::of("[0-9]{1,20}"),
            proptest::option::of((any::<bool>(), "[0-9]{1,25}")),
        )
            .prop_map(|(n, i, f, e)| {
                let mut s = String::new();
                if n {
                    s.push('-');
                }
                s += &i;
                if let Some(f) = f {
                    s.push('.');
                    s += &f;
                }
                if let Some((en, e)) = e {
                    s.push('e');
                    if en {
                        s.push('-');
                    }
                    s += &e;
                }
                s
            })
    }

    /// Una cota del esquema: a veces con miles de cifras, ceros o un
    /// exponente larguísimo.
    fn cota() -> impl Strategy<Value = String> {
        (
            corto(),
            0usize..4,
            0usize..3000,
            "[0-9]{1,3}",
            proptest::option::of(1usize..1200),
        )
            .prop_map(|(base, forma, n, rel, exp)| {
                let mut s = match forma {
                    0 => base,
                    1 => format!("{base}{}", rel.repeat(n / rel.len().max(1) + 1)),
                    2 => format!("0.{}{rel}", "0".repeat(n)),
                    _ => format!("{}{rel}", "9".repeat(n)),
                };
                if let Some(k) = exp {
                    if !s.contains(['e', 'E']) {
                        s += &format!("e-{}", "7".repeat(k));
                    }
                }
                s
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]
        #[test]
        fn compara_igual(x in corto(), b in cota()) {
            let c = canonical(&b);
            prop_assert_eq!(compare(x.as_bytes(), c.as_bytes()), compare(x.as_bytes(), b.as_bytes()), "x={} b={}", x, &b[..b.len().min(60)]);
            // Y la cota canónica es tan corta como promete.
            prop_assert!(c.len() <= CANON_DIGITS + CANON_EXP_DIGITS + 8);
        }

        #[test]
        fn multiplo_igual(x in corto(), d in "[1-9][0-9]{0,6}", ceros in 0usize..2000, e in proptest::option::of((any::<bool>(), "[0-9]{1,40}"))) {
            let mut l = format!("{d}{}", "0".repeat(ceros % 5));
            if ceros > 1000 {
                l = format!("0.{}{d}", "0".repeat(ceros));
            }
            if let Some((en, e)) = e {
                l += &format!("e{}{e}", if en { "-" } else { "" });
            }
            if let Ok(div) = Divisor::of(l.as_bytes()) {
                let c = canonical(&l);
                prop_assert_eq!(Divisor::of(c.as_bytes()), Ok(div));
                prop_assert_eq!(is_multiple(x.as_bytes(), c.as_bytes(), &div), is_multiple(x.as_bytes(), l.as_bytes(), &div), "x={} d={}", x, l);
            }
        }
    }

    #[test]
    fn orden_de_la_cota_recortada() {
        // x coincide en todas sus cifras con la cota y la cota sigue.
        let b = format!("1.{}", "2".repeat(5000));
        let x = format!("1.{}", "2".repeat(1000));
        assert_eq!(
            compare(x.as_bytes(), canonical(&b).as_bytes()),
            Ordering::Less
        );
        assert_eq!(compare(x.as_bytes(), b.as_bytes()), Ordering::Less);
    }
}
