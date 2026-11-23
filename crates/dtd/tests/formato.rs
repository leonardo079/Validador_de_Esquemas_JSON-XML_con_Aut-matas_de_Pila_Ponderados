//! DTD.4: `dtd::formato` contra la tabla común `tests/schemas/formato.json`
//! (la misma que prueba la web) y contra los casos de bytes que no caben en
//! una cadena JSON (BOM incompleto).

use dtd::{formato, Formato};

const TABLA: &str = include_str!("../../../tests/schemas/formato.json");

#[test]
fn la_tabla_comun() {
    let v: serde_json::Value = serde_json::from_str(TABLA).unwrap();
    let casos = v["casos"].as_array().unwrap();
    assert!(casos.len() >= 9, "la tabla de E1 tiene nueve filas");
    for c in casos {
        let texto = c["texto"].as_str().unwrap();
        let esperado = match c["formato"].as_str().unwrap() {
            "json" => Formato::Json,
            "xsd" => Formato::Xsd,
            "dtd" => Formato::Dtd,
            otro => panic!("formato desconocido {otro}"),
        };
        assert_eq!(formato(texto.as_bytes()), esperado, "{texto:?}");
    }
}

#[test]
fn un_bom_incompleto_no_es_un_bom() {
    assert_eq!(formato(b"\xEF\xBB<x/>"), Formato::Json);
    assert_eq!(formato(b"\xEF\xBB<!ELEMENT a EMPTY>"), Formato::Json);
}

#[test]
fn solo_mira_el_prologo() {
    // Un comentario enorme se recorre una vez; lo que sigue decide.
    let mut s = b"<!--".to_vec();
    s.extend(std::iter::repeat_n(b'x', 1 << 20));
    s.extend(b"--><!ELEMENT a EMPTY>");
    assert_eq!(formato(&s), Formato::Dtd);
}
