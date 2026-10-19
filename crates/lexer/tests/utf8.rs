//! Pruebas del AFD UTF-8 (T1.7).

use lexer::utf8::{validate, Step, Utf8Decoder};
use proptest::prelude::*;

#[test]
fn secuencias_validas() {
    for s in [
        "",
        "abc",
        "ñ",
        "€",
        "😀",
        "\u{7f}\u{80}\u{7ff}\u{800}\u{ffff}\u{10000}\u{10ffff}",
    ] {
        assert_eq!(validate(s.as_bytes()), Ok(()), "{s:?}");
    }
    // Bordes alrededor del hueco de sustitutos.
    assert_eq!(validate("\u{d7ff}\u{e000}".as_bytes()), Ok(()));
}

#[test]
fn secuencias_invalidas() {
    let cases: &[(&[u8], usize)] = &[
        (b"\x80", 0),             // continuación suelta
        (b"a\xbf", 1),            // continuación suelta
        (b"\xc0\x80", 0),         // sobrelarga de U+0000
        (b"\xc1\xbf", 0),         // sobrelarga
        (b"\xe0\x80\x80", 0),     // sobrelarga de 3 bytes
        (b"\xe0\x9f\xbf", 0),     // sobrelarga de 3 bytes (U+07FF)
        (b"\xf0\x80\x80\x80", 0), // sobrelarga de 4 bytes
        (b"\xf0\x8f\xbf\xbf", 0), // sobrelarga de 4 bytes (U+FFFF)
        (b"\xed\xa0\x80", 0),     // sustituto U+D800
        (b"\xed\xbf\xbf", 0),     // sustituto U+DFFF
        (b"\xf4\x90\x80\x80", 0), // U+110000
        (b"\xf5\x80\x80\x80", 0), // byte inicial prohibido
        (b"\xff", 0),             // byte prohibido
        (b"ab\xc3", 2),           // truncada al final
        (b"\xe2\x82", 0),         // truncada
        (b"\xe2\x82a", 0),        // interrumpida por ASCII
        (b"\xf0\x9f\x98", 0),     // truncada de 4 bytes
    ];
    for &(bytes, at) in cases {
        assert_eq!(validate(bytes), Err(at), "{bytes:x?}");
    }
}

#[test]
fn decodifica_puntos_de_codigo() {
    let mut d = Utf8Decoder::new();
    let out: Vec<Step> = "a€".bytes().map(|b| d.step(b)).collect();
    assert_eq!(
        out,
        [
            Step::Char('a'),
            Step::Pending,
            Step::Pending,
            Step::Char('€')
        ]
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]

    /// El AFD acepta exactamente lo que acepta `core::str::from_utf8`.
    #[test]
    fn coincide_con_std(bytes in prop::collection::vec(any::<u8>(), 0..16)) {
        prop_assert_eq!(
            validate(&bytes).is_ok(),
            std::str::from_utf8(&bytes).is_ok()
        );
    }

    /// Y decodifica los mismos caracteres.
    #[test]
    fn decodifica_igual_que_std(s in any::<String>()) {
        let mut d = Utf8Decoder::new();
        let chars: String = s.bytes().filter_map(|b| match d.step(b) {
            Step::Char(c) => Some(c),
            _ => None,
        }).collect();
        prop_assert_eq!(chars, s);
    }
}

/// El estado de cada tokenizador cabe en un tamaño fijo, independiente de la
/// entrada: memoria O(1) en la fase léxica.
#[test]
fn memoria_constante() {
    use std::mem::size_of;
    assert_eq!(size_of::<Utf8Decoder>(), 8);
    assert!(
        size_of::<lexer::json::JsonLexer>() <= 128,
        "{}",
        size_of::<lexer::json::JsonLexer>()
    );
    assert!(
        size_of::<lexer::xml::XmlLexer>() <= 128,
        "{}",
        size_of::<lexer::xml::XmlLexer>()
    );
    println!(
        "Utf8Decoder: {} B, JsonLexer: {} B, XmlLexer: {} B",
        size_of::<Utf8Decoder>(),
        size_of::<lexer::json::JsonLexer>(),
        size_of::<lexer::xml::XmlLexer>()
    );
}
