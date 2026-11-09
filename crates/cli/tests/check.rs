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

// --- XSD (tarea 5.4) ----------------------------------------------------------

/// Un XSD válido: 0 y el subconjunto que corresponde (D22).
#[test]
fn xsd_valido() {
    let (code, out, err) = schemac(&Path::new(TESTS_DIR).join("xsd/parse-ok/pedido.xsd"));
    assert_eq!(code, 0, "{err}");
    assert!(err.is_empty(), "{err}");
    assert!(
        out.ends_with("pedido.xsd: esquema válido (subconjunto de XSD 1.0); 2 tipos complejos\n"),
        "{out}"
    );
}

/// 1 con línea y columna: un error de la Fase 2 (el primero), uno del
/// análisis y todos los de la primera etapa que falla (B2), con las cifras
/// del manifiesto `errores-semanticos-xsd.json`.
#[test]
fn errores_xsd_con_linea_y_columna() {
    let (code, out, err) = schemac(&Path::new(TESTS_DIR).join("xsd/parse-error/doctype.xsd"));
    assert_eq!(code, 1);
    assert!(out.is_empty(), "{out}");
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("doctype.xsd: línea 2, columna 3: error léxico: <!DOCTYPE"));

    let (code, _, err) =
        schemac(&Path::new(TESTS_DIR).join("xsd/semantic-error/ref-sin-resolver.xsd"));
    assert_eq!(code, 1);
    assert!(
        err.contains("línea 4, columna 24: no hay ningún elemento global \"nadie\""),
        "{err}"
    );

    let (code, _, err) =
        schemac(&Path::new(TESTS_DIR).join("xsd/semantic-error/todos-los-de-la-etapa.xsd"));
    assert_eq!(code, 1);
    assert_eq!(err.lines().count(), 4, "{err}");
    assert!(err.starts_with(&format!(
        "{}: línea 6, columna 34: no hay ningún tipo \"Nadie\"",
        Path::new(TESTS_DIR)
            .join("xsd/semantic-error/todos-los-de-la-etapa.xsd")
            .display()
    )));
}

/// D22: el formato sale del contenido, no de la extensión. Un XSD en un
/// `.json` es XSD; un JSON en un `.xsd`, JSON; un XSD con BOM y espacios
/// delante, XSD; un JSON con BOM, JSON; y un prefijo incompleto de BOM
/// delante de `<` no es un BOM: es JSON, y su lexer lo rechaza en el
/// offset 0 (D10).
#[test]
fn formato_por_el_contenido() {
    let d = std::env::temp_dir().join(format!("schemac-formato-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let xsd = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
               <xs:element name='e' type='xs:int'/></xs:schema>";
    let casos = [
        ("xsd.json", xsd.as_bytes().to_vec()),
        ("json.xsd", br#"{"type": "object"}"#.to_vec()),
        (
            "bom.xsd",
            [&b"\xEF\xBB\xBF \n\t"[..], xsd.as_bytes()].concat(),
        ),
        ("bom.json", b"\xEF\xBB\xBF{}".to_vec()),
        ("medio-bom.xsd", [&b"\xEF\xBB"[..], xsd.as_bytes()].concat()),
    ];
    let mut res = Vec::new();
    for (nombre, bytes) in &casos {
        let p = d.join(nombre);
        std::fs::write(&p, bytes).unwrap();
        res.push(schemac(&p));
    }
    let _ = std::fs::remove_dir_all(&d);
    let xsd_ok = "esquema válido (subconjunto de XSD 1.0); 0 tipos complejos\n";
    let json_ok = "esquema válido (subconjunto de JSON Schema 2020-12)";
    assert_eq!(res[0].0, 0, "{}", res[0].2);
    assert!(res[0].1.ends_with(xsd_ok), "{}", res[0].1);
    assert_eq!(res[1].0, 0, "{}", res[1].2);
    assert!(res[1].1.contains(json_ok), "{}", res[1].1);
    assert_eq!(res[2].0, 0, "{}", res[2].2);
    assert!(res[2].1.ends_with(xsd_ok), "{}", res[2].1);
    assert_eq!(res[3].0, 0, "{}", res[3].2);
    assert!(res[3].1.contains(json_ok), "{}", res[3].1);
    assert_eq!(res[4].0, 1);
    assert!(res[4].2.contains("línea 1, columna 1: "), "{}", res[4].2);
}

/// XSD de `n` niveles de elementos anidados: `xs:schema`, `xs:complexType`,
/// `n − 3` `xs:sequence` y un `xs:element`.
fn xsd_de_profundidad(n: usize) -> String {
    format!(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'><xs:complexType name='T'>{}\
         <xs:element name='a' type='xs:int'/>{}</xs:complexType></xs:schema>",
        "<xs:sequence>".repeat(n - 3),
        "</xs:sequence>".repeat(n - 3)
    )
}

/// D7 con XSD: parser, análisis y liberación del AST corren en el hilo de
/// 8 MiB, así que 256 niveles (el tope del lector, D36) dan 0 y 257, el
/// error `limit` con código 1, nunca un desbordamiento.
#[test]
fn profundidad_maxima_xsd() {
    let mut res = Vec::new();
    for n in [256, 257] {
        let ruta: PathBuf =
            std::env::temp_dir().join(format!("schemac-profundo-{n}-{}.xsd", std::process::id()));
        std::fs::write(&ruta, xsd_de_profundidad(n)).unwrap();
        res.push(schemac(&ruta));
        let _ = std::fs::remove_file(&ruta);
    }
    assert_eq!(res[0].0, 0, "{}", res[0].2);
    assert_eq!(res[1].0, 1, "{}", res[1].2);
    assert!(
        res[1]
            .2
            .contains("profundidad máxima de 256 elementos anidados"),
        "{}",
        res[1].2
    );
}
