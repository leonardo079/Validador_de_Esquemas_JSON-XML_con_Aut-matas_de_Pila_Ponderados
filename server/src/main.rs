//! Fase 7 — Servidor de la aplicación web: lee la configuración del entorno
//! ([`server::Config::from_env`]), escucha en `SERVER_ADDR` y sirve la API
//! hasta Ctrl-C (o SIGTERM en Unix, el aviso de `docker stop`), con apagado
//! ordenado: deja de aceptar conexiones y termina las peticiones en curso.

use std::future::Future;
use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let var = |k: &str| std::env::var_os(k).map(|v| v.to_string_lossy().into_owned());
    let (addr, cfg) = match server::Config::from_env(var) {
        Ok(c) => c,
        Err(e) => return fallar(&e),
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return fallar(&format!("no se pudo crear el runtime: {e}")),
    };
    rt.block_on(async move {
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => return fallar(&format!("no se pudo escuchar en {addr}: {e}")),
        };
        avisar(&format!(
            "escuchando en http://{addr} (subida ≤ {} bytes, {} ms, {} compilaciones a la vez, \
             IR ≤ {} bytes, cuerpos en vuelo ≤ {} bytes, lectura ≤ {} ms)",
            cfg.max_upload_bytes,
            cfg.compile_timeout.as_millis(),
            cfg.max_concurrent_compiles,
            cfg.max_ir_bytes,
            cfg.max_inflight_body_bytes,
            cfg.body_read_timeout.as_millis()
        ));
        // `axum::serve` arma su `hyper_util` `auto::Builder` sin temporizador,
        // así que no hay plazo de cabeceras ni tope de conexiones aquí: los
        // pone Caddy delante (F7.7, `docs/fase7/pendientes.md` §8). Una
        // conexión callada sin cabeceras ocupa ≈ 15 KB y ningún permiso; las
        // que mandan un cuerpo pasan por el presupuesto y el plazo de
        // lectura de `api.rs` (SEC-H1).
        match axum::serve(listener, server::router(cfg))
            .with_graceful_shutdown(apagado())
            .await
        {
            Ok(()) => {
                avisar("apagado");
                ExitCode::SUCCESS
            }
            Err(e) => fallar(&format!("error del servidor: {e}")),
        }
    })
}

/// Termina cuando llega Ctrl-C o, en Unix, SIGTERM. Si no se puede instalar
/// el manejador de una señal, esa nunca llega (no se apaga en el acto).
async fn apagado() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    // La primera de las dos (sin `tokio::select!`, que pide la feature
    // `macros` fuera de las pruebas).
    let (mut a, mut b) = (std::pin::pin!(ctrl_c), std::pin::pin!(term));
    std::future::poll_fn(|cx| {
        if a.as_mut().poll(cx).is_ready() || b.as_mut().poll(cx).is_ready() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
}

/// Una línea en stderr; si falla, se ignora (`eprintln!` haría pánico).
fn avisar(linea: &str) {
    let _ = writeln!(std::io::stderr(), "server: {linea}");
}

fn fallar(msg: &str) -> ExitCode {
    avisar(msg);
    ExitCode::from(2)
}
