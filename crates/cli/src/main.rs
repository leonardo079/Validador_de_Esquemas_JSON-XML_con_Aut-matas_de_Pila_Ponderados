//! `schemac`: interfaz de línea de comandos del compilador.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use ir::{InternalError, VerifiedIr};

const USO: &str = "\
uso: schemac <comando>

comandos:
  check <esquema.json>       analiza el esquema (Fases 2 y 3: sintaxis, meta-esquema,
                             resolución de $ref y ciclos no productivos) y lista sus
                             errores con línea y columna
  ir [--dot] <esquema.json>  analiza el esquema, lo baja a la IR verificada y la
                             escribe en JSON (o en DOT con --dot), con la clase
                             (a)/(b)/(c) y la estrategia de cada nodo y regla
  dot <utf8|json|xml>        escribe el AFD indicado en formato DOT (Graphviz)
  version                    muestra la versión

el esquema debe ser un archivo regular de como mucho 16 MiB.

códigos de salida: 0 bien (también si el lector de la salida la cierra
antes de tiempo); 1 el esquema tiene errores; 2 no se pudo leer el archivo,
supera el tamaño máximo, los argumentos son inválidos, no se pudo escribir
la salida, no se pudo crear el hilo de análisis o (solo en check) fallo
interno; 3 error interno del compilador en ir";

/// Pila del hilo que analiza el esquema. El parser es recursivo y su tope de
/// profundidad (256) está garantizado con ≥ 2 MiB de pila (D7,
/// `docs/fase2/gramatica-esquema.md` §6); el hilo principal de Windows solo
/// tiene 1 MiB, así que el análisis nunca corre en él.
const STACK_SIZE: usize = 8 << 20;

/// Código de salida de un error interno del compilador en `schemac ir`: la
/// IR bajada no pasa `Ir::verify` (`docs/fase3/ir.md` §4) o el análisis hace
/// pánico. No es culpa del esquema (1) ni de la entrada (2).
const SALIDA_INTERNA: u8 = 3;

/// Errores internos que se muestran en el detalle; el resto solo se cuenta.
const MAX_DETALLE: usize = 8;

/// Tamaño máximo del esquema que lee `schemac`: 16 MiB
/// (`docs/00-alcance.md` §4). Se lee como mucho un byte más, así que un
/// archivo enorme nunca se carga entero. Lo que no es un archivo regular
/// (un directorio, una FIFO, `/dev/zero`) se rechaza antes de abrirlo
/// ([`leer`]): abrir una FIFO sin escritor bloquearía.
const MAX_SCHEMA_BYTES: u64 = 16 << 20;

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
            ExitCode::from(imprimir(&out))
        }
        ([Some("check"), _], [_, ruta]) => check(Path::new(ruta)),
        // Una ruta que empiece por `--` es una opción desconocida, en
        // cualquier posición (`ir --dot --json`, `ir --json --dot`).
        ([Some("ir"), Some("--dot"), w], [_, _, ruta])
        | ([Some("ir"), w, Some("--dot")], [_, ruta, _])
            if !es_opcion(*w) =>
        {
            ir(Path::new(ruta), Salida::Dot)
        }
        ([Some("ir"), w], [_, ruta]) if !es_opcion(*w) => ir(Path::new(ruta), Salida::Json),
        ([Some("version")], _) => ExitCode::from(imprimir(&format!(
            "schemac {}\n",
            env!("CARGO_PKG_VERSION")
        ))),
        _ => error("argumentos inválidos"),
    }
}

/// Un argumento con forma de opción (`--algo`).
fn es_opcion(w: Option<&str>) -> bool {
    w.is_some_and(|w| w.starts_with("--"))
}

/// Escribe una línea de diagnóstico. Si falla (stderr cerrado o roto), se
/// ignora: `eprintln!` haría pánico, y no hay otro sitio donde avisar.
fn avisar(err: &mut dyn Write, linea: &str) {
    let _ = writeln!(err, "{linea}");
}

/// Lee el esquema y ejecuta `f` sobre sus bytes en un hilo con pila
/// explícita (D7). `Err` es el código de salida: 2 si no se pudo leer o
/// crear el hilo, `panico` si `f` hizo pánico (el pánico se imprime en
/// stderr por el gancho por defecto). Los mensajes van a `err`.
fn en_hilo<T: Send + 'static>(
    path: &Path,
    nombre: &str,
    panico: u8,
    err: &mut dyn Write,
    f: impl FnOnce(&[u8]) -> T + Send + 'static,
) -> Result<T, u8> {
    let ruta = path.display();
    let bytes = match leer(path) {
        Ok(b) => b,
        Err(e) => {
            avisar(err, &format!("schemac: {ruta}: {e}"));
            return Err(2);
        }
    };
    let hilo = std::thread::Builder::new()
        .name(nombre.into())
        .stack_size(STACK_SIZE)
        .spawn(move || f(&bytes));
    match hilo.map(std::thread::JoinHandle::join) {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(_)) => {
            avisar(
                err,
                &format!("schemac: {ruta}: fallo interno al analizar el esquema"),
            );
            Err(panico)
        }
        Err(e) => {
            avisar(
                err,
                &format!("schemac: no se pudo crear el hilo de análisis: {e}"),
            );
            Err(2)
        }
    }
}

/// Lee el esquema: solo un archivo regular (no un directorio, una FIFO ni
/// `/dev/zero`) y como mucho [`MAX_SCHEMA_BYTES`]. Lee un byte más que el
/// tope para saber si lo pasa, sin cargar el resto.
///
/// El tipo se comprueba **antes de abrir** (`fs::metadata`), porque abrir
/// una FIFO sin escritor se bloquea esperándolo (4.2-L1,
/// `docs/fase3/pendientes.md` §19), y otra vez sobre el descriptor abierto.
/// Entre las dos queda una carrera (la ruta se cambia por una FIFO justo
/// después de la primera): las rutas de la CLI son de **confianza**, las
/// elige quien ejecuta `schemac`. El servidor de la Fase 7 no pasa rutas de
/// los usuarios, sino bytes.
fn leer(path: &Path) -> Result<Vec<u8>, String> {
    let no_regular = || "no es un archivo regular".to_owned();
    if !std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err(no_regular());
    }
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let meta = f.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err(no_regular());
    }
    let mut bytes = Vec::new();
    f.take(MAX_SCHEMA_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SCHEMA_BYTES {
        return Err(format!(
            "el esquema supera el tamaño máximo de {MAX_SCHEMA_BYTES} bytes (16 MiB)"
        ));
    }
    Ok(bytes)
}

/// Escribe `texto` en stdout: [`imprimir_en`] con stdout y stderr.
fn imprimir(texto: &str) -> u8 {
    imprimir_en(&mut io::stdout().lock(), &mut io::stderr(), texto)
}

/// Escribe `texto` en `out` y devuelve el código de salida: 0 si se
/// escribió, o si el lector cerró la tubería (`schemac ir … | head`: la
/// convención de Unix es terminar en silencio); 2 con un mensaje en `err`
/// si la escritura falla por otra causa (disco lleno…). `print!` haría
/// pánico.
fn imprimir_en(out: &mut dyn Write, err: &mut dyn Write, texto: &str) -> u8 {
    match out.write_all(texto.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => 0,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            avisar(err, &format!("schemac: no se pudo escribir la salida: {e}"));
            2
        }
    }
}

/// `schemac check`: 0 si el esquema es válido, 1 si tiene errores, 2 si no
/// se pudo leer, escribir la salida o analizar (un pánico también da 2).
fn check(path: &Path) -> ExitCode {
    let ruta = path.display();
    let mut err = io::stderr();
    let resultado = match en_hilo(path, "schemac-check", 2, &mut err, analizar) {
        Ok(r) => r,
        Err(code) => return ExitCode::from(code),
    };
    let errores = match resultado {
        Ok(resumen) => {
            return ExitCode::from(imprimir(&format!(
                "{ruta}: esquema válido (subconjunto de JSON Schema 2020-12); {resumen}\n"
            )));
        }
        Err(errores) => errores,
    };
    for e in &errores {
        avisar(&mut err, &format!("{ruta}: {e}"));
    }
    ExitCode::from(1)
}

/// Resumen de la tabla de símbolos si el esquema es válido; si no, el error
/// de la Fase 2 (el primero: el parser se detiene en él) o todos los de la
/// Fase 3 (meta-esquema, resolución de `$ref` y ciclos no productivos, por
/// etapas: ver `semantic::analyze`), como mensajes de una línea.
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

/// Formato de salida de `schemac ir`.
#[derive(Clone, Copy, Debug)]
enum Salida {
    Json,
    Dot,
}

/// Por qué `schemac ir` no da una IR.
#[derive(Debug)]
enum Fallo {
    /// Los mismos errores que `schemac check` (código 1).
    Esquema(Vec<String>),
    /// La IR no pasa `verify` (código 3).
    Interno(InternalError),
}

/// `schemac ir`: 0 con la IR en stdout; 1 con los errores del esquema, los
/// mismos que `check`; 2 si no se pudo leer el esquema o escribir la
/// salida; 3 si hay un error interno.
fn ir(path: &Path, salida: Salida) -> ExitCode {
    ExitCode::from(ejecutar_ir(
        path,
        salida,
        bajar,
        &mut io::stdout().lock(),
        &mut io::stderr(),
    ))
}

/// El cuerpo de `schemac ir` con la bajada y los destinos como parámetros
/// (en el programa, siempre [`bajar`], stdout y stderr; las pruebas le pasan
/// una bajada que falla o hace pánico para recorrer las ramas del error
/// interno hasta el código de salida, y búferes para no escribir en la
/// salida de `cargo test`). Escribe la IR en `out` o los errores en `err` y
/// devuelve el código.
fn ejecutar_ir(
    path: &Path,
    salida: Salida,
    bajar: fn(&[u8]) -> Result<VerifiedIr, Fallo>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let ruta = path.display().to_string();
    let resultado = match en_hilo(path, "schemac-ir", SALIDA_INTERNA, err, move |b| {
        bajar(b).map(|ir| escribir(&ir, salida))
    }) {
        Ok(r) => r,
        Err(code) => return code,
    };
    match resultado {
        Ok(texto) => imprimir_en(out, err, &texto),
        Err(Fallo::Esquema(errores)) => {
            for e in &errores {
                avisar(err, &format!("{ruta}: {e}"));
            }
            1
        }
        Err(Fallo::Interno(e)) => {
            for linea in informe_interno(&ruta, &e) {
                avisar(err, &linea);
            }
            SALIDA_INTERNA
        }
    }
}

/// Fases 2 y 3 y la bajada a la IR verificada. Los errores del esquema son
/// los de [`analizar`].
fn bajar(bytes: &[u8]) -> Result<VerifiedIr, Fallo> {
    let schema = schema_parser::parse(bytes).map_err(|e| Fallo::Esquema(vec![e.to_string()]))?;
    let table = semantic::analyze(&schema)
        .map_err(|e| Fallo::Esquema(e.iter().map(ToString::to_string).collect()))?;
    semantic::lower(&table).map_err(Fallo::Interno)
}

fn escribir(ir: &VerifiedIr, salida: Salida) -> String {
    let mut out = String::new();
    match salida {
        Salida::Json => ir::write_json(ir, &mut out),
        Salida::Dot => ir::write_dot(ir, &mut out),
    }
    .expect("escribir en un String no falla");
    out
}

/// Lo que se escribe en stderr ante un error interno: **un** error de texto
/// fijo para el usuario (D15) y, como registro, el recuento de invariantes
/// rotos y los [`MAX_DETALLE`] primeros. Tamaño acotado sea cual sea el
/// número de errores.
fn informe_interno(ruta: &str, e: &InternalError) -> Vec<String> {
    let mut out = vec![format!("{ruta}: {e}")];
    out.extend(
        e.detail(MAX_DETALLE)
            .map(|l| format!("schemac: registro: {l}")),
    );
    out
}

fn error(msg: &str) -> ExitCode {
    avisar(&mut io::stderr(), &format!("schemac: {msg}\n\n{USO}"));
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{Constraint, Format, Ir, Node, NodeId, Origin, Pos, Rule, Sort, Span, Step};

    /// IR que rompe I2 e I4: 100 `$ref` colgantes en la raíz.
    fn ir_rota() -> Ir {
        let sp = Span {
            start: Pos::START,
            end: 0,
        };
        let rules = (0..100)
            .map(|_| Constraint {
                rule: Rule::Ref(NodeId::new(usize::MAX)),
                span: sp,
            })
            .collect();
        Ir::new(
            Format::Json,
            Node {
                sort: Sort::Json,
                rules,
                origin: Origin {
                    span: sp,
                    parent: None,
                    step: Step::Root,
                },
            },
        )
    }

    /// Archivo temporal con un esquema válido, para `ejecutar_ir`.
    fn esquema_temporal(nombre: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("schemac-{nombre}-{}.json", std::process::id()));
        std::fs::write(&p, r#"{"type": "object"}"#).unwrap();
        p
    }

    /// `ejecutar_ir` con búferes como stdout y stderr: (código, salida,
    /// errores). Nada se escribe en la salida de `cargo test` (4.2-S2).
    fn ir_en_bufer(
        p: &Path,
        salida: Salida,
        bajar: fn(&[u8]) -> Result<VerifiedIr, Fallo>,
    ) -> (u8, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = ejecutar_ir(p, salida, bajar, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    /// De extremo a extremo hasta el código de salida: una bajada cuya IR
    /// no pasa `verify` da 3 (rama `Fallo::Interno`) con el error único en
    /// stderr; la bajada real del mismo archivo da 0 con el DOT en stdout.
    #[test]
    fn fallo_interno_da_codigo_3() {
        fn bajar_rota(_: &[u8]) -> Result<VerifiedIr, Fallo> {
            let (_, errores) = ir_rota().verify().unwrap_err();
            Err(Fallo::Interno(InternalError::new(errores)))
        }
        let p = esquema_temporal("interno");
        let rota = ir_en_bufer(&p, Salida::Json, bajar_rota);
        let real = ir_en_bufer(&p, Salida::Dot, bajar);
        let _ = std::fs::remove_file(&p);
        assert_eq!((rota.0, real.0), (3, 0));
        assert!(rota.1.is_empty());
        assert!(
            rota.2.contains("error interno del compilador"),
            "{}",
            rota.2
        );
        assert!(real.1.starts_with("digraph ir {"), "{}", real.1);
        assert!(real.2.is_empty(), "{}", real.2);
    }

    /// Un pánico en el hilo de análisis de `schemac ir` también es un error
    /// interno: código 3 (en `check` sigue siendo 2).
    #[test]
    fn panico_en_el_hilo_da_codigo_3() {
        fn bajar_con_panico(_: &[u8]) -> Result<VerifiedIr, Fallo> {
            panic!("pánico provocado por la prueba")
        }
        let p = esquema_temporal("panico");
        let (code, _, _) = ir_en_bufer(&p, Salida::Json, bajar_con_panico);
        let check = en_hilo(&p, "prueba-check", 2, &mut Vec::new(), |_| -> () {
            panic!("pánico de prueba")
        });
        let _ = std::fs::remove_file(&p);
        assert_eq!(code, 3);
        assert_eq!(check, Err(2));
    }

    /// El error interno (`verify` falla tras la bajada) se presenta como un
    /// único error de texto fijo más un registro acotado, y su código de
    /// salida es 3, distinto de 1 (esquema) y 2 (entrada).
    #[test]
    fn error_interno_unico_con_codigo_3() {
        let (_, errores) = ir_rota().verify().unwrap_err();
        let lineas = informe_interno("e.json", &InternalError::new(errores));
        assert_eq!(
            lineas[0],
            "e.json: error interno del compilador: la IR no cumple sus invariantes"
        );
        // 100 aristas colgantes (I2) y 99 `$ref` repetidos (I4).
        assert_eq!(
            lineas[1],
            "schemac: registro: 199 invariantes rotos; se muestran los 8 primeros"
        );
        assert_eq!(lineas.len(), 2 + MAX_DETALLE);
        assert_eq!(SALIDA_INTERNA, 3);
        assert!(USO.contains("3 error interno del compilador"));
    }
}
