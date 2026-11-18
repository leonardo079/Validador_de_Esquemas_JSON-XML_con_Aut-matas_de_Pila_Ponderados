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

use runtime_rs::{Flow, JsonValidator, Mode, Options};

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
    let mut mem = vec![0u64; a.stack_words(64)];
    for &p in payloads {
        for mode in [Mode::Total, Mode::EarlyFail] {
            for chunk in [1, 7, 4096] {
                let antes = reservas();
                let mut v = JsonValidator::new(a.tables(), &Options { mode }, &mut mem);
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
    // cerrar la cadena en cada uno de los 2 estados de su trie (vacío).
    assert_eq!(a.tables().checks.len(), 6 * 200 + 2 * 200);
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
