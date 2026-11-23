//! F5.4-P1: el generador de documentos XML (`bench::generar_xml`) frente al
//! DPDA en todos los XSD del repositorio y, con la feature `libxml2` (en
//! Docker, `docs/fase5/generador.md` §6), frente a la línea base libxml2.
//!
//! El generador sigue la semántica del XSD, no el autómata: si el DPDA
//! rechaza un documento que el generador construyó como válido, uno de los
//! dos está mal. Deterministas con la semilla (D1).

use std::path::Path;

use bench::generar::Azar;
use bench::generar_xml::{clase_dpda, generar_xml, generar_xml_invalido, malformar, Malformacion};
use bench::ir_y_automata;

const TESTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");
const SEMILLAS: u64 = 40;

/// Los XSD de `tests/schemas/xsd/parse-ok` que tienen M.
fn esquemas() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> =
        std::fs::read_dir(Path::new(TESTS).join("schemas/xsd/parse-ok"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "xsd"))
            .map(|p| {
                (
                    p.file_stem().unwrap().to_string_lossy().into_owned(),
                    std::fs::read(&p).unwrap(),
                )
            })
            .filter(|(_, x)| ir_y_automata(x).is_ok())
            .collect();
    v.sort();
    v
}

/// Todo documento que el generador construye como válido lo acepta el DPDA,
/// y casi todos los esquemas dan alguno (los que no: sin elementos globales
/// o solo con un elemento cuyo tipo no admite ningún texto que se sepa
/// generar).
#[test]
fn validos_los_acepta_el_dpda() {
    let mut con_documentos = 0;
    for (nombre, xsd) in esquemas() {
        let (ir, m) = ir_y_automata(&xsd).unwrap();
        let mut generados = 0;
        for s in 0..SEMILLAS {
            let Some(d) = generar_xml(&ir, s) else {
                continue;
            };
            generados += 1;
            assert_eq!(
                clase_dpda(&m, d.as_bytes()),
                None,
                "{nombre}, semilla {s}:\n{d}"
            );
        }
        if generados > 0 {
            con_documentos += 1;
        }
    }
    assert!(con_documentos >= 14, "{con_documentos}");
}

/// Misma semilla, mismo documento; y los tipos recursivos dan anidamiento.
#[test]
fn determinista_y_recursivo() {
    let xsd = std::fs::read(
        Path::new(TESTS).join("schemas/xsd/parse-ok/valid-xml-arbol-recursivo-01.xsd"),
    )
    .unwrap();
    let (ir, _) = ir_y_automata(&xsd).unwrap();
    for s in 0..10 {
        assert_eq!(generar_xml(&ir, s), generar_xml(&ir, s));
    }
    let hondo = (0..SEMILLAS)
        .filter_map(|s| generar_xml(&ir, s))
        .map(|d| d.matches("<nodo").count())
        .max()
        .unwrap();
    assert!(hondo >= 4, "{hondo}");
}

/// Los inválidos quedan bien formados: el DPDA los rechaza como `schema` o
/// los acepta (un sabotaje que no rompe nada, como omitir una rama de un
/// `xs:choice` que tiene otra), nunca como malformados; y en casi todos los
/// esquemas alguno se rechaza.
#[test]
fn invalidos_bien_formados_y_rechazados() {
    let mut rechazados_por_esquema = Vec::new();
    for (nombre, xsd) in esquemas() {
        let (ir, m) = ir_y_automata(&xsd).unwrap();
        let mut rechazados = 0;
        for s in 0..SEMILLAS {
            let Some((d, sab)) = generar_xml_invalido(&ir, s) else {
                continue;
            };
            match clase_dpda(&m, d.as_bytes()) {
                None => {}
                Some(automaton::ErrorKind::Schema) => rechazados += 1,
                Some(k) => panic!("{nombre}, {}: {k:?}\n{d}", sab.nombre()),
            }
        }
        rechazados_por_esquema.push((nombre, rechazados));
    }
    let con_rechazo = rechazados_por_esquema
        .iter()
        .filter(|(_, r)| *r > 0)
        .count();
    assert!(con_rechazo >= 12, "{rechazados_por_esquema:?}");
}

/// Cada malformación da su clase de error en todos los esquemas.
#[test]
fn malformados_con_su_clase() {
    let mut aplicadas = [0usize; 5];
    for (nombre, xsd) in esquemas() {
        let (ir, m) = ir_y_automata(&xsd).unwrap();
        let Some(d) = (0..SEMILLAS).find_map(|s| generar_xml(&ir, s)) else {
            continue;
        };
        let mut az = Azar::new(7);
        for (i, k) in Malformacion::TODAS.into_iter().enumerate() {
            for _ in 0..8 {
                let Some(x) = malformar(&d, k, &mut az) else {
                    continue;
                };
                aplicadas[i] += 1;
                let c = clase_dpda(&m, x.as_bytes());
                assert!(
                    c.is_some_and(|c| k.clases().contains(&c)),
                    "{nombre}, {}: {c:?}\n{x}",
                    k.nombre()
                );
            }
        }
    }
    assert!(aplicadas.iter().all(|&n| n > 0), "{aplicadas:?}");
}

/// Con libxml2 (Docker): el mismo veredicto que el DPDA en los válidos, los
/// inválidos y los malformados generados de cada XSD que libxml2 compila.
#[cfg(feature = "libxml2")]
#[test]
fn libxml2_da_el_mismo_veredicto() {
    use baseline::{with_stack, XmlBaseline};
    // `XmlBaseline` no es `Send`: todo dentro de un solo hilo de `with_stack`.
    let comparados = with_stack(|st| {
        let mut comparados = 0;
        for (nombre, xsd) in esquemas() {
            let (ir, m) = ir_y_automata(&xsd).unwrap();
            let Ok(lb) = XmlBaseline::compile(st, &xsd) else {
                continue;
            };
            let mut docs: Vec<String> = Vec::new();
            for s in 0..SEMILLAS {
                docs.extend(generar_xml(&ir, s));
                docs.extend(generar_xml_invalido(&ir, s).map(|(d, _)| d));
            }
            if let Some(d) = docs.first().cloned() {
                let mut az = Azar::new(3);
                for k in Malformacion::TODAS {
                    docs.extend(malformar(&d, k, &mut az));
                }
            }
            for d in docs {
                let dpda = clase_dpda(&m, d.as_bytes()).is_none();
                let libxml2 = lb.validate(st, d.as_bytes()).valid;
                assert_eq!(
                    dpda, libxml2,
                    "{nombre}: DPDA {dpda}, libxml2 {libxml2}\n{d}"
                );
                comparados += 1;
            }
        }
        comparados
    });
    assert!(comparados > 500, "{comparados}");
}
