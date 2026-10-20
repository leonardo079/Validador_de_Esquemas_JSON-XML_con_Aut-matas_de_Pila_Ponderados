//! `schemac check`: códigos de salida y mensajes con línea y columna.

use std::path::{Path, PathBuf};
use std::process::Command;

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/schemas");

fn schemac(ruta: &Path) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_schemac"))
        .arg("check")
        .arg(ruta)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn esquema_valido() {
    let (code, out, _) = schemac(&Path::new(TESTS_DIR).join("parse-ok/usuarios.json"));
    assert_eq!(code, 0);
    assert!(out.contains("esquema válido"), "{out}");
    let (code, out, _) = schemac(&Path::new(TESTS_DIR).join("parse-ok/arbol-recursivo.json"));
    assert_eq!(code, 0);
    assert!(
        out.contains("6 subesquemas, 1 entradas de $defs, 2 referencias resueltas"),
        "{out}"
    );
}

#[test]
fn errores_de_fase_2_y_3() {
    let (code, _, err) = schemac(&Path::new(TESTS_DIR).join("parse-error/coma-final.json"));
    assert_eq!(code, 1);
    assert!(err.contains("línea 3, columna 1: "), "{err}");

    let (code, _, err) = schemac(&Path::new(TESTS_DIR).join("semantic-error/varios-errores.json"));
    assert_eq!(code, 1);
    assert_eq!(err.lines().count(), 3, "{err}");
    assert!(err.contains("línea 3, columna 12: palabra clave \"if\" no soportada (clase (c)"));

    let (code, _, err) = schemac(&Path::new(TESTS_DIR).join("semantic-error/ref-inexistente.json"));
    assert_eq!(code, 1);
    assert!(
        err.contains("línea 3, columna 11: no se puede resolver \"$ref\""),
        "{err}"
    );

    let (code, _, err) = schemac(&Path::new(TESTS_DIR).join("semantic-error/ciclo-directo.json"));
    assert_eq!(code, 1);
    assert!(
        err.contains("línea 3, columna 20: ciclo de referencias que no consume entrada"),
        "{err}"
    );
}

/// D7: el análisis corre en un hilo con pila explícita, así que un esquema a
/// la profundidad máxima funciona sea cual sea la pila del hilo principal.
#[test]
fn profundidad_maxima() {
    let d = 255;
    let src = format!("{}{{}}{}", r#"{"not":"#.repeat(d), "}".repeat(d));
    let ruta: PathBuf =
        std::env::temp_dir().join(format!("schemac-profundo-{}.json", std::process::id()));
    std::fs::write(&ruta, src).unwrap();
    let (code, out, err) = schemac(&ruta);
    let _ = std::fs::remove_file(&ruta);
    assert_eq!(code, 0, "{out}{err}");
}

#[test]
fn archivo_inexistente() {
    let (code, _, err) = schemac(Path::new("/no/existe/esquema.json"));
    assert_eq!(code, 2);
    assert!(err.starts_with("schemac: "), "{err}");
}

/// Una ruta que no es UTF-8 no provoca un pánico: es un error de E/S (2).
#[test]
fn ruta_no_utf8() {
    #[cfg(unix)]
    let nombre: std::ffi::OsString = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(b"x\xff.json".to_vec())
    };
    #[cfg(windows)]
    let nombre: std::ffi::OsString = {
        use std::os::windows::ffi::OsStringExt;
        // `x\u{D800}.json`: sustituto alto suelto, UTF-16 inválido.
        std::ffi::OsString::from_wide(&[0x78, 0xD800, 0x2E, 0x6A, 0x73, 0x6F, 0x6E])
    };
    let (code, _, err) = schemac(Path::new(&nombre));
    assert_eq!(code, 2, "{err}");
    assert!(
        err.starts_with("schemac: ") && !err.contains("panicked"),
        "{err}"
    );
}
