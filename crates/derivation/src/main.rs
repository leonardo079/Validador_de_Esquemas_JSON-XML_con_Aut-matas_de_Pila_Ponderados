//! F5.5 — `arbol [--dot] [--max-nodos N] <payload>`: árbol de derivación del
//! payload (JSON o XML, por el primer byte) en JSON para d3-hierarchy, o en DOT
//! con `--dot`. Lo usa `make docs` para `docs/fase5/arbol-*.dot`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut dot = false;
    let mut max = derivation::MAX_NODOS;
    let mut ruta = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dot" => dot = true,
            "--max-nodos" => match it.next().and_then(|v| v.parse().ok()) {
                Some(n) => max = n,
                None => return fallar("--max-nodos espera un entero"),
            },
            _ if ruta.is_none() && !a.starts_with("--") => ruta = Some(a),
            _ => return fallar("uso: arbol [--dot] [--max-nodos N] <payload>"),
        }
    }
    let Some(ruta) = ruta else {
        return fallar("uso: arbol [--dot] [--max-nodos N] <payload>");
    };
    let bytes = match std::fs::read(&ruta) {
        Ok(b) => b,
        Err(e) => return fallar(&format!("{ruta}: {e}")),
    };
    match derivation::derivar(&bytes, max) {
        Ok(a) => {
            let nombre = std::path::Path::new(&ruta)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if dot {
                print!("{}", derivation::a_dot(&a, &nombre));
            } else {
                println!("{}", derivation::a_json(&a));
            }
            ExitCode::SUCCESS
        }
        Err(e) => fallar(&format!("{ruta}: {e}")),
    }
}

fn fallar(m: &str) -> ExitCode {
    eprintln!("arbol: {m}");
    ExitCode::from(1)
}
