//! Rutinas numéricas exactas `no_std` sobre lexemas de RFC 8259 (F4-U4,
//! F4-B12): comparación por valor ([`compare`]), `multipleOf`
//! ([`is_multiple`]) y el entero sin lexema ([`Number`], [`is_integer`]).
//!
//! Sin `f64`, sin `alloc` y sin recursión: un lexema se lee como
//! (−1)^s · 0.D · 10^E (D sin ceros en los extremos) y se comparan signo,
//! E y D. El exponente escrito puede tener cualquier longitud: la diferencia
//! de dos exponentes se calcula exacta mientras su valor absoluto es menor
//! que 2¹⁰⁰ y, por encima, se satura conservando el signo, que es lo único
//! que decide. Cada rutina es O(|a| + |b|). Teoría:
//! `docs/fase4/automata.md` §3.4-§3.5.

use core::cmp::Ordering;

use lexer::json::NumPart;

/// Tope de la diferencia de exponentes ([`exp_diff`]): más que la suma de
/// cualquier par de longitudes de lexema, así que saturar no cambia el
/// signo de E_a − E_b.
const CAP: i128 = 1 << 100;

/// Cifras que caben exactas en un `i128` con margen (10³¹ > [`CAP`]).
const SMALL: usize = 31;

/// 5²⁷, la mayor potencia de 5 que admite un divisor ([`Divisor::of`]).
const P5_27: u128 = 7_450_580_596_923_828_125;

/// Las partes de un lexema `-? int (. frac)? ([eE] [+-]? exp)?`.
struct Parts<'a> {
    neg: bool,
    int: &'a [u8],
    frac: &'a [u8],
    exp_neg: bool,
    exp: &'a [u8],
}

impl<'a> Parts<'a> {
    fn new(l: &'a [u8]) -> Parts<'a> {
        // El `+` inicial solo aparece en los números de XSD (el lexema de un
        // texto `xs:decimal`, `xs:integer` o `xs:float` tras el *collapse*,
        // F4.1c); RFC 8259 no lo admite, así que JSON no cambia.
        let (neg, rest) = match l.split_first() {
            Some((b'-', r)) => (true, r),
            Some((b'+', r)) => (false, r),
            _ => (false, l),
        };
        let (mant, exp) = match rest.iter().position(|&c| c == b'e' || c == b'E') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, &rest[rest.len()..]),
        };
        let (int, frac) = match mant.iter().position(|&c| c == b'.') {
            Some(i) => (&mant[..i], &mant[i + 1..]),
            None => (mant, &mant[mant.len()..]),
        };
        let (exp_neg, exp) = match exp.split_first() {
            Some((b'-', r)) => (true, r),
            Some((b'+', r)) => (false, r),
            _ => (false, exp),
        };
        Parts {
            neg,
            int,
            frac,
            exp_neg,
            exp,
        }
    }

    /// Cifras de la mantisa (parte entera y fracción), como valores 0-9.
    fn digits(&self) -> impl DoubleEndedIterator<Item = u8> + '_ {
        self.int.iter().chain(self.frac).map(|&c| dig(c))
    }

    fn total(&self) -> usize {
        self.int.len() + self.frac.len()
    }

    /// Ceros a la izquierda de la mantisa.
    fn lead(&self) -> usize {
        self.digits().take_while(|&d| d == 0).count()
    }

    /// Ceros finales de la mantisa.
    fn trailing(&self) -> usize {
        self.digits().rev().take_while(|&d| d == 0).count()
    }

    /// −1, 0 o 1 (−0 es 0), dado `lead`.
    fn sign(&self, lead: usize) -> i8 {
        match (lead == self.total(), self.neg) {
            (true, _) => 0,
            (false, true) => -1,
            (false, false) => 1,
        }
    }
}

/// Valor de una cifra ASCII; lo que no es cifra (no lo hay en un lexema de
/// RFC 8259) se lee como 9, sin pánico.
fn dig(c: u8) -> u8 {
    c.wrapping_sub(b'0').min(9)
}

fn strip(d: &[u8]) -> &[u8] {
    let i = d.iter().position(|&c| c != b'0').unwrap_or(d.len());
    &d[i..]
}

fn small(d: &[u8]) -> i128 {
    d.iter().fold(0i128, |v, &c| v * 10 + i128::from(dig(c)))
}

/// Magnitudes sin ceros a la izquierda: la más larga es la mayor.
fn cmp_mag(a: &[u8], b: &[u8]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// x − y para magnitudes x > y (sin ceros a la izquierda), saturado en
/// [`CAP`]: se resta de la cifra menos significativa a la más; si queda
/// alguna cifra no nula a partir de la posición [`SMALL`], la diferencia es
/// ≥ 10³¹ > [`CAP`].
fn sub_clamped(x: &[u8], y: &[u8]) -> i128 {
    let mut borrow = 0i32;
    let mut val = 0i128;
    let mut pow = 1i128;
    let mut big = false;
    for i in 0..x.len() {
        let dx = i32::from(dig(x[x.len() - 1 - i]));
        let dy = if i < y.len() {
            i32::from(dig(y[y.len() - 1 - i]))
        } else {
            0
        };
        let mut d = dx - dy - borrow;
        borrow = 0;
        if d < 0 {
            d += 10;
            borrow = 1;
        }
        if i < SMALL {
            val += i128::from(d) * pow;
            pow *= 10;
        } else if d != 0 {
            big = true;
        }
    }
    if big {
        CAP
    } else {
        val.min(CAP)
    }
}

/// (±a) − (±b) para dos exponentes escritos de cualquier longitud: exacta
/// si su valor absoluto es menor que [`CAP`]; si no, ±[`CAP`] con el signo
/// correcto.
fn exp_diff(a_neg: bool, a: &[u8], b_neg: bool, b: &[u8]) -> i128 {
    let (a, b) = (strip(a), strip(b));
    let sa: i128 = match (a.is_empty(), a_neg) {
        (true, _) => 0,
        (false, true) => -1,
        (false, false) => 1,
    };
    let sb: i128 = match (b.is_empty(), b_neg) {
        (true, _) => 0,
        (false, true) => -1,
        (false, false) => 1,
    };
    if a.len() <= SMALL && b.len() <= SMALL {
        return (sa * small(a) - sb * small(b)).clamp(-CAP, CAP);
    }
    // Uno tiene al menos 32 cifras (≥ 10³¹ > CAP).
    if sa != sb {
        // Signos distintos (o uno nulo): |A − B| = |A| + |B| > CAP.
        return if sa != 0 { sa * CAP } else { -sb * CAP };
    }
    match cmp_mag(a, b) {
        Ordering::Equal => 0,
        Ordering::Greater => sa * sub_clamped(a, b),
        Ordering::Less => -sa * sub_clamped(b, a),
    }
}

/// Compara las cifras significativas como fracciones 0.D: las dos
/// sucesiones se rellenan con ceros, así que los ceros finales no cuentan.
fn digits_cmp(mut a: impl Iterator<Item = u8>, mut b: impl Iterator<Item = u8>) -> Ordering {
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (Some(x), Some(y)) if x != y => return x.cmp(&y),
            (Some(x), None) if x != 0 => return Ordering::Greater,
            (None, Some(y)) if y != 0 => return Ordering::Less,
            _ => {}
        }
    }
}

/// Compara por valor dos lexemas de RFC 8259, exactamente: `-0` = `0`,
/// `1.0` = `1e0`, `10e-1` = `1`, con exponentes de cualquier longitud. Es
/// el mismo orden total que `ir::num::compare` (lo comprueba un proptest),
/// sin memoria dinámica. O(|a| + |b|).
pub fn compare(a: &[u8], b: &[u8]) -> Ordering {
    let (x, y) = (Parts::new(a), Parts::new(b));
    let (lx, ly) = (x.lead(), y.lead());
    let (sx, sy) = (x.sign(lx), y.sign(ly));
    if sx != sy {
        return sx.cmp(&sy);
    }
    if sx == 0 {
        return Ordering::Equal;
    }
    // E = exp + |int| − lead: E_x − E_y, exacta o saturada con su signo.
    let off = |p: &Parts<'_>, lead: usize| p.int.len() as i128 - lead as i128;
    let e = exp_diff(x.exp_neg, x.exp, y.exp_neg, y.exp).saturating_add(off(&x, lx) - off(&y, ly));
    let mag = if e != 0 {
        e.cmp(&0)
    } else {
        digits_cmp(x.digits().skip(lx), y.digits().skip(ly))
    };
    if x.neg {
        mag.reverse()
    } else {
        mag
    }
}

/// Un divisor de `multipleOf` analizado en compilación: d = 2ᵃ · 5ᵇ · M′ ·
/// 10^e′ con M′ coprimo con 10 (F4-U6: M′ < 2⁶⁰; además a ≤ 63 y b ≤ 27,
/// para que cada factor quepa en un registro de 64 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Divisor {
    /// M′, la parte coprima con 10 de las cifras significativas.
    pub coprime: u64,
    /// a.
    pub twos: u32,
    /// b.
    pub fives: u32,
}

/// Por qué un divisor pasa del tope (el constructor da `limit`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivisorError {
    /// Más de 38 cifras significativas: alguna parte pasa del tope.
    Digits(usize),
    /// La parte coprima con 10 tiene estos bits (≥ 60).
    Coprime(u32),
    /// 2ᵃ con a > 63.
    Twos(u32),
    /// 5ᵇ con b > 27.
    Fives(u32),
}

impl Divisor {
    /// Analiza el lexema de un divisor (> 0, RFC 8259). O(|lexema|).
    pub fn of(lexeme: &[u8]) -> Result<Divisor, DivisorError> {
        let p = Parts::new(lexeme);
        let (lead, trail, total) = (p.lead(), p.trailing(), p.total());
        let n = total.saturating_sub(lead + trail);
        if n > 38 {
            return Err(DivisorError::Digits(n));
        }
        let mut m = p
            .digits()
            .skip(lead)
            .take(n)
            .fold(0u128, |v, d| v * 10 + u128::from(d));
        if m == 0 {
            // El cero no es un divisor válido (I8 exige > 0): sin tope.
            return Ok(Divisor {
                coprime: 1,
                twos: 0,
                fives: 0,
            });
        }
        let mut twos = 0;
        while m % 2 == 0 {
            m /= 2;
            twos += 1;
        }
        let mut fives = 0;
        while m % 5 == 0 {
            m /= 5;
            fives += 1;
        }
        if m >= 1 << 60 {
            return Err(DivisorError::Coprime(128 - m.leading_zeros()));
        }
        if twos > 63 {
            return Err(DivisorError::Twos(twos));
        }
        if fives > 27 {
            return Err(DivisorError::Fives(fives));
        }
        Ok(Divisor {
            coprime: m as u64,
            twos,
            fives,
        })
    }
}

/// Si el lexema `x` es múltiplo del divisor `d` (su lexema y su análisis
/// [`Divisor::of`]), exactamente.
///
/// Con x = Dₓ · 10^eₓ y d = D_d · 10^e_d (D sin ceros finales, así que no
/// es múltiplo de 10) y k = eₓ − e_d: si k < 0, x/d = Dₓ / (D_d · 10^|k|)
/// no es entero (Dₓ no es múltiplo de 10); si k ≥ 0, x/d = Dₓ · 10^k / D_d
/// es entero ⇔ M′ | Dₓ, 2^(a−k) | Dₓ y 5^(b−k) | Dₓ. Los tres restos se
/// calculan por Horner en registros de 64 y 128 bits. O(|x| + |d|).
pub fn is_multiple(x: &[u8], d: &[u8], div: &Divisor) -> bool {
    let px = Parts::new(x);
    let lead = px.lead();
    let total = px.total();
    if lead == total {
        return true;
    }
    let pd = Parts::new(d);
    let zx = px.trailing();
    // e = exp − |frac| + ceros finales.
    let ex = zx as i128 - px.frac.len() as i128;
    let ed = pd.trailing() as i128 - pd.frac.len() as i128;
    let k = exp_diff(px.exp_neg, px.exp, pd.exp_neg, pd.exp).saturating_add(ex - ed);
    if k < 0 {
        return false;
    }
    let m = div.coprime.max(1);
    let (mut rm, mut r2, mut r5) = (0u64, 0u64, 0u128);
    for v in px.digits().skip(lead).take(total - lead - zx) {
        // rm < 2⁶⁰: rm · 10 + 9 < 2⁶⁴.
        rm = (rm * 10 + u64::from(v)) % m;
        r2 = r2.wrapping_mul(10).wrapping_add(u64::from(v));
        r5 = (r5 * 10 + u128::from(v)) % P5_27;
    }
    let k = u32::try_from(k.min(64)).unwrap_or(64);
    let need2 = div.twos.saturating_sub(k).min(63);
    let need5 = div.fives.saturating_sub(k).min(27);
    rm == 0 && r2 & ((1u64 << need2) - 1) == 0 && r5 % 5u128.pow(need5) == 0
}

/// Registros del número abierto que bastan para decidir si es entero sin
/// guardar el lexema (`docs/fase4/automata.md` §3.3).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Number {
    /// Alguna cifra de la mantisa (parte entera y fracción) no es 0.
    nonzero: bool,
    /// Ceros finales de la mantisa, saturado.
    zeros: u64,
    /// Cifras de la fracción, saturado.
    frac: u64,
    /// Valor absoluto del exponente, saturado en [`EXP_CAP`].
    exp: u64,
    exp_neg: bool,
}

/// Tope del exponente: mayor que cualquier número de cifras alcanzable, así
/// que saturar no cambia la respuesta de [`Number::is_integer`].
const EXP_CAP: u64 = 1 << 62;

impl Number {
    pub(crate) fn byte(&mut self, part: NumPart, b: u8) {
        let d = u64::from(b.wrapping_sub(b'0'));
        match part {
            NumPart::Int | NumPart::Frac => {
                if part == NumPart::Frac {
                    self.frac = self.frac.saturating_add(1);
                }
                if d == 0 {
                    self.zeros = self.zeros.saturating_add(1);
                } else {
                    self.nonzero = true;
                    self.zeros = 0;
                }
            }
            NumPart::ExpSign => self.exp_neg = b == b'-',
            NumPart::Exp => {
                self.exp = if self.exp > EXP_CAP / 10 {
                    EXP_CAP
                } else {
                    (self.exp * 10 + d).min(EXP_CAP)
                }
            }
            NumPart::Minus | NumPart::Dot | NumPart::ExpMark => {}
        }
    }

    /// Mantisa D (cifras de la parte entera y la fracción), valor
    /// D · 10^(e − |fracción|): es entero si D = 0 o si e + ceros finales de
    /// D ≥ |fracción|.
    pub(crate) fn is_integer(&self) -> bool {
        if !self.nonzero {
            return true;
        }
        let e = if self.exp_neg {
            -i128::from(self.exp)
        } else {
            i128::from(self.exp)
        };
        e + i128::from(self.zeros) >= i128::from(self.frac)
    }
}

/// [`Number::is_integer`] sobre un lexema entero, con la misma partición en
/// partes que hace el lexer (para pruebas y herramientas).
pub fn is_integer(lexeme: &[u8]) -> bool {
    let mut n = Number::default();
    let mut part = NumPart::Minus;
    let mut first = true;
    for &b in lexeme {
        part = match (part, b) {
            (_, b'-') if first => NumPart::Minus,
            (_, b'.') => NumPart::Dot,
            (_, b'e' | b'E') => NumPart::ExpMark,
            (NumPart::ExpMark, b'+' | b'-') => NumPart::ExpSign,
            (NumPart::ExpMark | NumPart::ExpSign | NumPart::Exp, _) => NumPart::Exp,
            (NumPart::Dot | NumPart::Frac, _) => NumPart::Frac,
            _ => NumPart::Int,
        };
        first = false;
        n.byte(part, b);
    }
    n.is_integer()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponente_satura() {
        let mut n = Number::default();
        for (p, b) in [
            (NumPart::Int, b'1'),
            (NumPart::Dot, b'.'),
            (NumPart::Frac, b'5'),
            (NumPart::ExpMark, b'e'),
        ] {
            n.byte(p, b);
        }
        for _ in 0..100 {
            n.byte(NumPart::Exp, b'9');
        }
        assert_eq!(n.exp, EXP_CAP);
        assert!(n.is_integer());
        n.exp_neg = true;
        assert!(!n.is_integer());
    }

    #[test]
    fn diferencia_de_exponentes_saturada() {
        assert_eq!(exp_diff(false, b"5", true, b"3"), 8);
        assert_eq!(exp_diff(false, b"0007", false, b"7"), 0);
        let big = [b'9'; 40];
        assert_eq!(exp_diff(false, &big, false, b"1"), CAP);
        assert_eq!(exp_diff(true, &big, false, b"1"), -CAP);
        let mut casi = big;
        casi[39] = b'8';
        assert_eq!(exp_diff(false, &big, false, &casi), 1);
        assert_eq!(exp_diff(true, &big, true, &casi), -1);
    }
}
