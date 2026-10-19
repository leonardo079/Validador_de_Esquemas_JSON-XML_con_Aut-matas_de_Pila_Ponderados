//! Pruebas del tokenizador XML (T1.11).

use std::convert::Infallible;
use std::fmt::Write;

use lexer::xml::{Event, XmlLexer};
use lexer::{Error, LexError, LexErrorKind, Sink, Span};
use proptest::prelude::*;

mod common;

#[derive(Default)]
struct Collect(Vec<(Event, Span)>);

impl Sink<Event> for Collect {
    type Error = Infallible;
    fn event(&mut self, ev: Event, span: Span) -> Result<(), Infallible> {
        self.0.push((ev, span));
        Ok(())
    }
}

type Outcome = (Vec<(Event, Span)>, Option<LexError>);

fn lex_parts<'a>(parts: impl Iterator<Item = &'a [u8]>) -> Outcome {
    let mut lx = XmlLexer::new();
    let mut out = Collect::default();
    let mut res = Ok(());
    for p in parts {
        res = lx.feed(p, &mut out);
        if res.is_err() {
            break;
        }
    }
    if res.is_ok() {
        res = lx.finish(&mut out);
    }
    let err = res.err().map(|e| match e {
        Error::Lex(e) => e,
        Error::Sink(never) => match never {},
    });
    (out.0, err)
}

fn lex_chunked(input: &[u8], chunk: usize) -> Outcome {
    lex_parts(input.chunks(chunk.max(1)))
}

fn lex(input: &str) -> Outcome {
    lex_chunked(input.as_bytes(), usize::MAX)
}

/// Reconstruye los tokens: `<nombre`, `@atributo=valor`, `>`, `/>`,
/// `</nombre>` y `"texto` (el texto contiguo se une en un solo token).
fn render(events: &[(Event, Span)]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |out: &mut Vec<String>, cur: &mut String| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    for (ev, _) in events {
        match *ev {
            Event::StartTagOpen => {
                flush(&mut out, &mut cur);
                cur.push('<');
            }
            Event::EndTagOpen => {
                flush(&mut out, &mut cur);
                cur.push_str("</");
            }
            Event::NameChar(c) | Event::AttrValueChar(c) => cur.push(c),
            Event::AttrBegin => {
                flush(&mut out, &mut cur);
                cur.push('@');
            }
            Event::AttrValueBegin => cur.push('='),
            Event::AttrValueEnd => flush(&mut out, &mut cur),
            Event::StartTagClose => {
                flush(&mut out, &mut cur);
                out.push(">".into());
            }
            Event::EmptyTagClose => {
                flush(&mut out, &mut cur);
                out.push("/>".into());
            }
            Event::EndTagClose => {
                cur.push('>');
                flush(&mut out, &mut cur);
            }
            Event::PiOpen => {
                flush(&mut out, &mut cur);
                cur.push_str("<?");
            }
            Event::PiChar(c) => {
                // El nombre no tiene espacios: el primero separa el contenido.
                if !cur.contains(' ') {
                    cur.push(' ');
                }
                cur.push(c);
            }
            Event::PiClose => {
                cur.push_str("?>");
                flush(&mut out, &mut cur);
            }
            Event::TextChar(c) => {
                if !cur.starts_with('"') {
                    flush(&mut out, &mut cur);
                    cur.push('"');
                }
                cur.push(c);
            }
        }
    }
    flush(&mut out, &mut cur);
    out
}

fn tokens(input: &str) -> Vec<String> {
    let (events, err) = lex(input);
    assert_eq!(err, None, "error inesperado en {input:?}");
    render(&events)
}

fn error(input: &[u8]) -> LexError {
    lex_chunked(input, usize::MAX)
        .1
        .unwrap_or_else(|| panic!("se esperaba error en {:?}", String::from_utf8_lossy(input)))
}

fn assert_error(input: &[u8], kind: LexErrorKind, offset: u64) {
    let e = error(input);
    assert_eq!(
        (e.kind, e.at.offset),
        (kind, offset),
        "{:?}",
        String::from_utf8_lossy(input)
    );
}

#[test]
fn etiquetas_y_atributos() {
    assert_eq!(
        tokens(r#"<a x="1" y='2'>hola</a>"#),
        ["<a", "@x=1", "@y=2", ">", "\"hola", "</a>"]
    );
    assert_eq!(tokens("<br/>"), ["<br", "/>"]);
    assert_eq!(tokens("<br />"), ["<br", "/>"]);
    assert_eq!(
        tokens("<a\n  k = \"v\"\n></a >"),
        ["<a", "@k=v", ">", "</a>"]
    );
    assert_eq!(tokens(r#"<a x="">"#), ["<a", "@x=", ">"]);
    assert_eq!(tokens("<ns:a-b.c_1/>"), ["<ns:a-b.c_1", "/>"]);
    assert_eq!(
        tokens(r#"<ñandú año="2026"/>"#),
        ["<ñandú", "@año=2026", "/>"]
    );
    // Comillas del otro tipo dentro del valor.
    assert_eq!(
        tokens(r#"<a x='di "hola"'/>"#),
        ["<a", "@x=di \"hola\"", "/>"]
    );
}

#[test]
fn texto_y_espacios() {
    assert_eq!(tokens(""), Vec::<String>::new());
    assert_eq!(tokens("<a> x > y </a>"), ["<a", ">", "\" x > y ", "</a>"]);
    assert_eq!(tokens("<a>\n\t<b/>\n</a>").len(), 7);
    assert_eq!(tokens("<a>]] ]</a>"), ["<a", ">", "\"]] ]", "</a>"]);
}

#[test]
fn declaracion_comentarios_e_instrucciones() {
    assert_eq!(
        tokens("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- comentario - con guion -->\n<?pi a?b ??><r/>"),
        [
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\"\n\n",
            "<?pi a?b ??>",
            "<r",
            "/>"
        ]
    );
    // El espacio tras el nombre es separador, no contenido.
    assert_eq!(tokens("<?a?><?b ?>"), ["<?a?>", "<?b?>"]);
    // La declaración y su contenido llegan al parser, que puede rechazar
    // un `encoding` distinto de UTF-8 o una declaración tras la raíz.
    assert_eq!(
        tokens("<a/><?xml encoding=\"ISO-8859-1\"?>"),
        ["<a", "/>", "<?xml encoding=\"ISO-8859-1\"?>"]
    );
    assert_eq!(tokens("<!---->"), Vec::<String>::new());
    assert_eq!(tokens("<!-- ñ <a> & -->"), Vec::<String>::new());
}

#[test]
fn referencias() {
    assert_eq!(
        tokens("<a t=\"&lt;&gt;&amp;&quot;&apos;\">&lt;&gt;&amp;&quot;&apos;</a>"),
        ["<a", "@t=<>&\"'", ">", "\"<>&\"'", "</a>"]
    );
    assert_eq!(tokens("<a>&#65;&#x42;&#x1F600;&#10;</a>")[2], "\"AB😀\n");
    assert_eq!(tokens("<a b='&#39;'/>"), ["<a", "@b='", "/>"]);
}

#[test]
fn cdata() {
    assert_eq!(tokens("<a><![CDATA[x<y && z]]></a>")[2], "\"x<y && z");
    assert_eq!(tokens("<a><![CDATA[x]]]></a>")[2], "\"x]");
    assert_eq!(tokens("<a><![CDATA[]]]]></a>")[2], "\"]]");
    assert_eq!(tokens("<a><![CDATA[a]b]]c]>]]></a>")[2], "\"a]b]]c]>");
    assert_eq!(tokens("<a><![CDATA[]]></a>"), ["<a", ">", "</a>"]);
}

#[test]
fn doctype_y_entidades_prohibidas() {
    // Billion laughs: se rechaza en el byte `D` de `<!DOCTYPE`, antes de leer
    // una sola declaración de entidad.
    let bomba = br#"<?xml version="1.0"?>
<!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;">]>
<lolz>&lol2;</lolz>"#;
    assert_error(bomba, LexErrorKind::DoctypeForbidden, 24);
    // XXE.
    assert_error(
        br#"<!DOCTYPE r [<!ENTITY x SYSTEM "file:///etc/passwd">]><r>&x;</r>"#,
        LexErrorKind::DoctypeForbidden,
        2,
    );
    assert_error(b"<!ENTITY x 'y'>", LexErrorKind::UnexpectedByte, 2);
    assert_error(b"<a>&x;</a>", LexErrorKind::UndefinedEntity, 3);
    assert_error(b"<a>&lol2;</a>", LexErrorKind::UndefinedEntity, 3);
    assert_error(b"<a>&quote;</a>", LexErrorKind::UndefinedEntity, 3);
}

#[test]
fn referencias_invalidas() {
    assert_error(b"<a>&;</a>", LexErrorKind::InvalidReference, 4);
    assert_error(b"<a>& b</a>", LexErrorKind::InvalidReference, 4);
    assert_error(b"<a>&#;</a>", LexErrorKind::InvalidReference, 5);
    assert_error(b"<a>&#x;</a>", LexErrorKind::InvalidReference, 6);
    assert_error(b"<a>&#xG;</a>", LexErrorKind::InvalidReference, 6);
    assert_error(b"<a>&amp</a>", LexErrorKind::InvalidReference, 7);
    assert_error(b"<a>&#0;</a>", LexErrorKind::InvalidCharRef, 3);
    assert_error(b"<a>&#x1;</a>", LexErrorKind::InvalidCharRef, 3);
    assert_error(b"<a>&#xD800;</a>", LexErrorKind::InvalidCharRef, 3);
    assert_error(b"<a>&#xFFFE;</a>", LexErrorKind::InvalidCharRef, 3);
    assert_error(b"<a>&#x110000;</a>", LexErrorKind::InvalidCharRef, 3);
    assert_error(
        b"<a>&#99999999999999999999;</a>",
        LexErrorKind::InvalidCharRef,
        3,
    );
}

#[test]
fn instrucciones_invalidas() {
    assert_error(b"<??><a/>", LexErrorKind::InvalidPi, 2);
    assert_error(b"<? x?>", LexErrorKind::InvalidPi, 2);
    assert_error(b"<?a?b?>", LexErrorKind::InvalidPi, 4);
    assert_error(b"<?1?>", LexErrorKind::InvalidPi, 2);
}

#[test]
fn error_del_sink_detiene_el_lexer() {
    struct Falla;
    impl Sink<Event> for Falla {
        type Error = ();
        fn event(&mut self, _: Event, _: Span) -> Result<(), ()> {
            Err(())
        }
    }
    let mut lx = XmlLexer::new();
    assert_eq!(lx.feed(b"<a", &mut Falla), Err(Error::Sink(())));
    assert!(matches!(
        lx.feed(b"/>", &mut Falla),
        Err(Error::Lex(LexError {
            kind: LexErrorKind::Aborted,
            ..
        }))
    ));
}

#[test]
fn errores_de_marcado() {
    assert_error(b"<a>]]></a>", LexErrorKind::UnexpectedByte, 5);
    assert_error(b"<!-- a -- b -->", LexErrorKind::UnexpectedByte, 9);
    // `->` es contenido válido de comentario: falta el cierre `-->`.
    assert_error(b"<!--->", LexErrorKind::UnexpectedEof, 6);
    assert_error(br#"<a x="1"y="2">"#, LexErrorKind::UnexpectedByte, 8);
    assert_error(br#"<a x="<">"#, LexErrorKind::UnexpectedByte, 6);
    assert_error(b"<a x=1>", LexErrorKind::UnexpectedByte, 5);
    assert_error(b"<a x>", LexErrorKind::UnexpectedByte, 4);
    assert_error(b"< a>", LexErrorKind::UnexpectedByte, 1);
    assert_error(b"<1a>", LexErrorKind::UnexpectedByte, 1);
    assert_error(b"</ a>", LexErrorKind::UnexpectedByte, 2);
    assert_error(b"<a/ >", LexErrorKind::UnexpectedByte, 3);
    assert_error(b"<![CDATA(x]]>", LexErrorKind::UnexpectedByte, 8);
    assert_error(b"<![cdata[x]]>", LexErrorKind::UnexpectedByte, 3);
}

#[test]
fn caracteres_invalidos() {
    assert_error(b"<a>\x01</a>", LexErrorKind::InvalidXmlChar, 3);
    assert_error(b"<a x='\x00'/>", LexErrorKind::InvalidXmlChar, 6);
    assert_error(b"<!-- \x08 -->", LexErrorKind::InvalidXmlChar, 5);
    assert_error(
        "<a>\u{fffe}</a>".as_bytes(),
        LexErrorKind::InvalidXmlChar,
        3,
    );
    assert_error(b"<a>\xff</a>", LexErrorKind::InvalidUtf8, 3);
    assert_error(b"<a>\xc3(</a>", LexErrorKind::InvalidUtf8, 3);
    assert_error(b"<!-- \xed\xa0\x80 -->", LexErrorKind::InvalidUtf8, 5);
    assert_error(b"<\xc3\xb1\xff>", LexErrorKind::InvalidUtf8, 3);
}

#[test]
fn fin_de_entrada() {
    for input in [
        "<a",
        "<a x",
        "<a x=\"1",
        "</a",
        "<!-- x",
        "<![CDATA[x",
        "<?xml",
        "<a>&amp",
    ] {
        assert_eq!(
            error(input.as_bytes()).kind,
            LexErrorKind::UnexpectedEof,
            "{input}"
        );
    }
    assert_error(b"<a>\xe2\x82", LexErrorKind::InvalidUtf8, 3);
    // Texto al final es válido para el lexer (la sintaxis lo juzga el DPDA).
    assert_eq!(tokens("<a/>x"), ["<a", "/>", "\"x"]);
}

#[test]
fn bom_inicial() {
    assert_eq!(tokens("\u{feff}<a/>"), ["<a", "/>"]);
    assert_eq!(error(b"\xef\xbb").kind, LexErrorKind::InvalidUtf8);
    // El BOM no ocupa columna.
    let e = error(b"\xef\xbb\xbf<\x01");
    assert_eq!((e.at.offset, e.at.column), (4, 2));
    // Una referencia a U+FEFF en el byte 0 no es un BOM: se conserva.
    assert_eq!(tokens("&#xFEFF;<a/>"), ["\"\u{feff}", "<a", "/>"]);
    // Texto inicial que empieza por 0xEF y no es BOM.
    assert_eq!(tokens("\u{f000}<a/>"), ["\"\u{f000}", "<a", "/>"]);
}

#[test]
fn rangos_y_posiciones() {
    let input = r#"<a k="v&amp;">x&lt;</a>"#;
    let (events, err) = lex(input);
    assert_eq!(err, None);
    let text = |s: &Span| &input[s.start.offset as usize..s.end as usize];
    let spans: Vec<(Event, &str)> = events
        .iter()
        .filter(|(e, _)| {
            !matches!(
                e,
                Event::NameChar(_) | Event::AttrBegin | Event::AttrValueBegin
            )
        })
        .map(|(e, s)| (*e, text(s)))
        .collect();
    assert_eq!(
        spans,
        [
            (Event::StartTagOpen, "<"),
            (Event::AttrValueChar('v'), "v"),
            (Event::AttrValueChar('&'), "&amp;"),
            (Event::AttrValueEnd, "\"v&amp;\""),
            (Event::StartTagClose, r#"<a k="v&amp;">"#),
            (Event::TextChar('x'), "x"),
            (Event::TextChar('<'), "&lt;"),
            (Event::EndTagOpen, "</"),
            (Event::EndTagClose, "</a>"),
        ]
    );
    let e = error("<r>\n  <ñ a='1' b=2/>\n</r>".as_bytes());
    assert_eq!((e.at.line, e.at.column), (2, 14));
}

#[test]
fn corpus_en_bloques() {
    let cases = common::corpus();
    assert!(cases
        .iter()
        .any(|c| c.format == "xml" && c.payload.is_some()));
    for case in cases.iter().filter(|c| c.format == "xml") {
        if let Some(p) = &case.payload {
            por_bloques(p, case.lexical_offset, &case.path.display().to_string());
        }
    }
}

/// Tokeniza `bytes` entero y en bloques de 1, 7 y 4096 bytes: los cuatro
/// resultados deben ser idénticos. Si el caso declara un error léxico, debe
/// ocurrir en el offset declarado; si no, no debe haber error léxico.
fn por_bloques(bytes: &[u8], lexical_offset: Option<u64>, what: &str) {
    let whole = lex_chunked(bytes, usize::MAX);
    assert_eq!(
        whole.1.map(|e| e.at.offset),
        lexical_offset,
        "{what}: {:?}",
        whole.1
    );
    for size in [1, 7, 4096] {
        assert_eq!(lex_chunked(bytes, size), whole, "{what}, bloques de {size}");
    }
}

#[test]
fn bloques_de_1_7_y_4096_bytes() {
    let input = "<?xml version=\"1.0\"?>\n<!-- pedidos -->\n<pedidos fecha='2026-10-19'>\n\
        <pedido id=\"7\"><cliente>Ñandú &amp; Cía 😀</cliente><nota><![CDATA[a]]b<c>]]></nota>\
        <total moneda=\"COP\">1&#x30;5.5</total><vacío/></pedido>\n</pedidos>";
    let whole = lex(input);
    assert_eq!(whole.1, None);
    for size in [1, 7, 4096] {
        assert_eq!(
            lex_chunked(input.as_bytes(), size),
            whole,
            "bloques de {size}"
        );
    }
}

// ------------------------------------------------------------ propiedades ---

fn fragment() -> impl Strategy<Value = &'static [u8]> {
    prop::sample::select(vec![
        &b"<"[..],
        b">",
        b"/",
        b"</",
        b"/>",
        b"a",
        b"b1",
        b"x",
        b"=",
        b"\"",
        b"'",
        b" ",
        b"\n",
        b"&",
        b"&amp;",
        b"&lt",
        b";",
        b"&#",
        b"&#x",
        b"4",
        b"1F",
        b"<!--",
        b"-->",
        b"-",
        b"<![CDATA[",
        b"]",
        b"]]>",
        b"<?",
        b"?>",
        b"<!DOCTYPE",
        b"!",
        b"?",
        "ñ".as_bytes(),
        "😀".as_bytes(),
        b"\xf0\x9f",
        b"\x80",
        b"\x01",
        b"\xef\xbb\xbf",
    ])
}

/// Elemento XML generado al azar.
#[derive(Clone, Debug)]
enum Node {
    Elem {
        name: String,
        attrs: Vec<(String, String)>,
        children: Vec<Node>,
    },
    Text(String),
}

fn name() -> impl Strategy<Value = String> {
    "[a-zA-Z_:ñé][a-zA-Z0-9_.:ñé-]{0,6}"
}

fn xml_text() -> impl Strategy<Value = String> {
    any::<String>().prop_map(|s| {
        s.chars()
            .filter(|&c| matches!(u32::from(c), 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..))
            .collect()
    })
}

fn node() -> impl Strategy<Value = Node> {
    let leaf = xml_text().prop_map(Node::Text);
    leaf.prop_recursive(4, 48, 6, |inner| {
        (
            name(),
            prop::collection::vec((name(), xml_text()), 0..3),
            prop::collection::vec(inner, 0..6),
        )
            .prop_map(|(name, attrs, children)| Node::Elem {
                name,
                attrs,
                children,
            })
    })
}

/// Serializa escapando lo necesario; con `refs`, todo carácter de texto o de
/// valor se escribe como referencia `&#x…;`.
fn serialize(n: &Node, refs: bool, out: &mut String) {
    let esc = |s: &str, out: &mut String| {
        for c in s.chars() {
            match c {
                _ if refs => write!(out, "&#x{:X};", u32::from(c)).unwrap(),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '&' => out.push_str("&amp;"),
                '"' => out.push_str("&quot;"),
                _ => out.push(c),
            }
        }
    };
    match n {
        Node::Text(t) => esc(t, out),
        Node::Elem {
            name,
            attrs,
            children,
        } => {
            write!(out, "<{name}").unwrap();
            for (k, v) in attrs {
                write!(out, " {k}=\"").unwrap();
                esc(v, out);
                out.push('"');
            }
            if children.is_empty() {
                out.push_str("/>");
            } else {
                out.push('>');
                children.iter().for_each(|c| serialize(c, refs, out));
                write!(out, "</{name}>").unwrap();
            }
        }
    }
}

/// Tokens esperados, con el mismo formato que `render`.
fn expected(n: &Node, out: &mut Vec<String>) {
    match n {
        Node::Text(t) if t.is_empty() => {}
        Node::Text(t) => match out.last_mut() {
            Some(last) if last.starts_with('"') => last.push_str(t),
            _ => out.push(format!("\"{t}")),
        },
        Node::Elem {
            name,
            attrs,
            children,
        } => {
            out.push(format!("<{name}"));
            out.extend(attrs.iter().map(|(k, v)| format!("@{k}={v}")));
            if children.is_empty() {
                out.push("/>".into());
            } else {
                out.push(">".into());
                children.iter().for_each(|c| expected(c, out));
                out.push(format!("</{name}>"));
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Tokenizar entero == tokenizar en trozos arbitrarios.
    #[test]
    fn trozos_arbitrarios(
        bytes in prop::collection::vec(fragment(), 0..40).prop_map(|v| v.concat()),
        cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..8),
    ) {
        let whole = lex_chunked(&bytes, usize::MAX);
        let mut points: Vec<usize> = cuts.iter().map(|i| i.index(bytes.len() + 1)).collect();
        points.sort_unstable();
        let mut parts = Vec::new();
        let mut prev = 0;
        for p in points.into_iter().chain([bytes.len()]) {
            parts.push(&bytes[prev..p]);
            prev = p;
        }
        prop_assert_eq!(lex_parts(parts.into_iter()), whole);
    }

    /// Un documento serializado (con o sin referencias de carácter) se
    /// tokeniza en exactamente los nombres, atributos y textos generados.
    #[test]
    fn oraculo_documento(n in node(), refs in any::<bool>(), chunk in 1usize..64) {
        let mut text = String::new();
        serialize(&n, refs, &mut text);
        let mut want = Vec::new();
        expected(&n, &mut want);
        let (events, err) = lex_chunked(text.as_bytes(), chunk);
        prop_assert_eq!(err, None);
        prop_assert_eq!(render(&events), want);
    }
}
