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
//! Que el crate no use `alloc` lo garantiza su `#![no_std]` sin
//! `extern crate alloc` (no compilaría con un `Vec`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use runtime_rs::{Flow, JsonValidator, Mode, Options};

struct Contador;

thread_local! {
    /// Reservas de este hilo (las pruebas corren en hilos distintos).
    static RESERVAS: Cell<u64> = const { Cell::new(0) };
}

// La única excepción a `unsafe_code = "deny"` del workspace, y solo en este
// binario de prueba: `GlobalAlloc` es un rasgo `unsafe` por definición. Las
// tres funciones solo cuentan y delegan en `System` con los mismos
// argumentos, así que cumplen su contrato si `System` lo cumple.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Contador {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = RESERVAS.try_with(|r| r.set(r.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = RESERVAS.try_with(|r| r.set(r.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ASIGNADOR: Contador = Contador;

fn reservas() -> u64 {
    RESERVAS.with(Cell::get)
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
    let ir = ir_de(
        r##"{"$defs": {"n": {"type": "object", "properties": {"v": {"type": "integer"},
            "c": {"type": "array", "prefixItems": [true], "items": {"$ref": "#/$defs/n"}}},
            "required": ["v"], "dependentRequired": {"c": ["v"]},
            "additionalProperties": false}}, "$ref": "#/$defs/n"}"##,
    );
    let a = automaton::build(&ir).unwrap();
    let mut mem = vec![0u64; a.stack_words(64)];
    let payloads: [&[u8]; 6] = [
        br#"{"v": 1, "c": [0, {"v": 2.0}, {"v": 3, "c": []}]}"#,
        br#"{"v": 1.5, "x": {"y": ["\u00f1", true, null]}, "c": [1, {}]}"#,
        br#"{"v": 1, "v": 2}"#,
        br#"{"v": 1e99999, "c": [1, 2,]}"#,
        b"[\xff]",
        b"",
    ];
    for p in payloads {
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
