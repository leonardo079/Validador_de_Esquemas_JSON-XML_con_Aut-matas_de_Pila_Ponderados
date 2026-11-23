//! Tokenizador de DTD externas (subconjunto de XML 1.0 §3; D86) en streaming.
//!
//! Igual que los de JSON y XML (`crates/lexer`), es un AFD de tabla con salida
//! (máquina de Mealy) que corre en producto con el AFD UTF-8 y emite eventos
//! sin copiar contenido. La teoría está en `docs/fase1/lexico-dtd.md` y el
//! diagrama, generado desde esta misma tabla, en `docs/fase1/afd-dtd.dot`.
//!
//! Tokens (`docs/00-alcance.md` §3bis):
//!
//! - `<!` seguido de la palabra clave de la declaración (`ELEMENT`,
//!   `ATTLIST`…), que llega como un nombre más;
//! - nombres y *nmtokens* (`a`, `CDATA`, `EMPTY`, `1`): el parser los
//!   clasifica por su posición (palabras clave contextuales);
//! - `#` seguido de un nombre (`#PCDATA`, `#REQUIRED`, `#IMPLIED`, `#FIXED`);
//! - `( ) | , ? * +` y `>`;
//! - literales entre comillas, con las 5 entidades predefinidas y las
//!   referencias de carácter ya resueltas;
//! - instrucciones de procesamiento (`<?xml …?>`: su posición la comprueba el
//!   parser). Los comentarios se descartan.
//!
//! **Errores léxicos por seguridad** (el espíritu de D8; enmienda E4):
//! `<!ENTITY` y `<!DOCTYPE` (palabra clave en cualquier caja, en su `<`), `<![`
//! (sección condicional), `%` fuera de un literal (referencia de parámetro) y,
//! dentro de un literal, `&nombre;` que no sea una de las 5 predefinidas.
//! `SYSTEM` y `PUBLIC` no son palabras prohibidas: son nombres válidos.
//!
//! Registros acotados además de la tabla: el decodificador UTF-8, la palabra
//! clave de la declaración (a lo sumo 8 bytes; más larga no puede ser
//! `ENTITY` ni `DOCTYPE`), el nombre de una entidad (4 bytes), el valor de una
//! referencia de carácter (acotado a U+10FFFF) y la comilla del literal al
//! que se vuelve tras una referencia.

use lexer::dot::{self, Dfa};
use lexer::utf8::{Step, Utf8Decoder};
use lexer::{Pos, Span};

/// Evento léxico de una DTD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `<!` de una declaración; su palabra clave llega después como nombre.
    DeclOpen,
    /// Comienzo de un nombre o *nmtoken* (rango vacío en su primer byte).
    NameBegin,
    /// `#`; el nombre que sigue (`PCDATA`…) llega como `NameChar`.
    HashBegin,
    /// Carácter de un nombre, de una palabra clave o del destino de una
    /// instrucción de procesamiento.
    NameChar(char),
    LParen,
    RParen,
    Pipe,
    Comma,
    /// `?`
    Opt,
    /// `*`
    Star,
    /// `+`
    Plus,
    /// Comilla de apertura de un literal.
    LiteralBegin,
    /// Carácter de un literal, con las referencias resueltas.
    LiteralChar(char),
    /// Comilla de cierre; el `Span` cubre el literal con sus comillas.
    LiteralEnd,
    /// `>` de una declaración (el rango es solo el `>`: un error ahí se marca en él).
    DeclClose,
    /// `<?` (el destino llega como `NameChar`).
    PiOpen,
    /// Carácter del contenido de la instrucción.
    PiChar(char),
    /// `?>`; el `Span` cubre la instrucción completa.
    PiClose,
}

/// Error léxico de una DTD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Byte que no puede ir ahí.
    UnexpectedByte,
    /// Secuencia UTF-8 inválida.
    InvalidUtf8,
    /// Carácter prohibido por la producción `Char` (controles, U+FFFE…).
    InvalidChar,
    /// La entrada terminó dentro de una declaración, literal o comentario.
    UnexpectedEof,
    /// `<!ENTITY`: entidades prohibidas (XXE, *billion laughs*).
    EntityForbidden,
    /// `<!DOCTYPE` dentro de la DTD.
    DoctypeForbidden,
    /// `<![INCLUDE[` / `<![IGNORE[`.
    ConditionalSectionForbidden,
    /// `%nombre;` fuera de un literal: referencia de parámetro.
    ParameterEntityForbidden,
    /// `&nombre;` que no es una de las 5 predefinidas.
    UndefinedEntity,
    /// Referencia mal formada (`&;`, `&#;`, `&#x;`).
    InvalidReference,
    /// Referencia de carácter fuera de `Char` (`&#0;`).
    InvalidCharRef,
    /// Instrucción de procesamiento sin nombre (`<??>`).
    InvalidPi,
    /// El consumidor detuvo el análisis.
    Aborted,
}

impl ErrorKind {
    /// Mensaje para el usuario.
    pub fn message(self) -> &'static str {
        match self {
            ErrorKind::UnexpectedByte => "carácter inesperado",
            ErrorKind::InvalidUtf8 => "UTF-8 inválido",
            ErrorKind::InvalidChar => "carácter prohibido en XML",
            ErrorKind::UnexpectedEof => "fin de la entrada dentro de una declaración, literal o comentario",
            ErrorKind::EntityForbidden => {
                "<!ENTITY está prohibido: las entidades abren la puerta a XXE y a billion laughs (D86)"
            }
            ErrorKind::DoctypeForbidden => "<!DOCTYPE está prohibido dentro de una DTD externa (D86)",
            ErrorKind::ConditionalSectionForbidden => {
                "las secciones condicionales <![INCLUDE[ / <![IGNORE[ están prohibidas (D86)"
            }
            ErrorKind::ParameterEntityForbidden => {
                "las referencias de parámetro %nombre; están prohibidas (D86)"
            }
            ErrorKind::UndefinedEntity => {
                "entidad no predefinida: solo se admiten &lt; &gt; &amp; &quot; &apos; y &#N;"
            }
            ErrorKind::InvalidReference => "referencia mal formada",
            ErrorKind::InvalidCharRef => "referencia de carácter fuera de la producción Char",
            ErrorKind::InvalidPi => "instrucción de procesamiento sin nombre",
            ErrorKind::Aborted => "análisis detenido",
        }
    }
}

/// Error léxico con su posición.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexError {
    pub kind: ErrorKind,
    pub at: Pos,
}

/// Error del lexer: léxico, o propagado desde el consumidor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<E> {
    Lex(LexError),
    Sink(E),
}

/// Consumidor de los eventos. Devolver `Err` detiene el análisis.
pub trait Sink {
    type Error;
    fn event(&mut self, ev: Event, span: Span) -> Result<(), Self::Error>;
}

// ---------------------------------------------------------------- clases ---

pub const C_SPACE: usize = 0;
pub const C_WS: usize = 1; // \t \n \r
pub const C_LT: usize = 2;
pub const C_GT: usize = 3;
pub const C_BANG: usize = 4;
pub const C_QMARK: usize = 5;
pub const C_DASH: usize = 6;
pub const C_LBRACK: usize = 7;
pub const C_LPAREN: usize = 8;
pub const C_RPAREN: usize = 9;
pub const C_PIPE: usize = 10;
pub const C_COMMA: usize = 11;
pub const C_STAR: usize = 12;
pub const C_PLUS: usize = 13;
pub const C_HASH: usize = 14;
pub const C_PCT: usize = 15;
pub const C_AMP: usize = 16;
pub const C_SEMI: usize = 17;
pub const C_DQ: usize = 18;
pub const C_SQ: usize = 19;
pub const C_DIGIT: usize = 20;
pub const C_X: usize = 21; // x
pub const C_HEX: usize = 22; // a–f A–F
pub const C_LETTER: usize = 23; // resto de letras ASCII, _ :
pub const C_DOT: usize = 24;
pub const C_NONASCII: usize = 25;
pub const C_CTRL: usize = 26; // 00–1F salvo \t \n \r
pub const C_OTHER: usize = 27;

pub const NUM_CLASSES: usize = 28;

pub const CLASS_NAMES: [&str; NUM_CLASSES] = [
    "␠",
    "\\t \\n \\r",
    "<",
    ">",
    "!",
    "?",
    "-",
    "[",
    "(",
    ")",
    "|",
    ",",
    "*",
    "+",
    "#",
    "%",
    "&",
    ";",
    "\"",
    "'",
    "0–9",
    "x",
    "a–f A–F",
    "letra _ :",
    ".",
    "80–FF",
    "control",
    "otro ASCII",
];

const WSP: [usize; 2] = [C_SPACE, C_WS];
/// Inicio de un nombre ASCII (palabra clave, destino de instrucción, `#…`).
const ASCII_START: [usize; 3] = [C_X, C_HEX, C_LETTER];
/// Inicio de un nombre o *nmtoken* dentro de una declaración: `Nmtoken` de
/// XML admite dígitos, `-` y `.` al principio (`(1|2)`); el parser exige
/// `Name` donde toca.
const NMTOKEN: [usize; 7] = [C_X, C_HEX, C_LETTER, C_NONASCII, C_DIGIT, C_DASH, C_DOT];
/// Caracteres del nombre de una entidad (`&lt;`): solo ASCII.
const ENTITY_CHAR: [usize; 6] = [C_X, C_HEX, C_LETTER, C_DIGIT, C_DASH, C_DOT];
const HEX: [usize; 2] = [C_DIGIT, C_HEX];

/// Clase de cada byte.
pub static CLASS: [u8; 256] = build_classes();

const fn build_classes() -> [u8; 256] {
    let mut c = [C_OTHER as u8; 256];
    let mut b = 0;
    while b < 256 {
        c[b] = match b as u8 {
            b' ' => C_SPACE,
            b'\t' | b'\n' | b'\r' => C_WS,
            0x00..=0x1F => C_CTRL,
            0x80..=0xFF => C_NONASCII,
            b'<' => C_LT,
            b'>' => C_GT,
            b'!' => C_BANG,
            b'?' => C_QMARK,
            b'-' => C_DASH,
            b'[' => C_LBRACK,
            b'(' => C_LPAREN,
            b')' => C_RPAREN,
            b'|' => C_PIPE,
            b',' => C_COMMA,
            b'*' => C_STAR,
            b'+' => C_PLUS,
            b'#' => C_HASH,
            b'%' => C_PCT,
            b'&' => C_AMP,
            b';' => C_SEMI,
            b'"' => C_DQ,
            b'\'' => C_SQ,
            b'.' => C_DOT,
            b'0'..=b'9' => C_DIGIT,
            b'x' => C_X,
            b'a'..=b'f' | b'A'..=b'F' => C_HEX,
            b'g'..=b'z' | b'G'..=b'Z' | b'_' | b':' => C_LETTER,
            _ => C_OTHER,
        } as u8;
        b += 1;
    }
    c
}

// --------------------------------------------------------------- estados ---

pub const S_TOP: usize = 0; // entre declaraciones
pub const S_LT: usize = 1; // `<`
pub const S_BANG: usize = 2; // `<!`
pub const S_KEYWORD: usize = 3; // palabra clave de la declaración
pub const S_DECL: usize = 4; // dentro de la declaración, entre tokens
pub const S_NAME: usize = 5; // nombre, nmtoken o `#nombre`
pub const S_HASH: usize = 6; // `#`: se exige un nombre
pub const S_CLOSED: usize = 7; // tras `)` o un sufijo `? * +`
pub const S_LIT_DQ: usize = 8; // literal entre comillas dobles
pub const S_LIT_SQ: usize = 9; // literal entre comillas simples
pub const S_LIT_DONE: usize = 10; // tras la comilla de cierre
pub const S_REF: usize = 11; // `&`
pub const S_REF_NAME: usize = 12;
pub const S_REF_HASH: usize = 13; // `&#`
pub const S_REF_DEC: usize = 14;
pub const S_REF_X: usize = 15; // `&#x`
pub const S_REF_HEX: usize = 16;
pub const S_BANG_DASH: usize = 17; // `<!-`
pub const S_COMMENT: usize = 18;
pub const S_COM_D1: usize = 19; // `…-`
pub const S_COM_D2: usize = 20; // `…--` (solo puede seguir `>`)
pub const S_PI_START: usize = 21; // `<?`: se exige un nombre
pub const S_PI_TARGET: usize = 22;
pub const S_PI_TQ: usize = 23; // `<?nombre?`: solo puede seguir `>`
pub const S_PI: usize = 24; // contenido de la instrucción
pub const S_PI_Q: usize = 25; // contenido seguido de `?`
pub const S_ERR: usize = 26;

pub const NUM_STATES: usize = 27;

pub const STATE_NAMES: [&str; NUM_STATES] = [
    "entre declaraciones",
    "<",
    "<!",
    "palabra clave",
    "declaración",
    "nombre",
    "#",
    ") o sufijo",
    "literal\"",
    "literal'",
    "fin literal",
    "&",
    "&nombre",
    "&#",
    "&#dec",
    "&#x",
    "&#xhex",
    "<!-",
    "comentario",
    "com -",
    "com --",
    "<?",
    "<?nombre",
    "<?nombre?",
    "<?nombre …",
    "<?… ?",
    "error",
];

/// Estado de aceptación: entre declaraciones.
pub const fn is_accepting(s: usize) -> bool {
    s == S_TOP
}

// -------------------------------------------------------------- acciones ---

pub const A_ERR: u8 = 0;
pub const A_SKIP: u8 = 1;
/// Recuerda el inicio de la marca (`<`).
pub const A_MARK: u8 = 2;
/// Carácter (ASCII o primer byte UTF-8); el evento depende del estado destino.
pub const A_CHAR: u8 = 3;
/// `<!` + primera letra de la palabra clave.
pub const A_DECL_OPEN: u8 = 4;
/// Otra letra de la palabra clave.
pub const A_KW_CHAR: u8 = 5;
/// Fin de la palabra clave con un espacio: comprueba `ENTITY`/`DOCTYPE`.
pub const A_KW_END: u8 = 6;
/// Fin de la palabra clave con `>`.
pub const A_KW_CLOSE: u8 = 7;
pub const A_NAME_BEGIN: u8 = 8;
pub const A_HASH: u8 = 9;
pub const A_LPAREN: u8 = 10;
pub const A_RPAREN: u8 = 11;
pub const A_PIPE: u8 = 12;
pub const A_COMMA: u8 = 13;
pub const A_OPT: u8 = 14;
pub const A_STAR: u8 = 15;
pub const A_PLUS: u8 = 16;
pub const A_LIT_BEGIN: u8 = 17;
pub const A_LIT_END: u8 = 18;
pub const A_DECL_CLOSE: u8 = 19;
pub const A_REF_START: u8 = 20;
pub const A_REF_NAME: u8 = 21;
pub const A_REF_NAME_END: u8 = 22;
pub const A_REF_DEC: u8 = 23;
pub const A_REF_HEX: u8 = 24;
pub const A_REF_NUM_END: u8 = 25;
pub const A_PI_OPEN: u8 = 26;
/// `??`: el primer `?` era contenido.
pub const A_PI_Q: u8 = 27;
/// `?x`: el `?` retenido era contenido; luego `x`.
pub const A_PI_FLUSH: u8 = 28;
pub const A_PI_CLOSE: u8 = 29;
/// Byte no ASCII entre declaraciones: solo vale el BOM inicial.
pub const A_BOM: u8 = 30;

/// Entrada de la tabla: estado siguiente y acción.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tr {
    pub next: u8,
    pub action: u8,
}

const fn tr(next: usize, action: u8) -> Tr {
    Tr {
        next: next as u8,
        action,
    }
}

const ERR: Tr = tr(S_ERR, A_ERR);

pub type Table = [[Tr; NUM_CLASSES]; NUM_STATES];

/// δ y la salida de cada transición.
pub static TABLE: Table = build_table();

const fn set(t: &mut Table, s: usize, classes: &[usize], x: Tr) {
    let mut i = 0;
    while i < classes.len() {
        t[s][classes[i]] = x;
        i += 1;
    }
}

const fn fill(t: &mut Table, s: usize, x: Tr) {
    let mut c = 0;
    while c < NUM_CLASSES {
        t[s][c] = x;
        c += 1;
    }
}

/// Lo que puede seguir a un token de una declaración sin espacio: la
/// puntuación del modelo de contenido y el cierre `>`.
const fn punct(t: &mut Table, s: usize) {
    set(t, s, &WSP, tr(S_DECL, A_SKIP));
    set(t, s, &[C_LPAREN], tr(S_DECL, A_LPAREN));
    set(t, s, &[C_RPAREN], tr(S_CLOSED, A_RPAREN));
    set(t, s, &[C_PIPE], tr(S_DECL, A_PIPE));
    set(t, s, &[C_COMMA], tr(S_DECL, A_COMMA));
    set(t, s, &[C_GT], tr(S_TOP, A_DECL_CLOSE));
}

const fn build_table() -> Table {
    let mut t = [[ERR; NUM_CLASSES]; NUM_STATES];

    // Entre declaraciones: espacios, `<` y el BOM inicial.
    set(&mut t, S_TOP, &WSP, tr(S_TOP, A_SKIP));
    set(&mut t, S_TOP, &[C_LT], tr(S_LT, A_MARK));
    set(&mut t, S_TOP, &[C_NONASCII], tr(S_TOP, A_BOM));

    set(&mut t, S_LT, &[C_BANG], tr(S_BANG, A_SKIP));
    set(&mut t, S_LT, &[C_QMARK], tr(S_PI_START, A_SKIP));

    // `<!`: comentario o palabra clave. `<![` es error (sección condicional).
    set(&mut t, S_BANG, &[C_DASH], tr(S_BANG_DASH, A_SKIP));
    set(&mut t, S_BANG, &ASCII_START, tr(S_KEYWORD, A_DECL_OPEN));
    set(&mut t, S_KEYWORD, &ASCII_START, tr(S_KEYWORD, A_KW_CHAR));
    set(&mut t, S_KEYWORD, &WSP, tr(S_DECL, A_KW_END));
    set(&mut t, S_KEYWORD, &[C_GT], tr(S_TOP, A_KW_CLOSE));

    // Dentro de una declaración.
    punct(&mut t, S_DECL);
    set(&mut t, S_DECL, &NMTOKEN, tr(S_NAME, A_NAME_BEGIN));
    set(&mut t, S_DECL, &[C_HASH], tr(S_HASH, A_HASH));
    set(&mut t, S_DECL, &[C_DQ], tr(S_LIT_DQ, A_LIT_BEGIN));
    set(&mut t, S_DECL, &[C_SQ], tr(S_LIT_SQ, A_LIT_BEGIN));

    // Nombre (también tras `#`). Tras un nombre no puede ir otro token
    // sin espacio salvo la puntuación: `a(`, `CDATA"x"` y `a#` son error.
    punct(&mut t, S_NAME);
    set(&mut t, S_NAME, &NMTOKEN, tr(S_NAME, A_CHAR));
    set(&mut t, S_NAME, &[C_QMARK], tr(S_CLOSED, A_OPT));
    set(&mut t, S_NAME, &[C_STAR], tr(S_CLOSED, A_STAR));
    set(&mut t, S_NAME, &[C_PLUS], tr(S_CLOSED, A_PLUS));
    // `(` tras un nombre exige espacio (`<!ELEMENT a(b)>` es error en XML).
    set(&mut t, S_NAME, &[C_LPAREN], ERR);
    set(&mut t, S_HASH, &ASCII_START, tr(S_NAME, A_CHAR));

    // Tras `)` o un sufijo: solo más sufijos, puntuación, espacio o `>`.
    punct(&mut t, S_CLOSED);
    set(&mut t, S_CLOSED, &[C_LPAREN], ERR);
    set(&mut t, S_CLOSED, &[C_QMARK], tr(S_CLOSED, A_OPT));
    set(&mut t, S_CLOSED, &[C_STAR], tr(S_CLOSED, A_STAR));
    set(&mut t, S_CLOSED, &[C_PLUS], tr(S_CLOSED, A_PLUS));

    // Literales. `<` está prohibido (`AttValue`); `%` es un carácter más
    // (enmienda E4); `&` abre una referencia.
    fill(&mut t, S_LIT_DQ, tr(S_LIT_DQ, A_CHAR));
    set(&mut t, S_LIT_DQ, &[C_DQ], tr(S_LIT_DONE, A_LIT_END));
    set(&mut t, S_LIT_DQ, &[C_AMP], tr(S_REF, A_REF_START));
    set(&mut t, S_LIT_DQ, &[C_LT, C_CTRL], ERR);
    fill(&mut t, S_LIT_SQ, tr(S_LIT_SQ, A_CHAR));
    set(&mut t, S_LIT_SQ, &[C_SQ], tr(S_LIT_DONE, A_LIT_END));
    set(&mut t, S_LIT_SQ, &[C_AMP], tr(S_REF, A_REF_START));
    set(&mut t, S_LIT_SQ, &[C_LT, C_CTRL], ERR);
    // Tras un literal, espacio o `>`: `"a"b` es error.
    set(&mut t, S_LIT_DONE, &WSP, tr(S_DECL, A_SKIP));
    set(&mut t, S_LIT_DONE, &[C_GT], tr(S_TOP, A_DECL_CLOSE));

    // Referencias dentro de un literal. El `;` vuelve al literal de donde se
    // salió (registro `ref_ret`): la tabla dice comillas dobles y la otra
    // vuelta se dibuja punteada.
    set(
        &mut t,
        S_REF,
        &[C_X, C_HEX, C_LETTER],
        tr(S_REF_NAME, A_REF_NAME),
    );
    set(&mut t, S_REF, &[C_HASH], tr(S_REF_HASH, A_SKIP));
    set(&mut t, S_REF_NAME, &ENTITY_CHAR, tr(S_REF_NAME, A_REF_NAME));
    set(&mut t, S_REF_NAME, &[C_SEMI], tr(S_LIT_DQ, A_REF_NAME_END));
    set(&mut t, S_REF_HASH, &[C_DIGIT], tr(S_REF_DEC, A_REF_DEC));
    set(&mut t, S_REF_HASH, &[C_X], tr(S_REF_X, A_SKIP));
    set(&mut t, S_REF_DEC, &[C_DIGIT], tr(S_REF_DEC, A_REF_DEC));
    set(&mut t, S_REF_DEC, &[C_SEMI], tr(S_LIT_DQ, A_REF_NUM_END));
    set(&mut t, S_REF_X, &HEX, tr(S_REF_HEX, A_REF_HEX));
    set(&mut t, S_REF_HEX, &HEX, tr(S_REF_HEX, A_REF_HEX));
    set(&mut t, S_REF_HEX, &[C_SEMI], tr(S_LIT_DQ, A_REF_NUM_END));

    // Comentarios: `--` solo puede ir seguido de `>`.
    set(&mut t, S_BANG_DASH, &[C_DASH], tr(S_COMMENT, A_SKIP));
    fill(&mut t, S_COMMENT, tr(S_COMMENT, A_CHAR));
    set(&mut t, S_COMMENT, &[C_DASH], tr(S_COM_D1, A_SKIP));
    set(&mut t, S_COMMENT, &[C_CTRL], ERR);
    fill(&mut t, S_COM_D1, tr(S_COMMENT, A_CHAR));
    set(&mut t, S_COM_D1, &[C_DASH], tr(S_COM_D2, A_SKIP));
    set(&mut t, S_COM_D1, &[C_CTRL], ERR);
    set(&mut t, S_COM_D2, &[C_GT], tr(S_TOP, A_SKIP));

    // Instrucciones de procesamiento.
    set(
        &mut t,
        S_PI_START,
        &[C_X, C_HEX, C_LETTER, C_NONASCII],
        tr(S_PI_TARGET, A_PI_OPEN),
    );
    set(&mut t, S_PI_TARGET, &NMTOKEN, tr(S_PI_TARGET, A_CHAR));
    set(&mut t, S_PI_TARGET, &WSP, tr(S_PI, A_SKIP));
    set(&mut t, S_PI_TARGET, &[C_QMARK], tr(S_PI_TQ, A_SKIP));
    set(&mut t, S_PI_TQ, &[C_GT], tr(S_TOP, A_PI_CLOSE));
    fill(&mut t, S_PI, tr(S_PI, A_CHAR));
    set(&mut t, S_PI, &[C_QMARK], tr(S_PI_Q, A_SKIP));
    set(&mut t, S_PI, &[C_CTRL], ERR);
    fill(&mut t, S_PI_Q, tr(S_PI, A_PI_FLUSH));
    set(&mut t, S_PI_Q, &[C_QMARK], tr(S_PI_Q, A_PI_Q));
    set(&mut t, S_PI_Q, &[C_GT], tr(S_TOP, A_PI_CLOSE));
    set(&mut t, S_PI_Q, &[C_CTRL], ERR);

    t
}

/// Clase de error de una transición al estado de error.
fn error_kind(state: usize, class: usize) -> ErrorKind {
    match (state, class) {
        (_, C_CTRL) => ErrorKind::InvalidChar,
        (S_BANG, C_LBRACK) => ErrorKind::ConditionalSectionForbidden,
        (S_TOP | S_DECL | S_NAME | S_CLOSED | S_LIT_DONE | S_KEYWORD | S_HASH, C_PCT) => {
            ErrorKind::ParameterEntityForbidden
        }
        (S_PI_START | S_PI_TARGET | S_PI_TQ, _) => ErrorKind::InvalidPi,
        (S_REF..=S_REF_HEX, _) => ErrorKind::InvalidReference,
        _ => ErrorKind::UnexpectedByte,
    }
}

/// Producción `Char` de XML 1.0.
pub fn is_xml_char(cp: u32) -> bool {
    matches!(cp, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// Palabra clave prohibida, en cualquier caja.
fn forbidden_keyword(kw: &[u8]) -> Option<ErrorKind> {
    if kw.eq_ignore_ascii_case(b"ENTITY") {
        Some(ErrorKind::EntityForbidden)
    } else if kw.eq_ignore_ascii_case(b"DOCTYPE") {
        Some(ErrorKind::DoctypeForbidden)
    } else {
        None
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

/// Tokenizador DTD reanudable. No reserva memoria.
#[derive(Clone, Debug)]
pub struct DtdLexer {
    state: u8,
    pos: Pos,
    /// Inicio de la marca en curso (`<`).
    tok_start: Pos,
    /// Inicio del carácter en curso (referencia o secuencia UTF-8).
    char_start: Pos,
    /// Comilla de apertura del literal en curso.
    lit_start: Pos,
    utf8: Utf8Decoder,
    /// Palabra clave de la declaración (para `ENTITY`/`DOCTYPE`).
    kw: [u8; 8],
    kw_len: u8,
    /// Literal al que se vuelve tras una referencia.
    ref_ret: u8,
    ref_name: [u8; 4],
    ref_len: u8,
    ref_val: u32,
    failed: Option<LexError>,
}

impl Default for DtdLexer {
    fn default() -> Self {
        Self::new()
    }
}

type Res<S> = Result<(), Error<<S as Sink>::Error>>;

impl DtdLexer {
    pub const fn new() -> Self {
        Self {
            state: S_TOP as u8,
            pos: Pos::START,
            tok_start: Pos::START,
            char_start: Pos::START,
            lit_start: Pos::START,
            utf8: Utf8Decoder::new(),
            kw: [0; 8],
            kw_len: 0,
            ref_ret: S_LIT_DQ as u8,
            ref_name: [0; 4],
            ref_len: 0,
            ref_val: 0,
            failed: None,
        }
    }

    /// Posición del próximo byte que se espera.
    pub fn position(&self) -> Pos {
        self.pos
    }

    /// Consume un bloque de la entrada. Tras un error el lexer queda detenido
    /// y devuelve siempre el mismo error.
    pub fn feed<S: Sink>(&mut self, chunk: &[u8], sink: &mut S) -> Res<S> {
        if let Some(e) = self.failed {
            return Err(Error::Lex(e));
        }
        for &b in chunk {
            self.byte(b, sink)?;
            self.pos.advance(b);
        }
        Ok(())
    }

    /// Indica el fin de la entrada.
    pub fn finish<S: Sink>(&mut self, _sink: &mut S) -> Res<S> {
        if let Some(e) = self.failed {
            return Err(Error::Lex(e));
        }
        if self.utf8.is_pending() {
            return self.fail_at(ErrorKind::InvalidUtf8, self.char_start);
        }
        if usize::from(self.state) == S_KEYWORD {
            self.check_keyword()?;
        }
        if !is_accepting(self.state.into()) {
            return self.fail(ErrorKind::UnexpectedEof);
        }
        Ok(())
    }

    fn byte<S: Sink>(&mut self, b: u8, sink: &mut S) -> Res<S> {
        if self.utf8.is_pending() {
            return match self.utf8.step(b) {
                Step::Pending => Ok(()),
                // BOM inicial: se descarta (no ocupa columna).
                Step::Char('\u{feff}') if self.char_start.offset == 0 => {
                    self.pos.column = 1;
                    Ok(())
                }
                Step::Char(c) => self.emit_char(sink, c),
                Step::Invalid => self.fail_at(ErrorKind::InvalidUtf8, self.char_start),
            };
        }

        let state = usize::from(self.state);
        let class = usize::from(CLASS[usize::from(b)]);
        let t = TABLE[state][class];
        self.state = t.next;
        let here = self.pos;
        match t.action {
            A_SKIP => Ok(()),
            A_MARK => {
                self.tok_start = here;
                Ok(())
            }
            A_CHAR => self.char(sink, b),
            A_BOM => {
                if here.offset == 0 {
                    self.char(sink, b)
                } else {
                    self.state = state as u8;
                    self.fail(ErrorKind::UnexpectedByte)
                }
            }
            A_DECL_OPEN => {
                let lt = self.tok_start;
                self.emit_until(sink, Event::DeclOpen, lt, lt.offset + 2)?;
                self.emit_until(sink, Event::NameBegin, here, here.offset)?;
                self.kw_len = 0;
                self.kw_char(sink, b)
            }
            A_KW_CHAR => self.kw_char(sink, b),
            A_KW_END => self.check_keyword(),
            A_KW_CLOSE => {
                self.check_keyword()?;
                self.emit(sink, Event::DeclClose, here)
            }
            A_NAME_BEGIN => {
                self.emit_until(sink, Event::NameBegin, here, here.offset)?;
                self.char(sink, b)
            }
            A_HASH => self.emit(sink, Event::HashBegin, here),
            A_LPAREN => self.emit(sink, Event::LParen, here),
            A_RPAREN => self.emit(sink, Event::RParen, here),
            A_PIPE => self.emit(sink, Event::Pipe, here),
            A_COMMA => self.emit(sink, Event::Comma, here),
            A_OPT => self.emit(sink, Event::Opt, here),
            A_STAR => self.emit(sink, Event::Star, here),
            A_PLUS => self.emit(sink, Event::Plus, here),
            A_LIT_BEGIN => {
                self.lit_start = here;
                self.emit(sink, Event::LiteralBegin, here)
            }
            A_LIT_END => self.emit(sink, Event::LiteralEnd, self.lit_start),
            A_DECL_CLOSE => self.emit(sink, Event::DeclClose, here),
            A_REF_START => {
                self.ref_ret = state as u8;
                self.char_start = here;
                self.ref_len = 0;
                self.ref_val = 0;
                Ok(())
            }
            A_REF_NAME => {
                if usize::from(self.ref_len) == self.ref_name.len() {
                    return self.fail_at(ErrorKind::UndefinedEntity, self.char_start);
                }
                self.ref_name[usize::from(self.ref_len)] = b;
                self.ref_len += 1;
                Ok(())
            }
            A_REF_NAME_END => {
                let c = match &self.ref_name[..usize::from(self.ref_len)] {
                    b"lt" => '<',
                    b"gt" => '>',
                    b"amp" => '&',
                    b"quot" => '"',
                    b"apos" => '\'',
                    _ => return self.fail_at(ErrorKind::UndefinedEntity, self.char_start),
                };
                self.state = self.ref_ret;
                self.emit_char(sink, c)
            }
            A_REF_DEC | A_REF_HEX => {
                let (base, digit) = if t.action == A_REF_DEC {
                    (10, u32::from(b - b'0'))
                } else {
                    (16, hex_value(b))
                };
                self.ref_val = self.ref_val * base + digit;
                if self.ref_val > 0x10FFFF {
                    return self.fail_at(ErrorKind::InvalidCharRef, self.char_start);
                }
                Ok(())
            }
            A_REF_NUM_END => {
                let Some(c) = char::from_u32(self.ref_val).filter(|&c| is_xml_char(c.into()))
                else {
                    return self.fail_at(ErrorKind::InvalidCharRef, self.char_start);
                };
                self.state = self.ref_ret;
                self.emit_char(sink, c)
            }
            A_PI_OPEN => {
                let lt = self.tok_start;
                self.emit_until(sink, Event::PiOpen, lt, lt.offset + 2)?;
                self.char(sink, b)
            }
            A_PI_Q => self.emit_until(sink, Event::PiChar('?'), back(here), here.offset),
            A_PI_FLUSH => {
                self.emit_until(sink, Event::PiChar('?'), back(here), here.offset)?;
                self.char(sink, b)
            }
            A_PI_CLOSE => self.emit(sink, Event::PiClose, self.tok_start),
            _ => {
                self.state = state as u8;
                // `<!ENTITY%` o `<!DOCTYPE[`: el error de seguridad manda.
                if state == S_KEYWORD {
                    self.check_keyword()?;
                }
                // `<![`: el error va en el `<` de la declaración, como los demás.
                let kind = error_kind(state, class);
                if kind == ErrorKind::ConditionalSectionForbidden {
                    return self.fail_at(kind, self.tok_start);
                }
                self.fail(kind)
            }
        }
    }

    /// Letra de la palabra clave: se guarda (las 8 primeras) y se emite.
    fn kw_char<S: Sink>(&mut self, sink: &mut S, b: u8) -> Res<S> {
        if usize::from(self.kw_len) < self.kw.len() {
            self.kw[usize::from(self.kw_len)] = b;
        }
        self.kw_len = self.kw_len.saturating_add(1);
        self.char(sink, b)
    }

    /// `ENTITY` y `DOCTYPE` (en cualquier caja) son error en su `<`.
    fn check_keyword<E>(&mut self) -> Result<(), Error<E>> {
        let len = usize::from(self.kw_len);
        if len <= self.kw.len() {
            if let Some(kind) = forbidden_keyword(&self.kw[..len]) {
                return self.fail_at(kind, self.tok_start);
            }
        }
        Ok(())
    }

    /// Carácter ASCII, o primer byte de una secuencia UTF-8.
    fn char<S: Sink>(&mut self, sink: &mut S, b: u8) -> Res<S> {
        self.char_start = self.pos;
        if b.is_ascii() {
            self.emit_char(sink, char::from(b))
        } else if self.utf8.step(b) == Step::Invalid {
            self.fail(ErrorKind::InvalidUtf8)
        } else {
            Ok(())
        }
    }

    /// Emite un carácter completo según el estado.
    fn emit_char<S: Sink>(&mut self, sink: &mut S, c: char) -> Res<S> {
        let ev = match usize::from(self.state) {
            S_KEYWORD | S_NAME | S_PI_TARGET => Some(Event::NameChar(c)),
            S_LIT_DQ | S_LIT_SQ => Some(Event::LiteralChar(c)),
            S_PI | S_PI_Q => Some(Event::PiChar(c)),
            S_COMMENT | S_COM_D1 => None,
            // Un carácter no ASCII entre declaraciones que no es el BOM:
            // `UnexpectedByte`, sea o no un `Char` (como en cualquier offset).
            _ => return self.fail_at(ErrorKind::UnexpectedByte, self.char_start),
        };
        if !is_xml_char(c.into()) {
            return self.fail_at(ErrorKind::InvalidChar, self.char_start);
        }
        match ev {
            Some(ev) => self.emit(sink, ev, self.char_start),
            None => Ok(()),
        }
    }

    /// Emite un evento cuyo rango termina en el byte actual (inclusive).
    fn emit<S: Sink>(&mut self, sink: &mut S, ev: Event, start: Pos) -> Res<S> {
        self.emit_until(sink, ev, start, self.pos.offset + 1)
    }

    fn emit_until<S: Sink>(&mut self, sink: &mut S, ev: Event, start: Pos, end: u64) -> Res<S> {
        sink.event(ev, Span { start, end }).map_err(|e| {
            self.failed = Some(LexError {
                kind: ErrorKind::Aborted,
                at: self.pos,
            });
            Error::Sink(e)
        })
    }

    fn fail<E>(&mut self, kind: ErrorKind) -> Result<(), Error<E>> {
        self.fail_at(kind, self.pos)
    }

    fn fail_at<E>(&mut self, kind: ErrorKind, at: Pos) -> Result<(), Error<E>> {
        let e = LexError { kind, at };
        self.failed = Some(e);
        Err(Error::Lex(e))
    }
}

/// Posición un byte atrás en la misma línea (el `?` retenido de una
/// instrucción, que es ASCII). Con la columna saturada se deja saturada, como
/// en el lexer XML.
fn back(p: Pos) -> Pos {
    Pos {
        offset: p.offset.saturating_sub(1),
        line: p.line,
        column: if p.column == u32::MAX {
            u32::MAX
        } else {
            p.column.saturating_sub(1)
        },
    }
}

/// Tokeniza una entrada completa. Atajo para el parser y las pruebas.
pub fn tokenize<S: Sink>(input: &[u8], sink: &mut S) -> Res<S> {
    let mut lx = DtdLexer::new();
    lx.feed(input, sink)?;
    lx.finish(sink)
}

// ------------------------------------------------------------------- DOT ---

/// Escribe el AFD en DOT (`schemac dot dtd`, `make docs`).
pub fn write_dot(w: &mut impl core::fmt::Write) -> core::fmt::Result {
    let next = |s: usize, c: usize| usize::from(TABLE[s][c].next);
    let accepting = |s: usize| is_accepting(s);
    dot::write(
        w,
        &Dfa {
            name: "afd_dtd",
            title: "AFD del tokenizador DTD (D86)",
            num_states: NUM_STATES,
            num_classes: NUM_CLASSES,
            next: &next,
            state_names: &STATE_NAMES,
            class_names: &CLASS_NAMES,
            start: S_TOP,
            accepting: &accepting,
            error: S_ERR,
            guarded: &[
                (S_REF_NAME, S_LIT_SQ, "; (si el literal es con ')"),
                (S_REF_DEC, S_LIT_SQ, "; (si el literal es con ')"),
                (S_REF_HEX, S_LIT_SQ, "; (si el literal es con ')"),
            ],
            register_errors: &[
                "<!ENTITY y <!DOCTYPE (palabra clave en cualquier caja): error en su <",
                "&nombre; que no es lt, gt, amp, quot ni apos",
                "&#N; o &#xN; fuera de la producción Char",
                "carácter no ASCII entre declaraciones que no es el BOM inicial",
            ],
        },
    )
}
