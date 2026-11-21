//! JSON Schemas arbitrarios (F6.4): las Fases 2-4 nunca entran en pánico
//! ni pasan la pila (D7) y, si construyen M, M cumple las propiedades de
//! [`comun::propiedades`] sobre unos payloads fijos. Cubre `$ref` cíclicos,
//! `pattern` y los presupuestos de construcción (F4-U6).
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../comun/mod.rs"]
mod comun;

const PAYLOADS: &[&[u8]] = &[
    b"{}",
    b"[]",
    b"0",
    b"-1.5e3",
    b"\"abc\"",
    b"null",
    b"true",
    br#"{"a":[1,{"b":null}],"c":"x"}"#,
    br#"[1,"a",[],{}]"#,
    br#"{"a":1,"a":2}"#,
];

fuzz_target!(|texto: &[u8]| {
    comun::en_hilo(|| {
        let Some(ir) = comun::ir_json(texto) else {
            return;
        };
        let Ok(m) = automaton::build(&ir) else {
            return;
        };
        for p in PAYLOADS {
            comun::propiedades(&m, p);
        }
    });
});
