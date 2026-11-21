//! Pruebas de F7.5 (D81): ZIP del código generado, `POST /api/code`,
//! `POST /api/validate` y el protocolo de `WS /ws/validate`. Sin red ni
//! tiempos (D1); las que necesitan Docker van con `#[ignore]`
//! (`scripts/test-sandbox.ps1`).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use server::zip::{crc32, zip};
use server::{router, Config};

const ESQUEMA: &str =
    r#"{"type":"object","properties":{"a":{"type":"integer","maximum":10}},"required":["a"]}"#;

/// Lee un ZIP *stored*: (nombre, datos) de cada entrada local, comprobando
/// el CRC y el directorio central.
fn leer_zip(z: &[u8]) -> Vec<(String, Vec<u8>)> {
    let u16le = |i: usize| u16::from_le_bytes([z[i], z[i + 1]]) as usize;
    let u32le = |i: usize| u32::from_le_bytes([z[i], z[i + 1], z[i + 2], z[i + 3]]);
    let mut v = Vec::new();
    let mut i = 0;
    while u32le(i) == 0x0403_4B50 {
        assert_eq!(u16le(i + 8), 0, "sin comprimir");
        let crc = u32le(i + 14);
        let tam = u32le(i + 18) as usize;
        let n = u16le(i + 26);
        let extra = u16le(i + 28);
        let nombre = String::from_utf8(z[i + 30..i + 30 + n].to_vec()).unwrap();
        let ini = i + 30 + n + extra;
        let datos = z[ini..ini + tam].to_vec();
        assert_eq!(crc32(&datos), crc, "{nombre}");
        v.push((nombre, datos));
        i = ini + tam;
    }
    // Fin del directorio central: tantas entradas como archivos locales.
    let fin = z.len() - 22;
    assert_eq!(u32le(fin), 0x0605_4B50);
    assert_eq!(u16le(fin + 10), v.len());
    assert_eq!(
        u32le(fin + 16) as usize,
        i,
        "el directorio central empieza tras los datos"
    );
    v
}

#[test]
fn crc32_conocido() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn zip_ida_y_vuelta_y_reproducible() {
    let archivos: &[(&str, &[u8])] = &[
        ("main.rs", b"fn main() {}\n"),
        ("vacio.h", b""),
        ("LEEME.txt", "ñ\n".as_bytes()),
    ];
    let z = zip(archivos);
    assert_eq!(z, zip(archivos), "mismos archivos, mismos bytes");
    let l = leer_zip(&z);
    assert_eq!(l.len(), 3);
    for ((n, d), (m, e)) in archivos.iter().zip(&l) {
        assert_eq!(n, m);
        assert_eq!(*d, e.as_slice());
    }
}

async fn pedir(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let r = app.clone().oneshot(req).await.unwrap();
    let (e, h) = (r.status(), r.headers().clone());
    let b = axum::body::to_bytes(r.into_body(), usize::MAX)
        .await
        .unwrap();
    (e, h, b.to_vec())
}

fn post(ruta: &str, cuerpo: impl Into<Body>) -> Request<Body> {
    Request::post(ruta).body(cuerpo.into()).unwrap()
}

#[tokio::test]
async fn code_da_un_zip_con_el_validador() {
    let app = router(Config::default());
    for (lang, principal) in [("rust", "main.rs"), ("c", "main.c")] {
        let (e, h, z) = pedir(&app, post(&format!("/api/code?lang={lang}"), ESQUEMA)).await;
        assert_eq!(e, StatusCode::OK, "{lang}: {}", String::from_utf8_lossy(&z));
        assert_eq!(h.get("content-type").unwrap(), "application/zip");
        let cd = h.get("content-disposition").unwrap().to_str().unwrap();
        assert_eq!(
            cd,
            format!("attachment; filename=\"validador-json-{lang}.zip\"")
        );
        let l = leer_zip(&z);
        let nombres: Vec<&str> = l.iter().map(|(n, _)| n.as_str()).collect();
        assert!(nombres.contains(&principal), "{nombres:?}");
        assert!(nombres.contains(&"LEEME.txt"));
        // Todos los nombres los acepta el sandbox (F7.3).
        for n in nombres.iter().filter(|n| **n != "LEEME.txt") {
            let (base, ext) = n.rsplit_once('.').unwrap();
            assert!(
                base.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{n}"
            );
            assert!(["rs", "c", "h"].contains(&ext), "{n}");
        }
        // Mismo esquema, mismo ZIP.
        let (_, _, z2) = pedir(&app, post(&format!("/api/code?lang={lang}"), ESQUEMA)).await;
        assert_eq!(z, z2);
    }
}

#[tokio::test]
async fn code_errores() {
    let app = router(Config::default());
    let (e, _, b) = pedir(&app, post("/api/code?lang=go", ESQUEMA)).await;
    assert_eq!(e, StatusCode::BAD_REQUEST);
    assert_eq!(
        serde_json::from_slice::<Value>(&b).unwrap()["error"],
        "request"
    );
    let (e, _, b) = pedir(&app, post("/api/code?lang=c", r#"{"type": 3}"#)).await;
    assert_eq!(e, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        serde_json::from_slice::<Value>(&b).unwrap()["error"],
        "schema"
    );
}

#[tokio::test]
async fn validate_sin_sandbox_da_503_y_cuerpo_malo_400() {
    let app = router(Config::default());
    let ok = json!({"schema": ESQUEMA, "payload": "{\"a\": 1}", "lang": "c"}).to_string();
    let (e, h, b) = pedir(&app, post("/api/validate", ok)).await;
    assert_eq!(
        e,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        String::from_utf8_lossy(&b)
    );
    assert!(h.get("retry-after").is_some());
    assert_eq!(
        serde_json::from_slice::<Value>(&b).unwrap()["error"],
        "no-sandbox"
    );
    for malo in [
        "no json",
        r#"{"schema": "{}"}"#,
        r#"{"schema": "{}", "payload": "1", "lang": "go"}"#,
    ] {
        let (e, _, _) = pedir(&app, post("/api/validate", malo)).await;
        assert_eq!(e, StatusCode::BAD_REQUEST, "{malo}");
    }
}

// --- Protocolo de WS /ws/validate (sin red: `flujo::conversar`).

#[test]
fn flujo_valido_por_bloques() {
    let p = br#"{"a": 7, "b": [1, 2, 3]}"#;
    let bloques: Vec<&[u8]> = p.chunks(5).collect();
    let m = server::flujo::conversar(ESQUEMA, &bloques);
    assert_eq!(m[0]["type"], "ready");
    assert_eq!(m[0]["format"], "json");
    assert!(m[0]["memoryBytes"].as_u64().unwrap() > 0);
    assert_eq!(m.len(), 1 + bloques.len() + 1);
    for (i, x) in m[1..=bloques.len()].iter().enumerate() {
        assert_eq!(x["type"], "progress");
        assert_eq!(x["chunk"], (i + 1) as u64);
        assert_eq!(x["stopped"], false);
    }
    let r = m.last().unwrap();
    assert_eq!(r["type"], "report");
    assert_eq!(r["bytes"], p.len() as u64);
    assert_eq!(r["report"]["valid"], true);
    assert_eq!(r["report"]["violations"], 0);
}

#[test]
fn flujo_da_lo_mismo_con_cualquier_bloque() {
    // Mismo reporte en bloques de 1, 3 y todo junto (invariante del streaming).
    let p = br#"{"a": 30, "c": "x"}"#;
    let informe = |n: usize| {
        let b: Vec<&[u8]> = p.chunks(n).collect();
        server::flujo::conversar(ESQUEMA, &b).last().unwrap()["report"].clone()
    };
    let r = informe(p.len());
    assert_eq!(r["valid"], false);
    assert_eq!(r["violations"], 1);
    assert_eq!(r["errorKind"], "schema");
    assert_eq!(informe(1), r);
    assert_eq!(informe(3), r);
}

#[test]
fn flujo_malformado_se_detiene() {
    let bloques: [&[u8]; 3] = [b"{\"a\": 01", b", \"b\": 2", b"}"];
    let m = server::flujo::conversar(ESQUEMA, &bloques);
    assert_eq!(
        m[1]["stopped"], true,
        "el error léxico detiene el validador"
    );
    assert_eq!(m[3]["stopped"], true);
    let r = &m.last().unwrap()["report"];
    assert_eq!(r["errorKind"], "lexical");
    assert_eq!(r["violations"], Value::Null);
    assert_eq!(r["offset"], 7);
}

#[test]
fn flujo_con_esquema_malo_da_error() {
    let m = server::flujo::conversar(r#"{"type": 3}"#, &[b"1"]);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0]["type"], "error");
    assert!(m[0]["message"].as_str().unwrap().contains("no compila"));
}

#[test]
fn flujo_xml() {
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="a" type="xs:integer"/></xs:schema>"#;
    let m = server::flujo::conversar(xsd, &[b"<a>4", b"2</a>"]);
    assert_eq!(m[0]["format"], "xml");
    assert_eq!(m.last().unwrap()["report"]["valid"], true);
}

// --- Con Docker (`scripts/test-sandbox.ps1`).

#[tokio::test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
async fn validate_compila_y_ejecuta_en_el_sandbox() {
    let app = server::router_completo(
        Config::default(),
        None,
        Some(server::sandbox::Sandbox::default()),
    );
    for lang in ["c", "rust"] {
        for (payload, valido) in [
            ("{\"a\": 1}", true),
            ("{\"a\": 30}", false),
            ("{\"a\": 01}", false),
        ] {
            let cuerpo = json!({"schema": ESQUEMA, "payload": payload, "lang": lang}).to_string();
            let (e, _, b) = pedir(&app, post("/api/validate", cuerpo)).await;
            assert_eq!(e, StatusCode::OK, "{lang}: {}", String::from_utf8_lossy(&b));
            let v: Value = serde_json::from_slice(&b).unwrap();
            assert_eq!(v["status"], "ok", "{v}");
            assert_eq!(v["agree"], true, "{lang} {payload}: {v}");
            assert_eq!(v["generated"]["valid"], valido);
            let codigo = if valido {
                0
            } else if payload.contains("01") {
                2
            } else {
                1
            };
            assert_eq!(v["exitCode"], codigo, "{v}");
        }
    }
}

// --- E3 (`docs/fase7/pendientes.md` §17-§25).

/// E3C-3: el presupuesto de respuestas grandes se suelta al terminar de
/// enviar el cuerpo. 300 ZIP leídos enteros (≈ 135 KB cada uno, más de lo que
/// cabe en 48 MiB a la vez) dan siempre 200.
#[tokio::test]
async fn presupuesto_de_respuestas_se_suelta_al_leer() {
    let app = router(Config::default());
    for i in 0..300 {
        let (e, _, z) = pedir(&app, post("/api/code?lang=c", ESQUEMA)).await;
        assert_eq!(e, StatusCode::OK, "petición {i}");
        assert!(z.len() > 64 << 10);
    }
}

/// E3C-3: si el cliente no lee, las respuestas retienen su reserva hasta
/// agotar el presupuesto (503 `busy`); al soltarlas, vuelve a haber sitio.
#[tokio::test]
async fn presupuesto_de_respuestas_se_agota_sin_leer() {
    let app = router(Config::default());
    let mut retenidas = Vec::new();
    let mut ocupado = false;
    for _ in 0..600 {
        let r = app
            .clone()
            .oneshot(post("/api/code?lang=c", ESQUEMA))
            .await
            .unwrap();
        if r.status() == StatusCode::SERVICE_UNAVAILABLE {
            let b = axum::body::to_bytes(r.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&b).unwrap()["error"],
                "busy"
            );
            ocupado = true;
            break;
        }
        assert_eq!(r.status(), StatusCode::OK);
        retenidas.push(r); // cuerpo sin leer: la reserva sigue tomada
    }
    assert!(ocupado, "nunca se agotó el presupuesto");
    assert!(retenidas.len() < 600);
    drop(retenidas);
    let (e, _, _) = pedir(&app, post("/api/code?lang=c", ESQUEMA)).await;
    assert_eq!(
        e,
        StatusCode::OK,
        "al soltar las respuestas vuelve a haber presupuesto"
    );
}

#[test]
fn errores_del_flujo_con_codigo() {
    let m = server::flujo::conversar(r#"{"type": 3}"#, &[b"1"]);
    assert_eq!(m[0]["code"], "schema");
}
