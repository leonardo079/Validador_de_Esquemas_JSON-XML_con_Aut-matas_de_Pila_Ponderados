//! Piezas sueltas de F4.1a: la forma de `Dfa` (F4-B14) y
//! `location_clipped` (F4-B8, `docs/fase3/pendientes.md` §4).

use automaton::dfa::Dfa;
use automaton::location_clipped;
use ir::{NodeId, VerifiedIr};

/// Trie completo con el estado muerto explícito.
#[test]
fn trie_completo_con_muerto() {
    let t = Dfa::trie(3, &[vec![1, 2], vec![1], vec![2, 2, 2]]);
    // ε, 1, 12, 2, 22, 222, muerto.
    assert_eq!(t.n_states(), 7);
    assert_eq!(t.trans.len(), 7 * 3);
    assert_eq!(t.run(&[1, 2]), Some(0));
    assert_eq!(t.run(&[1]), Some(1));
    assert_eq!(t.run(&[2, 2, 2]), Some(2));
    assert_eq!(t.run(&[2, 2]), None);
    assert_eq!(t.run(&[0]), None);
    assert_eq!(t.run(&[1, 2, 0, 1]), None);
    let muertos: Vec<u32> = (0..7).filter(|&s| t.is_dead(s)).collect();
    assert_eq!(muertos, [6]);
    // Sin palabras: el inicial y el muerto.
    let v = Dfa::trie(2, &[]);
    assert_eq!(v.n_states(), 2);
    assert!(v.is_dead(1));
    assert_eq!(v.out[0], None);
}

fn bajar(src: &str) -> VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = schema_parser::parse(src.as_bytes()).unwrap();
            semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Si cabe, es `location`; si no, principio, `…` y final, sin partir
/// caracteres ni escapes, y nunca más de `max` bytes.
#[test]
fn ubicacion_recortada() {
    let largo = "x".repeat(5000);
    let src = format!(
        r#"{{"properties": {{"a/b~c": {{"properties": {{"{largo}": {{"items": {{"prefixItems": [true, {{"properties": {{"ñ😀ñ": true}}}}]}}}}}}}}}}}}"#
    );
    let ir = bajar(&src);
    for id in 0..ir.nodes().len() {
        let id = NodeId::new(id);
        let entera = ir.location(id);
        for max in [16, 40, 64, 256, 20_000] {
            let r = location_clipped(&ir, id, max);
            assert!(r.len() <= max.max(16), "{max}: {} bytes", r.len());
            if entera.len() <= max.max(16) {
                assert_eq!(r, entera);
            } else {
                let (ini, fin) = r.split_once('…').expect(&r);
                assert!(entera.starts_with(ini), "{r}");
                assert!(entera.ends_with(fin), "{r}");
                // No parte `~0`/`~1`.
                assert!(!ini.ends_with('~'), "{r}");
            }
        }
    }
    let ultimo = NodeId::new(ir.nodes().len() - 1);
    let r = location_clipped(&ir, ultimo, 128);
    assert!(r.starts_with("#/properties/a~1b~0c/"), "{r}");
    assert!(r.ends_with("/prefixItems/1/properties/ñ😀ñ"), "{r}");
    // Fuera de rango, como `location`.
    assert_eq!(location_clipped(&ir, NodeId::new(usize::MAX), 64), "#");
}
