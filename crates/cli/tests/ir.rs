//! `schemac ir`: la IR en JSON estable o en DOT, y los códigos de salida
//! (0 con la IR, 1 con los mismos errores que `check`, 2 si no puede leer;
//! el 3 del error interno se prueba en `src/main.rs`).

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

/// Tarea 4.3: `schemac ir` marca cada nodo con su clase y sus estrategias.
/// Sobre el esquema del caso `non-dpda-anyof-ambiguous-01` (leído del caso;
/// las aserciones no dependen del orden de las claves, así que basta
/// `serde_json`), la raíz es el nodo (c) con determinización acotada, y sus
/// ramas son (b) (`required`); en DOT, el nodo (c) lleva la clase en su
/// etiqueta y el borde grueso.
#[test]
fn ir_senala_el_nodo_c_y_su_estrategia() {
    let caso: serde_json::Value = serde_json::from_slice(
        &std::fs::read(ruta("cases/non-dpda/non-dpda-anyof-ambiguous-01.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(caso["strategy"], "bounded-determinization");
    let src = caso["schema"].to_string();
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
