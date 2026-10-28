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

/// Valor de un entero no negativo (lo que el meta-esquema admite en
/// `minLength`, `maxItems`…: `5`, `5.0`, `1.5e1`, `1e400`) como `u64`,
/// **saturado** en `u64::MAX` (`docs/fase3/ir.md` §5.1). Es exacto: el
/// valor es `D × 10^s` con `s ≥ 0` tras quitar los ceros a la derecha de
/// `D` (ver el método al principio del módulo), y se satura en cuanto `D`
/// tiene más de 20 cifras o `s` pasa de 19. Ante un lexema que no es un
/// entero no negativo (la bajada nunca lo pasa) devuelve la parte que
/// cabría, sin fallar: 0 para negativos y la parte entera truncada para
/// fracciones.
pub(crate) fn to_u64_saturating(lexeme: &str) -> u64 {
    let bytes = lexeme.as_bytes();
    if bytes.first() == Some(&b'-') {
        return 0;
    }
    let (mantissa, exp) = match bytes.iter().position(|&b| b == b'e' || b == b'E') {
        Some(i) => (&bytes[..i], exponent(&bytes[i + 1..])),
        None => (bytes, 0),
    };
    let (int, frac) = match mantissa.iter().position(|&b| b == b'.') {
        Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
        None => (mantissa, &mantissa[mantissa.len()..]),
    };
    let digits: Vec<u8> = int
        .iter()
        .chain(frac)
        .copied()
        .filter(u8::is_ascii_digit)
        .skip_while(|&d| d == b'0')
        .collect();
    let trailing = digits.iter().rev().take_while(|&&d| d == b'0').count();
    let significant = &digits[..digits.len() - trailing];
    if significant.is_empty() {
        return 0;
    }
    // Valor = significant × 10^s.
    let frac_len = frac.iter().filter(|b| b.is_ascii_digit()).count();
    let s = exp
        .saturating_sub(to_i64(frac_len))
        .saturating_add(to_i64(trailing));
    // Si s < 0 se descartan las −s últimas cifras (truncar la fracción).
    let keep = if s < 0 {
        let drop = usize::try_from(s.unsigned_abs()).unwrap_or(usize::MAX);
        &significant[..significant.len().saturating_sub(drop)]
    } else {
        significant
    };
    let mut v: u64 = 0;
    for &d in keep {
        v = match v
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(d - b'0')))
        {
            Some(v) => v,
            None => return u64::MAX,
        };
    }
    if v == 0 || s <= 0 {
        return v;
    }
    // u64::MAX < 10^20: con s ≥ 20 y v ≥ 1 ya satura.
    let Ok(s) = u32::try_from(s) else {
        return u64::MAX;
    };
    10u64
        .checked_pow(s)
        .and_then(|p| v.checked_mul(p))
        .unwrap_or(u64::MAX)
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

    #[test]
    fn enteros_a_u64_saturado() {
        let casos = [
            ("0", 0),
            ("-0", 0),
            ("0e999999999999999999999", 0),
            ("7", 7),
            ("5.0", 5),
            ("1.5e1", 15),
            ("100e-2", 1),
            ("1E+2", 100),
            ("0.000120e6", 120),
            ("18446744073709551615", u64::MAX),
            ("18446744073709551614", u64::MAX - 1),
            ("18446744073709551616", u64::MAX),
            ("1844674407370955161.5e1", u64::MAX),
            ("1e19", 10_000_000_000_000_000_000),
            ("2e19", u64::MAX),
            ("1e20", u64::MAX),
            ("1e400", u64::MAX),
            ("1e999999999999999999999", u64::MAX),
            ("000000000000000000000000003", 3),
            // No son enteros no negativos: no fallan.
            ("-3", 0),
            ("2.5", 2),
            ("1e-999999999999999999999", 0),
        ];
        for (lexema, esperado) in casos {
            assert_eq!(to_u64_saturating(lexema), esperado, "{lexema}");
        }
    }

    proptest! {
        /// Coincide con la aritmética exacta en el rango de `u64`.
        #[test]
        fn u64_coincide_con_aritmetica_exacta(v in any::<u64>(), ceros in 0usize..5, e in 0u32..3) {
            // v·10^ceros escrito como "v" + ceros + "e-" ceros… y en otras formas.
            let lex = format!("{v}{}e-{ceros}", "0".repeat(ceros));
            prop_assert_eq!(to_u64_saturating(&lex), v);
            let lex = format!("{v}.{}", "0".repeat(ceros + 1));
            prop_assert_eq!(to_u64_saturating(&lex), v);
            let esperado = u128::from(v) * 10u128.pow(e);
            let esperado = u64::try_from(esperado).unwrap_or(u64::MAX);
            prop_assert_eq!(to_u64_saturating(&format!("{v}e{e}")), esperado);
        }

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
