//! Pruebas del modelo de documento (F7.2, D72), sin red (D1): todo caso del
//! corpus da un documento BSON y vuelve igual al leerlo.

use std::path::Path;

use atlas::{a_bson, a_json, base64, de_base64, documento, leer};
use baseline::corpus::{self, Payload};
use serde_json::json;

fn raiz() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn todo_el_corpus_ida_y_vuelta() {
    let casos = corpus::casos().unwrap();
    assert!(casos.len() >= 70, "{}", casos.len());
    for c in &casos {
        let v = documento(c, &raiz()).unwrap_or_else(|e| panic!("{e}"));
        let d = a_bson(&v).unwrap();
        let g = leer(&a_json(d)).unwrap_or_else(|e| panic!("{}: {e}", c.id));
        assert_eq!(g.id, c.id);
        assert_eq!(g.category, c.category);
        assert_eq!(g.schema.as_bytes(), c.schema_text.as_slice(), "{}", c.id);
        match &c.payload {
            Payload::Bytes(b) => assert_eq!(g.payload.as_deref(), Some(b.as_slice()), "{}", c.id),
            Payload::Generator { .. } => assert_eq!(g.payload, None),
        }
        assert_eq!(g.expected["valid"], c.expected.valid);
        assert_eq!(
            g.expected.get("violations").and_then(|v| v.as_u64()),
            c.expected.violations,
            "{}",
            c.id
        );
        assert_eq!(
            g.expected.get("offset").and_then(|v| v.as_u64()),
            c.expected.offset,
            "{}",
            c.id
        );
        assert!(!g.description.is_empty(), "{}", c.id);
        assert!(
            v["source"].as_str().unwrap().starts_with("tests/cases/"),
            "{}",
            v["source"]
        );
    }
}

#[test]
fn payload_no_utf8_va_en_base64() {
    let casos = corpus::casos().unwrap();
    let c = casos
        .iter()
        .find(|c| c.id == "malformed-json-invalid-utf8-01")
        .unwrap();
    let v = documento(c, &raiz()).unwrap();
    assert_eq!(v["payload"]["kind"], "base64");
    assert_eq!(v["payload"]["value"], "eyJuIjogIv8ifQ==");
}

#[test]
fn base64_ida_y_vuelta() {
    for n in 0..300usize {
        let b: Vec<u8> = (0..n).map(|i| (i * 37 + n) as u8).collect();
        assert_eq!(de_base64(&base64(&b)).as_deref(), Some(b.as_slice()), "{n}");
    }
    for malo in ["abc", "ab=c", "a===", "????"] {
        assert_eq!(de_base64(malo), None, "{malo}");
    }
}

#[test]
fn leer_rechaza_documentos_incompletos() {
    let bueno = json!({
        "_id": "x", "category": "valid", "format": "json", "schema": "true",
        "payload": {"kind": "text", "value": "1"}, "expected": {"valid": true}
    });
    assert!(leer(&bueno).is_ok());
    for (campo, valor) in [
        ("schema", json!(null)),
        ("payload", json!({"kind": "raro"})),
        ("payload", json!({"kind": "base64", "value": "!!"})),
        ("expected", json!({})),
    ] {
        let mut d = bueno.clone();
        d[campo] = valor;
        assert!(leer(&d).is_err(), "{campo}");
    }
}

#[test]
fn uri_del_entorno_o_del_env() {
    let dir = std::env::temp_dir().join(format!("atlas-prueba-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".env"), "OTRA=1\nMONGODB_URI=\"mongodb://h/x\"\n").unwrap();
    if std::env::var("MONGODB_URI").is_err() {
        assert_eq!(
            atlas::uri_de_entorno(&dir).as_deref(),
            Some("mongodb://h/x")
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        atlas::uri_de_entorno(&dir.join("no-existe")).is_some(),
        std::env::var("MONGODB_URI").is_ok()
    );
}
