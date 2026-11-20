//! F6.5 — Arnés de métricas: DPDA frente a la línea base DOM.
//!
//! ```text
//! cargo run -p bench --release --bin metricas                  # corpus + bombeo
//! cargo run -p bench --release --bin metricas -- --corpus      # solo el corpus
//! cargo run -p bench --release --bin metricas -- --bombeo --csv bombeo.csv
//! cargo run -p bench --release --bin metricas -- --repeticiones 21
//! ```
//!
//! Por cada caso mide, para el DPDA (M de `automaton`, alimentado en bloques
//! de 4 KiB) y para la línea base (`serde_json` + `jsonschema`, D48):
//! mediana del tiempo de validar (sin compilar el esquema), MB/s y ns/byte,
//! asignaciones y pico de memoria de montón de una validación (con `dhat`
//! como asignador global; en las pasadas de tiempo apenas añade coste
//! porque no hay perfilador activo), profundidad del payload y tamaño de M.
//! Ciclos por byte = ns/byte × GHz de la máquina (no se leen contadores de
//! ciclos: exigiría `unsafe`, D52). Las mediciones de tiempo viven aquí y no en
//! `cargo test` (D1). Teoría y cifras: `docs/fase6/metricas.md`.

use std::path::PathBuf;
use std::time::Instant;

use automaton::Automaton;
use bench::{
    automata, bombeo, csv, profundidad, tabla_markdown, validar_dpda, validar_linea_base, Generado,
    Medida, Validador, Veredicto,
};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

struct Args {
    corpus: bool,
    bombeo: bool,
    repeticiones: usize,
    csv: Option<PathBuf>,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        corpus: false,
        bombeo: false,
        repeticiones: 11,
        csv: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--corpus" => a.corpus = true,
            "--bombeo" => a.bombeo = true,
            "--repeticiones" => {
                a.repeticiones = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|&n| n > 0)
                    .ok_or("--repeticiones espera un entero positivo")?
            }
            "--csv" => a.csv = Some(it.next().ok_or("--csv espera una ruta")?.into()),
            otro => return Err(format!("opción desconocida: {otro}")),
        }
    }
    if !a.corpus && !a.bombeo {
        (a.corpus, a.bombeo) = (true, true);
    }
    Ok(a)
}

/// Mediana de `rep` ejecuciones de `f`, en segundos.
fn tiempo(rep: usize, mut f: impl FnMut() -> Veredicto) -> (f64, Veredicto) {
    let mut v = f(); // calentamiento
    let mut t: Vec<f64> = (0..rep)
        .map(|_| {
            let i = Instant::now();
            v = f();
            i.elapsed().as_secs_f64()
        })
        .collect();
    t.sort_by(f64::total_cmp);
    (t[t.len() / 2], v)
}

/// Asignaciones y pico de montón de una ejecución de `f`.
fn memoria(f: impl FnOnce() -> Veredicto) -> (u64, usize) {
    let _p = dhat::Profiler::builder().testing().build();
    std::hint::black_box(f());
    let s = dhat::HeapStats::get();
    (s.total_blocks, s.max_bytes)
}

/// Mide un payload con los dos validadores (la línea base solo en JSON).
fn medir(
    rep: usize,
    caso: &str,
    m: Option<&Automaton>,
    esquema: &[u8],
    payload: &[u8],
    json: bool,
    salida: &mut Vec<Medida>,
) {
    if let Some(a) = m {
        let (segundos, veredicto) = tiempo(rep, || validar_dpda(a, payload));
        let (asignaciones, pico_bytes) = memoria(|| validar_dpda(a, payload));
        let s = a.summary();
        salida.push(Medida {
            caso: caso.into(),
            validador: Validador::Dpda,
            bytes: payload.len(),
            profundidad: profundidad(payload, a.format()),
            veredicto,
            segundos,
            asignaciones,
            pico_bytes,
            estados: Some(s.states),
            transiciones: Some(s.transitions),
        });
    }
    if !json {
        return; // la línea base XML necesita la feature `libxml2` (D54)
    }
    let fila = baseline::with_stack(|st| {
        let lb = baseline::JsonBaseline::compile_text(esquema).ok()?;
        let (segundos, veredicto) = tiempo(rep, || validar_linea_base(&lb, st, payload));
        let (asignaciones, pico_bytes) = memoria(|| validar_linea_base(&lb, st, payload));
        Some((segundos, veredicto, asignaciones, pico_bytes))
    });
    if let Some((segundos, veredicto, asignaciones, pico_bytes)) = fila {
        salida.push(Medida {
            caso: caso.into(),
            validador: Validador::LineaBase,
            bytes: payload.len(),
            profundidad: profundidad(payload, automaton::PayloadFormat::Json),
            veredicto,
            segundos,
            asignaciones,
            pico_bytes,
            estados: None,
            transiciones: None,
        });
    }
}

fn main() -> std::process::ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("metricas: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let mut medidas = Vec::new();

    if a.corpus {
        let casos = match baseline::with_stack(|_| baseline::corpus::casos()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "metricas: no se pudo leer el corpus: {} ({})",
                    e.message,
                    e.path.display()
                );
                return std::process::ExitCode::from(2);
            }
        };
        for c in &casos {
            let baseline::corpus::Payload::Bytes(payload) = &c.payload else {
                continue; // los `generator` los cubre el bombeo
            };
            let m = match automata(&c.schema_text) {
                Ok(m) => Some(m),
                Err(e) => {
                    eprintln!("metricas: {}: sin DPDA ({e})", c.id);
                    None
                }
            };
            let json = c.format == baseline::corpus::Format::Json;
            medir(
                a.repeticiones,
                &c.id,
                m.as_ref(),
                &c.schema_text,
                payload,
                json,
                &mut medidas,
            );
        }
    }

    if a.bombeo {
        let familias: Vec<Generado> = [1, 10, 100, 255]
            .into_iter()
            .map(bombeo::anidado)
            .chain(
                [1_000, 10_000, 100_000, 1_000_000]
                    .into_iter()
                    .map(bombeo::enteros),
            )
            .chain([1_000, 10_000, 100_000].into_iter().map(bombeo::objetos))
            .collect();
        for g in &familias {
            let m = match automata(g.esquema.as_bytes()) {
                Ok(m) => Some(m),
                Err(e) => {
                    eprintln!("metricas: {}: sin DPDA ({e})", g.nombre);
                    None
                }
            };
            medir(
                a.repeticiones,
                &g.nombre,
                m.as_ref(),
                g.esquema.as_bytes(),
                &g.payload,
                true,
                &mut medidas,
            );
        }
    }

    print!("{}", tabla_markdown(&medidas));
    if let Some(ruta) = a.csv {
        if let Err(e) = std::fs::write(&ruta, csv(&medidas)) {
            eprintln!("metricas: no se pudo escribir {}: {e}", ruta.display());
            return std::process::ExitCode::from(2);
        }
        eprintln!("metricas: CSV en {}", ruta.display());
    }
    std::process::ExitCode::SUCCESS
}
