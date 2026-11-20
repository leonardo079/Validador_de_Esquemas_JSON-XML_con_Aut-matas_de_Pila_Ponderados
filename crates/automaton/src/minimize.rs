//! Minimización de las partes finitas de M (F4.3; `docs/fase4/automata.md`
//! §17): el algoritmo de Hopcroft sobre un [`Dfa`] completo con salidas
//! (F4-B14, F4-B15), y el de Moore como oráculo de las pruebas.
//!
//! Dos estados son equivalentes (Myhill-Nerode, con salidas) si dan la misma
//! salida tras cualquier palabra. Hopcroft parte de la partición por salida
//! y la refina con los predecesores de cada bloque «separador», metiendo en
//! la cola solo la mitad menor de cada bloque partido: O(k·n·log n) con n
//! estados y k clases. Es iterativo (D7) y no reserva más que O(k·n).

use std::collections::HashMap;
use std::hash::Hash;

use crate::dfa::Dfa;

/// El AFD mínimo equivalente a `d` (misma salida tras cada palabra), sin
/// estados inalcanzables y con numeración canónica: los estados en el orden
/// en que los visita un recorrido en anchura desde el inicial, con las
/// clases en orden (el inicial es el 0). Dos AFD equivalentes dan el mismo
/// resultado, byte a byte.
pub fn hopcroft<O: Copy + Eq + Hash + Default>(d: &Dfa<O>) -> Dfa<O> {
    let k = d.n_classes;
    // 1. Alcanzables desde el inicial.
    let (reach, index) = reachable(d);
    let n = reach.len();
    if n == 0 || k == 0 {
        return canonical(d, &reach, &index, &vec![0; n]);
    }
    // 2. Predecesores por clase (CSR): pred[a][off[a][t] .. off[a][t + 1]].
    let mut off = vec![0usize; k * (n + 1)];
    for &s in &reach {
        for a in 0..k {
            let t = index[d.next(s, a) as usize] as usize;
            off[a * (n + 1) + t + 1] += 1;
        }
    }
    for a in 0..k {
        for t in 0..n {
            off[a * (n + 1) + t + 1] += off[a * (n + 1) + t];
        }
    }
    let mut pred = vec![0u32; k * n];
    let mut fill = off.clone();
    for (i, &s) in reach.iter().enumerate() {
        for a in 0..k {
            let t = index[d.next(s, a) as usize] as usize;
            let p = &mut fill[a * (n + 1) + t];
            pred[a * n + *p] = i as u32;
            *p += 1;
        }
    }

    // 3. Partición inicial por salida, en orden de primera aparición.
    let mut by_out: HashMap<O, usize> = HashMap::new();
    let mut blk = vec![0usize; n];
    for (i, &s) in reach.iter().enumerate() {
        let next = by_out.len();
        blk[i] = *by_out.entry(d.out[s as usize]).or_insert(next);
    }
    let nb = by_out.len();
    // Elementos agrupados por bloque: elems[start[b] .. end[b]].
    let mut start = vec![0usize; nb];
    let mut size = vec![0usize; nb];
    for &b in &blk {
        size[b] += 1;
    }
    for b in 1..nb {
        start[b] = start[b - 1] + size[b - 1];
    }
    let mut end = start.clone();
    let mut elems = vec![0u32; n];
    let mut pos = vec![0usize; n];
    for (i, &b) in blk.iter().enumerate() {
        elems[end[b]] = i as u32;
        pos[i] = end[b];
        end[b] += 1;
    }

    // 4. Refinamiento. La cola guarda (bloque, clase); `queued` evita
    // repetirlos. Al principio, todos los bloques con todas las clases.
    let mut queued = vec![true; nb * k];
    let mut work: Vec<(usize, usize)> = (0..nb).flat_map(|b| (0..k).map(move |a| (b, a))).collect();
    let mut marked = vec![0usize; nb];
    let mut touched: Vec<usize> = Vec::new();
    let mut x: Vec<u32> = Vec::new();
    while let Some((s, a)) = work.pop() {
        queued[s * k + a] = false;
        // X = predecesores por `a` de los estados de S.
        x.clear();
        for &t in &elems[start[s]..end[s]] {
            let o = a * (n + 1) + t as usize;
            x.extend_from_slice(&pred[a * n + off[o]..a * n + off[o + 1]]);
        }
        // Marca: cada p de X pasa al principio de su bloque.
        for &p in &x {
            let p = p as usize;
            let b = blk[p];
            if marked[b] == 0 {
                touched.push(b);
            }
            let dest = start[b] + marked[b];
            let q = elems[dest] as usize;
            elems.swap(dest, pos[p]);
            pos[q] = pos[p];
            pos[p] = dest;
            marked[b] += 1;
        }
        // Parte cada bloque tocado que no quedó entero marcado.
        for b in touched.drain(..) {
            let m = marked[b];
            marked[b] = 0;
            if m == end[b] - start[b] {
                continue;
            }
            let nuevo = start.len();
            start.push(start[b]);
            end.push(start[b] + m);
            start[b] += m;
            marked.push(0);
            for &e in &elems[start[nuevo]..end[nuevo]] {
                blk[e as usize] = nuevo;
            }
            queued.extend(std::iter::repeat_n(false, k));
            let menor = if end[nuevo] - start[nuevo] <= end[b] - start[b] {
                nuevo
            } else {
                b
            };
            for c in 0..k {
                let cual = if queued[b * k + c] { nuevo } else { menor };
                if !queued[cual * k + c] {
                    queued[cual * k + c] = true;
                    work.push((cual, c));
                }
            }
        }
    }
    canonical(d, &reach, &index, &blk)
}

/// Los estados alcanzables desde el inicial en orden de anchura, y el índice
/// de cada estado de `d` en esa lista (`u32::MAX` si no es alcanzable).
fn reachable<O: Copy + Default + PartialEq>(d: &Dfa<O>) -> (Vec<u32>, Vec<u32>) {
    let mut index = vec![u32::MAX; d.n_states()];
    let mut reach = Vec::new();
    if d.n_states() == 0 {
        return (reach, index);
    }
    index[d.start as usize] = 0;
    reach.push(d.start);
    let mut i = 0;
    while i < reach.len() {
        let s = reach[i];
        for a in 0..d.n_classes {
            let t = d.next(s, a);
            if index[t as usize] == u32::MAX {
                index[t as usize] = reach.len() as u32;
                reach.push(t);
            }
        }
        i += 1;
    }
    (reach, index)
}

/// El cociente de `d` por la partición `blk` (de los alcanzables), numerado
/// en anchura desde el bloque del inicial.
fn canonical<O: Copy + Default + PartialEq>(
    d: &Dfa<O>,
    reach: &[u32],
    index: &[u32],
    blk: &[usize],
) -> Dfa<O> {
    let k = d.n_classes;
    if reach.is_empty() {
        return d.clone();
    }
    let nb = blk.iter().copied().max().unwrap_or(0) + 1;
    // Un representante por bloque.
    let mut rep = vec![u32::MAX; nb];
    for (i, &b) in blk.iter().enumerate() {
        if rep[b] == u32::MAX {
            rep[b] = reach[i];
        }
    }
    let mut id = vec![u32::MAX; nb];
    let mut order: Vec<usize> = Vec::with_capacity(nb);
    let b0 = blk[index[d.start as usize] as usize];
    id[b0] = 0;
    order.push(b0);
    let mut trans = Vec::with_capacity(nb * k);
    let mut out = Vec::with_capacity(nb);
    let mut i = 0;
    while i < order.len() {
        let b = order[i];
        let s = rep[b];
        out.push(d.out[s as usize]);
        for a in 0..k {
            let t = blk[index[d.next(s, a) as usize] as usize];
            if id[t] == u32::MAX {
                id[t] = order.len() as u32;
                order.push(t);
            }
            trans.push(id[t]);
        }
        i += 1;
    }
    Dfa {
        n_classes: k,
        start: 0,
        trans,
        out,
    }
}

/// Minimización de Moore (refinar por la firma (bloque, bloques de los
/// sucesores) hasta que no cambia): O(n²·k). Solo es el oráculo de las
/// pruebas de [`hopcroft`] (F4-B15); da la misma numeración canónica.
pub fn moore<O: Copy + Eq + Hash + Default>(d: &Dfa<O>) -> Dfa<O> {
    let k = d.n_classes;
    let (reach, index) = reachable(d);
    let n = reach.len();
    let mut ids: HashMap<O, usize> = HashMap::new();
    let mut blk: Vec<usize> = reach
        .iter()
        .map(|&s| {
            let next = ids.len();
            *ids.entry(d.out[s as usize]).or_insert(next)
        })
        .collect();
    let mut nb = ids.len();
    loop {
        let mut sig: HashMap<Vec<usize>, usize> = HashMap::new();
        let mut nuevo = vec![0usize; n];
        for i in 0..n {
            let mut v = Vec::with_capacity(k + 1);
            v.push(blk[i]);
            for a in 0..k {
                v.push(blk[index[d.next(reach[i], a) as usize] as usize]);
            }
            let next = sig.len();
            nuevo[i] = *sig.entry(v).or_insert(next);
        }
        let m = sig.len();
        blk = nuevo;
        if m == nb {
            break;
        }
        nb = m;
    }
    canonical(d, &reach, &index, &blk)
}
