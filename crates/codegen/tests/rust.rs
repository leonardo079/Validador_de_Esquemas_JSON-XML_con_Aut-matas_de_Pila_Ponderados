//! Emisor Rust (F5.1, D74; criterios en `tasks/todo.md` «Tarea F5.1»).
//!
//! Compila de verdad lo emitido con `rustc` en `CARGO_TARGET_TMPDIR`:
//!
//! - **Banco** (`banco_equivale_al_interprete`): un solo `rustc` en modo
//!   *debug* (con los desbordamientos aritméticos comprobados) con las tablas
//!   y la fachada de **cada** esquema del corpus sobre una copia común del
//!   runtime; cada payload en bloques de 1, 7 y 4 096 bytes y en los dos
//!   modos da el mismo `Report` (`{:?}`) que `automaton::validate_with`.
//!   Un `rustc` por esquema en debug costaría más que este único banco.
//! - **El `main.rs` real** (`main_real_en_todo_el_corpus`) de cada esquema,
//!   con la orden exacta del sandbox (`rustc --edition 2021 -O main.rs`, D68),
//!   sin avisos, con cada payload por stdin: la línea JSON y el código de
//!   salida son los del intérprete y los de `expected` (PC-E).
//! - **Sin reservas por construcción** (`nucleo_sin_std_ni_alloc`): la
//!   fachada, las tablas y la copia del runtime compilan como `#![no_std]`
//!   sin `alloc`, así que no pueden llamar al asignador.
//!
//! Ninguna prueba mide tiempo (D1).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use automaton::{build, validate_with, Mode, Options};
use codegen::fuentes::{self, COPIADOS};
use codegen::rust::emit;
use codegen::Archivo;
use serde_json::Value;

mod comun;
use comun::*;

/// Criterio 1: el banco de un solo `rustc` (debug) con todos los esquemas.
#[test]
fn banco_equivale_al_interprete() {
    let grupos = grupos();
    let dir = directorio("rust-banco");
    let primeros = emit(&grupos[0].a);
    let runtime: Vec<Archivo> = primeros
        .iter()
        .filter(|f| f.nombre.starts_with("lexer") || f.nombre.starts_with("runtime_rs"))
        .cloned()
        .collect();
    escribir(&dir, &runtime);

    let mut main =
        String::from("#![allow(dead_code, unused_imports)]\nmod lexer;\nmod runtime_rs;\n");
    let mut ramas = String::new();
    let mut trabajos = String::new();
    let mut j = 0;
    for (k, g) in grupos.iter().enumerate() {
        let emitido = emit(&g.a);
        // La copia del runtime es la misma para todos los esquemas.
        for f in emitido
            .iter()
            .filter(|f| runtime.iter().any(|r| r.nombre == f.nombre))
        {
            assert!(runtime.contains(f), "{}", f.nombre);
        }
        for f in &emitido {
            if f.nombre == "tablas.rs" || f.nombre == "validador.rs" {
                std::fs::write(dir.join(format!("e{k}_{}", f.nombre)), &f.contenido).unwrap();
            }
        }
        std::fs::write(
            dir.join(format!("e{k}.rs")),
            format!(
                "#[path = \"e{k}_tablas.rs\"]\npub mod tablas;\n\
                 #[path = \"e{k}_validador.rs\"]\npub mod validador;\n"
            ),
        )
        .unwrap();
        main.push_str(&format!("mod e{k};\n"));
        ramas.push_str(&format!(
            "        {k} => (e{k}::validador::validar_por_bloques(p, b, m), e{k}::validador::escribir_json, e{k}::validador::codigo_de_salida),\n"
        ));
        for (_, p) in &g.casos {
            std::fs::write(dir.join(format!("p{j}.bin")), p).unwrap();
            trabajos.push_str(&format!("{k} {j}\n"));
            j += 1;
        }
    }
    std::fs::write(dir.join("trabajos.txt"), &trabajos).unwrap();
    main.push_str(&BANCO.replace("{{RAMAS}}", &ramas));
    std::fs::write(dir.join("main.rs"), main).unwrap();
    rustc(&dir, &["--edition", "2021", "-o", "banco", "main.rs"]);

    let out = Command::new(dir.join("banco"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let texto = String::from_utf8(out.stdout).unwrap();
    let mut lineas = texto.lines();
    assert_eq!(lineas.next(), Some("INTERNO 3"));
    let mut j = 0;
    let mut n = 0;
    for (k, g) in grupos.iter().enumerate() {
        for (c, p) in &g.casos {
            for b in BLOQUES {
                for (modo, nombre) in [(Mode::Total, "total"), (Mode::EarlyFail, "temprano")] {
                    let r = validate_with(&g.a, p, &Options::new(modo), b);
                    let esperada = format!("{k} {j} {b} {nombre} {r:?}");
                    assert_eq!(lineas.next(), Some(esperada.as_str()), "{}", c.id);
                    n += 1;
                }
            }
            let r = validate_with(&g.a, p, &Options::new(Mode::Total), 4096);
            let l = lineas.next().unwrap();
            let json = l.strip_prefix(&format!("JSON {k} {j} ")).expect(l);
            let v: Value = serde_json::from_str(json).expect(json);
            assert_eq!(v, json_esperado(&g.a, &r), "{}", c.id);
            assert_eq!(
                lineas.next(),
                Some(format!("CODIGO {k} {j} {}", codigo_esperado(&r)).as_str()),
                "{}",
                c.id
            );
            j += 1;
        }
    }
    assert_eq!(lineas.next(), None);
    assert!(n >= 6 * 70, "{n}");
}

/// El `main` del banco: lee `trabajos.txt` (esquema, payload) y escribe, por
/// payload, el `Report` de cada tamaño de bloque y modo, la línea JSON y el
/// código de salida. Corre en un hilo con pila explícita (D7): la memoria
/// de `validar_por_bloques` va en la pila.
const BANCO: &str = r#"
use runtime_rs::{ErrorKind, Mode, Pointer, Report, Weight};

type Fila = (Report, fn(&Report, &mut String) -> core::fmt::Result, fn(&Report) -> u8);

fn uno(k: usize, p: &[u8], b: usize, m: Mode) -> Fila {
    match k {
{{RAMAS}}        _ => unreachable!(),
    }
}

fn correr() {
    let interno = Report {
        valid: false,
        violations: Weight::INFINITY,
        error_kind: Some(ErrorKind::Internal),
        first: None,
        pointer: Pointer::ROOT,
    };
    println!("INTERNO {}", e0::validador::codigo_de_salida(&interno));
    let trabajos = std::fs::read_to_string("trabajos.txt").unwrap();
    for l in trabajos.lines() {
        let mut it = l.split(' ');
        let k: usize = it.next().unwrap().parse().unwrap();
        let j: usize = it.next().unwrap().parse().unwrap();
        let p = std::fs::read(format!("p{j}.bin")).unwrap();
        for b in [1usize, 7, 4096] {
            for (m, nombre) in [(Mode::Total, "total"), (Mode::EarlyFail, "temprano")] {
                let (r, _, _) = uno(k, &p, b, m);
                println!("{k} {j} {b} {nombre} {r:?}");
            }
        }
        let (r, json, codigo) = uno(k, &p, 4096, Mode::Total);
        let mut s = String::new();
        json(&r, &mut s).unwrap();
        println!("JSON {k} {j} {s}");
        println!("CODIGO {k} {j} {}", codigo(&r));
    }
}

fn main() {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(correr)
        .unwrap()
        .join()
        .unwrap();
}
"#;

/// Criterios 2 y 3 y PC-E: el `main.rs` real de cada esquema, compilado con
/// la orden del sandbox, sin avisos; cada payload por stdin da una línea
/// JSON igual a la del intérprete y a `expected`, y su código de salida.
#[test]
fn main_real_en_todo_el_corpus() {
    let grupos = grupos();
    let binarios = en_paralelo(&grupos, |k, g| {
        let archivos = emit(&g.a);
        nombres_del_sandbox(&archivos, &[".rs"], "main.rs");
        let dir = directorio(&format!("rust-main{k}"));
        escribir(&dir, &archivos);
        let avisos = rustc(
            &dir,
            &["--edition", "2021", "-O", "-o", "validador", "main.rs"],
        );
        assert!(avisos.is_empty(), "esquema {k}: {avisos}");
        dir.join("validador")
    });
    corpus_por_stdin(&grupos, &binarios);
}

/// Criterio 4: misma M, mismos bytes; también reconstruyendo M desde el
/// esquema.
#[test]
fn salida_reproducible() {
    for g in grupos() {
        let otra = build(&ir_de(&g.texto, g.xml)).unwrap();
        let (x, y) = (emit(&g.a), emit(&otra));
        assert_eq!(x, y);
        assert_eq!(x, emit(&g.a));
    }
}

/// Criterio 5: la parte que valida (fachada, tablas y la copia del runtime)
/// compila como `#![no_std]` sin `alloc`: no tiene asignador al que llamar.
/// El testigo negativo muestra que la prueba distingue: con un `Vec` no
/// compila.
#[test]
fn nucleo_sin_std_ni_alloc() {
    let grupos = grupos();
    let json = grupos.iter().find(|g| !g.xml).unwrap();
    let xml = grupos.iter().find(|g| g.xml).unwrap();
    for (nombre, g) in [("nostd_json", json), ("nostd_xml", xml)] {
        let dir = directorio(&format!("rust-{nombre}"));
        let archivos: Vec<Archivo> = emit(&g.a)
            .into_iter()
            .filter(|f| f.nombre != "main.rs")
            .collect();
        escribir(&dir, &archivos);
        let raiz = "#![no_std]\n#![allow(dead_code, unused_imports)]\n\
                    mod lexer;\nmod runtime_rs;\nmod tablas;\npub mod validador;\n";
        std::fs::write(dir.join("nucleo.rs"), raiz).unwrap();
        rustc(
            &dir,
            &["--edition", "2021", "--crate-type", "rlib", "nucleo.rs"],
        );
        std::fs::write(
            dir.join("testigo.rs"),
            format!("{raiz}pub fn f() -> usize {{ Vec::<u8>::new().len() }}\n"),
        )
        .unwrap();
        let st = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
            .current_dir(&dir)
            .args(["--edition", "2021", "--crate-type", "rlib", "testigo.rs"])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!st.success(), "el testigo con Vec no debería compilar");
    }
}

/// Criterio 6: la copia no se desincroniza del original. Los archivos
/// incrustados son exactamente los `.rs` de los dos `src/`, con su
/// contenido; cada submódulo de los `lib.rs` lleva su `#[path]` hacia un
/// archivo emitido, y no queda ningún `#![no_std]`.
#[test]
fn copia_sincronizada() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let g = grupos();
    let emitidos: BTreeSet<String> = emit(&g[0].a).into_iter().map(|f| f.nombre).collect();
    for c in COPIADOS {
        let en_disco: BTreeSet<String> = std::fs::read_dir(raiz.join(c.origen))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter_map(|n| n.strip_suffix(".rs").map(str::to_owned))
            .collect();
        let incrustados: BTreeSet<String> =
            c.archivos.iter().map(|(n, _)| (*n).to_owned()).collect();
        assert_eq!(
            en_disco, incrustados,
            "{}: actualizar `codegen/src/fuentes.rs`",
            c.origen
        );
        for (nombre, fuente) in c.archivos {
            let original =
                std::fs::read_to_string(raiz.join(c.origen).join(format!("{nombre}.rs"))).unwrap();
            assert_eq!(*fuente, original, "{}/{nombre}.rs", c.origen);
            let copia = c.reescribir(nombre, fuente);
            assert!(!copia.contains("#![no_std]"), "{nombre}");
            assert!(emitidos.contains(&c.nombre(nombre)), "{nombre}");
            if *nombre == "lib" {
                let lineas: Vec<&str> = copia.lines().collect();
                let mut n = 0;
                for (i, l) in lineas.iter().enumerate() {
                    if let Some(m) = fuentes::submodulo(l.trim_end()) {
                        let path = format!("#[path = \"{}_{m}.rs\"]", c.modulo);
                        assert_eq!(lineas[i - 1], path, "{}", c.origen);
                        assert!(emitidos.contains(&format!("{}_{m}.rs", c.modulo)), "{m}");
                        n += 1;
                    }
                }
                assert_eq!(
                    n,
                    c.archivos.len() - 1,
                    "{}: un `mod` por archivo",
                    c.origen
                );
            }
        }
    }
}
