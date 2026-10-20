//! Formato de los casos de `tests/cases` (reglas de `tests/README.md`).
//!
//! `common::corpus()` aplica `validar_formato` a cada caso antes de que lo
//! usen las pruebas de `json.rs` y `xml.rs`; aquí se recorre el corpus una vez
//! más por sí solo y se comprueba que la validación detecta cada tipo de
//! incumplimiento, sin dejar archivos en `tests/cases`.

mod common;

use common::{schema_file_existe, validar_formato, CATEGORIAS};
use serde_json::{json, Value};

/// Un caso mínimo correcto de `invalid/invalid-tema-01.json`.
fn caso() -> Value {
    json!({
        "_id": "invalid-tema-01",
        "category": "invalid",
        "format": "json",
        "description": "Caso de prueba.",
        "phase": 4,
        "schema": {"type": "object"},
        "payload": "{}",
        "expected": {"valid": false, "violations": 1, "errorKind": "schema", "offset": 1}
    })
}

/// Valida `v` como si estuviera en `invalid/invalid-tema-01.json` y exige un
/// único error que contenga `fragmento`.
fn falla(v: &Value, fragmento: &str) {
    falla_en("invalid", "invalid-tema-01", v, fragmento);
}

fn falla_en(carpeta: &str, stem: &str, v: &Value, fragmento: &str) {
    let e = validar_formato(carpeta, stem, v);
    assert!(
        e.len() == 1 && e[0].contains(fragmento),
        "se esperaba un error con {fragmento:?}, hubo {e:?}"
    );
}

fn con(campo: &str, valor: Value) -> Value {
    let mut v = caso();
    v[campo] = valor;
    v
}

fn sin(campo: &str) -> Value {
    let mut v = caso();
    v.as_object_mut().unwrap().remove(campo);
    v
}

#[test]
fn el_corpus_cumple_el_formato() {
    let cases = common::corpus();
    assert!(!cases.is_empty());
    // Las carpetas de tests/cases son exactamente las categorías.
    let mut carpetas: Vec<String> =
        std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cases"))
            .unwrap()
            .map(|d| d.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
    carpetas.sort();
    let mut esperadas: Vec<String> = CATEGORIAS.iter().map(|c| c.to_string()).collect();
    esperadas.sort();
    assert_eq!(carpetas, esperadas, "carpetas de tests/cases");
}

#[test]
fn caso_correcto_no_tiene_errores() {
    assert_eq!(
        validar_formato("invalid", "invalid-tema-01", &caso()),
        Vec::<String>::new()
    );
    let mut v = con("_id", json!("valid-xml-pedido-01"));
    v["category"] = json!("valid");
    v["format"] = json!("xml");
    v["schema"] = json!("<xs:schema/>");
    v["expected"] = json!({"valid": true, "violations": 0});
    assert_eq!(
        validar_formato("valid", "valid-xml-pedido-01", &v),
        Vec::<String>::new()
    );
}

#[test]
fn id_distinto_del_nombre_del_archivo() {
    falla_en(
        "invalid",
        "invalid-otro-01",
        &caso(),
        "`_id` \"invalid-tema-01\" no coincide con el nombre del archivo \"invalid-otro-01.json\"",
    );
}

#[test]
fn categoria_distinta_de_la_carpeta() {
    // Mismo `_id` y nombre, pero el archivo está en malformed/.
    let e = validar_formato("malformed", "invalid-tema-01", &caso());
    assert_eq!(
        e,
        ["`category` \"invalid\" no coincide con la carpeta \"malformed\""],
        "{e:?}"
    );
    // Una categoría inexistente también deja al `_id` sin su prefijo.
    let e = validar_formato("invalid", "invalid-tema-01", &con("category", json!("inv")));
    assert!(
        e.iter()
            .any(|m| m.contains("`category` \"inv\" no es una de")),
        "{e:?}"
    );
    let e = validar_formato("invalid", "invalid-tema-01", &sin("category"));
    assert!(e.iter().any(|m| m.contains("`category` falta")), "{e:?}");
}

#[test]
fn forma_del_id() {
    for id in [
        "inv-required-001", // prefijo distinto de la categoría
        "invalid-tema-1",
        "invalid-01",
        "invalid-Tema-01",
        "invalid-tema--x-01",
        "invalid--tema-01",
        "invalid-tema--01",
        "invalid-tema-0a",
    ] {
        let v = con("_id", json!(id));
        let e = validar_formato("invalid", id, &v);
        assert!(
            e.len() == 1 && e[0].contains("no tiene la forma"),
            "{id}: {e:?}"
        );
    }
}

#[test]
fn campos_basicos() {
    falla(&con("format", json!("yaml")), "`format`");
    falla(&con("description", json!("  ")), "`description`");
    falla(&con("phase", json!("4")), "`phase`");
    falla(
        &con("payloadfile", json!("x")),
        "campo desconocido `payloadfile`",
    );
    falla(&con("metrics", json!({})), "campo desconocido `metrics`");
    falla(
        &con("schemaFile", json!("esquemas/a.json")),
        "exactamente uno de `schema`",
    );
    falla(&sin("schema"), "exactamente uno de `schema`");
    falla(
        &con("schema", json!("<xs:schema/>")),
        "objeto o un booleano",
    );
    falla(
        &con("strategy", json!("deferred")),
        "solo se admite en la categoría non-dpda",
    );
}

#[test]
fn expected() {
    falla(&sin("expected"), "`expected` falta");
    falla(
        &con("expected", json!({"violations": 0})),
        "`expected.valid`",
    );
    falla(
        &con("expected", json!({"valid": true, "errorKind": "schema"})),
        "solo se admite si `valid` es false",
    );
    falla(
        &con("expected", json!({"valid": false, "errorKind": "tipo"})),
        "no es lexical",
    );
    falla(
        &con("expected", json!({"valid": false, "violations": 0})),
        "0 ⇔ valid",
    );
    falla(
        &con(
            "expected",
            json!({"valid": false, "violations": 1, "errorKind": "lexical", "offset": 3}),
        ),
        "se omite con errorKind lexical",
    );
    falla(
        &con("expected", json!({"valid": false, "offset": -1})),
        "`expected.offset`",
    );
}

/// Reglas que antes no tenían una prueba que fallara al quitarlas.
#[test]
fn tipos_y_dominios() {
    // strategy con un valor fuera de su dominio, en un caso non-dpda.
    let mut v = con("_id", json!("non-dpda-tema-01"));
    v["category"] = json!("non-dpda");
    v["strategy"] = json!("otra");
    falla_en(
        "non-dpda",
        "non-dpda-tema-01",
        &v,
        "`strategy` \"otra\" no es una de",
    );
    v["strategy"] = json!("deferred");
    assert_eq!(
        validar_formato("non-dpda", "non-dpda-tema-01", &v),
        Vec::<String>::new()
    );

    // schema objeto en un caso xml.
    let mut v = con("format", json!("xml"));
    v["schema"] = json!({"type": "object"});
    falla(&v, "debe ser una cadena con el XSD");

    // schemaFile que no es cadena.
    let mut v = sin("schema");
    v["schemaFile"] = json!(3);
    falla(&v, "`schemaFile` debe ser una cadena");

    falla(
        &con("expected", json!({"valid": false, "pointer": 3})),
        "`expected.pointer` debe ser una cadena",
    );
    falla(
        &con("expected", json!({"valid": false, "violations": "1"})),
        "`expected.violations` debe ser un entero",
    );
}

/// JSON Schema 2020-12 admite los esquemas booleanos `true` y `false`.
#[test]
fn schema_booleano_aceptado() {
    for b in [true, false] {
        assert_eq!(
            validar_formato("invalid", "invalid-tema-01", &con("schema", json!(b))),
            Vec::<String>::new()
        );
    }
    falla(&con("schema", json!(3)), "objeto o un booleano");
}

/// Un campo desconocido dentro de `expected` (una errata) es un error.
#[test]
fn errata_en_expected() {
    let v = con(
        "expected",
        json!({"valid": false, "violation": 1, "errrorKind": "schema", "offset": 64}),
    );
    let e = validar_formato("invalid", "invalid-tema-01", &v);
    assert_eq!(
        e,
        [
            "campo desconocido `expected.errrorKind`",
            "campo desconocido `expected.violation`"
        ],
        "{e:?}"
    );
}

/// `schemaFile` y `payloadFile`: relativas, sin `./` ni `..`, y los `.json` en una
/// subcarpeta.
#[test]
fn rutas_de_archivos() {
    for (campo, f, fragmento) in [
        ("schemaFile", "../../../etc/passwd", "sin `..`"),
        ("schemaFile", "/etc/passwd", "sin `..`"),
        ("schemaFile", "", "sin `..`"),
        // `./` haría pasar un `.json` del primer nivel por "subcarpeta".
        ("schemaFile", "./arbol.json", "tampoco se admite `./`"),
        ("schemaFile", "./pedido.xsd", "tampoco se admite `./`"),
        ("schemaFile", "arbol.json", "va en una subcarpeta"),
        ("payloadFile", "sub/../../x.xml", "sin `..`"),
        ("payloadFile", "datos.json", "va en una subcarpeta"),
    ] {
        let mut v = caso();
        let o = v.as_object_mut().unwrap();
        if campo == "schemaFile" {
            o.remove("schema");
        } else {
            o.remove("payload");
        }
        v[campo] = json!(f);
        let e = validar_formato("invalid", "invalid-tema-01", &v);
        assert!(
            e.len() == 1 && e[0].contains(campo) && e[0].contains(fragmento),
            "{campo} = {f:?}: {e:?}"
        );
    }
    for f in ["pedido.xsd", "esquemas/arbol.json"] {
        let mut v = sin("schema");
        v["schemaFile"] = json!(f);
        assert_eq!(
            validar_formato("invalid", "invalid-tema-01", &v),
            Vec::<String>::new()
        );
    }
}

/// El archivo de `schemaFile` debe existir en la carpeta del caso. Se prueba
/// con una carpeta temporal propia que se borra al final.
#[test]
fn schema_file_debe_existir() {
    let dir = std::env::temp_dir().join(format!("corpus-schemafile-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("esquemas")).unwrap();
    std::fs::write(dir.join("pedido.xsd"), "<xs:schema/>").unwrap();
    std::fs::write(dir.join("esquemas/arbol.json"), "{}").unwrap();
    let con_archivo = |f: &str| {
        let mut v = sin("schema");
        v["schemaFile"] = json!(f);
        schema_file_existe(&dir, &v)
    };
    let resultados = (
        con_archivo("pedido.xsd"),
        con_archivo("esquemas/arbol.json"),
        con_archivo("falta.xsd"),
        con_archivo("esquemas"),           // una carpeta no es un archivo
        schema_file_existe(&dir, &caso()), // sin schemaFile no hay nada que comprobar
    );
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(resultados.0, Ok(()));
    assert_eq!(resultados.1, Ok(()));
    assert!(
        resultados
            .2
            .as_ref()
            .is_err_and(|m| m.contains("falta.xsd") && m.contains("no existe")),
        "{:?}",
        resultados.2
    );
    assert!(resultados.3.is_err(), "{:?}", resultados.3);
    assert_eq!(resultados.4, Ok(()));
}
