//! Números de XSD (tarea 5.5, `docs/fase3/numeros-xsd.md`): forma léxica de
//! `xs:integer`, `xs:decimal`, `xs:float`/`xs:double` y `xs:boolean` con
//! *collapse*; reescritura a un lexema de RFC 8259 que `Ir::verify` acepta
//! (I8) y tiene el mismo valor; comparación decimal exacta contra un oráculo
//! de números racionales (y antisimétrica y transitiva con exponentes de
//! cualquier longitud); redondeo IEEE de `xs:float`/`xs:double` y el rechazo
//! de D24 con su motivo; exponentes y fracciones de 10⁵ cifras sin pánico.

use std::cmp::Ordering;

use ir::num::{
    compare, compare_parsed, is_rfc8259_number, parse_boolean, rewrite, round_f32, round_f64,
    xsd_number, NumError, NumErrorKind,
};
use ir::{
    Constraint, ContentModel, Format, Ir, Lexical, LitToken, Literal, Node, Num, Occurs, Origin,
    Particle, Pos, Rule, Sort, Span, Step, Term,
};
use proptest::prelude::*;

use Lexical::{Boolean, Decimal, Double, Float, Integer};
use NumErrorKind::{Empty, Exponent, Fraction, NotNumeric, Overflow, Special, Syntax};

// --- Ayudas ----------------------------------------------------------------

#[track_caller]
fn acepta(lx: Lexical, valor: &str, esperado: &str) {
    match xsd_number(lx, valor) {
        Ok(n) => assert_eq!(n.lexeme, esperado, "{lx:?} {valor:?}"),
        Err(e) => panic!("{lx:?} {valor:?}: {e}"),
    }
}

#[track_caller]
fn rechaza(lx: Lexical, valor: &str, kind: NumErrorKind) {
    assert_eq!(
        xsd_number(lx, valor),
        Err(NumError { lexical: lx, kind }),
        "{lx:?} {valor:?}"
    );
}

fn sp() -> Span {
    Span {
        start: Pos::START,
        end: 0,
    }
}

fn con_span(rules: Vec<Rule>) -> Vec<Constraint> {
    rules
        .into_iter()
        .map(|rule| Constraint { rule, span: sp() })
        .collect()
}

/// IR de XML con un texto `lx` que usa `n` como cota inferior, como cota
/// exclusiva superior y como único valor de una enumeración: lo que hará la
/// 5.6 con una faceta o una `xs:enumeration` reescrita.
fn ir_con_texto(lx: Lexical, n: &Num) -> Ir {
    let mut ir = Ir::new(
        Format::Xml {
            target_namespace: None,
        },
        Node {
            sort: Sort::Element,
            rules: con_span(vec![Rule::Content(ContentModel {
                particles: vec![Particle {
                    term: Term::Choice(vec![]),
                    occurs: Occurs::ONCE,
                    span: sp(),
                }],
            })]),
            origin: Origin {
                span: sp(),
                parent: None,
                step: Step::Root,
            },
        },
    );
    ir.push(Node {
        sort: Sort::Text,
        rules: con_span(vec![
            Rule::Lexical(lx),
            Rule::Minimum(n.clone()),
            Rule::ExclusiveMaximum(n.clone()),
            Rule::Enum(vec![Literal(vec![LitToken::Number(n.lexeme.clone())])]),
        ]),
        origin: Origin {
            span: sp(),
            parent: Some(ir::NodeId::ROOT),
            step: Step::Entry("simpleType".into(), "T".into()),
        },
    });
    ir
}

/// Oráculo: el valor racional m × 10^s de un lexema de RFC 8259 pequeño
/// (mantisa de ≤ 18 cifras, exponente de ≤ 4), leído sin `ir::num`.
fn racional(lex: &str) -> (i128, i32) {
    let (mant, exp) = match lex.find(['e', 'E']) {
        Some(i) => (&lex[..i], lex[i + 1..].parse::<i32>().unwrap()),
        None => (lex, 0),
    };
    let (neg, mant) = match mant.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mant),
    };
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    let m: i128 = format!("{int}{frac}").parse().unwrap();
    (if neg { -m } else { m }, exp - frac.len() as i32)
}

/// Orden exacto de dos racionales m × 10^s, llevándolos a la misma escala.
fn orden_racional((ma, sa): (i128, i32), (mb, sb): (i128, i32)) -> Ordering {
    let s = sa.min(sb);
    let escala = |m: i128, e: i32| m * 10i128.pow((e - s) as u32);
    escala(ma, sa).cmp(&escala(mb, sb))
}

// --- Forma léxica y reescritura --------------------------------------------

#[test]
fn integer_forma_lexica_y_reescritura() {
    for (valor, esperado) in [
        ("0", "0"),
        ("+1", "1"),
        ("-0", "0"),
        ("+000", "0"),
        ("007", "7"),
        ("-12", "-12"),
        (" \t12\r\n ", "12"),
        (
            "123456789012345678901234567890",
            "123456789012345678901234567890",
        ),
    ] {
        acepta(Integer, valor, esperado);
    }
    for (valor, kind) in [
        ("1.0", Fraction),
        ("1.", Fraction),
        (".5", Fraction),
        ("-0.0", Fraction),
        ("1e2", Exponent),
        ("1E2", Exponent),
        ("+-1", Syntax),
        ("--1", Syntax),
        ("+", Syntax),
        ("1 2", Syntax),
        ("1_000", Syntax),
        ("0x1", Syntax),
        ("INF", Syntax),
        ("NaN", Syntax),
        ("\u{a0}1", Syntax),
        ("１", Syntax),
        ("", Empty),
        (" \t\n", Empty),
    ] {
        rechaza(Integer, valor, kind);
    }
}

#[test]
fn decimal_forma_lexica_y_reescritura() {
    for (valor, esperado) in [
        ("+1", "1"),
        (".5", "0.5"),
        ("1.", "1"),
        ("-.50", "-0.5"),
        ("00.0100", "0.01"),
        ("-0.000", "0"),
        ("+0", "0"),
        ("123.456", "123.456"),
        ("\n -3.14 \t", "-3.14"),
    ] {
        acepta(Decimal, valor, esperado);
    }
    for (valor, kind) in [
        ("1e2", Exponent),
        ("1E-2", Exponent),
        ("1.5e0", Exponent),
        (".", Syntax),
        ("-", Syntax),
        ("+.", Syntax),
        ("1..2", Syntax),
        ("1.2.3", Syntax),
        ("1,5", Syntax),
        ("1e", Syntax),
        ("INF", Syntax),
        ("NaN", Syntax),
        ("", Empty),
    ] {
        rechaza(Decimal, valor, kind);
    }
}

#[test]
fn float_y_double_reescritura_exacta() {
    for lx in [Float, Double] {
        for (valor, esperado) in [
            ("1e2", "1e2"),
            ("+1.5E+03", "1.5e3"),
            (".5e-007", "0.5e-7"),
            ("1.e0", "1"),
            ("0e99", "0"),
            ("-0.0E5", "0"),
            ("  2.50E1 ", "2.5e1"),
            // La reescritura exacta no redondea (el redondeo es de `xsd_number`).
            ("16777217", "16777217"),
            ("1e39", "1e39"),
        ] {
            assert_eq!(
                rewrite(lx, valor).as_deref(),
                Ok(esperado),
                "{lx:?} {valor:?}"
            );
        }
    }
}

#[test]
fn float_y_double_redondeados_al_decimal_mas_corto() {
    for (valor, esperado) in [
        ("0.1", "0.1"),
        ("1.5", "1.5"),
        ("100", "100"),
        ("16777217", "16777216"),
        ("1e-50", "0"),
        ("-1e-50", "0"),
        ("3.4028235e38", "3.4028235e38"),
        ("1e38", "1e38"),
        ("340282356779733661637539395458142568447", "3.4028235e38"),
        ("1.4e-45", "1e-45"),
    ] {
        acepta(Float, valor, esperado);
    }
    for (valor, esperado) in [
        ("0.1", "0.1"),
        ("1e39", "1e39"),
        ("9007199254740993", "9007199254740992"),
        ("5e-324", "5e-324"),
        ("3e-324", "5e-324"),
        ("2e-324", "0"),
        ("1.7976931348623157e308", "1.7976931348623157e308"),
    ] {
        acepta(Double, valor, esperado);
    }
}

#[test]
fn d24_inf_nan_y_desbordes_se_rechazan_con_motivo() {
    for lx in [Float, Double] {
        for valor in ["INF", "-INF", "+INF", "NaN", " INF\n"] {
            rechaza(lx, valor, Special);
        }
        for valor in ["inf", "nan", "Infinity", "-NaN", "1e", "e1", "1e+", "1e2.5"] {
            rechaza(lx, valor, Syntax);
        }
    }
    rechaza(Float, "1e39", Overflow);
    rechaza(Float, "-1e39", Overflow);
    // Umbral de binary32: (2 − 2⁻²⁴)·2¹²⁷, el punto medio entre el máximo
    // finito y 2¹²⁸; empata al par, que es ∞.
    rechaza(Float, "340282356779733661637539395458142568448", Overflow);
    rechaza(Double, "1e309", Overflow);
    rechaza(Double, "-1.8e308", Overflow);

    let motivo = |lx, valor| xsd_number(lx, valor).unwrap_err().to_string();
    let m = motivo(Float, "INF");
    assert!(
        m.contains("INF") && m.contains("NaN") && m.contains("D24"),
        "{m}"
    );
    let m = motivo(Double, "NaN");
    assert!(m.contains("xs:double") && m.contains("D24"), "{m}");
    let m = motivo(Float, "1e39");
    assert!(
        m.contains("desborda") && m.contains("binary32") && m.contains("D24"),
        "{m}"
    );
    let m = motivo(Double, "1e309");
    assert!(m.contains("binary64"), "{m}");
    let m = motivo(Integer, "1.0");
    assert!(
        m.contains("xs:integer") && m.contains("fraccionaria"),
        "{m}"
    );
    let m = motivo(Decimal, "1e2");
    assert!(m.contains("xs:decimal") && m.contains("exponente"), "{m}");
    let m = motivo(Integer, "+-1");
    assert!(
        m.contains("forma léxica") && m.contains("xs:integer"),
        "{m}"
    );
    let m = motivo(Decimal, " ");
    assert!(m.contains("vacío"), "{m}");
    for m in [
        motivo(Float, "INF"),
        motivo(Integer, "1.0"),
        motivo(Lexical::String, "1"),
    ] {
        assert!(!m.contains('\n'), "{m}");
    }
}

#[test]
fn tipos_no_numericos() {
    for lx in [Lexical::String, Boolean] {
        rechaza(lx, "1", NotNumeric);
        assert_eq!(
            rewrite(lx, "1"),
            Err(NumError {
                lexical: lx,
                kind: NotNumeric
            })
        );
    }
}

#[test]
fn boolean_forma_lexica() {
    for (valor, esperado) in [
        ("true", true),
        ("false", false),
        ("1", true),
        ("0", false),
        ("\t true \n", true),
    ] {
        assert_eq!(parse_boolean(valor), Ok(esperado), "{valor:?}");
    }
    for (valor, kind) in [
        ("TRUE", Syntax),
        ("True", Syntax),
        ("yes", Syntax),
        ("01", Syntax),
        ("t rue", Syntax),
        ("+1", Syntax),
        ("", Empty),
        ("  ", Empty),
    ] {
        assert_eq!(
            parse_boolean(valor),
            Err(NumError {
                lexical: Boolean,
                kind
            }),
            "{valor:?}"
        );
    }
}

// --- Comparación exacta -----------------------------------------------------

#[test]
fn comparacion_casos_conocidos() {
    use Ordering::{Equal, Greater, Less};
    for (a, b, esperado) in [
        ("0", "-0", Equal),
        ("0", "0.000e99", Equal),
        ("-0.0", "0e-5", Equal),
        ("1", "1.0", Equal),
        ("1", "1e0", Equal),
        ("100", "1e2", Equal),
        ("0.01", "1E-2", Equal),
        ("1.5e1", "15", Equal),
        ("1", "2", Less),
        ("-1", "-2", Greater),
        ("-1", "1", Less),
        ("-1", "0", Less),
        ("0", "1e-999", Less),
        ("-1e-999", "0", Less),
        ("9", "10", Less),
        ("0.9", "0.10", Greater),
        ("123.4", "123.39999999999999999999", Greater),
        ("1e400", "9e399", Greater),
        ("-1e400", "-9e399", Less),
        ("1e-400", "9e-401", Greater),
        ("12345678901234567890", "12345678901234567891", Less),
        // Exponentes que no caben en `i64`, distintos por una unidad.
        (
            "1e99999999999999999999999999999",
            "1e99999999999999999999999999998",
            Greater,
        ),
        (
            "0.1e100000000000000000000000000000",
            "1e99999999999999999999999999999",
            Equal,
        ),
        (
            "1e-99999999999999999999999999999",
            "10e-100000000000000000000000000000",
            Equal,
        ),
        (
            "-1e-99999999999999999999999999999",
            "-1e-100000000000000000000000000000",
            Less,
        ),
        ("1e+0000000000000000000000000000002", "100", Equal),
    ] {
        assert_eq!(compare(a, b), esperado, "{a} frente a {b}");
        assert_eq!(compare(b, a), esperado.reverse(), "{b} frente a {a}");
    }
}

/// Exponentes y fracciones de 10⁵ cifras: resultado exacto, sin pánico ni
/// desbordamiento, en las cuatro funciones (D1: no se mide el tiempo).
#[test]
fn exponentes_y_fracciones_de_cien_mil_cifras() {
    let n = 100_000;
    // X = 10^(n−1), con n cifras; X + 1 también.
    let x = format!("1{}", "0".repeat(n - 1));
    let x1 = format!("1{}1", "0".repeat(n - 2));
    let nueves = "9".repeat(n);
    let casi = format!("{}8", "9".repeat(n - 1));

    assert_eq!(
        compare(&format!("1e{nueves}"), &format!("1e{casi}")),
        Ordering::Greater
    );
    assert_eq!(
        compare(&format!("1e{x}"), &format!("0.1e{x1}")),
        Ordering::Equal
    );
    assert_eq!(
        compare(&format!("1e-{x1}"), &format!("10e-{x1}")),
        Ordering::Less
    );
    assert_eq!(
        compare(&format!("-1e{x}"), &format!("1e-{x}")),
        Ordering::Less
    );
    // 0,000…01 (n ceros) = 10^−(n+1).
    let fraccion = format!("0.{}1", "0".repeat(n));
    assert_eq!(
        compare(&fraccion, &format!("1e-{}", n + 1)),
        Ordering::Equal
    );
    assert_eq!(compare(&fraccion, &format!("1e-{n}")), Ordering::Less);
    // Mantisa de n cifras con el mismo valor que 1e(n−1).
    assert_eq!(compare(&x, &format!("1e{}", n - 1)), Ordering::Equal);

    assert_eq!(round_f64(&format!("1e{nueves}")), f64::INFINITY);
    assert_eq!(round_f64(&format!("-1e{nueves}")), f64::NEG_INFINITY);
    assert_eq!(round_f64(&format!("1e-{nueves}")), 0.0);
    assert_eq!(round_f32(&format!("1e-{nueves}")), 0.0);
    // 10^n · 10^(−n) = 1: la mantisa larga se compensa con el exponente.
    assert_eq!(round_f64(&format!("{fraccion}e{}", n + 1)), 1.0);
    assert_eq!(round_f32(&format!("{x}e-{}", n - 1)), 1.0);

    for lx in [Float, Double] {
        rechaza(lx, &format!("1e{nueves}"), Overflow);
        acepta(lx, &format!("1e-{nueves}"), "0");
        acepta(lx, &format!("{fraccion}E+{}", n + 1), "1");
        assert_eq!(
            rewrite(lx, &format!("+1E+{nueves}")),
            Ok(format!("1e{nueves}"))
        );
    }
    acepta(Integer, &x, &x);
    acepta(Decimal, &format!("+{fraccion}000"), &fraccion);
}

#[test]
fn redondeo_ieee_casos() {
    assert_eq!(round_f32("0.1"), 0.1f32);
    assert_eq!(round_f64("0.1"), 0.1f64);
    assert_eq!(round_f32("16777217"), 16_777_216.0);
    assert_eq!(round_f32("16777219"), 16_777_220.0);
    assert_eq!(
        round_f32("340282356779733661637539395458142568447"),
        f32::MAX
    );
    assert_eq!(
        round_f32("340282356779733661637539395458142568448"),
        f32::INFINITY
    );
    assert_eq!(round_f32("1e39"), f32::INFINITY);
    assert_eq!(round_f64("1e39"), 1e39);
    assert_eq!(round_f64("-0"), 0.0);
    assert!(round_f64("-0").is_sign_negative());
    assert_eq!(round_f64("5e-324"), 5e-324);
    assert_eq!(round_f64("2.4703282292062328e-324"), 5e-324);
    // Más de 800 cifras significativas: la cola decide el redondeo. 1 + 2⁻⁵³
    // es el punto medio entre 1 y el siguiente doble; empata a 1 y cualquier
    // cola no nula lo lleva arriba.
    let medio = "1.00000000000000011102230246251565404236316680908203125";
    assert_eq!(round_f64(medio), 1.0);
    let arriba = format!("{medio}{}1", "0".repeat(900));
    assert_eq!(round_f64(&arriba), 1.0 + f64::EPSILON);
}

// --- Propiedades --------------------------------------------------------------

/// Un valor de la forma léxica de `lx` (signo opcional, ceros a la
/// izquierda, `.5`, `1.`, exponente con signo y ceros) y su valor exacto.
fn lexema_xsd(lx: Lexical) -> impl Strategy<Value = (String, (i128, i32))> {
    let con_exp = matches!(lx, Float | Double);
    let con_frac = lx != Integer;
    (
        0u8..3,
        0usize..3,
        0u64..10_000_000,
        0usize..5,
        any::<bool>(),
        -6i32..7,
        0u8..4,
    )
        .prop_map(move |(signo, ceros, m, frac_len, punto, e, forma)| {
            let frac_len = if con_frac { frac_len } else { 0 };
            let t = format!("{m:0w$}", w = frac_len + 1);
            let (int, frac) = t.split_at(t.len() - frac_len);
            let mut int = int.trim_start_matches('0').to_owned();
            // Parte entera vacía solo si hay fracción (`.5`).
            if int.is_empty() && (frac.is_empty() || forma == 0) {
                int.push('0');
            }
            let mut lex = match signo {
                0 => String::new(),
                1 => "+".to_owned(),
                _ => "-".to_owned(),
            };
            lex.push_str(&"0".repeat(ceros));
            lex.push_str(&int);
            if !frac.is_empty() || (con_frac && punto) {
                lex.push('.');
                lex.push_str(frac);
            }
            let e = if con_exp { e } else { 0 };
            if con_exp && (e != 0 || forma == 1) {
                let marca = if forma == 2 { 'E' } else { 'e' };
                match forma {
                    3 if e >= 0 => lex.push_str(&format!("{marca}+0{e}")),
                    _ => lex.push_str(&format!("{marca}{e}")),
                }
            }
            if forma == 3 {
                lex = format!(" \t{lex}\n");
            }
            let v = i128::from(m);
            (
                (lex),
                (if signo == 2 { -v } else { v }, e - frac_len as i32),
            )
        })
}

/// Lexema de RFC 8259 pequeño (mantisa ≤ 10⁹, exponente en −10..10).
fn lexema_rfc() -> impl Strategy<Value = String> {
    (
        any::<bool>(),
        0u64..1_000_000_000,
        0usize..6,
        -10i32..10,
        0u8..3,
    )
        .prop_map(|(neg, m, frac_len, e, forma)| {
            let t = format!("{m:0w$}", w = frac_len + 1);
            let (int, frac) = t.split_at(t.len() - frac_len);
            let int = int.trim_start_matches('0');
            let mut lex = String::from(if neg { "-" } else { "" });
            lex.push_str(if int.is_empty() { "0" } else { int });
            if !frac.is_empty() {
                lex.push('.');
                lex.push_str(frac);
            }
            match forma {
                0 if e == 0 => {}
                0 | 1 => lex.push_str(&format!("e{e}")),
                _ if e >= 0 => lex.push_str(&format!("E+{e}")),
                _ => lex.push_str(&format!("E{e}")),
            }
            lex
        })
}

/// Lexema de RFC 8259 con exponentes de hasta 40 cifras (no caben en `i64`)
/// y mantisas con muchos ceros: para las propiedades de orden.
fn lexema_largo() -> impl Strategy<Value = String> {
    (
        any::<bool>(),
        "(0|[1-9][0-9]{0,6})",
        prop::option::of("[0-9]{1,30}"),
        prop::option::of(("[-+]?", "[0-9]{1,40}")),
    )
        .prop_map(|(neg, int, frac, exp)| {
            let mut lex = String::from(if neg { "-" } else { "" });
            lex.push_str(&int);
            if let Some(f) = frac {
                lex.push('.');
                lex.push_str(&f);
            }
            if let Some((s, d)) = exp {
                lex.push('e');
                lex.push_str(&s);
                lex.push_str(&d);
            }
            lex
        })
}

fn lexical() -> impl Strategy<Value = Lexical> {
    prop_oneof![Just(Integer), Just(Decimal), Just(Float), Just(Double)]
}

proptest! {
    /// Toda reescritura exacta es un lexema de RFC 8259 con el mismo valor
    /// (oráculo racional), y la de `xsd_number` además pasa `Ir::verify`
    /// (I8) como faceta y como enumeración del texto.
    #[test]
    fn reescritura_valida_y_del_mismo_valor(
        (lx, (lex, valor)) in lexical().prop_flat_map(|lx| (Just(lx), lexema_xsd(lx)))
    ) {
        let r = rewrite(lx, &lex).map_err(|e| TestCaseError::fail(format!("{:?}: {}", lex, e)))?;
        prop_assert!(is_rfc8259_number(&r), "{:?} → {:?}", lex, r);
        prop_assert_eq!(orden_racional(racional(&r), valor), Ordering::Equal, "{:?} → {:?}", lex, r);
        let n = xsd_number(lx, &lex).map_err(|e| TestCaseError::fail(format!("{:?}: {}", lex, e)))?;
        prop_assert!(is_rfc8259_number(&n.lexeme));
        match lx {
            Float => prop_assert_eq!(round_f32(&n.lexeme), round_f32(&r)),
            Double => prop_assert_eq!(round_f64(&n.lexeme), round_f64(&r)),
            _ => prop_assert_eq!(&n.lexeme, &r),
        }
        if let Err((_, e)) = ir_con_texto(lx, &n).verify() {
            return Err(TestCaseError::fail(format!("{:?} → {:?}: {}", lex, n.lexeme, e[0])));
        }
    }

    /// Dos valores iguales de `xs:integer`/`xs:decimal` dan el mismo lexema.
    #[test]
    fn reescritura_decimal_canonica(
        (a, va) in lexema_xsd(Decimal),
        (b, vb) in lexema_xsd(Decimal),
    ) {
        let (ra, rb) = (rewrite(Decimal, &a).unwrap(), rewrite(Decimal, &b).unwrap());
        prop_assert_eq!(ra == rb, orden_racional(va, vb) == Ordering::Equal, "{} {}", ra, rb);
    }

    /// La comparación coincide con el oráculo de números racionales.
    #[test]
    fn comparacion_coincide_con_oraculo(a in lexema_rfc(), b in lexema_rfc()) {
        prop_assert_eq!(compare(&a, &b), orden_racional(racional(&a), racional(&b)), "{} {}", a, b);
    }

    /// 5.6-CR-I1: `compare_parsed` sobre `Decimal` (el lexema analizado una
    /// vez) da el mismo orden que `compare`, con mantisas y exponentes
    /// cortos y largos.
    #[test]
    fn comparacion_analizada_coincide(a in lexema_rfc(), b in lexema_rfc(), c in lexema_largo(), d in lexema_largo()) {
        for (x, y) in [(&a, &b), (&c, &d), (&a, &c), (&d, &b)] {
            prop_assert_eq!(
                compare_parsed(&ir::num::Decimal::new(x), &ir::num::Decimal::new(y)),
                compare(x, y),
                "{} {}", x, y
            );
        }
    }

    /// Orden total: reflexiva, antisimétrica y transitiva, también con
    /// exponentes que no caben en `i64`.
    #[test]
    fn comparacion_antisimetrica_y_transitiva(
        a in lexema_largo(), b in lexema_largo(), c in lexema_largo(),
    ) {
        prop_assert_eq!(compare(&a, &a), Ordering::Equal);
        prop_assert_eq!(compare(&a, &b), compare(&b, &a).reverse(), "{} {}", a, b);
        let (ab, bc, ac) = (compare(&a, &b), compare(&b, &c), compare(&a, &c));
        if ab != Ordering::Greater && bc != Ordering::Greater {
            prop_assert_ne!(ac, Ordering::Greater, "{} ≤ {} ≤ {}", a, b, c);
        }
        if ab == Ordering::Equal && bc == Ordering::Equal {
            prop_assert_eq!(ac, Ordering::Equal);
        }
    }

    /// El redondeo coincide con el de `std` (correcto, al par más cercano)
    /// donde `std` lee el lexema tal cual.
    #[test]
    fn redondeo_coincide_con_std(a in lexema_rfc()) {
        prop_assert_eq!(round_f64(&a).to_bits(), a.parse::<f64>().unwrap().to_bits(), "{}", a);
        prop_assert_eq!(round_f32(&a).to_bits(), a.parse::<f32>().unwrap().to_bits(), "{}", a);
    }

    /// Todo `f32`/`f64` finito escrito en decimal vuelve a sí mismo, y el
    /// lexema guardado es el decimal más corto que lo hace.
    #[test]
    fn decimal_mas_corto_vuelve_al_mismo_valor(x in any::<f64>(), y in any::<f32>()) {
        if x.is_finite() {
            let n = xsd_number(Double, &format!("{:e}", x)).unwrap();
            prop_assert_eq!(round_f64(&n.lexeme), if x == 0.0 { 0.0 } else { x });
        }
        if y.is_finite() {
            let plano = format!("{}", y);
            let n = xsd_number(Float, &plano).unwrap();
            prop_assert_eq!(round_f32(&n.lexeme), if y == 0.0 { 0.0 } else { y });
            let largo = plano.len();
            prop_assert!(n.lexeme.len() <= largo, "{} → {}", plano, n.lexeme);
        }
    }

    /// 5.5-CR-S2 (monotonía del decimal más corto, `numeros-xsd.md` §6): si
    /// x < y en binary64 (o binary32), `compare` de sus `Num` da `Less`; con
    /// pares cualesquiera y con **vecinos** (y el siguiente flotante de x),
    /// donde los intervalos de redondeo se tocan. Es lo que deja a la 5.6
    /// comparar las cotas de `xs:float`/`xs:double` con `compare`.
    #[test]
    fn decimal_mas_corto_es_monotono(a in any::<f64>(), b in any::<f64>(), c in any::<f32>(), d in any::<f32>()) {
        let n64 = |x: f64| xsd_number(Double, &format!("{:e}", x)).unwrap().lexeme;
        let n32 = |x: f32| xsd_number(Float, &format!("{:e}", x)).unwrap().lexeme;
        let siguiente64 = |x: f64| {
            if x == 0.0 { f64::from_bits(1) }
            else if x > 0.0 { f64::from_bits(x.to_bits() + 1) }
            else { f64::from_bits(x.to_bits() - 1) }
        };
        let siguiente32 = |x: f32| {
            if x == 0.0 { f32::from_bits(1) }
            else if x > 0.0 { f32::from_bits(x.to_bits() + 1) }
            else { f32::from_bits(x.to_bits() - 1) }
        };
        if a.is_finite() && b.is_finite() {
            prop_assert_eq!(compare(&n64(a), &n64(b)), a.partial_cmp(&b).unwrap(), "{} {}", a, b);
            let s = siguiente64(a);
            if s.is_finite() {
                prop_assert_eq!(compare(&n64(a), &n64(s)), Ordering::Less, "{} {}", a, s);
            }
        }
        if c.is_finite() && d.is_finite() {
            prop_assert_eq!(compare(&n32(c), &n32(d)), c.partial_cmp(&d).unwrap(), "{} {}", c, d);
            let s = siguiente32(c);
            if s.is_finite() {
                prop_assert_eq!(compare(&n32(c), &n32(s)), Ordering::Less, "{} {}", c, s);
            }
        }
    }

    /// Nunca fallan, aunque reciban algo que no es un lexema.
    #[test]
    fn no_fallan_con_texto_arbitrario(s in ".{0,40}", t in "[-+.eE0-9 ]{0,40}") {
        for lx in [Integer, Decimal, Float, Double, Boolean] {
            let _ = xsd_number(lx, &s);
            let _ = xsd_number(lx, &t);
        }
        let _ = parse_boolean(&s);
        let _ = compare(&s, &t);
        let _ = round_f32(&s);
        let _ = round_f64(&t);
    }
}
