//! DTD.1: propiedades de la tabla del AFD y cifras de `docs/fase1/lexico-dtd.md`.

use dtd::lexer::{
    is_accepting, A_ERR, CLASS, CLASS_NAMES, NUM_CLASSES, NUM_STATES, STATE_NAMES, S_ERR, S_TOP,
    TABLE,
};

const LEXICO: &str = include_str!("../../../docs/fase1/lexico-dtd.md");
const DOT: &str = include_str!("../../../docs/fase1/afd-dtd.dot");

/// Clases distintas: columnas diferentes de la tabla.
fn clases_distintas() -> usize {
    let mut cols: Vec<Vec<(u8, u8)>> = (0..NUM_CLASSES)
        .map(|c| {
            (0..NUM_STATES)
                .map(|s| (TABLE[s][c].next, TABLE[s][c].action))
                .collect()
        })
        .collect();
    cols.sort();
    cols.dedup();
    cols.len()
}

#[test]
fn el_documento_cita_las_cifras_de_la_tabla() {
    let fila = format!(
        "| DTD | {} | {} | {} |",
        NUM_STATES - 1,
        NUM_CLASSES,
        clases_distintas()
    );
    assert!(
        LEXICO.contains(&fila),
        "docs/fase1/lexico-dtd.md no contiene la fila `{fila}`: actualizar el documento"
    );
}

#[test]
fn el_diagrama_versionado_es_el_de_la_tabla() {
    let mut s = String::new();
    dtd::lexer::write_dot(&mut s).unwrap();
    assert_eq!(s, DOT, "docs/fase1/afd-dtd.dot desactualizado: `make docs`");
}

#[test]
fn el_estado_de_error_es_un_sumidero_y_solo_acepta_entre_declaraciones() {
    for t in &TABLE[S_ERR] {
        assert_eq!(usize::from(t.next), S_ERR);
        assert_eq!(t.action, A_ERR);
    }
    for s in 0..NUM_STATES {
        assert_eq!(is_accepting(s), s == S_TOP, "{}", STATE_NAMES[s]);
        for c in 0..NUM_CLASSES {
            let t = TABLE[s][c];
            assert!(usize::from(t.next) < NUM_STATES);
            // Ir al error y la acción de error van siempre juntos.
            assert_eq!(
                usize::from(t.next) == S_ERR,
                t.action == A_ERR,
                "{} × {}",
                STATE_NAMES[s],
                CLASS_NAMES[c]
            );
        }
    }
}

#[test]
fn cada_byte_tiene_una_clase_valida() {
    assert!(CLASS.iter().all(|&c| usize::from(c) < NUM_CLASSES));
    // Todas las clases se usan.
    for (c, nombre) in CLASS_NAMES.iter().enumerate() {
        assert!(
            CLASS.iter().any(|&x| usize::from(x) == c),
            "clase sin bytes: {nombre}"
        );
    }
}
