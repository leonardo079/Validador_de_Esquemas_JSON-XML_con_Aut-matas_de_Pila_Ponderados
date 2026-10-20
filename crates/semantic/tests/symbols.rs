//! Tabla de símbolos y resolución de `$ref` (Fase 3, segunda tarea): ids
//! estables en preorden, punteros JSON, `$defs`, anclas, escapes de la
//! RFC 6901 y de la RFC 3986, recursión y profundidad (D7).

use std::path::Path;

use proptest::prelude::*;
use schema_parser::{parse, Keyword, Kw, Pos, Schema, SchemaKind, Span, Str, Value};
use semantic::{analyze, RefProblem, SchemaId, SemanticError, SymbolTable};

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

/// Etapa que prueban estas pruebas: meta-esquema y tabla de símbolos, sin la
/// detección de ciclos. Varias usan a propósito referencias circulares no
/// productivas (A → B → A, `#` desde un `not`), que la resolución debe
/// resolver sin bucle aunque después `analyze` las rechace en la etapa de
/// ciclos (`tests/cycles.rs` lo comprueba con los mismos esquemas).
fn resolved(s: &Schema) -> Result<SymbolTable<'_>, Vec<SemanticError>> {
    semantic::check_meta_schema(s)?;
    SymbolTable::build(s)
}

fn parsed(src: &str) -> Schema {
    parse(src.as_bytes()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// Destinos de los `$ref` del esquema, como punteros, en orden de documento.
fn targets(src: &str) -> Vec<String> {
    let s = parsed(src);
    let t = resolved(&s).unwrap_or_else(|e| panic!("{src}: {}", e[0]));
    t.refs().iter().map(|r| t.pointer(r.target)).collect()
}

fn errs(src: &str) -> Vec<SemanticError> {
    let s = parsed(src);
    resolved(&s).map(|_| ()).expect_err(src)
}

#[test]
fn ids_en_preorden_de_documento() {
    let src = r#"{
        "properties": { "b": { "items": true }, "a": false },
        "$defs": { "d": { "not": {} } },
        "allOf": [{}, { "prefixItems": [true, false] }]
    }"#;
    let s = parsed(src);
    let t = resolved(&s).unwrap();
    let ptrs: Vec<String> = t.ids().map(|id| t.pointer(id)).collect();
    assert_eq!(
        ptrs,
        [
            "",
            "/properties/b",
            "/properties/b/items",
            "/properties/a",
            "/$defs/d",
            "/$defs/d/not",
            "/allOf/0",
            "/allOf/1",
            "/allOf/1/prefixItems/0",
            "/allOf/1/prefixItems/1",
        ]
    );
    // El preorden es el orden del primer byte de cada subesquema.
    assert!(t
        .nodes()
        .windows(2)
        .all(|w| w[0].schema.span.start.offset < w[1].schema.span.start.offset));
    assert_eq!(t.node(SchemaId::ROOT).parent, None);
    assert!(t.ids().enumerate().all(|(i, id)| id.index() == i));
    // Cada padre tiene un id menor que su hijo.
    for id in t.ids() {
        if let Some((p, _)) = t.node(id).parent {
            assert!(p < id);
        }
    }
    assert!(!t.is_empty());
}

/// El fragmento recortado de un nodo bajo una clave enorme no depende de
/// la longitud de la clave.
#[test]
fn fragmento_recortado_con_clave_larga() {
    let key = format!("{}ñ/~", "k".repeat(100_000));
    let src = format!(
        r#"{{"properties": {{{}: {{"items": true}}}}}}"#,
        serde_json::to_string(&key).unwrap()
    );
    let s = parsed(&src);
    let t = resolved(&s).unwrap();
    let items = t.id(2).unwrap();
    // La clave escapada: 10⁵ `k`, `ñ` (2 bytes), `~1` y `~0`.
    assert_eq!(
        t.pointer(items).len(),
        "/properties/".len() + 100_006 + "/items".len()
    );
    let c = t.fragment_clipped(items, 80);
    assert!(c.len() <= 80, "{c}");
    assert!(c.starts_with("#/properties/kkk"), "{c}");
    assert!(c.ends_with("k%C3%B1~1~0/items"), "{c}");
}

#[test]
fn defs_con_su_puntero() {
    let src = r#"{
        "$defs": { "a": true, "b/c": { "$defs": { "d": {} } } },
        "definitions": { "e": false },
        "properties": { "p": { "$defs": { "f": {} } } }
    }"#;
    let s = parsed(src);
    let t = resolved(&s).unwrap();
    let defs: Vec<String> = t.defs().iter().map(|&d| t.pointer(d)).collect();
    assert_eq!(
        defs,
        [
            "/$defs/a",
            "/$defs/b~1c",
            "/$defs/b~1c/$defs/d",
            "/definitions/e",
            "/properties/p/$defs/f",
        ]
    );
    let frags: Vec<String> = t.defs().iter().map(|&d| t.fragment(d)).collect();
    assert_eq!(frags[1], "#/$defs/b~1c");
}

/// El caso `valid-recursive-tree-01` (árbol recursivo) resuelve: la raíz y
/// `items` apuntan a `/$defs/node`, y el ciclo no provoca un bucle.
#[test]
fn arbol_recursivo_del_caso() {
    let path = Path::new(TESTS_DIR).join("cases/valid/valid-recursive-tree-01.json");
    let case: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let text = serde_json::to_string_pretty(&case["schema"]).unwrap();
    assert_eq!(
        targets(&text),
        ["/$defs/node", "/$defs/node"],
        "orden: el $ref de items va dentro de $defs, antes que el de la raíz"
    );
    let s = parsed(&text);
    let t = resolved(&s).unwrap();
    let from: Vec<String> = t.refs().iter().map(|r| t.pointer(r.from)).collect();
    assert_eq!(from, ["/$defs/node/properties/children/items", ""]);
    for r in t.refs() {
        assert_eq!(t.node(r.from).ref_target, Some(r.target));
    }
}

#[test]
fn referencias_circulares_y_a_la_raiz() {
    // A → B → A, B → B, la raíz a sí misma con "#" y con "".
    let src = r##"{
        "$defs": {
            "a": { "$ref": "#/$defs/b" },
            "b": { "$ref": "#/$defs/a", "not": { "$ref": "#/$defs/b" } },
            "r1": { "$ref": "#" },
            "r2": { "$ref": "" }
        }
    }"##;
    assert_eq!(targets(src), ["/$defs/b", "/$defs/a", "/$defs/b", "", ""]);
}

#[test]
fn todos_los_lugares_con_subesquemas() {
    let src = r##"{
        "items": true,
        "additionalProperties": { "type": "string" },
        "not": false,
        "dependentSchemas": { "x": {} },
        "prefixItems": [true, { "items": { "$defs": { "z": {} } } }],
        "allOf": [{}], "anyOf": [{}, {}], "oneOf": [{}],
        "definitions": { "d": {} },
        "properties": { "p": {} },
        "$defs": { "refs": { "allOf": [
            { "$ref": "#/items" },
            { "$ref": "#/additionalProperties" },
            { "$ref": "#/not" },
            { "$ref": "#/dependentSchemas/x" },
            { "$ref": "#/prefixItems/1/items/$defs/z" },
            { "$ref": "#/allOf/0" },
            { "$ref": "#/anyOf/1" },
            { "$ref": "#/oneOf/0" },
            { "$ref": "#/definitions/d" },
            { "$ref": "#/properties/p" },
            { "$ref": "#/$defs/refs/allOf/0" }
        ] } }
    }"##;
    assert_eq!(
        targets(src),
        [
            "/items",
            "/additionalProperties",
            "/not",
            "/dependentSchemas/x",
            "/prefixItems/1/items/$defs/z",
            "/allOf/0",
            "/anyOf/1",
            "/oneOf/0",
            "/definitions/d",
            "/properties/p",
            "/$defs/refs/allOf/0",
        ]
    );
}

/// Escapes de la RFC 6901 (`~0`, `~1`) y de la RFC 3986 (`%XX`).
#[test]
fn escapes_de_puntero_y_de_uri() {
    let src = r##"{
        "properties": {
            "a/b": {}, "m~n": {}, "~1": {}, "a b": {}, "ñ": {}, "%": {}, "": {}
        },
        "$defs": { "r": { "anyOf": [
            { "$ref": "#/properties/a~1b" },
            { "$ref": "#/properties/m~0n" },
            { "$ref": "#/properties/~01" },
            { "$ref": "#/properties/a%20b" },
            { "$ref": "#/properties/%C3%B1" },
            { "$ref": "#/properties/ñ" },
            { "$ref": "#/properties/%25" },
            { "$ref": "#%2Fproperties%2Fa~1b" },
            { "$ref": "#/properties/a%7E1b" },
            { "$ref": "#/properties/" }
        ] } }
    }"##;
    assert_eq!(
        targets(src),
        [
            "/properties/a~1b",
            "/properties/m~0n",
            "/properties/~01",
            "/properties/a b",
            "/properties/ñ",
            "/properties/ñ",
            "/properties/%",
            "/properties/a~1b",
            "/properties/a~1b",
            "/properties/",
        ]
    );
}

#[test]
fn anclas() {
    let src = r##"{
        "$anchor": "raiz",
        "not": { "$anchor": "n" },
        "$defs": { "a": { "allOf": [{ "$anchor": "hondo_1.x-y" }] } },
        "anyOf": [
            { "$ref": "#raiz" }, { "$ref": "#n" }, { "$ref": "#hondo_1.x-y" },
            { "$ref": "#%6E" }
        ]
    }"##;
    assert_eq!(targets(src), ["", "/not", "/$defs/a/allOf/0", "/not"]);
    let s = parsed(src);
    let t = resolved(&s).unwrap();
    assert_eq!(t.anchor("raiz"), Some(SchemaId::ROOT));
    assert_eq!(t.anchor("nada"), None);
    // Un ancla dentro de un literal no se registra.
    assert!(matches!(
        errs(r##"{"const": {"$anchor": "a"}, "$ref": "#a"}"##)[0].kind,
        semantic::ErrorKind::UnresolvedRef {
            problem: RefProblem::UnknownAnchor,
            ..
        }
    ));
}

#[test]
fn errores_de_resolucion() {
    let casos: [(&str, RefProblem); 7] = [
        (
            r##"{"$ref": "#/$defs/x"}"##,
            RefProblem::NotFound {
                segment: "$defs".into(),
                index: 1,
            },
        ),
        (
            r##"{"properties": {"a": {}}, "$ref": "#/properties/b"}"##,
            RefProblem::NotFound {
                segment: "b".into(),
                index: 2,
            },
        ),
        (
            r##"{"allOf": [{}], "$ref": "#/allOf/-"}"##,
            RefProblem::NotFound {
                segment: "-".into(),
                index: 2,
            },
        ),
        (
            r##"{"items": {}, "$ref": "#/items/0"}"##,
            RefProblem::NotFound {
                segment: "0".into(),
                index: 2,
            },
        ),
        (
            r##"{"required": ["a"], "$ref": "#/required/0"}"##,
            RefProblem::NotASchema {
                keyword: "required".into(),
            },
        ),
        (
            r##"{"examples": [{}], "$ref": "#/examples/0"}"##,
            RefProblem::IntoLiteral {
                keyword: "examples".into(),
            },
        ),
        (
            r##"{"allOf": [{}], "$ref": "#/allOf"}"##,
            RefProblem::NotASchema {
                keyword: "allOf".into(),
            },
        ),
    ];
    for (src, problem) in casos {
        let e = errs(src);
        assert_eq!(e.len(), 1, "{src}");
        assert_eq!(e[0].code(), "reference");
        match &e[0].kind {
            semantic::ErrorKind::UnresolvedRef { problem: p, .. } => {
                assert_eq!(p, &problem, "{src}")
            }
            k => panic!("{src}: {k:?}"),
        }
        // Se señala el valor del `$ref`.
        let col = src.find("\"#").unwrap() + 1;
        assert_eq!(e[0].column() as usize, col, "{src}");
        assert!(!e[0].to_string().contains('\n'));
    }
}

/// Si el meta-esquema falla, `analyze` no resuelve: un `$ref` remoto tiene
/// un solo error, el del meta-esquema.
#[test]
fn analyze_se_detiene_en_el_meta_esquema() {
    let s = parsed(r#"{"$ref": "otro.json#/a", "if": {}}"#);
    let e = analyze(&s).map(|_| ()).unwrap_err();
    let codes: Vec<&str> = e.iter().map(SemanticError::code).collect();
    assert_eq!(codes, ["unsupported", "unsupported"]);
    // `build` por sí solo tampoco falla con un `$ref` no local.
    let s = parsed(r#"{"$ref": "otro.json"}"#);
    let e = SymbolTable::build(&s).unwrap_err();
    assert!(matches!(
        e[0].kind,
        semantic::ErrorKind::UnresolvedRef {
            problem: RefProblem::NotLocal,
            ..
        }
    ));
}

#[test]
fn ids_estables() {
    let text =
        std::fs::read(Path::new(TESTS_DIR).join("schemas/parse-ok/limites-meta-esquema.json"))
            .unwrap();
    let a = parse(&text).unwrap();
    let b = parse(&text).unwrap();
    let ta = resolved(&a).unwrap();
    let tb = resolved(&b).unwrap();
    let ids = |t: &SymbolTable<'_>| -> Vec<(usize, usize, String)> {
        t.refs()
            .iter()
            .map(|r| (r.from.index(), r.target.index(), t.pointer(r.target)))
            .collect()
    };
    assert_eq!(ids(&ta), ids(&tb));
    assert_eq!(ta.len(), tb.len());
    assert_eq!(ta.anchor("ancla"), tb.anchor("ancla"));
}

// ---------------------------------------------------------- profundidad ---

/// D7: a la profundidad máxima del parser, con un `$ref` en el fondo que
/// apunta a la raíz por un puntero de 255 segmentos, en un hilo de 2 MiB.
#[test]
fn profundidad_maxima() {
    let d = schema_parser::MAX_DEPTH_LIMIT as usize - 1;
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let ptr = "/not".repeat(d);
            let src = format!(
                "{}{{\"$ref\": \"#{ptr}\"}}{}",
                r#"{"not":"#.repeat(d),
                "}".repeat(d)
            );
            let s = parsed(&src);
            let t = resolved(&s).unwrap();
            assert_eq!(t.len(), d + 1);
            assert_eq!(t.refs()[0].target.index(), d);
            assert_eq!(t.pointer(t.refs()[0].target), ptr);
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Ni la construcción ni la resolución son recursivas: un AST de 2·10⁵
/// niveles construido a mano cabe en un hilo de 256 KiB.
#[test]
fn ast_muy_profundo_no_desborda() {
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            const N: usize = 200_000;
            let span = Span {
                start: Pos::START,
                end: 0,
            };
            let name = |n: &str| Str {
                value: n.into(),
                span,
            };
            let reference = format!("#{}", "/not".repeat(N));
            let mut s = Schema {
                kind: SchemaKind::Object(vec![Keyword {
                    name: name("$ref"),
                    kw: Kw::Ref,
                    value: Value::Str(name(&reference)),
                    value_span: span,
                }]),
                span,
            };
            for _ in 0..N {
                s = Schema {
                    kind: SchemaKind::Object(vec![Keyword {
                        name: name("not"),
                        kw: Kw::Not,
                        value: Value::Schema(Box::new(s)),
                        value_span: span,
                    }]),
                    span,
                };
            }
            {
                let t = SymbolTable::build(&s).unwrap();
                assert_eq!(t.len(), N + 1);
                assert_eq!(t.refs()[0].target.index(), N);
                assert_eq!(t.pointer(t.refs()[0].target).len(), 4 * N);
            }
            // Destruir el AST es recursivo (lo genera el compilador) y sí
            // desbordaría esta pila; se libera al terminar el proceso.
            std::mem::forget(s);
        })
        .unwrap()
        .join()
        .unwrap();
}

// ------------------------------------------------------------ propiedades ---

/// Esquema aleatorio sin `$ref` ni `$anchor`, con nombres que necesitan
/// escapes (`~`, `/`, `%`, espacio, no ASCII).
fn esquema(depth: u32) -> BoxedStrategy<String> {
    let hoja = prop_oneof![Just("true".to_owned()), Just("{}".to_owned())];
    if depth == 0 {
        return hoja.boxed();
    }
    let sub = esquema(depth - 1);
    let mapa = |kw: &'static str, sub: BoxedStrategy<String>| {
        prop::collection::btree_map("[a~/% ñ0-9]{0,3}", sub, 0..3).prop_map(move |m| {
            let entries: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}: {v}", serde_json::to_string(k).unwrap()))
                .collect();
            format!(r#"{{"{kw}": {{{}}}}}"#, entries.join(", "))
        })
    };
    prop_oneof![
        hoja,
        mapa("properties", sub.clone()),
        mapa("$defs", sub.clone()),
        sub.clone()
            .prop_map(|a| format!(r#"{{"items": {a}, "minItems": 1}}"#)),
        sub.clone().prop_map(|a| format!(r#"{{"not": {a}}}"#)),
        prop::collection::vec(sub, 1..3)
            .prop_map(|v| format!(r#"{{"allOf": [{}]}}"#, v.join(", "))),
    ]
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// `fragment_clipped` es el fragmento entero si cabe y, si no, un
    /// principio y un final suyos separados por `…`, sin pasar de `max`.
    #[test]
    fn fragmento_recortado(s in esquema(3), max in 16usize..48) {
        let base = parsed(&s);
        let t = resolved(&base).unwrap();
        for id in t.ids() {
            let full = t.fragment(id);
            let c = t.fragment_clipped(id, max);
            prop_assert_eq!(t.fragment_clipped(id, usize::MAX), full.clone());
            if full.len() <= max {
                prop_assert_eq!(c, full);
            } else {
                prop_assert!(c.len() <= max, "{} > {}", c.len(), max);
                let (head, tail) = c.split_once('…').unwrap();
                prop_assert!(head.starts_with('#') && full.starts_with(head), "{} / {}", c, full);
                prop_assert!(full.ends_with(tail), "{} / {}", c, full);
            }
        }
    }

    /// Ida y vuelta: el fragmento de cada subesquema, usado como `$ref`,
    /// resuelve a ese mismo subesquema; y los ids están en orden de texto.
    #[test]
    fn fragmento_de_cada_nodo_resuelve_a_el(s in esquema(3)) {
        let base = parsed(&s);
        let t = resolved(&base).unwrap();
        prop_assert!(t.nodes().windows(2).all(|w| w[0].schema.span.start.offset
            < w[1].schema.span.start.offset));
        let refs: Vec<String> = t
            .ids()
            .map(|id| {
                let frag = format!("#/allOf/0{}", &t.fragment(id)[1..]);
                format!(r#"{{"$ref": {}}}"#, serde_json::to_string(&frag).unwrap())
            })
            .collect();
        let doc = format!(r#"{{"allOf": [{s}, {{"anyOf": [{}]}}]}}"#, refs.join(", "));
        let d = parsed(&doc);
        let t2 = resolved(&d).unwrap();
        prop_assert_eq!(t2.refs().len(), t.len());
        for (id, r) in t.ids().zip(t2.refs()) {
            let esperado = format!("/allOf/0{}", t.pointer(id));
            prop_assert_eq!(t2.pointer(r.target), esperado);
        }
    }
}
