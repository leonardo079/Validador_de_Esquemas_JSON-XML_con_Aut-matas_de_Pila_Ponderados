//! Emisor Rust (F5.1, D74; `docs/fase5/codegen.md`).
//!
//! [`emit`] da los archivos de un validador autocontenido:
//!
//! | Archivo | Contenido |
//! | --- | --- |
//! | `main.rs` | raíz `std`: stdin por bloques → una línea JSON en stdout y el código de salida |
//! | `validador.rs` | fachada `no_std`: `STATE_WORDS`, `SV_MAX_DEPTH`, `Validador`, `validar` |
//! | `tablas.rs` | las tablas de M como `static` (densas, D46 (1)) |
//! | `lexer*.rs`, `runtime_rs*.rs` | las fuentes copiadas ([`crate::fuentes`]) |
//!
//! La salida es determinista: solo depende de M (recorre las tablas en
//! orden, sin mapas ni fechas ni rutas del anfitrión).

use std::fmt::Write;

use automaton::{Automaton, Limits, PayloadFormat};
use runtime_rs::tables::{
    CharRange, Check, CountOp, CounterOp, Dep, DivisorRef, Fault, Frame, Gate, GateKind, Interval,
    Lexeme, Op, OutMap, Rule, Test, Trans,
};
use runtime_rs::Tables;

use crate::fuentes::COPIADOS;
use crate::Archivo;

/// Bytes que `main` lee de stdin por llamada (y entrega al validador como un
/// bloque).
pub const BLOQUE_STDIN: usize = 64 * 1024;

/// Códigos de salida del validador generado (`docs/fase5/codegen.md` §3).
/// Ninguno choca con los reservados por el sandbox (100-103, 124-127; D68).
pub mod salida {
    /// Bien formado y peso 0.
    pub const VALIDO: u8 = 0;
    /// Bien formado con violaciones (`schema`).
    pub const INVALIDO: u8 = 1;
    /// Malformado (`lexical`, `syntax`) o un límite (`limit`).
    pub const MALFORMADO: u8 = 2;
    /// Error interno: tablas incoherentes, o no se pudo leer stdin o
    /// escribir stdout (como el 3 de `schemac`, D33).
    pub const INTERNO: u8 = 3;
}

/// Los archivos del validador Rust de `a`, en orden: `main.rs`,
/// `validador.rs`, `tablas.rs` y las fuentes copiadas.
pub fn emit(a: &Automaton) -> Vec<Archivo> {
    let mut v = vec![
        Archivo {
            nombre: "main.rs".into(),
            contenido: main_rs(),
        },
        Archivo {
            nombre: "validador.rs".into(),
            contenido: validador_rs(a),
        },
        Archivo {
            nombre: "tablas.rs".into(),
            contenido: tablas_rs(a),
        },
    ];
    for c in COPIADOS {
        for (archivo, fuente) in c.archivos {
            v.push(Archivo {
                nombre: c.nombre(archivo),
                contenido: format!(
                    "// Copia generada por schemac (F5.1) de `{}/{archivo}.rs`; no editar.\n{}",
                    c.origen,
                    c.reescribir(archivo, fuente)
                ),
            });
        }
    }
    v
}

const CABECERA: &str = "// Generado por schemac (F5.1, `docs/fase5/codegen.md`); no editar.\n";

fn main_rs() -> String {
    let mut s = String::from(CABECERA);
    s.push_str(MAIN);
    s.replace("{{BLOQUE}}", &BLOQUE_STDIN.to_string())
        .replace("{{INTERNO}}", &salida::INTERNO.to_string())
}

/// `main.rs`. La memoria del núcleo es un `static` (en `.bss`: no ocupa la
/// pila del hilo principal ni reserva); el `Mutex` solo existe porque un
/// `static` mutable exigiría `unsafe`.
const MAIN: &str = r#"//! Validador generado: lee el payload de stdin en bloques, escribe el
//! reporte en una línea JSON en stdout y sale con 0 (válido), 1 (inválido),
//! 2 (malformado o límite) o 3 (error interno o de E/S).
//!
//! Compilar: `rustc --edition 2021 -O main.rs` (sin Cargo ni red).
#![allow(dead_code, unused_imports)]

mod lexer;
mod runtime_rs;
mod tablas;
mod validador;

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::sync::Mutex;

use runtime_rs::{Flow, Mode};
use validador::{Validador, STATE_WORDS};

/// Bytes por lectura de stdin.
const BLOQUE: usize = {{BLOQUE}};
/// Código de salida de un error interno o de E/S.
const INTERNO: u8 = {{INTERNO}};

/// Memoria fija del núcleo: pila de marcos y reservas de hashes
/// (`Tables::state_words` con los límites por defecto).
static MEMORIA: Mutex<[u64; STATE_WORDS]> = Mutex::new([0; STATE_WORDS]);

fn main() -> ExitCode {
    ExitCode::from(ejecutar())
}

fn ejecutar() -> u8 {
    let mut mem = MEMORIA.lock().unwrap_or_else(|e| e.into_inner());
    let mut bloque = [0u8; BLOQUE];
    let mut entrada = io::stdin().lock();
    let mut v = Validador::new(&mut mem, Mode::Total);
    loop {
        match entrada.read(&mut bloque) {
            Ok(0) => break,
            Ok(n) => {
                if v.feed(&bloque[..n]) == Flow::Stop {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => {
                let _ = writeln!(io::stderr(), "validador: no se pudo leer la entrada: {e}");
                return INTERNO;
            }
        }
    }
    let r = v.finish();
    let mut linea = String::new();
    if validador::escribir_json(&r, &mut linea).is_err() {
        return INTERNO;
    }
    linea.push('\n');
    let mut out = io::stdout().lock();
    if out.write_all(linea.as_bytes()).and_then(|()| out.flush()).is_err() {
        return INTERNO;
    }
    validador::codigo_de_salida(&r)
}
"#;

fn validador_rs(a: &Automaton) -> String {
    let (formato, tipo) = match a.format() {
        PayloadFormat::Json => ("json", "JsonValidator"),
        PayloadFormat::Xml => ("xml", "XmlValidator"),
    };
    let mut s = String::from(CABECERA);
    s.push_str(VALIDADOR);
    s.replace("{{FORMATO}}", formato)
        .replace("{{TIPO}}", tipo)
        .replace(
            "{{STATE_WORDS}}",
            &a.state_words(&Limits::DEFAULT).to_string(),
        )
        .replace("{{SV_MAX_DEPTH}}", &Limits::DEFAULT.max_depth.to_string())
        .replace("{{VALIDO}}", &salida::VALIDO.to_string())
        .replace("{{INVALIDO}}", &salida::INVALIDO.to_string())
        .replace("{{MALFORMADO}}", &salida::MALFORMADO.to_string())
        .replace("{{INTERNO}}", &salida::INTERNO.to_string())
}

/// `validador.rs`: la fachada. Solo usa `core` (lo comprueba una prueba
/// compilándola como `#![no_std]`).
const VALIDADOR: &str = r#"//! Fachada del validador generado (sin `std` ni `alloc`): las tablas de
//! `tablas.rs` delante del núcleo de `runtime_rs`, con memoria fija.

use core::fmt::{self, Write};

pub use crate::runtime_rs::{ErrorKind, Flow, Limits, Mode, Options, Report};
use crate::runtime_rs::{{TIPO}} as Nucleo;

use super::tablas::{PALABRAS, TABLAS};

/// Formato del payload que valida (`json` o `xml`).
pub const FORMATO: &str = "{{FORMATO}}";

/// Profundidad máxima de anidamiento: la pila de marcos tiene sitio para
/// `SV_MAX_DEPTH` marcos; el siguiente es `limit`.
pub const SV_MAX_DEPTH: usize = {{SV_MAX_DEPTH}};

/// Palabras `u64` de memoria del núcleo con los límites por defecto
/// (`Tables::state_words(&Limits::DEFAULT)`).
pub const STATE_WORDS: usize = {{STATE_WORDS}};

const _: () = assert!(SV_MAX_DEPTH == Limits::DEFAULT.max_depth);

/// Validador en streaming: `feed` por bloques de cualquier tamaño y
/// `finish`. No reserva memoria: usa la que se le presta.
#[derive(Debug)]
pub struct Validador<'m> {
    v: Nucleo<'static, 'm>,
}

impl<'m> Validador<'m> {
    pub fn new(mem: &'m mut [u64; STATE_WORDS], modo: Mode) -> Validador<'m> {
        Validador {
            v: Nucleo::new(TABLAS, &Options::new(modo), mem),
        }
    }

    pub fn feed(&mut self, bloque: &[u8]) -> Flow {
        self.v.feed(bloque)
    }

    pub fn finish(self) -> Report {
        self.v.finish()
    }
}

/// Valida `payload` entero, en conformidad total, con la memoria en la pila
/// (`STATE_WORDS` × 8 bytes).
pub fn validar(payload: &[u8]) -> Report {
    validar_por_bloques(payload, usize::MAX, Mode::Total)
}

/// Valida `payload` en bloques de `bloque` bytes (≥ 1), con la memoria en la
/// pila.
pub fn validar_por_bloques(payload: &[u8], bloque: usize, modo: Mode) -> Report {
    let mut mem = [0u64; STATE_WORDS];
    validar_en(&mut mem, payload, bloque, modo)
}

/// Como [`validar_por_bloques`], con la memoria del llamador.
pub fn validar_en(mem: &mut [u64; STATE_WORDS], payload: &[u8], bloque: usize, modo: Mode) -> Report {
    let mut v = Validador::new(mem, modo);
    for c in payload.chunks(bloque.max(1)) {
        if v.feed(c) == Flow::Stop {
            break;
        }
    }
    v.finish()
}

/// Código de salida del proceso para `r`: {{VALIDO}} válido, {{INVALIDO}} inválido,
/// {{MALFORMADO}} malformado o límite, {{INTERNO}} error interno.
pub fn codigo_de_salida(r: &Report) -> u8 {
    match (r.valid, r.error_kind) {
        (true, _) => {{VALIDO}},
        (false, Some(ErrorKind::Schema)) => {{INVALIDO}},
        (false, Some(ErrorKind::Lexical | ErrorKind::Syntax | ErrorKind::Limit)) => {{MALFORMADO}},
        (false, Some(ErrorKind::Internal) | None) => {{INTERNO}},
    }
}

/// Nombre de la clase de error (`expected.errorKind` del corpus).
pub fn clase(k: ErrorKind) -> &'static str {
    match k {
        ErrorKind::Lexical => "lexical",
        ErrorKind::Syntax => "syntax",
        ErrorKind::Schema => "schema",
        ErrorKind::Limit => "limit",
        ErrorKind::Internal => "internal",
    }
}

/// El reporte en una línea JSON (sin el salto final), con los campos del
/// DPDA de `/api/compare`: `valid`, `violations` (`null` si es ∞),
/// `errorKind`, `offset`, `line`, `column`, `rule` (ubicación de la regla
/// en el esquema) y `keyword`.
pub fn escribir_json(r: &Report, out: &mut impl Write) -> fmt::Result {
    write!(out, "{{\"valid\":{},\"violations\":", r.valid)?;
    match r.violations.get() {
        Some(n) => write!(out, "{n}")?,
        None => out.write_str("null")?,
    }
    out.write_str(",\"errorKind\":")?;
    match r.error_kind {
        Some(k) => write!(out, "\"{}\"", clase(k))?,
        None => out.write_str("null")?,
    }
    match r.first {
        Some(f) => write!(out, ",\"offset\":{},\"line\":{},\"column\":{}", f.offset, f.line, f.column)?,
        None => out.write_str(",\"offset\":null,\"line\":null,\"column\":null")?,
    }
    out.write_str(",\"rule\":")?;
    let regla = r.first.and_then(|f| f.rule);
    match regla {
        Some(k) => cadena(TABLAS.location(k), out)?,
        None => out.write_str("null")?,
    }
    out.write_str(",\"keyword\":")?;
    match regla.and_then(|k| PALABRAS.get(k as usize)) {
        Some(p) => cadena(p.as_bytes(), out)?,
        None => out.write_str("null")?,
    }
    out.write_char('}')
}

/// Cadena JSON de `bytes`: escapa `"`, `\` y los controles; un byte que no
/// es UTF-8 válido sale como U+FFFD.
fn cadena(bytes: &[u8], out: &mut impl Write) -> fmt::Result {
    out.write_char('"')?;
    for trozo in bytes.utf8_chunks() {
        for c in trozo.valid().chars() {
            match c {
                '"' => out.write_str("\\\"")?,
                '\\' => out.write_str("\\\\")?,
                c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32)?,
                c => out.write_char(c)?,
            }
        }
        if !trozo.invalid().is_empty() {
            out.write_str("\\ufffd")?;
        }
    }
    out.write_char('"')
}
"#;

/// `tablas.rs`: cada tabla de [`Tables`] como un `static` de literales.
fn tablas_rs(a: &Automaton) -> String {
    let t = a.tables();
    // Exhaustivo: un campo nuevo en `Tables` rompe la compilación aquí.
    let Tables {
        n_classes,
        start,
        accept,
        delta,
        trans,
        checks,
        frames,
        masks,
        deps,
        nums,
        lexemes,
        divisors,
        counter_ops,
        intervals,
        gates,
        gate_inputs,
        outmaps,
        out_targets,
        scalar_slots,
        ascii,
        ranges,
        rules,
        text,
    } = t;
    let s = a.summary();
    let mut o = String::from(CABECERA);
    let _ = write!(
        o,
        "//! Tablas densas de M (D46 (1)) para payloads {}: {} estados, |Σ| = {}, {} transiciones,\n\
         //! {} tipos de marco.\n\n",
        a.format().name(),
        s.states,
        s.classes,
        s.transitions,
        s.frames
    );
    o.push_str(
        "use crate::runtime_rs::num::Divisor;\n\
         use crate::runtime_rs::tables::{\n    \
         CharRange, Check, CountOp, CounterOp, Dep, DivisorRef, Fault, Frame, Gate, GateKind,\n    \
         Interval, Lexeme, Op, OutMap, Rule, Tables, Test, Trans,\n};\n\n\
         /// `u32::MAX` (`NO_BIT`, `OUT_IDENTITY`).\n\
         const X: u32 = u32::MAX;\n\n",
    );
    let _ = writeln!(
        o,
        "pub static TABLAS: Tables<'static> = Tables {{\n    \
         n_classes: {n_classes},\n    start: {start},\n    accept: {accept},\n    \
         delta: &DELTA,\n    trans: &TRANS,\n    checks: &CHECKS,\n    frames: &FRAMES,\n    \
         masks: &MASKS,\n    deps: &DEPS,\n    nums: &NUMS,\n    lexemes: &LEXEMES,\n    \
         divisors: &DIVISORS,\n    counter_ops: &COUNTER_OPS,\n    intervals: &INTERVALS,\n    \
         gates: &GATES,\n    gate_inputs: &GATE_INPUTS,\n    outmaps: &OUTMAPS,\n    \
         out_targets: &OUT_TARGETS,\n    scalar_slots: {scalar_slots},\n    ascii: &ASCII,\n    \
         ranges: &RANGES,\n    rules: &RULES,\n    text: &TEXT,\n}};\n"
    );
    o.push_str(
        "const fn t(op: Op, next: u32, ret: u32, frame: u32, bit: u32, mark: u32, counts: u32,\n    \
         n_counts: u32, checks: u32, n_checks: u32, outs: u32) -> Trans {\n    \
         Trans { op, next, ret, frame, bit, mark, counts, n_counts, checks, n_checks, outs }\n}\n\n\
         const fn c(test: Test, rule: u32, cost: u32, slot: u32, counter: u32, arg: u32, len: u32) -> Check {\n    \
         Check { test, rule, cost, slot, counter, arg, len }\n}\n\n",
    );

    numeros(&mut o, "DELTA", "u32", delta);
    lista(&mut o, "TRANS", "Trans", trans, trans_lit);
    lista(&mut o, "CHECKS", "Check", checks, check_lit);
    lista(&mut o, "FRAMES", "Frame", frames, frame_lit);
    let hex: Vec<String> = masks.iter().map(|m| format!("{m:#x}")).collect();
    lista(&mut o, "MASKS", "u64", &hex, |s| s.clone());
    lista(&mut o, "DEPS", "Dep", deps, |d| {
        let Dep { bit, mask } = *d;
        format!("Dep {{ bit: {}, mask: {mask} }}", n(bit))
    });
    lista(&mut o, "NUMS", "Lexeme", nums, |l| {
        let Lexeme { off, len } = *l;
        format!("Lexeme {{ off: {off}, len: {len} }}")
    });
    bytes(&mut o, "LEXEMES", lexemes);
    lista(&mut o, "DIVISORS", "DivisorRef", divisors, |d| {
        let DivisorRef { lexeme, div } = *d;
        let runtime_rs::num::Divisor {
            coprime,
            twos,
            fives,
        } = div;
        format!(
            "DivisorRef {{ lexeme: {lexeme}, div: Divisor {{ coprime: {coprime}, twos: {twos}, fives: {fives} }} }}"
        )
    });
    lista(&mut o, "COUNTER_OPS", "CounterOp", counter_ops, |c| {
        let CounterOp { counter, op } = *c;
        let op = match op {
            CountOp::Add => "Add",
            CountOp::Restart => "Restart",
        };
        format!("CounterOp {{ counter: {counter}, op: CountOp::{op} }}")
    });
    lista(&mut o, "INTERVALS", "Interval", intervals, |i| {
        let Interval { lo, hi, closed } = *i;
        format!("Interval {{ lo: {lo}, hi: {hi}, closed: {closed} }}")
    });
    lista(&mut o, "GATES", "Gate", gates, gate_lit);
    numeros(&mut o, "GATE_INPUTS", "u32", gate_inputs);
    lista(&mut o, "OUTMAPS", "OutMap", outmaps, |m| {
        let OutMap { off, len } = *m;
        format!("OutMap {{ off: {off}, len: {len} }}")
    });
    numeros(&mut o, "OUT_TARGETS", "u32", out_targets);
    numeros(&mut o, "ASCII", "u32", ascii);
    lista(&mut o, "RANGES", "CharRange", ranges, |r| {
        let CharRange { lo, hi, class } = *r;
        format!("CharRange {{ lo: {lo}, hi: {hi}, class: {class} }}")
    });
    lista(&mut o, "RULES", "Rule", rules, |r| {
        let Rule {
            node,
            kind,
            loc,
            loc_len,
        } = *r;
        format!("Rule {{ node: {node}, kind: {kind}, loc: {loc}, loc_len: {loc_len} }}")
    });
    bytes(&mut o, "TEXT", text);

    o.push_str("/// Palabra clave de cada regla (`RULES`), para el reporte.\n");
    let palabras: Vec<String> = (0..rules.len())
        .map(|k| format!("{:?}", a.rule(k as u32).1.name()))
        .collect();
    let _ = writeln!(o, "pub static PALABRAS: [&str; {}] = [", palabras.len());
    for p in &palabras {
        let _ = writeln!(o, "    {p},");
    }
    o.push_str("];\n");
    o
}

/// Un `u32` de las tablas: `X` si es `u32::MAX`.
fn n(v: u32) -> String {
    if v == u32::MAX {
        "X".into()
    } else {
        v.to_string()
    }
}

/// `static NOMBRE: [u32; k] = [...]`, 16 por línea.
fn numeros(o: &mut String, nombre: &str, tipo: &str, v: &[u32]) {
    let _ = writeln!(o, "static {nombre}: [{tipo}; {}] = [", v.len());
    for fila in v.chunks(16) {
        let fila: Vec<String> = fila.iter().map(|&x| n(x)).collect();
        let _ = writeln!(o, "    {},", fila.join(", "));
    }
    o.push_str("];\n\n");
}

/// `static NOMBRE: [T; k] = [...]`, un elemento por línea.
fn lista<T>(o: &mut String, nombre: &str, tipo: &str, v: &[T], lit: impl Fn(&T) -> String) {
    let _ = writeln!(o, "static {nombre}: [{tipo}; {}] = [", v.len());
    for x in v {
        let _ = writeln!(o, "    {},", lit(x));
    }
    o.push_str("];\n\n");
}

/// `static NOMBRE: [u8; k] = *b"...";` con los bytes no imprimibles
/// escapados.
fn bytes(o: &mut String, nombre: &str, v: &[u8]) {
    let _ = write!(o, "static {nombre}: [u8; {}] = *b\"", v.len());
    for &b in v {
        match b {
            b'"' => o.push_str("\\\""),
            b'\\' => o.push_str("\\\\"),
            0x20..=0x7e => o.push(b as char),
            _ => {
                let _ = write!(o, "\\x{b:02x}");
            }
        }
    }
    o.push_str("\";\n\n");
}

fn op_lit(op: Op) -> String {
    match op {
        Op::Move => "Op::Move".into(),
        Op::Push => "Op::Push".into(),
        Op::Pop => "Op::Pop".into(),
        Op::Enter => "Op::Enter".into(),
        Op::Leave => "Op::Leave".into(),
        Op::Fail(f) => {
            let f = match f {
                Fault::Syntax => "Syntax",
                Fault::Duplicate => "Duplicate",
                Fault::Limit => "Limit",
                Fault::Internal => "Internal",
            };
            format!("Op::Fail(Fault::{f})")
        }
    }
}

fn trans_lit(t: &Trans) -> String {
    let Trans {
        op,
        next,
        ret,
        frame,
        bit,
        mark,
        counts,
        n_counts,
        checks,
        n_checks,
        outs,
    } = *t;
    format!(
        "t({}, {next}, {ret}, {frame}, {}, {}, {counts}, {n_counts}, {checks}, {n_checks}, {})",
        op_lit(op),
        n(bit),
        n(mark),
        n(outs)
    )
}

fn test_lit(t: Test) -> &'static str {
    match t {
        Test::Fails => "Fails",
        Test::Integer => "Integer",
        Test::Mask => "Mask",
        Test::Dependent => "Dependent",
        Test::CountMin => "CountMin",
        Test::CountMax => "CountMax",
        Test::Unset => "Unset",
        Test::LengthMin => "LengthMin",
        Test::LengthMax => "LengthMax",
        Test::NumMin => "NumMin",
        Test::NumExMin => "NumExMin",
        Test::NumMax => "NumMax",
        Test::NumExMax => "NumExMax",
        Test::NumEnum => "NumEnum",
        Test::MultipleOf => "MultipleOf",
        Test::Unique => "Unique",
        Test::NumIn => "NumIn",
    }
}

fn check_lit(c: &Check) -> String {
    let Check {
        test,
        rule,
        cost,
        slot,
        counter,
        arg,
        len,
    } = *c;
    format!(
        "c(Test::{}, {rule}, {cost}, {slot}, {counter}, {}, {})",
        test_lit(test),
        n(arg),
        n(len)
    )
}

fn frame_lit(f: &Frame) -> String {
    let Frame {
        words,
        mask_words,
        slots,
        gates,
        n_gates,
        unique,
    } = *f;
    format!(
        "Frame {{ words: {words}, mask_words: {mask_words}, slots: {slots}, gates: {gates}, \
         n_gates: {n_gates}, unique: {unique} }}"
    )
}

fn gate_lit(g: &Gate) -> String {
    let Gate {
        kind,
        rule,
        target,
        inputs,
        n_inputs,
        bit,
    } = *g;
    let kind = match kind {
        GateKind::Any => "Any",
        GateKind::One => "One",
        GateKind::Not => "Not",
        GateKind::Implies => "Implies",
    };
    format!(
        "Gate {{ kind: GateKind::{kind}, rule: {rule}, target: {target}, inputs: {inputs}, \
         n_inputs: {n_inputs}, bit: {} }}",
        n(bit)
    )
}
