//! Pruebas del arnés de métricas (F6.5). Ninguna mide tiempo (D1): comprueban
//! que lo que se mide es correcto (profundidad, generadores, veredictos que
//! coinciden con la línea base y con el corpus) y el formato del informe.

use automaton::PayloadFormat;
use bench::{
    automata, bombeo, csv, profundidad, tabla_markdown, validar_dpda, validar_linea_base, Medida,
    Validador, Veredicto,
};

#[test]
fn profundidad_json_salta_las_cadenas() {
    let p = |s: &str| profundidad(s.as_bytes(), PayloadFormat::Json);
    assert_eq!(p("1"), 0);
    assert_eq!(p("[]"), 1);
    assert_eq!(p(r#"{"a":[1,{"b":[]}]}"#), 4);
    assert_eq!(p(r#"["[[[[", "\"{{", {}]"#), 2);
    assert_eq!(p(r#"[[]] [[[ ]]]"#), 3);
}

#[test]
fn profundidad_xml_salta_comentarios_cdata_y_pi() {
    let p = |s: &str| profundidad(s.as_bytes(), PayloadFormat::Xml);
    assert_eq!(p("<a/>"), 1);
    assert_eq!(p("<?xml version=\"1.0\"?><a><b><c/></b></a>"), 3);
    assert_eq!(p("<a><!-- <x><y><z> --><![CDATA[<q><r>]]><b>t</b></a>"), 2);
    assert_eq!(p("<a x=\"1\"><b/><b/></a>"), 2);
}

#[test]
fn bombeo_genera_lo_que_dice() {
    for k in [1, 7, 100, 255] {
        let g = bombeo::anidado(k);
        assert_eq!(g.payload.len(), 2 * k);
        assert_eq!(profundidad(&g.payload, PayloadFormat::Json), k);
    }
    let g = bombeo::enteros(5);
    assert_eq!(g.payload, b"[1,1,1,1,1]");
    let g = bombeo::objetos(2);
    assert_eq!(g.payload, br#"[{"a":1},{"a":1}]"#);
    assert_eq!(profundidad(&g.payload, PayloadFormat::Json), 2);
}

/// El DPDA y la línea base aceptan todas las familias de bombeo (los
/// payloads son válidos por construcción).
#[test]
fn dpda_y_linea_base_aceptan_el_bombeo() {
    let familias = [1, 10, 255]
        .into_iter()
        .map(bombeo::anidado)
        .chain([0, 1, 1000].into_iter().map(bombeo::enteros))
        .chain([0, 1, 1000].into_iter().map(bombeo::objetos));
    for g in familias {
        let m = automata(g.esquema.as_bytes()).expect(&g.nombre);
        assert_eq!(
            validar_dpda(&m, &g.payload),
            Veredicto {
                valido: true,
                violaciones: Some(0)
            },
            "{}",
            g.nombre
        );
        let lb = baseline::with_stack(|st| {
            let lb = baseline::JsonBaseline::compile_text(g.esquema.as_bytes()).unwrap();
            validar_linea_base(&lb, st, &g.payload)
        });
        assert!(lb.valido, "{}: la línea base rechaza el payload", g.nombre);
    }
}

/// Un objeto sin la propiedad requerida es una violación para los dos.
#[test]
fn una_violacion_en_el_bombeo_de_objetos() {
    let m = automata(bombeo::ESQUEMA_OBJETOS.as_bytes()).unwrap();
    let payload = br#"[{"a":1},{},{"a":2}]"#;
    let v = validar_dpda(&m, payload);
    assert!(!v.valido);
    assert_eq!(v.violaciones, Some(1));
    let lb = baseline::with_stack(|st| {
        let lb = baseline::JsonBaseline::compile_text(bombeo::ESQUEMA_OBJETOS.as_bytes()).unwrap();
        validar_linea_base(&lb, st, payload)
    });
    assert_eq!(
        lb,
        Veredicto {
            valido: false,
            violaciones: Some(1)
        }
    );
}

/// En cada caso del corpus con DPDA construible, el DPDA da el `valid`
/// esperado, y en JSON coincide con la línea base.
#[test]
fn veredictos_del_corpus() {
    let casos = baseline::with_stack(|_| baseline::corpus::casos()).expect("corpus");
    let mut con_dpda = 0;
    for c in &casos {
        let baseline::corpus::Payload::Bytes(payload) = &c.payload else {
            continue;
        };
        let Ok(m) = automata(&c.schema_text) else {
            continue; // reglas aún pendientes en M (F4.4b/F4.5)
        };
        con_dpda += 1;
        let d = validar_dpda(&m, payload);
        assert_eq!(d.valido, c.expected.valid, "{}: DPDA", c.id);
        if c.format == baseline::corpus::Format::Json {
            let lb = baseline::with_stack(|st| {
                let lb = baseline::JsonBaseline::compile_text(&c.schema_text).unwrap();
                validar_linea_base(&lb, st, payload)
            });
            assert_eq!(lb.valido, d.valido, "{}: DPDA frente a línea base", c.id);
        }
    }
    assert!(con_dpda >= 5, "solo {con_dpda} casos con DPDA");
}

#[test]
fn esquema_invalido_da_su_error() {
    let Err(e) = automata(br#"{"type": 5}"#) else {
        panic!("debe fallar");
    };
    assert!(e.contains("type"), "{e}");
}

#[test]
fn informe_en_markdown_y_csv() {
    let m = Medida {
        caso: "a|b".into(),
        validador: Validador::Dpda,
        bytes: 1_000_000,
        profundidad: 2,
        veredicto: Veredicto {
            valido: false,
            violaciones: Some(3),
        },
        segundos: 0.01,
        asignaciones: 1,
        pico_bytes: 2048,
        estados: Some(10),
        transiciones: Some(20),
    };
    assert!((m.mb_s() - 100.0).abs() < 1e-9);
    assert!((m.ns_por_byte() - 10.0).abs() < 1e-9);
    let t = tabla_markdown(std::slice::from_ref(&m));
    assert!(t.lines().count() == 3, "{t}");
    assert!(t.contains("| a\\|b | DPDA | 1000000 | 2 | 3 violaciones | 100.0 | 10.00 | 1 | 2.0 KiB | 10 | 20 |"), "{t}");
    let c = csv(&[m]);
    assert!(c.starts_with("caso,validador,"));
    assert!(
        c.lines()
            .nth(1)
            .unwrap()
            .starts_with("\"a|b\",DPDA,1000000,2,false,3,"),
        "{c}"
    );
}
