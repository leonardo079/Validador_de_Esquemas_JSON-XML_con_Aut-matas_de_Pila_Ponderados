//! Pruebas de la API de F7.1 (`docs/fase7/servidor.md`) llamando al `Router`
//! con `tower::ServiceExt::oneshot`: sin red ni puertos, y ninguna aserción
//! sobre el tiempo de pared (D1). Las compilaciones que hacen pánico o se
//! bloquean se inyectan con `router_con` (B1).

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use http_body::Frame;
use serde_json::Value;
use tower::ServiceExt;

use server::compilar::{self, Compilado};
use server::{router, router_con, Compilador, Config};

/// Hace una petición y devuelve el estado, las cabeceras y el cuerpo.
async fn pedir_con_cabeceras(app: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, String) {
    let r = app.clone().oneshot(req).await.unwrap();
    let (estado, cabeceras) = (r.status(), r.headers().clone());
    let b = axum::body::to_bytes(r.into_body(), usize::MAX)
        .await
        .unwrap();
    (estado, cabeceras, String::from_utf8(b.to_vec()).unwrap())
}

/// Hace una petición y devuelve el estado y el cuerpo en texto.
async fn pedir(app: &Router, req: Request<Body>) -> (StatusCode, String) {
    let (estado, cabeceras, cuerpo) = pedir_con_cabeceras(app, req).await;
    assert_eq!(
        cabeceras.get("content-type").unwrap(),
        "application/json",
        "{estado}: {cuerpo}"
    );
    // SEC-L2: en todas las respuestas del `Router`.
    assert_eq!(
        cabeceras.get("x-content-type-options").unwrap(),
        "nosniff",
        "{estado}: {cuerpo}"
    );
    (estado, cuerpo)
}

fn json(texto: &str) -> Value {
    serde_json::from_str(texto).unwrap_or_else(|e| panic!("{e}: {texto}"))
}

/// Ruta de un archivo de `tests/` del repositorio.
fn repo(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests")
        .join(rel)
}

fn post(cuerpo: impl Into<Body>) -> Request<Body> {
    Request::post("/api/schemas").body(cuerpo.into()).unwrap()
}

/// SHA-256 en hexadecimal (U5), calculado aparte en la prueba.
fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn health_da_ok_y_la_version() {
    let app = router(Config::default());
    let (estado, cuerpo) = pedir(
        &app,
        Request::get("/api/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(estado, StatusCode::OK);
    let v = json(&cuerpo);
    assert_eq!(v["status"], "ok");
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    // E3 §25.4: sin Atlas ni sandbox lo anuncia.
    assert_eq!(v["database"], false);
    assert_eq!(v["sandbox"], false);
}

#[tokio::test]
async fn ruta_o_metodo_desconocidos_dan_json() {
    let app = router(Config::default());
    let (estado, cuerpo) = pedir(&app, Request::get("/api/x").body(Body::empty()).unwrap()).await;
    assert_eq!(estado, StatusCode::NOT_FOUND);
    assert_eq!(json(&cuerpo)["error"], "not-found");
    let (estado, cuerpo) = pedir(
        &app,
        Request::get("/api/schemas").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(estado, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(json(&cuerpo)["error"], "method-not-allowed");
    // CR-S4: el 405 dice qué métodos admite la ruta (RFC 9110 §15.5.6).
    let (estado, cabeceras, _) = pedir_con_cabeceras(
        &app,
        Request::get("/api/schemas").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(estado, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(cabeceras.get("allow").unwrap(), "POST");
    let (_, cabeceras, _) = pedir_con_cabeceras(
        &app,
        Request::post("/api/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert!(
        cabeceras
            .get("allow")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("GET"),
        "{cabeceras:?}"
    );
}

/// CR2-S1: un `summary` con recuentos distintos de cero, para que se note
/// si los argumentos del resumen cambian de orden (el texto de
/// `crates/cli/tests/check.rs` para el mismo archivo).
#[tokio::test]
async fn summary_con_defs_y_referencias() {
    let esquema = std::fs::read(repo("schemas/parse-ok/arbol-recursivo.json")).unwrap();
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::OK, "{cuerpo}");
    assert_eq!(
        json(&cuerpo)["summary"],
        "esquema válido (subconjunto de JSON Schema 2020-12); \
         6 subesquemas, 1 entradas de $defs, 2 referencias resueltas"
    );
}

/// JSON Schema válido → 200 sin `Content-Type`, con el contrato de U1; la
/// IR es la de `ir::write_json` byte a byte (la dorada de `tests/ir`), al
/// final del cuerpo, y el `summary` es el texto de `schemac check`.
#[tokio::test]
async fn json_valido_da_200_con_la_ir_dorada() {
    let esquema = std::fs::read(repo("schemas/parse-ok/usuarios.json")).unwrap();
    let dorada = std::fs::read_to_string(repo("ir/usuarios.json")).unwrap();
    let app = router(Config::default());
    let (estado, cuerpo) = pedir(&app, post(esquema.clone())).await;
    assert_eq!(estado, StatusCode::OK, "{cuerpo}");
    assert!(
        cuerpo.ends_with(&format!("\"ir\":{dorada}}}")),
        "la IR no es la dorada: {cuerpo}"
    );
    let v = json(&cuerpo);
    assert_eq!(v["apiVersion"], 1);
    assert_eq!(v["id"], sha256(&esquema));
    assert_eq!(v["format"], "json");
    // El texto de `schemac check tests/schemas/parse-ok/usuarios.json`.
    assert_eq!(
        v["summary"],
        "esquema válido (subconjunto de JSON Schema 2020-12); \
         7 subesquemas, 0 entradas de $defs, 0 referencias resueltas"
    );
    assert_eq!(v["irBytes"], dorada.len());
    assert_eq!(v["irOmitted"], false);
    assert!(
        ["ok", "pending", "limit"].contains(&v["automaton"]["status"].as_str().unwrap()),
        "{}",
        v["automaton"]
    );
}

/// El resumen del autómata se incrusta tal cual (F4-U7): el de la dorada
/// de `tests/automata`.
#[tokio::test]
async fn automata_construible_da_su_resumen() {
    let esquema = std::fs::read(repo("schemas/parse-ok/objeto-cerrado.json")).unwrap();
    let dorada = std::fs::read_to_string(repo("automata/objeto-cerrado.json")).unwrap();
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::OK, "{cuerpo}");
    let v = json(&cuerpo);
    assert_eq!(v["automaton"]["status"], "ok");
    assert_eq!(v["automaton"]["summary"], json(&dorada));
}

/// Una regla que el constructor no compila (un `const` compuesto, F4-U5 y
/// recorte 2 de F4.5b; antes `pattern` y `uniqueItems`) no es un error del
/// esquema: 200 con `automaton.status = "pending"`.
#[tokio::test]
async fn regla_pendiente_da_200_con_automata_pendiente() {
    let esquema = r#"{"const": {"a": 1}}"#;
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::OK, "{cuerpo}");
    assert_eq!(json(&cuerpo)["automaton"]["status"], "pending");
}

/// XSD válido → 200 con `format: "xml"`, su IR (la dorada de
/// `tests/ir/xsd`) y, desde F4.1c (D57), su autómata construido.
#[tokio::test]
async fn xsd_valido_da_200_con_automata() {
    let esquema = std::fs::read(repo("schemas/xsd/parse-ok/pedido.xsd")).unwrap();
    let dorada = std::fs::read_to_string(repo("ir/xsd/pedido.json")).unwrap();
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::OK, "{cuerpo}");
    assert!(cuerpo.ends_with(&format!("\"ir\":{dorada}}}")), "{cuerpo}");
    let v = json(&cuerpo);
    assert_eq!(v["format"], "xml");
    assert!(v["summary"]
        .as_str()
        .unwrap()
        .starts_with("esquema válido (subconjunto de XSD 1.0); "));
    assert_eq!(v["automaton"]["status"], "ok", "{cuerpo}");
    assert_eq!(v["automaton"]["summary"]["format"], "xml");
    assert_eq!(v["automaton"]["summary"]["deferred"], 0);
}

/// Esquema inválido → 422 con los errores de la primera etapa que falla:
/// con ≥ 150 errores, los 100 primeros y el total, y el tamaño de la
/// respuesta acotado (`docs/fase3/pendientes.md` §1).
#[tokio::test]
async fn esquema_invalido_da_422_con_los_100_primeros() {
    let esquema = format!("{{\"type\": [{}]}}", vec!["\"\""; 150].join(","));
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY);
    let v = json(&cuerpo);
    assert_eq!(v["error"], "schema");
    assert_eq!(v["format"], "json");
    assert_eq!(v["truncated"], true);
    assert!(v["total"].as_u64().unwrap() >= 150, "{}", v["total"]);
    let errores = v["errors"].as_array().unwrap();
    assert_eq!(errores.len(), 100);
    for e in errores {
        assert_eq!(e["line"], 1, "{e}");
        assert!(e["column"].as_u64().unwrap() > 0);
        assert!(e["code"].is_string());
        let m = e["message"].as_str().unwrap();
        assert!(!m.starts_with("línea"), "{m}");
    }
    // Cien mensajes de ≲ 200 bytes más los campos, sea cual sea el total.
    assert!(cuerpo.len() < 40_000, "{}", cuerpo.len());
}

/// El cuerpo del 422 no crece con el número de errores: con 20 000
/// elementos (≈ 40 000 errores) mide lo mismo que con 150.
#[tokio::test]
async fn respuesta_422_acotada_sea_cual_sea_el_total() {
    let esquema = format!("{{\"type\": [{}]}}", vec!["\"\""; 20_000].join(","));
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY);
    let v = json(&cuerpo);
    assert!(v["total"].as_u64().unwrap() >= 20_000);
    assert_eq!(v["errors"].as_array().unwrap().len(), 100);
    assert!(cuerpo.len() < 40_000, "{}", cuerpo.len());
}

/// Un solo error (Fase 2) no se marca truncado.
#[tokio::test]
async fn un_error_del_parser_no_esta_truncado() {
    let (estado, cuerpo) = pedir(&router(Config::default()), post("{\"type\": ")).await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY);
    let v = json(&cuerpo);
    assert_eq!(
        (v["total"].as_u64(), v["truncated"].as_bool()),
        (Some(1), Some(false))
    );
}

/// Los errores XSD salen igual, con `format: "xml"`.
#[tokio::test]
async fn xsd_invalido_da_422_con_format_xml() {
    let esquema = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
                   <xs:element name='a'/></xs:schema>";
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY, "{cuerpo}");
    let v = json(&cuerpo);
    assert_eq!(v["format"], "xml");
    assert_eq!(v["total"], 1);
}

/// Cuerpo vacío o sin UTF-8: es JSON con su error léxico o del parser
/// (422); ni pánico ni 400.
#[tokio::test]
async fn cuerpo_vacio_o_sin_utf8_da_422() {
    let app = router(Config::default());
    for cuerpo in [&b""[..], b"\xff\xfe", b"{\"\xc3\x28\": 1}"] {
        let (estado, texto) = pedir(&app, post(cuerpo.to_vec())).await;
        assert_eq!(
            estado,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{cuerpo:?}: {texto}"
        );
        let v = json(&texto);
        assert_eq!(v["format"], "json");
        assert!(
            ["lexical", "syntax"].contains(&v["errors"][0]["code"].as_str().unwrap()),
            "{texto}"
        );
    }
}

/// Tope de subida (1 MiB por defecto, U3): exactamente el tope pasa, un
/// byte más da 413 en JSON con el tope; también si lo anuncia
/// `Content-Length`.
#[tokio::test]
async fn cuerpo_de_tope_mas_uno_da_413() {
    let app = router(Config::default());
    let tope = 1 << 20;
    let (estado, _) = pedir(&app, post(vec![b' '; tope])).await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY);
    let (estado, cuerpo) = pedir(&app, post(vec![b' '; tope + 1])).await;
    assert_eq!(estado, StatusCode::PAYLOAD_TOO_LARGE);
    let v = json(&cuerpo);
    assert_eq!(v["error"], "too-large");
    assert_eq!(v["limit"], tope);
    let req = Request::post("/api/schemas")
        .header("content-length", tope + 1)
        .body(Body::from(vec![b' '; tope + 1]))
        .unwrap();
    let (estado, cuerpo) = pedir(&app, req).await;
    assert_eq!(estado, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(json(&cuerpo)["error"], "too-large");
}

/// El tope de subida es configurable.
#[tokio::test]
async fn tope_de_subida_configurable() {
    let app = router(Config {
        max_upload_bytes: 4,
        ..Config::default()
    });
    assert_eq!(pedir(&app, post("{}")).await.0, StatusCode::OK);
    let (estado, cuerpo) = pedir(&app, post("{   }")).await;
    assert_eq!(estado, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(json(&cuerpo)["limit"], 4);
}

/// Un cuerpo que no termina nunca (bloques de 1 KiB) y cuenta los bytes que
/// alguien le leyó: si el servidor lo leyera sin tope, la prueba no
/// terminaría; si lo lee con tope, el contador lo dice.
struct CuerpoSinFin(Arc<AtomicUsize>);

impl http_body::Body for CuerpoSinFin {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        const BLOQUE: [u8; 1024] = [b' '; 1024];
        self.0.fetch_add(BLOQUE.len(), Ordering::SeqCst);
        Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(&BLOQUE)))))
    }
}

fn sin_fin() -> (Body, Arc<AtomicUsize>) {
    let leidos = Arc::new(AtomicUsize::new(0));
    (Body::new(CuerpoSinFin(leidos.clone())), leidos)
}

/// [`pedir`] con una guarda de 60 s: si el servidor leyera el cuerpo sin fin
/// sin tope, la prueba falla en lugar de colgarse. No mide tiempos (D1): con
/// el código correcto responde enseguida, y la guarda solo convierte un
/// cuelgue en un fallo.
async fn pedir_con_guarda(app: &Router, req: Request<Body>) -> (StatusCode, String) {
    tokio::time::timeout(std::time::Duration::from_secs(60), pedir(app, req))
        .await
        .expect("el servidor se quedó leyendo el cuerpo")
}

/// Fallo 1 del Tester (ronda 0): un `Content-Length` mayor que el tope da
/// 413 en JSON **sin leer el cuerpo**, con un cuerpo que no termina o vacío,
/// y también con `Expect: 100-continue` (hyper solo manda `100 Continue` si
/// se lee el cuerpo, y aquí no se lee).
#[tokio::test]
async fn content_length_de_mas_da_413_sin_leer_el_cuerpo() {
    let app = router(Config::default());
    for n in [(1u64 << 20) + 1, 2_000_000, 99_999_999_999, u64::MAX] {
        for expect in [false, true] {
            let (cuerpo, leidos) = sin_fin();
            let mut req = Request::post("/api/schemas").header("content-length", n);
            if expect {
                req = req.header("expect", "100-continue");
            }
            let (estado, texto) = pedir_con_guarda(&app, req.body(cuerpo).unwrap()).await;
            assert_eq!(estado, StatusCode::PAYLOAD_TOO_LARGE, "{n}: {texto}");
            let v = json(&texto);
            assert_eq!(v["error"], "too-large");
            assert_eq!(v["limit"], 1 << 20);
            assert!(v["message"].is_string());
            assert_eq!(leidos.load(Ordering::SeqCst), 0, "{n}: leyó el cuerpo");
        }
        let req = Request::post("/api/schemas")
            .header("content-length", n)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            pedir_con_guarda(&app, req).await.0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}

/// Con varios `Content-Length`, basta que uno anuncie de más; uno que no es
/// un número no cuenta y decide el cuerpo (hyper ya rechaza esas peticiones
/// con su 400 antes del `Router`). Un `Content-Length` que miente por lo bajo
/// no salta el tope: se lee como mucho tope + 1 bytes de un cuerpo sin fin.
#[tokio::test]
async fn content_length_raro_o_que_miente() {
    let app = router(Config::default());
    let (cuerpo, leidos) = sin_fin();
    let req = Request::post("/api/schemas")
        .header("content-length", "2")
        .header("content-length", "5000000")
        .body(cuerpo)
        .unwrap();
    assert_eq!(
        pedir_con_guarda(&app, req).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(leidos.load(Ordering::SeqCst), 0);
    for raro in ["abc", "-1", "1e9", ""] {
        let req = Request::post("/api/schemas")
            .header("content-length", raro)
            .body(Body::from("{}"))
            .unwrap();
        let (estado, texto) = pedir_con_guarda(&app, req).await;
        assert_eq!(estado, StatusCode::OK, "{raro:?}: {texto}");
    }
    let (cuerpo, leidos) = sin_fin();
    let req = Request::post("/api/schemas")
        .header("content-length", "2")
        .body(cuerpo)
        .unwrap();
    let (estado, texto) = pedir_con_guarda(&app, req).await;
    assert_eq!(estado, StatusCode::PAYLOAD_TOO_LARGE, "{texto}");
    let n = leidos.load(Ordering::SeqCst);
    assert!(n > 1 << 20 && n <= (1 << 20) + 1024, "{n}");
}

/// Un cuerpo que entrega un bloque, avisa por `leyendo` de que alguien ya lo
/// está leyendo y después no llega nunca más (ni despierta a nadie): una
/// conexión que manda la subida a medias y se queda callada.
struct CuerpoColgado {
    aviso: Option<tokio::sync::mpsc::UnboundedSender<()>>,
}

impl http_body::Body for CuerpoColgado {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        match self.aviso.take() {
            Some(tx) => {
                let _ = tx.send(());
                Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b"{")))))
            }
            None => Poll::Pending,
        }
    }
}

fn colgado() -> (Body, tokio::sync::mpsc::UnboundedReceiver<()>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (Body::new(CuerpoColgado { aviso: Some(tx) }), rx)
}

/// SEC-H1: los cuerpos que se están leyendo tienen un presupuesto global.
/// Con un presupuesto de una sola subida (1 MiB), una lectura `chunked` a
/// medias lo reserva entero: mientras siga, otra subida da 503 `busy` sin
/// leer nada de su cuerpo (contador en 0) y una compilación pequeña también;
/// al cortarse la primera, el presupuesto vuelve y la siguiente da 200.
/// Determinista: la prueba espera el aviso de que la lectura empezó.
#[tokio::test]
async fn presupuesto_de_cuerpos_en_vuelo_da_503() {
    let app = router(Config {
        max_inflight_body_bytes: 1 << 20,
        // CR2-S2: que el plazo de lectura no venza mientras la prueba mira.
        body_read_timeout: std::time::Duration::from_millis(server::MAX_TIMEOUT_MS),
        ..Config::default()
    });
    let (cuerpo, mut leyendo) = colgado();
    let primera = tokio::spawn({
        let app = app.clone();
        async move { pedir(&app, post(cuerpo)).await }
    });
    leyendo.recv().await.unwrap();
    let (otro, leidos) = sin_fin();
    let (estado, texto) = pedir_con_guarda(&app, post(otro)).await;
    assert_eq!(estado, StatusCode::SERVICE_UNAVAILABLE, "{texto}");
    assert_eq!(json(&texto)["error"], "busy");
    assert_eq!(leidos.load(Ordering::SeqCst), 0, "leyó el cuerpo");
    let (estado, cabeceras, _) = pedir_con_cabeceras(&app, post("{}")).await;
    assert_eq!(estado, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(cabeceras.get("retry-after").unwrap(), "5");
    // Cortar la conexión suelta la reserva (el `JoinHandle` devuelve
    // `Cancelled` cuando la tarea ya se soltó).
    primera.abort();
    assert!(primera.await.unwrap_err().is_cancelled());
    assert_eq!(pedir_con_guarda(&app, post("{}")).await.0, StatusCode::OK);
}

/// La reserva es la del `Content-Length` (redondeada a bloques de 1 KiB),
/// no la del tope: con un presupuesto de 1 MiB y una lectura colgada que
/// anuncia 600 KiB, otra de 400 KiB cabe y una de 500 KiB no.
#[tokio::test]
async fn la_reserva_sigue_al_content_length() {
    let app = router(Config {
        max_inflight_body_bytes: 1 << 20,
        // CR2-S2: que el plazo de lectura no venza mientras la prueba mira.
        body_read_timeout: std::time::Duration::from_millis(server::MAX_TIMEOUT_MS),
        ..Config::default()
    });
    let (cuerpo, mut leyendo) = colgado();
    let req = Request::post("/api/schemas")
        .header("content-length", 600 << 10)
        .body(cuerpo)
        .unwrap();
    let primera = tokio::spawn({
        let app = app.clone();
        async move { pedir(&app, req).await }
    });
    leyendo.recv().await.unwrap();
    let cabe = Request::post("/api/schemas")
        .header("content-length", 400 << 10)
        .body(Body::from(vec![b' '; 400 << 10]))
        .unwrap();
    assert_eq!(
        pedir_con_guarda(&app, cabe).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let no_cabe = Request::post("/api/schemas")
        .header("content-length", 500 << 10)
        .body(Body::from(vec![b' '; 500 << 10]))
        .unwrap();
    let (estado, texto) = pedir_con_guarda(&app, no_cabe).await;
    assert_eq!(estado, StatusCode::SERVICE_UNAVAILABLE, "{texto}");
    primera.abort();
    let _ = primera.await;
}

/// SEC-H1: plazo de lectura del cuerpo. Con 50 ms y un cuerpo que no llega
/// nunca, 408 `body-timeout` en JSON; la guarda de 60 s solo convierte un
/// cuelgue en un fallo, sin medir tiempos (D1). Después, el presupuesto está
/// libre: una subida del tope entero (que necesita todo el presupuesto de
/// 1 MiB) se lee y da 422.
#[tokio::test]
async fn cuerpo_que_no_llega_da_408() {
    let app = router(Config {
        body_read_timeout: std::time::Duration::from_millis(50),
        max_inflight_body_bytes: 1 << 20,
        ..Config::default()
    });
    let (cuerpo, _leyendo) = colgado();
    let (estado, texto) = pedir_con_guarda(&app, post(cuerpo)).await;
    assert_eq!(estado, StatusCode::REQUEST_TIMEOUT, "{texto}");
    let v = json(&texto);
    assert_eq!(v["error"], "body-timeout");
    assert!(v["message"].is_string());
    let (estado, _) = pedir_con_guarda(&app, post(vec![b' '; 1 << 20])).await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Un cuerpo que entrega `datos` de un byte por frame, como un `chunked` con
/// trozos `1\r\nx\r\n`.
struct UnoAUno(Vec<u8>, usize);

impl http_body::Body for UnoAUno {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        let i = self.1;
        let Some(&b) = self.0.get(i) else {
            return Poll::Ready(None);
        };
        self.1 += 1;
        Poll::Ready(Some(Ok(Frame::data(Bytes::copy_from_slice(&[b])))))
    }
}

/// SEC2-H1 / SEC2-L1: un esquema que llega en 100 000 frames de 1 byte
/// (99 998 espacios y `{}`) compila igual que entero: 200, con el `id` de
/// sus bytes. La capacidad del búfer la comprueba
/// `api::tests::frames_de_un_byte_en_un_solo_bufer`.
#[tokio::test]
async fn esquema_en_frames_de_un_byte_da_200() {
    let mut datos = vec![b' '; 99_998];
    datos.extend_from_slice(b"{}");
    let app = router(Config::default());
    let cuerpo = Body::new(UnoAUno(datos.clone(), 0));
    let (estado, texto) = pedir_con_guarda(&app, post(cuerpo)).await;
    assert_eq!(estado, StatusCode::OK, "{texto}");
    assert_eq!(json(&texto)["id"], sha256(&datos));
}

/// IR por encima del tope (U8): `ir: null`, `irOmitted: true`, 200.
#[tokio::test]
async fn ir_por_encima_del_tope_se_omite() {
    let esquema = std::fs::read(repo("schemas/parse-ok/usuarios.json")).unwrap();
    let app = router(Config {
        max_ir_bytes: 100,
        ..Config::default()
    });
    let (estado, cuerpo) = pedir(&app, post(esquema)).await;
    assert_eq!(estado, StatusCode::OK, "{cuerpo}");
    let v = json(&cuerpo);
    assert_eq!(v["ir"], Value::Null);
    assert_eq!(v["irOmitted"], true);
    assert_eq!(v["irBytes"], Value::Null);
    assert!(v["automaton"]["status"].is_string());
}

/// Un pánico en la compilación da 500 con el mensaje fijo, y la petición
/// siguiente al mismo `Router` da 200 (`docs/fase3/pendientes.md` §2.1):
/// con concurrencia 1, eso prueba también que el permiso se liberó.
#[tokio::test]
async fn panico_da_500_y_el_servidor_sigue() {
    let compilador: Compilador = Arc::new(|b: &[u8], max_ir| {
        if b == b"panico" {
            panic!("pánico provocado por la prueba");
        }
        compilar::compilar(b, max_ir)
    });
    let app = router_con(
        Config {
            max_concurrent_compiles: 1,
            ..Config::default()
        },
        compilador,
    );
    let (estado, cuerpo) = pedir(&app, post("panico")).await;
    assert_eq!(estado, StatusCode::INTERNAL_SERVER_ERROR);
    let v = json(&cuerpo);
    assert_eq!(v["error"], "internal");
    assert_eq!(v["message"], "error interno del compilador");
    assert!(!cuerpo.contains("provocado"), "{cuerpo}");
    assert_eq!(pedir(&app, post("{}")).await.0, StatusCode::OK);
}

/// Un error interno (`verify` o `BuildError::Internal`) da 500 con un único
/// mensaje fijo; el detalle solo va al registro (D15).
#[tokio::test]
async fn error_interno_da_500_con_mensaje_fijo() {
    let compilador: Compilador =
        Arc::new(|_: &[u8], _| Compilado::Interno(vec!["detalle secreto".into()]));
    let (estado, cuerpo) = pedir(&router_con(Config::default(), compilador), post("{}")).await;
    assert_eq!(estado, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        json(&cuerpo),
        json(r#"{"error": "internal", "message": "error interno del compilador"}"#)
    );
}

/// Una compilación que se bloquea hasta que la prueba la suelta: avisa por
/// `empezo` al entrar y espera un mensaje de `soltar` por cada vez que se
/// llama.
fn bloqueada() -> (
    Compilador,
    tokio::sync::mpsc::UnboundedReceiver<()>,
    mpsc::Sender<()>,
) {
    let (empezo_tx, empezo) = tokio::sync::mpsc::unbounded_channel();
    let (soltar, soltar_rx) = mpsc::channel::<()>();
    let soltar_rx = Mutex::new(soltar_rx);
    let compilador: Compilador = Arc::new(move |b: &[u8], max_ir| {
        let _ = empezo_tx.send(());
        let _ = soltar_rx.lock().unwrap().recv();
        compilar::compilar(b, max_ir)
    });
    (compilador, empezo, soltar)
}

/// Concurrencia 1: con la primera compilación en curso (bloqueada), la
/// segunda da 503 `busy` con `Retry-After`, sin cola; al soltar la primera,
/// esta da 200. Determinista: la prueba espera el aviso de que empezó.
#[tokio::test]
async fn concurrencia_llena_da_503_sin_cola() {
    let (compilador, mut empezo, soltar) = bloqueada();
    let app = router_con(
        Config {
            max_concurrent_compiles: 1,
            ..Config::default()
        },
        compilador,
    );
    let primera = tokio::spawn({
        let app = app.clone();
        async move { pedir(&app, post("{}")).await }
    });
    empezo.recv().await.unwrap();
    let (estado, cabeceras, cuerpo) = pedir_con_cabeceras(&app, post("{}")).await;
    assert_eq!(estado, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(cabeceras.get("retry-after").unwrap(), "5");
    assert_eq!(json(&cuerpo)["error"], "busy");
    soltar.send(()).unwrap();
    assert_eq!(primera.await.unwrap().0, StatusCode::OK);
}

/// Tope de tiempo de 50 ms con una compilación bloqueada → 504; la
/// aserción no mide tiempo (D1). El permiso sigue con el hilo: mientras siga
/// bloqueado, otra petición con concurrencia 1 da 503; al soltarlo, 200.
#[tokio::test]
async fn tope_de_tiempo_da_504_y_el_permiso_sigue_en_el_hilo() {
    let (compilador, mut empezo, soltar) = bloqueada();
    let app = router_con(
        Config {
            compile_timeout: std::time::Duration::from_millis(50),
            max_concurrent_compiles: 1,
            ..Config::default()
        },
        compilador,
    );
    let (estado, cuerpo) = pedir(&app, post("{}")).await;
    assert_eq!(estado, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(json(&cuerpo)["error"], "timeout");
    empezo.recv().await.unwrap();
    let (estado, cuerpo) = pedir(&app, post("{}")).await;
    assert_eq!(estado, StatusCode::SERVICE_UNAVAILABLE, "{cuerpo}");
    assert_eq!(json(&cuerpo)["error"], "busy");
    // Uno para la compilación bloqueada y otro para la que dará 200.
    soltar.send(()).unwrap();
    soltar.send(()).unwrap();
    // El hilo suelta el permiso al terminar, en un momento que la prueba no
    // controla: se reintenta (sin medir el tiempo) mientras dé 503. Cualquier
    // otro estado prueba que el permiso se liberó: 200, o 504 si esta
    // compilación, ya sin bloqueo, tarda más de los 50 ms del mismo `Router`
    // en una máquina cargada (CR-C1: exigir 200 hacía la prueba depender del
    // reloj, D1). El tope de reintentos solo guarda contra un cuelgue.
    for _ in 0..1000 {
        let (estado, cuerpo) = pedir(&app, post("{}")).await;
        if estado == StatusCode::SERVICE_UNAVAILABLE {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            continue;
        }
        assert!(
            matches!(estado, StatusCode::OK | StatusCode::GATEWAY_TIMEOUT),
            "{estado}: {cuerpo}"
        );
        return;
    }
    panic!("el permiso no se liberó al terminar el hilo");
}

/// Si no se puede crear el hilo, 503 y el permiso se libera: con
/// concurrencia 1, la segunda petición vuelve a dar `no-thread`, no `busy`.
#[cfg(target_pointer_width = "64")]
#[tokio::test]
async fn sin_hilo_da_503_y_libera_el_permiso() {
    let app = router(Config {
        max_concurrent_compiles: 1,
        // Ningún sistema reserva 4 EiB de pila.
        stack_size: 1 << 62,
        ..Config::default()
    });
    for _ in 0..2 {
        let (estado, cabeceras, cuerpo) = pedir_con_cabeceras(&app, post("{}")).await;
        assert_eq!(estado, StatusCode::SERVICE_UNAVAILABLE, "{cuerpo}");
        assert_eq!(cabeceras.get("retry-after").unwrap(), "5");
        assert_eq!(json(&cuerpo)["error"], "no-thread");
    }
}

/// D86 (DTD.4): una DTD válida → 200 con `format: "xml"` (la API no cambia),
/// su IR (la dorada de `tests/ir/dtd`) y su autómata construido.
#[tokio::test]
async fn dtd_valida_da_200_con_automata() {
    let esquema = std::fs::read(repo("schemas/dtd/parse-ok/pedido.dtd")).unwrap();
    let dorada = std::fs::read_to_string(repo("ir/dtd/pedido.json")).unwrap();
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::OK, "{cuerpo}");
    assert!(cuerpo.ends_with(&format!("\"ir\":{dorada}}}")), "{cuerpo}");
    let v = json(&cuerpo);
    assert_eq!(v["format"], "xml");
    assert_eq!(
        v["summary"],
        "esquema válido (subconjunto de DTD); 6 elementos, 4 atributos"
    );
    assert_eq!(v["automaton"]["status"], "ok", "{cuerpo}");
}

/// Los errores de una DTD salen con su `code`, línea y columna, sin el
/// prefijo «línea L, columna C: » en el mensaje: una entidad (prohibida,
/// error léxico en su `<`) y todos los de la Fase 3.
#[tokio::test]
async fn dtd_invalida_da_422_con_sus_codigos() {
    let (estado, cuerpo) = pedir(
        &router(Config::default()),
        post("<!ELEMENT a (#PCDATA)>\n<!ENTITY x SYSTEM 'file:///etc/passwd'>"),
    )
    .await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY, "{cuerpo}");
    let v = json(&cuerpo);
    assert_eq!(v["format"], "xml");
    let e = &v["errors"][0];
    assert_eq!(
        (e["code"].as_str(), e["line"].as_u64(), e["column"].as_u64()),
        (Some("lexical"), Some(2), Some(1))
    );
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .starts_with("<!ENTITY está prohibido"),
        "{e}"
    );

    let esquema = std::fs::read(repo("schemas/dtd/semantic-error/varios.dtd")).unwrap();
    let (estado, cuerpo) = pedir(&router(Config::default()), post(esquema)).await;
    assert_eq!(estado, StatusCode::UNPROCESSABLE_ENTITY, "{cuerpo}");
    let v = json(&cuerpo);
    assert_eq!(v["total"], 3);
    let codigos: Vec<&str> = v["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["code"].as_str().unwrap())
        .collect();
    assert_eq!(codigos, ["content-model", "duplicate-name", "value"]);
}
