//! Entrada y salida de `schemac`: lectura acotada del esquema (tope de
//! 16 MiB, `docs/00-alcance.md` §4; solo archivos regulares) y escritura en
//! stdout sin pánico (tubería cerrada: código 0 en silencio; otro error:
//! mensaje y código 2), en `check`, `ir` y `dot`.

use std::path::PathBuf;
use std::process::Command;
// Solo lo usan las pruebas de Unix: en Windows sería un `use` sin usar, que
// la CI rechaza con `RUSTFLAGS=-D warnings`.
#[cfg(unix)]
use std::process::Stdio;

/// Tope de tamaño del esquema de `schemac` (`MAX_SCHEMA_BYTES`).
const TOPE: usize = 16 << 20;

fn schemac(args: &[&std::ffi::OsStr]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_schemac"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// Directorio temporal propio de la prueba (se borra al final).
fn dir_temporal(nombre: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("schemac-es-{nombre}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `{"$comment": "aaa…"}` de exactamente `n` bytes.
fn esquema_de(n: usize) -> Vec<u8> {
    let marco = br#"{"$comment": ""}"#.len();
    let mut v = br#"{"$comment": ""#.to_vec();
    v.resize(v.len() + (n - marco), b'a');
    v.extend_from_slice(br#""}"#);
    assert_eq!(v.len(), n);
    v
}

/// Un esquema de tope + 1 bytes da 2 en `check` y en `ir`, sin leerlo
/// entero; uno de exactamente el tope se acepta.
#[test]
fn esquema_mayor_que_el_tope_da_codigo_2() {
    let d = dir_temporal("tope");
    let grande = d.join("grande.json");
    let justo = d.join("justo.json");
    std::fs::write(&grande, esquema_de(TOPE + 1)).unwrap();
    std::fs::write(&justo, esquema_de(TOPE)).unwrap();
    let mut res = Vec::new();
    for cmd in ["check", "ir"] {
        res.push((cmd, schemac(&[cmd.as_ref(), grande.as_os_str()])));
        res.push((cmd, schemac(&[cmd.as_ref(), justo.as_os_str()])));
    }
    let _ = std::fs::remove_dir_all(&d);
    for (cmd, pareja) in res.chunks(2).map(|c| (c[0].0, (&c[0].1, &c[1].1))) {
        let ((code, out, err), (code_justo, _, err_justo)) = pareja;
        assert_eq!(*code, 2, "{cmd}: {err}");
        assert!(out.is_empty(), "{cmd}");
        assert!(
            err.contains("tamaño máximo de 16777216 bytes"),
            "{cmd}: {err}"
        );
        assert_eq!(*code_justo, 0, "{cmd}: {err_justo}");
    }
}

/// Un directorio no es un esquema: 2 en `check` y en `ir`.
#[test]
fn directorio_da_codigo_2() {
    let d = dir_temporal("dir");
    let r: Vec<_> = ["check", "ir"]
        .iter()
        .map(|cmd| schemac(&[cmd.as_ref(), d.as_os_str()]))
        .collect();
    let _ = std::fs::remove_dir_all(&d);
    for (code, out, err) in r {
        assert_eq!(code, 2, "{err}");
        assert!(out.is_empty());
        assert!(
            err.starts_with("schemac: ") && !err.contains("panicked"),
            "{err}"
        );
    }
}

/// Solo la usa la prueba de Linux (`/dev/full`).
#[cfg(target_os = "linux")]
fn esquema_valido() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/schemas/parse-ok/usuarios.json")
}

/// stdout que falla al escribir (`/dev/full`, solo Linux): mensaje en
/// stderr y código 2 en los tres comandos, sin pánico.
#[cfg(target_os = "linux")]
#[test]
fn stdout_lleno_da_codigo_2() {
    let p = esquema_valido();
    for args in [
        vec!["ir".as_ref(), p.as_os_str()],
        vec!["check".as_ref(), p.as_os_str()],
        vec!["dot".as_ref(), "json".as_ref()],
    ] {
        let lleno = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_schemac"))
            .args(&args)
            .stdout(lleno)
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        let err = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}: {err}");
        assert!(
            err.starts_with("schemac: no se pudo escribir la salida"),
            "{err}"
        );
        assert!(!err.contains("panicked"), "{err}");
    }
}

/// Tubería cerrada por el lector (`schemac ir … | head -c 10`): salida
/// silenciosa con código 0, la convención de Unix. La salida (varios MB)
/// es mucho mayor que el búfer de la tubería, así que la escritura llega a
/// fallar con `BrokenPipe` aunque el proceso empiece a escribir antes de
/// que se cierre: nunca se bloquea, porque el lector ya no existe.
#[cfg(unix)]
#[test]
fn tuberia_cerrada_da_codigo_0() {
    let d = dir_temporal("tuberia");
    let p = d.join("allof.json");
    let mut src = String::from(r#"{"allOf": [{}"#);
    src.push_str(&",{}".repeat(20_000));
    src.push_str("]}");
    std::fs::write(&p, src).unwrap();
    for args in [
        vec!["ir".as_ref(), p.as_os_str()],
        vec!["ir".as_ref(), "--dot".as_ref(), p.as_os_str()],
    ] {
        let mut hijo = Command::new(env!("CARGO_BIN_EXE_schemac"))
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(hijo.stdout.take());
        let out = hijo.wait_with_output().unwrap();
        let err = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(0), "{args:?}: {err}");
        assert!(err.is_empty(), "{args:?}: {err}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// 4.2-L1 (`docs/fase3/pendientes.md` §19): una FIFO sin escritor se
/// rechaza como «no es un archivo regular» con código 2, sin abrirla (abrirla
/// bloquearía esperando a un escritor). La FIFO se crea con `mkfifo`; si
/// `schemac` no termina en 60 s se mata y la prueba falla (es una guarda
/// contra el bloqueo, no una medición de tiempo, D1).
#[cfg(unix)]
#[test]
fn fifo_sin_escritor_no_bloquea() {
    let d = dir_temporal("fifo");
    let fifo = d.join("esquema.json");
    let ok = Command::new("mkfifo").arg(&fifo).status().unwrap();
    assert!(ok.success(), "mkfifo falló");
    let mut res = Vec::new();
    for cmd in ["check", "ir"] {
        let mut hijo = Command::new(env!("CARGO_BIN_EXE_schemac"))
            .args([cmd.as_ref(), fifo.as_os_str()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while hijo.try_wait().unwrap().is_none() && std::time::Instant::now() < limite {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let colgado = hijo.try_wait().unwrap().is_none();
        if colgado {
            let _ = hijo.kill();
        }
        let out = hijo.wait_with_output().unwrap();
        res.push((cmd, colgado, out));
    }
    let _ = std::fs::remove_dir_all(&d);
    for (cmd, colgado, out) in res {
        assert!(!colgado, "{cmd}: schemac se bloqueó al abrir la FIFO");
        let err = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(2), "{cmd}: {err}");
        assert!(out.stdout.is_empty(), "{cmd}");
        assert!(err.contains("no es un archivo regular"), "{cmd}: {err}");
    }
}
