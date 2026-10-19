//! `schemac`: interfaz de línea de comandos del compilador.

use std::process::ExitCode;

const USO: &str = "\
uso: schemac <comando>

comandos:
  dot <utf8|json>   escribe el AFD indicado en formato DOT (Graphviz)
  version           muestra la versión";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["dot", afd] => {
            let mut out = String::new();
            let res = match *afd {
                "utf8" => lexer::utf8::write_dot(&mut out),
                "json" => lexer::json::write_dot(&mut out),
                _ => return error(&format!("AFD desconocido: {afd}")),
            };
            res.expect("escribir en un String no falla");
            print!("{out}");
            ExitCode::SUCCESS
        }
        ["version"] => {
            println!("schemac {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => error("argumentos inválidos"),
    }
}

fn error(msg: &str) -> ExitCode {
    eprintln!("schemac: {msg}\n\n{USO}");
    ExitCode::from(2)
}
