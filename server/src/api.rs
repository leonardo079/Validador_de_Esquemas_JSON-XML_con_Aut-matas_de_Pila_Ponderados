//! Los manejadores HTTP y el contrato de las respuestas (U1,
//! `docs/fase7/servidor.md` §2). Todas son JSON; las de error llevan un campo
//! `error`.

use std::io::Write;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::RequestExt;
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{oneshot, Semaphore};

use crate::compilar::{self, Automata, Compilado};
use crate::{Config, STACK_SIZE};

/// Versión del contrato de `POST /api/schemas`.
const API_VERSION: u32 = 1;

/// Segundos de `Retry-After` en los 503 (B8).
const RETRY_AFTER: &str = "5";

/// Mensaje fijo de un error interno o de un pánico (D15): el detalle solo va
/// al registro.
const MENSAJE_INTERNO: &str = "error interno del compilador";

/// La compilación que corre en el hilo: los bytes del esquema y el tope de
/// la IR. En el servidor, siempre [`compilar::compilar`]; las pruebas
/// inyectan una que hace pánico o se bloquea (B1).
pub type Compilador = Arc<dyn Fn(&[u8], usize) -> Compilado + Send + Sync>;

/// Unidad del presupuesto de cuerpos en vuelo (SEC-H1).
const BLOQUE: usize = 1024;

/// Respuestas de más de este tamaño reservan su tamaño del presupuesto
/// hasta que se envían enteras (E3C-3, `docs/fase7/pendientes.md` §19).
const RESPUESTA_GRANDE: usize = 64 << 10;

/// Presupuesto global de bytes de respuestas grandes pendientes de enviar:
/// con él, cien clientes que no leen no retienen más de esto (antes, 121 ZIP
/// sin leer llevaron el proceso a 1 GB).
const PRESUPUESTO_RESPUESTAS: usize = 48 << 20;

struct App {
    cfg: Config,
    /// Permisos de compilación (`max_concurrent_compiles`).
    permisos: Arc<Semaphore>,
    /// Bloques de [`BLOQUE`] bytes de cuerpos que se están leyendo
    /// (`max_inflight_body_bytes`).
    cuerpos: Arc<Semaphore>,
    compilar: Compilador,
    /// La colección `cases` de Atlas (F7.2), si hay `MONGODB_URI`.
    casos: Option<crate::casos::Coleccion>,
    /// El sandbox Docker (F7.3) para `POST /api/validate` (F7.5), si está
    /// configurado (`SANDBOX=docker`).
    sandbox: Option<Arc<crate::sandbox::Sandbox>>,
    /// Sesiones de `WS /ws/validate` a la vez (F7.5).
    flujos: Arc<Semaphore>,
    /// Presupuesto de bytes de respuestas grandes que aún no se han enviado
    /// (E3C-3), en bloques de [`BLOQUE`].
    respuestas: Arc<Semaphore>,
    /// Un solo trabajo del sandbox a la vez, aparte de las compilaciones
    /// (E3C-4): `/api/validate` ya no retiene uno de los dos permisos.
    sandbox_permisos: Arc<Semaphore>,
}

/// La API con la compilación real y sin base de datos.
pub fn router(cfg: Config) -> Router {
    router_con(cfg, Arc::new(compilar::compilar))
}

/// [`router`] con la colección de casos de Atlas (F7.2, D72): habilita
/// `GET /api/tests` y `POST /api/tests/run`.
pub fn router_con_casos(cfg: Config, casos: Option<crate::casos::Coleccion>) -> Router {
    construir(cfg, Arc::new(compilar::compilar), casos, None)
}

/// [`router_con_casos`] con el sandbox de F7.3 para `POST /api/validate`
/// (F7.5, D81). Sin sandbox, esa ruta responde 503.
pub fn router_completo(
    cfg: Config,
    casos: Option<crate::casos::Coleccion>,
    sandbox: Option<crate::sandbox::Sandbox>,
) -> Router {
    construir(cfg, Arc::new(compilar::compilar), casos, sandbox)
}

/// [`router`] con otra compilación (para las pruebas).
pub fn router_con(cfg: Config, compilar: Compilador) -> Router {
    construir(cfg, compilar, None, None)
}

fn construir(
    cfg: Config,
    compilar: Compilador,
    casos: Option<crate::casos::Coleccion>,
    sandbox: Option<crate::sandbox::Sandbox>,
) -> Router {
    let limite = cfg.max_upload_bytes;
    // Nunca menos que una subida entera: si no, un cuerpo del tope no
    // cabría jamás.
    let presupuesto = cfg.max_inflight_body_bytes.max(limite);
    let app = Arc::new(App {
        permisos: Arc::new(Semaphore::new(cfg.max_concurrent_compiles)),
        cuerpos: Arc::new(Semaphore::new(bloques(presupuesto) as usize)),
        cfg,
        compilar,
        casos,
        sandbox: sandbox.map(Arc::new),
        flujos: Arc::new(Semaphore::new(crate::flujo::MAX_FLUJOS)),
        respuestas: Arc::new(Semaphore::new(PRESUPUESTO_RESPUESTAS / BLOQUE)),
        sandbox_permisos: Arc::new(Semaphore::new(1)),
    });
    Router::new()
        .route("/api/health", get(health))
        .route("/api/schemas", post(esquemas))
        .route("/api/automaton", post(automata))
        .route("/api/derivation", post(derivacion))
        .route("/api/compare", post(comparar))
        .route("/api/tests", get(listar_casos))
        .route("/api/tests/run", post(ejecutar_casos))
        .route("/api/code", post(codigo))
        .route("/api/validate", post(validar))
        .route("/ws/validate", get(flujo))
        .fallback(no_encontrado)
        .method_not_allowed_fallback(metodo_no_permitido)
        .layer(DefaultBodyLimit::max(limite))
        .layer(middleware::map_response(cabeceras_comunes))
        .layer(middleware::from_fn_with_state(
            app.clone(),
            presupuesto_respuestas,
        ))
        .with_state(app)
}

/// Cabeceras de todas las respuestas del `Router` (SEC-L2): que el
/// navegador no adivine otro tipo de contenido que el declarado. Las de
/// HSTS y CSP las pone Caddy (F7.7). Los 400/431 que genera hyper antes del
/// `Router` no pasan por aquí.
async fn cabeceras_comunes(mut r: Response) -> Response {
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

/// Una respuesta JSON con su estado.
fn responder(estado: StatusCode, cuerpo: String) -> Response {
    (estado, [(header::CONTENT_TYPE, "application/json")], cuerpo).into_response()
}

/// Un 503 con `Retry-After`.
fn no_disponible(error: &str, message: &str) -> Response {
    let cuerpo = json!({"error": error, "message": message}).to_string();
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::RETRY_AFTER, RETRY_AFTER),
        ],
        cuerpo,
    )
        .into_response()
}

/// Un 503 de una parte que este servidor **no tiene** (`no-sandbox`,
/// `no-database`): sin `Retry-After`, porque reintentar no sirve (D82 (3)).
fn no_configurado(error: &str, message: &str) -> Response {
    responder(
        StatusCode::SERVICE_UNAVAILABLE,
        json!({"error": error, "message": message}).to_string(),
    )
}

fn interno() -> Response {
    responder(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"error": "internal", "message": MENSAJE_INTERNO}).to_string(),
    )
}

/// El registro del servidor es stderr (`docs/fase3/pendientes.md` §14). Si
/// escribir falla, se ignora: `eprintln!` haría pánico (B5).
fn registrar(linea: &str) {
    let _ = writeln!(std::io::stderr(), "server: {linea}");
}

/// `GET /api/health`. Desde E3 (§25.4) anuncia también qué partes opcionales
/// tiene el servidor: `database` (Atlas, F7.2) y `sandbox` (F7.5), para que
/// la web no tenga que descubrirlo con un 503.
async fn health(State(app): State<Arc<App>>) -> Response {
    responder(
        StatusCode::OK,
        json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "database": app.casos.is_some(),
            "sandbox": app.sandbox.is_some(),
        })
        .to_string(),
    )
}

async fn no_encontrado() -> Response {
    responder(
        StatusCode::NOT_FOUND,
        json!({"error": "not-found", "message": "ruta desconocida"}).to_string(),
    )
}

async fn metodo_no_permitido() -> Response {
    responder(
        StatusCode::METHOD_NOT_ALLOWED,
        json!({"error": "method-not-allowed", "message": "método no permitido en esta ruta"})
            .to_string(),
    )
}

/// El 413 con el tope de subida.
fn demasiado_grande(limit: usize) -> Response {
    responder(
        StatusCode::PAYLOAD_TOO_LARGE,
        json!({
            "error": "too-large",
            "limit": limit,
            "message": format!("el cuerpo de la petición supera el tamaño máximo de {limit} bytes"),
        })
        .to_string(),
    )
}

/// El mayor `Content-Length` decimal de la petición, si hay alguno. Un
/// valor que no es un número decimal no cuenta: hyper ya rechaza esas
/// peticiones (y las de dos valores distintos) con su propio 400 o 431
/// antes de llegar aquí, y si alguna pasara, decide el cuerpo leído con el
/// tope de `DefaultBodyLimit`. Si llega `Transfer-Encoding`, hyper quita el
/// `Content-Length` (`hyper` 1.12, `proto/h1/role.rs`), así que un
/// `Content-Length: 1` con `chunked` no reserva 1 KiB sino el tope entero:
/// la garantía depende de hyper (CR2-S3).
fn anunciado(cabeceras: &HeaderMap) -> Option<u64> {
    cabeceras
        .get_all(header::CONTENT_LENGTH)
        .iter()
        .filter_map(|v| v.to_str().ok()?.trim().parse::<u64>().ok())
        .max()
}

/// Por qué no se pudo leer un cuerpo.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lectura {
    /// Pasa del tope de subida (413).
    DeMas,
    /// La conexión se cortó a mitad u otro error del cuerpo (400).
    Fallo,
}

/// Lee el cuerpo frame a frame y lo copia a **un solo** búfer con capacidad
/// para `reservado` bytes (lo que se reservó del presupuesto, ≤ `tope`), y
/// suelta cada frame en cuanto lo copia (SEC2-H1). `Bytes::from_request`
/// guardaba un `Bytes` de 32 B por frame hasta el final y después copiaba
/// todo a un bloque contiguo: con trozos `chunked` de 1 byte, 1 MiB de datos
/// ocupaba ≈ 40 MiB y unas pocas conexiones tumbaban el proceso. Así lo
/// retenido por un cuerpo es su búfer (sin copia final), y el búfer solo
/// crece por encima de lo reservado si el `Content-Length` miente por lo
/// bajo, algo que hyper impide por la red y que el tope acota igual.
pub(crate) async fn leer(req: Request, tope: usize, reservado: usize) -> Result<Vec<u8>, Lectura> {
    let mut cuerpo = req.into_limited_body();
    let mut buf = Vec::with_capacity(reservado.min(tope));
    while let Some(frame) = cuerpo.frame().await {
        let frame = frame.map_err(|e| {
            let es_tope = e
                .into_inner()
                .downcast_ref::<http_body_util::LengthLimitError>()
                .is_some();
            if es_tope {
                Lectura::DeMas
            } else {
                Lectura::Fallo
            }
        })?;
        // Los frames que no son datos (trailers) no cuentan.
        if let Ok(datos) = frame.into_data() {
            if datos.len() > tope - buf.len() {
                return Err(Lectura::DeMas);
            }
            buf.extend_from_slice(&datos);
        }
    }
    Ok(buf)
}

/// Bloques de [`BLOQUE`] bytes que reserva la lectura de un cuerpo que
/// puede ocupar `bytes`: al menos uno, para que también cuente una petición
/// vacía.
fn bloques(bytes: usize) -> u32 {
    u32::try_from(bytes.div_ceil(BLOQUE).max(1)).unwrap_or(u32::MAX)
}

/// `POST /api/schemas`: el cuerpo crudo es el esquema (sin `Content-Type`
/// obligatorio; el formato sale del contenido, D22).
///
/// Se extrae la petición entera, no `Bytes`, para mirar `Content-Length`
/// **antes** de leer el cuerpo (todos los extractores corren antes del
/// manejador): si anuncia más que el tope, 413 sin tocar el cuerpo. hyper
/// solo envía `100 Continue` cuando alguien lee el cuerpo, así que a un
/// cliente con `Expect: 100-continue` le llega el 413 directamente. Sin
/// cabecera (`chunked`) o con una que miente, el tope lo pone
/// `DefaultBodyLimit` al leer: tope + 1 bytes y 413.
///
/// La lectura tiene además un presupuesto global y un plazo (SEC-H1): sin
/// ellos, mil conexiones con 1 MiB a medias retienen 1 GiB sin pasar nunca
/// por el semáforo de las compilaciones. Antes de leer se reservan los
/// bloques de 1 KiB que el cuerpo puede llegar a ocupar
/// (`min(Content-Length, tope)`, o el tope si es `chunked`); si no hay, 503
/// `busy` sin leer nada. Si la lectura pasa de `body_read_timeout`, 408. La
/// reserva se suelta al terminar de leer, antes de pedir el permiso de
/// compilación: desde ahí, los bytes del cuerpo los acota la concurrencia
/// de las compilaciones (o se sueltan con el 503).
async fn esquemas(State(app): State<Arc<App>>, req: Request) -> Response {
    let compilar = app.compilar.clone();
    let max_ir = app.cfg.max_ir_bytes;
    procesar(&app, req, move |bytes| {
        let id = hex(&Sha256::digest(&bytes));
        let resultado = compilar(&bytes, max_ir);
        drop(bytes);
        respuesta(&id, resultado)
    })
    .await
}

/// `POST /api/automaton` (F7.6, D70): el grafo de M del esquema del cuerpo,
/// para Cytoscape.
async fn automata(State(app): State<Arc<App>>, req: Request) -> Response {
    procesar(&app, req, |bytes| vista(crate::vistas::grafo(&bytes))).await
}

/// `POST /api/derivation` (F7.6, D70): el árbol de derivación del payload del
/// cuerpo (F5.5), para d3-hierarchy. No necesita el esquema.
async fn derivacion(State(app): State<Arc<App>>, req: Request) -> Response {
    procesar(&app, req, |bytes| vista(crate::vistas::arbol(&bytes))).await
}

/// `POST /api/compare` (F7.6, D70): `{"schema": "…", "payload": "…"}`; valida
/// el payload con el DPDA y con la línea base DOM y devuelve los dos
/// veredictos con sus tiempos.
async fn comparar(State(app): State<Arc<App>>, req: Request) -> Response {
    procesar(&app, req, |bytes| vista(crate::vistas::comparar(&bytes))).await
}

/// La categoría pedida en `?category=…` (solo minúsculas y guiones).
/// `Err(())` si `category` no es `[a-z-]{1,32}` (400, [`categoria_no_valida`]).
fn categoria(req: &Request) -> Result<Option<String>, ()> {
    let Some(q) = req.uri().query() else {
        return Ok(None);
    };
    for par in q.split('&') {
        if let Some(v) = par.strip_prefix("category=") {
            if v.is_empty()
                || v.len() > 32
                || !v.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
            {
                return Err(());
            }
            return Ok(Some(v.to_owned()));
        }
    }
    Ok(None)
}

/// El 400 de una `category` no válida.
fn categoria_no_valida() -> Response {
    responder(
        StatusCode::BAD_REQUEST,
        json!({"error": "request", "message": "`category` no válida"}).to_string(),
    )
}

/// La respuesta de un error de la base de datos (el detalle, al registro).
fn fallo_casos(e: crate::casos::Error) -> Response {
    use crate::casos::Error;
    match e {
        Error::SinBase => no_configurado(
            "no-database",
            "el servidor no tiene base de datos (MONGODB_URI sin configurar)",
        ),
        Error::Base(detalle) => {
            registrar(&format!("atlas: {detalle}"));
            responder(
                StatusCode::BAD_GATEWAY,
                json!({"error": "database", "message": "no se pudo consultar la base de datos de casos"})
                    .to_string(),
            )
        }
    }
}

/// `GET /api/tests[?category=…]` (F7.2, D72): los casos de Atlas, sin esquema
/// ni payload.
async fn listar_casos(State(app): State<Arc<App>>, req: Request) -> Response {
    let cat = match categoria(&req) {
        Ok(c) => c,
        Err(()) => return categoria_no_valida(),
    };
    match crate::casos::listar(app.casos.as_ref(), cat.as_deref()).await {
        Ok(v) => responder(StatusCode::OK, v.to_string()),
        Err(e) => fallo_casos(e),
    }
}

/// `POST /api/tests/run[?category=…]` (F7.2, D72): trae los casos de Atlas y
/// los ejecuta con el DPDA en el hilo de [`procesar`] (permiso, pila y tope
/// de tiempo), comparando con su `expected`. El cuerpo se ignora.
async fn ejecutar_casos(State(app): State<Arc<App>>, req: Request) -> Response {
    let cat = match categoria(&req) {
        Ok(c) => c,
        Err(()) => return categoria_no_valida(),
    };
    let lista = match crate::casos::traer(app.casos.as_ref(), cat.as_deref()).await {
        Ok(l) => l,
        Err(e) => return fallo_casos(e),
    };
    procesar(&app, req, move |_| {
        responder(StatusCode::OK, crate::casos::ejecutar(&lista).to_string())
    })
    .await
}

/// El lenguaje de `?lang=rust|c` (por defecto [`crate::generado::LANG_POR_DEFECTO`],
/// el mismo que `/api/validate`, D82 (1)); `Err` con el motivo si es otro o si
/// `lang` viene más de una vez.
fn lenguaje_de(req: &Request) -> Result<crate::sandbox::Lenguaje, &'static str> {
    let valores: Vec<&str> = req
        .uri()
        .query()
        .map(|q| {
            q.split('&')
                .filter_map(|p| p.strip_prefix("lang="))
                .collect()
        })
        .unwrap_or_default();
    if valores.len() > 1 {
        return Err("`lang` aparece más de una vez");
    }
    let valor = valores
        .first()
        .copied()
        .unwrap_or(crate::generado::LANG_POR_DEFECTO);
    crate::generado::lenguaje(valor).ok_or("`lang` debe ser `rust` o `c`")
}

/// `POST /api/code?lang=rust|c` (F7.5, D81): el cuerpo es el esquema; la
/// respuesta, un ZIP con el validador generado y un `LEEME.txt`.
async fn codigo(State(app): State<Arc<App>>, req: Request) -> Response {
    let l = match lenguaje_de(&req) {
        Ok(l) => l,
        Err(m) => {
            return responder(
                StatusCode::BAD_REQUEST,
                json!({"error": "request", "message": m}).to_string(),
            )
        }
    };
    procesar(&app, req, move |bytes| {
        match crate::generado::zip(&bytes, l) {
            Ok((nombre, zip)) => (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip".to_owned()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{nombre}\""),
                    ),
                ],
                zip,
            )
                .into_response(),
            Err(f) => vista(Err(f)),
        }
    })
    .await
}

/// `POST /api/validate` (F7.5, D81): `{"schema", "payload", "lang"}`;
/// compila y ejecuta el código generado en el sandbox (F7.3) y lo compara
/// con el intérprete. Plazo: el del sandbox (compilar + ejecutar + holgura).
async fn validar(State(app): State<Arc<App>>, req: Request) -> Response {
    // D82 (3): sin sandbox, 503 sin leer ni validar el cuerpo.
    if app.sandbox.is_none() {
        return vista(Err(crate::vistas::Fallo::SinSandbox));
    }
    let sb = app.sandbox.clone();
    let plazo = sb
        .as_ref()
        .map(|s| s.limites.compilar + s.limites.ejecutar + s.limites.holgura)
        .unwrap_or(app.cfg.compile_timeout);
    procesar_con_plazo(&app, req, &app.sandbox_permisos, plazo, move |bytes| {
        vista(crate::generado::validar(&bytes, sb.as_deref()))
    })
    .await
}

/// `WS /ws/validate` (F7.5, D81): validación por bloques en vivo
/// (`flujo.rs`). Sin cola: con todas las sesiones ocupadas, 503.
async fn flujo(
    State(app): State<Arc<App>>,
    cabeceras: HeaderMap,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    if !mismo_origen(&cabeceras) {
        return responder(
            StatusCode::FORBIDDEN,
            json!({"error": "origin", "message": "WebSocket desde otro origen"}).to_string(),
        );
    }
    let Ok(permiso) = app.flujos.clone().try_acquire_owned() else {
        return no_disponible(
            "busy",
            "hay demasiadas validaciones en vivo; reintente en unos segundos",
        );
    };
    let cx = crate::flujo::Contexto {
        permisos: app.permisos.clone(),
        plazo_compilar: app.cfg.compile_timeout,
    };
    ws.max_message_size(crate::flujo::MAX_BLOQUE)
        .max_frame_size(crate::flujo::MAX_BLOQUE)
        .on_upgrade(move |s| crate::flujo::sesion(s, permiso, cx))
}

/// E3C-7: un navegador manda `Origin`; si lo manda, su host tiene que ser el
/// de `Host` (la web y la API comparten origen, D56). Sin `Origin` (clientes
/// que no son navegadores) se acepta: no hay cookies ni credenciales que
/// robar, el riesgo es solo de cuota (topes de sesión).
fn mismo_origen(c: &HeaderMap) -> bool {
    let Some(origen) = c.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let Some(host) = c.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let sin_esquema = origen
        .strip_prefix("https://")
        .or_else(|| origen.strip_prefix("http://"))
        .unwrap_or("");
    sin_esquema.eq_ignore_ascii_case(host)
}

/// La respuesta de una vista: 200 con su JSON, o el error que toque.
fn vista(r: Result<String, crate::vistas::Fallo>) -> Response {
    use crate::vistas::Fallo;
    match r {
        Ok(cuerpo) => responder(StatusCode::OK, cuerpo),
        Err(Fallo::Compilacion(c)) => respuesta("", c),
        Err(Fallo::Peticion(m)) => responder(
            StatusCode::BAD_REQUEST,
            json!({"error": "request", "message": m}).to_string(),
        ),
        Err(Fallo::SinAutomata { status, message }) => responder(
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({"error": "automaton", "status": status, "message": message}).to_string(),
        ),
        Err(Fallo::Payload(v)) => responder(StatusCode::UNPROCESSABLE_ENTITY, v.to_string()),
        Err(Fallo::SinSandbox) => no_configurado(
            "no-sandbox",
            "el servidor no tiene sandbox para compilar el código generado (SANDBOX sin configurar)",
        ),
        Err(Fallo::Codegen(detalle)) => {
            // D82 (4): el código generado no compiló: es un fallo del emisor (500),
            // no del sandbox. El diagnóstico, solo al registro (D15).
            registrar(&format!("codegen: {detalle}"));
            responder(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "codegen", "message": "el código generado no compiló (error del emisor)"})
                    .to_string(),
            )
        }
        Err(Fallo::Sandbox(detalle)) => {
            registrar(&format!("sandbox: {detalle}"));
            // E3C-6 (D15): el diagnóstico del compilador o de `docker run` va solo al
            // registro; el cliente recibe un mensaje fijo.
            responder(
                StatusCode::BAD_GATEWAY,
                json!({"error": "sandbox", "message": "el sandbox no pudo compilar o ejecutar el código generado"})
                    .to_string(),
            )
        }
    }
}

/// Lo común de todos los `POST`: lee el cuerpo con el tope, el presupuesto y
/// el plazo de [`esquemas`]; toma un permiso de concurrencia (sin cola) y
/// corre `trabajo` en un hilo propio con pila explícita (D7) y tope de
/// tiempo. Un pánico del trabajo da 500; pasarse de tiempo, 504.
async fn procesar<F>(app: &App, req: Request, trabajo: F) -> Response
where
    F: FnOnce(Vec<u8>) -> Response + Send + 'static,
{
    procesar_con_plazo(app, req, &app.permisos, app.cfg.compile_timeout, trabajo).await
}

/// [`procesar`] con otro semáforo y otro tope de tiempo (los del sandbox,
/// F7.5; E3C-4).
async fn procesar_con_plazo<F>(
    app: &App,
    req: Request,
    permisos: &Arc<Semaphore>,
    plazo: std::time::Duration,
    trabajo: F,
) -> Response
where
    F: FnOnce(Vec<u8>) -> Response + Send + 'static,
{
    let limit = app.cfg.max_upload_bytes;
    let anuncia = anunciado(req.headers());
    if anuncia.is_some_and(|n| n > limit as u64) {
        return demasiado_grande(limit);
    }
    // `n ≤ limit`, así que cabe en `usize`.
    let maximo = anuncia.map_or(limit, |n| usize::try_from(n).unwrap_or(limit));
    let Ok(reserva) = app.cuerpos.clone().try_acquire_many_owned(bloques(maximo)) else {
        return no_disponible(
            "busy",
            "hay demasiadas subidas en curso; reintente en unos segundos",
        );
    };
    let leido = tokio::time::timeout(app.cfg.body_read_timeout, leer(req, limit, maximo)).await;
    drop(reserva);
    // hyper ya cierra la conexión tras el 408, porque el cuerpo quedó sin
    // leer; no hace falta `Connection: close` (CR2-S4).
    let bytes = match leido {
        Err(_) => {
            return responder(
                StatusCode::REQUEST_TIMEOUT,
                json!({
                    "error": "body-timeout",
                    "message": "el cuerpo de la petición no llegó a tiempo",
                })
                .to_string(),
            )
        }
        Ok(Ok(b)) => b,
        Ok(Err(Lectura::DeMas)) => return demasiado_grande(limit),
        // La conexión se cortó a mitad del cuerpo, u otro fallo al leerlo.
        Ok(Err(Lectura::Fallo)) => {
            return responder(
                StatusCode::BAD_REQUEST,
                json!({"error": "body", "message": "no se pudo leer el cuerpo de la petición"})
                    .to_string(),
            )
        }
    };
    // Sin cola: con todas las compilaciones ocupadas, 503 enseguida.
    let Ok(permiso) = permisos.clone().try_acquire_owned() else {
        return no_disponible(
            "busy",
            "hay demasiadas compilaciones en curso; reintente en unos segundos",
        );
    };
    let (tx, rx) = oneshot::channel();
    // Nunca se compila en un hilo de tokio: hilo propio con pila explícita
    // (D7). El permiso viaja con el hilo y se libera cuando la compilación
    // termina (también tras un 504 o un pánico), así que una compilación que
    // se pasa de tiempo sigue contando en el tope de concurrencia. Un pánico
    // suelta `tx` al desenrollar y llega como un `RecvError`
    // (`docs/fase3/pendientes.md` §2.1).
    let hilo = std::thread::Builder::new()
        .name("server-compilar".into())
        .stack_size(app.cfg.stack_size.max(STACK_SIZE))
        .spawn(move || {
            // Al desenrollar, las variables se sueltan en orden inverso: el
            // permiso antes que `tx`, así que quien recibe el 500 ya
            // encuentra el permiso libre. En el camino normal, igual.
            let tx = tx;
            let permiso = permiso;
            let r = trabajo(bytes);
            drop(permiso);
            let _ = tx.send(r);
        });
    if let Err(e) = hilo {
        // El cierre (y con él el permiso) ya se soltó.
        registrar(&format!("no se pudo crear el hilo de compilación: {e}"));
        return no_disponible(
            "no-thread",
            "no se pudo crear el hilo de compilación; reintente en unos segundos",
        );
    }
    match tokio::time::timeout(plazo, rx).await {
        Err(_) => responder(
            StatusCode::GATEWAY_TIMEOUT,
            json!({
                "error": "timeout",
                "message": "la compilación superó el tiempo máximo",
            })
            .to_string(),
        ),
        Ok(Err(_)) => {
            registrar("la compilación terminó con un pánico");
            interno()
        }
        Ok(Ok(r)) => r,
    }
}

/// La respuesta de una compilación terminada.
fn respuesta(id: &str, c: Compilado) -> Response {
    match c {
        Compilado::Valido {
            formato,
            resumen,
            ir,
            automata,
        } => responder(StatusCode::OK, valido(id, formato, &resumen, ir, automata)),
        Compilado::Esquema {
            formato,
            errores,
            total,
        } => {
            let lista: Vec<Value> = errores
                .iter()
                .map(|e| {
                    json!({
                        "code": e.code,
                        "line": e.line,
                        "column": e.column,
                        "message": e.message,
                    })
                })
                .collect();
            responder(
                StatusCode::UNPROCESSABLE_ENTITY,
                json!({
                    "error": "schema",
                    "format": formato.nombre(),
                    "truncated": total > lista.len(),
                    "total": total,
                    "errors": lista,
                })
                .to_string(),
            )
        }
        Compilado::Interno(registro) => {
            for linea in &registro {
                registrar(&format!("registro: {linea}"));
            }
            interno()
        }
    }
}

/// El cuerpo del 200, armado a mano para incrustar la IR y el resumen del
/// autómata tal cual, sin volver a parsearlos (B4). La IR va al final.
fn valido(
    id: &str,
    formato: compilar::Formato,
    resumen: &str,
    ir: Option<String>,
    automata: Automata,
) -> String {
    let automata = match automata {
        Automata::Ok(resumen) => {
            format!("{{\"status\":\"ok\",\"summary\":{}}}", resumen.trim_end())
        }
        Automata::Pendiente(m) => json!({"status": "pending", "message": m}).to_string(),
        Automata::Limite(m) => json!({"status": "limit", "message": m}).to_string(),
    };
    let (ir_bytes, ir_omitida) = match &ir {
        Some(t) => (t.len().to_string(), false),
        None => ("null".to_owned(), true),
    };
    let ir = ir.unwrap_or_else(|| "null".to_owned());
    let mut s = String::with_capacity(ir.len() + automata.len() + resumen.len() + 256);
    s.push_str(&format!(
        "{{\"apiVersion\":{API_VERSION},\"id\":{},\"format\":{},\"summary\":{},\
         \"irBytes\":{ir_bytes},\"irOmitted\":{ir_omitida},\"automaton\":{automata},\"ir\":",
        Value::from(id),
        Value::from(formato.nombre()),
        Value::from(resumen),
    ));
    s.push_str(&ir);
    s.push('}');
    s
}

/// Hexadecimal en minúsculas.
fn hex(bytes: &[u8]) -> String {
    const CIFRAS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(2 * bytes.len());
    for &b in bytes {
        s.push(char::from(CIFRAS[usize::from(b >> 4)]));
        s.push(char::from(CIFRAS[usize::from(b & 0xf)]));
    }
    s
}

/// Un cuerpo que retiene su reserva del presupuesto de respuestas hasta que
/// hyper lo suelta: tras enviarlo entero o al cortarse la conexión.
struct ConReserva {
    cuerpo: axum::body::Body,
    _reserva: tokio::sync::OwnedSemaphorePermit,
}

impl http_body::Body for ConReserva {
    type Data = axum::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        std::pin::Pin::new(&mut self.cuerpo).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.cuerpo.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.cuerpo.size_hint()
    }
}

/// E3C-3 (`docs/fase7/pendientes.md` §19): una respuesta de más de
/// [`RESPUESTA_GRANDE`] reserva su tamaño de [`PRESUPUESTO_RESPUESTAS`]
/// hasta que se termina de enviar; sin presupuesto, 503 `busy` (y la
/// respuesta construida se suelta en el acto).
async fn presupuesto_respuestas(
    State(app): State<Arc<App>>,
    req: Request,
    siguiente: middleware::Next,
) -> Response {
    let r = siguiente.run(req).await;
    let tam = http_body::Body::size_hint(r.body())
        .exact()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(usize::MAX);
    if tam <= RESPUESTA_GRANDE {
        return r;
    }
    let Ok(reserva) = app
        .respuestas
        .clone()
        .try_acquire_many_owned(bloques(tam.min(PRESUPUESTO_RESPUESTAS)))
    else {
        return no_disponible(
            "busy",
            "hay demasiadas respuestas grandes pendientes de enviar; reintente en unos segundos",
        );
    };
    let (partes, cuerpo) = r.into_parts();
    Response::from_parts(
        partes,
        axum::body::Body::new(ConReserva {
            cuerpo,
            _reserva: reserva,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use axum::body::{Body, Bytes};
    use http_body::Frame;

    /// Un cuerpo de `n` frames de un byte cada uno (SEC2-H1): la forma que
    /// da hyper a un `chunked` con trozos `1\r\nx\r\n`.
    struct Migajas(usize);

    impl http_body::Body for Migajas {
        type Data = Bytes;
        type Error = Infallible;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
            if self.0 == 0 {
                return Poll::Ready(None);
            }
            self.0 -= 1;
            Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b" ")))))
        }
    }

    fn peticion(n: usize, tope: usize) -> Request {
        let mut req = Request::new(Body::new(Migajas(n)));
        req.extensions_mut().insert(DefaultBodyLimit::max(tope));
        req
    }

    /// SEC2-H1 / SEC2-L1: 100 000 frames de 1 byte quedan en **un** búfer
    /// con la capacidad reservada (el tope, como en `chunked`), sin crecer:
    /// lo retenido por el cuerpo es lo reservado del presupuesto.
    #[tokio::test]
    async fn frames_de_un_byte_en_un_solo_bufer() {
        let tope = 1 << 20;
        let b = leer(peticion(100_000, tope), tope, tope).await.unwrap();
        assert_eq!(b.len(), 100_000);
        assert_eq!(b.capacity(), tope);
        // Con un `Content-Length` (reserva menor), la capacidad es esa.
        let b = leer(peticion(100_000, tope), tope, 100_000).await.unwrap();
        assert_eq!((b.len(), b.capacity()), (100_000, 100_000));
    }

    /// E3C-7: el WebSocket solo acepta un `Origin` del mismo host (o ninguno).
    #[test]
    fn origen_del_websocket() {
        let c = |o: Option<&str>, h: Option<&str>| {
            let mut m = HeaderMap::new();
            if let Some(o) = o {
                m.insert(header::ORIGIN, HeaderValue::from_str(o).unwrap());
            }
            if let Some(h) = h {
                m.insert(header::HOST, HeaderValue::from_str(h).unwrap());
            }
            mismo_origen(&m)
        };
        assert!(
            c(None, Some("x.sslip.io")),
            "sin Origin: no es un navegador"
        );
        assert!(c(
            Some("https://3-4-5-6.sslip.io"),
            Some("3-4-5-6.sslip.io")
        ));
        assert!(c(Some("http://localhost:5173"), Some("localhost:5173")));
        assert!(c(
            Some("https://EJEMPLO.sslip.io"),
            Some("ejemplo.sslip.io")
        ));
        assert!(!c(Some("https://malo.example"), Some("3-4-5-6.sslip.io")));
        assert!(!c(
            Some("https://3-4-5-6.sslip.io.malo.example"),
            Some("3-4-5-6.sslip.io")
        ));
        assert!(!c(Some("null"), Some("x")));
        assert!(!c(Some("https://x"), None));
    }

    /// Pasar del tope con frames pequeños es un 413, no un 400.
    #[tokio::test]
    async fn frames_pequenos_de_mas_dan_413() {
        let tope = 1000;
        assert_eq!(
            leer(peticion(1001, tope), tope, tope).await,
            Err(Lectura::DeMas)
        );
        assert_eq!(
            leer(peticion(1000, tope), tope, tope).await.unwrap().len(),
            1000
        );
    }
}
