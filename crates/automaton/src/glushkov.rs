//! Autómata de Glushkov con contadores de un modelo de contenido XSD
//! (F4.1c, 5.7-P1 de `docs/fase3/pendientes.md` §38; teoría en
//! `docs/fase4/automata.md` §12).
//!
//! Los estados son el inicial y las **posiciones** (las partículas
//! elemento vivas); no se determiniza: el frontal comprobó la UPA sobre la
//! abstracción de D25 (toda partícula con `maxOccurs` > 1 es `+` o `*`), así
//! que de cada posición sale a lo sumo una transición por nombre, y la regla
//! de las vueltas con contador de la enmienda de D25, así que cada
//! transición (x, y) fija **una** actualización de contadores (determinismo
//! fuerte). Las dos cosas se vuelven a comprobar aquí como defensa en
//! profundidad: si fallan, `Internal`.
//!
//! Contadores (D16): uno por partícula con `minOccurs` ≥ 2 o `maxOccurs`
//! acotado > 1, con la vuelta actual (1 al entrar en la partícula). Cada
//! transición x → y nace en una partícula Q: un paso de secuencia (x en el
//! hijo i de Q, y en el hijo j > i, los de en medio anulables) o una
//! **vuelta** de Q (`maxOccurs` > 1, x en last(Q), y en first(Q)). Sale de
//! las partículas que contienen x estrictamente dentro de Q (guarda: su
//! mínimo, salvo si su cuerpo es anulable), vuelve a empezar Q si es una
//! vuelta (guarda: no pasar del máximo; suma 1) y entra en las que contienen
//! y estrictamente dentro de Q (reinicio a 1). Las guardas solo suman peso
//! (F4-B13): el autómata sigue con la misma transición.
//!
//! Todo es iterativo (D7): las partículas están en preorden (los hijos
//! tienen índice mayor que su padre), así que las pasadas hacia arriba y
//! hacia abajo son bucles sobre los índices.

use ir::{ContentModel, NodeId, Term};
use runtime_rs::tables::{CountOp, CounterOp};

use crate::BuildError;

/// Una guarda de contador de una transición o de un cierre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Guard {
    /// Contador ≥ cota (el mínimo al salir de la partícula).
    Min(u32, u64),
    /// Contador ≤ cota (el máximo − 1 antes de volver a empezar).
    Max(u32, u64),
}

/// Una transición del autómata de posiciones con sus contadores.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edge {
    /// Posición destino.
    pub(crate) to: u32,
    pub(crate) guards: Vec<Guard>,
    pub(crate) ops: Vec<CounterOp>,
}

/// Qué pasa al cerrar el elemento en un estado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Close {
    /// El estado no es final: el contenido está incompleto.
    No,
    /// Final, con las guardas de los mínimos de las partículas de las que
    /// sale.
    Guards(Vec<Guard>),
}

/// Una posición: el nombre local y el tipo de su elemento.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Position<'a> {
    pub(crate) name: &'a str,
    pub(crate) ty: NodeId,
}

/// El autómata de un modelo: el estado 0 es el inicial y el estado p + 1 es
/// la posición p.
#[derive(Debug)]
pub(crate) struct Glushkov<'a> {
    pub(crate) positions: Vec<Position<'a>>,
    /// Número de contadores (partículas con contador, en preorden).
    pub(crate) counters: u32,
    /// Transiciones de cada estado (0 = inicial), en orden de destino.
    pub(crate) edges: Vec<Vec<Edge>>,
    pub(crate) close: Vec<Close>,
    /// Pares de follow contados (con las derivaciones repetidas): lo que se
    /// materializó; el constructor los acumula entre modelos (F4D-M2).
    pub(crate) pairs: usize,
}

const NONE: usize = usize::MAX;

/// `what` del `limit` de los pares de follow (el constructor lo traduce al
/// presupuesto de F4-U6).
pub(crate) const PAIRS_WHAT: &str = "transiciones del autómata de Glushkov como mucho";

/// Construye el autómata de `model`. `work` es el presupuesto de pasos (los
/// tamaños de los conjuntos first/last y de los caminos recorridos) y
/// `max_pairs` la cota de transiciones (pares de follow) que **aún** admite
/// el presupuesto de δ (el constructor le resta los de los modelos ya
/// construidos, F4D-M2): se comprueban **antes** de materializar los pares.
/// `ctx` es la ubicación para los errores.
pub(crate) fn build<'a>(
    model: &'a ContentModel,
    work: &mut Work,
    max_pairs: usize,
    ctx: &dyn Fn() -> String,
) -> Result<Glushkov<'a>, BuildError> {
    let ps = &model.particles;
    let n = ps.len();
    let internal =
        |m: &str| BuildError::Internal(format!("{m} en el modelo de contenido de {}", ctx()));
    if n == 0 {
        return Err(internal("modelo sin partícula raíz"));
    }
    // Padres y tamaños de subárbol (I11: preorden, hijos posteriores).
    let mut parent = vec![NONE; n];
    for (p, part) in ps.iter().enumerate() {
        if let Term::Sequence(c) | Term::Choice(c) | Term::All(c) = &part.term {
            for &k in c {
                if k <= p || k >= n || parent[k] != NONE {
                    return Err(internal("árbol de partículas inválido (I11)"));
                }
                parent[k] = p;
            }
            if p != 0 && matches!(part.term, Term::All(_)) {
                return Err(internal("xs:all anidado (I12)"));
            }
        }
    }
    let mut size = vec![1usize; n];
    for p in (1..n).rev() {
        if parent[p] == NONE {
            return Err(internal("partícula sin padre (I11)"));
        }
        size[parent[p]] += size[p];
    }
    let inside = |q: usize, p: usize| p > q && p < q + size[q];
    // Vivas: ni la partícula ni un antepasado tienen `maxOccurs="0"` (esas
    // solo admiten ε y no tienen posiciones).
    let mut live = vec![true; n];
    for p in 0..n {
        live[p] = ps[p].occurs.max != Some(0) && (p == 0 || live[parent[p]]);
    }
    // Anulabilidad del cuerpo y de la partícula, de abajo arriba.
    let mut body = vec![false; n];
    let mut nullable = vec![true; n];
    for p in (0..n).rev() {
        body[p] = match &ps[p].term {
            Term::Element { .. } => false,
            Term::Sequence(c) | Term::All(c) => c.iter().all(|&k| nullable[k]),
            Term::Choice(c) => c.iter().any(|&k| nullable[k]),
        };
        nullable[p] = !live[p] || ps[p].occurs.min == 0 || body[p];
    }
    // Contadores (D16) y el contador más cercano por encima (o la propia).
    let mut counter = vec![u32::MAX; n];
    let mut counters = 0u32;
    let mut cpar = vec![NONE; n];
    for p in 0..n {
        let o = ps[p].occurs;
        if live[p] && (o.min >= 2 || o.max.is_some_and(|m| m > 1)) {
            counter[p] = counters;
            counters += 1;
        }
        cpar[p] = if counter[p] != u32::MAX {
            p
        } else if p == 0 {
            NONE
        } else {
            cpar[parent[p]]
        };
    }
    // Posiciones.
    let mut positions = Vec::new();
    let mut pos_of = vec![NONE; n];
    let mut leaf = Vec::new();
    for (p, part) in ps.iter().enumerate() {
        if let Term::Element { name, ty } = &part.term {
            if live[p] {
                pos_of[p] = positions.len();
                positions.push(Position { name, ty: *ty });
                leaf.push(p);
            }
        }
    }
    // first y last, de abajo arriba; su tamaño total se descuenta del
    // presupuesto de pasos.
    let mut first: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut last: Vec<Vec<u32>> = vec![Vec::new(); n];
    for p in (0..n).rev() {
        if !live[p] {
            continue;
        }
        let (f, l) = match &ps[p].term {
            Term::Element { .. } => (vec![pos_of[p] as u32], vec![pos_of[p] as u32]),
            Term::Choice(c) | Term::All(c) => (
                c.iter().flat_map(|&k| first[k].iter().copied()).collect(),
                c.iter().flat_map(|&k| last[k].iter().copied()).collect(),
            ),
            Term::Sequence(c) => {
                let mut f = Vec::new();
                for &k in c {
                    f.extend_from_slice(&first[k]);
                    if !nullable[k] {
                        break;
                    }
                }
                let mut l = Vec::new();
                for &k in c.iter().rev() {
                    l.extend_from_slice(&last[k]);
                    if !nullable[k] {
                        break;
                    }
                }
                (f, l)
            }
        };
        work.spend(f.len() + l.len() + 1, ctx)?;
        first[p] = f;
        last[p] = l;
    }

    // Pares de follow: una primera pasada solo aritmética los cuenta (y los
    // bloques que darán, saltando los hijos sin first/last y parando en el
    // primer hijo no anulable); el tope y el trabajo se comprueban antes de
    // materializar nada (I1 / F4C-S2 de la ronda 2).
    let is_loop = |p: usize| live[p] && ps[p].occurs.max != Some(1);
    let blocks_of = |q: usize, out: &mut dyn FnMut(usize, usize, usize, bool)| {
        if !live[q] {
            return;
        }
        if let Term::Sequence(c) = &ps[q].term {
            for i in 0..c.len() {
                if last[c[i]].is_empty() {
                    continue;
                }
                // Cada paso del bucle se entrega (también los hijos sin
                // first, que no dan pares) para que se cuente como trabajo.
                for &cj in &c[i + 1..] {
                    out(q, c[i], cj, false);
                    if !nullable[cj] {
                        break;
                    }
                }
            }
        }
        if is_loop(q) && !first[q].is_empty() {
            out(q, q, q, true);
        }
    };
    let mut pairs = 0usize;
    let mut n_blocks = 0usize;
    let mut steps = 0usize;
    for q in 0..n {
        steps = steps.saturating_add(1);
        blocks_of(q, &mut |_, from, to, _| {
            steps = steps.saturating_add(1);
            if !first[to].is_empty() {
                n_blocks = n_blocks.saturating_add(1);
                pairs = pairs.saturating_add(last[from].len().saturating_mul(first[to].len()));
            }
        });
    }
    work.spend(steps.saturating_add(n_blocks), ctx)?;
    if pairs > max_pairs {
        return Err(BuildError::Limit {
            what: PAIRS_WHAT,
            limit: max_pairs,
            found: pairs,
            location: ctx(),
        });
    }
    let mut blocks: Vec<(usize, usize, usize, bool)> = Vec::with_capacity(n_blocks);
    for q in 0..n {
        blocks_of(q, &mut |q, from, to, lp| {
            if !first[to].is_empty() {
                blocks.push((q, from, to, lp));
            }
        });
    }
    // (y, Q, vuelta) por posición x.
    let mut raw: Vec<Vec<(u32, usize, bool)>> = vec![Vec::new(); positions.len()];
    for &(q, from, to, lp) in &blocks {
        for &x in &last[from] {
            for &y in &first[to] {
                raw[x as usize].push((y, q, lp));
            }
        }
    }
    work.spend(pairs, ctx)?;
    // I6 = F4E-M1 (ronda extra): la UPA de abajo y los tries de follow(x)
    // del constructor recorren los nombres de cada destino por estado:
    // Σₓ Σ_{y ∈ follow(x)} |nombre(y)| bytes (más los del estado inicial),
    // cargados con una pasada aritmética **antes** de compararlos.
    let name = |y: u32| positions[y as usize].name.len();
    let mut bytes = first[0]
        .iter()
        .fold(0usize, |n, &y| n.saturating_add(name(y)));
    for list in &raw {
        for &(y, _, _) in list {
            bytes = bytes.saturating_add(name(y));
        }
    }
    work.spend(bytes, ctx)?;

    // Los contadores que contienen la hoja `l` estrictamente dentro de `q`
    // (`q` = NONE: todos), de abajo arriba.
    let chain = |l: usize, q: usize, work: &mut Work| -> Result<Vec<usize>, BuildError> {
        let mut out = Vec::new();
        let mut p = cpar[l];
        while p != NONE && (q == NONE || inside(q, p)) {
            out.push(p);
            p = if p == 0 { NONE } else { cpar[parent[p]] };
        }
        work.spend(out.len(), ctx)?;
        Ok(out)
    };
    let exit_guard = |a: usize| -> Option<Guard> {
        let o = ps[a].occurs;
        (o.min >= 2 && !body[a]).then_some(Guard::Min(counter[a], o.min))
    };
    let effect =
        |x: usize, y: usize, q: usize, lp: bool, work: &mut Work| -> Result<Edge, BuildError> {
            let mut guards = Vec::new();
            let mut ops = Vec::new();
            for a in chain(leaf[x], q, work)? {
                guards.extend(exit_guard(a));
            }
            if lp && counter[q] != u32::MAX {
                if let Some(m) = ps[q].occurs.max {
                    guards.push(Guard::Max(counter[q], m - 1));
                }
                ops.push(CounterOp {
                    counter: counter[q],
                    op: CountOp::Add,
                });
            }
            for b in chain(leaf[y], q, work)?.into_iter().rev() {
                ops.push(CounterOp {
                    counter: counter[b],
                    op: CountOp::Restart,
                });
            }
            guards.sort_unstable();
            Ok(Edge {
                to: y as u32,
                guards,
                ops,
            })
        };

    let mut edges: Vec<Vec<Edge>> = Vec::with_capacity(positions.len() + 1);
    // Desde el inicial: se entra en todas las partículas que contienen y.
    let mut e0 = Vec::new();
    let mut f0 = first[0].clone();
    f0.sort_unstable();
    f0.dedup();
    for &y in &f0 {
        let ops = chain(leaf[y as usize], NONE, work)?
            .into_iter()
            .rev()
            .map(|b| CounterOp {
                counter: counter[b],
                op: CountOp::Restart,
            })
            .collect();
        e0.push(Edge {
            to: y,
            guards: Vec::new(),
            ops,
        });
    }
    edges.push(e0);
    for (x, mut list) in raw.into_iter().enumerate() {
        list.sort_unstable_by_key(|&(y, q, lp)| (y, q, lp));
        let mut out: Vec<Edge> = Vec::new();
        let mut k = 0;
        while k < list.len() {
            let y = list[k].0;
            let e = effect(x, y as usize, list[k].1, list[k].2, work)?;
            let mut j = k + 1;
            while j < list.len() && list[j].0 == y {
                // Otra derivación del mismo par: con determinismo fuerte, la
                // misma actualización (enmienda de D25).
                if effect(x, y as usize, list[j].1, list[j].2, work)? != e {
                    return Err(internal(&format!(
                        "la transición de «{}» a «{}» cambia los contadores de dos formas (vuelta ambigua, enmienda de D25)",
                        positions[x].name, positions[y as usize].name
                    )));
                }
                j += 1;
            }
            out.push(e);
            k = j;
        }
        edges.push(out);
    }
    // UPA (D25): de cada estado, a lo sumo un destino por nombre.
    for (s, list) in edges.iter().enumerate() {
        let mut names: Vec<(&str, u32)> = list
            .iter()
            .map(|e| (positions[e.to as usize].name, e.to))
            .collect();
        names.sort_unstable();
        if let Some(w) = names.windows(2).find(|w| w[0].0 == w[1].0) {
            let desde = if s == 0 {
                "el inicio".to_owned()
            } else {
                format!("«{}»", positions[s - 1].name)
            };
            return Err(internal(&format!(
                "dos partículas «{}» compiten tras {desde} (UPA, D25)",
                w[0].0
            )));
        }
    }
    // Cierres.
    let mut close = Vec::with_capacity(positions.len() + 1);
    close.push(if nullable[0] {
        Close::Guards(Vec::new())
    } else {
        Close::No
    });
    let mut fin = vec![false; positions.len()];
    for &x in &last[0] {
        fin[x as usize] = true;
    }
    for (x, &f) in fin.iter().enumerate() {
        close.push(if f {
            let mut g: Vec<Guard> = chain(leaf[x], NONE, work)?
                .into_iter()
                .filter_map(exit_guard)
                .collect();
            g.sort_unstable();
            Close::Guards(g)
        } else {
            Close::No
        });
    }
    Ok(Glushkov {
        positions,
        counters,
        edges,
        close,
        pairs,
    })
}

/// Presupuesto de pasos de la construcción de los autómatas de Glushkov
/// (el mismo tope que el de los cierres conjuntivos, `MAX_CLOSURE_WORK`).
#[derive(Debug)]
pub(crate) struct Work {
    pub(crate) used: usize,
    pub(crate) limit: usize,
}

impl Work {
    pub(crate) fn spend(&mut self, n: usize, ctx: &dyn Fn() -> String) -> Result<(), BuildError> {
        self.used = self.used.saturating_add(n);
        if self.used > self.limit {
            return Err(BuildError::Limit {
                what: "pasos de la construcción XML (autómatas de Glushkov con sus nombres, intervalos IEEE y literales de los textos)",
                limit: self.limit,
                found: self.used,
                location: ctx(),
            });
        }
        Ok(())
    }
}
