//! La línea base JSON recorre el corpus (`tests/cases`) y da el resultado de
//! `expected` en cada caso JSON con bytes (U8: el corpus como oráculo
//! certificado para F4.6).
//!
//! Se comparan siempre `valid` y la clase del error (U7). El número de
//! violaciones se compara salvo en las entradas de [`DIVERGENCIAS`], y una
//! entrada que ya no diverge también es un fallo, para que la tabla no
//! envejezca. El `pointer` esperado debe estar entre los de la línea base
//! (B10: `jsonschema` los da en su orden), y el offset de un error léxico o
//! sintáctico debe ser exacto.

use baseline::corpus::{self, Case, ErrorKind, Format, Payload};
use baseline::{run_case, with_stack, Outcome, Unavailable};

/// Casos cuyo resultado en la línea base difiere a propósito del de
/// `expected` (el del DPDA): (`_id`, campo, motivo). Campos admitidos:
/// `violations` (el recuento) y `errorKind` (la clase del error, solo con el
/// mismo `valid`; añadido en F4.6). Las diferencias de recuento conocidas
/// están en `docs/fase6/linea-base.md` §4.
const DIVERGENCIAS: &[(&str, &str, &str)] = &[(
    "extreme-big-number-01",
    "errorKind",
    "el DPDA guarda como mucho 1 024 bytes del lexema (`docs/00-alcance.md` §4) y da \
     `limit` cuando una comprobación lo lee; `serde_json` no representa 10¹⁰²⁴ en `f64` \
     y la línea base da `Unsupported` (D50, B19)",
)];

/// Diferencias entre el resultado de la línea base y `expected` (vacío =
/// coincide), con el archivo, el campo, lo esperado y lo obtenido (B11).
fn comparar(case: &Case, o: &Outcome, divergencias: &[(&str, &str, &str)]) -> Vec<String> {
    let x = &case.expected;
    let quien = case.path.display();
    let mut e = Vec::new();
    if o.valid != x.valid {
        e.push(format!(
            "{quien}: `valid` esperado {}, obtenido {} ({o:?})",
            x.valid, o.valid
        ));
    }
    let clase = x.error_kind.map(ErrorKind::kind);
    let div_clase = divergencias
        .iter()
        .find(|(id, campo, _)| *id == case.id && *campo == "errorKind");
    if let Some((_, _, motivo)) = div_clase {
        if o.kind == clase {
            e.push(format!(
                "{quien}: está en DIVERGENCIAS («{motivo}») pero la clase ya no diverge: \
                 quitar la entrada"
            ));
        }
    } else if o.kind != clase {
        e.push(format!(
            "{quien}: `errorKind` esperado {:?} (clase {clase:?}), obtenido {:?}",
            x.error_kind, o.kind
        ));
    }
    if o.truncated {
        e.push(format!(
            "{quien}: la lista de violaciones se cortó en {} (`Limits::max_violations`): \
             el recuento no es comparable",
            o.violation_count()
        ));
    }
    let divergencia = divergencias
        .iter()
        .find(|(id, campo, _)| *id == case.id && *campo == "violations");
    match (x.violations, divergencia) {
        (Some(n), None) if n != o.violation_count() as u64 => e.push(format!(
            "{quien}: `violations` esperado {n}, obtenido {} ({:?}); si es una \
             divergencia de recuento, va en DIVERGENCIAS con su motivo",
            o.violation_count(),
            o.violations
        )),
        (Some(n), Some((_, _, motivo))) if n == o.violation_count() as u64 => e.push(format!(
            "{quien}: está en DIVERGENCIAS («{motivo}») pero ya no diverge \
             (`violations` = {n} en los dos): quitar la entrada"
        )),
        (None, Some(_)) => e.push(format!(
            "{quien}: está en DIVERGENCIAS pero no declara `expected.violations`"
        )),
        _ => {}
    }
    if let Some(p) = &x.pointer {
        if !o.violations.iter().any(|v| &v.pointer == p) {
            let obtenidos: Vec<&str> = o.violations.iter().map(|v| v.pointer.as_str()).collect();
            e.push(format!(
                "{quien}: `pointer` esperado {p:?}, no está entre los obtenidos {obtenidos:?}"
            ));
        }
    }
    if matches!(x.error_kind, Some(ErrorKind::Lexical | ErrorKind::Syntax)) {
        if let Some(off) = x.offset {
            let obtenido = o.malformed.as_ref().and_then(|m| m.offset);
            if obtenido != Some(off) {
                e.push(format!(
                    "{quien}: `offset` esperado {off}, obtenido {obtenido:?} ({:?})",
                    o.malformed
                ));
            }
        }
    }
    e
}

#[test]
fn la_linea_base_recorre_el_corpus_json() {
    for (id, campo, motivo) in DIVERGENCIAS {
        assert!(
            matches!(*campo, "violations" | "errorKind"),
            "{id}: campo de DIVERGENCIAS no admitido"
        );
        assert!(!motivo.trim().is_empty(), "{id}: DIVERGENCIAS sin motivo");
    }
    let casos = corpus::casos().unwrap_or_else(|e| panic!("{e}"));
    let json: Vec<&Case> = casos.iter().filter(|c| c.format == Format::Json).collect();
    for (id, _, _) in DIVERGENCIAS {
        assert!(
            json.iter()
                .any(|c| c.id == *id && matches!(c.payload, Payload::Bytes(_))),
            "{id}: está en DIVERGENCIAS pero no es un caso JSON con bytes del corpus"
        );
    }

    let (corridos, fallos) = with_stack(|s| {
        let mut corridos = 0;
        let mut fallos = Vec::new();
        for case in &json {
            match run_case(case, s) {
                Ok(o) => {
                    corridos += 1;
                    fallos.extend(comparar(case, &o, DIVERGENCIAS));
                }
                Err(Unavailable::Generator(g)) => eprintln!(
                    "omitido {}: lo produce el generador `{g}`, aún sin implementar",
                    case.path.display()
                ),
                Err(e) => fallos.push(format!("{}: {e}", case.path.display())),
            }
        }
        (corridos, fallos)
    });

    assert!(
        fallos.is_empty(),
        "la línea base no da `expected`:\n  - {}",
        fallos.join("\n  - ")
    );
    assert!(
        corridos >= 8,
        "solo corrieron {corridos} casos JSON (se esperan ≥ 8): ¿se leyó el corpus?"
    );
}

/// La tabla [`DIVERGENCIAS`] no puede envejecer: una entrada cuyo caso ya
/// coincide con `expected` hace fallar la comparación (U7).
#[test]
fn una_divergencia_que_ya_no_diverge_es_un_fallo() {
    let casos = corpus::casos().unwrap_or_else(|e| panic!("{e}"));
    let case = casos
        .iter()
        .find(|c| c.id == "invalid-missing-required-01")
        .expect("el corpus no tiene invalid-missing-required-01");
    let o = with_stack(|s| run_case(case, s)).unwrap();
    assert!(comparar(case, &o, &[]).is_empty());
    let tabla = [("invalid-missing-required-01", "violations", "prueba")];
    let fallos = comparar(case, &o, &tabla);
    assert!(
        fallos.len() == 1 && fallos[0].contains("ya no diverge"),
        "{fallos:?}"
    );
}

/// Sin la feature `libxml2`, los casos XML no se ejecutan: `run_case` lo
/// dice con un error, nunca con un pánico. Con la feature, los casos XML los
/// prueba `tests/corpus_xml.rs`.
#[cfg(not(feature = "libxml2"))]
#[test]
fn sin_libxml2_un_caso_xml_no_esta_disponible() {
    let casos = corpus::casos().unwrap_or_else(|e| panic!("{e}"));
    let xml = casos
        .iter()
        .find(|c| c.format == Format::Xml && c.schema_text.starts_with(b"<xs:schema"))
        .expect("el corpus no tiene casos XSD");
    assert!(xml.schema_text.starts_with(b"<xs:schema"));
    assert_eq!(
        with_stack(|s| run_case(xml, s)),
        Err(Unavailable::XmlDisabled)
    );
}

/// El esquema de un caso JSON es el texto original del archivo (D20), no
/// una reserialización: conserva el orden de las palabras clave.
#[test]
fn el_esquema_de_un_caso_es_su_texto_original() {
    let casos = corpus::casos().unwrap_or_else(|e| panic!("{e}"));
    let case = casos
        .iter()
        .find(|c| c.id == "non-dpda-pattern-01")
        .expect("el corpus no tiene non-dpda-pattern-01");
    assert_eq!(
        case.schema_text,
        br#"{ "type": "string", "pattern": "^[a-z]+[0-9]*$" }"#
    );
    assert_eq!(case.schema_value["pattern"], "^[a-z]+[0-9]*$");
}

// --- Rutas de error de `corpus.rs` (Tester 4), sobre una carpeta temporal ---

/// Escribe `archivos` (ruta relativa, contenido) en una carpeta temporal
/// propia de la prueba, lee el corpus de ahí con `casos_en` y borra la
/// carpeta.
fn corpus_temporal(
    nombre: &str,
    archivos: &[(&str, &str)],
) -> Result<Vec<Case>, corpus::CorpusError> {
    let raiz =
        std::env::temp_dir().join(format!("baseline-corpus-{}-{nombre}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    for (ruta, texto) in archivos {
        let p = raiz.join(ruta);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, texto).unwrap();
    }
    let r = corpus::casos_en(&raiz);
    std::fs::remove_dir_all(&raiz).unwrap();
    r
}

/// Un caso `valid` con los campos de payload y esquema dados.
fn caso(id: &str, resto: &str) -> String {
    format!(
        r#"{{"_id": "{id}", "category": "valid", "format": "json",
            "description": "prueba", {resto},
            "expected": {{"valid": true, "violations": 0}}}}"#
    )
}

#[test]
fn payload_base64_mal_formado_es_un_error_del_corpus() {
    let c = caso(
        "valid-b64-01",
        r#""schema": true, "payloadBase64": "@@no es base64@@""#,
    );
    let e = corpus_temporal("b64", &[("valid/valid-b64-01.json", &c)]).unwrap_err();
    assert!(e.message.contains("payloadBase64"), "{e}");
}

#[test]
fn rutas_de_archivo_fuera_de_la_carpeta_del_caso_se_rechazan() {
    // `..\x.json` escrito en JSON: la barra invertida (92) va doblada.
    let barra = char::from(92);
    let invertida = format!("..{barra}{barra}x.json");
    let rutas = [
        "../x.json",
        "/etc/x.json",
        "C:/x.json",
        "./x.json",
        "",
        "a:flujo.txt",
        &invertida,
    ];
    for (i, ruta) in rutas.into_iter().enumerate() {
        for campo in ["schemaFile", "payloadFile"] {
            let resto = if campo == "schemaFile" {
                format!(r#""schemaFile": "{ruta}", "payload": "1""#)
            } else {
                format!(r#""schema": true, "payloadFile": "{ruta}""#)
            };
            let c = caso("valid-ruta-01", &resto);
            let e = corpus_temporal(
                &format!("ruta-{campo}-{i}"),
                &[("valid/valid-ruta-01.json", &c)],
            )
            .unwrap_err();
            assert!(e.message.contains("ruta relativa"), "{campo} {ruta:?}: {e}");
        }
    }
}

#[test]
fn rutas_de_archivo_en_una_subcarpeta_se_leen() {
    let c = caso(
        "valid-archivos-01",
        r#""schemaFile": "archivos/e.json", "payloadFile": "archivos/p.txt""#,
    );
    let casos = corpus_temporal(
        "archivos",
        &[
            ("valid/valid-archivos-01.json", &c),
            ("valid/archivos/e.json", r#"{"type": "integer"}"#),
            ("valid/archivos/p.txt", "7"),
        ],
    )
    .unwrap();
    assert_eq!(casos.len(), 1);
    assert_eq!(casos[0].schema_text, br#"{"type": "integer"}"#);
    assert_eq!(casos[0].payload, Payload::Bytes(b"7".to_vec()));
    assert!(with_stack(|s| run_case(&casos[0], s)).unwrap().valid);
}

#[test]
fn run_case_sin_resultado_por_generador_limites_o_esquema() {
    let casos = corpus_temporal(
        "no-disponible",
        &[
            (
                "valid/valid-gen-01.json",
                &caso(
                    "valid-gen-01",
                    r#""schema": true, "generator": {"name": "bombeo", "params": {"k": 3}}"#,
                ),
            ),
            (
                "valid/valid-lim-01.json",
                &caso(
                    "valid-lim-01",
                    r#""schema": true, "payload": "1", "limits": {}"#,
                ),
            ),
            (
                "valid/valid-patron-01.json",
                &caso(
                    "valid-patron-01",
                    r#""schema": {"pattern": "(?=a)"}, "payload": "\"a\"""#,
                ),
            ),
        ],
    )
    .unwrap();
    let r: Vec<_> = with_stack(|s| casos.iter().map(|c| run_case(c, s)).collect());
    assert_eq!(r[0], Err(Unavailable::Generator("bombeo".into())));
    assert!(matches!(&casos[0].payload, Payload::Generator { params, .. } if params["k"] == 3));
    assert_eq!(r[1], Err(Unavailable::CaseLimits));
    assert!(matches!(&r[2], Err(Unavailable::Compile(_))), "{:?}", r[2]);
}
