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

/// Número de columnas distintas de una tabla: cuántas clases de bytes
/// distingue la función `col(clase)` (la columna completa sobre los estados).
/// Lo usan las pruebas que fijan las cifras de `docs/fase1/lexico.md` §1.
#[cfg(test)]
pub(crate) fn distintas<T: Ord>(num_classes: usize, col: impl Fn(usize) -> T) -> usize {
    extern crate std;
    let mut v: std::vec::Vec<T> = (0..num_classes).map(col).collect();
    v.sort();
    v.dedup();
    v.len()
}

/// Falla si `docs/fase1/lexico.md` no contiene `fila` tal cual. Así las cifras
/// de estados y clases del documento no son manuales: si cambia un AFD, la
/// prueba que calcula la fila desde las tablas deja de encontrarla.
#[cfg(test)]
pub(crate) fn en_lexico_md(fila: &str) {
    const LEXICO: &str = include_str!("../../../docs/fase1/lexico.md");
    assert!(
        LEXICO.contains(fila),
        "docs/fase1/lexico.md no contiene la fila `{fila}`: actualizar el documento"
    );
}
