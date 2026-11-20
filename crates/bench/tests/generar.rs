//! Pruebas del generador de payloads válidos (F5.4). Deterministas (D1): la
//! misma semilla da el mismo payload, y todo lo generado lo aceptan el DPDA y
//! la línea base.

use bench::generar::{cadena_json, generar, generar_valido, Azar};
use bench::{ir_y_automata, validar_dpda};

const ESQUEMAS: &[&str] = &[
    r#"{"type":"object","properties":{"id":{"type":"integer","minimum":1},"sku":{"type":"string","pattern":"^[A-Z]{3}-[0-9]{3}$"},"total":{"type":"number","minimum":0,"multipleOf":0.01}},"required":["id","sku"],"additionalProperties":false}"#,
    r#"{"type":"array","minItems":3,"maxItems":6,"uniqueItems":true,"items":{"type":"integer","minimum":0,"maximum":100}}"#,
    r#"{"type":"array","prefixItems":[{"type":"string"},{"type":"boolean"}],"items":false}"#,
    r#"{"enum":["a","b",3]}"#,
    r#"{"type":"object","properties":{"k":{"const":"fijo"},"n":{"const":-2.5}},"required":["k","n"],"additionalProperties":false}"#,
    r#"{"type":"object","properties":{"t":{"type":"string"},"c":{"type":"integer"}},"dependentRequired":{"t":["c"]},"minProperties":1}"#,
    r#"{"oneOf":[{"type":"string","maxLength":3},{"type":"integer","exclusiveMinimum":0}]}"#,
    r##"{"$defs":{"n":{"type":"object","properties":{"v":{"type":"integer"},"h":{"type":"array","items":{"$ref":"#/$defs/n"}}},"required":["v"]}},"$ref":"#/$defs/n"}"##,
];

fn linea_base(esquema: &str) -> baseline::JsonBaseline {
    baseline::with_stack(|_| baseline::JsonBaseline::compile_text(esquema.as_bytes()).ok())
        .expect("la línea base compila el esquema")
}

#[test]
fn misma_semilla_mismo_payload() {
    for e in ESQUEMAS {
        let (ir, _) = ir_y_automata(e.as_bytes()).expect("tiene M");
        for s in 0..5 {
            assert_eq!(generar(&ir, s), generar(&ir, s), "esquema {e}, semilla {s}");
        }
    }
}

#[test]
fn lo_generado_es_valido_para_el_dpda_y_la_linea_base() {
    for e in ESQUEMAS {
        let (ir, m) = ir_y_automata(e.as_bytes()).expect("tiene M");
        let lb = linea_base(e);
        for s in [1, 2, 3, 10, 100] {
            let (p, _) = generar_valido(&ir, &m, s, 500, |p| {
                baseline::with_stack(|st| lb.validate(st, p).valid)
            })
            .unwrap_or_else(|| panic!("sin payload válido para {e} con la semilla {s}"));
            assert!(validar_dpda(&m, p.as_bytes()).valido, "{e}: {p}");
            assert!(
                baseline::with_stack(|st| lb.validate(st, p.as_bytes()).valid),
                "{e}: {p}"
            );
        }
    }
}

#[test]
fn el_patron_se_cumple_por_construccion() {
    let e = r#"{"type":"string","pattern":"^[A-Z]{3}-[0-9]{3}$"}"#;
    let (ir, _) = ir_y_automata(e.as_bytes()).expect("tiene M");
    for s in 0..20 {
        let p = generar(&ir, s).expect("genera");
        let b = p.as_bytes();
        assert_eq!(b.len(), 9, "{p}");
        assert!(
            b[1..4].iter().all(u8::is_ascii_uppercase) && b[4] == b'-',
            "{p}"
        );
        assert!(b[5..8].iter().all(u8::is_ascii_digit), "{p}");
    }
}

#[test]
fn const_se_copia_tal_cual() {
    let e = r#"{"type":"object","properties":{"k":{"const":"fijo"},"n":{"const":-2.5}},"required":["k","n"],"additionalProperties":false}"#;
    let (ir, _) = ir_y_automata(e.as_bytes()).expect("tiene M");
    assert_eq!(generar(&ir, 7).as_deref(), Some(r#"{"k":"fijo","n":-2.5}"#));
}

#[test]
fn cadena_json_escapa() {
    assert_eq!(cadena_json("a\"b\\c\n\u{1}"), r#""a\"b\\c\u000a\u0001""#);
    assert_eq!(cadena_json("ñ«»"), "\"ñ«»\"");
}

#[test]
fn azar_es_reproducible_y_respeta_el_rango() {
    let (mut a, mut b) = (Azar::new(42), Azar::new(42));
    for _ in 0..1000 {
        let x = a.rango(3, 9);
        assert_eq!(x, b.rango(3, 9));
        assert!((3..=9).contains(&x));
    }
}

#[test]
fn xsd_no_se_genera() {
    let x = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="a" type="xs:string"/></xs:schema>"#;
    let (ir, _) = ir_y_automata(x.as_bytes()).expect("tiene M");
    assert_eq!(generar(&ir, 1), None);
}
