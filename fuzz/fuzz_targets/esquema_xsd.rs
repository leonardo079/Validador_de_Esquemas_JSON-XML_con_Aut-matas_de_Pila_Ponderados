//! XSD arbitrarios (F6.4): como `esquema_json`, con el frontal XSD
//! (D8: sin DTD ni entidades externas en el esquema).
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../comun/mod.rs"]
mod comun;

const PAYLOADS: &[&[u8]] = &[
    b"<a/>",
    b"<a>1</a>",
    b"<a b='1'><c/></a>",
    b"<r a='x'><d>1</d><s>t</s></r>",
    b"<pedido><cliente>A</cliente></pedido>",
];

fuzz_target!(|texto: &[u8]| {
    comun::en_hilo(|| {
        let Some(ir) = comun::ir_xsd(texto) else {
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
