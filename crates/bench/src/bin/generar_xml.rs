//! F5.4-P1 — Genera documentos XML válidos, inválidos y malformados desde la
//! IR de un XSD y, con `--escribir`, los guarda en el corpus.
//!
//! ```text
//! cargo run -p bench --release --bin generar_xml                 # solo muestra
//! cargo run -p bench --release --bin generar_xml -- --escribir   # tests/cases/
//! ```
//!
//! Por cada XSD de [`ESQUEMAS`] escribe un caso de cada categoría:
//! `valid-gen-xml-<esquema>-01`, `invalid-mut-xml-<esquema>-01` y
//! `malformed-xml-mut-<esquema>-01`, con el formato de `tests/README.md`
//! (`phase: 5`) y la copia del XSD en `tests/schemas/xsd/parse-ok/` que el
//! corpus exige. El `expected` sale del reporte del DPDA (como en F6.3); que
//! libxml2 da el mismo veredicto con estos mismos generadores lo comprueba
//! `tests/generar_xml.rs` con la feature `libxml2` (en Docker).

use std::path::{Path, PathBuf};

use automaton::{validate, ErrorKind, Mode};
use bench::generar::{cadena_json, Azar};
use bench::generar_xml::{generar_xml_invalido, generar_xml_valido, malformar, Malformacion};
use bench::ir_y_automata;

/// (nombre corto, caso del corpus del que se toma el XSD).
const ESQUEMAS: &[(&str, &str)] = &[
    ("pedido", "valid-xml-pedido-01"),
    ("arbol", "valid-xml-arbol-recursivo-01"),
];

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn clase(k: ErrorKind) -> &'static str {
    match k {
        ErrorKind::Lexical => "lexical",
        ErrorKind::Syntax => "syntax",
        ErrorKind::Schema => "schema",
        ErrorKind::Limit => "limit",
        ErrorKind::Internal => "internal",
    }
}

fn main() -> std::process::ExitCode {
    let escribir = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--escribir") => true,
        Some(otro) => {
            eprintln!("generar_xml: opción desconocida: {otro}");
            return std::process::ExitCode::from(2);
        }
    };
    for (corto, origen) in ESQUEMAS {
        let xsd_ruta = raiz()
            .join("schemas/xsd/parse-ok")
            .join(format!("{origen}.xsd"));
        let Ok(xsd) = std::fs::read_to_string(&xsd_ruta) else {
            eprintln!("generar_xml: falta {}", xsd_ruta.display());
            return std::process::ExitCode::from(2);
        };
        let Ok((ir, m)) = ir_y_automata(xsd.as_bytes()) else {
            eprintln!("generar_xml: {origen}: sin M");
            return std::process::ExitCode::from(2);
        };
        let Some((valido, s)) = generar_xml_valido(&ir, &m, 1, 200) else {
            eprintln!("generar_xml: {origen}: sin documento válido");
            return std::process::ExitCode::from(1);
        };
        // Un inválido que el DPDA rechace con una sola violación (un sabotaje
        // puede no romper nada, y con un hijo de menos libxml2 se detiene en
        // el primer error del modelo y cuenta menos que la conformidad total).
        let Some((invalido, sab, si)) = (1..400u64).find_map(|k| {
            let (d, sab) = generar_xml_invalido(&ir, k)?;
            let r = validate(&m, d.as_bytes(), Mode::Total, 4096);
            (r.error_kind == Some(ErrorKind::Schema) && r.violations.get() == Some(1))
                .then_some((d, sab, k))
        }) else {
            eprintln!("generar_xml: {origen}: sin inválido");
            return std::process::ExitCode::from(1);
        };
        // Un malformado: la malformación que cambia según el esquema.
        let mut az = Azar::new(s);
        let mal = if *corto == "pedido" {
            Malformacion::AtributoRepetido
        } else {
            Malformacion::CierreDistinto
        };
        let Some(malformado) = malformar(&valido, mal, &mut az) else {
            eprintln!("generar_xml: {origen}: sin malformado");
            return std::process::ExitCode::from(1);
        };
        let casos = [
            (
                "valid",
                format!("valid-gen-xml-{corto}-01"),
                format!("Documento generado desde la IR de «{origen}» con la semilla {s} (F5.4-P1); válido para el DPDA."),
                valido,
            ),
            (
                "invalid",
                format!("invalid-mut-xml-{corto}-01"),
                format!("Documento generado desde la IR de «{origen}» con la semilla {si} y el sabotaje «{}» (F5.4-P1).", sab.nombre()),
                invalido,
            ),
            (
                "malformed",
                format!("malformed-xml-mut-{corto}-01"),
                format!("El válido de «{origen}» con la malformación «{}» (F5.4-P1).", mal.nombre()),
                malformado,
            ),
        ];
        for (cat, id, desc, payload) in casos {
            let r = validate(&m, payload.as_bytes(), Mode::Total, 4096);
            // Solo lo que comparten el DPDA y libxml2: el offset de un
            // malformado (libxml2 da el final de la etiqueta) no se fija, y el
            // inválido se elige con una sola violación.
            let mut expected = format!("\"valid\": {}", r.valid);
            if r.valid {
                expected += ", \"violations\": 0";
            }
            if let Some(k) = r.error_kind {
                expected += &format!(", \"errorKind\": \"{}\"", clase(k));
            }
            if r.error_kind == Some(ErrorKind::Schema) {
                expected += &format!(
                    ", \"violations\": 1, \"pointer\": {}",
                    cadena_json(r.pointer.as_str())
                );
            }
            println!("{id}: {{{expected}}}\n{payload}\n");
            if !escribir {
                continue;
            }
            let caso = format!(
                "{{\n  \"_id\": {},\n  \"category\": \"{cat}\",\n  \"format\": \"xml\",\n  \"description\": {},\n  \"phase\": 5,\n  \"schema\": {},\n  \"payload\": {},\n  \"expected\": {{ {expected} }}\n}}\n",
                cadena_json(&id),
                cadena_json(&desc),
                cadena_json(&xsd),
                cadena_json(&payload),
            );
            let ruta = raiz().join("cases").join(cat).join(format!("{id}.json"));
            let copia = raiz()
                .join("schemas/xsd/parse-ok")
                .join(format!("{id}.xsd"));
            if let Err(e) = std::fs::write(&ruta, caso).and_then(|()| std::fs::write(&copia, &xsd))
            {
                eprintln!("generar_xml: no se pudo escribir {}: {e}", ruta.display());
                return std::process::ExitCode::from(2);
            }
        }
    }
    if escribir {
        eprintln!("generar_xml: 6 casos escritos en tests/cases/ y sus XSD en tests/schemas/xsd/parse-ok/");
    }
    std::process::ExitCode::SUCCESS
}
