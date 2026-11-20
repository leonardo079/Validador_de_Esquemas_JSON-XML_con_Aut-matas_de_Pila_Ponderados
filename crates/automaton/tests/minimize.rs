//! F4.3: minimización de Hopcroft de las partes finitas
//! (`automaton::minimize`) y verificación del determinismo de M en todo el
//! corpus (`automaton::determinism`).

use std::collections::{HashMap, VecDeque};

use automaton::dfa::Dfa;
use automaton::minimize::{hopcroft, moore};
use proptest::prelude::*;

/// Si `a` y `b` dan la misma salida tras toda palabra: recorrido en anchura
/// del producto desde los iniciales.
fn equivalentes<O: Copy + Default + PartialEq>(a: &Dfa<O>, b: &Dfa<O>) -> bool {
    assert_eq!(a.n_classes, b.n_classes);
    let mut vistos = HashMap::new();
    let mut cola = VecDeque::from([(a.start, b.start)]);
    vistos.insert((a.start, b.start), ());
    while let Some((p, q)) = cola.pop_front() {
        if a.out[p as usize] != b.out[q as usize] {
            return false;
        }
        for c in 0..a.n_classes {
            let par = (a.next(p, c), b.next(q, c));
            if vistos.insert(par, ()).is_none() {
                cola.push_back(par);
            }
        }
    }
    true
}

/// AFD completo aleatorio: ≤ 40 estados, ≤ 4 clases, ≤ 3 salidas.
fn afd() -> impl Strategy<Value = Dfa<u8>> {
    (1usize..=40, 1usize..=4).prop_flat_map(|(n, k)| {
        (
            prop::collection::vec(0..n as u32, n * k),
            prop::collection::vec(0u8..3, n),
            0..n as u32,
        )
            .prop_map(move |(trans, out, start)| Dfa {
                n_classes: k,
                start,
                trans,
                out,
            })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Hopcroft da el mismo AFD que Moore (mismo número de estados y la
    /// misma numeración canónica), equivalente al original e idempotente.
    #[test]
    fn hopcroft_es_moore_y_equivalente(d in afd()) {
        let h = hopcroft(&d);
        let m = moore(&d);
        prop_assert_eq!(h.n_states(), m.n_states());
        prop_assert_eq!(&h, &m);
        prop_assert!(equivalentes(&d, &h));
        prop_assert_eq!(hopcroft(&h), h.clone());
        prop_assert!(h.n_states() <= d.n_states());
    }
}

/// El AFD de subconjuntos de `(a|b)*abb` (Aho et al., fig. 3.36): 5
/// estados, de los que A y C son equivalentes; el mínimo tiene 4.
#[test]
fn subconjuntos_de_ab_estrella_abb() {
    // Estados A=0, B=1, C=2, D=3, E=4; clases a=0, b=1.
    let d = Dfa {
        n_classes: 2,
        start: 0,
        trans: vec![1, 2, 1, 3, 1, 2, 1, 4, 1, 2],
        out: vec![false, false, false, false, true],
    };
    let m = hopcroft(&d);
    assert_eq!(m.n_states(), 4);
    assert!(equivalentes(&d, &m));
    let a = |w: &str| -> Vec<usize> { w.bytes().map(|c| usize::from(c - b'a')).collect() };
    for (w, acepta) in [
        ("abb", true),
        ("babb", true),
        ("ab", false),
        ("abba", false),
    ] {
        assert_eq!(m.run(&a(w)), acepta, "{w}");
    }
}

/// El trie de {"ab", "cb"}: con el mismo destino (la misma salida) los dos
/// caminos comparten el sufijo (6 → 4 estados: inicio, {a, c}, {ab, cb} y
/// el muerto); con destinos distintos no se junta nada.
#[test]
fn trie_comparte_sufijo_solo_con_el_mismo_destino() {
    // Clases: a=0, b=1, c=2.
    let t = Dfa::trie(3, &[vec![0, 1], vec![2, 1]]);
    assert_eq!(t.n_states(), 6);
    assert_eq!(hopcroft(&t).n_states(), 6);
    let mismo = Dfa {
        out: t.out.iter().map(|o| o.map(|_| 7u32)).collect(),
        ..t.clone()
    };
    let m = hopcroft(&mismo);
    assert_eq!(m.n_states(), 4);
    assert!(equivalentes(&mismo, &m));
}

/// Los estados inalcanzables desaparecen.
#[test]
fn inalcanzables_desaparecen() {
    // 0 → 1 → 1; 2 y 3 no se alcanzan.
    let d = Dfa {
        n_classes: 1,
        start: 0,
        trans: vec![1, 1, 3, 2],
        out: vec![0u8, 1, 2, 2],
    };
    let m = hopcroft(&d);
    assert_eq!(m.n_states(), 2);
    assert_eq!(m.out, vec![0, 1]);
}

/// D7: un AFD en cadena de 10⁵ estados (todos distinguibles) se minimiza
/// en un hilo de 256 KiB, sin recursión.
#[test]
fn cadena_de_cien_mil_en_poca_pila() {
    let n = 100_000u32;
    let mut trans = Vec::with_capacity(2 * n as usize);
    for s in 0..n {
        trans.push((s + 1).min(n - 1));
        trans.push(n - 1);
    }
    let mut out = vec![0u8; n as usize];
    out[n as usize - 2] = 1;
    let d = Dfa {
        n_classes: 2,
        start: 0,
        trans,
        out,
    };
    let m = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || hopcroft(&d))
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(m.n_states(), n as usize);
}

/// `determinism::check` acepta el M de todos los esquemas del corpus y de
/// `tests/schemas/**/parse-ok` que compilan (también lo corre cada `build`;
/// aquí se llama de nuevo para que la prueba no dependa de eso), y el
/// resumen da los estados de las partes finitas antes y después.
#[test]
fn determinismo_en_todo_el_corpus() {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests");
    let mut textos: Vec<(bool, Vec<u8>)> = Vec::new();
    for (dir, xml) in [("schemas/parse-ok", false), ("schemas/xsd/parse-ok", true)] {
        for f in std::fs::read_dir(raiz.join(dir)).unwrap() {
            textos.push((xml, std::fs::read(f.unwrap().path()).unwrap()));
        }
    }
    let mut n = 0;
    let mut reducidos = 0;
    for (xml, t) in textos {
        let ir = std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                if xml {
                    let s = xsd::parse(&t).ok()?;
                    xsd::lower(&xsd::analyze(&s).ok()?).ok()
                } else {
                    let s = schema_parser::parse(&t).ok()?;
                    semantic::lower(&semantic::analyze(&s).ok()?).ok()
                }
            })
            .unwrap()
            .join()
            .unwrap();
        let Some(ir) = ir else { continue };
        let Ok(a) = automaton::build(&ir) else {
            continue;
        };
        automaton::determinism::check(&a).unwrap();
        let s = a.summary();
        assert!(s.dfa_states_min <= s.dfa_states);
        if s.dfa_states_min < s.dfa_states {
            reducidos += 1;
        }
        n += 1;
    }
    assert!(n >= 20, "{n}");
    // Los textos XML (espacios léxicos con *collapse*) se reducen.
    assert!(reducidos >= 1, "{reducidos}");
}
