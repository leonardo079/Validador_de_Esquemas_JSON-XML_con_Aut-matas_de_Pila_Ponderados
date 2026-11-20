//! Pruebas de las vistas de F7.6 (D70): `POST /api/automaton`,
//! `POST /api/derivation` y `POST /api/compare`, con `oneshot` (sin red) y
//! sin aserciones de tiempo (D1).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use server::{router, Config};

async fn pedir(app: &Router, ruta: &str, cuerpo: impl Into<Body>) -> (StatusCode, Value) {
    let req = Request::post(ruta).body(cuerpo.into()).unwrap();
    let r = app.clone().oneshot(req).await.unwrap();
    let estado = r.status();
    assert_eq!(r.headers().get("content-type").unwrap(), "application/json");
    assert_eq!(
        r.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    let b = axum::body::to_bytes(r.into_body(), usize::MAX)
        .await
        .unwrap();
    let v = serde_json::from_slice(&b)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&b)));
    (estado, v)
}

fn app() -> Router {
    router(Config::default())
}

const OBJETO: &str = r#"{"type":"object","properties":{"a":{"type":"integer","maximum":10}},"required":["a"],"additionalProperties":false}"#;

#[tokio::test]
async fn grafo_del_automata() {
    let (e, v) = pedir(&app(), "/api/automaton", OBJETO).await;
    assert_eq!(e, StatusCode::OK, "{v}");
    assert_eq!(v["format"], "json");
    assert_eq!(v["truncated"], false);
    let nodos = v["nodes"].as_array().unwrap();
    let aristas = v["edges"].as_array().unwrap();
    // Todos los estados de M (más los «↩» especiales).
    let estados = nodos.iter().filter(|n| n["special"] == false).count();
    assert_eq!(estados as u64, v["states"].as_u64().unwrap());
    assert_eq!(nodos.iter().filter(|n| n["accept"] == true).count(), 1);
    assert!(nodos.iter().any(|n| n["id"] == v["start"]));
    // Toda arista une nodos enviados y lleva su etiqueta.
    for a in aristas {
        for k in ["source", "target"] {
            assert!(nodos.iter().any(|n| n["id"] == a[k]), "{a}");
        }
        assert!(!a["label"].as_str().unwrap().is_empty(), "{a}");
    }
    assert!(!v["groups"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn grafo_xsd_grande_se_recorta() {
    let xsd = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/schemas/xsd/parse-ok/pedido.xsd"),
    )
    .unwrap();
    let (e, v) = pedir(&app(), "/api/automaton", xsd).await;
    assert_eq!(e, StatusCode::OK, "{v}");
    assert_eq!(v["format"], "xml");
    assert!(v["nodes"].as_array().unwrap().len() <= server::vistas::MAX_NODOS + 2);
    assert!(v["edges"].as_array().unwrap().len() <= server::vistas::MAX_ARISTAS);
}

#[tokio::test]
async fn grafo_de_un_esquema_invalido_da_422() {
    let (e, v) = pedir(&app(), "/api/automaton", r#"{"type": 3}"#).await;
    assert_eq!(e, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["error"], "schema");
}

#[tokio::test]
async fn arbol_del_payload() {
    let (e, v) = pedir(&app(), "/api/derivation", r#"{"a": [1, true]}"#).await;
    assert_eq!(e, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "value");
    assert_eq!(v["pointer"], "");
    assert_eq!(v["children"][0]["name"], "object");
    let (e, v) = pedir(&app(), "/api/derivation", "<a><b/></a>").await;
    assert_eq!(e, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "document");
}

#[tokio::test]
async fn arbol_de_un_payload_malformado_da_422_con_offset() {
    let (e, v) = pedir(&app(), "/api/derivation", "[1,]").await;
    assert_eq!(e, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        v,
        json!({"error": "derivation", "kind": "syntax", "offset": 3, "message": v["message"]})
    );
    let (_, v) = pedir(&app(), "/api/derivation", "[01]").await;
    assert_eq!(
        (v["kind"].as_str(), v["offset"].as_u64()),
        (Some("lexical"), Some(2))
    );
}

#[tokio::test]
async fn comparar_valido_e_invalido() {
    let cuerpo = |p: &str| json!({"schema": OBJETO, "payload": p}).to_string();
    let (e, v) = pedir(&app(), "/api/compare", cuerpo(r#"{"a": 3}"#)).await;
    assert_eq!(e, StatusCode::OK, "{v}");
    assert_eq!(v["dpda"]["valid"], true);
    assert_eq!(v["baseline"]["valid"], true);
    assert_eq!(v["agree"], true);
    assert!(v["dpda"]["memoryBytes"].as_u64().unwrap() > 0);

    let (_, v) = pedir(&app(), "/api/compare", cuerpo(r#"{"a": 30, "b": 1}"#)).await;
    assert_eq!(v["dpda"]["valid"], false);
    assert_eq!(v["dpda"]["violations"], 2);
    assert_eq!(v["dpda"]["errorKind"], "schema");
    assert!(v["dpda"]["rule"].is_string());
    assert_eq!(v["baseline"]["violations"], 2);
    assert_eq!(v["agree"], true);

    let (_, v) = pedir(&app(), "/api/compare", cuerpo("[1,")).await;
    assert_eq!(v["dpda"]["errorKind"], "syntax");
    assert_eq!(v["dpda"]["violations"], Value::Null);
    assert_eq!(v["baseline"]["kind"], "malformed");
}

#[tokio::test]
async fn comparar_xsd_sin_linea_base() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="a" type="xs:integer"/></xs:schema>"#;
    let (e, v) = pedir(
        &app(),
        "/api/compare",
        json!({"schema": xsd, "payload": "<a>7</a>"}).to_string(),
    )
    .await;
    assert_eq!(e, StatusCode::OK, "{v}");
    assert_eq!(v["format"], "xml");
    assert_eq!(v["dpda"]["valid"], true);
    assert!(v["baseline"]["error"].is_string());
    assert_eq!(v["agree"], Value::Null);
}

#[tokio::test]
async fn comparar_cuerpo_mal_formado_da_400() {
    for c in [
        "no es json",
        r#"{"schema": "{}"}"#,
        r#"{"schema": 1, "payload": "1"}"#,
    ] {
        let (e, v) = pedir(&app(), "/api/compare", c).await;
        assert_eq!(e, StatusCode::BAD_REQUEST, "{c}");
        assert_eq!(v["error"], "request");
    }
}

#[tokio::test]
async fn las_vistas_tienen_el_tope_de_subida() {
    let cfg = Config {
        max_upload_bytes: 100,
        ..Config::default()
    };
    let app = router(cfg);
    for ruta in ["/api/automaton", "/api/derivation", "/api/compare"] {
        let (e, v) = pedir(&app, ruta, "x".repeat(101)).await;
        assert_eq!(e, StatusCode::PAYLOAD_TOO_LARGE, "{ruta}");
        assert_eq!(v["error"], "too-large");
    }
}

#[tokio::test]
async fn get_en_una_vista_da_405() {
    let req = Request::get("/api/automaton").body(Body::empty()).unwrap();
    let r = app().oneshot(req).await.unwrap();
    assert_eq!(r.status(), StatusCode::METHOD_NOT_ALLOWED);
}
