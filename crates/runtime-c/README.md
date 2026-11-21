# runtime-c

Núcleo C11 del validador generado (F5.2): `sv_nucleo.h` y `sv_nucleo.c`, port función por función del
ejecutor de `crates/runtime-rs` y de los lexers de `crates/lexer`. No es un crate de Cargo: el emisor
(`codegen::c::emit`) copia estos archivos junto a `tablas.c`, `validador.h`/`.c`, `sv_lexico.h`/`.c` (tablas de
los AFD volcadas del crate `lexer`) y `main.c`.

Requisitos: `-std=c11 -Wall -Wextra -Werror -pedantic -O2` sin avisos en gcc y clang, sin `malloc` (lo vigila
`nm -u` en `crates/codegen/tests/c.rs`), y el mismo reporte que Rust en todo el corpus (prueba diferencial,
`crates/codegen/tests/diferencial.rs`). `include/schema_validator.h` es el marcador antiguo, sin uso.
