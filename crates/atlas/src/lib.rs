//! F7.2 — Los casos de prueba en MongoDB Atlas (D72; `docs/fase7/atlas.md`).
//!
//! El corpus de `tests/cases/` es la semilla: `cargar-casos` lo sube a la
//! colección `cases` (un documento por caso, `_id` = id del caso, con
//! *upsert*: repetir la carga no duplica). El servidor lo lee para
//! `GET /api/tests` y `POST /api/tests/run`.
//!
//! [`documento`] y [`leer`] son puras (sin red), para probarlas en
//! `cargo test` (D1). La conexión ([`coleccion`]) solo la usan el binario y el
//! servidor, con `MONGODB_URI` (en `.env`, nunca en el repo).

use std::time::Duration;

use baseline::corpus::{Case, ErrorKind, Format, Payload};
use mongodb::bson::{self, Document};
use mongodb::options::ClientOptions;
use mongodb::{Client, Collection};
use serde_json::{json, Value};

/// Nombre de la colección.
pub const COLECCION: &str = "cases";
/// Base por defecto si la URI no trae una.
pub const BASE: &str = "validador";
/// Versión del formato del documento.
pub const VERSION: u32 = 1;

fn clase(k: ErrorKind) -> &'static str {
    match k {
        ErrorKind::Lexical => "lexical",
        ErrorKind::Syntax => "syntax",
        ErrorKind::Schema => "schema",
        ErrorKind::Limit => "limit",
    }
}

/// Base64 estándar con relleno (RFC 4648).
pub fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for (k, s) in [18, 12, 6, 0].into_iter().enumerate() {
            o.push(if k <= c.len() {
                A[(n >> s) as usize & 63] as char
            } else {
                '='
            });
        }
    }
    o
}

/// Inverso de [`base64`]; `None` si no es base64 estándar.
pub fn de_base64(s: &str) -> Option<Vec<u8>> {
    let valor = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let b = s.as_bytes();
    if !b.len().is_multiple_of(4) {
        return None;
    }
    let mut o = Vec::with_capacity(b.len() / 4 * 3);
    for g in b.chunks(4) {
        let relleno = g.iter().rev().take_while(|&&c| c == b'=').count();
        if relleno > 2 {
            return None;
        }
        let mut n = 0u32;
        for &c in &g[..4 - relleno] {
            n = (n << 6) | valor(c)?;
        }
        n <<= 6 * relleno as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        o.extend_from_slice(&bytes[..3 - relleno]);
    }
    Some(o)
}

/// El documento de un caso (como JSON; [`a_bson`] lo convierte).
///
/// - `schema`: el texto exacto del esquema (el mismo que leen las pruebas).
/// - `payload`: `{"kind": "text", "value"}` si es UTF-8, `{"kind": "base64",
///   "value"}` si no, o `{"kind": "generator", "name", "params"}`.
/// - `expected`: los campos del corpus (`valid`, `violations`, `errorKind`,
///   `pointer`, `offset`) que trae el caso.
/// - `description` y `phase` se leen del archivo del caso.
pub fn documento(c: &Case, raiz: &std::path::Path) -> Result<Value, String> {
    let archivo: Value = std::fs::read(&c.path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| format!("{}: no se pudo releer", c.path.display()))?;
    let esquema = String::from_utf8(c.schema_text.clone())
        .map_err(|_| format!("{}: esquema no UTF-8", c.id))?;
    let payload = match &c.payload {
        Payload::Bytes(b) => match std::str::from_utf8(b) {
            Ok(s) => json!({"kind": "text", "value": s}),
            Err(_) => json!({"kind": "base64", "value": base64(b)}),
        },
        Payload::Generator { name, params } => {
            json!({"kind": "generator", "name": name, "params": params})
        }
    };
    let e = &c.expected;
    let mut esperado = json!({"valid": e.valid});
    if let Some(v) = e.violations {
        esperado["violations"] = json!(v);
    }
    if let Some(k) = e.error_kind {
        esperado["errorKind"] = json!(clase(k));
    }
    if let Some(p) = &e.pointer {
        esperado["pointer"] = json!(p);
    }
    if let Some(o) = e.offset {
        esperado["offset"] = json!(o);
    }
    // Ruta relativa al repo («tests/cases/…»), nunca la absoluta de la
    // máquina que carga (las rutas del corpus no están normalizadas: llevan
    // `crates/baseline/../..`).
    let _ = raiz;
    let completa = c.path.to_string_lossy().replace('\\', "/");
    let fuente = match completa.rfind("tests/cases/") {
        Some(i) => completa[i..].to_owned(),
        None => c
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let mut d = json!({
        "_id": c.id,
        "version": VERSION,
        "category": c.category,
        "format": if c.format == Format::Json { "json" } else { "xml" },
        "description": archivo["description"],
        "schema": esquema,
        "payload": payload,
        "expected": esperado,
        "source": fuente,
    });
    if let Some(f) = archivo.get("phase") {
        d["phase"] = f.clone();
    }
    if let Some(s) = &c.strategy {
        d["strategy"] = json!(s);
    }
    Ok(d)
}

/// JSON → documento BSON.
pub fn a_bson(v: &Value) -> Result<Document, String> {
    bson::serialize_to_document(v).map_err(|e| e.to_string())
}

/// Documento BSON → JSON, por serde (los enteros quedan enteros y las
/// cadenas, cadenas). Un documento que no se puede convertir da `null`, y
/// [`leer`] lo rechaza.
pub fn a_json(d: Document) -> Value {
    bson::deserialize_from_document::<Value>(d).unwrap_or(Value::Null)
}

/// Un caso leído de Atlas, listo para ejecutar.
#[derive(Clone, Debug, PartialEq)]
pub struct Guardado {
    pub id: String,
    pub category: String,
    pub format: String,
    pub description: String,
    pub schema: String,
    /// `None` si el payload lo produce un generador (no se ejecuta aquí).
    pub payload: Option<Vec<u8>>,
    pub expected: Value,
}

/// Lee un documento de la colección (ya en JSON).
pub fn leer(v: &Value) -> Result<Guardado, String> {
    let texto = |k: &str| {
        v[k].as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("falta `{k}`"))
    };
    let p = &v["payload"];
    let payload = match p["kind"].as_str() {
        Some("text") => Some(
            p["value"]
                .as_str()
                .ok_or("payload sin `value`")?
                .as_bytes()
                .to_vec(),
        ),
        Some("base64") => Some(
            de_base64(p["value"].as_str().ok_or("payload sin `value`")?)
                .ok_or("base64 no válido")?,
        ),
        Some("generator") => None,
        _ => return Err("payload con `kind` desconocido".into()),
    };
    if !v["expected"]["valid"].is_boolean() {
        return Err("falta `expected.valid`".into());
    }
    Ok(Guardado {
        id: texto("_id")?,
        category: texto("category")?,
        format: texto("format")?,
        description: v["description"].as_str().unwrap_or_default().to_owned(),
        schema: texto("schema")?,
        payload,
        expected: v["expected"].clone(),
    })
}

/// Conecta con la colección de casos (ver [`base`]).
pub async fn coleccion(uri: &str) -> Result<Collection<Document>, String> {
    Ok(base(uri).await?.collection(COLECCION))
}

/// La base de la URI (o [`BASE`]). No envía nada hasta la primera
/// operación; los plazos son cortos para que un Atlas caído no cuelgue al
/// servidor.
pub async fn base(uri: &str) -> Result<mongodb::Database, String> {
    // El error del driver puede citar la URI (con la contraseña): no se
    // incluye. `mongodb+srv` resuelve el registro SRV aquí: sin red, falla.
    let mut o = ClientOptions::parse(uri)
        .await
        .map_err(|_| "MONGODB_URI no válida o su registro DNS no resuelve".to_owned())?;
    o.app_name = Some("validador-pda".into());
    o.server_selection_timeout = Some(Duration::from_secs(5));
    o.connect_timeout = Some(Duration::from_secs(5));
    let base = o
        .default_database
        .clone()
        .unwrap_or_else(|| BASE.to_owned());
    let c = Client::with_options(o).map_err(|e| e.to_string())?;
    Ok(c.database(&base))
}

/// `MONGODB_URI` del entorno o, si no está, de un `.env` en `dir` (una línea
/// `MONGODB_URI=…`). Nunca se imprime.
pub fn uri_de_entorno(dir: &std::path::Path) -> Option<String> {
    if let Ok(u) = std::env::var("MONGODB_URI") {
        if !u.trim().is_empty() {
            return Some(u.trim().to_owned());
        }
    }
    let texto = std::fs::read_to_string(dir.join(".env")).ok()?;
    texto
        .lines()
        .find_map(|l| l.trim().strip_prefix("MONGODB_URI="))
        .map(|u| u.trim().trim_matches('"').to_owned())
        .filter(|u| !u.is_empty())
}

// ---------------------------------------------------------------------------
// E2 (D73): las métricas de F6.5 (`metricas --csv`) en la colección `metrics`.

/// Nombre de la colección de métricas.
pub const COLECCION_METRICAS: &str = "metrics";

/// Cabecera que se espera del CSV de `bench::csv`.
pub const CABECERA_METRICAS: &str =
    "caso,validador,bytes,profundidad,valido,violaciones,segundos,mb_s,ns_por_byte,asignaciones,pico_bytes,estados,transiciones";

/// Divide una línea del CSV: el primer campo va entre comillas (con `""`
/// para una comilla) y el resto no tiene comas.
fn campos(linea: &str) -> Option<Vec<String>> {
    let resto = linea.strip_prefix('"')?;
    let mut caso = String::new();
    let mut it = resto.char_indices().peekable();
    let fin = loop {
        let (i, c) = it.next()?;
        if c == '"' {
            if it.peek().map(|&(_, d)| d) == Some('"') {
                caso.push('"');
                it.next();
            } else {
                break i + 1;
            }
        } else {
            caso.push(c);
        }
    };
    let cola = resto[fin..].strip_prefix(',')?;
    let mut v = vec![caso];
    v.extend(cola.split(',').map(str::to_owned));
    Some(v)
}

fn numero(s: &str) -> Value {
    if s.is_empty() {
        return Value::Null;
    }
    if let Ok(n) = s.parse::<u64>() {
        return json!(n);
    }
    match s.parse::<f64>() {
        Ok(x) if x.is_finite() => json!(x),
        _ => Value::Null,
    }
}

/// Los documentos de `metrics` a partir del CSV de `metricas --csv`.
/// `_id` = `"<caso>|<validador>"`; `case` es el `_id` del caso en `cases` si
/// la medida es del corpus (familia `corpus`) o `null` en el bombeo.
/// `medido` es la fecha de la medición (texto ISO, la pone quien carga).
pub fn metricas(csv: &str, medido: &str, ids_corpus: &[String]) -> Result<Vec<Value>, String> {
    let mut lineas = csv.lines();
    if lineas.next().map(str::trim) != Some(CABECERA_METRICAS) {
        return Err("cabecera inesperada: ¿es la salida de `metricas --csv`?".into());
    }
    let mut v = Vec::new();
    for (n, l) in lineas.enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let c = campos(l).ok_or_else(|| format!("línea {}: no se pudo leer", n + 2))?;
        if c.len() != 13 {
            return Err(format!(
                "línea {}: {} campos, se esperaban 13",
                n + 2,
                c.len()
            ));
        }
        let del_corpus = ids_corpus.iter().any(|id| *id == c[0]);
        v.push(json!({
            "_id": format!("{}|{}", c[0], c[1]),
            "case": if del_corpus { json!(c[0]) } else { Value::Null },
            "family": if del_corpus { "corpus" } else { "bombeo" },
            "label": c[0],
            "validator": c[1],
            "bytes": numero(&c[2]),
            "depth": numero(&c[3]),
            "valid": c[4] == "true",
            "violations": numero(&c[5]),
            "seconds": numero(&c[6]),
            "mbPerSecond": numero(&c[7]),
            "nsPerByte": numero(&c[8]),
            "allocations": numero(&c[9]),
            "peakHeapBytes": numero(&c[10]),
            "states": numero(&c[11]),
            "transitions": numero(&c[12]),
            "measuredAt": medido,
            "source": "cargo run -p bench --release --bin metricas (F6.5)",
        }));
    }
    Ok(v)
}
