//! Números de XSD y comparación exacta de lexemas (tarea 5.5).
//!
//! Funciones puras, sin AST, que usan el frontal XSD (facetas, enumeraciones,
//! `default`/`fixed`, tarea 5.6) y la Fase 4 (comparadores sobre
//! [`Num::lexeme`]). Teoría, corrección y costo: `docs/fase3/numeros-xsd.md`.
//!
//! - **Forma léxica** de `xs:integer`, `xs:decimal`, `xs:float`/`xs:double`
//!   ([`rewrite`], [`xsd_number`]) y `xs:boolean` ([`parse_boolean`]), con el
//!   *whiteSpace* `collapse` de esos tipos: como su forma léxica no admite
//!   espacios, `collapse` equivale a quitar los espacios XML de los extremos
//!   (un espacio interior, que `collapse` dejaría, ya es un error).
//! - **Reescritura** a un lexema de RFC 8259 del mismo valor: sin `+`, sin
//!   ceros a la izquierda ni ceros finales de la fracción, `.5` → `0.5`,
//!   `1.` → `1` y el cero como `0`.
//! - **Comparación** exacta de dos lexemas de RFC 8259 ([`compare`]), sin
//!   `f64`, para cualquier longitud de mantisa y de exponente.
//! - **Redondeo IEEE** al par más cercano a binary32/binary64 ([`round_f32`],
//!   [`round_f64`]); en `xs:float`/`xs:double`, [`xsd_number`] guarda el
//!   decimal más corto del valor redondeado y rechaza `INF`, `-INF`, `NaN` y
//!   los desbordes (D24).
//!
//! **Método** (el de `semantic::number`, sin su saturación). Un lexema vale
//! 0.D × 10^E, con D las cifras significativas (las de la parte entera y la
//! fracción, sin los ceros de los extremos) y E = exp + p − z, donde p es el
//! número de cifras de la parte entera y z el de ceros a la izquierda de D.
//! p y z caben en `i64` (son longitudes); exp no tiene por qué, así que E se
//! calcula como entero de longitud arbitraria (`Big`): la comparación es
//! exacta también con exponentes de 10⁵ cifras, y cada operación es
//! O(longitud del lexema). Ante un texto que no es un lexema de RFC 8259
//! [`compare`] y el redondeo no fallan: los caracteres que no son cifras se
//! ignoran, como en `semantic::number`.

use core::cmp::Ordering;
use core::fmt;

use crate::{Lexical, Num};

/// Un valor que no es del espacio léxico (o de valores) de su tipo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumError {
    /// El espacio léxico contra el que se leyó.
    pub lexical: Lexical,
    pub kind: NumErrorKind,
}

/// Por qué se rechaza un valor. No guarda el texto del usuario: quien da el
/// error con su posición (el frontal XSD) cita el valor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumErrorKind {
    /// Vacío tras `collapse`.
    Empty,
    /// No tiene la forma léxica del tipo (`+-1`, `1,5`, `inf`, `1 2`).
    Syntax,
    /// Punto decimal en `xs:integer` (`1.0`, `1.`, `.5`).
    Fraction,
    /// Exponente en `xs:integer` o `xs:decimal` (`1e2`).
    Exponent,
    /// `INF`, `-INF`, `+INF` o `NaN` en `xs:float`/`xs:double` (D24).
    Special,
    /// Desborda a ±∞ al redondear a binary32/binary64 (D24).
    Overflow,
    /// El espacio léxico no es numérico (`xs:string`, o `xs:boolean` en
    /// [`rewrite`]/[`xsd_number`]).
    NotNumeric,
}

/// Nombre del tipo predefinido primitivo de un espacio léxico.
const fn xs_name(lx: Lexical) -> &'static str {
    match lx {
        Lexical::String => "xs:string",
        Lexical::Boolean => "xs:boolean",
        Lexical::Decimal => "xs:decimal",
        Lexical::Integer => "xs:integer",
        Lexical::Float => "xs:float",
        Lexical::Double => "xs:double",
    }
}

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let xs = xs_name(self.lexical);
        match self.kind {
            NumErrorKind::Empty => write!(f, "valor vacío: {xs} no admite la cadena vacía"),
            NumErrorKind::Syntax => write!(f, "no tiene la forma léxica de {xs}"),
            NumErrorKind::Fraction => write!(
                f,
                "{xs} no admite parte fraccionaria ni punto decimal (como en 1.0 o .5)"
            ),
            NumErrorKind::Exponent => write!(f, "{xs} no admite exponente (como en 1e2)"),
            NumErrorKind::Special => write!(
                f,
                "INF, -INF y NaN no se admiten en las facetas ni en las enumeraciones \
                 de {xs} (D24)"
            ),
            NumErrorKind::Overflow => write!(
                f,
                "el valor desborda al redondear a {}: es mayor en valor absoluto que \
                 el máximo finito de {xs} (D24)",
                if self.lexical == Lexical::Float {
                    "binary32"
                } else {
                    "binary64"
                }
            ),
            NumErrorKind::NotNumeric => write!(f, "{xs} no es un tipo numérico"),
        }
    }
}

impl std::error::Error for NumError {}

/// *Collapse* de un valor cuyo espacio léxico no admite espacios: quitar los
/// espacios XML (#x20, #x9, #xD, #xA) de los extremos.
fn collapse(value: &str) -> &str {
    value.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
}

/// `xs:boolean` (XSD 1.0 Parte 2 §3.2.2): `true`, `false`, `1` o `0`, tras
/// `collapse`.
pub fn parse_boolean(value: &str) -> Result<bool, NumError> {
    let err = |kind| NumError {
        lexical: Lexical::Boolean,
        kind,
    };
    match collapse(value) {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        "" => Err(err(NumErrorKind::Empty)),
        _ => Err(err(NumErrorKind::Syntax)),
    }
}

/// Partes de `[+-]? [0-9]* ('.' [0-9]*)? ([eE] [+-]? [0-9]+)?` con al menos
/// una cifra en la mantisa: la unión de las formas léxicas de `xs:decimal`
/// (sin exponente), `xs:integer` (sin punto) y `xs:float`/`xs:double` (sin
/// sus valores especiales), XSD 1.0 Parte 2 §3.2.3-§3.2.5 y §3.3.13.
struct Parts<'a> {
    neg: bool,
    int: &'a str,
    dot: bool,
    frac: &'a str,
    exp: Option<(bool, &'a str)>,
}

fn scan(s: &str) -> Option<Parts<'_>> {
    let b = s.as_bytes();
    let mut i = 0;
    let sign = |i: &mut usize| match b.get(*i) {
        Some(b'-') => {
            *i += 1;
            true
        }
        Some(b'+') => {
            *i += 1;
            false
        }
        _ => false,
    };
    // Todo lo que se salta es ASCII, así que los índices son fronteras de
    // carácter.
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        &s[start..*i]
    };
    let neg = sign(&mut i);
    let int = digits(&mut i);
    let dot = b.get(i) == Some(&b'.');
    let frac = if dot {
        i += 1;
        digits(&mut i)
    } else {
        ""
    };
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    let exp = if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        let neg = sign(&mut i);
        let d = digits(&mut i);
        if d.is_empty() {
            return None;
        }
        Some((neg, d))
    } else {
        None
    };
    (i == b.len()).then_some(Parts {
        neg,
        int,
        dot,
        frac,
        exp,
    })
}

/// Comprueba la forma léxica de `value` en el tipo numérico `lexical`, tras
/// `collapse`, y la reescribe **exactamente** (sin redondear, también en
/// `xs:float`/`xs:double`) a un lexema de RFC 8259 del mismo valor. En
/// `xs:integer` y `xs:decimal` el resultado es canónico: dos valores iguales
/// dan el mismo lexema. O(longitud).
pub fn rewrite(lexical: Lexical, value: &str) -> Result<String, NumError> {
    let err = |kind| NumError { lexical, kind };
    if !lexical.is_numeric() {
        return Err(err(NumErrorKind::NotNumeric));
    }
    let v = collapse(value);
    if v.is_empty() {
        return Err(err(NumErrorKind::Empty));
    }
    let ieee = matches!(lexical, Lexical::Float | Lexical::Double);
    // `+INF` es de XSD 1.1, no de 1.0; se rechaza con el mismo motivo.
    if ieee && matches!(v, "INF" | "-INF" | "+INF" | "NaN") {
        return Err(err(NumErrorKind::Special));
    }
    let p = scan(v).ok_or(err(NumErrorKind::Syntax))?;
    if lexical == Lexical::Integer && p.dot {
        return Err(err(NumErrorKind::Fraction));
    }
    if !ieee && p.exp.is_some() {
        return Err(err(NumErrorKind::Exponent));
    }
    Ok(canonical(&p))
}

/// `-? (0 | [1-9][0-9]*) ('.' [0-9]*[1-9])? ('e' '-'? [1-9][0-9]*)?`, o `0`.
fn canonical(p: &Parts<'_>) -> String {
    let int = p.int.trim_start_matches('0');
    let frac = p.frac.trim_end_matches('0');
    if int.is_empty() && frac.is_empty() {
        return "0".to_owned();
    }
    let mut out = String::with_capacity(p.int.len() + p.frac.len() + 4);
    if p.neg {
        out.push('-');
    }
    out.push_str(if int.is_empty() { "0" } else { int });
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    if let Some((neg, digits)) = p.exp {
        let digits = digits.trim_start_matches('0');
        if !digits.is_empty() {
            out.push('e');
            if neg {
                out.push('-');
            }
            out.push_str(digits);
        }
    }
    out
}

/// El [`Num`] de un valor de una faceta o una enumeración de tipo numérico:
/// la forma léxica de [`rewrite`] y su reescritura. En `xs:integer` y
/// `xs:decimal` es el valor exacto. En `xs:float`/`xs:double` (XSD 1.0 Parte
/// 2 §3.2.4-§3.2.5) es el **decimal más corto** que redondea al mismo
/// binary32/binary64 que el valor (el redondeo IEEE al par más cercano), con
/// el cero como `0` (−0 y +0 son iguales en IEEE); un valor que redondea a
/// ±∞ se rechaza (D24). La Fase 4 compara un texto de `xs:float` redondeando
/// los dos lados con [`round_f32`]: como el lexema guardado vuelve a su
/// valor, el resultado es el del espacio de valores de XSD. O(longitud).
pub fn xsd_number(lexical: Lexical, value: &str) -> Result<Num, NumError> {
    let exact = rewrite(lexical, value)?;
    let overflow = NumError {
        lexical,
        kind: NumErrorKind::Overflow,
    };
    let lexeme = match lexical {
        Lexical::Float => {
            let x = round_f32(&exact);
            if x.is_infinite() {
                return Err(overflow);
            }
            if x == 0.0 {
                "0".to_owned()
            } else {
                shortest(format!("{x}"), format!("{x:e}"))
            }
        }
        Lexical::Double => {
            let x = round_f64(&exact);
            if x.is_infinite() {
                return Err(overflow);
            }
            if x == 0.0 {
                "0".to_owned()
            } else {
                shortest(format!("{x}"), format!("{x:e}"))
            }
        }
        _ => exact,
    };
    Ok(Num { lexeme })
}

/// De las dos escrituras de `std` de un flotante finito (las dos con las
/// cifras más cortas que vuelven al mismo valor, y las dos lexemas de RFC
/// 8259: `0.1`, `1e38`, `-1.5e-7`), la más corta; con empate, la sin
/// exponente.
fn shortest(plain: String, sci: String) -> String {
    if sci.len() < plain.len() {
        sci
    } else {
        plain
    }
}

/// `-? (0 | [1-9][0-9]*) ('.' [0-9]+)? ([eE] [+-]? [0-9]+)?` (RFC 8259 §6):
/// lo que I8 exige a un [`Num`] (`docs/fase3/ir.md` §4).
pub fn is_rfc8259_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i > start
    };
    if b.first() == Some(&b'-') {
        i += 1;
    }
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            digits(&mut i);
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if !digits(&mut i) {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if !digits(&mut i) {
            return false;
        }
    }
    i == b.len()
}

// --- Forma científica 0.D × 10^E ------------------------------------------

/// Entero con signo de longitud arbitraria: las cifras (0-9) de la magnitud,
/// la más significativa primero y sin ceros a la izquierda. El cero es la
/// magnitud vacía, sin signo. Solo hace falta sumar y comparar.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Big {
    neg: bool,
    mag: Vec<u8>,
}

impl Big {
    /// De las cifras ASCII de un exponente (lo que no es cifra se ignora).
    fn from_ascii(neg: bool, ascii: &[u8]) -> Big {
        let mag: Vec<u8> = ascii
            .iter()
            .filter(|b| b.is_ascii_digit())
            .map(|d| d - b'0')
            .skip_while(|&d| d == 0)
            .collect();
        Big {
            neg: neg && !mag.is_empty(),
            mag,
        }
    }

    fn from_i64(k: i64) -> Big {
        let digits = k.unsigned_abs().to_string();
        Big::from_ascii(k < 0, digits.as_bytes())
    }

    fn add(&self, o: &Big) -> Big {
        if self.neg == o.neg {
            return Big {
                neg: self.neg,
                mag: add_mag(&self.mag, &o.mag),
            };
        }
        match cmp_mag(&self.mag, &o.mag) {
            Ordering::Equal => Big {
                neg: false,
                mag: Vec::new(),
            },
            Ordering::Greater => Big {
                neg: self.neg,
                mag: sub_mag(&self.mag, &o.mag),
            },
            Ordering::Less => Big {
                neg: o.neg,
                mag: sub_mag(&o.mag, &self.mag),
            },
        }
    }

    /// El valor si |valor| ≤ `limit` (< 10¹⁸).
    fn small(&self, limit: i64) -> Option<i64> {
        if self.mag.len() > 18 {
            return None;
        }
        let v = self.mag.iter().fold(0i64, |v, &d| v * 10 + i64::from(d));
        (v <= limit).then_some(if self.neg { -v } else { v })
    }
}

impl Ord for Big {
    fn cmp(&self, o: &Big) -> Ordering {
        match (self.neg, o.neg) {
            (false, false) => cmp_mag(&self.mag, &o.mag),
            (true, true) => cmp_mag(&o.mag, &self.mag),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, o: &Big) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Magnitudes sin ceros a la izquierda: la más larga es la mayor.
fn cmp_mag(a: &[u8], b: &[u8]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn add_mag(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let (mut x, mut y) = (a.iter().rev(), b.iter().rev());
    let mut carry = 0;
    loop {
        let (dx, dy) = (x.next(), y.next());
        if dx.is_none() && dy.is_none() {
            break;
        }
        let s = dx.copied().unwrap_or(0) + dy.copied().unwrap_or(0) + carry;
        out.push(s % 10);
        carry = s / 10;
    }
    if carry > 0 {
        out.push(carry);
    }
    out.reverse();
    out
}

/// a − b con a > b.
fn sub_mag(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len());
    let mut y = b.iter().rev();
    let mut borrow = 0;
    for &dx in a.iter().rev() {
        let sub = y.next().copied().unwrap_or(0) + borrow;
        if dx >= sub {
            out.push(dx - sub);
            borrow = 0;
        } else {
            out.push(dx + 10 - sub);
            borrow = 1;
        }
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out.reverse();
    out
}

fn to_i64(n: usize) -> i64 {
    // Longitudes de un lexema en memoria: nunca pasan de `i64::MAX`.
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Un lexema como (−1)^neg × 0.D × 10^E.
struct Sci<'a> {
    neg: bool,
    int: &'a [u8],
    frac: &'a [u8],
    /// Ceros a la izquierda y número de cifras de D; `sig == 0` es el cero.
    lead: usize,
    sig: usize,
    exp: Big,
}

impl Sci<'_> {
    fn new(lexeme: &str) -> Sci<'_> {
        #[cfg(feature = "contadores")]
        contador::sumar(lexeme.len());
        let b = lexeme.as_bytes();
        let (neg, rest) = match b.split_first() {
            Some((b'-', rest)) => (true, rest),
            _ => (false, b),
        };
        let (mantissa, exp) = match rest.iter().position(|&c| c == b'e' || c == b'E') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, &rest[rest.len()..]),
        };
        let (int, frac) = match mantissa.iter().position(|&c| c == b'.') {
            Some(i) => (&mantissa[..i], &mantissa[i + 1..]),
            None => (mantissa, &mantissa[mantissa.len()..]),
        };
        let digits = || int.iter().chain(frac).copied().filter(u8::is_ascii_digit);
        let total = digits().count();
        let lead = digits().take_while(|&d| d == b'0').count();
        let mut sci = Sci {
            neg,
            int,
            frac,
            lead,
            sig: 0,
            exp: Big::from_i64(0),
        };
        if lead == total {
            return sci;
        }
        let trail = digits().rev().take_while(|&d| d == b'0').count();
        sci.sig = total - lead - trail;
        let p = int.iter().filter(|b| b.is_ascii_digit()).count();
        let (eneg, edigits) = match exp.split_first() {
            Some((b'-', r)) => (true, r),
            Some((b'+', r)) => (false, r),
            _ => (false, exp),
        };
        // E = exp + p − z; p y z son longitudes, su diferencia cabe en i64.
        sci.exp = Big::from_ascii(eneg, edigits).add(&Big::from_i64(to_i64(p) - to_i64(lead)));
        sci
    }

    fn significant(&self) -> impl Iterator<Item = u8> + '_ {
        self.int
            .iter()
            .chain(self.frac)
            .copied()
            .filter(u8::is_ascii_digit)
            .skip(self.lead)
            .take(self.sig)
    }

    /// −1, 0 o 1 (−0 es 0).
    fn sign(&self) -> i8 {
        match (self.sig, self.neg) {
            (0, _) => 0,
            (_, true) => -1,
            (_, false) => 1,
        }
    }
}

/// Compara por valor dos lexemas de RFC 8259, exactamente y sin `f64`: `-0`
/// = `0`, `1.0` = `1e0`, `0.1e100…0` = `1e99…9` con exponentes de cualquier
/// longitud. Signo, después E (a mayor E, mayor magnitud, porque D empieza
/// por una cifra no nula) y después D en orden lexicográfico (sin ceros
/// finales, un prefijo es menor). Es un orden total. O(|a| + |b|).
pub fn compare(a: &str, b: &str) -> Ordering {
    let (x, y) = (Sci::new(a), Sci::new(b));
    match x.sign().cmp(&y.sign()) {
        Ordering::Equal if x.sig == 0 => Ordering::Equal,
        Ordering::Equal => {
            let mag = x
                .exp
                .cmp(&y.exp)
                .then_with(|| x.significant().cmp(y.significant()));
            if x.neg {
                mag.reverse()
            } else {
                mag
            }
        }
        o => o,
    }
}

/// Un lexema de RFC 8259 **analizado una vez** para compararlo muchas veces
/// (5.6-CR-I1): signo, cifras significativas D (sin los ceros de los
/// extremos) y exponente E de la forma científica. Construirlo cuesta
/// O(|lexema|), como [`compare`]; [`compare_parsed`] ya no vuelve a recorrer
/// el lexema. Lo usa el frontal XSD para las cotas de las facetas, que se
/// comparan con cada enumeración, `default`/`fixed` y faceta de los
/// derivados.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal {
    neg: bool,
    /// Cifras ASCII de D; vacía en el cero.
    digits: Vec<u8>,
    exp: Big,
}

impl Decimal {
    /// Analiza `lexeme` (un lexema de RFC 8259; con cualquier otro texto no
    /// falla, como [`compare`]). O(|lexema|).
    pub fn new(lexeme: &str) -> Decimal {
        let s = Sci::new(lexeme);
        Decimal {
            neg: s.neg && s.sig > 0,
            digits: s.significant().collect(),
            exp: if s.sig == 0 { Big::from_i64(0) } else { s.exp },
        }
    }

    /// −1, 0 o 1 (−0 es 0).
    fn sign(&self) -> i8 {
        match (self.digits.is_empty(), self.neg) {
            (true, _) => 0,
            (_, true) => -1,
            (_, false) => 1,
        }
    }
}

/// [`compare`] sobre dos lexemas ya analizados: el mismo orden total, en
/// O(min(|a|, |b|)) (el exponente se compara primero por longitud y D en
/// orden lexicográfico, que se detiene en la primera cifra distinta).
pub fn compare_parsed(a: &Decimal, b: &Decimal) -> Ordering {
    match a.sign().cmp(&b.sign()) {
        Ordering::Equal if a.digits.is_empty() => Ordering::Equal,
        Ordering::Equal => {
            let mag = a.exp.cmp(&b.exp).then_with(|| a.digits.cmp(&b.digits));
            if a.neg {
                mag.reverse()
            } else {
                mag
            }
        }
        o => o,
    }
}

/// Contador de los bytes que recorre el análisis de lexemas (la forma
/// científica de [`compare`], [`Decimal::new`] y el redondeo IEEE), por hilo.
/// Solo existe con la *feature* `contadores`, que activan las pruebas de
/// `xsd` (dev-dependency) para comprobar sin reloj (D1) que cada cota se
/// analiza una vez; no cambia ningún resultado.
#[cfg(feature = "contadores")]
pub mod contador {
    use core::cell::Cell;

    std::thread_local! {
        static BYTES: Cell<u64> = const { Cell::new(0) };
    }

    pub(crate) fn sumar(n: usize) {
        BYTES.with(|b| b.set(b.get().saturating_add(n as u64)));
    }

    /// Bytes recorridos en este hilo desde el último [`reiniciar`].
    pub fn bytes() -> u64 {
        BYTES.with(Cell::get)
    }

    /// Pone a cero el contador de este hilo.
    pub fn reiniciar() {
        BYTES.with(|b| b.set(0));
    }
}

/// Cifras significativas que se pasan a `std` al redondear. Todo punto medio
/// entre dos binary64 consecutivos (y entre ±0, los subnormales o el máximo
/// y ±∞) tiene a lo sumo 768 cifras significativas ((2⁵⁴ − 1)·5¹⁰⁷⁵; 113 en
/// binary32), así que uno que caiga en [10^(E−1), 10^E) es múltiplo de
/// 10^(E−768) y por tanto de 10^(E−800). Cortar D en 800 cifras y poner un
/// `1` detrás si sobra algo (D termina en una cifra no nula) deja el valor
/// en el mismo intervalo abierto entre dos múltiplos consecutivos de
/// 10^(E−800), donde no cae ningún punto medio: el redondeo no cambia.
const ROUND_DIGITS: usize = 800;

/// |E| a partir del que el valor es ±∞ o ±0 en binary32 y binary64: con
/// E ≥ 401 vale al menos 10⁴⁰⁰ > 2¹⁰²⁴, y con E ≤ −401 menos de 10⁻⁴⁰¹, por
/// debajo de la mitad del menor subnormal (2⁻¹⁰⁷⁵ ≈ 2,5·10⁻³²⁴).
const ROUND_EXP: i64 = 400;

/// El lexema reducido a `0.D'eE` con D' de ≤ 801 cifras y |E| ≤ 400, que
/// `std` redondea igual que el lexema entero; `Err(true)` si es ±∞,
/// `Err(false)` si es ±0.
fn reduced(s: &Sci<'_>) -> Result<String, bool> {
    if s.sig == 0 {
        return Err(false);
    }
    let Some(e) = s.exp.small(ROUND_EXP) else {
        return Err(!s.exp.neg);
    };
    let mut t = String::with_capacity(ROUND_DIGITS + 10);
    t.push_str("0.");
    t.extend(s.significant().take(ROUND_DIGITS).map(char::from));
    if s.sig > ROUND_DIGITS {
        t.push('1');
    }
    t.push('e');
    t.push_str(&e.to_string());
    Ok(t)
}

/// Redondeo IEEE 754 al par más cercano de un lexema de RFC 8259 a binary32,
/// con exponentes de cualquier longitud (sin la saturación del exponente de
/// `str::parse`). Puede dar ±∞ y ±0; nunca NaN. O(longitud).
pub fn round_f32(lexeme: &str) -> f32 {
    let s = Sci::new(lexeme);
    let v = match reduced(&s) {
        // La cadena reducida siempre es un flotante válido para `std`.
        Ok(t) => t.parse::<f32>().unwrap_or(0.0),
        Err(true) => f32::INFINITY,
        Err(false) => 0.0,
    };
    if s.neg {
        -v
    } else {
        v
    }
}

/// Como [`round_f32`], a binary64.
pub fn round_f64(lexeme: &str) -> f64 {
    let s = Sci::new(lexeme);
    let v = match reduced(&s) {
        Ok(t) => t.parse::<f64>().unwrap_or(0.0),
        Err(true) => f64::INFINITY,
        Err(false) => 0.0,
    };
    if s.neg {
        -v
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn big(v: i128) -> Big {
        Big::from_ascii(v < 0, v.unsigned_abs().to_string().as_bytes())
    }

    #[test]
    fn suma_y_orden_de_enteros_largos() {
        let casos: [(i128, i128); 8] = [
            (0, 0),
            (999, 1),
            (-999, -1),
            (1000, -1),
            (-1000, 1),
            (5, -5),
            (-12, 3),
            (i128::from(i64::MAX), i128::from(i64::MAX)),
        ];
        for (a, b) in casos {
            assert_eq!(big(a).add(&big(b)), big(a + b), "{a} + {b}");
            assert_eq!(big(a).cmp(&big(b)), a.cmp(&b), "{a} frente a {b}");
        }
        assert_eq!(Big::from_ascii(true, b"000"), big(0));
        assert_eq!(Big::from_i64(i64::MIN), big(i128::from(i64::MIN)));
        assert_eq!(big(-401).small(400), None);
        assert_eq!(big(-400).small(400), Some(-400));
        assert_eq!(big(10i128.pow(19)).small(400), None);
    }

    #[test]
    fn forma_cientifica() {
        for (lex, neg, digitos, e) in [
            ("0.00120e3", false, "12", 1),
            ("-12.5", true, "125", 2),
            ("100", false, "1", 3),
            ("0.5e-7", false, "5", -7),
        ] {
            let s = Sci::new(lex);
            assert_eq!(s.neg, neg, "{lex}");
            let d: Vec<u8> = s.significant().collect();
            assert_eq!(d, digitos.as_bytes(), "{lex}");
            assert_eq!(s.exp, big(e), "{lex}");
        }
        assert_eq!(Sci::new("-0.000e5").sign(), 0);
    }

    proptest! {
        /// La suma y el orden coinciden con `i128` dentro de su rango.
        #[test]
        fn enteros_largos_como_i128(a in any::<i64>(), b in any::<i64>()) {
            let (a, b) = (i128::from(a), i128::from(b));
            prop_assert_eq!(big(a).add(&big(b)), big(a + b));
            prop_assert_eq!(big(a).cmp(&big(b)), a.cmp(&b));
        }
    }
}
