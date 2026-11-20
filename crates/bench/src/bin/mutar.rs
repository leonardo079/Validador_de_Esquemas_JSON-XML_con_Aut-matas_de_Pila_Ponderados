//! F6.3 — Genera casos `invalid` y `malformed` mutando los `valid` JSON del
//! corpus.
//!
//! ```text
//! cargo run -p bench --release --bin mutar                 # solo muestra
//! cargo run -p bench --release --bin mutar -- --escribir   # tests/cases/
//! cargo run -p bench --release --bin mutar -- --invalidos 30 --malformados 15 --semilla 7
//! ```
//!
//! Por cada caso válido con M prueba las mutaciones de
//! [`bench::mutar`] (rotando el orden para variar) y se queda con hasta dos
//! inválidos y un malformado, cada uno de una mutación distinta, en los que el
//! DPDA y la línea base coinciden ([`bench::mutar::clasificar`]). Un
//! malformado tiene que dar además lo mismo con el esquema `true`: el error
//! léxico o sintáctico no depende del esquema.

use std::path::{Path, PathBuf};

use baseline::corpus::{self, Format, Payload};
use bench::generar::{cadena_json, Azar};
use bench::mutar::{base64, clasificar, mutar, Esperado, Mutacion, SEMANTICAS, SINTACTICAS};

struct Args {
    escribir: bool,
    invalidos: usize,
    malformados: usize,
    semilla: u64,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        escribir: false,
        invalidos: 20,
        malformados: 10,
        semilla: 1,
    };
    let mut it = std::env::args().skip(1);
    let num = |it: &mut std::iter::Skip<std::env::Args>, o: &str| -> Result<u64, String> {
        it.next()
            .and_then(|v| v.parse().ok())
            .ok_or(format!("{o} espera un entero"))
    };
    while let Some(x) = it.next() {
        match x.as_str() {
            "--escribir" => a.escribir = true,
            "--invalidos" => a.invalidos = num(&mut it, "--invalidos")? as usize,
            "--malformados" => a.malformados = num(&mut it, "--malformados")? as usize,
            "--semilla" => a.semilla = num(&mut it, "--semilla")?,
            otro => return Err(format!("opción desconocida: {otro}")),
        }
    }
    Ok(a)
}

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cases")
}

/// `valid-gen-pedido-01` → `gen-pedido`.
fn tema(id: &str) -> String {
    let s = id.strip_prefix("valid-").unwrap_or(id);
    match s.rsplit_once('-') {
        Some((t, n)) if n.chars().all(|c| c.is_ascii_digit()) => t.to_owned(),
        _ => s.to_owned(),
    }
}

/// Un esquema compilado para los dos validadores.
type Jueces = (automaton::Automaton, baseline::JsonBaseline);

/// Busca un payload mutado con `m` que los dos validadores clasifiquen igual
/// y en la categoría pedida; un malformado tiene que dar lo mismo con el
/// esquema `true` (`libre`).
fn buscar(
    payload: &[u8],
    m: Mutacion,
    semilla: u64,
    (a, lb): (&automaton::Automaton, &baseline::JsonBaseline),
    libre: &Jueces,
    malformado: bool,
) -> Option<(Vec<u8>, Esperado, u64)> {
    for k in 0..40 {
        let s = semilla.wrapping_add(k);
        let Some(p) = mutar(payload, m, &mut Azar::new(s)) else {
            continue;
        };
        match clasificar(a, lb, &p) {
            Some(e @ Esperado::Invalido { .. }) if !malformado => return Some((p, e, s)),
            Some(e @ Esperado::Malformado { .. })
                if malformado && clasificar(&libre.0, &libre.1, &p).as_ref() == Some(&e) =>
            {
                return Some((p, e, s))
            }
            _ => {}
        }
    }
    None
}

fn caso_json(
    id: &str,
    categoria: &str,
    descripcion: &str,
    esquema: &str,
    p: &[u8],
    e: &Esperado,
) -> String {
    let payload = match std::str::from_utf8(p) {
        Ok(s) => format!("\"payload\": {}", cadena_json(s)),
        Err(_) => format!("\"payloadBase64\": \"{}\"", base64(p)),
    };
    let expected = match e {
        Esperado::Invalido { violaciones, offset, .. } => format!(
            "{{\n    \"valid\": false,\n    \"violations\": {violaciones},\n    \"errorKind\": \"schema\",\n    \"offset\": {offset}\n  }}"
        ),
        Esperado::Malformado { lexico, offset } => format!(
            "{{\n    \"valid\": false,\n    \"errorKind\": \"{}\",\n    \"offset\": {offset}\n  }}",
            if *lexico { "lexical" } else { "syntax" }
        ),
    };
    format!(
        "{{\n  \"_id\": {},\n  \"category\": \"{categoria}\",\n  \"format\": \"json\",\n  \"description\": {},\n  \"phase\": 6,\n  \"schema\": {esquema},\n  {payload},\n  \"expected\": {expected}\n}}\n",
        cadena_json(id),
        cadena_json(descripcion),
    )
}

fn main() -> std::process::ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mutar: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let casos = match corpus::casos() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mutar: no se pudo leer el corpus: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let Ok(t) = bench::automata(b"true") else {
        eprintln!("mutar: el esquema `true` no compila");
        return std::process::ExitCode::from(2);
    };
    let Some(lb_true) =
        baseline::with_stack(|_| baseline::JsonBaseline::compile_text(b"true").ok())
    else {
        eprintln!("mutar: la línea base no compila `true`");
        return std::process::ExitCode::from(2);
    };
    let libre: Jueces = (t, lb_true);
    // Fuentes: los válidos JSON con M y con línea base.
    let mut fuentes = Vec::new();
    for c in casos
        .iter()
        .filter(|c| c.category == "valid" && c.format == Format::Json)
    {
        let Payload::Bytes(payload) = &c.payload else {
            continue;
        };
        let Ok(m) = bench::automata(&c.schema_text) else {
            eprintln!("mutar: {}: sin M (se omite)", c.id);
            continue;
        };
        let Some(lb) =
            baseline::with_stack(|_| baseline::JsonBaseline::compile_text(&c.schema_text).ok())
        else {
            eprintln!("mutar: {}: la línea base no lo compila (se omite)", c.id);
            continue;
        };
        fuentes.push((
            c,
            payload,
            m,
            lb,
            String::from_utf8_lossy(&c.schema_text).into_owned(),
        ));
    }
    let (mut inv, mut mal) = (0usize, 0usize);
    let (mut rot_inv, mut rot_mal) = (0usize, 0usize);
    let mut ids: Vec<String> = Vec::new();
    let mut usados: Vec<(usize, Mutacion)> = Vec::new();
    // Dos rondas: en cada una, como mucho un inválido por fuente; malformados,
    // uno por fuente y solo en la primera. Las mutaciones rotan entre fuentes.
    for ronda in 0..2u64 {
        for (i, (c, payload, m, lb, esquema)) in fuentes.iter().enumerate() {
            for malformado in [false, true] {
                let (total, max, lista, rot) = if malformado {
                    (&mut mal, a.malformados, SINTACTICAS, &mut rot_mal)
                } else {
                    (&mut inv, a.invalidos, SEMANTICAS, &mut rot_inv)
                };
                if *total >= max || (malformado && ronda > 0) {
                    continue;
                }
                let semilla = a.semilla.wrapping_add(1000 * i as u64 + 100 * ronda);
                let hallado = (0..lista.len()).find_map(|k| {
                    let mu = lista[(*rot + k) % lista.len()];
                    if usados.contains(&(i, mu)) {
                        return None;
                    }
                    buscar(payload, mu, semilla, (m, lb), &libre, malformado).map(|r| (k, mu, r))
                });
                let Some((k, mu, (p, e, s))) = hallado else {
                    continue;
                };
                *rot += k + 1;
                *total += 1;
                usados.push((i, mu));
                let (categoria, prefijo) = if malformado {
                    ("malformed", "malformed-json-mut")
                } else {
                    ("invalid", "invalid-mut")
                };
                let base = format!("{prefijo}-{}", tema(&c.id));
                let n = (1..)
                    .find(|n| !ids.contains(&format!("{base}-{n:02}")))
                    .unwrap_or(1);
                let id = format!("{base}-{n:02}");
                ids.push(id.clone());
                let detalle = match &e {
                    Esperado::Invalido {
                        violaciones,
                        palabras,
                        ..
                    } => {
                        let mut ps = palabras.clone();
                        ps.dedup();
                        format!("{violaciones} violación(es) ({})", ps.join(", "))
                    }
                    Esperado::Malformado { lexico, offset } => {
                        format!(
                            "error {} en el byte {offset}",
                            if *lexico { "léxico" } else { "sintáctico" }
                        )
                    }
                };
                let descripcion = format!(
                    "Mutación «{}» de {} con la semilla {s} (F6.3): {detalle}; el DPDA y la línea base coinciden.",
                    mu.nombre(),
                    c.id
                );
                println!("{id}: {descripcion}\n    {}", String::from_utf8_lossy(&p));
                if a.escribir {
                    let ruta = raiz().join(categoria).join(format!("{id}.json"));
                    if let Err(err) = std::fs::write(
                        &ruta,
                        caso_json(&id, categoria, &descripcion, esquema, &p, &e),
                    ) {
                        eprintln!("mutar: no se pudo escribir {}: {err}", ruta.display());
                        return std::process::ExitCode::from(2);
                    }
                }
            }
        }
    }
    eprintln!(
        "mutar: {inv} inválidos y {mal} malformados{}",
        if a.escribir {
            " escritos en tests/cases/"
        } else {
            ""
        }
    );
    std::process::ExitCode::SUCCESS
}
