//! Fase 7 — Servidor de la aplicación web (axum). F7.1: `POST /api/schemas` y
//! `GET /api/health` (`docs/fase7/servidor.md`).
//!
//! [`router`] arma la API con una [`Config`]; `main.rs` la lee del entorno
//! ([`Config::from_env`]) y la sirve. Cada compilación corre en un hilo propio
//! con 8 MiB de pila (D7) y sus topes de subida, tiempo, concurrencia, errores
//! devueltos y tamaño de la IR (`docs/fase3/pendientes.md` §1-§2).

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

mod api;
pub mod casos;
pub mod compilar;
pub mod flujo;
pub mod generado;
pub mod medir;
pub mod sandbox;
pub mod vistas;
pub mod zip;

pub use api::{router, router_completo, router_con, router_con_casos, Compilador};

/// Pila del hilo de cada compilación: el parser es recursivo y su tope de
/// profundidad (256) está garantizado con ≥ 2 MiB (D7); el mismo valor que
/// `schemac`. [`Config::stack_size`] nunca baja de aquí.
pub const STACK_SIZE: usize = 8 << 20;

/// Topes y dirección del servidor. Los valores por defecto y su porqué, en
/// `docs/fase7/servidor.md` §3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Tamaño máximo del cuerpo de `POST /api/schemas` (U3; 1 MiB). Una
    /// compilación usa ≈ 165 veces la subida.
    pub max_upload_bytes: usize,
    /// Tiempo máximo de espera de una compilación (10 s); al pasarlo, 504.
    pub compile_timeout: Duration,
    /// Compilaciones a la vez (2); con todas ocupadas, 503 sin cola.
    pub max_concurrent_compiles: usize,
    /// Tamaño máximo del JSON de la IR en la respuesta (U8; 4 MiB); si no
    /// cabe, `ir: null` e `irOmitted: true`.
    pub max_ir_bytes: usize,
    /// Presupuesto global de bytes de cuerpos que se están leyendo a la vez
    /// (SEC-H1; 32 MiB). Cada lectura reserva antes de empezar lo que puede
    /// llegar a ocupar (`min(Content-Length, max_upload_bytes)`, o
    /// `max_upload_bytes` sin `Content-Length`), en bloques de 1 KiB; si no
    /// cabe, 503. Se usa `max(max_inflight_body_bytes, max_upload_bytes)`.
    pub max_inflight_body_bytes: usize,
    /// Plazo para leer el cuerpo entero (SEC-H1; 10 s); al pasarlo, 408.
    pub body_read_timeout: Duration,
    /// Pila del hilo de compilación; se usa `max(stack_size, STACK_SIZE)`.
    /// No se lee del entorno: solo existe para que una prueba haga fallar la
    /// creación del hilo.
    pub stack_size: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_upload_bytes: 1 << 20,
            compile_timeout: Duration::from_secs(10),
            max_concurrent_compiles: 2,
            max_ir_bytes: 4 << 20,
            max_inflight_body_bytes: 32 << 20,
            body_read_timeout: Duration::from_secs(10),
            stack_size: STACK_SIZE,
        }
    }
}

/// Dirección por defecto: solo la máquina local. En el contenedor de F7.7,
/// `SERVER_ADDR=0.0.0.0:8080` (U7).
pub const SERVER_ADDR: &str = "127.0.0.1:8080";

/// Máximo de `MAX_UPLOAD_BYTES`: el tope de `schemac` (16 MiB,
/// `docs/00-alcance.md` §4). Más no lo compila ni la CLI, y con ≈ 165 veces
/// la subida ya serían ≈ 2,7 GB por compilación.
pub const MAX_UPLOAD_LIMIT: usize = 16 << 20;

/// Máximo de `COMPILE_TIMEOUT_MS`: 10 minutos, de sobra para el peor
/// esquema de 16 MiB (≈ 7-28 s, `docs/fase3/pendientes.md` §38).
pub const MAX_TIMEOUT_MS: u64 = 600_000;

/// Máximo de `MAX_CONCURRENT_COMPILES`: cada compilación es un hilo con
/// 8 MiB de pila y hasta ≈ 165 veces la subida de memoria; 64 ya es mucho
/// más de lo que cabe en la máquina de F7.7.
pub const MAX_CONCURRENT_LIMIT: usize = 64;

/// Máximo de `MAX_INFLIGHT_BODY_BYTES`: 1 GiB, el `mem_limit` que propone
/// F7.7 para el contenedor entero.
pub const MAX_INFLIGHT_LIMIT: usize = 1 << 30;

impl Config {
    /// La dirección y la configuración de las variables `SERVER_ADDR`,
    /// `MAX_UPLOAD_BYTES` (1 a [`MAX_UPLOAD_LIMIT`]), `COMPILE_TIMEOUT_MS`
    /// (1 a [`MAX_TIMEOUT_MS`]), `MAX_CONCURRENT_COMPILES` (1 a
    /// [`MAX_CONCURRENT_LIMIT`]), `MAX_IR_BYTES` (cualquier `usize`, 0
    /// incluido), `MAX_INFLIGHT_BODY_BYTES` (de `MAX_UPLOAD_BYTES` a
    /// [`MAX_INFLIGHT_LIMIT`]) y `BODY_READ_TIMEOUT_MS` (1 a
    /// [`MAX_TIMEOUT_MS`]), leídas con `var` (en el programa, el entorno; en
    /// las pruebas, una tabla). Una variable ausente toma su valor por
    /// defecto; una que no se entiende o se sale de su rango es un error con
    /// su nombre, y el servidor no arranca.
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Result<(SocketAddr, Config), String> {
        let d = Config::default();
        let addr = leer(&var, "SERVER_ADDR", SERVER_ADDR.parse().ok(), |_| true)?;
        let max_upload_bytes = leer(&var, "MAX_UPLOAD_BYTES", Some(d.max_upload_bytes), |&n| {
            (1..=MAX_UPLOAD_LIMIT).contains(&n)
        })?;
        let ms = leer(
            &var,
            "COMPILE_TIMEOUT_MS",
            u64::try_from(d.compile_timeout.as_millis()).ok(),
            |&n| (1..=MAX_TIMEOUT_MS).contains(&n),
        )?;
        let max_concurrent_compiles = leer(
            &var,
            "MAX_CONCURRENT_COMPILES",
            Some(d.max_concurrent_compiles),
            |&n| (1..=MAX_CONCURRENT_LIMIT).contains(&n),
        )?;
        let max_ir_bytes = leer(&var, "MAX_IR_BYTES", Some(d.max_ir_bytes), |_| true)?;
        let max_inflight_body_bytes = leer(
            &var,
            "MAX_INFLIGHT_BODY_BYTES",
            Some(d.max_inflight_body_bytes.max(max_upload_bytes)),
            |&n| (max_upload_bytes..=MAX_INFLIGHT_LIMIT).contains(&n),
        )?;
        let body_ms = leer(
            &var,
            "BODY_READ_TIMEOUT_MS",
            u64::try_from(d.body_read_timeout.as_millis()).ok(),
            |&n| (1..=MAX_TIMEOUT_MS).contains(&n),
        )?;
        Ok((
            addr,
            Config {
                max_upload_bytes,
                compile_timeout: Duration::from_millis(ms),
                max_concurrent_compiles,
                max_ir_bytes,
                max_inflight_body_bytes,
                body_read_timeout: Duration::from_millis(body_ms),
                stack_size: d.stack_size,
            },
        ))
    }
}

/// Lee la variable `nombre`: `defecto` si no está; error si no se entiende
/// o no cumple `valido`.
fn leer<T: FromStr>(
    var: &impl Fn(&str) -> Option<String>,
    nombre: &str,
    defecto: Option<T>,
    valido: impl Fn(&T) -> bool,
) -> Result<T, String> {
    let error = |v: &str| format!("variable de entorno {nombre} inválida: {v:?}");
    match var(nombre) {
        None => defecto.ok_or_else(|| error("")),
        Some(v) => match v.trim().parse::<T>() {
            Ok(n) if valido(&n) => Ok(n),
            _ => Err(error(&v)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn con(pares: &[(&str, &str)]) -> Result<(SocketAddr, Config), String> {
        let pares: Vec<(String, String)> = pares
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_env(move |k| pares.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()))
    }

    #[test]
    fn sin_variables_toma_los_valores_por_defecto() {
        let (addr, cfg) = con(&[]).unwrap();
        assert_eq!(addr, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(cfg, Config::default());
        assert_eq!(cfg.max_upload_bytes, 1 << 20);
        assert_eq!(cfg.compile_timeout, Duration::from_secs(10));
        assert_eq!(cfg.max_concurrent_compiles, 2);
        assert_eq!(cfg.max_ir_bytes, 4 << 20);
        assert_eq!(cfg.max_inflight_body_bytes, 32 << 20);
        assert_eq!(cfg.body_read_timeout, Duration::from_secs(10));
    }

    #[test]
    fn lee_cada_variable() {
        let (addr, cfg) = con(&[
            ("SERVER_ADDR", "0.0.0.0:9000"),
            ("MAX_UPLOAD_BYTES", "2048"),
            ("COMPILE_TIMEOUT_MS", "500"),
            ("MAX_CONCURRENT_COMPILES", "4"),
            ("MAX_IR_BYTES", "0"),
        ])
        .unwrap();
        assert_eq!(addr, "0.0.0.0:9000".parse().unwrap());
        assert_eq!(cfg.max_upload_bytes, 2048);
        assert_eq!(cfg.compile_timeout, Duration::from_millis(500));
        assert_eq!(cfg.max_concurrent_compiles, 4);
        assert_eq!(cfg.max_ir_bytes, 0);
    }

    #[test]
    fn un_valor_invalido_es_un_error_con_su_nombre() {
        for (k, v) in [
            ("SERVER_ADDR", "localhost"),
            ("MAX_UPLOAD_BYTES", "0"),
            ("MAX_UPLOAD_BYTES", "1MiB"),
            ("COMPILE_TIMEOUT_MS", "0"),
            ("COMPILE_TIMEOUT_MS", "-5"),
            ("MAX_CONCURRENT_COMPILES", "0"),
            ("MAX_IR_BYTES", "x"),
            // Por encima de los máximos.
            ("MAX_UPLOAD_BYTES", "16777217"),
            ("COMPILE_TIMEOUT_MS", "600001"),
            ("COMPILE_TIMEOUT_MS", "18446744073709551615"),
            ("MAX_CONCURRENT_COMPILES", "65"),
            ("MAX_CONCURRENT_COMPILES", "99999999999999"),
            ("MAX_INFLIGHT_BODY_BYTES", "1073741825"),
            ("MAX_INFLIGHT_BODY_BYTES", "1048575"),
            ("BODY_READ_TIMEOUT_MS", "0"),
            ("BODY_READ_TIMEOUT_MS", "600001"),
        ] {
            let e = con(&[(k, v)]).unwrap_err();
            assert!(e.contains(k), "{e}");
        }
    }

    /// Los máximos mismos son válidos.
    #[test]
    fn los_maximos_son_validos() {
        let (_, cfg) = con(&[
            ("MAX_UPLOAD_BYTES", "16777216"),
            ("COMPILE_TIMEOUT_MS", "600000"),
            ("MAX_CONCURRENT_COMPILES", "64"),
            ("MAX_INFLIGHT_BODY_BYTES", "1073741824"),
            ("BODY_READ_TIMEOUT_MS", "600000"),
        ])
        .unwrap();
        assert_eq!(cfg.max_upload_bytes, MAX_UPLOAD_LIMIT);
        assert_eq!(cfg.compile_timeout, Duration::from_millis(MAX_TIMEOUT_MS));
        assert_eq!(cfg.max_concurrent_compiles, MAX_CONCURRENT_LIMIT);
        assert_eq!(cfg.max_inflight_body_bytes, MAX_INFLIGHT_LIMIT);
        assert_eq!(cfg.body_read_timeout, Duration::from_millis(MAX_TIMEOUT_MS));
    }
}
