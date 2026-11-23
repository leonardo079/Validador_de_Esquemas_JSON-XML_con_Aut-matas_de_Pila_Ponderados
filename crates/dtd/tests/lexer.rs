//! DTD.1: pruebas del tokenizador DTD (`crates/dtd/src/lexer.rs`).
//!
//! - casos concretos de cada token y de cada error, con su posición;
//! - el mismo resultado con bloques de 1, 7 y 4 096 bytes (streaming);
//! - un **oráculo**: un tokenizador de referencia escrito aparte, sin tabla,
//!   que tiene que dar los mismos eventos y el mismo error (clase y byte) que
//!   el AFD en entradas generadas al azar.

use dtd::lexer::{tokenize, DtdLexer, Error, ErrorKind, Event, Sink};
use lexer::Span;
use proptest::prelude::*;

struct Rec(Vec<(Event, Span)>);

impl Sink for Rec {
    type Error = ();
    fn event(&mut self, ev: Event, span: Span) -> Result<(), ()> {
        self.0.push((ev, span));
        Ok(())
    }
}

/// Resultado comparable: eventos y, si hubo, el error (clase y byte).
type Salida = (Vec<Event>, Option<(ErrorKind, u64)>);

/// Eventos con su rango y, si hubo, el error.
type ConRangos = (Vec<(Event, Span)>, Option<(ErrorKind, u64)>);

fn por_bloques(input: &[u8], bloque: usize) -> ConRangos {
    let mut lx = DtdLexer::new();
    let mut rec = Rec(Vec::new());
    let mut err = None;
    for trozo in input.chunks(bloque.max(1)) {
        if let Err(e) = lx.feed(trozo, &mut rec) {
            err = Some(e);
            break;
        }
    }
    if err.is_none() {
        err = lx.finish(&mut rec).err();
    }
    let err = err.map(|e| match e {
        Error::Lex(l) => (l.kind, l.at.offset),
        Error::Sink(()) => unreachable!(),
    });
    (rec.0, err)
}

fn lexer(input: &[u8]) -> Salida {
    let (ev, err) = por_bloques(input, 4096);
    (ev.into_iter().map(|(e, _)| e).collect(), err)
}

/// Representación compacta de los eventos para los casos concretos.
fn compacto(evs: &[Event]) -> String {
    let mut s = String::new();
    let mut en = None::<char>;
    for e in evs {
        let (marca, c) = match e {
            Event::NameChar(c) => ('n', Some(*c)),
            Event::LiteralChar(c) => ('l', Some(*c)),
            Event::PiChar(c) => ('p', Some(*c)),
            _ => (' ', None),
        };
        if let Some(c) = c {
            if en != Some(marca) {
                s.push(' ');
                en = Some(marca);
            }
            s.push(c);
            continue;
        }
        en = None;
        s.push(' ');
        s.push_str(match e {
            Event::DeclOpen => "<!",
            Event::NameBegin => "N:",
            Event::HashBegin => "#",
            Event::LParen => "(",
            Event::RParen => ")",
            Event::Pipe => "|",
            Event::Comma => ",",
            Event::Opt => "?",
            Event::Star => "*",
            Event::Plus => "+",
            Event::LiteralBegin => "\"",
            Event::LiteralEnd => "\"/",
            Event::DeclClose => ">",
            Event::PiOpen => "<?",
            Event::PiClose => "?>",
            _ => unreachable!(),
        });
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ok(input: &str) -> String {
    let (ev, err) = lexer(input.as_bytes());
    assert_eq!(err, None, "{input:?}");
    compacto(&ev)
}

fn falla(input: &str) -> (ErrorKind, u64) {
    lexer(input.as_bytes())
        .1
        .unwrap_or_else(|| panic!("{input:?} debía fallar"))
}

// ------------------------------------------------------- casos concretos ---

#[test]
fn declaracion_de_elemento_con_modelo() {
    assert_eq!(
        ok("<!ELEMENT pedido (cliente, linea+, nota?)>"),
        "<! N: ELEMENT N: pedido ( N: cliente , N: linea + , N: nota ? ) >"
    );
    assert_eq!(ok("<!ELEMENT a EMPTY>"), "<! N: ELEMENT N: a N: EMPTY >");
    assert_eq!(
        ok("<!ELEMENT a (#PCDATA)>"),
        "<! N: ELEMENT N: a ( # PCDATA ) >"
    );
    assert_eq!(
        ok("<!ELEMENT a ((b|c)*,d)+>"),
        "<! N: ELEMENT N: a ( ( N: b | N: c ) * , N: d ) + >"
    );
}

#[test]
fn lista_de_atributos_con_literales() {
    assert_eq!(
        ok("<!ATTLIST a id CDATA #REQUIRED tipo (x|y) \"x\" v CDATA #FIXED '1'>"),
        "<! N: ATTLIST N: a N: id N: CDATA # REQUIRED N: tipo ( N: x | N: y ) \" x \"/ \
         N: v N: CDATA # FIXED \" 1 \"/ >"
    );
    // Enumeración de nmtokens que empiezan por dígito.
    assert_eq!(
        ok("<!ATTLIST a n (1|2) #IMPLIED>"),
        "<! N: ATTLIST N: a N: n ( N: 1 | N: 2 ) # IMPLIED >"
    );
}

#[test]
fn referencias_resueltas_en_literales() {
    assert_eq!(
        ok("<!ATTLIST a b CDATA \"&lt;&#65;&#x42;&amp;'\">"),
        "<! N: ATTLIST N: a N: b N: CDATA \" <AB&' \"/ >"
    );
    // `%` dentro de un literal es un carácter más (enmienda E4).
    assert_eq!(
        ok("<!ATTLIST a b CDATA '50%'>"),
        "<! N: ATTLIST N: a N: b N: CDATA \" 50% \"/ >"
    );
}

#[test]
fn comentarios_e_instrucciones() {
    assert_eq!(
        ok("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- c - x -->\n<!ELEMENT a EMPTY>"),
        "<? xml version=\"1.0\" encoding=\"UTF-8\" ?> <! N: ELEMENT N: a N: EMPTY >"
    );
    assert_eq!(ok("<?pi a??b?>"), "<? pi a??b ?>");
    assert_eq!(ok("<?pi?>"), "<? pi ?>");
    assert_eq!(ok(""), "");
    assert_eq!(ok("\u{feff}  <!-- solo -->  "), "");
}

#[test]
fn system_y_public_son_nombres_validos() {
    // Enmienda E4: no son palabras prohibidas.
    assert_eq!(
        ok("<!ELEMENT SYSTEM EMPTY>"),
        "<! N: ELEMENT N: SYSTEM N: EMPTY >"
    );
    assert_eq!(
        ok("<!ATTLIST a PUBLIC CDATA #IMPLIED>"),
        "<! N: ATTLIST N: a N: PUBLIC N: CDATA # IMPLIED >"
    );
}

#[test]
fn no_ascii_en_nombres_y_literales() {
    assert_eq!(
        ok("<!ELEMENT año (niño)>"),
        "<! N: ELEMENT N: año ( N: niño ) >"
    );
    assert_eq!(
        ok("<!ATTLIST a b CDATA \"ü€😀\">"),
        "<! N: ATTLIST N: a N: b N: CDATA \" ü€😀 \"/ >"
    );
}

#[test]
fn rangos_de_los_eventos() {
    let (ev, err) = por_bloques(b"<!ELEMENT a (b)>\n<!ATTLIST a c CDATA 'v'>", 4096);
    assert_eq!(err, None);
    let rango = |e: Event, n: usize| -> (u64, u64) {
        let s = ev.iter().filter(|(x, _)| *x == e).nth(n).expect("evento").1;
        (s.start.offset, s.end)
    };
    assert_eq!(rango(Event::DeclOpen, 0), (0, 2));
    assert_eq!(rango(Event::LParen, 0), (12, 13));
    assert_eq!(rango(Event::DeclClose, 0), (15, 16));
    assert_eq!(rango(Event::DeclClose, 1), (40, 41));
    assert_eq!(rango(Event::LiteralEnd, 0), (37, 40));
    let ultimo = ev
        .iter()
        .filter(|(e, _)| *e == Event::DeclOpen)
        .nth(1)
        .unwrap()
        .1
        .start;
    assert_eq!((ultimo.line, ultimo.column), (2, 1));
}

// -------------------------------------------------- prohibido por seguridad ---

#[test]
fn entity_y_doctype_son_error_en_su_menor_que() {
    assert_eq!(falla("<!ENTITY x \"y\">"), (ErrorKind::EntityForbidden, 0));
    assert_eq!(
        falla("<!ELEMENT a EMPTY>\n<!ENTITY % p \"y\">"),
        (ErrorKind::EntityForbidden, 19)
    );
    assert_eq!(falla("<!entity x 'y'>"), (ErrorKind::EntityForbidden, 0));
    assert_eq!(falla("<!DOCTYPE x>"), (ErrorKind::DoctypeForbidden, 0));
    assert_eq!(falla("<!doctype x>"), (ErrorKind::DoctypeForbidden, 0));
    assert_eq!(falla("<!DOCTYPE>"), (ErrorKind::DoctypeForbidden, 0));
    assert_eq!(falla("<!ENTITY%x>"), (ErrorKind::EntityForbidden, 0));
    assert_eq!(falla("<!ENTITY"), (ErrorKind::EntityForbidden, 0));
    // Otras palabras pasan al parser (que rechaza `NOTATION` por alcance).
    assert_eq!(lexer(b"<!NOTATION n>").1, None);
    assert_eq!(lexer(b"<!ENTITYX a>").1, None);
}

#[test]
fn secciones_condicionales_y_referencias_de_parametro() {
    assert_eq!(
        falla("<![INCLUDE[ <!ELEMENT a EMPTY> ]]>"),
        (ErrorKind::ConditionalSectionForbidden, 0)
    );
    assert_eq!(
        falla("  <![IGNORE["),
        (ErrorKind::ConditionalSectionForbidden, 2)
    );
    assert_eq!(falla("%p;"), (ErrorKind::ParameterEntityForbidden, 0));
    assert_eq!(
        falla("<!ELEMENT a %p;>"),
        (ErrorKind::ParameterEntityForbidden, 12)
    );
    assert_eq!(
        falla("<!ELEMENT a (b|%p;)>"),
        (ErrorKind::ParameterEntityForbidden, 15)
    );
}

#[test]
fn entidades_no_predefinidas_en_literales() {
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"&x;\">"),
        (ErrorKind::UndefinedEntity, 21)
    );
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"&quote;\">"),
        (ErrorKind::UndefinedEntity, 21)
    );
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"&;\">"),
        (ErrorKind::InvalidReference, 22)
    );
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"&#0;\">"),
        (ErrorKind::InvalidCharRef, 21)
    );
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"&#x110000;\">"),
        (ErrorKind::InvalidCharRef, 21)
    );
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"a & b\">"),
        (ErrorKind::InvalidReference, 24)
    );
    // `&` y `<` fuera de un literal no son referencias: son bytes inesperados.
    assert_eq!(falla("<!ELEMENT a &b;>"), (ErrorKind::UnexpectedByte, 12));
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"<\">"),
        (ErrorKind::UnexpectedByte, 21)
    );
}

#[test]
fn errores_de_forma() {
    assert_eq!(falla("<!ELEMENT a(b)>"), (ErrorKind::UnexpectedByte, 11));
    assert_eq!(
        falla("<!ATTLIST a b CDATA\"x\">"),
        (ErrorKind::UnexpectedByte, 19)
    );
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"x\"c CDATA #IMPLIED>"),
        (ErrorKind::UnexpectedByte, 23)
    );
    assert_eq!(falla("<!ELEMENT a (b)c>"), (ErrorKind::UnexpectedByte, 15));
    assert_eq!(falla("<!ELEMENT a ?>"), (ErrorKind::UnexpectedByte, 12));
    assert_eq!(falla("<!-- a -- b -->"), (ErrorKind::UnexpectedByte, 9));
    assert_eq!(falla("<??>"), (ErrorKind::InvalidPi, 2));
    assert_eq!(falla("<!ELEMENT a EMPTY"), (ErrorKind::UnexpectedEof, 17));
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"x"),
        (ErrorKind::UnexpectedEof, 22)
    );
    assert_eq!(falla("<!-- sin cerrar"), (ErrorKind::UnexpectedEof, 15));
    assert_eq!(falla("texto"), (ErrorKind::UnexpectedByte, 0));
    assert_eq!(falla("<!ELEMENT a\u{1}>"), (ErrorKind::InvalidChar, 11));
    assert_eq!(falla("  é"), (ErrorKind::UnexpectedByte, 2));
    assert_eq!(
        falla("<!ATTLIST a b CDATA \"\u{fffe}\">"),
        (ErrorKind::InvalidChar, 21)
    );
}

#[test]
fn utf8_invalido() {
    assert_eq!(
        lexer(b"<!ELEMENT a\xC3(b)>").1,
        Some((ErrorKind::InvalidUtf8, 11))
    );
    assert_eq!(
        lexer(b"<!ATTLIST a b CDATA \"\xE2\x82\">").1,
        Some((ErrorKind::InvalidUtf8, 21))
    );
    assert_eq!(
        lexer(b"<!-- \xC0\xAF -->").1,
        Some((ErrorKind::InvalidUtf8, 5))
    );
    assert_eq!(
        lexer(b"<!ATTLIST a b CDATA \"\xF0\x9F\"").1,
        Some((ErrorKind::InvalidUtf8, 21))
    );
}

#[test]
fn tras_un_error_el_lexer_queda_detenido() {
    let mut lx = DtdLexer::new();
    let mut rec = Rec(Vec::new());
    let e = lx.feed(b"<!ENTITY x 'y'>", &mut rec).unwrap_err();
    assert_eq!(lx.feed(b"<!ELEMENT a EMPTY>", &mut rec).unwrap_err(), e);
    assert_eq!(lx.finish(&mut rec).unwrap_err(), e);
}

#[test]
fn el_consumidor_puede_detener_el_analisis() {
    struct Para;
    impl Sink for Para {
        type Error = &'static str;
        fn event(&mut self, ev: Event, _: Span) -> Result<(), &'static str> {
            if ev == Event::DeclClose {
                Err("basta")
            } else {
                Ok(())
            }
        }
    }
    assert!(matches!(
        tokenize(b"<!ELEMENT a EMPTY>", &mut Para),
        Err(Error::Sink("basta"))
    ));
}

// -------------------------------------------------------- por bloques ---

const EJEMPLOS: &[&str] = &[
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- pedido -->\n<!ELEMENT pedido (cliente, linea+, nota?)>\n\
     <!ELEMENT cliente (#PCDATA)>\n<!ELEMENT linea EMPTY>\n<!ATTLIST linea sku CDATA #REQUIRED cant CDATA '1'>\n\
     <!ELEMENT nota (#PCDATA)>\n<!ATTLIST pedido estado (nuevo|pagado) \"nuevo\" moneda CDATA #FIXED 'COP'>",
    "\u{feff}<!ELEMENT año ((niño|niña)*, ü?)> <!ATTLIST año v CDATA \"&lt;€&#x1F600;&apos;\">",
    "<!ELEMENT a EMPTY> <!ENTITY x 'y'>",
    "<!ATTLIST a b CDATA \"&#x110000;\">",
    "<!ELEMENT a (b|c)+>   <![IGNORE[",
];

#[test]
fn mismo_resultado_con_bloques_de_1_7_y_4096() {
    for e in EJEMPLOS {
        let completo = por_bloques(e.as_bytes(), 4096);
        for b in [1, 7] {
            assert_eq!(por_bloques(e.as_bytes(), b), completo, "bloque {b}: {e:?}");
        }
    }
}

// ---------------------------------------------------------------- oráculo ---

fn ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}
fn ascii_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':'
}
fn nmtoken(c: char) -> bool {
    ascii_start(c) || c.is_ascii_digit() || c == '-' || c == '.' || !c.is_ascii()
}
fn control(c: char) -> bool {
    (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r')
}
fn xml_char(c: char) -> bool {
    dtd::lexer::is_xml_char(c as u32)
}

/// Tokenizador de referencia: lee la DTD de arriba abajo con código
/// estructurado (sin tabla) y aplica las reglas de `docs/fase1/lexico-dtd.md`.
fn oraculo(input: &str) -> Salida {
    let cs: Vec<(u64, char)> = input.char_indices().map(|(i, c)| (i as u64, c)).collect();
    let fin = input.len() as u64;
    let mut ev = Vec::new();
    let r = oraculo_en(&cs, fin, &mut ev);
    (ev, r.err())
}

type R = Result<(), (ErrorKind, u64)>;

/// Error genérico para un carácter que no cabe: control → `InvalidChar`.
fn inesperado(c: char, at: u64, pct: bool) -> (ErrorKind, u64) {
    if control(c) {
        (ErrorKind::InvalidChar, at)
    } else if pct && c == '%' {
        (ErrorKind::ParameterEntityForbidden, at)
    } else {
        (ErrorKind::UnexpectedByte, at)
    }
}

fn oraculo_en(cs: &[(u64, char)], fin: u64, ev: &mut Vec<Event>) -> R {
    let mut i = 0;
    if matches!(cs.first(), Some((0, '\u{feff}'))) {
        i = 1;
    }
    let eof = Err((ErrorKind::UnexpectedEof, fin));
    let nombre_valido = |c: char, at: u64| {
        if xml_char(c) {
            Ok(())
        } else {
            Err((ErrorKind::InvalidChar, at))
        }
    };
    loop {
        // Entre declaraciones.
        while i < cs.len() && ws(cs[i].1) {
            i += 1;
        }
        let Some(&(lt, c)) = cs.get(i) else {
            return Ok(());
        };
        if c != '<' {
            return Err(inesperado(c, lt, true));
        }
        i += 1;
        let Some(&(at, c)) = cs.get(i) else {
            return eof;
        };
        i += 1;
        match c {
            '?' => {
                let Some(&(at, c)) = cs.get(i) else {
                    return eof;
                };
                if !(ascii_start(c) || !c.is_ascii()) {
                    return Err(if control(c) {
                        (ErrorKind::InvalidChar, at)
                    } else {
                        (ErrorKind::InvalidPi, at)
                    });
                }
                // El AFD emite `<?` antes de decodificar el primer carácter.
                ev.push(Event::PiOpen);
                // Destino.
                loop {
                    let Some(&(at, c)) = cs.get(i) else {
                        return eof;
                    };
                    if nmtoken(c) {
                        nombre_valido(c, at)?;
                        ev.push(Event::NameChar(c));
                        i += 1;
                    } else if ws(c) {
                        i += 1;
                        break;
                    } else if c == '?' {
                        let Some(&(at2, c2)) = cs.get(i + 1) else {
                            return eof;
                        };
                        if c2 == '>' {
                            ev.push(Event::PiClose);
                            i += 2;
                            break;
                        }
                        return Err(if control(c2) {
                            (ErrorKind::InvalidChar, at2)
                        } else {
                            (ErrorKind::InvalidPi, at2)
                        });
                    } else {
                        return Err(if control(c) {
                            (ErrorKind::InvalidChar, at)
                        } else {
                            (ErrorKind::InvalidPi, at)
                        });
                    }
                }
                if ev.last() == Some(&Event::PiClose) {
                    continue;
                }
                // Contenido hasta `?>`.
                loop {
                    let Some(&(at, c)) = cs.get(i) else {
                        return eof;
                    };
                    i += 1;
                    if control(c) {
                        return Err((ErrorKind::InvalidChar, at));
                    }
                    if c == '?' {
                        // `?`s retenidos hasta saber si cierran.
                        loop {
                            let Some(&(at2, c2)) = cs.get(i) else {
                                return eof;
                            };
                            if c2 == '?' {
                                ev.push(Event::PiChar('?'));
                                i += 1;
                            } else if c2 == '>' {
                                i += 1;
                                ev.push(Event::PiClose);
                                break;
                            } else if control(c2) {
                                return Err((ErrorKind::InvalidChar, at2));
                            } else {
                                ev.push(Event::PiChar('?'));
                                break;
                            }
                        }
                        if ev.last() == Some(&Event::PiClose) {
                            break;
                        }
                        continue;
                    }
                    nombre_valido(c, at)?;
                    ev.push(Event::PiChar(c));
                }
            }
            '!' => {
                let Some(&(at, c)) = cs.get(i) else {
                    return eof;
                };
                i += 1;
                if c == '-' {
                    let Some(&(at, c)) = cs.get(i) else {
                        return eof;
                    };
                    i += 1;
                    if c != '-' {
                        return Err(inesperado(c, at, false));
                    }
                    // Comentario.
                    let mut guiones = 0;
                    loop {
                        let Some(&(at, c)) = cs.get(i) else {
                            return eof;
                        };
                        i += 1;
                        if guiones == 2 {
                            if c == '>' {
                                break;
                            }
                            return Err(inesperado(c, at, false));
                        }
                        if control(c) {
                            return Err((ErrorKind::InvalidChar, at));
                        }
                        if c == '-' {
                            guiones += 1;
                        } else {
                            nombre_valido(c, at)?;
                            guiones = 0;
                        }
                    }
                    continue;
                }
                if c == '[' {
                    return Err((ErrorKind::ConditionalSectionForbidden, lt));
                }
                if !ascii_start(c) {
                    return Err(inesperado(c, at, false));
                }
                ev.push(Event::DeclOpen);
                ev.push(Event::NameBegin);
                ev.push(Event::NameChar(c));
                let mut kw = String::from(c);
                let prohibida = |kw: &str| {
                    if kw.eq_ignore_ascii_case("ENTITY") {
                        Some(ErrorKind::EntityForbidden)
                    } else if kw.eq_ignore_ascii_case("DOCTYPE") {
                        Some(ErrorKind::DoctypeForbidden)
                    } else {
                        None
                    }
                };
                loop {
                    let Some(&(at, c)) = cs.get(i) else {
                        return Err(prohibida(&kw)
                            .map(|k| (k, lt))
                            .unwrap_or((ErrorKind::UnexpectedEof, fin)));
                    };
                    if ascii_start(c) {
                        ev.push(Event::NameChar(c));
                        kw.push(c);
                        i += 1;
                        continue;
                    }
                    if let Some(k) = prohibida(&kw) {
                        return Err((k, lt));
                    }
                    if ws(c) {
                        i += 1;
                        break;
                    }
                    if c == '>' {
                        i += 1;
                        ev.push(Event::DeclClose);
                        break;
                    }
                    return Err(inesperado(c, at, true));
                }
                if ev.last() == Some(&Event::DeclClose) {
                    continue;
                }
                cuerpo(cs, &mut i, fin, ev)?;
            }
            c => return Err(inesperado(c, at, false)),
        }
    }
}

/// Lo que va antes del carácter actual dentro de una declaración.
#[derive(Clone, Copy, PartialEq)]
enum Previo {
    Separador,
    Nombre,
    Cerrado,
    Literal,
}

/// Cuerpo de una declaración, hasta su `>`.
fn cuerpo(cs: &[(u64, char)], i: &mut usize, fin: u64, ev: &mut Vec<Event>) -> R {
    use Previo::*;
    let mut previo = Separador;
    loop {
        let Some(&(at, c)) = cs.get(*i) else {
            return Err((ErrorKind::UnexpectedEof, fin));
        };
        *i += 1;
        let malo = Err(inesperado(c, at, true));
        match c {
            _ if ws(c) => previo = Separador,
            '>' => {
                ev.push(Event::DeclClose);
                return Ok(());
            }
            _ if previo == Literal => return malo,
            '(' if previo == Separador => ev.push(Event::LParen),
            ')' => {
                ev.push(Event::RParen);
                previo = Cerrado;
            }
            '|' | ',' => {
                ev.push(if c == '|' { Event::Pipe } else { Event::Comma });
                previo = Separador;
            }
            '?' | '*' | '+' if previo != Separador => {
                ev.push(match c {
                    '?' => Event::Opt,
                    '*' => Event::Star,
                    _ => Event::Plus,
                });
                previo = Cerrado;
            }
            _ if nmtoken(c) && previo != Cerrado => {
                if previo == Separador {
                    ev.push(Event::NameBegin);
                }
                if !xml_char(c) {
                    return Err((ErrorKind::InvalidChar, at));
                }
                ev.push(Event::NameChar(c));
                previo = Nombre;
            }
            '#' if previo == Separador => {
                ev.push(Event::HashBegin);
                let Some(&(at, c)) = cs.get(*i) else {
                    return Err((ErrorKind::UnexpectedEof, fin));
                };
                *i += 1;
                if !ascii_start(c) {
                    return Err(inesperado(c, at, true));
                }
                ev.push(Event::NameChar(c));
                previo = Nombre;
            }
            '"' | '\'' if previo == Separador => {
                ev.push(Event::LiteralBegin);
                literal(cs, i, fin, c, ev)?;
                ev.push(Event::LiteralEnd);
                previo = Literal;
            }
            _ => return malo,
        }
    }
}

fn literal(cs: &[(u64, char)], i: &mut usize, fin: u64, comilla: char, ev: &mut Vec<Event>) -> R {
    let eof = Err((ErrorKind::UnexpectedEof, fin));
    loop {
        let Some(&(at, c)) = cs.get(*i) else {
            return eof;
        };
        *i += 1;
        if c == comilla {
            return Ok(());
        }
        if control(c) {
            return Err((ErrorKind::InvalidChar, at));
        }
        if c == '<' {
            return Err((ErrorKind::UnexpectedByte, at));
        }
        if c != '&' {
            if !xml_char(c) {
                return Err((ErrorKind::InvalidChar, at));
            }
            ev.push(Event::LiteralChar(c));
            continue;
        }
        // Referencia.
        let amp = at;
        let mala = |c: char, at: u64| {
            if control(c) {
                (ErrorKind::InvalidChar, at)
            } else {
                (ErrorKind::InvalidReference, at)
            }
        };
        let Some(&(at, c)) = cs.get(*i) else {
            return eof;
        };
        *i += 1;
        if c == '#' {
            let Some(&(at, c)) = cs.get(*i) else {
                return eof;
            };
            *i += 1;
            let (base, mut valor) = if c == 'x' {
                (16, None)
            } else if c.is_ascii_digit() {
                (10, Some(c.to_digit(10).unwrap()))
            } else {
                return Err(mala(c, at));
            };
            loop {
                let Some(&(at, c)) = cs.get(*i) else {
                    return eof;
                };
                *i += 1;
                if let (';', Some(v)) = (c, valor) {
                    match char::from_u32(v).filter(|&c| xml_char(c)) {
                        Some(c) => ev.push(Event::LiteralChar(c)),
                        None => return Err((ErrorKind::InvalidCharRef, amp)),
                    }
                    break;
                }
                let Some(d) = c.to_digit(base).filter(|_| c.is_ascii()) else {
                    return Err(mala(c, at));
                };
                let v = valor.unwrap_or(0) * base + d;
                if v > 0x10FFFF {
                    return Err((ErrorKind::InvalidCharRef, amp));
                }
                valor = Some(v);
            }
            continue;
        }
        if !ascii_start(c) {
            return Err(mala(c, at));
        }
        let mut nombre = String::from(c);
        loop {
            let Some(&(at, c)) = cs.get(*i) else {
                return eof;
            };
            *i += 1;
            if c == ';' {
                break;
            }
            if !(c.is_ascii() && nmtoken(c)) {
                return Err(mala(c, at));
            }
            if nombre.len() == 4 {
                return Err((ErrorKind::UndefinedEntity, amp));
            }
            nombre.push(c);
        }
        let c = match nombre.as_str() {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => return Err((ErrorKind::UndefinedEntity, amp)),
        };
        ev.push(Event::LiteralChar(c));
    }
}

#[test]
fn el_oraculo_coincide_en_los_casos_escritos_a_mano() {
    for e in EJEMPLOS {
        assert_eq!(oraculo(e), lexer(e.as_bytes()), "{e:?}");
    }
}

/// Trozos con los que se arman DTD al azar: tokens válidos, prohibidos y
/// caracteres sueltos que rompen la forma.
const TROZOS: &[&str] = &[
    "<!ELEMENT ",
    "<!ATTLIST ",
    "<!ENTITY ",
    "<!DOCTYPE ",
    "<!doctype",
    "<!NOTATION ",
    "<![",
    "<!-- c -->",
    "<!--",
    "-->",
    "--",
    "<?xml version='1.0'?>",
    "<?pi ??x?>",
    "<??>",
    "<",
    ">",
    " ",
    "\n",
    "\t",
    "a",
    "año",
    "b1",
    "1",
    "-x",
    ".",
    "SYSTEM",
    "EMPTY",
    "(",
    ")",
    "|",
    ",",
    "?",
    "*",
    "+",
    "#PCDATA",
    "#REQUIRED",
    "#",
    "\"",
    "'",
    "\"v\"",
    "'50%'",
    "&lt;",
    "&#65;",
    "&#x1F600;",
    "&x;",
    "&",
    "&#;",
    "&#0;",
    "%",
    "%p;",
    ";",
    "€",
    "\u{1}",
    "\u{fffe}",
    "\u{feff}",
    "[",
    "]",
    "=",
    "x",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20000))]

    #[test]
    fn el_afd_coincide_con_el_oraculo(trozos in prop::collection::vec(0..TROZOS.len(), 0..24)) {
        let input: String = trozos.iter().map(|&k| TROZOS[k]).collect();
        prop_assert_eq!(lexer(input.as_bytes()), oraculo(&input), "{:?}", input);
    }

    #[test]
    fn declaraciones_validas_coinciden_y_no_fallan(
        cuerpo in prop::collection::vec(prop::sample::select(vec![
            " a", " (b, c)", " (b|c)*", " (#PCDATA)", " EMPTY", " CDATA", " #IMPLIED", " 'v'", " \"&amp;\"",
            " (x|y)", "+", "?",
        ]), 0..8),
        kw in prop::sample::select(vec!["ELEMENT", "ATTLIST"]),
    ) {
        let input = format!("<!{kw} n{}>", cuerpo.concat());
        let esperado = oraculo(&input);
        let real = lexer(input.as_bytes());
        prop_assert_eq!(&real, &esperado, "{:?}", input);
        for b in [1, 7] {
            let (ev, err) = por_bloques(input.as_bytes(), b);
            prop_assert_eq!((ev.into_iter().map(|(e, _)| e).collect::<Vec<_>>(), err), real.clone());
        }
    }
}
