//! Propiedades exactas de un número a partir de su lexema decimal.
//!
//! El parser guarda los números como el lexema de la RFC 8259
//! (`-? int frac? exp?`), nunca como `f64` (`docs/00-alcance.md` §1.1). Aquí
//! se decide, sin convertir a binario y sin perder precisión, si el valor es
//! cero, negativo o entero. «Entero» sigue a 2020-12: la parte fraccionaria
//! es cero, así que `1.0` y `1e2` son enteros y `1.5e1` también (vale 15).
//!
//! Método: el valor es `D × 10^s`, donde `D` son los dígitos de `int` y
//! `frac` concatenados y `s = exp − |frac|`. Se quitan los ceros a la derecha
//! de `D` (cada uno suma 1 a `s`) y los de la izquierda (no cambian el
//! valor). Si `D` queda vacío el valor es cero; si no, es entero si y solo si
//! `s ≥ 0`. El exponente puede tener cualquier número de dígitos: se acumula
//! con saturación, lo que no cambia el signo de `s` (el lexema tiene a lo
//! sumo unos pocos millones de dígitos y la saturación está muy por encima).

/// Lo que la validación del meta-esquema necesita saber de un número.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NumInfo {
    /// El valor es exactamente cero (`0`, `-0`, `0.000`, `0e99`…).
    pub zero: bool,
    /// El valor es estrictamente negativo (`-0` no lo es).
    pub negative: bool,
    /// La parte fraccionaria es cero.
    pub integer: bool,
}

/// Analiza un lexema que el lexer JSON ya aceptó. Ante un lexema que no
/// cumple la gramática no falla: los caracteres inesperados se ignoran (el
/// parser nunca los produce; ver la prueba de propiedades).
pub(crate) fn classify(lexeme: &str) -> NumInfo {
    let bytes = lexeme.as_bytes();
    let (sign, rest) = match bytes.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, bytes),
    };
    // Mantisa y exponente.
    let (mantissa, exp) = match rest.iter().position(|&b| b == b'e' || b == b'E') {
        Some(i) => (&rest[..i], exponent(&rest[i + 1..])),
        None => (rest, 0),
    };
    let (int, frac) = match mantissa.iter().position(|&b| b == b'.') {
        Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
        None => (mantissa, &mantissa[mantissa.len()..]),
    };
    let digits = || int.iter().chain(frac).copied().filter(u8::is_ascii_digit);
    let zero = digits().all(|d| d == b'0');
    if zero {
        return NumInfo {
            zero: true,
            negative: false,
            integer: true,
        };
    }
    // Ceros a la derecha de D (no todos son cero, así que hay un no cero).
    let trailing = digits().rev().take_while(|&d| d == b'0').count();
    let frac_len = frac.iter().filter(|b| b.is_ascii_digit()).count();
    let s = exp
        .saturating_sub(to_i64(frac_len))
        .saturating_add(to_i64(trailing));
    NumInfo {
        zero: false,
        negative: sign,
        integer: s >= 0,
    }
}

/// Exponente con signo, acumulado con saturación.
fn exponent(bytes: &[u8]) -> i64 {
    let (neg, digits) = match bytes.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, bytes),
    };
    let mut v: i64 = 0;
    for &d in digits.iter().filter(|b| b.is_ascii_digit()) {
        v = v.saturating_mul(10).saturating_add(i64::from(d - b'0'));
    }
    if neg {
        -v
    } else {
        v
    }
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn info(zero: bool, negative: bool, integer: bool) -> NumInfo {
        NumInfo {
            zero,
            negative,
            integer,
        }
    }

    #[test]
    fn casos_conocidos() {
        let casos = [
            ("0", info(true, false, true)),
            ("-0", info(true, false, true)),
            ("0.000", info(true, false, true)),
            ("-0.0e-5", info(true, false, true)),
            ("0e999999999999999999999", info(true, false, true)),
            ("1", info(false, false, true)),
            ("-1", info(false, true, true)),
            ("1.0", info(false, false, true)),
            ("1.5", info(false, false, false)),
            ("1.5e1", info(false, false, true)),
            ("1.25e1", info(false, false, false)),
            ("100e-2", info(false, false, true)),
            ("100e-3", info(false, false, false)),
            ("1E+2", info(false, false, true)),
            ("0.01", info(false, false, false)),
            ("-273.15", info(false, true, false)),
            ("1e999999999999999999999", info(false, false, true)),
            ("1e-999999999999999999999", info(false, false, false)),
            ("10000000000000000000000000000001", info(false, false, true)),
            (
                "0.00000000000000000000000000000001",
                info(false, false, false),
            ),
        ];
        for (lexema, esperado) in casos {
            assert_eq!(classify(lexema), esperado, "{lexema}");
        }
    }

    /// Lexema de la RFC 8259 con valor `m × 10^(e − |frac|)` conocido.
    fn lexema() -> impl Strategy<Value = (String, i128, i32)> {
        (
            any::<bool>(),
            0u64..1_000_000,
            0usize..6,
            -8i32..8,
            0u8..3,
            any::<bool>(),
        )
            .prop_map(|(neg, m, frac_len, e, forma, mayus)| {
                // m con frac_len cifras decimales: int.frac
                let s = format!("{m:0width$}", width = frac_len + 1);
                let (int, frac) = s.split_at(s.len() - frac_len);
                let int = int.trim_start_matches('0');
                let int = if int.is_empty() { "0" } else { int };
                let mut lex = String::new();
                if neg {
                    lex.push('-');
                }
                lex.push_str(int);
                if !frac.is_empty() {
                    lex.push('.');
                    lex.push_str(frac);
                }
                let e_txt = match forma {
                    0 if e == 0 => String::new(),
                    0 | 1 => format!("{}{e}", if mayus { 'E' } else { 'e' }),
                    _ if e >= 0 => format!("e+{e}"),
                    _ => format!("e{e}"),
                };
                lex.push_str(&e_txt);
                let v = if neg { -i128::from(m) } else { i128::from(m) };
                (lex, v, e - frac_len as i32)
            })
    }

    proptest! {
        /// Coincide con aritmética entera exacta: v × 10^s es entero si
        /// s ≥ 0 o si 10^(−s) divide a v.
        #[test]
        fn coincide_con_aritmetica_exacta((lex, v, s) in lexema()) {
            let i = classify(&lex);
            let integer = s >= 0 || v % 10i128.pow(s.unsigned_abs()) == 0;
            prop_assert_eq!(i, info(v == 0, v < 0, integer), "{}", lex);
        }

        /// Nunca falla, aunque reciba algo que no es un lexema.
        #[test]
        fn no_falla_con_texto_arbitrario(s in ".{0,40}") {
            let _ = classify(&s);
        }
    }
}
