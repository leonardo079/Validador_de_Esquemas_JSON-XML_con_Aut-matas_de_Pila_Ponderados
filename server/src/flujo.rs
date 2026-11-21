//! F7.5 — `WS /ws/validate`: validación por bloques en vivo (D81;
//! `docs/fase7/codigo.md` §3).
//!
//! Protocolo (mensajes de texto JSON del servidor):
//!
//! 1. El cliente manda el **esquema** como primer mensaje de texto. El
//!    servidor responde `{"type":"ready","format","states","transitions",
//!    "memoryBytes"}` o `{"type":"error","message"}` y cierra.
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
//! payload. Corre en un hilo propio con pila explícita (D7); cada sesión
//! tiene un permiso propio (como mucho [`MAX_FLUJOS`] a la vez), un tope de
//! bytes y un plazo de inactividad.

use std::sync::mpsc;
use std::time::Duration;

use automaton::{Mode, Options, PayloadFormat};
use axum::extract::ws::{Message, WebSocket};
use runtime_rs::{Flow, JsonValidator, XmlValidator};
use serde_json::{json, Value};
use tokio::sync::OwnedSemaphorePermit;

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

/// El hilo del validador: construye M y luego atiende bloques hasta `Fin`.
fn hilo(esquema: String, rx: mpsc::Receiver<Peticion>, tx: tokio::sync::mpsc::Sender<Value>) {
    let a = match construir(esquema.as_bytes()) {
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
            let _ = tx.blocking_send(json!({"type": "error", "message": describir(&f)}));
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
    while let Ok(p) = rx.recv() {
        match p {
            Peticion::Bloque(b) => {
                bytes += b.len() as u64;
                bloques += 1;
                if !parado {
                    let flujo = match &mut v {
                        V::J(j) => j.feed(&b),
                        V::X(x) => x.feed(&b),
                    };
                    parado = flujo == Flow::Stop;
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

async fn error(s: &mut WebSocket, m: &str) {
    let _ = enviar(s, json!({"type": "error", "message": m})).await;
    let _ = s.send(Message::Close(None)).await;
}

/// Una sesión. `permiso` se suelta al terminar.
pub async fn sesion(mut s: WebSocket, permiso: OwnedSemaphorePermit) {
    let _permiso = permiso;
    let esquema = match tokio::time::timeout(INACTIVIDAD, s.recv()).await {
        Ok(Some(Ok(Message::Text(t)))) => t.to_string(),
        _ => return error(&mut s, "el primer mensaje debe ser el esquema (texto)").await,
    };
    let (tx_pet, rx_pet) = mpsc::channel::<Peticion>();
    let (tx_res, mut rx_res) = tokio::sync::mpsc::channel::<Value>(2);
    let hilo = std::thread::Builder::new()
        .name("server-flujo".into())
        .stack_size(crate::STACK_SIZE)
        .spawn(move || hilo(esquema, rx_pet, tx_res));
    if hilo.is_err() {
        return error(&mut s, "no se pudo crear el hilo de validación").await;
    }
    // ready o error.
    match rx_res.recv().await {
        Some(v) if v["type"] == "ready" => {
            if !enviar(&mut s, v).await {
                return;
            }
        }
        Some(v) => {
            let _ = enviar(&mut s, v).await;
            let _ = s.send(Message::Close(None)).await;
            return;
        }
        None => return error(&mut s, "error interno del compilador").await,
    }
    let mut total = 0u64;
    loop {
        let m = match tokio::time::timeout(INACTIVIDAD, s.recv()).await {
            Err(_) => return error(&mut s, "sin datos durante 30 s").await,
            Ok(None) | Ok(Some(Err(_))) => return,
            Ok(Some(Ok(m))) => m,
        };
        let fin = match m {
            Message::Binary(b) => {
                total += b.len() as u64;
                if total > MAX_TOTAL {
                    return error(&mut s, "el payload supera 64 MiB").await;
                }
                if tx_pet.send(Peticion::Bloque(b.to_vec())).is_err() {
                    return error(&mut s, "el validador terminó").await;
                }
                false
            }
            Message::Text(t) if t.as_str() == "fin" => {
                if tx_pet.send(Peticion::Fin).is_err() {
                    return error(&mut s, "el validador terminó").await;
                }
                true
            }
            Message::Text(_) => {
                return error(&mut s, "mensaje de texto desconocido (se espera `fin`)").await
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(_) => return,
        };
        match rx_res.recv().await {
            Some(v) => {
                if !enviar(&mut s, v).await {
                    return;
                }
            }
            None => return error(&mut s, "el validador terminó").await,
        }
        if fin {
            let _ = s.send(Message::Close(None)).await;
            return;
        }
    }
}

/// La misma conversación que [`sesion`] sin red, para las pruebas: el
/// esquema, los bloques y `fin` pasan por el hilo real del validador; devuelve
/// los mensajes del servidor en orden (`ready` o `error`, un `progress` por
/// bloque y el `report`).
pub fn conversar(esquema: &str, bloques: &[&[u8]]) -> Vec<Value> {
    let (tx_pet, rx_pet) = mpsc::channel::<Peticion>();
    let (tx_res, mut rx_res) = tokio::sync::mpsc::channel::<Value>(2);
    let e = esquema.to_owned();
    let h = std::thread::Builder::new()
        .stack_size(crate::STACK_SIZE)
        .spawn(move || hilo(e, rx_pet, tx_res))
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
        if let Some(m) = rx_res.blocking_recv() {
            v.push(m);
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
