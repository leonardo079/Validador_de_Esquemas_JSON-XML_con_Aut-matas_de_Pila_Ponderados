//! Lectura tipada del corpus de `tests/cases` (formato en `tests/README.md`)
//! y ejecución de un caso con la línea base.
//!
//! No comprueba el formato de los casos: eso lo hace `validar_formato`
//! (`crates/lexer/tests/common/mod.rs`) en cada `cargo test` (B7). Aquí solo
//! se exige lo necesario para leerlos, y un campo mal escrito es un
//! [`CorpusError`]. El texto original del esquema de un caso JSON se recorta
//! del archivo del caso con `schema_parser`, por el rango del valor de
//! `schema`, como en `crates/semantic/tests/common/mod.rs` (D20).

use std::fmt;
use std::path::{Component, Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::Value;

use crate::{with_stack, JsonBaseline, Kind, Outcome, Stack, Unavailable};

/// Carpeta del corpus del repositorio.
pub const CASES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cases");

/// Formato del payload y del esquema de un caso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// JSON Schema y payload JSON.
    Json,
    /// XSD y payload XML.
    Xml,
}

/// `expected.errorKind` del corpus (`tests/README.md`, tabla `expected`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// `lexical`: error del lexer (byte inválido, UTF-8 inválido, `01`…).
    Lexical,
    /// `syntax`: error del parser (falta `:`, coma final, fin prematuro…).
    Syntax,
    /// `schema`: bien formado, pero viola el esquema.
    Schema,
    /// `limit`: supera un límite de `docs/00-alcance.md` §4.
    Limit,
}

impl ErrorKind {
    /// La clase de la línea base que le corresponde: `lexical` y `syntax`
    /// son [`Kind::Malformed`] (la línea base no distingue entre los dos).
    pub fn kind(self) -> Kind {
        match self {
            ErrorKind::Lexical | ErrorKind::Syntax => Kind::Malformed,
            ErrorKind::Schema => Kind::Schema,
            ErrorKind::Limit => Kind::Limit,
        }
    }
}

/// El campo `expected` de un caso.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expected {
    /// `valid`: `true` si el payload es bien formado y conforme.
    pub valid: bool,
    /// `violations`: número de violaciones (peso del recorrido en el
    /// semianillo tropical). Se omite con `errorKind` `lexical` o `syntax`.
    pub violations: Option<u64>,
    /// `errorKind`, solo si `valid` es `false`.
    pub error_kind: Option<ErrorKind>,
    /// Puntero JSON (o XPath simple en XML) del primer error.
    pub pointer: Option<String>,
    /// Posición en bytes donde se detecta el primer error.
    pub offset: Option<u64>,
}

/// Payload de un caso.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    /// Bytes concretos (de `payload`, `payloadBase64` o `payloadFile`).
    Bytes(Vec<u8>),
    /// Lo produce un generador (`generator`).
    Generator {
        /// `generator.name`: qué generador.
        name: String,
        /// `generator.params`, tal cual (`Value::Null` si no tiene).
        params: Value,
    },
}

/// Un caso del corpus.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    /// `_id`.
    pub id: String,
    /// Archivo del caso.
    pub path: PathBuf,
    /// `valid`, `invalid`, `malformed`, `non-dpda`, `pumping` o `extreme`.
    pub category: String,
    /// `format`: JSON o XML.
    pub format: Format,
    /// Texto exacto del esquema: el valor de `schema` recortado del archivo
    /// (JSON), el XSD de la cadena `schema` (XML) o el de `schemaFile`.
    pub schema_text: Vec<u8>,
    /// El esquema como valor de `serde_json` (en XML, la cadena del XSD).
    pub schema_value: Value,
    /// El payload, de cualquiera de sus cuatro formas.
    pub payload: Payload,
    /// El campo `expected`.
    pub expected: Expected,
    /// Solo en `non-dpda`: estrategia de respaldo esperada.
    pub strategy: Option<String>,
    /// El campo `limits`, tal cual (su formato aún no está fijado).
    pub limits: Option<Value>,
}

/// Un caso no se pudo leer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusError {
    /// El archivo (o la carpeta) que falló.
    pub path: PathBuf,
    /// Qué campo o qué lectura falló.
    pub message: String,
}

impl fmt::Display for CorpusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for CorpusError {}

/// Todos los casos de [`CASES_DIR`], ordenados por ruta.
pub fn casos() -> Result<Vec<Case>, CorpusError> {
    casos_en(Path::new(CASES_DIR))
}

/// Todos los casos de `raiz` (una carpeta con las de categoría), ordenados
/// por ruta: cada `.json` del primer nivel de cada subcarpeta. Corre en un
/// hilo de [`with_stack`], porque `schema_parser` es recursivo (D7).
pub fn casos_en(raiz: &Path) -> Result<Vec<Case>, CorpusError> {
    with_stack(|_| {
        let mut out = Vec::new();
        for dir in ordenados(raiz, |p| p.is_dir())? {
            for path in ordenados(&dir, |p| {
                p.is_file() && p.extension().is_some_and(|e| e == "json")
            })? {
                out.push(leer_caso(&path)?);
            }
        }
        Ok(out)
    })
}

fn error(path: &Path, message: impl Into<String>) -> CorpusError {
    CorpusError {
        path: path.to_owned(),
        message: message.into(),
    }
}

fn ordenados(dir: &Path, f: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>, CorpusError> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| error(dir, e.to_string()))? {
        let p = e.map_err(|e| error(dir, e.to_string()))?.path();
        if f(&p) {
            v.push(p);
        }
    }
    v.sort();
    Ok(v)
}

fn leer_caso(path: &Path) -> Result<Case, CorpusError> {
    let err = |m: String| error(path, m);
    let file = std::fs::read(path).map_err(|e| err(e.to_string()))?;
    let v: Value = serde_json::from_slice(&file).map_err(|e| err(e.to_string()))?;
    let texto = |k: &str| -> Result<String, CorpusError> {
        v[k].as_str()
            .map(str::to_owned)
            .ok_or_else(|| err(format!("`{k}` falta o no es una cadena")))
    };
    let dir = path.parent().unwrap_or(Path::new("."));

    let format = match texto("format")?.as_str() {
        "json" => Format::Json,
        "xml" => Format::Xml,
        f => return Err(err(format!("`format` \"{f}\" no es json ni xml"))),
    };

    let (schema_text, schema_value) = match v.get("schemaFile") {
        Some(f) => {
            let f = f
                .as_str()
                .ok_or_else(|| err("`schemaFile` no es una cadena".into()))?;
            let bytes = leer_relativo(dir, f).map_err(err)?;
            let value = match format {
                Format::Json => serde_json::from_slice(&bytes)
                    .map_err(|e| err(format!("`schemaFile` {f}: {e}")))?,
                Format::Xml => Value::String(String::from_utf8_lossy(&bytes).into_owned()),
            };
            (bytes, value)
        }
        None => {
            let value = v
                .get("schema")
                .cloned()
                .ok_or_else(|| err("falta `schema` o `schemaFile`".into()))?;
            let bytes = match (format, &value) {
                (Format::Json, _) => texto_del_esquema(&file).map_err(err)?,
                (Format::Xml, Value::String(s)) => s.clone().into_bytes(),
                (Format::Xml, _) => return Err(err("`schema` xml no es una cadena".into())),
            };
            (bytes, value)
        }
    };

    Ok(Case {
        id: texto("_id")?,
        category: texto("category")?,
        path: path.to_owned(),
        format,
        schema_text,
        schema_value,
        payload: payload(&v, dir).map_err(err)?,
        expected: expected(&v["expected"]).map_err(err)?,
        strategy: v["strategy"].as_str().map(str::to_owned),
        limits: v.get("limits").cloned(),
    })
}

/// Texto exacto del valor del miembro `schema` del archivo del caso: el
/// parser del proyecto conserva el rango del valor de cada miembro.
fn texto_del_esquema(caso: &[u8]) -> Result<Vec<u8>, String> {
    let raiz = schema_parser::parse(caso).map_err(|e| e.to_string())?;
    let kw = raiz
        .keywords()
        .iter()
        .find(|k| k.name.value == "schema")
        .ok_or("el caso no tiene `schema`")?;
    let inicio = usize::try_from(kw.value_span.start.offset).map_err(|e| e.to_string())?;
    let fin = usize::try_from(kw.value_span.end).map_err(|e| e.to_string())?;
    caso.get(inicio..fin)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| "rango de `schema` fuera del archivo".into())
}

/// Lee `rel` dentro de `dir` con las reglas de ruta de `tests/README.md`
/// (B6): relativa y formada solo por nombres, sin `./`, `..` ni raíz. Además
/// rechaza `:` y `\` en cualquier sistema (B22): en Linux `C:/x` o `..\x`
/// serían nombres normales, y en Windows `a:b` abre un flujo NTFS.
fn leer_relativo(dir: &Path, rel: &str) -> Result<Vec<u8>, String> {
    let p = Path::new(rel);
    let solo_nombres = p.components().all(|c| matches!(c, Component::Normal(_)));
    if rel.is_empty() || rel.contains([':', '\\']) || !solo_nombres {
        return Err(format!(
            "\"{rel}\" debe ser una ruta relativa dentro de la carpeta del caso"
        ));
    }
    std::fs::read(dir.join(p)).map_err(|e| format!("{rel}: {e}"))
}

fn payload(v: &Value, dir: &Path) -> Result<Payload, String> {
    const FORMAS: [&str; 4] = ["payload", "payloadBase64", "payloadFile", "generator"];
    let presentes: Vec<&str> = FORMAS.into_iter().filter(|k| v.get(*k).is_some()).collect();
    let [forma] = presentes[..] else {
        return Err(format!(
            "debe tener exactamente uno de {FORMAS:?}, tiene {presentes:?}"
        ));
    };
    let texto = |k: &str| {
        v[k].as_str()
            .ok_or_else(|| format!("`{k}` debe ser una cadena"))
    };
    Ok(match forma {
        "payload" => Payload::Bytes(texto("payload")?.as_bytes().to_vec()),
        "payloadBase64" => Payload::Bytes(
            STANDARD
                .decode(texto("payloadBase64")?)
                .map_err(|e| format!("payloadBase64 inválido: {e}"))?,
        ),
        "payloadFile" => Payload::Bytes(leer_relativo(dir, texto("payloadFile")?)?),
        _ => Payload::Generator {
            name: v["generator"]["name"]
                .as_str()
                .ok_or("generator.name debe ser una cadena")?
                .to_owned(),
            params: v["generator"]["params"].clone(),
        },
    })
}

fn expected(x: &Value) -> Result<Expected, String> {
    let error_kind = match x["errorKind"].as_str() {
        None if x["errorKind"].is_null() => None,
        Some("lexical") => Some(ErrorKind::Lexical),
        Some("syntax") => Some(ErrorKind::Syntax),
        Some("schema") => Some(ErrorKind::Schema),
        Some("limit") => Some(ErrorKind::Limit),
        _ => return Err(format!("`expected.errorKind` {} inválido", x["errorKind"])),
    };
    Ok(Expected {
        valid: x["valid"]
            .as_bool()
            .ok_or("`expected.valid` falta o no es booleano")?,
        violations: x["violations"].as_u64(),
        error_kind,
        pointer: x["pointer"].as_str().map(str::to_owned),
        offset: x["offset"].as_u64(),
    })
}

/// Ejecuta un caso con la línea base de su formato: compila el esquema
/// (desde su texto original) y valida el payload. Los casos JSON van con
/// [`JsonBaseline`]; los XML, con `XmlBaseline` (feature `libxml2`), que se
/// compila en el hilo de `stack`.
///
/// `Err` si la línea base no puede dar un resultado: payload de un generador,
/// caso XML sin la feature `libxml2`, caso con `limits` o esquema que no
/// compila.
pub fn run_case(case: &Case, stack: &Stack) -> Result<Outcome, Unavailable> {
    let bytes = match &case.payload {
        Payload::Bytes(b) => b,
        Payload::Generator { name, .. } => return Err(Unavailable::Generator(name.clone())),
    };
    if case.limits.is_some() {
        return Err(Unavailable::CaseLimits);
    }
    match case.format {
        Format::Json => {
            let base =
                JsonBaseline::compile_text(&case.schema_text).map_err(Unavailable::Compile)?;
            Ok(base.validate(stack, bytes))
        }
        Format::Xml if dtd::formato(&case.schema_text) == dtd::Formato::Dtd => {
            Err(Unavailable::DtdSchema)
        }
        Format::Xml => validar_xml(&case.schema_text, stack, bytes),
    }
}

/// Un caso XML con libxml2 (F6.7b): el XSD se compila dentro del hilo del
/// testigo, porque `XmlBaseline` no es `Send`.
#[cfg(feature = "libxml2")]
fn validar_xml(xsd: &[u8], stack: &Stack, bytes: &[u8]) -> Result<Outcome, Unavailable> {
    let base = crate::XmlBaseline::compile(stack, xsd).map_err(Unavailable::Compile)?;
    Ok(base.validate(stack, bytes))
}

/// Sin la feature `libxml2`, un caso XML no está disponible (CR-S9: es la
/// única comprobación; un caso XML con `limits` da antes `CaseLimits`).
#[cfg(not(feature = "libxml2"))]
fn validar_xml(_xsd: &[u8], _stack: &Stack, _bytes: &[u8]) -> Result<Outcome, Unavailable> {
    Err(Unavailable::XmlDisabled)
}
