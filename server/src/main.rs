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
        // F7.2 (D72): los casos de Atlas, solo si hay `MONGODB_URI`. El
        // cliente no conecta hasta la primera consulta; una URI mal formada
        // deja el servidor sin base (503 en `/api/tests`), nunca lo tumba.
        let casos = match std::env::var("MONGODB_URI")
            .ok()
            .filter(|u| !u.trim().is_empty())
        {
            None => {
                avisar("sin MONGODB_URI: /api/tests responde 503");
                None
            }
            Some(uri) => match atlas::coleccion(uri.trim()).await {
                Ok(c) => {
                    avisar("casos de prueba: MongoDB Atlas configurado");
                    Some(c)
                }
                Err(e) => {
                    avisar(&format!(
                        "MONGODB_URI no válida ({e}); /api/tests responde 503"
                    ));
                    None
                }
            },
        };
        // F7.5 (D81): el sandbox Docker de F7.3 para `POST /api/validate`,
        // solo con `SANDBOX=docker` (en el despliegue hace falta el socket de
        // Docker, `docs/fase7/sandbox.md` §5).
        let sandbox = match std::env::var("SANDBOX").ok().as_deref() {
            Some("docker") => {
                let mut s = server::sandbox::Sandbox::default();
                // E3C-4: compilar + ejecutar + holgura = 55 s, por debajo del
                // `write 60s` de Caddy (si no, el 504 no llega nunca).
                s.limites.compilar = std::time::Duration::from_secs(40);
                s.limites.holgura = std::time::Duration::from_secs(5);
                if let Ok(img) = std::env::var("SANDBOX_IMAGE") {
                    s.imagen = img;
                }
                avisar(&format!("sandbox: Docker con la imagen {}", s.imagen));
                Some(s)
            }
            _ => {
                avisar("sin SANDBOX=docker: /api/validate responde 503");
                None
            }
        };
        match axum::serve(listener, server::router_completo(cfg, casos, sandbox))
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
