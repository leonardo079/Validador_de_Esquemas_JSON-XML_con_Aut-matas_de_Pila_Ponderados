//! Fase 6 — Instrumentación (F6.5, D65) y benchmarks (F6.6, D58).
//!
//! Este módulo tiene lo que el arnés `metricas` (`src/bin/metricas.rs`) mide y
//! las pruebas comprueban sin reloj (D1):
//!
//! - [`automata`]: esquema → IR verificada → M, la misma tubería que `schemac`
//!   y el servidor (D55), en un hilo con pila de 8 MiB (D7);
//! - [`validar_dpda`]: M sobre el payload en bloques, con la memoria fija que
//!   dan los límites por defecto (`state_words`; no construye árbol), y
//!   [`validar_linea_base`]: la línea base DOM de F6.7 (O(n));
//! - [`profundidad`]: anidamiento máximo de un payload, sin recursión;
//! - [`bombeo`]: las familias u·vᵏ·x·yᵏ·z y de longitud del plan («Pruebas de
//!   bombeo, cómo hacerlas»), con su esquema;
//! - [`Medida`] y [`tabla_markdown`]/[`csv`]: el informe.
//!
//! Teoría y cifras: `docs/fase6/metricas.md`.

use automaton::{Automaton, PayloadFormat};
use ir::VerifiedIr;
use runtime_rs::{Flow, JsonValidator, Mode, Options, XmlValidator};

pub mod generar;

/// Pila de los hilos que recorren entrada del usuario (D7), como `schemac`.
pub const STACK_SIZE: usize = 8 << 20;

/// Tamaño de bloque con el que se alimenta el DPDA (`sv_feed`), como en las
/// pruebas de bloques del proyecto (1, 7 y 4096).
pub const BLOQUE: usize = 4096;

/// Ejecuta `f` en un hilo con pila de 8 MiB (D7).
fn en_hilo<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(f)
        .map_err(|e| format!("no se pudo crear el hilo: {e}"))?
        .join()
        .unwrap_or_else(|_| Err("pánico al compilar el esquema".into()))
}

/// Compila un esquema (JSON Schema o XSD, detectado por contenido, D22) a su
/// autómata M. El error es el texto que daría `schemac`.
pub fn automata(esquema: &[u8]) -> Result<Automaton, String> {
    let esquema = esquema.to_vec();
    en_hilo(move || automaton::build(&bajar(&esquema)?).map_err(|e| e.to_string()))
}

/// La IR verificada de un esquema y su autómata (F5.4 necesita las dos).
pub fn ir_y_automata(esquema: &[u8]) -> Result<(VerifiedIr, Automaton), String> {
    let esquema = esquema.to_vec();
    en_hilo(move || {
        let ir = bajar(&esquema)?;
        let m = automaton::build(&ir).map_err(|e| e.to_string())?;
        Ok((ir, m))
    })
}

fn bajar(esquema: &[u8]) -> Result<VerifiedIr, String> {
    Ok(if es_xsd(esquema) {
        let s = xsd::parse(esquema).map_err(|e| e.to_string())?;
        let a = xsd::analyze(&s).map_err(|e| primero(&e))?;
        xsd::lower(&a).map_err(|e| e.to_string())?
    } else {
        let s = schema_parser::parse(esquema).map_err(|e| e.to_string())?;
        // `lower` exige la tabla de `analyze` (`docs/fase3/pendientes.md` §13).
        let t = semantic::analyze(&s).map_err(|e| primero(&e))?;
        semantic::lower(&t).map_err(|e| e.to_string())?
    })
}

fn primero<E: std::fmt::Display>(errores: &[E]) -> String {
    match errores.first() {
        Some(e) if errores.len() > 1 => format!("{e} (y {} más)", errores.len() - 1),
        Some(e) => e.to_string(),
        None => "esquema no válido".into(),
    }
}

/// D22: tras un BOM y espacios, `<` es XSD.
fn es_xsd(b: &[u8]) -> bool {
    let b = b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b);
    b.iter()
        .find(|c| !matches!(c, b' ' | b'\t' | b'\r' | b'\n'))
        == Some(&b'<')
}

/// Anidamiento máximo de `payload` (contenedores JSON o elementos XML), en
/// una pasada y sin recursión. Para JSON salta el contenido de las cadenas;
/// para XML cuenta `<x` que abren y `</x` o `/>` que cierran, y salta
/// comentarios, CDATA e instrucciones de procesamiento.
pub fn profundidad(payload: &[u8], formato: PayloadFormat) -> usize {
    match formato {
        PayloadFormat::Json => profundidad_json(payload),
        PayloadFormat::Xml => profundidad_xml(payload),
    }
}

fn profundidad_json(p: &[u8]) -> usize {
    let (mut d, mut max, mut en_cadena, mut escape) = (0usize, 0usize, false, false);
    for &b in p {
        if en_cadena {
            match (escape, b) {
                (true, _) => escape = false,
                (false, b'\\') => escape = true,
                (false, b'"') => en_cadena = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => en_cadena = true,
            b'{' | b'[' => {
                d += 1;
                max = max.max(d);
            }
            b'}' | b']' => d = d.saturating_sub(1),
            _ => {}
        }
    }
    max
}

fn profundidad_xml(p: &[u8]) -> usize {
    let (mut d, mut max, mut i) = (0usize, 0usize, 0usize);
    let salta = |i: usize, fin: &[u8]| -> usize {
        p[i..]
            .windows(fin.len())
            .position(|w| w == fin)
            .map_or(p.len(), |k| i + k + fin.len())
    };
    while i < p.len() {
        if p[i] != b'<' {
            i += 1;
            continue;
        }
        let resto = &p[i..];
        if resto.starts_with(b"<!--") {
            i = salta(i, b"-->");
        } else if resto.starts_with(b"<![CDATA[") {
            i = salta(i, b"]]>");
        } else if resto.starts_with(b"<?") || resto.starts_with(b"<!") {
            i = salta(i, b">");
        } else if resto.starts_with(b"</") {
            d = d.saturating_sub(1);
            i = salta(i, b">");
        } else {
            let fin = salta(i, b">");
            if p[..fin].ends_with(b"/>") {
                max = max.max(d + 1);
            } else {
                d += 1;
                max = max.max(d);
            }
            i = fin;
        }
    }
    max
}

/// Resultado de una validación, reducido a lo que se compara.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Veredicto {
    pub valido: bool,
    /// Violaciones (peso tropical para el DPDA; errores de `jsonschema`
    /// para la línea base); `None` si el payload es malformado o pasa un
    /// límite (peso ∞).
    pub violaciones: Option<u64>,
}

/// Valida con M. La memoria del ejecutor se reserva aquí, una vez, con el
/// tamaño que fijan los límites por defecto (`state_words`, F4-U18; pila de
/// 256 marcos, reserva de claves y de `uniqueItems`, `docs/00-alcance.md`
/// §4): es **toda** la memoria que usa el DPDA, de tamaño fijo como el
/// `sv_state_t` del validador generado, e independiente de la longitud y de
/// la profundidad del payload.
pub fn validar_dpda(a: &Automaton, payload: &[u8]) -> Veredicto {
    let opts = Options::new(Mode::Total);
    let mut mem = vec![0u64; a.state_words(&opts.limits)];
    let r = match a.format() {
        PayloadFormat::Json => {
            let mut v = JsonValidator::new(a.tables(), &opts, &mut mem);
            for c in payload.chunks(BLOQUE) {
                if v.feed(c) == Flow::Stop {
                    break;
                }
            }
            v.finish()
        }
        PayloadFormat::Xml => {
            let mut v = XmlValidator::new(a.tables(), &opts, &mut mem);
            for c in payload.chunks(BLOQUE) {
                if v.feed(c) == Flow::Stop {
                    break;
                }
            }
            v.finish()
        }
    };
    Veredicto {
        valido: r.valid,
        violaciones: r.violations.get(),
    }
}

/// Línea base DOM (F6.7, D48): `serde_json` + `jsonschema`. Solo JSON; la
/// de XML necesita la feature `libxml2` y Docker (D54).
pub fn validar_linea_base(
    lb: &baseline::JsonBaseline,
    stack: &baseline::Stack,
    payload: &[u8],
) -> Veredicto {
    let o = lb.validate(stack, payload);
    let malformado = matches!(
        o.kind,
        Some(baseline::Kind::Malformed | baseline::Kind::Limit | baseline::Kind::Unsupported)
    );
    Veredicto {
        valido: o.valid,
        violaciones: (!malformado).then(|| o.violation_count() as u64),
    }
}

/// Un payload generado con su esquema.
#[derive(Clone, Debug)]
pub struct Generado {
    pub nombre: String,
    pub esquema: &'static str,
    pub payload: Vec<u8>,
    /// El parámetro que crece (k o n).
    pub parametro: usize,
}

/// Familias de bombeo del plan (`tasks/plan-general.md`, «Pruebas de bombeo,
/// cómo hacerlas»), todas válidas para su esquema.
pub mod bombeo {
    use super::Generado;

    /// Arreglos anidados: el lenguaje recursivo de los arreglos de arreglos.
    pub const ESQUEMA_ANIDADO: &str =
        r##"{"$defs":{"n":{"type":"array","items":{"$ref":"#/$defs/n"}}},"$ref":"#/$defs/n"}"##;
    /// Arreglo de enteros.
    pub const ESQUEMA_ENTEROS: &str = r#"{"type":"array","items":{"type":"integer"}}"#;
    /// Arreglo de objetos con una propiedad requerida.
    pub const ESQUEMA_OBJETOS: &str = r#"{"type":"array","items":{"type":"object","properties":{"a":{"type":"integer"}},"required":["a"],"additionalProperties":false}}"#;

    /// u·vᵏ·x·yᵏ·z con u = z = ε, v = `[`, x = ε, y = `]`: profundidad k.
    pub fn anidado(k: usize) -> Generado {
        let mut p = Vec::with_capacity(2 * k);
        p.extend(std::iter::repeat_n(b'[', k));
        p.extend(std::iter::repeat_n(b']', k));
        Generado {
            nombre: format!("bombeo [^k ]^k, k={k}"),
            esquema: ESQUEMA_ANIDADO,
            payload: p,
            parametro: k,
        }
    }

    /// `[1,1,…,1]` con n elementos: profundidad 1, longitud n.
    pub fn enteros(n: usize) -> Generado {
        let mut p = Vec::with_capacity(2 * n + 2);
        p.push(b'[');
        for i in 0..n {
            if i > 0 {
                p.push(b',');
            }
            p.push(b'1');
        }
        p.push(b']');
        Generado {
            nombre: format!("arreglo de enteros, n={n}"),
            esquema: ESQUEMA_ENTEROS,
            payload: p,
            parametro: n,
        }
    }

    /// `[{"a":1},…]` con n objetos: profundidad 2, n nodos que la línea base
    /// guarda en su DOM.
    pub fn objetos(n: usize) -> Generado {
        let mut p = Vec::with_capacity(8 * n + 2);
        p.push(b'[');
        for i in 0..n {
            if i > 0 {
                p.push(b',');
            }
            p.extend_from_slice(br#"{"a":1}"#);
        }
        p.push(b']');
        Generado {
            nombre: format!("arreglo de objetos, n={n}"),
            esquema: ESQUEMA_OBJETOS,
            payload: p,
            parametro: n,
        }
    }
}

/// Validador medido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validador {
    Dpda,
    LineaBase,
}

impl Validador {
    pub fn nombre(self) -> &'static str {
        match self {
            Validador::Dpda => "DPDA",
            Validador::LineaBase => "línea base DOM",
        }
    }
}

/// Una fila del informe.
#[derive(Clone, Debug, PartialEq)]
pub struct Medida {
    pub caso: String,
    pub validador: Validador,
    pub bytes: usize,
    pub profundidad: usize,
    pub veredicto: Veredicto,
    /// Mediana de las repeticiones, en segundos.
    pub segundos: f64,
    /// Asignaciones (bloques) de una validación.
    pub asignaciones: u64,
    /// Pico de memoria de montón durante una validación, en bytes.
    pub pico_bytes: usize,
    /// Tamaño de M (solo DPDA).
    pub estados: Option<usize>,
    pub transiciones: Option<usize>,
}

impl Medida {
    pub fn mb_s(&self) -> f64 {
        if self.segundos > 0.0 {
            self.bytes as f64 / self.segundos / 1e6
        } else {
            f64::INFINITY
        }
    }

    pub fn ns_por_byte(&self) -> f64 {
        self.segundos * 1e9 / (self.bytes.max(1) as f64)
    }
}

fn opt(v: Option<usize>) -> String {
    v.map_or_else(|| "—".into(), |x| x.to_string())
}

fn veredicto(v: Veredicto) -> String {
    match (v.valido, v.violaciones) {
        (true, _) => "válido".into(),
        (false, Some(n)) => format!("{n} violaciones"),
        (false, None) => "malformado o límite".into(),
    }
}

/// Tabla Markdown del informe (para `docs/` y el informe final).
pub fn tabla_markdown(medidas: &[Medida]) -> String {
    let mut s = String::from(
        "| Caso | Validador | Bytes | Prof. | Resultado | MB/s | ns/byte | Asignaciones | Pico de memoria | Estados | Transiciones |\n\
         | --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for m in medidas {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.1} | {:.2} | {} | {} | {} | {} |\n",
            m.caso.replace('|', "\\|"),
            m.validador.nombre(),
            m.bytes,
            m.profundidad,
            veredicto(m.veredicto),
            m.mb_s(),
            m.ns_por_byte(),
            m.asignaciones,
            legible(m.pico_bytes),
            opt(m.estados),
            opt(m.transiciones),
        ));
    }
    s
}

/// CSV del informe (una fila por medida), para graficar (E1).
pub fn csv(medidas: &[Medida]) -> String {
    let mut s = String::from(
        "caso,validador,bytes,profundidad,valido,violaciones,segundos,mb_s,ns_por_byte,asignaciones,pico_bytes,estados,transiciones\n",
    );
    for m in medidas {
        s.push_str(&format!(
            "\"{}\",{},{},{},{},{},{:e},{:.3},{:.4},{},{},{},{}\n",
            m.caso.replace('"', "\"\""),
            m.validador.nombre(),
            m.bytes,
            m.profundidad,
            m.veredicto.valido,
            m.veredicto
                .violaciones
                .map_or_else(String::new, |v| v.to_string()),
            m.segundos,
            m.mb_s(),
            m.ns_por_byte(),
            m.asignaciones,
            m.pico_bytes,
            m.estados.map_or_else(String::new, |v| v.to_string()),
            m.transiciones.map_or_else(String::new, |v| v.to_string()),
        ));
    }
    s
}

/// Bytes en B, KiB o MiB.
pub fn legible(b: usize) -> String {
    match b {
        0..=1023 => format!("{b} B"),
        1024..=1_048_575 => format!("{:.1} KiB", b as f64 / 1024.0),
        _ => format!("{:.1} MiB", b as f64 / 1_048_576.0),
    }
}
