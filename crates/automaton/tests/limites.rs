//! Criterio 5 de F4.6: cada límite de `docs/00-alcance.md` §4 que aplica el
//! ejecutor ([`Limits`]) se prueba en su valor (pasa) y en su valor + 1
//! (`limit`, peso ∞, en el offset del evento que lo pasa), con el mismo
//! reporte en bloques de 1, 7 y 4096 bytes. También la capa de buena
//! formación: claves y atributos repetidos sin bit, fines de línea XML.

use automaton::{build, validate_with, Automaton, ErrorKind, Limits, Mode, Options, Report};
use runtime_rs::Weight;

fn json(src: &str) -> Automaton {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = schema_parser::parse(src.as_bytes()).unwrap();
            build(&semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

fn xml(cuerpo: &str) -> Automaton {
    let src =
        format!("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>{cuerpo}</xs:schema>");
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = xsd::parse(src.as_bytes()).unwrap();
            build(&xsd::lower(&xsd::analyze(&s).unwrap()).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Valida con `limits` en bloques de 1, 7 y 4096 bytes y exige el mismo
/// reporte.
fn con(a: &Automaton, payload: &[u8], limits: Limits) -> Report {
    let o = Options {
        mode: Mode::Total,
        limits,
    };
    let r = validate_with(a, payload, &o, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate_with(a, payload, &o, chunk),
            r,
            "bloques de {chunk}"
        );
    }
    r
}

fn defecto(a: &Automaton, payload: &[u8]) -> Report {
    con(a, payload, Limits::default())
}

#[track_caller]
fn limite_en(r: &Report, offset: u64) {
    assert_eq!(r.error_kind, Some(ErrorKind::Limit), "{r:?}");
    assert_eq!(r.violations, Weight::INFINITY);
    assert!(!r.valid);
    assert_eq!(r.first.unwrap().offset, offset, "{r:?}");
}

#[test]
fn los_valores_por_defecto_son_los_del_alcance() {
    let l = Limits::default();
    assert_eq!(l.max_depth, 256);
    assert_eq!(l.max_string, 1 << 20);
    assert_eq!(l.max_keys, 1024);
    assert_eq!(l.key_pool, 4096);
    assert_eq!(l.max_unique, 4096);
    assert_eq!(l.max_xml_refs, 100_000);
    assert_eq!(l.max_payload, u64::MAX);
    assert_eq!(runtime_rs::LEXEME_MAX, 1024);
}

/// Profundidad: 256 niveles pasan y el `[` 257.º da `limit` (JSON); en XML,
/// el `<` del elemento 257.º. Con otro tope, igual.
#[test]
fn profundidad() {
    let a = json("true");
    let niveles = |d: usize| format!("{}{}", "[".repeat(d), "]".repeat(d));
    assert!(defecto(&a, niveles(256).as_bytes()).valid);
    limite_en(&defecto(&a, niveles(257).as_bytes()), 256);
    let l = Limits {
        max_depth: 3,
        ..Limits::default()
    };
    assert!(con(&a, b"[{\"a\":[]}]", l).valid);
    limite_en(&con(&a, b"[{\"a\":[[]]}]", l), 7);

    let x = xml("<xs:complexType name='T'><xs:sequence>\
         <xs:element name='a' type='T' minOccurs='0'/></xs:sequence></xs:complexType>\
         <xs:element name='a' type='T'/>");
    let niveles = |d: usize| format!("{}{}", "<a>".repeat(d), "</a>".repeat(d));
    assert!(defecto(&x, niveles(256).as_bytes()).valid);
    limite_en(&defecto(&x, niveles(257).as_bytes()), 3 * 256);
    // Un elemento vacío (`<a/>`) no apila: no cuenta.
    let p = format!("{}<a/>{}", "<a>".repeat(256), "</a>".repeat(256));
    assert!(defecto(&x, p.as_bytes()).valid);
}

/// Cadena: `max_string` bytes UTF-8 (tras los escapes) pasan; uno más da
/// `limit` en el carácter que lo pasa. Un escape cuenta lo que da.
#[test]
fn cadena() {
    let a = json("true");
    let l = Limits {
        max_string: 8,
        ..Limits::default()
    };
    assert!(con(&a, b"\"12345678\"", l).valid);
    limite_en(&con(&a, b"\"123456789\"", l), 9);
    // «é» son 2 bytes: 4 caben, la 5.ª no.
    assert!(con(&a, "\"éééé\"".as_bytes(), l).valid);
    limite_en(&con(&a, "\"ééééé\"".as_bytes(), l), 9);
    assert!(con(&a, b"\"\\u0041234567A\"", l).valid);
    // También las claves.
    limite_en(&con(&a, b"{\"123456789\": 1}", l), 10);
    // Con el valor por defecto (1 MiB).
    let mib = 1usize << 20;
    let justa = format!("\"{}\"", "x".repeat(mib));
    assert!(defecto(&a, justa.as_bytes()).valid);
    let larga = format!("\"{}\"", "x".repeat(mib + 1));
    limite_en(&defecto(&a, larga.as_bytes()), mib as u64 + 1);
    // XML: un texto y un valor de atributo.
    let x = xml("<xs:element name='a'><xs:complexType><xs:simpleContent>\
         <xs:extension base='xs:string'><xs:attribute name='b' type='xs:string'/>\
         </xs:extension></xs:simpleContent></xs:complexType></xs:element>");
    assert!(con(&x, b"<a b='12345678'>12345678</a>", l).valid);
    limite_en(&con(&x, b"<a b='123456789'>1</a>", l), 14);
    limite_en(&con(&x, b"<a>123456789</a>", l), 11);
}

/// Lexema de un número: 1 024 bytes caben aunque una comprobación lo lea; el
/// 1 025.º da `limit` al cerrar el número (donde se lee). Sin comprobaciones
/// que lo lean, no hay límite (§21 de `pendientes.md`).
#[test]
fn lexema() {
    let a = json(r#"{"maximum": 10}"#);
    let n = "1".repeat(1024);
    let r = defecto(&a, n.as_bytes());
    assert_eq!(r.violations.get(), Some(1), "{r:?}");
    limite_en(&defecto(&a, "1".repeat(1025).as_bytes()), 0);
    assert!(defecto(&json("true"), "1".repeat(5000).as_bytes()).valid);
}

/// Claves no declaradas: `max_keys` por objeto y `key_pool` entre todos los
/// abiertos; la que no cabe da `limit` en su comilla de cierre. Las
/// declaradas (con bit) no cuentan.
#[test]
fn claves_no_declaradas() {
    let a = json(r#"{"properties": {"d": true}}"#);
    let objeto = |n: usize| {
        let ks: Vec<String> = (0..n).map(|i| format!("\"k{i}\":0")).collect();
        format!("{{{}}}", ks.join(","))
    };
    let l = Limits {
        max_keys: 3,
        key_pool: 5,
        ..Limits::default()
    };
    assert!(con(&a, objeto(3).as_bytes(), l).valid);
    // {"k0":0,"k1":0,"k2":0,"k3":0}: la comilla de cierre de "k3" es la 25.
    limite_en(&con(&a, objeto(4).as_bytes(), l), 25);
    assert!(con(&a, br#"{"k0":0,"d":1,"k1":0,"k2":0}"#, l).valid);
    // La reserva común: 3 en el padre + 2 en el hijo caben; la 3.ª del hijo no.
    let p = br#"{"a":0,"b":0,"c":{"x":0,"y":0}}"#;
    assert!(con(&a, p, l).valid);
    let p = br#"{"a":0,"b":0,"c":{"x":0,"y":0,"z":0}}"#;
    limite_en(&con(&a, p, l), 32);
    // Al cerrar el hijo se libera su región.
    let p = br#"{"a":{"x":0,"y":0,"z":0},"b":{"x":0,"y":0,"z":0},"c":0}"#;
    assert!(con(&a, p, l).valid);
    // Con los valores por defecto: 1 024 por objeto.
    assert!(defecto(&a, objeto(1024).as_bytes()).valid);
    let p = objeto(1025);
    let fin = p.rfind("\":0").unwrap() as u64;
    limite_en(&defecto(&a, p.as_bytes()), fin);
}

/// Atributos XML sin bit: los mismos topes por etiqueta; se liberan al
/// cerrar la etiqueta de apertura.
#[test]
fn atributos_sin_bit() {
    let x = xml("<xs:element name='a'><xs:complexType><xs:sequence>\
         <xs:element name='a' minOccurs='0' maxOccurs='unbounded'><xs:complexType/></xs:element>\
         </xs:sequence></xs:complexType></xs:element>");
    let l = Limits {
        max_keys: 2,
        key_pool: 2,
        ..Limits::default()
    };
    // Dos atributos no declarados: 2 violaciones, sin límite.
    let r = con(&x, b"<a p='1' q='2'><a r='1' s='2'/></a>", l);
    assert_eq!(r.violations.get(), Some(4), "{r:?}");
    limite_en(&con(&x, b"<a p='1' q='2' r='3'></a>", l), 15);
}

/// Referencias XML: `max_xml_refs` por documento, de entidad o de carácter,
/// en texto y en valores; la que pasa da `limit` en su `&`.
#[test]
fn referencias_xml() {
    let x = xml("<xs:element name='a'><xs:complexType><xs:simpleContent>\
         <xs:extension base='xs:string'><xs:attribute name='b' type='xs:string'/>\
         </xs:extension></xs:simpleContent></xs:complexType></xs:element>");
    let l = Limits {
        max_xml_refs: 3,
        ..Limits::default()
    };
    assert!(con(&x, b"<a b='&amp;'>&lt;&#65;</a>", l).valid);
    limite_en(&con(&x, b"<a b='&amp;'>&lt;&#65;&#x42;</a>", l), 22);
    // Los caracteres literales no cuentan.
    assert!(con(&x, "<a b='ñ'>áé<![CDATA[&amp;]]></a>".as_bytes(), l).valid);
}

/// Tamaño del payload: `max_payload` bytes se leen; el primero de más da
/// `limit` en su offset, sin leerlo.
#[test]
fn tamano_del_payload() {
    let a = json("true");
    let p = br#"{"a": [1, 2, 3]}"#;
    let n = p.len() as u64;
    let l = |m| Limits {
        max_payload: m,
        ..Limits::default()
    };
    assert!(con(&a, p, l(n)).valid);
    limite_en(&con(&a, p, l(n - 1)), n - 1);
    limite_en(&con(&a, p, l(0)), 0);
    let x = xml("<xs:element name='a' type='xs:string'/>");
    assert!(con(&x, b"<a>x</a>", l(8)).valid);
    limite_en(&con(&x, b"<a>x</a>", l(7)), 7);
}

/// Memoria: `Tables::state_words` alcanza para los límites dados; con una
/// palabra menos que lo que necesita la pila, `limit`, nunca un pánico.
#[test]
fn memoria_de_los_limites() {
    let a = json("true");
    let l = Limits {
        max_depth: 4,
        key_pool: 0,
        max_unique: 0,
        ..Limits::default()
    };
    let w = a.state_words(&l);
    assert_eq!(w, 4 * runtime_rs::FRAME_OVERHEAD);
    let mut mem = vec![0u64; w];
    let o = Options {
        mode: Mode::Total,
        limits: l,
    };
    let mut v = runtime_rs::JsonValidator::new(a.tables(), &o, &mut mem);
    v.feed(b"[[[[]]]]");
    assert!(v.finish().valid);
    let mut mem = vec![0u64; w - 1];
    let mut v = runtime_rs::JsonValidator::new(a.tables(), &o, &mut mem);
    v.feed(b"[[[[]]]]");
    limite_en(&v.finish(), 3);
}

/// Fines de línea XML (XML 1.0 §2.11 y §3.3.3; `pendientes.md` §35):
/// `\r\n` y `\r` cuentan como un `\n`, y en un valor de atributo `\t \n \r`
/// son espacios; una referencia de carácter (`&#13;`) no se normaliza.
#[test]
fn fines_de_linea_xml() {
    let x = xml("<xs:element name='a'><xs:complexType><xs:simpleContent>\
         <xs:extension base='L'><xs:attribute name='b' type='E'/></xs:extension>\
         </xs:simpleContent></xs:complexType></xs:element>\
         <xs:simpleType name='L'><xs:restriction base='xs:string'>\
         <xs:length value='3'/></xs:restriction></xs:simpleType>\
         <xs:simpleType name='E'><xs:restriction base='xs:string'>\
         <xs:enumeration value='x y'/></xs:restriction></xs:simpleType>");
    for p in [
        "<a>a\r\nb</a>",
        "<a>a\rb</a>",
        "<a>a\nb</a>",
        "<a b='x\ty'>a\r\r</a>",
    ] {
        let r = defecto(&x, p.as_bytes());
        assert!(r.valid, "{p:?}: {r:?}");
    }
    for p in ["<a b='x\r\ny'>abc</a>", "<a b='x\ny'>abc</a>"] {
        assert!(defecto(&x, p.as_bytes()).valid, "{p:?}");
    }
    // `\r\r\n` son dos fines de línea; `&#13;\n`, dos caracteres.
    assert_eq!(defecto(&x, b"<a>a\r\r\nb</a>").violations.get(), Some(1));
    assert_eq!(defecto(&x, b"<a>a&#13;\nb</a>").violations.get(), Some(1));
    assert_eq!(
        defecto(&x, b"<a b='x&#9;y'>abc</a>").violations.get(),
        Some(1)
    );
}
