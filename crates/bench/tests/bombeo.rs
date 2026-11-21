//! E1 — Pruebas de bombeo u·vᵏ·x·yᵏ·z (D80; `docs/fase6/bombeo.md`).
//! Deterministas (D1): nada de tiempos, solo veredictos, pesos y la pila.
//!
//! El lenguaje de los arreglos anidados `[ᵏ ]ᵏ` es el ejemplo clásico de
//! lenguaje libre de contexto no regular: el lema de bombeo para regulares
//! falla con él (con p estados, `[ᵖ ]ᵖ` obliga a bombear solo corchetes de
//! apertura y desequilibra la palabra), y el de libres de contexto se cumple
//! con v = `[` e y = `]`. Un AFD no lo reconoce; el DPDA sí, con pila O(k).

use automaton::{validate, ErrorKind, Mode};
use bench::{automata, bombeo, validar_dpda};

/// u·vᵏ·x·yᵏ·z.
fn bombear(u: &str, v: &str, x: &str, y: &str, z: &str, k: usize) -> Vec<u8> {
    format!("{u}{}{x}{}{z}", v.repeat(k), y.repeat(k)).into_bytes()
}

fn linea_base(esquema: &str) -> baseline::JsonBaseline {
    baseline::with_stack(|_| baseline::JsonBaseline::compile_text(esquema.as_bytes()).ok())
        .expect("compila")
}

#[test]
fn anidado_se_acepta_para_todo_k_hasta_el_tope() {
    let m = automata(bombeo::ESQUEMA_ANIDADO.as_bytes()).unwrap();
    let lb = linea_base(bombeo::ESQUEMA_ANIDADO);
    for k in 1..=255 {
        let p = bombear("", "[", "", "]", "", k);
        assert_eq!(p, bombeo::anidado(k).payload);
        let v = validar_dpda(&m, &p);
        assert!(v.valido, "k={k}");
        assert_eq!(v.violaciones, Some(0), "k={k}");
        if k % 32 == 1 || k == 255 {
            assert!(
                baseline::with_stack(|st| lb.validate(st, &p).valid),
                "línea base, k={k}"
            );
        }
    }
}

#[test]
fn bombear_solo_v_o_solo_y_rompe_la_palabra() {
    // Si se bombea v sin y (lo que haría un AFD), la palabra queda
    // desequilibrada: sintáctico al final o en el corchete de más.
    let m = automata(bombeo::ESQUEMA_ANIDADO.as_bytes()).unwrap();
    for k in [1usize, 2, 10, 100, 200] {
        let falta = format!("{}{}", "[".repeat(k + 1), "]".repeat(k)).into_bytes();
        let r = validate(&m, &falta, Mode::Total, 4096);
        assert_eq!(r.error_kind, Some(ErrorKind::Syntax), "k={k}");
        assert_eq!(
            r.first.unwrap().offset,
            falta.len() as u64,
            "k={k}: se detecta al acabar la entrada"
        );
        let sobra = format!("{}{}", "[".repeat(k), "]".repeat(k + 1)).into_bytes();
        let r = validate(&m, &sobra, Mode::Total, 4096);
        assert_eq!(r.error_kind, Some(ErrorKind::Syntax), "k={k}");
        assert_eq!(
            r.first.unwrap().offset,
            2 * k as u64,
            "k={k}: en el `]` de más"
        );
    }
}

#[test]
fn pasar_el_tope_de_profundidad_es_un_limite() {
    let m = automata(bombeo::ESQUEMA_ANIDADO.as_bytes()).unwrap();
    for k in [257usize, 300, 10_000] {
        let p = bombear("", "[", "", "]", "", k);
        let r = validate(&m, &p, Mode::Total, 4096);
        assert_eq!(r.error_kind, Some(ErrorKind::Limit), "k={k}");
        // Se corta en el primer `[` que pasa del tope: no lee el resto.
        assert_eq!(r.first.unwrap().offset, 256, "k={k}");
    }
}

#[test]
fn pila_lineal_en_k_y_memoria_fija() {
    let m = automata(bombeo::ESQUEMA_ANIDADO.as_bytes()).unwrap();
    let w0 = m.stack_words(0);
    let w1 = m.stack_words(1);
    assert!(w1 > w0);
    // Afín en k: w(k) = w(0) + k·(w(1) − w(0)), es decir, O(k).
    for k in [2usize, 10, 100, 255] {
        assert_eq!(m.stack_words(k), w0 + k * (w1 - w0), "k={k}");
    }
    // La memoria del ejecutor sale de los límites, no del payload.
    let opts = automaton::Options::new(Mode::Total);
    assert!(m.state_words(&opts.limits) >= m.stack_words(256));
}

#[test]
fn bombear_una_violacion_suma_k_en_el_semianillo_tropical() {
    // u = `[`, v = `9,`, x = `0`, y = ε, z = `]`: k elementos por encima del
    // máximo. El peso del recorrido (⊗ = suma) es exactamente k.
    let esquema = r#"{"type":"array","items":{"type":"integer","maximum":5}}"#;
    let m = automata(esquema.as_bytes()).unwrap();
    let lb = linea_base(esquema);
    for k in [0usize, 1, 2, 7, 50, 500] {
        let p = bombear("[", "9,", "0", "", "]", k);
        let v = validar_dpda(&m, &p);
        assert_eq!(v.violaciones, Some(k as u64), "k={k}");
        assert_eq!(v.valido, k == 0, "k={k}");
        let o = baseline::with_stack(|st| lb.validate(st, &p));
        assert_eq!(o.violation_count(), k.min(o.violation_count()), "k={k}");
        if k <= 50 {
            assert_eq!(o.violation_count(), k, "línea base, k={k}");
        }
    }
}

#[test]
fn bombeo_con_v_e_y_no_vacios_y_recursion() {
    // u = ε, v = `[9,`, x = `[0]`, y = `]`, z = ε: k niveles, cada uno con un
    // primer elemento > 5 (`prefixItems`) y el resto recursivo (`items`).
    let esquema = r##"{"$defs":{"n":{"type":"array","prefixItems":[{"type":"integer","maximum":5}],"items":{"$ref":"#/$defs/n"}}},"$ref":"#/$defs/n"}"##;
    let m = automata(esquema.as_bytes()).unwrap();
    let lb = linea_base(esquema);
    for k in [0usize, 1, 3, 20, 200] {
        let p = bombear("", "[9,", "[0]", "]", "", k);
        let v = validar_dpda(&m, &p);
        assert_eq!(
            v.violaciones,
            Some(k as u64),
            "k={k}: {}",
            String::from_utf8_lossy(&p)
        );
        if k <= 20 {
            let o = baseline::with_stack(|st| lb.validate(st, &p));
            assert_eq!(o.violation_count(), k, "línea base, k={k}");
        }
    }
}

#[test]
fn bloques_de_cualquier_tamano_dan_lo_mismo() {
    // Invariante del streaming: el veredicto no depende de cómo se parta la
    // entrada, tampoco en el bombeo.
    let m = automata(bombeo::ESQUEMA_ANIDADO.as_bytes()).unwrap();
    for k in [1usize, 64, 255, 257] {
        let p = bombear("", "[", "", "]", "", k);
        let r = validate(&m, &p, Mode::Total, 4096);
        for bloque in [1, 7, 64] {
            assert_eq!(
                validate(&m, &p, Mode::Total, bloque),
                r,
                "k={k}, bloque {bloque}"
            );
        }
    }
}
