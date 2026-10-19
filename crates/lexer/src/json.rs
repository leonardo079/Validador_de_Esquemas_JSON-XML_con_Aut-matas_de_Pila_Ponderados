//! Tokenizador JSON (RFC 8259) en streaming.
//!
//! Es un AFD de tabla con salida (máquina de Mealy): `TABLE[estado][clase]`
//! da el siguiente estado y una acción que emite eventos. Los bytes se agrupan
//! en 31 clases de equivalencia ([`CLASS`]).
//!
//! Dos registros acotados complementan la tabla sin cambiar su naturaleza
//! finita: el valor de un escape `\uXXXX` (para combinar pares sustitutos) y el
//! decodificador [`Utf8Decoder`] dentro de las cadenas, que es otro AFD que
//! corre en producto con este.
//!
//! El estado completo cabe en [`JsonLexer`] (unas pocas decenas de bytes), por
//! eso la entrada puede llegar en bloques de cualquier tamaño, incluso de un
//! byte, y un token puede quedar partido entre dos bloques.

use crate::table::{fill, set, tr, Tr};
use crate::utf8::{Step, Utf8Decoder};
use crate::{Error, LexError, LexErrorKind, Pos, Sink, Span};

/// Evento léxico. Los eventos de cadena y número permiten procesar el
/// contenido sin copiarlo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    BeginObject,
    EndObject,
    BeginArray,
    EndArray,
    Colon,
    Comma,
    True,
    False,
    Null,
    /// Comilla de apertura.
    StringBegin,
    /// Un carácter de la cadena, ya sin escapes ni codificación UTF-8.
    StringChar(char),
    /// Comilla de cierre; el `Span` cubre la cadena completa.
    StringEnd,
    NumberBegin,
    /// Un byte del lexema del número, con la parte a la que pertenece.
    Number(NumPart, u8),
    /// Fin del número; el `Span` cubre el lexema completo.
    NumberEnd,
}

/// Parte de un número: `-? int (. frac)? ((e|E) (+|-)? exp)?`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumPart {
    Minus,
    Int,
    Dot,
    Frac,
    ExpMark,
    ExpSign,
    Exp,
}

// ---------------------------------------------------------------- clases ---

const C_SPACE: usize = 0;
const C_WSCTRL: usize = 1; // \t \n \r: espacio fuera de cadenas, prohibidos dentro
const C_LBRACE: usize = 2;
const C_RBRACE: usize = 3;
const C_LBRACK: usize = 4;
const C_RBRACK: usize = 5;
const C_COLON: usize = 6;
const C_COMMA: usize = 7;
const C_QUOTE: usize = 8;
const C_BSLASH: usize = 9;
const C_SLASH: usize = 10;
const C_MINUS: usize = 11;
const C_PLUS: usize = 12;
const C_DOT: usize = 13;
const C_ZERO: usize = 14;
const C_DIGIT: usize = 15; // 1–9
const C_A: usize = 16;
const C_B: usize = 17;
const C_E: usize = 18;
const C_F: usize = 19;
const C_L: usize = 20;
const C_N: usize = 21;
const C_R: usize = 22;
const C_S: usize = 23;
const C_T: usize = 24;
const C_U: usize = 25;
const C_UPPER_E: usize = 26;
const C_HEX: usize = 27; // c d A B C D F: solo dígitos hexadecimales
const C_CTRL: usize = 28; // 00–1F salvo \t \n \r
const C_OTHER: usize = 29; // resto del ASCII imprimible
const C_NONASCII: usize = 30; // 80–FF

pub const NUM_CLASSES: usize = 31;

pub const CLASS_NAMES: [&str; NUM_CLASSES] = [
    "␠",
    "\\t \\n \\r",
    "{",
    "}",
    "[",
    "]",
    ":",
    ",",
    "\"",
    "\\",
    "/",
    "-",
    "+",
    ".",
    "0",
    "1–9",
    "a",
    "b",
    "e",
    "f",
    "l",
    "n",
    "r",
    "s",
    "t",
    "u",
    "E",
    "c d A–D F",
    "control",
    "otro ASCII",
    "80–FF",
];

const DIGITS: [usize; 2] = [C_ZERO, C_DIGIT];
const HEX: [usize; 8] = [C_ZERO, C_DIGIT, C_A, C_B, C_E, C_F, C_UPPER_E, C_HEX];

/// Clase de cada byte.
pub static CLASS: [u8; 256] = build_classes();

const fn build_classes() -> [u8; 256] {
    let mut c = [C_OTHER as u8; 256];
    let mut b = 0;
    while b < 256 {
        c[b] = match b as u8 {
            b' ' => C_SPACE,
            b'\t' | b'\n' | b'\r' => C_WSCTRL,
            0x00..=0x1F => C_CTRL,
            0x80..=0xFF => C_NONASCII,
            b'{' => C_LBRACE,
            b'}' => C_RBRACE,
            b'[' => C_LBRACK,
            b']' => C_RBRACK,
            b':' => C_COLON,
            b',' => C_COMMA,
            b'"' => C_QUOTE,
            b'\\' => C_BSLASH,
            b'/' => C_SLASH,
            b'-' => C_MINUS,
            b'+' => C_PLUS,
            b'.' => C_DOT,
            b'0' => C_ZERO,
            b'1'..=b'9' => C_DIGIT,
            b'a' => C_A,
            b'b' => C_B,
            b'e' => C_E,
            b'f' => C_F,
            b'l' => C_L,
            b'n' => C_N,
            b'r' => C_R,
            b's' => C_S,
            b't' => C_T,
            b'u' => C_U,
            b'E' => C_UPPER_E,
            b'c' | b'd' | b'A'..=b'D' | b'F' => C_HEX,
            _ => C_OTHER,
        } as u8;
        b += 1;
    }
    c
}

// --------------------------------------------------------------- estados ---

const S_VALUE: usize = 0; // entre tokens
const S_T1: usize = 1; // t
const S_T2: usize = 2; // tr
const S_T3: usize = 3; // tru
const S_F1: usize = 4; // f
const S_F2: usize = 5; // fa
const S_F3: usize = 6; // fal
const S_F4: usize = 7; // fals
const S_N1: usize = 8; // n
const S_N2: usize = 9; // nu
const S_N3: usize = 10; // nul
const S_MINUS: usize = 11; // -
const S_ZERO: usize = 12; // -?0            (acepta)
const S_INT: usize = 13; // -?[1-9][0-9]*   (acepta)
const S_DOT: usize = 14; // int .
const S_FRAC: usize = 15; // int . [0-9]+   (acepta)
const S_EXP: usize = 16; // … e
const S_EXP_SIGN: usize = 17; // … e±
const S_EXP_DIG: usize = 18; // … e±?[0-9]+ (acepta)
const S_STR: usize = 19; // dentro de una cadena
const S_ESC: usize = 20; // tras '\'
const S_U1: usize = 21; // \u
const S_U2: usize = 22; // \uX
const S_U3: usize = 23; // \uXX
const S_U4: usize = 24; // \uXXX
const S_LOW_BS: usize = 25; // tras sustituto alto: se exige '\'
const S_LOW_U: usize = 26; // … '\' se exige 'u'
const S_L1: usize = 27; // \u (sustituto bajo)
const S_L2: usize = 28;
const S_L3: usize = 29;
const S_L4: usize = 30;
const S_ERR: usize = 31;

pub const NUM_STATES: usize = 32;

pub const STATE_NAMES: [&str; NUM_STATES] = [
    "valor",
    "t",
    "tr",
    "tru",
    "f",
    "fa",
    "fal",
    "fals",
    "n",
    "nu",
    "nul",
    "-",
    "0",
    "int",
    "int.",
    "frac",
    "e",
    "e±",
    "exp",
    "cadena",
    "\\",
    "\\u",
    "\\uX",
    "\\uXX",
    "\\uXXX",
    "alto",
    "alto\\",
    "alto\\u",
    "alto\\uX",
    "alto\\uXX",
    "alto\\uXXX",
    "error",
];

/// Estados de aceptación: entre tokens o al final de un número completo.
pub const fn is_accepting(s: usize) -> bool {
    matches!(s, S_VALUE | S_ZERO | S_INT | S_FRAC | S_EXP_DIG)
}

// -------------------------------------------------------------- acciones ---

const A_ERR: u8 = 0;
const A_SKIP: u8 = 1;
const A_PUNCT: u8 = 2;
const A_LIT_START: u8 = 3;
const A_LIT_END: u8 = 4;
const A_NUM_BEGIN: u8 = 5;
const A_NUM_BYTE: u8 = 6;
/// Fin de número: el byte actual no le pertenece y se reprocesa en `S_VALUE`.
const A_NUM_END: u8 = 7;
const A_STR_BEGIN: u8 = 8;
const A_STR_CHAR: u8 = 9;
const A_STR_END: u8 = 10;
const A_ESC_START: u8 = 11;
const A_ESC_CHAR: u8 = 12;
const A_U_START: u8 = 13;
const A_HEX: u8 = 14;
const A_HEX_LAST: u8 = 15;
const A_LOW_HEX_LAST: u8 = 16;
const A_UTF8: u8 = 17;

const ERR: Tr = tr(S_ERR, A_ERR);

type Table = [[Tr; NUM_CLASSES]; NUM_STATES];

static TABLE: Table = build_table();

const fn build_table() -> Table {
    let mut t = [[ERR; NUM_CLASSES]; NUM_STATES];

    // Entre tokens.
    set(&mut t, S_VALUE, &[C_SPACE, C_WSCTRL], tr(S_VALUE, A_SKIP));
    set(
        &mut t,
        S_VALUE,
        &[C_LBRACE, C_RBRACE, C_LBRACK, C_RBRACK, C_COLON, C_COMMA],
        tr(S_VALUE, A_PUNCT),
    );
    set(&mut t, S_VALUE, &[C_QUOTE], tr(S_STR, A_STR_BEGIN));
    set(&mut t, S_VALUE, &[C_MINUS], tr(S_MINUS, A_NUM_BEGIN));
    set(&mut t, S_VALUE, &[C_ZERO], tr(S_ZERO, A_NUM_BEGIN));
    set(&mut t, S_VALUE, &[C_DIGIT], tr(S_INT, A_NUM_BEGIN));
    set(&mut t, S_VALUE, &[C_T], tr(S_T1, A_LIT_START));
    set(&mut t, S_VALUE, &[C_F], tr(S_F1, A_LIT_START));
    set(&mut t, S_VALUE, &[C_N], tr(S_N1, A_LIT_START));

    // Literales: true, false, null.
    set(&mut t, S_T1, &[C_R], tr(S_T2, A_SKIP));
    set(&mut t, S_T2, &[C_U], tr(S_T3, A_SKIP));
    set(&mut t, S_T3, &[C_E], tr(S_VALUE, A_LIT_END));
    set(&mut t, S_F1, &[C_A], tr(S_F2, A_SKIP));
    set(&mut t, S_F2, &[C_L], tr(S_F3, A_SKIP));
    set(&mut t, S_F3, &[C_S], tr(S_F4, A_SKIP));
    set(&mut t, S_F4, &[C_E], tr(S_VALUE, A_LIT_END));
    set(&mut t, S_N1, &[C_U], tr(S_N2, A_SKIP));
    set(&mut t, S_N2, &[C_L], tr(S_N3, A_SKIP));
    set(&mut t, S_N3, &[C_L], tr(S_VALUE, A_LIT_END));

    // Números. En los estados de aceptación cualquier byte ajeno al número lo
    // termina y se reprocesa; en los demás es un error.
    let end = tr(S_VALUE, A_NUM_END);
    fill(&mut t, S_ZERO, end);
    fill(&mut t, S_INT, end);
    fill(&mut t, S_FRAC, end);
    fill(&mut t, S_EXP_DIG, end);
    set(&mut t, S_MINUS, &[C_ZERO], tr(S_ZERO, A_NUM_BYTE));
    set(&mut t, S_MINUS, &[C_DIGIT], tr(S_INT, A_NUM_BYTE));
    set(&mut t, S_ZERO, &DIGITS, ERR); // sin ceros a la izquierda
    set(&mut t, S_ZERO, &[C_DOT], tr(S_DOT, A_NUM_BYTE));
    set(&mut t, S_ZERO, &[C_E, C_UPPER_E], tr(S_EXP, A_NUM_BYTE));
    set(&mut t, S_INT, &DIGITS, tr(S_INT, A_NUM_BYTE));
    set(&mut t, S_INT, &[C_DOT], tr(S_DOT, A_NUM_BYTE));
    set(&mut t, S_INT, &[C_E, C_UPPER_E], tr(S_EXP, A_NUM_BYTE));
    set(&mut t, S_DOT, &DIGITS, tr(S_FRAC, A_NUM_BYTE));
    set(&mut t, S_FRAC, &DIGITS, tr(S_FRAC, A_NUM_BYTE));
    set(&mut t, S_FRAC, &[C_E, C_UPPER_E], tr(S_EXP, A_NUM_BYTE));
    set(
        &mut t,
        S_EXP,
        &[C_PLUS, C_MINUS],
        tr(S_EXP_SIGN, A_NUM_BYTE),
    );
    set(&mut t, S_EXP, &DIGITS, tr(S_EXP_DIG, A_NUM_BYTE));
    set(&mut t, S_EXP_SIGN, &DIGITS, tr(S_EXP_DIG, A_NUM_BYTE));
    set(&mut t, S_EXP_DIG, &DIGITS, tr(S_EXP_DIG, A_NUM_BYTE));
    // Un número seguido de algo que solo puede continuar un número mal
    // formado (`1.2.3`, `1e5e`, `-0-`) es error aquí y no un segundo token.
    set(&mut t, S_INT, &[C_MINUS, C_PLUS], ERR);
    set(&mut t, S_ZERO, &[C_MINUS, C_PLUS], ERR);
    set(&mut t, S_FRAC, &[C_DOT, C_MINUS, C_PLUS], ERR);
    set(
        &mut t,
        S_EXP_DIG,
        &[C_DOT, C_E, C_UPPER_E, C_MINUS, C_PLUS],
        ERR,
    );

    // Cadenas.
    fill(&mut t, S_STR, tr(S_STR, A_STR_CHAR));
    set(&mut t, S_STR, &[C_QUOTE], tr(S_VALUE, A_STR_END));
    set(&mut t, S_STR, &[C_BSLASH], tr(S_ESC, A_ESC_START));
    set(&mut t, S_STR, &[C_CTRL, C_WSCTRL], ERR);
    set(&mut t, S_STR, &[C_NONASCII], tr(S_STR, A_UTF8));

    // Escapes: \" \\ \/ \b \f \n \r \t \uXXXX.
    set(
        &mut t,
        S_ESC,
        &[C_QUOTE, C_BSLASH, C_SLASH, C_B, C_F, C_N, C_R, C_T],
        tr(S_STR, A_ESC_CHAR),
    );
    set(&mut t, S_ESC, &[C_U], tr(S_U1, A_U_START));
    set(&mut t, S_U1, &HEX, tr(S_U2, A_HEX));
    set(&mut t, S_U2, &HEX, tr(S_U3, A_HEX));
    set(&mut t, S_U3, &HEX, tr(S_U4, A_HEX));
    // A_HEX_LAST desvía a S_LOW_BS si el valor es un sustituto alto.
    set(&mut t, S_U4, &HEX, tr(S_STR, A_HEX_LAST));

    // Pareja de un sustituto alto: \uDC00–\uDFFF.
    set(&mut t, S_LOW_BS, &[C_BSLASH], tr(S_LOW_U, A_SKIP));
    set(&mut t, S_LOW_U, &[C_U], tr(S_L1, A_U_START));
    set(&mut t, S_L1, &HEX, tr(S_L2, A_HEX));
    set(&mut t, S_L2, &HEX, tr(S_L3, A_HEX));
    set(&mut t, S_L3, &HEX, tr(S_L4, A_HEX));
    set(&mut t, S_L4, &HEX, tr(S_STR, A_LOW_HEX_LAST));

    t
}

fn error_kind(state: usize) -> LexErrorKind {
    match state {
        S_VALUE => LexErrorKind::UnexpectedByte,
        S_T1..=S_N3 => LexErrorKind::InvalidLiteral,
        S_MINUS..=S_EXP_DIG => LexErrorKind::InvalidNumber,
        S_STR => LexErrorKind::ControlCharInString,
        S_LOW_BS | S_LOW_U => LexErrorKind::LoneSurrogate,
        _ => LexErrorKind::InvalidEscape,
    }
}

fn num_part(state: usize) -> NumPart {
    match state {
        S_MINUS => NumPart::Minus,
        S_ZERO | S_INT => NumPart::Int,
        S_DOT => NumPart::Dot,
        S_FRAC => NumPart::Frac,
        S_EXP => NumPart::ExpMark,
        S_EXP_SIGN => NumPart::ExpSign,
        _ => NumPart::Exp,
    }
}

fn hex_value(b: u8) -> u32 {
    u32::from(match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
    })
}

// ----------------------------------------------------------------- lexer ---

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Tokenizador JSON reanudable. No reserva memoria.
#[derive(Clone, Debug)]
pub struct JsonLexer {
    state: u8,
    /// Posición del próximo byte.
    pos: Pos,
    /// Inicio del token en curso (cadena, número o literal).
    tok_start: Pos,
    /// Inicio del carácter en curso dentro de una cadena (escape o UTF-8).
    char_start: Pos,
    /// Registro del escape `\uXXXX` en curso.
    hex: u32,
    /// Sustituto alto pendiente de su pareja.
    high: u32,
    utf8: Utf8Decoder,
    /// Bytes del BOM inicial ya consumidos; 3 = fase de BOM terminada.
    bom: u8,
    failed: Option<LexError>,
}

impl Default for JsonLexer {
    fn default() -> Self {
        Self::new()
    }
}

type Res<S> = Result<(), Error<<S as Sink<Event>>::Error>>;

impl JsonLexer {
    pub const fn new() -> Self {
        Self {
            state: S_VALUE as u8,
            pos: Pos::START,
            tok_start: Pos::START,
            char_start: Pos::START,
            hex: 0,
            high: 0,
            utf8: Utf8Decoder::new(),
            bom: 0,
            failed: None,
        }
    }

    /// Posición del próximo byte que se espera.
    pub fn position(&self) -> Pos {
        self.pos
    }

    /// Consume un bloque de la entrada. Tras un error léxico el lexer queda
    /// detenido y devuelve siempre el mismo error.
    pub fn feed<S: Sink<Event>>(&mut self, chunk: &[u8], sink: &mut S) -> Res<S> {
        if let Some(e) = self.failed {
            return Err(Error::Lex(e));
        }
        for &b in chunk {
            self.byte(b, sink)?;
            self.pos.advance(b);
        }
        Ok(())
    }

    /// Indica el fin de la entrada: cierra un número pendiente o informa de
    /// un token incompleto.
    pub fn finish<S: Sink<Event>>(&mut self, sink: &mut S) -> Res<S> {
        if let Some(e) = self.failed {
            return Err(Error::Lex(e));
        }
        // Una secuencia UTF-8 truncada (también un BOM a medias) es un error
        // de codificación, igual que en XML.
        if (1..3).contains(&self.bom) {
            return self.fail_at(LexErrorKind::InvalidUtf8, Pos::START);
        }
        if self.utf8.is_pending() {
            return self.fail_at(LexErrorKind::InvalidUtf8, self.char_start);
        }
        match usize::from(self.state) {
            S_VALUE => Ok(()),
            S_ZERO | S_INT | S_FRAC | S_EXP_DIG => {
                self.state = S_VALUE as u8;
                self.emit_until(sink, Event::NumberEnd, self.tok_start, self.pos.offset)
            }
            S_MINUS | S_DOT | S_EXP | S_EXP_SIGN => self.fail(LexErrorKind::InvalidNumber),
            _ => self.fail(LexErrorKind::UnexpectedEof),
        }
    }

    fn byte<S: Sink<Event>>(&mut self, b: u8, sink: &mut S) -> Res<S> {
        if self.bom < 3 {
            if b == BOM[usize::from(self.bom)] {
                self.bom += 1;
                if self.bom == 3 {
                    // El BOM no ocupa columna: lo que sigue está en la columna 1.
                    self.pos.column = 1;
                }
                return Ok(());
            }
            if self.bom > 0 {
                return self.fail(LexErrorKind::InvalidUtf8);
            }
            self.bom = 3;
        }

        // Continuación de una secuencia UTF-8 dentro de una cadena: el AFD
        // UTF-8 corre en producto con este y consume el byte.
        if self.utf8.is_pending() {
            return match self.utf8.step(b) {
                Step::Pending => Ok(()),
                Step::Char(c) => self.emit_char(sink, c),
                Step::Invalid => self.fail_at(LexErrorKind::InvalidUtf8, self.char_start),
            };
        }

        loop {
            let state = usize::from(self.state);
            let t = TABLE[state][usize::from(CLASS[usize::from(b)])];
            self.state = t.next;
            let here = self.pos;
            match t.action {
                A_SKIP => {}
                A_PUNCT => {
                    let ev = match b {
                        b'{' => Event::BeginObject,
                        b'}' => Event::EndObject,
                        b'[' => Event::BeginArray,
                        b']' => Event::EndArray,
                        b':' => Event::Colon,
                        _ => Event::Comma,
                    };
                    self.emit(sink, ev, here)?;
                }
                A_LIT_START => self.tok_start = here,
                A_LIT_END => {
                    let ev = match state {
                        S_T3 => Event::True,
                        S_F4 => Event::False,
                        _ => Event::Null,
                    };
                    self.emit(sink, ev, self.tok_start)?;
                }
                A_NUM_BEGIN => {
                    self.tok_start = here;
                    self.emit(sink, Event::NumberBegin, here)?;
                    self.emit(sink, Event::Number(num_part(t.next.into()), b), here)?;
                }
                A_NUM_BYTE => self.emit(sink, Event::Number(num_part(t.next.into()), b), here)?,
                A_NUM_END => {
                    self.emit_until(sink, Event::NumberEnd, self.tok_start, here.offset)?;
                    continue; // reprocesar `b` en S_VALUE
                }
                A_STR_BEGIN => {
                    self.tok_start = here;
                    self.emit(sink, Event::StringBegin, here)?;
                }
                A_STR_CHAR => self.emit(sink, Event::StringChar(char::from(b)), here)?,
                A_STR_END => self.emit(sink, Event::StringEnd, self.tok_start)?,
                A_ESC_START => self.char_start = here,
                A_ESC_CHAR => {
                    let c = match b {
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        _ => char::from(b), // " \ /
                    };
                    self.emit_char(sink, c)?;
                }
                A_U_START => self.hex = 0,
                A_HEX => self.hex = (self.hex << 4) | hex_value(b),
                A_HEX_LAST => {
                    let cp = (self.hex << 4) | hex_value(b);
                    match cp {
                        0xD800..=0xDBFF => {
                            self.high = cp;
                            self.state = S_LOW_BS as u8;
                        }
                        0xDC00..=0xDFFF => {
                            return self.fail_at(LexErrorKind::LoneSurrogate, self.char_start)
                        }
                        // Fuera del rango de sustitutos todo valor es un `char`.
                        _ => self.emit_char(sink, char::from_u32(cp).unwrap_or('\u{FFFD}'))?,
                    }
                }
                A_LOW_HEX_LAST => {
                    let low = (self.hex << 4) | hex_value(b);
                    if !(0xDC00..=0xDFFF).contains(&low) {
                        return self.fail_at(LexErrorKind::LoneSurrogate, self.char_start);
                    }
                    let cp = 0x10000 + ((self.high - 0xD800) << 10) + (low - 0xDC00);
                    self.emit_char(sink, char::from_u32(cp).unwrap_or('\u{FFFD}'))?;
                }
                A_UTF8 => {
                    self.char_start = here;
                    if self.utf8.step(b) == Step::Invalid {
                        return self.fail(LexErrorKind::InvalidUtf8);
                    }
                }
                _ => {
                    self.state = state as u8;
                    if matches!(state, S_LOW_BS | S_LOW_U) {
                        return self.fail_at(LexErrorKind::LoneSurrogate, self.char_start);
                    }
                    let kind = if CLASS[usize::from(b)] == C_NONASCII as u8 && state != S_STR {
                        LexErrorKind::UnexpectedByte
                    } else {
                        error_kind(state)
                    };
                    return self.fail(kind);
                }
            }
            return Ok(());
        }
    }

    /// Emite un evento cuyo rango termina en el byte actual (inclusive).
    fn emit<S: Sink<Event>>(&mut self, sink: &mut S, ev: Event, start: Pos) -> Res<S> {
        self.emit_until(sink, ev, start, self.pos.offset + 1)
    }

    fn emit_until<S: Sink<Event>>(
        &mut self,
        sink: &mut S,
        ev: Event,
        start: Pos,
        end: u64,
    ) -> Res<S> {
        sink.event(ev, Span { start, end }).map_err(|e| {
            self.failed = Some(LexError {
                kind: LexErrorKind::Aborted,
                at: self.pos,
            });
            Error::Sink(e)
        })
    }

    fn emit_char<S: Sink<Event>>(&mut self, sink: &mut S, c: char) -> Res<S> {
        self.emit(sink, Event::StringChar(c), self.char_start)
    }

    fn fail<E>(&mut self, kind: LexErrorKind) -> Result<(), Error<E>> {
        self.fail_at(kind, self.pos)
    }

    fn fail_at<E>(&mut self, kind: LexErrorKind, at: Pos) -> Result<(), Error<E>> {
        let e = LexError { kind, at };
        self.failed = Some(e);
        Err(Error::Lex(e))
    }
}

/// Escribe el AFD del tokenizador en formato DOT (Graphviz).
pub fn write_dot(w: &mut impl core::fmt::Write) -> core::fmt::Result {
    crate::dot::write(
        w,
        &crate::dot::Dfa {
            name: "json",
            title: "AFD del tokenizador JSON (RFC 8259)",
            num_states: NUM_STATES,
            num_classes: NUM_CLASSES,
            next: &|s, c| usize::from(TABLE[s][c].next),
            state_names: &STATE_NAMES,
            class_names: &CLASS_NAMES,
            start: S_VALUE,
            accepting: &is_accepting,
            error: S_ERR,
            guarded: &[(S_U4, S_LOW_BS, "hex [valor ∈ D800–DBFF]")],
        },
    )
}
