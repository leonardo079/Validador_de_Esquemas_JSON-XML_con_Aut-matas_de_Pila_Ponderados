//! Tokenizador XML (subconjunto de XML 1.0) en streaming.
//!
//! Igual que el de JSON, es un AFD de tabla con salida (máquina de Mealy) que
//! corre en producto con el AFD UTF-8 y emite eventos sin copiar contenido.
//!
//! Subconjunto (`docs/00-alcance.md` §1.2):
//!
//! - Etiquetas de apertura, cierre y vacías; atributos con comillas dobles o
//!   simples; texto; comentarios (se descartan); secciones CDATA;
//!   instrucciones de procesamiento, incluida la declaración `<?xml …?>`.
//!   De estas se emiten el nombre y el contenido: validar la declaración
//!   (que vaya al inicio, `encoding` UTF-8) es tarea sintáctica.
//! - Las 5 entidades predefinidas y las referencias de carácter `&#N;` y
//!   `&#xN;`. Cualquier otra entidad es error.
//! - **`<!DOCTYPE` es un error léxico**: sin DTD no hay entidades externas
//!   (XXE) ni expansión recursiva (*billion laughs*).
//!
//! El emparejamiento de etiquetas de apertura y cierre no es regular y no se
//! hace aquí: lo resuelve la pila del autómata de la Fase 4.
//!
//! Simplificaciones documentadas: los nombres aceptan cualquier carácter no
//! ASCII (XML restringe algunos rangos), no se normalizan los fines de línea
//! ni los espacios de los atributos.
//!
//! Registros acotados además de la tabla: el decodificador UTF-8, el nombre de
//! una entidad (a lo sumo 4 bytes: `quot`, `apos`), el valor de una referencia
//! de carácter (acotado a U+10FFFF) y el estado de regreso tras una referencia
//! (texto, valor con `"` o valor con `'`).

use crate::table::{fill, set, tr, Tr};
use crate::utf8::{Step, Utf8Decoder};
use crate::{Error, LexError, LexErrorKind, Pos, Sink, Span};

/// Evento léxico XML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `<` de una etiqueta de apertura (se emite con el primer carácter del nombre).
    StartTagOpen,
    /// `</`.
    EndTagOpen,
    /// Carácter del nombre de una etiqueta o de un atributo.
    NameChar(char),
    /// Comienzo del nombre de un atributo.
    AttrBegin,
    /// Comilla de apertura del valor.
    AttrValueBegin,
    /// Carácter del valor, con las referencias ya resueltas.
    AttrValueChar(char),
    /// Comilla de cierre; el `Span` cubre el valor con sus comillas.
    AttrValueEnd,
    /// `>` de una etiqueta de apertura; el `Span` cubre la etiqueta completa.
    StartTagClose,
    /// `/>`; el `Span` cubre la etiqueta completa.
    EmptyTagClose,
    /// `>` de una etiqueta de cierre; el `Span` cubre la etiqueta completa.
    EndTagClose,
    /// Carácter de texto o de una sección CDATA, con las referencias resueltas.
    TextChar(char),
    /// `<?` de una instrucción de procesamiento (se emite con el primer
    /// carácter del nombre, que llega como `NameChar`).
    PiOpen,
    /// Carácter del contenido de la instrucción (tras el espacio que sigue al nombre).
    PiChar(char),
    /// `?>`; el `Span` cubre la instrucción completa.
    PiClose,
}

// ---------------------------------------------------------------- clases ---

const C_SPACE: usize = 0;
const C_WS: usize = 1; // \t \n \r
const C_LT: usize = 2;
const C_GT: usize = 3;
const C_SLASH: usize = 4;
const C_QMARK: usize = 5;
const C_BANG: usize = 6;
const C_EQ: usize = 7;
const C_DQ: usize = 8;
const C_SQ: usize = 9;
const C_AMP: usize = 10;
const C_HASH: usize = 11;
const C_SEMI: usize = 12;
const C_DASH: usize = 13;
const C_DOT: usize = 14;
const C_RBRACK: usize = 15;
const C_LBRACK: usize = 16;
const C_DIGIT: usize = 17;
const C_X: usize = 18; // x
const C_HEXLETTER: usize = 19; // a–f B E F
const C_UA: usize = 20; // A (CDATA y hexadecimal)
const C_UC: usize = 21; // C
const C_UD: usize = 22; // D (CDATA, hexadecimal y DOCTYPE)
const C_UT: usize = 23; // T
const C_NAMESTART: usize = 24; // resto de letras, _ :
const C_CTRL: usize = 25; // 00–1F salvo \t \n \r
const C_OTHER: usize = 26;
const C_NONASCII: usize = 27;

pub const NUM_CLASSES: usize = 28;

pub const CLASS_NAMES: [&str; NUM_CLASSES] = [
    "␠",
    "\\t \\n \\r",
    "<",
    ">",
    "/",
    "?",
    "!",
    "=",
    "\"",
    "'",
    "&",
    "#",
    ";",
    "-",
    ".",
    "]",
    "[",
    "0–9",
    "x",
    "a–f B E F",
    "A",
    "C",
    "D",
    "T",
    "letra _ :",
    "control",
    "otro ASCII",
    "80–FF",
];

const WSP: [usize; 2] = [C_SPACE, C_WS];
const NAME_START: [usize; 8] = [
    C_X,
    C_HEXLETTER,
    C_UA,
    C_UC,
    C_UD,
    C_UT,
    C_NAMESTART,
    C_NONASCII,
];
const NAME_CHAR: [usize; 11] = [
    C_X,
    C_HEXLETTER,
    C_UA,
    C_UC,
    C_UD,
    C_UT,
    C_NAMESTART,
    C_NONASCII,
    C_DIGIT,
    C_DASH,
    C_DOT,
];
const ENTITY_START: [usize; 7] = [C_X, C_HEXLETTER, C_UA, C_UC, C_UD, C_UT, C_NAMESTART];
const ENTITY_CHAR: [usize; 10] = [
    C_X,
    C_HEXLETTER,
    C_UA,
    C_UC,
    C_UD,
    C_UT,
    C_NAMESTART,
    C_DIGIT,
    C_DASH,
    C_DOT,
];
const HEX: [usize; 5] = [C_DIGIT, C_HEXLETTER, C_UA, C_UC, C_UD];

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
            b'/' => C_SLASH,
            b'?' => C_QMARK,
            b'!' => C_BANG,
            b'=' => C_EQ,
            b'"' => C_DQ,
            b'\'' => C_SQ,
            b'&' => C_AMP,
            b'#' => C_HASH,
            b';' => C_SEMI,
            b'-' => C_DASH,
            b'.' => C_DOT,
            b']' => C_RBRACK,
            b'[' => C_LBRACK,
            b'0'..=b'9' => C_DIGIT,
            b'x' => C_X,
            b'A' => C_UA,
            b'C' => C_UC,
            b'D' => C_UD,
            b'T' => C_UT,
            b'a'..=b'f' | b'B' | b'E' | b'F' => C_HEXLETTER,
            b'g'..=b'z' | b'G'..=b'Z' | b'_' | b':' => C_NAMESTART,
            _ => C_OTHER,
        } as u8;
        b += 1;
    }
    c
}

// --------------------------------------------------------------- estados ---

const S_CONTENT: usize = 0; // texto entre etiquetas
const S_RB1: usize = 1; // texto tras `]`
const S_RB2: usize = 2; // texto tras `]]` (`]]>` está prohibido en el texto)
const S_LT: usize = 3; // `<`
const S_STAG_NAME: usize = 4; // nombre de etiqueta de apertura
const S_STAG_WS: usize = 5; // dentro de la etiqueta, tras un espacio
const S_ATTR_NAME: usize = 6;
const S_ATTR_EQ_WS: usize = 7; // tras el nombre del atributo, antes de `=`
const S_ATTR_VAL_WS: usize = 8; // tras `=`, antes de la comilla
const S_VAL_DQ: usize = 9; // valor entre comillas dobles
const S_VAL_SQ: usize = 10; // valor entre comillas simples
const S_ATTR_DONE: usize = 11; // tras la comilla de cierre
const S_EMPTY: usize = 12; // `/` dentro de una etiqueta de apertura
const S_ETAG: usize = 13; // `</`
const S_ETAG_NAME: usize = 14;
const S_ETAG_WS: usize = 15;
const S_PI: usize = 16; // contenido de `<?nombre …`
const S_PI_Q: usize = 17; // contenido seguido de `?`
const S_BANG: usize = 18; // `<!`
const S_BANG_DASH: usize = 19; // `<!-`
const S_COMMENT: usize = 20; // `<!--…`
const S_COM_D1: usize = 21; // `…-`
const S_COM_D2: usize = 22; // `…--` (solo puede seguir `>`)
const S_CD_LB: usize = 23; // `<![`
const S_CD_C: usize = 24;
const S_CD_D: usize = 25;
const S_CD_A: usize = 26;
const S_CD_T: usize = 27;
const S_CD_A2: usize = 28; // `<![CDATA`
const S_CDATA: usize = 29;
const S_CD_RB1: usize = 30; // `]` pendiente
const S_CD_RB2: usize = 31; // `]]` pendientes
const S_REF: usize = 32; // `&`
const S_REF_NAME: usize = 33;
const S_REF_HASH: usize = 34; // `&#`
const S_REF_DEC: usize = 35;
const S_REF_X: usize = 36; // `&#x`
const S_REF_HEX: usize = 37;
const S_PI_START: usize = 38; // `<?`: se exige un nombre
const S_PI_TARGET: usize = 39; // nombre de la instrucción
const S_PI_TQ: usize = 40; // `<?nombre?`: solo puede seguir `>`
const S_ERR: usize = 41;

pub const NUM_STATES: usize = 42;

pub const STATE_NAMES: [&str; NUM_STATES] = [
    "contenido",
    "]",
    "]]",
    "<",
    "nombre",
    "etiqueta␠",
    "atributo",
    "atributo␠",
    "=",
    "valor\"",
    "valor'",
    "fin valor",
    "/",
    "</",
    "nombre cierre",
    "cierre␠",
    "<?nombre …",
    "<?… ?",
    "<!",
    "<!-",
    "comentario",
    "com -",
    "com --",
    "<![",
    "<![C",
    "<![CD",
    "<![CDA",
    "<![CDAT",
    "<![CDATA",
    "CDATA",
    "CDATA ]",
    "CDATA ]]",
    "&",
    "&nombre",
    "&#",
    "&#dec",
    "&#x",
    "&#xhex",
    "<?",
    "<?nombre",
    "<?nombre?",
    "error",
];

/// Estados de aceptación: fuera de toda etiqueta, comentario o referencia.
pub const fn is_accepting(s: usize) -> bool {
    matches!(s, S_CONTENT | S_RB1 | S_RB2)
}

// -------------------------------------------------------------- acciones ---

const A_ERR: u8 = 0;
const A_SKIP: u8 = 1;
/// Recuerda el inicio de la marca (`<`).
const A_MARK: u8 = 2;
/// Carácter ASCII o primer byte de una secuencia UTF-8; el evento depende
/// del estado destino (texto, nombre, valor o nada en comentarios).
const A_CHAR: u8 = 3;
const A_STAG_OPEN: u8 = 4;
const A_ETAG_OPEN: u8 = 5;
const A_ATTR_BEGIN: u8 = 6;
const A_VAL_BEGIN: u8 = 7;
const A_VAL_END: u8 = 8;
const A_STAG_CLOSE: u8 = 9;
const A_EMPTY_CLOSE: u8 = 10;
const A_ETAG_CLOSE: u8 = 11;
const A_REF_START: u8 = 12;
const A_REF_NAME: u8 = 13;
const A_REF_NAME_END: u8 = 14;
const A_REF_DEC: u8 = 15;
const A_REF_HEX: u8 = 16;
const A_REF_NUM_END: u8 = 17;
/// En CDATA, `]]]`: el primer `]` pendiente era texto.
const A_CD_RB: u8 = 18;
/// En CDATA, `]x`: el `]` pendiente era texto; luego `x`.
const A_CD_FLUSH1: u8 = 19;
/// En CDATA, `]]x`: los dos `]` pendientes eran texto; luego `x`.
const A_CD_FLUSH2: u8 = 20;
const A_PI_OPEN: u8 = 21;
/// En el contenido de una instrucción, `??`: el primer `?` era contenido.
const A_PI_Q: u8 = 22;
/// En el contenido, `?x`: el `?` retenido era contenido; luego `x`.
const A_PI_FLUSH: u8 = 23;
const A_PI_CLOSE: u8 = 24;

const ERR: Tr = tr(S_ERR, A_ERR);

type Table = [[Tr; NUM_CLASSES]; NUM_STATES];

static TABLE: Table = build_table();

const fn build_table() -> Table {
    let mut t = [[ERR; NUM_CLASSES]; NUM_STATES];

    // Texto. `<` abre marca, `&` abre referencia; `]]>` está prohibido.
    fill(&mut t, S_CONTENT, tr(S_CONTENT, A_CHAR));
    set(&mut t, S_CONTENT, &[C_LT], tr(S_LT, A_MARK));
    set(&mut t, S_CONTENT, &[C_AMP], tr(S_REF, A_REF_START));
    set(&mut t, S_CONTENT, &[C_RBRACK], tr(S_RB1, A_CHAR));
    set(&mut t, S_CONTENT, &[C_CTRL], ERR);
    t[S_RB1] = t[S_CONTENT];
    set(&mut t, S_RB1, &[C_RBRACK], tr(S_RB2, A_CHAR));
    t[S_RB2] = t[S_RB1];
    set(&mut t, S_RB2, &[C_GT], ERR);

    // Tras `<`.
    set(&mut t, S_LT, &[C_SLASH], tr(S_ETAG, A_ETAG_OPEN));
    set(&mut t, S_LT, &[C_QMARK], tr(S_PI_START, A_SKIP));
    set(&mut t, S_LT, &[C_BANG], tr(S_BANG, A_SKIP));
    set(&mut t, S_LT, &NAME_START, tr(S_STAG_NAME, A_STAG_OPEN));

    // Etiqueta de apertura y atributos.
    let stag_close = tr(S_CONTENT, A_STAG_CLOSE);
    let to_empty = tr(S_EMPTY, A_SKIP);
    set(&mut t, S_STAG_NAME, &NAME_CHAR, tr(S_STAG_NAME, A_CHAR));
    set(&mut t, S_STAG_NAME, &WSP, tr(S_STAG_WS, A_SKIP));
    set(&mut t, S_STAG_NAME, &[C_GT], stag_close);
    set(&mut t, S_STAG_NAME, &[C_SLASH], to_empty);
    set(&mut t, S_STAG_WS, &WSP, tr(S_STAG_WS, A_SKIP));
    set(
        &mut t,
        S_STAG_WS,
        &NAME_START,
        tr(S_ATTR_NAME, A_ATTR_BEGIN),
    );
    set(&mut t, S_STAG_WS, &[C_GT], stag_close);
    set(&mut t, S_STAG_WS, &[C_SLASH], to_empty);
    set(&mut t, S_ATTR_NAME, &NAME_CHAR, tr(S_ATTR_NAME, A_CHAR));
    set(&mut t, S_ATTR_NAME, &WSP, tr(S_ATTR_EQ_WS, A_SKIP));
    set(&mut t, S_ATTR_NAME, &[C_EQ], tr(S_ATTR_VAL_WS, A_SKIP));
    set(&mut t, S_ATTR_EQ_WS, &WSP, tr(S_ATTR_EQ_WS, A_SKIP));
    set(&mut t, S_ATTR_EQ_WS, &[C_EQ], tr(S_ATTR_VAL_WS, A_SKIP));
    set(&mut t, S_ATTR_VAL_WS, &WSP, tr(S_ATTR_VAL_WS, A_SKIP));
    set(&mut t, S_ATTR_VAL_WS, &[C_DQ], tr(S_VAL_DQ, A_VAL_BEGIN));
    set(&mut t, S_ATTR_VAL_WS, &[C_SQ], tr(S_VAL_SQ, A_VAL_BEGIN));
    fill(&mut t, S_VAL_DQ, tr(S_VAL_DQ, A_CHAR));
    set(&mut t, S_VAL_DQ, &[C_DQ], tr(S_ATTR_DONE, A_VAL_END));
    set(&mut t, S_VAL_DQ, &[C_AMP], tr(S_REF, A_REF_START));
    set(&mut t, S_VAL_DQ, &[C_LT, C_CTRL], ERR);
    fill(&mut t, S_VAL_SQ, tr(S_VAL_SQ, A_CHAR));
    set(&mut t, S_VAL_SQ, &[C_SQ], tr(S_ATTR_DONE, A_VAL_END));
    set(&mut t, S_VAL_SQ, &[C_AMP], tr(S_REF, A_REF_START));
    set(&mut t, S_VAL_SQ, &[C_LT, C_CTRL], ERR);
    // Entre atributos el espacio es obligatorio: `a="1"b="2"` es error.
    set(&mut t, S_ATTR_DONE, &WSP, tr(S_STAG_WS, A_SKIP));
    set(&mut t, S_ATTR_DONE, &[C_GT], stag_close);
    set(&mut t, S_ATTR_DONE, &[C_SLASH], to_empty);
    set(&mut t, S_EMPTY, &[C_GT], tr(S_CONTENT, A_EMPTY_CLOSE));

    // Etiqueta de cierre.
    set(&mut t, S_ETAG, &NAME_START, tr(S_ETAG_NAME, A_CHAR));
    set(&mut t, S_ETAG_NAME, &NAME_CHAR, tr(S_ETAG_NAME, A_CHAR));
    set(&mut t, S_ETAG_NAME, &WSP, tr(S_ETAG_WS, A_SKIP));
    set(&mut t, S_ETAG_NAME, &[C_GT], tr(S_CONTENT, A_ETAG_CLOSE));
    set(&mut t, S_ETAG_WS, &WSP, tr(S_ETAG_WS, A_SKIP));
    set(&mut t, S_ETAG_WS, &[C_GT], tr(S_CONTENT, A_ETAG_CLOSE));

    // Instrucciones de procesamiento: `<?nombre?>` o `<?nombre S contenido?>`.
    // El `?` del contenido se retiene hasta saber si cierra.
    set(&mut t, S_PI_START, &NAME_START, tr(S_PI_TARGET, A_PI_OPEN));
    set(&mut t, S_PI_TARGET, &NAME_CHAR, tr(S_PI_TARGET, A_CHAR));
    set(&mut t, S_PI_TARGET, &WSP, tr(S_PI, A_SKIP));
    set(&mut t, S_PI_TARGET, &[C_QMARK], tr(S_PI_TQ, A_SKIP));
    set(&mut t, S_PI_TQ, &[C_GT], tr(S_CONTENT, A_PI_CLOSE));
    fill(&mut t, S_PI, tr(S_PI, A_CHAR));
    set(&mut t, S_PI, &[C_QMARK], tr(S_PI_Q, A_SKIP));
    set(&mut t, S_PI, &[C_CTRL], ERR);
    fill(&mut t, S_PI_Q, tr(S_PI, A_PI_FLUSH));
    set(&mut t, S_PI_Q, &[C_QMARK], tr(S_PI_Q, A_PI_Q));
    set(&mut t, S_PI_Q, &[C_GT], tr(S_CONTENT, A_PI_CLOSE));
    set(&mut t, S_PI_Q, &[C_CTRL], ERR);

    // `<!`: comentario o CDATA. `<!D…` (DOCTYPE) y todo lo demás es error.
    set(&mut t, S_BANG, &[C_DASH], tr(S_BANG_DASH, A_SKIP));
    set(&mut t, S_BANG, &[C_LBRACK], tr(S_CD_LB, A_SKIP));
    set(&mut t, S_BANG_DASH, &[C_DASH], tr(S_COMMENT, A_SKIP));
    fill(&mut t, S_COMMENT, tr(S_COMMENT, A_CHAR));
    set(&mut t, S_COMMENT, &[C_DASH], tr(S_COM_D1, A_SKIP));
    set(&mut t, S_COMMENT, &[C_CTRL], ERR);
    fill(&mut t, S_COM_D1, tr(S_COMMENT, A_CHAR));
    set(&mut t, S_COM_D1, &[C_DASH], tr(S_COM_D2, A_SKIP));
    set(&mut t, S_COM_D1, &[C_CTRL], ERR);
    // `--` dentro de un comentario está prohibido salvo como `-->`.
    set(&mut t, S_COM_D2, &[C_GT], tr(S_CONTENT, A_SKIP));

    // CDATA: `<![CDATA[ … ]]>`. Los `]` se retienen hasta saber si cierran.
    set(&mut t, S_CD_LB, &[C_UC], tr(S_CD_C, A_SKIP));
    set(&mut t, S_CD_C, &[C_UD], tr(S_CD_D, A_SKIP));
    set(&mut t, S_CD_D, &[C_UA], tr(S_CD_A, A_SKIP));
    set(&mut t, S_CD_A, &[C_UT], tr(S_CD_T, A_SKIP));
    set(&mut t, S_CD_T, &[C_UA], tr(S_CD_A2, A_SKIP));
    set(&mut t, S_CD_A2, &[C_LBRACK], tr(S_CDATA, A_SKIP));
    fill(&mut t, S_CDATA, tr(S_CDATA, A_CHAR));
    set(&mut t, S_CDATA, &[C_RBRACK], tr(S_CD_RB1, A_SKIP));
    set(&mut t, S_CDATA, &[C_CTRL], ERR);
    fill(&mut t, S_CD_RB1, tr(S_CDATA, A_CD_FLUSH1));
    set(&mut t, S_CD_RB1, &[C_RBRACK], tr(S_CD_RB2, A_SKIP));
    set(&mut t, S_CD_RB1, &[C_CTRL], ERR);
    fill(&mut t, S_CD_RB2, tr(S_CDATA, A_CD_FLUSH2));
    set(&mut t, S_CD_RB2, &[C_RBRACK], tr(S_CD_RB2, A_CD_RB));
    set(&mut t, S_CD_RB2, &[C_GT], tr(S_CONTENT, A_SKIP));
    set(&mut t, S_CD_RB2, &[C_CTRL], ERR);

    // Referencias. Tras `;` se vuelve al estado guardado en `ref_ret`
    // (contenido, valor" o valor'); la tabla indica el caso del contenido.
    set(&mut t, S_REF, &[C_HASH], tr(S_REF_HASH, A_SKIP));
    set(&mut t, S_REF, &ENTITY_START, tr(S_REF_NAME, A_REF_NAME));
    set(&mut t, S_REF_NAME, &ENTITY_CHAR, tr(S_REF_NAME, A_REF_NAME));
    set(&mut t, S_REF_NAME, &[C_SEMI], tr(S_CONTENT, A_REF_NAME_END));
    set(&mut t, S_REF_HASH, &[C_X], tr(S_REF_X, A_SKIP));
    set(&mut t, S_REF_HASH, &[C_DIGIT], tr(S_REF_DEC, A_REF_DEC));
    set(&mut t, S_REF_DEC, &[C_DIGIT], tr(S_REF_DEC, A_REF_DEC));
    set(&mut t, S_REF_DEC, &[C_SEMI], tr(S_CONTENT, A_REF_NUM_END));
    set(&mut t, S_REF_X, &HEX, tr(S_REF_HEX, A_REF_HEX));
    set(&mut t, S_REF_HEX, &HEX, tr(S_REF_HEX, A_REF_HEX));
    set(&mut t, S_REF_HEX, &[C_SEMI], tr(S_CONTENT, A_REF_NUM_END));

    t
}

fn error_kind(state: usize, class: usize) -> LexErrorKind {
    match (state, class) {
        (_, C_CTRL) => LexErrorKind::InvalidXmlChar,
        (S_BANG, C_UD) => LexErrorKind::DoctypeForbidden,
        (S_PI_START | S_PI_TARGET | S_PI_TQ, _) => LexErrorKind::InvalidPi,
        (S_REF..=S_REF_HEX, _) => LexErrorKind::InvalidReference,
        _ => LexErrorKind::UnexpectedByte,
    }
}

/// Producción `Char` de XML 1.0.
fn is_xml_char(cp: u32) -> bool {
    matches!(cp, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn hex_value(b: u8) -> u32 {
    u32::from(match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
    })
}

// ----------------------------------------------------------------- lexer ---

/// Tokenizador XML reanudable. No reserva memoria.
#[derive(Clone, Debug)]
pub struct XmlLexer {
    state: u8,
    pos: Pos,
    /// Inicio de la marca en curso (`<`).
    tok_start: Pos,
    /// Inicio del carácter en curso (referencia o secuencia UTF-8).
    char_start: Pos,
    /// Comilla de apertura del valor de atributo en curso.
    val_start: Pos,
    utf8: Utf8Decoder,
    /// Estado al que se vuelve tras una referencia.
    ref_ret: u8,
    ref_name: [u8; 4],
    ref_len: u8,
    ref_val: u32,
    failed: Option<LexError>,
}

impl Default for XmlLexer {
    fn default() -> Self {
        Self::new()
    }
}

type Res<S> = Result<(), Error<<S as Sink<Event>>::Error>>;

impl XmlLexer {
    pub const fn new() -> Self {
        Self {
            state: S_CONTENT as u8,
            pos: Pos::START,
            tok_start: Pos::START,
            char_start: Pos::START,
            val_start: Pos::START,
            utf8: Utf8Decoder::new(),
            ref_ret: S_CONTENT as u8,
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

    /// Indica el fin de la entrada.
    pub fn finish<S: Sink<Event>>(&mut self, _sink: &mut S) -> Res<S> {
        if let Some(e) = self.failed {
            return Err(Error::Lex(e));
        }
        if self.utf8.is_pending() {
            return self.fail_at(LexErrorKind::InvalidUtf8, self.char_start);
        }
        if !is_accepting(self.state.into()) {
            return self.fail(LexErrorKind::UnexpectedEof);
        }
        Ok(())
    }

    fn byte<S: Sink<Event>>(&mut self, b: u8, sink: &mut S) -> Res<S> {
        if self.utf8.is_pending() {
            return match self.utf8.step(b) {
                Step::Pending => Ok(()),
                // BOM inicial: lo decodifica el AFD UTF-8 como cualquier
                // carácter y se descarta aquí (no ocupa columna). Una
                // referencia `&#xFEFF;` al inicio no pasa por aquí y se conserva.
                Step::Char('\u{feff}') if self.char_start.offset == 0 => {
                    self.pos.column = 1;
                    Ok(())
                }
                Step::Char(c) => self.emit_char(sink, c),
                Step::Invalid => self.fail_at(LexErrorKind::InvalidUtf8, self.char_start),
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
            A_STAG_OPEN => {
                let lt = self.tok_start;
                self.emit_until(sink, Event::StartTagOpen, lt, lt.offset + 1)?;
                self.char(sink, b)
            }
            A_ETAG_OPEN => self.emit(sink, Event::EndTagOpen, self.tok_start),
            A_ATTR_BEGIN => {
                self.emit(sink, Event::AttrBegin, here)?;
                self.char(sink, b)
            }
            A_VAL_BEGIN => {
                self.val_start = here;
                self.emit(sink, Event::AttrValueBegin, here)
            }
            A_VAL_END => self.emit(sink, Event::AttrValueEnd, self.val_start),
            A_STAG_CLOSE => self.emit(sink, Event::StartTagClose, self.tok_start),
            A_EMPTY_CLOSE => self.emit(sink, Event::EmptyTagClose, self.tok_start),
            A_ETAG_CLOSE => self.emit(sink, Event::EndTagClose, self.tok_start),
            A_REF_START => {
                self.ref_ret = match state {
                    S_VAL_DQ | S_VAL_SQ => state as u8,
                    _ => S_CONTENT as u8,
                };
                self.char_start = here;
                self.ref_len = 0;
                self.ref_val = 0;
                Ok(())
            }
            A_REF_NAME => {
                if usize::from(self.ref_len) == self.ref_name.len() {
                    return self.fail_at(LexErrorKind::UndefinedEntity, self.char_start);
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
                    _ => return self.fail_at(LexErrorKind::UndefinedEntity, self.char_start),
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
                    return self.fail_at(LexErrorKind::InvalidCharRef, self.char_start);
                }
                Ok(())
            }
            A_REF_NUM_END => {
                let Some(c) = char::from_u32(self.ref_val).filter(|&c| is_xml_char(c.into()))
                else {
                    return self.fail_at(LexErrorKind::InvalidCharRef, self.char_start);
                };
                self.state = self.ref_ret;
                self.emit_char(sink, c)
            }
            A_CD_RB => self.emit_until(sink, Event::TextChar(']'), back(here, 2), here.offset - 1),
            A_CD_FLUSH1 => {
                self.emit_until(sink, Event::TextChar(']'), back(here, 1), here.offset)?;
                self.char(sink, b)
            }
            A_PI_OPEN => {
                let lt = self.tok_start;
                self.emit_until(sink, Event::PiOpen, lt, lt.offset + 2)?;
                self.char(sink, b)
            }
            A_PI_Q => self.emit_until(sink, Event::PiChar('?'), back(here, 1), here.offset),
            A_PI_FLUSH => {
                self.emit_until(sink, Event::PiChar('?'), back(here, 1), here.offset)?;
                self.char(sink, b)
            }
            A_PI_CLOSE => self.emit(sink, Event::PiClose, self.tok_start),
            A_CD_FLUSH2 => {
                self.emit_until(sink, Event::TextChar(']'), back(here, 2), here.offset - 1)?;
                self.emit_until(sink, Event::TextChar(']'), back(here, 1), here.offset)?;
                self.char(sink, b)
            }
            _ => {
                self.state = state as u8;
                self.fail(error_kind(state, class))
            }
        }
    }

    /// Carácter ASCII, o primer byte de una secuencia UTF-8.
    fn char<S: Sink<Event>>(&mut self, sink: &mut S, b: u8) -> Res<S> {
        self.char_start = self.pos;
        if b.is_ascii() {
            self.emit_char(sink, char::from(b))
        } else if self.utf8.step(b) == Step::Invalid {
            self.fail(LexErrorKind::InvalidUtf8)
        } else {
            Ok(())
        }
    }

    /// Emite un carácter completo según el estado: nombre, valor de atributo,
    /// texto, contenido de instrucción, o nada dentro de comentarios.
    fn emit_char<S: Sink<Event>>(&mut self, sink: &mut S, c: char) -> Res<S> {
        if !is_xml_char(c.into()) {
            return self.fail_at(LexErrorKind::InvalidXmlChar, self.char_start);
        }
        let ev = match usize::from(self.state) {
            S_STAG_NAME | S_ATTR_NAME | S_ETAG_NAME | S_PI_TARGET => Event::NameChar(c),
            S_VAL_DQ | S_VAL_SQ => Event::AttrValueChar(c),
            S_PI | S_PI_Q => Event::PiChar(c),
            S_COMMENT | S_COM_D1 => return Ok(()),
            _ => Event::TextChar(c),
        };
        self.emit(sink, ev, self.char_start)
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

    fn fail<E>(&mut self, kind: LexErrorKind) -> Result<(), Error<E>> {
        self.fail_at(kind, self.pos)
    }

    fn fail_at<E>(&mut self, kind: LexErrorKind, at: Pos) -> Result<(), Error<E>> {
        let e = LexError { kind, at };
        self.failed = Some(e);
        Err(Error::Lex(e))
    }
}

/// Posición `k` bytes atrás en la misma línea (solo para `]` retenidos).
fn back(p: Pos, k: u32) -> Pos {
    Pos {
        offset: p.offset - u64::from(k),
        line: p.line,
        column: p.column - k,
    }
}

/// Escribe el AFD del tokenizador en formato DOT (Graphviz).
pub fn write_dot(w: &mut impl core::fmt::Write) -> core::fmt::Result {
    const RET_DQ: &str = "; [regreso = valor\"]";
    const RET_SQ: &str = "; [regreso = valor']";
    crate::dot::write(
        w,
        &crate::dot::Dfa {
            name: "xml",
            title: "AFD del tokenizador XML (subconjunto de XML 1.0, sin DTD)",
            num_states: NUM_STATES,
            num_classes: NUM_CLASSES,
            next: &|s, c| usize::from(TABLE[s][c].next),
            state_names: &STATE_NAMES,
            class_names: &CLASS_NAMES,
            start: S_CONTENT,
            accepting: &is_accepting,
            error: S_ERR,
            guarded: &[
                (S_REF_NAME, S_VAL_DQ, RET_DQ),
                (S_REF_NAME, S_VAL_SQ, RET_SQ),
                (S_REF_DEC, S_VAL_DQ, RET_DQ),
                (S_REF_DEC, S_VAL_SQ, RET_SQ),
                (S_REF_HEX, S_VAL_DQ, RET_DQ),
                (S_REF_HEX, S_VAL_SQ, RET_SQ),
            ],
        },
    )
}
