//! Ciclos no productivos (Fase 3, tercera tarea): qué recursión es válida
//! (a través de `properties`, `items`…), qué ciclos se rechazan (`$ref` y
//! aplicadores sobre la misma instancia), el mensaje con el camino, una
//! comparación con fuerza bruta y la profundidad (D7).

use std::path::Path;

use proptest::prelude::*;
use schema_parser::parse;
use semantic::{analyze, ErrorKind, SemanticError, FRAGMENT_BYTES};

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");

fn result(src: &str) -> Result<(), Vec<SemanticError>> {
    let s = parse(src.as_bytes()).unwrap_or_else(|e| panic!("{src}: {e}"));
    analyze(&s).map(|_| ())
}

fn ok(src: &str) {
    if let Err(e) = result(src) {
        panic!("{src}: {}", e[0]);
    }
}

fn cycles(src: &str) -> Vec<SemanticError> {
    let e = result(src).expect_err(src);
    for err in &e {
        assert_eq!(err.code(), "cycle", "{src}: {err}");
    }
    e
}

/// La recursión que consume entrada es válida: usa la pila del DPDA.
#[test]
fn recursion_productiva_valida() {
    let path = Path::new(TESTS_DIR).join("cases/valid/valid-recursive-tree-01.json");
    let case: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    ok(&serde_json::to_string(&case["schema"]).unwrap());

    for src in [
        // Por `properties`, `items`, `prefixItems`, `additionalProperties`.
        r##"{"properties": {"hijo": {"$ref": "#"}}}"##,
        r##"{"items": {"$ref": "#"}}"##,
        r##"{"prefixItems": [{"$ref": "#"}]}"##,
        r##"{"additionalProperties": {"$ref": "#"}}"##,
        // A → B → A con `properties` de por medio.
        r##"{"$defs": {
            "a": {"properties": {"b": {"$ref": "#/$defs/b"}}},
            "b": {"$ref": "#/$defs/a"}
        }, "$ref": "#/$defs/a"}"##,
        // Un aplicador en el mismo nivel y después una hija: productivo.
        r##"{"allOf": [{"properties": {"x": {"$ref": "#"}}}]}"##,
        r##"{"not": {"items": {"$ref": "#"}}}"##,
        r##"{"dependentSchemas": {"k": {"properties": {"k": {"$ref": "#"}}}}}"##,
        // `$defs` no aplica sus entradas: una definición que apunta a la raíz
        // no forma ciclo si la raíz no la usa.
        r##"{"$defs": {"a": {"$ref": "#"}}}"##,
        // Cadena de `$ref` sin ciclo y `$ref` con hermanos que consumen.
        r##"{"$ref": "#/$defs/a", "$defs": {"a": {"$ref": "#/$defs/b"}, "b": {"type": "string"}}}"##,
        r##"{"$defs": {"n": {"type": "object", "$ref": "#/$defs/m",
             "properties": {"h": {"$ref": "#/$defs/n"}}}, "m": true}}"##,
    ] {
        ok(src);
    }
}

/// Los esquemas circulares de `tests/symbols.rs` resuelven en la etapa de la
/// tabla, pero la etapa de ciclos los rechaza.
#[test]
fn circulares_de_la_tabla_se_rechazan() {
    let e = cycles(
        r##"{"$defs": {
            "a": { "$ref": "#/$defs/b" },
            "b": { "$ref": "#/$defs/a", "not": { "$ref": "#/$defs/b" } }
        }}"##,
    );
    // Una sola componente {a, b, b/not}: un error.
    assert_eq!(e.len(), 1);
    cycles(r##"{"$anchor": "raiz", "anyOf": [{"$ref": "#raiz"}]}"##);
}

#[test]
fn mensaje_con_el_camino_y_posicion() {
    let src =
        r##"{"$defs": {"a": {"$ref": "#/$defs/b"}, "b": {"allOf": [{"$ref": "#/$defs/a"}]}}}"##;
    let e = cycles(src);
    assert_eq!(e.len(), 1);
    // El primer `$ref` del ciclo en el texto.
    assert_eq!(
        e[0].column() as usize,
        src.find("\"#/$defs/b\"").unwrap() + 1
    );
    let msg = e[0].to_string();
    assert!(
        msg.ends_with(": #/$defs/a —$ref→ #/$defs/b —allOf→ #/$defs/b/allOf/0 —$ref→ #/$defs/a"),
        "{msg}"
    );
    let ErrorKind::NonProductiveCycle {
        start,
        edges,
        total,
    } = &e[0].kind
    else {
        panic!()
    };
    assert_eq!((start.as_str(), edges.len(), *total), ("#/$defs/a", 3, 3));
    assert_eq!(edges.last().map(|e| e.1.as_str()), Some("#/$defs/a"));
}

/// Hallazgo 1 de la auditoría (ronda 1): m autoaristas bajo una clave de
/// longitud L. Antes cada error guardaba los fragmentos enteros, Θ(m·L)
/// bytes (1,27 GB con L = 8·10⁴, m = 8·10³). Ahora cada fragmento guardado
/// tiene como mucho `FRAGMENT_BYTES` y el total es lineal en la entrada. Mide
/// tamaños, no tiempo (D1).
#[test]
fn errores_de_ciclo_de_tamano_lineal() {
    const L: usize = 20_000;
    const M: usize = 2_000;
    let defs: Vec<String> = (0..M)
        .map(|i| format!(r##""a{i}": {{"$anchor": "x{i}", "$ref": "#x{i}"}}"##))
        .collect();
    let src = format!(
        r#"{{"properties": {{"{}": {{"$defs": {{{}}}}}}}}}"#,
        "k".repeat(L),
        defs.join(", ")
    );
    let e = cycles(&src);
    assert_eq!(e.len(), M);
    let mut guardado = 0;
    let mut mensajes = 0;
    for err in &e {
        let ErrorKind::NonProductiveCycle { start, edges, .. } = &err.kind else {
            panic!()
        };
        assert!(start.len() <= FRAGMENT_BYTES, "{}", start.len());
        guardado += start.len();
        for (via, to) in edges {
            assert!(to.len() <= FRAGMENT_BYTES, "{}", to.len());
            guardado += via.len() + to.len();
        }
        mensajes += err.to_string().len();
    }
    // Con fragmentos enteros serían ≥ 2·M·L = 8·10⁷ bytes.
    assert!(
        guardado <= 4 * src.len(),
        "{guardado} bytes para {} de entrada",
        src.len()
    );
    assert!(
        mensajes <= 8 * src.len(),
        "{mensajes} bytes para {} de entrada",
        src.len()
    );
}

/// Hallazgo 3 de la auditoría (ronda 1): dos componentes con ciclo, la
/// primera alcanza a la segunda. La búsqueda del camino de la primera no debe
/// salir de su componente: si marcara como vistos los nodos de la segunda, el
/// camino de esta quedaría incompleto.
#[test]
fn componentes_encadenadas_con_su_camino_completo() {
    let src = r##"{"$defs": {
        "a": {"$ref": "#/$defs/b"},
        "b": {"allOf": [{"$ref": "#/$defs/a"}, {"$ref": "#/$defs/c"}]},
        "c": {"$ref": "#/$defs/d"},
        "d": {"$ref": "#/$defs/c"}
    }}"##;
    let e = cycles(src);
    let caminos: Vec<String> = e
        .iter()
        .map(|x| {
            let m = x.to_string();
            m[m.find("): #").unwrap() + 3..].to_owned()
        })
        .collect();
    assert_eq!(
        caminos,
        [
            "#/$defs/a —$ref→ #/$defs/b —allOf→ #/$defs/b/allOf/0 —$ref→ #/$defs/a",
            "#/$defs/c —$ref→ #/$defs/d —$ref→ #/$defs/c",
        ]
    );
}

fn cadena(n: usize, ciclo: bool) -> String {
    let mut defs: Vec<String> = (0..n)
        .map(|i| format!(r##""d{i}": {{"$ref": "#/$defs/d{}"}}"##, i + 1))
        .collect();
    let last = if ciclo {
        r##"{"$ref": "#/$defs/d0"}"##
    } else {
        "true"
    };
    defs.push(format!(r#""d{n}": {last}"#));
    format!(
        r##"{{"$ref": "#/$defs/d0", "$defs": {{{}}}}}"##,
        defs.join(", ")
    )
}

/// D7: una cadena de 3·10⁴ `$ref` sin ciclo y con ciclo, en un hilo de
/// 256 KiB (parser incluido: el esquema es plano). Un DFS recursivo sobre una
/// cadena así necesitaría varios MiB de pila. El mensaje de un ciclo largo se
/// recorta. (El costo es lineal: 2,5·10⁴, 5·10⁴ y 10⁵ definiciones tardan
/// 1,0 s, 1,8 s y 3,8 s en debug con `schemac check`, con ciclo o sin él.)
#[test]
fn cadena_larga_de_referencias() {
    std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            const N: usize = 30_000;
            ok(&cadena(N, false));
            let e = cycles(&cadena(N, true));
            assert_eq!(e.len(), 1);
            let msg = e[0].to_string();
            assert!(
                msg.contains(" … —$ref→ #/$defs/d0 (30001 aristas)"),
                "{msg}"
            );
            assert!(msg.len() < 1000, "mensaje de {} bytes", msg.len());
        })
        .unwrap()
        .join()
        .unwrap();
}

/// D7: un ciclo de 256 aristas a la profundidad máxima del parser (`not`
/// anidados con `$ref` a la raíz en el fondo), en un hilo de 2 MiB.
#[test]
fn ciclo_a_la_profundidad_maxima() {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(|| {
            let d = schema_parser::MAX_DEPTH_LIMIT as usize - 1;
            let src = format!(
                "{}{{\"$ref\": \"#\"}}{}",
                r#"{"not":"#.repeat(d),
                "}".repeat(d)
            );
            let e = cycles(&src);
            let ErrorKind::NonProductiveCycle { total, .. } = &e[0].kind else {
                panic!()
            };
            assert_eq!(*total, d + 1);
            // Con `items` en lugar de `not`, la misma recursión es válida.
            ok(&format!(
                "{}{{\"$ref\": \"#\"}}{}",
                r#"{"items":"#.repeat(d),
                "}".repeat(d)
            ));
        })
        .unwrap()
        .join()
        .unwrap();
}

// ------------------------------------------------------------ propiedades ---

/// Referencia de una definición a otra, envuelta en una palabra clave que
/// consume o no.
#[derive(Clone, Copy, Debug)]
enum Envoltura {
    Directa,
    AllOf,
    Not,
    Properties,
    Items,
}

impl Envoltura {
    fn consume(self) -> bool {
        matches!(self, Envoltura::Properties | Envoltura::Items)
    }

    fn json(self, j: usize) -> String {
        let r = format!(r##"{{"$ref": "#/$defs/d{j}"}}"##);
        match self {
            Envoltura::Directa => r,
            Envoltura::AllOf => format!(r#"{{"allOf": [{r}]}}"#),
            Envoltura::Not => format!(r#"{{"not": {r}}}"#),
            Envoltura::Properties => format!(r#"{{"properties": {{"p": {r}}}}}"#),
            Envoltura::Items => format!(r#"{{"items": {r}}}"#),
        }
    }
}

fn envoltura() -> impl Strategy<Value = Envoltura> {
    prop_oneof![
        Just(Envoltura::Directa),
        Just(Envoltura::AllOf),
        Just(Envoltura::Not),
        Just(Envoltura::Properties),
        Just(Envoltura::Items),
    ]
}

/// Componentes con ciclo del grafo de definiciones restringido a las aristas
/// que no consumen, por fuerza bruta (clausura transitiva).
fn componentes_con_ciclo(n: usize, aristas: &[(usize, usize, Envoltura)]) -> usize {
    let mut r = vec![vec![false; n]; n];
    for &(i, j, e) in aristas {
        if !e.consume() {
            r[i][j] = true;
        }
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                if r[i][k] && r[k][j] {
                    r[i][j] = true;
                }
            }
        }
    }
    // Representante de cada componente con ciclo: el menor nodo de ella.
    (0..n)
        .filter(|&i| r[i][i] && (0..i).all(|j| !(r[i][j] && r[j][i])))
        .count()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// El número de errores `cycle` es el número de componentes con ciclo
    /// del grafo de definiciones sin las aristas que consumen.
    #[test]
    fn coincide_con_fuerza_bruta(
        (n, aristas) in (1usize..6).prop_flat_map(|n| {
            (Just(n), prop::collection::vec((0..n, 0..n, envoltura()), 0..10))
        })
    ) {
        let defs: Vec<String> = (0..n)
            .map(|i| {
                let mias: Vec<String> = aristas
                    .iter()
                    .filter(|a| a.0 == i)
                    .map(|&(_, j, e)| e.json(j))
                    .collect();
                if mias.is_empty() {
                    format!(r#""d{i}": {{}}"#)
                } else {
                    format!(r#""d{i}": {{"allOf": [{}]}}"#, mias.join(", "))
                }
            })
            .collect();
        let src = format!(r#"{{"$defs": {{{}}}}}"#, defs.join(", "));
        let esperado = componentes_con_ciclo(n, &aristas);
        match result(&src) {
            Ok(()) => prop_assert_eq!(esperado, 0, "{}", src),
            Err(e) => {
                prop_assert!(e.iter().all(|x| x.code() == "cycle"));
                prop_assert_eq!(e.len(), esperado, "{}", src);
            }
        }
    }
}
