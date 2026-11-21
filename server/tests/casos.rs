//! Pruebas de F7.2 en el servidor (D72), sin red (D1): sin base de datos las
//! rutas dan 503, y la ejecución de los casos (pura) sobre el corpus entero da
//! lo mismo que las pruebas de corpus.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use server::{router, Config};

#[tokio::test]
async fn sin_base_las_rutas_dan_503() {
    for (metodo, ruta) in [
        ("GET", "/api/tests"),
        ("POST", "/api/tests/run"),
        ("GET", "/api/tests?category=valid"),
    ] {
        let req = Request::builder()
            .method(metodo)
            .uri(ruta)
            .body(Body::empty())
            .unwrap();
        let r = router(Config::default()).oneshot(req).await.unwrap();
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE, "{ruta}");
        assert!(r.headers().get("retry-after").is_some());
        let b = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: Value = serde_json::from_slice(&b).unwrap();
        assert_eq!(v["error"], "no-database");
    }
}

#[tokio::test]
async fn categoria_no_valida_da_400() {
    for q in [
        "category=",
        "category=VALID",
        "category=a%20b",
        "category=../x",
    ] {
        let req = Request::get(format!("/api/tests?{q}"))
            .body(Body::empty())
            .unwrap();
        let r = router(Config::default()).oneshot(req).await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{q}");
    }
}

/// El mismo camino que `POST /api/tests/run`, sin Atlas: el corpus pasa por el
/// documento de `atlas` y se ejecuta. Todos pasan o se saltan (generadores y
/// `non-dpda` pendientes); ninguno falla.
#[test]
fn ejecutar_el_corpus_desde_sus_documentos() {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let casos = baseline::corpus::casos().unwrap();
    let lista: Vec<_> = casos
        .iter()
        .map(|c| {
            let v = atlas::documento(c, &raiz).unwrap();
            let v = atlas::a_json(atlas::a_bson(&v).unwrap());
            atlas::leer(&v).map_err(|e| (c.id.clone(), e))
        })
        .collect();
    let r = std::thread::Builder::new()
        .stack_size(server::STACK_SIZE)
        .spawn(move || server::casos::ejecutar(&lista))
        .unwrap()
        .join()
        .unwrap();
    let fallidos: Vec<&Value> = r["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["status"] == "failed" || x["status"] == "error")
        .collect();
    assert!(fallidos.is_empty(), "{fallidos:#?}");
    assert_eq!(r["total"].as_u64().unwrap() as usize, casos.len());
    assert!(r["passed"].as_u64().unwrap() >= 60, "{}", r["passed"]);
    // Pasan o se saltan (con su motivo); hoy todos traen bytes y compilan.
    assert_eq!(
        r["passed"].as_u64().unwrap() + r["skipped"].as_u64().unwrap(),
        casos.len() as u64
    );
    for x in r["results"].as_array().unwrap() {
        if x["status"] == "skipped" {
            assert!(x["reason"].is_string(), "{x}");
        } else {
            assert!(x["got"]["valid"].is_boolean(), "{x}");
        }
    }
}

/// Un caso cuyo `expected` no coincide da `failed` con los campos distintos.
#[test]
fn un_expected_falso_da_failed() {
    let c = atlas::Guardado {
        id: "invalid-falso-01".into(),
        category: "invalid".into(),
        format: "json".into(),
        description: "x".into(),
        schema: r#"{"type": "integer", "maximum": 3}"#.into(),
        payload: Some(b"2".to_vec()),
        expected: serde_json::json!({"valid": false, "violations": 1, "errorKind": "schema"}),
    };
    let r = std::thread::Builder::new()
        .stack_size(server::STACK_SIZE)
        .spawn(move || {
            server::casos::ejecutar(&[Ok(c), Err(("roto-01".into(), "falta `schema`".into()))])
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(
        (r["failed"].as_u64(), r["errors"].as_u64()),
        (Some(1), Some(1))
    );
    let f = &r["results"][0];
    assert_eq!(f["status"], "failed");
    assert_eq!(
        f["mismatch"],
        serde_json::json!(["valid", "errorKind", "violations"])
    );
    assert_eq!(f["got"]["valid"], true);
}
