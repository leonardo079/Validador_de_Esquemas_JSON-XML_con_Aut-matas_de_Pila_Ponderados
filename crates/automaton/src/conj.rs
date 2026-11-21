//! Cierre conjuntivo de E₌ (F4-U2, S-3 de `docs/fase3/pendientes.md` §5):
//! el conjunto canónico de `NodeId` cuyas reglas lee una instancia.
//!
//! C(v) son los nodos que **aportan reglas propias** (alguna regla que no es
//! `$ref` ni `allOf`) entre v y los alcanzables desde v por las aristas
//! `$ref` y `allOf`, que aplican su destino a la misma instancia. Es un
//! conjunto, no una lista de caminos: la cadena de diamantes de
//! `referencias_compartidas_tamano_lineal` (2^(10⁴ − 1) caminos) da un solo
//! nodo. Es finito y se calcula sin ciclos porque el subgrafo de E₌ es
//! acíclico (I14); si no lo fuera, es un error interno (defensa en
//! profundidad, F4-B11), nunca un cuelgue. Cada conjunto se guarda una vez
//! (`Sets`, por su lista ordenada) y su id es el tipo de marco que lo lee.
//! Teoría: `docs/fase4/automata.md` §6.4.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ir::{Node, NodeId, Rule};
use runtime_rs::tables::{GateKind, OUT_IDENTITY};

use crate::BuildError;

/// Una puerta de un conjunto (F4.5a): combina las salidas `inputs` y suma a
/// `target` (0: el destino base del conjunto). La regla es la del
/// combinador; `key`, la clave de `dependentSchemas`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GateSpec {
    pub(crate) kind: GateKind,
    pub(crate) node: NodeId,
    pub(crate) rule: usize,
    pub(crate) target: u32,
    pub(crate) inputs: Vec<u32>,
    pub(crate) key: Option<String>,
}

/// Lo que cada conjunto tiene además de sus componentes: sus salidas (las
/// 1..=`n_out` van al padre por el mapa de la transición que lo abre; las
/// demás son de las ramas) y sus puertas en orden de evaluación.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Meta {
    pub(crate) n_slots: u32,
    pub(crate) gates: Rc<[GateSpec]>,
}

/// Lo que da el DFS conjuntivo de un nodo: los que aportan reglas propias y
/// las reglas de los combinadores encontrados (nodo, índice).
#[derive(Debug, Default)]
struct Raw {
    nodes: Vec<NodeId>,
    combs: Vec<(usize, usize)>,
}

/// Si la regla es un combinador (F4.5a).
fn combinator(r: &Rule) -> bool {
    matches!(
        r,
        Rule::AnyOf(_) | Rule::OneOf(_) | Rule::Not(_) | Rule::DependentSchemas(_)
    )
}

/// Si el nodo aporta reglas que el constructor compila (ni aristas
/// conjuntivas ni combinadores, que el cierre resuelve).
fn builds(node: &Node) -> bool {
    node.rules
        .iter()
        .any(|c| !matches!(c.rule, Rule::Ref(_) | Rule::AllOf(_)) && !combinator(&c.rule))
}

/// Presupuesto de trabajo de los cierres por esquema (nodos y aristas
/// recorridos, y tamaños de las uniones): con memoria por destino y alias
/// resueltos es O(n) en los esquemas normales; un grafo hostil (muchos
/// destinos distintos con cierres grandes) da `limit` en vez de tiempo
/// cuadrático.
pub const MAX_CLOSURE_WORK: usize = 1 << 24;

/// `what` del `limit` del presupuesto de los cierres (el constructor le pone
/// la ubicación del conjunto que lo agotó).
pub(crate) const CLOSURE_WHAT: &str = "pasos del cierre conjuntivo de $ref/allOf";

/// Id de un conjunto canónico; [`ANY`] es el vacío, «cualquiera».
pub(crate) type SetId = u32;

/// El conjunto vacío: el marco «cualquiera» (esquema `true`).
pub(crate) const ANY: SetId = 0;

/// Sin destino (un nodo sin reglas o solo con `allOf` vacío).
const NONE: u32 = u32::MAX;

/// Si el nodo aporta reglas propias a la instancia.
pub(crate) fn contributes(node: &Node) -> bool {
    node.rules
        .iter()
        .any(|c| !matches!(c.rule, Rule::Ref(_) | Rule::AllOf(_)))
}

/// Destinos de las aristas conjuntivas de E₌ (`$ref`, `allOf`), en orden.
fn conj_targets(node: &Node, out: &mut Vec<NodeId>) {
    out.clear();
    for c in &node.rules {
        match &c.rule {
            Rule::Ref(t) => out.push(*t),
            Rule::AllOf(v) => out.extend(v),
            _ => {}
        }
    }
}

/// Resuelve los **alias**: un nodo que no aporta reglas y tiene un solo
/// destino conjuntivo distinto (`$ref` sin hermanas, `allOf: [X, X]`) tiene
/// el cierre de ese destino. Cada cadena se recorre una vez (memoria por
/// nodo, compresión de caminos): O(n + m). `alias[v]` es el representante,
/// o [`NONE`] si no aporta nada (`true`, `{}`). Una cadena de más de n
/// pasos solo puede ser un ciclo, que I14 excluye: `Internal`.
pub(crate) fn resolve_aliases(nodes: &[Node]) -> Result<Vec<u32>, BuildError> {
    const PENDIENTE: u32 = u32::MAX - 1;
    let n = nodes.len();
    let mut out = vec![PENDIENTE; n];
    let mut path = Vec::new();
    let mut buf = Vec::new();
    for v in 0..n {
        let mut cur = v;
        let fin = loop {
            if out[cur] != PENDIENTE {
                break out[cur];
            }
            if path.len() > n {
                return Err(BuildError::Internal(format!(
                    "ciclo de $ref/allOf sin reglas propias desde el nodo {v} (I14)"
                )));
            }
            let node = &nodes[cur];
            if contributes(node) {
                break cur as u32;
            }
            conj_targets(node, &mut buf);
            buf.sort_unstable();
            buf.dedup();
            match buf.as_slice() {
                [] => break NONE,
                [t] if t.index() < n => {
                    path.push(cur);
                    cur = t.index();
                }
                [t] => {
                    return Err(BuildError::Internal(format!(
                        "destino {} fuera de la IR",
                        t.index()
                    )))
                }
                _ => break cur as u32,
            }
        };
        out[cur] = fin;
        for p in path.drain(..) {
            out[p] = fin;
        }
        out[v] = fin;
    }
    Ok(out)
}

/// Los conjuntos canónicos, cada uno una vez: la lista y la tabla de ids
/// comparten la misma copia (`Rc`; F4D-M1 de la ronda 3 de F4.1c: antes cada
/// conjunto se guardaba dos veces).
#[derive(Debug)]
pub(crate) struct Sets {
    list: Vec<Rc<[NodeId]>>,
    meta: Vec<Meta>,
    ids: HashMap<(Rc<[NodeId]>, Meta), SetId>,
}

impl Sets {
    fn new() -> Sets {
        let vacio: Rc<[NodeId]> = Rc::from(Vec::new());
        Sets {
            list: vec![Rc::clone(&vacio)],
            meta: vec![Meta::default()],
            ids: HashMap::from([((vacio, Meta::default()), ANY)]),
        }
    }

    /// Id de `v` (ya ordenado y sin repetir) con sus salidas y puertas.
    fn intern(&mut self, v: Vec<NodeId>, meta: Meta) -> SetId {
        let v: Rc<[NodeId]> = Rc::from(v);
        let key = (Rc::clone(&v), meta.clone());
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.list.len() as SetId;
        self.list.push(v);
        self.meta.push(meta);
        self.ids.insert(key, id);
        id
    }

    /// Los componentes (con destino ≠ 0, ids virtuales: `Closures::real`).
    pub(crate) fn get(&self, id: SetId) -> &[NodeId] {
        &self.list[id as usize]
    }

    /// Salidas y puertas del conjunto.
    pub(crate) fn meta(&self, id: SetId) -> &Meta {
        &self.meta[id as usize]
    }

    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }
}

/// Cálculo memorizado de C(v) y de uniones de cierres.
#[derive(Debug)]
pub(crate) struct Closures<'a> {
    nodes: &'a [Node],
    alias: Vec<u32>,
    memo: Vec<Option<SetId>>,
    raws: Vec<Option<Rc<Raw>>>,
    /// Ids virtuales (F4.5a): el componente (nodo real, destino k ≥ 1) es
    /// el id `nodes.len() + i` con `virt[i]` = (nodo, k).
    virt: Vec<(NodeId, u32)>,
    virt_ids: HashMap<(usize, u32), usize>,
    /// Mapas de salidas internados (destinos en el padre).
    pub(crate) outmaps: Vec<Vec<u32>>,
    outmap_ids: HashMap<Vec<u32>, u32>,
    /// Marcas del DFS: (generación, en la pila).
    mark: Vec<(u32, bool)>,
    generation: u32,
    work: usize,
    pub(crate) sets: Sets,
}

impl<'a> Closures<'a> {
    pub(crate) fn new(nodes: &'a [Node]) -> Result<Closures<'a>, BuildError> {
        Ok(Closures {
            nodes,
            alias: resolve_aliases(nodes)?,
            memo: vec![None; nodes.len()],
            raws: vec![None; nodes.len()],
            virt: Vec::new(),
            virt_ids: HashMap::new(),
            outmaps: Vec::new(),
            outmap_ids: HashMap::new(),
            mark: vec![(0, false); nodes.len()],
            generation: 0,
            work: 0,
            sets: Sets::new(),
        })
    }

    /// Descuenta `n` pasos del presupuesto; `limit` si se agota. El
    /// constructor descuenta también, **antes** de reservar, el trabajo
    /// proporcional al producto (nombres × componentes).
    pub(crate) fn spend(&mut self, n: usize) -> Result<(), BuildError> {
        self.work = self.work.saturating_add(n);
        if self.work > MAX_CLOSURE_WORK {
            return Err(BuildError::Limit {
                what: CLOSURE_WHAT,
                limit: MAX_CLOSURE_WORK,
                found: self.work,
                location: "#".to_owned(),
            });
        }
        Ok(())
    }

    /// El nodo real de un componente (F4.5a: los ids virtuales).
    pub(crate) fn real(&self, id: NodeId) -> NodeId {
        match id.index().checked_sub(self.nodes.len()) {
            Some(i) => self.virt.get(i).map_or(id, |v| v.0),
            None => id,
        }
    }

    /// El destino de un componente: 0 (el destino base) o la salida k.
    pub(crate) fn tgt(&self, id: NodeId) -> u32 {
        match id.index().checked_sub(self.nodes.len()) {
            Some(i) => self.virt.get(i).map_or(0, |v| v.1),
            None => 0,
        }
    }

    /// Los ids virtuales resueltos: (nodo real, destino) de cada uno.
    pub(crate) fn virtuals(&self) -> &[(NodeId, u32)] {
        &self.virt
    }

    /// El id del componente (nodo real `x`, destino `t`).
    fn vid(&mut self, x: usize, t: u32) -> NodeId {
        if t == 0 {
            return NodeId::new(x);
        }
        let n = self.nodes.len();
        let i = *self.virt_ids.entry((x, t)).or_insert_with(|| {
            self.virt.push((NodeId::new(x), t));
            self.virt.len() - 1
        });
        NodeId::new(n + i)
    }

    /// El alias de `v`, o `None` si no aporta nada.
    fn alias_of(&self, v: NodeId) -> Result<Option<usize>, BuildError> {
        match self.alias.get(v.index()) {
            Some(&r) if r == NONE => Ok(None),
            Some(&r) => Ok(Some(r as usize)),
            None => Err(BuildError::Internal(format!(
                "nodo {} fuera de la IR",
                v.index()
            ))),
        }
    }

    /// Id interno del mapa de salidas `targets`.
    pub(crate) fn outmap(&mut self, targets: Vec<u32>) -> u32 {
        if let Some(&i) = self.outmap_ids.get(&targets) {
            return i;
        }
        let i = self.outmaps.len() as u32;
        self.outmaps.push(targets.clone());
        self.outmap_ids.insert(targets, i);
        i
    }

    /// El mapa de salidas con el que se abre el conjunto `s` desde la raíz
    /// (sin padre: ninguna salida va a él).
    pub(crate) fn root_outs(&mut self, s: SetId) -> u32 {
        if self.sets.meta(s).n_slots == 0 {
            OUT_IDENTITY
        } else {
            self.outmap(Vec::new())
        }
    }

    /// C(v) con los combinadores (F4.5a): los componentes del cierre
    /// conjuntivo de v con destino 0; cada rama de un `anyOf`, `oneOf`,
    /// `not` o `dependentSchemas` alcanzado abre una salida (una por nodo de
    /// rama) con el cierre de la rama como destino, y una puerta que la
    /// combina hacia el destino del combinador. Sin combinadores es el
    /// cierre conjuntivo de antes. Las puertas se ordenan para que cada una
    /// se evalúe después de las que escriben sus entradas.
    pub(crate) fn of(&mut self, v: NodeId) -> Result<SetId, BuildError> {
        let Some(r) = self.alias_of(v)? else {
            return Ok(ANY);
        };
        if let Some(id) = self.memo[r] {
            return Ok(id);
        }
        let raw = self.raw(r)?;
        if raw.combs.is_empty() {
            let id = self.sets.intern(raw.nodes.clone(), Meta::default());
            self.memo[r] = Some(id);
            return Ok(id);
        }
        let mut comps: Vec<NodeId> = Vec::new();
        let mut slot_of: HashMap<Option<usize>, u32> = HashMap::new();
        let mut n_slots = 0u32;
        let mut gates: Vec<GateSpec> = Vec::new();
        let mut seen: HashSet<(usize, u32)> = HashSet::new();
        let mut work = vec![(r, 0u32)];
        while let Some((x, t)) = work.pop() {
            self.spend(1)?;
            if !seen.insert((x, t)) {
                continue;
            }
            let raw = self.raw(x)?;
            for &n in &raw.nodes {
                let id = self.vid(n.index(), t);
                comps.push(id);
            }
            for &(y, i) in &raw.combs {
                let nodes = self.nodes;
                let rule = &nodes[y].rules[i].rule;
                let ramas: Vec<(Option<String>, NodeId)> = match rule {
                    Rule::AnyOf(v) | Rule::OneOf(v) => v.iter().map(|&m| (None, m)).collect(),
                    Rule::Not(m) => vec![(None, *m)],
                    Rule::DependentSchemas(v) => {
                        v.iter().map(|(k, m)| (Some(k.clone()), *m)).collect()
                    }
                    _ => continue,
                };
                let mut ins = Vec::with_capacity(ramas.len());
                for (_, m) in &ramas {
                    let a = self.alias_of(*m)?;
                    let k = *slot_of.entry(a).or_insert_with(|| {
                        n_slots += 1;
                        n_slots
                    });
                    ins.push(k);
                    if let Some(a) = a {
                        work.push((a, k));
                    }
                }
                let node = NodeId::new(y);
                match rule {
                    Rule::DependentSchemas(_) => {
                        for ((key, _), k) in ramas.into_iter().zip(ins) {
                            gates.push(GateSpec {
                                kind: GateKind::Implies,
                                node,
                                rule: i,
                                target: t,
                                inputs: vec![k],
                                key,
                            });
                        }
                    }
                    _ => gates.push(GateSpec {
                        kind: match rule {
                            Rule::AnyOf(_) => GateKind::Any,
                            Rule::OneOf(_) => GateKind::One,
                            _ => GateKind::Not,
                        },
                        node,
                        rule: i,
                        target: t,
                        inputs: ins,
                        key: None,
                    }),
                }
            }
        }
        comps.sort_unstable();
        comps.dedup();
        let gates = topo(gates)?;
        self.spend(comps.len() + gates.len())?;
        let id = self.sets.intern(
            comps,
            Meta {
                n_slots,
                gates: Rc::from(gates),
            },
        );
        self.memo[r] = Some(id);
        Ok(id)
    }

    /// El DFS conjuntivo de `r` (alias resuelto): sus nodos con reglas
    /// propias y sus combinadores, memorizado.
    fn raw(&mut self, r: usize) -> Result<Rc<Raw>, BuildError> {
        if let Some(x) = &self.raws[r] {
            return Ok(Rc::clone(x));
        }
        self.generation += 1;
        let g = self.generation;
        let mut found = Vec::new();
        let mut combs = Vec::new();
        let mut buf = Vec::new();
        // (nodo, destinos pendientes).
        let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut enter = |c: &mut Self, x: usize, stack: &mut Vec<(usize, Vec<usize>)>| {
            c.mark[x] = (g, true);
            let node = &c.nodes[x];
            if builds(node) {
                found.push(NodeId::new(x));
            }
            for (i, rc) in node.rules.iter().enumerate() {
                if combinator(&rc.rule) {
                    combs.push((x, i));
                }
            }
            conj_targets(node, &mut buf);
            let next: Vec<usize> = buf
                .iter()
                .rev()
                .filter_map(|t| match c.alias.get(t.index()) {
                    Some(&a) if a != NONE => Some(a as usize),
                    _ => None,
                })
                .collect();
            stack.push((x, next));
        };
        enter(self, r, &mut stack);
        while let Some((x, next)) = stack.last_mut() {
            let x = *x;
            match next.pop() {
                Some(t) => {
                    self.spend(1)?;
                    match self.mark[t] {
                        (gen, true) if gen == g => {
                            return Err(BuildError::Internal(format!(
                                "ciclo de $ref/allOf en el nodo {t} (I14)"
                            )))
                        }
                        (gen, false) if gen == g => {}
                        _ => enter(self, t, &mut stack),
                    }
                }
                None => {
                    self.mark[x].1 = false;
                    stack.pop();
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        combs.sort_unstable();
        combs.dedup();
        self.spend(found.len() + combs.len())?;
        let raw = Rc::new(Raw {
            nodes: found,
            combs,
        });
        self.raws[r] = Some(Rc::clone(&raw));
        Ok(raw)
    }

    /// La unión de varios conjuntos (los componentes de un hijo del
    /// producto, F4-U2), cada uno con el destino en el padre del componente
    /// que lo aporta (F4.5a): 0 (el destino base del padre) o una salida
    /// del padre. Las salidas del padre distintas pasan a ser las salidas
    /// 1..=n del hijo, cuyo mapa (el destino en el padre de cada una) se
    /// devuelve; las de las ramas de cada parte se renumeran detrás. Sin
    /// salidas, el mapa es [`OUT_IDENTITY`] y el resultado es la unión de
    /// antes.
    pub(crate) fn union(&mut self, parts: &[(SetId, u32)]) -> Result<(SetId, u32), BuildError> {
        let mut distintos: Vec<(SetId, u32)> =
            parts.iter().copied().filter(|&(i, _)| i != ANY).collect();
        distintos.sort_unstable();
        distintos.dedup();
        let simple = distintos
            .iter()
            .all(|&(i, t)| t == 0 && self.sets.meta(i).n_slots == 0);
        if simple {
            if let [(solo, _)] = distintos.as_slice() {
                return Ok((*solo, OUT_IDENTITY));
            }
            let mut v: Vec<NodeId> = Vec::new();
            for &(i, _) in &distintos {
                v.extend_from_slice(self.sets.get(i));
            }
            self.spend(v.len())?;
            v.sort_unstable();
            v.dedup();
            return Ok((self.sets.intern(v, Meta::default()), OUT_IDENTITY));
        }
        // Salidas del hijo que van al padre, por orden de aparición.
        // E3B-1: el índice de cada destino en un mapa; `contains` y
        // `position` sobre `targets` eran O(partes²).
        let mut targets: Vec<u32> = Vec::new();
        let mut indice: HashMap<u32, u32> = HashMap::new();
        for &(_, t) in &distintos {
            if t != 0 && !indice.contains_key(&t) {
                indice.insert(t, targets.len() as u32);
                targets.push(t);
            }
        }
        self.spend(distintos.len())?;
        let mut next = targets.len() as u32;
        let mut comps: Vec<NodeId> = Vec::new();
        let mut gates: Vec<GateSpec> = Vec::new();
        // E3B-1: la búsqueda de repetidas era lineal en `gates` (O(g²) por
        // conjunto, con g que crece de forma combinatoria en cadenas de
        // `anyOf`); con el conjunto de claves es O(g), que es lo que cobra
        // `spend`.
        let mut vistas: HashSet<(NodeId, usize, Option<String>, u32)> = HashSet::new();
        for &(p, t) in &distintos {
            let base = match indice.get(&t) {
                Some(&j) if t != 0 => j + 1,
                _ => 0,
            };
            let off = next;
            let meta = self.sets.meta(p).clone();
            next += meta.n_slots;
            let remap = |k: u32| if k == 0 { base } else { off + k };
            let ids: Vec<NodeId> = self.sets.get(p).to_vec();
            self.spend(ids.len() + meta.gates.len())?;
            for id in ids {
                let (x, k) = (self.real(id), self.tgt(id));
                let v = self.vid(x.index(), remap(k));
                comps.push(v);
            }
            for g in meta.gates.iter() {
                let target = remap(g.target);
                // S-3: el mismo combinador hacia el mismo destino cuenta una
                // vez aunque llegue por dos partes (sus ramas repetidas
                // escriben salidas que nadie lee).
                if !vistas.insert((g.node, g.rule, g.key.clone(), target)) {
                    continue;
                }
                // E3B-1: copiar la puerta cuesta sus entradas, que no
                // cobraba nadie.
                self.spend(g.inputs.len())?;
                let mut g = g.clone();
                g.target = target;
                for k in &mut g.inputs {
                    *k = remap(*k);
                }
                gates.push(g);
            }
        }
        comps.sort_unstable();
        comps.dedup();
        let id = self.sets.intern(
            comps,
            Meta {
                n_slots: next,
                gates: Rc::from(gates),
            },
        );
        let outs = if next == 0 {
            OUT_IDENTITY
        } else {
            self.outmap(targets)
        };
        Ok((id, outs))
    }
}

/// Ordena las puertas para que cada una vaya después de las que escriben
/// sus entradas (Kahn); un ciclo es `Internal` (la Fase 3 rechaza los
/// ciclos de combinadores que no consumen entrada).
fn topo(gates: Vec<GateSpec>) -> Result<Vec<GateSpec>, BuildError> {
    let n = gates.len();
    let mut indeg = vec![0usize; n];
    let mut after: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut writers: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, g) in gates.iter().enumerate() {
        if g.target != 0 {
            writers.entry(g.target).or_default().push(i);
        }
    }
    for (j, g) in gates.iter().enumerate() {
        for k in &g.inputs {
            for &i in writers.get(k).map_or(&[][..], Vec::as_slice) {
                after[i].push(j);
                indeg[j] += 1;
            }
        }
    }
    let mut ready: Vec<usize> = (0..n).rev().filter(|&i| indeg[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while let Some(i) = ready.pop() {
        order.push(i);
        for &j in &after[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                ready.push(j);
            }
        }
    }
    if order.len() != n {
        return Err(BuildError::Internal(
            "ciclo de combinadores sin consumir entrada (I14)".to_owned(),
        ));
    }
    let mut gates: Vec<Option<GateSpec>> = gates.into_iter().map(Some).collect();
    Ok(order.into_iter().filter_map(|i| gates[i].take()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{Constraint, JsonType, Origin, Pos, Sort, Span, Step, TypeSet};

    const SPAN: Span = Span {
        start: Pos::START,
        end: 0,
    };

    fn nodo(rules: Vec<Rule>) -> Node {
        Node {
            sort: Sort::Json,
            rules: rules
                .into_iter()
                .map(|rule| Constraint { rule, span: SPAN })
                .collect(),
            origin: Origin {
                span: SPAN,
                parent: None,
                step: Step::Root,
            },
        }
    }

    /// F4.1a-CR-S4: un ciclo de alias (que I14 excluye, así que solo puede
    /// llegar con una IR sin verificar) da `Internal`, no un cuelgue.
    #[test]
    fn ciclo_de_alias_es_internal() {
        let nodes = vec![
            nodo(vec![Rule::Ref(NodeId::new(1))]),
            nodo(vec![Rule::Ref(NodeId::new(0))]),
        ];
        assert!(matches!(
            resolve_aliases(&nodes),
            Err(BuildError::Internal(_))
        ));
    }

    /// Un ciclo con reglas propias da `Internal` en el DFS del cierre.
    #[test]
    fn ciclo_en_el_cierre_es_internal() {
        let int = Rule::Type(TypeSet::of(&[JsonType::Integer]));
        let nodes = vec![
            nodo(vec![Rule::AllOf(vec![NodeId::new(1), NodeId::new(2)])]),
            nodo(vec![int.clone(), Rule::Ref(NodeId::new(2))]),
            nodo(vec![int, Rule::Ref(NodeId::new(1))]),
        ];
        let mut c = Closures::new(&nodes).unwrap();
        assert!(matches!(c.of(NodeId::new(0)), Err(BuildError::Internal(_))));
    }

    /// Componentes = `NodeId` distintos (S-3): un diamante da un conjunto.
    #[test]
    fn diamante_da_nodos_distintos() {
        let int = Rule::Type(TypeSet::of(&[JsonType::Integer]));
        let nodes = vec![
            nodo(vec![Rule::AllOf(vec![NodeId::new(1), NodeId::new(2)])]),
            nodo(vec![Rule::MinLength(1), Rule::Ref(NodeId::new(3))]),
            nodo(vec![Rule::MaxLength(9), Rule::Ref(NodeId::new(3))]),
            nodo(vec![int]),
        ];
        let mut c = Closures::new(&nodes).unwrap();
        let id = c.of(NodeId::new(0)).unwrap();
        assert_eq!(
            c.sets.get(id),
            &[NodeId::new(1), NodeId::new(2), NodeId::new(3)]
        );
    }
}
