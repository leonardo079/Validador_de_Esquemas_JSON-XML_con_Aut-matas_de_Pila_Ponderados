//! Núcleo del ejecutor de M (F4-U9, D51): lee las [`Tables`], usa la
//! memoria del llamador como pila de marcos y recibe los eventos del lexer.
//!
//! Por evento hace una consulta a δ, las comprobaciones de la transición
//! (acotadas por el esquema) y a lo sumo una operación de pila: O(1) por
//! evento respecto del payload, sin recursión y sin reservar memoria. La
//! pila es `mem`: cada marco son sus registros, una palabra de buena
//! formación y una cabecera `(retorno << 32) | tipo`; la altura es la
//! profundidad de anidamiento de objetos y arreglos, O(d). El escalar
//! abierto (a lo sumo uno) no se apila: su estado de retorno, los registros
//! del número, el lexema del número (≤ [`LEXEME_MAX`] bytes) y el contador de
//! puntos de código de la cadena son globales, de tamaño fijo
//! (`docs/fase4/automata.md` §3).
//!
//! **Capa de buena formación (F4.6, F4-B7; `docs/fase4/runtime.md` §4).** Lo
//! que M no ve por diseño lo comprueba el núcleo junto a δ, en la misma
//! pasada y con memoria acotada por [`Limits`]: las claves JSON no
//! declaradas repetidas (D2) y los atributos XML sin bit repetidos, por su
//! hash en una reserva que crece desde el final de `mem` (una región por
//! objeto abierto o por etiqueta); el nombre de cierre de **todo** elemento
//! XML, comparado con el de apertura por el hash del QName guardado en su
//! marco; los nombres XML con `:` al principio, al final o repetido; los
//! fines de línea XML (`\r\n` y `\r` → `\n`, y `\t \n \r` → espacio en un
//! valor de atributo); y los límites de profundidad, cadena, claves,
//! referencias XML y tamaño del payload.

use lexer::json::Event;
use lexer::xml::Event as XmlEvent;
use lexer::{LexError, Pos, Sink, Span};

use crate::hash::SipHasher13;
use crate::limits::Limits;
use crate::num::{self, Number};
use crate::tables::{
    class, xclass, Check, CountOp, Fault, Frame, GateKind, Op, Tables, Test, Trans, NO_BIT,
    OUT_IDENTITY, SLOT_CHILD,
};
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

/// Opciones del ejecutor (F4-U18): el modo y los límites. F6.2 los
/// expone en `sv_limits_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
    pub limits: Limits,
}

impl Options {
    /// El modo `mode` con los límites de `docs/00-alcance.md` §4.
    pub const fn new(mode: Mode) -> Options {
        Options {
            mode,
            limits: Limits::DEFAULT,
        }
    }
}

impl Default for Options {
    /// Conformidad total con los límites por defecto.
    fn default() -> Options {
        Options::new(Mode::Total)
    }
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
    limits: Limits,
    /// Entradas de la reserva de hashes, que ocupa las últimas palabras de
    /// `mem` (la entrada i en `mem[len − 1 − i]`) y crece hacia la pila.
    pool_top: usize,
    /// Palabra de buena formación del próximo marco que se apile.
    next_wf: u64,
    /// El payload es XML (la palabra de buena formación es el hash del
    /// nombre, no la región de claves).
    xml: bool,
    /// JSON: la próxima cadena es una clave (tras `{` o `,` en un objeto).
    key_next: bool,
    /// JSON: la cadena abierta es una clave.
    in_key: bool,
    /// XML: qué nombre se está leyendo.
    name: Name,
    /// Hash del nombre o de la clave que se lee, y su forma (`:`).
    hasher: SipHasher13,
    colons: u32,
    first_colon: bool,
    last_colon: bool,
    /// Hash del último nombre XML terminado.
    name_hash: u64,
    /// Offset de un repetido: la comilla de cierre de la clave JSON o el
    /// comienzo del atributo XML.
    dup_at: Pos,
    /// XML: primera entrada de la reserva de la etiqueta abierta.
    tag_region: usize,
    /// XML: referencias de entidad o de carácter leídas.
    refs: u64,
    /// XML: el último carácter de texto fue un `\r` (se normalizó a `\n`).
    prev_cr: bool,
    /// Bytes del tramo de caracteres abierto (cadena, nombre o texto).
    run: u64,
    /// Arreglos con `uniqueItems` abiertos: si > 0, cada valor que se cierra
    /// calcula su hash canónico y lo entrega a su padre (F4.5b).
    hash_depth: u32,
    /// Hashes de elementos guardados en la reserva por esos arreglos.
    unique_count: usize,
    /// Hash de la cadena (valor) abierta.
    vhash: SipHasher13,
    /// Hay un escalar abierto (sus comprobaciones cuentan en la zona).
    scalar_open: bool,
    /// Tipo de marco (sin apilar) del escalar abierto con salidas y su mapa
    /// de salidas ([`OUT_IDENTITY`] si no tiene).
    scalar_frame: u32,
    scalar_outs: u32,
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

/// Dónde cuenta una comprobación o una puerta: la zona de la instancia
/// escalar o el marco de la cima.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Space {
    Zone,
    Top,
}

/// Nombre XML que se está leyendo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Name {
    None,
    /// De una etiqueta de apertura.
    Elem,
    /// De una etiqueta de cierre.
    End,
    /// De un atributo.
    Attr,
    /// De una instrucción de procesamiento (sin comprobar).
    Pi,
}

/// Bit de la palabra de buena formación de un marco JSON que es un objeto;
/// los 32 bits bajos son el comienzo de su región en la reserva.
const WF_OBJECT: u64 = 1 << 63;

/// Palabras de un marco además de sus registros: el acumulador del hash
/// canónico de su valor y la clave del miembro abierto (`uniqueItems`,
/// F4.5b), la de buena formación y la cabecera.
pub const FRAME_OVERHEAD: usize = 4;

/// Etiquetas del hash canónico de un valor (F4.5b).
const TAG_TRUE: u64 = 0x7472_7565;
const TAG_FALSE: u64 = 0x6661_6c73;
const TAG_NULL: u64 = 0x6e75_6c6c;
const TAG_OBJECT: u64 = 0x6f62_6a65;
const TAG_ARRAY: u64 = 0x6172_7261;
const TAG_ZERO: u64 = 0x7a65_726f;

impl<'t, 'm> Core<'t, 'm> {
    /// Ejecutor en q₀ con la pila vacía (Z₀). `mem` es la zona global del
    /// escalar ([`Tables::scalar_slots`] palabras), la pila que crece desde
    /// ahí (cada marco ocupa `Frame::words + FRAME_OVERHEAD` palabras) y la
    /// reserva de hashes de la capa de buena formación, que crece desde el
    /// final; si no cabe un marco o una entrada, es un error de límite.
    /// [`Tables::state_words`] da el tamaño para unos [`Limits`]. Unas
    /// tablas con menos clases que las fijas dan `Internal` en el primer
    /// evento.
    pub fn new(t: Tables<'t>, opts: &Options, mem: &'m mut [u64]) -> Core<'t, 'm> {
        let base = t.scalar_slots as usize;
        let mut c = Core {
            state: t.start,
            t,
            mem,
            base,
            sp: base,
            limits: opts.limits,
            pool_top: 0,
            next_wf: 0,
            xml: false,
            key_next: false,
            in_key: false,
            name: Name::None,
            hasher: SipHasher13::new(opts.limits.hash_key),
            colons: 0,
            first_colon: false,
            last_colon: false,
            name_hash: 0,
            dup_at: Pos::START,
            tag_region: 0,
            refs: 0,
            prev_cr: false,
            run: 0,
            hash_depth: 0,
            unique_count: 0,
            vhash: SipHasher13::new(opts.limits.hash_key),
            scalar_open: false,
            scalar_frame: 0,
            scalar_outs: OUT_IDENTITY,
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
        };
        // El comienzo de una región va en los 32 bits bajos de la palabra de
        // buena formación: la reserva no pasa de 2³² − 1 entradas.
        c.limits.key_pool = c.limits.key_pool.min(u32::MAX as usize);
        if c.t.n_classes <= class::CHAR {
            c.stop(ErrorKind::Internal, Pos::START);
        }
        c
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
        if !matches!(ev, Event::StringChar(_)) {
            self.run = 0;
        }
        let a = match ev {
            Event::BeginObject => {
                self.next_wf = WF_OBJECT | self.pool_top as u64;
                class::BEGIN_OBJECT
            }
            Event::EndObject => class::END_OBJECT,
            Event::BeginArray => {
                self.next_wf = self.pool_top as u64;
                class::BEGIN_ARRAY
            }
            Event::EndArray => class::END_ARRAY,
            Event::Colon => class::COLON,
            Event::Comma => class::COMMA,
            Event::True => class::TRUE,
            Event::False => class::FALSE,
            Event::Null => class::NULL,
            Event::StringBegin => {
                self.chars = 0;
                self.in_key = self.key_next;
                if self.in_key {
                    self.hasher = SipHasher13::new(self.limits.hash_key);
                } else if self.hash_depth > 0 {
                    self.vhash = SipHasher13::new(self.limits.hash_key);
                    self.vhash.write_u8(b's');
                }
                self.dup_at = after(span.start, 1, 1);
                class::STRING_BEGIN
            }
            Event::StringChar(c) => {
                self.chars = self.chars.saturating_add(1);
                self.run_char(c, span.start)?;
                if self.in_key {
                    self.hasher.write_char(c);
                } else if self.hash_depth > 0 {
                    self.vhash.write_char(c);
                }
                // Dónde acaba el carácter en la fuente: un escape (`\u00e9`)
                // ocupa tantas columnas como bytes; un carácter literal, una.
                let len = span.end.saturating_sub(span.start.offset);
                let cols = if len == c.len_utf8() as u64 { 1 } else { len };
                self.dup_at = after(span.start, len, cols);
                class::CHAR.saturating_add(self.t.char_class(c))
            }
            Event::StringEnd => {
                // `dup_at` ya es la comilla de cierre (el `Span` de este evento
                // empieza en la de apertura): ahí se informa una clave
                // repetida, como en la línea base (D50, B14).
                if self.in_key {
                    self.name_hash = self.hasher.finish();
                }
                class::STRING_END
            }
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
        // El cierre de una cadena se informa en su comilla de cierre (donde
        // se detecta; el `Span` del evento empieza en la de apertura).
        let at = if ev == Event::StringEnd {
            self.dup_at
        } else {
            span.start
        };
        // F4.5b: dentro de un arreglo con `uniqueItems`, el hash canónico del
        // valor que se cierra (antes de desapilarlo).
        let value_hash = if self.hash_depth > 0 {
            match ev {
                Event::True => Some(self.h2(TAG_TRUE, 0)),
                Event::False => Some(self.h2(TAG_FALSE, 0)),
                Event::Null => Some(self.h2(TAG_NULL, 0)),
                Event::StringEnd if !self.in_key => Some(self.vhash.finish()),
                Event::NumberEnd => match self.number_hash() {
                    Some(h) => Some(h),
                    None => return self.fail(Fault::Limit, span.start),
                },
                Event::EndObject | Event::EndArray => self.container_hash(),
                _ => None,
            }
        } else {
            None
        };
        self.step(a, at)?;
        if self.hash_depth > 0 {
            if let Some(h) = value_hash {
                self.deliver(h, at)?;
            } else if ev == Event::StringEnd && self.in_key {
                // La clave del miembro abierto del objeto.
                if let Some(i) = self.sp.checked_sub(3).filter(|_| self.sp > self.base) {
                    self.mem[i] = self.name_hash;
                }
            }
        }
        match ev {
            Event::StringChar(_) | Event::StringBegin => self.key_next = false,
            Event::StringEnd => {
                self.in_key = false;
                self.key_next = false;
            }
            Event::BeginObject => self.key_next = true,
            Event::Comma => self.key_next = self.top_is_object(),
            _ => self.key_next = false,
        }
        Ok(())
    }

    /// SipHash-1-3 de dos palabras (la clave de `Limits::hash_key`), con el
    /// byte de dominio `h`: ningún mensaje de cadena (`s`…) o de número
    /// (`n`…) coincide con uno de estos (informe de F4.5b, 7.0).
    fn h2(&self, a: u64, b: u64) -> u64 {
        let mut h = SipHasher13::new(self.limits.hash_key);
        h.write_u8(b'h');
        h.write(&a.to_le_bytes());
        h.write(&b.to_le_bytes());
        h.finish()
    }

    /// El hash canónico del objeto o arreglo de la cima: sus miembros sin
    /// orden (suma) o sus elementos en orden (encadenados).
    fn container_hash(&self) -> Option<u64> {
        let acc = *self.mem.get(self.sp.checked_sub(4)?)?;
        Some(if self.top_is_object() {
            self.h2(TAG_OBJECT, acc)
        } else {
            self.h2(TAG_ARRAY, acc)
        })
    }

    /// El hash canónico del número que se cierra, por valor: 0.D × 10^E con
    /// D sin ceros a los lados (`1`, `1.0` y `10e-1` son iguales). `None`
    /// si el lexema no cupo o el exponente no cabe en un `i64` (`limit`).
    fn number_hash(&self) -> Option<u64> {
        let lex = self.lex.get(..self.lex_len)?;
        let (neg, rest) = match lex.split_first() {
            Some((b'-', r)) => (true, r),
            _ => (false, lex),
        };
        let mant_end = rest
            .iter()
            .position(|&b| b == b'e' || b == b'E')
            .unwrap_or(rest.len());
        let (mant, exp) = rest.split_at(mant_end);
        let int_len = mant.iter().position(|&b| b == b'.').unwrap_or(mant.len());
        // El cero no depende del exponente: se decide antes de leerlo.
        if mant.iter().all(|&b| b == b'0' || b == b'.') {
            return Some(self.h2(TAG_ZERO, 0));
        }
        let mut e: i64 = 0;
        if let Some((_, ex)) = exp.split_first() {
            let (sign, digits) = match ex.split_first() {
                Some((b'-', d)) => (-1i64, d),
                Some((b'+', d)) => (1, d),
                _ => (1, ex),
            };
            for &d in digits {
                e = e.checked_mul(10)?.checked_add(i64::from(d - b'0'))?;
                if e > (1 << 60) {
                    return None;
                }
            }
            e *= sign;
        }
        let digit = |i: usize| -> Option<u8> {
            let b = if i < int_len {
                mant[i]
            } else {
                *mant.get(i + 1)?
            };
            Some(b)
        };
        let n = mant.len() - usize::from(int_len < mant.len());
        let first = (0..n).find(|&i| digit(i) != Some(b'0'));
        let Some(f) = first else {
            return Some(self.h2(TAG_ZERO, 0));
        };
        let l = (0..n).rev().find(|&i| digit(i) != Some(b'0')).unwrap_or(f);
        let exp10 = (int_len as i64) - (f as i64) + e;
        let mut h = SipHasher13::new(self.limits.hash_key);
        h.write_u8(b'n');
        h.write_u8(u8::from(neg));
        for i in f..=l {
            h.write_u8(digit(i)?);
        }
        h.write(&exp10.to_le_bytes());
        Some(h.finish())
    }

    /// Entrega el hash de un valor cerrado a su padre (la cima): en un
    /// objeto, suma H(clave, valor); en un arreglo, lo encadena y, si el
    /// arreglo tiene `uniqueItems`, lo guarda en la reserva.
    fn deliver(&mut self, h: u64, at: Pos) -> Result<(), Stop> {
        let Some((_, f)) = self.top_frame() else {
            return Ok(());
        };
        let (acc_i, aux_i) = (self.sp - 4, self.sp - 3);
        if self.top_is_object() {
            let m = self.h2(self.mem[aux_i], h);
            self.mem[acc_i] = self.mem[acc_i].wrapping_add(m);
        } else {
            self.mem[acc_i] = self.h2(self.mem[acc_i], h);
            if f.unique {
                let len = self.mem.len();
                if self.unique_count >= self.limits.max_unique || len - self.pool_top <= self.sp {
                    return self.fail(Fault::Limit, at);
                }
                self.mem[len - 1 - self.pool_top] = h;
                self.pool_top += 1;
                self.unique_count += 1;
            }
        }
        Ok(())
    }

    /// `uniqueItems` al cerrar el arreglo de la cima: ordena su región de la
    /// reserva (en su sitio, sin reservar) y busca dos hashes iguales.
    fn unique_ok(&mut self) -> Option<bool> {
        let region = (self.top_wf()? & 0xFFFF_FFFF) as usize;
        let len = self.mem.len();
        let (lo, hi) = (len.checked_sub(self.pool_top)?, len.checked_sub(region)?);
        let v = self.mem.get_mut(lo..hi)?;
        v.sort_unstable();
        Some(v.windows(2).all(|w| w[0] != w[1]))
    }

    /// Un carácter de un tramo (cadena, nombre o texto): cuenta sus bytes
    /// contra [`Limits::max_string`].
    fn run_char(&mut self, c: char, at: Pos) -> Result<(), Stop> {
        self.run = self.run.saturating_add(c.len_utf8() as u64);
        if self.run > self.limits.max_string {
            return self.fail(Fault::Limit, at);
        }
        Ok(())
    }

    /// Si el marco de la cima es un objeto JSON.
    fn top_is_object(&self) -> bool {
        !self.xml && self.top_wf().is_some_and(|w| w & WF_OBJECT != 0)
    }

    /// Procesa un evento del lexer XML (F4.1c). Los caracteres de nombres,
    /// valores, texto e instrucciones van a su clase de carácter; los de un
    /// texto (valor de atributo o contenido) actualizan además los registros
    /// globales del texto abierto: el contador de puntos de código (todos)
    /// y el lexema (sin los espacios XML: el texto de un tipo numérico tras
    /// el *collapse* no los tiene, `docs/fase4/automata.md` §14). Un texto
    /// empieza en la comilla de apertura de un valor o en el `>`/`/>` de una
    /// etiqueta de apertura. Antes de δ, la capa de buena formación (F4.6):
    /// forma de los nombres, nombre de cierre, atributos repetidos sin bit,
    /// referencias y fines de línea.
    pub fn xml_event(&mut self, ev: XmlEvent, span: Span) -> Result<(), Stop> {
        if self.stopped {
            return Err(Stop);
        }
        self.xml = true;
        let at = span.start;
        let is_char = matches!(
            ev,
            XmlEvent::NameChar(_)
                | XmlEvent::AttrValueChar(_)
                | XmlEvent::TextChar(_)
                | XmlEvent::PiChar(_)
        );
        if !is_char {
            self.run = 0;
            self.prev_cr = false;
        }
        let a = match ev {
            XmlEvent::StartTagOpen => {
                self.begin_name(Name::Elem);
                self.tag_region = self.pool_top;
                xclass::START_TAG
            }
            XmlEvent::EndTagOpen => {
                self.begin_name(Name::End);
                xclass::END_TAG
            }
            XmlEvent::AttrBegin => {
                self.end_elem_name(at)?;
                xclass::ATTR
            }
            XmlEvent::AttrValueBegin => {
                if self.name == Name::Attr {
                    self.end_name(at)?;
                }
                self.text_begin();
                xclass::VALUE_BEGIN
            }
            XmlEvent::AttrValueEnd => xclass::VALUE_END,
            XmlEvent::StartTagClose => {
                self.end_elem_name(at)?;
                self.text_begin();
                xclass::TAG_CLOSE
            }
            XmlEvent::EmptyTagClose => {
                self.end_elem_name(at)?;
                self.text_begin();
                xclass::EMPTY_CLOSE
            }
            XmlEvent::EndTagClose => {
                if self.name == Name::End {
                    self.end_name(at)?;
                    // XML 1.0 §3 (GIMatch): el nombre de cierre es el de
                    // apertura, prefijo incluido, en todo elemento.
                    if self.top_wf().is_some_and(|w| w != self.name_hash) {
                        return self.fail(Fault::Syntax, at);
                    }
                }
                xclass::END_CLOSE
            }
            XmlEvent::PiOpen => {
                self.name = Name::Pi;
                self.pi_target = true;
                xclass::PI_OPEN
            }
            XmlEvent::PiClose => {
                self.pi_target = false;
                xclass::PI_CLOSE
            }
            XmlEvent::PiChar(c) => {
                self.run_char(c, at)?;
                if self.pi_target {
                    self.pi_target = false;
                    self.step(xclass::PI_BODY, at)?;
                }
                class::CHAR.saturating_add(self.t.char_class(c))
            }
            XmlEvent::NameChar(c) => {
                self.run_char(c, at)?;
                if matches!(self.name, Name::Elem | Name::End | Name::Attr) {
                    self.hasher.write_char(c);
                    self.last_colon = c == ':';
                    if c == ':' {
                        if self.colons == 0 && self.run == 1 {
                            self.first_colon = true;
                        }
                        self.colons = self.colons.saturating_add(1);
                    }
                }
                class::CHAR.saturating_add(self.t.char_class(c))
            }
            XmlEvent::AttrValueChar(c) | XmlEvent::TextChar(c) => {
                let attr = matches!(ev, XmlEvent::AttrValueChar(_));
                let Some(c) = self.xml_text_char(c, attr, span)? else {
                    return Ok(());
                };
                self.text_char(c);
                class::CHAR.saturating_add(self.t.char_class(c))
            }
        };
        self.step(a, at)?;
        match ev {
            XmlEvent::AttrBegin => {
                self.begin_name(Name::Attr);
                self.dup_at = at;
            }
            XmlEvent::AttrValueBegin | XmlEvent::EndTagClose | XmlEvent::PiClose => {
                self.name = Name::None;
            }
            XmlEvent::StartTagClose | XmlEvent::EmptyTagClose => {
                // Los atributos de la etiqueta ya no se repiten.
                self.pool_top = self.pool_top.min(self.tag_region);
                self.name = Name::None;
            }
            _ => {}
        }
        Ok(())
    }

    /// Empieza a leer un nombre XML.
    fn begin_name(&mut self, n: Name) {
        self.name = n;
        self.hasher = SipHasher13::new(self.limits.hash_key);
        self.colons = 0;
        self.first_colon = false;
        self.last_colon = false;
    }

    /// Termina el nombre que se lee: su hash en `name_hash`; `syntax` si no
    /// es un QName (`:` al principio, al final o más de uno; Namespaces in
    /// XML §3).
    fn end_name(&mut self, at: Pos) -> Result<(), Stop> {
        self.name_hash = self.hasher.finish();
        if self.colons > 1 || self.first_colon || self.last_colon {
            return self.fail(Fault::Syntax, at);
        }
        Ok(())
    }

    /// Fin del nombre de una etiqueta de apertura: su hash irá en la palabra
    /// de buena formación del marco que se apile.
    fn end_elem_name(&mut self, at: Pos) -> Result<(), Stop> {
        if self.name == Name::Elem {
            self.end_name(at)?;
            self.next_wf = self.name_hash;
            self.name = Name::None;
        }
        Ok(())
    }

    /// Un carácter de texto o de un valor de atributo antes de δ: cuenta
    /// las referencias ([`Limits::max_xml_refs`]; una referencia es un
    /// evento cuyo rango es más largo que el carácter que da) y normaliza
    /// los fines de línea literales (XML 1.0 §2.11: `\r\n` y `\r` → `\n`)
    /// y, en un valor, los espacios (§3.3.3: `\t \n` → espacio). `None`: el
    /// `\n` de un `\r\n`, que no llega a M.
    fn xml_text_char(&mut self, c: char, attr: bool, span: Span) -> Result<Option<char>, Stop> {
        let len = span.end.saturating_sub(span.start.offset);
        let from_ref = len != c.len_utf8() as u64;
        let mut c = c;
        if from_ref {
            self.prev_cr = false;
            self.refs = self.refs.saturating_add(1);
            if self.refs > self.limits.max_xml_refs {
                self.fail(Fault::Limit, span.start)?;
            }
        } else {
            if c == '\n' && self.prev_cr {
                self.prev_cr = false;
                return Ok(None);
            }
            self.prev_cr = c == '\r';
            if c == '\r' {
                c = '\n';
            }
            if attr && matches!(c, '\t' | '\n') {
                c = ' ';
            }
        }
        self.run_char(c, span.start)?;
        Ok(Some(c))
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

    /// Error `limit` en `at` (el tamaño del payload, que vigila el
    /// validador que alimenta al núcleo).
    pub(crate) fn limit(&mut self, at: Pos) {
        if !self.stopped {
            self.stop(ErrorKind::Limit, at);
        }
    }

    /// La capa de buena formación sobre la transición `tr` de la clase `a`:
    /// la clave JSON o el atributo XML que termina y no tiene bit en la
    /// máscara entra en la región de su objeto o etiqueta; si ya estaba,
    /// es un repetido (∞, D2), y si la región o la reserva están llenas,
    /// `limit`. Los dos en la comilla de cierre de la clave o en el comienzo
    /// del atributo.
    fn well_formed(&mut self, a: u32, tr: &Trans) -> Result<(), Stop> {
        if tr.bit != NO_BIT {
            return Ok(());
        }
        let region = if self.xml {
            if a != xclass::VALUE_BEGIN || self.name != Name::Attr {
                return Ok(());
            }
            self.tag_region
        } else {
            if a != class::STRING_END || !self.in_key {
                return Ok(());
            }
            match self.top_wf() {
                Some(w) => (w & 0xFFFF_FFFF) as usize,
                None => return self.fail(Fault::Internal, self.dup_at),
            }
        };
        let at = self.dup_at;
        let h = self.name_hash;
        let len = self.mem.len();
        let region = region.min(self.pool_top);
        for i in region..self.pool_top {
            if self.mem[len - 1 - i] == h {
                return self.fail(Fault::Duplicate, at);
            }
        }
        if self.pool_top - region >= self.limits.max_keys
            || self.pool_top - self.unique_count >= self.limits.key_pool
            || len - self.pool_top <= self.sp
        {
            return self.fail(Fault::Limit, at);
        }
        self.mem[len - 1 - self.pool_top] = h;
        self.pool_top += 1;
        Ok(())
    }

    /// Una transición de M: δ(estado, a, cima).
    fn step(&mut self, a: u32, at: Pos) -> Result<(), Stop> {
        let t = self.t;
        if a >= t.n_classes {
            // F4.1a-CR-S2: con tablas incoherentes, nunca la fila siguiente.
            return self.fail(Fault::Internal, at);
        }
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
        self.well_formed(a, &tr)?;
        if tr.bit != NO_BIT {
            match self.set_bit(tr.bit) {
                Some(true) => {}
                // La clave repetida en su comilla de cierre; el atributo, en
                // su comienzo.
                Some(false) => return self.fail(Fault::Duplicate, self.dup_at),
                None => return self.fail(Fault::Internal, at),
            }
        }
        let cur = if self.scalar_open {
            Space::Zone
        } else {
            Space::Top
        };
        self.run_checks(&tr, at, false, cur)?;
        if tr.mark != NO_BIT && self.set_bit(tr.mark).is_none() {
            return self.fail(Fault::Internal, at);
        }
        if tr.n_counts > 0 && self.counters(&tr).is_none() {
            return self.fail(Fault::Internal, at);
        }
        let slotted = tr.outs != OUT_IDENTITY;
        match tr.op {
            Op::Move => {
                self.state = tr.next;
                if slotted {
                    // Un valor de un solo evento con salidas: se abre, cuenta
                    // sus comprobaciones de entrada y se cierra aquí mismo.
                    self.zone_clear(tr.frame);
                    self.run_checks(&tr, at, true, Space::Zone)?;
                    self.close(Space::Zone, tr.frame, at)?;
                    self.propagate(Space::Zone, None, tr.outs, at)?;
                } else {
                    self.run_checks(&tr, at, true, Space::Top)?;
                }
            }
            Op::Push => {
                self.push(tr.frame, tr.ret, tr.outs)
                    .or_else(|f| self.fail(f, at))?;
                self.state = tr.next;
                self.run_checks(&tr, at, true, Space::Top)?;
            }
            Op::Pop => {
                let Some((ini, f)) = self.top_frame() else {
                    return self.fail(Fault::Internal, at);
                };
                let outs = self.frame_outs();
                self.close(Space::Top, self.top_id(), at)?;
                match self.pop() {
                    Some(ret) => self.state = ret,
                    None => return self.fail(Fault::Internal, at),
                }
                // Las salidas del marco cerrado siguen en `mem` por encima de
                // la cima: se suman a las del padre.
                let first_slot = ini + (f.words - f.slots) as usize;
                self.propagate(Space::Top, Some(first_slot), outs, at)?;
            }
            Op::Enter => {
                self.scalar_ret = tr.ret;
                self.state = tr.next;
                self.scalar_open = true;
                if slotted {
                    self.zone_clear(tr.frame);
                    self.scalar_frame = tr.frame;
                    self.scalar_outs = tr.outs;
                } else {
                    self.scalar_outs = OUT_IDENTITY;
                }
                self.run_checks(&tr, at, true, Space::Zone)?;
            }
            Op::Leave => {
                self.state = self.scalar_ret;
                self.scalar_open = false;
                if self.scalar_outs != OUT_IDENTITY {
                    let (f, o) = (self.scalar_frame, self.scalar_outs);
                    self.scalar_outs = OUT_IDENTITY;
                    self.close(Space::Zone, f, at)?;
                    self.propagate(Space::Zone, None, o, at)?;
                }
            }
            Op::Fail(_) => {}
        }
        Ok(())
    }

    /// Pone a 0 las salidas de la zona que usa el tipo de marco `frame`
    /// (solo las suyas: O(salidas del escalar), no de toda la zona).
    fn zone_clear(&mut self, frame: u32) {
        let s = self
            .t
            .frames
            .get(frame as usize)
            .map_or(0, |f| f.slots as usize);
        let n = s.min(self.base).min(self.mem.len());
        self.mem[..n].fill(0);
    }

    /// La palabra de la salida `k` (desde 1) de `space`.
    fn slot_index(&self, space: Space, k: u32) -> Option<usize> {
        let k = (k as usize).checked_sub(1)?;
        match space {
            Space::Zone => (k < self.base).then_some(k),
            Space::Top => {
                let (ini, f) = self.top_frame()?;
                (k < f.slots as usize).then(|| ini + (f.words - f.slots) as usize + k)
            }
        }
    }

    /// Suma `n` al destino `target` de `space` (0: el peso global, atribuido
    /// a `rule`; en fallo temprano, detiene).
    fn add_to(
        &mut self,
        space: Space,
        target: u32,
        n: u64,
        rule: Option<u32>,
        at: Pos,
    ) -> Result<(), Stop> {
        if n == 0 {
            return Ok(());
        }
        if target == 0 {
            self.weight = self.weight.times(Weight::finite(n));
            if self.first.is_none() {
                self.first = Some(error_at(at, rule));
            }
            if self.mode == Mode::EarlyFail {
                self.stopped = true;
                return Err(Stop);
            }
            return Ok(());
        }
        match self.slot_index(space, target) {
            Some(i) => {
                self.mem[i] = self.mem[i].saturating_add(n);
                Ok(())
            }
            None => self.fail(Fault::Internal, at),
        }
    }

    /// Evalúa las puertas del tipo de marco `frame` sobre las salidas de
    /// `space`, en su orden (F4.5a).
    fn close(&mut self, space: Space, frame: u32, at: Pos) -> Result<(), Stop> {
        let t = self.t;
        let Some(f) = t.frames.get(frame as usize).copied() else {
            return self.fail(Fault::Internal, at);
        };
        let ini = f.gates as usize;
        let Some(gates) = t.gates.get(ini..ini.saturating_add(f.n_gates as usize)) else {
            return self.fail(Fault::Internal, at);
        };
        for g in gates {
            let i0 = g.inputs as usize;
            let Some(ins) = t
                .gate_inputs
                .get(i0..i0.saturating_add(g.n_inputs as usize))
            else {
                return self.fail(Fault::Internal, at);
            };
            let mut ceros = 0u32;
            let mut suma = 0u64;
            for &k in ins {
                let Some(i) = self.slot_index(space, k) else {
                    return self.fail(Fault::Internal, at);
                };
                let v = self.mem[i];
                ceros += u32::from(v == 0);
                suma = suma.saturating_add(v);
            }
            let n = ins.len() as u32;
            let add = match g.kind {
                GateKind::Any => u64::from(n > 0 && ceros == 0),
                GateKind::One => u64::from(ceros != 1),
                GateKind::Not => u64::from(n == 1 && ceros == 1),
                GateKind::Implies => {
                    let on = g.bit != NO_BIT
                        && matches!(space, Space::Top)
                        && self.bit(g.bit).unwrap_or(false);
                    if on {
                        suma
                    } else {
                        0
                    }
                }
            };
            self.add_to(space, g.target, add, Some(g.rule), at)?;
        }
        Ok(())
    }

    /// Suma las salidas de la instancia que se cierra (en `space`, o desde
    /// `from` si es un marco ya desapilado) a las del padre (la cima) según
    /// el mapa `outs`.
    fn propagate(
        &mut self,
        space: Space,
        from: Option<usize>,
        outs: u32,
        at: Pos,
    ) -> Result<(), Stop> {
        if outs == OUT_IDENTITY {
            return Ok(());
        }
        let t = self.t;
        let Some(m) = t.outmaps.get(outs as usize).copied() else {
            return self.fail(Fault::Internal, at);
        };
        for j in 0..m.len {
            let Some(&target) = t.out_targets.get((m.off + j) as usize) else {
                return self.fail(Fault::Internal, at);
            };
            let src = match from {
                Some(base) => Some(base + j as usize),
                None => self.slot_index(space, j + 1),
            };
            let Some(v) = src.and_then(|i| self.mem.get(i)).copied() else {
                return self.fail(Fault::Internal, at);
            };
            self.add_to(Space::Top, target, v, None, at)?;
        }
        Ok(())
    }

    /// Evalúa las comprobaciones en orden de costo: cada una que falla suma
    /// 1 al peso; en fallo temprano la primera que falla (la de menor
    /// costo) detiene el núcleo.
    fn run_checks(&mut self, tr: &Trans, at: Pos, child: bool, space: Space) -> Result<(), Stop> {
        let t = self.t;
        let ini = tr.checks as usize;
        let Some(checks) = t.checks.get(ini..ini.saturating_add(tr.n_checks as usize)) else {
            return self.fail(Fault::Internal, at);
        };
        for c in checks {
            if (c.slot & SLOT_CHILD != 0) != child {
                continue;
            }
            let r = if c.test == Test::Unique {
                self.unique_ok().ok_or(Fault::Internal)
            } else {
                self.holds(c)
            };
            match r {
                Ok(true) => {}
                Ok(false) => self.add_to(space, c.slot & !SLOT_CHILD, 1, Some(c.rule), at)?,
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
            Test::Unique => return Err(internal),
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
        // Las salidas van al final de los registros: contadores y máscara
        // solo ven `words − slots`.
        let regs = words.checked_sub(frame.slots as usize)?;
        if mask > regs {
            return None;
        }
        let ini = self.sp.checked_sub(words.checked_add(FRAME_OVERHEAD)?)?;
        if ini < self.base {
            return None;
        }
        Some((ini, regs, mask))
    }

    /// El tipo de marco de la cima y el comienzo de sus registros.
    fn top_frame(&self) -> Option<(usize, Frame)> {
        let (ini, _, _) = self.top()?;
        let f = *self
            .t
            .frames
            .get((self.mem[self.sp - 1] & 0xFFFF_FFFF) as usize)?;
        Some((ini, f))
    }

    /// El id del tipo de marco de la cima.
    fn top_id(&self) -> u32 {
        self.mem
            .get(self.sp.wrapping_sub(1))
            .map_or(u32::MAX, |h| (h & 0xFFFF_FFFF) as u32)
    }

    /// El mapa de salidas del marco de la cima (JSON: en la palabra de buena
    /// formación).
    fn frame_outs(&self) -> u32 {
        if self.xml {
            return OUT_IDENTITY;
        }
        match self.top_wf().map(|w| (w >> 32) & 0x7FFF_FFFF) {
            Some(o) if o > 0 => (o - 1) as u32,
            _ => OUT_IDENTITY,
        }
    }

    /// La palabra de buena formación del marco de la cima.
    fn top_wf(&self) -> Option<u64> {
        self.top()?;
        self.mem.get(self.sp - 2).copied()
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

    fn push(&mut self, frame: u32, ret: u32, outs: u32) -> Result<(), Fault> {
        let f = self.t.frames.get(frame as usize).ok_or(Fault::Internal)?;
        if self.depth >= self.limits.max_depth {
            return Err(Fault::Limit);
        }
        // `checked_add` también en 32 bits (F4C-S7).
        let size = (f.words as usize)
            .checked_add(FRAME_OVERHEAD)
            .ok_or(Fault::Limit)?;
        let free = self.mem.len().saturating_sub(self.pool_top);
        let fin = self
            .sp
            .checked_add(size)
            .filter(|&e| e <= free)
            .ok_or(Fault::Limit)?;
        self.mem[self.sp..fin - 2].fill(0);
        let wf = if outs == OUT_IDENTITY || self.xml {
            self.next_wf
        } else {
            self.next_wf | ((u64::from(outs) + 1) & 0x7FFF_FFFF) << 32
        };
        self.mem[fin - 2] = wf;
        self.mem[fin - 1] = (u64::from(ret) << 32) | u64::from(frame);
        self.sp = fin;
        if f.unique {
            self.hash_depth += 1;
        }
        self.depth += 1;
        self.max_depth = self.max_depth.max(self.depth);
        Ok(())
    }

    /// Desapila y devuelve el estado de retorno. En JSON libera la región
    /// de claves del objeto.
    fn pop(&mut self) -> Option<u32> {
        let (ini, _, _) = self.top()?;
        let header = self.mem[self.sp - 1];
        if !self.xml {
            let region = (self.mem[self.sp - 2] & 0xFFFF_FFFF) as usize;
            if let Some(f) = self.t.frames.get((header & 0xFFFF_FFFF) as usize) {
                if f.unique {
                    self.unique_count -=
                        self.pool_top.saturating_sub(region).min(self.unique_count);
                    self.hash_depth = self.hash_depth.saturating_sub(1);
                }
            }
            self.pool_top = self.pool_top.min(region);
        }
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

/// La posición `bytes` bytes y `cols` columnas después de `p`, en la misma
/// línea (una cadena JSON no tiene saltos de línea literales).
fn after(p: Pos, bytes: u64, cols: u64) -> Pos {
    Pos {
        offset: p.offset.saturating_add(bytes),
        line: p.line,
        column: u32::try_from(u64::from(p.column).saturating_add(cols)).unwrap_or(u32::MAX),
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
