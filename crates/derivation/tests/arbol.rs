//! Pruebas del árbol de derivación (F5.5, D69). Deterministas (D1).

use derivation::{
    a_dot, a_json, derivar, Arbol, Clase, Error, Formato, MAX_NODOS, MAX_PROFUNDIDAD,
};

fn arbol(p: &str) -> Arbol {
    derivar(p.as_bytes(), MAX_NODOS).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// Producciones de `docs/fase2/json.ll1` y `xml.ll1` (lado derecho; `[]` = ε).
fn producciones(nt: &str) -> Vec<Vec<&'static str>> {
    let v: &[&[&str]] = match nt {
        "value" => &[
            &["object"],
            &["array"],
            &["STRING"],
            &["NUMBER"],
            &["true"],
            &["false"],
            &["null"],
        ],
        "object" => &[&["{", "members", "}"]],
        "members" => &[&["pair", "members_tail"], &[]],
        "members_tail" => &[&[",", "pair", "members_tail"], &[]],
        "pair" => &[&["STRING", ":", "value"]],
        "array" => &[&["[", "elements", "]"]],
        "elements" => &[&["value", "elements_tail"], &[]],
        "elements_tail" => &[&[",", "value", "elements_tail"], &[]],
        "document" => &[&["misc", "element", "misc"]],
        "misc" => &[&["TEXT", "misc"], &["PI", "misc"], &[]],
        "element" => &[&["STAG", "attrs", "element_rest"]],
        "element_rest" => &[&["/>"], &[">", "content", "ETAG"]],
        "attrs" => &[&["ATTR", "attrs"], &[]],
        "content" => &[
            &["TEXT", "content"],
            &["PI", "content"],
            &["element", "content"],
            &[],
        ],
        otro => panic!("no terminal desconocido {otro}"),
    };
    v.iter().map(|r| r.to_vec()).collect()
}

/// Cada no terminal se expande con una producción de la gramática, y la
/// frontera (los terminales en orden) cubre el payload sin huecos salvo
/// espacios.
fn comprobar(a: &Arbol, p: &[u8]) {
    let mut frontera = Vec::new();
    for n in &a.nodos {
        match n.clase {
            Clase::NoTerminal => {
                let hijos: Vec<&str> = n
                    .hijos
                    .iter()
                    .map(|&h| &a.nodos[h])
                    .filter(|h| h.clase != Clase::Epsilon)
                    .map(|h| h.simbolo)
                    .collect();
                assert!(
                    producciones(n.simbolo).contains(&hijos),
                    "{} -> {hijos:?}",
                    n.simbolo
                );
                if n.hijos.iter().any(|&h| a.nodos[h].clase == Clase::Epsilon) {
                    assert_eq!(n.hijos.len(), 1, "ε solo en una producción vacía");
                }
            }
            Clase::Terminal => frontera.push((n.inicio as usize, n.fin as usize)),
            Clase::Epsilon => assert!(n.hijos.is_empty()),
        }
    }
    // En preorden los terminales salen de izquierda a derecha.
    let mut fin = 0;
    for &(i, f) in &frontera {
        assert!(i >= fin && f > i, "frontera desordenada");
        let hueco = &p[fin..i];
        assert!(
            hueco_valido(a.formato, hueco),
            "hueco {:?}",
            String::from_utf8_lossy(hueco)
        );
        fin = f;
    }
    assert!(p[fin..].iter().all(|b| b" \t\r\n".contains(b)));
}

#[test]
fn json_sigue_la_gramatica_y_cubre_el_payload() {
    for p in [
        "1",
        "\"x\"",
        "[]",
        "{}",
        "[1, [2, []], {\"a\": null}]",
        "{\"pedido\": 7, \"lineas\": [{\"sku\": \"ABC-123\", \"cantidad\": 2}], \"pagado\": true}",
        " \n{ \"a\" : -1.5e3 , \"b\" : [ false ] } ",
    ] {
        let a = arbol(p);
        assert_eq!(a.formato, Formato::Json);
        assert_eq!(a.nodos[0].simbolo, "value");
        comprobar(&a, p.as_bytes());
    }
}

#[test]
fn xml_sigue_la_gramatica_y_cubre_el_payload() {
    for p in [
        "<a/>",
        "<a></a>",
        "<?xml version=\"1.0\"?>\n<pedido id=\"7\"><linea sku=\"ABC-123\">2</linea><pagado/></pedido>\n",
        "<a x=\"1\" y='2'>t<b/>u<?pi z?></a>",
    ] {
        let a = arbol(p);
        assert_eq!(a.formato, Formato::Xml);
        assert_eq!(a.nodos[0].simbolo, "document");
        comprobar(&a, p.as_bytes());
    }
}

#[test]
fn punteros_json_de_cada_valor() {
    let a = arbol(r#"{"a": [10, {"b/c": true, "d~": null}], "": 1}"#);
    let mut v: Vec<(String, String)> = a
        .nodos
        .iter()
        .filter(|n| n.simbolo == "value")
        .map(|n| {
            let hoja = a.nodos[n.hijos[0]].clone();
            (
                n.puntero.clone().unwrap(),
                if hoja.clase == Clase::Terminal {
                    hoja.lexema
                } else {
                    hoja.simbolo.to_string()
                },
            )
        })
        .collect();
    v.sort();
    let esperado = [
        ("", "object"),
        ("/", "1"),
        ("/a", "array"),
        ("/a/0", "10"),
        ("/a/1", "object"),
        ("/a/1/b~1c", "true"),
        ("/a/1/d~0", "null"),
    ];
    let esperado: Vec<(String, String)> = esperado
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert_eq!(v, esperado);
}

#[test]
fn rangos_de_los_no_terminales() {
    let p = r#"{"a": [1, 2]}"#;
    let a = arbol(p);
    let arr = a.nodos.iter().find(|n| n.simbolo == "array").unwrap();
    assert_eq!(&p[arr.inicio as usize..arr.fin as usize], "[1, 2]");
    assert_eq!((a.nodos[0].inicio, a.nodos[0].fin), (0, p.len() as u64));
}

#[test]
fn errores_con_su_offset() {
    let e = |p: &str| derivar(p.as_bytes(), MAX_NODOS).unwrap_err();
    assert!(
        matches!(e("[01]"), Error::Lexico { offset: 2, .. }),
        "{:?}",
        e("[01]")
    );
    assert!(
        matches!(e("[1,]"), Error::Sintactico { offset: 3, .. }),
        "{:?}",
        e("[1,]")
    );
    assert!(matches!(
        e("{\"a\" 1}"),
        Error::Sintactico { offset: 5, .. }
    ));
    assert!(matches!(e("1 2"), Error::Sintactico { offset: 2, .. }));
    assert!(matches!(e(""), Error::Sintactico { offset: 0, .. }));
    assert!(
        matches!(e("<a></b>"), Error::Sintactico { offset: 3, .. }),
        "{:?}",
        e("<a></b>")
    );
    assert!(matches!(e("<a><b></a>"), Error::Sintactico { .. }));
    assert!(matches!(e("<a/><b/>"), Error::Sintactico { .. }));
    let msg = e("[1,]").to_string();
    assert!(msg.contains("byte 3") && msg.contains("value"), "{msg}");
}

#[test]
fn tope_de_nodos() {
    let p = format!("[{}]", vec!["1"; 1000].join(","));
    assert_eq!(derivar(p.as_bytes(), 100), Err(Error::Grande { max: 100 }));
    assert!(derivar(p.as_bytes(), 10_000).is_ok());
}

#[test]
fn anidamiento_profundo_sin_recursion() {
    // 256 niveles (el máximo del proyecto) dan árbol, en JSON y en XML; el
    // 257 da `Profundo` en su byte. Ni el parser ni las salidas usan
    // recursión (D7).
    let n = MAX_PROFUNDIDAD;
    let p = format!("{}{}", "[".repeat(n), "]".repeat(n));
    let a = derivar(p.as_bytes(), usize::MAX).unwrap();
    comprobar(&a, p.as_bytes());
    assert!(a_json(&a).starts_with("{\"name\":\"value\""));
    assert!(a_dot(&a, "x").ends_with("}\n"));
    let q = format!("{}{}", "[".repeat(n + 1), "]".repeat(n + 1));
    assert_eq!(
        derivar(q.as_bytes(), usize::MAX),
        Err(Error::Profundo {
            offset: n as u64,
            max: n
        })
    );
    let x = format!("{}{}", "<a>".repeat(n), "</a>".repeat(n));
    assert!(derivar(x.as_bytes(), usize::MAX).is_ok());
    let y = format!("{}{}", "<a>".repeat(n + 1), "</a>".repeat(n + 1));
    assert_eq!(
        derivar(y.as_bytes(), usize::MAX),
        Err(Error::Profundo {
            offset: 3 * n as u64,
            max: n
        })
    );
    // Mucho texto a lo ancho no tiene tope de profundidad: solo el de nodos.
    let ancho = format!("[{}]", vec!["[]"; 50_000].join(","));
    assert!(derivar(ancho.as_bytes(), usize::MAX).is_ok());
}

#[test]
fn json_para_d3_es_valido_y_jerarquico() {
    let p = r#"{"a": ["x\"y", 1]}"#;
    let a = arbol(p);
    let v: serde_json::Value = serde_json::from_str(&a_json(&a)).unwrap();
    // Recorrido iterativo: mismo número de nodos y mismos campos.
    let mut pila = vec![&v];
    let mut n = 0;
    while let Some(x) = pila.pop() {
        n += 1;
        assert!(
            x["name"].is_string()
                && x["kind"].is_string()
                && x["start"].is_u64()
                && x["end"].is_u64()
        );
        match x["kind"].as_str().unwrap() {
            "t" => assert!(x["lexeme"].is_string()),
            "eps" => assert_eq!(x["name"], "ε"),
            _ => {}
        }
        if let Some(h) = x.get("children") {
            pila.extend(h.as_array().unwrap());
        }
    }
    assert_eq!(n, a.nodos.len());
    assert_eq!(v["pointer"], "");
    assert!(a_json(&a).contains(r#""lexeme":"\"x\\\"y\"""#));
}

#[test]
fn dot_con_hijos_en_orden() {
    let a = arbol("[true]");
    let d = a_dot(&a, "t.json");
    assert!(d.contains("ordering=out"));
    assert!(d.contains("n0 [label=\"value\"]"));
    assert!(d.contains("label=\"true\""));
    assert!(d.contains("label=\"ε\""));
    assert_eq!(d.matches(" -> ").count(), a.nodos.len() - 1);
}

#[test]
fn lexema_recortado() {
    let largo = format!("\"{}\"", "x".repeat(200));
    let a = arbol(&largo);
    let t = a.nodos.iter().find(|n| n.simbolo == "STRING").unwrap();
    assert_eq!(t.lexema.chars().count(), derivation::MAX_LEXEMA);
    assert!(t.lexema.ends_with('…'));
    assert_eq!(t.fin - t.inicio, 202);
}

/// Todo payload `valid` del corpus (JSON y XML) tiene árbol.
#[test]
fn corpus_valido() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cases/valid");
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let ruta = e.unwrap().path();
        if ruta.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let caso: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&ruta).unwrap()).unwrap();
        let Some(p) = caso["payload"].as_str() else {
            continue;
        };
        let a =
            derivar(p.as_bytes(), usize::MAX).unwrap_or_else(|e| panic!("{}: {e}", ruta.display()));
        comprobar(&a, p.as_bytes());
        n += 1;
    }
    assert!(n >= 20, "{n}");
}

/// Entre dos terminales solo puede haber espacios, un BOM o, en XML, lo que
/// el lexer no convierte en token: comentarios y los delimitadores de CDATA.
fn hueco_valido(f: Formato, h: &[u8]) -> bool {
    let s = String::from_utf8_lossy(h);
    let s = s.trim_start_matches('\u{feff}');
    if f == Formato::Json {
        return s.chars().all(|c| " \t\r\n".contains(c));
    }
    let mut resto = s.trim();
    loop {
        if resto.is_empty() {
            return true;
        }
        if let Some(r) = resto.strip_prefix("<!--") {
            match r.find("-->") {
                Some(i) => resto = r[i + 3..].trim_start(),
                None => return false,
            }
        } else if let Some(r) = resto
            .strip_prefix("<![CDATA[")
            .or_else(|| resto.strip_prefix("]]>"))
        {
            resto = r.trim_start();
        } else {
            return false;
        }
    }
}
