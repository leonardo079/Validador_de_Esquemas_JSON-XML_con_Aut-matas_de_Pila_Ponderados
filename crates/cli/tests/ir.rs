//! `schemac ir`: la IR en JSON estable o en DOT, y los códigos de salida
//! (0 con la IR, 1 con los mismos errores que `check`, 2 si no puede leer;
//! el 3 del error interno se prueba en `src/main.rs`), con esquemas JSON y
//! XSD (tarea 5.4; el formato sale del contenido, D22).

use std::path::{Path, PathBuf};
use std::process::Command;

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

fn schemac(args: &[&std::ffi::OsStr]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_schemac"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn ruta(rel: &str) -> PathBuf {
    Path::new(TESTS_DIR).join(rel)
}

fn ir(p: &Path) -> (i32, String, String) {
    schemac(&["ir".as_ref(), p.as_os_str()])
}

/// 0 y la IR en JSON: un nodo por subesquema.
#[test]
fn ir_de_un_esquema_valido() {
    let (code, out, err) = ir(&ruta("schemas/parse-ok/arbol-recursivo.json"));
    assert_eq!(code, 0, "{err}");
    assert!(err.is_empty(), "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).expect(&out);
    assert_eq!(v["irVersion"], 2);
    assert_eq!(v["format"], "json");
    // Los mismos 6 subesquemas que cuenta `schemac check`.
    assert_eq!(v["nodeCount"], 6);
    assert_eq!(v["nodes"].as_array().unwrap().len(), 6);
}

/// La salida es determinista y es la dorada versionada en `tests/ir`.
#[test]
fn ir_determinista_y_dorada() {
    let p = ruta("schemas/parse-ok/arbol-recursivo.json");
    let (_, a, _) = ir(&p);
    let (_, b, _) = ir(&p);
    assert_eq!(a, b);
    let dorada = std::fs::read_to_string(ruta("ir/arbol-recursivo.json")).unwrap();
    assert_eq!(a, dorada);
}

/// `--dot` (antes o después de la ruta) da el grafo; es el `.dot` versionado
/// en `docs/fase3`.
#[test]
fn ir_en_dot() {
    let p = ruta("schemas/parse-ok/arbol-recursivo.json");
    let (code, out, err) = schemac(&["ir".as_ref(), "--dot".as_ref(), p.as_os_str()]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("digraph ir {\n"), "{out}");
    let (code, despues, _) = schemac(&["ir".as_ref(), p.as_os_str(), "--dot".as_ref()]);
    assert_eq!(code, 0);
    assert_eq!(out, despues);
    let versionado = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/fase3/ir-arbol-recursivo.dot"),
    )
    .unwrap();
    assert_eq!(out, versionado, "ejecutar `make docs`");
}

/// 1 con exactamente los mismos mensajes que `schemac check`, de la Fase 2
/// y de cada etapa de la Fase 3.
#[test]
fn errores_iguales_que_check() {
    for rel in [
        "schemas/parse-error/coma-final.json",
        "schemas/semantic-error/varios-errores.json",
        "schemas/semantic-error/ref-inexistente.json",
        "schemas/semantic-error/ciclo-directo.json",
        "schemas/parse-ok/combinadores.json",
    ] {
        let p = ruta(rel);
        let (code, out, err) = ir(&p);
        let (code_check, _, err_check) = schemac(&["check".as_ref(), p.as_os_str()]);
        assert_eq!((code, code_check), (1, 1), "{rel}: {err}");
        assert!(out.is_empty(), "{rel}: {out}");
        assert_eq!(err, err_check, "{rel}");
    }
}

/// 2 si no puede leer el archivo, con una ruta que no es UTF-8 o con
/// argumentos inválidos.
#[test]
fn no_puede_leer_o_argumentos_invalidos() {
    let (code, out, err) = ir(Path::new("/no/existe/esquema.json"));
    assert_eq!(code, 2);
    assert!(out.is_empty() && err.starts_with("schemac: "), "{err}");

    #[cfg(unix)]
    let nombre: std::ffi::OsString = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(b"x\xff.json".to_vec())
    };
    #[cfg(windows)]
    let nombre: std::ffi::OsString = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0x78, 0xD800, 0x2E, 0x6A, 0x73, 0x6F, 0x6E])
    };
    let (code, _, err) = ir(Path::new(&nombre));
    assert_eq!(code, 2, "{err}");
    assert!(!err.contains("panicked"), "{err}");

    let p = ruta("schemas/parse-ok/usuarios.json");
    for args in [
        vec!["ir".as_ref()],
        vec!["ir".as_ref(), "--svg".as_ref(), p.as_os_str()],
        vec!["ir".as_ref(), p.as_os_str(), p.as_os_str()],
        vec!["ir".as_ref(), "--dot".as_ref(), "--dot".as_ref()],
        // 4.2-S1: una opción desconocida junto a `--dot`, en las dos
        // posiciones, es un error de uso y no una ruta.
        vec!["ir".as_ref(), "--dot".as_ref(), "--json".as_ref()],
        vec!["ir".as_ref(), "--json".as_ref(), "--dot".as_ref()],
    ] {
        let (code, out, err) = schemac(&args);
        assert_eq!(code, 2, "{args:?}");
        assert!(out.is_empty() && err.contains("uso: schemac"), "{err}");
    }
}

/// D7: la bajada corre en el mismo hilo con pila explícita que `check`, así
/// que un esquema a la profundidad máxima del parser funciona.
#[test]
fn profundidad_maxima() {
    let d = 255;
    let src = format!("{}{{}}{}", r#"{"not":"#.repeat(d), "}".repeat(d));
    let p: PathBuf =
        std::env::temp_dir().join(format!("schemac-ir-profundo-{}.json", std::process::id()));
    std::fs::write(&p, src).unwrap();
    let (code, out, err) = ir(&p);
    let _ = std::fs::remove_file(&p);
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["nodeCount"], d + 1);
}

/// Pasar la profundidad máxima del parser (257 niveles) es un error del
/// esquema: código 1 y el mismo mensaje que `check`, no un desbordamiento.
#[test]
fn profundidad_257_igual_que_check() {
    let d = 256;
    let src = format!("{}{{}}{}", r#"{"not":"#.repeat(d), "}".repeat(d));
    let p: PathBuf =
        std::env::temp_dir().join(format!("schemac-ir-257-{}.json", std::process::id()));
    std::fs::write(&p, src).unwrap();
    let (code, out, err) = ir(&p);
    let (code_check, _, err_check) = schemac(&["check".as_ref(), p.as_os_str()]);
    let _ = std::fs::remove_file(&p);
    assert_eq!((code, code_check), (1, 1), "{err}");
    assert!(out.is_empty(), "{out}");
    assert!(err.contains("profundidad máxima de 256"), "{err}");
    assert_eq!(err, err_check);
}

/// Texto exacto del valor del miembro `schema` de un caso JSON, recortado
/// del archivo por el rango que da `schema_parser` (sus campos son palabras
/// clave fuera del subconjunto, que el parser conserva con el rango de su
/// valor), como `texto_del_esquema` de `crates/semantic/tests/common`: sin
/// reserializarlo con `serde_json`, que ordenaría las claves y pasaría los
/// números por `f64` (`docs/fase3/pendientes.md` §18 y §28).
fn texto_del_esquema(caso: &[u8]) -> Vec<u8> {
    let raiz = schema_parser::parse(caso).unwrap_or_else(|e| panic!("{e}"));
    let kw = raiz
        .keywords()
        .iter()
        .find(|k| k.name.value == "schema")
        .expect("el caso no tiene `schema`");
    let inicio = usize::try_from(kw.value_span.start.offset).unwrap();
    let fin = usize::try_from(kw.value_span.end).unwrap();
    caso[inicio..fin].to_vec()
}

/// Tarea 4.3: `schemac ir` marca cada nodo con su clase y sus estrategias.
/// Sobre el esquema del caso `non-dpda-anyof-ambiguous-01`, con el texto
/// original de su campo `schema` (4.3-S3, `pendientes.md` §28), la raíz es
/// el nodo (c) con determinización acotada, y sus ramas son (b)
/// (`required`); en DOT, el nodo (c) lleva la clase en su etiqueta y el
/// borde grueso.
#[test]
fn ir_senala_el_nodo_c_y_su_estrategia() {
    let bytes = std::fs::read(ruta("cases/non-dpda/non-dpda-anyof-ambiguous-01.json")).unwrap();
    let caso: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(caso["strategy"], "bounded-determinization");
    let src = texto_del_esquema(&bytes);
    // Es el texto del caso, no una reescritura: empieza en su `{` y
    // vuelve a dar el mismo valor.
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&src).unwrap(),
        caso["schema"]
    );
    let p: PathBuf =
        std::env::temp_dir().join(format!("schemac-ir-clase-{}.json", std::process::id()));
    std::fs::write(&p, src).unwrap();
    let (code, out, err) = ir(&p);
    let (code_dot, dot, _) = schemac(&["ir".as_ref(), "--dot".as_ref(), p.as_os_str()]);
    let _ = std::fs::remove_file(&p);
    assert_eq!((code, code_dot), (0, 0), "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["classCount"], serde_json::json!({"a": 0, "b": 2, "c": 1}));
    let raiz = &v["nodes"][0];
    assert_eq!(raiz["class"], "c");
    assert_eq!(
        raiz["strategies"],
        serde_json::json!(["transition", "register", "bounded-determinization"])
    );
    assert_eq!(raiz["rules"][0]["strategy"], "bounded-determinization");
    assert_eq!(v["nodes"][1]["class"], "b");
    assert!(
        dot.contains(
            "n0 [label=\"0 (raíz)\\nanyOf\\n(c) transición, registro o contador, \
             determinización acotada\", peripheries=2, style=bold];"
        ),
        "{dot}"
    );
}

// --- XSD (tarea 5.4) ----------------------------------------------------------

/// 0 y la IR JSON v2 de un XSD: `format: "xml"`, `targetNamespace`, clase
/// y estrategia. El pedido de `tests/cases` (`valid-xml-pedido-01`): el
/// documento con la `Choice` de los globales, el tipo anónimo de `pedido`
/// como único nodo (b), por el `Attributes` del atributo `id`, y
/// `#/builtin/positiveInteger` con `Minimum(1)`.
#[test]
fn ir_de_un_xsd() {
    let (code, out, err) = ir(&ruta("schemas/xsd/parse-ok/valid-xml-pedido-01.xsd"));
    assert_eq!(code, 0, "{err}");
    assert!(err.is_empty(), "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).expect(&out);
    assert_eq!(v["irVersion"], 2);
    assert_eq!(v["format"], "xml");
    assert_eq!(v["targetNamespace"], serde_json::Value::Null);
    assert_eq!(v["classCount"], serde_json::json!({"a": 6, "b": 1, "c": 0}));
    let doc = &v["nodes"][0];
    assert_eq!(doc["rules"][0]["rule"], "content");
    assert_eq!(doc["rules"][0]["particles"][0]["term"], "choice");
    let pedido = &v["nodes"][1];
    assert_eq!(
        pedido["step"],
        serde_json::json!({"keyword": "element", "entry": "pedido"})
    );
    assert_eq!(pedido["class"], "b");
    assert_eq!(
        pedido["strategies"],
        serde_json::json!(["transition", "register"])
    );
    assert_eq!(pedido["rules"][1]["rule"], "attributes");
    assert_eq!(pedido["rules"][1]["class"], "b");
    assert_eq!(pedido["rules"][1]["attributes"][0]["name"], "id");
    let pi = &v["nodes"][6];
    assert_eq!(pi["step"]["entry"], "positiveInteger");
    assert_eq!(pi["rules"][1]["rule"], "minimum");
    assert_eq!(pi["rules"][1]["value"], "1");

    // Con `targetNamespace` (recortado, `docs/fase3/xsd.md` §4).
    let (code, out, err) = ir(&ruta("schemas/xsd/parse-ok/espacio-por-defecto.xsd"));
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["targetNamespace"], "urn:ejemplo:t");
}

/// Las doradas XSD de `tests/ir/xsd` son la salida de `schemac ir`, que es
/// determinista; `--dot` del pedido (antes o después de la ruta) es el
/// `docs/fase3/ir-pedido.dot` versionado (`make docs`, `make docs-check`).
#[test]
fn ir_xsd_determinista_doradas_y_dot() {
    let mut doradas: Vec<PathBuf> = std::fs::read_dir(ruta("ir/xsd"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    doradas.sort();
    assert!(doradas.len() >= 2, "{doradas:?}");
    for dorada in doradas {
        let n = dorada.file_stem().unwrap().to_string_lossy().into_owned();
        let p = ruta(&format!("schemas/xsd/parse-ok/{n}.xsd"));
        let (code, a, err) = ir(&p);
        let (_, b, _) = ir(&p);
        assert_eq!(code, 0, "{n}: {err}");
        assert_eq!(a, b, "{n}");
        assert_eq!(
            a,
            std::fs::read_to_string(&dorada).unwrap(),
            "{n}: `make doradas`"
        );
    }
    let p = ruta("schemas/xsd/parse-ok/pedido.xsd");
    let (code, dot, err) = schemac(&["ir".as_ref(), "--dot".as_ref(), p.as_os_str()]);
    let (_, despues, _) = schemac(&["ir".as_ref(), p.as_os_str(), "--dot".as_ref()]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(dot, despues);
    assert!(dot.contains("label=\"IR (xml): 8 nodos"), "{dot}");
    let versionado = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/fase3/ir-pedido.dot"),
    )
    .unwrap();
    assert_eq!(dot, versionado, "ejecutar `make docs`");
}

/// 1 con exactamente los mismos mensajes que `schemac check` con XSD: de la
/// Fase 2 (lector y parser), de cada etapa del análisis y de un esquema con
/// varios errores en la misma etapa.
#[test]
fn errores_xsd_iguales_que_check() {
    for rel in [
        "schemas/xsd/parse-error/doctype.xsd",
        "schemas/xsd/parse-error/cierre-distinto.xsd",
        "schemas/xsd/parse-error/any.xsd",
        "schemas/xsd/semantic-error/ref-sin-resolver.xsd",
        "schemas/xsd/semantic-error/min-mayor-que-max.xsd",
        "schemas/xsd/semantic-error/todos-los-de-la-etapa.xsd",
        "schemas/xsd/parse-ok/tipos-simples.xsd",
    ] {
        let p = ruta(rel);
        let (code, out, err) = ir(&p);
        let (code_check, _, err_check) = schemac(&["check".as_ref(), p.as_os_str()]);
        assert_eq!((code, code_check), (1, 1), "{rel}: {err}");
        assert!(out.is_empty(), "{rel}: {out}");
        assert!(err.contains(": línea "), "{rel}: {err}");
        assert_eq!(err, err_check, "{rel}");
    }
    let (code, out, err) = ir(Path::new("/no/existe/esquema.xsd"));
    assert_eq!(code, 2);
    assert!(out.is_empty() && err.starts_with("schemac: "), "{err}");
}

/// D7 con XSD en `schemac ir`: 256 niveles bajan (en el hilo de 8 MiB) y
/// 257 dan el error `limit` de `check` con código 1.
#[test]
fn profundidad_xsd_igual_que_check() {
    let nivel = |n: usize| {
        format!(
            "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'><xs:complexType name='T'>{}\
             <xs:element name='a' type='xs:int'/>{}</xs:complexType></xs:schema>",
            "<xs:sequence>".repeat(n - 3),
            "</xs:sequence>".repeat(n - 3)
        )
    };
    let mut res = Vec::new();
    for n in [256, 257] {
        let p: PathBuf =
            std::env::temp_dir().join(format!("schemac-ir-xsd-{n}-{}.xsd", std::process::id()));
        std::fs::write(&p, nivel(n)).unwrap();
        res.push((ir(&p), schemac(&["check".as_ref(), p.as_os_str()])));
        let _ = std::fs::remove_file(&p);
    }
    let ((code, out, err), _) = &res[0];
    assert_eq!(*code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(out).unwrap();
    assert_eq!(v["format"], "xml");
    let ((code, out, err), (code_check, _, err_check)) = &res[1];
    assert_eq!((*code, *code_check), (1, 1), "{err}");
    assert!(out.is_empty());
    assert!(err.contains("profundidad máxima de 256"), "{err}");
    assert_eq!(err, err_check);
}
