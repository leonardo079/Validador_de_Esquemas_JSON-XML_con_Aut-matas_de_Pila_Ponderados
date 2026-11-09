//! Análisis y bajada del subconjunto XSD a la IR (tarea 5.3,
//! `docs/fase3/xsd.md`).
//!
//! Criterios: `pedido.xsd` baja a la misma IR que `pedido_xsd`
//! (`crates/ir/tests/forma.rs`) salvo el orden de nodos de B3, con las
//! mismas ubicaciones; los esquemas de `tests/cases` y del corpus bajan a
//! una IR verificada o dan un error del usuario, nunca un error interno;
//! cada entrada de `semantic-error` da su error con línea y columna; el
//! análisis y la bajada son iterativos y O(n) (un tipo usado k veces es una
//! arista por uso); y las tablas se indexan por `NsId`, sin releer un URI
//! largo por referencia.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ir::{
    Attribute, Constraint, ContentModel, Format, Ir, Lexical, Node, NodeId, Num, Occurs, Origin,
    Particle, Pos, Rule, Sort, Span, Step, Term, VerifiedIr,
};
use xsd::{analyze, lower, parse, XsdError};

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");
const XS: &str = "xmlns:xs='http://www.w3.org/2001/XMLSchema'";

fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    v.sort();
    v
}

fn corpus(sub: &str) -> Vec<PathBuf> {
    files(&Path::new(TESTS_DIR).join("schemas/xsd").join(sub), "xsd")
}

/// Resultado de todo el frontal sobre un texto: la IR verificada, o los
/// errores del usuario (de la Fase 2 o del análisis). Un error interno de
/// `verify` hace fallar la prueba con su registro.
fn compilar(input: &[u8]) -> Result<VerifiedIr, Vec<XsdError>> {
    let schema = parse(input).map_err(|e| vec![e])?;
    let analysis = analyze(&schema)?;
    match lower(&analysis) {
        Ok(ir) => Ok(ir),
        Err(e) => panic!(
            "error interno de la bajada:\n{}",
            e.detail(8).collect::<Vec<_>>().join("\n")
        ),
    }
}

fn ir_de(input: &str) -> VerifiedIr {
    compilar(input.as_bytes()).unwrap_or_else(|e| {
        panic!(
            "{}\n{input}",
            e.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

/// Errores del análisis como (código, línea, columna).
fn errores(input: &str) -> Vec<(&'static str, u32, u32)> {
    let schema = parse(input.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{input}"));
    let errs = analyze(&schema).expect_err(input);
    for e in &errs {
        let msg = e.to_string();
        assert!(!msg.contains('\n'), "{msg}");
        assert!(
            msg.starts_with(&format!("línea {}, columna {}: ", e.line(), e.column())),
            "{msg}"
        );
    }
    errs.iter()
        .map(|e| (e.code(), e.line(), e.column()))
        .collect()
}

fn col(doc: &str, s: &str) -> u32 {
    doc.find(s).unwrap_or_else(|| panic!("{s}")) as u32 + 1
}

// --------------------------------------------------- `pedido_xsd` a mano

fn sp() -> Span {
    Span {
        start: Pos::START,
        end: 0,
    }
}

fn id(i: usize) -> NodeId {
    NodeId::new(i)
}

fn entry(k: &str, e: &str) -> Step {
    Step::Entry(k.to_owned(), e.to_owned())
}

fn node(sort: Sort, parent: Option<usize>, step: Step, rules: Vec<Rule>) -> Node {
    Node {
        sort,
        rules: rules
            .into_iter()
            .map(|rule| Constraint { rule, span: sp() })
            .collect(),
        origin: Origin {
            span: sp(),
            parent: parent.map(id),
            step,
        },
    }
}

fn el(name: &str, ty: usize, occurs: Occurs) -> Particle {
    Particle {
        term: Term::Element {
            name: name.into(),
            ty: id(ty),
        },
        occurs,
        span: sp(),
    }
}

fn group(term: Term, occurs: Occurs) -> Particle {
    Particle {
        term,
        occurs,
        span: sp(),
    }
}

fn attr(name: &str, required: bool, ty: usize) -> Attribute {
    Attribute {
        name: name.into(),
        required,
        ty: id(ty),
        default: None,
        fixed: None,
        span: sp(),
    }
}

/// Copia literal de `pedido_xsd` (`crates/ir/tests/forma.rs`), la IR hecha
/// a mano del esquema de `tests/schemas/xsd/parse-ok/pedido.xsd`; el
/// frontal XSD no puede depender de las pruebas de `ir`.
fn pedido_xsd() -> Ir {
    let mut ir = Ir::new(
        Format::Xml {
            target_namespace: None,
        },
        node(
            Sort::Element,
            None,
            Step::Root,
            vec![Rule::Content(ContentModel {
                particles: vec![
                    group(Term::Choice(vec![1]), Occurs::ONCE),
                    el("pedido", 1, Occurs::ONCE),
                ],
            })],
        ),
    );
    let mut moneda = attr("moneda", false, 6);
    moneda.default = Some("COP".into());
    ir.push(node(
        Sort::Element,
        Some(0),
        entry("complexType", "Pedido"),
        vec![
            Rule::Content(ContentModel {
                particles: vec![
                    group(Term::Sequence(vec![1, 2, 5, 6]), Occurs::ONCE),
                    el("cliente", 3, Occurs::ONCE),
                    group(Term::Choice(vec![3, 4]), Occurs::ONCE),
                    el("tarjeta", 3, Occurs::ONCE),
                    el("efectivo", 4, Occurs::ONCE),
                    el("item", 5, Occurs { min: 1, max: None }),
                    el(
                        "subpedido",
                        1,
                        Occurs {
                            min: 0,
                            max: Some(1),
                        },
                    ),
                ],
            }),
            Rule::Attributes(vec![attr("id", true, 2)]),
        ],
    ));
    ir.push(node(
        Sort::Text,
        Some(0),
        entry("builtin", "int"),
        vec![
            Rule::Lexical(Lexical::Integer),
            Rule::Minimum(Num::new("-2147483648")),
            Rule::Maximum(Num::new("2147483647")),
        ],
    ));
    ir.push(node(
        Sort::Element,
        Some(0),
        entry("simpleContent", "xs:string"),
        vec![Rule::SimpleContent(id(6))],
    ));
    ir.push(node(
        Sort::Element,
        Some(0),
        entry("simpleContent", "xs:decimal"),
        vec![Rule::SimpleContent(id(7))],
    ));
    ir.push(node(
        Sort::Element,
        Some(1),
        entry("element", "item"),
        vec![
            Rule::SimpleContent(id(7)),
            Rule::Attributes(vec![attr("sku", true, 6), moneda]),
        ],
    ));
    ir.push(node(
        Sort::Text,
        Some(0),
        entry("builtin", "string"),
        vec![Rule::Lexical(Lexical::String)],
    ));
    ir.push(node(
        Sort::Text,
        Some(0),
        entry("builtin", "decimal"),
        vec![Rule::Lexical(Lexical::Decimal)],
    ));
    ir
}

/// Forma canónica de una IR para compararla con otra salvo el orden de los
/// nodos y los rangos: cada nodo por su ubicación, con las aristas y el
/// padre también como ubicaciones y los rangos a cero.
fn canonica(ir: &Ir) -> Vec<(String, String)> {
    let loc: Vec<String> = (0..ir.nodes.len()).map(|i| ir.location(id(i))).collect();
    let ren = |t: NodeId| loc[t.index()].clone();
    let mut out: Vec<(String, String)> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let rules: Vec<String> = n
                .rules
                .iter()
                .map(|c| match &c.rule {
                    Rule::Content(m) => {
                        let ps: Vec<String> = m
                            .particles
                            .iter()
                            .map(|p| {
                                let t = match &p.term {
                                    Term::Element { name, ty } => {
                                        format!("{name}:{}", ren(*ty))
                                    }
                                    t => format!("{t:?}"),
                                };
                                format!("{t}{:?}", p.occurs)
                            })
                            .collect();
                        format!("Content{ps:?}")
                    }
                    Rule::Attributes(attrs) => {
                        let v: Vec<String> = attrs
                            .iter()
                            .map(|a| {
                                format!(
                                    "{} {} {} {:?} {:?}",
                                    a.name,
                                    a.required,
                                    ren(a.ty),
                                    a.default,
                                    a.fixed
                                )
                            })
                            .collect();
                        format!("Attributes{v:?}")
                    }
                    Rule::SimpleContent(t) => format!("SimpleContent({})", ren(*t)),
                    Rule::Ref(t) => format!("Ref({})", ren(*t)),
                    r => format!("{r:?}"),
                })
                .collect();
            let parent = n.origin.parent.map(ren);
            (
                loc[i].clone(),
                format!("{:?} {rules:?} padre {parent:?}", n.sort),
            )
        })
        .collect();
    out.sort();
    out
}

/// Criterio: `pedido.xsd` baja a la IR de `pedido_xsd` salvo el orden de
/// los nodos (B3: documento, declaraciones en preorden, sintéticos por
/// primer uso) y los rangos, con las mismas ubicaciones.
#[test]
fn pedido_baja_igual_que_la_ir_hecha_a_mano() {
    let bytes =
        std::fs::read(Path::new(TESTS_DIR).join("schemas/xsd/parse-ok/pedido.xsd")).unwrap();
    let v = compilar(&bytes).unwrap_or_else(|e| panic!("{e:?}"));
    let a_mano = pedido_xsd().verify().unwrap();
    assert_eq!(canonica(v.ir()), canonica(a_mano.ir()));
    assert_eq!(v.format(), a_mano.format());
    // B3: el orden de los nodos.
    let orden: Vec<String> = (0..v.nodes().len()).map(|i| v.location(id(i))).collect();
    assert_eq!(
        orden,
        [
            "#",
            "#/complexType/Pedido",
            "#/complexType/Pedido/element/item",
            "#/simpleContent/xs:string",
            "#/builtin/string",
            "#/simpleContent/xs:decimal",
            "#/builtin/decimal",
            "#/builtin/int",
        ]
    );
    // Rangos: el de cada declaración y el de cada regla, en su construcción.
    let texto = String::from_utf8(bytes).unwrap();
    let linea = |s: &str| texto[..texto.find(s).unwrap()].matches('\n').count() as u32 + 1;
    let pedido = v.node(id(1));
    assert_eq!(
        pedido.origin.span.start.line,
        linea("<xs:complexType name=\"Pedido\"")
    );
    assert_eq!(pedido.rules[0].span.start.line, linea("<xs:sequence>"));
    assert_eq!(
        pedido.rules[1].span.start.line,
        linea("<xs:attribute name=\"id\"")
    );
    // El predefinido lleva el rango de su primer uso (`type="xs:int"`).
    let int = v.node(id(7));
    let l = linea("type=\"xs:int\"");
    let c = texto
        .lines()
        .nth(l as usize - 1)
        .unwrap()
        .find("\"xs:int\"")
        .unwrap() as u32
        + 1;
    assert_eq!(
        (int.origin.span.start.line, int.origin.span.start.column),
        (l, c)
    );
}

// ------------------------------------------------------------- corpus

/// Criterio: cada entrada de `tests/schemas/xsd/semantic-error` da el
/// código, la línea y la columna del manifiesto `errores-semanticos-xsd.json`
/// en su primer error (con `count`, el número de errores de la etapa), con un
/// mensaje de una línea que contiene el texto declarado. Todas parsean: son
/// errores del análisis, no de la Fase 2.
#[test]
fn errores_semanticos_con_linea_y_columna() {
    let dir = Path::new(TESTS_DIR).join("schemas/xsd/semantic-error");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(Path::new(TESTS_DIR).join("schemas/errores-semanticos-xsd.json")).unwrap(),
    )
    .unwrap();
    let entries = manifest.as_array().unwrap();
    let en_disco: BTreeSet<String> = corpus("semantic-error")
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    let declarados: BTreeSet<String> = entries
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        en_disco, declarados,
        "manifiesto y semantic-error/ no coinciden"
    );
    assert_eq!(declarados.len(), entries.len(), "entradas repetidas");
    assert!(entries.len() >= 30, "corpus demasiado pequeño");

    for e in entries {
        let file = e["file"].as_str().unwrap();
        let text = std::fs::read_to_string(dir.join(file)).unwrap();
        let got = errores(&text);
        let (line, column) = (e["line"].as_u64().unwrap(), e["column"].as_u64().unwrap());
        assert_eq!(
            (got[0].0, u64::from(got[0].1), u64::from(got[0].2)),
            (e["code"].as_str().unwrap(), line, column),
            "{file}: {got:?}"
        );
        let n = e["count"].as_u64().unwrap_or(1) as usize;
        assert_eq!(got.len(), n, "{file}: {got:?}");
        let schema = parse(text.as_bytes()).unwrap();
        let msg = analyze(&schema).unwrap_err()[0].to_string();
        let sub = e["contains"].as_str().unwrap();
        assert!(msg.contains(sub), "{file}: «{msg}» no contiene «{sub}»");
    }
}

/// Criterio: ningún esquema del corpus (`parse-ok`, `parse-error`,
/// `semantic-error` y los XSD de `tests/cases`) produce un error de
/// `verify`: o baja a una IR verificada o da un error del usuario
/// (`compilar` hace fallar la prueba con un error interno).
#[test]
fn corpus_entero_sin_error_interno() {
    let (mut ok, mut usuario) = (0, 0);
    let mut textos: Vec<(String, Vec<u8>)> = ["parse-ok", "parse-error", "semantic-error"]
        .iter()
        .flat_map(|d| corpus(d))
        .map(|p| (p.display().to_string(), std::fs::read(&p).unwrap()))
        .collect();
    textos.extend(casos_xml());
    for (nombre, bytes) in textos {
        match compilar(&bytes) {
            Ok(_) => ok += 1,
            Err(e) => {
                assert!(!e.is_empty(), "{nombre}");
                usuario += 1;
            }
        }
    }
    assert!(ok >= 7 && usuario >= 100, "{ok} {usuario}");
}

/// Los esquemas XSD de los casos de `tests/cases` (`format: "xml"`).
fn casos_xml() -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    for cat in std::fs::read_dir(Path::new(TESTS_DIR).join("cases")).unwrap() {
        let cat = cat.unwrap().path();
        if !cat.is_dir() {
            continue;
        }
        for path in files(&cat, "json") {
            let case: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            if case["format"] == "xml" {
                let text = case["schema"].as_str().unwrap().as_bytes().to_vec();
                v.push((path.display().to_string(), text));
            }
        }
    }
    v
}

/// Criterio: los dos esquemas XSD de `tests/cases` bajan a una IR que pasa
/// `verify`, con la forma esperada.
#[test]
fn esquemas_de_los_casos_bajan() {
    let casos = casos_xml();
    assert_eq!(casos.len(), 2, "{casos:?}");
    for (nombre, bytes) in casos {
        let v = compilar(&bytes).unwrap_or_else(|e| panic!("{nombre}: {e:?}"));
        let orden: Vec<String> = (0..v.nodes().len()).map(|i| v.location(id(i))).collect();
        if nombre.contains("pedido") {
            // B3: el tipo anónimo de un elemento global cuelga del
            // documento con el paso `element/pedido` (S-5, primera mitad).
            assert_eq!(
                orden,
                [
                    "#",
                    "#/element/pedido",
                    "#/simpleContent/xs:string",
                    "#/builtin/string",
                    "#/simpleContent/xs:decimal",
                    "#/builtin/decimal",
                    "#/builtin/positiveInteger",
                ]
            );
            let pi = v.node(id(6));
            assert_eq!(pi.rules.len(), 2, "Lexical y Minimum(1)");
            assert_eq!(pi.rules[1].rule, Rule::Minimum(Num::new("1")));
        } else {
            assert_eq!(
                orden,
                ["#", "#/simpleContent/xs:string", "#/builtin/string"]
            );
        }
    }
}

// ------------------------------------------------ correspondencia y B3

/// `ir.md` §5.2: ocurrencias saturadas, contenido vacío (cláusulas 2.1.1 a
/// 2.1.3), `Choice([])` con `minOccurs` ≥ 1, `ref` con el tipo de su
/// declaración (arista compartida), atributo sin tipo como
/// `xs:anySimpleType` compartido, `default`/`fixed` intactos,
/// `targetNamespace` en el formato y los atributos globales sin bajar.
#[test]
fn correspondencia_de_cada_construccion() {
    let doc = format!(
        "<xs:schema {XS} xmlns:t='urn:t' targetNamespace=' urn:t '>\
         <xs:element name='g' type='xs:int'/>\
         <xs:attribute name='global' type='xs:string'/>\
         <xs:complexType name='T'><xs:sequence>\
           <xs:element name='a' type='xs:string' minOccurs='+0' maxOccurs='99999999999999999999999'/>\
           <xs:element ref='t:g' maxOccurs='unbounded'/>\
           <xs:choice/>\
         </xs:sequence>\
         <xs:attribute name='x'/><xs:attribute name='y' fixed=' 1 '/>\
         <xs:attribute name='z' type='xs:int' default='-0'/>\
         </xs:complexType>\
         <xs:complexType name='V1'/>\
         <xs:complexType name='V2'><xs:sequence maxOccurs='3'/></xs:complexType>\
         <xs:complexType name='V3'><xs:all/></xs:complexType>\
         <xs:complexType name='V4'><xs:choice minOccurs='0'/></xs:complexType>\
         <xs:complexType name='N'><xs:choice/></xs:complexType>\
         </xs:schema>"
    );
    let v = ir_de(&doc);
    assert_eq!(
        v.format(),
        &Format::Xml {
            target_namespace: Some("urn:t".into())
        }
    );
    let loc = |i: usize| v.location(id(i));
    let por_ubicacion: HashMap<String, usize> = (0..v.nodes().len()).map(|i| (loc(i), i)).collect();
    let n = |s: &str| v.node(id(por_ubicacion[s]));
    let Rule::Content(m) = &n("#/complexType/T").rules[0].rule else {
        panic!("Content");
    };
    assert_eq!(
        m.particles[1].occurs,
        Occurs {
            min: 0,
            max: Some(u64::MAX)
        }
    );
    let wrapper_int = id(por_ubicacion["#/simpleContent/xs:int"]);
    assert_eq!(
        m.particles[2].term,
        Term::Element {
            name: "g".into(),
            ty: wrapper_int
        }
    );
    assert_eq!(m.particles[2].occurs, Occurs { min: 1, max: None });
    assert_eq!(m.particles[3].term, Term::Choice(vec![]));
    let Rule::Content(doc_m) = &v.node(NodeId::ROOT).rules[0].rule else {
        panic!("documento");
    };
    // El `ref` comparte el tipo de su declaración global.
    assert_eq!(
        doc_m.particles[1].term,
        Term::Element {
            name: "g".into(),
            ty: wrapper_int
        }
    );
    let Rule::Attributes(attrs) = &n("#/complexType/T").rules[1].rule else {
        panic!("Attributes");
    };
    let any = id(por_ubicacion["#/builtin/anySimpleType"]);
    assert_eq!((attrs[0].ty, attrs[1].ty), (any, any));
    assert_eq!(attrs[1].fixed.as_deref(), Some(" 1 "));
    assert_eq!(attrs[2].default.as_deref(), Some("-0"));
    assert_eq!(
        n("#/builtin/anySimpleType").rules[0].rule,
        Rule::Lexical(Lexical::String)
    );
    // Contenido vacío: sin reglas.
    for t in ["V1", "V2", "V3", "V4"] {
        assert!(n(&format!("#/complexType/{t}")).rules.is_empty(), "{t}");
    }
    // `xs:choice` vacío con minOccurs 1: ningún contenido es válido.
    let Rule::Content(m) = &n("#/complexType/N").rules[0].rule else {
        panic!("N");
    };
    assert_eq!(m.particles.len(), 1);
    // Atributo global: no se baja (no hay `ref` de atributo).
    assert!(!(0..v.nodes().len()).any(|i| loc(i).contains("global")));
}

/// B3: documento, declaraciones en preorden (con los tipos anónimos de cada
/// tipo antes que los hermanos siguientes) y sintéticos por primer uso; el
/// tipo anónimo de un elemento global cuelga del documento.
#[test]
fn orden_de_nodos_b3() {
    let doc = format!(
        "<xs:schema {XS}>\
         <xs:element name='r'><xs:complexType><xs:sequence>\
           <xs:element name='a'><xs:complexType><xs:sequence>\
             <xs:element name='a1' type='xs:boolean'/>\
             <xs:element name='a2'><xs:complexType/></xs:element>\
           </xs:sequence></xs:complexType></xs:element>\
           <xs:element name='b' type='B'/>\
         </xs:sequence></xs:complexType></xs:element>\
         <xs:complexType name='B'><xs:sequence>\
           <xs:element name='c'><xs:complexType/></xs:element>\
         </xs:sequence><xs:attribute name='k' type='xs:byte'/></xs:complexType>\
         </xs:schema>"
    );
    let v = ir_de(&doc);
    let orden: Vec<String> = (0..v.nodes().len()).map(|i| v.location(id(i))).collect();
    assert_eq!(
        orden,
        [
            "#",
            "#/element/r",
            "#/element/r/element/a",
            "#/element/r/element/a/element/a2",
            "#/complexType/B",
            "#/complexType/B/element/c",
            "#/simpleContent/xs:boolean",
            "#/builtin/boolean",
            "#/builtin/byte",
        ]
    );
}

/// D39: los valores por defecto sin efecto no llegan a la IR: el esquema
/// con ellos da la misma IR que sin ellos (salvo rangos).
#[test]
fn d39_no_llega_a_la_ir() {
    let plantilla = |s: &str, e: &str, a: &str| {
        format!(
            "<xs:schema {XS}{s}><xs:element name='r' type='T'{e}/>\
             <xs:complexType name='T'><xs:sequence>\
             <xs:element name='a' type='xs:int'{e}/></xs:sequence>\
             <xs:attribute name='k' type='xs:string'{a}/></xs:complexType></xs:schema>"
        )
    };
    let sin = ir_de(&plantilla("", "", ""));
    let con = ir_de(&plantilla(
        " attributeFormDefault='unqualified'",
        " nillable='false'",
        " form='unqualified'",
    ));
    assert_eq!(canonica(con.ir()), canonica(sin.ir()));
}

// ------------------------------------------------------- etapas (B2)

/// B2: todos los errores de la primera etapa que falla, en orden del
/// documento; un error de la etapa 1 oculta los de la 2.
#[test]
fn errores_por_etapas_en_orden_del_documento() {
    // Etapa 2 sola: dos errores, en orden.
    let doc = format!(
        "<xs:schema {XS}><xs:complexType name='T'><xs:sequence>\
         <xs:element name='a' type='xs:int'/><xs:element name='a' type='xs:string'/>\
         <xs:element name='b' type='xs:int' minOccurs='2' maxOccurs='1'/>\
         </xs:sequence></xs:complexType></xs:schema>"
    );
    let segunda = doc.find("<xs:element name='a' type='xs:string'").unwrap() as u32 + 1;
    assert_eq!(
        errores(&doc),
        [
            ("content-model", 1, segunda),
            ("content-model", 1, col(&doc, "1'/>"))
        ]
    );
    // Con un error de la etapa 1, solo ese.
    let con_uno = doc.replace("type='xs:int' minOccurs", "type='Nadie' minOccurs");
    assert_eq!(
        errores(&con_uno),
        [("unresolved", 1, col(&con_uno, "Nadie"))]
    );
}

/// Los mensajes recortan los textos del usuario (nombres y URI de varios
/// MiB) y son de una línea.
#[test]
fn errores_con_nombres_largos_se_recortan() {
    let largo = "n".repeat(1 << 20);
    let uri = format!("urn:{largo}");
    let doc = format!(
        "<xs:schema {XS} xmlns:p='{uri}'><xs:element name='{largo}' type='p:{largo}'/>\
         <xs:element name='{largo}' type='xs:int'/></xs:schema>"
    );
    let schema = parse(doc.as_bytes()).unwrap();
    let errs = analyze(&schema).unwrap_err();
    assert_eq!(errs.len(), 2);
    for e in errs {
        let msg = e.to_string();
        assert!(msg.len() < 1000, "{}", &msg[..200]);
        assert!(!msg.contains('\n'));
    }
}

// --------------------------------------------- tamaño, URI largo y pila

/// Criterio (O(n)): un tipo complejo usado k veces es **una arista por
/// uso**, no una copia: el número de nodos no depende de k y el de aristas
/// crece en uno por uso.
#[test]
fn tipo_usado_k_veces_es_una_arista_por_uso() {
    let ir_con = |k: usize| {
        let usos: String = (0..k)
            .map(|i| format!("<xs:element name='e{i}' type='T'/>"))
            .collect();
        ir_de(&format!(
            "<xs:schema {XS}><xs:element name='r' type='R'/>\
             <xs:complexType name='R'><xs:sequence>{usos}</xs:sequence></xs:complexType>\
             <xs:complexType name='T'><xs:sequence>\
             <xs:element name='t' type='T' minOccurs='0'/>\
             <xs:element name='s' type='xs:string'/></xs:sequence>\
             <xs:attribute name='a' type='xs:int'/></xs:complexType></xs:schema>"
        ))
    };
    let (a, b) = (ir_con(10), ir_con(10_000));
    assert_eq!(a.nodes().len(), b.nodes().len());
    assert_eq!(b.edge_count() - a.edge_count(), 10_000 - 10);
}

/// Criterio (`docs/fase2/pendientes.md` §11, 5.1-SEC3-I1): las tablas se
/// indexan por `(NsId, nombre local)`, no por el texto del URI. Un URI de
/// ~8 MiB como `targetNamespace` y como prefijo de 5·10⁵ referencias
/// `type='p:T'`: con O(referencias · |URI|) serían ~4·10¹² bytes hasheados y
/// la prueba no terminaría. Sin reloj (D1): comprueba el resultado.
#[test]
fn uri_de_8_mib_y_medio_millon_de_referencias() {
    const N: usize = 500_000;
    let uri = format!("urn:{}", "u".repeat(8 << 20));
    let mut doc = format!(
        "<xs:schema {XS} xmlns:p='{uri}' targetNamespace='{uri}'>\
         <xs:element name='r' type='p:R'/><xs:complexType name='T'/>\
         <xs:complexType name='R'><xs:sequence>"
    );
    doc.reserve(N * 40);
    for _ in 0..N {
        doc.push_str("<xs:element name='e' type='p:T'/>");
    }
    doc.push_str("</xs:sequence></xs:complexType></xs:schema>");
    let v = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || compilar(doc.as_bytes()).map(|v| (v.nodes().len(), v.edge_count())))
        .unwrap()
        .join()
        .unwrap();
    // Documento, R, T, y nada más: todas las referencias son aristas a T.
    assert_eq!(v.unwrap(), (3, 1 + N));
}

/// D7: el análisis y la bajada no recurren sobre el AST. La cadena de
/// anidamiento más profunda (`element`/`complexType`/`sequence`, 254
/// niveles, 84 tipos anónimos anidados) se parsea en un hilo de 2 MiB y se
/// analiza y baja en otro de 64 KiB; la liberación del AST vuelve al primero.
#[test]
fn analisis_y_bajada_sin_recursion() {
    let k = 84;
    let doc = format!(
        "<xs:schema {XS}>{}<xs:element name='hoja' type='xs:int'/>{}</xs:schema>",
        "<xs:element name='e'><xs:complexType><xs:sequence>".repeat(k),
        "</xs:sequence></xs:complexType></xs:element>".repeat(k)
    );
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let schema = parse(doc.as_bytes()).unwrap();
            let (schema, n) = std::thread::Builder::new()
                .stack_size(64 << 10)
                .spawn(move || {
                    let n = {
                        let a = analyze(&schema).unwrap_or_else(|_| panic!("análisis"));
                        let v = lower(&a).unwrap_or_else(|_| panic!("bajada"));
                        v.nodes().len()
                    };
                    (schema, n)
                })
                .unwrap()
                .join()
                .unwrap();
            // Documento, 84 tipos anónimos, envoltorio y predefinido.
            assert_eq!(n, 1 + k + 2);
            drop(schema);
        })
        .unwrap()
        .join()
        .unwrap();
}
