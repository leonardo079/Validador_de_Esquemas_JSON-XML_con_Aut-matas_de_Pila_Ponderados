//! Fase 6 — Líneas base DOM.
//!
//! Validadores de referencia que **construyen el árbol** del payload entero y
//! lo validan después, en dos pasadas, frente al DPDA del proyecto, que
//! valida en una sola pasada en streaming sin árbol (`docs/fase6/linea-base.md`).
//! Sirven para dos cosas:
//!
//! - **Oráculo** del intérprete de referencia (F4.6): sobre el corpus
//!   (`tests/cases`) la línea base da el resultado de `expected`, salvo las
//!   divergencias documentadas.
//! - **Comparación de costo** (F6.5, E1): compilar el esquema, parsear el
//!   payload y validar el árbol se pueden llamar por separado para medir cada
//!   etapa ([`JsonBaseline::compile`], [`JsonBaseline::parse`],
//!   [`JsonBaseline::validate_dom`]).
//!
//! La línea base JSON es `serde_json` (DOM) + `jsonschema` (Draft 2020-12,
//! sin red, `format` como anotación y motor de `pattern` lineal). La de XML
//! es libxml2 (DOM + XSD 1.0, sin red, sin DTD ni entidades declaradas, D8),
//! detrás de la feature `libxml2`, apagada por defecto porque necesita
//! libxml2, `pkg-config` y libclang (F6.7b; se prueba en Docker,
//! `docs/fase6/linea-base.md` §7). `XmlBaseline` no es `Send`: se compila
//! dentro del hilo de [`with_stack`] y su `compile` pide el testigo.
//!
//! **Pila (D7).** Parsear y validar el árbol es recursivo (profundidad ≤ 256,
//! [`MAX_DEPTH_LIMIT`]). Por eso [`JsonBaseline::parse`],
//! [`JsonBaseline::validate_dom`] y [`run_case`] piden un testigo [`Stack`],
//! que solo da [`with_stack`]: un hilo con [`STACK_SIZE`] de pila. El árbol
//! leído ([`JsonDom`]) tampoco sale de ese hilo (no es `Send`), porque
//! liberarlo, copiarlo o compararlo también es recursivo. La compilación del
//! esquema corre en un hilo propio con la misma pila. La excepción es el
//! `Drop` de [`JsonBaseline`]: también es recursivo, pero corre en el hilo de
//! quien lo tenga; con la profundidad acotada pide unos 384 KiB en `dev`
//! (`docs/fase6/linea-base.md` §3.5), así que cabe en el hilo principal.

use std::fmt;
use std::marker::PhantomData;

pub mod corpus;
mod json;
#[cfg(feature = "libxml2")]
mod xml;

pub use corpus::run_case;
pub use json::{JsonBaseline, JsonDom};
#[cfg(feature = "libxml2")]
pub use xml::{XmlBaseline, XmlDom};

/// Pila del hilo de [`with_stack`] y del hilo que compila un esquema: 8 MiB,
/// como `schemac` (`crates/cli/src/main.rs`). El hilo principal de Windows
/// solo tiene 1 MiB. Medido (B2, `docs/fase6/linea-base.md` §3.5): en `dev`,
/// leer y validar a profundidad 256 (también un `$ref` recursivo,
/// `ref_recursivo_a_profundidad_maxima`) cabe en 1 MiB y desborda en
/// 512 KiB; 8 MiB deja más de 8 veces de margen.
pub const STACK_SIZE: usize = 8 << 20;

/// Tope de la profundidad de anidamiento de payloads y esquemas: 256, el
/// valor por defecto de `docs/00-alcance.md` §4. [`Limits::max_depth`] solo
/// se puede bajar; un valor mayor se recorta a este, como en el parser de
/// esquemas (D7), porque la recursión de `serde_json` y de `jsonschema` solo
/// está medida hasta aquí.
pub const MAX_DEPTH_LIMIT: u32 = 256;

/// Tope, en bytes, de cada mensaje de [`Violation::message`] y de
/// [`CompileError::message`]: 1 KiB. Los mensajes de `jsonschema` copian la
/// instancia o el esquema, así que sin tope crecen con la entrada (SEC-M1,
/// SEC-L1). Un mensaje recortado termina en `…`.
pub const MAX_MESSAGE_BYTES: usize = 1024;

/// Clase de un resultado negativo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// El payload no es JSON bien formado: error léxico o sintáctico, clave
    /// duplicada (D2) o fin de entrada prematuro. Corresponde a `errorKind`
    /// `lexical` y `syntax` del corpus.
    Malformed,
    /// Bien formado, pero viola el esquema (`errorKind` `schema`).
    Schema,
    /// Supera un límite de `docs/00-alcance.md` §4 (`errorKind` `limit`).
    Limit,
    /// La línea base no puede dar un veredicto sobre este payload aunque el
    /// proyecto sí lo haga: hoy, números que no caben en un `f64` (`1e400`,
    /// U9). F4.6 excluye estos casos de la comparación.
    Unsupported,
}

/// Una violación del esquema, tal como la informa `jsonschema`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    /// JSON Pointer (RFC 6901) de la instancia que falla; `""` es la raíz.
    pub pointer: String,
    /// Palabra clave que falla (`required`, `type`…).
    pub keyword: String,
    /// Mensaje de `jsonschema` (en inglés), recortado a
    /// [`MAX_MESSAGE_BYTES`].
    pub message: String,
}

/// Error que detuvo la lectura del payload, con su posición en los bytes de
/// entrada (BOM incluido). Lo llevan los resultados [`Kind::Malformed`],
/// [`Kind::Limit`] (profundidad) y [`Kind::Unsupported`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Malformed {
    /// Línea, desde 1. Saturante: `u32::MAX` es «`u32::MAX` o más» (D9).
    pub line: u32,
    /// Columna en **bytes**, desde 1, del byte en `offset` (el proyecto
    /// cuenta puntos de código; ver `docs/fase6/linea-base.md`). Saturante.
    pub column: u32,
    /// Posición en bytes, desde 0, donde se detecta el error; en un fin de
    /// entrada prematuro, la longitud de la entrada. Es la referencia, como
    /// en el proyecto (D9). `None` solo si no se pudo calcular.
    pub offset: Option<u64>,
    /// Descripción del error, sin la posición.
    pub message: String,
}

/// Resultado de validar un payload, comparable con `expected` del corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// `true` si el payload es bien formado y conforme.
    pub valid: bool,
    /// `None` si `valid`; si no, la clase del error.
    pub kind: Option<Kind>,
    /// Errores de `jsonschema` en su orden (solo con [`Kind::Schema`]), como
    /// mucho [`Limits::max_violations`].
    pub violations: Vec<Violation>,
    /// `true` si había más violaciones que [`Limits::max_violations`] y la
    /// lista se cortó (aditivo a U2; SEC-M1, SEC-M2). Con `truncated`,
    /// [`violation_count`](Self::violation_count) es una cota inferior.
    pub truncated: bool,
    /// El error de lectura (con [`Kind::Malformed`], [`Kind::Limit`] y
    /// [`Kind::Unsupported`]).
    pub malformed: Option<Malformed>,
}

impl Outcome {
    /// Número de violaciones: el de errores de `iter_errors` de
    /// `jsonschema` (U7), hasta [`Limits::max_violations`]. No siempre
    /// coincide con el peso del DPDA (`docs/fase6/linea-base.md`, tabla de
    /// divergencias).
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }

    pub(crate) fn conforme() -> Self {
        Outcome {
            valid: true,
            kind: None,
            violations: Vec::new(),
            truncated: false,
            malformed: None,
        }
    }
}

/// Límites de la línea base (`docs/00-alcance.md` §4).
///
/// Para que un campo nuevo no rompa al llamador, se construye a partir de
/// los valores por defecto: `Limits { max_depth: 16, ..Limits::default() }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Contenedores abiertos a la vez, contando la raíz: `[` y `{` en JSON,
    /// elementos en XML (CR-S8). Con 256, 256 `[` (o 256 elementos) anidados
    /// se aceptan y el 257 es [`Kind::Limit`]. Se recorta a
    /// [`MAX_DEPTH_LIMIT`].
    pub max_depth: u32,
    /// Violaciones que se guardan en [`Outcome::violations`]: 100 por
    /// defecto; con más, la lista se corta y [`Outcome::truncated`] es
    /// `true`. Acota la lista y sus mensajes, no el trabajo de `jsonschema`
    /// (`docs/fase6/linea-base.md` §3.6). `0` se trata como `1`, para que un
    /// resultado de [`Kind::Schema`] siempre lleve al menos una violación.
    pub max_violations: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_depth: MAX_DEPTH_LIMIT,
            max_violations: 100,
        }
    }
}

impl Limits {
    /// La profundidad que se aplica de verdad: `max_depth` recortada.
    pub fn effective_max_depth(&self) -> u32 {
        self.max_depth.min(MAX_DEPTH_LIMIT)
    }

    /// Las violaciones que se guardan de verdad: `max_violations`, al menos 1.
    pub fn effective_max_violations(&self) -> usize {
        usize::try_from(self.max_violations.max(1)).unwrap_or(usize::MAX)
    }
}

/// Testigo de que el código corre en un hilo con [`STACK_SIZE`] de pila.
/// Solo lo crea [`with_stack`], y no se puede pasar a otro hilo (no es
/// `Send` ni `Sync`): el tipo hace cumplir D7 sin crear un hilo por llamada.
pub struct Stack {
    _solo_este_hilo: PhantomData<*const ()>,
}

impl fmt::Debug for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Stack")
    }
}

/// Ejecuta `f` en un hilo nuevo con [`STACK_SIZE`] de pila y devuelve su
/// resultado. Es la única forma de obtener un [`Stack`]. Dentro de `f` se
/// pueden validar tantos payloads como se quiera.
///
/// # Panics
///
/// Si el sistema no puede crear el hilo, o si `f` hace pánico (el pánico se
/// propaga tal cual al llamador).
pub fn with_stack<R: Send>(f: impl FnOnce(&Stack) -> R + Send) -> R {
    std::thread::scope(|s| {
        let hilo = std::thread::Builder::new()
            .name("baseline".into())
            .stack_size(STACK_SIZE)
            .spawn_scoped(s, move || {
                f(&Stack {
                    _solo_este_hilo: PhantomData,
                })
            })
            .expect("baseline: no se pudo crear el hilo con STACK_SIZE de pila");
        match hilo.join() {
            Ok(r) => r,
            Err(panico) => std::panic::resume_unwind(panico),
        }
    })
}

/// El esquema no se pudo compilar: texto mal formado, palabra clave
/// inválida según el meta-esquema de 2020-12, `pattern` que el motor lineal
/// no admite, `$ref` que no se resuelve dentro del documento…
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    /// Descripción del error, recortada a [`MAX_MESSAGE_BYTES`].
    pub message: String,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "el esquema no compila en la línea base: {}",
            self.message
        )
    }
}

impl std::error::Error for CompileError {}

/// Por qué [`run_case`] no da un resultado para un caso.
///
/// `#[non_exhaustive]` (CR-S2): F6.7b y E1 pueden añadir motivos, así que un
/// `match` fuera del crate necesita un brazo `_`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unavailable {
    /// El payload lo produce un generador (`generator`), aún sin implementar;
    /// lleva su nombre.
    Generator(String),
    /// Caso XML sin la feature `libxml2` (F6.7b).
    XmlDisabled,
    /// El caso declara `limits`, cuyo formato aún no está fijado
    /// (`docs/fase6/pendientes.md`).
    CaseLimits,
    /// El esquema del caso no compila en la línea base.
    Compile(CompileError),
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unavailable::Generator(g) => {
                write!(f, "lo produce el generador `{g}`, aún sin implementar")
            }
            Unavailable::XmlDisabled => {
                f.write_str("caso XML: la línea base XML necesita la feature `libxml2`")
            }
            Unavailable::CaseLimits => {
                f.write_str("el caso declara `limits`, cuyo formato aún no está fijado")
            }
            Unavailable::Compile(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Unavailable {}
