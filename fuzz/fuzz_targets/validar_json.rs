//! Payloads JSON arbitrarios contra esquemas fijos que cubren todas las
//! estrategias (F6.4): las propiedades de [`comun::propiedades`] y, como
//! oráculo, la línea base DOM (`serde_json` + `jsonschema`, D48): el mismo
//! veredicto (válido, malformado, violación, límite), el mismo número de
//! violaciones donde no hay divergencias declaradas y un puntero que está
//! entre los suyos o debajo de uno (F6.1).
#![no_main]

use std::sync::OnceLock;

use automaton::{Automaton, ErrorKind};
use baseline::{with_stack, JsonBaseline, Kind};
use libfuzzer_sys::fuzz_target;

#[path = "../comun/mod.rs"]
mod comun;

/// Los esquemas: anidados con `$ref` recursivo, números, cadenas con
/// `pattern`, combinadores, `uniqueItems`, dependencias y objetos cerrados.
const ESQUEMAS: &[&str] = &[
    r##"{"$ref":"#/$defs/v","$defs":{"v":{"minimum":0,"maxLength":5,
        "items":{"$ref":"#/$defs/v"},"additionalProperties":{"$ref":"#/$defs/v"}}}}"##,
    r#"{"type":"object","properties":{"id":{"type":"integer","minimum":1,"multipleOf":3},
        "nombre":{"type":"string","pattern":"^[a-z]+(-[a-z]+)*$","minLength":2},
        "tags":{"type":"array","items":{"enum":["a","b",1,null]},"uniqueItems":true,"maxItems":4}},
        "required":["id"],"additionalProperties":false,
        "dependentRequired":{"nombre":["tags"]}}"#,
    r#"{"anyOf":[{"type":"string"},{"type":"array","prefixItems":[{"type":"number"},{"type":"boolean"}],"items":false}],
        "not":{"const":"x"}}"#,
    r#"{"oneOf":[{"type":"object","required":["a"]},{"type":"object","required":["b"]}],
        "properties":{"a":{"exclusiveMaximum":1.5},"b":{"type":"null"}}}"#,
    r#"{"allOf":[{"type":["object","array"]},{"minProperties":1}],"maxProperties":3,
        "items":{"type":"string","maxLength":2}}"#,
];

struct Par {
    m: Automaton,
    base: JsonBaseline,
}

fn pares() -> &'static [Par] {
    static P: OnceLock<Vec<Par>> = OnceLock::new();
    P.get_or_init(|| {
        ESQUEMAS
            .iter()
            .map(|s| Par {
                m: comun::m(comun::en_hilo(|| comun::ir_json(s.as_bytes()))),
                base: JsonBaseline::compile_text(s.as_bytes()).expect("línea base"),
            })
            .collect()
    })
}

/// Lo que el DPDA no puede comparar con la línea base: números que no caben
/// en un `f64` (`Unsupported`, D50 B19), lexemas de más de 1 024 bytes y
/// límites (profundidad, cadenas) que la línea base aplica de otra forma.
fn comparable(r: &automaton::Report, k: Option<Kind>) -> bool {
    r.error_kind != Some(ErrorKind::Limit) && !matches!(k, Some(Kind::Limit | Kind::Unsupported))
}

fuzz_target!(|p: &[u8]| {
    for par in pares() {
        let r = comun::en_hilo(|| comun::propiedades(&par.m, p));
        let o = with_stack(|s| par.base.validate(s, p));
        if !comparable(&r, o.kind) {
            continue;
        }
        let esperado = match r.error_kind {
            None => None,
            Some(ErrorKind::Lexical | ErrorKind::Syntax) => Some(Kind::Malformed),
            Some(ErrorKind::Schema) => Some(Kind::Schema),
            Some(_) => unreachable!(),
        };
        assert_eq!(o.kind, esperado, "DPDA {r:?} / línea base {o:?}");
        if r.error_kind == Some(ErrorKind::Schema) {
            let ptr = r.pointer.as_str();
            let bajo =
                |q: &str| ptr == q || ptr.strip_prefix(q).is_some_and(|x| x.starts_with('/'));
            assert!(
                r.pointer.truncated() || o.violations.iter().any(|v| bajo(&v.pointer)),
                "puntero {ptr:?} / {:?}",
                o.violations.iter().map(|v| &v.pointer).collect::<Vec<_>>()
            );
        }
    }
});
