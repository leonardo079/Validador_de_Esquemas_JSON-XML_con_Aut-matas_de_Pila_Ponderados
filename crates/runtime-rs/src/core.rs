//! Núcleo del ejecutor de M (F4-U9, D51): lee las [`Tables`], usa la
//! memoria del llamador como pila de marcos y recibe los eventos del lexer.
//!
//! Por evento hace una consulta a δ, las comprobaciones de la transición
//! (acotadas por el esquema) y a lo sumo una operación de pila: O(1) por
//! evento respecto del payload, sin recursión y sin reservar memoria. La
//! pila es `mem`: cada marco son sus registros seguidos de una cabecera
//! `(retorno << 32) | tipo`; la altura es la profundidad de anidamiento de
//! objetos y arreglos, O(d). El escalar abierto (a lo sumo uno) no se apila:
//! su estado de retorno y los registros del número son globales
//! (`docs/fase4/automata.md` §3).

use lexer::json::{Event, NumPart};
use lexer::{LexError, Pos, Sink, Span};

use crate::tables::{class, Check, Fault, Op, Tables, Test, Trans, NO_BIT};
use crate::weight::Weight;

/// Modo de ejecución (F4-U3): los dos sobre el mismo M.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Conformidad total: sigue tras cada violación y las cuenta todas.
    Total,
    /// Fallo temprano: se detiene en la primera transición con peso > 0 e
    /// informa de su comprobación de menor costo.
    EarlyFail,
}

/// Opciones del ejecutor. F4.6 y F6.2 añaden los límites (`sv_limits_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
}

/// Clase del error del reporte (`expected.errorKind` del corpus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Lexical,
    Syntax,
    Schema,
    Limit,
    /// Tablas incoherentes: un error del compilador, nunca del payload.
    Internal,
}

/// Dónde se detectó el primer error y, si es una violación, de qué regla.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorAt {
    pub offset: u64,
    pub line: u32,
    pub column: u32,
    /// Índice en [`Tables::rules`]; `None` en un error de peso ∞.
    pub rule: Option<u32>,
}

/// Resultado de una validación.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    /// Bien formado y peso 0.
    pub valid: bool,
    /// Peso del recorrido: número de violaciones, o ∞ si es malformado o
    /// pasa un límite.
    pub violations: Weight,
    pub error_kind: Option<ErrorKind>,
    /// El primer error: el de peso ∞ si lo hay (el payload no es JSON o
    /// pasa un límite); si no, la primera violación.
    pub first: Option<ErrorAt>,
}

/// El núcleo se detuvo (error de peso ∞ o fallo temprano).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stop;

/// Registros del número abierto: bastan para decidir si es entero sin
/// guardar el lexema (`docs/fase4/automata.md` §3.3).
#[derive(Clone, Copy, Debug, Default)]
struct Number {
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
    fn byte(&mut self, part: NumPart, b: u8) {
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
    fn is_integer(&self) -> bool {
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

/// El ejecutor: estado de control, pila en `mem`, registros globales y
/// peso acumulado.
#[derive(Debug)]
pub struct Core<'t, 'm> {
    t: Tables<'t>,
    mem: &'m mut [u64],
    /// Palabras de `mem` ocupadas.
    sp: usize,
    depth: usize,
    max_depth: usize,
    state: u32,
    /// Estado de retorno del escalar abierto.
    scalar_ret: u32,
    num: Number,
    mode: Mode,
    weight: Weight,
    first: Option<ErrorAt>,
    fault: Option<ErrorKind>,
    stopped: bool,
}

impl<'t, 'm> Core<'t, 'm> {
    /// Ejecutor en q₀ con la pila vacía (Z₀). `mem` es la pila: cada marco
    /// ocupa `1 + Frame::words` palabras; si no cabe uno, es un error de
    /// límite.
    pub fn new(t: Tables<'t>, opts: &Options, mem: &'m mut [u64]) -> Core<'t, 'm> {
        Core {
            state: t.start,
            t,
            mem,
            sp: 0,
            depth: 0,
            max_depth: 0,
            scalar_ret: 0,
            num: Number::default(),
            mode: opts.mode,
            weight: Weight::ZERO,
            first: None,
            fault: None,
            stopped: false,
        }
    }

    /// Si ya se detuvo.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Mayor profundidad de pila alcanzada (marcos).
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Procesa un evento del lexer JSON.
    pub fn event(&mut self, ev: Event, span: Span) -> Result<(), Stop> {
        if self.stopped {
            return Err(Stop);
        }
        let a = match ev {
            Event::BeginObject => class::BEGIN_OBJECT,
            Event::EndObject => class::END_OBJECT,
            Event::BeginArray => class::BEGIN_ARRAY,
            Event::EndArray => class::END_ARRAY,
            Event::Colon => class::COLON,
            Event::Comma => class::COMMA,
            Event::True => class::TRUE,
            Event::False => class::FALSE,
            Event::Null => class::NULL,
            Event::StringBegin => class::STRING_BEGIN,
            Event::StringChar(c) => class::CHAR.saturating_add(self.t.char_class(c)),
            Event::StringEnd => class::STRING_END,
            Event::NumberBegin => {
                self.num = Number::default();
                class::NUMBER_BEGIN
            }
            Event::Number(part, b) => {
                self.num.byte(part, b);
                class::NUMBER_BYTE
            }
            Event::NumberEnd => class::NUMBER_END,
        };
        self.step(a, span.start)
    }

    /// Error léxico del lexer que va delante: peso ∞.
    pub fn lex_error(&mut self, e: LexError) {
        if !self.stopped {
            self.stop(ErrorKind::Lexical, e.at);
        }
    }

    /// Fin de la entrada en la posición `end`: se acepta si el control está
    /// en F con la pila vacía; si no, es un error de sintaxis (entrada
    /// incompleta) en `end`.
    pub fn finish(mut self, end: Pos) -> Report {
        if !self.stopped && (self.state != self.t.accept || self.sp != 0) {
            self.stop(ErrorKind::Syntax, end);
        }
        Report {
            valid: self.fault.is_none() && self.weight == Weight::ZERO,
            violations: self.weight,
            error_kind: self.fault.or(if self.weight == Weight::ZERO {
                None
            } else {
                Some(ErrorKind::Schema)
            }),
            first: self.first,
        }
    }

    fn stop(&mut self, kind: ErrorKind, at: Pos) {
        self.weight = Weight::INFINITY;
        self.fault = Some(kind);
        self.first = Some(error_at(at, None));
        self.stopped = true;
    }

    fn fail(&mut self, f: Fault, at: Pos) -> Result<(), Stop> {
        let kind = match f {
            Fault::Syntax | Fault::Duplicate => ErrorKind::Syntax,
            Fault::Limit => ErrorKind::Limit,
            Fault::Internal => ErrorKind::Internal,
        };
        self.stop(kind, at);
        Err(Stop)
    }

    /// Una transición de M: δ(estado, a, cima).
    fn step(&mut self, a: u32, at: Pos) -> Result<(), Stop> {
        let t = self.t;
        let idx = (self.state as usize)
            .checked_mul(t.n_classes as usize)
            .and_then(|i| i.checked_add(a as usize));
        let Some(tr) = idx
            .and_then(|i| t.delta.get(i))
            .and_then(|&id| t.trans.get(id as usize))
            .copied()
        else {
            return self.fail(Fault::Internal, at);
        };
        if let Op::Fail(f) = tr.op {
            return self.fail(f, at);
        }
        if tr.bit != NO_BIT {
            match self.set_bit(tr.bit) {
                Some(true) => {}
                Some(false) => return self.fail(Fault::Duplicate, at),
                None => return self.fail(Fault::Internal, at),
            }
        }
        self.run_checks(&tr, at)?;
        match tr.op {
            Op::Move => self.state = tr.next,
            Op::Push => {
                self.push(tr.frame, tr.ret).or_else(|f| self.fail(f, at))?;
                self.state = tr.next;
            }
            Op::Pop => match self.pop() {
                Some(ret) => self.state = ret,
                None => return self.fail(Fault::Internal, at),
            },
            Op::Enter => {
                self.scalar_ret = tr.ret;
                self.state = tr.next;
            }
            Op::Leave => self.state = self.scalar_ret,
            Op::Fail(_) => {}
        }
        Ok(())
    }

    /// Evalúa las comprobaciones en orden de costo: cada una que falla suma
    /// 1 al peso; en fallo temprano la primera que falla (la de menor
    /// costo) detiene el núcleo.
    fn run_checks(&mut self, tr: &Trans, at: Pos) -> Result<(), Stop> {
        let t = self.t;
        let ini = tr.checks as usize;
        let Some(checks) = t.checks.get(ini..ini.saturating_add(tr.n_checks as usize)) else {
            return self.fail(Fault::Internal, at);
        };
        for c in checks {
            match self.holds(c) {
                Some(true) => {}
                Some(false) => {
                    self.weight = self.weight.times(Weight::ONE);
                    if self.first.is_none() {
                        self.first = Some(error_at(at, Some(c.rule)));
                    }
                    if self.mode == Mode::EarlyFail {
                        self.stopped = true;
                        return Err(Stop);
                    }
                }
                None => return self.fail(Fault::Internal, at),
            }
        }
        Ok(())
    }

    /// Si se cumple `c`; `None` si las tablas son incoherentes.
    fn holds(&self, c: &Check) -> Option<bool> {
        match c.test {
            Test::Fails => Some(false),
            Test::Integer => Some(self.num.is_integer()),
            Test::Mask => self.contains(c.arg),
            Test::Dependent => {
                let ini = c.arg as usize;
                let deps = self.t.deps.get(ini..ini.saturating_add(c.len as usize))?;
                for d in deps {
                    if self.bit(d.bit)? && !self.contains(d.mask)? {
                        return Some(false);
                    }
                }
                Some(true)
            }
        }
    }

    /// Registros del marco de la cima: `(inicio, palabras)` en `mem`.
    fn top(&self) -> Option<(usize, usize)> {
        let header = *self.mem.get(self.sp.checked_sub(1)?)?;
        let frame = self.t.frames.get((header & 0xFFFF_FFFF) as usize)?;
        let words = frame.words as usize;
        Some((self.sp.checked_sub(1 + words)?, words))
    }

    fn bit(&self, bit: u32) -> Option<bool> {
        let (ini, words) = self.top()?;
        let w = (bit / 64) as usize;
        if w >= words {
            return None;
        }
        Some(self.mem[ini + w] & (1 << (bit % 64)) != 0)
    }

    /// Fija `bit` en la máscara de la cima: `Some(false)` si ya estaba.
    fn set_bit(&mut self, bit: u32) -> Option<bool> {
        let (ini, words) = self.top()?;
        let w = (bit / 64) as usize;
        if w >= words {
            return None;
        }
        let m = 1 << (bit % 64);
        let word = &mut self.mem[ini + w];
        let nuevo = *word & m == 0;
        *word |= m;
        Some(nuevo)
    }

    /// Si la máscara de la cima contiene `masks[mask .. mask + palabras]`.
    fn contains(&self, mask: u32) -> Option<bool> {
        let (ini, words) = self.top()?;
        let m = mask as usize;
        let want = self.t.masks.get(m..m.checked_add(words)?)?;
        let have = self.mem.get(ini..ini + words)?;
        Some(want.iter().zip(have).all(|(w, h)| h & w == *w))
    }

    fn push(&mut self, frame: u32, ret: u32) -> Result<(), Fault> {
        let f = self.t.frames.get(frame as usize).ok_or(Fault::Internal)?;
        let words = f.words as usize;
        let fin = self
            .sp
            .checked_add(words + 1)
            .filter(|&e| e <= self.mem.len())
            .ok_or(Fault::Limit)?;
        self.mem[self.sp..fin - 1].fill(0);
        self.mem[fin - 1] = (u64::from(ret) << 32) | u64::from(frame);
        self.sp = fin;
        self.depth += 1;
        self.max_depth = self.max_depth.max(self.depth);
        Ok(())
    }

    /// Desapila y devuelve el estado de retorno.
    fn pop(&mut self) -> Option<u32> {
        let (ini, _) = self.top()?;
        let header = self.mem[self.sp - 1];
        self.sp = ini;
        self.depth -= 1;
        Some((header >> 32) as u32)
    }
}

impl Sink<Event> for Core<'_, '_> {
    type Error = Stop;

    fn event(&mut self, ev: Event, span: Span) -> Result<(), Stop> {
        Core::event(self, ev, span)
    }
}

fn error_at(at: Pos, rule: Option<u32>) -> ErrorAt {
    ErrorAt {
        offset: at.offset,
        line: at.line,
        column: at.column,
        rule,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numero(lexema: &str) -> Number {
        let mut n = Number::default();
        let mut part = NumPart::Int;
        for b in lexema.bytes() {
            part = match (part, b) {
                (_, b'-') if part == NumPart::Int && n.frac == 0 && !n.nonzero && n.zeros == 0 => {
                    NumPart::Minus
                }
                (_, b'.') => NumPart::Dot,
                (_, b'e' | b'E') => NumPart::ExpMark,
                (NumPart::ExpMark, b'+' | b'-') => NumPart::ExpSign,
                (NumPart::ExpMark | NumPart::ExpSign | NumPart::Exp, _) => NumPart::Exp,
                (NumPart::Dot | NumPart::Frac, _) => NumPart::Frac,
                _ => NumPart::Int,
            };
            n.byte(part, b);
        }
        n
    }

    /// Entero = mantisa nula o exponente + ceros finales ≥ cifras de la
    /// fracción (`docs/fase4/automata.md` §3.3).
    #[test]
    fn entero_sin_lexema() {
        for l in [
            "0", "-0", "1.0", "1e2", "10e-1", "100e-2", "0.000e-9", "1.50e1",
        ] {
            assert!(numero(l).is_integer(), "{l}");
        }
        for l in ["1.5", "1e-1", "0.10", "100e-3", "-2.25e1", "1.01e1"] {
            assert!(!numero(l).is_integer(), "{l}");
        }
    }

    #[test]
    fn exponente_satura() {
        let mut n = numero("1.5e");
        for _ in 0..100 {
            n.byte(NumPart::Exp, b'9');
        }
        assert_eq!(n.exp, EXP_CAP);
        assert!(n.is_integer());
        n.exp_neg = true;
        assert!(!n.is_integer());
    }
}
