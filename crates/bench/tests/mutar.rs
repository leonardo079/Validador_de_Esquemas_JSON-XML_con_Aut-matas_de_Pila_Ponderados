//! Pruebas de las mutaciones (F6.3). Deterministas (D1).

use bench::generar::Azar;
use bench::mutar::{
    base64, clasificar, mutar, tramos, Esperado, Mutacion, Tipo, SEMANTICAS, SINTACTICAS,
};

const P: &[u8] = br#"{"a": [1, "x", true], "b": {"c": null}}"#;

#[test]
fn tramos_en_preorden_con_claves() {
    let t = tramos(P);
    let tipos: Vec<Tipo> = t.iter().map(|t| t.tipo).collect();
    assert_eq!(
        tipos,
        [
            Tipo::Objeto,
            Tipo::Arreglo,
            Tipo::Numero,
            Tipo::Cadena,
            Tipo::Literal,
            Tipo::Objeto,
            Tipo::Literal
        ]
    );
    let texto = |i: usize| &P[t[i].ini..t[i].fin];
    assert_eq!(texto(0), P);
    assert_eq!(texto(1), br#"[1, "x", true]"#);
    assert_eq!(texto(3), br#""x""#);
    assert_eq!(texto(5), br#"{"c": null}"#);
    assert_eq!(t[1].clave, Some(1));
    assert_eq!(t[6].clave, Some(28));
    assert!(t[2].en_arreglo && !t[1].en_arreglo);
    assert_eq!(t[0].clave, None);
}

#[test]
fn las_semanticas_dejan_json_bien_formado() {
    for &m in SEMANTICAS {
        for s in 0..30 {
            if let Some(q) = mutar(P, m, &mut Azar::new(s)) {
                assert_ne!(q, P);
                let r = serde_json::from_slice::<serde_json::Value>(&q);
                assert!(r.is_ok(), "{m:?} {s}: {}", String::from_utf8_lossy(&q));
            }
        }
    }
}

#[test]
fn las_sintacticas_rompen_el_json() {
    for &m in SINTACTICAS {
        let mut alguna = false;
        for s in 0..30 {
            if let Some(q) = mutar(P, m, &mut Azar::new(s)) {
                alguna = true;
                // `serde_json` acepta claves repetidas; el proyecto no (D2).
                if m != Mutacion::ClaveDuplicada {
                    let r = serde_json::from_slice::<serde_json::Value>(&q);
                    assert!(r.is_err(), "{m:?} {s}: {}", String::from_utf8_lossy(&q));
                }
            }
        }
        assert!(alguna, "{m:?} no se aplicó nunca");
    }
}

#[test]
fn misma_semilla_misma_mutacion() {
    for &m in SEMANTICAS.iter().chain(SINTACTICAS) {
        for s in 0..10 {
            assert_eq!(
                mutar(P, m, &mut Azar::new(s)),
                mutar(P, m, &mut Azar::new(s))
            );
        }
    }
}

#[test]
fn base64_rfc4648() {
    let casos: [(&[u8], &str); 7] = [
        (b"", ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"fooba", "Zm9vYmE="),
        (b"foobar", "Zm9vYmFy"),
    ];
    for (b, e) in casos {
        assert_eq!(base64(b), e);
    }
    // El caso `malformed-json-invalid-utf8-01` del corpus.
    assert_eq!(base64(b"{\"n\": \"\xff\"}"), "eyJuIjogIv8ifQ==");
}

#[test]
fn clasificar_coincide_con_casos_conocidos() {
    let e =
        br#"{"type":"object","properties":{"n":{"type":"integer","maximum":10}},"required":["n"]}"#;
    let m = bench::automata(e).expect("tiene M");
    let lb =
        baseline::with_stack(|_| baseline::JsonBaseline::compile_text(e).ok()).expect("compila");
    assert_eq!(clasificar(&m, &lb, br#"{"n": 3}"#), None);
    assert!(matches!(
        clasificar(&m, &lb, br#"{"n": 30}"#),
        Some(Esperado::Invalido { violaciones: 1, .. })
    ));
    assert_eq!(
        clasificar(&m, &lb, br#"{"n": 01}"#),
        Some(Esperado::Malformado {
            lexico: true,
            offset: 7
        })
    );
    assert_eq!(
        clasificar(&m, &lb, br#"{"n": 1,}"#),
        Some(Esperado::Malformado {
            lexico: false,
            offset: 8
        })
    );
}
