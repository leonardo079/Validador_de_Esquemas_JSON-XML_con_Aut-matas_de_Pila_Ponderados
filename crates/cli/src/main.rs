//! `schemac`: interfaz de línea de comandos del compilador.

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

const USO: &str = "\
uso: schemac <comando>

comandos:
  check <esquema.json>  analiza el esquema (Fases 2 y 3: sintaxis, meta-esquema y
                        resolución de $ref) y lista sus errores con línea y columna
  dot <utf8|json|xml>   escribe el AFD indicado en formato DOT (Graphviz)
  version               muestra la versión";

/// Pila del hilo que analiza el esquema. El parser es recursivo y su tope de
/// profundidad (256) está garantizado con ≥ 2 MiB de pila (D7,
/// `docs/fase2/gramatica-esquema.md` §6); el hilo principal de Windows solo
/// tiene 1 MiB, así que el análisis nunca corre en él.
const STACK_SIZE: usize = 8 << 20;

fn main() -> ExitCode {
    // `args_os`: una ruta que no es UTF-8 (válida en Linux y en Windows) no
    // debe hacer fallar a `schemac` con un pánico. Los nombres de comando sí
    // son UTF-8; si no lo son, el argumento no coincide con ningún comando.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let words: Vec<Option<&str>> = args.iter().map(|a| a.to_str()).collect();
    match (words.as_slice(), args.as_slice()) {
        ([Some("dot"), Some(afd)], _) => {
            let mut out = String::new();
            let res = match *afd {
                "utf8" => lexer::utf8::write_dot(&mut out),
                "json" => lexer::json::write_dot(&mut out),
                "xml" => lexer::xml::write_dot(&mut out),
                _ => return error(&format!("AFD desconocido: {afd}")),
            };
            res.expect("escribir en un String no falla");
            print!("{out}");
            ExitCode::SUCCESS
        }
        ([Some("check"), _], [_, ruta]) => check(Path::new(ruta)),
        ([Some("version")], _) => {
            println!("schemac {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => error("argumentos inválidos"),
    }
}

/// `schemac check`: 0 si el esquema es válido, 1 si tiene errores, 2 si no
/// se pudo leer o analizar.
fn check(path: &Path) -> ExitCode {
    // Para los mensajes; los bytes que no son UTF-8 se muestran como U+FFFD.
    let ruta = path.display();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("schemac: {ruta}: {e}");
            return ExitCode::from(2);
        }
    };
    let hilo = std::thread::Builder::new()
        .name("schemac-check".into())
        .stack_size(STACK_SIZE)
        .spawn(move || analizar(&bytes));
    let resultado = match hilo.map(std::thread::JoinHandle::join) {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => {
            eprintln!("schemac: {ruta}: fallo interno al analizar el esquema");
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("schemac: no se pudo crear el hilo de análisis: {e}");
            return ExitCode::from(2);
        }
    };
    let errores = match resultado {
        Ok(resumen) => {
            println!("{ruta}: esquema válido (subconjunto de JSON Schema 2020-12); {resumen}");
            return ExitCode::SUCCESS;
        }
        Err(errores) => errores,
    };
    for e in &errores {
        eprintln!("{ruta}: {e}");
    }
    ExitCode::from(1)
}

/// Resumen de la tabla de símbolos si el esquema es válido; si no, el error
/// de la Fase 2 (el primero: el parser se detiene en él) o todos los de la
/// Fase 3 (meta-esquema y, si pasa, resolución de `$ref`), como mensajes de
/// una línea.
fn analizar(bytes: &[u8]) -> Result<String, Vec<String>> {
    let schema = schema_parser::parse(bytes).map_err(|e| vec![e.to_string()])?;
    match semantic::analyze(&schema) {
        Ok(t) => Ok(format!(
            "{} subesquemas, {} entradas de $defs, {} referencias resueltas",
            t.len(),
            t.defs().len(),
            t.refs().len()
        )),
        Err(errores) => Err(errores.iter().map(ToString::to_string).collect()),
    }
}

fn error(msg: &str) -> ExitCode {
    eprintln!("schemac: {msg}\n\n{USO}");
    ExitCode::from(2)
}
