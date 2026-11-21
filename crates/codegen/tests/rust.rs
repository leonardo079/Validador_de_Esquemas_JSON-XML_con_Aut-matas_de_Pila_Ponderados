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
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use automaton::{build, validate_with, Automaton, ErrorKind, Mode, Options, Report};
use baseline::corpus::{self, Case, ErrorKind as Esperado, Format, Payload};
use codegen::fuentes::{self, COPIADOS};
use codegen::rust::{emit, salida};
use codegen::Archivo;
use ir::VerifiedIr;
use serde_json::{json, Value};

const BLOQUES: [usize; 3] = [1, 7, 4096];

/// Los esquemas distintos del corpus (por su texto exacto) con su M y sus
/// casos con bytes, en el orden del corpus.
struct Grupo {
    texto: Vec<u8>,
    xml: bool,
    a: Automaton,
    casos: Vec<(Case, Vec<u8>)>,
}

fn ir_de(texto: &[u8], xml: bool) -> VerifiedIr {
    let texto = texto.to_vec();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            if xml {
                let s = xsd::parse(&texto).unwrap_or_else(|e| panic!("{e}"));
                xsd::lower(&xsd::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"))).unwrap()
            } else {
                let s = schema_parser::parse(&texto).unwrap_or_else(|e| panic!("{e}"));
                let t = semantic::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"));
                semantic::lower(&t).unwrap_or_else(|e| panic!("{e}"))
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn grupos() -> Vec<Grupo> {
    let mut g: Vec<Grupo> = Vec::new();
    for c in corpus::casos().unwrap() {
        let Payload::Bytes(p) = &c.payload else {
            continue;
        };
        let p = p.clone();
        let xml = c.format == Format::Xml;
        if let Some(gr) = g
            .iter_mut()
            .find(|gr| gr.texto == c.schema_text && gr.xml == xml)
        {
            gr.casos.push((c, p));
            continue;
        }
        let a = build(&ir_de(&c.schema_text, xml)).unwrap_or_else(|e| panic!("{}: {e}", c.id));
        g.push(Grupo {
            texto: c.schema_text.clone(),
            xml,
            a,
            casos: vec![(c, p)],
        });
    }
    assert!(g.iter().any(|gr| gr.xml) && g.iter().any(|gr| !gr.xml));
    g
}

/// Un directorio vacío bajo `target/` para `nombre`.
fn directorio(nombre: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("codegen-rust")
        .join(nombre);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn escribir(dir: &Path, archivos: &[Archivo]) {
    for a in archivos {
        std::fs::write(dir.join(&a.nombre), &a.contenido).unwrap();
    }
}

/// `rustc` (el de la cadena de herramientas del repositorio) en `dir`;
/// devuelve stderr y falla si no compila.
fn rustc(dir: &Path, args: &[&str]) -> String {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out = Command::new(rustc)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{}: {err}", dir.display());
    err
}

fn clase(k: ErrorKind) -> &'static str {
    match k {
        ErrorKind::Lexical => "lexical",
        ErrorKind::Syntax => "syntax",
        ErrorKind::Schema => "schema",
        ErrorKind::Limit => "limit",
        ErrorKind::Internal => "internal",
    }
}

/// La línea JSON que debe escribir el validador para el reporte `r` del
/// intérprete.
fn json_esperado(a: &Automaton, r: &Report) -> Value {
    let regla = r.first.and_then(|f| f.rule);
    json!({
        "valid": r.valid,
        "violations": r.violations.get(),
        "errorKind": r.error_kind.map(clase),
        "offset": r.first.map(|f| f.offset),
        "line": r.first.map(|f| f.line),
        "column": r.first.map(|f| f.column),
        "rule": regla.map(|k| a.rule_location(k)),
        "keyword": regla.map(|k| a.rule(k).1.name()),
    })
}

fn codigo_esperado(r: &Report) -> u8 {
    match (r.valid, r.error_kind) {
        (true, _) => salida::VALIDO,
        (false, Some(ErrorKind::Schema)) => salida::INVALIDO,
        (false, Some(ErrorKind::Lexical | ErrorKind::Syntax | ErrorKind::Limit)) => {
            salida::MALFORMADO
        }
        _ => salida::INTERNO,
    }
}

/// El caso cumple su `expected` (lo que comprueba el corpus del intérprete,
/// aquí sobre la salida del binario generado).
fn cumple_expected(c: &Case, v: &Value) {
    let e = &c.expected;
    assert_eq!(v["valid"], e.valid, "{}: {v}", c.id);
    let kind = e.error_kind.map(|k| match k {
        Esperado::Lexical => "lexical",
        Esperado::Syntax => "syntax",
        Esperado::Schema => "schema",
        Esperado::Limit => "limit",
    });
    assert_eq!(v["errorKind"].as_str(), kind, "{}: {v}", c.id);
    match e.violations {
        Some(n) => assert_eq!(v["violations"], n, "{}: {v}", c.id),
        None if !e.valid => assert!(v["violations"].is_null(), "{}: {v}", c.id),
        None => {}
    }
    if let Some(off) = e.offset {
        assert_eq!(v["offset"], off, "{}: {v}", c.id);
    }
}

/// Lo que el sandbox comprueba de los nombres (`server::sandbox::comprobar`):
/// `[a-z0-9_]+.rs`, sin repetidos, con `main.rs`.
fn nombres_del_sandbox(archivos: &[Archivo]) {
    let mut vistos = BTreeSet::new();
    for a in archivos {
        let base = a.nombre.strip_suffix(".rs").expect(&a.nombre);
        assert!(
            !base.is_empty()
                && base
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "{}",
            a.nombre
        );
        assert!(vistos.insert(a.nombre.clone()), "{} repetido", a.nombre);
    }
    assert!(vistos.contains("main.rs"));
}

/// Criterio 1: el banco de un solo `rustc` (debug) con todos los esquemas.
#[test]
fn banco_equivale_al_interprete() {
    let grupos = grupos();
    let dir = directorio("banco");
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
use runtime_rs::{ErrorKind, Mode, Report, Weight};

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
    let hilos = std::thread::available_parallelism().map_or(2, |n| n.get().min(8));
    let binarios: Vec<PathBuf> = std::thread::scope(|s| {
        let mut out = Vec::new();
        for lote in grupos.iter().enumerate().collect::<Vec<_>>().chunks(hilos) {
            let hs: Vec<_> = lote
                .iter()
                .map(|&(k, g)| {
                    s.spawn(move || {
                        let archivos = emit(&g.a);
                        nombres_del_sandbox(&archivos);
                        let dir = directorio(&format!("main{k}"));
                        escribir(&dir, &archivos);
                        let avisos = rustc(
                            &dir,
                            &["--edition", "2021", "-O", "-o", "validador", "main.rs"],
                        );
                        assert!(avisos.is_empty(), "esquema {k}: {avisos}");
                        dir.join("validador")
                    })
                })
                .collect();
            out.extend(hs.into_iter().map(|h| h.join().unwrap()));
        }
        out
    });

    let mut codigos = BTreeSet::new();
    let mut limite = false;
    let mut n = 0;
    for (g, bin) in grupos.iter().zip(&binarios) {
        for (c, p) in &g.casos {
            let mut h = Command::new(bin)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut stdin = h.stdin.take().unwrap();
            let p2 = p.clone();
            // El validador puede parar antes de leerlo todo (`Flow::Stop`):
            // un error de escritura (tubería cerrada) no es un fallo.
            let escritor = std::thread::spawn(move || {
                let _ = std::io::Write::write_all(&mut stdin, &p2);
            });
            let out = h.wait_with_output().unwrap();
            escritor.join().unwrap();
            assert!(out.stderr.is_empty(), "{}", c.id);
            let texto = String::from_utf8(out.stdout).unwrap();
            let linea = texto.strip_suffix('\n').expect(&texto);
            assert!(!linea.contains('\n'), "{}: una sola línea", c.id);
            let v: Value = serde_json::from_str(linea).expect(linea);
            let r = validate_with(&g.a, p, &Options::new(Mode::Total), 4096);
            assert_eq!(v, json_esperado(&g.a, &r), "{}", c.id);
            cumple_expected(c, &v);
            let codigo = out.status.code().unwrap();
            assert_eq!(codigo, i32::from(codigo_esperado(&r)), "{}", c.id);
            assert!(
                !(100..=103).contains(&codigo) && !(124..=127).contains(&codigo),
                "{}",
                c.id
            );
            codigos.insert(codigo);
            limite |= r.error_kind == Some(ErrorKind::Limit);
            n += 1;
        }
    }
    // Un caso de cada tipo: válido 0, inválido 1, malformado 2 y límite 2.
    assert_eq!(codigos, BTreeSet::from([0, 1, 2]));
    assert!(limite);
    assert!(n >= 70, "{n}");

    // Un fallo al leer stdin (un directorio) da 3 con el motivo en stderr.
    let dir = std::fs::File::open(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let out = Command::new(&binarios[0])
        .stdin(Stdio::from(dir))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(i32::from(salida::INTERNO)));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no se pudo leer la entrada"));
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
        let dir = directorio(nombre);
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
