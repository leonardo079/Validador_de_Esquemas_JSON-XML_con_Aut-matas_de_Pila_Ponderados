//! F7.2 — Los casos de prueba de MongoDB Atlas en el servidor (D72;
//! `docs/fase7/atlas.md`): `GET /api/tests` los lista y `POST /api/tests/run`
//! los ejecuta con el DPDA y compara con su `expected`, como las pruebas de
//! corpus (`crates/automaton/tests/corpus.rs`).
//!
//! Las consultas tienen plazo ([`PLAZO`]) y tope ([`MAX_CASOS`]); [`ejecutar`]
//! es pura (sin red) y corre en el hilo de `api::procesar`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use atlas::Guardado;
use automaton::{BuildError, ErrorKind, Mode};
use mongodb::bson::{doc, Document};
use serde_json::{json, Value};

/// La colección `cases`.
pub type Coleccion = mongodb::Collection<Document>;

/// Plazo de una consulta a Atlas.
pub const PLAZO: Duration = Duration::from_secs(10);
/// Casos como mucho por consulta.
pub const MAX_CASOS: i64 = 1000;

#[derive(Debug)]
pub enum Error {
    /// Sin `MONGODB_URI` (503).
    SinBase,
    /// Atlas falló o no respondió a tiempo (502; el detalle va al registro).
    Base(String),
}

fn filtro(categoria: Option<&str>) -> Document {
    match categoria {
        Some(c) => doc! {"category": c},
        None => doc! {},
    }
}

async fn buscar(
    col: Option<&Coleccion>,
    f: Document,
    proyeccion: Option<Document>,
) -> Result<Vec<Value>, Error> {
    let col = col.ok_or(Error::SinBase)?;
    let consulta = async {
        let mut q = col.find(f).sort(doc! {"_id": 1}).limit(MAX_CASOS);
        if let Some(p) = proyeccion {
            q = q.projection(p);
        }
        let mut cursor = q.await?;
        let mut v = Vec::new();
        while cursor.advance().await? {
            v.push(atlas::a_json(cursor.deserialize_current()?));
        }
        Ok::<_, mongodb::error::Error>(v)
    };
    match tokio::time::timeout(PLAZO, consulta).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(Error::Base(e.to_string())),
        Err(_) => Err(Error::Base("plazo agotado".into())),
    }
}

/// `GET /api/tests`: los casos sin esquema ni payload.
/// Caducidad de la caché del corpus (E3C-5): el corpus es estático (se
/// recarga con `cargo run -p atlas`), así que no hace falta consultar Atlas
/// en cada petición ni gastar la cuota del M0.
pub const CADUCIDAD: Duration = Duration::from_secs(300);

/// (categoría, cuándo, documentos).
type Entrada = (String, Instant, Arc<Vec<Value>>);
static CACHE: Mutex<Vec<Entrada>> = Mutex::new(Vec::new());

/// Los documentos completos de una categoría (o de todas), de la caché si
/// tienen menos de [`CADUCIDAD`].
async fn todos(col: Option<&Coleccion>, categoria: Option<&str>) -> Result<Arc<Vec<Value>>, Error> {
    let col = col.ok_or(Error::SinBase)?;
    let clave = categoria.unwrap_or("").to_owned();
    if let Ok(c) = CACHE.lock() {
        if let Some((_, _, v)) = c
            .iter()
            .find(|(k, t, _)| *k == clave && t.elapsed() < CADUCIDAD)
        {
            return Ok(v.clone());
        }
    }
    let v = Arc::new(buscar(Some(col), filtro(categoria), None).await?);
    if let Ok(mut c) = CACHE.lock() {
        c.retain(|(k, t, _)| *k != clave && t.elapsed() < CADUCIDAD);
        c.push((clave, Instant::now(), v.clone()));
    }
    Ok(v)
}

pub async fn listar(col: Option<&Coleccion>, categoria: Option<&str>) -> Result<Value, Error> {
    let v = todos(col, categoria).await?;
    let casos: Vec<Value> = v
        .iter()
        .map(|d| {
            json!({
                "id": d["_id"],
                "category": d["category"],
                "format": d["format"],
                "description": d["description"],
                "phase": d["phase"],
                "expected": d["expected"],
            })
        })
        .collect();
    // `truncated` (§25.8): la consulta devuelve como mucho MAX_CASOS.
    let truncated = casos.len() as i64 >= MAX_CASOS;
    Ok(json!({"total": casos.len(), "truncated": truncated, "cases": casos}))
}

/// Los casos completos para ejecutar; los que no se pueden leer cuentan como
/// error en [`ejecutar`].
pub async fn traer(
    col: Option<&Coleccion>,
    categoria: Option<&str>,
) -> Result<Vec<Result<Guardado, (String, String)>>, Error> {
    let v = todos(col, categoria).await?;
    Ok(v.iter()
        .map(|d| atlas::leer(d).map_err(|e| (d["_id"].as_str().unwrap_or("?").to_owned(), e)))
        .collect())
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

/// Ejecuta un caso: `("passed" | "failed" | "skipped" | "error", detalle)`.
fn uno(c: &Guardado) -> Value {
    let base =
        json!({"id": c.id, "category": c.category, "format": c.format, "expected": c.expected});
    let con = |mut b: Value, estado: &str, extra: Value| {
        b["status"] = json!(estado);
        if let (Some(o), Some(e)) = (b.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        b
    };
    let Some(payload) = &c.payload else {
        return con(
            base,
            "skipped",
            json!({"reason": "el payload lo produce un generador"}),
        );
    };
    let ir = match crate::compilar::ir_de(c.schema.as_bytes()) {
        Ok((_, _, ir)) => ir,
        Err(_) => return con(base, "error", json!({"reason": "el esquema no compila"})),
    };
    let m = match automaton::build(&ir) {
        Ok(m) => m,
        Err(e @ BuildError::Pending { .. }) => {
            return con(base, "skipped", json!({"reason": e.to_string()}));
        }
        Err(e) => return con(base, "error", json!({"reason": e.to_string()})),
    };
    let r = automaton::validate(&m, payload, Mode::Total, 4096);
    let obtenido = json!({
        "valid": r.valid,
        "violations": r.violations.get(),
        "errorKind": r.error_kind.map(clase),
        "offset": r.first.map(|f| f.offset),
    });
    let e = &c.expected;
    let mut diferencias = Vec::new();
    if e["valid"] != obtenido["valid"] {
        diferencias.push("valid");
    }
    if e.get("errorKind").is_some() && e["errorKind"] != obtenido["errorKind"] {
        diferencias.push("errorKind");
    }
    if e.get("violations").is_some() && e["violations"] != obtenido["violations"] {
        diferencias.push("violations");
    }
    if e.get("offset").is_some() && e["offset"] != obtenido["offset"] {
        diferencias.push("offset");
    }
    let estado = if diferencias.is_empty() {
        "passed"
    } else {
        "failed"
    };
    con(
        base,
        estado,
        json!({"got": obtenido, "mismatch": diferencias}),
    )
}

/// Ejecuta todos y resume.
pub fn ejecutar(lista: &[Result<Guardado, (String, String)>]) -> Value {
    let resultados: Vec<Value> = lista
        .iter()
        .map(|c| match c {
            Ok(c) => uno(c),
            Err((id, e)) => {
                json!({"id": id, "status": "error", "reason": format!("documento no válido: {e}")})
            }
        })
        .collect();
    let cuenta = |s: &str| resultados.iter().filter(|r| r["status"] == s).count();
    json!({
        "total": resultados.len(),
        "passed": cuenta("passed"),
        "failed": cuenta("failed"),
        "skipped": cuenta("skipped"),
        "errors": cuenta("error"),
        "results": resultados,
    })
}
