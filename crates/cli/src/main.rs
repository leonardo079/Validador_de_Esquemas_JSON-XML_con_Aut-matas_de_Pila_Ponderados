//! `schemac`: interfaz de línea de comandos del compilador.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use ir::{InternalError, VerifiedIr};

const USO: &str = "\
uso: schemac <comando>

comandos:
  check <esquema>        analiza el esquema (Fases 2 y 3; en JSON Schema: sintaxis,
                         meta-esquema, resolución de $ref y ciclos no productivos;
                         en XSD: sintaxis, nombres, tipos y modelos de contenido)
                         y lista sus errores con línea y columna
  ir [--dot] <esquema>   analiza el esquema, lo baja a la IR verificada y la
                         escribe en JSON (o en DOT con --dot), con la clase
                         (a)/(b)/(c) y la estrategia de cada nodo y regla
  dot <utf8|json|xml>    escribe el AFD indicado en formato DOT (Graphviz)
  version                muestra la versión

el formato del esquema sale de su contenido, no de su nombre: si su primer
carácter significativo (tras un BOM y espacios) es «<», es XSD (subconjunto
de XSD 1.0); si no, JSON Schema (subconjunto de 2020-12). Debe ser un
archivo regular de como mucho 16 MiB.

códigos de salida: 0 bien (también si el lector de la salida la cierra
antes de tiempo); 1 el esquema tiene errores; 2 no se pudo leer el archivo,
supera el tamaño máximo, los argumentos son inválidos, no se pudo escribir
la salida o no se pudo crear el hilo de análisis;
3 error interno del compilador (la IR no pasa su verificación o el análisis
falló con un pánico)";

/// Pila del hilo que analiza el esquema. El parser es recursivo y su tope de
/// profundidad (256) está garantizado con ≥ 2 MiB de pila (D7,
/// `docs/fase2/gramatica-esquema.md` §6); el hilo principal de Windows solo
/// tiene 1 MiB, así que el análisis nunca corre en él.
const STACK_SIZE: usize = 8 << 20;

/// Código de salida de un error interno del compilador: la IR bajada no
/// pasa `Ir::verify` (`docs/fase3/ir.md` §4, solo en `schemac ir`) o el
/// hilo de análisis hace pánico (en `ir` y, desde la tarea 5.4, también en
/// `check`, D33). No es culpa del esquema (1) ni de la entrada (2).
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

/// Formato del esquema, detectado por su contenido (D22).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Formato {
    Json,
    Xsd,
}

/// D22: si el primer byte significativo (tras un BOM UTF-8 y los espacios
/// de XML y de JSON, que son los mismos cuatro) es `<`, el esquema es XSD;
/// si no (también si no hay ninguno), JSON. El BOM no se quita de la
/// entrada: los dos lexers lo saltan (y en JSON un prefijo incompleto de
/// BOM es `UnexpectedByte` en el offset 0, D10). Mira solo el principio:
/// O(espacios iniciales).
fn formato(bytes: &[u8]) -> Formato {
    let sin_bom = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match sin_bom
        .iter()
        .find(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
    {
        Some(b'<') => Formato::Xsd,
        _ => Formato::Json,
    }
}

/// Lee el esquema y ejecuta `f` sobre sus bytes en un hilo con pila
/// explícita (D7). `Err` es el código de salida: 2 si no se pudo leer o
/// crear el hilo, [`SALIDA_INTERNA`] si `f` hizo pánico (el pánico se
/// imprime en stderr por el gancho por defecto). Los mensajes van a `err`.
///
/// Todo lo que construye el AST lo hace y lo libera dentro de `f`: los
/// `Drop`/`Clone`/`Debug` derivados del AST XSD son recursivos (D36), así
/// que el AST nunca sale del hilo; `f` devuelve solo texto o la IR.
fn en_hilo<T: Send + 'static>(
    path: &Path,
    nombre: &str,
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
            Err(SALIDA_INTERNA)
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
/// El tipo se comprueba **antes de abrir** (`fs::metadata`), para no abrir
/// siquiera un dispositivo o una FIFO en el caso normal, y otra vez sobre el
/// descriptor abierto ([`abrir`]), que es la comprobación que decide. Abrir
/// una FIFO sin escritor se bloquea esperándolo (4.2-L1,
/// `docs/fase3/pendientes.md` §19); en Unix [`abrir`] usa `O_NONBLOCK`, así
/// que tampoco se bloquea si la ruta se cambia por una FIFO entre las dos
/// comprobaciones (4.3-L1, §23). Las rutas de la CLI siguen siendo de
/// **confianza**, las elige quien ejecuta `schemac`; el servidor de la
/// Fase 7 no pasa rutas de los usuarios, sino bytes.
fn leer(path: &Path) -> Result<Vec<u8>, String> {
    if !std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err(NO_REGULAR.to_owned());
    }
    let f = abrir(path)?;
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

const NO_REGULAR: &str = "no es un archivo regular";

/// Abre `path` para leer y comprueba sobre el descriptor que es un archivo
/// regular. En Unix lo abre con `O_NONBLOCK` (de `libc`, cuyo valor cambia
/// entre sistemas): abrir una FIFO sin escritor vuelve enseguida en lugar de
/// esperarlo, y la comprobación la rechaza. En un archivo regular
/// `O_NONBLOCK` no cambia nada (POSIX). Fuera de Unix basta `File::open`:
/// no hay FIFOs con nombre en el sistema de archivos.
fn abrir(path: &Path) -> Result<std::fs::File, String> {
    let mut opciones = std::fs::OpenOptions::new();
    opciones.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.custom_flags(libc::O_NONBLOCK);
    }
    let f = opciones.open(path).map_err(|e| e.to_string())?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(NO_REGULAR.to_owned());
    }
    Ok(f)
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
/// se pudo leer o escribir la salida, 3 si el análisis hace pánico (D33).
fn check(path: &Path) -> ExitCode {
    ExitCode::from(ejecutar_check(
        path,
        analizar,
        &mut io::stdout().lock(),
        &mut io::stderr(),
    ))
}

/// El cuerpo de `schemac check` con el análisis y los destinos como
/// parámetros, como [`ejecutar_ir`] (las pruebas le pasan un análisis que
/// hace pánico). Escribe el resumen en `out` o los errores en `err` y
/// devuelve el código.
fn ejecutar_check(
    path: &Path,
    analizar: fn(&[u8]) -> Result<String, Vec<String>>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let ruta = path.display();
    let resultado = match en_hilo(path, "schemac-check", err, analizar) {
        Ok(r) => r,
        Err(code) => return code,
    };
    match resultado {
        Ok(resumen) => imprimir_en(out, err, &format!("{ruta}: {resumen}\n")),
        Err(errores) => {
            for e in &errores {
                avisar(err, &format!("{ruta}: {e}"));
            }
            1
        }
    }
}

/// Fases 2 y 3 del formato que dice el contenido (D22): si el esquema es
/// válido, «esquema válido (subconjunto de …)» con un resumen; si no, el
/// error de la Fase 2 (el primero: los dos parsers se detienen en él) o
/// todos los de la primera etapa de la Fase 3 que falla, como mensajes de
/// una línea.
fn analizar(bytes: &[u8]) -> Result<String, Vec<String>> {
    match formato(bytes) {
        Formato::Json => analizar_json(bytes),
        Formato::Xsd => analizar_xsd(bytes),
    }
}

/// JSON Schema: resumen de la tabla de símbolos; errores del meta-esquema,
/// de la resolución de `$ref` y de los ciclos no productivos, por etapas
/// (`semantic::analyze`).
fn analizar_json(bytes: &[u8]) -> Result<String, Vec<String>> {
    let schema = schema_parser::parse(bytes).map_err(|e| vec![e.to_string()])?;
    match semantic::analyze(&schema) {
        Ok(t) => Ok(format!(
            "esquema válido (subconjunto de JSON Schema 2020-12); \
             {} subesquemas, {} entradas de $defs, {} referencias resueltas",
            t.len(),
            t.defs().len(),
            t.refs().len()
        )),
        Err(errores) => Err(errores.iter().map(ToString::to_string).collect()),
    }
}

/// XSD: número de tipos complejos (con nombre y anónimos); errores de
/// componentes y nombres o de modelos de contenido, por etapas
/// (`xsd::analyze`, `docs/fase3/xsd.md` §3).
fn analizar_xsd(bytes: &[u8]) -> Result<String, Vec<String>> {
    let schema = xsd::parse(bytes).map_err(|e| vec![e.to_string()])?;
    match xsd::analyze(&schema) {
        Ok(a) => Ok(format!(
            "esquema válido (subconjunto de XSD 1.0); {} tipos complejos",
            a.type_count()
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
    let resultado = match en_hilo(path, "schemac-ir", err, move |b| {
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

/// Fases 2 y 3 y la bajada a la IR verificada del formato que dice el
/// contenido (D22). Los errores del esquema son los de [`analizar`].
fn bajar(bytes: &[u8]) -> Result<VerifiedIr, Fallo> {
    match formato(bytes) {
        Formato::Json => {
            let schema =
                schema_parser::parse(bytes).map_err(|e| Fallo::Esquema(vec![e.to_string()]))?;
            let table = semantic::analyze(&schema)
                .map_err(|e| Fallo::Esquema(e.iter().map(ToString::to_string).collect()))?;
            semantic::lower(&table).map_err(Fallo::Interno)
        }
        Formato::Xsd => {
            let schema = xsd::parse(bytes).map_err(|e| Fallo::Esquema(vec![e.to_string()]))?;
            let analysis = xsd::analyze(&schema)
                .map_err(|e| Fallo::Esquema(e.iter().map(ToString::to_string).collect()))?;
            xsd::lower(&analysis).map_err(Fallo::Interno)
        }
    }
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

    /// Un pánico en el hilo de análisis es un error interno: código 3 en
    /// `schemac ir` y también en `schemac check` (D33; antes de la tarea 5.4
    /// `check` daba 2, `docs/fase3/pendientes.md` §8). Las dos recorren su
    /// cuerpo hasta el código de salida, con el mensaje en stderr y nada en
    /// stdout.
    #[test]
    fn panico_en_el_hilo_da_codigo_3() {
        fn bajar_con_panico(_: &[u8]) -> Result<VerifiedIr, Fallo> {
            panic!("pánico provocado por la prueba")
        }
        fn analizar_con_panico(_: &[u8]) -> Result<String, Vec<String>> {
            panic!("pánico provocado por la prueba")
        }
        let p = esquema_temporal("panico");
        let (code, _, _) = ir_en_bufer(&p, Salida::Json, bajar_con_panico);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let check = ejecutar_check(&p, analizar_con_panico, &mut out, &mut err);
        let real = ejecutar_check(&p, analizar, &mut Vec::new(), &mut Vec::new());
        let _ = std::fs::remove_file(&p);
        assert_eq!((code, check, real), (3, 3, 0));
        assert!(out.is_empty());
        let err = String::from_utf8(err).unwrap();
        assert!(
            err.contains("fallo interno al analizar el esquema"),
            "{err}"
        );
    }

    /// Archivo temporal con un XSD válido de un elemento global.
    fn xsd_temporal(nombre: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("schemac-{nombre}-{}.xsd", std::process::id()));
        std::fs::write(
            &p,
            "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
             <xs:element name='e' type='xs:int'/></xs:schema>",
        )
        .unwrap();
        p
    }

    /// Criterio de la 5.4: el código 3 también con un XSD. La bajada que
    /// falla da 3 con el error único; la real del mismo archivo (que pasa
    /// por `xsd::lower`), 0 con la IR de `format: "xml"`.
    #[test]
    fn fallo_interno_xsd_da_codigo_3() {
        fn bajar_rota(_: &[u8]) -> Result<VerifiedIr, Fallo> {
            let (_, errores) = ir_rota().verify().unwrap_err();
            Err(Fallo::Interno(InternalError::new(errores)))
        }
        let p = xsd_temporal("interno");
        let rota = ir_en_bufer(&p, Salida::Dot, bajar_rota);
        let real = ir_en_bufer(&p, Salida::Json, bajar);
        let _ = std::fs::remove_file(&p);
        assert_eq!((rota.0, real.0), (3, 0), "{}", real.2);
        assert!(rota.1.is_empty());
        assert!(
            rota.2.contains("error interno del compilador"),
            "{}",
            rota.2
        );
        assert!(real.1.contains("\"format\": \"xml\""), "{}", real.1);
        assert!(real.2.is_empty(), "{}", real.2);
    }

    /// 4.3-L1 (`docs/fase3/pendientes.md` §23): `abrir`, la comprobación que
    /// decide, no se bloquea con una FIFO sin escritor aunque se salte la
    /// comprobación previa de `leer` (lo que pasa si la ruta se cambia por
    /// una FIFO entre las dos): con `O_NONBLOCK` el `open` vuelve enseguida y
    /// el descriptor no es un archivo regular. Sin `O_NONBLOCK` el hilo se
    /// queda en el `open`; la guarda de 60 s hace fallar la prueba en lugar
    /// de colgarla (no mide tiempos, D1).
    #[cfg(unix)]
    #[test]
    fn abrir_una_fifo_sin_escritor_no_bloquea() {
        let d = std::env::temp_dir().join(format!("schemac-abrir-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let fifo = d.join("esquema.json");
        let ok = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(ok.success(), "mkfifo falló");
        let (tx, rx) = std::sync::mpsc::channel();
        let f = fifo.clone();
        std::thread::spawn(move || tx.send(abrir(&f).map(|_| ())));
        let res = rx.recv_timeout(std::time::Duration::from_secs(60));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(
            res.expect("abrir se bloqueó en la FIFO"),
            Err(NO_REGULAR.to_owned())
        );
    }

    /// D22: el formato sale del primer byte significativo, tras un BOM y
    /// espacios (los cuatro de XML y de JSON): `<` es XSD; cualquier otro,
    /// también ninguno, JSON. Un BOM delante de JSON sigue siendo JSON, y un
    /// prefijo incompleto de BOM no es un BOM.
    #[test]
    fn formato_por_el_primer_byte_significativo() {
        for (bytes, esperado) in [
            (&b"<xs:schema/>"[..], Formato::Xsd),
            (b" \t\r\n<x/>", Formato::Xsd),
            (b"\xEF\xBB\xBF<x/>", Formato::Xsd),
            (b"\xEF\xBB\xBF \n<x/>", Formato::Xsd),
            (b"{\"type\": \"object\"}", Formato::Json),
            (b"\xEF\xBB\xBF{}", Formato::Json),
            (b"  true", Formato::Json),
            (b"", Formato::Json),
            (b" \n ", Formato::Json),
            // Solo se salta un BOM, y solo al principio.
            (b"\xEF\xBB\xBF\xEF\xBB\xBF<x/>", Formato::Json),
            (b" \xEF\xBB\xBF<x/>", Formato::Json),
            // Otros espacios de Unicode no cuentan.
            (b"\x0c<x/>", Formato::Json),
            (b"\xEF\xBB<x/>", Formato::Json),
        ] {
            assert_eq!(formato(bytes), esperado, "{bytes:?}");
        }
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
