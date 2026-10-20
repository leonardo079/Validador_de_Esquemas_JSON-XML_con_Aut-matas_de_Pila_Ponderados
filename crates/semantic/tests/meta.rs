//! Validación contra el meta-esquema restringido (Fase 3, primera tarea):
//! rechazo de las palabras clave fuera del subconjunto con su categoría (D6),
//! comprobaciones de valor, posiciones y profundidad (D7).

use proptest::prelude::*;
use schema_parser::{parse, Kw, Pos, Schema, SchemaKind, Span, Str, Value};
use semantic::{check_meta_schema, Category, ErrorKind, SemanticError};

fn check(src: &str) -> Result<(), Vec<SemanticError>> {
    let schema = parse(src.as_bytes()).unwrap_or_else(|e| panic!("{src}: {e}"));
    check_meta_schema(&schema)
}

fn ok(src: &str) {
    if let Err(errs) = check(src) {
        panic!("{src}: {}", errs[0]);
    }
}

fn errs(src: &str) -> Vec<SemanticError> {
    check(src).expect_err(src)
}

/// Único error del esquema: (código, línea, columna, mensaje).
fn one(src: &str) -> (&'static str, u32, u32, String) {
    let e = errs(src);
    assert_eq!(e.len(), 1, "{src}: {e:?}");
    (e[0].code(), e[0].line(), e[0].column(), e[0].to_string())
}

// ------------------------------------- palabras clave fuera del subconjunto ---

/// Una prueba por palabra clave de `docs/00-alcance.md` §2.8: se rechaza en
/// su nombre (columna 2 de `{"…": …}`) con un mensaje que la nombra, dice su
/// categoría y da un motivo.
macro_rules! fuera_de_alcance {
    ($($test:ident: $name:literal, $value:literal, $cat:expr;)*) => {$(
        #[test]
        fn $test() {
            let src = format!("{{\"{}\": {}}}", $name, $value);
            let e = errs(&src);
            assert_eq!(e.len(), 1, "{src}: {e:?}");
            let ErrorKind::UnsupportedKeyword { name, category, reason, suggestion } = &e[0].kind
            else {
                panic!("{src}: {:?}", e[0]);
            };
            assert_eq!((name.as_str(), *category, *suggestion), ($name, $cat, None));
            assert!(reason.len() > 20, "{src}: motivo demasiado corto");
            assert_eq!((e[0].code(), e[0].line(), e[0].column()), ("unsupported", 1, 2));
            let msg = e[0].to_string();
            let esperado = format!(
                "línea 1, columna 2: palabra clave \"{}\" no soportada ({}): ",
                $name,
                $cat.describe()
            );
            assert!(msg.starts_with(&esperado), "{msg}");
        }
    )*};
}

fuera_de_alcance! {
    fuera_if: "if", "{}", Category::C;
    fuera_then: "then", "{}", Category::C;
    fuera_else: "else", "{}", Category::C;
    fuera_contains: "contains", "{}", Category::C;
    fuera_min_contains: "minContains", "1", Category::B;
    fuera_max_contains: "maxContains", "1", Category::B;
    fuera_pattern_properties: "patternProperties", r#"{"^a": {}}"#, Category::C;
    fuera_property_names: "propertyNames", r#"{"maxLength": 3}"#, Category::C;
    fuera_unevaluated_properties: "unevaluatedProperties", "false", Category::C;
    fuera_unevaluated_items: "unevaluatedItems", "false", Category::C;
    fuera_dynamic_ref: "$dynamicRef", r##""#meta""##, Category::Core;
    fuera_dynamic_anchor: "$dynamicAnchor", r#""meta""#, Category::Core;
    fuera_vocabulary: "$vocabulary",
        r#"{"https://json-schema.org/draft/2020-12/vocab/core": true}"#, Category::Core;
    fuera_content_encoding: "contentEncoding", r#""base64""#, Category::Annotation;
    fuera_content_media_type: "contentMediaType", r#""application/json""#, Category::Annotation;
    fuera_content_schema: "contentSchema", r#"{"type": "object"}"#, Category::Annotation;
    // Borradores anteriores (no están en §2.8, pero tienen motivo propio).
    fuera_additional_items: "additionalItems", "false", Category::Legacy;
    fuera_dependencies: "dependencies", r#"{"a": ["b"]}"#, Category::Legacy;
    fuera_recursive_ref: "$recursiveRef", r##""#""##, Category::Legacy;
    fuera_recursive_anchor: "$recursiveAnchor", "true", Category::Legacy;
}

/// Las formas de §2.8 que no son palabras clave: `$ref` remoto (Fase 3), la
/// forma booleana de `exclusiveMinimum`/`exclusiveMaximum` y la de arreglo de
/// `items` (las rechaza ya el parser, Fase 2), y la validación de `format`
/// (se acepta como anotación y no se valida).
#[test]
fn formas_fuera_de_alcance() {
    let (code, line, col, msg) = one(r#"{"$ref": "otro.json#/$defs/x"}"#);
    assert_eq!((code, line, col), ("unsupported", 1, 10));
    assert!(msg.contains("referencias locales"), "{msg}");
    one(r#"{"$ref": "https://ejemplo.co/esquema.json"}"#);
    // Un URI que, resuelto contra `$id`, nombra este mismo documento tampoco
    // se acepta, pero el mensaje no lo llama remoto: sugiere escribir `#…`.
    let (code, _, _, msg) = one(
        r##"{"$id": "https://e.co/s.json", "$defs": {"a": true}, "$ref": "https://e.co/s.json#/$defs/a"}"##,
    );
    assert_eq!(code, "unsupported");
    assert!(msg.contains("escriba `#…` en su lugar"), "{msg}");
    assert!(!msg.contains("remoto"), "{msg}");

    for src in [
        r#"{"exclusiveMinimum": true}"#,
        r#"{"exclusiveMaximum": false}"#,
        r#"{"items": [{"type": "string"}]}"#,
    ] {
        let e = parse(src.as_bytes()).expect_err(src);
        assert_eq!(e.code(), "syntax", "{src}");
    }

    ok(r#"{"type": "string", "format": "email"}"#);
    ok(r#"{"format": "formato-inventado"}"#);
}

#[test]
fn desconocidas_con_y_sin_sugerencia() {
    let (code, _, _, msg) = one(r#"{"minLenght": 3}"#);
    assert_eq!(code, "unsupported");
    assert!(msg.contains("desconocida"), "{msg}");
    assert!(msg.ends_with("(¿quiso decir \"minLength\"?)"), "{msg}");

    let e = errs(r#"{"x-extension": [1, {"if": null}]}"#);
    assert_eq!(e.len(), 1, "el valor de una desconocida no se recorre");
    assert!(matches!(
        &e[0].kind,
        ErrorKind::UnsupportedKeyword {
            category: Category::Unknown,
            suggestion: None,
            ..
        }
    ));
    // El nombre se escapa y se recorta: el mensaje ocupa una línea.
    let largo = format!("{{\"a\\n{}\": 1}}", "b".repeat(100));
    let msg = errs(&largo)[0].to_string();
    assert!(!msg.contains('\n') && msg.contains("a\\nbbb") && msg.contains('…'));
}

/// Se buscan en todos los lugares donde hay un subesquema, pero no dentro de
/// valores literales (`const`, `enum`, `default`, `examples`) ni en nombres de
/// propiedades.
#[test]
fn se_buscan_en_todos_los_subesquemas() {
    let src = r#"{
        "properties": { "if": { "then": 1 } },
        "$defs": { "d": { "else": 1 } },
        "definitions": { "d": { "contains": 1 } },
        "dependentSchemas": { "x": { "propertyNames": 1 } },
        "items": { "minContains": 1 },
        "additionalProperties": { "maxContains": 1 },
        "not": { "unevaluatedItems": 1 },
        "prefixItems": [{ "unevaluatedProperties": 1 }],
        "allOf": [{ "patternProperties": 1 }],
        "anyOf": [true, { "$dynamicRef": 1 }],
        "oneOf": [{ "$dynamicAnchor": 1 }],
        "const": { "if": 1 },
        "enum": [{ "contains": 1 }],
        "default": { "then": 1 },
        "examples": [{ "else": 1 }]
    }"#;
    let nombres: Vec<String> = errs(src)
        .iter()
        .map(|e| match &e.kind {
            ErrorKind::UnsupportedKeyword { name, .. } => name.clone(),
            k => panic!("{k:?}"),
        })
        .collect();
    assert_eq!(
        nombres,
        [
            "then",
            "else",
            "contains",
            "propertyNames",
            "minContains",
            "maxContains",
            "unevaluatedItems",
            "unevaluatedProperties",
            "patternProperties",
            "$dynamicRef",
            "$dynamicAnchor",
        ]
    );
}

/// Las anotaciones de la lista blanca (`docs/00-alcance.md` §2.1) se aceptan
/// en la raíz y en cualquier subesquema.
#[test]
fn anotaciones_de_la_lista_blanca() {
    let a = r#""$comment": "c", "title": "t", "description": "d", "default": {"if": 1},
               "examples": [1, "x"], "deprecated": true, "readOnly": false, "writeOnly": true,
               "format": "uri""#;
    ok(&format!("{{{a}}}"));
    ok(&format!(
        "{{\"properties\": {{\"p\": {{{a}}}}}, \"items\": {{{a}}}}}"
    ));
}

// ------------------------------------------------------ comprobaciones ---

#[test]
fn enteros_no_negativos() {
    for kw in [
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "minProperties",
        "maxProperties",
    ] {
        for v in [
            "0", "-0", "7", "1.0", "1e2", "1.50e1", "2E+0", "100e-2", "1e400",
        ] {
            ok(&format!("{{\"{kw}\": {v}}}"));
        }
        for v in ["-1", "1.5", "0.1", "-1e2", "25e-1", "1e-400", "-0.5"] {
            let src = format!("{{\"{kw}\": {v}}}");
            let (code, line, col, msg) = one(&src);
            assert_eq!((code, line, col), ("meta-schema", 1, kw.len() as u32 + 6));
            assert!(
                msg.contains(&format!("\"{kw}\"")) && msg.contains("entero no negativo"),
                "{msg}"
            );
        }
    }
}

#[test]
fn multiple_of_positivo() {
    for v in ["1", "0.01", "1e-300", "3.5"] {
        ok(&format!("{{\"multipleOf\": {v}}}"));
    }
    for v in ["0", "-0", "0.0", "0e5", "-1", "-0.5"] {
        let (code, _, col, msg) = one(&format!("{{\"multipleOf\": {v}}}"));
        assert_eq!((code, col), ("meta-schema", 16));
        assert!(msg.contains("mayor que 0"), "{msg}");
    }
    // Los demás números del subconjunto no tienen restricción de rango.
    ok(r#"{"minimum": -1e400, "maximum": 0, "exclusiveMinimum": -0.5, "exclusiveMaximum": 1e9}"#);
}

#[test]
fn nombres_de_type() {
    for t in semantic::TYPE_NAMES {
        ok(&format!("{{\"type\": \"{t}\"}}"));
    }
    ok(r#"{"type": ["null", "boolean", "object", "array", "number", "integer", "string"]}"#);
    for (src, col, frag) in [
        (r#"{"type": "Integer"}"#, 10, "\"Integer\""),
        (r#"{"type": "any"}"#, 10, "\"any\""),
        (r#"{"type": ["string", "int"]}"#, 21, "\"int\""),
        (
            r#"{"type": ["null", "null"]}"#,
            19,
            "se esperaba un arreglo sin nombres de tipo repetidos, se encontró \"null\" repetida",
        ),
        (r#"{"type": []}"#, 10, "`[]`"),
    ] {
        let (code, _, c, msg) = one(src);
        assert_eq!((code, c), ("meta-schema", col), "{src}");
        assert!(msg.contains(frag), "{msg}");
    }
    // Dos errores en el mismo arreglo: nombre inválido y repetido.
    assert_eq!(errs(r#"{"type": ["x", "x"]}"#).len(), 3);
}

#[test]
fn arreglos_sin_repetidos_y_enum_vacio() {
    ok(r#"{"required": [], "dependentRequired": {"a": []}, "enum": [null]}"#);
    ok(r#"{"enum": [1, 1]}"#); // el oficial solo recomienda la unicidad
                               // D11: `enum` vacío se acepta, como en el oficial (`enum: {type: array,
                               // items: true}`, sin `minItems`). No acepta ningún valor, igual que `false`.
    ok(r#"{"enum": []}"#);
    ok(r#"{"properties": {"p": {"enum": []}}}"#);
    let e = errs(r#"{"required": ["a", "b", "a", "b", "a"]}"#);
    let cols: Vec<u32> = e.iter().map(SemanticError::column).collect();
    assert_eq!(cols, [25, 30, 35]);
    assert!(e[0].to_string().ends_with(
        "valor inválido para \"required\" según el meta-esquema: se esperaba un arreglo de \
         cadenas sin repetir, se encontró \"a\" repetida"
    ));
    let (_, _, col, msg) = one(r#"{"dependentRequired": {"a": ["b"], "c": ["d", "d"]}}"#);
    assert_eq!(col, 47);
    assert!(
        msg.contains("\"dependentRequired\"") && msg.contains("se esperaba un arreglo de cadenas"),
        "{msg}"
    );
    // `minItems: 1` de `schemaArray` en el oficial.
    for kw in ["prefixItems", "allOf", "anyOf", "oneOf"] {
        let (code, _, col, _) = one(&format!("{{\"{kw}\": []}}"));
        assert_eq!((code, col), ("meta-schema", kw.len() as u32 + 6), "{kw}");
    }
}

#[test]
fn nucleo_schema_id_anchor_ref() {
    ok(r#"{"$schema": "https://json-schema.org/draft/2020-12/schema"}"#);
    // Fragmento vacío: el mismo recurso (RFC 3986), la forma copiada de Draft 7.
    ok(r##"{"$schema": "https://json-schema.org/draft/2020-12/schema#"}"##);
    ok(r##"{"$id": "https://ejemplo.co/a.json#"}"##);
    ok(r#"{"$id": "urn:ejemplo:a"}"#);
    for a in ["a", "_", "A-b.c_9", "nodo"] {
        ok(&format!(r#"{{"$anchor": "{a}"}}"#));
    }
    // `""` es el mismo documento (RFC 3986 §4.4), equivalente a `#` (D11).
    for r in ["", "#", "#/$defs/x", "#ancla", "#/properties/a~1b"] {
        ok(&format!(r#"{{"$ref": "{r}"}}"#));
    }

    for (src, code, col, frag) in [
        (
            r##"{"$schema": "http://json-schema.org/draft-07/schema#"}"##,
            "unsupported",
            13,
            "dialecto",
        ),
        (
            r##"{"$schema": "https://json-schema.org/draft/2020-12/schema##"}"##,
            "unsupported",
            13,
            "dialecto",
        ),
        (
            r##"{"$schema": "https://json-schema.org/draft/2020-12/schema#x"}"##,
            "unsupported",
            13,
            "con o sin `#` final",
        ),
        (
            r#"{"not": {"$schema": "https://json-schema.org/draft/2020-12/schema"}}"#,
            "unsupported",
            10,
            "en un subesquema",
        ),
        (
            r#"{"items": {"$id": "b.json"}}"#,
            "unsupported",
            12,
            "solo se admite en la raíz",
        ),
        (
            r##"{"$id": "a.json#x"}"##,
            "meta-schema",
            9,
            "sin fragmento",
        ),
        (r#"{"$anchor": ""}"#, "meta-schema", 13, "letra ASCII"),
        (r#"{"$anchor": "a b"}"#, "meta-schema", 13, "\"a b\""),
        (r#"{"$anchor": "ñandú"}"#, "meta-schema", 13, "ñandú"),
        (
            r#"{"$ref": "a.json"}"#,
            "unsupported",
            10,
            "referencias locales",
        ),
    ] {
        let (c, _, column, msg) = one(src);
        assert_eq!((c, column), (code, col), "{src}: {msg}");
        assert!(msg.contains(frag), "{msg}");
    }
}

/// Todos los errores, ordenados por posición aunque el recorrido visite los
/// subesquemas después que las palabras clave de su padre.
#[test]
fn todos_los_errores_en_orden() {
    let src = "{\n  \"properties\": { \"a\": { \"if\": true } },\n  \"minLength\": -1,\n  \"items\": { \"type\": \"x\" },\n  \"contains\": {}\n}";
    let e = errs(src);
    let pos: Vec<(u32, u32, &str)> = e.iter().map(|e| (e.line(), e.column(), e.code())).collect();
    assert_eq!(
        pos,
        [
            (2, 26, "unsupported"),
            (3, 16, "meta-schema"),
            (4, 22, "meta-schema"),
            (5, 3, "unsupported"),
        ]
    );
    assert!(e.windows(2).all(|w| w[0].at.offset < w[1].at.offset));
}

#[test]
fn esquemas_booleanos_y_vacio() {
    ok("true");
    ok("false");
    ok("{}");
}

// ---------------------------------------------------------- profundidad ---

fn anidado(n: usize) -> String {
    format!("{}{{}}{}", r#"{"not":"#.repeat(n), "}".repeat(n))
}

/// D7: a la profundidad máxima del parser (256) el análisis completo
/// (parser + meta-esquema) cabe en un hilo de 2 MiB, también sin optimizar,
/// y un error en el nivel más profundo se reporta.
#[test]
fn profundidad_maxima() {
    let d = schema_parser::MAX_DEPTH_LIMIT as usize;
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            // `{}` interior ocupa el nivel d.
            ok(&anidado(d - 1));
            ok(&format!(
                "{}true{}",
                r#"{"properties":{"a":"#.repeat(d / 2),
                "}}".repeat(d / 2)
            ));
            let hondo = format!(
                "{}{{\"if\": 1}}{}",
                r#"{"not":"#.repeat(d - 1),
                "}".repeat(d - 1)
            );
            let (code, _, col, _) = one(&hondo);
            assert_eq!((code, col as usize), ("unsupported", 7 * (d - 1) + 2));
        })
        .unwrap()
        .join()
        .unwrap();
}

/// El recorrido es iterativo: ni siquiera un AST mucho más profundo que el
/// que puede producir el parser (construido a mano) usa pila del hilo en
/// proporción a su profundidad.
#[test]
fn ast_muy_profundo_no_desborda() {
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            let span = Span {
                start: Pos::START,
                end: 0,
            };
            let mut s = Schema {
                kind: SchemaKind::Object(vec![kw_unsupported("if", span)]),
                span,
            };
            for _ in 0..200_000 {
                s = Schema {
                    kind: SchemaKind::Object(vec![schema_parser::Keyword {
                        name: Str {
                            value: "not".into(),
                            span,
                        },
                        kw: Kw::Not,
                        value: Value::Schema(Box::new(s)),
                        value_span: span,
                    }]),
                    span,
                };
            }
            let e = check_meta_schema(&s).unwrap_err();
            assert_eq!(e.len(), 1);
            // Destruir el AST es recursivo (lo genera el compilador) y sí
            // desbordaría esta pila; se libera al terminar el proceso.
            std::mem::forget(s);
        })
        .unwrap()
        .join()
        .unwrap();
}

fn kw_unsupported(name: &str, span: Span) -> schema_parser::Keyword {
    schema_parser::Keyword {
        name: Str {
            value: name.into(),
            span,
        },
        kw: Kw::Unsupported,
        value: Value::Json(schema_parser::Json {
            kind: schema_parser::JsonKind::Null,
            span,
        }),
        value_span: span,
    }
}

// ------------------------------------------------------------ propiedades ---

proptest! {
    /// Ninguna clave se ignora en silencio: `{"k": null}` es válido si `k`
    /// es una palabra clave soportada que admite `null` (`const`, `default`),
    /// el parser lo rechaza si es soportada y no lo admite, y si no es
    /// soportada da exactamente un error `unsupported` en su nombre.
    #[test]
    fn ninguna_clave_se_ignora(k in "[$a-zA-Z_-]{0,12}|if|then|const|default|minLength|\\$ref") {
        let src = format!("{{{}: null}}", serde_json::to_string(&k).unwrap());
        let Ok(schema) = parse(src.as_bytes()) else {
            prop_assert!(Kw::from_name(&k) != Kw::Unsupported);
            return Ok(());
        };
        match check_meta_schema(&schema) {
            Ok(()) => prop_assert!(matches!(Kw::from_name(&k), Kw::Const | Kw::Default)),
            Err(e) => {
                prop_assert_eq!(Kw::from_name(&k), Kw::Unsupported);
                prop_assert_eq!(e.len(), 1);
                prop_assert_eq!((e[0].code(), e[0].column()), ("unsupported", 2));
            }
        }
    }

    /// Sobre bytes arbitrarios que el parser acepte, la validación no falla
    /// y devuelve los errores ordenados.
    #[test]
    fn no_falla_y_ordena(bytes in prop::collection::vec(any::<u8>(), 0..128)) {
        if let Ok(s) = parse(&bytes) {
            if let Err(e) = check_meta_schema(&s) {
                prop_assert!(!e.is_empty());
                prop_assert!(e.windows(2).all(|w| w[0].at.offset <= w[1].at.offset));
            }
        }
    }

    /// Lo mismo sobre esquemas con forma correcta generados al azar.
    #[test]
    fn esquemas_generados(src in esquema(3)) {
        let s = parse(src.as_bytes()).unwrap();
        if let Err(e) = check_meta_schema(&s) {
            prop_assert!(e.windows(2).all(|w| w[0].at.offset <= w[1].at.offset));
            for err in &e {
                prop_assert!(err.at.offset < src.len() as u64);
            }
        }
    }
}

/// Esquema aleatorio bien formado, con palabras clave soportadas y no
/// soportadas y valores dentro y fuera de rango.
fn esquema(depth: u32) -> BoxedStrategy<String> {
    let hoja = prop_oneof![
        Just("true".to_owned()),
        Just("{}".to_owned()),
        Just(r#"{"type": "string"}"#.to_owned()),
        Just(r#"{"type": "strin"}"#.to_owned()),
        Just(r#"{"minLength": -1}"#.to_owned()),
        Just(r#"{"multipleOf": 0}"#.to_owned()),
        Just(r#"{"if": {}}"#.to_owned()),
        Just(r#"{"required": ["a", "a"]}"#.to_owned()),
    ];
    if depth == 0 {
        return hoja.boxed();
    }
    let sub = esquema(depth - 1);
    prop_oneof![
        hoja,
        (sub.clone(), sub.clone())
            .prop_map(|(a, b)| format!(r#"{{"properties": {{"x": {a}}}, "items": {b}}}"#)),
        prop::collection::vec(sub.clone(), 0..3)
            .prop_map(|v| format!(r#"{{"allOf": [{}]}}"#, v.join(", "))),
        sub.prop_map(|a| format!(r#"{{"not": {a}, "contains": 1}}"#)),
    ]
    .boxed()
}
