//! Detección de ciclos no productivos (Fase 3).
//!
//! Sobre la tabla de símbolos se construye el grafo G₀ de las aristas que
//! **no consumen entrada**: las de `$ref` y las de los aplicadores que
//! aplican su subesquema a la **misma instancia** (`allOf`, `anyOf`,
//! `oneOf`, `not`, `dependentSchemas`). Las de `properties`,
//! `additionalProperties`, `items` y `prefixItems` aplican el subesquema a
//! una instancia **hija**, que empieza después de al menos un token del
//! padre (`{` y la clave, o `[`), así que consumen entrada y no entran en G₀.
//! Las entradas de `$defs`/`definitions` no se aplican a nada por sí mismas:
//! solo se alcanzan por un `$ref`, así que tampoco son aristas.
//!
//! Un ciclo de G₀ es un ciclo **no productivo**: validar un valor con
//! cualquier esquema del ciclo exige volver a validarlo, el mismo valor, con
//! el mismo esquema, sin haber leído nada (`docs/fase3/ciclos.md`). Se
//! rechaza. Las componentes fuertemente conexas se calculan con el algoritmo
//! de Tarjan **iterativo** (pila explícita, D7), en O(V + E).
//!
//! Cada componente con un ciclo da **un** error, en el `$ref` del ciclo que
//! aparece primero en el texto, con un camino concreto del ciclo que pasa
//! por ese `$ref`.

use std::collections::VecDeque;

use schema_parser::Kw;

use crate::error::{ErrorKind, SemanticError};
use crate::symbols::{SchemaId, Step, SymbolTable};

/// A qué instancia aplica una palabra clave sus subesquemas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Application {
    /// A la misma instancia que el esquema que la contiene: la arista **no
    /// consume** entrada (`$ref`, `allOf`, `anyOf`, `oneOf`, `not`,
    /// `dependentSchemas`).
    SameInstance,
    /// A una instancia hija (un miembro o un elemento): la arista **consume**
    /// al menos un token (`properties`, `additionalProperties`, `items`,
    /// `prefixItems`).
    ChildInstance,
    /// No aplica subesquemas: no los tiene, o son definiciones que solo se
    /// alcanzan por `$ref` (`$defs`, `definitions`).
    None,
}

/// Clasificación de cada palabra clave del subconjunto (tabla en
/// `docs/fase3/ciclos.md` §2).
pub fn application(kw: Kw) -> Application {
    match kw {
        Kw::Ref | Kw::AllOf | Kw::AnyOf | Kw::OneOf | Kw::Not | Kw::DependentSchemas => {
            Application::SameInstance
        }
        Kw::Properties | Kw::AdditionalProperties | Kw::Items | Kw::PrefixItems => {
            Application::ChildInstance
        }
        _ => Application::None,
    }
}

/// Paso del padre al hijo: palabra clave.
fn step_keyword<'a>(step: Step<'a>) -> &'a str {
    match step {
        Step::Keyword(k) | Step::Entry(k, _) | Step::Index(k, _) => k,
    }
}

/// Grafo dirigido en formato CSR: las aristas de `v` son
/// `targets[offsets[v]..offsets[v + 1]]`, con su etiqueta en `labels`.
#[derive(Debug)]
pub(crate) struct Graph<'a> {
    offsets: Vec<usize>,
    targets: Vec<usize>,
    labels: Vec<&'a str>,
}

impl<'a> Graph<'a> {
    /// Construye el grafo con `n` nodos a partir de una lista de aristas
    /// (origen, destino, etiqueta), ordenándolas por origen de forma estable
    /// (conteo): O(V + E).
    pub(crate) fn new(n: usize, edges: &[(usize, usize, &'a str)]) -> Self {
        let mut offsets = vec![0; n + 1];
        for &(u, _, _) in edges {
            offsets[u + 1] += 1;
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut fill = offsets.clone();
        let mut targets = vec![0; edges.len()];
        let mut labels = vec![""; edges.len()];
        for &(u, v, l) in edges {
            targets[fill[u]] = v;
            labels[fill[u]] = l;
            fill[u] += 1;
        }
        Graph {
            offsets,
            targets,
            labels,
        }
    }

    fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    fn edges(&self, v: usize) -> impl Iterator<Item = (usize, &'a str)> + '_ {
        let r = self.offsets[v]..self.offsets[v + 1];
        self.targets[r.clone()]
            .iter()
            .copied()
            .zip(self.labels[r].iter().copied())
    }

    /// Componentes fuertemente conexas (Tarjan, iterativo). Devuelve la
    /// componente de cada nodo y si esa componente contiene un ciclo (más de
    /// un nodo, o un nodo con autoarista).
    pub(crate) fn cyclic_components(&self) -> (Vec<usize>, Vec<bool>) {
        const NONE: usize = usize::MAX;
        let n = self.len();
        let mut index = vec![NONE; n];
        let mut low = vec![0; n];
        let mut on_stack = vec![false; n];
        let mut comp = vec![NONE; n];
        let mut cyclic: Vec<bool> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        // Pila de llamadas explícita: (nodo, siguiente arista por mirar).
        let mut call: Vec<(usize, usize)> = Vec::new();
        let mut counter = 0;
        for s in 0..n {
            if index[s] != NONE {
                continue;
            }
            index[s] = counter;
            low[s] = counter;
            counter += 1;
            stack.push(s);
            on_stack[s] = true;
            call.push((s, self.offsets[s]));
            while let Some(&mut (v, ref mut e)) = call.last_mut() {
                if *e < self.offsets[v + 1] {
                    let w = self.targets[*e];
                    *e += 1;
                    if index[w] == NONE {
                        index[w] = counter;
                        low[w] = counter;
                        counter += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        call.push((w, self.offsets[w]));
                    } else if on_stack[w] {
                        low[v] = low[v].min(index[w]);
                    }
                    continue;
                }
                call.pop();
                if let Some(&(u, _)) = call.last() {
                    low[u] = low[u].min(low[v]);
                }
                if low[v] == index[v] {
                    let c = cyclic.len();
                    let mut size = 0;
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        comp[w] = c;
                        size += 1;
                        if w == v {
                            break;
                        }
                    }
                    let self_loop = self.edges(v).any(|(w, _)| w == v);
                    cyclic.push(size > 1 || self_loop);
                }
            }
        }
        (comp, cyclic)
    }

    /// Camino más corto de `from` a `to` sin salir de la componente `c`
    /// (BFS). `seen` es un marcador compartido entre llamadas: como las
    /// componentes son disjuntas, cada nodo se visita en a lo sumo una
    /// búsqueda y el total es O(V + E). Devuelve las aristas (etiqueta,
    /// destino); vacío si `from == to`.
    fn path_in(
        &self,
        from: usize,
        to: usize,
        c: usize,
        comp: &[usize],
        seen: &mut [bool],
        pred: &mut [(usize, &'a str)],
    ) -> Vec<(&'a str, usize)> {
        if from == to {
            return Vec::new();
        }
        let mut queue = VecDeque::from([from]);
        seen[from] = true;
        while let Some(v) = queue.pop_front() {
            if v == to {
                break;
            }
            for (w, l) in self.edges(v) {
                if comp[w] == c && !seen[w] {
                    seen[w] = true;
                    pred[w] = (v, l);
                    queue.push_back(w);
                }
            }
        }
        // Las dos salidas con camino vacío solo se dan si se rompe un
        // invariante; en release dan un error incoherente pero no ocultan
        // ninguno (la componente sigue informándose), en debug avisan.
        // No ocurre si `from` y `to` están en la misma componente.
        debug_assert!(seen[to], "path_in: destino {to} fuera de la componente {c}");
        if !seen[to] {
            return Vec::new();
        }
        // Un camino simple tiene menos de n aristas. Con el invariante de
        // arriba (`seen` y `pred` de un nodo solo los escribe la búsqueda de
        // su componente) la cadena de `pred` llega a `from`; el tope evita
        // un bucle sin fin si un cambio futuro lo rompiera.
        let mut path = Vec::new();
        let mut cur = to;
        while cur != from {
            debug_assert!(
                path.len() < self.len(),
                "path_in: la cadena de pred no llega al origen {from}"
            );
            if path.len() >= self.len() {
                return Vec::new();
            }
            let (p, l) = pred[cur];
            path.push((l, cur));
            cur = p;
        }
        path.reverse();
        path
    }
}

/// Aristas mostradas como mucho en el mensaje de un ciclo.
const SHOWN_EDGES: usize = 8;

/// Bytes como mucho de cada fragmento guardado en el error de un ciclo.
pub const FRAGMENT_BYTES: usize = 80;

/// Rechaza los ciclos no productivos. Un error por componente con ciclo, en
/// la posición de su primer `$ref` en el texto, ordenados por posición. Se
/// informan todos (sin tope): ver el costo.
///
/// Costo: O(V + E) para el grafo, Tarjan y los caminos, más
/// O(m · 9 · (h + `FRAGMENT_BYTES`)) para construir los m errores (h, la
/// profundidad máxima de un subesquema) y O(m log m) para ordenarlos. Cada
/// error guarda a lo sumo 9 fragmentos ya recortados a `FRAGMENT_BYTES` con
/// [`SymbolTable::fragment_clipped`], que sube los h padres en cada una de
/// las 9 llamadas y no construye el puntero entero: la salida es O(m), y
/// m ≤ n/9 porque cada componente tiene un `$ref` propio de al menos 9
/// bytes (`docs/fase3/ciclos.md` §5).
pub fn check_cycles(table: &SymbolTable<'_>) -> Result<(), Vec<SemanticError>> {
    let n = table.len();
    let mut edges: Vec<(usize, usize, &str)> = Vec::new();
    for id in table.ids() {
        if let Some((parent, step)) = table.node(id).parent {
            let k = step_keyword(step);
            if application(Kw::from_name(k)) == Application::SameInstance {
                edges.push((parent.index(), id.index(), k));
            }
        }
    }
    for r in table.refs() {
        edges.push((r.from.index(), r.target.index(), "$ref"));
    }
    let g = Graph::new(n, &edges);
    let (comp, cyclic) = g.cyclic_components();
    if !cyclic.iter().any(|&c| c) {
        return Ok(());
    }

    // Toda arista estructural va de un padre a un hijo, con id mayor; un
    // ciclo no puede estar hecho solo de ellas, así que cada componente con
    // ciclo tiene al menos un `$ref` interno. `refs()` está en orden de texto:
    // el primero de cada componente es su representante.
    let mut done = vec![false; cyclic.len()];
    let mut seen = vec![false; n];
    let mut pred: Vec<(usize, &str)> = vec![(0, ""); n];
    let mut errors = Vec::new();
    for r in table.refs() {
        let (f, t) = (r.from.index(), r.target.index());
        let c = comp[f];
        if c != comp[t] || !cyclic[c] || done[c] {
            continue;
        }
        done[c] = true;
        // f —$ref→ t ⇝ f
        let back = g.path_in(t, f, c, &comp, &mut seen, &mut pred);
        let total = 1 + back.len();
        let mut all: Vec<(&str, usize)> = Vec::with_capacity(SHOWN_EDGES);
        all.push(("$ref", t));
        all.extend(back.iter().take(SHOWN_EDGES - 2).copied());
        if total > SHOWN_EDGES - 1 {
            if let Some(&last) = back.last() {
                all.push(last);
            }
        } else {
            all.extend(back.iter().skip(SHOWN_EDGES - 2).copied());
        }
        // Recortados al construirlos, nunca enteros: un nodo profundo con una
        // clave de longitud L en cada uno de m errores costaría Θ(m·L).
        let frag = |i: usize| {
            table.fragment_clipped(table.id(i).unwrap_or(SchemaId::ROOT), FRAGMENT_BYTES)
        };
        errors.push(SemanticError {
            kind: ErrorKind::NonProductiveCycle {
                start: frag(f),
                edges: all.iter().map(|&(l, v)| (l.to_owned(), frag(v))).collect(),
                total,
            },
            at: r.at,
        });
    }
    errors.sort_by_key(|e: &SemanticError| e.at.offset);
    Err(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn clasificacion_completa() {
        use schema_parser::Class;
        for name in Kw::names() {
            let kw = Kw::from_name(name);
            let tiene_esquemas = matches!(
                kw.class(),
                Class::Schema | Class::SchemaMap | Class::SchemaList
            );
            let a = application(kw);
            match name {
                "$defs" | "definitions" => assert_eq!(a, Application::None),
                "$ref" => assert_eq!(a, Application::SameInstance),
                _ if tiene_esquemas => assert_ne!(a, Application::None, "{name}"),
                _ => assert_eq!(a, Application::None, "{name}"),
            }
        }
    }

    #[test]
    fn tarjan_casos() {
        let g = Graph::new(4, &[(0, 1, ""), (1, 0, ""), (2, 2, ""), (2, 3, "")]);
        let (comp, cyc) = g.cyclic_components();
        assert_eq!(comp[0], comp[1]);
        assert!(cyc[comp[0]] && cyc[comp[2]] && !cyc[comp[3]]);
        let g = Graph::new(3, &[(0, 1, ""), (1, 2, "")]);
        assert!(g.cyclic_components().1.iter().all(|&c| !c));
        let g = Graph::new(0, &[]);
        assert!(g.cyclic_components().1.is_empty());
    }

    /// Nodo en un ciclo ⇔ se alcanza a sí mismo con al menos una arista,
    /// por clausura transitiva (Floyd–Warshall).
    fn en_ciclo_fuerza_bruta(n: usize, edges: &[(usize, usize, &str)]) -> Vec<bool> {
        let mut r = vec![vec![false; n]; n];
        for &(u, v, _) in edges {
            r[u][v] = true;
        }
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    if r[i][k] && r[k][j] {
                        r[i][j] = true;
                    }
                }
            }
        }
        (0..n).map(|i| r[i][i]).collect()
    }

    fn grafo() -> impl Strategy<Value = (usize, Vec<(usize, usize)>)> {
        (1usize..9).prop_flat_map(|n| (Just(n), prop::collection::vec((0..n, 0..n), 0..20)))
    }

    proptest! {
        /// Tarjan coincide con la clausura transitiva: un nodo está en una
        /// componente con ciclo si y solo si se alcanza a sí mismo; y dos
        /// nodos están en la misma componente si y solo si se alcanzan
        /// mutuamente.
        #[test]
        fn coincide_con_fuerza_bruta((n, e) in grafo()) {
            let edges: Vec<(usize, usize, &str)> = e.iter().map(|&(u, v)| (u, v, "")).collect();
            let g = Graph::new(n, &edges);
            let (comp, cyc) = g.cyclic_components();
            let esperado = en_ciclo_fuerza_bruta(n, &edges);
            for v in 0..n {
                prop_assert_eq!(cyc[comp[v]], esperado[v], "nodo {}", v);
            }
            // Misma componente ⇔ alcanzables mutuamente.
            let mut r = vec![vec![false; n]; n];
            for (i, row) in r.iter_mut().enumerate() { row[i] = true; }
            for &(u, v, _) in &edges { r[u][v] = true; }
            for k in 0..n { for i in 0..n { for j in 0..n {
                if r[i][k] && r[k][j] { r[i][j] = true; }
            }}}
            for u in 0..n { for v in 0..n {
                prop_assert_eq!(comp[u] == comp[v], r[u][v] && r[v][u]);
            }}
        }

        /// El camino que devuelve `path_in` es un camino real dentro de la
        /// componente.
        #[test]
        fn caminos_validos((n, e) in grafo()) {
            let edges: Vec<(usize, usize, &str)> = e.iter().map(|&(u, v)| (u, v, "x")).collect();
            let g = Graph::new(n, &edges);
            let (comp, cyc) = g.cyclic_components();
            for u in 0..n {
                for v in 0..n {
                    if comp[u] != comp[v] || !cyc[comp[u]] { continue; }
                    let mut seen = vec![false; n];
                    let mut pred = vec![(0, ""); n];
                    let p = g.path_in(u, v, comp[u], &comp, &mut seen, &mut pred);
                    let mut cur = u;
                    for &(_, w) in &p {
                        prop_assert!(edges.iter().any(|&(a, b, _)| a == cur && b == w));
                        prop_assert_eq!(comp[w], comp[u]);
                        cur = w;
                    }
                    prop_assert_eq!(cur, v);
                }
            }
        }
    }

    /// Las dos salidas anómalas de `path_in` no son silenciosas en debug:
    /// destino fuera de la componente.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "fuera de la componente")]
    fn camino_a_otra_componente_avisa() {
        let g = Graph::new(2, &[(0, 1, "x")]);
        let (comp, _) = g.cyclic_components();
        let (mut seen, mut pred) = (vec![false; 2], vec![(0, ""); 2]);
        g.path_in(0, 1, comp[0], &comp, &mut seen, &mut pred);
    }

    /// … y cadena de `pred` que no vuelve a `from` (marcadores de otra
    /// búsqueda que forman un bucle 1 → 2 → 1).
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "no llega al origen")]
    fn cadena_de_pred_rota_avisa() {
        let g = Graph::new(3, &[(0, 1, "x"), (1, 0, "x")]);
        let (comp, _) = g.cyclic_components();
        let mut seen = vec![false, true, true];
        let mut pred = vec![(0, ""), (2, "x"), (1, "x")];
        g.path_in(0, 1, comp[0], &comp, &mut seen, &mut pred);
    }
}
