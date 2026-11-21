//! F6.1: el puntero de la instancia del primer error (`Report::pointer`;
//! criterios en `tasks/todo.md` «Tarea F6.1», teoría en
//! `docs/fase6/puntero.md`). Todo payload se valida en bloques de 1, 7 y
//! 4096 bytes y en los dos modos con el mismo puntero.

use automaton::{build, validate, Automaton, ErrorKind, Mode, Report, POINTER_MAX};
use ir::VerifiedIr;
use proptest::prelude::*;

fn ir_json(src: &str) -> VerifiedIr {
    let src = src.to_owned();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let s = schema_parser::parse(src.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
            let t = semantic::analyze(&s).unwrap_or_else(|e| panic!("{e:?}"));
            semantic::lower(&t).unwrap_or_else(|e| panic!("{e}"))
        })
        .unwrap()
        .join()
        .unwrap()
}

fn ir_xsd(cuerpo: &str) -> VerifiedIr {
    let src =
        format!("<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>{cuerpo}</xs:schema>");
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

fn json(src: &str) -> Automaton {
    build(&ir_json(src)).unwrap_or_else(|e| panic!("{e}"))
}

/// El reporte, igual en bloques de 1, 7 y 4096.
fn correr(a: &Automaton, payload: &[u8]) -> Report {
    let r = validate(a, payload, Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(
            validate(a, payload, Mode::Total, chunk),
            r,
            "bloques de {chunk}"
        );
    }
    // Sin error de peso ∞, el fallo temprano se detiene en el mismo primer
    // error (con uno, se detiene antes, en la primera violación).
    if r.error_kind == Some(ErrorKind::Schema) {
        let e = validate(a, payload, Mode::EarlyFail, 7);
        assert_eq!(e.first, r.first);
        assert_eq!(e.pointer, r.pointer);
    }
    r
}

fn puntero(a: &Automaton, payload: &str) -> String {
    let r = correr(a, payload.as_bytes());
    assert!(r.first.is_some(), "{payload}: {r:?}");
    assert!(!r.pointer.truncated(), "{payload}");
    r.pointer.as_str().to_owned()
}

/// Cualquier valor, con los números ≥ 0 a toda profundidad.
const NO_NEGATIVOS: &str = r##"{"$ref":"#/$defs/v","$defs":{"v":{"minimum":0,
    "items":{"$ref":"#/$defs/v"},"additionalProperties":{"$ref":"#/$defs/v"}}}}"##;

/// La igualdad de reportes mira el puntero visible, no lo que quedó en el
/// búfer de un puntero anterior más largo.
#[test]
fn igualdad_por_el_puntero_visible() {
    let a = json(NO_NEGATIVOS);
    let x = correr(&a, br#"{"z":{"y":-1},"w":["#);
    let y = correr(&a, br#"{"z":{"y":1},"w":["#);
    assert_eq!(x.error_kind, Some(ErrorKind::Syntax));
    assert_eq!(x.pointer.as_str(), y.pointer.as_str());
    assert_eq!(x.pointer, y.pointer);
}

#[test]
fn sin_error_no_hay_puntero() {
    let a = json(NO_NEGATIVOS);
    let r = correr(&a, br#"{"a":[1,{"b":2}]}"#);
    assert!(r.valid && r.first.is_none());
    assert_eq!(r.pointer.as_str(), "");
}

#[test]
fn miembros_y_elementos_anidados() {
    let a = json(NO_NEGATIVOS);
    assert_eq!(puntero(&a, "-1"), "");
    assert_eq!(puntero(&a, "[-1]"), "/0");
    assert_eq!(puntero(&a, r#"{"a":[1,{"b":-2}]}"#), "/a/1/b");
    assert_eq!(puntero(&a, r#"[[1],[2,[3,-4]]]"#), "/1/1/1");
    // Tras cerrar un miembro anidado se vuelve al padre.
    assert_eq!(puntero(&a, r#"{"a":{"b":1},"c":-1}"#), "/c");
    assert_eq!(puntero(&a, r#"[{"x":[0]},{"y":1},-1]"#), "/2");
    // Espacios y saltos de línea no cambian nada.
    assert_eq!(puntero(&a, "{ \"a\" :\n [ 1 ,\n -1 ] }"), "/a/1");
}

#[test]
fn indices_con_mas_cifras() {
    let a = json(NO_NEGATIVOS);
    for n in [9usize, 10, 99, 100, 1000] {
        let mut p = String::from("[");
        for _ in 0..n {
            p.push_str("0,");
        }
        p.push_str("-1]");
        assert_eq!(puntero(&a, &p), format!("/{n}"));
    }
}

#[test]
fn claves_escapadas_rfc_6901() {
    let a = json(NO_NEGATIVOS);
    assert_eq!(puntero(&a, r#"{"a/b~c":-1}"#), "/a~1b~0c");
    assert_eq!(puntero(&a, r#"{"~01":-1}"#), "/~001");
    assert_eq!(puntero(&a, r#"{"":-1}"#), "/");
    assert_eq!(puntero(&a, r#"{"A\/\n":-1}"#), "/A~1\n");
    assert_eq!(puntero(&a, r#"{"ñandú😀":{"é":-1}}"#), "/ñandú😀/é");
}

#[test]
fn errores_del_contenedor() {
    // `required` en el `}` y `minItems` en el `]`: el contenedor.
    let a = json(r#"{"properties":{"u":{"items":{"required":["e"]}}}}"#);
    assert_eq!(puntero(&a, r#"{"u":[{"e":1},{"n":2}]}"#), "/u/1");
    let a = json(r#"{"properties":{"l":{"minItems":3}}}"#);
    assert_eq!(puntero(&a, r#"{"l":[1,2]}"#), "/l");
    assert_eq!(puntero(&a, r#"{"l":[]}"#), "/l");
    // `maxItems` se cuenta en la coma o el elemento de más: del arreglo o
    // de ese elemento, nunca de otro.
    let a = json(r#"{"maxItems":1}"#);
    let p = puntero(&a, "[0,1]");
    assert!(p.is_empty() || p == "/1", "{p}");
    // `maxProperties` / `additionalProperties: false`.
    let a = json(r#"{"properties":{"a":true},"additionalProperties":false}"#);
    assert_eq!(puntero(&a, r#"{"a":1,"b":2}"#), "/b");
}

#[test]
fn errores_lexicos_y_de_sintaxis() {
    let a = json("true");
    // La clave a medio leer no cuenta; el error es del objeto.
    let r = correr(&a, br#"{"a":{"b"1}}"#);
    assert_eq!(r.error_kind, Some(ErrorKind::Syntax));
    assert_eq!(r.pointer.as_str(), "/a");
    let r = correr(&a, br#"[1,[2,tru]]"#);
    assert_eq!(r.error_kind, Some(ErrorKind::Lexical));
    assert_eq!(r.pointer.as_str(), "/1/1");
    // Clave repetida: en su comilla de cierre, del objeto.
    let r = correr(&a, br#"{"x":{"k":1,"k":2}}"#);
    assert_eq!(r.error_kind, Some(ErrorKind::Syntax));
    assert_eq!(r.pointer.as_str(), "/x");
    // Entrada incompleta: donde se quedó.
    let r = correr(&a, br#"{"a":[1,"#);
    assert_eq!(r.pointer.as_str(), "/a/1");
    // El primer error es el de peso ∞ aunque antes hubiera una violación.
    let b = json(NO_NEGATIVOS);
    let r = correr(&b, br#"{"a":-1,"b":[1,}"#);
    assert_eq!(r.error_kind, Some(ErrorKind::Syntax));
    assert_eq!(r.pointer.as_str(), "/b");
}

/// Un segmento por nivel, de `len` bytes.
fn profundo(niveles: usize, len: usize) -> (String, Vec<String>) {
    let mut p = String::new();
    let mut segs = Vec::new();
    for i in 0..niveles {
        let k: String = std::iter::repeat_n(char::from(b'a' + (i % 26) as u8), len).collect();
        p.push_str(&format!("{{\"{k}\":"));
        segs.push(k);
    }
    p.push_str("-1");
    p.push_str(&"}".repeat(niveles));
    (p, segs)
}

#[test]
fn puntero_recortado_y_vuelta_atras() {
    let a = json(NO_NEGATIVOS);
    // 10 niveles de 150 bytes: 1 510 bytes; caben 6 segmentos (906).
    let (p, segs) = profundo(10, 150);
    let r = correr(&a, p.as_bytes());
    assert!(r.pointer.truncated());
    let esperado: String = segs[..6].iter().map(|s| format!("/{s}")).collect();
    assert_eq!(r.pointer.as_str(), esperado);
    assert!(r.pointer.as_bytes().len() <= POINTER_MAX);
    // Tras salir de los niveles que no cupieron, el puntero vuelve a ser
    // exacto (y los índices de un arreglo recortado, también).
    let k = "z".repeat(1000);
    let p = format!(r#"{{"{k}":{{"{k}":[0,1,2]}},"b":[0,0,-1]}}"#);
    assert_eq!(puntero(&a, &p), "/b/2");
    let p = format!(r#"[{{"{k}":[[0,0],[0,0]]}},[0,-1]]"#);
    assert_eq!(puntero(&a, &p), "/1/1");
    // Un segmento que cabe justo, y el índice que deja de caber al crecer.
    let k = "q".repeat(POINTER_MAX - 3);
    let mut p = format!(r#"{{"{k}":["#);
    p.push_str(&"0,".repeat(10));
    p.push_str("-1]}");
    let r = correr(&a, p.as_bytes());
    assert!(r.pointer.truncated());
    assert_eq!(r.pointer.as_str(), format!("/{k}"));
}

#[test]
fn xml_ruta_de_elementos() {
    let a = build(&ir_xsd(
        "<xs:element name='pedido'><xs:complexType><xs:sequence>\
         <xs:element name='linea' maxOccurs='unbounded'><xs:complexType><xs:sequence>\
         <xs:element name='cantidad' type='xs:positiveInteger'/>\
         </xs:sequence><xs:attribute name='id' type='xs:int' use='required'/>\
         </xs:complexType></xs:element>\
         </xs:sequence></xs:complexType></xs:element>",
    ))
    .unwrap();
    let ok = "<pedido><linea id='1'><cantidad>2</cantidad></linea></pedido>";
    assert!(correr(&a, ok.as_bytes()).valid);
    let p = "<pedido><linea id='1'><cantidad>2</cantidad></linea>\
             <linea id='2'><cantidad>0</cantidad></linea></pedido>";
    assert_eq!(puntero(&a, p), "/pedido/linea/cantidad");
    // Atributo que falta (en el `>`) o mal tipado: del elemento.
    let p = "<pedido><linea><cantidad>2</cantidad></linea></pedido>";
    assert_eq!(puntero(&a, p), "/pedido/linea");
    let p = "<pedido><linea id='x'><cantidad>2</cantidad></linea></pedido>";
    assert_eq!(puntero(&a, p), "/pedido/linea");
    // Falta un hijo: en la etiqueta de cierre, del elemento.
    let p = "<pedido><linea id='1'></linea></pedido>";
    assert_eq!(puntero(&a, p), "/pedido/linea");
    // Etiqueta de cierre que no empareja.
    let r = correr(
        &a,
        b"<pedido><linea id='1'><cantidad>2</cantidad></lineas></pedido>",
    );
    assert_eq!(r.error_kind, Some(ErrorKind::Syntax));
    assert_eq!(r.pointer.as_str(), "/pedido/linea");
    // Tras un elemento vacío se vuelve al padre.
    let a = build(&ir_xsd(
        "<xs:element name='r'><xs:complexType><xs:sequence>\
         <xs:element name='v' minOccurs='0' maxOccurs='unbounded'><xs:complexType/></xs:element>\
         <xs:element name='n' type='xs:int'/>\
         </xs:sequence></xs:complexType></xs:element>",
    ))
    .unwrap();
    assert_eq!(puntero(&a, "<r><v/><v/><n>x</n></r>"), "/r/n");
    // Un elemento que el modelo no admite: ese elemento (se ve al terminar
    // su nombre), como el miembro de `additionalProperties: false`.
    assert_eq!(puntero(&a, "<r><v/><zz/><n>1</n></r>"), "/r/zz");
}

/// Un valor JSON aleatorio con a lo sumo un `-1` en `marca`: el payload y la
/// ruta (segmentos sin escapar) de cada número en orden de documento.
#[derive(Clone, Debug)]
enum V {
    Num(bool),
    Arr(Vec<V>),
    Obj(Vec<(String, V)>),
}

fn valor() -> impl Strategy<Value = V> {
    let hoja = any::<bool>().prop_map(V::Num);
    hoja.prop_recursive(6, 64, 12, |v| {
        prop_oneof![
            prop::collection::vec(v.clone(), 0..12).prop_map(V::Arr),
            prop::collection::vec(("[a-c~/é😀\"\\\\]{0,4}|[x]{100,400}", v), 0..5).prop_map(|ms| {
                // Sin claves repetidas (sería un error de sintaxis).
                let mut vistos = std::collections::HashSet::new();
                V::Obj(
                    ms.into_iter()
                        .filter(|(k, _)| vistos.insert(k.clone()))
                        .collect(),
                )
            }),
        ]
    })
}

fn escribir(v: &V, out: &mut String, ruta: &mut Vec<String>, primero: &mut Option<Vec<String>>) {
    match v {
        V::Num(neg) => {
            out.push_str(if *neg { "-1" } else { "1" });
            if *neg && primero.is_none() {
                *primero = Some(ruta.clone());
            }
        }
        V::Arr(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                ruta.push(i.to_string());
                escribir(x, out, ruta, primero);
                ruta.pop();
            }
            out.push(']');
        }
        V::Obj(ms) => {
            out.push('{');
            for (i, (k, x)) in ms.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push(':');
                ruta.push(k.clone());
                escribir(x, out, ruta, primero);
                ruta.pop();
            }
            out.push('}');
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// El puntero es el del primer número negativo en orden de documento
    /// (RFC 6901), recortado al prefijo de segmentos más largo que cabe en
    /// [`POINTER_MAX`] bytes.
    #[test]
    fn puntero_del_primer_negativo(v in valor()) {
        let a = json(NO_NEGATIVOS);
        let (mut p, mut primero) = (String::new(), None);
        escribir(&v, &mut p, &mut Vec::new(), &mut primero);
        let r = correr(&a, p.as_bytes());
        match primero {
            None => prop_assert!(r.valid, "{p}"),
            Some(segs) => {
                let mut esperado = String::new();
                let mut recortado = false;
                for s in &segs {
                    let e = format!("/{}", s.replace('~', "~0").replace('/', "~1"));
                    if esperado.len() + e.len() > POINTER_MAX {
                        recortado = true;
                        break;
                    }
                    esperado.push_str(&e);
                }
                prop_assert_eq!(r.pointer.as_str(), esperado.as_str(), "{}", p);
                prop_assert_eq!(r.pointer.truncated(), recortado);
            }
        }
    }
}
