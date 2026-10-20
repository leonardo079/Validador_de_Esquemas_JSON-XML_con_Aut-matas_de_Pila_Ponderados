//! Pruebas del tokenizador JSON (T1.8–T1.10, T1.13).

use std::convert::Infallible;

use lexer::json::{Event, JsonLexer};
use lexer::{Error, LexError, LexErrorKind, Sink, Span};
use proptest::prelude::*;

mod common;

#[derive(Default)]
struct Collect(Vec<(Event, Span)>);

impl Sink<Event> for Collect {
    type Error = Infallible;
    fn event(&mut self, ev: Event, span: Span) -> Result<(), Infallible> {
        self.0.push((ev, span));
        Ok(())
    }
}

type Outcome = (Vec<(Event, Span)>, Option<LexError>);

/// Tokeniza `input` partido en bloques de `chunk` bytes.
fn lex_chunked(input: &[u8], chunk: usize) -> Outcome {
    lex_split(input, input.chunks(chunk.max(1)))
}

fn lex_split<'a>(input: &[u8], parts: impl Iterator<Item = &'a [u8]>) -> Outcome {
    let _ = input;
    let mut lx = JsonLexer::new();
    let mut out = Collect::default();
    let mut res = Ok(());
    for p in parts {
        res = lx.feed(p, &mut out);
        if res.is_err() {
            break;
        }
    }
    if res.is_ok() {
        res = lx.finish(&mut out);
    }
    let err = res.err().map(|e| match e {
        Error::Lex(e) => e,
        Error::Sink(never) => match never {},
    });
    (out.0, err)
}

fn lex(input: &str) -> Outcome {
    lex_chunked(input.as_bytes(), usize::MAX)
}

/// Reconstruye los tokens como texto: cadenas decodificadas entre comillas,
/// números y literales tal cual.
fn tokens(input: &str) -> Vec<String> {
    let (events, err) = lex(input);
    assert_eq!(err, None, "error inesperado en {input:?}");
    let mut out = Vec::new();
    let mut cur = String::new();
    for (ev, _) in events {
        match ev {
            Event::BeginObject => out.push("{".into()),
            Event::EndObject => out.push("}".into()),
            Event::BeginArray => out.push("[".into()),
            Event::EndArray => out.push("]".into()),
            Event::Colon => out.push(":".into()),
            Event::Comma => out.push(",".into()),
            Event::True => out.push("true".into()),
            Event::False => out.push("false".into()),
            Event::Null => out.push("null".into()),
            Event::StringBegin | Event::NumberBegin => cur.clear(),
            Event::StringChar(c) => cur.push(c),
            Event::Number(_, b) => cur.push(char::from(b)),
            Event::StringEnd => out.push(format!("\"{cur}\"")),
            Event::NumberEnd => out.push(cur.clone()),
        }
    }
    out
}

fn error(input: &[u8]) -> LexError {
    lex_chunked(input, usize::MAX)
        .1
        .unwrap_or_else(|| panic!("se esperaba error en {:?}", String::from_utf8_lossy(input)))
}

#[test]
fn estructura_y_literales() {
    assert_eq!(
        tokens(r#" { "a" : [ true , false , null ] } "#),
        ["{", "\"a\"", ":", "[", "true", ",", "false", ",", "null", "]", "}"]
    );
    assert_eq!(tokens(""), Vec::<String>::new());
    assert_eq!(tokens("\t\r\n "), Vec::<String>::new());
    // Sin separadores: el lexer no exige espacios entre tokens.
    assert_eq!(tokens("[1,2]"), ["[", "1", ",", "2", "]"]);
    assert_eq!(tokens("{}[]"), ["{", "}", "[", "]"]);
}

#[test]
fn numeros_validos() {
    for n in [
        "0",
        "-0",
        "7",
        "-7",
        "10",
        "123456789",
        "0.5",
        "-0.5",
        "3.14159",
        "1e5",
        "1E5",
        "1e+5",
        "1e-5",
        "0e0",
        "-1.5E-300",
        "12.0e10",
        "100000000000000000000000000000",
    ] {
        assert_eq!(tokens(n), [n], "número {n}");
        assert_eq!(
            tokens(&format!("[{n}]")),
            ["[", n, "]"],
            "número {n} en arreglo"
        );
    }
}

#[test]
fn numeros_invalidos() {
    for n in [
        "01", "-", "-a", "1.", "1.e5", ".5", "1e", "1e+", "+1", "1.2.3", "1e5e", "-0-", "00",
    ] {
        let e = error(n.as_bytes());
        assert!(
            matches!(
                e.kind,
                LexErrorKind::InvalidNumber
                    | LexErrorKind::UnexpectedByte
                    | LexErrorKind::UnexpectedEof
            ),
            "número {n}: {e:?}"
        );
    }
    assert_eq!(error(b"01").kind, LexErrorKind::InvalidNumber);
    assert_eq!(error(b"01").at.offset, 1);
    // Un número incompleto al final es un número inválido, igual que `[1.]`.
    assert_eq!(error(b"1.").kind, LexErrorKind::InvalidNumber);
    assert_eq!(error(b"-").kind, LexErrorKind::InvalidNumber);
    assert_eq!(error(b"1e+").kind, LexErrorKind::InvalidNumber);
    assert_eq!(error(b"[1.]").kind, LexErrorKind::InvalidNumber);
    assert_eq!(error(b"+1").kind, LexErrorKind::UnexpectedByte);
}

#[test]
fn partes_de_numero() {
    use lexer::json::NumPart::*;
    let (events, _) = lex("-12.5e+3");
    let parts: Vec<_> = events
        .iter()
        .filter_map(|(e, _)| match e {
            Event::Number(p, _) => Some(*p),
            _ => None,
        })
        .collect();
    assert_eq!(parts, [Minus, Int, Int, Dot, Frac, ExpMark, ExpSign, Exp]);
}

#[test]
fn cadenas_y_escapes() {
    assert_eq!(tokens(r#""""#), [r#""""#]);
    assert_eq!(tokens(r#""hola mundo""#), [r#""hola mundo""#]);
    assert_eq!(tokens(r#""a\"b\\c\/d""#), ["\"a\"b\\c/d\""]);
    assert_eq!(tokens(r#""\b\f\n\r\t""#), ["\"\u{8}\u{c}\n\r\t\""]);
    assert_eq!(tokens(r#""\u00e9\u00E9""#), ["\"éé\""]);
    assert_eq!(tokens(r#""\u0000""#), ["\"\u{0}\""]);
    // Par sustituto → U+1F600.
    assert_eq!(tokens(r#""\ud83d\ude00""#), ["\"😀\""]);
    // UTF-8 directo de 2, 3 y 4 bytes.
    assert_eq!(tokens("\"ñ€😀\""), ["\"ñ€😀\""]);
    // DEL (0x7F) está permitido sin escapar.
    assert_eq!(tokens("\"\x7f\""), ["\"\x7f\""]);
}

#[test]
fn cadenas_invalidas() {
    let cases: &[(&[u8], LexErrorKind, u64)] = &[
        (b"\"a\nb\"", LexErrorKind::ControlCharInString, 2),
        (b"\"a\tb\"", LexErrorKind::ControlCharInString, 2),
        (b"\"\x01\"", LexErrorKind::ControlCharInString, 1),
        (b"\"\\x\"", LexErrorKind::InvalidEscape, 2),
        (b"\"\\u12G4\"", LexErrorKind::InvalidEscape, 5),
        (b"\"\\ud800\"", LexErrorKind::LoneSurrogate, 1),
        (b"\"\\ud800\\n\"", LexErrorKind::LoneSurrogate, 1),
        (b"\"\\ud800\\u0041\"", LexErrorKind::LoneSurrogate, 1),
        (b"\"\\udc00\"", LexErrorKind::LoneSurrogate, 1),
        (b"\"abc", LexErrorKind::UnexpectedEof, 4),
        (b"\"\xc3\"", LexErrorKind::InvalidUtf8, 1),
        (b"\"\xc0\x80\"", LexErrorKind::InvalidUtf8, 1),
        (b"\"\xed\xa0\x80\"", LexErrorKind::InvalidUtf8, 1),
        (b"\"\xf4\x90\x80\x80\"", LexErrorKind::InvalidUtf8, 1),
        (b"\"\x80\"", LexErrorKind::InvalidUtf8, 1),
        (b"\"\xff\"", LexErrorKind::InvalidUtf8, 1),
    ];
    for &(input, kind, offset) in cases {
        let e = error(input);
        assert_eq!(
            (e.kind, e.at.offset),
            (kind, offset),
            "{:?}",
            String::from_utf8_lossy(input)
        );
    }
}

#[test]
fn literales_invalidos() {
    assert_eq!(error(b"tru").kind, LexErrorKind::UnexpectedEof);
    assert_eq!(error(b"trUe").kind, LexErrorKind::InvalidLiteral);
    assert_eq!(error(b"nul ").kind, LexErrorKind::InvalidLiteral);
    assert_eq!(error(b"True").kind, LexErrorKind::UnexpectedByte);
    // `truex`: el literal está completo y la `x` no inicia ningún token.
    assert_eq!(error(b"truex").kind, LexErrorKind::UnexpectedByte);
    assert_eq!(error(b"truex").at.offset, 4);
}

#[test]
fn bytes_fuera_de_cadena() {
    for (input, offset) in [
        (&b"[1, @]"[..], 4),
        (b"'a'", 0),
        (b"\xc3\xb1", 0),
        (b"[\x00]", 1),
    ] {
        let e = error(input);
        assert_eq!(
            (e.kind, e.at.offset),
            (LexErrorKind::UnexpectedByte, offset)
        );
    }
}

#[test]
fn bom_inicial() {
    assert_eq!(tokens("\u{feff}[1]"), ["[", "1", "]"]);
    assert_eq!(error(b"\xef\xbb").kind, LexErrorKind::InvalidUtf8);
    // El BOM no ocupa columna.
    let e = error(b"\xef\xbb\xbf@");
    assert_eq!((e.at.offset, e.at.column), (3, 1));
    // D10: un prefijo de BOM seguido de otro byte es un byte no ASCII fuera
    // de cadena (antes de D10, este caso era `InvalidUtf8`).
    assert_eq!(error(b"\xef\xbbx").kind, LexErrorKind::UnexpectedByte);
    // Un BOM fuera del inicio no se acepta.
    assert_eq!(error(b" \xef\xbb\xbf").kind, LexErrorKind::UnexpectedByte);
}

/// D10 (`docs/fase1/lexico.md` §7): al inicio, un prefijo de BOM (`EF`,
/// `EF BB`) seguido de otro byte es `UnexpectedByte` en el offset 0, línea 1,
/// columna 1, igual que cualquier byte no ASCII fuera de cadena, aunque la
/// secuencia sea UTF-8 válido (`EF BC 81` = U+FF01). Si la entrada termina
/// justo después del prefijo, el error es `InvalidUtf8` en el offset 0
/// (UTF-8 truncado, como en `finish`).
#[test]
fn prefijo_de_bom_seguido_de_otro_byte() {
    use LexErrorKind::{InvalidUtf8, UnexpectedByte};
    let casos: [(&[u8], LexErrorKind, u64, u32, u32); 8] = [
        (b"\xef\xbc\x81", UnexpectedByte, 0, 1, 1),
        (b"\xef\xbb\x81[1]", UnexpectedByte, 0, 1, 1),
        (b"\xef1", UnexpectedByte, 0, 1, 1),
        (b"[1,\xef\xbc\x81]", UnexpectedByte, 3, 1, 4),
        (b"\xc3\xb1", UnexpectedByte, 0, 1, 1),
        // Prefijo al final de la entrada: UTF-8 truncado.
        (b"\xef", InvalidUtf8, 0, 1, 1),
        (b"\xef\xbb", InvalidUtf8, 0, 1, 1),
        // Diferencia conocida: `C3` solo, al inicio, no pasa por el contador
        // del BOM y es un byte no ASCII fuera de cadena.
        (b"\xc3", UnexpectedByte, 0, 1, 1),
    ];
    for (input, kind, offset, line, column) in casos {
        for size in [1, 7, 4096] {
            let e = lex_chunked(input, size)
                .1
                .unwrap_or_else(|| panic!("se esperaba error en {input:?}"));
            assert_eq!(
                (e.kind, e.at.offset, e.at.line, e.at.column),
                (kind, offset, line, column),
                "{input:?} en bloques de {size}"
            );
        }
    }
}

#[test]
fn linea_y_columna() {
    let e = error("{\n  \"ñandú\": 01\n}".as_bytes());
    assert_eq!(e.kind, LexErrorKind::InvalidNumber);
    assert_eq!((e.at.line, e.at.column), (2, 13));
    let e = error(b"[\n\n   x]");
    assert_eq!((e.at.line, e.at.column, e.at.offset), (3, 4, 6));
}

#[test]
fn rangos_de_los_tokens() {
    let input = r#"{"clé": [-1.5e3, true, "\u00e9"]}"#;
    let (events, err) = lex(input);
    assert_eq!(err, None);
    let text = |s: &Span| &input[s.start.offset as usize..s.end as usize];
    let mut seen = Vec::new();
    for (ev, span) in &events {
        match ev {
            Event::StringEnd | Event::NumberEnd | Event::True | Event::Colon => {
                seen.push(text(span))
            }
            _ => {}
        }
    }
    assert_eq!(seen, [r#""clé""#, ":", "-1.5e3", "true", r#""\u00e9""#]);
    // Cada carácter cubre sus bytes de origen: 2 en UTF-8, 6 en el escape.
    let e_spans: Vec<&str> = events
        .iter()
        .filter(|(e, _)| *e == Event::StringChar('\u{e9}'))
        .map(|(_, s)| text(s))
        .collect();
    assert_eq!(e_spans, ["\u{e9}", "\\u00e9"]);
}

#[test]
fn utf8_truncado_al_final() {
    // Mismo error que en XML: la secuencia está incompleta.
    let e = error(b"\"\xe2\x82");
    assert_eq!((e.kind, e.at.offset), (LexErrorKind::InvalidUtf8, 1));
}

#[test]
fn error_del_sink_detiene_el_lexer() {
    struct Falla;
    impl Sink<Event> for Falla {
        type Error = &'static str;
        fn event(&mut self, _: Event, _: Span) -> Result<(), &'static str> {
            Err("basta")
        }
    }
    let mut lx = JsonLexer::new();
    assert_eq!(lx.feed(b"[", &mut Falla), Err(Error::Sink("basta")));
    let siguiente = lx.feed(b"]", &mut Falla);
    assert!(matches!(
        siguiente,
        Err(Error::Lex(LexError {
            kind: LexErrorKind::Aborted,
            ..
        }))
    ));
    assert!(lx.finish(&mut Falla).is_err());
}

#[test]
fn error_detiene_el_lexer() {
    let mut lx = JsonLexer::new();
    let mut out = Collect::default();
    assert!(lx.feed(b"[01", &mut out).is_err());
    let first = lx.feed(b"]", &mut out);
    assert!(matches!(
        first,
        Err(Error::Lex(LexError {
            kind: LexErrorKind::InvalidNumber,
            ..
        }))
    ));
}

#[test]
fn corpus_en_bloques() {
    // El archivo de todo caso es JSON, sea cual sea el formato del payload.
    for case in common::corpus() {
        common::por_bloques(&case.file, None, &case.name(), lex_chunked);
    }
    common::corpus_en_bloques("json", lex_chunked);
}

#[test]
fn bloques_de_1_7_y_4096_bytes() {
    let input = r#"{"usuarios": [{"nombre": "Ñandú 😀", "edad": 30, "saldo": -1.5e-3},
        {"nombre": "\ud83d\ude00\n", "activo": true, "x": null}], "vacio": {}, "n": 0}"#;
    let whole = lex(input);
    assert_eq!(whole.1, None);
    for size in [1, 7, 4096] {
        assert_eq!(
            lex_chunked(input.as_bytes(), size),
            whole,
            "bloques de {size}"
        );
    }
}

// ------------------------------------------------------------ propiedades ---

/// Fragmentos que recorren casi todos los estados, válidos o no.
fn fragment() -> impl Strategy<Value = &'static [u8]> {
    prop::sample::select(vec![
        &b"{"[..],
        b"}",
        b"[",
        b"]",
        b":",
        b",",
        b" ",
        b"\n",
        b"\"",
        b"\\",
        b"\\u",
        b"d83d",
        b"\\ude00",
        b"00e9",
        b"0",
        b"1",
        b"9",
        b"-",
        b"+",
        b".",
        b"e",
        b"E",
        b"true",
        b"tr",
        b"false",
        b"null",
        b"nu",
        b"abc",
        "ñ".as_bytes(),
        "😀".as_bytes(),
        b"\xf0\x9f",
        b"\x80",
        b"\xff",
        b"\x01",
        b"\t",
        b"\xef\xbb\xbf",
    ])
}

fn input() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(fragment(), 0..40).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Tokenizar entero == tokenizar en trozos arbitrarios (T1.9).
    #[test]
    fn trozos_arbitrarios(bytes in input(), cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..8)) {
        let whole = lex_chunked(&bytes, usize::MAX);
        let mut points: Vec<usize> = cuts.iter().map(|i| i.index(bytes.len() + 1)).collect();
        points.sort_unstable();
        let mut parts = Vec::new();
        let mut prev = 0;
        for p in points.into_iter().chain([bytes.len()]) {
            parts.push(&bytes[prev..p]);
            prev = p;
        }
        prop_assert_eq!(lex_split(&bytes, parts.into_iter()), whole);
    }

    /// Todo lo que serializa serde_json se tokeniza sin error y con el mismo
    /// contenido (claves, cadenas, números y literales en el mismo orden).
    #[test]
    fn oraculo_serde_json(v in json_value(), pretty in any::<bool>()) {
        let text = if pretty { serde_json::to_string_pretty(&v) } else { serde_json::to_string(&v) }.unwrap();
        let mut expected = Vec::new();
        atoms(&v, &mut expected);
        let got: Vec<String> = tokens(&text)
            .into_iter()
            .filter(|t| !matches!(t.as_str(), "{" | "}" | "[" | "]" | ":" | ","))
            .collect();
        prop_assert_eq!(got, expected);
    }
}

fn json_value() -> impl Strategy<Value = serde_json::Value> {
    use serde_json::Value;
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<f64>()
            .prop_filter("finito", |f| f.is_finite())
            .prop_map(Value::from),
        any::<String>().prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..8).prop_map(Value::Array),
            prop::collection::btree_map(any::<String>(), inner, 0..8)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

fn atoms(v: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value;
    match v {
        Value::Null => out.push("null".into()),
        Value::Bool(b) => out.push(b.to_string()),
        Value::Number(n) => out.push(n.to_string()),
        Value::String(s) => out.push(format!("\"{s}\"")),
        Value::Array(a) => a.iter().for_each(|x| atoms(x, out)),
        Value::Object(m) => m.iter().for_each(|(k, x)| {
            out.push(format!("\"{k}\""));
            atoms(x, out);
        }),
    }
}
