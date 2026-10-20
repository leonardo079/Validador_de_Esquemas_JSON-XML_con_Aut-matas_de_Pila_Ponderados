//! Lectura del corpus de casos de `tests/cases` (formato en `tests/README.md`)
//! y comprobación de un payload en bloques.
//!
//! Lo comparten varias pruebas y no todas usan todos los campos.
#![allow(dead_code)]

use std::fmt::Debug;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use lexer::LexError;

/// Payload de un caso.
pub enum Payload {
    /// Bytes concretos (de `payload`, `payloadBase64` o `payloadFile`).
    Bytes(Vec<u8>),
    /// Se produce con un generador (`generator`); el nombre del generador.
    Generator(String),
}

/// Un caso del corpus visto desde el lexer.
pub struct Case {
    pub path: PathBuf,
    /// Bytes del archivo del caso (siempre JSON).
    pub file: Vec<u8>,
    pub format: String,
    pub payload: Payload,
    /// Offset esperado si el caso declara un error léxico.
    pub lexical_offset: Option<u64>,
}

impl Case {
    pub fn name(&self) -> String {
        self.path.display().to_string()
    }
}

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

/// Lee todos los casos de `tests/cases`. Antes de usar un caso comprueba su
/// formato con [`validar_formato`] (nombre = `_id`, carpeta = `category`…) y
/// que `_id` no se repita; cualquier incumplimiento hace fallar la prueba con
/// el archivo y los campos que no cumplen.
pub fn corpus() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut ids = std::collections::BTreeMap::new();
    for category in std::fs::read_dir(format!("{TESTS_DIR}/cases")).unwrap() {
        let dir = category.unwrap().path();
        // Un archivo suelto en tests/cases/ no es una categoría: lo reporta
        // `el_corpus_cumple_el_formato` (tests/corpus.rs) con su nombre.
        if !dir.is_dir() {
            continue;
        }
        let carpeta = nombre(&dir);
        let entradas = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        for entry in entradas {
            let path = entry.unwrap().path();
            // Solo los `.json` del primer nivel son casos; las subcarpetas
            // guardan los archivos que referencian `schemaFile`/`payloadFile`.
            if path.is_dir() || path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let file = std::fs::read(&path).unwrap();
            let v: serde_json::Value =
                serde_json::from_slice(&file).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            let errores = validar_formato(&carpeta, stem.as_deref().unwrap_or_default(), &v);
            assert!(
                errores.is_empty(),
                "{}: no cumple el formato de tests/README.md:\n  - {}{}",
                path.display(),
                errores.join("\n  - "),
                if v.get("_id").is_none() {
                    "\n(si es un archivo referenciado por `schemaFile`/`payloadFile`, \
                     va en una subcarpeta de la categoría)"
                } else {
                    ""
                }
            );
            if let Err(m) = schema_file_existe(&dir, &v) {
                panic!("{}: {m}", path.display());
            }
            if let Some(otro) = ids.insert(v["_id"].to_string(), path.clone()) {
                panic!(
                    "{}: `_id` {} repetido (también en {})",
                    path.display(),
                    v["_id"],
                    otro.display()
                );
            }
            let expected = &v["expected"];
            cases.push(Case {
                format: v["format"].as_str().unwrap_or_default().to_owned(),
                payload: payload(&v, &path),
                lexical_offset: (expected["errorKind"] == "lexical").then(|| {
                    expected["offset"].as_u64().unwrap_or_else(|| {
                        panic!("{}: falta el offset del error léxico", path.display())
                    })
                }),
                path,
                file,
            });
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    cases
}

fn nombre(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Categorías de `tests/README.md`; cada una es también el nombre de su carpeta.
pub const CATEGORIAS: [&str; 6] = [
    "valid",
    "invalid",
    "malformed",
    "non-dpda",
    "pumping",
    "extreme",
];

/// Campos de primer nivel que define `tests/README.md`. Uno desconocido
/// (p. ej. `payloadfile` o `metrics`, que no se versiona) es un error en lugar
/// de ignorarse.
const CAMPOS: [&str; 14] = [
    "_id",
    "category",
    "format",
    "description",
    "phase",
    "schema",
    "schemaFile",
    "payload",
    "payloadFile",
    "payloadBase64",
    "generator",
    "strategy",
    "limits",
    "expected",
];

/// Reglas de `tests/README.md` que se pueden comprobar sin ejecutar el caso,
/// para el archivo `<carpeta>/<stem>.json`. Devuelve un mensaje por regla
/// incumplida, cada uno con el campo que falla (vacío = cumple). La forma del
/// payload la comprueba aparte `payload` al leerlo, y la unicidad de `_id`,
/// [`corpus`].
pub fn validar_formato(carpeta: &str, stem: &str, v: &serde_json::Value) -> Vec<String> {
    let mut e = Vec::new();
    let Some(obj) = v.as_object() else {
        return vec!["el caso debe ser un objeto JSON".into()];
    };
    for k in obj.keys().filter(|k| !CAMPOS.contains(&k.as_str())) {
        e.push(format!("campo desconocido `{k}`"));
    }

    // category: uno de los seis y igual a la carpeta.
    let category = v["category"].as_str();
    match category {
        None => e.push("`category` falta o no es una cadena".into()),
        Some(c) if !CATEGORIAS.contains(&c) => {
            e.push(format!("`category` \"{c}\" no es una de {CATEGORIAS:?}"))
        }
        Some(c) if c != carpeta => e.push(format!(
            "`category` \"{c}\" no coincide con la carpeta \"{carpeta}\""
        )),
        Some(_) => {}
    }

    // _id: igual al nombre del archivo y con la forma <categoría>-<tema>-<NN>.
    match v["_id"].as_str() {
        None => e.push("`_id` falta o no es una cadena".into()),
        Some(id) => {
            if id != stem {
                e.push(format!(
                    "`_id` \"{id}\" no coincide con el nombre del archivo \"{stem}.json\""
                ));
            }
            if let Some(c) = category {
                if !id_bien_formado(id, c) {
                    e.push(format!(
                        "`_id` \"{id}\" no tiene la forma `{c}-<tema>-<NN>` \
                         (minúsculas, dígitos y guiones; NN de al menos dos dígitos)"
                    ));
                }
            }
        }
    }

    let format = v["format"].as_str();
    if !matches!(format, Some("json" | "xml")) {
        e.push(format!(
            "`format` debe ser \"json\" o \"xml\", es {}",
            v["format"]
        ));
    }
    if v["description"]
        .as_str()
        .is_none_or(|d| d.trim().is_empty())
    {
        e.push("`description` falta o está vacía".into());
    }
    if !v["phase"].is_null() && !v["phase"].is_u64() {
        e.push(format!("`phase` debe ser un entero, es {}", v["phase"]));
    }

    for campo in ["schemaFile", "payloadFile"] {
        if let Some(f) = v[campo].as_str() {
            if let Err(m) = ruta_de_archivo(f) {
                e.push(format!("`{campo}` \"{f}\": {m}"));
            }
        }
    }

    // Exactamente uno de schema / schemaFile; JSON Schema como objeto o
    // booleano, XSD como cadena.
    match (v.get("schema"), v.get("schemaFile")) {
        (Some(_), Some(_)) | (None, None) => {
            e.push("debe tener exactamente uno de `schema` / `schemaFile`".into())
        }
        (None, Some(f)) if !f.is_string() => e.push("`schemaFile` debe ser una cadena".into()),
        (Some(s), None) => match format {
            Some("json") if !(s.is_object() || s.is_boolean()) => {
                e.push("`schema` de un caso json debe ser un objeto o un booleano".into())
            }
            Some("xml") if !s.is_string() => {
                e.push("`schema` de un caso xml debe ser una cadena con el XSD".into())
            }
            _ => {}
        },
        _ => {}
    }

    if let Some(st) = v.get("strategy") {
        if category != Some("non-dpda") {
            e.push("`strategy` solo se admite en la categoría non-dpda".into());
        }
        let validas = ["product", "bounded-determinization", "deferred"];
        if !st.as_str().is_some_and(|s| validas.contains(&s)) {
            e.push(format!("`strategy` {st} no es una de {validas:?}"));
        }
    }

    validar_expected(&v["expected"], &mut e);
    e
}

/// Si el caso trae `schemaFile`, el archivo debe existir dentro de `dir` (la
/// carpeta del caso). La forma de la ruta la comprueba [`ruta_de_archivo`].
pub fn schema_file_existe(dir: &Path, v: &serde_json::Value) -> Result<(), String> {
    match v["schemaFile"].as_str() {
        Some(f) if !dir.join(f).is_file() => {
            Err(format!("`schemaFile` {} no existe", dir.join(f).display()))
        }
        _ => Ok(()),
    }
}

/// Ruta de `schemaFile`/`payloadFile`: relativa y dentro de la carpeta del
/// caso, formada solo por nombres (sin `./`, `..` ni rutas absolutas). Si
/// termina en `.json` debe ir en una subcarpeta, porque todo `.json` del
/// primer nivel se lee como un caso.
pub fn ruta_de_archivo(f: &str) -> Result<(), &'static str> {
    let rel = Path::new(f);
    let normales = rel
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)));
    if f.is_empty() || !normales {
        return Err("debe ser una ruta relativa dentro de la carpeta del caso \
                    (sin `..` ni rutas absolutas); tampoco se admite `./` \
                    (escribir `x.xsd`, no `./x.xsd`)");
    }
    if rel.extension().is_some_and(|e| e == "json") && rel.components().count() < 2 {
        return Err("un archivo `.json` referenciado va en una subcarpeta \
                    (p. ej. `archivos/x.json`): en el primer nivel se leería como un caso");
    }
    Ok(())
}

/// `<categoría>-<tema>-<NN>`: el tema no vacío, todo en `[a-z0-9-]` sin guiones
/// dobles ni en los extremos, y NN de al menos dos dígitos.
fn id_bien_formado(id: &str, category: &str) -> bool {
    let Some(resto) = id.strip_prefix(category).and_then(|r| r.strip_prefix('-')) else {
        return false;
    };
    let Some((tema, nn)) = resto.rsplit_once('-') else {
        return false;
    };
    let alfabeto = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    !tema.is_empty()
        && alfabeto(tema)
        && !tema.starts_with('-')
        && !tema.ends_with('-')
        && !tema.contains("--")
        && nn.len() >= 2
        && nn.bytes().all(|b| b.is_ascii_digit())
}

/// Campos de `expected` (tabla de `tests/README.md`). Como en el primer
/// nivel, uno desconocido es un error: una errata (`errrorKind`) borraría la
/// expectativa sin avisar.
const CAMPOS_EXPECTED: [&str; 5] = ["valid", "violations", "errorKind", "pointer", "offset"];

/// Reglas de la tabla `expected` de `tests/README.md`.
fn validar_expected(x: &serde_json::Value, e: &mut Vec<String>) {
    let Some(obj) = x.as_object() else {
        e.push("`expected` falta o no es un objeto".into());
        return;
    };
    for k in obj
        .keys()
        .filter(|k| !CAMPOS_EXPECTED.contains(&k.as_str()))
    {
        e.push(format!("campo desconocido `expected.{k}`"));
    }
    let Some(valid) = x["valid"].as_bool() else {
        e.push("`expected.valid` falta o no es booleano".into());
        return;
    };
    let kind = x["errorKind"].as_str();
    if !x["errorKind"].is_null() {
        if valid {
            e.push("`expected.errorKind` solo se admite si `valid` es false".into());
        }
        if !matches!(kind, Some("lexical" | "syntax" | "schema" | "limit")) {
            e.push(format!(
                "`expected.errorKind` {} no es lexical, syntax, schema ni limit",
                x["errorKind"]
            ));
        }
    }
    if !x["violations"].is_null() {
        match x["violations"].as_u64() {
            None => e.push("`expected.violations` debe ser un entero".into()),
            Some(n) if (n == 0) != valid => e.push(format!(
                "`expected.violations` = {n} con `valid` = {valid} (0 ⇔ valid)"
            )),
            Some(_) => {}
        }
        if matches!(kind, Some("lexical" | "syntax")) {
            e.push(
                "`expected.violations` se omite con errorKind lexical o syntax \
                 (esos errores detienen el análisis)"
                    .into(),
            );
        }
    }
    if !x["offset"].is_null() && !x["offset"].is_u64() {
        e.push("`expected.offset` debe ser un entero no negativo".into());
    }
    if !x["pointer"].is_null() && !x["pointer"].is_string() {
        e.push("`expected.pointer` debe ser una cadena".into());
    }
}

/// Lee el payload. Un caso debe traer exactamente una de las cuatro formas;
/// si no, o si no se puede leer, la prueba falla en lugar de saltarse el caso.
fn payload(v: &serde_json::Value, path: &Path) -> Payload {
    const FORMAS: [&str; 4] = ["payload", "payloadBase64", "payloadFile", "generator"];
    let presentes: Vec<&str> = FORMAS.into_iter().filter(|k| !v[*k].is_null()).collect();
    let [forma] = presentes[..] else {
        panic!(
            "{}: debe tener exactamente uno de {FORMAS:?}, tiene {presentes:?}",
            path.display()
        );
    };
    let texto = |k: &str| {
        v[k].as_str()
            .unwrap_or_else(|| panic!("{}: `{k}` debe ser una cadena", path.display()))
    };
    match forma {
        "payload" => Payload::Bytes(texto("payload").as_bytes().to_vec()),
        "payloadBase64" => Payload::Bytes(
            STANDARD
                .decode(texto("payloadBase64"))
                .unwrap_or_else(|e| panic!("{}: payloadBase64 inválido: {e}", path.display())),
        ),
        "payloadFile" => {
            // Solo archivos versionados junto al caso. Los payloads grandes no
            // se guardan (tests/large/ está fuera de git): se describen con
            // `generator`, y así la CI nunca depende de un archivo ausente.
            let rel = Path::new(texto("payloadFile"));
            if let Err(m) = ruta_de_archivo(texto("payloadFile")) {
                panic!(
                    "{}: payloadFile {m}; para payloads grandes use `generator`",
                    path.display()
                );
            }
            let file = path.parent().unwrap().join(rel);
            Payload::Bytes(std::fs::read(&file).unwrap_or_else(|e| {
                panic!(
                    "{}: no se pudo leer {}: {e}",
                    path.display(),
                    file.display()
                )
            }))
        }
        _ => Payload::Generator(
            v["generator"]["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: generator.name debe ser una cadena", path.display()))
                .to_owned(),
        ),
    }
}

/// Tokeniza `bytes` entero y en bloques de 1, 7 y 4096 bytes con `lex`: los
/// cuatro resultados deben ser idénticos. Si el caso declara un error léxico,
/// debe ocurrir en el offset declarado; si no, no debe haber error léxico.
pub fn por_bloques<T: PartialEq + Debug>(
    bytes: &[u8],
    lexical_offset: Option<u64>,
    what: &str,
    lex: impl Fn(&[u8], usize) -> (Vec<T>, Option<LexError>),
) {
    let whole = lex(bytes, usize::MAX);
    assert_eq!(
        whole.1.map(|e| e.at.offset),
        lexical_offset,
        "{what}: {:?}",
        whole.1
    );
    for size in [1, 7, 4096] {
        assert_eq!(lex(bytes, size), whole, "{what}, bloques de {size}");
    }
}

/// Comprueba el payload de cada caso del formato dado. Los casos con
/// generador se informan y se omiten hasta que existan los generadores.
pub fn corpus_en_bloques<T: PartialEq + Debug>(
    format: &str,
    lex: impl Fn(&[u8], usize) -> (Vec<T>, Option<LexError>) + Copy,
) {
    let cases: Vec<Case> = corpus()
        .into_iter()
        .filter(|c| c.format == format)
        .collect();
    assert!(
        cases.iter().any(|c| matches!(c.payload, Payload::Bytes(_))),
        "el corpus no tiene payloads {format}"
    );
    for case in &cases {
        match &case.payload {
            Payload::Bytes(b) => por_bloques(b, case.lexical_offset, &case.name(), lex),
            Payload::Generator(g) => eprintln!(
                "omitido {}: lo produce el generador `{g}`, aún sin implementar",
                case.name()
            ),
        }
    }
}
