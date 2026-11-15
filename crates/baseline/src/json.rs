//! Línea base JSON: `serde_json` construye el árbol (DOM) y `jsonschema`
//! lo valida (`docs/fase6/linea-base.md` §1-§3).
//!
//! El árbol se construye con un visitante propio en lugar de
//! `serde_json::from_slice::<Value>` por dos motivos (U5): `serde_json`
//! acepta claves duplicadas (gana la última), que el proyecto rechaza como
//! malformado (D2), y su tope de recursión es 128, no 256. El visitante
//! falla en la clave repetida y cuenta la profundidad; el tope propio de
//! `serde_json` se apaga (`unbounded_depth`, U3).

use std::cell::Cell;
use std::fmt::{self, Write as _};
use std::marker::PhantomData;

use jsonschema::{PatternOptions, Retrieve, Uri, Validator};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::map::Entry;
use serde_json::{Map, Number, Value};

use crate::{
    with_stack, CompileError, Kind, Limits, Malformed, Outcome, Stack, Violation, MAX_DEPTH_LIMIT,
    MAX_MESSAGE_BYTES,
};

/// Marca de orden de bytes UTF-8: se quita antes de parsear y sus 3 bytes
/// se suman a los offsets (B4; `docs/00-alcance.md` §1.1: «un BOM inicial se
/// ignora»).
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Un esquema JSON compilado por `jsonschema` (Draft 2020-12 fijo, sin red,
/// `format` como anotación, `pattern` con el motor lineal `regex`) y los
/// límites con que se leen los payloads.
///
/// Es `Send` y `Sync`: se compila una vez y se usa desde cualquier hilo de
/// [`with_stack`]. Su `Drop` es recursivo (el validador tiene la forma del
/// esquema) y corre en el hilo que lo suelta: acotado por
/// [`MAX_DEPTH_LIMIT`], pide unos 384 KiB de pila en `dev` con 256 `not`
/// anidados, y cabe en el hilo principal de Windows (1 MiB) con unas 3 veces
/// de margen (`docs/fase6/linea-base.md` §3.5, `pendientes.md` §19).
#[derive(Debug)]
pub struct JsonBaseline {
    validator: Validator,
    limits: Limits,
}

/// El árbol de un payload JSON ya leído (la primera de las dos pasadas).
///
/// Su `Drop`, `Clone`, `PartialEq` y `Debug` son recursivos. La profundidad
/// está acotada por [`MAX_DEPTH_LIMIT`], pero con 256 objetos anidados piden,
/// en `dev`, unos 320 KiB de pila (`drop`), 384 KiB (`==`, `{:?}`) y entre
/// 512 y 768 KiB (`clone`) (`docs/fase6/linea-base.md` §2). Por eso, como
/// [`Stack`], no es `Send` ni `Sync`: solo existe dentro del hilo de
/// [`with_stack`] que lo leyó (D7). Un `serde_json::Value` copiado de
/// [`value`](Self::value) sí puede salir; recorrerlo fuera de un hilo con
/// pila holgada es responsabilidad de quien lo copia.
///
/// ```compile_fail,E0277
/// fn send<T: Send>() {}
/// send::<baseline::JsonDom>();
/// ```
///
/// ```compile_fail,E0277
/// fn sync<T: Sync>() {}
/// sync::<baseline::JsonDom>();
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct JsonDom {
    value: Value,
    _solo_este_hilo: PhantomData<*const ()>,
}

impl JsonDom {
    /// El valor leído.
    pub fn value(&self) -> &Value {
        &self.value
    }
}

impl JsonBaseline {
    /// Compila `schema` con la configuración segura (B5): Draft 2020-12 fijo
    /// (aunque `$schema` diga otro), `format` sin validar, `pattern` y
    /// `patternProperties` con el motor `regex` (sin retroceso: sin
    /// *lookaround* ni retrorreferencias, que no compilan), y un recuperador
    /// que rechaza todo `$ref` fuera del documento sin abrir red ni archivos.
    ///
    /// Un esquema con más de [`MAX_DEPTH_LIMIT`] contenedores anidados no
    /// compila (se comprueba sin recursión). La compilación corre en un hilo
    /// propio con [`crate::STACK_SIZE`] de pila (D7): se paga un hilo por
    /// esquema, no por payload.
    pub fn compile(schema: &Value) -> Result<Self, CompileError> {
        profundidad_acotada(schema)?;
        with_stack(|_| construir(schema))
    }

    /// Como [`compile`](Self::compile), sobre el texto del esquema: lo lee
    /// con el mismo visitante que los payloads, así que una clave duplicada
    /// (D2), un BOM inicial y la profundidad se tratan igual que en ellos.
    pub fn compile_text(text: &[u8]) -> Result<Self, CompileError> {
        // El árbol del esquema se construye y se libera dentro del hilo.
        with_stack(|_| {
            let schema = leer(text, MAX_DEPTH_LIMIT).map_err(|o| CompileError {
                message: o.malformed.map_or_else(String::new, |m| {
                    acotado(format_args!(
                        "{} en la línea {}, columna {} (offset {:?})",
                        m.message, m.line, m.column, m.offset
                    ))
                }),
            })?;
            construir(&schema)
        })
    }

    /// Sustituye los límites de los payloads (por defecto, los de
    /// `docs/00-alcance.md` §4 y [`Limits::max_violations`]). La
    /// profundidad se recorta a [`MAX_DEPTH_LIMIT`].
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Primera pasada: lee `payload` entero a un árbol. `Err` es el
    /// resultado negativo ([`Kind::Malformed`], [`Kind::Limit`] o
    /// [`Kind::Unsupported`]) con su posición.
    pub fn parse(&self, _stack: &Stack, payload: &[u8]) -> Result<JsonDom, Outcome> {
        leer(payload, self.limits.effective_max_depth()).map(|value| JsonDom {
            value,
            _solo_este_hilo: PhantomData,
        })
    }

    /// Segunda pasada: valida el árbol y devuelve sus violaciones en el
    /// orden de `jsonschema` (B10), como mucho
    /// [`Limits::effective_max_violations`]; si había más,
    /// [`Outcome::truncated`]. Cada mensaje se recorta a
    /// [`MAX_MESSAGE_BYTES`].
    ///
    /// El tope acota la lista y sus mensajes, no el trabajo de `jsonschema`:
    /// `iter_errors` reúne todos los errores antes de entregar el primero, y
    /// un esquema con `$ref` en abanico puede costar tiempo exponencial en
    /// su tamaño (`docs/fase6/linea-base.md` §3.6).
    pub fn validate_dom(&self, _stack: &Stack, dom: &JsonDom) -> Outcome {
        let max = self.limits.effective_max_violations();
        let mut errores = self.validator.iter_errors(&dom.value);
        let violations: Vec<Violation> = errores
            .by_ref()
            .take(max)
            .map(|e| Violation {
                pointer: e.instance_path().as_str().to_owned(),
                keyword: e.kind().keyword().to_owned(),
                message: acotado(&e),
            })
            .collect();
        if violations.is_empty() {
            return Outcome::conforme();
        }
        Outcome {
            valid: false,
            kind: Some(Kind::Schema),
            truncated: errores.next().is_some(),
            violations,
            malformed: None,
        }
    }

    /// Las dos pasadas seguidas: [`parse`](Self::parse) y
    /// [`validate_dom`](Self::validate_dom).
    pub fn validate(&self, stack: &Stack, payload: &[u8]) -> Outcome {
        match self.parse(stack, payload) {
            Ok(dom) => self.validate_dom(stack, &dom),
            Err(o) => o,
        }
    }
}

/// Recuperador de recursos externos que siempre falla (B5): aunque las
/// features de red y de archivos de `jsonschema` están apagadas, así ningún
/// `$ref` fuera del documento llega a abrir nada.
struct SinRed;

impl Retrieve for SinRed {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(format!(
            "la línea base no recupera recursos externos (sin red ni archivos): {}",
            recortar(uri.as_str())
        )
        .into())
    }
}

fn construir(schema: &Value) -> Result<JsonBaseline, CompileError> {
    jsonschema::draft202012::options()
        .should_validate_formats(false)
        .with_pattern_options(PatternOptions::regex())
        .with_retriever(SinRed)
        .build(schema)
        .map(|validator| JsonBaseline {
            validator,
            limits: Limits::default(),
        })
        .map_err(|e| CompileError {
            message: acotado(&e),
        })
}

/// Comprueba, con una pila explícita, que `schema` no tiene más de
/// [`MAX_DEPTH_LIMIT`] contenedores anidados.
fn profundidad_acotada(schema: &Value) -> Result<(), CompileError> {
    // (valor, contenedores que lo rodean)
    let mut pendientes = vec![(schema, 0u32)];
    while let Some((v, fuera)) = pendientes.pop() {
        let d = fuera + 1;
        if (v.is_array() || v.is_object()) && d > MAX_DEPTH_LIMIT {
            return Err(CompileError {
                message: format!(
                    "el esquema supera la profundidad máxima de {MAX_DEPTH_LIMIT} niveles"
                ),
            });
        }
        match v {
            Value::Array(a) => pendientes.extend(a.iter().map(|h| (h, d))),
            Value::Object(o) => pendientes.extend(o.values().map(|h| (h, d))),
            _ => {}
        }
    }
    Ok(())
}

/// Lee un valor JSON completo de `bytes` (sin nada después, salvo espacios)
/// con un tope de `max_depth` contenedores anidados.
fn leer(bytes: &[u8], max_depth: u32) -> Result<Value, Outcome> {
    let (cuerpo, desfase) = match bytes.strip_prefix(BOM) {
        Some(resto) => (resto, BOM.len()),
        None => (bytes, 0),
    };
    let ctx = Contexto {
        max_depth,
        motivo: Cell::new(None),
    };
    let mut de = serde_json::Deserializer::from_slice(cuerpo);
    de.disable_recursion_limit();
    let leido = Semilla {
        depth: 0,
        ctx: &ctx,
    }
    .deserialize(&mut de)
    .and_then(|v| de.end().map(|()| v));
    leido.map_err(|e| error_de_lectura(&e, ctx.motivo.get(), bytes, cuerpo, desfase))
}

/// Por qué falló el visitante; `serde_json` solo ve un `Error::custom`.
#[derive(Clone, Copy, Debug)]
enum Motivo {
    Profundidad,
    Duplicada,
}

struct Contexto {
    max_depth: u32,
    motivo: Cell<Option<Motivo>>,
}

/// Construye el `Value` de un nivel. `depth` es el número de contenedores
/// que lo rodean.
#[derive(Clone, Copy)]
struct Semilla<'c> {
    depth: u32,
    ctx: &'c Contexto,
}

impl Semilla<'_> {
    /// Entra en un contenedor: la semilla de sus hijos, o el error de límite.
    fn entrar<E: de::Error>(self) -> Result<Self, E> {
        let depth = self.depth + 1;
        if depth > self.ctx.max_depth {
            self.ctx.motivo.set(Some(Motivo::Profundidad));
            return Err(E::custom(format_args!(
                "se supera la profundidad máxima de {} niveles",
                self.ctx.max_depth
            )));
        }
        Ok(Semilla { depth, ..self })
    }
}

impl<'de> DeserializeSeed<'de> for Semilla<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Semilla<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("un valor JSON")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        // `serde_json` nunca entrega un no finito (`1e400` falla antes).
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("número no finito"))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let hijo = self.entrar()?;
        let mut v = Vec::new();
        while let Some(x) = seq.next_element_seed(hijo)? {
            v.push(x);
        }
        Ok(Value::Array(v))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let hijo = self.entrar()?;
        let mut m = Map::new();
        while let Some(k) = map.next_key::<String>()? {
            // Se comprueba antes de leer el valor: el error queda en la clave.
            match m.entry(k) {
                Entry::Occupied(o) => {
                    self.ctx.motivo.set(Some(Motivo::Duplicada));
                    return Err(de::Error::custom(format_args!(
                        "clave duplicada \"{}\"",
                        recortar(o.key())
                    )));
                }
                Entry::Vacant(hueco) => {
                    hueco.insert(map.next_value_seed(hijo)?);
                }
            }
        }
        Ok(Value::Object(m))
    }
}

/// Los primeros 32 caracteres de `s` (con `…` si hay más), para que un
/// mensaje nunca crezca con la entrada.
fn recortar(s: &str) -> String {
    let mut r: String = s.chars().take(32).collect();
    if r.len() < s.len() {
        r.push('…');
    }
    r
}

/// El texto de `d` recortado a [`MAX_MESSAGE_BYTES`] (en un límite de
/// carácter, con `…` si se cortó). Se escribe en un búfer que deja de
/// aceptar texto al llenarse, así que un `Display` que escribe por partes no
/// llega a formatear el resto (SEC-M1, SEC-L1: los mensajes de `jsonschema`
/// copian la instancia y el esquema).
fn acotado(d: impl fmt::Display) -> String {
    struct Acotado(String);

    impl fmt::Write for Acotado {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let libre = MAX_MESSAGE_BYTES - self.0.len();
            if s.len() <= libre {
                self.0.push_str(s);
                return Ok(());
            }
            let mut corte = libre;
            while !s.is_char_boundary(corte) {
                corte -= 1;
            }
            self.0.push_str(&s[..corte]);
            Err(fmt::Error)
        }
    }

    let mut b = Acotado(String::new());
    if write!(b, "{d}").is_err() {
        // Se deja sitio para `…` (3 bytes): el total no pasa del tope.
        let mut fin = MAX_MESSAGE_BYTES - '…'.len_utf8();
        while !b.0.is_char_boundary(fin) {
            fin -= 1;
        }
        b.0.truncate(fin);
        b.0.push('…');
    }
    b.0
}

fn es_espacio(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// Convierte el error de `serde_json` en el resultado negativo, con la
/// posición en `bytes` (la entrada original, BOM incluido); `cuerpo` es la
/// entrada sin BOM, que es lo que leyó `serde_json`, y `desfase` el tamaño
/// del BOM quitado.
fn error_de_lectura(
    e: &serde_json::Error,
    motivo: Option<Motivo>,
    bytes: &[u8],
    cuerpo: &[u8],
    desfase: usize,
) -> Outcome {
    let texto = e.to_string();
    let sufijo = format!(" at line {} column {}", e.line(), e.column());
    let message = texto.strip_suffix(&sufijo).unwrap_or(&texto).to_owned();

    // Posición de `serde_json` (línea y columna en bytes, B3): la del último
    // byte consumido; en un fin de entrada, la longitud.
    let mut offset = if e.classify() == serde_json::error::Category::Eof {
        cuerpo.len()
    } else {
        offset_de(cuerpo, e.line(), e.column())
    };
    // Los errores del visitante salen en la posición donde `serde_json`
    // termina de cerrar el contenedor que falló, que puede estar unos bytes
    // después; se retrocede hasta la comilla o el `[`/`{` (`corregir`).
    if let Some(m) = motivo {
        offset = corregir(cuerpo, offset, m);
    }

    let kind = match motivo {
        Some(Motivo::Profundidad) => Kind::Limit,
        Some(Motivo::Duplicada) => Kind::Malformed,
        // U9: `serde_json` sin `arbitrary_precision` no representa números
        // fuera del rango de `f64`; el proyecto sí (compara lexemas).
        None if message == "number out of range" => Kind::Unsupported,
        None => Kind::Malformed,
    };
    let offset = offset + desfase;
    let (line, column) = linea_y_columna(bytes, offset);
    Outcome {
        valid: false,
        kind: Some(kind),
        violations: Vec::new(),
        truncated: false,
        malformed: Some(Malformed {
            line,
            column,
            offset: Some(offset as u64),
            message,
        }),
    }
}

/// Offset del byte de la línea `line` y columna `column` (desde 1, en bytes)
/// de `serde_json`. La columna 0 (ningún byte consumido en esa línea) es el
/// inicio de la línea.
fn offset_de(cuerpo: &[u8], line: usize, column: usize) -> usize {
    let inicio = if line <= 1 {
        0
    } else {
        cuerpo
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .nth(line - 2)
            .map_or(cuerpo.len(), |(i, _)| i + 1)
    };
    (inicio + column.saturating_sub(1)).min(cuerpo.len())
}

/// Retrocede desde `offset` hasta el byte que provocó el error del
/// visitante (CR-I1). Después de ese byte, el `end_map`/`end_seq` de
/// `serde_json` puede haber consumido espacios, como mucho un `]`, `}` o
/// `,`, y otra vez espacios; se deshace eso y se acepta el resultado solo si
/// es el byte esperado (la comilla que cierra la clave repetida, o el
/// `[`/`{` que pasa el tope). Si no (no debería pasar), deja `offset`.
fn corregir(cuerpo: &[u8], offset: usize, motivo: Motivo) -> usize {
    let atras_espacios = |mut i: usize| {
        while i > 0 && cuerpo.get(i).copied().is_some_and(es_espacio) {
            i -= 1;
        }
        i
    };
    let mut i = atras_espacios(offset.min(cuerpo.len().saturating_sub(1)));
    if i > 0 && matches!(cuerpo.get(i), Some(b']' | b'}' | b',')) {
        i = atras_espacios(i - 1);
    }
    let esperado: &[u8] = match motivo {
        Motivo::Profundidad => b"[{",
        Motivo::Duplicada => b"\"",
    };
    match cuerpo.get(i) {
        Some(b) if esperado.contains(b) => i,
        _ => offset,
    }
}

/// Línea y columna (desde 1, columna en bytes) del byte en `offset`, como
/// las cuenta `serde_json` (solo `\n` cambia de línea). Saturantes (D9).
fn linea_y_columna(bytes: &[u8], offset: usize) -> (u32, u32) {
    let antes = &bytes[..offset.min(bytes.len())];
    let linea = 1 + antes.iter().filter(|b| **b == b'\n').count();
    let inicio = antes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let sat = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    (sat(linea), sat(offset - inicio + 1))
}
