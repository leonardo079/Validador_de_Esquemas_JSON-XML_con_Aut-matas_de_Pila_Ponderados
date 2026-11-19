//! La línea base XML (libxml2, feature `libxml2`) recorre el corpus
//! (`tests/cases`) y da el resultado de `expected` en cada caso XML con bytes
//! (U8). Solo se compila con la feature: `make test-baseline-xml`
//! (`docs/fase6/linea-base.md` §7).
//!
//! Se comparan `valid`, la clase del error, `violations` (sin tabla de
//! divergencias: hoy no diverge ninguno) y el offset de los errores léxicos
//! y sintácticos. El `pointer` no: libxml2 no da la ruta del nodo (§7.4).
#![cfg(feature = "libxml2")]

use baseline::corpus::{self, Case, ErrorKind, Format};
use baseline::{run_case, with_stack, Outcome, Unavailable};

/// Diferencias entre el resultado y `expected` (vacío = coincide), con el
/// archivo, el campo, lo esperado y lo obtenido (B11).
fn comparar(case: &Case, o: &Outcome) -> Vec<String> {
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
    if o.kind != clase {
        e.push(format!(
            "{quien}: `errorKind` esperado {:?} (clase {clase:?}), obtenido {:?} ({o:?})",
            x.error_kind, o.kind
        ));
    }
    if o.truncated {
        e.push(format!("{quien}: la lista de violaciones se cortó"));
    }
    if let Some(n) = x.violations {
        if n != o.violation_count() as u64 {
            e.push(format!(
                "{quien}: `violations` esperado {n}, obtenido {} ({:?})",
                o.violation_count(),
                o.violations
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
fn la_linea_base_recorre_el_corpus_xml() {
    let casos = corpus::casos().unwrap_or_else(|e| panic!("{e}"));
    let xml: Vec<&Case> = casos.iter().filter(|c| c.format == Format::Xml).collect();
    let (corridos, fallos) = with_stack(|s| {
        let mut corridos = 0;
        let mut fallos = Vec::new();
        for case in &xml {
            match run_case(case, s) {
                Ok(o) => {
                    corridos += 1;
                    fallos.extend(comparar(case, &o));
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
        "la línea base XML no da `expected`:\n  - {}",
        fallos.join("\n  - ")
    );
    assert!(
        corridos >= 2,
        "solo corrieron {corridos} casos XML (se esperan ≥ 2): ¿se leyó el corpus?"
    );
}

/// Los dos casos XML de hoy, uno por uno: el pedido es válido y el
/// *billion laughs* es malformado en la `D` de `<!DOCTYPE` (D8, U6), como
/// en el lexer del proyecto.
#[test]
fn pedido_valido_y_billion_laughs_malformado() {
    let casos = corpus::casos().unwrap_or_else(|e| panic!("{e}"));
    let caso = |id: &str| {
        casos
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("el corpus no tiene {id}"))
    };
    let (pedido, risas) = with_stack(|s| {
        (
            run_case(caso("valid-xml-pedido-01"), s),
            run_case(caso("malformed-xml-billion-laughs-01"), s),
        )
    });
    let pedido = pedido.unwrap();
    assert!(pedido.valid, "{pedido:?}");
    let risas = risas.unwrap();
    assert_eq!(risas.kind, Some(baseline::Kind::Malformed), "{risas:?}");
    let m = risas.malformed.unwrap();
    assert_eq!((m.line, m.column, m.offset), (2, 3, Some(24)), "{m:?}");
    assert!(m.message.contains("D8"), "{}", m.message);
}
