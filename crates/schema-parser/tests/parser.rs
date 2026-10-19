//! Pruebas del parser LL(1) de JSON Schema (Fase 2).

use proptest::prelude::*;
use schema_parser::{
    parse, parse_with_max_depth, Class, ErrorKind, Json, JsonKind, Kw, ParseError, Schema,
    SchemaKind, Value, DEFAULT_MAX_DEPTH,
};

fn ok(src: &str) -> Schema {
    parse(src.as_bytes()).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn err(src: &str) -> ParseError {
    match parse(src.as_bytes()) {
        Ok(s) => panic!("{src:?} debía fallar y dio {s:?}"),
        Err(e) => e,
    }
}

/// (línea, columna, clase) del error.
fn where_(src: &str) -> (u32, u32, &'static str) {
    let e = err(src);
    (e.line(), e.column(), e.code())
}

// --------------------------------------------------------------- AST ---

#[test]
fn esquemas_booleanos() {
    assert_eq!(ok("true").kind, SchemaKind::Bool(true));
    assert_eq!(ok(" false ").kind, SchemaKind::Bool(false));
    assert_eq!(ok("{}").kind, SchemaKind::Object(vec![]));
}

#[test]
fn forma_de_cada_clase() {
    let s = ok(r##"{
        "$ref": "#/$defs/a",
        "minimum": -0.10000000000000000000001,
        "uniqueItems": true,
        "not": false,
        "$defs": {"a": true, "b": {}},
        "anyOf": [true, {"type": "null"}],
        "const": [1, "x", null],
        "enum": [{}, []],
        "type": ["string", "null"],
        "required": [],
        "dependentRequired": {"a": ["b", "c"]},
        "if": 3
    }"##);
    let kws = s.keywords();
    let kinds: Vec<Kw> = kws.iter().map(|k| k.kw).collect();
    assert_eq!(
        kinds,
        [
            Kw::Ref,
            Kw::Minimum,
            Kw::UniqueItems,
            Kw::Not,
            Kw::Defs,
            Kw::AnyOf,
            Kw::Const,
            Kw::Enum,
            Kw::Type,
            Kw::Required,
            Kw::DependentRequired,
            Kw::Unsupported,
        ]
    );
    assert!(matches!(&kws[0].value, Value::Str(s) if s.value == "#/$defs/a"));
    // El número se guarda como lexema exacto, sin pasar por f64.
    assert!(matches!(&kws[1].value, Value::Num(n) if n.lexeme == "-0.10000000000000000000001"));
    assert_eq!(kws[2].value, Value::Bool(true));
    assert!(matches!(&kws[3].value, Value::Schema(b) if b.kind == SchemaKind::Bool(false)));
    assert!(matches!(&kws[4].value, Value::SchemaMap(m)
        if m.iter().map(|(k, _)| k.value.as_str()).eq(["a", "b"])));
    assert!(matches!(&kws[5].value, Value::SchemaList(l) if l.len() == 2));
    assert!(
        matches!(&kws[6].value, Value::Json(Json { kind: JsonKind::Array(a), .. }) if a.len() == 3)
    );
    assert!(matches!(&kws[7].value, Value::JsonList(l) if l.len() == 2));
    assert!(matches!(&kws[8].value, Value::Strings(t)
        if t.iter().map(|s| s.value.as_str()).eq(["string", "null"])));
    assert!(matches!(&kws[9].value, Value::Strings(t) if t.is_empty()));
    assert!(matches!(&kws[10].value, Value::StringMap(m) if m[0].1.len() == 2));
    assert!(
        matches!(&kws[11].value, Value::Json(Json { kind: JsonKind::Number(n), .. }) if n == "3")
    );
    assert_eq!(kws[11].name.value, "if");
}

#[test]
fn type_con_un_nombre() {
    let s = ok(r#"{"type": "integer"}"#);
    assert!(matches!(&s.keywords()[0].value, Value::Str(t) if t.value == "integer"));
}

#[test]
fn posiciones_de_claves_y_valores() {
    let src = "{\n  \"properties\": {\n    \"año\": {\"type\": \"string\"}\n  }\n}";
    let s = ok(src);
    let k = &s.keywords()[0];
    assert_eq!((k.name.span.start.line, k.name.span.start.column), (2, 3));
    assert_eq!(
        (k.value_span.start.line, k.value_span.start.column),
        (2, 17)
    );
    // El rango del valor va del `{` al `}` que lo cierra (inclusive).
    let v = &src.as_bytes()[k.value_span.start.offset as usize..k.value_span.end as usize];
    assert!(v.starts_with(b"{") && v.ends_with(b"}"));
    let Value::SchemaMap(m) = &k.value else {
        panic!()
    };
    assert_eq!(m[0].0.value, "año");
    assert_eq!((m[0].0.span.start.line, m[0].0.span.start.column), (3, 5));
    // Columna en puntos de código: `ñ` ocupa dos bytes pero una columna.
    assert_eq!(m[0].1.span.start.column, 12);
    assert_eq!(s.span.start.offset, 0);
    assert_eq!(s.span.end, src.len() as u64);
}

#[test]
fn escapes_decodificados() {
    let s = ok(r#"{"pattern": "^\\d+\u00e9\ud83d\ude00$"}"#);
    assert!(matches!(&s.keywords()[0].value, Value::Str(p) if p.value == "^\\d+é😀$"));
}

#[test]
fn bom_inicial() {
    let mut src = vec![0xEF, 0xBB, 0xBF];
    src.extend_from_slice(b"{\"type\" 1}");
    let e = parse(&src).unwrap_err();
    // El BOM no ocupa columna.
    assert_eq!((e.line(), e.column()), (1, 9));
}

#[test]
fn tabla_de_palabras_clave() {
    for kw in TODAS {
        let name = kw.name().unwrap();
        assert_eq!(Kw::from_name(name), kw, "{name}");
        assert_ne!(kw.class(), Class::Other, "{name}");
    }
    for fuera in [
        "if",
        "then",
        "else",
        "contains",
        "$dynamicRef",
        "Type",
        "",
        "items ",
    ] {
        assert_eq!(Kw::from_name(fuera), Kw::Unsupported, "{fuera}");
    }
    assert_eq!(Kw::Unsupported.name(), None);
    assert_eq!(Kw::Unsupported.class(), Class::Other);
    assert_eq!(Kw::from_name("$schema"), Kw::DollarSchema);
    assert_eq!(Kw::Items.class(), Class::Schema);
    assert_eq!(Kw::Type.class(), Class::Type);
}

// ----------------------------------------------------------- errores ---

#[test]
fn errores_sintacticos_con_posicion() {
    assert_eq!(where_(""), (1, 1, "syntax"));
    assert_eq!(where_("   \n  "), (2, 3, "syntax"));
    assert_eq!(where_("null"), (1, 1, "syntax"));
    assert_eq!(where_("{,}"), (1, 2, "syntax"));
    assert_eq!(where_(r#"{"type": "x",}"#), (1, 14, "syntax"));
    assert_eq!(where_(r#"{"type": "x" "minimum": 1}"#), (1, 14, "syntax"));
    assert_eq!(where_(r#"{"items": [true]}"#), (1, 11, "syntax"));
    assert_eq!(where_(r#"{"allOf": [true,]}"#), (1, 17, "syntax"));
    assert_eq!(where_(r#"{"allOf": {}}"#), (1, 11, "syntax"));
    assert_eq!(where_(r#"{"required": ["a", 1]}"#), (1, 20, "syntax"));
    assert_eq!(where_(r#"{"enum": 1}"#), (1, 10, "syntax"));
    assert_eq!(where_(r#"{"uniqueItems": 1}"#), (1, 17, "syntax"));
    assert_eq!(where_(r#"{"const": [1 2]}"#), (1, 14, "syntax"));
    assert_eq!(where_(r#"{"const": {"a" 1}}"#), (1, 16, "syntax"));
    assert_eq!(where_(r#"{"not": true}}"#), (1, 14, "syntax"));
    assert_eq!(
        where_(r#"{"dependentRequired": {"a": "b"}}"#),
        (1, 29, "syntax")
    );
}

#[test]
fn errores_lexicos_con_posicion() {
    assert_eq!(where_("{\"a\": tru}"), (1, 10, "lexical"));
    assert_eq!(where_("{\"a\": 1.}"), (1, 9, "lexical"));
    // Sustituto sin pareja: el lexer señala el inicio del escape (`\`).
    assert_eq!(where_("{\"a\": \"\\uD800\"}"), (1, 8, "lexical"));
    assert!(matches!(err("{\"a\" @").kind, ErrorKind::Lex(_)));
}

#[test]
fn claves_repetidas() {
    assert_eq!(where_(r#"{"a": 1, "a": 2}"#), (1, 10, "duplicate-key"));
    assert_eq!(
        where_(r#"{"properties": {"x": true, "x": false}}"#),
        (1, 28, "duplicate-key")
    );
    assert_eq!(
        where_(r#"{"dependentRequired": {"x": [], "x": []}}"#),
        (1, 33, "duplicate-key")
    );
    // La misma clave en objetos distintos no es repetida.
    ok(r#"{"properties": {"type": {"type": "string"}}, "type": "object"}"#);
    // Dentro de `required` los valores repetidos son un problema de la Fase 3.
    ok(r#"{"required": ["a", "a"]}"#);
}

#[test]
fn mensajes() {
    let e = err(r#"{"items": [true]}"#);
    assert_eq!(
        e.to_string(),
        "línea 1, columna 11: se esperaba un esquema (objeto `{…}`, `true` o `false`) \
         como valor de \"items\", se encontró `[` (la forma de arreglo de `items` \
         (Draft 2019-09) está fuera del subconjunto; use `prefixItems`)"
    );
    let e = err(r#"{"title": "una cadena de más de treinta y dos caracteres" 1}"#);
    assert!(e.to_string().contains("se encontró el número 1"), "{e}");
    let e = err(r#"{"a": 1 "una cadena de más de treinta y dos caracteres"}"#);
    assert!(
        e.to_string()
            .ends_with("se encontró la cadena \"una cadena de más de treinta y d…\""),
        "{e}"
    );
    let e = err("{\"a\": 01}");
    assert_eq!(
        e.to_string(),
        "línea 1, columna 8: error léxico: número mal formado"
    );
}

/// Todas las palabras clave del subconjunto, para recorrer la tabla LL(1).
const TODAS: [Kw; 42] = [
    Kw::DollarSchema,
    Kw::Id,
    Kw::Anchor,
    Kw::Ref,
    Kw::Comment,
    Kw::Defs,
    Kw::Definitions,
    Kw::Title,
    Kw::Description,
    Kw::Default,
    Kw::Examples,
    Kw::Deprecated,
    Kw::ReadOnly,
    Kw::WriteOnly,
    Kw::Type,
    Kw::Enum,
    Kw::Const,
    Kw::Minimum,
    Kw::Maximum,
    Kw::ExclusiveMinimum,
    Kw::ExclusiveMaximum,
    Kw::MultipleOf,
    Kw::MinLength,
    Kw::MaxLength,
    Kw::Pattern,
    Kw::Format,
    Kw::Items,
    Kw::PrefixItems,
    Kw::MinItems,
    Kw::MaxItems,
    Kw::UniqueItems,
    Kw::Properties,
    Kw::Required,
    Kw::AdditionalProperties,
    Kw::MinProperties,
    Kw::MaxProperties,
    Kw::DependentRequired,
    Kw::DependentSchemas,
    Kw::AllOf,
    Kw::AnyOf,
    Kw::OneOf,
    Kw::Not,
];

/// Fila `keyword → K_* : …` de la tabla LL(1): para cada palabra clave y un
/// valor que empieza con cada terminal que puede iniciar un valor JSON, el
/// parser acepta exactamente cuando la celda (clase, primer token) no está
/// vacía, y si no, falla en la columna del valor.
#[test]
fn cada_fila_k_rechaza_sus_celdas_vacias() {
    // Valor de prueba por terminal: STRING, NUMBER, true, false, null, {, [.
    let valores = ["\"s\"", "1", "true", "false", "null", "{}", "[]"];
    for kw in TODAS.into_iter().chain([Kw::Unsupported]) {
        let name = kw.name().unwrap_or("x-otra");
        let acepta: &[&str] = match kw.class() {
            Class::String => &["\"s\""],
            Class::Number => &["1"],
            Class::Bool => &["true", "false"],
            Class::Schema => &["true", "false", "{}"],
            Class::SchemaMap | Class::DependentRequired => &["{}"],
            Class::SchemaList | Class::Values | Class::Required => &["[]"],
            Class::Type => &["\"s\"", "[]"],
            Class::Value | Class::Other => &valores,
        };
        for v in valores {
            let src = format!("{{\"{name}\": {v}}}");
            match parse(src.as_bytes()) {
                Ok(_) => assert!(acepta.contains(&v), "{src} debía fallar"),
                Err(e) => {
                    assert!(!acepta.contains(&v), "{src}: {e}");
                    // `{"` + nombre + `": ` → el valor empieza en len + 6.
                    let col = name.chars().count() as u32 + 6;
                    assert_eq!(
                        (e.code(), e.line(), e.column()),
                        ("syntax", 1, col),
                        "{src}: {e}"
                    );
                }
            }
        }
    }
}

/// Celdas vacías dentro de los valores compuestos y los casos mínimos del
/// auditor (fila `K_DEPREQ` con `[`, etc.).
#[test]
fn celdas_vacias_dentro_de_los_valores() {
    assert_eq!(where_(r#"{"dependentRequired": []}"#), (1, 23, "syntax"));
    assert_eq!(where_(r#"{"dependentRequired": ["a"]}"#), (1, 23, "syntax"));
    assert_eq!(where_(r#"{"required": {}}"#), (1, 14, "syntax"));
    assert_eq!(where_(r#"{"prefixItems": {}}"#), (1, 17, "syntax"));
    assert_eq!(where_(r#"{"properties": []}"#), (1, 16, "syntax"));
    assert_eq!(where_(r#"{"enum": {}}"#), (1, 10, "syntax"));
    // Elementos de la forma equivocada.
    assert_eq!(
        where_(r#"{"dependentRequired": {"a": {}}}"#),
        (1, 29, "syntax")
    );
    assert_eq!(
        where_(r#"{"dependentRequired": {"a": [1]}}"#),
        (1, 30, "syntax")
    );
    assert_eq!(where_(r#"{"properties": {"a": 1}}"#), (1, 22, "syntax"));
    assert_eq!(where_(r#"{"properties": {1: true}}"#), (1, 17, "syntax"));
    assert_eq!(where_(r#"{"$defs": {"a": []}}"#), (1, 17, "syntax"));
    assert_eq!(where_(r#"{"anyOf": [null]}"#), (1, 12, "syntax"));
    assert_eq!(where_(r#"{"type": [null]}"#), (1, 11, "syntax"));
    assert_eq!(where_(r#"{"type": ["a",]}"#), (1, 15, "syntax"));
    assert_eq!(where_(r#"{"not": "x"}"#), (1, 9, "syntax"));
}

/// La detección de claves repetidas es lineal: 10⁵ claves en un objeto se
/// analizan rápido (con la versión cuadrática anterior, ~18 s en release), y
/// una repetida al final se sigue detectando en su posición.
#[test]
fn muchas_claves_en_tiempo_lineal() {
    const N: usize = 100_000;
    let claves =
        |plantilla: &dyn Fn(usize) -> String| (0..N).map(plantilla).collect::<Vec<_>>().join(",");
    let t = std::time::Instant::now();
    ok(&format!(
        "{{\"properties\": {{{}}}}}",
        claves(&|i| format!("\"p{i}\": true"))
    ));
    ok(&format!(
        "{{\"const\": {{{}}}}}",
        claves(&|i| format!("\"p{i}\": 0"))
    ));
    ok(&format!(
        "{{\"$defs\": {{{}}}}}",
        claves(&|i| format!("\"d{i}\": {{}}"))
    ));
    ok(&format!(
        "{{\"dependentRequired\": {{{}}}}}",
        claves(&|i| format!("\"r{i}\": []"))
    ));
    // Margen amplio para máquinas lentas y compilación sin optimizar; la
    // versión cuadrática tardaba mucho más que esto incluso optimizada.
    let dt = t.elapsed();
    assert!(dt.as_secs() < 10, "demasiado lento: {dt:?}");

    let src = format!(
        "{{\"properties\": {{{}, \"p0\": true}}}}",
        claves(&|i| format!("\"p{i}\": true"))
    );
    let e = err(&src);
    assert_eq!(e.kind, ErrorKind::DuplicateKey("p0".into()));
    assert_eq!(e.at.offset as usize, src.len() - "\"p0\": true}}".len());
}

/// El mensaje ocupa siempre una línea: la clave repetida se escapa y se
/// recorta como en los demás mensajes.
#[test]
fn clave_repetida_escapada() {
    let e = err("{\"a\\nb\": 1, \"a\\nb\": 2}");
    assert_eq!(
        e.to_string(),
        "línea 1, columna 13: clave repetida \"a\\nb\""
    );
    let larga = "x".repeat(40);
    let e = err(&format!("{{\"{larga}\": 1, \"{larga}\": 2}}"));
    assert!(
        e.to_string().ends_with(&format!("\"{}…\"", "x".repeat(32))),
        "{e}"
    );
    let e = err("{\"a\\u0001\" 1}");
    assert!(!e.to_string().contains('\u{1}'), "{e}");
    for e in [err("{\"a\\nb\": 1, \"a\\nb\": 2}"), err("{\"a\\tb\" 1}")] {
        assert!(!e.to_string().contains(['\n', '\t']), "{e}");
    }
}

// ------------------------------------------------------- profundidad ---

fn anidado(n: usize) -> String {
    format!("{}true{}", r#"{"not":"#.repeat(n), "}".repeat(n))
}

#[test]
fn profundidad_maxima() {
    let d = DEFAULT_MAX_DEPTH as usize;
    // A la profundidad máxima el parser cabe en la pila por defecto de un
    // hilo de Rust o de tokio (2 MiB), también sin optimizar.
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            ok(&anidado(d));
            ok(&format!(
                "{}true{}",
                r#"{"properties":{"a":"#.repeat(d / 2),
                "}}".repeat(d / 2)
            ));
            ok(&format!(
                "{{\"const\": {}{}}}",
                "[".repeat(d - 1),
                "]".repeat(d - 1)
            ));
        })
        .unwrap()
        .join()
        .unwrap();
    let e = err(&anidado(d + 1));
    assert_eq!(
        e.kind,
        ErrorKind::TooDeep {
            max: DEFAULT_MAX_DEPTH
        }
    );
    assert_eq!(e.column() as usize, 1 + 7 * d);
    // Los arreglos y los valores JSON también cuentan.
    let arr = format!("{{\"const\": {}{}}}", "[".repeat(d), "]".repeat(d));
    assert_eq!(err(&arr).code(), "limit");
    // Otro límite.
    assert!(parse_with_max_depth(anidado(3).as_bytes(), 3).is_ok());
    assert_eq!(
        parse_with_max_depth(anidado(4).as_bytes(), 3)
            .unwrap_err()
            .code(),
        "limit"
    );
}

/// Una entrada muy anidada es un error controlado, no un desbordamiento de
/// la pila del proceso.
#[test]
fn anidamiento_enorme_no_desborda() {
    let src = "[".repeat(1_000_000);
    let e = err(&format!("{{\"const\": {src}"));
    assert_eq!(e.code(), "limit");
    let src = anidado(200_000);
    assert_eq!(err(&src).code(), "limit");
}

// -------------------------------------------------------- propiedades ---

/// Convierte el valor de `serde_json` a nuestro [`JsonKind`] sin posiciones,
/// para comparar.
fn same(a: &Json, b: &serde_json::Value) -> bool {
    match (&a.kind, b) {
        (JsonKind::Null, serde_json::Value::Null) => true,
        (JsonKind::Bool(x), serde_json::Value::Bool(y)) => x == y,
        (JsonKind::Number(x), serde_json::Value::Number(y)) => *x == y.to_string(),
        (JsonKind::String(x), serde_json::Value::String(y)) => x == y,
        (JsonKind::Array(x), serde_json::Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (JsonKind::Object(x), serde_json::Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((ka, va), (kb, vb))| ka.value == *kb && same(va, vb))
        }
        _ => false,
    }
}

fn json_value() -> impl Strategy<Value = serde_json::Value> {
    let hoja = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(|n| n.into()),
        (-1e9f64..1e9).prop_map(|f| f.into()),
        any::<String>().prop_map(serde_json::Value::String),
    ];
    hoja.prop_recursive(5, 64, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(serde_json::Value::Array),
            prop::collection::btree_map(any::<String>(), inner, 0..6)
                .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

proptest! {
    /// El valor de `const` llega al AST igual que lo entiende `serde_json`,
    /// con el texto compacto y con el indentado.
    #[test]
    fn const_igual_a_serde(v in json_value()) {
        for text in [serde_json::to_string(&v).unwrap(), serde_json::to_string_pretty(&v).unwrap()] {
            let src = format!("{{\"const\": {text}}}");
            let s = parse(src.as_bytes()).unwrap();
            let Value::Json(j) = &s.keywords()[0].value else { panic!() };
            prop_assert!(same(j, &v), "{src}");
        }
    }

    /// Ninguna entrada hace entrar en pánico al parser, y todo error cae
    /// dentro de la entrada con línea y columna válidas.
    #[test]
    fn bytes_arbitrarios_no_fallan(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        check_no_panic(&bytes)?;
    }

    /// Mutaciones de un esquema real: más cerca de la gramática que los
    /// bytes al azar, así que recorren más celdas de la tabla.
    #[test]
    fn mutaciones_no_fallan(pos in any::<prop::sample::Index>(), b in any::<u8>(), op in 0..3u8) {
        let mut src = include_bytes!("../../../tests/schemas/parse-ok/combinadores.json").to_vec();
        let i = pos.index(src.len());
        match op {
            0 => { src.remove(i); }
            1 => src.insert(i, b),
            _ => src[i] = b,
        }
        check_no_panic(&src)?;
    }
}

fn check_no_panic(bytes: &[u8]) -> Result<(), TestCaseError> {
    if let Err(e) = parse(bytes) {
        prop_assert!(e.at.offset <= bytes.len() as u64);
        prop_assert!(e.line() >= 1 && e.column() >= 1);
        prop_assert!(e.to_string().starts_with("línea "));
    }
    Ok(())
}
