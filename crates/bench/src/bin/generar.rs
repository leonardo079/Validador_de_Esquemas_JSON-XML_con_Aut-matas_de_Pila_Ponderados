//! F5.4 — Genera payloads JSON válidos desde la IR y los escribe al corpus.
//!
//! ```text
//! cargo run -p bench --release --bin generar                 # solo muestra
//! cargo run -p bench --release --bin generar -- --escribir   # tests/cases/valid/
//! cargo run -p bench --release --bin generar -- --n 40 --semilla 7
//! ```
//!
//! Por cada esquema (los de `tests/schemas/parse-ok/` que tienen M, más los
//! de [`EXTRA`]) genera hasta dos payloads distintos con
//! [`bench::generar::generar_valido`], que solo acepta los que el DPDA **y** la
//! línea base (D48) dan por válidos, y para al llegar a `--n` (20 por defecto).
//! Con `--escribir` los guarda como casos `valid-gen-<esquema>-NN.json` con el
//! formato de `tests/README.md` (`phase: 5`).

use std::path::{Path, PathBuf};

use bench::generar::{cadena_json, generar_valido};
use bench::ir_y_automata;

/// Esquemas propios para cubrir más reglas que los de `parse-ok`.
const EXTRA: &[(&str, &str)] = &[
    (
        "pedido",
        r#"{
  "type": "object",
  "properties": {
    "id": { "type": "integer", "minimum": 1 },
    "cliente": { "type": "string", "minLength": 2, "maxLength": 20, "pattern": "^[A-Z][a-z]+$" },
    "lineas": {
      "type": "array",
      "minItems": 1,
      "maxItems": 3,
      "items": {
        "type": "object",
        "properties": {
          "sku": { "type": "string", "pattern": "^[A-Z]{3}-[0-9]{3}$" },
          "cantidad": { "type": "integer", "minimum": 1, "maximum": 10 }
        },
        "required": ["sku", "cantidad"],
        "additionalProperties": false
      }
    },
    "total": { "type": "number", "minimum": 0, "multipleOf": 0.01 },
    "estado": { "enum": ["nuevo", "pagado", "enviado"] }
  },
  "required": ["id", "cliente", "lineas", "estado"],
  "additionalProperties": false
}"#,
    ),
    (
        "dependencias",
        r#"{
  "type": "object",
  "properties": {
    "tarjeta": { "type": "string", "pattern": "^[0-9]{4}$" },
    "titular": { "type": "string", "minLength": 1 },
    "cvv": { "type": "integer", "minimum": 100, "maximum": 999 }
  },
  "dependentRequired": { "tarjeta": ["titular", "cvv"] },
  "minProperties": 1
}"#,
    ),
    (
        "alternativas",
        r#"{
  "type": "array",
  "minItems": 2,
  "items": { "oneOf": [ { "type": "string", "maxLength": 3 }, { "type": "integer", "exclusiveMinimum": 0 } ] }
}"#,
    ),
    (
        "tupla",
        r#"{
  "type": "array",
  "prefixItems": [ { "type": "string" }, { "type": "integer", "minimum": 0, "maximum": 120 }, { "type": "boolean" } ],
  "items": false
}"#,
    ),
    (
        "conjunto",
        r#"{
  "type": "array",
  "minItems": 3,
  "maxItems": 6,
  "uniqueItems": true,
  "items": { "type": "integer", "minimum": 0, "maximum": 100 }
}"#,
    ),
    (
        "combinados",
        r#"{
  "allOf": [
    { "type": "object", "required": ["nombre"] },
    { "properties": { "nombre": { "type": "string", "minLength": 3 }, "edad": { "type": "integer", "minimum": 18 } } }
  ],
  "anyOf": [ { "required": ["edad"] }, { "properties": { "menor": { "const": false } }, "required": ["menor"] } ]
}"#,
    ),
];

struct Args {
    escribir: bool,
    n: usize,
    semilla: u64,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        escribir: false,
        n: 20,
        semilla: 1,
    };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--escribir" => a.escribir = true,
            "--n" => {
                a.n = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--n espera un entero")?
            }
            "--semilla" => {
                a.semilla = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--semilla espera un entero")?
            }
            otro => return Err(format!("opción desconocida: {otro}")),
        }
    }
    Ok(a)
}

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

/// Esquemas de `parse-ok` que aceptan cualquier valor o ninguno: no aportan.
const TRIVIALES: &[&str] = &["vacio", "booleano-true", "booleano-false"];

/// Los esquemas: primero [`EXTRA`] (cubren más reglas) y después los de
/// `parse-ok` (por nombre de archivo), sin los [`TRIVIALES`].
fn esquemas() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = EXTRA
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect();
    if let Ok(dir) = std::fs::read_dir(raiz().join("schemas/parse-ok")) {
        let mut archivos: Vec<PathBuf> = dir
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        archivos.sort();
        for p in archivos {
            let nombre = p
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if TRIVIALES.contains(&nombre.as_str()) {
                continue;
            }
            if let Ok(t) = std::fs::read_to_string(&p) {
                v.push((nombre, t.trim_start_matches('\u{feff}').trim().to_owned()));
            }
        }
    }
    v
}

fn main() -> std::process::ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("generar: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let mut hechos = 0usize;
    for (nombre, texto) in esquemas() {
        if hechos >= a.n {
            break;
        }
        let Ok((ir, m)) = ir_y_automata(texto.as_bytes()) else {
            eprintln!("generar: {nombre}: sin M (se omite)");
            continue;
        };
        let lb =
            baseline::with_stack(|_| baseline::JsonBaseline::compile_text(texto.as_bytes()).ok());
        let Some(lb) = lb else {
            eprintln!("generar: {nombre}: la línea base no lo compila (se omite)");
            continue;
        };
        let mut vistos: Vec<String> = Vec::new();
        let mut semilla = a.semilla;
        for _ in 0..2 {
            if hechos >= a.n {
                break;
            }
            let valido = |p: &[u8]| baseline::with_stack(|st| lb.validate(st, p).valid);
            let Some((p, s)) = (0..20).find_map(|_| {
                let r = generar_valido(&ir, &m, semilla, 500, valido)?;
                semilla = r.1 + 1;
                (!vistos.contains(&r.0)).then_some(r)
            }) else {
                eprintln!("generar: {nombre}: sin payload válido nuevo");
                break;
            };
            vistos.push(p.clone());
            hechos += 1;
            let id = format!("valid-gen-{}-{:02}", nombre, vistos.len());
            println!("{id}  (semilla {s}): {p}");
            if a.escribir {
                let caso = format!(
                    "{{\n  \"_id\": {},\n  \"category\": \"valid\",\n  \"format\": \"json\",\n  \"description\": {},\n  \"phase\": 5,\n  \"schema\": {},\n  \"payload\": {},\n  \"expected\": {{\n    \"valid\": true,\n    \"violations\": 0\n  }}\n}}\n",
                    cadena_json(&id),
                    cadena_json(&format!(
                        "Generado desde la IR de «{nombre}» con la semilla {s} (F5.4); válido para el DPDA y la línea base."
                    )),
                    texto.replace('\n', "\n  "),
                    cadena_json(&p),
                );
                let ruta = raiz().join("cases/valid").join(format!("{id}.json"));
                if let Err(e) = std::fs::write(&ruta, caso) {
                    eprintln!("generar: no se pudo escribir {}: {e}", ruta.display());
                    return std::process::ExitCode::from(2);
                }
            }
        }
    }
    eprintln!(
        "generar: {hechos} payloads válidos{}",
        if a.escribir {
            " escritos en tests/cases/valid/"
        } else {
            ""
        }
    );
    std::process::ExitCode::SUCCESS
}
