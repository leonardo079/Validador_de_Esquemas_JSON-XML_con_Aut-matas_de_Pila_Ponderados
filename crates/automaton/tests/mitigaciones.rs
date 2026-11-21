//! F6.4: las mitigaciones de `docs/fase6/mitigaciones.md`, de punta a punta
//! (esquema → M → validación) y en un solo sitio: DTD y entidades (D8),
//! `pattern` como AFD (sin vuelta atrás: ReDoS imposible), `$ref` cíclico,
//! anidamiento y números enormes. Las pruebas no miden tiempo (D1): lo que
//! acota el costo es la forma (un AFD, una pasada, límites), y eso es lo
//! que se comprueba.

use automaton::{build, validate, Automaton, BuildError, ErrorKind, Mode, Report};
use runtime_rs::Weight;

fn en_hilo<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn_scoped(s, f)
            .unwrap()
            .join()
            .unwrap()
    })
}

/// M de un JSON Schema, o el error de la etapa que lo rechaza.
fn json(src: &str) -> Result<Automaton, String> {
    en_hilo(|| {
        let s = schema_parser::parse(src.as_bytes()).map_err(|e| e.to_string())?;
        let t = semantic::analyze(&s).map_err(|e| format!("{e:?}"))?;
        let ir = semantic::lower(&t).map_err(|e| e.to_string())?;
        build(&ir).map_err(|e| match e {
            BuildError::Limit { .. } => format!("limit: {e}"),
            _ => e.to_string(),
        })
    })
}

fn xsd(cuerpo: &str) -> Result<Automaton, String> {
    let src =
        format!("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>{cuerpo}</xs:schema>");
    xsd_texto(&src)
}

fn xsd_texto(src: &str) -> Result<Automaton, String> {
    en_hilo(|| {
        let s = xsd::parse(src.as_bytes()).map_err(|e| e.to_string())?;
        let a = xsd::analyze(&s).map_err(|e| format!("{e:?}"))?;
        build(&xsd::lower(&a).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    })
}

fn correr(a: &Automaton, p: &[u8]) -> Report {
    let r = validate(a, p, Mode::Total, 4096);
    assert_eq!(validate(a, p, Mode::Total, 7), r);
    r
}

#[track_caller]
fn infinito(r: &Report, kind: ErrorKind) {
    assert_eq!(r.violations, Weight::INFINITY, "{r:?}");
    assert_eq!(r.error_kind, Some(kind), "{r:?}");
}

/// D8: ningún DTD llega a M, ni en el payload ni en el esquema: el
/// lexer XML lo rechaza en `<!DOCTYPE` antes de leer declaraciones, así que
/// no hay expansión de entidades (billion laughs), entidades externas (XXE)
/// ni entidades de parámetro. Una referencia a una entidad no predefinida
/// es un error léxico; las de carácter deben dar un carácter XML.
#[test]
fn dtd_y_entidades() {
    let a = xsd("<xs:element name='r' type='xs:string'/>").unwrap();
    for (p, off) in [
        (&b"<!DOCTYPE r [<!ENTITY a 'x'>]><r>&a;</r>"[..], 2),
        (b"<!DOCTYPE r SYSTEM 'file:///etc/passwd'><r/>", 2),
        (
            b"<!DOCTYPE r [<!ENTITY % p SYSTEM 'http://x/'> %p;]><r/>",
            2,
        ),
        (b"<?xml version='1.0'?><!DOCTYPE r><r/>", 23),
    ] {
        let r = correr(&a, p);
        infinito(&r, ErrorKind::Lexical);
        assert_eq!(
            r.first.unwrap().offset,
            off,
            "{}",
            String::from_utf8_lossy(p)
        );
    }
    for p in [
        &b"<r>&xxe;</r>"[..],
        b"<r>&#0;</r>",
        b"<r>&#xD800;</r>",
        b"<r>&lol</r>",
    ] {
        infinito(&correr(&a, p), ErrorKind::Lexical);
    }
    assert!(correr(&a, b"<r>&lt;&#x41;&amp;</r>").valid);
    // El esquema XSD tampoco admite un DTD.
    let e = xsd_texto(
        "<!DOCTYPE xs:schema [<!ENTITY t 'xs:string'>]>\
         <xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'/>",
    )
    .unwrap_err();
    assert!(!e.is_empty());
}

/// `pattern` se compila a un AFD mínimo dentro de M (F4.4b): validar una
/// cadena es un paso de δ por carácter, sin vuelta atrás. Los patrones de
/// ReDoS clásicos dan AFD de pocos estados y la cadena atacante se valida
/// en una pasada con una violación. Lo que no se puede determinizar dentro
/// del presupuesto (F4-U16) es un error de compilación `limit`, nunca un
/// AFD enorme ni una espera. Las construcciones con vuelta atrás
/// (retrorreferencias, *lookaround*) se rechazan en la Fase 3.
#[test]
fn pattern_como_afd() {
    let ataque = format!("\"{}!\"", "a".repeat(100_000));
    for p in [
        "^(a+)+$",
        "^(a|a)*$",
        "^(a|aa)+$",
        "^(a*)*b$",
        "^([a-z]+)*[0-9]$",
    ] {
        let esquema = format!(r#"{{"type":"string","pattern":"{p}"}}"#);
        let a = json(&esquema).unwrap_or_else(|e| panic!("{p}: {e}"));
        let s = a.summary();
        assert!(s.states <= 40, "{p}: {s:?}");
        let r = correr(&a, ataque.as_bytes());
        assert_eq!(r.violations.get(), Some(1), "{p}: {r:?}");
    }
    // Explosión exponencial al determinizar: `limit` al compilar.
    let e = json(r#"{"type":"string","pattern":"(a|b)*a(a|b){24}"}"#).unwrap_err();
    assert!(e.contains("limit") || e.contains("límite"), "{e}");
    for p in [r"(a)\\1", "(?=a)a", "(?<!a)b", "a+?"] {
        let esquema = format!(r#"{{"type":"string","pattern":"{p}"}}"#);
        assert!(json(&esquema).is_err(), "{p}");
    }
}

/// `$ref`: un ciclo que no consume entrada (`{"$ref":"#"}`, dos
/// definiciones que se nombran entre sí, `allOf`/`not` sobre sí mismo) se
/// rechaza en la Fase 3; uno productivo (a través de `properties` o
/// `items`) compila a un marco recursivo de M, y la pila la acota
/// `max_depth` con `limit`, nunca un desbordamiento.
#[test]
fn ref_ciclico() {
    for e in [
        r##"{"$ref":"#"}"##,
        r##"{"$defs":{"a":{"$ref":"#/$defs/b"},"b":{"$ref":"#/$defs/a"}},"$ref":"#/$defs/a"}"##,
        r##"{"allOf":[{"$ref":"#"}]}"##,
        r##"{"not":{"$ref":"#"}}"##,
    ] {
        assert!(json(e).is_err(), "{e}");
    }
    let a = json(r##"{"type":"object","properties":{"h":{"$ref":"#"}}}"##).unwrap();
    let ok = format!("{}{{}}{}", r#"{"h":"#.repeat(255), "}".repeat(255));
    assert!(correr(&a, ok.as_bytes()).valid);
    let hondo = format!("{}{}", r#"{"h":"#.repeat(100_000), "{}");
    let r = correr(&a, hondo.as_bytes());
    infinito(&r, ErrorKind::Limit);
    assert_eq!(r.first.unwrap().offset, 256 * 5);
    let a = json(r##"{"items":{"$ref":"#"}}"##).unwrap();
    infinito(
        &correr(&a, "[".repeat(1 << 20).as_bytes()),
        ErrorKind::Limit,
    );
}

/// Números: el lexema de más de 1 024 bytes que tiene que leer una
/// comprobación numérica es `limit` (el costo de comparar queda acotado);
/// sin comprobación numérica no se guarda. Un exponente enorme no se
/// expande.
#[test]
fn numeros_enormes() {
    let a = json(r#"{"type":"array","items":{"maximum":10,"multipleOf":0.5}}"#).unwrap();
    let r = correr(&a, format!("[{}]", "9".repeat(5000)).as_bytes());
    infinito(&r, ErrorKind::Limit);
    let r = correr(&a, b"[1e999999999999999999, 2.5e-999999999999999999, 3]");
    assert_eq!(r.violations.get(), Some(2), "{r:?}");
    let t = json("true").unwrap();
    assert!(correr(&t, format!("[{}]", "9".repeat(100_000)).as_bytes()).valid);
}
