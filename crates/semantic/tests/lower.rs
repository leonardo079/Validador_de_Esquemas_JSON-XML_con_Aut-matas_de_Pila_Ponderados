//! Bajada de JSON Schema a la IR (tarea 4.2, `docs/fase3/bajada.md`): todo
//! el corpus válido baja a una IR que pasa `Ir::verify`, con un nodo por
//! subesquema (`NodeId` = `SchemaId`) que conserva su `Span` y su puntero;
//! un `$ref` es una arista y no una copia, así que la recursión productiva
//! queda como un ciclo y el tamaño es lineal; cada palabra clave baja como
//! dice `docs/fase3/ir.md` §5.1; y la bajada no es recursiva (D7).

use std::path::{Path, PathBuf};

use ir::{JsonType, LitToken, Literal, NodeId, Num, Rule, Sort, Step, TypeSet, VerifiedIr};
use schema_parser::parse;
use semantic::{analyze, lower, SymbolTable};

mod common;

use common::TESTS_DIR;

/// Esquemas de `parse-ok` que no pasan la Fase 3 a propósito; la lista y su
/// justificación están en `tests/corpus.rs` (`EXCEPCIONES`).
const EXCEPCIONES: [&str; 2] = ["combinadores.json", "fuera-del-subconjunto.json"];

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    v.sort();
    v
}

/// Texto de todos los esquemas JSON válidos del corpus, con su nombre.
fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for path in json_files(&Path::new(TESTS_DIR).join("schemas/parse-ok")) {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        if !EXCEPCIONES.contains(&file.as_str()) {
            out.push((file, std::fs::read(&path).unwrap()));
        }
    }
    // Con el texto original del esquema, no reserializado (§18 de
    // `docs/fase3/pendientes.md`).
    for caso in common::casos_json() {
        out.push((caso.path.display().to_string(), caso.schema));
    }
    out
}

/// Esquema → IR verificada; falla la prueba si algo no pasa.
#[track_caller]
fn bajar(src: &str) -> VerifiedIr {
    let schema = parse(src.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
    let table = analyze(&schema).unwrap_or_else(|e| panic!("{}", e[0]));
    lower(&table).unwrap_or_else(|e| panic!("{:?}", e.errors()))
}

fn id(i: usize) -> NodeId {
    NodeId::new(i)
}

/// Reglas del nodo `i`, sin los rangos.
fn reglas(ir: &VerifiedIr, i: usize) -> Vec<Rule> {
    ir.node(id(i))
        .rules
        .iter()
        .map(|c| c.rule.clone())
        .collect()
}

/// El grafo de la IR tiene algún ciclo (DFS iterativo con colores).
fn tiene_ciclo(ir: &VerifiedIr) -> bool {
    let n = ir.nodes().len();
    let succ: Vec<Vec<usize>> = ir
        .nodes()
        .iter()
        .map(|node| {
            let mut t = Vec::new();
            for c in &node.rules {
                c.rule.targets(&mut t);
            }
            t.iter().map(|t| t.index()).collect()
        })
        .collect();
    let mut color = vec![0u8; n];
    for s in 0..n {
        if color[s] != 0 {
            continue;
        }
        let mut stack = vec![(s, 0)];
        color[s] = 1;
        while let Some(&mut (v, ref mut k)) = stack.last_mut() {
            if let Some(&w) = succ[v].get(*k) {
                *k += 1;
                match color[w] {
                    1 => return true,
                    0 => {
                        color[w] = 1;
                        stack.push((w, 0));
                    }
                    _ => {}
                }
            } else {
                color[v] = 2;
                stack.pop();
            }
        }
    }
    false
}

/// Criterio 1: todo el corpus válido baja a una IR que pasa `verify`, con un
/// nodo por subesquema (`NodeId` = `SchemaId`), y cada nodo conserva el
/// `Span` del subesquema, su padre y su puntero.
#[test]
fn corpus_baja_a_ir_verificada() {
    let corpus = corpus();
    assert!(corpus.len() >= 11, "corpus demasiado pequeño");
    for (nombre, texto) in &corpus {
        let schema = parse(texto).unwrap_or_else(|e| panic!("{nombre}: {e}"));
        let table: SymbolTable<'_> =
            analyze(&schema).unwrap_or_else(|e| panic!("{nombre}: {}", e[0]));
        let ir = lower(&table).unwrap_or_else(|e| panic!("{nombre}: {:?}", e.errors()));
        assert_eq!(ir.nodes().len(), table.len(), "{nombre}");
        for sid in table.ids() {
            let node = ir.node(id(sid.index()));
            let t = table.node(sid);
            assert_eq!(node.sort, Sort::Json, "{nombre}");
            assert_eq!(node.origin.span, t.schema.span, "{nombre}: span de {sid:?}");
            assert_eq!(
                node.origin.parent.map(NodeId::index),
                t.parent.map(|(p, _)| p.index()),
                "{nombre}: padre de {sid:?}"
            );
            assert_eq!(
                ir.location(id(sid.index())),
                format!("#{}", table.pointer(sid)),
                "{nombre}"
            );
        }
    }
}

/// §18 de `docs/fase3/pendientes.md` (4.2-S3): los casos de `tests/cases`
/// se bajan con el texto original de su esquema, no reserializado. En
/// `valid-recursive-tree-01` las reglas de `$defs/node` salen en el orden del
/// caso (`type`, `properties` con `additionalProperties`, `required`); con
/// las claves ordenadas por `serde_json` saldrían `properties`, `required`,
/// `type`. Y un lexema que `f64` no conserva llega intacto.
#[test]
fn casos_con_el_texto_original_del_esquema() {
    let caso = common::casos_json()
        .into_iter()
        .find(|c| c.path.ends_with("valid/valid-recursive-tree-01.json"))
        .expect("falta valid-recursive-tree-01");
    let ir = bajar(std::str::from_utf8(&caso.schema).unwrap());
    let nodo = (0..ir.nodes().len())
        .find(|&i| ir.node(id(i)).origin.step == Step::Entry("$defs".into(), "node".into()))
        .unwrap();
    let kinds: Vec<&str> = ir
        .node(id(nodo))
        .rules
        .iter()
        .map(|c| c.rule.kind().key())
        .collect();
    assert_eq!(kinds, ["type", "properties", "required"]);

    let texto = common::texto_del_esquema(
        br#"{"_id": "x", "schema": {"z": 1, "maximum": 0.10000000000000000000001, "a": [1e400]}}"#,
    );
    assert_eq!(
        texto,
        br#"{"z": 1, "maximum": 0.10000000000000000000001, "a": [1e400]}"#
    );
}

/// Criterio 2: un `$ref` es una arista al nodo destino; dos referencias al
/// mismo destino no lo duplican.
#[test]
fn ref_es_arista_sin_duplicar_el_destino() {
    let ir = bajar(
        r##"{"$defs": {"a": {"type": "string"}},
             "properties": {"x": {"$ref": "#/$defs/a"}, "y": {"$ref": "#/$defs/a"}}}"##,
    );
    // raíz, $defs/a, properties/x, properties/y
    assert_eq!(ir.nodes().len(), 4);
    assert_eq!(reglas(&ir, 2), [Rule::Ref(id(1))]);
    assert_eq!(reglas(&ir, 3), [Rule::Ref(id(1))]);
    assert_eq!(
        reglas(&ir, 0),
        [Rule::Properties {
            declared: vec![("x".into(), id(2)), ("y".into(), id(3))],
            additional: None,
        }]
    );
    // La entrada de `$defs` es un nodo sin regla propia.
    assert_eq!(
        ir.node(id(1)).origin.step,
        Step::Entry("$defs".into(), "a".into())
    );
    assert_eq!(ir.edge_count(), 4);
}

/// Criterio 2: la recursión productiva queda como un ciclo del grafo (que
/// `verify` admite porque pasa por una arista que consume entrada).
#[test]
fn recursion_productiva_es_un_ciclo() {
    let arbol =
        std::fs::read_to_string(Path::new(TESTS_DIR).join("schemas/parse-ok/arbol-recursivo.json"))
            .unwrap();
    assert!(tiene_ciclo(&bajar(&arbol)));
    let directo = bajar(r##"{"properties": {"hijo": {"$ref": "#"}}}"##);
    assert_eq!(reglas(&directo, 1), [Rule::Ref(NodeId::ROOT)]);
    assert!(tiene_ciclo(&directo));
    // Sin recursión no hay ciclo.
    assert!(!tiene_ciclo(&bajar(
        r##"{"$defs": {"a": {}}, "items": {"$ref": "#/$defs/a"}}"##
    )));
}

/// `true` es un nodo sin reglas y `false` es `Reject`, también como
/// subesquema.
#[test]
fn esquemas_booleanos() {
    assert_eq!(reglas(&bajar("true"), 0), []);
    assert_eq!(reglas(&bajar("false"), 0), [Rule::Reject]);
    let ir = bajar(r#"{"additionalProperties": false, "items": true}"#);
    assert_eq!(reglas(&ir, 1), [Rule::Reject]);
    assert_eq!(reglas(&ir, 2), []);
    assert_eq!(
        reglas(&ir, 0),
        [
            Rule::Properties {
                declared: vec![],
                additional: Some(id(1)),
            },
            Rule::Items {
                prefix: vec![],
                rest: Some(id(2)),
            },
        ]
    );
}

/// Las anotaciones, `format`, `$schema`, `$id`, `$anchor`, `$comment` y
/// `uniqueItems: false` no producen reglas.
#[test]
fn anotaciones_se_descartan() {
    let ir = bajar(
        r##"{"$schema": "https://json-schema.org/draft/2020-12/schema", "$id": "urn:x",
             "$anchor": "r", "$comment": "c", "title": "t", "description": "d",
             "default": {"a": 1}, "examples": [1], "deprecated": true, "readOnly": false,
             "writeOnly": true, "format": "email", "uniqueItems": false}"##,
    );
    assert_eq!(reglas(&ir, 0), []);
}

/// Cada palabra clave baja a su regla, en el orden del esquema
/// (`ir.md` §5.1); `prefixItems` + `items` y `properties` +
/// `additionalProperties` son una sola regla en la posición de la primera.
#[test]
fn correspondencia_de_cada_palabra_clave() {
    let ir = bajar(
        r##"{
          "type": ["object", "integer"],
          "enum": [1, "a"],
          "const": null,
          "minimum": -1.50, "maximum": 1e3, "exclusiveMinimum": 0, "exclusiveMaximum": 2E+3,
          "multipleOf": 0.5,
          "minLength": 1, "maxLength": 2, "pattern": "^a+$",
          "items": {}, "prefixItems": [{}, {}], "minItems": 3, "maxItems": 4, "uniqueItems": true,
          "properties": {"p": {}}, "additionalProperties": {}, "required": ["p", "q"],
          "minProperties": 5, "maxProperties": 6,
          "dependentRequired": {"p": ["q"], "r": []},
          "dependentSchemas": {"p": {}},
          "allOf": [{}], "anyOf": [{}, {}], "oneOf": [{}], "not": {}
        }"##,
    );
    // Ids en preorden: items 1, prefixItems 2 y 3, properties/p 4,
    // additionalProperties 5, dependentSchemas/p 6, allOf 7, anyOf 8 y 9,
    // oneOf 10, not 11.
    assert_eq!(ir.nodes().len(), 12);
    let n = |s: &str| Num::new(s);
    assert_eq!(
        reglas(&ir, 0),
        [
            Rule::Type(TypeSet::of(&[JsonType::Object, JsonType::Integer])),
            Rule::Enum(vec![
                Literal(vec![LitToken::Number("1".into())]),
                Literal(vec![LitToken::String("a".into())]),
            ]),
            Rule::Const(Literal(vec![LitToken::Null])),
            Rule::Minimum(n("-1.50")),
            Rule::Maximum(n("1e3")),
            Rule::ExclusiveMinimum(n("0")),
            Rule::ExclusiveMaximum(n("2E+3")),
            Rule::MultipleOf(n("0.5")),
            Rule::MinLength(1),
            Rule::MaxLength(2),
            Rule::Pattern("^a+$".into()),
            Rule::Items {
                prefix: vec![id(2), id(3)],
                rest: Some(id(1)),
            },
            Rule::MinItems(3),
            Rule::MaxItems(4),
            Rule::UniqueItems,
            Rule::Properties {
                declared: vec![("p".into(), id(4))],
                additional: Some(id(5)),
            },
            Rule::Required(vec!["p".into(), "q".into()]),
            Rule::MinProperties(5),
            Rule::MaxProperties(6),
            Rule::DependentRequired(vec![("p".into(), vec!["q".into()]), ("r".into(), vec![]),]),
            Rule::DependentSchemas(vec![("p".into(), id(6))]),
            Rule::AllOf(vec![id(7)]),
            Rule::AnyOf(vec![id(8), id(9)]),
            Rule::OneOf(vec![id(10)]),
            Rule::Not(id(11)),
        ]
    );
    // El rango de cada regla es el de su palabra clave (nombre y valor); el
    // de una regla doble, el de la primera de las dos.
    let src_type = r#""type": ["object", "integer"]"#;
    let c = &ir.node(NodeId::ROOT).rules[0];
    assert_eq!(c.span.end - c.span.start.offset, src_type.len() as u64);
    assert_eq!((c.span.start.line, c.span.start.column), (2, 11));
    let items = &ir.node(NodeId::ROOT).rules[11];
    assert_eq!(items.span.start.line, 8);
    assert_eq!(items.span.start.column, 11);
    assert_eq!(
        items.span.end - items.span.start.offset,
        r#""items": {}"#.len() as u64
    );
}

/// `type` con un solo nombre.
#[test]
fn type_con_un_nombre() {
    assert_eq!(
        reglas(&bajar(r#"{"type": "null"}"#), 0),
        [Rule::Type(TypeSet::of(&[JsonType::Null]))]
    );
}

/// Los números de las cotas van como el lexema exacto (sin pasar por
/// `f64`); los contadores se convierten a `u64` exacto y saturan en
/// `u64::MAX` (`ir.md` §5.1).
#[test]
fn numeros_como_lexema_y_contadores_saturados() {
    let ir = bajar(
        r#"{"minimum": 0.1000000000000000000000000000001,
            "maximum": 1e400, "minLength": 5.0, "maxLength": 1.5e1,
            "minItems": 18446744073709551615, "maxItems": 18446744073709551616,
            "minProperties": 1e400, "maxProperties": 0e999999999999999999999}"#,
    );
    assert_eq!(
        reglas(&ir, 0),
        [
            Rule::Minimum(Num::new("0.1000000000000000000000000000001")),
            Rule::Maximum(Num::new("1e400")),
            Rule::MinLength(5),
            Rule::MaxLength(15),
            Rule::MinItems(u64::MAX),
            Rule::MaxItems(u64::MAX),
            Rule::MinProperties(u64::MAX),
            Rule::MaxProperties(0),
        ]
    );
}

/// Los literales de `enum` y `const` van en preorden, con las claves de
/// cada objeto ordenadas por punto de código (I9), y los números con su
/// lexema.
#[test]
fn literales_en_preorden_con_claves_ordenadas() {
    let ir = bajar(r#"{"const": {"b": [1.0, {"z": true, "ä": null, "a": false}], "a": "x"}}"#);
    use LitToken as T;
    assert_eq!(
        reglas(&ir, 0),
        [Rule::Const(Literal(vec![
            T::Object(2),
            T::Key("a".into()),
            T::String("x".into()),
            T::Key("b".into()),
            T::Array(2),
            T::Number("1.0".into()),
            T::Object(3),
            T::Key("a".into()),
            T::Bool(false),
            T::Key("z".into()),
            T::Bool(true),
            T::Key("ä".into()),
            T::Null,
        ]))]
    );
}

/// D13: se bajan también las definiciones inalcanzables, con su contenido.
#[test]
fn definiciones_inalcanzables_se_bajan() {
    let ir = bajar(r#"{"definitions": {"x": {"minimum": 1}}, "$defs": {"y": false}}"#);
    assert_eq!(ir.nodes().len(), 3);
    assert_eq!(reglas(&ir, 1), [Rule::Minimum(Num::new("1"))]);
    assert_eq!(reglas(&ir, 2), [Rule::Reject]);
    assert_eq!(ir.location(id(1)), "#/definitions/x");
    assert_eq!(ir.edge_count(), 0);
}

/// Criterio 3 (O(n), sin umbrales de tiempo, D1): una cadena de m = 10⁴
/// definiciones, cada una con `allOf` a la siguiente dos veces (2^(m − 1)
/// caminos desde la primera). Si la bajada copiara el destino de cada
/// `$ref`, el tamaño sería exponencial; con aristas compartidas la IR tiene
/// exactamente un nodo por subesquema (1 + 3m − 2) y una arista por `$ref`,
/// `allOf` y entrada de `allOf`.
#[test]
fn cadena_de_referencias_tamano_lineal() {
    let m = 10_000;
    let mut src = String::from(r##"{"$ref": "#/$defs/d0", "$defs": {"##);
    for i in 0..m {
        if i > 0 {
            src.push(',');
        }
        if i + 1 < m {
            let r = format!(r##"{{"$ref": "#/$defs/d{}"}}"##, i + 1);
            src.push_str(&format!(r#""d{i}": {{"allOf": [{r}, {r}]}}"#));
        } else {
            src.push_str(&format!(r#""d{i}": {{"type": "integer"}}"#));
        }
    }
    src.push_str("}}");
    let ir = bajar(&src);
    // raíz + m definiciones + 2 ramas de `allOf` en las m − 1 primeras.
    assert_eq!(ir.nodes().len(), 1 + m + 2 * (m - 1));
    // `$ref` de la raíz, `allOf` de 2 ramas en m − 1 nodos y un `$ref` en
    // cada rama.
    assert_eq!(ir.edge_count(), 1 + 2 * (m - 1) + 2 * (m - 1));
}

/// D7: la bajada no es recursiva. Un esquema con 241 niveles de `{`/`[`
/// anidados (120 objetos de `not`, el objeto del `const` y un literal de
/// 120 arreglos; 242 contando el escalar `0` del fondo), cerca del tope de
/// 256 que cuenta el parser, se analiza y se baja en un
/// hilo de 256 KiB; el parser, que sí es recursivo, y la liberación del AST
/// corren en un hilo con pila suficiente. El tope exacto (256 y 257) lo
/// prueban `crates/cli/tests/ir.rs` (`profundidad_maxima`,
/// `profundidad_257_igual_que_check`).
#[test]
fn bajada_sin_recursion() {
    let d = 120;
    let src = format!(
        "{}{{\"const\": {}0{}}}{}",
        r#"{"not":"#.repeat(d),
        "[".repeat(d),
        "]".repeat(d),
        "}".repeat(d)
    );
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let schema = parse(src.as_bytes()).unwrap();
            let n = std::thread::scope(|s| {
                std::thread::Builder::new()
                    .stack_size(256 << 10)
                    .spawn_scoped(s, || {
                        let table = analyze(&schema).unwrap();
                        let ir = lower(&table).unwrap();
                        let Rule::Const(Literal(toks)) = &reglas(&ir, d)[0] else {
                            panic!("const")
                        };
                        assert_eq!(toks.len(), d + 1);
                        ir.nodes().len()
                    })
                    .unwrap()
                    .join()
                    .unwrap()
            });
            assert_eq!(n, d + 1);
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Criterio 2 (determinismo): cada IR dorada de `tests/ir/<nombre>.json` es
/// exactamente la salida de `ir::write_json` para
/// `tests/schemas/parse-ok/<nombre>.json`. Se regeneran con `make doradas`
/// tras revisar el cambio a mano.
#[test]
fn ir_doradas() {
    let doradas = json_files(&Path::new(TESTS_DIR).join("ir"));
    assert!(doradas.len() >= 3, "hacen falta al menos 3 doradas");
    for dorada in doradas {
        let nombre = dorada.file_name().unwrap().to_string_lossy().into_owned();
        let fuente = Path::new(TESTS_DIR).join("schemas/parse-ok").join(&nombre);
        let texto = std::fs::read_to_string(&fuente).unwrap_or_else(|e| panic!("{nombre}: {e}"));
        let ir = bajar(&texto);
        let mut salida = String::new();
        ir::write_json(&ir, &mut salida).unwrap();
        // Dos bajadas del mismo texto dan la misma salida.
        let mut otra = String::new();
        ir::write_json(&bajar(&texto), &mut otra).unwrap();
        assert_eq!(salida, otra, "{nombre}: no determinista");
        // Cualquier lector JSON la lee sin perder nada: los números de los
        // literales son lexemas en cadenas (`bajada.md` §5.1).
        let v: serde_json::Value = serde_json::from_str(&salida)
            .unwrap_or_else(|e| panic!("{nombre}: JSON ilegible: {e}"));
        if nombre == "literales.json" {
            let reglas = &v["nodes"][0]["rules"];
            assert_eq!(reglas[0]["rule"], "enum");
            assert_eq!(
                reglas[0]["values"][0],
                serde_json::json!([["number", "1e400"]])
            );
            assert_eq!(
                reglas[0]["values"][1],
                serde_json::json!([["number", "0.10000000000000000000001"]])
            );
            assert_eq!(
                reglas[1]["value"][1],
                serde_json::json!(["key", "k\"ey/~1"])
            );
            assert_eq!(reglas[1]["value"][5], serde_json::json!(["key", "b\n"]));
            assert_eq!(
                reglas[1]["value"][7],
                serde_json::json!(["number", "1e400"])
            );
        }
        let esperado = std::fs::read_to_string(&dorada).unwrap();
        assert!(
            salida == esperado,
            "{nombre}: la IR no coincide con la dorada (`make doradas`)\n{salida}"
        );
    }
}

/// Tarea 4.3 (`docs/fase3/clasificacion.md` §4) sobre esquemas reales: el
/// nodo de `not`/`anyOf` es (c) tanto sobre ramas (a) como (c), sus ramas
/// conservan su clase, y la de un nodo sube por `$ref` pero no por
/// `properties`.
#[test]
fn clasificacion_de_not_y_any_of_sobre_a_y_sobre_c() {
    use ir::{classify, Class, Strategy as S};
    let clases = |src: &str| -> Vec<(Class, Vec<S>)> {
        let ir = bajar(src);
        let c = classify(&ir);
        c.nodes()
            .iter()
            .map(|n| (n.class, n.strategies.iter().collect()))
            .collect()
    };
    assert_eq!(
        clases(r#"{"not": {"type": "string"}}"#),
        [
            (Class::C, vec![S::Transition, S::BoundedProduct]),
            (Class::A, vec![S::Transition]),
        ]
    );
    assert_eq!(
        clases(r#"{"not": {"pattern": "^a"}}"#),
        [
            (Class::C, vec![S::BoundedProduct, S::Dfa]),
            (Class::C, vec![S::Dfa]),
        ]
    );
    assert_eq!(
        clases(r#"{"anyOf": [{"type": "string"}, {"minLength": 2}]}"#),
        [
            (
                Class::C,
                vec![S::Transition, S::Register, S::BoundedDeterminization]
            ),
            (Class::A, vec![S::Transition]),
            (Class::B, vec![S::Register]),
        ]
    );
    assert_eq!(
        clases(r#"{"anyOf": [{"type": "string"}, {"uniqueItems": true}]}"#)[0],
        (
            Class::C,
            vec![S::Transition, S::BoundedDeterminization, S::Deferred]
        )
    );
    let por_ref = clases(r##"{"$defs": {"p": {"pattern": "x"}}, "$ref": "#/$defs/p"}"##);
    assert_eq!(por_ref[0], (Class::C, vec![S::Transition, S::Dfa]));
    let por_propiedad = clases(r#"{"properties": {"a": {"pattern": "x"}}}"#);
    assert_eq!(por_propiedad[0], (Class::A, vec![S::Transition]));
    assert_eq!(por_propiedad[1].0, Class::C);
}
