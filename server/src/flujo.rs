//! F7.5 — `WS /ws/validate`: validación por bloques en vivo (D81;
//! `docs/fase7/codigo.md` §3).
//!
//! Protocolo (mensajes de texto JSON del servidor):
//!
//! 1. El cliente manda el **esquema** como primer mensaje de texto. El
//!    servidor responde `{"type":"ready","format","states","transitions",
//!    "memoryBytes"}` o `{"type":"error","code","message"}` y cierra.
//! 2. El cliente manda el payload en **bloques binarios** (como mucho
//!    [`MAX_BLOQUE`] cada uno). Tras cada uno, el servidor responde
//!    `{"type":"progress","chunk","bytes","stopped"}`: `stopped` es que el
//!    validador ya encontró un error de peso ∞ (malformado o límite) y lo que
//!    llegue después no cambia el veredicto.
//! 3. El cliente manda el texto `fin`. El servidor responde
//!    `{"type":"report","bytes","chunks","report":{…}}` (los campos del
//!    binario generado) y cierra.
//!
//! El validador es el incremental de `runtime-rs` (el mismo núcleo que el
//! código generado), con memoria fija y sin árbol: el servidor no guarda el
//! payload. Corre en un hilo propio con pila explícita (D7).
//!
//! **Topes (E3, `docs/fase7/pendientes.md` §17, §18, §21):**
//! - la compilación del esquema pide un permiso de compilación del servidor
//!   (el mismo semáforo que `POST /api/schemas`) y tiene su plazo;
//! - [`CPU_POR_SESION`] de validación por sesión, medidos cada [`PASO`]
//!   bytes dentro de cada bloque (una cota numérica larga hace caro cada
//!   número);
//! - [`DURACION_MAXIMA`] por sesión y [`MAX_MENSAJES`] mensajes (también los
//!   *ping* y los bloques vacíos), además de [`INACTIVIDAD`] y [`MAX_TOTAL`].
//!
//! Los errores llevan un `code` estable y el cierre su código WebSocket
//! (1008 protocolo, 1009 demasiado grande, 1011 interno, 1013 reintentar).

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use automaton::{Mode, Options, PayloadFormat};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use runtime_rs::{Flow, JsonValidator, XmlValidator};
use serde_json::{json, Value};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::compilar::Compilado;
use crate::vistas::{construir, Fallo};

/// Sesiones a la vez.
pub const MAX_FLUJOS: usize = 4;
/// Tamaño máximo de un mensaje (un bloque o el esquema).
pub const MAX_BLOQUE: usize = 1 << 20;
/// Bytes de payload por sesión.
pub const MAX_TOTAL: u64 = 64 << 20;
/// Sin mensajes del cliente durante este tiempo, se cierra.
pub const INACTIVIDAD: Duration = Duration::from_secs(30);
/// Duración máxima de una sesión, haga lo que haga el cliente (E3C-2).
pub const DURACION_MAXIMA: Duration = Duration::from_secs(120);
/// Mensajes del cliente por sesión, de cualquier tipo (E3C-2).
pub const MAX_MENSAJES: u64 = 20_000;
/// Tiempo de validación (en el hilo) por sesión (E3A-2).
pub const CPU_POR_SESION: Duration = Duration::from_secs(20);
/// Cada cuántos bytes se mira el reloj dentro de un bloque.
pub const PASO: usize = 4096;

/// Códigos de cierre WebSocket (RFC 6455 §7.4.1).
const PROTOCOLO: u16 = 1008;
const GRANDE: u16 = 1009;
const INTERNO: u16 = 1011;
const REINTENTAR: u16 = 1013;

enum Peticion {
    Bloque(Vec<u8>),
    Fin,
}

/// Texto breve de por qué no se pudo construir M.
fn describir(f: &Fallo) -> String {
    match f {
        Fallo::Compilacion(Compilado::Esquema { errores, total, .. }) => match errores.first() {
            Some(e) => format!(
                "el esquema no compila ({total} errores; el primero, línea {}, columna {}: {})",
                e.line, e.column, e.message
            ),
            None => "el esquema no compila".into(),
        },
        Fallo::Compilacion(_) => "error interno del compilador".into(),
        Fallo::SinAutomata { message, .. } => message.clone(),
        _ => "no se pudo construir el autómata".into(),
    }
}

fn error_json(code: &str, message: &str) -> Value {
    json!({"type": "error", "code": code, "message": message})
}

/// El hilo del validador: construye M (con el permiso de compilación, que se
/// suelta al acabar) y atiende bloques hasta `Fin` o hasta agotar
/// [`CPU_POR_SESION`].
fn hilo(
    esquema: String,
    permiso: Option<OwnedSemaphorePermit>,
    rx: mpsc::Receiver<Peticion>,
    tx: tokio::sync::mpsc::Sender<Value>,
) {
    let construido = construir(esquema.as_bytes());
    drop(esquema);
    drop(permiso);
    let a = match construido {
        Ok((f, a)) => {
            let s = a.summary();
            let palabras = a.state_words(&Options::new(Mode::Total).limits);
            let _ = tx.blocking_send(json!({
                "type": "ready",
                "format": f.nombre(),
                "states": s.states,
                "transitions": s.transitions,
                "memoryBytes": palabras * 8,
            }));
            a
        }
        Err(f) => {
            let _ = tx.blocking_send(error_json("schema", &describir(&f)));
            return;
        }
    };
    let opts = Options::new(Mode::Total);
    let mut mem = vec![0u64; a.state_words(&opts.limits)];
    enum V<'t, 'm> {
        J(JsonValidator<'t, 'm>),
        X(XmlValidator<'t, 'm>),
    }
    let mut v = match a.format() {
        PayloadFormat::Json => V::J(JsonValidator::new(a.tables(), &opts, &mut mem)),
        PayloadFormat::Xml => V::X(XmlValidator::new(a.tables(), &opts, &mut mem)),
    };
    let (mut bytes, mut bloques, mut parado) = (0u64, 0u64, false);
    let mut gastado = Duration::ZERO;
    while let Ok(p) = rx.recv() {
        match p {
            Peticion::Bloque(b) => {
                bytes += b.len() as u64;
                bloques += 1;
                // Por trozos de PASO bytes, mirando el reloj entre trozos.
                for trozo in b.chunks(PASO) {
                    if parado {
                        break;
                    }
                    let t = Instant::now();
                    let flujo = match &mut v {
                        V::J(j) => j.feed(trozo),
                        V::X(x) => x.feed(trozo),
                    };
                    gastado += t.elapsed();
                    parado = flujo == Flow::Stop;
                    if gastado > CPU_POR_SESION {
                        let _ = tx.blocking_send(error_json(
                            "cpu-limit",
                            &format!(
                                "la validación pasó de {} s de CPU",
                                CPU_POR_SESION.as_secs()
                            ),
                        ));
                        return;
                    }
                }
                let m = json!({"type": "progress", "chunk": bloques, "bytes": bytes, "stopped": parado});
                if tx.blocking_send(m).is_err() {
                    return;
                }
            }
            Peticion::Fin => {
                let r = match v {
                    V::J(j) => j.finish(),
                    V::X(x) => x.finish(),
                };
                let informe = crate::generado::informe(&a, &r);
                let _ = tx.blocking_send(
                    json!({"type": "report", "bytes": bytes, "chunks": bloques, "report": informe}),
                );
                return;
            }
        }
    }
}

async fn enviar(s: &mut WebSocket, v: Value) -> bool {
    s.send(Message::Text(v.to_string().into())).await.is_ok()
}

async fn cerrar(s: &mut WebSocket, codigo: u16, motivo: &str) {
    let _ = s
        .send(Message::Close(Some(CloseFrame {
            code: codigo,
            reason: motivo.to_owned().into(),
        })))
        .await;
}

/// Manda el error con su `code` y cierra con `cierre`.
async fn error(s: &mut WebSocket, code: &str, message: &str, cierre: u16) {
    let _ = enviar(s, error_json(code, message)).await;
    cerrar(s, cierre, code).await;
}

/// Lo que necesita una sesión del servidor.
#[derive(Clone, Debug)]
pub struct Contexto {
    /// El semáforo de compilaciones del servidor (`app.permisos`, E3C-1).
    pub permisos: Arc<Semaphore>,
    /// Plazo de la compilación del esquema (`compile_timeout`).
    pub plazo_compilar: Duration,
}

/// Una sesión. `permiso` (el de las sesiones) se suelta al terminar.
pub async fn sesion(mut s: WebSocket, permiso: OwnedSemaphorePermit, cx: Contexto) {
    let _permiso = permiso;
    let inicio = Instant::now();
    let esquema = match tokio::time::timeout(INACTIVIDAD, s.recv()).await {
        Ok(Some(Ok(Message::Text(t)))) => t.to_string(),
        _ => {
            return error(
                &mut s,
                "protocol",
                "el primer mensaje debe ser el esquema (texto)",
                PROTOCOLO,
            )
            .await
        }
    };
    // E3C-1: la compilación pide un permiso, como `POST /api/schemas`.
    let Ok(compilar) = cx.permisos.clone().try_acquire_owned() else {
        return error(
            &mut s,
            "busy",
            "hay demasiadas compilaciones en curso; reintente en unos segundos",
            REINTENTAR,
        )
        .await;
    };
    let (tx_pet, rx_pet) = mpsc::channel::<Peticion>();
    let (tx_res, mut rx_res) = tokio::sync::mpsc::channel::<Value>(2);
    let hilo = std::thread::Builder::new()
        .name("server-flujo".into())
        .stack_size(crate::STACK_SIZE)
        .spawn(move || hilo(esquema, Some(compilar), rx_pet, tx_res));
    if hilo.is_err() {
        return error(
            &mut s,
            "internal",
            "no se pudo crear el hilo de validación",
            INTERNO,
        )
        .await;
    }
    // ready o error, con el plazo de compilación.
    match tokio::time::timeout(cx.plazo_compilar, rx_res.recv()).await {
        Err(_) => {
            return error(
                &mut s,
                "timeout",
                "la compilación superó el tiempo máximo",
                REINTENTAR,
            )
            .await
        }
        Ok(Some(v)) if v["type"] == "ready" => {
            if !enviar(&mut s, v).await {
                return;
            }
        }
        Ok(Some(v)) => {
            let _ = enviar(&mut s, v).await;
            return cerrar(&mut s, PROTOCOLO, "schema").await;
        }
        Ok(None) => {
            return error(&mut s, "internal", "error interno del compilador", INTERNO).await
        }
    }
    let (mut total, mut mensajes) = (0u64, 0u64);
    loop {
        // El plazo de cada espera: la inactividad, sin pasar de la duración
        // máxima de la sesión (E3C-2).
        let Some(queda) = DURACION_MAXIMA.checked_sub(inicio.elapsed()) else {
            return error(
                &mut s,
                "session-limit",
                "la sesión pasó de su duración máxima (120 s)",
                REINTENTAR,
            )
            .await;
        };
        let m = match tokio::time::timeout(INACTIVIDAD.min(queda), s.recv()).await {
            Err(_) if queda <= INACTIVIDAD => {
                return error(
                    &mut s,
                    "session-limit",
                    "la sesión pasó de su duración máxima (120 s)",
                    REINTENTAR,
                )
                .await
            }
            Err(_) => return error(&mut s, "idle", "sin datos durante 30 s", REINTENTAR).await,
            Ok(None) | Ok(Some(Err(_))) => return,
            Ok(Some(Ok(m))) => m,
        };
        mensajes += 1;
        if mensajes > MAX_MENSAJES {
            return error(
                &mut s,
                "session-limit",
                "demasiados mensajes en la sesión",
                PROTOCOLO,
            )
            .await;
        }
        let fin = match m {
            Message::Binary(b) => {
                total += b.len() as u64;
                if total > MAX_TOTAL {
                    return error(&mut s, "too-large", "el payload supera 64 MiB", GRANDE).await;
                }
                if tx_pet.send(Peticion::Bloque(b.to_vec())).is_err() {
                    return error(&mut s, "internal", "el validador terminó", INTERNO).await;
                }
                false
            }
            Message::Text(t) if t.as_str() == "fin" => {
                if tx_pet.send(Peticion::Fin).is_err() {
                    return error(&mut s, "internal", "el validador terminó", INTERNO).await;
                }
                true
            }
            Message::Text(_) => {
                return error(
                    &mut s,
                    "protocol",
                    "mensaje de texto desconocido (se espera `fin`)",
                    PROTOCOLO,
                )
                .await
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(_) => return,
        };
        match rx_res.recv().await {
            Some(v) if v["type"] == "error" => {
                // `cpu-limit` u otro error del hilo: se manda y se cierra.
                let code = v["code"].as_str().unwrap_or("internal").to_owned();
                let _ = enviar(&mut s, v).await;
                return cerrar(
                    &mut s,
                    if code == "cpu-limit" {
                        REINTENTAR
                    } else {
                        INTERNO
                    },
                    &code,
                )
                .await;
            }
            Some(v) => {
                if !enviar(&mut s, v).await {
                    return;
                }
            }
            None => return error(&mut s, "internal", "el validador terminó", INTERNO).await,
        }
        if fin {
            return cerrar(&mut s, 1000, "fin").await;
        }
    }
}

/// La misma conversación que [`sesion`] sin red, para las pruebas: el
/// esquema, los bloques y `fin` pasan por el hilo real del validador; devuelve
/// los mensajes del servidor en orden (`ready` o `error`, un `progress` por
/// bloque y el `report`, o el `error` que corte la sesión).
pub fn conversar(esquema: &str, bloques: &[&[u8]]) -> Vec<Value> {
    let (tx_pet, rx_pet) = mpsc::channel::<Peticion>();
    let (tx_res, mut rx_res) = tokio::sync::mpsc::channel::<Value>(2);
    let e = esquema.to_owned();
    let h = std::thread::Builder::new()
        .stack_size(crate::STACK_SIZE)
        .spawn(move || hilo(e, None, rx_pet, tx_res))
        .expect("hilo");
    let mut v = Vec::new();
    match rx_res.blocking_recv() {
        Some(m) if m["type"] == "ready" => v.push(m),
        Some(m) => {
            v.push(m);
            let _ = h.join();
            return v;
        }
        None => return v,
    }
    for b in bloques {
        if tx_pet.send(Peticion::Bloque(b.to_vec())).is_err() {
            break;
        }
        match rx_res.blocking_recv() {
            Some(m) if m["type"] == "error" => {
                v.push(m);
                let _ = h.join();
                return v;
            }
            Some(m) => v.push(m),
            None => break,
        }
    }
    if tx_pet.send(Peticion::Fin).is_ok() {
        if let Some(m) = rx_res.blocking_recv() {
            v.push(m);
        }
    }
    let _ = h.join();
    v
}
