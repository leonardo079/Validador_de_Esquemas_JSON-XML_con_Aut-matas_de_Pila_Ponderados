//! DTD.5: diferencial **por veredictos** entre una DTD y el mismo esquema
//! escrito en XSD (D86, enmienda E3).
//!
//! Comparar el JSON v2 no sirve: la bajada de XSD crea nodos sintéticos y
//! otro orden de `NodeId`. Lo que tiene que coincidir es el **lenguaje**: para
//! cada par equivalente se generan documentos válidos, inválidos (con un
//! sabotaje) y malformados desde las **dos** IR, y los dos autómatas tienen que
//! dar el mismo veredicto y la misma clase de error. Deterministas con la
//! semilla (D1).

use std::path::Path;

use bench::generar::Azar;
use bench::generar_xml::{
    clase_dpda, generar_xml_invalido, generar_xml_valido, malformar, Malformacion,
};
use bench::ir_y_automata;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/schemas/dtd");
const SEMILLAS: u64 = 120;
const PARES: [&str; 2] = ["pedido", "biblioteca"];

#[test]
fn dtd_y_xsd_equivalentes_dan_los_mismos_veredictos() {
    for par in PARES {
        let dtd = std::fs::read(Path::new(DIR).join(format!("parse-ok/{par}.dtd"))).unwrap();
        let xsd = std::fs::read(Path::new(DIR).join(format!("xsd-equivalente/{par}.xsd"))).unwrap();
        let (ir_d, m_d) = ir_y_automata(&dtd).unwrap_or_else(|e| panic!("{par}.dtd: {e}"));
        let (ir_x, m_x) = ir_y_automata(&xsd).unwrap_or_else(|e| panic!("{par}.xsd: {e}"));

        let mut documentos: Vec<(String, String)> = Vec::new();
        let (mut validos, mut invalidos, mut malformados) = (0, 0, 0);
        for (lado, ir, m) in [("dtd", &ir_d, &m_d), ("xsd", &ir_x, &m_x)] {
            for s in 0..SEMILLAS {
                if let Some((doc, _)) = generar_xml_valido(ir, m, s, 8) {
                    validos += 1;
                    let mut azar = Azar::new(s ^ 0xD7D);
                    for mal in Malformacion::TODAS {
                        if let Some(roto) = malformar(&doc, mal, &mut azar) {
                            malformados += 1;
                            documentos.push((format!("{lado} {s} {}", mal.nombre()), roto));
                        }
                    }
                    documentos.push((format!("{lado} {s} válido"), doc));
                }
                if let Some((doc, sabotaje)) = generar_xml_invalido(ir, s) {
                    invalidos += 1;
                    documentos.push((format!("{lado} {s} {sabotaje:?}"), doc));
                }
            }
        }
        // El generador funciona con las dos IR: hay de los tres tipos. Los
        // inválidos son menos: con muchas semillas no hay dónde sabotear (el
        // pedido tiene pocos atributos y textos con restricciones).
        assert!(
            validos >= SEMILLAS && invalidos >= 20 && malformados >= SEMILLAS,
            "{par}: {validos} válidos, {invalidos} inválidos, {malformados} malformados"
        );
        eprintln!("{par}: {validos} válidos, {invalidos} inválidos, {malformados} malformados; {} documentos comparados", documentos.len());

        for (origen, doc) in &documentos {
            let (d, x) = (
                clase_dpda(&m_d, doc.as_bytes()),
                clase_dpda(&m_x, doc.as_bytes()),
            );
            assert_eq!(
                d, x,
                "{par} ({origen}): la DTD dice {d:?} y el XSD {x:?}\n{doc}"
            );
        }
    }
}
