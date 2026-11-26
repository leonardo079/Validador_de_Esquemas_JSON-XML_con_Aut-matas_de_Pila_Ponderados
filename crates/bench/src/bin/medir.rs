//! Medición en vivo de un solo payload, para el panel de comparación de la
//! web (`POST /api/compare`; enunciado, objetivo 3: «medir en vivo
//! rendimiento, memoria y asignaciones frente a validadores basados en AST»).
//!
//! ```text
//! echo '{"schema": "…", "payload": "…"}' | medir
//! ```
//!
//! Lee el esquema y el payload de la entrada estándar y escribe una línea
//! JSON con lo que mide `metricas` (F6.5) para cada validador: mediana del
//! tiempo de validar, MB/s, ns/byte, **asignaciones y pico de montón** (con
//! `dhat` como asignador global) y la profundidad del payload. La línea base
//! (`serde_json` + `jsonschema`, D48) solo existe para JSON.
//!
//! Corre como **proceso aparte**: cada medición tiene su propio montón, así
//! que no se mezcla con otras peticiones del servidor, y el servidor no
//! necesita un asignador global ni `unsafe` (D52). El servidor lo lanza con
//! un tope de tiempo (`server::medir`).

use std::io::Read;
use std::time::{Duration, Instant};

use automaton::PayloadFormat;
use bench::{automata, profundidad, validar_dpda, validar_linea_base, Veredicto};
use serde_json::{json, Value};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Presupuesto de tiempo de las repeticiones de cada validador.
const PRESUPUESTO: Duration = Duration::from_millis(400);
const MAX_REPETICIONES: usize = 15;

/// Mediana de varias ejecuciones (al menos 3, como mucho
/// [`MAX_REPETICIONES`], mientras quepan en [`PRESUPUESTO`]), en segundos.
fn tiempo(mut f: impl FnMut() -> Veredicto) -> (f64, Veredicto, usize) {
    let mut v = f(); // calentamiento
    let inicio = Instant::now();
    let mut t = Vec::new();
    while t.len() < 3 || (t.len() < MAX_REPETICIONES && inicio.elapsed() < PRESUPUESTO) {
        let i = Instant::now();
        v = f();
        t.push(i.elapsed().as_secs_f64());
    }
    t.sort_by(f64::total_cmp);
    (t[t.len() / 2], v, t.len())
}

/// Asignaciones y pico de montón de una ejecución de `f`.
fn memoria(f: impl FnOnce() -> Veredicto) -> (u64, usize) {
    let _p = dhat::Profiler::builder().testing().build();
    std::hint::black_box(f());
    let s = dhat::HeapStats::get();
    (s.total_blocks, s.max_bytes)
}

fn fila(
    bytes: usize,
    segundos: f64,
    v: Veredicto,
    rep: usize,
    asignaciones: u64,
    pico: usize,
) -> Value {
    let mb_s = if segundos > 0.0 {
        bytes as f64 / segundos / 1e6
    } else {
        0.0
    };
    json!({
        "valido": v.valido,
        "violaciones": v.violaciones,
        "segundos": segundos,
        "mbPorSegundo": mb_s,
        "nsPorByte": if bytes > 0 { segundos * 1e9 / bytes as f64 } else { 0.0 },
        "asignaciones": asignaciones,
        "picoBytes": pico,
        "repeticiones": rep,
    })
}

fn principal() -> Result<Value, String> {
    let mut entrada = Vec::new();
    std::io::stdin()
        .read_to_end(&mut entrada)
        .map_err(|e| format!("no se pudo leer la entrada: {e}"))?;
    let v: Value =
        serde_json::from_slice(&entrada).map_err(|e| format!("entrada no es JSON: {e}"))?;
    let (Some(esquema), Some(payload)) = (v["schema"].as_str(), v["payload"].as_str()) else {
        return Err("faltan `schema` o `payload`".into());
    };
    let (esquema, payload) = (esquema.as_bytes(), payload.as_bytes());

    let a = automata(esquema)?;
    let s = a.summary();
    let (seg, ver, rep) = tiempo(|| validar_dpda(&a, payload));
    let (asig, pico) = memoria(|| validar_dpda(&a, payload));
    let mut dpda = fila(payload.len(), seg, ver, rep, asig, pico);
    dpda["estados"] = json!(s.states);
    dpda["transiciones"] = json!(s.transitions);

    let linea_base = if a.format() == PayloadFormat::Json {
        baseline::with_stack(|st| {
            let lb = baseline::JsonBaseline::compile_text(esquema).ok()?;
            let (seg, ver, rep) = tiempo(|| validar_linea_base(&lb, st, payload));
            let (asig, pico) = memoria(|| validar_linea_base(&lb, st, payload));
            Some(fila(payload.len(), seg, ver, rep, asig, pico))
        })
    } else {
        None
    };

    Ok(json!({
        "bytes": payload.len(),
        "profundidad": profundidad(payload, a.format()),
        "dpda": dpda,
        "lineaBase": linea_base,
        "motivoLineaBase": if linea_base.is_none() {
            Value::from(if a.format() == PayloadFormat::Json {
                "la línea base no compila el esquema"
            } else {
                "la línea base XML (libxml2) no está en el servidor (D54)"
            })
        } else {
            Value::Null
        },
    }))
}

fn main() -> std::process::ExitCode {
    // Un hilo con pila propia (D7): el compilador y la línea base recorren
    // estructuras anidadas.
    let r = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(principal)
        .map_err(|e| e.to_string())
        .and_then(|h| h.join().map_err(|_| "pánico al medir".to_owned()))
        .and_then(|r| r);
    match r {
        Ok(v) => {
            println!("{v}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            println!("{}", json!({ "error": e }));
            std::process::ExitCode::from(1)
        }
    }
}
