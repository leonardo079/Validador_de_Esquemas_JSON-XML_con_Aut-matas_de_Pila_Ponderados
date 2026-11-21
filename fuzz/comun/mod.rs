//! Lo común a los objetivos de fuzzing (F6.4, `docs/fase6/fuzzing.md`): la
//! tubería esquema → M en un hilo de 8 MiB (D7) y las propiedades que debe
//! cumplir todo reporte.
#![allow(dead_code)]

use automaton::{build, validate, Automaton, ErrorKind, Mode, Report, POINTER_MAX};
use ir::VerifiedIr;

/// Ejecuta `f` en un hilo de 8 MiB de pila (D7); un pánico dentro se
/// propaga (lo ve libFuzzer).
pub fn en_hilo<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    std::thread::scope(|s| {
        let h = std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn_scoped(s, f)
            .unwrap();
        match h.join() {
            Ok(r) => r,
            Err(p) => std::panic::resume_unwind(p),
        }
    })
}

/// La IR de un JSON Schema, o `None` si no pasa las Fases 2-3.
pub fn ir_json(texto: &[u8]) -> Option<VerifiedIr> {
    let s = schema_parser::parse(texto).ok()?;
    let t = semantic::analyze(&s).ok()?;
    semantic::lower(&t).ok()
}

/// La IR de un XSD, o `None` si no pasa las Fases 2-3.
pub fn ir_xsd(texto: &[u8]) -> Option<VerifiedIr> {
    let s = xsd::parse(texto).ok()?;
    let a = xsd::analyze(&s).ok()?;
    xsd::lower(&a).ok()
}

/// M de un esquema que debe compilar (los fijos de los objetivos).
pub fn m(ir: Option<VerifiedIr>) -> Automaton {
    build(&ir.expect("el esquema fijo no compila")).expect("el esquema fijo no construye M")
}

/// Las propiedades de todo reporte de `a` sobre `p` (F4.6, F6.1, D1): el
/// mismo en bloques de 1, 7 y 4096 bytes; el fallo temprano da el mismo
/// veredicto y, sin error de peso ∞, la misma primera violación con peso 1
/// (con uno, o el mismo error o una violación anterior a él);
/// nunca un error interno; `valid` ⇔ sin primer error; el puntero cabe y
/// es vacío o empieza por `/`. Devuelve el reporte.
pub fn propiedades(a: &Automaton, p: &[u8]) -> Report {
    let r = validate(a, p, Mode::Total, 4096);
    for chunk in [1, 7] {
        assert_eq!(validate(a, p, Mode::Total, chunk), r, "bloques de {chunk}");
    }
    assert_ne!(r.error_kind, Some(ErrorKind::Internal), "{r:?}");
    assert_eq!(r.valid, r.first.is_none(), "{r:?}");
    assert_eq!(r.valid, r.error_kind.is_none(), "{r:?}");
    let ptr = r.pointer.as_bytes();
    assert!(ptr.len() <= POINTER_MAX);
    assert!(ptr.is_empty() || ptr[0] == b'/', "{r:?}");
    let e = validate(a, p, Mode::EarlyFail, 7);
    assert_eq!(e.valid, r.valid, "fallo temprano: {e:?} / {r:?}");
    if e.error_kind != r.error_kind {
        assert_eq!(e.error_kind, Some(ErrorKind::Schema), "{e:?} / {r:?}");
        assert!(
            e.first.unwrap().offset <= r.first.unwrap().offset,
            "{e:?} / {r:?}"
        );
    } else if r.error_kind != Some(ErrorKind::Schema) {
        assert_eq!(e, r, "fallo temprano");
    }
    if r.error_kind == Some(ErrorKind::Schema) {
        assert_eq!(e.violations.get(), Some(1), "{e:?}");
        assert_eq!(e.first, r.first);
        assert_eq!(e.pointer, r.pointer);
    }
    r
}
