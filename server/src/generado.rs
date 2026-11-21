//! F7.5 — El código generado en el servidor (D81; `docs/fase7/codigo.md`):
//! descargarlo como ZIP (`POST /api/code`) y validarlo **compilado y
//! ejecutado dentro del sandbox** (`POST /api/validate`, F7.3), comparado
//! con el intérprete de referencia (M sobre `runtime-rs`).

use automaton::{Automaton, ErrorKind, Mode};
use codegen::Archivo;
use runtime_rs::Report;
use serde_json::{json, Value};

use crate::sandbox::{self, Lenguaje, Resultado, Sandbox, Trabajo};
use crate::vistas::{construir, Fallo};

/// Lenguaje por defecto en `POST /api/code` y `POST /api/validate` (D82 (1)):
/// el mismo en las dos; C porque compila en 1-2 s (Rust con `-O`, decenas).
pub const LANG_POR_DEFECTO: &str = "c";

/// `rust` o `c`.
pub fn lenguaje(s: &str) -> Option<Lenguaje> {
    match s {
        "rust" => Some(Lenguaje::Rust),
        "c" => Some(Lenguaje::C),
        _ => None,
    }
}

fn nombre(l: Lenguaje) -> &'static str {
    match l {
        Lenguaje::Rust => "rust",
        Lenguaje::C => "c",
    }
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

/// El reporte del intérprete con los campos de la línea JSON del binario
/// generado (`docs/fase5/codegen.md` §3).
pub fn informe(a: &Automaton, r: &Report) -> Value {
    json!({
        "valid": r.valid,
        "violations": r.violations.get(),
        "errorKind": r.error_kind.map(clase),
        "offset": r.first.map(|f| f.offset),
        "line": r.first.map(|f| f.line),
        "column": r.first.map(|f| f.column),
        "pointer": r.first.map(|_| String::from_utf8_lossy(r.pointer.as_bytes()).into_owned()),
        "pointerTruncated": r.pointer.truncated(),
        "rule": r.first.and_then(|f| f.rule).map(|k| a.rule_location(k).to_owned()),
    })
}

fn emitir(a: &Automaton, l: Lenguaje) -> Vec<Archivo> {
    match l {
        Lenguaje::Rust => codegen::rust::emit(a),
        Lenguaje::C => codegen::c::emit(a),
    }
}

fn leeme(l: Lenguaje, formato: &str) -> String {
    let (compilar, ejecutar) = match l {
        Lenguaje::Rust => (
            "rustc --edition 2021 -O main.rs -o validador",
            "./validador < payload",
        ),
        Lenguaje::C => (
            "cc -std=c11 -O2 -Wall -Wextra -o validador *.c",
            "./validador < payload",
        ),
    };
    format!(
        "Validador generado por schemac (Validador de esquemas JSON/XML con autómatas de pila ponderados).\n\
         Formato del payload: {formato}. Sin dependencias: todo el núcleo va copiado aquí.\n\n\
         Compilar:  {compilar}\n\
         Ejecutar:  {ejecutar}\n\n\
         Escribe una línea JSON en stdout (valid, violations, errorKind, offset, line, column,\n\
         pointer, rule, keyword) y sale con 0 (válido), 1 (inválido), 2 (malformado o límite)\n\
         o 3 (error interno o de E/S). Valida en streaming, con memoria fija y pila acotada.\n"
    )
}

/// Tope del ZIP (E3C-3, `docs/fase7/pendientes.md` §19): un esquema de 107 KB
/// daba 6,4 MB. Más que esto es 422 `too-large`.
pub const MAX_ZIP: usize = 8 << 20;

/// `POST /api/code?lang=…`: el ZIP con el código y un `LEEME.txt`.
pub fn zip(esquema: &[u8], l: Lenguaje) -> Result<(String, Vec<u8>), Fallo> {
    let (f, a) = construir(esquema)?;
    let archivos = emitir(&a, l);
    let texto = leeme(l, f.nombre());
    let mut lista: Vec<(&str, &[u8])> = archivos
        .iter()
        .map(|x| (x.nombre.as_str(), x.contenido.as_bytes()))
        .collect();
    lista.push(("LEEME.txt", texto.as_bytes()));
    let total: usize = lista.iter().map(|(_, d)| d.len()).sum();
    if total > MAX_ZIP {
        return Err(Fallo::Payload(json!({
            "error": "too-large",
            "limit": MAX_ZIP,
            "message": format!("el código generado ocupa {total} bytes; el máximo es {MAX_ZIP}"),
        })));
    }
    Ok((
        format!("validador-{}-{}.zip", f.nombre(), nombre(l)),
        crate::zip::zip(&lista),
    ))
}

/// `POST /api/validate`: `{"schema", "payload", "lang": "rust"|"c"}`. Emite
/// el código, lo compila y lo ejecuta en el sandbox con el payload por stdin,
/// y compara su línea JSON con el intérprete.
pub fn validar(cuerpo: &[u8], sb: Option<&Sandbox>) -> Result<String, Fallo> {
    let v: Value = serde_json::from_slice(cuerpo).map_err(|e| {
        Fallo::Peticion(format!(
            "se esperaba {{\"schema\", \"payload\", \"lang\"}} en JSON: {e}"
        ))
    })?;
    let (Some(esquema), Some(payload)) = (v["schema"].as_str(), v["payload"].as_str()) else {
        return Err(Fallo::Peticion(
            "faltan `schema` o `payload` (cadenas)".into(),
        ));
    };
    let l = lenguaje(v["lang"].as_str().unwrap_or(LANG_POR_DEFECTO))
        .ok_or_else(|| Fallo::Peticion("`lang` debe ser `rust` o `c`".into()))?;
    let sb = sb.ok_or(Fallo::SinSandbox)?;
    let (formato, a) = construir(esquema.as_bytes())?;
    let interprete = informe(
        &a,
        &automaton::validate(&a, payload.as_bytes(), Mode::Total, 4096),
    );
    let archivos = emitir(&a, l);
    let lista: Vec<(&str, &[u8])> = archivos
        .iter()
        .map(|x| (x.nombre.as_str(), x.contenido.as_bytes()))
        .collect();
    let inicio = std::time::Instant::now();
    let r = sandbox::ejecutar(
        sb,
        &Trabajo {
            lenguaje: l,
            archivos: &lista,
            payload: payload.as_bytes(),
        },
    );
    let ms = inicio.elapsed().as_millis() as u64;
    let (estado, codigo, generado) = match r {
        Resultado::Terminado { codigo, stdout, .. } => {
            let linea = String::from_utf8_lossy(&stdout);
            let g: Value = serde_json::from_str(linea.trim()).unwrap_or(Value::Null);
            ("ok", Some(codigo), g)
        }
        Resultado::ErrorCompilacion { diagnostico } => {
            return Err(Fallo::Codegen(format!(
                "el código generado no compiló: {}",
                String::from_utf8_lossy(&diagnostico)
                    .chars()
                    .take(500)
                    .collect::<String>()
            )))
        }
        Resultado::TiempoAgotado { compilando } => (
            if compilando {
                "compile-timeout"
            } else {
                "run-timeout"
            },
            None,
            Value::Null,
        ),
        Resultado::Matado { .. } => ("killed", None, Value::Null),
        Resultado::Rechazado(m) => return Err(Fallo::Sandbox(m)),
    };
    let campos = ["valid", "violations", "errorKind", "offset"];
    let coinciden = generado
        .is_object()
        .then(|| campos.iter().all(|k| generado[k] == interprete[k]));
    Ok(json!({
        "format": formato.nombre(),
        "lang": nombre(l),
        "status": estado,
        "exitCode": codigo,
        "sandboxMs": ms,
        "generated": generado,
        "interpreter": interprete,
        "agree": coinciden,
        "files": archivos.len(),
    })
    .to_string())
}
