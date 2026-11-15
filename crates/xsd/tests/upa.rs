//! *Unique Particle Attribution* y tope de partículas por modelo (tarea 5.7,
//! `docs/fase3/xsd.md` §11; D25, D26).
//!
//! La UPA es conservadora (D25): se comprueba sobre la abstracción en la que
//! toda partícula con `maxOccurs` > 1 es `+` o `*`, `maxOccurs="0"` es `?` y
//! un `xs:choice` vacío es ε, así que rechaza algunos modelos que XSD 1.0
//! admite (`a{2,2}, a`) y nunca acepta uno que no lo sea. Los nombres se
//! comparan por nombre **local**, como EDC (D40) y la IR. El error lleva la
//! posición de las dos partículas: en el `<` de la segunda (en orden del
//! documento) y la de la primera en el mensaje. El tope de D26 (4 096
//! partículas por modelo, elementos y grupos) se comprueba antes de calcular
//! ningún conjunto (la prueba con contador está en `crates/xsd/src/upa.rs`).
//!
//! Los casos con línea y columna están también en el corpus
//! (`tests/schemas/xsd/semantic-error/upa-*.xsd`, manifiesto
//! `errores-semanticos-xsd.json`).

use xsd::{analyze, lower, parse, ErrorKind, XsdError};

const XS: &str = "xmlns:xs='http://www.w3.org/2001/XMLSchema'";

/// Errores del análisis (un error de la Fase 2 o interno hace fallar la
/// prueba).
fn analizar(input: &str) -> Result<(), Vec<XsdError>> {
    let schema = parse(input.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{input}"));
    let analysis = analyze(&schema)?;
    lower(&analysis).unwrap_or_else(|e| {
        panic!(
            "error interno:\n{}\n{input}",
            e.detail(8).collect::<Vec<_>>().join("\n")
        )
    });
    Ok(())
}

fn acepta(input: &str) {
    if let Err(e) = analizar(input) {
        panic!(
            "{}\n{input}",
            e.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

fn rechaza(input: &str) -> Vec<XsdError> {
    let errs = analizar(input)
        .err()
        .unwrap_or_else(|| panic!("acepta:\n{input}"));
    for e in &errs {
        let msg = e.to_string();
        assert!(!msg.contains('\n'), "{msg}");
    }
    errs
}

/// Un tipo complejo `T` con el modelo `modelo` y un elemento global de ese
/// tipo; todo en la línea 1.
fn tipo(modelo: &str) -> String {
    format!(
        "<xs:schema {XS}><xs:element name='r' type='T'/>\
         <xs:complexType name='T'>{modelo}</xs:complexType></xs:schema>"
    )
}

/// Columna (1-based) de la `n`-ésima aparición (desde 0) de `s` en `doc`.
fn col_n(doc: &str, s: &str, n: usize) -> u32 {
    let mut desde = 0;
    for _ in 0..n {
        desde += doc[desde..].find(s).unwrap() + 1;
    }
    (desde + doc[desde..].find(s).unwrap_or_else(|| panic!("{s}"))) as u32 + 1
}

/// Un único error de UPA, en la partícula `segunda` y con la `primera` en
/// el mensaje.
#[track_caller]
fn conflicto(doc: &str, nombre: &str, primera: u32, segunda: u32) {
    let errs = rechaza(doc);
    assert_eq!(errs.len(), 1, "{errs:?}");
    let e = &errs[0];
    assert_eq!(
        (e.code(), e.line(), e.column()),
        ("content-model", 1, segunda),
        "{e}"
    );
    match &e.kind {
        ErrorKind::Ambiguous { name, first } => {
            assert_eq!(name, nombre);
            assert_eq!((first.line, first.column), (1, primera), "{e}");
        }
        k => panic!("{k:?}"),
    }
    let msg = e.to_string();
    assert!(msg.contains("Unique Particle Attribution"), "{msg}");
    assert!(
        msg.contains(&format!("línea 1, columna {primera}")),
        "{msg}"
    );
}

const A: &str = "<xs:element name='a' type='xs:int'";

/// Criterio: `(a?, a)`, `(a | a)`, `(a*, a)` y un `choice` anidado ambiguo
/// se rechazan con la posición de las dos partículas.
#[test]
fn modelos_ambiguos_con_las_dos_posiciones() {
    let casos = [
        format!("<xs:sequence>{A} minOccurs='0'/>{A}/></xs:sequence>"),
        format!("<xs:choice>{A}/>{A}/></xs:choice>"),
        format!("<xs:sequence>{A} maxOccurs='unbounded'/>{A}/></xs:sequence>"),
        format!("<xs:sequence>{A} minOccurs='0' maxOccurs='unbounded'/>{A}/></xs:sequence>"),
        // `(b, (c | (d | c)))`: tras `b`, dos `c` posibles.
        "<xs:sequence><xs:element name='b' type='xs:int'/><xs:choice>\
         <xs:element name='c' type='xs:int'/><xs:choice><xs:element name='d' type='xs:int'/>\
         <xs:element name='c' type='xs:int'/></xs:choice></xs:choice></xs:sequence>"
            .to_owned(),
    ];
    for (k, modelo) in casos.iter().enumerate() {
        let doc = tipo(modelo);
        let nombre = if k == 4 { "c" } else { "a" };
        let p = format!("<xs:element name='{nombre}'");
        conflicto(&doc, nombre, col_n(&doc, &p, 0), col_n(&doc, &p, 1));
    }
}

/// Ambigüedades que solo se ven a través de un grupo: la vuelta de un grupo
/// repetido, un grupo opcional entero y un `ref`.
#[test]
fn ambiguos_a_traves_de_grupos_y_ref() {
    // `((a, b?)*, a)`: tras `a` (sin `b`), la vuelta del grupo y el `a` final.
    let doc = tipo(&format!(
        "<xs:sequence><xs:sequence maxOccurs='unbounded'>{A}/>\
         <xs:element name='b' type='xs:int' minOccurs='0'/></xs:sequence>{A}/></xs:sequence>"
    ));
    conflicto(&doc, "a", col_n(&doc, A, 0), col_n(&doc, A, 1));
    // `((b, a)?, a)`: al comienzo no, pero `b` es opcional por el grupo.
    let doc = tipo(&format!(
        "<xs:sequence><xs:sequence minOccurs='0'><xs:element name='b' type='xs:int' minOccurs='0'/>\
         {A}/></xs:sequence>{A}/></xs:sequence>"
    ));
    conflicto(&doc, "a", col_n(&doc, A, 0), col_n(&doc, A, 1));
    // Un `ref` y un local con el mismo nombre y tipo (pasan EDC).
    let doc = format!(
        "<xs:schema {XS}><xs:element name='a' type='xs:int'/>\
         <xs:complexType name='T'><xs:choice><xs:element ref='a'/>{A}/></xs:choice>\
         </xs:complexType></xs:schema>"
    );
    conflicto(
        &doc,
        "a",
        col_n(&doc, "<xs:element ref='a'", 0),
        col_n(&doc, A, 1),
    );
}

/// Criterio: los modelos deterministas pasan (los del corpus, pedido y
/// `pedido_xsd`, los recorre `corpus_clasificado`): repetir un nombre en
/// secuencia, opcionales seguidos de otro nombre, la vuelta de un grupo
/// con un primer nombre distinto, y `(a, (b | c)?, a)`.
#[test]
fn modelos_deterministas_pasan() {
    let b = "<xs:element name='b' type='xs:int'";
    let c = "<xs:element name='c' type='xs:int'";
    for modelo in [
        format!("<xs:sequence>{A}/>{A}/></xs:sequence>"),
        format!("<xs:sequence>{A} minOccurs='0'/>{b}/>{A}/></xs:sequence>"),
        format!("<xs:sequence>{A} maxOccurs='unbounded'/>{b}/>{A} minOccurs='0'/></xs:sequence>"),
        format!("<xs:sequence maxOccurs='unbounded'>{A}/>{b} minOccurs='0'/></xs:sequence>"),
        format!(
            "<xs:sequence><xs:choice maxOccurs='unbounded'>{A}/>{b}/></xs:choice>{c}/></xs:sequence>"
        ),
        format!(
            "<xs:sequence>{A}/><xs:choice minOccurs='0'>{b}/>{c}/></xs:choice>{A}/></xs:sequence>"
        ),
        format!("<xs:sequence>{A}/><xs:sequence>{A}/></xs:sequence>{A}/></xs:sequence>"),
        format!("<xs:all>{A}/>{b} minOccurs='0'/>{c}/></xs:all>"),
        // El modelo del pedido (`crates/ir/tests/forma.rs`, `pedido_xsd`).
        format!(
            "<xs:sequence>{A}/><xs:choice>{b}/>{c}/></xs:choice>\
             <xs:element name='item' type='xs:int' maxOccurs='unbounded'/>\
             <xs:element name='sub' type='T' minOccurs='0'/></xs:sequence>"
        ),
    ] {
        acepta(&tipo(&modelo));
    }
}

/// D25: la abstracción con `+`/`*` rechaza modelos que XSD 1.0 admite con
/// contadores (`a{2,2}, a`; `a{1,2}, b, a`, no: ese es determinista), y
/// `maxOccurs="0"` cuenta como `?` y un `xs:choice` vacío como ε (las dos,
/// lecturas que solo rechazan de más). Todo lo que acepta lo acepta también
/// el Glushkov exacto.
#[test]
fn abstraccion_conservadora_de_d25() {
    for modelo in [
        format!("<xs:sequence>{A} minOccurs='2' maxOccurs='2'/>{A}/></xs:sequence>"),
        format!("<xs:sequence>{A} minOccurs='3' maxOccurs='5'/>{A} minOccurs='0'/></xs:sequence>"),
        format!("<xs:sequence>{A} minOccurs='0' maxOccurs='0'/>{A}/></xs:sequence>"),
        format!("<xs:sequence>{A} maxOccurs='unbounded'/><xs:choice/>{A}/></xs:sequence>"),
    ] {
        let errs = rechaza(&tipo(&modelo));
        assert!(
            errs.iter()
                .all(|e| matches!(e.kind, ErrorKind::Ambiguous { .. })),
            "{errs:?}"
        );
    }
    acepta(&tipo(&format!(
        "<xs:sequence>{A} maxOccurs='2'/><xs:element name='b' type='xs:int'/>{A}/></xs:sequence>"
    )));
}

/// Criterio heredado 5.3-CR1-S4: un nombre repetido en `xs:all`, con el
/// mismo tipo (pasa EDC y `verify`), lo rechaza la UPA.
#[test]
fn nombre_repetido_en_all() {
    let doc = tipo(&format!(
        "<xs:all>{A}/><xs:element name='b' type='xs:int'/>{A} minOccurs='0'/></xs:all>"
    ));
    conflicto(&doc, "a", col_n(&doc, A, 0), col_n(&doc, A, 1));
}

/// Cada partícula se informa a lo sumo una vez (como la segunda de su
/// pareja): `(a | a | a)` da dos errores, en orden del documento, y uno de
/// 1000 `a` da 999, no un error por pareja.
#[test]
fn un_error_por_particula_en_orden() {
    let doc = tipo(&format!("<xs:choice>{A}/>{A}/>{A}/></xs:choice>"));
    let errs = rechaza(&doc);
    let pos: Vec<(u32, u32)> = errs
        .iter()
        .map(|e| match e.kind {
            ErrorKind::Ambiguous { first, .. } => (e.column(), first.column),
            _ => panic!("{e}"),
        })
        .collect();
    let c = |n| col_n(&doc, A, n);
    assert_eq!(pos, [(c(1), c(0)), (c(2), c(0))]);
    let doc = tipo(&format!(
        "<xs:choice maxOccurs='unbounded'>{}</xs:choice>",
        format!("{A}/>").repeat(1000)
    ));
    assert_eq!(rechaza(&doc).len(), 999);
}

/// B2: la UPA es de la etapa 3 (modelos de contenido), con I11 y EDC, todos
/// por offset; un error de la etapa 1 la oculta.
#[test]
fn upa_en_la_etapa_de_modelos() {
    let doc = tipo(&format!(
        "<xs:sequence>{A} minOccurs='0'/>{A}/>\
         <xs:element name='b' type='xs:int' minOccurs='2' maxOccurs='1'/></xs:sequence>"
    ));
    let codigos: Vec<(&str, u32)> = rechaza(&doc)
        .iter()
        .map(|e| (e.code(), e.column()))
        .collect();
    assert_eq!(
        codigos,
        [
            ("content-model", col_n(&doc, A, 1)),
            ("content-model", col_n(&doc, "1'/>", 0))
        ]
    );
    let con_uno = doc.replace("name='b' type='xs:int'", "name='b' type='Nadie'");
    let errs = rechaza(&con_uno);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].code(), "unresolved");
}

/// Un tipo con `n` partículas: el `xs:sequence` raíz y `n − 1` elementos.
fn con_particulas(n: usize) -> String {
    tipo(&format!(
        "<xs:sequence>{}</xs:sequence>",
        "<xs:element name='e' type='xs:int'/>".repeat(n - 1)
    ))
}

/// Criterio (D26): pasar de 4 096 partículas por modelo (elementos y grupos)
/// es un error `limit` en el `<` de la partícula 4 097; 4 096 pasan. El tope
/// es por modelo: dos modelos de 4 000 pasan.
#[test]
fn tope_de_particulas_por_modelo() {
    acepta(&con_particulas(4096));
    let doc = con_particulas(4097);
    let errs = rechaza(&doc);
    assert_eq!(errs.len(), 1, "{errs:?}");
    let e = &errs[0];
    let ultimo = doc.rfind("<xs:element name='e'").unwrap() as u32 + 1;
    assert_eq!(
        (e.code(), e.line(), e.column()),
        ("limit", 1, ultimo),
        "{e}"
    );
    assert!(
        matches!(e.kind, ErrorKind::TooManyParticles { max: 4096 }),
        "{e}"
    );
    assert!(e.to_string().contains("4096 partículas"), "{e}");
    // Los grupos cuentan: 4 096 `xs:sequence` vacíos dentro de la raíz.
    let doc = tipo(&format!(
        "<xs:sequence>{}</xs:sequence>",
        "<xs:sequence/>".repeat(4096)
    ));
    assert_eq!(
        rechaza(&doc).iter().map(XsdError::code).collect::<Vec<_>>(),
        ["limit"]
    );
    // Por modelo, no por esquema.
    let e = "<xs:element name='e' type='xs:int'/>".repeat(3999);
    acepta(&format!(
        "<xs:schema {XS}><xs:element name='r' type='T'/>\
         <xs:complexType name='T'><xs:sequence>{e}</xs:sequence></xs:complexType>\
         <xs:complexType name='U'><xs:sequence>{e}</xs:sequence></xs:complexType></xs:schema>"
    ));
}

/// El mensaje del tope y el de UPA son de una línea y recortan el nombre.
#[test]
fn mensajes_recortados() {
    let largo = "n".repeat(1 << 16);
    let doc = tipo(&format!(
        "<xs:choice><xs:element name='{largo}' type='xs:int'/>\
         <xs:element name='{largo}' type='xs:int'/></xs:choice>"
    ));
    let errs = rechaza(&doc);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].to_string().len() < 400, "{}", errs[0]);
}

/// Las posiciones de un error de vuelta ambigua con contador: (columna del
/// error, de la partícula que se repite, de la del contador).
fn vuelta(e: &XsdError) -> (u32, u32, u32) {
    match e.kind {
        ErrorKind::AmbiguousCounter {
            repeat, counter, ..
        } => (e.column(), repeat.column, counter.column),
        _ => panic!("{e}"),
    }
}

/// I-1 de la revisión (enmienda de D25): la UPA asegura que el autómata de
/// **posiciones** es determinista (lo que pide XSD 1.0); con contadores
/// hace falta además que cada transición actualice los contadores de una
/// sola forma. Si una posición vuelve a entrar en *follow*(x) desde dos
/// niveles de la subida con una partícula con contador entre ellos, error
/// `content-model` en esa partícula, con la que se repite y la del contador.
#[test]
fn vueltas_ambiguas_con_contador() {
    // `(a{1,2}){2,2}`: lenguaje a{2,4}; tras la primera `a`, la segunda
    // puede ser de la misma vuelta del grupo o de la siguiente.
    let doc = tipo(&format!(
        "<xs:sequence minOccurs='2' maxOccurs='2'>{A} maxOccurs='2'/></xs:sequence>"
    ));
    let errs = rechaza(&doc);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!((errs[0].code(), errs[0].line()), ("content-model", 1));
    let grupo = col_n(&doc, "<xs:sequence", 0);
    assert_eq!(vuelta(&errs[0]), (col_n(&doc, A, 0), grupo, grupo));
    let msg = errs[0].to_string();
    assert!(msg.contains("no es determinista con contadores"), "{msg}");
    assert!(msg.contains(&format!("línea 1, columna {grupo}")), "{msg}");
    // `(a{1,2}, b?)*`: el contador es el de `a`; se repite el grupo.
    let doc = tipo(&format!(
        "<xs:sequence maxOccurs='unbounded'>{A} maxOccurs='2'/>\
         <xs:element name='b' type='xs:int' minOccurs='0'/></xs:sequence>"
    ));
    let errs = rechaza(&doc);
    assert_eq!(errs.len(), 1, "{errs:?}");
    let a = col_n(&doc, A, 0);
    assert_eq!(vuelta(&errs[0]), (a, col_n(&doc, "<xs:sequence", 0), a));
    // `(a+){2,3}`, `(a*){2,3}` y `((a, b){2,2})*`: el contador es el de un
    // grupo (la última es determinista con los valores del contador, pero
    // la regla es conservadora: la entrada a la vuelta del grupo exterior
    // también vuelve a pasar por el interior).
    for modelo in [
        format!(
            "<xs:sequence minOccurs='2' maxOccurs='3'>{A} maxOccurs='unbounded'/></xs:sequence>"
        ),
        format!(
            "<xs:sequence minOccurs='2' maxOccurs='3'>{A} minOccurs='0' maxOccurs='unbounded'/>\
             </xs:sequence>"
        ),
        format!(
            "<xs:sequence maxOccurs='unbounded'><xs:sequence minOccurs='2' maxOccurs='2'>{A}/>\
             <xs:element name='b' type='xs:int'/></xs:sequence></xs:sequence>"
        ),
    ] {
        let errs = rechaza(&tipo(&modelo));
        assert!(
            errs.iter()
                .all(|e| matches!(e.kind, ErrorKind::AmbiguousCounter { .. })),
            "{errs:?}"
        );
    }
}

/// Sin contador por medio, o sin vuelta que entre dos veces, no cambia
/// nada: `a+`, `(a*)*`, `((a+)+)*`, `(a, b)*`, `(a{2,3}, b)`, `(a{2,3}, b)*`
/// y `(a, b{2,5})*` pasan; y en `(a?, ()){2,2}` el grupo vacío vuelve a
/// entrar por la vuelta del contador, pero sin posiciones no cambia nada.
#[test]
fn contadores_sin_vuelta_ambigua_pasan() {
    let b = "<xs:element name='b' type='xs:int'";
    for modelo in [
        format!("<xs:sequence>{A} maxOccurs='unbounded'/></xs:sequence>"),
        format!(
            "<xs:sequence minOccurs='0' maxOccurs='unbounded'>{A} minOccurs='0' \
             maxOccurs='unbounded'/></xs:sequence>"
        ),
        format!(
            "<xs:sequence maxOccurs='unbounded'><xs:sequence maxOccurs='unbounded'>{A} \
             maxOccurs='unbounded'/></xs:sequence></xs:sequence>"
        ),
        format!("<xs:sequence maxOccurs='unbounded'>{A}/>{b}/></xs:sequence>"),
        format!("<xs:sequence>{A} minOccurs='2' maxOccurs='3'/>{b}/></xs:sequence>"),
        format!(
            "<xs:sequence maxOccurs='unbounded'>{A} minOccurs='2' maxOccurs='3'/>{b}/></xs:sequence>"
        ),
        format!(
            "<xs:sequence maxOccurs='unbounded'>{A}/>{b} minOccurs='2' maxOccurs='5'/></xs:sequence>"
        ),
        format!(
            "<xs:sequence minOccurs='2' maxOccurs='2'>{A} minOccurs='0'/><xs:sequence/></xs:sequence>"
        ),
    ] {
        acepta(&tipo(&modelo));
    }
}
