//! DTD.2: pruebas del parser LL(1) (`crates/dtd/src/parser.rs`).
//!
//! - el AST de cada construcción del subconjunto;
//! - cada rechazo por alcance y cada error sintáctico, con su clase y su byte;
//! - el tope de 256 paréntesis, y que ni un millón de `(` desborda la pila
//!   (el parser es iterativo, D7);
//! - ida y vuelta: una DTD aleatoria escrita como texto y vuelta a analizar
//!   da el mismo AST.

use dtd::ast::{AttType, ContentSpec, DefaultDecl, Dtd, Occurs, Particle, ParticleKind};
use dtd::lexer::ErrorKind as Lex;
use dtd::{parse, ErrorKind, MAX_DEPTH};
use proptest::prelude::*;

fn ok(s: &str) -> Dtd {
    parse(s.as_bytes()).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

fn falla(s: &str) -> (ErrorKind, u64) {
    let e = parse(s.as_bytes()).expect_err(s);
    (e.kind, e.at.offset)
}

// ------------------------------------------------------- forma canónica ---

fn occ(o: Occurs) -> &'static str {
    match o {
        Occurs::One => "",
        Occurs::Opt => "?",
        Occurs::Star => "*",
        Occurs::Plus => "+",
    }
}

fn particula(p: &Particle, s: &mut String) {
    match &p.kind {
        ParticleKind::Name(n) => s.push_str(&n.text),
        ParticleKind::Seq(xs) | ParticleKind::Choice(xs) => {
            let sep = if matches!(p.kind, ParticleKind::Seq(_)) {
                ", "
            } else {
                " | "
            };
            s.push('(');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    s.push_str(sep);
                }
                particula(x, s);
            }
            s.push(')');
        }
    }
    s.push_str(occ(p.occurs));
}

fn literal(v: &str, s: &mut String) {
    s.push('"');
    for c in v.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '"' => s.push_str("&quot;"),
            c => s.push(c),
        }
    }
    s.push('"');
}

/// Escribe el AST como DTD (sin posiciones): dos AST con el mismo texto son
/// iguales salvo los `Span`.
fn texto(d: &Dtd) -> String {
    let mut s = String::new();
    for e in &d.elements {
        s.push_str("<!ELEMENT ");
        s.push_str(&e.name.text);
        s.push(' ');
        match &e.content {
            ContentSpec::Empty => s.push_str("EMPTY"),
            ContentSpec::Pcdata => s.push_str("(#PCDATA)"),
            ContentSpec::Children(p) => particula(p, &mut s),
        }
        s.push_str(">\n");
    }
    for a in &d.attlists {
        s.push_str("<!ATTLIST ");
        s.push_str(&a.element.text);
        for def in &a.defs {
            s.push_str("\n  ");
            s.push_str(&def.name.text);
            s.push(' ');
            match &def.ty {
                AttType::Cdata => s.push_str("CDATA"),
                AttType::Enumeration(vs) => {
                    s.push('(');
                    s.push_str(
                        &vs.iter()
                            .map(|v| v.text.as_str())
                            .collect::<Vec<_>>()
                            .join("|"),
                    );
                    s.push(')');
                }
            }
            s.push(' ');
            match &def.default {
                DefaultDecl::Required => s.push_str("#REQUIRED"),
                DefaultDecl::Implied => s.push_str("#IMPLIED"),
                DefaultDecl::Fixed(l) => {
                    s.push_str("#FIXED ");
                    literal(&l.value, &mut s);
                }
                DefaultDecl::Default(l) => literal(&l.value, &mut s),
            }
        }
        s.push_str(">\n");
    }
    s
}

// ------------------------------------------------------- casos concretos ---

const PEDIDO: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!-- Pedido de una tienda -->
<!ELEMENT pedido (cliente, linea+, nota?)>
<!ELEMENT cliente (#PCDATA)>
<!ELEMENT linea EMPTY>
<!ATTLIST linea sku CDATA #REQUIRED cant CDATA '1'>
<!ELEMENT nota (#PCDATA)*>
<!ATTLIST pedido estado (nuevo|pagado|enviado) \"nuevo\"
                 moneda CDATA #FIXED 'COP'
                 ref CDATA #IMPLIED>
<?otra instruccion?>
";

#[test]
fn el_pedido_completo() {
    let d = ok(PEDIDO);
    assert_eq!(
        texto(&d),
        "<!ELEMENT pedido (cliente, linea+, nota?)>\n<!ELEMENT cliente (#PCDATA)>\n<!ELEMENT linea EMPTY>\n\
         <!ELEMENT nota (#PCDATA)>\n<!ATTLIST linea\n  sku CDATA #REQUIRED\n  cant CDATA \"1\">\n\
         <!ATTLIST pedido\n  estado (nuevo|pagado|enviado) \"nuevo\"\n  moneda CDATA #FIXED \"COP\"\n  \
         ref CDATA #IMPLIED>\n"
    );
    // Posiciones: la declaración entera y el nombre.
    let pedido = &d.elements[0];
    assert_eq!(pedido.name.text, "pedido");
    assert_eq!(
        &PEDIDO[pedido.span.start.offset as usize..pedido.span.end as usize],
        "<!ELEMENT pedido (cliente, linea+, nota?)>"
    );
    assert_eq!(
        (pedido.name.span.start.line, pedido.name.span.start.column),
        (3, 11)
    );
    let ContentSpec::Children(p) = &pedido.content else {
        panic!()
    };
    assert_eq!(
        &PEDIDO[p.span.start.offset as usize..p.span.end as usize],
        "(cliente, linea+, nota?)"
    );
}

#[test]
fn grupos_anidados_y_sufijos() {
    let d = ok("<!ELEMENT a ((b|c)*, (d, e)+, f?)?>");
    assert_eq!(texto(&d), "<!ELEMENT a ((b | c)*, (d, e)+, f?)?>\n");
    let ContentSpec::Children(p) = &d.elements[0].content else {
        panic!()
    };
    assert_eq!(p.occurs, Occurs::Opt);
    let ParticleKind::Seq(xs) = &p.kind else {
        panic!()
    };
    assert!(matches!(xs[0].kind, ParticleKind::Choice(_)));
    // Un grupo de un solo elemento es una secuencia.
    assert_eq!(texto(&ok("<!ELEMENT a (b)>")), "<!ELEMENT a (b)>\n");
}

#[test]
fn palabras_clave_contextuales() {
    // `EMPTY`, `CDATA`, `SYSTEM` como nombres (gramatica-dtd.md §2).
    assert_eq!(
        texto(&ok("<!ELEMENT EMPTY EMPTY>")),
        "<!ELEMENT EMPTY EMPTY>\n"
    );
    assert_eq!(
        texto(&ok("<!ELEMENT a (EMPTY|ANY)>")),
        "<!ELEMENT a (EMPTY | ANY)>\n"
    );
    assert_eq!(
        texto(&ok(
            "<!ATTLIST CDATA CDATA CDATA #IMPLIED SYSTEM (PUBLIC) #REQUIRED>"
        )),
        "<!ATTLIST CDATA\n  CDATA CDATA #IMPLIED\n  SYSTEM (PUBLIC) #REQUIRED>\n"
    );
    // Enumeraciones de nmtokens que empiezan por dígito; ATTLIST vacía.
    assert_eq!(
        texto(&ok("<!ATTLIST a n (1|2.5|-x) '1'>")),
        "<!ATTLIST a\n  n (1|2.5|-x) \"1\">\n"
    );
    assert_eq!(ok("<!ATTLIST a>").attlists[0].defs.len(), 0);
    assert_eq!(ok("").elements.len(), 0);
}

#[test]
fn literales_con_referencias() {
    let d = ok("<!ATTLIST a b CDATA \"x &lt; &#65;&amp; 50%\">");
    let DefaultDecl::Default(l) = &d.attlists[0].defs[0].default else {
        panic!()
    };
    assert_eq!(l.value, "x < A& 50%");
}

// ------------------------------------------------------ fuera de alcance ---

#[test]
fn rechazos_por_alcance_con_su_posicion() {
    use ErrorKind::OutOfScope;
    assert_eq!(falla("<!ELEMENT a ANY>"), (OutOfScope, 12));
    assert_eq!(falla("<!ELEMENT a (#PCDATA|b)*>"), (OutOfScope, 12));
    assert_eq!(falla("<!ATTLIST a i ID #REQUIRED>"), (OutOfScope, 14));
    assert_eq!(falla("<!ATTLIST a i IDREFS #IMPLIED>"), (OutOfScope, 14));
    assert_eq!(falla("<!ATTLIST a i ENTITY #IMPLIED>"), (OutOfScope, 14));
    assert_eq!(falla("<!ATTLIST a i NMTOKEN #IMPLIED>"), (OutOfScope, 14));
    assert_eq!(
        falla("<!ATTLIST a i NOTATION (n) #IMPLIED>"),
        (OutOfScope, 14)
    );
    assert_eq!(falla("<!NOTATION gif SYSTEM 'image/gif'>"), (OutOfScope, 0));
    assert_eq!(falla("<!ELEMENT x:a EMPTY>"), (OutOfScope, 10));
    assert_eq!(falla("<!ELEMENT a (b, x:c)>"), (OutOfScope, 16));
    assert_eq!(
        falla("<?xml version='1.0' encoding='ISO-8859-1'?>"),
        (OutOfScope, 0)
    );
    // El motivo se nombra en el mensaje.
    let e = parse(b"<!ELEMENT a ANY>").unwrap_err();
    assert!(
        e.message.contains("ANY") && e.message.contains("§3bis.2"),
        "{e}"
    );
}

#[test]
fn lo_prohibido_por_seguridad_llega_como_error_lexico() {
    assert_eq!(
        falla("<!ENTITY x 'y'>"),
        (ErrorKind::Lexical(Lex::EntityForbidden), 0)
    );
    assert_eq!(
        falla("<!ELEMENT a EMPTY><!DOCTYPE a>"),
        (ErrorKind::Lexical(Lex::DoctypeForbidden), 18)
    );
    assert_eq!(
        falla("<![INCLUDE[]]>"),
        (ErrorKind::Lexical(Lex::ConditionalSectionForbidden), 0)
    );
    assert_eq!(
        falla("<!ELEMENT a %m;>"),
        (ErrorKind::Lexical(Lex::ParameterEntityForbidden), 12)
    );
}

// ---------------------------------------------------------- sintácticos ---

#[test]
fn errores_sintacticos_con_su_posicion() {
    use ErrorKind::Syntax;
    assert_eq!(falla("<!ELEMENT a>"), (Syntax, 11));
    assert_eq!(falla("<!ELEMENT>"), (Syntax, 9));
    assert_eq!(falla("<!ELEMENT a b>"), (Syntax, 12));
    assert_eq!(falla("<!ELEMENT a (b, c | d)>"), (Syntax, 18));
    assert_eq!(falla("<!ELEMENT a (b | c, d)>"), (Syntax, 18));
    assert_eq!(falla("<!ELEMENT a ()>"), (Syntax, 13));
    assert_eq!(falla("<!ELEMENT a (b,)>"), (Syntax, 15));
    assert_eq!(falla("<!ELEMENT a (b, #PCDATA)>"), (Syntax, 16));
    assert_eq!(falla("<!ELEMENT a (#FOO)>"), (Syntax, 13));
    assert_eq!(falla("<!ELEMENT a (b) EMPTY>"), (Syntax, 16));
    assert_eq!(falla("<!ELEMENT 1a EMPTY>"), (Syntax, 10));
    assert_eq!(falla("<!ELEMENT a (-b)>"), (Syntax, 13));
    assert_eq!(falla("<!FOO a>"), (Syntax, 0));
    assert_eq!(falla("<!ATTLIST a b>"), (Syntax, 13));
    assert_eq!(falla("<!ATTLIST a b CDATA>"), (Syntax, 19));
    assert_eq!(falla("<!ATTLIST a b CDATA #FIXED>"), (Syntax, 26));
    assert_eq!(falla("<!ATTLIST a b (x y) #IMPLIED>"), (Syntax, 17));
    assert_eq!(falla("<!ATTLIST a b () #IMPLIED>"), (Syntax, 15));
    assert_eq!(falla("<!ATTLIST a b FOO #IMPLIED>"), (Syntax, 14));
    assert_eq!(falla("<!ATTLIST a b CDATA #PCDATA>"), (Syntax, 20));
    assert_eq!(
        falla("<!ELEMENT a EMPTY><?xml version='1.0'?>"),
        (Syntax, 18)
    );
    // El mensaje dice qué se esperaba y qué llegó.
    let e = parse(b"<!ELEMENT a (b, c | d)>").unwrap_err();
    assert!(e.message.contains("mezclar"), "{e}");
    let e = parse(b"<!ELEMENT a b>").unwrap_err();
    assert_eq!(e.to_string(), "1:13: se esperaba EMPTY, ANY o «(» tras el nombre del elemento, pero se encontró el nombre «b»");
}

// ----------------------------------------------------------- profundidad ---

fn anidado(n: usize) -> String {
    format!("<!ELEMENT a {}b{}>", "(".repeat(n), ")".repeat(n))
}

#[test]
fn hasta_256_parentesis_si_y_el_257_es_limite() {
    let d = ok(&anidado(MAX_DEPTH));
    let ContentSpec::Children(mut p) = d.elements[0].content.clone() else {
        panic!()
    };
    let mut niveles = 1;
    while let ParticleKind::Seq(mut xs) = p.kind {
        match xs.pop() {
            Some(x) if matches!(x.kind, ParticleKind::Seq(_)) => {
                niveles += 1;
                p = x;
            }
            _ => break,
        }
    }
    assert_eq!(niveles, MAX_DEPTH);
    // El `(` número 257 está en el byte 12 + 256.
    assert_eq!(
        falla(&anidado(MAX_DEPTH + 1)),
        (ErrorKind::Limit, 12 + MAX_DEPTH as u64)
    );
}

#[test]
fn un_millon_de_parentesis_no_desborda_la_pila() {
    // Hilo con 256 KiB de pila: un parser recursivo no llegaría al tope.
    let h = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| falla(&anidado(1_000_000)))
        .unwrap();
    assert_eq!(h.join().unwrap(), (ErrorKind::Limit, 12 + MAX_DEPTH as u64));
}

// --------------------------------------------------------- ida y vuelta ---

fn nombre() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_.-]{0,5}"
}

fn ocurrencia() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec!["", "?", "*", "+"])
}

/// Un modelo de contenido aleatorio como texto (siempre un grupo).
fn modelo() -> impl Strategy<Value = String> {
    let hoja = (nombre(), ocurrencia()).prop_map(|(n, o)| format!("{n}{o}"));
    let cp = hoja.prop_recursive(4, 32, 4, |interior| {
        (
            prop::collection::vec(interior, 1..4),
            prop::bool::ANY,
            ocurrencia(),
        )
            .prop_map(|(xs, eleccion, o)| {
                let sep = if eleccion && xs.len() > 1 {
                    " | "
                } else {
                    ", "
                };
                format!("({}){o}", xs.join(sep))
            })
    });
    (
        prop::collection::vec(cp, 1..4),
        prop::bool::ANY,
        ocurrencia(),
    )
        .prop_map(|(xs, eleccion, o)| {
            let sep = if eleccion && xs.len() > 1 {
                " | "
            } else {
                ", "
            };
            format!("({}){o}", xs.join(sep))
        })
}

fn contenido() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("EMPTY".to_string()),
        Just("(#PCDATA)".to_string()),
        modelo()
    ]
}

fn valor() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 <&\"'%é]{0,6}"
}

fn atributo() -> impl Strategy<Value = String> {
    let tipo = prop_oneof![
        Just("CDATA".to_string()),
        prop::collection::vec("[a-z0-9]{1,3}", 1..4).prop_map(|vs| format!("({})", vs.join("|"))),
    ];
    let defecto = prop_oneof![
        Just("#REQUIRED".to_string()),
        Just("#IMPLIED".to_string()),
        valor().prop_map(|v| {
            let mut s = "#FIXED ".to_string();
            literal(&v, &mut s);
            s
        }),
        valor().prop_map(|v| {
            let mut s = String::new();
            literal(&v, &mut s);
            s
        }),
    ];
    (nombre(), tipo, defecto).prop_map(|(n, t, d)| format!("{n} {t} {d}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn ida_y_vuelta(
        elementos in prop::collection::vec((nombre(), contenido()), 0..6),
        listas in prop::collection::vec((nombre(), prop::collection::vec(atributo(), 0..4)), 0..4),
    ) {
        let mut fuente = String::new();
        for (n, c) in &elementos {
            fuente.push_str(&format!("<!ELEMENT {n} {c}>\n"));
        }
        for (n, attrs) in &listas {
            fuente.push_str(&format!("<!ATTLIST {n}"));
            for a in attrs {
                fuente.push_str("\n  ");
                fuente.push_str(a);
            }
            fuente.push_str(">\n");
        }
        let d = parse(fuente.as_bytes()).map_err(|e| TestCaseError::fail(format!("{fuente}\n{e}")))?;
        prop_assert_eq!(d.elements.len(), elementos.len());
        prop_assert_eq!(d.attlists.len(), listas.len());
        // Escribir el AST y volver a analizarlo da el mismo AST.
        let t = texto(&d);
        let d2 = parse(t.as_bytes()).map_err(|e| TestCaseError::fail(format!("{t}\n{e}")))?;
        prop_assert_eq!(texto(&d2), t);
    }
}
