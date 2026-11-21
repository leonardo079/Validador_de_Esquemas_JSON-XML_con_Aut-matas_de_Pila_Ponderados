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
}

/// La API con la compilación real y sin base de datos.
pub fn router(cfg: Config) -> Router {
    router_con(cfg, Arc::new(compilar::compilar))
}

/// [`router`] con la colección de casos de Atlas (F7.2, D72): habilita
/// `GET /api/tests` y `POST /api/tests/run`.
pub fn router_con_casos(cfg: Config, casos: Option<crate::casos::Coleccion>) -> Router {
    construir(cfg, Arc::new(compilar::compilar), casos)
}

/// [`router`] con otra compilación (para las pruebas).
pub fn router_con(cfg: Config, compilar: Compilador) -> Router {
    construir(cfg, compilar, None)
}

fn construir(cfg: Config, compilar: Compilador, casos: Option<crate::casos::Coleccion>) -> Router {
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
    });
    Router::new()
        .route("/api/health", get(health))
        .route("/api/schemas", post(esquemas))
        .route("/api/automaton", post(automata))
        .route("/api/derivation", post(derivacion))
        .route("/api/compare", post(comparar))
        .route("/api/tests", get(listar_casos))
        .route("/api/tests/run", post(ejecutar_casos))
        .fallback(no_encontrado)
        .method_not_allowed_fallback(metodo_no_permitido)
        .layer(DefaultBodyLimit::max(limite))
        .layer(middleware::map_response(cabeceras_comunes))
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

async fn health() -> Response {
    responder(
        StatusCode::OK,
        json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}).to_string(),
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
            "message": format!("el esquema supera el tamaño máximo de {limit} bytes"),
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
        Error::SinBase => no_disponible(
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
    let Ok(permiso) = app.permisos.clone().try_acquire_owned() else {
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
    match tokio::time::timeout(app.cfg.compile_timeout, rx).await {
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
