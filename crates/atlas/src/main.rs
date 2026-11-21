//! F7.2 — `cargar-casos`: sube `tests/cases/` a la colección `cases` de Atlas
//! (D72). Idempotente: *upsert* por `_id`.
//!
//! ```text
//! cargo run -p atlas --release                  # sube todo el corpus
//! cargo run -p atlas --release -- --podar       # y borra de Atlas los que ya no están
//! cargo run -p atlas --release -- --en-seco     # solo cuenta, sin conectar
//! ```
//!
//! La URI sale de `MONGODB_URI` o de `.env` en la raíz del repo; nunca se
//! imprime.

use std::path::Path;
use std::process::ExitCode;

use mongodb::bson::doc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let podar = args.iter().any(|a| a == "--podar");
    let en_seco = args.iter().any(|a| a == "--en-seco");
    if let Some(a) = args
        .iter()
        .find(|a| !matches!(a.as_str(), "--podar" | "--en-seco"))
    {
        eprintln!("cargar-casos: opción desconocida: {a}");
        return ExitCode::from(2);
    }
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let casos = match baseline::corpus::casos() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cargar-casos: {e}");
            return ExitCode::from(2);
        }
    };
    let mut docs = Vec::with_capacity(casos.len());
    for c in &casos {
        match atlas::documento(c, &raiz).and_then(|v| atlas::a_bson(&v)) {
            Ok(d) => docs.push((c.id.clone(), d)),
            Err(e) => {
                eprintln!("cargar-casos: {}: {e}", c.id);
                return ExitCode::from(2);
            }
        }
    }
    let por_categoria = {
        let mut v: Vec<(String, usize)> = Vec::new();
        for c in &casos {
            match v.iter_mut().find(|(k, _)| *k == c.category) {
                Some((_, n)) => *n += 1,
                None => v.push((c.category.clone(), 1)),
            }
        }
        v.iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    eprintln!("cargar-casos: {} casos ({por_categoria})", docs.len());
    if en_seco {
        return ExitCode::SUCCESS;
    }
    let Some(uri) = atlas::uri_de_entorno(&raiz) else {
        eprintln!("cargar-casos: falta MONGODB_URI (variable de entorno o .env en la raíz)");
        return ExitCode::from(2);
    };
    let col = match atlas::coleccion(&uri).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cargar-casos: {e}");
            return ExitCode::from(1);
        }
    };
    let (mut nuevos, mut cambiados) = (0u64, 0u64);
    for (id, d) in &docs {
        match col.replace_one(doc! {"_id": id}, d).upsert(true).await {
            Ok(r) => {
                if r.upserted_id.is_some() {
                    nuevos += 1;
                } else {
                    cambiados += r.modified_count;
                }
            }
            Err(e) => {
                eprintln!("cargar-casos: {id}: {e}");
                return ExitCode::from(1);
            }
        }
    }
    let _ = col
        .create_index(
            mongodb::IndexModel::builder()
                .keys(doc! {"category": 1})
                .build(),
        )
        .await;
    let mut borrados = 0;
    if podar {
        let ids: Vec<&String> = docs.iter().map(|(id, _)| id).collect();
        match col.delete_many(doc! {"_id": {"$nin": ids}}).await {
            Ok(r) => borrados = r.deleted_count,
            Err(e) => {
                eprintln!("cargar-casos: podar: {e}");
                return ExitCode::from(1);
            }
        }
    }
    let total = col.count_documents(doc! {}).await.unwrap_or(0);
    eprintln!(
        "cargar-casos: {nuevos} nuevos, {cambiados} actualizados, {} sin cambios, {borrados} borrados; {total} en Atlas",
        docs.len() as u64 - nuevos - cambiados
    );
    ExitCode::SUCCESS
}
