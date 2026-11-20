//! Núcleo del ejecutor de M (F4-U9, D51): lee las [`Tables`], usa la
//! memoria del llamador como pila de marcos y recibe los eventos del lexer.
//!
//! Por evento hace una consulta a δ, las comprobaciones de la transición
//! (acotadas por el esquema) y a lo sumo una operación de pila: O(1) por
//! evento respecto del payload, sin recursión y sin reservar memoria. La
//! pila es `mem`: cada marco son sus registros seguidos de una cabecera
//! `(retorno << 32) | tipo`; la altura es la profundidad de anidamiento de
//! objetos y arreglos, O(d). El escalar abierto (a lo sumo uno) no se apila:
//! su estado de retorno, los registros del número, el lexema del número
//! (≤ [`LEXEME_MAX`] bytes) y el contador de puntos de código de la cadena
//! son globales, de tamaño fijo (`docs/fase4/automata.md` §3).

use lexer::json::Event;
use lexer::xml::Event as XmlEvent;
use lexer::{LexError, Pos, Sink, Span};

use crate::num::{self, Number};
use crate::tables::{class, xclass, Check, CountOp, Fault, Op, Tables, Test, Trans, NO_BIT};
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

/// Longitud máxima del lexema de un número que se guarda
/// (`docs/00-alcance.md` §4): las comprobaciones que lo leen (rangos,
/// `multipleOf`, `enum`/`const` de números) dan `limit` si es más largo;
/// `integer` no lo necesita.
pub const LEXEME_MAX: usize = 1024;

/// El núcleo se detuvo (error de peso ∞ o fallo temprano).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stop;

/// El ejecutor: estado de control, pila en `mem`, registros globales y
/// peso acumulado.
#[derive(Debug)]
pub struct Core<'t, 'm> {
    t: Tables<'t>,
    mem: &'m mut [u64],
    /// Palabras de `mem` de la zona global de salidas del escalar abierto
    /// ([`Tables::scalar_slots`]); la pila empieza aquí.
    base: usize,
    /// Palabras de `mem` ocupadas (zona y pila).
    sp: usize,
    depth: usize,
    max_depth: usize,
    state: u32,
    /// Estado de retorno del escalar abierto.
    scalar_ret: u32,
    num: Number,
    /// Lexema del número abierto: sus primeros [`LEXEME_MAX`] bytes y su
    /// longitud (saturada).
    lex: [u8; LEXEME_MAX],
    lex_len: usize,
    /// Puntos de código de la cadena abierta (tras los escapes), saturado.
    chars: u64,
    /// Se lee el nombre de una instrucción de procesamiento (XML): su primer
    /// carácter de contenido va precedido de `xclass::PI_BODY`.
    pi_target: bool,
    mode: Mode,
    weight: Weight,
    first: Option<ErrorAt>,
    fault: Option<ErrorKind>,
    stopped: bool,
}

impl<'t, 'm> Core<'t, 'm> {
    /// Ejecutor en q₀ con la pila vacía (Z₀). `mem` es la zona global del
    /// escalar ([`Tables::scalar_slots`] palabras) seguida de la pila: cada
    /// marco ocupa `1 + Frame::words` palabras; si no cabe uno, es un error
    /// de límite.
    pub fn new(t: Tables<'t>, opts: &Options, mem: &'m mut [u64]) -> Core<'t, 'm> {
        let base = t.scalar_slots as usize;
        Core {
            state: t.start,
            t,
            mem,
            base,
            sp: base,
            depth: 0,
            max_depth: 0,
            scalar_ret: 0,
            num: Number::default(),
            lex: [0; LEXEME_MAX],
            lex_len: 0,
            chars: 0,
            pi_target: false,
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
            Event::StringBegin => {
                self.chars = 0;
                class::STRING_BEGIN
            }
            Event::StringChar(c) => {
                self.chars = self.chars.saturating_add(1);
                class::CHAR.saturating_add(self.t.char_class(c))
            }
            Event::StringEnd => class::STRING_END,
            Event::NumberBegin => {
                self.num = Number::default();
                self.lex_len = 0;
                class::NUMBER_BEGIN
            }
            Event::Number(part, b) => {
                self.num.byte(part, b);
                if let Some(x) = self.lex.get_mut(self.lex_len) {
                    *x = b;
                }
                self.lex_len = self.lex_len.saturating_add(1);
                class::NUMBER_BYTE
            }
            Event::NumberEnd => class::NUMBER_END,
        };
        self.step(a, span.start)
    }

    /// Procesa un evento del lexer XML (F4.1c). Los caracteres de nombres,
    /// valores, texto e instrucciones van a su clase de carácter; los de un
    /// texto (valor de atributo o contenido) actualizan además los registros
    /// globales del texto abierto: el contador de puntos de código (todos)
    /// y el lexema (sin los espacios XML: el texto de un tipo numérico tras
    /// el *collapse* no los tiene, `docs/fase4/automata.md` §14). Un texto
    /// empieza en la comilla de apertura de un valor o en el `>`/`/>` de una
    /// etiqueta de apertura.
    pub fn xml_event(&mut self, ev: XmlEvent, span: Span) -> Result<(), Stop> {
        if self.stopped {
            return Err(Stop);
        }
        let a = match ev {
            XmlEvent::StartTagOpen => xclass::START_TAG,
            XmlEvent::EndTagOpen => xclass::END_TAG,
            XmlEvent::AttrBegin => xclass::ATTR,
            XmlEvent::AttrValueBegin => {
                self.text_begin();
                xclass::VALUE_BEGIN
            }
            XmlEvent::AttrValueEnd => xclass::VALUE_END,
            XmlEvent::StartTagClose => {
                self.text_begin();
                xclass::TAG_CLOSE
            }
            XmlEvent::EmptyTagClose => {
                self.text_begin();
                xclass::EMPTY_CLOSE
            }
            XmlEvent::EndTagClose => xclass::END_CLOSE,
            XmlEvent::PiOpen => {
                self.pi_target = true;
                xclass::PI_OPEN
            }
            XmlEvent::PiClose => {
                self.pi_target = false;
                xclass::PI_CLOSE
            }
            XmlEvent::PiChar(c) => {
                if self.pi_target {
                    self.pi_target = false;
                    self.step(xclass::PI_BODY, span.start)?;
                }
                class::CHAR.saturating_add(self.t.char_class(c))
            }
            XmlEvent::NameChar(c) => class::CHAR.saturating_add(self.t.char_class(c)),
            XmlEvent::AttrValueChar(c) | XmlEvent::TextChar(c) => {
                self.text_char(c);
                class::CHAR.saturating_add(self.t.char_class(c))
            }
        };
        self.step(a, span.start)
    }

    fn text_begin(&mut self) {
        self.chars = 0;
        self.lex_len = 0;
    }

    fn text_char(&mut self, c: char) {
        self.chars = self.chars.saturating_add(1);
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            return;
        }
        let mut buf = [0u8; 4];
        for &b in c.encode_utf8(&mut buf).as_bytes() {
            if let Some(x) = self.lex.get_mut(self.lex_len) {
                *x = b;
            }
            self.lex_len = self.lex_len.saturating_add(1);
        }
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
        if !self.stopped && (self.state != self.t.accept || self.sp != self.base) {
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
        if tr.mark != NO_BIT && self.set_bit(tr.mark).is_none() {
            return self.fail(Fault::Internal, at);
        }
        if tr.n_counts > 0 && self.counters(&tr).is_none() {
            return self.fail(Fault::Internal, at);
        }
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
                Ok(true) => {}
                Ok(false) => {
                    self.weight = self.weight.times(Weight::ONE);
                    if self.first.is_none() {
                        self.first = Some(error_at(at, Some(c.rule)));
                    }
                    if self.mode == Mode::EarlyFail {
                        self.stopped = true;
                        return Err(Stop);
                    }
                }
                Err(f) => return self.fail(f, at),
            }
        }
        Ok(())
    }

    /// Si se cumple `c`: [`Fault::Internal`] si las tablas son
    /// incoherentes y [`Fault::Limit`] si hace falta un lexema de más de
    /// [`LEXEME_MAX`] bytes.
    fn holds(&self, c: &Check) -> Result<bool, Fault> {
        let internal = Fault::Internal;
        Ok(match c.test {
            Test::Fails => false,
            Test::Integer => self.num.is_integer(),
            Test::Mask => self.contains(c.arg).ok_or(internal)?,
            Test::Dependent => {
                let ini = c.arg as usize;
                let deps = self
                    .t
                    .deps
                    .get(ini..ini.saturating_add(c.len as usize))
                    .ok_or(internal)?;
                for d in deps {
                    let b = self.bit(d.bit).ok_or(internal)?;
                    if b && !self.contains(d.mask).ok_or(internal)? {
                        return Ok(false);
                    }
                }
                true
            }
            Test::CountMin => self.counter(c.counter).ok_or(internal)? >= c.bound(),
            Test::CountMax => self.counter(c.counter).ok_or(internal)? <= c.bound(),
            Test::Unset => !self.bit(c.arg).ok_or(internal)?,
            Test::LengthMin => self.chars >= c.bound(),
            Test::LengthMax => self.chars <= c.bound(),
            Test::NumMin | Test::NumExMin | Test::NumMax | Test::NumExMax => {
                let o = num::compare(self.lexeme()?, self.t.num(c.arg).ok_or(internal)?);
                match c.test {
                    Test::NumMin => o.is_ge(),
                    Test::NumExMin => o.is_gt(),
                    Test::NumMax => o.is_le(),
                    _ => o.is_lt(),
                }
            }
            Test::NumEnum => {
                let x = self.lexeme()?;
                // Búsqueda binaria en los valores ordenados: O(log n) comparaciones.
                let (mut lo, mut hi) = (c.arg, c.arg.checked_add(c.len).ok_or(internal)?);
                let mut found = false;
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    match num::compare(x, self.t.num(mid).ok_or(internal)?) {
                        core::cmp::Ordering::Equal => {
                            found = true;
                            break;
                        }
                        core::cmp::Ordering::Less => hi = mid,
                        core::cmp::Ordering::Greater => lo = mid + 1,
                    }
                }
                found
            }
            Test::MultipleOf => {
                let d = self.t.divisors.get(c.arg as usize).ok_or(internal)?;
                let dl = self.t.num(d.lexeme).ok_or(internal)?;
                num::is_multiple(self.lexeme()?, dl, &d.div)
            }
            Test::NumIn => {
                let x = self.lexeme()?;
                let ini = c.arg as usize;
                let list = self
                    .t
                    .intervals
                    .get(ini..ini.checked_add(c.len as usize).ok_or(internal)?)
                    .ok_or(internal)?;
                // El último intervalo cuyo extremo inferior no pasa de x
                // (bisección: O(log n) comparaciones); x está en la unión
                // solo si está en ese.
                let (mut lo, mut hi) = (0usize, list.len());
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    let l = self.t.num(list[mid].lo).ok_or(internal)?;
                    if num::compare(l, x).is_le() {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                match lo.checked_sub(1).map(|i| list[i]) {
                    None => false,
                    Some(iv) => {
                        let a = num::compare(x, self.t.num(iv.lo).ok_or(internal)?);
                        let b = num::compare(x, self.t.num(iv.hi).ok_or(internal)?);
                        if iv.closed {
                            a.is_ge() && b.is_le()
                        } else {
                            a.is_gt() && b.is_lt()
                        }
                    }
                }
            }
        })
    }

    /// El lexema del número que se cierra, o [`Fault::Limit`] si no cupo.
    fn lexeme(&self) -> Result<&[u8], Fault> {
        self.lex.get(..self.lex_len).ok_or(Fault::Limit)
    }

    /// Registros del marco de la cima: `(inicio, palabras, palabras de la
    /// máscara)` en `mem`; `None` con la pila vacía.
    fn top(&self) -> Option<(usize, usize, usize)> {
        if self.sp <= self.base {
            return None;
        }
        let header = *self.mem.get(self.sp - 1)?;
        let frame = self.t.frames.get((header & 0xFFFF_FFFF) as usize)?;
        let words = frame.words as usize;
        let mask = frame.mask_words as usize;
        if mask > words {
            return None;
        }
        let ini = self.sp.checked_sub(1 + words)?;
        if ini < self.base {
            return None;
        }
        Some((ini, words, mask))
    }

    /// El contador `k` del marco de la cima (la palabra `mask + k`).
    fn counter(&self, k: u32) -> Option<u64> {
        let (ini, words, mask) = self.top()?;
        let w = mask.checked_add(k as usize)?;
        if w >= words {
            return None;
        }
        self.mem.get(ini + w).copied()
    }

    /// Las operaciones de contador de `tr` sobre el marco de la cima.
    fn counters(&mut self, tr: &Trans) -> Option<()> {
        let (ini, words, mask) = self.top()?;
        let i = tr.counts as usize;
        let ops = self
            .t
            .counter_ops
            .get(i..i.checked_add(tr.n_counts as usize)?)?;
        for o in ops {
            let w = mask.checked_add(o.counter as usize)?;
            if w >= words {
                return None;
            }
            let x = self.mem.get_mut(ini + w)?;
            *x = match o.op {
                CountOp::Add => x.saturating_add(1),
                CountOp::Restart => 1,
            };
        }
        Some(())
    }

    fn bit(&self, bit: u32) -> Option<bool> {
        let (ini, _, words) = self.top()?;
        let w = (bit / 64) as usize;
        if w >= words {
            return None;
        }
        Some(self.mem[ini + w] & (1 << (bit % 64)) != 0)
    }

    /// Fija `bit` en la máscara de la cima: `Some(false)` si ya estaba.
    fn set_bit(&mut self, bit: u32) -> Option<bool> {
        let (ini, _, words) = self.top()?;
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
        let (ini, _, words) = self.top()?;
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
        let (ini, _, _) = self.top()?;
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

impl Sink<XmlEvent> for Core<'_, '_> {
    type Error = Stop;

    fn event(&mut self, ev: XmlEvent, span: Span) -> Result<(), Stop> {
        Core::xml_event(self, ev, span)
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
