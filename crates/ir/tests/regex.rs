//! F4.4a: el analizador de `pattern` (`ir::regex`) en los dos dialectos:
//! qué acepta, qué rechaza (con la clase y el carácter del constructo) y
//! `matches` como oráculo.

use ir::regex::{parse, Dialect, RegexErrorKind as K};
use proptest::prelude::*;

use Dialect::{Ecma, Xsd};

/// (dialecto, patrón, `None` si se acepta o `Some((clase, at))`).
#[allow(clippy::type_complexity)]
const TABLA: &[(Dialect, &str, Option<(K, usize)>)] = &[
    // ECMA acepta.
    (Ecma, r"^[A-Z]{3}-\d{4}$", None),
    (Ecma, r"^(?:a|b)+$", None),
    (Ecma, r"[^\n]", None),
    (Ecma, "é", None),
    (Ecma, r"^a|b$", None),
    (Ecma, r"a{2,}", None),
    (Ecma, r"[a-z-]", None),
    (Ecma, r"é\x41\t", None),
    (Ecma, r"😀", None),
    (Ecma, r"", None),
    (Ecma, r"()", None),
    // ECMA rechaza.
    (Ecma, r"(a)\1", Some((K::Unsupported, 3))),
    (Ecma, r"\k<n>", Some((K::Unsupported, 0))),
    (Ecma, r"(?=a)", Some((K::Unsupported, 0))),
    (Ecma, r"(?!a)", Some((K::Unsupported, 0))),
    (Ecma, r"(?<=a)", Some((K::Unsupported, 0))),
    (Ecma, r"(?<!a)", Some((K::Unsupported, 0))),
    (Ecma, r"a\b", Some((K::Unsupported, 1))),
    (Ecma, r"a*?", Some((K::Unsupported, 1))),
    (Ecma, r"a{2,}?", Some((K::Unsupported, 1))),
    (Ecma, r"(?<n>a)", Some((K::Unsupported, 0))),
    (Ecma, r"\p{L}", Some((K::Unsupported, 0))),
    (Ecma, r"a^b", Some((K::Unsupported, 1))),
    (Ecma, r"a$b", Some((K::Unsupported, 1))),
    (Ecma, r"(^a)", Some((K::Unsupported, 1))),
    (Ecma, r"[z-a]", Some((K::Malformed, 1))),
    (Ecma, r"(a", Some((K::Malformed, 0))),
    (Ecma, r"a)", Some((K::Malformed, 1))),
    (Ecma, r"a{3,2}", Some((K::Malformed, 1))),
    (Ecma, r"*a", Some((K::Malformed, 0))),
    (Ecma, r"a**", Some((K::Malformed, 1))),
    (Ecma, r"\u{1F600}", Some((K::Unsupported, 0))),
    (Ecma, r"\cA", Some((K::Unsupported, 0))),
    (Ecma, r"a{999999999}", Some((K::Limit, 0))),
    (Ecma, r"\q", Some((K::Malformed, 0))),
    // XSD acepta.
    (Xsd, r"[A-Z]{3}", None),
    (Xsd, r"X+L", None),
    (Xsd, r"^a$", None),
    (Xsd, r"\d{3}-\s\S", None),
    (Xsd, r"[\-\[\]]", None),
    // XSD rechaza.
    (Xsd, r"\w", Some((K::Unsupported, 0))),
    (Xsd, r"\i", Some((K::Unsupported, 0))),
    (Xsd, r"\p{Nd}", Some((K::Unsupported, 0))),
    (Xsd, r"\D", Some((K::Unsupported, 0))),
    (Xsd, r"[a-z-[aeiou]]", Some((K::Unsupported, 4))),
    (Xsd, r"(?:a)", Some((K::Malformed, 0))),
    (Xsd, r"a*?", Some((K::Malformed, 1))),
    (Xsd, r"(a", Some((K::Malformed, 0))),
];

#[test]
fn tabla_de_patrones() {
    assert!(TABLA.len() >= 30);
    for &(d, p, esperado) in TABLA {
        let r = parse(p, d);
        match (esperado, r) {
            (None, Ok(_)) => {}
            (Some((k, at)), Err(e)) => {
                assert_eq!((e.kind, e.at), (k, at), "{d:?} {p:?}: {e}");
                assert!(!e.reason.is_empty());
            }
            (esperado, r) => panic!("{d:?} {p:?}: esperado {esperado:?}, obtenido {r:?}"),
        }
    }
}

/// (dialecto, patrón, cadena, ¿la cumple?).
const COINCIDENCIAS: &[(Dialect, &str, &str, bool)] = &[
    // Anclaje según el dialecto.
    (Ecma, "a", "xay", true),
    (Xsd, "a", "xay", false),
    (Xsd, "a", "a", true),
    (Ecma, "^a", "ba", false),
    (Ecma, "^a", "ab", true),
    (Ecma, "a$", "ba", true),
    (Ecma, "a$", "ab", false),
    (Ecma, "^a|b$", "xa", false),
    (Ecma, "^a|b$", "xb", true),
    (Ecma, "^a|b$", "ax", true),
    (Ecma, "^$", "", true),
    (Ecma, "^$", "x", false),
    (Ecma, "", "cualquiera", true),
    (Xsd, "", "", true),
    (Xsd, "", "x", false),
    (Xsd, "^a$", "^a$", true),
    (Xsd, "^a$", "a", false),
    // `.` frente a fines de línea y a un astral (un carácter).
    (Ecma, "^.$", "\n", false),
    (Ecma, "^.$", "\u{2028}", false),
    (Xsd, ".", "\u{2028}", true),
    (Xsd, ".", "\r", false),
    (Ecma, "^.$", "😀", true),
    (Xsd, "..", "😀", false),
    (Ecma, r"^😀$", "😀", true),
    // Clases negadas.
    (Ecma, "^[^a-c]+$", "xyz", true),
    (Ecma, "^[^a-c]+$", "xbz", false),
    (Xsd, "[^\\n]*", "a b", true),
    (Xsd, "[^\\n]*", "a\nb", false),
    // {m,n}.
    (Xsd, "a{2,3}", "a", false),
    (Xsd, "a{2,3}", "aa", true),
    (Xsd, "a{2,3}", "aaa", true),
    (Xsd, "a{2,3}", "aaaa", false),
    (Xsd, "(ab){2,}", "abab", true),
    (Xsd, "(ab){2,}", "ababab", true),
    (Xsd, "(ab){2,}", "ab", false),
    (Xsd, "a{0}", "", true),
    (Xsd, "(a|b)*a(a|b){2}", "bbabb", true),
    (Xsd, "(a|b)*a(a|b){2}", "bbbab", false),
    (Ecma, r"^[A-Z]{3}-\d{4}$", "ABC-1234", true),
    (Ecma, r"^[A-Z]{3}-\d{4}$", "ABC-123", false),
    (Ecma, r"^(a+)+$", "aaaa", true),
    (Ecma, r"^(a+)+$", "aaaa!", false),
    (Xsd, r"\d", "٣", false),
    (Ecma, r"^\s$", "\u{a0}", true),
    (Xsd, r"\s", "\u{a0}", false),
];

#[test]
fn tabla_de_coincidencias() {
    for &(d, p, s, esperado) in COINCIDENCIAS {
        let r = parse(p, d).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert_eq!(r.matches(s), esperado, "{d:?} {p:?} sobre {s:?}");
    }
}

#[test]
fn posiciones_expandidas() {
    assert_eq!(parse("a{2,3}", Xsd).unwrap().positions(), 3);
    assert_eq!(parse("(ab){3,}", Xsd).unwrap().positions(), 6);
    assert_eq!(parse("a{4096}", Xsd).unwrap().positions(), 4096);
    assert_eq!(parse("a{4097}", Xsd).unwrap_err().kind, K::Limit);
    assert_eq!(parse("(a{64}){64}", Xsd).unwrap().positions(), 4096);
    assert_eq!(parse("((a{65}){64})", Xsd).unwrap_err().kind, K::Limit);
}

/// D7: 10⁵ `(` en un hilo de 256 KiB dan un error (`limit` en el 257.º),
/// no un desbordamiento; con 256 grupos anidados se acepta.
#[test]
fn anidamiento_en_poca_pila() {
    let r = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            let p = "(".repeat(100_000);
            let e = parse(&p, Ecma).unwrap_err();
            let ok = format!("{}a{}", "(".repeat(256), ")".repeat(256));
            (e, parse(&ok, Ecma).map(|r| r.matches("a")))
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!((r.0.kind, r.0.at), (K::Limit, 256));
    assert_eq!(r.1, Ok(true));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Sin pánico con patrones y cadenas arbitrarios en los dos dialectos.
    #[test]
    fn sin_panico(p in "[ab()|*+?{}0-9,\\[\\]^$.\\\\dsSwnu-]{0,24}", s in "[ab\\n0-9 ]{0,12}") {
        for d in [Ecma, Xsd] {
            if let Ok(r) = parse(&p, d) {
                let _ = r.matches(&s);
            }
        }
    }

    /// `matches` (XSD, anclado) frente a un oráculo de retroceso ingenuo
    /// sobre patrones pequeños: literales, clases, alternancia, grupos y
    /// cuantificadores.
    #[test]
    fn coincide_con_un_oraculo_de_retroceso(p in "([ab]|\\[ab\\]|\\(a\\|b\\)|\\(ab\\)|\\(a\\{1,2\\}\\))([*+?]?)([ab]|\\(a\\|bb\\))([*+?]?)", s in "[ab]{0,8}") {
        let nuestro = parse(&p, Xsd).unwrap();
        let suyo = regex_lite(&p);
        prop_assert_eq!(nuestro.matches(&s), suyo(&s), "{} sobre {}", p, s);
    }
}

/// Oráculo con backtracking ingenuo, suficiente para los patrones pequeños
/// de la prueba anterior (sin dependencias nuevas).
fn regex_lite(p: &str) -> impl Fn(&str) -> bool {
    // Traducción a un AST mínimo y búsqueda exhaustiva de particiones.
    #[derive(Clone, Debug)]
    enum R {
        Lit(Vec<char>),
        Seq(Vec<R>),
        Alt(Vec<R>),
        Rep(Box<R>, usize, Option<usize>),
    }
    fn parse_seq(cs: &[char], i: &mut usize) -> R {
        let mut alts = vec![];
        let mut seq = vec![];
        while *i < cs.len() {
            let c = cs[*i];
            match c {
                ')' => break,
                '|' => {
                    *i += 1;
                    alts.push(R::Seq(std::mem::take(&mut seq)));
                }
                '(' => {
                    *i += 1;
                    let r = parse_seq(cs, i);
                    *i += 1;
                    seq.push(r);
                }
                '[' => {
                    let mut v = vec![];
                    *i += 1;
                    while cs[*i] != ']' {
                        v.push(cs[*i]);
                        *i += 1;
                    }
                    *i += 1;
                    seq.push(R::Lit(v));
                }
                '*' | '+' | '?' => {
                    *i += 1;
                    let x = seq.pop().unwrap();
                    let (a, b) = match c {
                        '*' => (0, None),
                        '+' => (1, None),
                        _ => (0, Some(1)),
                    };
                    seq.push(R::Rep(Box::new(x), a, b));
                }
                '{' => {
                    // Solo `{1,2}`.
                    *i += 5;
                    let x = seq.pop().unwrap();
                    seq.push(R::Rep(Box::new(x), 1, Some(2)));
                }
                _ => {
                    *i += 1;
                    seq.push(R::Lit(vec![c]));
                }
            }
        }
        alts.push(R::Seq(seq));
        R::Alt(alts)
    }
    fn m(r: &R, s: &[char]) -> Vec<usize> {
        // Longitudes que `r` puede consumir desde el principio de `s`.
        match r {
            R::Lit(v) => {
                if s.first().is_some_and(|c| v.contains(c)) {
                    vec![1]
                } else {
                    vec![]
                }
            }
            R::Seq(v) => {
                let mut ends = vec![0];
                for x in v {
                    let mut nuevo = vec![];
                    for &e in &ends {
                        for l in m(x, &s[e..]) {
                            nuevo.push(e + l);
                        }
                    }
                    nuevo.sort_unstable();
                    nuevo.dedup();
                    ends = nuevo;
                }
                ends
            }
            R::Alt(v) => {
                let mut o: Vec<usize> = v.iter().flat_map(|x| m(x, s)).collect();
                o.sort_unstable();
                o.dedup();
                o
            }
            R::Rep(x, a, b) => {
                let mut res = vec![];
                let mut ends = vec![0usize];
                let mut k = 0;
                loop {
                    if k >= *a {
                        res.extend(ends.iter().copied());
                    }
                    if b.is_some_and(|b| k >= b) || ends.is_empty() || k > s.len() + 1 {
                        break;
                    }
                    let mut nuevo = vec![];
                    for &e in &ends {
                        for l in m(x, &s[e..]) {
                            if l > 0 || b.is_some() {
                                nuevo.push(e + l);
                            }
                        }
                    }
                    nuevo.sort_unstable();
                    nuevo.dedup();
                    ends = nuevo;
                    k += 1;
                }
                res.sort_unstable();
                res.dedup();
                res
            }
        }
    }
    let cs: Vec<char> = p.chars().collect();
    let mut i = 0;
    let r = parse_seq(&cs, &mut i);
    move |s: &str| {
        let v: Vec<char> = s.chars().collect();
        m(&r, &v).contains(&v.len())
    }
}
