//! Textos XML (F4.1c; `docs/fase4/automata.md` §14): los espacios léxicos
//! de los tipos predefinidos como [`Dfa`] (con el transductor de
//! *collapse* delante en todos salvo `xs:string`), los umbrales decimales
//! exactos de las cotas IEEE de `xs:float`/`xs:double` (5.5-CR-S1) y el AFD
//! del contenido de la declaración `<?xml …?>`.
//!
//! Cada estado de un AFD de texto lleva una salida [`TextOut`] que dice qué
//! valor ha leído: el constructor decide con ella, **al compilar**, las
//! comprobaciones del cierre del texto en ese estado (un `NaN` viola toda
//! faceta de rango, un `INF` cumple los mínimos y viola los máximos finitos;
//! 5.5-P3), y deja para el núcleo solo las que leen los registros del texto
//! (el lexema y el contador de puntos de código).

use ir::Lexical;

use crate::dfa::{collapse, Dfa, DfaBuilder};

/// Sin palabra literal ([`TextOut::Str`]).
pub(crate) const NO_WORD: u32 = u32::MAX;

/// Lo que ha leído un AFD de texto al llegar a un estado.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum TextOut {
    /// Fuera del espacio léxico.
    #[default]
    Reject,
    /// Una cadena: la palabra literal `i` del trie (o [`NO_WORD`]).
    Str(u32),
    /// `xs:boolean`: `true`/`1` o `false`/`0`.
    Bool(bool),
    /// Un número finito (en `xs:float`/`xs:double`, uno decimal: su valor
    /// IEEE se decide con los umbrales).
    Num,
    /// `INF` (false) o `-INF` (true).
    Inf(bool),
    NaN,
    /// La declaración `<?xml …?>` completa.
    Decl,
}

/// Los cuatro espacios de XML (`S`).
pub(crate) const WS: [char; 4] = [' ', '\t', '\n', '\r'];

/// Caracteres que distingue el espacio léxico de `l` (además de los
/// espacios, en los que pasan por *collapse*).
pub(crate) fn lexical_chars(l: Lexical) -> &'static str {
    match l {
        Lexical::String => "",
        Lexical::Boolean => "truefals10",
        Lexical::Integer => "0123456789+-",
        Lexical::Decimal => "0123456789+-.",
        Lexical::Float | Lexical::Double => "0123456789+-.eEINFa",
    }
}

/// Caracteres del AFD de la declaración `<?xml …?>` y del nombre de las
/// instrucciones de procesamiento (`xml` reservado en cualquier caja).
pub(crate) const DECL_CHARS: &str = "versionencodingstandaloneyesUTF-8utf0123456789.=\"'xXmMlL";

fn cls(class_of: &dyn Fn(char) -> usize, s: &str) -> Vec<usize> {
    s.chars().map(class_of).collect()
}

/// El AFD del espacio léxico de un tipo numérico o booleano, sin
/// *collapse* (XSD 1.0 Parte 2 §3.2.2-§3.2.5): `xs:integer` `[+-]?[0-9]+`;
/// `xs:decimal` `[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)`; `xs:float`/`xs:double`
/// la mantisa de `xs:decimal` con exponente `[eE][+-]?[0-9]+` opcional, o
/// `INF`, `-INF`, `NaN` (`+INF` no es de XSD 1.0, D24); `xs:boolean`
/// `true|false|1|0`.
fn lexical_raw(l: Lexical, n: usize, class_of: &dyn Fn(char) -> usize) -> Dfa<TextOut> {
    let mut b = DfaBuilder::new(n);
    let digits = cls(class_of, "0123456789");
    match l {
        Lexical::Boolean => {
            let s0 = b.state(TextOut::Reject);
            let one = b.state(TextOut::Bool(true));
            let zero = b.state(TextOut::Bool(false));
            b.on(s0, &cls(class_of, "1"), one);
            b.on(s0, &cls(class_of, "0"), zero);
            for (w, v) in [("true", true), ("false", false)] {
                let mut s = s0;
                let chars: Vec<char> = w.chars().collect();
                for (i, &c) in chars.iter().enumerate() {
                    let o = if i + 1 == chars.len() {
                        TextOut::Bool(v)
                    } else {
                        TextOut::Reject
                    };
                    let t = b.state(o);
                    b.on(s, &[class_of(c)], t);
                    s = t;
                }
            }
            b.finish(s0)
        }
        _ => {
            let int = l == Lexical::Integer;
            let float = matches!(l, Lexical::Float | Lexical::Double);
            let s0 = b.state(TextOut::Reject);
            let plus = b.state(TextOut::Reject);
            let minus = b.state(TextOut::Reject);
            let ip = b.state(TextOut::Num); // cifras de la parte entera
            b.on(s0, &cls(class_of, "+"), plus);
            b.on(s0, &cls(class_of, "-"), minus);
            for s in [s0, plus, minus, ip] {
                b.on(s, &digits, ip);
            }
            let mut mant = vec![ip];
            if !int {
                let dot = b.state(TextOut::Reject); // `.` sin cifras antes
                let ipd = b.state(TextOut::Num); // `1.`
                let fr = b.state(TextOut::Num); // cifras de la fracción
                for s in [s0, plus, minus] {
                    b.on(s, &cls(class_of, "."), dot);
                }
                b.on(ip, &cls(class_of, "."), ipd);
                for s in [dot, ipd, fr] {
                    b.on(s, &digits, fr);
                }
                mant.extend([ipd, fr]);
            }
            if float {
                let e = b.state(TextOut::Reject);
                let es = b.state(TextOut::Reject);
                let ed = b.state(TextOut::Num);
                for &m in &mant {
                    b.on(m, &cls(class_of, "eE"), e);
                }
                b.on(e, &cls(class_of, "+-"), es);
                for s in [e, es, ed] {
                    b.on(s, &digits, ed);
                }
                // INF, -INF, NaN.
                for (from, neg) in [(s0, false), (minus, true)] {
                    let i = b.state(TextOut::Reject);
                    let n_ = b.state(TextOut::Reject);
                    let f = b.state(TextOut::Inf(neg));
                    b.on(from, &cls(class_of, "I"), i);
                    b.on(i, &cls(class_of, "N"), n_);
                    b.on(n_, &cls(class_of, "F"), f);
                }
                let n1 = b.state(TextOut::Reject);
                let n2 = b.state(TextOut::Reject);
                let nan = b.state(TextOut::NaN);
                b.on(s0, &cls(class_of, "N"), n1);
                b.on(n1, &cls(class_of, "a"), n2);
                b.on(n2, &cls(class_of, "N"), nan);
            }
            b.finish(s0)
        }
    }
}

/// El AFD de un texto: para `xs:string` (*whiteSpace* `preserve`), el trie
/// de las palabras literales (`enum` ∪ `fixed`) con todos los estados
/// aceptadores, o un solo estado si no hay; para los demás, su espacio
/// léxico con el transductor de *collapse* delante.
pub(crate) fn text_dfa(
    l: Lexical,
    words: &[Vec<usize>],
    n: usize,
    class_of: &dyn Fn(char) -> usize,
) -> Dfa<TextOut> {
    match l {
        Lexical::String if words.is_empty() => Dfa {
            n_classes: n,
            start: 0,
            trans: vec![0; n],
            out: vec![TextOut::Str(NO_WORD)],
        },
        Lexical::String => {
            let t = Dfa::trie(n, words);
            Dfa {
                n_classes: n,
                start: t.start,
                trans: t.trans,
                out: t
                    .out
                    .iter()
                    .map(|o| TextOut::Str(o.unwrap_or(NO_WORD)))
                    .collect(),
            }
        }
        _ => {
            let ws = cls(class_of, &WS.iter().collect::<String>());
            collapse(&lexical_raw(l, n, class_of), &ws, class_of(' '))
        }
    }
}

/// Estados de [`text_dfa`] de un tipo no `xs:string` (fijo: no depende de
/// las clases, porque cada carácter que distingue tiene la suya).
pub(crate) fn lexical_size(l: Lexical) -> usize {
    let chars: Vec<char> = WS.iter().copied().chain(lexical_chars(l).chars()).collect();
    let class_of = |c: char| chars.iter().position(|&x| x == c).map_or(0, |i| i + 1);
    text_dfa(l, &[], chars.len() + 1, &class_of).n_states()
}

/// El AFD del contenido de la declaración `<?xml …?>` tras el nombre (el
/// lexer ya descartó el primer espacio): `S? version Eq Q 1.[0-9]+ Q
/// (S encoding Eq Q UTF-8 Q)? (S standalone Eq Q (yes|no) Q)? S?`, con
/// `Eq` = `S? = S?`, la misma comilla a los dos lados y `UTF-8` en
/// cualquier caja (XML 1.0 §2.8, §4.3.3; `docs/00-alcance.md` §1.2: otra
/// codificación es un error).
pub(crate) fn decl_dfa(n: usize, class_of: &dyn Fn(char) -> usize) -> Dfa<TextOut> {
    let mut b = DfaBuilder::new(n);
    let ws = cls(class_of, &WS.iter().collect::<String>());
    let word = |b: &mut DfaBuilder<TextOut>, from: u32, w: &str| -> u32 {
        let mut s = from;
        for c in w.chars() {
            let t = b.state(TextOut::Reject);
            b.on(s, &[class_of(c)], t);
            s = t;
        }
        s
    };
    // `Eq` y la comilla: devuelve (estado tras `"`, estado tras `'`).
    let eq = |b: &mut DfaBuilder<TextOut>, from: u32| -> (u32, u32) {
        let pre = b.state(TextOut::Reject);
        b.on(from, &ws, pre);
        b.on(pre, &ws, pre);
        let post = b.state(TextOut::Reject);
        b.on(from, &cls(class_of, "="), post);
        b.on(pre, &cls(class_of, "="), post);
        b.on(post, &ws, post);
        let dq = b.state(TextOut::Reject);
        let sq = b.state(TextOut::Reject);
        b.on(post, &cls(class_of, "\""), dq);
        b.on(post, &cls(class_of, "'"), sq);
        (dq, sq)
    };
    // Tras cada parte: aceptador, con espacios opcionales al final; desde
    // los espacios puede empezar la parte siguiente.
    let after = |b: &mut DfaBuilder<TextOut>| -> (u32, u32) {
        let a = b.state(TextOut::Decl);
        let s = b.state(TextOut::Decl);
        b.on(a, &ws, s);
        b.on(s, &ws, s);
        (a, s)
    };
    let s0 = b.state(TextOut::Reject);
    b.on(s0, &ws, s0);
    let v = word(&mut b, s0, "version");
    let (dq, sq) = eq(&mut b, v);
    let (av, av_s) = after(&mut b);
    for (q, quote) in [(dq, "\""), (sq, "'")] {
        let one = word(&mut b, q, "1.");
        let d = b.state(TextOut::Reject);
        b.on(one, &cls(class_of, "0123456789"), d);
        b.on(d, &cls(class_of, "0123456789"), d);
        b.on(d, &cls(class_of, quote), av);
    }
    // encoding (opcional), desde los espacios tras version.
    let e = word(&mut b, av_s, "encoding");
    let (dq, sq) = eq(&mut b, e);
    let (ae, ae_s) = after(&mut b);
    for (q, quote) in [(dq, "\""), (sq, "'")] {
        let mut s = q;
        for (lo, up) in [("u", "U"), ("t", "T"), ("f", "F"), ("-", "-"), ("8", "8")] {
            let t = b.state(TextOut::Reject);
            b.on(s, &cls(class_of, &format!("{lo}{up}")), t);
            s = t;
        }
        b.on(s, &cls(class_of, quote), ae);
    }
    // standalone (opcional), tras version o tras encoding.
    let (asd, _) = after(&mut b);
    for from in [av_s, ae_s] {
        let st = word(&mut b, from, "standalone");
        let (dq, sq) = eq(&mut b, st);
        for (q, quote) in [(dq, "\""), (sq, "'")] {
            for w in ["yes", "no"] {
                let s = word(&mut b, q, w);
                b.on(s, &cls(class_of, quote), asd);
            }
        }
    }
    b.finish(s0)
}

// ------------------------------------------------- umbrales IEEE ---

/// El intervalo de redondeo de un valor F de `xs:float`/`xs:double`: los
/// números decimales p con round(p) = F (al par más cercano) son los de
/// [m⁻, m⁺] si la mantisa de F es par y los de (m⁻, m⁺) si es impar, con
/// m± los puntos medios con los vecinos (`docs/fase3/numeros-xsd.md` §6).
/// Los dos extremos son racionales diádicos y se escriben como lexemas
/// decimales **exactos** (a lo sumo 768 cifras significativas), así que el
/// comparador decimal del núcleo decide sin flotantes (5.5-CR-S1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rounding {
    pub(crate) lo: String,
    pub(crate) hi: String,
    pub(crate) closed: bool,
}

/// (mantisa, exponente) con |F| = mantisa · 2^exponente, y si F es
/// negativo; `None` si F no es finito.
fn decompose(lexeme: &str, double: bool) -> Option<(u64, i32, bool)> {
    if double {
        let f: f64 = lexeme.parse().ok()?;
        if !f.is_finite() {
            return None;
        }
        let bits = f.to_bits();
        let e = ((bits >> 52) & 0x7ff) as i32;
        let frac = bits & ((1 << 52) - 1);
        let neg = f.is_sign_negative() && f != 0.0;
        Some(if e == 0 {
            (frac, -1074, neg)
        } else {
            (frac | (1 << 52), e - 1075, neg)
        })
    } else {
        let f: f32 = lexeme.parse().ok()?;
        if !f.is_finite() {
            return None;
        }
        let bits = f.to_bits();
        let e = ((bits >> 23) & 0xff) as i32;
        let frac = u64::from(bits & ((1 << 23) - 1));
        let neg = f.is_sign_negative() && f != 0.0;
        Some(if e == 0 {
            (frac, -149, neg)
        } else {
            (frac | (1 << 23), e - 150, neg)
        })
    }
}

/// El intervalo de redondeo del valor del lexema `f` (el decimal más corto
/// de un flotante finito, como lo guarda `ir::num::xsd_number`).
pub(crate) fn rounding(f: &str, double: bool) -> Option<Rounding> {
    let (m, e, neg) = decompose(f, double)?;
    let min_normal: u64 = if double { 1 << 52 } else { 1 << 23 };
    let subnormal_exp = if double { -1074 } else { -149 };
    // m⁺ = (2m + 1) · 2^(e − 1): el vecino superior vale (m + 1) · 2^e
    // aunque cambie de binada (y en el máximo finito es el umbral de ∞).
    let hi = dyadic(2 * m + 1, e - 1);
    if m == 0 {
        // El cero (uno solo, también −0): simétrico y cerrado.
        return Some(Rounding {
            lo: format!("-{hi}"),
            hi,
            closed: true,
        });
    }
    // m⁻: el vecino inferior tiene el mismo exponente salvo en una potencia
    // de 2 normal que no es la menor, donde tiene uno menos.
    let lo = if m == min_normal && e > subnormal_exp {
        dyadic(4 * m - 1, e - 2)
    } else {
        dyadic(2 * m - 1, e - 1)
    };
    let closed = m % 2 == 0;
    Some(if neg {
        Rounding {
            lo: format!("-{hi}"),
            hi: format!("-{lo}"),
            closed,
        }
    } else {
        Rounding { lo, hi, closed }
    })
}

/// El umbral de desborde positivo: el menor decimal que redondea a +∞ (el
/// punto medio entre el máximo finito y 2^128 o 2^1024, que empata al par,
/// ∞; `numeros-xsd.md` §6).
pub(crate) fn overflow(double: bool) -> String {
    let max = if double {
        f64::MAX.to_string()
    } else {
        f32::MAX.to_string()
    };
    rounding(&max, double).map(|r| r.hi).unwrap_or_default()
}

/// k · 2^e como lexema decimal exacto de RFC 8259 (k ≥ 1): k · 2^e si
/// e ≥ 0; k · 5^(−e) seguido de `e−…` si e < 0.
fn dyadic(k: u64, e: i32) -> String {
    let mut big: Vec<u32> = vec![k as u32, (k >> 32) as u32];
    let mul = |big: &mut Vec<u32>, x: u32| {
        let mut carry = 0u64;
        for d in big.iter_mut() {
            let v = u64::from(*d) * u64::from(x) + carry;
            *d = v as u32;
            carry = v >> 32;
        }
        if carry > 0 {
            big.push(carry as u32);
        }
    };
    let (times, by) = if e >= 0 { (e, 2) } else { (-e, 5) };
    for _ in 0..times {
        mul(&mut big, by);
    }
    // A decimal, de 10⁹ en 10⁹.
    let mut parts = Vec::new();
    while big.iter().any(|&d| d != 0) {
        let mut rem = 0u64;
        for d in big.iter_mut().rev() {
            let v = (rem << 32) | u64::from(*d);
            *d = (v / 1_000_000_000) as u32;
            rem = v % 1_000_000_000;
        }
        parts.push(rem as u32);
        while big.last() == Some(&0) {
            big.pop();
        }
    }
    let mut s = String::new();
    for (i, p) in parts.iter().rev().enumerate() {
        if i == 0 {
            s.push_str(&p.to_string());
        } else {
            s.push_str(&format!("{p:09}"));
        }
    }
    if s.is_empty() {
        s.push('0');
    }
    if e < 0 {
        s.push_str(&format!("e{e}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Los extremos son exactos: el punto medio redondea al par y un
    /// decimal apenas por dentro redondea a F (comprobado con `std`, que
    /// redondea bien con exponentes cortos).
    #[test]
    fn intervalos_de_redondeo() {
        for f in [
            "0.1",
            "1",
            "3.4028235e38",
            "1e-45",
            "0",
            "-0.1",
            "16777216",
            "1.1754944e-38",
        ] {
            let r = rounding(f, false).unwrap();
            let v: f32 = f.parse().unwrap();
            let lo: f32 = r.lo.parse().unwrap();
            let hi: f32 = r.hi.parse().unwrap();
            assert_eq!(lo == v, r.closed, "{f}: {r:?}");
            assert_eq!(hi == v, r.closed, "{f}: {r:?}");
        }
        for f in [
            "0.1",
            "1",
            "1.7976931348623157e308",
            "5e-324",
            "-2",
            "2.2250738585072014e-308",
        ] {
            let r = rounding(f, true).unwrap();
            let v: f64 = f.parse().unwrap();
            let lo: f64 = r.lo.parse().unwrap();
            let hi: f64 = r.hi.parse().unwrap();
            assert_eq!(lo == v, r.closed, "{f}: {r:?}");
            assert_eq!(hi == v, r.closed, "{f}: {r:?}");
        }
        assert_eq!(dyadic(3, 2), "12");
        assert_eq!(dyadic(1, -1), "5e-1");
        assert_eq!(overflow(false), "340282356779733661637539395458142568448");
    }
}
