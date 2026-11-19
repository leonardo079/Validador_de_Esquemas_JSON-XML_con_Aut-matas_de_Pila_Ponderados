//! Casos unitarios de la línea base XML (libxml2, feature `libxml2`;
//! `docs/fase6/linea-base.md` §7). Solo se compila con la feature:
//! `make test-baseline-xml` las corre en Docker, sin red (`--network none`).
//!
//! Todo corre dentro de `with_stack` (D7: `XmlBaseline` no es `Send` y se
//! compila en el hilo del testigo), y ninguna prueba mide tiempo (D1). El
//! estado global de libxml2 lo serializa la propia línea base (B8).
#![cfg(feature = "libxml2")]

use baseline::{
    with_stack, CompileError, Kind, Limits, Malformed, Outcome, XmlBaseline, MAX_MESSAGE_BYTES,
};

/// El XSD de `valid-xml-pedido-01`.
const PEDIDO: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="pedido">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="cliente" type="xs:string"/>
        <xs:element name="nota" type="xs:string" minOccurs="0"/>
        <xs:element name="total" type="xs:decimal"/>
      </xs:sequence>
      <xs:attribute name="id" type="xs:positiveInteger" use="required"/>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// Un elemento `a` que puede contener otro `a` (anidamiento sin tope).
const RECURSIVO: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="a">
    <xs:complexType>
      <xs:sequence>
        <xs:element ref="a" minOccurs="0"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// Una lista de `n` enteros.
const LISTA: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="l">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="n" type="xs:integer" minOccurs="0" maxOccurs="unbounded"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;

/// Un archivo local que no debe aparecer nunca en la salida.
const SECRETO: &str = "/etc/passwd";

fn validar_con(xsd: &str, limits: Limits, payload: &[u8]) -> Outcome {
    with_stack(|s| {
        XmlBaseline::compile(s, xsd.as_bytes())
            .unwrap_or_else(|e| panic!("{xsd}: {e}"))
            .with_limits(limits)
            .validate(s, payload)
    })
}

fn validar(xsd: &str, payload: &[u8]) -> Outcome {
    validar_con(xsd, Limits::default(), payload)
}

fn compilar(xsd: &str) -> Result<(), CompileError> {
    with_stack(|s| XmlBaseline::compile(s, xsd.as_bytes()).map(drop))
}

/// La posición de un resultado negativo de lectura.
fn lectura(o: &Outcome, kind: Kind) -> Malformed {
    assert!(!o.valid, "{o:?}");
    assert_eq!(o.kind, Some(kind), "{o:?}");
    assert!(o.violations.is_empty(), "{o:?}");
    o.malformed.clone().unwrap_or_else(|| panic!("{o:?}"))
}

/// Ningún byte del archivo secreto (su primera línea en Debian) ni su ruta
/// abierta aparecen en el resultado.
fn sin_secreto(texto: &str) {
    assert!(!texto.contains("root:"), "se filtró {SECRETO}: {texto}");
}

fn anidado(n: usize) -> Vec<u8> {
    let mut s = "<a>".repeat(n);
    s.push_str(&"</a>".repeat(n));
    s.into_bytes()
}

const PEDIDO_OK: &str = r#"<pedido id="7"><cliente>Ana</cliente><total>1.5</total></pedido>"#;

// --- Configuración y resultados básicos ---

#[test]
fn pedido_valido() {
    let o = validar(PEDIDO, PEDIDO_OK.as_bytes());
    assert!(o.valid, "{o:?}");
    assert_eq!(
        o,
        Outcome {
            valid: true,
            kind: None,
            violations: vec![],
            truncated: false,
            malformed: None
        }
    );
}

#[test]
fn las_etapas_se_llaman_por_separado() {
    let (o, malo) = with_stack(|s| {
        let base = XmlBaseline::compile(s, PEDIDO.as_bytes()).unwrap();
        let dom = base.parse(s, PEDIDO_OK.as_bytes()).unwrap();
        // El mismo árbol se valida dos veces y se suelta tras el esquema.
        let o = base.validate_dom(s, &dom);
        assert_eq!(base.validate_dom(s, &dom), o);
        drop(base);
        let malo = base_y_dom_malo(s);
        drop(dom);
        (o, malo)
    });
    assert!(o.valid, "{o:?}");
    assert_eq!(malo.kind, Some(Kind::Schema), "{malo:?}");
}

/// Un árbol leído con un esquema y validado con otro.
fn base_y_dom_malo(s: &baseline::Stack) -> Outcome {
    let otro = XmlBaseline::compile(s, LISTA.as_bytes()).unwrap();
    let dom = otro.parse(s, PEDIDO_OK.as_bytes()).unwrap();
    otro.validate_dom(s, &dom)
}

#[test]
fn un_mismo_xsd_valida_muchos_payloads() {
    // El registro de errores del contexto se vacía tras cada validación.
    let r: Vec<usize> = with_stack(|s| {
        let base = XmlBaseline::compile(s, PEDIDO.as_bytes()).unwrap();
        let malo = br#"<pedido><cliente>x</cliente><total>1</total></pedido>"#;
        (0..3)
            .flat_map(|_| {
                [
                    base.validate(s, malo).violation_count(),
                    base.validate(s, PEDIDO_OK.as_bytes()).violation_count(),
                ]
            })
            .collect()
    });
    assert_eq!(r, [1, 0, 1, 0, 1, 0]);
}

// --- Malformados ---

#[test]
fn etiqueta_sin_cerrar_es_malformado_con_linea_columna_y_offset() {
    let p = b"<pedido id=\"7\">\n  <cliente>Ana</pedido>";
    let m = lectura(&validar(PEDIDO, p), Kind::Malformed);
    assert!(m.message.contains("mismatch"), "{m:?}");
    // libxml2 lo detecta tras leer el nombre de `</pedido`: línea 2.
    assert_eq!(m.line, 2, "{m:?}");
    let off = m.offset.unwrap() as usize;
    assert!((18..=p.len()).contains(&off), "{m:?}");
    assert_eq!(m.column as usize, off - 16 + 1, "{m:?}");
}

#[test]
fn fin_de_entrada_prematuro_es_malformado_en_la_longitud() {
    let p = b"<pedido id=\"7\"><cliente>Ana";
    let m = lectura(&validar(PEDIDO, p), Kind::Malformed);
    assert_eq!(m.offset, Some(p.len() as u64), "{m:?}");
    assert_eq!((m.line, m.column), (1, p.len() as u32 + 1), "{m:?}");
}

#[test]
fn basura_despues_de_la_raiz_es_malformado() {
    let m = lectura(&validar(PEDIDO, b"<pedido id=\"7\"/> x"), Kind::Malformed);
    assert!(m.offset.is_some(), "{m:?}");
}

#[test]
fn columna_en_caracteres_y_offset_en_bytes() {
    // «Ñandú» ocupa 7 bytes y 5 caracteres; el error está en el `<` de `</b>`.
    let p = "<a>Ñandú</b>".as_bytes();
    let m = lectura(&validar(PEDIDO, p), Kind::Malformed);
    let off = m.offset.unwrap() as usize;
    let caracteres = std::str::from_utf8(&p[..off]).unwrap().chars().count();
    assert_eq!(m.column as usize, caracteres + 1, "{m:?}");
}

#[test]
fn prefijo_sin_declarar_es_malformado() {
    // libxml2 devuelve el árbol con un error de espacios de nombres.
    let m = lectura(&validar(PEDIDO, b"<p:pedido id=\"7\"/>"), Kind::Malformed);
    assert!(
        m.message.contains("Namespace") || m.message.contains("namespace"),
        "{m:?}"
    );
}

#[test]
fn entidad_no_predefinida_es_malformado() {
    let m = lectura(
        &validar(PEDIDO, b"<pedido id=\"7\">&lol;</pedido>"),
        Kind::Malformed,
    );
    assert!(m.message.contains("lol"), "{m:?}");
}

#[test]
fn las_cinco_entidades_predefinidas_y_las_de_caracter_se_aceptan() {
    let p = r#"<pedido id="7"><cliente>&lt;&gt;&amp;&quot;&apos;&#65;&#x42;</cliente><total>1</total></pedido>"#;
    assert!(validar(PEDIDO, p.as_bytes()).valid);
}

// --- Política D8 (U6): DTD y entidades ---

#[test]
fn entidad_externa_es_malformado_sin_leer_el_archivo() {
    let p = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE pedido [<!ENTITY x SYSTEM \"file://{SECRETO}\">]>\n\
         <pedido id=\"7\"><cliente>&x;</cliente><total>1</total></pedido>"
    );
    let o = validar(PEDIDO, p.as_bytes());
    sin_secreto(&format!("{o:?}"));
    let m = lectura(&o, Kind::Malformed);
    assert_eq!(m.offset, Some(24), "{m:?}");
    assert_eq!((m.line, m.column), (2, 3), "{m:?}");
}

#[test]
fn dtd_externa_y_parametros_son_malformado() {
    for p in [
        format!("<!DOCTYPE pedido SYSTEM \"file://{SECRETO}\"><pedido id=\"7\"/>"),
        format!("<!DOCTYPE pedido [<!ENTITY % e SYSTEM \"file://{SECRETO}\"> %e;]><pedido/>"),
        "<!DOCTYPE pedido PUBLIC \"-//x//y\" \"http://127.0.0.1:9/x.dtd\"><pedido/>".to_owned(),
    ] {
        let o = validar(PEDIDO, p.as_bytes());
        sin_secreto(&format!("{o:?}"));
        assert_eq!(lectura(&o, Kind::Malformed).offset, Some(2), "{p}");
    }
}

#[test]
fn doctype_tras_bom_comentario_e_instruccion_y_en_minusculas() {
    let p = b"\xEF\xBB\xBF<?xml version=\"1.0\"?><!-- c --><?pi x?>\n <!doctype pedido><pedido/>";
    let m = lectura(&validar(PEDIDO, p), Kind::Malformed);
    let d = p.windows(3).position(|w| w == b"<!d").unwrap() + 2;
    assert_eq!(m.offset, Some(d as u64), "{m:?}");
    assert_eq!((m.line, m.column), (2, 4), "{m:?}");
}

#[test]
fn un_doctype_dentro_de_un_comentario_no_cuenta() {
    let p = br#"<!-- <!DOCTYPE x> --><pedido id="7"><cliente>a</cliente><total>1</total></pedido>"#;
    assert!(validar(PEDIDO, p).valid);
}

#[test]
fn xsi_schema_location_del_payload_se_ignora() {
    let p = format!(
        "<pedido xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xsi:noNamespaceSchemaLocation=\"file://{SECRETO}\" id=\"7\">\
         <cliente>a</cliente><total>1</total></pedido>"
    );
    let o = validar(PEDIDO, p.as_bytes());
    sin_secreto(&format!("{o:?}"));
    assert!(o.valid, "{o:?}");
}

// --- Validación contra el XSD ---

#[test]
fn falta_el_atributo_requerido_es_una_violacion() {
    let o = validar(
        PEDIDO,
        br#"<pedido><cliente>Ana</cliente><total>1</total></pedido>"#,
    );
    assert_eq!(o.kind, Some(Kind::Schema), "{o:?}");
    assert_eq!(o.violation_count(), 1, "{o:?}");
    let v = &o.violations[0];
    assert!(v.message.starts_with("línea 1: "), "{v:?}");
    assert!(v.message.contains("'id'"), "{v:?}");
    assert!(v.keyword.starts_with("libxml2:"), "{v:?}");
    assert_eq!(v.pointer, "", "libxml2 no da la ruta: {v:?}");
    assert!(o.malformed.is_none());
}

#[test]
fn total_no_decimal_es_una_violacion() {
    let o = validar(
        PEDIDO,
        br#"<pedido id="7"><cliente>Ana</cliente><total>abc</total></pedido>"#,
    );
    assert_eq!(o.kind, Some(Kind::Schema), "{o:?}");
    assert_eq!(o.violation_count(), 1, "{o:?}");
    assert!(o.violations[0].message.contains("xs:decimal"), "{o:?}");
}

#[test]
fn elemento_inesperado_y_raiz_desconocida() {
    let o = validar(
        PEDIDO,
        br#"<pedido id="7"><cliente>a</cliente><extra/><total>1</total></pedido>"#,
    );
    assert_eq!(
        (o.kind, o.violation_count()),
        (Some(Kind::Schema), 1),
        "{o:?}"
    );
    let o = validar(PEDIDO, b"<otro/>");
    assert_eq!(
        (o.kind, o.violation_count()),
        (Some(Kind::Schema), 1),
        "{o:?}"
    );
}

#[test]
fn la_lista_de_violaciones_se_corta_en_max_violations() {
    let mut p = String::from("<l>");
    for _ in 0..1000 {
        p.push_str("<n>x</n>");
    }
    p.push_str("</l>");
    let todas = validar(LISTA, p.as_bytes());
    assert_eq!(todas.violation_count(), 100, "{:?}", todas.kind);
    assert!(todas.truncated);
    let limits = Limits {
        max_violations: 2000,
        ..Limits::default()
    };
    let o = validar_con(LISTA, limits, p.as_bytes());
    assert_eq!((o.violation_count(), o.truncated), (1000, false));
}

#[test]
fn el_mensaje_de_una_violacion_tiene_tope() {
    // libxml2 ya acorta el valor que copia en el mensaje: 200 KB de `x`
    // dan un mensaje corto.
    let p = format!("<l><n>{}</n></l>", "x".repeat(200_000));
    let o = validar(LISTA, p.as_bytes());
    assert_eq!(o.kind, Some(Kind::Schema), "{:?}", o.kind);
    assert!(o.violations[0].message.len() <= MAX_MESSAGE_BYTES);
    // Pero copia entera la lista de una enumeración: con 2 000 valores, el
    // mensaje pasaría de 20 KB y se recorta a MAX_MESSAGE_BYTES.
    let valores: String = (0..2000)
        .map(|i| format!("<xs:enumeration value=\"valor{i:05}\"/>"))
        .collect();
    let xsd = format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="e">
        <xs:simpleType><xs:restriction base="xs:string">{valores}</xs:restriction></xs:simpleType>
        </xs:element></xs:schema>"#
    );
    let o = validar(&xsd, b"<e>otro</e>");
    assert_eq!(o.violation_count(), 1, "{:?}", o.kind);
    let m = &o.violations[0].message;
    assert!(
        m.len() <= MAX_MESSAGE_BYTES && m.ends_with('…'),
        "{}: {m}",
        m.len()
    );
}

// --- Profundidad (D7, `00-alcance.md` §4) ---

#[test]
fn profundidad_256_es_valida() {
    let o = validar(RECURSIVO, &anidado(256));
    assert!(o.valid, "{o:?}");
}

#[test]
fn profundidad_257_es_limite_en_el_menor_que_que_la_pasa() {
    let o = validar(RECURSIVO, &anidado(257));
    let m = lectura(&o, Kind::Limit);
    assert_eq!(m.offset, Some(256 * 3), "{m:?}");
    assert_eq!((m.line, m.column), (1, 256 * 3 + 1), "{m:?}");
}

#[test]
fn anidamiento_de_300_y_de_cien_mil_niveles_es_limite_sin_caida() {
    for n in [300, 100_000] {
        let m = lectura(&validar(RECURSIVO, &anidado(n)), Kind::Limit);
        assert_eq!(m.offset, Some(256 * 3), "{n}: {m:?}");
    }
}

#[test]
fn tope_de_profundidad_configurable_a_la_baja() {
    let limits = Limits {
        max_depth: 3,
        ..Limits::default()
    };
    assert!(validar_con(RECURSIVO, limits, &anidado(3)).valid);
    // El recorrido propio salta comentarios, CDATA, instrucciones y `>`
    // entre comillas, y un elemento vacío no queda abierto: el cuarto nivel
    // es el `<a>` de dentro del último.
    let p = br#"<a><!-- <a> --><a x='>'><a/><![CDATA[<a>]]><?p <a>?><a><a></a></a></a></a>"#;
    let m = lectura(&validar_con(RECURSIVO, limits, p), Kind::Limit);
    let cuarto = p.windows(10).position(|w| w == b"<a><a></a>").unwrap() + 3;
    assert_eq!(m.offset, Some(cuarto as u64), "{m:?}");
    // Un tope mayor que 256 se recorta.
    let limits = Limits {
        max_depth: 10_000,
        ..Limits::default()
    };
    assert_eq!(
        lectura(&validar_con(RECURSIVO, limits, &anidado(257)), Kind::Limit).offset,
        Some(768)
    );
}

// --- Posiciones con BOM y `encoding` ---

#[test]
fn bom_inicial_se_ignora_y_los_offsets_cuentan_sus_3_bytes() {
    let mut p = b"\xEF\xBB\xBF".to_vec();
    p.extend_from_slice(PEDIDO_OK.as_bytes());
    assert!(validar(PEDIDO, &p).valid);
    let p = b"\xEF\xBB\xBF<a></b>";
    let m = lectura(&validar(PEDIDO, p), Kind::Malformed);
    let off = m.offset.unwrap();
    assert!((6..=p.len() as u64).contains(&off), "{m:?}");
    assert_eq!(u64::from(m.column), off - 3 + 1, "{m:?}");
}

/// `s` en UTF-16LE, con BOM si `bom`.
fn utf16le(s: &str, bom: bool) -> Vec<u8> {
    let mut v = if bom { vec![0xFF, 0xFE] } else { vec![] };
    v.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
    v
}

/// `s` en UTF-16BE con BOM.
fn utf16be(s: &str) -> Vec<u8> {
    let mut v = vec![0xFE, 0xFF];
    v.extend(s.encode_utf16().flat_map(u16::to_be_bytes));
    v
}

/// `s` en UTF-32LE con BOM.
fn utf32le(s: &str) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFE, 0x00, 0x00];
    v.extend(s.chars().flat_map(|c| u32::from(c).to_le_bytes()));
    v
}

/// Las entradas que no son UTF-8 (`00-alcance.md` §1.2; CR-I1, SEC-M1, O1):
/// `(descripción, bytes, offset esperado)`. Las que empiezan como UTF-16,
/// UTF-32 o EBCDIC dan el offset 0; las que declaran otro `encoding`, el
/// offset del valor.
fn entradas_no_utf8(xml: &str) -> Vec<(&'static str, Vec<u8>, u64)> {
    let decl = |enc: &str| format!("<?xml version=\"1.0\" encoding=\"{enc}\"?>{xml}");
    vec![
        ("UTF-16LE con BOM", utf16le(xml, true), 0),
        ("UTF-16LE sin BOM", utf16le(xml, false), 0),
        ("UTF-16BE con BOM", utf16be(xml), 0),
        ("UTF-32LE con BOM", utf32le(xml), 0),
        ("EBCDIC", b"\x4C\x6F\xA7\x94\x93".to_vec(), 0),
        // UTF-7: `<!DOCTYPE` escrito como `+ADwAIQ-DOCTYPE`.
        ("UTF-7 declarado", decl("UTF-7").into_bytes(), 30),
        ("ISO-8859-1 declarado", decl("ISO-8859-1").into_bytes(), 30),
        ("utf-16 declarado en ASCII", decl("utf-16").into_bytes(), 30),
    ]
}

#[test]
fn entradas_que_no_son_utf8_son_malformado() {
    // `<a>` + 300 `<b/>` + `</a>`: en UTF-16 el recorrido de bytes contaba
    // cada etiqueta como abierta y daba un `Limit` falso (CR-I1).
    let muchas = format!("<l>{}</l>", "<n>1</n>".repeat(300));
    for (que, p, off) in entradas_no_utf8(&muchas) {
        let o = validar(LISTA, &p);
        let m = lectura(&o, Kind::Malformed);
        assert_eq!(m.offset, Some(off), "{que}: {m:?}");
        assert!(m.message.contains("UTF-8"), "{que}: {m:?}");
    }
    // Lo mismo con una DTD dentro: no se llega a mirar.
    let dtd = format!("<!DOCTYPE l [<!ENTITY x SYSTEM \"file://{SECRETO}\">]><l>&x;</l>");
    for (que, p, off) in entradas_no_utf8(&dtd) {
        let o = validar(LISTA, &p);
        sin_secreto(&format!("{o:?}"));
        assert_eq!(lectura(&o, Kind::Malformed).offset, Some(off), "{que}");
    }
}

#[test]
fn utf8_declarado_en_cualquier_caja_se_acepta() {
    for enc in ["UTF-8", "utf-8", "Utf-8"] {
        let p = format!("<?xml version=\"1.0\" encoding=\"{enc}\"?>\n{PEDIDO_OK}");
        assert!(validar(PEDIDO, p.as_bytes()).valid, "{enc}");
    }
    // Sin declaración, «Ñandú» en UTF-8.
    let p = r#"<pedido id="7"><cliente>Ñandú</cliente><total>1</total></pedido>"#;
    assert!(validar(PEDIDO, p.as_bytes()).valid);
}

/// Bytes de Latin-1 sin declarar: libxml2 los lee como UTF-8 inválido.
#[test]
fn latin1_sin_declarar_es_malformado() {
    let mut p = b"<pedido id=\"7\"><cliente>".to_vec();
    p.push(0xD1);
    p.extend_from_slice(b"and\xFA</cliente><total>1</total></pedido>");
    let m = lectura(&validar(PEDIDO, &p), Kind::Malformed);
    assert!(m.offset.is_some(), "{m:?}");
}

#[test]
fn xsd_que_no_es_utf8_no_compila() {
    for (que, xsd, _) in entradas_no_utf8(PEDIDO) {
        let e = with_stack(|s| XmlBaseline::compile(s, &xsd).map(drop)).unwrap_err();
        assert!(e.message.contains("UTF-8"), "{que}: {e}");
    }
    let dtd = format!("<!DOCTYPE xs:schema [<!ENTITY x SYSTEM \"file://{SECRETO}\">]>{PEDIDO}");
    for (que, xsd, _) in entradas_no_utf8(&dtd) {
        let e = with_stack(|s| XmlBaseline::compile(s, &xsd).map(drop)).unwrap_err();
        sin_secreto(&e.message);
        assert!(e.message.contains("UTF-8"), "{que}: {e}");
    }
}

// --- Compilación del XSD ---

#[test]
fn xs_import_e_include_no_compilan_y_no_usan_la_red_ni_el_disco() {
    let xsd = |dentro: &str| {
        format!(
            "<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">{dentro}\
             <xs:element name=\"a\" type=\"xs:string\"/></xs:schema>"
        )
    };
    for (dentro, nombre) in [
        (
            r#"<xs:import namespace="urn:x" schemaLocation="http://127.0.0.1:9/x.xsd"/>"#
                .to_owned(),
            "import",
        ),
        (
            format!(r#"<xs:include schemaLocation="file://{SECRETO}"/>"#),
            "include",
        ),
        (
            r#"<xs:redefine schemaLocation="x.xsd"/>"#.to_owned(),
            "redefine",
        ),
        (
            format!(r#"<xs:override schemaLocation="file://{SECRETO}"/>"#),
            "override",
        ),
        // Dentro de `xs:appinfo` también (más estricto que libxml2).
        (
            r#"<xs:annotation><xs:appinfo><xs:import namespace="urn:x"/></xs:appinfo></xs:annotation>"#
                .to_owned(),
            "import",
        ),
    ] {
        let e = compilar(&xsd(&dentro)).unwrap_err();
        sin_secreto(&e.message);
        assert!(e.message.contains(&format!("xs:{nombre}")), "{e}");
    }
    // Con otro prefijo también.
    let e = compilar(
        r#"<s:schema xmlns:s="http://www.w3.org/2001/XMLSchema"><s:annotation/><s:import namespace="urn:x"/></s:schema>"#,
    )
    .unwrap_err();
    assert!(e.message.contains("xs:import"), "{e}");
}

#[test]
fn xsd_con_doctype_no_compila() {
    let xsd = format!(
        "<!DOCTYPE xs:schema [<!ENTITY x SYSTEM \"file://{SECRETO}\">]>\n{}",
        PEDIDO.replace("type=\"xs:string\"/>", "type=\"xs:string\"/><!-- &x; -->")
    );
    let e = compilar(&xsd).unwrap_err();
    sin_secreto(&e.message);
    assert!(e.message.contains("D8"), "{e}");
}

#[test]
fn xsd_mal_formado_o_invalido_no_compila() {
    let e = compilar("<xs:schema").unwrap_err();
    assert!(e.message.contains("bien formado"), "{e}");
    let e = compilar(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="a" type="xs:nope"/></xs:schema>"#,
    )
    .unwrap_err();
    assert!(e.message.contains("nope"), "{e}");
    let e = compilar("<raiz/>").unwrap_err();
    assert!(!e.message.is_empty(), "{e}");
}

#[test]
fn xsd_con_construcciones_fuera_del_subconjunto_compila() {
    // libxml2 implementa XSD 1.0 completo (§7.5): `xs:any`, `mixed`,
    // `xs:list`… compilan, aunque el proyecto los rechace (`00-alcance.md` §3.3).
    let xsd = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
      <xs:element name="a"><xs:complexType mixed="true"><xs:sequence>
        <xs:any processContents="skip" minOccurs="0"/>
      </xs:sequence></xs:complexType></xs:element></xs:schema>"#;
    assert!(validar(xsd, b"<a>texto<b/></a>").valid);
}

#[test]
fn el_mensaje_de_compile_error_tiene_tope() {
    let xsd = format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="a" type="xs:{}"/></xs:schema>"#,
        "n".repeat(100_000)
    );
    let e = compilar(&xsd).unwrap_err();
    assert!(e.message.len() <= MAX_MESSAGE_BYTES, "{}", e.message.len());
}

#[test]
fn xsd_sin_prefijo_compila_y_su_import_se_rechaza() {
    let sin_prefijo = r#"<schema xmlns="http://www.w3.org/2001/XMLSchema">
      <element name="a" type="string"/></schema>"#;
    assert!(validar(sin_prefijo, b"<a>x</a>").valid);
    let e = compilar(
        r#"<schema xmlns="http://www.w3.org/2001/XMLSchema"><import namespace="urn:x"/></schema>"#,
    )
    .unwrap_err();
    assert!(e.message.contains("xs:import"), "{e}");
}

#[test]
fn xsd_demasiado_profundo_es_un_limite_no_un_mal_formado() {
    // CR-S7: 300 `xs:annotation` anidadas pasan de 256 niveles.
    let mut xsd = String::from(r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">"#);
    xsd.push_str(&"<xs:annotation>".repeat(300));
    xsd.push_str(&"</xs:annotation>".repeat(300));
    xsd.push_str("</xs:schema>");
    let e = compilar(&xsd).unwrap_err();
    assert!(e.message.starts_with("el XSD supera un límite"), "{e}");
}

// --- Malformados variados (CR-S10, O3) ---

#[test]
fn malformados_variados_tienen_posicion() {
    let casos: [(&str, &[u8]); 10] = [
        ("vacío", b""),
        ("solo espacios", b"  \n "),
        ("UTF-8 inválido en el texto", b"<a>\xFF</a>"),
        ("UTF-8 inválido en un nombre", b"<a\xC3>x</a\xC3>"),
        ("UTF-8 inválido en un atributo", b"<a x=\"\xE0\x80\"/>"),
        ("&#0;", b"<a>&#0;</a>"),
        ("&#xD800;", b"<a>&#xD800;</a>"),
        ("atributo duplicado", b"<a x=\"1\" x=\"2\"/>"),
        ("BOM incompleto EF BB", b"\xEF\xBB<a/>"),
        ("BOM incompleto EF", b"\xEF<a/>"),
    ];
    for (que, p) in casos {
        let m = lectura(&validar(PEDIDO, p), Kind::Malformed);
        let off = m.offset.unwrap_or_else(|| panic!("{que}: {m:?}"));
        assert!(off <= p.len() as u64, "{que}: {m:?}");
        assert!(m.line >= 1 && m.column >= 1, "{que}: {m:?}");
    }
}

/// SEC-L1: los avisos de libxml2 no tapan un error posterior. Con
/// `no_warning`, libxml2 no entrega los avisos (aquí, 150 espacios de nombres
/// con URI relativa), así que el tope de 100 diagnósticos del crate lo llenan
/// solo errores, y el primero se ve siempre.
#[test]
fn muchos_avisos_no_ocultan_un_error_de_espacios_de_nombres() {
    let decl: String = (0..150)
        .map(|i| format!(" xmlns:p{i}=\"rel{i}\""))
        .collect();
    let valido = format!("<a{decl}/>");
    let o = validar(RECURSIVO, valido.as_bytes());
    assert!(o.valid, "{o:?}");
    let malo = format!("<a{decl}><q:a/></a>");
    let m = lectura(&validar(RECURSIVO, malo.as_bytes()), Kind::Malformed);
    assert!(m.message.contains("amespace"), "{m:?}");
}

// --- Límites propios de libxml2 sin `huge` (CR-I2) ---

#[test]
fn comentario_cdata_e_instruccion_de_mas_de_10_mb_son_limite() {
    let grande = "x".repeat(10_000_001);
    for p in [
        format!("<a><!--{grande}--></a>"),
        format!("<a><![CDATA[{grande}]]></a>"),
        format!("<a><?p {grande}?></a>"),
    ] {
        let m = lectura(&validar(RECURSIVO, p.as_bytes()), Kind::Limit);
        assert!(m.message.contains("too big"), "{}", m.message);
    }
}

#[test]
fn atributo_de_mas_de_10_mb_y_nombre_de_mas_de_50_000_bytes_son_limite() {
    let p = format!("<a x=\"{}\"/>", "x".repeat(10_000_001));
    let m = lectura(&validar(RECURSIVO, p.as_bytes()), Kind::Limit);
    assert!(m.message.contains("too long"), "{}", m.message);
    let p = format!("<{}/>", "a".repeat(50_001));
    let m = lectura(&validar(RECURSIVO, p.as_bytes()), Kind::Limit);
    assert!(m.message.contains("too long"), "{}", m.message);
}

/// Un nodo de texto de más de 10 MB, seguido o en líneas de 80 bytes, es
/// un límite en libxml2 2.9.14: «Huge input lookup» al pasar de
/// `XML_MAX_LOOKUP_LIMIT` (10 000 000), no «huge text node».
#[test]
fn texto_de_mas_de_10_mb_es_limite() {
    let seguido = "x".repeat(10_000_001);
    let lineas = format!("{}\n", "x".repeat(79)).repeat(125_001);
    for t in [seguido, lineas] {
        let p = format!("<pedido id=\"7\"><cliente>{t}</cliente><total>1</total></pedido>");
        let m = lectura(&validar(PEDIDO, p.as_bytes()), Kind::Limit);
        assert!(m.message.contains("Huge input lookup"), "{}", m.message);
    }
}
