//! F6.6 — Benchmark del parser de esquemas con objetos de N y 4N claves.
//!
//! Sustituye como detector de una regresión cuadrática a la prueba ignorada
//! `muchas_claves_en_tiempo_lineal` de `crates/schema-parser/tests/parser.rs`
//! (D1, hallazgo 2, `docs/fase2/pendientes.md` §1): las mediciones de tiempo no
//! van en `cargo test`, que debe ser determinista, sino aquí.
//!
//! Mide el análisis de un objeto con N = 10⁴ y con 4N claves en las cuatro
//! palabras clave cuyo objeto se comprueba contra claves repetidas (D2):
//! `properties`, `const`, `$defs` y `dependentRequired`. Con un parser lineal,
//! el tiempo por clave (la línea `thrpt` de criterion, en elementos por
//! segundo) es casi igual en N y en 4N, y t(4N)/t(N) ≈ 4; una búsqueda lineal
//! por clave lo dispara a ≈ 16 o más (26-28 medido en release con la versión
//! cuadrática). Se corre a mano, en una máquina sin carga:
//!
//! ```text
//! cargo bench -p bench --bench parser_claves
//! cargo bench -p bench --bench parser_claves -- --test   # una pasada, sin medir
//! ```
//!
//! Los informes quedan en `target/criterion/` (ignorado por git).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use schema_parser::parse;

/// N de la prueba ignorada: t(N) de milisegundos en release.
const N: usize = 10_000;

type Entrada = fn(usize) -> String;

/// Las cuatro palabras clave con un objeto de claves arbitrarias (las mismas
/// entradas que la prueba ignorada).
const CASOS: [(&str, Entrada); 4] = [
    ("properties", |i| format!("\"p{i}\": true")),
    ("const", |i| format!("\"p{i}\": 0")),
    ("$defs", |i| format!("\"d{i}\": {{}}")),
    ("dependentRequired", |i| format!("\"r{i}\": []")),
];

/// `{"<kw>": {<n> claves>}}`, como `objeto_con_claves` de la prueba.
fn objeto_con_claves(kw: &str, n: usize, entrada: Entrada) -> String {
    let claves: Vec<String> = (0..n).map(entrada).collect();
    format!("{{\"{kw}\": {{{}}}}}", claves.join(","))
}

fn claves(c: &mut Criterion) {
    for (kw, entrada) in CASOS {
        let mut grupo = c.benchmark_group(format!("claves/{kw}"));
        for n in [N, 4 * N] {
            let src = objeto_con_claves(kw, n, entrada);
            // Comprobación fuera de la medida: el esquema se acepta.
            assert!(
                parse(src.as_bytes()).is_ok(),
                "{kw} con {n} claves no se acepta"
            );
            grupo.throughput(Throughput::Elements(n as u64));
            grupo.bench_with_input(BenchmarkId::from_parameter(n), &src, |b, src| {
                b.iter(|| parse(black_box(src.as_bytes())).is_ok());
            });
        }
        grupo.finish();
    }
}

criterion_group! {
    name = benches;
    // Pocas muestras: cada análisis de 4N dura decenas de ms sin optimizar.
    config = Criterion::default().sample_size(20);
    targets = claves
}
criterion_main!(benches);
