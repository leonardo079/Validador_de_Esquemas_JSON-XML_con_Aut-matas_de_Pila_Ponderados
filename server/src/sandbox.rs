//! F7.3 — Sandbox Docker para compilar y ejecutar el código generado (D68;
//! `docs/fase7/sandbox.md`).
//!
//! Cada [`Trabajo`] corre en un contenedor nuevo de la imagen de `sandbox/`
//! (`validador-sandbox:1`), con:
//!
//! - **sin red** (`--network none`), **sin root** (`--user 10001:10001`),
//!   **raíz de solo lectura** (`--read-only`) y un tmpfs en `/tmp` como única
//!   zona escribible;
//! - **límites** de memoria (sin swap), CPU, procesos (`--pids-limit`),
//!   archivos abiertos y tamaño de archivo (`--ulimit`);
//! - **sin capacidades** (`--cap-drop ALL`) ni escalada
//!   (`no-new-privileges`);
//! - **tope de tiempo** doble: `timeout` dentro (compilación y ejecución) y,
//!   fuera, un plazo de pared tras el cual se hace `docker kill`.
//!
//! Los fuentes y el payload entran como un tar por la entrada estándar: no hay
//! montajes, así que funciona igual si el servidor corre en otro contenedor
//! (con el socket de Docker, F7.7). Las órdenes de compilación y ejecución son
//! fijas (`sandbox/sandbox-run.sh`); de fuera solo llegan el lenguaje, los
//! nombres de archivo (validados) y los bytes.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Imagen por defecto (`docker build -t validador-sandbox:1 sandbox`).
pub const IMAGEN: &str = "validador-sandbox:1";

/// Lenguaje del código generado: elige la orden fija de compilación.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lenguaje {
    /// `cc -std=c11 -O2 -Wall -Wextra *.c` (F5.2).
    C,
    /// `rustc --edition 2021 -O main.rs` (F5.1).
    Rust,
}

impl Lenguaje {
    fn arg(self) -> &'static str {
        match self {
            Lenguaje::C => "c",
            Lenguaje::Rust => "rust",
        }
    }
    fn extensiones(self) -> &'static [&'static str] {
        match self {
            Lenguaje::C => &["c", "h"],
            Lenguaje::Rust => &["rs"],
        }
    }
}

/// Límites de un trabajo. Los valores por defecto, en `docs/fase7/sandbox.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limites {
    /// Memoria del contenedor, sin swap (`--memory` = `--memory-swap`).
    pub memoria_mib: u32,
    /// CPU en milésimas de núcleo (`--cpus`; 1000 = un núcleo).
    pub cpu_milis: u32,
    /// Procesos e hilos a la vez (`--pids-limit`): corta una bomba de `fork`.
    pub procesos: u32,
    /// Tamaño del tmpfs de `/tmp` (fuentes, binario y temporales del compilador).
    pub tmpfs_mib: u32,
    /// Tope de la compilación (dentro del contenedor).
    pub compilar: Duration,
    /// Tope de la ejecución (dentro del contenedor).
    pub ejecutar: Duration,
    /// Holgura del plazo de pared sobre `compilar + ejecutar` (arranque del
    /// contenedor); al pasarlo, `docker kill`.
    pub holgura: Duration,
    /// Bytes que se guardan de `stdout` y de `stderr` cada uno; el resto se
    /// lee y se descarta.
    pub max_salida: usize,
    /// Tamaño máximo de la entrada (fuentes + payload).
    pub max_entrada: usize,
}

impl Default for Limites {
    fn default() -> Limites {
        Limites {
            memoria_mib: 512,
            cpu_milis: 1000,
            procesos: 64,
            tmpfs_mib: 256,
            compilar: Duration::from_secs(60),
            ejecutar: Duration::from_secs(10),
            holgura: Duration::from_secs(15),
            max_salida: 64 << 10,
            max_entrada: 16 << 20,
        }
    }
}

/// Lo que se compila y ejecuta.
#[derive(Clone, Copy, Debug)]
pub struct Trabajo<'a> {
    pub lenguaje: Lenguaje,
    /// `(nombre, contenido)`; nombres `[a-z0-9_]+` con extensión del lenguaje
    /// (`.c`/`.h` o `.rs`; en Rust, `main.rs` obligatorio).
    pub archivos: &'a [(&'a str, &'a [u8])],
    /// Entrada estándar del binario.
    pub payload: &'a [u8],
}

/// Resultado de un trabajo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resultado {
    /// El binario terminó solo (con cualquier código).
    Terminado {
        codigo: i32,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    /// No compiló; el diagnóstico (recortado).
    ErrorCompilacion { diagnostico: Vec<u8> },
    /// Pasó un tope de tiempo (de compilación, de ejecución o el de pared).
    TiempoAgotado { compilando: bool },
    /// Lo mató el núcleo (código 137): normalmente, el límite de memoria.
    Matado { stderr: Vec<u8> },
    /// El trabajo no es aceptable (nombres, tamaño) o Docker falló.
    Rechazado(String),
}

/// Configuración del ejecutor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sandbox {
    /// Programa de Docker (`docker`, o una ruta).
    pub docker: String,
    pub imagen: String,
    pub limites: Limites,
}

impl Default for Sandbox {
    fn default() -> Sandbox {
        Sandbox {
            docker: "docker".into(),
            imagen: IMAGEN.into(),
            limites: Limites::default(),
        }
    }
}

fn nombre_valido(n: &str, l: Lenguaje) -> bool {
    let Some((base, ext)) = n.rsplit_once('.') else {
        return false;
    };
    !base.is_empty()
        && base.len() <= 64
        && base
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && l.extensiones().contains(&ext)
}

/// Comprueba el trabajo antes de lanzar nada.
pub fn comprobar(t: &Trabajo<'_>, lim: &Limites) -> Result<(), String> {
    if t.archivos.is_empty() {
        return Err("no hay archivos".into());
    }
    for (i, (n, _)) in t.archivos.iter().enumerate() {
        if !nombre_valido(n, t.lenguaje) {
            return Err(format!("nombre de archivo no permitido: {n:?}"));
        }
        if t.archivos[..i].iter().any(|(m, _)| m == n) {
            return Err(format!("archivo repetido: {n:?}"));
        }
    }
    if t.lenguaje == Lenguaje::Rust && !t.archivos.iter().any(|(n, _)| *n == "main.rs") {
        return Err("falta main.rs".into());
    }
    let total: usize = t.archivos.iter().map(|(_, c)| c.len()).sum::<usize>() + t.payload.len();
    if total > lim.max_entrada {
        return Err(format!("la entrada pasa de {} bytes", lim.max_entrada));
    }
    Ok(())
}

/// Un tar ustar mínimo (POSIX.1-1988) con archivos regulares en la raíz.
pub fn tar(archivos: &[(&str, &[u8])]) -> Vec<u8> {
    let mut o = Vec::new();
    for (nombre, datos) in archivos {
        let mut h = [0u8; 512];
        h[..nombre.len().min(100)].copy_from_slice(&nombre.as_bytes()[..nombre.len().min(100)]);
        let octal = |h: &mut [u8], ini: usize, ancho: usize, v: u64| {
            let s = format!("{v:0w$o}\0", w = ancho - 1);
            h[ini..ini + ancho].copy_from_slice(s.as_bytes());
        };
        octal(&mut h, 100, 8, 0o644);
        octal(&mut h, 108, 8, 0);
        octal(&mut h, 116, 8, 0);
        octal(&mut h, 124, 12, datos.len() as u64);
        octal(&mut h, 136, 12, 0);
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let suma: u32 = h.iter().map(|&b| u32::from(b)).sum();
        let s = format!("{suma:06o}\0 ");
        h[148..156].copy_from_slice(s.as_bytes());
        o.extend_from_slice(&h);
        o.extend_from_slice(datos);
        o.resize(o.len().div_ceil(512) * 512, 0);
    }
    o.resize(o.len() + 1024, 0);
    o
}

/// Los argumentos de `docker run` (sin el programa), para el contenedor
/// `nombre`.
pub fn argumentos(s: &Sandbox, lenguaje: Lenguaje, nombre: &str) -> Vec<String> {
    let l = &s.limites;
    let mem = format!("{}m", l.memoria_mib);
    let segs = |d: Duration| d.as_secs().max(1).to_string();
    let mut a: Vec<String> = [
        "run",
        "--rm",
        "-i",
        "--name",
        nombre,
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--user",
        "10001:10001",
        "--ipc",
        "none",
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    a.extend([
        "--tmpfs".into(),
        format!("/tmp:rw,exec,nosuid,nodev,size={}m,mode=1777", l.tmpfs_mib),
        "--memory".into(),
        mem.clone(),
        "--memory-swap".into(),
        mem,
        "--cpus".into(),
        format!("{}.{:03}", l.cpu_milis / 1000, l.cpu_milis % 1000),
        "--pids-limit".into(),
        l.procesos.to_string(),
        "--ulimit".into(),
        "nofile=256:256".into(),
        "--ulimit".into(),
        format!("fsize={0}:{0}", u64::from(l.tmpfs_mib) << 20),
        "--ulimit".into(),
        "core=0:0".into(),
        s.imagen.clone(),
        lenguaje.arg().into(),
        segs(l.compilar),
        segs(l.ejecutar),
    ]);
    a
}

/// Lee `r` entero y guarda como mucho `max` bytes.
fn leer_acotado(mut r: impl Read, max: usize) -> Vec<u8> {
    let mut v = Vec::new();
    let mut buf = [0u8; 8192];
    while let Ok(n) = r.read(&mut buf) {
        if n == 0 {
            break;
        }
        let cabe = max.saturating_sub(v.len()).min(n);
        v.extend_from_slice(&buf[..cabe]);
    }
    v
}

static CONTADOR: AtomicU64 = AtomicU64::new(0);

/// Compila y ejecuta `t` en un contenedor nuevo. Bloquea hasta terminar (en
/// el servidor, desde `spawn_blocking`).
pub fn ejecutar(s: &Sandbox, t: &Trabajo<'_>) -> Resultado {
    if let Err(e) = comprobar(t, &s.limites) {
        return Resultado::Rechazado(e);
    }
    let mut entrada: Vec<(&str, &[u8])> = t.archivos.to_vec();
    entrada.push(("payload", t.payload));
    let tar = tar(&entrada);
    let nombre = format!(
        "sv-{}-{}",
        std::process::id(),
        CONTADOR.fetch_add(1, Ordering::Relaxed)
    );
    let mut hijo = match Command::new(&s.docker)
        .args(argumentos(s, t.lenguaje, &nombre))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(h) => h,
        Err(e) => return Resultado::Rechazado(format!("no se pudo lanzar Docker: {e}")),
    };
    let max = s.limites.max_salida;
    let (Some(mut stdin), Some(out), Some(err)) =
        (hijo.stdin.take(), hijo.stdout.take(), hijo.stderr.take())
    else {
        let _ = hijo.kill();
        return Resultado::Rechazado("sin tuberías hacia Docker".into());
    };
    let escritor = std::thread::spawn(move || {
        let _ = stdin.write_all(&tar);
    });
    let lector_out = std::thread::spawn(move || leer_acotado(out, max));
    let lector_err = std::thread::spawn(move || leer_acotado(err, max));

    let plazo = s.limites.compilar + s.limites.ejecutar + s.limites.holgura;
    let inicio = Instant::now();
    let estado = loop {
        match hijo.try_wait() {
            Ok(Some(e)) => break Some(e),
            Ok(None) if inicio.elapsed() >= plazo => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Resultado::Rechazado(format!("esperando a Docker: {e}")),
        }
    };
    let Some(estado) = estado else {
        // Plazo de pared: matar el contenedor (el cliente `docker` solo no
        // basta) y después el cliente.
        let _ = Command::new(&s.docker)
            .args(["kill", &nombre])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = hijo.kill();
        let _ = hijo.wait();
        return Resultado::TiempoAgotado { compilando: false };
    };
    let _ = escritor.join();
    let stdout = lector_out.join().unwrap_or_default();
    let stderr = lector_err.join().unwrap_or_default();
    match estado.code() {
        Some(100) => Resultado::ErrorCompilacion {
            diagnostico: stderr,
        },
        Some(101) => Resultado::Rechazado("el sandbox no pudo leer la entrada".into()),
        Some(102) => Resultado::Rechazado("argumentos del sandbox no válidos".into()),
        Some(103) => Resultado::TiempoAgotado { compilando: true },
        Some(124) => Resultado::TiempoAgotado { compilando: false },
        Some(137) => Resultado::Matado { stderr },
        // 125-127: el propio `docker run` falló (imagen, demonio…).
        Some(c @ 125..=127) => Resultado::Rechazado(format!(
            "docker run falló ({c}): {}",
            String::from_utf8_lossy(&stderr).trim()
        )),
        Some(codigo) => Resultado::Terminado {
            codigo,
            stdout,
            stderr,
        },
        None => Resultado::Matado { stderr },
    }
}
