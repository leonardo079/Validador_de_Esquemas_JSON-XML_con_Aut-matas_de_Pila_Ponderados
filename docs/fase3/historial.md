# Historial de la Fase 3 (análisis semántico del esquema)

Entradas cerradas (resueltas, decididas o sin efecto) que salieron de
[pendientes.md](pendientes.md) el 3 nov 2026, copiadas literalmente. Conservan su número
(§N), al que remiten el código y el resto de `docs/`. Lo abierto sigue en `pendientes.md`.

### 6. CI — SIN EFECTO (D32, 3 nov)

- **Permisos y versiones de las acciones** (SA-L5, anterior a la 4.1): los flujos de
  `.github/workflows/` no tienen bloque `permissions: contents: read`, y las acciones
  (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`…)
  están fijadas por etiqueta, no por SHA.
- **Paso de release** (`.github/workflows/ci.yml`, `cargo test -p semantic --release`, que
  corre las salvaguardas de §2): no se ha visto correr en GitHub; confirmarlo en el primer
  push.
- **Resuelto (D32, 3 nov):** ya no se usa GitHub Actions (GitHub solo es el remoto de los
  commits). Las salvaguardas de release de §2 se comprueban a mano con
  `cargo test -p semantic --release` cuando una tarea toque `semantic`; el resto de la
  verificación es local (`make lint`, `make test`, `make docs-check`).

### Observaciones abiertas de la revisión final de la 4.2

La 4.2 quedó aprobada (0 Critical, 0 High, 0 Important, 0 Medium). Estas observaciones
quedaron abiertas; 4.2-I2 es §15. La tarea 4.3, que toca la CLI y `crates/ir`, resolvió
§16-§21 (cada una conserva su texto original y una línea **Resuelto**).

### 16. 4.2-S1: argumentos con `--` según su posición — RESUELTO (tarea 4.3)

- **Resuelto:** las dos ramas de `--dot` usan la misma guarda que la de un argumento
  (`es_opcion`, `crates/cli/src/main.rs`): una opción desconocida antes o después de
  `--dot` da el mensaje de uso y el código 2. `no_puede_leer_o_argumentos_invalidos`
  prueba `ir --dot --json` e `ir --json --dot` (falla sin el arreglo).

- **Dónde:** `crates/cli/src/main.rs:68-76` (ramas de `ir` en `main`).
- **Estado:** la rama de 2 argumentos rechaza una ruta que empiece por `--`, pero las dos
  ramas con `--dot` solo excluyen `--dot`. Así `schemac ir --dot --json` y
  `schemac ir --json --dot` toman `--json` como ruta y dan «schemac: --json: No such file
  or directory»: el código es 2 igual, pero el mensaje no es el de uso.
- **Arreglo propuesto:** la misma guarda `!ruta.starts_with("--")` en las dos ramas de
  `--dot`, y añadir `["ir", "--dot", "--json"]` a `no_puede_leer_o_argumentos_invalidos`
  (`crates/cli/tests/ir.rs:276-285`).
- **Cuándo:** la próxima tarea que toque la CLI (`schemac`).

### 17. 4.2-S2: la prueba `fallo_interno_da_codigo_3` escribe el DOT en la salida de `cargo test` — RESUELTO (tarea 4.3)

- **Resuelto:** `ejecutar_ir` recibe los destinos (`out` y `err` como `&mut dyn Write`);
  `main` le pasa stdout y stderr, y las pruebas, búferes (`ir_en_bufer`), que además
  comprueban el contenido: el DOT de la bajada real y el error único de la rota.

- **Dónde:** `crates/cli/src/main.rs:334`.
- **Estado:** la prueba llama a `ejecutar_ir(&p, Salida::Dot, bajar)` con la bajada real, e
  `imprimir` escribe con `io::stdout().lock().write_all`, que la captura del arnés de
  pruebas no intercepta. El DOT aparece en la salida de `cargo test`: es ruido, no un error.
- **Arreglo propuesto:** que `ejecutar_ir` reciba el destino como `&mut impl io::Write` y
  la prueba le pase un `Vec<u8>`; o comprobar la rama real sin pasar por `imprimir`.
- **Cuándo:** la próxima tarea que toque la CLI.

### 18. 4.2-S3: el corpus de `tests/cases` se reserializa con `serde_json` — RESUELTO (tarea 4.3)

- **Resuelto:** `crates/semantic/tests/common/mod.rs` (`casos_json`,
  `texto_del_esquema`) recorta el **texto original** del valor `schema` de cada caso por el
  rango que da `schema_parser::parse` sobre el archivo del caso (sus campos son palabras
  clave fuera del subconjunto, que el parser conserva con su rango). Lo usan
  `corpus_baja_a_ir_verificada` (`lower.rs`), `esquemas_de_los_casos_pasan_el_meta_esquema`
  y `corpus_clasificado` (`corpus.rs`). No se activaron `preserve_order` ni
  `arbitrary_precision`: las features de `serde_json` se unifican en el espacio de trabajo y
  cambiarían el comportamiento de las demás pruebas. `casos_con_el_texto_original_del_esquema`
  comprueba el orden de las reglas de `valid-recursive-tree-01` y un lexema que `f64` no
  conserva.

- **Dónde:** `crates/semantic/tests/lower.rs:56-59` (`corpus_baja_a_ir_verificada`).
- **Estado:** los esquemas de `tests/cases` se reserializan con
  `serde_json::to_vec_pretty(&case["schema"])`. Sin las features `preserve_order` ni
  `arbitrary_precision`, `serde_json` ordena las claves y pasa los números por `f64`, así
  que la prueba pierde cobertura justo en el orden de las reglas, las reglas dobles y los
  lexemas exactos. (La prueba `corpus.rs` de la Fase 3 hace lo mismo, pero solo comprueba
  que el esquema pasa, no la forma de la IR.)
- **Arreglo propuesto:** activar `preserve_order` y `arbitrary_precision` en la
  dev-dependency `serde_json` de `semantic`, o sacar el texto en bruto del campo `schema`
  con el parser del proyecto.
- **Cuándo:** la tarea 4.3, que vuelve a recorrer el corpus para la clasificación.

### 20. 4.2-S4: orden de los `use` en `verify.rs` — RESUELTO (tarea 4.3)

- **Resuelto:** `core`/`std` juntos y `lexer::short` aparte, antes de los `use crate::…`.

- **Dónde:** `crates/ir/src/verify.rs:10-13`.
- **Estado:** `use lexer::short;` quedó entre `use core::fmt;` y
  `use std::collections::…`. Cosmético.
- **Arreglo propuesto:** agrupar los `use` de `core`/`std` y poner aparte los del proyecto.
- **Cuándo:** la próxima tarea que toque `crates/ir` (la 4.3 vive en `ir`).

### 21. 4.2-I1: `eprintln!` puede hacer pánico si falla stderr — RESUELTO (tarea 4.3)

- **Resuelto:** `schemac` no usa `eprintln!`: todos los diagnósticos pasan por `avisar`
  (`let _ = writeln!(err, …)`), que ignora el error de escritura.

- **Dónde:** los `eprintln!` de `crates/cli/src/main.rs`.
- **Estado:** harían pánico si la escritura en stderr fallara con un error distinto de
  `EBADF`; en el hilo principal eso sería un código 101. Es teórico.
- **Arreglo propuesto (opcional):** `let _ = writeln!(io::stderr(), …)`, como `imprimir`
  hace con stdout.
- **Cuándo:** la próxima tarea que toque la CLI, si se quiere.
