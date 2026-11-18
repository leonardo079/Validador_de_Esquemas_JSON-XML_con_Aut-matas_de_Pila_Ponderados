//! Rutinas numéricas `no_std` de `runtime-rs` (F4.1b, F4-U4, F4-B12)
//! frente a `ir::num` y a un oráculo racional: comparación exacta de
//! lexemas de RFC 8259, `multipleOf` y el entero sin lexema (F4.1a-CR-S10).

use std::cmp::Ordering;

use proptest::prelude::*;
use runtime_rs::num::{compare, is_integer, is_multiple, Divisor};

/// Lexema de RFC 8259 con exponentes largos (hasta 40 cifras, con ceros a
/// la izquierda) y negativos.
fn lexema() -> impl Strategy<Value = String> {
    (
        any::<bool>(),
        prop_oneof!["0", "[1-9][0-9]{0,6}"],
        prop::option::of("[0-9]{1,6}"),
        prop::option::of(("[eE]", prop_oneof!["", "\\+", "-"], "[0-9]{1,40}")),
    )
        .prop_map(|(neg, int, frac, exp)| {
            let mut s = String::new();
            if neg {
                s.push('-');
            }
            s.push_str(&int);
            if let Some(f) = frac {
                s.push('.');
                s.push_str(&f);
            }
            if let Some((e, sign, d)) = exp {
                s.push_str(&e);
                s.push_str(&sign);
                s.push_str(&d);
            }
            s
        })
}

/// Pareja de lexemas con el mismo exponente salvo la última cifra o un
/// desplazamiento de la coma: ejercita la resta exacta de exponentes largos.
fn pareja_cercana() -> impl Strategy<Value = (String, String)> {
    (
        "[1-9][0-9]{0,3}",
        "[0-9]{0,4}",
        "[1-9][0-9]{20,40}",
        0u8..10,
        any::<bool>(),
    )
        .prop_map(|(a, b, exp, ultima, neg)| {
            let mut otro = exp.clone();
            otro.pop();
            otro.push(char::from(b'0' + ultima));
            let s = if neg { "-" } else { "" };
            let x = format!("{a}e{s}{exp}");
            let y = if b.is_empty() {
                format!("{a}e{s}{otro}")
            } else {
                format!("{a}.{b}e{s}{otro}")
            };
            (x, y)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Criterio 1 de F4.1b: el comparador coincide con `ir::num::compare`.
    #[test]
    fn comparador_coincide_con_ir_num(a in lexema(), b in lexema()) {
        prop_assert_eq!(compare(a.as_bytes(), b.as_bytes()), ir::num::compare(&a, &b), "{} {}", a, b);
        prop_assert_eq!(compare(a.as_bytes(), a.as_bytes()), Ordering::Equal);
    }

    #[test]
    fn comparador_con_exponentes_cercanos((a, b) in pareja_cercana()) {
        prop_assert_eq!(compare(a.as_bytes(), b.as_bytes()), ir::num::compare(&a, &b), "{} {}", a, b);
        prop_assert_eq!(compare(b.as_bytes(), a.as_bytes()), ir::num::compare(&b, &a), "{} {}", b, a);
    }

    /// F4.1a-CR-S10: `is_integer` (los cinco registros saturados, sin
    /// lexema) frente a un oráculo hecho con `ir::num::compare`: x ≠ 0 de n
    /// cifras significativas es entero ⇔ |x| ≥ 10ⁿ⁻¹.
    #[test]
    fn entero_coincide_con_ir_num(x in lexema()) {
        prop_assert_eq!(is_integer(x.as_bytes()), entero_oraculo(&x), "{}", x);
    }

    /// `multipleOf` frente al oráculo racional con mantisas y exponentes
    /// pequeños.
    #[test]
    fn multiplo_coincide_con_el_oraculo(
        (xm, xe) in (0u64..100_000, -12i32..12),
        (dm, de) in (1u64..2_000, -8i32..8),
    ) {
        let x = format!("{xm}e{xe}");
        let d = format!("{dm}e{de}");
        let div = Divisor::of(d.as_bytes()).unwrap();
        prop_assert_eq!(is_multiple(x.as_bytes(), d.as_bytes(), &div), multiplo_oraculo(xm, xe, dm, de), "{} / {}", x, d);
    }
}

/// Oráculo del entero con `ir::num`: el cero es entero; si no, con n cifras
/// significativas (sin ceros en los extremos de la mantisa), x = 0.D·10^E
/// es entero ⇔ E ≥ n ⇔ |x| ≥ 10ⁿ⁻¹.
fn entero_oraculo(x: &str) -> bool {
    let abs = x.trim_start_matches('-');
    let mantisa: String = abs
        .split(['e', 'E'])
        .next()
        .unwrap()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let sig = mantisa.trim_start_matches('0').trim_end_matches('0');
    if sig.is_empty() {
        return true;
    }
    ir::num::compare(abs, &format!("1e{}", sig.len() - 1)) != Ordering::Less
}

/// Oráculo racional: x = xm·10^xe es múltiplo de d = dm·10^de ⇔
/// xm·10^(xe − de) / dm es entero.
fn multiplo_oraculo(xm: u64, xe: i32, dm: u64, de: i32) -> bool {
    let k = xe - de;
    if k >= 0 {
        (u128::from(xm) * 10u128.pow(k as u32)).is_multiple_of(u128::from(dm))
    } else {
        u128::from(xm) % (u128::from(dm) * 10u128.pow((-k) as u32)) == 0
    }
}

fn multiplo(x: &str, d: &str) -> bool {
    let div = Divisor::of(d.as_bytes()).unwrap_or_else(|e| panic!("{d}: {e:?}"));
    is_multiple(x.as_bytes(), d.as_bytes(), &div)
}

/// Criterio 2 de F4.1b: la tabla de `multipleOf` contra el oráculo racional
/// (hecha a mano), con exponentes de 10⁴ cifras en el divisor y en x.
#[test]
fn tabla_de_multiple_of() {
    let largo = "9".repeat(10_000);
    let casos: Vec<(String, &str, bool)> = vec![
        ("0.1".into(), "0.01", true),
        ("0.10".into(), "0.01", true),
        ("1e-2".into(), "0.01", true),
        ("0.005".into(), "0.01", false),
        ("1e2".into(), "3", false),
        ("3e2".into(), "3", true),
        ("7.5".into(), "2.5", true),
        ("5".into(), "2.5", true),
        ("1".into(), "2.5", false),
        ("1.25".into(), "2.5", false),
        ("0".into(), "2.5", true),
        ("-0.0e-7".into(), "3", true),
        ("-12".into(), "4", true),
        ("10".into(), "0.0625", true),
        ("0.125".into(), "0.0625", true),
        ("0.03125".into(), "0.0625", false),
        ("1024".into(), "512", true),
        ("1536".into(), "1024", false),
        // Exponentes de 10⁴ cifras: con un divisor diminuto todo es
        // múltiplo; con uno gigantesco, solo el cero.
        ("0.3".into(), "1e-1", true),
        (format!("1e{largo}"), "0.01", true),
        (format!("1e-{largo}"), "0.01", false),
        (format!("3e{largo}"), "3", true),
        (format!("1e{largo}"), "3", false),
        ("123.456".into(), "1e-3", true),
    ];
    for (x, d, esperado) in &casos {
        assert_eq!(multiplo(x, d), *esperado, "{x} / {d}");
    }
    let diminuto = format!("1e-{largo}");
    let gigante = format!("1e{largo}");
    let div = Divisor::of(diminuto.as_bytes()).unwrap();
    assert!(is_multiple(b"0.1234567", diminuto.as_bytes(), &div));
    let div = Divisor::of(gigante.as_bytes()).unwrap();
    assert!(!is_multiple(b"1e100", gigante.as_bytes(), &div));
    assert!(is_multiple(
        format!("1e{largo}").as_bytes(),
        gigante.as_bytes(),
        &div
    ));
    assert!(!is_multiple(
        format!("1e{}8", "9".repeat(9_999)).as_bytes(),
        gigante.as_bytes(),
        &div
    ));
    assert!(is_multiple(b"0", gigante.as_bytes(), &div));
}

/// F4-U6: el divisor admite una parte coprima con 10 menor que 2⁶⁰ y
/// factores 2ᵃ ≤ 2⁶³ y 5ᵇ ≤ 5²⁷; si no, `None` (el constructor da `limit`).
#[test]
fn divisor_por_encima_del_tope() {
    // 2⁶⁰ − 93 es primo, impar y no múltiplo de 5: por debajo.
    let primo = ((1u64 << 60) - 93).to_string();
    assert!(Divisor::of(primo.as_bytes()).is_ok());
    // 2⁶¹ − 1 (primo de Mersenne): parte coprima ≥ 2⁶⁰.
    assert!(Divisor::of((1u64 << 61).wrapping_sub(1).to_string().as_bytes()).is_err());
    assert!(Divisor::of((1u64 << 63).to_string().as_bytes()).is_ok());
    assert!(Divisor::of((1u128 << 64).to_string().as_bytes()).is_err());
    assert!(Divisor::of(5u128.pow(27).to_string().as_bytes()).is_ok());
    assert!(Divisor::of(5u128.pow(28).to_string().as_bytes()).is_err());
    assert!(Divisor::of(format!("1{}e-5", "0".repeat(50)).as_bytes()).is_ok());
    assert!(Divisor::of("7".repeat(40).as_bytes()).is_err());
    // El de mayor 2ᵃ admitido sigue siendo exacto.
    let d = (1u64 << 63).to_string();
    assert!(multiplo(&(1u128 << 64).to_string(), &d));
    assert!(!multiplo(&(1u128 << 62).to_string(), &d));
}

#[test]
fn comparador_casos_frontera() {
    let largo = "9".repeat(10_000);
    for (a, b, o) in [
        ("0", "-0", Ordering::Equal),
        ("1", "1.0", Ordering::Equal),
        ("1", "1e0", Ordering::Equal),
        ("10e-1", "1", Ordering::Equal),
        ("0.1e1", "1.000", Ordering::Equal),
        ("-1", "0", Ordering::Less),
        ("-2", "-1", Ordering::Less),
        ("1.5", "1.49999", Ordering::Greater),
        ("0.0001", "1e-4", Ordering::Equal),
        ("0e999", "0.0", Ordering::Equal),
    ] {
        assert_eq!(compare(a.as_bytes(), b.as_bytes()), o, "{a} {b}");
    }
    let big = format!("1e{largo}");
    let big2 = format!("10e{}8", "9".repeat(9_999));
    assert_eq!(compare(big.as_bytes(), big2.as_bytes()), Ordering::Equal);
    assert_eq!(compare(big.as_bytes(), b"1e100"), Ordering::Greater);
    let tiny = format!("-1e-{largo}");
    assert_eq!(compare(tiny.as_bytes(), b"0"), Ordering::Less);
    assert_eq!(compare(tiny.as_bytes(), b"-1e-100"), Ordering::Greater);
}

#[test]
fn entero_con_exponentes_largos() {
    let largo = "9".repeat(100_000);
    assert!(is_integer(format!("1.5e{largo}").as_bytes()));
    assert!(!is_integer(format!("1.5e-{largo}").as_bytes()));
    assert!(is_integer(format!("0.000e-{largo}").as_bytes()));
    for l in ["0", "-0", "1.0", "1e2", "10e-1", "100e-2", "1.50e1"] {
        assert!(is_integer(l.as_bytes()), "{l}");
    }
    for l in ["1.5", "1e-1", "0.10", "100e-3", "-2.25e1", "1.01e1"] {
        assert!(!is_integer(l.as_bytes()), "{l}");
    }
}
