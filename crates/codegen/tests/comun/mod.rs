//! Lo común a las pruebas de los emisores (F5.1-F5.3): los esquemas del
//! corpus con su M, directorios bajo `target/`, compilar con `rustc` o `cc`,
//! ejecutar un binario con un payload por stdin, y la línea JSON y el código
//! de salida que debe dar el validador generado para un reporte del
//! intérprete.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use automaton::{build, Automaton, ErrorKind, Report};
use baseline::corpus::{self, Case, ErrorKind as Esperado, Format, Payload};
use codegen::rust::salida;
use codegen::Archivo;
use ir::VerifiedIr;
use serde_json::{json, Value};

pub const BLOQUES: [usize; 3] = [1, 7, 4096];

/// Los esquemas distintos del corpus (por su texto exacto) con su M y sus
/// casos con bytes, en el orden del corpus.
pub struct Grupo {
    pub texto: Vec<u8>,
    pub xml: bool,
    pub a: Automaton,
    pub casos: Vec<(Case, Vec<u8>)>,
}

pub fn ir_de(texto: &[u8], xml: bool) -> VerifiedIr {
    let texto = texto.to_vec();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            // Un caso XML trae un XSD o una DTD: lo decide `dtd::formato` (D86).
            if xml && dtd::formato(&texto) == dtd::Formato::Dtd {
                dtd::compile(&texto).unwrap_or_else(|e| panic!("{e:?}"))
            } else if xml {
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

pub fn grupos() -> Vec<Grupo> {
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
pub fn directorio(nombre: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("codegen")
        .join(nombre);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn escribir(dir: &Path, archivos: &[Archivo]) {
    for a in archivos {
        std::fs::write(dir.join(&a.nombre), &a.contenido).unwrap();
    }
}

/// `rustc` (el de la cadena de herramientas del repositorio) en `dir`;
/// devuelve stderr y falla si no compila.
pub fn rustc(dir: &Path, args: &[&str]) -> String {
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

pub fn clase(k: ErrorKind) -> &'static str {
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
pub fn json_esperado(a: &Automaton, r: &Report) -> Value {
    let regla = r.first.and_then(|f| f.rule);
    json!({
        "valid": r.valid,
        "violations": r.violations.get(),
        "errorKind": r.error_kind.map(clase),
        "offset": r.first.map(|f| f.offset),
        "line": r.first.map(|f| f.line),
        "column": r.first.map(|f| f.column),
        "pointer": r.first.map(|_| r.pointer.as_str()),
        "pointerTruncated": r.first.is_some() && r.pointer.truncated(),
        "rule": regla.map(|k| a.rule_location(k)),
        "keyword": regla.map(|k| a.rule(k).1.name()),
    })
}

pub fn codigo_esperado(r: &Report) -> u8 {
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
pub fn cumple_expected(c: &Case, v: &Value) {
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
    // F6.1: el puntero del primer error.
    if let Some(p) = &e.pointer {
        assert_eq!(v["pointer"].as_str(), Some(p.as_str()), "{}: {v}", c.id);
        assert_eq!(v["pointerTruncated"], false, "{}: {v}", c.id);
    }
}

/// Lo que el sandbox comprueba de los nombres (`server::sandbox::comprobar`):
/// `[a-z0-9_]+` con una de las extensiones `ext`, sin repetidos, con
/// `principal`.
pub fn nombres_del_sandbox(archivos: &[Archivo], ext: &[&str], principal: &str) {
    let mut vistos = BTreeSet::new();
    for a in archivos {
        let base = ext
            .iter()
            .find_map(|e| a.nombre.strip_suffix(e))
            .expect(&a.nombre);
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
    assert!(vistos.contains(principal), "falta {principal}");
}

/// Un compilador de C: `CC` o `cc`.
pub fn cc() -> String {
    std::env::var("CC").unwrap_or_else(|_| "cc".into())
}

/// Ejecuta `orden` en `dir`; devuelve stderr y falla si no termina bien.
pub fn compilar_con(dir: &Path, orden: &str, args: &[&str]) -> String {
    let out = Command::new(orden)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{orden}: {e}"));
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{} ({orden}): {err}", dir.display());
    err
}

/// Ejecuta `bin` con `payload` por stdin: (código, stdout, stderr). El
/// validador puede parar antes de leerlo todo (fallo temprano o error de peso
/// ∞): un error de escritura en la tubería no es un fallo.
pub fn ejecutar(bin: &Path, payload: &[u8]) -> (i32, String, String) {
    let mut h = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = h.stdin.take().unwrap();
    let p = payload.to_vec();
    let escritor = std::thread::spawn(move || {
        let _ = std::io::Write::write_all(&mut stdin, &p);
    });
    let out = h.wait_with_output().unwrap();
    escritor.join().unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// Comprueba la salida de un validador generado (Rust o C) para el payload
/// `p` del caso `c`: una sola línea JSON igual a la del intérprete y que
/// cumple `expected`, sin nada en stderr, y su código de salida. Devuelve el
/// código.
pub fn comprobar_salida(g: &Grupo, c: &Case, p: &[u8], salida: (i32, String, String)) -> i32 {
    let (codigo, out, err) = salida;
    assert!(err.is_empty(), "{}: {err}", c.id);
    let linea = out.strip_suffix('\n').expect(&out);
    assert!(!linea.contains('\n'), "{}: una sola línea", c.id);
    let v: Value = serde_json::from_str(linea).expect(linea);
    let r = automaton::validate_with(
        &g.a,
        p,
        &automaton::Options::new(automaton::Mode::Total),
        4096,
    );
    assert_eq!(v, json_esperado(&g.a, &r), "{}", c.id);
    cumple_expected(c, &v);
    assert_eq!(codigo, i32::from(codigo_esperado(&r)), "{}", c.id);
    assert!(
        !(100..=103).contains(&codigo) && !(124..=127).contains(&codigo),
        "{}",
        c.id
    );
    codigo
}

/// `f` para cada esquema, en hilos (como mucho 8 a la vez), en orden.
pub fn en_paralelo<T: Send>(grupos: &[Grupo], f: impl Fn(usize, &Grupo) -> T + Sync) -> Vec<T> {
    let hilos = std::thread::available_parallelism().map_or(2, |n| n.get().min(8));
    let f = &f;
    std::thread::scope(|s| {
        let mut out = Vec::new();
        for lote in grupos.iter().enumerate().collect::<Vec<_>>().chunks(hilos) {
            let hs: Vec<_> = lote
                .iter()
                .map(|&(k, g)| s.spawn(move || f(k, g)))
                .collect();
            out.extend(hs.into_iter().map(|h| h.join().unwrap()));
        }
        out
    })
}

/// Cada payload del corpus por stdin al binario de su esquema
/// ([`comprobar_salida`]); salen los códigos 0, 1 y 2 (también por un
/// `limit`), y con stdin ilegible (un directorio) el binario da 3 con el
/// motivo en stderr.
pub fn corpus_por_stdin(grupos: &[Grupo], binarios: &[PathBuf]) {
    let mut codigos = BTreeSet::new();
    let mut limite = false;
    let mut n = 0;
    for (g, bin) in grupos.iter().zip(binarios) {
        for (c, p) in &g.casos {
            let codigo = comprobar_salida(g, c, p, ejecutar(bin, p));
            codigos.insert(codigo);
            limite |= c.expected.error_kind == Some(Esperado::Limit);
            n += 1;
        }
    }
    assert_eq!(codigos, BTreeSet::from([0, 1, 2]));
    assert!(limite);
    assert!(n >= 70, "{n}");
    let dir = std::fs::File::open(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let out = Command::new(&binarios[0])
        .stdin(Stdio::from(dir))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(i32::from(salida::INTERNO)));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no se pudo leer la entrada"));
}
