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

/// Los XSD distintos de `tests/schemas/xsd/parse-ok` que tienen M (las
/// copias de los casos del corpus repiten contenido: cuenta una vez).
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
    let mut vistos = std::collections::HashSet::new();
    v.retain(|(_, x)| vistos.insert(x.clone()));
    v
}

/// Profundidad de anidamiento de elementos de un documento bien formado.
fn profundidad(d: &str) -> usize {
    let (mut p, mut max) = (0usize, 0usize);
    let mut resto = d;
    while let Some(i) = resto.find('<') {
        let fin = i + resto[i..].find('>').unwrap();
        let t = &resto[i..=fin];
        if t.starts_with("</") {
            p -= 1;
        } else if !t.starts_with("<?") && !t.ends_with("/>") {
            p += 1;
            max = max.max(p);
        } else if t.ends_with("/>") {
            max = max.max(p + 1);
        }
        resto = &resto[fin + 1..];
    }
    max
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
    // 14 XSD distintos con M; dan documentos todos salvo `vacio` y
    // `tipo-simple-con-nombre`, que no tienen elementos globales (12).
    assert!(con_documentos >= 12, "{con_documentos}");
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
        .map(|d| profundidad(&d))
        .max()
        .unwrap();
    assert!(hondo >= 4, "{hondo}");
}

/// Lo que no tiene documento finito, o costaría recorrer, da `None` sin
/// colgarse (D7): un tipo con un hijo obligatorio de su mismo tipo y un
/// `minOccurs` enorme en un grupo de partículas opcionales.
#[test]
fn sin_documento_finito_da_none() {
    let x = |cuerpo: &str| {
        format!("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>{cuerpo}</xs:schema>")
    };
    let recursivo = x(
        "<xs:element name='n' type='N'/><xs:complexType name='N'><xs:sequence>\
        <xs:element name='n' type='N'/></xs:sequence></xs:complexType>",
    );
    let enorme = x(
        "<xs:element name='r'><xs:complexType><xs:sequence minOccurs='4000000000' \
        maxOccurs='unbounded'><xs:element name='a' type='xs:int' minOccurs='0'/></xs:sequence>\
        </xs:complexType></xs:element>",
    );
    for esquema in [recursivo, enorme] {
        let Ok((ir, _)) = ir_y_automata(esquema.as_bytes()) else {
            continue; // fuera del subconjunto: nada que generar
        };
        for s in 0..5 {
            assert_eq!(generar_xml(&ir, s), None, "{esquema}");
        }
    }
}

/// Dos `xs:enumeration` en la cadena de restricciones: solo la intersección.
#[test]
fn enumeraciones_encadenadas() {
    let x = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
        <xs:simpleType name='A'><xs:restriction base='xs:string'>\
        <xs:enumeration value='uno'/><xs:enumeration value='dos'/><xs:enumeration value='tres'/>\
        </xs:restriction></xs:simpleType>\
        <xs:simpleType name='B'><xs:restriction base='A'>\
        <xs:enumeration value='dos'/><xs:enumeration value='tres'/></xs:restriction></xs:simpleType>\
        <xs:simpleType name='C'><xs:restriction base='B'><xs:maxLength value='3'/></xs:restriction>\
        </xs:simpleType><xs:element name='e' type='C'/></xs:schema>";
    let (ir, m) = ir_y_automata(x.as_bytes()).unwrap();
    for s in 0..SEMILLAS {
        let d = generar_xml(&ir, s).unwrap();
        assert!(d.contains("<e>dos</e>"), "{d}");
        assert_eq!(clase_dpda(&m, d.as_bytes()), None, "{d}");
    }
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
    // 10 de los 14: los demás no tienen elementos globales o no ofrecen
    // ninguna oportunidad que rompa algo.
    assert!(con_rechazo >= 10, "{rechazados_por_esquema:?}");
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
