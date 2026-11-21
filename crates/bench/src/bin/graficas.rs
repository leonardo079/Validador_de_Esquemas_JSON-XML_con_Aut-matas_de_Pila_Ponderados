//! E1 — Dibuja las gráficas de bombeo (D80; `docs/fase6/bombeo.md`).
//!
//! ```text
//! cargo run -p bench --release --bin metricas -- --bombeo-fino --csv target/metricas/bombeo.csv
//! cargo run -p bench --release --bin graficas -- target/metricas/bombeo.csv docs/fase6/graficas
//! ```
//!
//! Escribe cinco SVG y `datos.md` (la tabla con los valores dibujados). La
//! pila del DPDA no se mide: se calcula de M (`Automaton::stack_words`), que
//! es exactamente lo que reserva el ejecutor por nivel.

use std::path::PathBuf;
use std::process::ExitCode;

use automaton::{Mode, Options};
use bench::bombeo;
use bench::graficas::{leer_csv, puntos, Escala, Grafica, Serie, AZUL, GRIS, NARANJA, VERDE};

const DPDA: &str = "DPDA";
const LB: &str = "línea base DOM";
const ANIDADO: &str = "bombeo [^k ]^k";
const ENTEROS: &str = "arreglo de enteros";
const OBJETOS: &str = "arreglo de objetos";

fn serie(nombre: &str, color: &'static str, puntos: Vec<(f64, f64)>) -> Serie {
    Serie {
        nombre: nombre.into(),
        color,
        puntos,
        discontinua: false,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [csv, dir] = args.as_slice() else {
        eprintln!("uso: graficas <bombeo.csv> <carpeta de salida>");
        return ExitCode::from(2);
    };
    let filas = match std::fs::read_to_string(csv)
        .map_err(|e| e.to_string())
        .and_then(|t| leer_csv(&t))
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("graficas: {csv}: {e}");
            return ExitCode::from(2);
        }
    };
    let dir = PathBuf::from(dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("graficas: {}: {e}", dir.display());
        return ExitCode::from(2);
    }

    // Pila del DPDA para k niveles: palabras de 8 bytes de `stack_words(k)`;
    // la memoria reservada es la de los límites por defecto (profundidad 256).
    let Ok(m) = bench::automata(bombeo::ESQUEMA_ANIDADO.as_bytes()) else {
        eprintln!("graficas: el esquema anidado no compila");
        return ExitCode::from(2);
    };
    let ks: Vec<f64> = puntos(&filas, ANIDADO, DPDA, |_| 0.0)
        .iter()
        .map(|p| p.0)
        .collect();
    let pila: Vec<(f64, f64)> = ks
        .iter()
        .map(|&k| (k, (m.stack_words(k as usize) * 8) as f64))
        .collect();
    let reservada = (m.state_words(&Options::new(Mode::Total).limits) * 8) as f64;

    let us = |r: &bench::graficas::Fila| r.segundos * 1e6;
    let ms = |r: &bench::graficas::Fila| r.segundos * 1e3;
    let kib = |r: &bench::graficas::Fila| r.pico_bytes / 1024.0;
    let graficas = vec![
        (
            "anidado-tiempo.svg",
            Grafica {
                titulo: "Tiempo de validación de [ᵏ ]ᵏ contra k".into(),
                eje_x: "k (profundidad de anidamiento)".into(),
                eje_y: "tiempo (µs, mediana)".into(),
                escala_x: Escala::Lineal,
                escala_y: Escala::Lineal,
                series: vec![
                    serie(DPDA, AZUL, puntos(&filas, ANIDADO, DPDA, us)),
                    serie(LB, NARANJA, puntos(&filas, ANIDADO, LB, us)),
                ],
            },
        ),
        (
            "anidado-pila-memoria.svg",
            Grafica {
                titulo: "Pila y memoria de [ᵏ ]ᵏ contra k".into(),
                eje_x: "k (profundidad de anidamiento)".into(),
                eje_y: "bytes".into(),
                escala_x: Escala::Lineal,
                escala_y: Escala::Lineal,
                series: vec![
                    serie("pila usada por el DPDA (O(k))", VERDE, pila.clone()),
                    Serie {
                        nombre: "memoria reservada del DPDA (fija)".into(),
                        color: AZUL,
                        puntos: ks.iter().map(|&k| (k, reservada)).collect(),
                        discontinua: true,
                    },
                    serie(
                        "pico de montón de la línea base",
                        NARANJA,
                        puntos(&filas, ANIDADO, LB, |r| r.pico_bytes),
                    ),
                ],
            },
        ),
        (
            "enteros-memoria.svg",
            Grafica {
                titulo: "Pico de memoria con n enteros [1,1,…,1]".into(),
                eje_x: "n (elementos)".into(),
                eje_y: "pico de montón (KiB)".into(),
                escala_x: Escala::Log,
                escala_y: Escala::Log,
                series: vec![
                    serie(DPDA, AZUL, puntos(&filas, ENTEROS, DPDA, kib)),
                    serie(LB, NARANJA, puntos(&filas, ENTEROS, LB, kib)),
                ],
            },
        ),
        (
            "objetos-memoria.svg",
            Grafica {
                titulo: "Pico de memoria con n objetos [{\"a\":1},…]".into(),
                eje_x: "n (objetos)".into(),
                eje_y: "pico de montón (KiB)".into(),
                escala_x: Escala::Log,
                escala_y: Escala::Log,
                series: vec![
                    serie(DPDA, AZUL, puntos(&filas, OBJETOS, DPDA, kib)),
                    serie(LB, NARANJA, puntos(&filas, OBJETOS, LB, kib)),
                ],
            },
        ),
        (
            "objetos-tiempo.svg",
            Grafica {
                titulo: "Tiempo con n objetos (log-log: pendiente 1 = lineal)".into(),
                eje_x: "n (objetos)".into(),
                eje_y: "tiempo (ms, mediana)".into(),
                escala_x: Escala::Log,
                escala_y: Escala::Log,
                series: vec![
                    serie(DPDA, AZUL, puntos(&filas, OBJETOS, DPDA, ms)),
                    serie(LB, NARANJA, puntos(&filas, OBJETOS, LB, ms)),
                    Serie {
                        nombre: "referencia O(n)".into(),
                        color: GRIS,
                        puntos: {
                            let p = puntos(&filas, OBJETOS, DPDA, ms);
                            match p.first() {
                                Some(&(n0, t0)) => {
                                    p.iter().map(|&(n, _)| (n, t0 * n / n0)).collect()
                                }
                                None => Vec::new(),
                            }
                        },
                        discontinua: true,
                    },
                ],
            },
        ),
    ];
    for (archivo, g) in &graficas {
        let ruta = dir.join(archivo);
        if let Err(e) = std::fs::write(&ruta, g.svg()) {
            eprintln!("graficas: {}: {e}", ruta.display());
            return ExitCode::from(2);
        }
        eprintln!("graficas: {}", ruta.display());
    }
    if let Err(e) = std::fs::write(dir.join("datos.md"), tabla(&filas, &pila, reservada)) {
        eprintln!("graficas: datos.md: {e}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// La tabla con los valores de las gráficas.
fn tabla(filas: &[bench::graficas::Fila], pila: &[(f64, f64)], reservada: f64) -> String {
    let mut o = String::from(
        "<!-- Generado por `cargo run -p bench --release --bin graficas` (E1, D80); no editar a mano. -->\n\n",
    );
    o.push_str("| Familia | Parámetro | Bytes | Prof. | DPDA (µs) | Línea base (µs) | DPDA pico | Línea base pico | Línea base asign. | Pila DPDA |\n");
    o.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for fam in [ANIDADO, ENTEROS, OBJETOS] {
        let d = puntos(filas, fam, DPDA, |r| r.segundos);
        for (p, _) in d {
            let fila = |v: &str| {
                filas.iter().find(|r| {
                    r.caso.starts_with(fam)
                        && r.validador == v
                        && bench::graficas::parametro(&r.caso) == Some(p)
                })
            };
            let (Some(a), b) = (fila(DPDA), fila(LB)) else {
                continue;
            };
            let pila_k = pila
                .iter()
                .find(|x| x.0 == p)
                .map_or_else(|| "—".to_owned(), |x| format!("{} B", x.1));
            o.push_str(&format!(
                "| {fam} | {p} | {} | {} | {:.1} | {} | {} | {} | {} | {} |\n",
                a.bytes,
                a.profundidad,
                a.segundos * 1e6,
                b.map_or("—".into(), |b| format!("{:.1}", b.segundos * 1e6)),
                bench::legible(a.pico_bytes as usize),
                b.map_or("—".into(), |b| bench::legible(b.pico_bytes as usize)),
                b.map_or("—".into(), |b| format!("{}", b.asignaciones)),
                if fam == ANIDADO { pila_k } else { "—".into() },
            ));
        }
    }
    o.push_str(&format!(
        "\nMemoria reservada del DPDA (fija, límites por defecto): {}.\n",
        bench::legible(reservada as usize)
    ));
    o
}
