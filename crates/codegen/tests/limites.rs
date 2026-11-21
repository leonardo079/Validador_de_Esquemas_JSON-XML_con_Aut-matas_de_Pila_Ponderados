//! F6.2: los validadores emitidos con límites propios
//! (`codegen::Opciones`, `schemac compile --limit`; criterios en
//! `tasks/todo.md` «Tarea F6.2», `docs/fase6/limites.md`). Se compilan de
//! verdad, en Rust y en C, con la orden del sandbox, y cada payload por
//! stdin da la línea JSON y el código del intérprete con esos mismos
//! límites. El tiempo se prueba con 0 ms (determinista: D1).

use std::path::PathBuf;

use automaton::{build, validate_with, Automaton, Mode, Options};
use codegen::{c, rust, Opciones, MAX_STATE_WORDS, NOMBRES_DE_LIMITES};
use serde_json::Value;

mod comun;
use comun::*;

/// Los límites de la prueba, por nombre.
fn opciones(pares: &[(&str, u64)]) -> Opciones {
    let mut op = Opciones::default();
    for (n, v) in pares {
        op.fijar(n, *v).unwrap();
    }
    op
}

fn compilar_rust(a: &Automaton, op: &Opciones, nombre: &str) -> PathBuf {
    op.comprobar(a).unwrap();
    let dir = directorio(nombre);
    escribir(&dir, &rust::emit_con(a, op).unwrap());
    let avisos = rustc(&dir, &["--edition", "2021", "-O", "main.rs"]);
    assert!(avisos.is_empty(), "{avisos}");
    dir.join("main")
}

fn compilar_c(a: &Automaton, op: &Opciones, nombre: &str) -> PathBuf {
    op.comprobar(a).unwrap();
    let dir = directorio(nombre);
    let archivos = c::emit_con(a, op).unwrap();
    escribir(&dir, &archivos);
    let mut fuentes: Vec<&str> = archivos
        .iter()
        .map(|x| x.nombre.as_str())
        .filter(|n| n.ends_with(".c"))
        .collect();
    fuentes.sort();
    let mut args = vec![
        "-std=c11",
        "-O2",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-o",
        "validador",
    ];
    args.extend(fuentes);
    compilar_con(&dir, &cc(), &args);
    dir.join("validador")
}

/// Cada payload da, en los dos binarios, la línea del intérprete con los
/// límites de `op`.
fn igual_que_el_interprete(a: &Automaton, op: &Opciones, binarios: &[PathBuf], payloads: &[&[u8]]) {
    let o = Options {
        mode: Mode::Total,
        limits: op.limites,
    };
    for p in payloads {
        let r = validate_with(a, p, &o, 4096);
        let esperado = json_esperado(a, &r);
        for b in binarios {
            let (codigo, out, err) = ejecutar(b, p);
            assert!(err.is_empty(), "{err}");
            let v: Value = serde_json::from_str(out.trim_end()).unwrap();
            assert_eq!(
                v,
                esperado,
                "{}: {}",
                b.display(),
                String::from_utf8_lossy(p)
            );
            assert_eq!(codigo, i32::from(codigo_esperado(&r)), "{}", b.display());
        }
    }
}

#[test]
fn limites_json_en_rust_y_c() {
    let a = build(&ir_de(
        br#"{"type":"object","properties":{"n":{"items":{"maximum":10}}},"additionalProperties":{"uniqueItems":true}}"#,
        false,
    ))
    .unwrap();
    let op = opciones(&[
        ("maxDepth", 3),
        ("maxString", 4),
        ("maxLexeme", 5),
        ("maxKeys", 2),
        ("keyPool", 3),
        ("maxUnique", 3),
        ("maxPayload", 60),
    ]);
    // Con estos límites la memoria es más pequeña que la de por defecto.
    assert!(op.comprobar(&a).unwrap() < Opciones::default().comprobar(&a).unwrap());
    let bins = [
        compilar_rust(&a, &op, "lim-rust-json"),
        compilar_c(&a, &op, "lim-c-json"),
    ];
    let largo = format!(r#"{{"n":[{}]}}"#, "1,".repeat(40));
    let payloads: Vec<&[u8]> = vec![
        br#"{"n":[1,2,3]}"#,
        br#"{"n":[[1]]}"#,            // profundidad 3: pasa
        br#"{"n":[[[1]]]}"#,          // profundidad 4: limit
        br#"{"abcd":[]}"#,            // cadena de 4: pasa
        br#"{"abcde":[]}"#,           // de 5: limit
        br#"{"n":[12345,99]}"#,       // lexema de 5: pasa (y viola maximum)
        br#"{"n":[123456]}"#,         // de 6: limit
        br#"{"a":[],"b":[]}"#,        // 2 claves no declaradas: pasa
        br#"{"a":[],"b":[],"c":[]}"#, // 3: limit
        br#"{"a":[1,2,3]}"#,          // 3 elementos únicos: pasa
        br#"{"a":[1,2,3,4]}"#,        // 4: limit
        largo.as_bytes(),             // más de 60 bytes: limit
    ];
    igual_que_el_interprete(&a, &op, &bins, &payloads);
    // Cada payload da lo que dice su comentario.
    let o = Options {
        mode: Mode::Total,
        limits: op.limites,
    };
    let clases: Vec<Option<&str>> = payloads
        .iter()
        .map(|p| validate_with(&a, p, &o, 4096).error_kind.map(clase))
        .collect();
    let (l, s) = (Some("limit"), Some("schema"));
    assert_eq!(clases, [None, None, l, None, l, s, l, None, l, None, l, l]);
    // Y con los límites por defecto, esos mismos payloads pasan los límites.
    let def = Opciones::default();
    let bins = [compilar_rust(&a, &def, "lim-rust-json-def")];
    igual_que_el_interprete(&a, &def, &bins, &payloads);
}

#[test]
fn limites_xml_en_rust_y_c() {
    let a = build(&ir_de(
        br#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="d" minOccurs="0" maxOccurs="unbounded"><xs:simpleType><xs:restriction base="xs:decimal"><xs:maxInclusive value="100"/></xs:restriction></xs:simpleType></xs:element>
  </xs:sequence><xs:attribute name="b" type="xs:string"/></xs:complexType></xs:element>
</xs:schema>"#,
        true,
    ))
    .unwrap();
    let op = opciones(&[("maxLexeme", 4), ("maxXmlRefs", 2), ("maxString", 6)]);
    let bins = [
        compilar_rust(&a, &op, "lim-rust-xml"),
        compilar_c(&a, &op, "lim-c-xml"),
    ];
    let payloads: Vec<&[u8]> = vec![
        b"<r><d>1.25</d></r>",
        b"<r><d>1.255</d></r>",
        b"<r b='&amp;&lt;'/>",
        b"<r b='&amp;&lt;&gt;'/>",
        b"<r b='abcdef'/>",
        b"<r b='abcdefg'/>",
    ];
    igual_que_el_interprete(&a, &op, &bins, &payloads);
    let o = Options {
        mode: Mode::Total,
        limits: op.limites,
    };
    let clases: Vec<Option<&str>> = payloads
        .iter()
        .map(|p| validate_with(&a, p, &o, 4096).error_kind.map(clase))
        .collect();
    let l = Some("limit");
    assert_eq!(clases, [None, l, None, l, None, l]);
}

/// `maxTimeMs` = 0: el tiempo se agota antes del primer bloque; `limit` en
/// el offset 0 y código 2, en los dos lenguajes. Sin `maxTimeMs` no hay
/// tiempo (el `main` no lo mira).
#[test]
fn tiempo_agotado() {
    let a = build(&ir_de(b"true", false)).unwrap();
    let op = opciones(&[("maxTimeMs", 0)]);
    for b in [
        compilar_rust(&a, &op, "lim-rust-tiempo"),
        compilar_c(&a, &op, "lim-c-tiempo"),
    ] {
        let (codigo, out, _) = ejecutar(&b, b"[1, 2, 3]");
        let v: Value = serde_json::from_str(out.trim_end()).unwrap();
        assert_eq!(v["errorKind"], "limit", "{out}");
        assert_eq!(v["offset"], 0, "{out}");
        assert_eq!(codigo, 2);
    }
    let sin = rust::emit_con(&a, &Opciones::default()).unwrap();
    let main = &sin
        .iter()
        .find(|x| x.nombre == "main.rs")
        .unwrap()
        .contenido;
    assert!(main.contains("const TIEMPO_MS: Option<u64> = None;"));
    let con = c::emit_con(&a, &op).unwrap();
    let main = &con.iter().find(|x| x.nombre == "main.c").unwrap().contenido;
    assert!(main.contains("#define TIEMPO_MS 0LL"));
    // El fin de la entrada no mira el tiempo: sin bloques, el reporte es el
    // de la entrada vacía.
    let b = compilar_c(&a, &op, "lim-c-tiempo-vacio");
    let (_, out, _) = ejecutar(&b, b"");
    let v: Value = serde_json::from_str(out.trim_end()).unwrap();
    assert_eq!(v["errorKind"], "syntax", "{out}");
    // El `main.c` con tiempo compila con las banderas estrictas.
    let dir = directorio("lim-c-tiempo-estricto");
    escribir(&dir, &con);
    compilar_con(
        &dir,
        &cc(),
        &[
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-Wshadow",
            "-Wconversion",
            "-Wno-sign-conversion",
            "-c",
            "main.c",
        ],
    );
}

/// Sin `--limit`, `emit` = `emit_con` con los de por defecto, y lo emitido
/// declara esos límites (`LIMITES`, `sv_limits_emitted`).
#[test]
fn sin_opciones_los_de_por_defecto() {
    let a = build(&ir_de(b"true", false)).unwrap();
    assert_eq!(
        rust::emit(&a),
        rust::emit_con(&a, &Opciones::default()).unwrap()
    );
    assert_eq!(c::emit(&a), c::emit_con(&a, &Opciones::default()).unwrap());
    let v = rust::emit(&a);
    let f = &v
        .iter()
        .find(|x| x.nombre == "validador.rs")
        .unwrap()
        .contenido;
    assert!(f.contains("max_depth: 256,"), "{f}");
    assert!(f.contains("max_lexeme: 1024,"), "{f}");
    let v = c::emit(&a);
    let f = &v
        .iter()
        .find(|x| x.nombre == "validador.c")
        .unwrap()
        .contenido;
    assert!(f.contains("lim->max_depth = 256u;"), "{f}");
}

#[test]
fn opciones_invalidas() {
    let a = build(&ir_de(b"true", false)).unwrap();
    let mut op = Opciones::default();
    assert!(op.fijar("maxProfundidad", 1).is_err());
    for n in NOMBRES_DE_LIMITES {
        op.fijar(n, 7).unwrap();
    }
    assert_eq!(op.limites.max_depth, 7);
    assert_eq!(op.tiempo_ms, Some(7));
    let e = opciones(&[("maxDepth", 0)]).comprobar(&a).unwrap_err();
    assert!(e.contains("maxDepth"), "{e}");
    let grande = opciones(&[("keyPool", MAX_STATE_WORDS as u64)]);
    let e = grande.comprobar(&a).unwrap_err();
    assert!(e.contains("palabras"), "{e}");
    // `emit_con` no emite lo que no pasa `comprobar`.
    assert!(rust::emit_con(&a, &grande).is_err());
    assert!(c::emit_con(&a, &grande).is_err());
    // El borde: justo MAX_STATE_WORDS pasa; una palabra más, no.
    let base = Opciones::default().comprobar(&a).unwrap() - Opciones::default().limites.key_pool;
    let justo = (MAX_STATE_WORDS - base) as u64;
    assert_eq!(
        opciones(&[("keyPool", justo)]).comprobar(&a),
        Ok(MAX_STATE_WORDS)
    );
    assert!(opciones(&[("keyPool", justo + 1)]).comprobar(&a).is_err());
}
