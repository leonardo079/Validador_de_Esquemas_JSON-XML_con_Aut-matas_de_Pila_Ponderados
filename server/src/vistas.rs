//! F7.6 — Las vistas de la web (D70; `docs/fase7/vistas.md`): el grafo de M
//! para Cytoscape, el árbol de derivación de un payload (F5.5) y la
//! comparación en vivo DPDA frente a la línea base DOM (D48).
//!
//! Corren en el hilo de la petición (`api::procesar`: pila de 8 MiB, permiso
//! de concurrencia y tope de tiempo) y devuelven el JSON ya escrito.

use std::time::Instant;

use automaton::{Automaton, BuildError, ErrorKind, Mode, Options};
use serde_json::{json, Value};

use crate::compilar::{self, Compilado, Formato};

/// Nodos y aristas que se mandan a Cytoscape: con más, el dibujo no se lee y
/// el navegador se arrastra. Si M es mayor, `truncated: true`.
pub const MAX_NODOS: usize = 400;
pub const MAX_ARISTAS: usize = 1500;

/// Errores de la línea base que se devuelven.
const MAX_ERRORES_LB: usize = 20;

/// Por qué una vista no da 200.
#[derive(Debug)]
pub enum Fallo {
    /// El esquema no compila (422) o error interno (500).
    Compilacion(Compilado),
    /// M no se puede construir: regla pendiente o límite (422).
    SinAutomata {
        status: &'static str,
        message: String,
    },
    /// Cuerpo mal formado (400).
    Peticion(String),
    /// El payload no tiene árbol (422), con su JSON.
    Payload(Value),
    /// El servidor no tiene sandbox (503, F7.5).
    SinSandbox,
    /// El sandbox falló o no está disponible (502; el detalle, al registro).
    Sandbox(String),
    /// El código generado no compiló en el sandbox (500 `codegen`, D82 (4)): es
    /// un fallo del emisor, no del sandbox. El detalle, al registro.
    Codegen(String),
}

pub(crate) fn construir(esquema: &[u8]) -> Result<(Formato, Automaton), Fallo> {
    let (f, _, ir) = compilar::ir_de(esquema).map_err(Fallo::Compilacion)?;
    match automaton::build(&ir) {
        Ok(a) => Ok((f, a)),
        Err(e @ BuildError::Pending { .. }) => Err(Fallo::SinAutomata {
            status: "pending",
            message: e.to_string(),
        }),
        Err(e @ (BuildError::Limit { .. } | BuildError::ProductLimit { .. })) => {
            Err(Fallo::SinAutomata {
                status: "limit",
                message: e.to_string(),
            })
        }
        Err(e @ BuildError::Internal(_)) => {
            Err(Fallo::Compilacion(Compilado::Interno(vec![e.to_string()])))
        }
    }
}

// --------------------------------------------------------------- grafo ---

/// Deshace los escapes de `esc` de `automaton::export` (`\"`, `\\`) y el
/// `\n` de las etiquetas DOT.
fn sin_escapes(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            o.push(c);
            continue;
        }
        match it.next() {
            Some('n') => o.push('\n'),
            Some(x) => o.push(x),
            None => o.push('\\'),
        }
    }
    o
}

/// El texto entre `label="` y la comilla de cierre no escapada.
fn etiqueta(linea: &str) -> Option<String> {
    let i = linea.find("label=\"")? + 7;
    let resto = &linea[i..];
    let mut fin = None;
    let mut escapado = false;
    for (k, c) in resto.char_indices() {
        match c {
            '\\' if !escapado => escapado = true,
            '"' if !escapado => {
                fin = Some(k);
                break;
            }
            _ => escapado = false,
        }
    }
    Some(sin_escapes(&resto[..fin?]))
}

/// El grafo de M desde su DOT (`Automaton::write_dot`, determinista: un
/// clúster por grupo, aristas ya agrupadas por destino y operación). Así el
/// servidor no duplica la forma de las etiquetas del crate `automaton`.
pub fn grafo_de(a: &Automaton) -> Value {
    let mut dot = String::new();
    if a.write_dot(&mut dot).is_err() {
        return json!({"error": "dot"});
    }
    let s = a.summary();
    let mut grupos: Vec<String> = Vec::new();
    let mut grupo: Option<usize> = None;
    let mut nodos: Vec<Value> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut aristas: Vec<Value> = Vec::new();
    let mut inicio = String::new();
    let mut truncado = false;
    for l in dot.lines() {
        let t = l.trim();
        if let Some(r) = t.strip_prefix("subgraph cluster_") {
            grupo = r.trim_end_matches(" {").parse().ok();
        } else if t == "}" {
            grupo = None;
        } else if t.starts_with("label=\"") && grupo.is_some() {
            grupos.push(etiqueta(t).unwrap_or_default());
        } else if let Some((origen, resto)) = t.split_once(" -> ") {
            let destino = resto.split([' ', ';']).next().unwrap_or("").to_owned();
            if origen == "inicio" {
                inicio = destino;
                continue;
            }
            if aristas.len() >= MAX_ARISTAS {
                truncado = true;
                continue;
            }
            for extremo in [origen, destino.as_str()] {
                if !ids.iter().any(|x| x == extremo) && matches!(extremo, "desapila" | "escalar") {
                    ids.push(extremo.to_owned());
                    let rotulo = if extremo == "desapila" {
                        "↩ marco"
                    } else {
                        "↩ escalar"
                    };
                    nodos.push(json!({"id": extremo, "label": rotulo, "special": true}));
                }
            }
            aristas.push(json!({
                "id": format!("e{}", aristas.len()),
                "source": origen,
                "target": destino,
                "label": etiqueta(t).unwrap_or_default(),
            }));
        } else if t.starts_with('q') && t.contains(" [label=\"") {
            let id = t.split(' ').next().unwrap_or("").to_owned();
            if nodos.len() >= MAX_NODOS {
                truncado = true;
                continue;
            }
            let texto = etiqueta(t).unwrap_or_default();
            let nombre = texto
                .split_once('\n')
                .map(|(_, n)| n.to_owned())
                .unwrap_or_default();
            nodos.push(json!({
                "id": id,
                "label": id,
                "name": nombre,
                "group": grupo,
                "accept": t.contains("doublecircle"),
                "special": false,
            }));
            ids.push(id);
        }
    }
    // Con el grafo recortado, solo las aristas entre nodos enviados.
    if truncado {
        aristas.retain(|e| {
            let dentro = |k: &str| ids.iter().any(|x| e[k].as_str() == Some(x.as_str()));
            dentro("source") && dentro("target")
        });
    }
    json!({
        "format": a.format().name(),
        "states": s.states,
        "transitions": s.transitions,
        "start": inicio,
        "groups": grupos,
        "truncated": truncado,
        "nodes": nodos,
        "edges": aristas,
    })
}

/// `POST /api/automaton`: el cuerpo es el esquema crudo (D22).
pub fn grafo(esquema: &[u8]) -> Result<String, Fallo> {
    let (_, a) = construir(esquema)?;
    Ok(grafo_de(&a).to_string())
}

// --------------------------------------------------------------- árbol ---

/// `POST /api/derivation`: el cuerpo es el payload crudo.
pub fn arbol(payload: &[u8]) -> Result<String, Fallo> {
    match derivation::derivar(payload, derivation::MAX_NODOS) {
        Ok(a) => Ok(derivation::a_json(&a)),
        Err(e) => {
            let (kind, offset) = match &e {
                derivation::Error::Lexico { offset, .. } => ("lexical", Some(*offset)),
                derivation::Error::Sintactico { offset, .. } => ("syntax", Some(*offset)),
                derivation::Error::Grande { .. } => ("too-many-nodes", None),
                derivation::Error::Profundo { offset, .. } => ("too-deep", Some(*offset)),
            };
            Err(Fallo::Payload(json!({
                "error": "derivation",
                "kind": kind,
                "offset": offset,
                "message": e.to_string(),
            })))
        }
    }
}

// ---------------------------------------------------------- comparación ---

fn clase(k: ErrorKind) -> &'static str {
    match k {
        ErrorKind::Lexical => "lexical",
        ErrorKind::Syntax => "syntax",
        ErrorKind::Schema => "schema",
        ErrorKind::Limit => "limit",
        ErrorKind::Internal => "internal",
    }
}

fn nanos(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// `POST /api/compare`: `{"schema": "…", "payload": "…"}` (texto). El DPDA
/// (M sobre `runtime-rs`, modo total, bloques de 4 KiB) y la línea base DOM
/// (solo JSON: la de XML necesita libxml2, D54).
pub fn comparar(cuerpo: &[u8]) -> Result<String, Fallo> {
    let v: Value = serde_json::from_slice(cuerpo).map_err(|e| {
        Fallo::Peticion(format!(
            "se esperaba {{\"schema\", \"payload\"}} en JSON: {e}"
        ))
    })?;
    let (Some(esquema), Some(payload)) = (v["schema"].as_str(), v["payload"].as_str()) else {
        return Err(Fallo::Peticion(
            "faltan `schema` o `payload` (cadenas)".into(),
        ));
    };
    let (esquema, payload) = (esquema.as_bytes(), payload.as_bytes());

    let t = Instant::now();
    let (formato, a) = construir(esquema)?;
    let compilar_ns = nanos(t);
    let t = Instant::now();
    let r = automaton::validate(&a, payload, Mode::Total, 4096);
    let validar_ns = nanos(t);
    let opts = Options::new(Mode::Total);
    let regla = r
        .first
        .and_then(|f| f.rule)
        .map(|k| a.rule_location(k).to_owned());
    let s = a.summary();
    let dpda = json!({
        "valid": r.valid,
        "violations": r.violations.get(),
        "errorKind": r.error_kind.map(clase),
        "offset": r.first.map(|f| f.offset),
        "line": r.first.map(|f| f.line),
        "column": r.first.map(|f| f.column),
        "rule": regla,
        "compileNs": compilar_ns,
        "validateNs": validar_ns,
        "memoryBytes": a.state_words(&opts.limits) * 8,
        "states": s.states,
        "transitions": s.transitions,
    });

    let linea_base = if formato == Formato::Json {
        let t = Instant::now();
        match baseline::JsonBaseline::compile_text(esquema) {
            Ok(lb) => {
                let compilar_ns = nanos(t);
                let t = Instant::now();
                let o = baseline::with_stack(|st| lb.validate(st, payload));
                let validar_ns = nanos(t);
                let errores: Vec<Value> = o
                    .violations
                    .iter()
                    .take(MAX_ERRORES_LB)
                    .map(|e| json!({"pointer": e.pointer, "keyword": e.keyword, "message": e.message}))
                    .collect();
                let kind = o.kind.map(|k| match k {
                    baseline::Kind::Malformed => "malformed",
                    baseline::Kind::Schema => "schema",
                    baseline::Kind::Limit => "limit",
                    baseline::Kind::Unsupported => "unsupported",
                });
                json!({
                    "valid": o.valid,
                    "kind": kind,
                    "violations": o.violation_count(),
                    "truncated": o.truncated,
                    "errors": errores,
                    "malformed": o.malformed.as_ref().map(|m| json!({"offset": m.offset, "message": m.message})),
                    "compileNs": compilar_ns,
                    "validateNs": validar_ns,
                })
            }
            Err(e) => json!({"error": format!("la línea base no compila el esquema: {e}")}),
        }
    } else {
        json!({"error": "la línea base XML (libxml2) no está en el servidor (D54)"})
    };
    let coinciden = linea_base["valid"].as_bool().map(|v| v == r.valid);
    Ok(json!({
        "format": formato.nombre(),
        "payloadBytes": payload.len(),
        "dpda": dpda,
        "baseline": linea_base,
        "agree": coinciden,
        // Medición en vivo, en un proceso aparte (`medir.rs`): tiempo con
        // repeticiones, MB/s, pico de memoria y asignaciones de los dos.
        "measurement": crate::medir::medir(
            std::str::from_utf8(esquema).unwrap_or_default(),
            std::str::from_utf8(payload).unwrap_or_default(),
        ),
    })
    .to_string())
}
