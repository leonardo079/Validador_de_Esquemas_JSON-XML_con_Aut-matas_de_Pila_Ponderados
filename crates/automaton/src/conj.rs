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

use std::collections::HashMap;

use ir::{Node, NodeId, Rule};

use crate::BuildError;

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

/// Los conjuntos canónicos, cada uno una vez.
#[derive(Debug)]
pub(crate) struct Sets {
    list: Vec<Vec<NodeId>>,
    ids: HashMap<Vec<NodeId>, SetId>,
}

impl Sets {
    fn new() -> Sets {
        Sets {
            list: vec![Vec::new()],
            ids: HashMap::from([(Vec::new(), ANY)]),
        }
    }

    /// Id de `v` (ya ordenado y sin repetir).
    fn intern(&mut self, v: Vec<NodeId>) -> SetId {
        if let Some(&id) = self.ids.get(&v) {
            return id;
        }
        let id = self.list.len() as SetId;
        self.list.push(v.clone());
        self.ids.insert(v, id);
        id
    }

    pub(crate) fn get(&self, id: SetId) -> &[NodeId] {
        &self.list[id as usize]
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

    /// C(v): DFS iterativo (pila explícita, D7) sobre las aristas
    /// conjuntivas con los alias resueltos; un nodo ya en la pila es un
    /// ciclo (`Internal`).
    pub(crate) fn of(&mut self, v: NodeId) -> Result<SetId, BuildError> {
        let r = match self.alias.get(v.index()) {
            Some(&r) if r == NONE => return Ok(ANY),
            Some(&r) => r as usize,
            None => {
                return Err(BuildError::Internal(format!(
                    "nodo {} fuera de la IR",
                    v.index()
                )))
            }
        };
        if let Some(id) = self.memo[r] {
            return Ok(id);
        }
        self.generation += 1;
        let g = self.generation;
        let mut found = Vec::new();
        let mut buf = Vec::new();
        // (nodo, destinos pendientes).
        let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut enter = |c: &mut Self, x: usize, stack: &mut Vec<(usize, Vec<usize>)>| {
            c.mark[x] = (g, true);
            let node = &c.nodes[x];
            if contributes(node) {
                found.push(NodeId::new(x));
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
        self.spend(found.len())?;
        let id = self.sets.intern(found);
        self.memo[r] = Some(id);
        Ok(id)
    }

    /// La unión de varios conjuntos (los componentes de un hijo del
    /// producto, F4-U2).
    pub(crate) fn union(&mut self, ids: &[SetId]) -> Result<SetId, BuildError> {
        let mut v: Vec<NodeId> = Vec::new();
        let mut distintos: Vec<SetId> = ids.iter().copied().filter(|&i| i != ANY).collect();
        distintos.sort_unstable();
        distintos.dedup();
        if let [solo] = distintos.as_slice() {
            return Ok(*solo);
        }
        for &i in &distintos {
            v.extend_from_slice(self.sets.get(i));
        }
        self.spend(v.len())?;
        v.sort_unstable();
        v.dedup();
        Ok(self.sets.intern(v))
    }
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
