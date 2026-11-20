//! Criterio 6 de F4.1a (F4-U9, F4-B25): el núcleo de `runtime-rs` no
//! reserva memoria en el camino de validación. Binario de prueba aparte
//! con un asignador global que cuenta: las tablas y la pila se preparan
//! antes (eso sí reserva) y se cuenta solo lo que pasa entre `new` y
//! `finish`, con payloads válidos, inválidos y malformados, en los dos
//! modos y con bloques de 1, 7 y 4096 bytes.
//!
//! Es el único `#[allow(unsafe_code)]` del workspace (F4-B25): contar
//! reservas exige implementar `GlobalAlloc`, que es `unsafe`.
//!
//! Cuenta también los bytes pedidos (ronda 1 de F4.1b): el constructor
//! comprueba el tope de un producto y el presupuesto de los cierres antes de
//! reservar nada proporcional al producto.
//!
//! Que el crate no use `alloc` lo garantiza su `#![no_std]` sin
//! `extern crate alloc` (no compilaría con un `Vec`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use runtime_rs::{Flow, JsonValidator, Mode, Options, XmlValidator};

struct Contador;

thread_local! {
    /// Reservas de este hilo (las pruebas corren en hilos distintos).
    static RESERVAS: Cell<u64> = const { Cell::new(0) };
    /// Bytes pedidos por este hilo (`alloc` y el tamaño nuevo de `realloc`).
    static BYTES: Cell<u64> = const { Cell::new(0) };
}

// La única excepción a `unsafe_code = "deny"` del workspace, y solo en este
// binario de prueba: `GlobalAlloc` es un rasgo `unsafe` por definición. Las
// tres funciones solo cuentan y delegan en `System` con los mismos
// argumentos, así que cumplen su contrato si `System` lo cumple.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Contador {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = RESERVAS.try_with(|r| r.set(r.get() + 1));
        let _ = BYTES.try_with(|b| b.set(b.get() + layout.size() as u64));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = RESERVAS.try_with(|r| r.set(r.get() + 1));
        let _ = BYTES.try_with(|b| b.set(b.get() + new_size as u64));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ASIGNADOR: Contador = Contador;

fn reservas() -> u64 {
    RESERVAS.with(Cell::get)
}

fn bytes() -> u64 {
    BYTES.with(Cell::get)
}

fn ir_de(src: &str) -> ir::VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = schema_parser::parse(src.as_bytes()).unwrap();
            semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// El contador ve las reservas (si no, la prueba de abajo no probaría nada).
#[test]
fn el_contador_cuenta() {
    let antes = reservas();
    let v: Vec<u64> = std::hint::black_box(vec![1, 2, 3]);
    assert!(reservas() > antes, "{v:?}");
}

#[test]
fn cero_reservas_al_validar() {
    cero_reservas(
        r##"{"$defs": {"n": {"type": "object", "properties": {"v": {"type": "integer"},
            "c": {"type": "array", "prefixItems": [true], "items": {"$ref": "#/$defs/n"}}},
            "required": ["v"], "dependentRequired": {"c": ["v"]},
            "additionalProperties": false}}, "$ref": "#/$defs/n"}"##,
        &[
            br#"{"v": 1, "c": [0, {"v": 2.0}, {"v": 3, "c": []}]}"#,
            br#"{"v": 1.5, "x": {"y": ["\u00f1", true, null]}, "c": [1, {}]}"#,
            br#"{"v": 1, "v": 2}"#,
            br#"{"v": 1e99999, "c": [1, 2,]}"#,
            b"[\xff]",
            b"",
        ],
    );
}

/// F4.1b: contadores, longitudes, `enum`/`const`, rangos y `multipleOf`
/// sobre el lexema (también el que pasa de 1 024 bytes) y un producto por
/// `allOf` y `$ref` con hermanas: tampoco reservan.
#[test]
fn cero_reservas_con_registros_y_productos() {
    let largo = format!("[1e{}]", "9".repeat(2000));
    cero_reservas(
        r##"{"$defs": {"e": {"enum": ["", "ab", "abc", 1.5, true]}},
            "allOf": [{"minProperties": 1}, {"maxProperties": 3}],
            "properties": {"l": {"type": "array", "minItems": 1, "maxItems": 2,
                "items": {"minimum": 0, "exclusiveMaximum": 1e3, "multipleOf": 0.25}},
              "s": {"minLength": 1, "maxLength": 3, "$ref": "#/$defs/e"},
              "k": {"const": "x"}}}"##,
        &[
            br#"{"l": [0.5, 999.75], "s": "abc", "k": "x"}"#,
            br#"{"l": [0.3, -1, 1e3], "s": "\u00f1", "k": 1}"#,
            br#"{"s": 1.50, "a": 1, "b": 2, "c": 3}"#,
            largo.as_bytes(),
            br#"{}"#,
        ],
    );
}

fn cero_reservas(esquema: &str, payloads: &[&[u8]]) {
    let ir = ir_de(esquema);
    let a = automaton::build(&ir).unwrap();
    let mut mem = vec![0u64; a.state_words(&runtime_rs::Limits::default())];
    for &p in payloads {
        for mode in [Mode::Total, Mode::EarlyFail] {
            for chunk in [1, 7, 4096] {
                let antes = reservas();
                let mut v = JsonValidator::new(a.tables(), &Options::new(mode), &mut mem);
                for c in p.chunks(chunk) {
                    if v.feed(c) == Flow::Stop {
                        break;
                    }
                }
                let r = v.finish();
                let despues = reservas();
                assert_eq!(despues - antes, 0, "{p:?}, {mode:?}, {chunk}: {r:?}");
            }
        }
    }
}

/// `allOf` de un objeto con `n` nombres y `ramas` ramas `{"properties":
/// {"k0": true}}`: un producto de objetos de `ramas + 1` componentes.
fn producto_ancho(n: usize, ramas: usize) -> ir::VerifiedIr {
    let props: Vec<String> = (0..n).map(|i| format!("\"k{i}\": true")).collect();
    let mut src = format!(r#"{{"allOf": [{{"properties": {{{}}}}}"#, props.join(","));
    for _ in 0..ramas {
        src.push_str(r#", {"properties": {"k0": true}}"#);
    }
    src.push_str("]}");
    ir_de(&src)
}

/// Ronda 1 de F4.1b (L-1): el tope de un producto de objetos se comprueba
/// **antes** de reservar nada proporcional al producto. Con 4 096 nombres y
/// 2 000 ramas el producto tiene 12 296 estados (> 4 096) y una matriz
/// nombres × componentes ocuparía 4 096 × 2 001 × 4 B ≈ 32 MB; `build_with`
/// da `ProductLimit` habiendo pedido menos de 4 MiB en total (sin reloj, D1).
#[test]
fn tope_de_producto_antes_de_reservar() {
    let ir = producto_ancho(4096, 2000);
    let antes = bytes();
    let r = automaton::build_with(&ir, &automaton::BuildOptions::default());
    let pedidos = bytes() - antes;
    match r {
        Err(automaton::BuildError::ProductLimit { states, scalar, .. }) => {
            assert_eq!((states, scalar), (12_296, false));
        }
        otro => panic!("{otro:?}"),
    }
    assert!(pedidos < 4 << 20, "{pedidos} bytes antes del tope");
}

/// Ronda 1 de F4.1b: el presupuesto de los cierres (`MAX_CLOSURE_WORK` =
/// 2²⁴) se alcanza y da `limit`, también antes de reservar: 1 000 nombres ×
/// 17 001 componentes ≈ 1,7·10⁷ pasos, con un producto de 3 118 estados que
/// cabe en el tope.
#[test]
fn presupuesto_de_los_cierres_da_limit() {
    let ir = producto_ancho(1000, 17_000);
    let antes = bytes();
    let r = automaton::build(&ir);
    let pedidos = bytes() - antes;
    match r {
        Err(automaton::BuildError::Limit {
            what, limit, found, ..
        }) => {
            assert_eq!(limit, automaton::MAX_CLOSURE_WORK);
            assert!(found > automaton::MAX_CLOSURE_WORK, "{found}");
            assert!(what.contains("cierre"), "{what}");
        }
        otro => panic!("{otro:?}"),
    }
    assert!(pedidos < 16 << 20, "{pedidos} bytes antes del límite");
    // Por debajo del presupuesto compila.
    assert!(automaton::build(&producto_ancho(1000, 100)).is_ok());
}

// --- Ronda 2 de F4.1b (I1): tablas que crecían con los productos ---------

/// Esquema y bytes que pide `build` sobre su IR (`Ok` o el error).
fn bytes_al_construir(
    src: &str,
) -> (
    usize,
    u64,
    Result<automaton::Automaton, automaton::BuildError>,
) {
    let ir = ir_de(src);
    let antes = bytes();
    let r = automaton::build(&ir);
    (src.len(), bytes() - antes, r)
}

/// `prefixItems` de `p` `$ref` a un `allOf` de `ramas` ramas
/// `{"type": "string", "maxLength": i}`: cada índice tiene su retorno, y las
/// comprobaciones de entrada del producto (una por rama y clase) no deben
/// copiarse por retorno (antes: p × ramas × 6).
fn prefix_items_de_producto(p: usize, ramas: usize) -> String {
    let refs: Vec<&str> = (0..p).map(|_| r##"{"$ref": "#/$defs/s"}"##).collect();
    let ramas: Vec<String> = (0..ramas)
        .map(|i| format!(r#"{{"type": "string", "maxLength": {}}}"#, i + 1))
        .collect();
    format!(
        r#"{{"prefixItems": [{}], "$defs": {{"s": {{"allOf": [{}]}}}}}}"#,
        refs.join(","),
        ramas.join(",")
    )
}

/// `n` claves con `allOf: [$ref E, {minimum: i}]` y E un `enum` de `m`
/// números: el `enum` se ordena y se copia a las tablas una sola vez.
fn enum_compartido(n: usize, m: usize) -> String {
    let nums: Vec<String> = (0..m).map(|i| format!("{i}.5")).collect();
    let props: Vec<String> = (0..n)
        .map(|i| format!(r##""k{i}": {{"allOf": [{{"$ref": "#/$defs/e"}}, {{"minimum": {i}}}]}}"##))
        .collect();
    format!(
        r#"{{"properties": {{{}}}, "$defs": {{"e": {{"enum": [{}]}}}}}}"#,
        props.join(","),
        nums.join(",")
    )
}

/// `allOf` de `n` `const` de cadenas distintas: el cierre de cada estado del
/// trie lleva una comprobación por literal (estados × literales).
fn consts_de_cadena(n: usize) -> String {
    let ramas: Vec<String> = (0..n).map(|i| format!(r#"{{"const": "c{i}"}}"#)).collect();
    format!(r#"{{"allOf": [{}]}}"#, ramas.join(","))
}

/// Cota de bytes que puede pedir el constructor por byte del esquema (las
/// medidas dan ≈ 100; la IR, los cierres, las tablas hash y δ).
const BYTES_POR_BYTE: u64 = 256;

/// Ronda 2 de F4.1b (I1, vía 1): con 2 000 índices de `prefixItems` que
/// esperan un producto de 200 ramas, las comprobaciones de entrada se
/// guardan una vez por (conjunto, clase) y no por retorno; antes eran
/// 2 000 × 6 × 200 = 2,4·10⁶ (≈ 58 MB).
#[test]
fn comprobaciones_de_entrada_compartidas() {
    let (n, b, r) = bytes_al_construir(&prefix_items_de_producto(2000, 200));
    let a = r.unwrap();
    // 6 clases × 200 ramas en la entrada, más los 200 `maxLength` al
    // cerrar la cadena en el único estado de su trie vacío (minimizado,
    // F4.3: el inicial y el muerto son equivalentes).
    assert_eq!(a.tables().checks.len(), 6 * 200 + 200);
    assert!(b < BYTES_POR_BYTE * n as u64, "{b} bytes para {n}");
}

/// Ronda 2 de F4.1b (I1, vía 3): un `enum` numérico leído por 2 000
/// conjuntos se ordena y se copia a `nums`/`lexemes` una sola vez.
#[test]
fn enum_numerico_compartido_una_vez() {
    let (n, b, r) = bytes_al_construir(&enum_compartido(2000, 5000));
    let a = r.unwrap();
    let t = a.tables();
    // 5 000 valores del `enum` y 2 000 cotas de `minimum`.
    assert_eq!(t.nums.len(), 5000 + 2000);
    assert!(b < BYTES_POR_BYTE * n as u64, "{b} bytes para {n}");
}

/// Ronda 2 de F4.1b (I1, vía 2): lo que sigue siendo multiplicativo (el
/// cierre de cada estado del trie con una comprobación por literal) cuenta
/// en el presupuesto de F4-U6 **antes** de reservar: 2 000 `const` en un
/// `allOf` dan `limit` (≈ 4·10⁶ entradas) pidiendo poco más que la IR.
#[test]
fn presupuesto_ampliado_da_limit_antes_de_reservar() {
    let (n, b, r) = bytes_al_construir(&consts_de_cadena(2000));
    match r {
        Err(automaton::BuildError::Limit {
            what, limit, found, ..
        }) => {
            assert_eq!(limit, automaton::MAX_DELTA);
            assert!(found > 4_000_000, "{found}");
            assert!(what.contains("comprobaciones"), "{what}");
        }
        otro => panic!("{otro:?}"),
    }
    assert!(b < BYTES_POR_BYTE * n as u64, "{b} bytes para {n}");
    // Un `enum` grande sin producto sigue compilando: 1 literal por estado.
    let lits: Vec<String> = (0..2000).map(|i| format!("\"c{i}\"")).collect();
    let (_, _, r) = bytes_al_construir(&format!(r#"{{"enum": [{}]}}"#, lits.join(",")));
    assert!(r.is_ok(), "{r:?}");
}

// --- F4.1c: XML ------------------------------------------------------------

fn xsd_de(src: &str) -> ir::VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = xsd::parse(src.as_bytes()).unwrap();
            xsd::lower(&xsd::analyze(&s).unwrap()).unwrap()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// F4.1c: el núcleo con el lexer XML tampoco reserva: elementos, atributos
/// con `fixed`, contadores, `xs:all`, textos numéricos con umbrales IEEE,
/// elementos saltados, instrucciones y un payload malformado.
#[test]
fn cero_reservas_con_xml() {
    let ir = xsd_de(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
         <xs:complexType name='V'><xs:all><xs:element name='x' type='xs:float'/>\
           <xs:element name='y' type='xs:string' minOccurs='0'/></xs:all></xs:complexType>\
         <xs:element name='r'><xs:complexType><xs:sequence minOccurs='2' maxOccurs='3'>\
           <xs:element name='a' type='V' maxOccurs='2'/><xs:element name='b' type='xs:int'/>\
         </xs:sequence><xs:attribute name='f' type='xs:boolean' fixed='true'/>\
         <xs:attribute name='d' type='xs:decimal' use='required'/></xs:complexType></xs:element>\
         </xs:schema>",
    );
    let a = automaton::build(&ir).unwrap();
    let mut mem = vec![0u64; a.state_words(&runtime_rs::Limits::default())];
    let payloads: [&[u8]; 7] = [
        b"<?xml version='1.0'?><r d='1.5' f='1'><a><y>t</y><x>0.1</x></a><b>7</b><a><x>NaN</x></a><b>-2</b></r>",
        b"<r d='x'><a><x>1</x><x>2</x></a><z q='1'><w/></z><b>99999999999</b><?p q?></r>",
        b"<r d='1'><a></a><b>1</b>",
        b"<r d='1' d='2'/>",
        b"<!DOCTYPE r><r/>",
        b"<r d='1'><a><x>\xff\xfe</x></a></r>",
        b"<r d=\"\xc3\"/>",
    ];
    for p in payloads {
        for mode in [Mode::Total, Mode::EarlyFail] {
            for chunk in [1, 7, 4096] {
                let antes = reservas();
                let mut v = XmlValidator::new(a.tables(), &Options::new(mode), &mut mem);
                for c in p.chunks(chunk) {
                    if v.feed(c) == Flow::Stop {
                        break;
                    }
                }
                let r = v.finish();
                assert_eq!(reservas() - antes, 0, "{p:?}, {mode:?}, {chunk}: {r:?}");
            }
        }
    }
}

/// Criterio de F4.1c (F4.1b-CR2-S3): `(e₁|…|e₄₀₉₅)*` con el presupuesto
/// bajado da `limit` antes de reservar lo proporcional a su Glushkov (16 M
/// pares de follow, ≈ 4 096 tries de 4 096 nombres): menos de 16 MiB.
#[test]
fn glushkov_ancho_da_limit_antes_de_reservar() {
    let hojas: String = (1..=4095)
        .map(|i| format!("<xs:element name='e{i}' type='xs:string'/>"))
        .collect();
    let ir = xsd_de(&format!(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
         <xs:element name='r'><xs:complexType><xs:choice maxOccurs='unbounded'>{hojas}\
         </xs:choice></xs:complexType></xs:element></xs:schema>"
    ));
    let opts = automaton::BuildOptions {
        max_delta: 1 << 16,
        ..automaton::BuildOptions::default()
    };
    let antes = bytes();
    let r = automaton::build_with(&ir, &opts);
    let pedidos = bytes() - antes;
    assert!(
        matches!(r, Err(automaton::BuildError::Limit { .. })),
        "{:?}",
        r.err()
    );
    assert!(pedidos < 16 << 20, "{pedidos} bytes antes del límite");
}

// --- Ronda 2 de F4.1c: lo que se emite y se calcula, contado antes ----------

/// Bytes que pide `build` sobre la IR de un XSD, y su resultado.
fn bytes_xsd(src: &str) -> (u64, Result<automaton::Automaton, automaton::BuildError>) {
    let ir = xsd_de(src);
    let antes = bytes();
    let r = automaton::build(&ir);
    (bytes() - antes, r)
}

fn es_limit(r: &Result<automaton::Automaton, automaton::BuildError>) -> bool {
    matches!(r, Err(automaton::BuildError::Limit { .. }))
}

/// C1: una cadena de 8 000 restricciones con `minLength` (el texto del
/// último tipo lleva 8 000 comprobaciones en cada estado) usada por 60 hijos
/// de tres `xs:choice*`: cada arista emitiría las comprobaciones del hijo
/// vacío (antes: 2,4 GB y ≈ 88 M comprobaciones). El presupuesto de F4-U6
/// las cuenta en el análisis: `limit` antes de reservar.
#[test]
fn comprobaciones_por_arista_dan_limit_antes_de_reservar() {
    let mut src = String::from("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>");
    src.push_str("<xs:simpleType name='T0'><xs:restriction base='xs:string'><xs:minLength value='0'/></xs:restriction></xs:simpleType>");
    for i in 1..8000 {
        src.push_str(&format!(
            "<xs:simpleType name='T{i}'><xs:restriction base='T{}'><xs:minLength value='0'/></xs:restriction></xs:simpleType>",
            i - 1
        ));
    }
    for k in 0..3 {
        src.push_str(&format!(
            "<xs:complexType name='K{k}'><xs:choice maxOccurs='unbounded'>"
        ));
        for c in 0..60 {
            src.push_str(&format!("<xs:element name='c{c}' type='T7999'/>"));
        }
        src.push_str("</xs:choice></xs:complexType>");
    }
    src.push_str("<xs:element name='r'><xs:complexType><xs:sequence><xs:element name='k0' type='K0'/>\
                  <xs:element name='k1' type='K1'/><xs:element name='k2' type='K2'/></xs:sequence></xs:complexType></xs:element></xs:schema>");
    let (pedidos, r) = bytes_xsd(&src);
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 64 << 20, "{pedidos} bytes antes del límite");
}

/// F4C-S1: 60 000 enumeraciones de `xs:double` (dos decimales exactos de
/// ≈ 770 cifras por valor) dan `limit` antes de calcular ningún intervalo.
#[test]
fn enumeraciones_flotantes_dan_limit_antes_de_calcular() {
    let vals: String = (0..60_000)
        .map(|i| format!("<xs:enumeration value='{i}.5e-{}'/>", i % 300 + 1))
        .collect();
    let src = format!(
        "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'><xs:element name='e'>\
         <xs:simpleType><xs:restriction base='xs:double'>{vals}</xs:restriction></xs:simpleType>\
         </xs:element></xs:schema>"
    );
    let (pedidos, r) = bytes_xsd(&src);
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 16 << 20, "{pedidos} bytes antes del límite");
    // Con pocas, compila.
    let src = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'><xs:element name='e'>\
         <xs:simpleType><xs:restriction base='xs:double'><xs:enumeration value='0.1'/>\
         <xs:enumeration value='2'/></xs:restriction></xs:simpleType></xs:element></xs:schema>";
    assert!(bytes_xsd(src).1.is_ok());
}

/// I1 / F4C-S2: una secuencia de 4 090 hijos opcionales tiene 8,4 M pares de
/// follow: `limit` sin materializar los bloques (antes, 251 MB); con
/// `maxOccurs="0"` no hay pares y compila sin reservar lo cuadrático.
#[test]
fn secuencias_anchas_sin_bloques_cuadraticos() {
    let hijos = |extra: &str| -> String {
        (0..4090)
            .map(|i| format!("<xs:element name='e{i}' type='xs:string' minOccurs='0'{extra}/>"))
            .collect()
    };
    let esquema = |h: String| {
        format!(
            "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'><xs:element name='r'>\
             <xs:complexType><xs:sequence>{h}</xs:sequence></xs:complexType></xs:element></xs:schema>"
        )
    };
    let (pedidos, r) = bytes_xsd(&esquema(hijos("")));
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 16 << 20, "{pedidos} bytes antes del límite");
    let (pedidos, r) = bytes_xsd(&esquema(hijos(" maxOccurs='0'")));
    assert!(r.is_ok(), "{:?}", r.err());
    assert!(pedidos < 16 << 20, "{pedidos} bytes");
}

// --- Ronda 3 de F4.1c: todo lo del análisis XML, contado antes -------------

const XS: &str = "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>";

/// Las `n` enumeraciones de cadena `w0`…: `<xs:enumeration value='w…'/>`.
fn enumeraciones(n: usize) -> String {
    (0..n)
        .map(|i| format!("<xs:enumeration value='w{i:05}'/>"))
        .collect()
}

/// C2 (repro A, `s2000.xsd`): una cadena de `n` restricciones que repiten el
/// mismo `enum` de `e` cadenas y un elemento con un atributo de cada tipo.
/// Cada texto veía los `enum` de toda su cadena: unía, copiaba y ordenaba
/// Σᵢ i · e palabras antes de comprobar nada (32,7 s y 179 MB con
/// 2 000 × 200). Ahora el análisis carga las palabras al presupuesto antes
/// de copiarlas y lo comprueba tras cada texto: `limit` pidiendo poco.
#[test]
fn cadena_de_enumeraciones_da_limit_antes_de_copiar() {
    let (n, e) = (300, 100);
    let mut src = format!(
        "{XS}<xs:simpleType name='t0'><xs:restriction base='xs:string'>{}</xs:restriction></xs:simpleType>",
        enumeraciones(e)
    );
    for i in 1..n {
        src.push_str(&format!(
            "<xs:simpleType name='t{i}'><xs:restriction base='t{}'>{}</xs:restriction></xs:simpleType>",
            i - 1,
            enumeraciones(e)
        ));
    }
    let attrs: String = (0..n)
        .map(|i| format!("<xs:attribute name='a{i}' type='t{i}'/>"))
        .collect();
    src.push_str(&format!(
        "<xs:element name='r'><xs:complexType>{attrs}</xs:complexType></xs:element></xs:schema>"
    ));
    let (pedidos, r) = bytes_xsd(&src);
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 16 << 20, "{pedidos} bytes antes del límite");
}

/// F4D-S1 (`audit_enum_derived_20000x4000.xsd`): `d` tipos derivados (cada
/// uno con su `maxLength`) de una base con `e` enumeraciones, usados por `d`
/// hijos de una secuencia. Cada cierre copiaba y ordenaba las `e` palabras
/// (15,9 s y 1,25 GB con 20 000 × 4 000); ahora las palabras se memorizan
/// por la lista de reglas `enum` (una copia) y el texto de cada hijo se
/// cuenta antes del siguiente: `limit` pidiendo poco.
#[test]
fn enumeraciones_derivadas_dan_limit_antes_de_copiar() {
    let (e, d) = (20_000, 400);
    let mut src = format!(
        "{XS}<xs:simpleType name='S'><xs:restriction base='xs:string'>{}</xs:restriction></xs:simpleType>",
        enumeraciones(e)
    );
    for k in 0..d {
        src.push_str(&format!(
            "<xs:simpleType name='D{k}'><xs:restriction base='S'><xs:maxLength value='{}'/></xs:restriction></xs:simpleType>",
            100_000 - k
        ));
    }
    let hijos: String = (0..d)
        .map(|k| format!("<xs:element name='e{k}' type='D{k}'/>"))
        .collect();
    src.push_str(&format!(
        "<xs:element name='r'><xs:complexType><xs:sequence>{hijos}</xs:sequence></xs:complexType></xs:element></xs:schema>"
    ));
    let (pedidos, r) = bytes_xsd(&src);
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 16 << 20, "{pedidos} bytes antes del límite");
}

/// F4D-M2 (C2 repro B, `m230.xsd`): `g` elementos globales con un
/// `xs:choice*` de 264 hijos cada uno (69 696 pares, justo por debajo del
/// tope de un modelo). El tope era por modelo: se materializaban los pares de
/// todos los modelos antes de comprobar (1,4 GB con 230; 1,1 GB con 40).
/// Ahora los pares se acumulan entre modelos y el tope se comprueba antes de
/// materializar: solo se construye el primer modelo (≈ 30 MB, legítimo).
#[test]
fn pares_de_glushkov_acumulados_dan_limit() {
    let hijos: String = (0..264)
        .map(|i| format!("<xs:element name='e{i}' type='xs:string'/>"))
        .collect();
    let globales: String = (0..40)
        .map(|g| {
            format!(
                "<xs:element name='g{g}'><xs:complexType><xs:choice maxOccurs='unbounded'>{hijos}\
                 </xs:choice></xs:complexType></xs:element>"
            )
        })
        .collect();
    let (pedidos, r) = bytes_xsd(&format!("{XS}{globales}</xs:schema>"));
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 48 << 20, "{pedidos} bytes antes del límite");
}

/// I4 / F4D-L2 (`r170.xsd`): `n` restricciones encadenadas de `xs:double`
/// con `maxInclusive='5e-324'` (dos decimales exactos de ≈ 770 cifras por
/// umbral) y un atributo de cada tipo. El umbral se calculaba por texto
/// (Σᵢ i redondeos, sin cargar al presupuesto de pasos); ahora una vez por
/// (nodo, regla), cargado antes: compila pidiendo ≈ 20 MB (antes, 69 MB).
#[test]
fn umbrales_flotantes_una_vez_por_regla() {
    let n = 100;
    let mut src = format!(
        "{XS}<xs:simpleType name='t0'><xs:restriction base='xs:double'><xs:maxInclusive value='5e-324'/></xs:restriction></xs:simpleType>"
    );
    for i in 1..n {
        src.push_str(&format!(
            "<xs:simpleType name='t{i}'><xs:restriction base='t{}'><xs:maxInclusive value='5e-324'/></xs:restriction></xs:simpleType>",
            i - 1
        ));
    }
    let attrs: String = (0..n)
        .map(|i| format!("<xs:attribute name='a{i}' type='t{i}'/>"))
        .collect();
    src.push_str(&format!(
        "<xs:element name='r'><xs:complexType>{attrs}</xs:complexType></xs:element></xs:schema>"
    ));
    let (pedidos, r) = bytes_xsd(&src);
    assert!(r.is_ok(), "{:?}", r.err());
    assert!(pedidos < 32 << 20, "{pedidos} bytes");
}

/// F4D-M1 (`audit_closure_chain.xsd`): una cadena de `n` restricciones con
/// `minExclusive` y `maxExclusive` y `u` globales de los tipos más profundos.
/// Cada texto copiaba las facetas de todo su cierre (828 MB con 6 000 ×
/// 4 000) antes de comprobar; ahora cada faceta pesa en el presupuesto de
/// los cierres y el texto se cuenta antes del siguiente global.
#[test]
fn cierres_largos_dan_limit_antes_de_copiar() {
    let (n, u) = (3000, 2000);
    let mut src = format!(
        "{XS}<xs:simpleType name='a0'><xs:restriction base='xs:integer'><xs:minInclusive value='0'/></xs:restriction></xs:simpleType>"
    );
    for i in 1..n {
        src.push_str(&format!(
            "<xs:simpleType name='a{i}'><xs:restriction base='a{}'><xs:minExclusive value='{i}'/><xs:maxExclusive value='{}'/></xs:restriction></xs:simpleType>",
            i - 1,
            100_000_000 - i
        ));
    }
    for i in n - u..n {
        src.push_str(&format!("<xs:element name='e{i}' type='a{i}'/>"));
    }
    src.push_str("</xs:schema>");
    let (pedidos, r) = bytes_xsd(&src);
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 32 << 20, "{pedidos} bytes antes del límite");
}

/// Ronda extra de F4.1c (I6 = F4E-M1, `sh260.xsd`): un `xs:choice*` de 260
/// hijos cuyos nombres comparten un prefijo de 50 000 caracteres (13 MB de
/// esquema). Tiene 67 600 pares, bajo el tope, pero la UPA y los tries por
/// estado recorrían ≈ 3,4·10⁹ bytes de nombres antes de comprobar nada
/// (≈ 11 s en release). Ahora esos bytes se cargan al presupuesto de pasos
/// con una pasada aritmética antes de compararlos: `limit` pidiendo poco.
/// El recorrido evitado lo mide sin reloj (D1) la unitaria
/// `nombres_largos_de_follow_se_cobran_antes` de `src/xml.rs`.
#[test]
fn nombres_largos_de_follow_dan_limit_antes_de_recorrerlos() {
    let prefijo = "p".repeat(50_000);
    let hijos: String = (0..260)
        .map(|i| format!("<xs:element name='{prefijo}{i}' type='xs:string'/>"))
        .collect();
    let (pedidos, r) = bytes_xsd(&format!(
        "{XS}<xs:element name='r'><xs:complexType><xs:choice maxOccurs='unbounded'>{hijos}\
         </xs:choice></xs:complexType></xs:element></xs:schema>"
    ));
    assert!(es_limit(&r), "{:?}", r.map(|a| a.summary()));
    assert!(pedidos < 16 << 20, "{pedidos} bytes antes del límite");
}

/// Criterio 4 de F4.6: cero reservas entre `new` y `finish` con los límites
/// por defecto (la reserva de claves en uso) en **todo** el corpus, JSON y
/// XML, en los dos modos y en bloques de 1, 7 y 4096 bytes; y con 10⁵
/// niveles de anidamiento con `max_depth` = 10⁵, sin recursión (el núcleo
/// corre en un hilo de 256 KiB).
#[test]
fn cero_reservas_en_todo_el_corpus_y_en_profundidad() {
    use baseline::corpus::{self, Format, Payload};
    let mut n = 0;
    for c in corpus::casos().unwrap() {
        let Payload::Bytes(p) = &c.payload else {
            continue;
        };
        let texto = String::from_utf8(c.schema_text.clone()).unwrap();
        let ir = match c.format {
            Format::Json => ir_de(&texto),
            Format::Xml => xsd_de(&texto),
        };
        let Ok(a) = automaton::build(&ir) else {
            continue;
        };
        let mut mem = vec![0u64; a.state_words(&runtime_rs::Limits::default())];
        for mode in [Mode::Total, Mode::EarlyFail] {
            for chunk in [1, 7, 4096] {
                let antes = reservas();
                let r = correr_sin_reservar(&a, p, &Options::new(mode), &mut mem, chunk);
                assert_eq!(reservas() - antes, 0, "{}, {mode:?}, {chunk}: {r:?}", c.id);
            }
        }
        n += 1;
    }
    assert!(n >= 17, "{n}");

    let a = automaton::build(&ir_de(r##"{"items": {"$ref": "#"}}"##)).unwrap();
    let d = 100_000;
    let p = format!("{}{}", "[".repeat(d), "]".repeat(d));
    let mut o = Options::new(Mode::Total);
    o.limits.max_depth = d;
    let mut mem = vec![0u64; a.state_words(&o.limits)];
    let (r, delta) = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(move || {
            let antes = reservas();
            let r = correr_sin_reservar(&a, p.as_bytes(), &o, &mut mem, 4096);
            (r, reservas() - antes)
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(r.valid, "{r:?}");
    assert_eq!(delta, 0);
}

fn correr_sin_reservar(
    a: &automaton::Automaton,
    p: &[u8],
    o: &Options,
    mem: &mut [u64],
    chunk: usize,
) -> runtime_rs::Report {
    match a.format() {
        automaton::PayloadFormat::Json => {
            let mut v = JsonValidator::new(a.tables(), o, mem);
            for c in p.chunks(chunk) {
                if v.feed(c) == Flow::Stop {
                    break;
                }
            }
            v.finish()
        }
        automaton::PayloadFormat::Xml => {
            let mut v = XmlValidator::new(a.tables(), o, mem);
            for c in p.chunks(chunk) {
                if v.feed(c) == Flow::Stop {
                    break;
                }
            }
            v.finish()
        }
    }
}
