//! Ayudantes para construir en tiempo de compilación las tablas de los AFD
//! con salida (máquinas de Mealy) de los tokenizadores.

/// Entrada de la tabla: estado siguiente y acción a ejecutar.
#[derive(Clone, Copy)]
pub(crate) struct Tr {
    pub next: u8,
    pub action: u8,
}

pub(crate) const fn tr(next: usize, action: u8) -> Tr {
    Tr {
        next: next as u8,
        action,
    }
}

/// Asigna `x` a las transiciones de `s` con las clases indicadas.
pub(crate) const fn set<const S: usize, const C: usize>(
    t: &mut [[Tr; C]; S],
    s: usize,
    classes: &[usize],
    x: Tr,
) {
    let mut i = 0;
    while i < classes.len() {
        t[s][classes[i]] = x;
        i += 1;
    }
}

/// Asigna `x` a todas las transiciones de `s`.
pub(crate) const fn fill<const S: usize, const C: usize>(t: &mut [[Tr; C]; S], s: usize, x: Tr) {
    let mut c = 0;
    while c < C {
        t[s][c] = x;
        c += 1;
    }
}
