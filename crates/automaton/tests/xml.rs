//! F4.1c: M sobre una IR de XSD (`docs/fase4/automata.md` §10-§15): el
//! documento, los marcos de elemento con despacho por nombre local, los
//! atributos, los textos con tipos simples y el contenido vacío o de solo
//! elementos. Los modelos con contadores y `xs:all` están en
//! `tests/glushkov.rs`. Todo payload se valida en bloques de 1, 7 y 4096
//! bytes con el mismo reporte.

use automaton::{build, validate, Automaton, ErrorKind, Mode, Report};
use ir::{RuleKind, VerifiedIr};
use runtime_rs::Weight;

#[path = "comun/presupuesto.rs"]
mod presupuesto;

/// La IR de un XSD (hilo de 8 MiB, D7).
fn xsd(src: &str) -> VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = xsd::parse(src.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
            let a = xsd::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"));
            xsd::lower(&a).unwrap_or_else(|e| panic!("{e}"))
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Un esquema con los elementos globales `cuerpo`.
fn esquema(cuerpo: &str) -> Automaton {
    let src =
        format!("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>{cuerpo}</xs:schema>");
    let ir = xsd(&src);
    presupuesto::cubre(&ir);
    build(&ir).unwrap_or_else(|e| panic!("{e}"))
}

fn correr(a: &Automaton, payload: &str) -> Report {
    let r = validate(a, payload.as_bytes(), Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate(a, payload.as_bytes(), Mode::Total, chunk),
            r,
            "bloques de {chunk}: {payload}"
        );
    }
    r
}

fn violaciones(a: &Automaton, payload: &str) -> Option<u64> {
    correr(a, payload).violations.get()
}

fn sintaxis(a: &Automaton, payload: &str) {
    let r = correr(a, payload);
    assert_eq!(r.violations, Weight::INFINITY, "{payload}: {r:?}");
    assert_eq!(r.error_kind, Some(ErrorKind::Syntax), "{payload}: {r:?}");
}

const PEDIDO: &str = r#"
  <xs:element name="pedido">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="cliente" type="xs:string"/>
        <xs:element name="nota" type="xs:string" minOccurs="0"/>
        <xs:element name="total" type="xs:decimal"/>
      </xs:sequence>
      <xs:attribute name="id" type="xs:positiveInteger" use="required"/>
    </xs:complexType>
  </xs:element>"#;

#[test]
fn pedido_valido_e_invalidos() {
    let a = esquema(PEDIDO);
    let ok = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- pedido -->\n<pedido id=\"7\">\n  <cliente>Ñandú &amp; Cía</cliente>\n  <nota><![CDATA[entregar <antes> de las 5]]></nota>\n  <total>105.50</total>\n</pedido>";
    let r = correr(&a, ok);
    assert!(r.valid, "{r:?}");
    // Sin la nota opcional, con prefijo y espacios en el decimal.
    assert_eq!(
        violaciones(
            &a,
            "<p:pedido xmlns:p='urn:x' id=' 7 '><cliente/><total> +1.5 </total></p:pedido>"
        ),
        Some(0)
    );
    // Falta el atributo requerido: 1, en el `<` de la etiqueta.
    let r = correr(&a, "<pedido><cliente/><total>1</total></pedido>");
    assert_eq!(r.violations.get(), Some(1));
    assert_eq!(r.first.unwrap().offset, 0);
    let (_, kind) = a.rule(r.first.unwrap().rule.unwrap());
    assert_eq!(kind, RuleKind::Attributes);
    // `id` fuera de rango y `total` no decimal: 2.
    assert_eq!(
        violaciones(&a, "<pedido id='0'><cliente/><total>x</total></pedido>"),
        Some(2)
    );
    // Falta `total`: el contenido incompleto, en `</pedido>`.
    let r = correr(&a, "<pedido id='1'><cliente/></pedido>");
    assert_eq!(r.violations.get(), Some(1));
    assert_eq!(r.first.unwrap().offset, 25);
    // Orden cambiado: `total` antes de `cliente` es inesperado (se salta),
    // después falta `total`.
    assert_eq!(
        violaciones(&a, "<pedido id='1'><total>1</total><cliente/></pedido>"),
        Some(2)
    );
    // Un atributo no declarado y un elemento raíz desconocido.
    assert_eq!(
        violaciones(
            &a,
            "<pedido id='1' x='2'><cliente/><total>1</total></pedido>"
        ),
        Some(1)
    );
    assert_eq!(violaciones(&a, "<otro><a/></otro>"), Some(1));
}

#[test]
fn buena_formacion_del_documento() {
    let a = esquema(PEDIDO);
    for p in [
        "",
        "<pedido id='1'><cliente/><total>1</total></pedido><pedido/>",
        "texto<pedido id='1'><cliente/><total>1</total></pedido>",
        "<pedido id='1'><cliente/><total>1</total></pedid>",
        "<pedido id='1'><cliente></total></pedido>",
        "<pedido id='1' id='2'><cliente/><total>1</total></pedido>",
        " <?xml version='1.0'?><pedido id='1'><cliente/><total>1</total></pedido>",
        "<?xml version='1.0' encoding='latin1'?><pedido id='1'><cliente/><total>1</total></pedido>",
        "<?xml?><pedido id='1'><cliente/><total>1</total></pedido>",
        "<pedido id='1'><?xml version='1.0'?><cliente/><total>1</total></pedido>",
    ] {
        sintaxis(&a, p);
    }
    for p in [
        "<?xml version='1.0'?>\n<pedido id='1'><cliente/><total>1</total></pedido>\n",
        "<?xml version=\"1.1\" encoding='utf-8' standalone=\"yes\" ?><pedido id='1'><cliente/><total>1</total></pedido>",
        "<?xml-stylesheet href='a'?><pedido id='1'><?pi x?><cliente>a<?p?>b</cliente><total>1<?q?>2</total></pedido><?fin?>",
        "<pedido id='1'><cliente/><total>1</total></pedido   >",
    ] {
        assert_eq!(violaciones(&a, p), Some(0), "{p}");
    }
}

#[test]
fn contenido_vacio_sin_espacios() {
    let a = esquema("<xs:element name='e'><xs:complexType/></xs:element>");
    assert_eq!(violaciones(&a, "<e/>"), Some(0));
    assert_eq!(violaciones(&a, "<e></e>"), Some(0));
    assert_eq!(violaciones(&a, "<e><!-- c --><?p?></e>"), Some(0));
    assert_eq!(violaciones(&a, "<e> </e>"), Some(1));
    assert_eq!(violaciones(&a, "<e><x/></e>"), Some(1));
    assert_eq!(violaciones(&a, "<e a='1'/>"), Some(1));
}

#[test]
fn solo_elementos_ignora_los_espacios() {
    let a = esquema(
        "<xs:element name='r'><xs:complexType><xs:sequence>\
         <xs:element name='a' type='xs:string' maxOccurs='unbounded'/>\
         </xs:sequence></xs:complexType></xs:element>",
    );
    assert_eq!(violaciones(&a, "<r>\n  <a>x</a>\n\t<a/>\r\n</r>"), Some(0));
    // Un tramo de texto cuenta una vez.
    assert_eq!(violaciones(&a, "<r>texto <a/></r>"), Some(1));
    assert_eq!(violaciones(&a, "<r>uno<a/>dos</r>"), Some(2));
}

#[test]
fn atributos_fixed_por_valor_y_xmlns() {
    let a = esquema(
        "<xs:element name='e'><xs:complexType>\
         <xs:attribute name='b' type='xs:boolean' fixed='1'/>\
         <xs:attribute name='i' type='xs:int' fixed='1'/>\
         <xs:attribute name='s' type='xs:string' fixed='a b'/>\
         <xs:attribute name='f' type='xs:float' fixed='NaN'/>\
         <xs:attribute name='d' type='xs:double' fixed='0.1'/>\
         <xs:attribute name='r' type='xs:string' use='required'/>\
         </xs:complexType></xs:element>",
    );
    assert_eq!(violaciones(&a, "<e r=''/>"), Some(0));
    assert_eq!(
        violaciones(
            &a,
            "<e xmlns='urn:a' xmlns:p='urn:b' r='' b='true' i='+01' s='a b' f='NaN' d=' 0.1000000000000000000001 '/>"
        ),
        Some(0)
    );
    assert_eq!(violaciones(&a, "<e r='' b='false'/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' b='0'/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' i='2'/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' s='a  b'/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' f='1'/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' f='INF'/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' d='0.2'/>"), Some(1));
    // Falta el requerido, uno no declarado, y no del tipo (sin `fixed`).
    assert_eq!(violaciones(&a, "<e/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' z='1'/>"), Some(1));
    assert_eq!(violaciones(&a, "<e r='' i='x'/>"), Some(1));
    // Repetido: mal formado.
    sintaxis(&a, "<e r='' r=''/>");
}

#[test]
fn float_umbral_ieee_nan_e_inf() {
    let a = esquema(
        "<xs:element name='e'><xs:simpleType><xs:restriction base='xs:float'>\
         <xs:maxInclusive value='0.1'/></xs:restriction></xs:simpleType></xs:element>",
    );
    assert_eq!(violaciones(&a, "<e>0.10000000000000000001</e>"), Some(0));
    assert_eq!(violaciones(&a, "<e>0.1</e>"), Some(0));
    // El flotante siguiente a 0.1 en binary32 (13421774 · 2⁻²⁷).
    assert_eq!(
        violaciones(&a, "<e>0.100000008940696716308594</e>"),
        Some(1)
    );
    assert_eq!(violaciones(&a, "<e>NaN</e>"), Some(1));
    assert_eq!(violaciones(&a, "<e>INF</e>"), Some(1));
    assert_eq!(violaciones(&a, "<e>-INF</e>"), Some(0));
    assert_eq!(violaciones(&a, "<e>+INF</e>"), Some(1));
    let b = esquema(
        "<xs:element name='e'><xs:simpleType><xs:restriction base='xs:double'>\
         <xs:minExclusive value='-1'/></xs:restriction></xs:simpleType></xs:element>",
    );
    assert_eq!(violaciones(&b, "<e>INF</e>"), Some(0));
    assert_eq!(violaciones(&b, "<e>NaN</e>"), Some(1));
    assert_eq!(violaciones(&b, "<e>-INF</e>"), Some(1));
    assert_eq!(violaciones(&b, "<e>-1</e>"), Some(1));
    assert_eq!(violaciones(&b, "<e>-0.99999999999999999999</e>"), Some(1));
    assert_eq!(violaciones(&b, "<e>-0.9999999999999999</e>"), Some(0));
    assert_eq!(violaciones(&b, "<e>1e400</e>"), Some(0));
}

#[test]
fn tipos_simples_enum_rangos_y_longitudes() {
    let a = esquema(
        "<xs:simpleType name='Edad'><xs:restriction base='xs:int'>\
           <xs:minInclusive value='0'/><xs:maxExclusive value='150'/></xs:restriction></xs:simpleType>\
         <xs:simpleType name='Color'><xs:restriction base='xs:string'>\
           <xs:enumeration value='rojo'/><xs:enumeration value='ro'/><xs:enumeration value=''/>\
           <xs:maxLength value='3'/></xs:restriction></xs:simpleType>\
         <xs:simpleType name='Talla'><xs:restriction base='xs:decimal'>\
           <xs:enumeration value='1'/><xs:enumeration value='1.0'/><xs:enumeration value='2.5'/>\
         </xs:restriction></xs:simpleType>\
         <xs:element name='r'><xs:complexType><xs:sequence>\
           <xs:element name='edad' type='Edad' minOccurs='0'/>\
           <xs:element name='color' type='Color' minOccurs='0'/>\
           <xs:element name='talla' type='Talla' minOccurs='0'/>\
           <xs:element name='b' type='xs:boolean' minOccurs='0'/>\
           <xs:element name='byte' type='xs:byte' minOccurs='0'/>\
         </xs:sequence></xs:complexType></xs:element>",
    );
    assert_eq!(violaciones(&a, "<r><edad>149</edad><color>ro</color><talla>1.00</talla><b> true </b><byte>-128</byte></r>"), Some(0));
    assert_eq!(
        violaciones(&a, "<r><color></color><talla>+2.50</talla></r>"),
        Some(0)
    );
    assert_eq!(violaciones(&a, "<r><edad>150</edad></r>"), Some(1));
    // -1 viola el mínimo de Edad (no el de xs:int).
    assert_eq!(violaciones(&a, "<r><edad>-1</edad></r>"), Some(1));
    // Fuera de xs:int y de Edad: las dos cotas máximas.
    assert_eq!(violaciones(&a, "<r><edad>3000000000</edad></r>"), Some(2));
    // No es un entero: una sola violación del espacio léxico.
    assert_eq!(violaciones(&a, "<r><edad>1.0</edad></r>"), Some(1));
    // `rojo` está en el enum pero pasa de maxLength; `r` no está.
    assert_eq!(violaciones(&a, "<r><color>rojo</color></r>"), Some(1));
    assert_eq!(violaciones(&a, "<r><color>r</color></r>"), Some(1));
    assert_eq!(violaciones(&a, "<r><color> ro</color></r>"), Some(1));
    assert_eq!(violaciones(&a, "<r><talla>3</talla></r>"), Some(1));
    assert_eq!(violaciones(&a, "<r><b>yes</b></r>"), Some(1));
    assert_eq!(violaciones(&a, "<r><byte>128</byte></r>"), Some(1));
    // Un hijo dentro de un contenido simple: inesperado.
    assert_eq!(violaciones(&a, "<r><edad>1<x/>2</edad></r>"), Some(1));
}

#[test]
fn nombres_de_cierre_y_prefijos() {
    let a = esquema(PEDIDO);
    // Nombre local: el prefijo no cuenta.
    assert_eq!(
        violaciones(
            &a,
            "<a:pedido id='1'><b:cliente/><total>1</total></c:pedido>"
        ),
        Some(0)
    );
}

/// Valida bytes (también los que no son UTF-8) en bloques de 1, 7 y 4096
/// con el mismo reporte.
fn correr_bytes(a: &Automaton, payload: &[u8]) -> Report {
    let r = validate(a, payload, Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(validate(a, payload, Mode::Total, chunk), r, "{payload:?}");
    }
    r
}

/// UTF-8 inválido en el texto, en un byte suelto de una secuencia y en un
/// valor de atributo: error léxico (∞) en el offset del byte.
#[test]
fn utf8_invalido_es_lexico() {
    let a = esquema("<xs:element name='e' type='xs:string'/>");
    for (p, off) in [
        (&b"<e>\xff\xfe</e>"[..], 3),
        (&b"<e>\xc3</e>"[..], 3),
        (&b"<e a=\"\xff\"/>"[..], 6),
    ] {
        let r = correr_bytes(&a, p);
        assert_eq!(r.error_kind, Some(ErrorKind::Lexical), "{p:?}: {r:?}");
        assert_eq!(r.violations, Weight::INFINITY);
        assert_eq!(r.first.unwrap().offset, off, "{p:?}");
    }
}

/// Valida con `XmlValidator` y una memoria de `words` palabras, en bloques
/// de `chunk` bytes.
fn con_memoria(a: &Automaton, payload: &[u8], words: usize, chunk: usize) -> Report {
    use runtime_rs::{Flow, Options, XmlValidator};
    let mut mem = vec![0u64; words];
    let mut v = XmlValidator::new(a.tables(), &Options { mode: Mode::Total }, &mut mem);
    for c in payload.chunks(chunk) {
        if v.feed(c) == Flow::Stop {
            break;
        }
    }
    v.finish()
}

/// Sin sitio para otro marco en la memoria del llamador: `limit` (∞), sin
/// pánico ni reservas, igual en bloques de 1, 7 y 4096. Con la memoria de
/// 256 marcos, 256 niveles valen y el 257.º da `limit` en su `<`.
#[test]
fn pila_llena_da_limit() {
    let a = esquema(
        "<xs:complexType name='T'><xs:sequence>\
         <xs:element name='a' type='T' minOccurs='0'/></xs:sequence></xs:complexType>\
         <xs:element name='a' type='T'/>",
    );
    let niveles = |d: usize| format!("{}{}", "<a>".repeat(d), "</a>".repeat(d));
    let p50 = niveles(50);
    let justa = a.stack_words(50);
    for chunk in [1, 7, 4096] {
        let r = con_memoria(&a, p50.as_bytes(), justa, chunk);
        assert!(r.valid, "{chunk}: {r:?}");
        let r = con_memoria(&a, p50.as_bytes(), justa - 1, chunk);
        assert_eq!(r.error_kind, Some(ErrorKind::Limit), "{chunk}: {r:?}");
        assert_eq!(r.violations, Weight::INFINITY);
        assert_eq!(r.first.unwrap().offset, 3 * 49);
        for w in [0, 1] {
            let r = con_memoria(&a, p50.as_bytes(), w, chunk);
            assert_eq!(r.error_kind, Some(ErrorKind::Limit), "{w}: {r:?}");
        }
    }
    let mem = a.stack_words(256);
    for chunk in [1, 7, 4096] {
        assert!(con_memoria(&a, niveles(256).as_bytes(), mem, chunk).valid);
        let r = con_memoria(&a, niveles(257).as_bytes(), mem, chunk);
        assert_eq!(r.error_kind, Some(ErrorKind::Limit), "{chunk}: {r:?}");
        assert_eq!(r.first.unwrap().offset, 3 * 256);
    }
}

/// Bordes de `fixed` en los flotantes: `fixed='INF'` en `xs:float` lo
/// cumplen `INF` y un número que desborda (`1e999`), no `NaN` ni uno
/// finito; `fixed='-INF'` en `xs:double`, igual con el signo.
#[test]
fn fixed_infinito_en_flotantes() {
    let a = esquema(
        "<xs:element name='e'><xs:complexType>\
         <xs:attribute name='f' type='xs:float' fixed='INF'/>\
         <xs:attribute name='d' type='xs:double' fixed='-INF'/>\
         </xs:complexType></xs:element>",
    );
    for (p, v) in [
        ("<e f='INF' d='-INF'/>", 0),
        ("<e f='1e999' d='-1e999'/>", 0),
        ("<e f=' 3.5e38 '/>", 0),
        ("<e f='NaN'/>", 1),
        ("<e d='NaN'/>", 1),
        ("<e f='-INF'/>", 1),
        ("<e d='INF'/>", 1),
        ("<e f='3.4e38'/>", 1),
        ("<e d='-1e308'/>", 1),
    ] {
        assert_eq!(violaciones(&a, p), Some(v), "{p}");
    }
}

/// Contadores por encima de 2¹⁶ y en `u32::MAX` (la cota de 64 bits en
/// `Check::arg` | `Check::len`): mínimos y máximos exactos con 70 000 hijos.
#[test]
fn contadores_grandes() {
    let modelo = |occurs: &str| {
        esquema(&format!(
            "<xs:complexType name='V'/><xs:element name='r'><xs:complexType><xs:sequence>\
             <xs:element name='a' type='V' {occurs}/></xs:sequence></xs:complexType></xs:element>"
        ))
    };
    let hijos = |n: usize| format!("<r>{}</r>", "<a/>".repeat(n));
    let min = modelo("minOccurs='70000' maxOccurs='unbounded'");
    assert_eq!(violaciones(&min, &hijos(70_000)), Some(0));
    assert_eq!(violaciones(&min, &hijos(70_001)), Some(0));
    assert_eq!(violaciones(&min, &hijos(69_999)), Some(1));
    let max = modelo("maxOccurs='70000'");
    assert_eq!(violaciones(&max, &hijos(70_000)), Some(0));
    assert_eq!(violaciones(&max, &hijos(70_001)), Some(1));
    let enorme = modelo("minOccurs='4294967295' maxOccurs='4294967295'");
    assert_eq!(violaciones(&enorme, &hijos(70_001)), Some(1));
    let r = correr(&enorme, &hijos(70_001));
    let (_, kind) = enorme.rule(r.first.unwrap().rule.unwrap());
    assert_eq!(kind, RuleKind::Content);
}

/// Ronda 3 (I5 y la propiedad del presupuesto, D53 (5)): en todos los XSD de
/// `tests/schemas/xsd/parse-ok` que compilan, el presupuesto contado antes de
/// reservar cubre las entradas que de verdad se emiten. Antes de la ronda 3,
/// `vacio.xsd` y `tipo-simple-con-nombre.xsd` daban 2 entradas de más que su
/// cuenta (los `Fails` del trie del documento).
#[test]
fn presupuesto_cubre_lo_emitido_en_parse_ok() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/schemas/xsd/parse-ok"
    );
    let mut compilan = 0;
    let mut nombres: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "xsd"))
        .collect();
    nombres.sort();
    for p in nombres {
        let src = std::fs::read_to_string(&p).unwrap();
        let ir = std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                let s = xsd::parse(src.as_bytes()).ok()?;
                xsd::lower(&xsd::analyze(&s).ok()?).ok()
            })
            .unwrap()
            .join()
            .unwrap();
        let Some(ir) = ir else { continue };
        if presupuesto::cubre(&ir) > 0 {
            compilan += 1;
        }
    }
    assert!(compilan >= 8, "{compilan} XSD compilados");
}
