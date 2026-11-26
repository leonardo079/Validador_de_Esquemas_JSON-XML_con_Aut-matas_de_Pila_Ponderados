//! Medición en vivo para el panel de comparación (`POST /api/compare`):
//! tiempo, MB/s, ns/byte, **pico de memoria y asignaciones** del DPDA y de la
//! línea base DOM sobre el payload de la petición (enunciado, objetivo 3).
//!
//! La mide el binario `medir` (`crates/bench/src/bin/medir.rs`), que usa
//! `dhat` como asignador global. Corre como **proceso aparte**: cada
//! medición tiene su propio montón, sin mezclarse con otras peticiones, y el
//! servidor no necesita un asignador global ni `unsafe` (D52). Se busca en
//! `MEDIR` (variable de entorno) o junto al ejecutable del servidor. Si no
//! está, o tarda más de [`PLAZO`], la respuesta lleva `{"error": …}` y el
//! resto del panel sigue igual.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Tope de la medición: menos que el plazo de la petición (10 s por defecto,
/// `docs/fase7/servidor.md` §3), para que su error llegue antes que el 504.
pub const PLAZO: Duration = Duration::from_secs(7);

/// Salida máxima que se lee del proceso (una línea JSON pequeña).
const MAX_SALIDA: u64 = 64 << 10;

/// Ruta del binario `medir`, si existe.
pub fn binario() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MEDIR") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let nombre = format!("medir{}", std::env::consts::EXE_SUFFIX);
    // Junto al servidor (imagen Docker: /usr/local/bin; `cargo run`:
    // target/<perfil>/) o un nivel arriba (las pruebas: target/<perfil>/deps).
    exe.ancestors()
        .skip(1)
        .take(2)
        .map(|d| d.join(&nombre))
        .find(|p| p.is_file())
}

fn error(m: impl Into<String>) -> Value {
    json!({ "error": m.into() })
}

/// Mide `payload` contra `esquema` con los dos validadores.
pub fn medir(esquema: &str, payload: &str) -> Value {
    let Some(bin) = binario() else {
        return error("medición en vivo no disponible: falta el binario `medir`");
    };
    let entrada = json!({ "schema": esquema, "payload": payload }).to_string();
    let mut hijo = match Command::new(&bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(h) => h,
        Err(e) => return error(format!("no se pudo lanzar `medir`: {e}")),
    };
    // La entrada se escribe en otro hilo: si el proceso no la leyera, el
    // tope de tiempo de abajo lo mataría igual.
    let Some(mut stdin) = hijo.stdin.take() else {
        return error("sin entrada estándar");
    };
    let escritor = std::thread::spawn(move || {
        let _ = stdin.write_all(entrada.as_bytes());
    });
    let Some(stdout) = hijo.stdout.take() else {
        return error("sin salida estándar");
    };
    let lector = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.take(MAX_SALIDA).read_to_string(&mut s);
        s
    });
    let inicio = Instant::now();
    loop {
        match hijo.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if inicio.elapsed() < PLAZO => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                return error(format!("la medición pasó de {} s", PLAZO.as_secs()));
            }
        }
    }
    let _ = escritor.join();
    let salida = lector.join().unwrap_or_default();
    serde_json::from_str(salida.trim()).unwrap_or_else(|_| error("la medición no devolvió JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sin binario, la respuesta es un error descriptivo, nunca un pánico.
    #[test]
    fn sin_binario_devuelve_un_error() {
        if binario().is_some() {
            return; // con el binario compilado, lo prueba `tests/vistas.rs`
        }
        let v = medir("{}", "1");
        assert!(v["error"].as_str().unwrap().contains("medir"), "{v}");
    }
}
