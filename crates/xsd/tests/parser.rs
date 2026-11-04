//! Pruebas del parser LL(1) del subconjunto XSD (tarea 5.2,
//! `docs/fase2/gramatica-xsd.md`).
//!
//! Cada prueba fija un criterio de aceptación: el corpus `parse-ok` da un
//! AST (con los esquemas de `tests/cases` y el equivalente de `pedido_xsd`);
//! cada entrada de `parse-error` da el error, la línea y la columna del
//! manifiesto `tests/schemas/errores-xsd.json`; la profundidad se corta en
//! el tope sin desbordar la pila (hilo de 2 MiB) ni leer el resto del
//! documento; `targetNamespace` y los QName de `type`/`base`/`ref` llegan
//! resueltos al AST; y el resultado no depende del tamaño de los bloques.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use xsd::ast::{
    AnonymousType, ComplexContent, Compositor, Element, FacetKind, Particle, Schema, TopLevel, Use,
};
use xsd::{
    parse, parse_chunks, parse_with_max_depth, ErrorKind, Reader, Tok, XsdError, DEFAULT_MAX_DEPTH,
    MAX_DEPTH_LIMIT, XS_NS,
};

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

fn ok(input: &str) -> Schema {
    parse(input.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{input}"))
}

/// (código, línea, columna) del error; el mensaje es de una línea y empieza
/// por la posición.
fn error(input: &str) -> (&'static str, u32, u32) {
    let e = parse(input.as_bytes()).expect_err(input);
    let msg = e.to_string();
    assert!(!msg.contains('\n'), "{msg}");
    assert!(
        msg.starts_with(&format!("línea {}, columna {}: ", e.line(), e.column())),
        "{msg}"
    );
    (e.code(), e.line(), e.column())
}

fn global_element(s: &Schema, i: usize) -> &Element {
    match &s.items[i] {
        TopLevel::Element(e) => e,
        other => panic!("se esperaba un elemento global: {other:?}"),
    }
}

// ------------------------------------------------------------- corpus

/// Criterio: todo `tests/schemas/xsd/parse-ok` da un AST.
#[test]
fn esquemas_correctos_parsean() {
    let fs = corpus("parse-ok");
    assert!(fs.len() >= 9, "corpus demasiado pequeño");
    for path in fs {
        let bytes = std::fs::read(&path).unwrap();
        if let Err(e) = parse(&bytes) {
            panic!("{}: {e}", path.display());
        }
    }
}

/// Criterio: cada entrada de `parse-error` da el código, la línea y la
/// columna del manifiesto, con un mensaje de una línea que contiene el texto
/// declarado (la categoría en los rechazos de D6).
#[test]
fn esquemas_erroneos_reportan_linea_y_columna() {
    let dir = Path::new(TESTS_DIR).join("schemas/xsd/parse-error");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(Path::new(TESTS_DIR).join("schemas/errores-xsd.json")).unwrap(),
    )
    .unwrap();
    let entries = manifest.as_array().unwrap();

    let en_disco: BTreeSet<String> = corpus("parse-error")
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    let declarados: BTreeSet<String> = entries
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        en_disco, declarados,
        "errores-xsd.json y parse-error/ no coinciden"
    );
    assert_eq!(declarados.len(), entries.len(), "entradas repetidas");

    for e in entries {
        let file = e["file"].as_str().unwrap();
        let bytes = std::fs::read(dir.join(file)).unwrap();
        let err = match parse(&bytes) {
            Ok(_) => panic!("{file}: se esperaba un error"),
            Err(err) => err,
        };
        let line = e["line"].as_u64().unwrap();
        let column = e["column"].as_u64().unwrap();
        let msg = err.to_string();
        assert_eq!(
            (err.code(), u64::from(err.line()), u64::from(err.column())),
            (e["code"].as_str().unwrap(), line, column),
            "{file}: {msg}"
        );
        assert!(!msg.contains('\n'), "{file}: {msg}");
        assert!(
            msg.starts_with(&format!("línea {line}, columna {column}: ")),
            "{file}: {msg}"
        );
        if let Some(sub) = e["contains"].as_str() {
            assert!(msg.contains(sub), "{file}: «{msg}» no contiene «{sub}»");
        }
    }
}

/// Criterio: los esquemas XSD de los casos de `tests/cases` dan un AST (y
/// están copiados en `parse-ok`, que los fija aunque cambien los casos).
#[test]
fn esquemas_xml_de_los_casos_parsean() {
    let mut n = 0;
    let cases = Path::new(TESTS_DIR).join("cases");
    for cat in std::fs::read_dir(&cases).unwrap() {
        let cat = cat.unwrap().path();
        if !cat.is_dir() {
            continue;
        }
        for path in files(&cat, "json") {
            let case: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            if case["format"] != "xml" {
                continue;
            }
            let text = match case["schemaFile"].as_str() {
                Some(f) => std::fs::read(path.parent().unwrap().join(f)).unwrap(),
                None => case["schema"].as_str().unwrap().as_bytes().to_vec(),
            };
            if let Err(e) = parse(&text) {
                panic!("{}: {e}", path.display());
            }
            let copia = Path::new(TESTS_DIR)
                .join("schemas/xsd/parse-ok")
                .join(format!("{}.xsd", case["_id"].as_str().unwrap()));
            assert!(copia.exists(), "falta {} en parse-ok", copia.display());
            n += 1;
        }
    }
    assert!(n >= 2, "no hay casos XML en tests/cases");
}

// ---------------------------------------------------------------- AST

/// El equivalente de `pedido_xsd` (`crates/ir/tests/forma.rs`) da el AST
/// esperado: `choice`, `subpedido` opcional, atributo requerido,
/// `maxOccurs="unbounded"` y `simpleContent` con extensión.
#[test]
fn pedido_da_el_ast_esperado() {
    let bytes =
        std::fs::read(Path::new(TESTS_DIR).join("schemas/xsd/parse-ok/pedido.xsd")).unwrap();
    let s = parse(&bytes).unwrap();
    assert!(s.target_namespace.is_none());
    assert_eq!(s.items.len(), 2);
    let pedido = global_element(&s, 0);
    assert_eq!(pedido.name.as_ref().unwrap().value, "pedido");
    let ty = &pedido.ty.as_ref().unwrap().value;
    assert_eq!((ty.ns(), ty.local()), (None, "Pedido"));

    let TopLevel::ComplexType(t) = &s.items[1] else {
        panic!("se esperaba el tipo Pedido");
    };
    assert_eq!(t.name.as_ref().unwrap().value, "Pedido");
    let ComplexContent::Elements { model, attributes } = &t.content else {
        panic!("contenido de elementos");
    };
    let seq = model.as_ref().unwrap();
    assert_eq!(seq.compositor, Compositor::Sequence);
    assert_eq!(seq.particles.len(), 4);
    let Particle::Group(choice) = &seq.particles[1] else {
        panic!("choice");
    };
    assert_eq!(choice.compositor, Compositor::Choice);
    assert_eq!(choice.particles.len(), 2);

    let Particle::Element(item) = &seq.particles[2] else {
        panic!("item");
    };
    assert_eq!(item.max_occurs.as_ref().unwrap().value, "unbounded");
    let Some(AnonymousType::Complex(it)) = &item.anonymous else {
        panic!("tipo anónimo de item");
    };
    let ComplexContent::Simple(sc) = &it.content else {
        panic!("simpleContent");
    };
    assert!(sc.base.value.is(XS_NS, "decimal"));
    assert_eq!(sc.attributes.len(), 2);
    assert_eq!(sc.attributes[0].use_.as_ref().unwrap().value, Use::Required);
    assert_eq!(sc.attributes[1].default.as_ref().unwrap().value, "COP");

    let Particle::Element(sub) = &seq.particles[3] else {
        panic!("subpedido");
    };
    assert_eq!(sub.min_occurs.as_ref().unwrap().value, "0");
    assert_eq!(sub.ty.as_ref().unwrap().value.local(), "Pedido");

    assert_eq!(attributes.len(), 1);
    assert_eq!(attributes[0].name.value, "id");
    assert!(attributes[0].ty.as_ref().unwrap().value.is(XS_NS, "int"));
    assert_eq!(attributes[0].use_.as_ref().unwrap().value, Use::Required);
}

/// Cada construcción lleva el rango de su elemento entero, y cada valor el
/// de su valor con las comillas; `at()` es su primer carácter.
#[test]
fn rangos_de_las_construcciones_y_los_valores() {
    let src = format!("<xs:schema {XS}>\n  <xs:element name='a' type='xs:int'/>\n</xs:schema>");
    let s = ok(&src);
    assert_eq!((s.span.start.offset, s.span.end), (0, src.len() as u64));
    let e = global_element(&s, 0);
    let el = &src[e.span.start.offset as usize..e.span.end as usize];
    assert_eq!(el, "<xs:element name='a' type='xs:int'/>");
    let ty = e.ty.as_ref().unwrap();
    assert_eq!(
        &src[ty.span.start.offset as usize..ty.span.end as usize],
        "'xs:int'"
    );
    assert_eq!((ty.at().line, ty.at().column), (2, 30));
}

/// 5.1-CR3-I1: `targetNamespace` se recorta antes de internarlo, así que
/// `' urn:t '` y el prefijo `t` de `type='t:T'` dan el mismo `NsId`.
#[test]
fn target_namespace_recortado_da_el_mismo_ns_id() {
    let s = ok(&format!(
        "<xs:schema {XS} xmlns:t='urn:t' targetNamespace=' urn:t '>\
         <xs:element name='r' type='t:T'/></xs:schema>"
    ));
    let tns = s.target_namespace.as_ref().unwrap();
    assert_eq!(tns.value.as_str(), "urn:t");
    let ty = global_element(&s, 0).ty.as_ref().unwrap();
    assert_eq!(ty.value.ns_id(), Some(&tns.value));

    // Sin recortar no coincidiría: el lector interna los URI de `xmlns:p`
    // tal cual (por eso lo hace el parser y no la 5.3).
    let mut r = Reader::new(b"<a xmlns:t='urn:t'/>");
    let mut ns = None;
    while let Ok(t) = r.next_token() {
        match t.tok {
            Tok::Attr(a) => ns = Some(a.value().to_owned()),
            Tok::Eof => break,
            _ => {}
        }
    }
    assert_eq!(ns.as_deref(), Some("urn:t"));
    assert_ne!(r.intern(" urn:t "), r.intern("urn:t"));
}

/// 5.1-SEC3-I3 y §11: el `type` se resuelve con el ámbito de su etiqueta
/// aunque el parser ya haya leído un hijo que redeclara el prefijo.
#[test]
fn qname_resuelto_con_el_ambito_de_su_etiqueta() {
    let s = ok(&format!(
        "<xs:schema {XS}>\
         <xs:element name='a' type='p:T' xmlns:p='urn:uno'>\
         <xs:annotation xmlns:p='urn:otro'/></xs:element>\
         <xs:element name='b' xmlns:p='urn:dos'>\
         <xs:simpleType><xs:restriction base='p:B' xmlns:p='urn:tres'/></xs:simpleType>\
         </xs:element></xs:schema>"
    ));
    let a = global_element(&s, 0).ty.as_ref().unwrap();
    assert_eq!(a.value.expanded(), (Some("urn:uno"), "T"));
    let Some(AnonymousType::Simple(st)) = &global_element(&s, 1).anonymous else {
        panic!("tipo simple anónimo");
    };
    let base = st.restriction.base.as_ref().unwrap();
    assert_eq!(base.value.expanded(), (Some("urn:tres"), "B"));
}

/// §9: *collapse* sobre una copia en `name`, `minOccurs`, `maxOccurs`, `use`
/// y las facetas numéricas; `xs:enumeration`, `xs:pattern`, `default` y
/// `fixed` conservan el valor del lector.
#[test]
fn valores_con_collapse_sobre_una_copia() {
    let s = ok(&format!(
        "<xs:schema {XS}>\
         <xs:complexType name=' T  x '><xs:sequence>\
         <xs:element name=' e ' type=' xs:int ' minOccurs=' 0 ' maxOccurs='\tunbounded '/>\
         </xs:sequence>\
         <xs:attribute name='a' use=' required ' fixed='  f  ' default=' d '/>\
         </xs:complexType>\
         <xs:simpleType name='S'><xs:restriction base='xs:string'>\
         <xs:enumeration value=' M '/><xs:maxLength value=' 3 '/><xs:pattern value=' p '/>\
         </xs:restriction></xs:simpleType></xs:schema>"
    ));
    let TopLevel::ComplexType(t) = &s.items[0] else {
        panic!()
    };
    assert_eq!(t.name.as_ref().unwrap().value, "T x");
    let ComplexContent::Elements { model, attributes } = &t.content else {
        panic!()
    };
    let Particle::Element(e) = &model.as_ref().unwrap().particles[0] else {
        panic!()
    };
    assert_eq!(e.name.as_ref().unwrap().value, "e");
    assert!(e.ty.as_ref().unwrap().value.is(XS_NS, "int"));
    assert_eq!(e.min_occurs.as_ref().unwrap().value, "0");
    assert_eq!(e.max_occurs.as_ref().unwrap().value, "unbounded");
    let a = &attributes[0];
    assert_eq!(a.use_.as_ref().unwrap().value, Use::Required);
    assert_eq!(a.fixed.as_ref().unwrap().value, "  f  ");
    assert_eq!(a.default.as_ref().unwrap().value, " d ");
    let TopLevel::SimpleType(st) = &s.items[1] else {
        panic!()
    };
    let f: Vec<(FacetKind, &str)> = st
        .restriction
        .facets
        .iter()
        .map(|f| (f.kind, f.value.value.as_str()))
        .collect();
    assert_eq!(
        f,
        [
            (FacetKind::Enumeration, " M "),
            (FacetKind::MaxLength, "3"),
            (FacetKind::Pattern, " p ")
        ]
    );
}

/// D23: `xs:annotation`, su contenido (XML cualquiera) y `id` no dejan nada
/// en el AST.
#[test]
fn anotaciones_e_id_se_ignoran() {
    let bytes =
        std::fs::read(Path::new(TESTS_DIR).join("schemas/xsd/parse-ok/anotaciones.xsd")).unwrap();
    let s = parse(&bytes).unwrap();
    assert_eq!(s.items.len(), 2);
    let raiz = global_element(&s, 0);
    let Some(AnonymousType::Complex(t)) = &raiz.anonymous else {
        panic!()
    };
    let ComplexContent::Elements { model, attributes } = &t.content else {
        panic!()
    };
    assert_eq!(model.as_ref().unwrap().particles.len(), 1);
    assert_eq!(attributes.len(), 1);
}

/// Texto: los espacios entre elementos y las instrucciones de procesamiento
/// se descartan; otro texto solo vale dentro de `xs:documentation`.
#[test]
fn texto_y_contenido_de_solo_elementos() {
    ok(&format!(
        "<xs:schema {XS}>\n\t<?pi x?>\r\n<xs:annotation><xs:documentation>hola <b>y</b> \
         adiós</xs:documentation></xs:annotation></xs:schema>"
    ));
    let h = cabeza() as u32;
    assert_eq!(
        error(&format!("<xs:schema {XS}>x</xs:schema>")),
        ("syntax", 1, h + 1)
    );
    assert_eq!(
        error(&format!(
            "<xs:schema {XS}><xs:annotation>x</xs:annotation></xs:schema>"
        )),
        ("syntax", 1, h + 16)
    );
    assert_eq!(
        error(&format!(
            "<xs:schema {XS}><xs:element name='a'>&#65;</xs:element></xs:schema>"
        )),
        ("syntax", 1, h + 22)
    );
}

// ---------------------------------------------------------- profundidad

/// Longitud de `<xs:schema xmlns:xs='…'>`.
fn cabeza() -> usize {
    format!("<xs:schema {XS}>").len()
}

/// `xs:schema` > `xs:complexType` > `n` `xs:sequence` anidadas: un documento
/// de profundidad `n + 2`.
fn secuencias(n: usize) -> String {
    format!(
        "<xs:schema {XS}><xs:complexType name='T'>{}{}</xs:complexType></xs:schema>",
        "<xs:sequence>".repeat(n),
        "</xs:sequence>".repeat(n)
    )
}

/// Las cadenas de anidamiento que recorren la recursión del parser: cada
/// una da `niveles` elementos por repetición.
fn peores_casos(profundidad: usize) -> Vec<(&'static str, String)> {
    // xs:schema + xs:complexType = 2 niveles fijos.
    let seq = secuencias(profundidad - 2);
    let choice = seq.replace("xs:sequence", "xs:choice");
    // element > complexType > sequence: 3 niveles por repetición; el
    // primero va directamente bajo xs:schema.
    let k = (profundidad - 1) / 3;
    let cadena = format!(
        "<xs:schema {XS}>{}{}</xs:schema>",
        "<xs:element name='e'><xs:complexType><xs:sequence>".repeat(k),
        "</xs:sequence></xs:complexType></xs:element>".repeat(k)
    );
    // element > simpleType > restriction > simpleType > restriction …
    let j = (profundidad - 2) / 2;
    let simple = format!(
        "<xs:schema {XS}><xs:element name='e'>{}<xs:simpleType><xs:restriction base='xs:int'/></xs:simpleType>{}</xs:element></xs:schema>",
        "<xs:simpleType><xs:restriction>".repeat(j - 1),
        "</xs:restriction></xs:simpleType>".repeat(j - 1)
    );
    // Anotación con contenido anidado (se salta sin recursión).
    let doc = format!(
        "<xs:schema {XS}><xs:annotation><xs:documentation>{}{}</xs:documentation></xs:annotation></xs:schema>",
        "<a>".repeat(profundidad - 3),
        "</a>".repeat(profundidad - 3)
    );
    vec![
        ("sequence", seq),
        ("choice", choice),
        ("element/complexType/sequence", cadena),
        ("simpleType/restriction", simple),
        ("documentation", doc),
    ]
}

/// Profundidad del documento: elementos abiertos a la vez como máximo.
fn profundidad(doc: &str) -> usize {
    let mut r = Reader::new(doc.as_bytes());
    let (mut d, mut max) = (0usize, 0usize);
    loop {
        match r.next_token().unwrap().tok {
            Tok::STag(_) => {
                d += 1;
                max = max.max(d);
            }
            Tok::EmptyClose | Tok::ETag(_) => d -= 1,
            Tok::Eof => return max,
            _ => {}
        }
    }
}

/// D7 y §5: a la profundidad máxima, cada camino de la recursión cabe en un
/// hilo de exactamente 2 MiB también sin optimizar (libtest da 2 MiB, pero
/// se fija para no depender de `RUST_MIN_STACK`), incluida la liberación
/// del AST; un nivel más es el error `limit` en el `<` que lo supera.
#[test]
fn profundidad_maxima_en_un_hilo_de_2_mib() {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(|| {
            for (que, doc) in peores_casos(DEFAULT_MAX_DEPTH as usize) {
                let d = profundidad(&doc);
                assert!(
                    d + 2 >= DEFAULT_MAX_DEPTH as usize && d <= 256,
                    "{que}: {d}"
                );
                let s = parse(doc.as_bytes()).unwrap_or_else(|e| panic!("{que}: {e}"));
                drop(s);
            }
            for (que, doc) in peores_casos(DEFAULT_MAX_DEPTH as usize + 3) {
                let e = parse(doc.as_bytes()).expect_err(que);
                assert_eq!(e.code(), "limit", "{que}: {e}");
                assert_eq!(e.kind, ErrorKind::TooDeep { max: 256 }, "{que}");
                assert!(e.to_string().contains("256"), "{que}");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// El tope solo se configura a la baja: cualquier valor mayor se recorta a
/// 256, y el error informa el máximo efectivo.
#[test]
fn profundidad_configurable_acotada() {
    let doc = secuencias(100_000);
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            for max in [u32::MAX, MAX_DEPTH_LIMIT + 1, 100_000] {
                let e = parse_with_max_depth(doc.as_bytes(), max).unwrap_err();
                assert_eq!(
                    e.kind,
                    ErrorKind::TooDeep {
                        max: MAX_DEPTH_LIMIT
                    }
                );
            }
            // A la baja se respeta: el elemento 11 (la secuencia 9) es el
            // error, en su `<`; con 10 niveles el esquema es válido.
            let e = parse_with_max_depth(doc.as_bytes(), 10).unwrap_err();
            assert_eq!(e.kind, ErrorKind::TooDeep { max: 10 });
            let col = cabeza() + "<xs:complexType name='T'>".len() + 8 * "<xs:sequence>".len() + 1;
            assert_eq!((e.line(), e.column() as usize), (1, col));
            parse_with_max_depth(secuencias(8).as_bytes(), 10).unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}

/// §12: con 10⁶ niveles el corte ocurre en cuanto se pasa el tope, sin
/// seguir leyendo el documento (se cuentan los bloques pedidos), y la pila
/// de elementos abiertos del lector no pasa de 256.
#[test]
fn corte_por_profundidad_sin_leer_el_resto() {
    const N: usize = 1_000_000;
    let cabeza = format!("<xs:schema {XS}><xs:complexType name='T'>");
    let pedidos = Cell::new(0usize);
    // Un bloque por nivel; el documento entero nunca se construye.
    let bloques = std::iter::once(cabeza.clone().into_bytes())
        .chain((0..N).map(|_| b"<xs:sequence>".to_vec()))
        .inspect(|_| pedidos.set(pedidos.get() + 1));
    let e = parse_chunks(bloques, DEFAULT_MAX_DEPTH).unwrap_err();
    assert_eq!(e.kind, ErrorKind::TooDeep { max: 256 });
    // Elemento 257 = sequence número 255 (tras schema y complexType).
    let col = cabeza.len() + 254 * "<xs:sequence>".len() + 1;
    assert_eq!((e.line(), e.column() as usize), (1, col));
    assert_eq!(pedidos.get(), 1 + 255, "se leyeron bloques de más");

    // El lector con tope: al fallar guarda 256 elementos abiertos.
    let doc = format!("{cabeza}{}", "<a>".repeat(1000));
    let mut r = Reader::new(doc.as_bytes()).with_max_depth(256);
    let e = loop {
        if let Err(e) = r.next_token() {
            break e;
        }
    };
    assert_eq!(e.code(), "limit");
    assert!(format!("{r:?}").contains("depth: 256"), "{r:?}");
}

/// O(n) sin reloj (D1): un URI de 1 MiB como `targetNamespace` y como
/// prefijo de 5·10⁴ referencias `type='t:T'`. Ningún paso del parser vuelve
/// a leer el URI por referencia (con O(referencias · |URI|) serían 5·10¹⁰
/// bytes); todas comparten el `NsId` del `targetNamespace`.
#[test]
fn uri_largo_y_muchas_referencias_en_una_pasada() {
    const N: usize = 50_000;
    let uri = format!("urn:{}", "u".repeat(1 << 20));
    let mut doc = format!("<xs:schema {XS} xmlns:t='{uri}' targetNamespace='{uri}'>");
    for i in 0..N {
        doc.push_str(&format!("<xs:element name='e{i}' type='t:T'/>"));
    }
    doc.push_str("</xs:schema>");
    let s = ok(&doc);
    assert_eq!(s.items.len(), N);
    let tns = &s.target_namespace.as_ref().unwrap().value;
    for i in [0, N / 2, N - 1] {
        let ty = &global_element(&s, i).ty.as_ref().unwrap().value;
        assert_eq!(ty.ns_id(), Some(tns));
    }
}

// ------------------------------------------------------------- errores

/// §13: los textos del usuario en los errores del parser se recortan al
/// crearlos; el mensaje sigue siendo de una línea y acotado.
#[test]
fn errores_con_nombres_largos_se_recortan() {
    let largo = "x".repeat(1 << 20);
    for doc in [
        format!("<xs:schema {XS}><xs:{largo}/></xs:schema>"),
        format!("<xs:schema {XS}><{largo}/></xs:schema>"),
        format!("<xs:schema {XS}><xs:element name='a' {largo}='1'/></xs:schema>"),
        format!("<xs:schema {XS}><xs:complexType name='T' mixed='{largo}'/></xs:schema>"),
        format!("<xs:schema {XS} elementFormDefault='{largo}'/>"),
        format!("<xs:schema {XS}><xs:element name='a' type='{largo}:{largo}:'/></xs:schema>"),
    ] {
        let e = parse(doc.as_bytes()).unwrap_err();
        let msg = e.to_string();
        assert!(msg.len() < 1000, "{}", &msg[..200]);
        let texto = match &e.kind {
            ErrorKind::UnknownElement(s)
            | ErrorKind::ForeignElement(s)
            | ErrorKind::InvalidQName(s) => s,
            ErrorKind::UnknownAttribute { name, .. } => name,
            ErrorKind::InvalidValue { found, .. } => found,
            k => panic!("{k:?}"),
        };
        assert!(texto.chars().count() <= 33, "{}", texto.len());
    }
}

/// D6: cada construcción de §3.3 que no tiene archivo en el corpus
/// (`notation`, `field`…), rechazada con su categoría.
#[test]
fn construcciones_fuera_del_subconjunto() {
    for (local, cat) in [
        ("notation", "notación"),
        ("selector", "clase (c)"),
        ("field", "clase (c)"),
        ("assert", "XML Schema 1.1"),
    ] {
        let doc = format!("<xs:schema {XS}><xs:{local}/></xs:schema>");
        let e = parse(doc.as_bytes()).unwrap_err();
        assert_eq!(
            (e.code(), e.line(), e.column() as usize),
            ("unsupported", 1, cabeza() + 1)
        );
        assert!(e.to_string().contains(cat), "{e}");
    }
}

/// El primer error en el orden del documento (CR-I2): los atributos de una
/// etiqueta se comprueban en su orden, antes de leer lo que la sigue, y un
/// atributo obligatorio que falta se informa en el `>` o `/>`, donde se sabe.
#[test]
fn primer_error_en_orden_del_documento() {
    let doc = format!(
        "<xs:schema {XS}><xs:complexType name='T' mixed='x'>&bogus;</xs:complexType></xs:schema>"
    );
    let col = doc.find("x'>").unwrap() as u32 + 1;
    assert_eq!(error(&doc), ("value", 1, col));

    // Dos QName erróneos: el de `ref`, que va antes, aunque el código pida
    // `type` primero.
    let doc = format!(
        "<xs:schema {XS}><xs:complexType name='T'><xs:sequence>\
         <xs:element ref='u:x' type='v:y'/></xs:sequence></xs:complexType></xs:schema>"
    );
    let e = parse(doc.as_bytes()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UndeclaredPrefix("u".into()));
    assert_eq!(e.column() as usize, doc.find("u:x").unwrap() + 1);

    // `use` inválido antes de un `type` con prefijo sin declarar.
    let doc = format!(
        "<xs:schema {XS}><xs:complexType name='T'>\
         <xs:attribute use='zz' name='a' type='v:y'/></xs:complexType></xs:schema>"
    );
    let col = doc.find("zz").unwrap() as u32 + 1;
    assert_eq!(error(&doc), ("value", 1, col));

    // Falta `name` (se sabe en el `/>`) y antes hay un `nillable`.
    let doc = format!("<xs:schema {XS}><xs:element nillable='true'/></xs:schema>");
    let col = doc.find("nillable").unwrap() as u32 + 1;
    assert_eq!(error(&doc), ("unsupported", 1, col));
    let doc = format!("<xs:schema {XS}><xs:element id='e'/></xs:schema>");
    let col = doc.find("/>").unwrap() as u32 + 1;
    assert_eq!(error(&doc), ("syntax", 1, col));
}

/// Forma de depuración del AST sin los rangos (`Span { … }`), para comparar
/// dos esquemas que solo difieren en posiciones.
fn sin_rangos(items: &[TopLevel]) -> String {
    let s = format!("{items:?}");
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find("Span { start: Pos {") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        // `Span { start: Pos { … }, end: N }`: hasta la segunda `}`.
        let first = tail.find('}').unwrap();
        let second = first + 1 + tail[first + 1..].find('}').unwrap();
        out.push_str("Span");
        rest = &tail[second + 1..];
    }
    out.push_str(rest);
    out
}

/// D34: los atributos de otro espacio de nombres que el de XSD se aceptan y
/// se ignoran en cualquier construcción, y no llegan al AST: el esquema da
/// el mismo AST con ellos y sin ellos, también con una etiqueta llena de
/// ellos. Uno calificado con el propio espacio de XSD sigue siendo un error.
#[test]
fn atributos_de_otro_espacio_de_nombres_se_ignoran() {
    let plantilla = |extra: &str| {
        format!(
            "<xs:schema {XS} xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' \
             xmlns:x='urn:x'{extra}>\
             <xs:element name='a'{extra}><xs:complexType{extra}><xs:sequence{extra}>\
             <xs:element name='b' type='xs:int'{extra}/></xs:sequence>\
             <xs:attribute name='k' type='xs:string'{extra}/></xs:complexType></xs:element>\
             <xs:simpleType name='S'{extra}><xs:restriction base='xs:int'{extra}>\
             <xs:maxInclusive value='3'{extra}/></xs:restriction></xs:simpleType>\
             </xs:schema>"
        )
    };
    let sin = ok(&plantilla(""));
    for extra in [
        " xml:lang='es'",
        " xsi:type='xs:int' xsi:nil='true'",
        " x:version='2' x:type='otro'",
    ] {
        let con = ok(&plantilla(extra));
        assert_eq!(sin_rangos(&con.items), sin_rangos(&sin.items), "{extra}");
    }
    let muchos: String = (0..10_000).map(|i| format!(" x:a{i}='{i}'")).collect();
    let con = ok(&plantilla(&muchos));
    assert_eq!(sin_rangos(&con.items), sin_rangos(&sin.items));

    // Calificado con el espacio de XSD: no es de XSD ni de otro espacio.
    let doc = plantilla(" xs:type='xs:int'");
    let e = parse(doc.as_bytes()).unwrap_err();
    assert_eq!(e.code(), "syntax");
    assert!(
        matches!(&e.kind, ErrorKind::UnknownAttribute { name, .. } if name == "xs:type"),
        "{e}"
    );
}

/// CR-I3: lo que la lista blanca y la gramática aceptan sin archivo propio
/// en el corpus de errores, y la celda vacía con un `ETAG`.
#[test]
fn lista_blanca_y_celdas_aceptadas() {
    ok(&format!(
        "<xs:schema {XS}><xs:complexType name='T' mixed=' 0 '/>\
         <xs:annotation><xs:documentation source='s'/><xs:appinfo source='s'/></xs:annotation>\
         </xs:schema>"
    ));
    let doc = format!(
        "<xs:schema {XS}><xs:complexType name='T'><xs:simpleContent></xs:simpleContent>\
         </xs:complexType></xs:schema>"
    );
    let e = parse(doc.as_bytes()).unwrap_err();
    assert!(
        matches!(
            &e.kind,
            ErrorKind::Unexpected { found: xsd::Found::EndTag(n), .. } if n == "xs:simpleContent"
        ),
        "{e}"
    );
}

// ------------------------------------------------- tamaño de los bloques

fn resultado(r: Result<Schema, XsdError>) -> String {
    // `NsId` se compara por identidad (un lector por análisis): se compara
    // la forma de depuración, que muestra el URI.
    format!("{r:?}")
}

/// Fase 1 «terminado cuando»: el mismo AST o el mismo error con bloques de
/// 1, 7 y 4096 bytes, en todo el corpus.
#[test]
fn mismo_resultado_con_bloques_de_1_7_y_4096() {
    for path in corpus("parse-ok").into_iter().chain(corpus("parse-error")) {
        let bytes = std::fs::read(&path).unwrap();
        let entero = resultado(parse(&bytes));
        for b in [1, 7, 4096] {
            let partido = resultado(parse_chunks(bytes.chunks(b), DEFAULT_MAX_DEPTH));
            assert_eq!(partido, entero, "{} con bloques de {b}", path.display());
        }
    }
}

/// Fragmentos de esquema casi válidos, para que las pruebas de propiedades
/// lleguen al parser y no solo al lexer.
fn fragmento() -> impl Strategy<Value = Vec<u8>> {
    prop::sample::select(vec![
        "<xs:element name='a'>",
        "<xs:element ref='a'/>",
        "</xs:element>",
        "<xs:complexType>",
        "</xs:complexType>",
        "<xs:sequence>",
        "</xs:sequence>",
        "<xs:choice minOccurs='0'>",
        "</xs:choice>",
        "<xs:all/>",
        "<xs:attribute name='x' type='xs:int'/>",
        "<xs:simpleType><xs:restriction base='xs:int'>",
        "<xs:enumeration value='1'/>",
        "</xs:restriction></xs:simpleType>",
        "<xs:annotation><xs:documentation>",
        "</xs:documentation></xs:annotation>",
        "<xs:any/>",
        "texto",
        " ",
        "<?pi?>",
        "<b xmlns:p='u'>",
        "</b>",
    ])
    .prop_map(|s: &str| s.as_bytes().to_vec())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sin pánico, y el mismo resultado entero y en bloques de 1 y 7 bytes.
    #[test]
    fn fragmentos_arbitrarios_sin_panico_y_sin_depender_del_bloque(
        partes in prop::collection::vec(fragmento(), 0..24)
    ) {
        let mut doc = format!("<xs:schema {XS}>").into_bytes();
        for p in &partes {
            doc.extend_from_slice(p);
        }
        doc.extend_from_slice(b"</xs:schema>");
        let entero = resultado(parse(&doc));
        for b in [1, 7] {
            prop_assert_eq!(resultado(parse_chunks(doc.chunks(b), DEFAULT_MAX_DEPTH)), entero.clone());
        }
    }

    /// Bytes arbitrarios: nunca un pánico.
    #[test]
    fn bytes_arbitrarios_sin_panico(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = parse(&bytes);
    }
}
