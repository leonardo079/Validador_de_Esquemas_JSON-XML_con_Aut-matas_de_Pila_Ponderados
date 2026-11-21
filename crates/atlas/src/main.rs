//! F7.2 / E2 — `cargar-casos`: sube `tests/cases/` a la colección `cases` de
//! Atlas (D72) y, con `--metricas`, las métricas de F6.5 a `metrics` (D73).
//! Idempotente: *upsert* por `_id`.
//!
//! ```text
//! cargo run -p atlas --release                      # sube todo el corpus
//! cargo run -p atlas --release -- --podar           # y borra de Atlas los que ya no están
//! cargo run -p atlas --release -- --en-seco         # solo cuenta, sin conectar
//! cargo run -p bench --release --bin metricas -- --csv target/metricas/metricas.csv
//! cargo run -p atlas --release -- --metricas target/metricas/metricas.csv
//! cargo run -p atlas --release -- --solo-metricas target/metricas/metricas.csv
//! ```
//!
//! La URI sale de `MONGODB_URI` o de `.env` en la raíz del repo; nunca se
//! imprime.

use std::path::Path;
use std::process::ExitCode;

use mongodb::bson::{doc, Document};
use mongodb::Collection;

struct Args {
    podar: bool,
    en_seco: bool,
    metricas: Option<String>,
    solo_metricas: bool,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        podar: false,
        en_seco: false,
        metricas: None,
        solo_metricas: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--podar" => a.podar = true,
            "--en-seco" => a.en_seco = true,
            "--metricas" | "--solo-metricas" => {
                a.solo_metricas = x == "--solo-metricas";
                a.metricas = Some(it.next().ok_or(format!("{x} espera la ruta del CSV"))?);
            }
            otro => return Err(format!("opción desconocida: {otro}")),
        }
    }
    Ok(a)
}

/// *Upsert* de cada documento por `_id`: (nuevos, actualizados).
async fn subir(
    col: &Collection<Document>,
    docs: &[(String, Document)],
) -> Result<(u64, u64), String> {
    let (mut nuevos, mut cambiados) = (0u64, 0u64);
    for (id, d) in docs {
        let r = col
            .replace_one(doc! {"_id": id}, d)
            .upsert(true)
            .await
            .map_err(|e| format!("{id}: {e}"))?;
        if r.upserted_id.is_some() {
            nuevos += 1;
        } else {
            cambiados += r.modified_count;
        }
    }
    Ok((nuevos, cambiados))
}

async fn podar(col: &Collection<Document>, docs: &[(String, Document)]) -> Result<u64, String> {
    let ids: Vec<&String> = docs.iter().map(|(id, _)| id).collect();
    col.delete_many(doc! {"_id": {"$nin": ids}})
        .await
        .map(|r| r.deleted_count)
        .map_err(|e| e.to_string())
}

/// Fecha UTC de hoy (AAAA-MM-DD) sin crates de fechas: días desde 1970
/// (algoritmo de Howard Hinnant, `civil_from_days`).
fn hoy() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let z = s.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn a_docs(v: &[serde_json::Value]) -> Result<Vec<(String, Document)>, String> {
    v.iter()
        .map(|x| {
            Ok((
                x["_id"].as_str().unwrap_or_default().to_owned(),
                atlas::a_bson(x)?,
            ))
        })
        .collect()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match correr().await {
        Ok(()) => ExitCode::SUCCESS,
        Err((codigo, m)) => {
            eprintln!("cargar-casos: {m}");
            ExitCode::from(codigo)
        }
    }
}

async fn correr() -> Result<(), (u8, String)> {
    let a = args().map_err(|e| (2, e))?;
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let casos = baseline::corpus::casos().map_err(|e| (2, e.to_string()))?;
    let ids: Vec<String> = casos.iter().map(|c| c.id.clone()).collect();
    let docs = casos
        .iter()
        .map(|c| {
            atlas::documento(c, &raiz)
                .and_then(|v| atlas::a_bson(&v))
                .map(|d| (c.id.clone(), d))
                .map_err(|e| (2, format!("{}: {e}", c.id)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut por_categoria: Vec<(String, usize)> = Vec::new();
    for c in &casos {
        match por_categoria.iter_mut().find(|(k, _)| *k == c.category) {
            Some((_, n)) => *n += 1,
            None => por_categoria.push((c.category.clone(), 1)),
        }
    }
    let resumen: Vec<String> = por_categoria
        .iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    eprintln!(
        "cargar-casos: {} casos ({})",
        docs.len(),
        resumen.join(", ")
    );

    let metricas = match &a.metricas {
        None => None,
        Some(ruta) => {
            let csv = std::fs::read_to_string(ruta).map_err(|e| (2, format!("{ruta}: {e}")))?;
            let v = atlas::metricas(&csv, &hoy(), &ids).map_err(|e| (2, format!("{ruta}: {e}")))?;
            let corpus = v.iter().filter(|m| m["family"] == "corpus").count();
            eprintln!(
                "cargar-casos: {} métricas ({corpus} del corpus, {} de bombeo)",
                v.len(),
                v.len() - corpus
            );
            Some(a_docs(&v).map_err(|e| (2, e))?)
        }
    };
    if a.en_seco {
        return Ok(());
    }
    let uri = atlas::uri_de_entorno(&raiz).ok_or((
        2,
        "falta MONGODB_URI (variable de entorno o .env en la raíz)".to_owned(),
    ))?;
    let base = atlas::base(&uri).await.map_err(|e| (1, e))?;

    if !a.solo_metricas {
        let col = base.collection::<Document>(atlas::COLECCION);
        let (nuevos, cambiados) = subir(&col, &docs).await.map_err(|e| (1, e))?;
        let _ = col
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! {"category": 1})
                    .build(),
            )
            .await;
        let borrados = if a.podar {
            podar(&col, &docs).await.map_err(|e| (1, e))?
        } else {
            0
        };
        let total = col.count_documents(doc! {}).await.unwrap_or(0);
        eprintln!(
            "cargar-casos: casos: {nuevos} nuevos, {cambiados} actualizados, {} sin cambios, {borrados} borrados; {total} en Atlas",
            docs.len() as u64 - nuevos - cambiados
        );
    }
    if let Some(m) = metricas {
        let col = base.collection::<Document>(atlas::COLECCION_METRICAS);
        let (nuevos, cambiados) = subir(&col, &m).await.map_err(|e| (1, e))?;
        let _ = col
            .create_index(
                mongodb::IndexModel::builder()
                    .keys(doc! {"case": 1})
                    .build(),
            )
            .await;
        let borrados = if a.podar {
            podar(&col, &m).await.map_err(|e| (1, e))?
        } else {
            0
        };
        let total = col.count_documents(doc! {}).await.unwrap_or(0);
        eprintln!(
            "cargar-casos: métricas: {nuevos} nuevas, {cambiados} actualizadas, {} sin cambios, {borrados} borradas; {total} en Atlas",
            m.len() as u64 - nuevos - cambiados
        );
    }
    Ok(())
}
