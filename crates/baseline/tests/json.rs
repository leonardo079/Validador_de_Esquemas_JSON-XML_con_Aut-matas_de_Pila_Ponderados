//! Casos unitarios de la línea base JSON (`docs/fase6/linea-base.md`).
//!
//! Todo lo que recorre un payload corre dentro de `with_stack` (D7), y
//! ninguna prueba mide tiempo (D1): la de ReDoS solo exige que termine.

use baseline::{with_stack, JsonBaseline, Kind, Limits, Outcome, MAX_DEPTH_LIMIT};
use serde_json::json;

fn compilar(schema: serde_json::Value) -> JsonBaseline {
    JsonBaseline::compile(&schema).unwrap_or_else(|e| panic!("{schema}: {e}"))
}

fn validar(schema: serde_json::Value, payload: &[u8]) -> Outcome {
    let base = compilar(schema);
    with_stack(|s| base.validate(s, payload))
}

/// `kind` y `offset` de un resultado negativo de lectura.
fn error_de_lectura(o: &Outcome) -> (Option<Kind>, Option<u64>) {
    assert!(!o.valid, "{o:?}");
    assert!(o.violations.is_empty(), "{o:?}");
    let m = o.malformed.as_ref().unwrap_or_else(|| panic!("{o:?}"));
    (o.kind, m.offset)
}

fn anidado(n: usize) -> Vec<u8> {
    let mut s = "[".repeat(n);
    s.push_str(&"]".repeat(n));
    s.into_bytes()
}

// --- Claves duplicadas (D2, U5) ---

#[test]
fn clave_duplicada_en_la_raiz_es_malformado_en_la_comilla_que_la_cierra() {
    let o = validar(json!(true), br#"{"a":1,"a":2}"#);
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(9)));
    let m = o.malformed.unwrap();
    assert_eq!((m.line, m.column), (1, 10));
    assert!(m.message.contains("duplicada"), "{}", m.message);
}

#[test]
fn clave_duplicada_con_espacios_antes_de_los_dos_puntos() {
    // serde_json ya consumió los espacios cuando informa el error; el offset
    // se corrige hacia atrás hasta la comilla.
    let o = validar(json!(true), b"{\"a\":1,\"a\" \n\t :2}");
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(9)));
}

#[test]
fn clave_duplicada_anidada() {
    let o = validar(json!(true), br#"{"x":{"b":1,"b":2}}"#);
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(14)));
}

#[test]
fn clave_duplicada_dentro_de_un_arreglo() {
    let o = validar(json!(true), br#"[{"k":0,"k":0}]"#);
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(10)));
}

/// CR-I1: `serde_json` ya consumió el `}` (y los espacios) que siguen a la
/// clave repetida cuando informa el error; el offset vuelve a la comilla.
#[test]
fn clave_duplicada_seguida_del_cierre_del_objeto() {
    for p in [&br#"{"a":1,"a"}"#[..], br#"{"a":1,"a"   }"#] {
        let o = validar(json!(true), p);
        assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(9)));
        assert_eq!(p[9], b'"');
    }
}

/// Tester 3: la duplicada se compara tras decodificar los escapes, en los
/// dos órdenes; el offset es la comilla que cierra la segunda.
#[test]
fn clave_duplicada_escrita_con_escape() {
    // `a` escrita como escape `a` (la barra, con su código: 92).
    let escape = format!("{}u0061", char::from(92));
    for p in [
        format!(r#"{{"a":1,"{escape}":2}}"#),
        format!(r#"{{"{escape}":1,"a":2}}"#),
    ] {
        assert_eq!(p.len(), 18, "{p}");
        let o = validar(json!(true), p.as_bytes());
        assert_eq!(
            error_de_lectura(&o),
            (Some(Kind::Malformed), Some(14)),
            "{p}"
        );
        assert_eq!(p.as_bytes()[14], b'"');
    }
}

#[test]
fn la_misma_clave_en_objetos_distintos_es_valida() {
    assert!(validar(json!(true), br#"[{"k":0},{"k":1}]"#).valid);
    assert!(validar(json!(true), br#"{"a":{"k":1},"b":{"k":2},"k":3}"#).valid);
}

// --- Profundidad (00-alcance.md §4, D7) ---

#[test]
fn profundidad_256_es_valida() {
    assert!(validar(json!(true), &anidado(256)).valid);
}

#[test]
fn profundidad_257_es_limite_en_el_corchete_que_la_pasa() {
    let o = validar(json!(true), &anidado(257));
    assert_eq!(error_de_lectura(&o), (Some(Kind::Limit), Some(256)));
    // También con un objeto y con espacios después del contenedor.
    let mut s = "[".repeat(256).into_bytes();
    s.extend_from_slice(b"{ \n }");
    s.extend(std::iter::repeat_n(b']', 256));
    let o = validar(json!(true), &s);
    assert_eq!(error_de_lectura(&o), (Some(Kind::Limit), Some(256)));
}

#[test]
fn cien_mil_corchetes_son_limite_sin_desbordar_la_pila() {
    let o = validar(json!(true), "[".repeat(100_000).as_bytes());
    assert_eq!(error_de_lectura(&o), (Some(Kind::Limit), Some(256)));
}

/// CR-I1: tras el `[` que pasa el tope, `end_seq` de `serde_json` consume
/// espacios, una `,` y más espacios antes de informar; el offset vuelve al `[`.
#[test]
fn limite_de_profundidad_seguido_de_una_coma() {
    for resto in [&b",1]"[..], b" , ]"] {
        let mut p = "[".repeat(257).into_bytes();
        p.extend_from_slice(resto);
        let o = validar(json!(true), &p);
        assert_eq!(error_de_lectura(&o), (Some(Kind::Limit), Some(256)));
        assert_eq!(p[256], b'[');
    }
}

/// Tester 3: la profundidad cuenta objetos y arreglos por igual.
#[test]
fn profundidad_con_objetos_y_con_mezcla() {
    assert!(validar(json!(true), &objetos(256)).valid);
    let o = validar(json!(true), &objetos(257));
    // Cada `{"a":` mide 5 bytes: el objeto 257 empieza en 256·5.
    assert_eq!(error_de_lectura(&o), (Some(Kind::Limit), Some(1280)));

    // Mezcla: `[` y `{"a":` alternados.
    let mezcla = |n: usize| {
        let (mut abre, mut cierra, mut ultimo) = (Vec::new(), Vec::new(), 0);
        for i in 0..n {
            ultimo = abre.len();
            if i % 2 == 0 {
                abre.push(b'[');
                cierra.push(b']');
            } else {
                abre.extend_from_slice(b"{\"a\":");
                cierra.push(b'}');
            }
        }
        abre.push(b'1');
        cierra.reverse();
        abre.extend(cierra);
        (abre, ultimo as u64)
    };
    assert!(validar(json!(true), &mezcla(256).0).valid);
    let (p, ultimo) = mezcla(257);
    let o = validar(json!(true), &p);
    assert_eq!(error_de_lectura(&o), (Some(Kind::Limit), Some(ultimo)));
}

#[test]
fn la_profundidad_se_configura_solo_a_la_baja() {
    let base = compilar(json!(true)).with_limits(Limits {
        max_depth: 2,
        ..Limits::default()
    });
    let o = with_stack(|s| base.validate(s, b"[[[]]]"));
    assert_eq!(error_de_lectura(&o), (Some(Kind::Limit), Some(2)));
    let base = compilar(json!(true)).with_limits(Limits {
        max_depth: u32::MAX,
        ..Limits::default()
    });
    let o = with_stack(|s| base.validate(s, &anidado(MAX_DEPTH_LIMIT as usize + 1)));
    assert_eq!(error_de_lectura(&o).0, Some(Kind::Limit));
}

/// B2: con `STACK_SIZE`, un `$ref` recursivo valida a la profundidad máxima
/// y también produce un error en la hoja (en `dev` y en release).
#[test]
fn ref_recursivo_a_profundidad_maxima() {
    let schema = json!({
        "$defs": { "n": { "type": "array", "items": { "$ref": "#/$defs/n" } } },
        "$ref": "#/$defs/n"
    });
    assert!(validar(schema.clone(), &anidado(256)).valid);
    let mut s = "[".repeat(255).into_bytes();
    s.extend_from_slice(b"[1]");
    s.extend(std::iter::repeat_n(b']', 255));
    let o = validar(schema, &s);
    assert_eq!(o.kind, Some(Kind::Schema), "{o:?}");
    assert_eq!(o.violations[0].pointer, "/0".repeat(256));
}

/// `n` objetos `{"a":…}` anidados con un `1` en el fondo.
fn objetos(n: usize) -> Vec<u8> {
    let mut s = "{\"a\":".repeat(n);
    s.push('1');
    s.push_str(&"}".repeat(n));
    s.into_bytes()
}

/// CR-I2: `drop`, `clone`, `==` y `{:?}` de `JsonDom` son recursivos, y con
/// objetos piden más pila que con arreglos (en `dev`, hasta 512-768 KiB el
/// `clone` con 256 objetos). Como `JsonDom` no es `Send`, solo se recorren
/// dentro de `with_stack`, donde caben.
#[test]
fn recorrer_un_dom_de_256_objetos_dentro_de_with_stack() {
    let base = compilar(json!(true));
    let (iguales, texto) = with_stack(|s| {
        let dom = base.parse(s, &objetos(256)).unwrap();
        let copia = dom.clone();
        let iguales = dom == copia;
        let texto = format!("{dom:?}").len();
        drop(copia);
        drop(dom);
        (iguales, texto)
    });
    assert!(iguales);
    assert!(texto > 256 * 4);
}

// --- Forma del payload ---

#[test]
fn bom_inicial_se_ignora_y_los_offsets_cuentan_sus_3_bytes() {
    assert!(validar(json!({"type": "object"}), b"\xEF\xBB\xBF{}").valid);
    let o = validar(json!(true), b"\xEF\xBB\xBF{\"a\": 01}");
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(10)));
    let m = o.malformed.unwrap();
    assert_eq!((m.line, m.column), (1, 11));
}

#[test]
fn numero_que_no_cabe_en_f64_no_esta_soportado() {
    for p in [&b"1e400"[..], b"-1e400", b"[0, 1E+400]"] {
        let o = validar(json!(true), p);
        assert_eq!(o.kind, Some(Kind::Unsupported), "{o:?}");
        assert!(o.malformed.is_some());
    }
}

#[test]
fn basura_despues_del_valor_es_malformado() {
    let o = validar(json!(true), b"{} x");
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(3)));
}

#[test]
fn fin_de_entrada_prematuro_tiene_el_offset_de_la_longitud() {
    for p in [&b"{\"a\":"[..], b"[1, 2", b"", b"\n"] {
        let o = validar(json!(true), p);
        assert_eq!(
            error_de_lectura(&o),
            (Some(Kind::Malformed), Some(p.len() as u64)),
            "{:?}",
            String::from_utf8_lossy(p)
        );
    }
}

#[test]
fn error_en_otra_linea_tiene_linea_y_columna_en_bytes() {
    let o = validar(json!(true), "{\n  \"ñ\": 01}".as_bytes());
    let m = o.malformed.unwrap();
    // `ñ` ocupa dos bytes: la columna del `1` es la 10 en bytes (9 en
    // puntos de código).
    assert_eq!((m.line, m.column, m.offset), (2, 10, Some(11)));
}

// --- Validación ---

#[test]
fn violacion_de_tipo_con_puntero_y_palabra_clave() {
    let o = validar(
        json!({"properties": {"a": {"type": "integer"}}}),
        br#"{"a": "x"}"#,
    );
    assert_eq!(o.kind, Some(Kind::Schema));
    assert_eq!(o.violation_count(), 1);
    assert_eq!(o.violations[0].pointer, "/a");
    assert_eq!(o.violations[0].keyword, "type");
}

#[test]
fn format_email_no_se_valida() {
    let schema = json!({"type": "string", "format": "email"});
    assert!(validar(schema, br#""esto no es un correo""#).valid);
}

#[test]
fn patron_con_retroceso_exponencial_termina() {
    let schema = json!({"type": "string", "pattern": "^(a+)+$"});
    let mut p = String::from("\"");
    p.push_str(&"a".repeat(4096));
    p.push_str("!\"");
    let o = validar(schema, p.as_bytes());
    assert_eq!(o.kind, Some(Kind::Schema), "{o:?}");
    assert_eq!(o.violations[0].keyword, "pattern");
}

// --- Compilación segura (B5) ---

#[test]
fn ref_remoto_no_compila_y_no_usa_la_red() {
    for uri in [
        "http://example.com/s.json",
        "https://example.com/s.json",
        "file:///etc/passwd",
        "file:///C:/Windows/win.ini",
    ] {
        let e = JsonBaseline::compile(&json!({"$ref": uri})).unwrap_err();
        // El error sale del recuperador propio, que nunca abre nada.
        assert!(e.message.contains("sin red"), "{uri}: {e}");
    }
}

#[test]
fn pattern_con_lookahead_no_compila() {
    // Control: con su motor por defecto (`fancy-regex`, con retroceso)
    // jsonschema lo acepta. Que la línea base lo rechace prueba que usa el
    // motor lineal (`regex`).
    for patron in ["^(?=a)a$", "(a)\\1"] {
        let schema = json!({ "pattern": patron });
        assert!(jsonschema::draft202012::options().build(&schema).is_ok());
        let e = JsonBaseline::compile(&schema).unwrap_err();
        assert!(e.message.contains("regex"), "{patron}: {e}");
    }
}

#[test]
fn compile_text_rechaza_claves_duplicadas_y_acepta_bom() {
    let e = JsonBaseline::compile_text(br#"{"type":"string","type":"integer"}"#).unwrap_err();
    assert!(e.message.contains("duplicada"), "{e}");
    assert!(JsonBaseline::compile_text(b"\xEF\xBB\xBF{\"type\":\"string\"}").is_ok());
    assert!(JsonBaseline::compile_text(b"{\"type\": ").is_err());
}

#[test]
fn esquema_mas_profundo_que_el_tope_no_compila() {
    let mut s = "{\"not\":".repeat(300);
    s.push_str("true");
    s.push_str(&"}".repeat(300));
    assert!(JsonBaseline::compile_text(s.as_bytes()).is_err());
    let v: serde_json::Value = {
        let mut v = json!(true);
        for _ in 0..300 {
            v = json!({ "not": v });
        }
        v
    };
    let e = JsonBaseline::compile(&v).unwrap_err();
    assert!(e.message.contains("profundidad"), "{e}");
    // `v` es recursivo al liberarse: se suelta en un hilo con pila holgada.
    with_stack(move |_| drop(v));
}

#[test]
fn las_etapas_se_pueden_llamar_por_separado() {
    let base = compilar(json!({"type": "array", "items": {"type": "integer"}}));
    let (a, b) = with_stack(|s| {
        let dom = base.parse(s, b"[1, \"x\", 3]").unwrap();
        (
            base.validate_dom(s, &dom),
            base.validate(s, b"[1, \"x\", 3]"),
        )
    });
    assert_eq!(a, b);
    assert_eq!(a.violations[0].pointer, "/1");
}

/// Un ciclo de `$ref` no productivo (que el proyecto rechaza al compilar,
/// D12) no cuelga ni desborda la pila de la línea base: `jsonschema` lo
/// compila y lo trata como `true`. Es una divergencia documentada.
#[test]
fn ref_ciclico_no_productivo_termina() {
    for (s, valido) in [
        (
            json!({"$defs": {"a": {"$ref": "#/$defs/a"}}, "$ref": "#/$defs/a"}),
            true,
        ),
        (json!({"$ref": "#"}), true),
        (
            json!({
                "$defs": {
                    "a": {"allOf": [{"$ref": "#/$defs/b"}]},
                    "b": {"anyOf": [{"$ref": "#/$defs/a"}]}
                },
                "$ref": "#/$defs/a"
            }),
            true,
        ),
        (json!({"not": {"$ref": "#"}}), false),
    ] {
        assert_eq!(validar(s.clone(), b"{\"a\": [1]}").valid, valido, "{s}");
    }
}

// --- Divergencias con el proyecto (docs/fase6/linea-base.md §4) ---

/// El recuento de `iter_errors` (U7) que la tabla de divergencias describe:
/// no es el peso del DPDA (F4.2), así que se fija aquí para notar si
/// `jsonschema` lo cambia.
#[test]
fn recuento_de_violaciones_de_jsonschema() {
    let casos: [(serde_json::Value, &[u8], &[&str]); 9] = [
        (
            json!({"required": ["a", "b", "c"]}),
            b"{}",
            &["required"; 3],
        ),
        (
            json!({"properties": {"a": {}}, "additionalProperties": false}),
            br#"{"x":1,"y":2}"#,
            &["additionalProperties"],
        ),
        (
            json!({"type": "string", "enum": ["a"]}),
            b"1",
            &["type", "enum"],
        ),
        (
            json!({"allOf": [{"type": "string"}, {"minimum": 5}, {"maximum": 0}]}),
            b"3",
            &["type", "minimum", "maximum"],
        ),
        (
            json!({"anyOf": [{"type": "string"}, {"type": "integer"}]}),
            b"true",
            &["anyOf"],
        ),
        (
            json!({"oneOf": [{"type": "integer"}, {"minimum": 0}]}),
            b"3",
            &["oneOf"],
        ),
        (json!({"not": {"type": "integer"}}), b"3", &["not"]),
        (
            json!({"items": {"type": "string"}}),
            b"[1,2,3]",
            &["type"; 3],
        ),
        (json!({"uniqueItems": true}), b"[1,1,1]", &["uniqueItems"]),
    ];
    for (schema, payload, palabras) in casos {
        let o = validar(schema.clone(), payload);
        let obtenidas: Vec<&str> = o.violations.iter().map(|v| v.keyword.as_str()).collect();
        assert_eq!(obtenidas, palabras, "{schema}");
        assert_eq!(o.violation_count(), palabras.len());
    }
}

/// Sin `arbitrary_precision` (U3), los números que no caben en `u64`/`i64`
/// pasan por `f64`. Hasta `u64::MAX` la comparación es exacta; por encima,
/// dos enteros distintos pueden ser el mismo `f64` (el proyecto compara el
/// lexema, `00-alcance.md` §1.1).
#[test]
fn numeros_mas_alla_de_f64() {
    let max_u64 = JsonBaseline::compile_text(br#"{"maximum": 18446744073709551615}"#).unwrap();
    let o = with_stack(|s| max_u64.validate(s, b"18446744073709551616"));
    assert_eq!(o.kind, Some(Kind::Schema), "{o:?}");
    // Divergencia: 2^64 + 1 > 2^64, pero los dos son el mismo `f64`.
    let max_2_64 = JsonBaseline::compile_text(br#"{"maximum": 18446744073709551616}"#).unwrap();
    assert!(with_stack(|s| max_2_64.validate(s, b"18446744073709551617")).valid);
    // Coinciden con el proyecto: `1.0` es entero y 0.3 es múltiplo de 0.1.
    assert!(validar(json!({"type": "integer"}), b"1.0").valid);
    assert!(validar(json!({"multipleOf": 0.1}), b"0.3").valid);
}

/// Un escape de sustituto alto sin su pareja es un error sintáctico de
/// `serde_json` (pendientes.md §1: comparar con el intérprete en F4.6).
#[test]
fn escape_de_sustituto_suelto_es_malformado() {
    let o = validar(json!({"type": "string"}), br#""\ud800""#);
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(7)));
}

/// `jsonschema` traduce `\d` de ECMA-262 a ASCII, como el proyecto.
#[test]
fn d_en_pattern_es_ascii() {
    let schema = json!({"pattern": r"^\d$"});
    assert!(validar(schema.clone(), br#""7""#).valid);
    assert_eq!(validar(schema, "\"٣\"".as_bytes()).kind, Some(Kind::Schema));
}

/// `jsonschema` compila lo que el proyecto rechaza en la Fase 3 (D6): aplica
/// `if`/`then`, ignora palabras clave desconocidas y fuerza 2020-12 aunque
/// `$schema` diga otro draft.
#[test]
fn palabras_clave_fuera_del_subconjunto_se_aceptan() {
    let o = validar(
        json!({"if": {"type": "string"}, "then": {"minLength": 3}}),
        br#""ab""#,
    );
    assert_eq!(o.violations[0].keyword, "minLength");
    assert!(validar(json!({"foo": 1}), b"1").valid);
    // `prefixItems` solo existe en 2020-12: con el draft que detecta
    // jsonschema por `$schema` (control) se ignora.
    let draft7 = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "prefixItems": [{"type": "string"}]
    });
    let detectado = jsonschema::options().build(&draft7).unwrap();
    assert!(detectado.is_valid(&json!([1])));
    assert_eq!(validar(draft7, b"[1]").kind, Some(Kind::Schema));
}

/// Cada nodo del árbol ocupa 32 bytes, más lo que reserve fuera (cadenas,
/// vectores, nodos del `BTreeMap`): la base de la estimación de memoria de
/// `docs/fase6/linea-base.md` §1.
#[cfg(target_pointer_width = "64")]
#[test]
fn tamano_de_un_nodo_del_dom() {
    assert_eq!(std::mem::size_of::<serde_json::Value>(), 32);
}

/// `JsonBaseline` se compila una vez y se comparte entre hilos de
/// `with_stack` (`linea-base.md` §2). Que `JsonDom` **no** sea `Send` ni
/// `Sync` (CR-I2) lo comprueban los dos doc-tests `compile_fail` de
/// `JsonDom` (`crates/baseline/src/json.rs`).
#[test]
fn json_baseline_es_send_y_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<JsonBaseline>();
    send_sync::<baseline::corpus::Case>();
    send_sync::<baseline::Outcome>();
}

// --- Forma del payload: más casos borde (Tester 1-3) ---

/// D10: un prefijo de BOM incompleto no es un BOM: error en el offset 0.
#[test]
fn bom_incompleto_es_malformado_en_el_offset_0() {
    for p in [&b"\xEF"[..], b"\xEF\xBB", b"\xEF\xBB\x7B\x7D"] {
        let o = validar(json!(true), p);
        assert_eq!(
            error_de_lectura(&o),
            (Some(Kind::Malformed), Some(0)),
            "{p:?}"
        );
    }
}

#[test]
fn utf8_invalido_en_un_valor_y_en_una_clave() {
    let o = validar(json!(true), b"\"a\xFFb\"");
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(2)));
    let o = validar(json!(true), b"{\"a\xFF\":1}");
    assert_eq!(error_de_lectura(&o), (Some(Kind::Malformed), Some(3)));
}

/// Solo `\n` cambia de línea, como en `serde_json`: `\r\n` es un salto y un
/// `\r` suelto no (pendientes.md §8: comparar con el lexer en F4.6).
#[test]
fn crlf_y_cr_suelto_en_linea_y_columna() {
    let o = validar(json!(true), b"{\"a\":\r\n  01}");
    let m = o.malformed.unwrap();
    assert_eq!((m.line, m.column, m.offset), (2, 4, Some(10)));
    let o = validar(json!(true), b"{\"a\":\r 01}");
    let m = o.malformed.unwrap();
    assert_eq!((m.line, m.column, m.offset), (1, 9, Some(8)));
}

/// Fila 10 de la tabla de divergencias: la línea base no distingue un
/// error léxico de uno sintáctico.
#[test]
fn lexico_y_sintactico_son_la_misma_clase() {
    let lexico = validar(json!(true), br#"{"a": 01}"#);
    let sintactico = validar(json!(true), br#"{"a" 1}"#);
    assert_eq!(error_de_lectura(&lexico), (Some(Kind::Malformed), Some(7)));
    assert_eq!(
        error_de_lectura(&sintactico),
        (Some(Kind::Malformed), Some(5))
    );
}

// --- Topes de la lista de violaciones y de los mensajes (SEC-M1, M2, L1) ---

#[test]
fn la_lista_de_violaciones_se_corta_en_max_violations() {
    let schema = json!({"items": {"type": "string"}});
    let mut p = "[1".to_owned();
    p.push_str(&",1".repeat(99_999));
    p.push(']');
    let o = validar(schema.clone(), p.as_bytes());
    assert_eq!(o.kind, Some(Kind::Schema));
    assert_eq!(o.violation_count(), 100);
    assert!(o.truncated);
    assert_eq!(o.violations[99].pointer, "/99");

    let base = compilar(schema).with_limits(Limits {
        max_violations: 3,
        ..Limits::default()
    });
    let o = with_stack(|s| base.validate(s, b"[1, 2, 3]"));
    assert_eq!((o.violation_count(), o.truncated), (3, false));
    let o = with_stack(|s| base.validate(s, b"[1, 2, 3, 4]"));
    assert_eq!((o.violation_count(), o.truncated), (3, true));
    // 0 se trata como 1: un `Schema` siempre trae una violación.
    let base = compilar(json!({"type": "string"})).with_limits(Limits {
        max_violations: 0,
        ..Limits::default()
    });
    let o = with_stack(|s| base.validate(s, b"1"));
    assert_eq!((o.violation_count(), o.truncated), (1, false));
}

#[test]
fn el_mensaje_de_una_violacion_tiene_tope() {
    // `jsonschema` copia la instancia en el mensaje: aquí, 200 KB.
    let mut p = "[\"".to_owned();
    p.push_str(&"x".repeat(200_000));
    p.push_str("\"]");
    let o = validar(json!({"not": {}}), p.as_bytes());
    let m = &o.violations[0].message;
    assert!(m.len() <= baseline::MAX_MESSAGE_BYTES, "{}", m.len());
    assert!(m.ends_with('…'));
    // Un mensaje corto no se toca.
    let o = validar(json!({"type": "string"}), b"1");
    assert!(!o.violations[0].message.ends_with('…'));
}

#[test]
fn el_mensaje_de_compile_error_tiene_tope() {
    // Un `(` sin cerrar seguido de 10⁶ letras: no compila, y el mensaje lo copia.
    let mut patron = String::from("(");
    patron.push_str(&"a".repeat(1_000_000));
    let e = JsonBaseline::compile(&json!({ "pattern": patron })).unwrap_err();
    assert!(
        e.message.len() <= baseline::MAX_MESSAGE_BYTES,
        "{}",
        e.message.len()
    );
    assert!(e.message.ends_with('…'));
}

// --- Más filas de la tabla de divergencias ---

/// Fila 4: un decimal con más cifras de las que guarda un `f64` se compara
/// redondeado (el proyecto compara el lexema y lo rechazaría).
#[test]
fn decimal_largo_se_redondea_a_f64() {
    let schema = JsonBaseline::compile_text(br#"{"maximum": 0.1}"#).unwrap();
    assert!(with_stack(|s| schema.validate(s, b"0.10000000000000000001")).valid);
}

/// Fila 9: `jsonschema` da los errores en el orden en que evalúa el
/// esquema, no por posición en el payload; por eso el corpus compara
/// `pointer` contra el conjunto (B10).
#[test]
fn orden_de_los_errores_es_el_del_esquema() {
    let o = validar(
        json!({"properties": {"b": {"type": "string"}, "a": {"type": "string"}}}),
        br#"{"b": 1, "a": 2}"#,
    );
    let punteros: Vec<&str> = o.violations.iter().map(|v| v.pointer.as_str()).collect();
    // Por posición, /b va antes que /a.
    assert_eq!(punteros, ["/a", "/b"]);
}

/// Filas 20-23: los demás límites de `00-alcance.md` §4 no los aplica la
/// línea base (pendientes.md §13).
#[test]
fn limites_del_alcance_que_la_linea_base_no_aplica() {
    // Cadena de 2 MiB (el proyecto: `limit` pasado 1 MiB).
    let mut p = "\"".to_owned();
    p.push_str(&"x".repeat(2 << 20));
    p.push('"');
    assert!(validar(json!({"type": "string"}), p.as_bytes()).valid);
    // Lexema numérico de 2 003 bytes (el proyecto: `limit` pasados 1 024).
    let mut n = "0.".to_owned();
    n.push_str(&"0".repeat(2000));
    n.push('1');
    assert!(validar(json!({"type": "number"}), n.as_bytes()).valid);
    // 2 000 nueves no caben en `f64`: `Unsupported`, no `Limit`.
    let o = validar(json!(true), "9".repeat(2000).as_bytes());
    assert_eq!(o.kind, Some(Kind::Unsupported));
    // 2 000 claves no declaradas (el proyecto: `limit` pasadas 1 024).
    let claves: Vec<String> = (0..2000).map(|i| format!("\"k{i}\":0")).collect();
    let p = format!("{{{}}}", claves.join(","));
    assert!(validar(json!({"type": "object"}), p.as_bytes()).valid);
}

/// Fila 2: un número fuera de `f64` en el **esquema** hace que no compile
/// (el proyecto lo acepta: compara lexemas).
#[test]
fn esquema_con_numero_fuera_de_f64_no_compila() {
    let e = JsonBaseline::compile_text(br#"{"maximum": 1e400}"#).unwrap_err();
    assert!(e.message.contains("number out of range"), "{e}");
}

/// Fila 18 (CR-S5): un `$ref` a un meta-esquema oficial compila sin red,
/// porque `jsonschema` los trae incluidos; el proyecto lo rechaza.
#[test]
fn ref_al_meta_esquema_compila_sin_red() {
    let base = compilar(json!({"$ref": "https://json-schema.org/draft/2020-12/schema"}));
    assert!(with_stack(|s| base.validate(s, br#"{"type": "string"}"#)).valid);
    assert_eq!(
        with_stack(|s| base.validate(s, br#"{"type": 5}"#)).kind,
        Some(Kind::Schema)
    );
}
