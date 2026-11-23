//! DTD arbitrarias (DTD.5, D86): como `esquema_xsd`, con el frontal DTD.
//! Ningún texto puede hacer fallar al lexer, al parser, al análisis ni a la
//! bajada (solo errores), y lo prohibido por seguridad (`<!ENTITY`,
//! `<!DOCTYPE`, `<![`, `%p;`) nunca llega a la IR. Si compila, M cumple las
//! propiedades de siempre con unos payloads fijos.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../comun/mod.rs"]
mod comun;

const PAYLOADS: &[&[u8]] = &[
    b"<a/>",
    b"<a>1</a>",
    b"<a b='1'><c/></a>",
    b"<pedido id='7'><cliente>A</cliente><efectivo/><item sku='x'>2</item></pedido>",
    b"<!DOCTYPE a><a/>",
];

fuzz_target!(|texto: &[u8]| {
    comun::en_hilo(|| {
        let Some(ir) = comun::ir_dtd(texto) else {
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
