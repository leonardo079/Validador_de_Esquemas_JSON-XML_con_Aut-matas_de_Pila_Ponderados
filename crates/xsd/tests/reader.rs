//! Pruebas del lector XML de documentos de esquema (tarea 5.1,
//! `docs/fase2/lector-xsd.md`).
//!
//! Cada prueba fija un criterio de aceptación de la tarea: la sucesión de
//! tokens de un documento bien formado con los nombres resueltos por espacio
//! de nombres, un error con línea y columna por cada restricción de buena
//! formación que añade el lector, el mismo resultado con bloques de 1, 7 y
//! 4096 bytes, y ningún pánico ni recursión ante entradas hostiles.

use std::path::Path;

use proptest::prelude::*;
use xsd::{ErrorKind, Name, Reader, Tok, Token, XsdError, XMLNS_NS, XML_NS, XS_NS};

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

/// Tokens hasta `Eof` (incluido) o hasta el primer error.
fn leer_trozos<'a>(trozos: impl Iterator<Item = &'a [u8]>) -> (Vec<Token>, Option<XsdError>) {
    let mut r = Reader::from_chunks(trozos);
    let mut out = Vec::new();
    loop {
        match r.next_token() {
            Ok(t) => {
                let fin = t.tok == Tok::Eof;
                out.push(t);
                if fin {
                    return (out, None);
                }
            }
            Err(e) => return (out, Some(e)),
        }
    }
}

fn leer_en_bloques(input: &[u8], bloque: usize) -> (Vec<Token>, Option<XsdError>) {
    leer_trozos(input.chunks(bloque.max(1)))
}

/// `{uri}local` o `local` si no tiene espacio de nombres.
fn expandido(n: &Name) -> String {
    match n.ns() {
        Some(ns) => format!("{{{ns}}}{}", n.local()),
        None => n.local().to_string(),
    }
}

/// Forma compacta de los tokens: `<{uri}local`, `@{uri}local=valor`, `>`,
/// `/>`, `</{uri}local>`, `"texto` y `?nombre datos`.
fn render(toks: &[Token]) -> Vec<String> {
    toks.iter()
        .map(|t| match &t.tok {
            Tok::STag(n) => format!("<{}", expandido(n)),
            Tok::Attr(a) => format!("@{}={}", expandido(&a.name), a.value()),
            Tok::Gt => ">".into(),
            Tok::EmptyClose => "/>".into(),
            Tok::ETag(n) => format!("</{}>", expandido(n)),
            Tok::Text(s) => format!("\"{s}"),
            Tok::Pi { target, data } => format!("?{target} {data}"),
            Tok::Eof => "$".into(),
        })
        .collect()
}

/// Tokens de una entrada que debe ser bien formada, sin los textos de solo
/// espacios (para que las comparaciones sean legibles).
fn tokens_ok(input: &str) -> Vec<String> {
    let (toks, err) = leer_en_bloques(input.as_bytes(), usize::MAX);
    assert_eq!(err, None, "{input}");
    render(&toks)
        .into_iter()
        .filter(|s| !(s.starts_with('"') && s[1..].trim().is_empty()))
        .collect()
}

/// (código, línea, columna) del error de una entrada mal formada.
fn error(input: &str) -> (&'static str, u32, u32) {
    let (_, err) = leer_en_bloques(input.as_bytes(), usize::MAX);
    let e = err.unwrap_or_else(|| panic!("se esperaba un error: {input}"));
    // Mensaje de una línea, con la posición delante.
    let msg = e.to_string();
    assert!(!msg.contains('\n'), "{msg}");
    assert!(
        msg.starts_with(&format!("línea {}, columna {}: ", e.line(), e.column())),
        "{msg}"
    );
    (e.code(), e.line(), e.column())
}

/// Campo de texto de un caso de `tests/cases` (texto original: es una
/// cadena JSON, que `serde_json` decodifica sin pérdida).
fn campo(caso: &str, campo: &str) -> serde_json::Value {
    let path = Path::new(TESTS_DIR).join("cases").join(caso);
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    v[campo].clone()
}

fn xs(local: &str) -> String {
    format!("{{{XS_NS}}}{local}")
}

// ------------------------------------------------- documentos bien formados

/// El esquema de `valid-xml-pedido-01` da la sucesión de tokens esperada,
/// con los nombres de `xs:` resueltos al espacio de XML Schema.
#[test]
fn esquema_del_pedido_da_los_tokens_esperados() {
    let schema = campo("valid/valid-xml-pedido-01.json", "schema");
    let toks = tokens_ok(schema.as_str().unwrap());
    let el = xs("element");
    let want = vec![
        format!("<{}", xs("schema")),
        format!("@{{{XMLNS_NS}}}xs={XS_NS}"),
        ">".into(),
        format!("<{el}"),
        "@name=pedido".into(),
        ">".into(),
        format!("<{}", xs("complexType")),
        ">".into(),
        format!("<{}", xs("sequence")),
        ">".into(),
        format!("<{el}"),
        "@name=cliente".into(),
        "@type=xs:string".into(),
        "/>".into(),
        format!("<{el}"),
        "@name=nota".into(),
        "@type=xs:string".into(),
        "@minOccurs=0".into(),
        "/>".into(),
        format!("<{el}"),
        "@name=total".into(),
        "@type=xs:decimal".into(),
        "/>".into(),
        format!("</{}>", xs("sequence")),
        format!("<{}", xs("attribute")),
        "@name=id".into(),
        "@type=xs:positiveInteger".into(),
        "@use=required".into(),
        "/>".into(),
        format!("</{}>", xs("complexType")),
        format!("</{el}>"),
        format!("</{}>", xs("schema")),
        "$".into(),
    ];
    assert_eq!(toks, want);
}

/// Los dos esquemas XSD del corpus (`format: "xml"`) se leen sin error.
#[test]
fn esquemas_xml_del_corpus_bien_formados() {
    for caso in [
        "valid/valid-xml-pedido-01.json",
        "malformed/malformed-xml-billion-laughs-01.json",
    ] {
        let schema = campo(caso, "schema");
        let (toks, err) = leer_en_bloques(schema.as_str().unwrap().as_bytes(), 7);
        assert_eq!(err, None, "{caso}");
        assert_eq!(toks.last().map(|t| &t.tok), Some(&Tok::Eof), "{caso}");
    }
}

/// `xs:`, `xsd:` y el espacio por defecto dan los mismos nombres expandidos.
#[test]
fn cualquier_prefijo_resuelve_al_mismo_nombre() {
    let con = |abre: &str, p: &str| {
        tokens_ok(&format!(
            "<{p}schema {abre}><{p}element name=\"a\" type=\"{p}string\"/></{p}schema>"
        ))
        .into_iter()
        // La declaración de espacio de nombres es lo único que cambia.
        .filter(|s| !s.starts_with(&format!("@{{{XMLNS_NS}}}")))
        .collect::<Vec<_>>()
    };
    let xs_ = con(&format!("xmlns:xs=\"{XS_NS}\""), "xs:");
    let xsd_ = con(&format!("xmlns:xsd=\"{XS_NS}\""), "xsd:");
    let defecto = con(&format!("xmlns=\"{XS_NS}\""), "");
    let el = xs("element");
    let nombres = |v: &[String]| -> Vec<String> {
        v.iter()
            .filter(|s| !s.starts_with("@type"))
            .cloned()
            .collect()
    };
    assert_eq!(nombres(&xs_), nombres(&xsd_));
    assert_eq!(nombres(&xs_), nombres(&defecto));
    assert!(xs_.contains(&format!("<{el}")));
    // Los atributos sin prefijo no toman el espacio por defecto.
    assert!(defecto.contains(&"@name=a".to_string()));
}

/// Ámbito de las declaraciones: una redeclaración interna tapa la externa
/// solo dentro de su elemento; `xmlns=""` quita el espacio por defecto.
#[test]
fn ambito_de_las_declaraciones() {
    let toks = tokens_ok(
        "<p:a xmlns:p=\"u1\" xmlns=\"d\">\
           <p:b xmlns:p=\"u2\"><c/></p:b>\
           <p:b/><c xmlns=\"\"/><x:c xmlns:x=\"u1\"/>\
         </p:a>",
    );
    let sin_decl: Vec<String> = toks
        .into_iter()
        .filter(|s| !s.starts_with(&format!("@{{{XMLNS_NS}}}")))
        .collect();
    assert_eq!(
        sin_decl,
        [
            "<{u1}a", ">", "<{u2}b", ">", "<{d}c", "/>", "</{u2}b>", "<{u1}b", "/>", "<c", "/>",
            "<{u1}c", "/>", "</{u1}a>", "$"
        ]
    );
}

/// El prefijo `xml` está siempre declarado; los atributos `xmlns` y
/// `xmlns:p` están en el espacio de nombres de `xmlns` (Namespaces in XML).
#[test]
fn prefijos_xml_y_xmlns() {
    let toks = tokens_ok("<a xml:lang=\"es\" xmlns:p=\"u\" xmlns=\"d\"/>");
    assert_eq!(
        toks,
        [
            "<{d}a".to_string(),
            format!("@{{{XML_NS}}}lang=es"),
            format!("@{{{XMLNS_NS}}}p=u"),
            format!("@{{{XMLNS_NS}}}xmlns=d"),
            "/>".into(),
            "$".into(),
        ]
    );
}

/// Declaración XML válida, BOM, comentarios, CDATA e instrucciones de
/// procesamiento.
#[test]
fn declaracion_bom_y_misc() {
    let doc = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone='yes' ?>\n\
               <!-- c --><?pi datos?>\n<a>x<!-- c -->y<![CDATA[<z>]]></a>\n<?fin?>\n";
    let (toks, err) = leer_en_bloques(doc.as_bytes(), 1);
    assert_eq!(err, None);
    assert_eq!(
        render(&toks),
        [
            "?xml version=\"1.0\" encoding=\"utf-8\" standalone='yes' ",
            "\"\n",
            "?pi datos",
            "\"\n",
            "<a",
            ">",
            "\"xy<z>",
            "</a>",
            "\"\n",
            "?fin ",
            "\"\n",
            "$"
        ]
    );
    // El texto contiguo es un solo token, con su rango.
    let texto = toks
        .iter()
        .find(|t| t.tok == Tok::Text("xy<z>".into()))
        .unwrap();
    assert_eq!((texto.span.start.line, texto.span.start.column), (3, 4));
}

/// Rangos de los tokens: la etiqueta desde `<` hasta el fin del nombre, el
/// atributo con su valor, la etiqueta de cierre entera.
#[test]
fn rangos_de_los_tokens() {
    let doc = "<a  b='1'>t</a>";
    let (toks, _) = leer_en_bloques(doc.as_bytes(), usize::MAX);
    let r: Vec<(u64, u64)> = toks
        .iter()
        .map(|t| (t.span.start.offset, t.span.end))
        .collect();
    assert_eq!(r, [(0, 2), (4, 9), (9, 10), (10, 11), (11, 15), (15, 15)]);
    let Tok::Attr(a) = &toks[1].tok else { panic!() };
    assert_eq!((a.value_span.start.offset, a.value_span.end), (6, 9));
    // `/>` ocupa sus dos bytes, con la línea y la columna del `/`.
    let (toks, _) = leer_en_bloques(b"<a\n b='1'/>", 1);
    let cierre = &toks[2].span;
    assert_eq!(
        (
            cierre.start.offset,
            cierre.end,
            cierre.start.line,
            cierre.start.column
        ),
        (9, 11, 2, 7)
    );
}

/// Fines de línea (XML 1.0 §2.11) en el texto y normalización de los
/// valores de atributo (§3.3.3): lo literal se normaliza, las referencias
/// de carácter se conservan.
#[test]
fn fines_de_linea_y_valores_de_atributo() {
    let doc = "<a v='x\r\ny\tz\n&#9;&#10;&#13;'>1\r\n2\r3\r<!-- c -->\n4&#13;\n</a>";
    for bloque in [1, 7, 4096] {
        let (toks, err) = leer_en_bloques(doc.as_bytes(), bloque);
        assert_eq!(err, None);
        let Tok::Attr(a) = &toks[1].tok else { panic!() };
        assert_eq!(a.value(), "x y z \t\n\r");
        // `\r` + comentario + `\n` no es un fin de línea `\r\n`.
        assert_eq!(toks[3].tok, Tok::Text("1\n2\n3\n\n4\r\n".into()));
    }
}

// -------------------------------------------------- resolución de QName

/// Todos los tokens de una entrada bien formada, leídos hasta `Eof` **antes**
/// de resolver nada: así se simula un parser que lee por adelantado.
fn todos(input: &[u8]) -> Vec<Token> {
    let (toks, err) = leer_en_bloques(input, usize::MAX);
    assert_eq!(err, None, "{}", String::from_utf8_lossy(input));
    toks
}

/// `Attr::as_qname` de cada atributo con nombre local `attr`, en orden:
/// `{uri}local`, o (código, línea, columna) del error.
fn qnames(toks: &[Token], attr: &str) -> Vec<Result<String, (&'static str, u32, u32)>> {
    toks.iter()
        .filter_map(|t| match &t.tok {
            Tok::Attr(a) if a.name.local() == attr => Some(
                a.as_qname()
                    .map(|n| expandido(&n))
                    .map_err(|e| (e.code(), e.line(), e.column())),
            ),
            _ => None,
        })
        .collect()
}

/// Los QName de los valores (`type`, `base`, `ref`) se resuelven contra el
/// ámbito de **su** etiqueta, con el espacio por defecto incluido, aunque se
/// pidan después de leer todo el documento. El error va en el primer
/// carácter del valor (el que sigue a la comilla).
#[test]
fn qname_de_valores_contra_el_ambito_de_su_etiqueta() {
    let doc = format!(
        "<s xmlns:xsd=\"{XS_NS}\" xmlns=\"tns\">\
           <e type=\"xsd:string\"/>\
           <e xmlns:xsd=\"otro\" type=\" xsd:int \"/>\
           <e type=\"local\"/>\
           <e type=\"zz:local\"/>\
         </s>"
    );
    assert_eq!(
        qnames(&todos(doc.as_bytes()), "type"),
        [
            Ok(xs("string")),
            Ok("{otro}int".to_string()),
            Ok("{tns}local".to_string()),
            Err(("namespace", 1, 147)),
        ]
    );
}

/// Un parser LL(1) que imita al de JSON Schema pide el token siguiente en
/// cuanto consume uno: cuando mira el `type` de un elemento, el lector ya
/// puede haber leído la etiqueta del hijo, que redeclara el mismo prefijo e
/// incluso el de XML Schema. La resolución sigue siendo la del padre.
#[test]
fn qname_resuelto_tras_leer_por_adelantado_un_hijo_que_redeclara() {
    let doc = format!(
        "<xs:schema xmlns:xs='{XS_NS}' xmlns:p='u'>\
           <xs:element name='e' type='p:T' ref='xs:string'>\
             <xs:complexType xmlns:p='otro' xmlns:xs='no-xs' base='p:T'/>\
           </xs:element>\
         </xs:schema>"
    );
    let mut r = Reader::new(doc.as_bytes());
    // Se guardan los atributos del elemento y se sigue leyendo: el `>` y la
    // etiqueta entera del hijo, con sus `xmlns`.
    let mut del_elemento = Vec::new();
    let mut hijo = None;
    loop {
        match r.next_token().unwrap().tok {
            Tok::Attr(a) if a.name.local() != "name" && !a.is_namespace_decl() => {
                del_elemento.push(a)
            }
            Tok::STag(n) if n.local() == "complexType" => hijo = Some(n),
            Tok::EmptyClose => break,
            _ => {}
        }
    }
    // El hijo sí está en su propio ámbito.
    assert_eq!(hijo.unwrap().expanded(), (Some("no-xs"), "complexType"));
    let resueltos: Vec<String> = del_elemento
        .iter()
        .map(|a| expandido(&a.as_qname().unwrap()))
        .collect();
    // `type` y `ref` del elemento: el ámbito del padre. `base` del hijo: el suyo.
    assert_eq!(
        resueltos,
        ["{u}T".to_string(), xs("string"), "{otro}T".to_string()]
    );
}

/// Tras un `/>` o un `ETAG`, un valor de un elemento posterior ya no ve las
/// declaraciones del elemento cerrado; el del elemento cerrado sí, aunque
/// se pida al final.
#[test]
fn qname_tras_empty_close_y_etag() {
    let toks = todos(b"<a xmlns:q='w' t='q:x'><b xmlns:p='u' t='p:x'/><c t='p:x'/></a><!-- -->");
    assert_eq!(
        qnames(&toks, "t"),
        [
            Ok("{w}x".to_string()),
            Ok("{u}x".to_string()),
            Err(("namespace", 1, 54)),
        ]
    );
    let Some(Tok::Attr(c)) = toks
        .iter()
        .rev()
        .map(|t| &t.tok)
        .find(|t| matches!(t, Tok::Attr(_)))
    else {
        panic!()
    };
    assert_eq!(
        c.as_qname().unwrap_err().kind,
        ErrorKind::UndeclaredPrefix("p".into())
    );
}

#[test]
fn qname_de_valor_mal_formado() {
    for v in ["p:", ":a", "a:b:c", "a b", "", "  ", "1a", "p:1", "a\tb"] {
        let doc = format!("<a xmlns:p='u' t='{v}'/>");
        let r = qnames(&todos(doc.as_bytes()), "t");
        assert_eq!(r, [Err(("namespace", 1, 19))], "{v:?}");
        let Tok::Attr(a) = &todos(doc.as_bytes())[2].tok else {
            panic!()
        };
        assert!(
            matches!(a.as_qname().unwrap_err().kind, ErrorKind::InvalidQName(_)),
            "{v:?}"
        );
    }
    let r = qnames(&todos(b"<a xmlns:p='u' t='zz:x' u='xmlns:x'/>"), "t");
    assert_eq!(r, [Err(("namespace", 1, 19))]);
    let toks = todos(b"<a xmlns:p='u' t=' p:x ' u='xml:lang' v='xmlns:x'/>");
    let attr = |i: usize| match &toks[i].tok {
        Tok::Attr(a) => a.as_qname(),
        _ => panic!(),
    };
    let n = attr(2).unwrap();
    assert_eq!(
        (n.as_str(), n.prefix(), n.expanded()),
        ("p:x", Some("p"), (Some("u"), "x"))
    );
    assert_eq!(expandido(&attr(3).unwrap()), format!("{{{XML_NS}}}lang"));
    // `xmlns` está ligado a XMLNS_NS y no puede usarse en un QName.
    let e = attr(4).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::ReservedNamespace(_)), "{e}");
    assert_eq!((e.code(), e.line(), e.column()), ("namespace", 1, 42));
}

// ------------------------------------------------------------ errores

#[test]
fn cierre_con_otro_nombre() {
    assert_eq!(error("<a></b>"), ("syntax", 1, 6));
    assert_eq!(error("<a>\n  <b></a></b>"), ("syntax", 2, 8));
    // Se compara el nombre tal como está escrito, no el expandido.
    assert_eq!(
        error("<p:a xmlns:p='u' xmlns:q='u'></q:a>"),
        ("syntax", 1, 32)
    );
}

#[test]
fn atributo_repetido() {
    assert_eq!(
        error("<a x=\"1\" x=\"2\"/>"),
        ("duplicate-attribute", 1, 10)
    );
    assert_eq!(
        error("<a\n y='1'\n x='1'\n y='2'>"),
        ("duplicate-attribute", 4, 2)
    );
    // Mismo nombre expandido con prefijos distintos.
    assert_eq!(
        error("<a xmlns:p='u' xmlns:q='u' p:x='1' q:x='2'/>"),
        ("duplicate-attribute", 1, 36)
    );
    // Dos declaraciones del mismo prefijo.
    assert_eq!(
        error("<a xmlns:p='u' xmlns:p='v'/>"),
        ("duplicate-attribute", 1, 16)
    );
}

#[test]
fn declaracion_xml_fuera_del_inicio() {
    assert_eq!(error(" <?xml version=\"1.0\"?><a/>"), ("syntax", 1, 2));
    assert_eq!(
        error("<!-- c --><?xml version=\"1.0\"?><a/>"),
        ("syntax", 1, 11)
    );
    assert_eq!(error("<a/>\n<?xml version=\"1.0\"?>"), ("syntax", 2, 1));
    assert_eq!(error("<a><?xml version=\"1.0\"?></a>"), ("syntax", 1, 4));
    // Nombres reservados de instrucción: [Xx][Mm][Ll].
    assert_eq!(error("<a><?XML x?></a>"), ("syntax", 1, 4));
}

#[test]
fn declaracion_xml_con_encoding_distinto_de_utf8() {
    assert_eq!(
        error("<?xml version=\"1.0\" encoding=\"latin1\"?><a/>"),
        ("syntax", 1, 31)
    );
    assert_eq!(
        error("<?xml version='1.0' encoding='ISO-8859-1'?><a/>"),
        ("syntax", 1, 31)
    );
    // Con BOM la declaración sigue en la columna 1.
    assert_eq!(
        error("\u{feff}<?xml version='1.0' encoding='UTF-16'?><a/>"),
        ("syntax", 1, 31)
    );
}

#[test]
fn declaracion_xml_mal_formada() {
    for d in [
        "<?xml?>",
        "<?xml encoding='UTF-8'?>",
        "<?xml version='2.0'?>",
        "<?xml version='1.0' standalone='si'?>",
        "<?xml version='1.0'encoding='UTF-8'?>",
        "<?xml version='1.0' standalone='yes' encoding='UTF-8'?>",
        "<?xml version='1.0' otro='x'?>",
        "<?xml version=\"1.0'?>",
    ] {
        let (code, l, _) = error(&format!("{d}<a/>"));
        assert_eq!((code, l), ("syntax", 1), "{d}");
    }
}

#[test]
fn texto_fuera_de_la_raiz() {
    assert_eq!(error("x<a/>"), ("syntax", 1, 1));
    assert_eq!(error("<a/>\n  y"), ("syntax", 2, 3));
    assert_eq!(error("  \n\t<a/> z"), ("syntax", 2, 7));
    // Una referencia de carácter no es texto de espacios aunque resuelva a uno.
    assert_eq!(error("<a/>&#32;"), ("syntax", 1, 5));
    // Espacios sí.
    assert_eq!(tokens_ok(" \r\n\t<a/>\n "), ["<a", "/>", "$"]);
}

#[test]
fn prefijo_sin_declarar() {
    assert_eq!(error("<p:a/>"), ("namespace", 1, 2));
    assert_eq!(error("<a>\n <b p:x='1'/></a>"), ("namespace", 2, 5));
    // Fuera de su ámbito.
    assert_eq!(error("<a><b xmlns:p='u'/><p:c/></a>"), ("namespace", 1, 21));
    // Nombres que no son QName.
    assert_eq!(error("<a:b:c/>"), ("namespace", 1, 2));
    assert_eq!(error("<:a/>"), ("namespace", 1, 2));
    assert_eq!(error("<a b:='1'/>"), ("namespace", 1, 4));
}

#[test]
fn reglas_de_los_prefijos_reservados() {
    for d in [
        "<a xmlns:p=''/>",
        "<a xmlns:xmlns='u'/>",
        "<a xmlns:xml='u'/>",
        &format!("<a xmlns:p='{XML_NS}'/>"),
        &format!("<a xmlns='{XML_NS}'/>"),
        &format!("<a xmlns:p='{XMLNS_NS}'/>"),
        &format!("<a xmlns='{XMLNS_NS}'/>"),
        "<xmlns:a/>",
    ] {
        assert_eq!(error(d).0, "namespace", "{d}");
    }
    // Declarar `xml` con su propio URI está permitido.
    assert_eq!(tokens_ok(&format!("<a xmlns:xml='{XML_NS}'/>")).len(), 4);
}

/// `<!DOCTYPE` es el error `DoctypeForbidden` del lexer, en el offset que
/// declara el caso billion-laughs (`expected.offset`).
#[test]
fn doctype_billion_laughs() {
    let caso = "malformed/malformed-xml-billion-laughs-01.json";
    let payload = campo(caso, "payload");
    let offset = campo(caso, "expected")["offset"].as_u64().unwrap();
    for bloque in [1, 7, 4096] {
        let (toks, err) = leer_en_bloques(payload.as_str().unwrap().as_bytes(), bloque);
        let e = err.unwrap();
        assert_eq!(
            e.kind,
            xsd::ErrorKind::Lex(lexer::LexErrorKind::DoctypeForbidden)
        );
        assert_eq!(
            (e.code(), e.at.offset, e.line(), e.column()),
            ("lexical", offset, 2, 3)
        );
        // La declaración y el salto de línea anteriores sí salen.
        assert!(matches!(toks[0].tok, Tok::Pi { .. }));
    }
    assert_eq!(error("<!doctype a><a/>"), ("lexical", 1, 3));
}

#[test]
fn fin_de_la_entrada() {
    assert_eq!(error(""), ("syntax", 1, 1));
    assert_eq!(error("  <!-- solo un comentario -->\n"), ("syntax", 2, 1));
    assert_eq!(error("<a><b>"), ("syntax", 1, 7));
    assert_eq!(error("<a/><b/>"), ("syntax", 1, 5));
    assert_eq!(error("<a/></a>"), ("syntax", 1, 7));
    assert_eq!(error("<a"), ("lexical", 1, 3));
}

/// Tras un error, el lector repite el mismo error.
#[test]
fn el_error_se_repite() {
    let mut r = Reader::new(b"<a></b>");
    let mut primero = None;
    for _ in 0..10 {
        if let Err(e) = r.next_token() {
            if let Some(p) = &primero {
                assert_eq!(p, &e);
            }
            primero = Some(e);
        }
    }
    assert!(primero.is_some());
}

// ---------------------------------------------------- entradas hostiles

/// 200 000 niveles de anidamiento: sin recursión (pila explícita), ni al
/// leer ni al liberar.
#[test]
fn anidamiento_profundo_sin_recursion() {
    const N: usize = 200_000;
    let doc = format!("{}{}", "<p:a xmlns:p='u'>".repeat(N), "</p:a>".repeat(N));
    let (toks, err) = leer_en_bloques(doc.as_bytes(), 4096);
    assert_eq!(err, None);
    assert_eq!(toks.len(), 4 * N + 1);
}

/// Muchos atributos y declaraciones en una etiqueta: la comprobación de
/// repetidos no es cuadrática (solo se comprueba que termina y da el
/// resultado correcto; el costo se argumenta en `lector-xsd.md`).
#[test]
fn muchos_atributos_y_declaraciones() {
    const N: usize = 50_000;
    let mut doc = String::from("<a");
    for i in 0..N {
        doc.push_str(&format!(" xmlns:p{i}='u{i}' p{i}:x='{i}'"));
    }
    doc.push_str("/>");
    let (toks, err) = leer_en_bloques(doc.as_bytes(), 4096);
    assert_eq!(err, None);
    assert_eq!(toks.len(), 2 * N + 3);
    doc.truncate(doc.len() - 2);
    doc.push_str(" p7:x='otra vez'/>");
    let (_, err) = leer_en_bloques(doc.as_bytes(), 4096);
    assert_eq!(err.unwrap().code(), "duplicate-attribute");
}

/// Fragmentos que forman documentos casi bien formados (etiquetas,
/// declaraciones, prefijos, texto, PI) para que las pruebas de propiedades
/// lleguen a las comprobaciones del lector y no solo al lexer.
fn fragmento() -> impl Strategy<Value = Vec<u8>> {
    prop::sample::select(vec![
        "<a",
        "<p:a",
        "<xs:schema",
        " xmlns:p='u'",
        " xmlns:xs='http://www.w3.org/2001/XMLSchema'",
        " xmlns='d'",
        " xmlns=''",
        " b='1'",
        " p:b=\"2\"",
        ">",
        "/>",
        "</a>",
        "</p:a>",
        "</xs:schema>",
        "texto",
        " ",
        "\n",
        "&amp;",
        "&#32;",
        "<?xml version='1.0'?>",
        "<?xml version='1.0' encoding='latin1'?>",
        "<?pi x?>",
        "<!-- c -->",
        "<![CDATA[x]]>",
        "<!DOCTYPE",
        "\u{feff}",
        "é",
    ])
    .prop_map(|s| s.as_bytes().to_vec())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Bloques de 1, 7 y 4096 bytes dan exactamente los mismos tokens y el
    /// mismo error (con su posición).
    #[test]
    fn mismo_resultado_con_bloques_de_1_7_y_4096(
        bytes in prop::collection::vec(fragmento(), 0..40).prop_map(|v| v.concat()),
    ) {
        let entero = leer_en_bloques(&bytes, usize::MAX);
        for bloque in [1, 7, 4096] {
            prop_assert_eq!(&leer_en_bloques(&bytes, bloque), &entero);
        }
    }

    /// Cortes arbitrarios (también bloques vacíos) tampoco cambian el resultado.
    #[test]
    fn cortes_arbitrarios(
        bytes in prop::collection::vec(fragmento(), 0..40).prop_map(|v| v.concat()),
        cortes in prop::collection::vec(any::<prop::sample::Index>(), 0..8),
    ) {
        let entero = leer_en_bloques(&bytes, usize::MAX);
        let mut puntos: Vec<usize> = cortes.iter().map(|i| i.index(bytes.len() + 1)).collect();
        puntos.sort_unstable();
        let mut partes = Vec::new();
        let mut prev = 0;
        for p in puntos.into_iter().chain([bytes.len()]) {
            partes.push(&bytes[prev..p]);
            prev = p;
        }
        prop_assert_eq!(leer_trozos(partes.into_iter()), entero);
    }

    /// Bytes arbitrarios: sin pánico, y el resultado tampoco depende del
    /// tamaño de bloque. Todo error tiene mensaje de una línea.
    #[test]
    fn bytes_arbitrarios_sin_panico(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let entero = leer_en_bloques(&bytes, usize::MAX);
        for bloque in [1, 7] {
            prop_assert_eq!(&leer_en_bloques(&bytes, bloque), &entero);
        }
        if let Some(e) = entero.1 {
            prop_assert!(!e.to_string().contains('\n'));
        }
    }

    /// Valores arbitrarios para `Attr::as_qname`: sin pánico, y un nombre
    /// resuelto es siempre el valor sin los espacios de los extremos.
    #[test]
    fn as_qname_sin_panico(v in ".{0,12}") {
        let esc = v.replace('&', "&amp;").replace('<', "&lt;").replace('\'', "&apos;");
        let doc = format!("<a xmlns:p='u' xmlns='d' t='{esc}'/>");
        let (toks, _) = leer_en_bloques(doc.as_bytes(), usize::MAX);
        for t in &toks {
            if let Tok::Attr(a) = &t.tok {
                if let Ok(n) = a.as_qname() {
                    prop_assert_eq!(n.as_str(), a.value().trim_matches([' ', '\t', '\n', '\r']));
                    prop_assert!(n.local().len() <= n.as_str().len());
                }
            }
        }
    }
}

// ------------------------------------------- ronda 1 de correcciones

/// Un URI largo (1 MiB) y muchos atributos con su prefijo: la comprobación
/// de nombres expandidos repetidos no vuelve a leer el URI por atributo (se
/// interna una vez, en su declaración). Antes era O(M · |URI|): con estos
/// tamaños, minutos; ahora es lineal. Sin reloj (D1): basta que termine y
/// que el resultado sea el correcto, también el repetido del final.
#[test]
fn uri_largo_y_muchos_atributos_con_prefijo() {
    const URI: usize = 1 << 20;
    const M: usize = 40_000;
    let mut doc = format!(
        "<a xmlns:p='{}' xmlns:q='{}'",
        "u".repeat(URI),
        "u".repeat(URI)
    );
    for i in 0..M {
        doc.push_str(&format!(" p:a{i}='1'"));
    }
    let ok = format!("{doc}/>");
    let (toks, err) = leer_en_bloques(ok.as_bytes(), 4096);
    assert_eq!(err, None);
    assert_eq!(toks.len(), M + 5);
    let Tok::Attr(a) = &toks[3 + M - 1].tok else {
        panic!()
    };
    assert_eq!(
        (a.name.local(), a.name.ns().map(str::len)),
        ("a39999", Some(URI))
    );
    // `q:a7` es `p:a7` con otro prefijo y el mismo URI: repetido.
    let mal = format!("{doc} q:a7='2'/>");
    let (_, err) = leer_en_bloques(mal.as_bytes(), 4096);
    let e = err.unwrap();
    assert!(
        matches!(&e.kind, ErrorKind::DuplicateAttribute(n) if n == "q:a7"),
        "{e}"
    );
}

/// Un documento de más de 8 KiB con nombres, atributos y textos que cruzan
/// las fronteras de 4096 y 8192: bloques de 4096, `Reader::new` (que no
/// parte la entrada) y la entrada entera dan exactamente los mismos tokens.
#[test]
fn documento_multibloque_con_fronteras_en_medio_de_un_nombre() {
    let mut doc = String::from("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\n");
    let mut i = 0;
    while doc.len() < 20_000 {
        doc.push_str(&format!(
            "<xs:element name='elemento_{i}_{}' type='xs:string'>Ñandú {i}</xs:element>\n",
            "n".repeat(i % 97)
        ));
        i += 1;
    }
    doc.push_str("</xs:schema>");
    let bytes = doc.as_bytes();
    // Alguna marca cruza cada frontera de bloque (no cae justo entre dos).
    for f in [4096, 8192, 12288, 16384] {
        assert!(
            bytes[f - 1] != b'>' || bytes[f] != b'<',
            "frontera {f} entre marcas"
        );
    }
    let entero = leer_en_bloques(bytes, usize::MAX);
    assert_eq!(entero.1, None);
    assert_eq!(leer_en_bloques(bytes, 4096), entero);
    let mut r = Reader::new(bytes);
    let mut toks = Vec::new();
    loop {
        let t = r.next_token().unwrap();
        let fin = t.tok == Tok::Eof;
        toks.push(t);
        if fin {
            break;
        }
    }
    assert_eq!(toks, entero.0);
}

fn kind(input: &[u8]) -> (ErrorKind, u32, u32) {
    let (_, err) = leer_en_bloques(input, usize::MAX);
    let e =
        err.unwrap_or_else(|| panic!("se esperaba un error: {}", String::from_utf8_lossy(input)));
    (e.kind.clone(), e.line(), e.column())
}

/// Cada variante de error, no solo su código.
#[test]
fn variante_de_cada_error() {
    use xsd::ErrorKind::*;
    let s = |x: &str| x.to_string();
    assert_eq!(
        kind(b"<a></b>"),
        (
            MismatchedEndTag {
                open: s("a"),
                close: s("b")
            },
            1,
            6
        )
    );
    assert_eq!(kind(b"<a/></a>"), (UnopenedEndTag(s("a")), 1, 7));
    assert_eq!(kind(b"<a><b>"), (UnclosedElement(s("b")), 1, 7));
    assert_eq!(kind(b""), (NoRootElement, 1, 1));
    assert_eq!(kind(b"<a/><b/>"), (MultipleRootElements, 1, 5));
    assert_eq!(kind(b"x<a/>"), (TextOutsideRoot, 1, 1));
    assert_eq!(
        kind(b"<a x='1' x='2'/>"),
        (DuplicateAttribute(s("x")), 1, 10)
    );
    // `<?XML` es un nombre reservado; `<?xml` tardío, una declaración fuera de sitio.
    assert_eq!(
        kind(b"<a><?XML x?></a>"),
        (ReservedPiTarget(s("XML")), 1, 4)
    );
    assert_eq!(
        kind(b"<a><?xml version='1.0'?></a>"),
        (XmlDeclNotFirst, 1, 4)
    );
    assert_eq!(
        kind(b"<?xml version='1.0' encoding='latin1'?><a/>"),
        (UnsupportedEncoding(s("latin1")), 1, 31)
    );
    assert_eq!(kind(b"<a:b:c/>"), (InvalidQName(s("a:b:c")), 1, 2));
    assert_eq!(kind(b"<p:a/>"), (UndeclaredPrefix(s("p")), 1, 2));
    assert_eq!(kind(b"<a b:='1'/>"), (InvalidQName(s("b:")), 1, 4));
    assert_eq!(
        kind(b"<a>\n <b p:x='1'/></a>"),
        (UndeclaredPrefix(s("p")), 2, 5)
    );
    assert!(matches!(
        kind(b"<a xmlns:p=''/>"),
        (ReservedNamespace(_), 1, 4)
    ));
    assert!(matches!(kind(b"<xmlns:a/>"), (ReservedNamespace(_), 1, 2)));
}

/// Cada forma mal escrita de la declaración da `BadXmlDecl` con su motivo y
/// la posición del carácter que falla (o la del `<` si falta algo al final).
#[test]
fn variantes_de_bad_xml_decl() {
    use xsd::ErrorKind::BadXmlDecl;
    for (d, motivo, col) in [
        ("<?xml?>", "falta version", 1),
        ("<?xml encoding='UTF-8'?>", "falta version", 7),
        ("<?xml version='2.0'?>", "version debe ser 1.x", 16),
        ("<?xml version='1.'?>", "version debe ser 1.x", 16),
        (
            "<?xml version='1.0' standalone='si'?>",
            "standalone debe ser yes o no",
            33,
        ),
        (
            "<?xml version='1.0'encoding='UTF-8'?>",
            "falta un espacio entre los pseudoatributos",
            20,
        ),
        (
            "<?xml version='1.0' standalone='yes' encoding='UTF-8'?>",
            "pseudoatributo repetido o fuera de orden",
            38,
        ),
        (
            "<?xml version='1.0' version='1.0'?>",
            "pseudoatributo repetido o fuera de orden",
            21,
        ),
        (
            "<?xml version='1.0' otro='x'?>",
            "se admiten version, encoding y standalone",
            21,
        ),
        ("<?xml version '1.0'?>", "se esperaba =", 15),
        ("<?xml version=1.0?>", "se esperaba una comilla", 15),
        ("<?xml version=\"1.0'?>", "comilla sin cerrar", 15),
    ] {
        assert_eq!(
            kind(format!("{d}<a/>").as_bytes()),
            (BadXmlDecl(motivo), 1, col),
            "{d}"
        );
    }
}

/// UTF-8 inválido: error léxico del lexer, con su posición.
#[test]
fn utf8_invalido() {
    use lexer::LexErrorKind::InvalidUtf8;
    for (input, col) in [
        (&b"<a\xFF/>"[..], 3),
        (&b"<a b='\xC3'/>"[..], 7),
        (&b"\xFF<a/>"[..], 1),
        (&b"<a>\xC3</a>"[..], 4),
    ] {
        for bloque in [1, 7, 4096] {
            let (_, err) = leer_en_bloques(input, bloque);
            let e = err.unwrap();
            assert_eq!(
                (&e.kind, e.code(), e.line(), e.column()),
                (&ErrorKind::Lex(InvalidUtf8), "lexical", 1, col)
            );
        }
    }
}

/// Solo BOM o solo espacios: no hay elemento raíz, en el fin de la entrada.
#[test]
fn solo_bom_o_solo_espacios() {
    assert_eq!(
        kind("\u{feff}".as_bytes()),
        (ErrorKind::NoRootElement, 1, 1)
    );
    assert_eq!(kind(b"  \n\t"), (ErrorKind::NoRootElement, 2, 2));
    assert_eq!(
        kind("\u{feff} \n".as_bytes()),
        (ErrorKind::NoRootElement, 2, 1)
    );
}

/// Tras `Eof`, siempre `Eof` en la misma posición.
#[test]
fn eof_repetido() {
    let mut r = Reader::new(b"<a/>\n");
    let mut eofs = Vec::new();
    for _ in 0..10 {
        let t = r.next_token().unwrap();
        if t.tok == Tok::Eof {
            eofs.push(t);
        }
    }
    assert_eq!(eofs.len(), 7);
    assert!(eofs.iter().all(|t| *t == eofs[0]));
    assert_eq!(
        (
            eofs[0].span.start.offset,
            eofs[0].span.end,
            eofs[0].span.start.line
        ),
        (5, 5, 2)
    );
}

// ------------------------------------------- ronda 2 de correcciones

/// XML 1.0 §2.11 también en el contenido de las instrucciones de
/// procesamiento (salvo la declaración `<?xml …?>`, que se analiza con sus
/// bytes y no se entrega normalizada).
#[test]
fn fines_de_linea_en_las_instrucciones() {
    for bloque in [1, 7, 4096] {
        let (toks, err) = leer_en_bloques(b"<?pi a\r\nb\rc\n?><a><?q x\r\n?></a>", bloque);
        assert_eq!(err, None);
        let datos: Vec<&str> = toks
            .iter()
            .filter_map(|t| match &t.tok {
                Tok::Pi { data, .. } => Some(data.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(datos, ["a\nb\nc\n", "x\n"]);
    }
    let (toks, err) = leer_en_bloques(b"<?xml version='1.0'\r\n?><a/>", 1);
    assert_eq!(err, None);
    assert_eq!(
        toks[0].tok,
        Tok::Pi {
            target: "xml".into(),
            data: "version='1.0'\r\n".into()
        }
    );
}

/// `<?xml` fuera de sitio y los nombres reservados se detectan con el primer
/// carácter del contenido, sin esperar al `?>`: una declaración tardía sin
/// cerrar da `XmlDeclNotFirst`, no el error léxico del fin de la entrada.
#[test]
fn declaracion_tardia_detectada_sin_esperar_al_cierre() {
    let mut doc = String::from("<a><?xml version='1.0' ");
    doc.push_str(&"x".repeat(100_000));
    assert_eq!(kind(doc.as_bytes()), (ErrorKind::XmlDeclNotFirst, 1, 4));
    assert_eq!(
        kind(b"<a><?XmL x"),
        (ErrorKind::ReservedPiTarget("XmL".into()), 1, 4)
    );
    // Sin contenido, al cerrar.
    assert_eq!(kind(b"<a><?xml?></a>"), (ErrorKind::XmlDeclNotFirst, 1, 4));
    assert_eq!(
        kind(b"<a><?xMl?></a>"),
        (ErrorKind::ReservedPiTarget("xMl".into()), 1, 4)
    );
}

/// La posición de un error de la declaración se reconstruye por bytes,
/// también tras caracteres no ASCII y saltos de línea.
#[test]
fn posicion_en_la_declaracion_con_no_ascii_y_saltos() {
    assert_eq!(
        kind("<?xml version='1.0'\n  encoding='ñandú'?><a/>".as_bytes()),
        (ErrorKind::UnsupportedEncoding("ñandú".into()), 2, 13)
    );
    assert_eq!(
        kind("<?xml version='1.0' standalone='ñ' ?><a/>".as_bytes()),
        (ErrorKind::BadXmlDecl("standalone debe ser yes o no"), 1, 33)
    );
    // `\r\n` en la declaración (que no se normaliza): línea, columna y
    // offset exactos.
    let (_, err) = leer_en_bloques(
        b"<?xml version=\"1.0\"\r\n\r\n   encoding=\"latin1\"?><a/>",
        7,
    );
    let e = err.unwrap();
    assert_eq!(
        (&e.kind, e.line(), e.column(), e.at.offset),
        (&ErrorKind::UnsupportedEncoding("latin1".into()), 3, 14, 36)
    );
}

/// Un error léxico dentro de una etiqueta gana aunque esté más adelante que
/// un error de la etiqueta: esta nunca llega a comprobarse (`lector-xsd.md` §7).
#[test]
fn error_lexico_dentro_de_la_etiqueta_gana() {
    assert_eq!(
        kind(b"<a b='1' b='2' c=<"),
        (ErrorKind::Lex(lexer::LexErrorKind::UnexpectedByte), 1, 18)
    );
}

// ------------------------------------------- ronda 3 de correcciones

/// `Attr::value` es de solo lectura: el parser normaliza (*collapse*) una
/// copia, y `as_qname` sigue usando los rangos del valor original.
#[test]
fn collapse_sobre_una_copia_no_rompe_as_qname() {
    let toks = todos(b"<a xmlns:p='u' t='   p:abc   '/>");
    let Tok::Attr(a) = &toks[2].tok else { panic!() };
    let colapsado = a.value().trim().to_string();
    assert_eq!(colapsado, "p:abc");
    assert_eq!(expandido(&a.as_qname().unwrap()), "{u}abc");
    assert_eq!(a.clone().into_value(), "   p:abc   ");
}

/// Identidad del espacio de nombres: la misma para el mismo URI con
/// prefijos distintos (y para el `targetNamespace` internado con
/// `Reader::intern`), distinta para URI distintos.
#[test]
fn identidad_del_espacio_de_nombres() {
    let doc =
        b"<p:a xmlns:p='u' xmlns:q='u' xmlns:r='v' q:x='1' r:y='p:t'><p:b xmlns:p='v'/></p:a>";
    let mut r = Reader::new(doc);
    let mut nombres = Vec::new();
    let mut valor = None;
    loop {
        match r.next_token().unwrap().tok {
            Tok::STag(n) => nombres.push(n),
            Tok::Attr(a) if !a.is_namespace_decl() => {
                if a.name.local() == "y" {
                    valor = Some(a.as_qname().unwrap());
                }
                nombres.push(a.name);
            }
            Tok::Eof => break,
            _ => {}
        }
    }
    // a (p→u), q:x (u), r:y (v), b (p→v).
    let ids: Vec<_> = nombres.iter().map(|n| n.ns_id().unwrap().clone()).collect();
    assert_eq!(ids[0], ids[1]);
    assert_ne!(ids[0], ids[2]);
    assert_eq!(ids[2], ids[3]);
    // El valor `p:t` de la etiqueta de `a` es de `u`.
    assert_eq!(valor.unwrap().ns_id(), Some(&ids[0]));
    // `Reader::intern` da la misma identidad que los nombres del documento.
    assert_eq!(r.intern("u"), ids[0]);
    assert_eq!(r.intern("v"), ids[2]);
    assert_ne!(r.intern("w"), ids[0]);
    // Como clave de una tabla: O(1) por consulta, sin leer el URI.
    let mut tabla = std::collections::HashMap::new();
    tabla.insert((ids[0].clone(), "x"), 1);
    assert_eq!(tabla.get(&(r.intern("u"), "x")), Some(&1));
    assert_eq!(ids[0].as_str(), "u");
    // Dos lectores distintos: identidades distintas, nombres iguales por contenido.
    let mut otro = Reader::new(doc);
    let Tok::STag(n) = otro.next_token().unwrap().tok else {
        panic!()
    };
    assert_eq!(n, nombres[0]);
    assert_ne!(n.ns_id(), nombres[0].ns_id());
}

/// Un valor de más de 1 MiB con `:` se resuelve bien (sin reloj, D1), y un
/// error sobre un valor largo lleva el texto recortado: pedirlo muchas
/// veces no copia el valor.
#[test]
fn valor_largo_con_prefijo() {
    const L: usize = (1 << 20) + 17;
    let local = "x".repeat(L);
    let doc = format!("<a xmlns:p='u' t='p:{local}' m='{local}:b:c'/>");
    let toks = todos(doc.as_bytes());
    let Tok::Attr(t) = &toks[2].tok else { panic!() };
    let n = t.as_qname().unwrap();
    assert_eq!(
        (n.ns(), n.prefix(), n.local().len()),
        (Some("u"), Some("p"), L)
    );
    let Tok::Attr(m) = &toks[3].tok else { panic!() };
    for _ in 0..1000 {
        let e = m.as_qname().unwrap_err();
        let ErrorKind::InvalidQName(texto) = &e.kind else {
            panic!()
        };
        assert_eq!(texto.chars().count(), lexer::SHORT_MAX + 1);
    }
    // El mensaje no cambia: los 32 primeros caracteres y `…`.
    let msg = m.as_qname().unwrap_err().to_string();
    assert!(
        msg.ends_with(&format!("\"{}…\" no es un QName", "x".repeat(32))),
        "{msg}"
    );
}

/// El error repetido tras fallar lleva el nombre recortado aunque el nombre
/// del documento mida 1 MiB.
#[test]
fn error_repetido_con_nombre_largo() {
    let nombre = "n".repeat(1 << 20);
    let doc = format!("<{nombre}></b>");
    let mut r = Reader::new(doc.as_bytes());
    let mut errores = 0;
    for _ in 0..100 {
        if let Err(e) = r.next_token() {
            let ErrorKind::MismatchedEndTag { open, close } = &e.kind else {
                panic!()
            };
            assert_eq!((open.len(), close.as_str()), (lexer::SHORT_MAX + 1, "b"));
            errores += 1;
        }
    }
    // `STAG` y `>` salen antes del error; las 98 llamadas siguientes lo repiten.
    assert_eq!(errores, 98);
}
