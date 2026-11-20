//! Criterio 7 de F4.6: payloads JSON y XML aleatorios y mutaciones de un
//! byte de payloads válidos no hacen entrar en pánico al ejecutor y dan el
//! mismo `Report` en bloques de 1, 7 y 4096 bytes. Con la capa de buena
//! formación, además: un payload JSON que `serde_json` rechaza nunca sale
//! válido.

use std::sync::OnceLock;

use automaton::{build, validate, Automaton, Mode, Report};
use proptest::prelude::*;

const ESQUEMA_JSON: &str = r##"{"$defs": {"n": {"type": "object",
    "properties": {"v": {"type": "integer", "minimum": 0},
      "s": {"type": "string", "maxLength": 4, "enum": ["a", "bb", "ccc"]},
      "c": {"type": "array", "maxItems": 3, "items": {"$ref": "#/$defs/n"}}},
    "required": ["v"], "dependentRequired": {"s": ["c"]}}}, "$ref": "#/$defs/n"}"##;

const ESQUEMA_XML: &str = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
    <xs:element name='pedido'><xs:complexType><xs:sequence>\
    <xs:element name='cliente' type='xs:string'/>\
    <xs:element name='nota' type='xs:string' minOccurs='0' maxOccurs='3'/>\
    <xs:element name='total' type='xs:decimal'/>\
    </xs:sequence><xs:attribute name='id' type='xs:positiveInteger' use='required'/>\
    </xs:complexType></xs:element></xs:schema>";

fn en_hilo<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn m_json() -> &'static Automaton {
    static M: OnceLock<Automaton> = OnceLock::new();
    M.get_or_init(|| {
        en_hilo(|| {
            let s = schema_parser::parse(ESQUEMA_JSON.as_bytes()).unwrap();
            build(&semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()).unwrap()
        })
    })
}

fn m_xml() -> &'static Automaton {
    static M: OnceLock<Automaton> = OnceLock::new();
    M.get_or_init(|| {
        en_hilo(|| {
            let s = xsd::parse(ESQUEMA_XML.as_bytes()).unwrap();
            build(&xsd::lower(&xsd::analyze(&s).unwrap()).unwrap()).unwrap()
        })
    })
}

fn tres_bloques(a: &Automaton, p: &[u8]) -> Report {
    let r = validate(a, p, Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate(a, p, Mode::Total, chunk),
            r,
            "bloques de {chunk}: {p:?}"
        );
    }
    let e = validate(a, p, Mode::EarlyFail, 7);
    assert_eq!(e.valid, r.valid, "{p:?}");
    r
}

/// Valores JSON de profundidad ≤ 4 con claves de un alfabeto pequeño (para
/// que se repitan) y escalares de todas las clases.
fn valor_json() -> impl Strategy<Value = String> {
    let hoja = prop_oneof![
        Just("null".to_owned()),
        Just("true".to_owned()),
        (-5i64..300).prop_map(|n| n.to_string()),
        Just("1.5e2".to_owned()),
        "[abcé\\\\ ]{0,5}".prop_map(|s| serde_json::to_string(&s).unwrap()),
    ];
    hoja.prop_recursive(4, 40, 4, |dentro| {
        prop_oneof![
            prop::collection::vec(dentro.clone(), 0..4).prop_map(|v| format!("[{}]", v.join(","))),
            prop::collection::vec(("[vscx]", dentro), 0..5).prop_map(|v| {
                let ms: Vec<String> = v.into_iter().map(|(k, x)| format!("\"{k}\":{x}")).collect();
                format!("{{{}}}", ms.join(", "))
            }),
        ]
    })
}

fn nota() -> impl Strategy<Value = String> {
    "[a-z &<>\r\n]{0,6}".prop_map(|s| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    })
}

/// Documentos de pedido casi válidos: atributos, notas, total y prefijos.
fn pedido() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just(""), Just("p:")],
        "[0-9]{1,3}",
        prop::collection::vec(nota(), 0..5),
        prop_oneof![Just("10.5"), Just("x"), Just(" 3 ")],
        prop_oneof![
            Just(""),
            Just(" z='1'"),
            Just(" id='2'"),
            Just(" xmlns:p='u'")
        ],
    )
        .prop_map(|(pre, id, notas, total, extra)| {
            let notas: String = notas.iter().map(|n| format!("<nota>{n}</nota>")).collect();
            format!(
                "<?xml version='1.0'?>\n<{pre}pedido id='{id}'{extra}>\
                 <cliente>Ana</cliente>{notas}<total>{total}</total></{pre}pedido>"
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn json_aleatorio(p in valor_json()) {
        let r = tres_bloques(m_json(), p.as_bytes());
        // `p` es JSON bien formado salvo claves repetidas (que `serde_json`
        // fusiona): el ejecutor da malformado si y solo si hay una repetida
        // en algún objeto, declarada o no (D2).
        prop_assert!(serde_json::from_str::<serde_json::Value>(&p).is_ok());
        prop_assert_eq!(
            r.error_kind == Some(automaton::ErrorKind::Syntax),
            tiene_repetidas(&p),
            "{}: {:?}", p, r
        );
    }

    #[test]
    fn json_mutado(p in valor_json(), i in any::<prop::sample::Index>(), b in any::<u8>()) {
        let mut v = p.into_bytes();
        let k = i.index(v.len());
        v[k] = b;
        let r = tres_bloques(m_json(), &v);
        if serde_json::from_slice::<serde_json::Value>(&v).is_err() {
            prop_assert!(!r.valid, "{:?}: {r:?}", String::from_utf8_lossy(&v));
        }
    }

    #[test]
    fn xml_aleatorio(p in pedido()) {
        tres_bloques(m_xml(), p.as_bytes());
    }

    #[test]
    fn xml_mutado(p in pedido(), i in any::<prop::sample::Index>(), b in any::<u8>()) {
        let mut v = p.into_bytes();
        let k = i.index(v.len());
        v[k] = b;
        tres_bloques(m_xml(), &v);
    }

    #[test]
    fn bytes_cualesquiera(v in prop::collection::vec(any::<u8>(), 0..64)) {
        tres_bloques(m_json(), &v);
        tres_bloques(m_xml(), &v);
    }
}

/// Si algún objeto de `p` repite una clave. Las cadenas de [`valor_json`] no
/// contienen `"` ni `:`, así que cada `":` es el final de una clave.
fn tiene_repetidas(p: &str) -> bool {
    use serde_json::Value;
    // Con `serde_json` las repetidas se fusionan: si el número de pares
    // escritos supera el de claves leídas en algún objeto, había repetidas.
    fn pares(v: &Value) -> usize {
        match v {
            Value::Object(m) => m.len() + m.values().map(pares).sum::<usize>(),
            Value::Array(a) => a.iter().map(pares).sum(),
            _ => 0,
        }
    }
    let v: Value = serde_json::from_str(p).unwrap();
    let escritas = p.matches("\":").count();
    escritas > pares(&v)
}
