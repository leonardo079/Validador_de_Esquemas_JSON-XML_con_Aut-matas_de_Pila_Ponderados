# Pendientes de la Fase 2 (parser de esquemas)

Observaciones que dejaron abiertas las rondas de auditoría del parser LL(1)
de `crates/schema-parser`. Las decisiones de diseño viven en el «Registro de
decisiones» del plan (D1–D9); aquí solo se remite a ellas.

## Resueltos

### 1. Prueba de rendimiento que depende del reloj — RESUELTO (fuera de la CI)

- **Resuelto:** `muchas_claves_en_tiempo_lineal` está marcada `#[ignore = "…"]` y ya no corre en `cargo test` ni en la CI. Decisión del plan: las pruebas que corren en la CI deben ser deterministas, y las mediciones de tiempo van al crate `bench` (criterion), no a `cargo test`. La prueba conserva su método (razón t(4N)/t(N) < 10 con N = 10⁴ en `properties`, `const`, `$defs` y `dependentRequired`, lotes calibrados de ≥ ~50 ms, mínimo de 3 muestras intercaladas y hasta 3 intentos) y se corre a mano, sin carga y en release: `cargo test -p schema-parser --release --test parser -- --ignored muchas_claves_en_tiempo_lineal`. Sin carga detecta la búsqueda cuadrática de forma estable (26,4–27,2 en los tres intentos, release).
- **Parte determinista:** la comprobación de que una clave repetida al final de un objeto de 4·10⁴ claves se reporta en su offset exacto se separó en `clave_repetida_al_final_de_objeto_grande`, que corre siempre.
- **Por qué no corre en la CI (mide tiempo de pared):** con la CPU sobresuscrita (32 `yes` en 8 núcleos) las razones del parser lineal fueron de 1,1 a 15,9 (falsos fallos) y, con la regresión, el auditor midió 8,99 en un intento (falso aprobado). Los reintentos reducen lo primero a costa de lo segundo.
- **Descartado:** medir tiempo de CPU del hilo con `clock_gettime`/`libc` (el workspace tiene `unsafe_code = "deny"` y la CI corre también en Windows). Una comprobación determinista que cuente operaciones (comparaciones de claves) exigiría instrumentar `json_schema.rs` con un contador expuesto solo para pruebas, y aun así una regresión que cambie la estructura de claves podría no actualizar el contador; no se consideró barata.
- **Descartado también:** leer el tiempo de CPU del hilo de `/proc/thread-self/schedstat` (sin `unsafe` ni dependencias). Se probó y se retiró: solo existe en Linux y sigue siendo una medición de tiempo dentro de `cargo test`, contra la decisión del plan (las mediciones de tiempo van a `bench`). Con carga sí era estable (lineal 4,1–5,6; cuadrática 12,6–12,8), por si sirve como referencia para el benchmark.
- **Queda abierto:** sin la prueba en la CI, una regresión cuadrática en la detección de claves repetidas solo se detecta corriendo la prueba ignorada o con el benchmark del crate `bench` (Fase 6, criterion), que todavía no existe. Cuando exista, conviene añadirle objetos de N y 4N claves.
- **Dónde:** `crates/schema-parser/tests/parser.rs`, prueba `muchas_claves_en_tiempo_lineal`.
- **Problema (original):** exigía un tope absoluto de 10 s; en una CI muy cargada
  podía fallar sin que hubiera un error real.
- **Corrección propuesta originalmente:** medir el tiempo con N y con 4N claves y
  comprobar que la razón es menor que 8. La segunda auditoría mostró que una sola
  estimación no aguanta carga alta; de ahí los lotes, el umbral 10 y los reintentos.

### 2. Claves repetidas dentro de `const` y `enum` — DECIDIDO (D2)

- **Decisión (D2 del plan):** las claves repetidas se rechazan en cualquier objeto del
  esquema, incluidos los valores literales de `const`/`enum`, con el error
  `duplicate-key` en la segunda aparición. Es lo que ya hacía el parser; no cambia código.
- **Justificación:** el PDF del curso lista «claves duplicadas» como caso **malformado**,
  y en los payloads el validador generado también las rechaza (`docs/00-alcance.md` §1).
  Que la RFC 8259 solo diga que los nombres *deberían* ser únicos no obliga a aceptarlos:
  el proyecto aplica la misma regla al esquema y al payload. Ver `gramatica-esquema.md`
  §4 y el caso `tests/schemas/parse-error/clave-repetida-en-const.json`.

### 3. «AST común» con XSD — DECIDIDO (D3)

- **Decisión (D3 del plan):** cada lenguaje tiene su propio AST (JSON Schema y XSD) y los
  dos bajan a una IR común. El parser XSD no reutiliza el AST de JSON Schema: la Fase 3
  (meta-esquema, `$ref`/`$defs`) es específica de JSON Schema, y XSD tiene orden de
  `sequence`, atributos frente a elementos y `minOccurs`/`maxOccurs`.
- **Hecho:** la documentación de `crates/schema-parser/src/lib.rs` y `docs/00-alcance.md`
  §3 y §5 ya no hablan de un «AST común». Queda acordar con la Persona B la forma de la
  IR antes de empezar el parser XSD (D3).

### 4. Revisar el manifiesto de errores — HECHO

- **Dónde:** `tests/schemas/errores.json`.
- **Revisión:** se verificaron a mano las 28 entradas (todas), con un script propio que
  no usa el parser: decodifica cada archivo de `parse-error/`, quita el BOM si lo hay,
  cuenta líneas por `\n` y columnas en puntos de código, y marca con `^` la posición
  declarada para comprobar a ojo que cae en el token que provoca el error (la segunda
  clave repetida, el `{` número 257 de `profundidad.json` en la columna 7·256 + 1 = 1793,
  el fin de archivo en `vacio`, `fin-inesperado` y `cadena-sin-cerrar`, etc.). Ninguna
  estaba mal; el manifiesto no cambió.

### 7. Palabras clave fuera del subconjunto — RESUELTO (Fase 3, D6)

- **Antes:** el parser las acepta y las deja en el AST como `Kw::Unsupported`, y nada
  las rechazaba: un esquema con `if`/`then` pasaba sin error.
- **Resuelto:** el parser sigue conservándolas (así puede informar todas, no solo la
  primera), y la validación contra el meta-esquema de la Fase 3
  (`semantic::check_meta_schema`) rechaza cada una con el código `unsupported`, en la
  posición de su nombre, con su categoría y el motivo (D6). No hay modo que las ignore.
  Ver [../fase3/meta-esquema.md](../fase3/meta-esquema.md) §5.
- **Corpus:** `tests/schemas/parse-ok/fuera-del-subconjunto.json` sigue en `parse-ok`
  (prueba el parser) y la Fase 3 lo rechaza con tres errores fijados en
  `crates/semantic/tests/corpus.rs`.

## Vigilar más adelante

### 5. Margen de pila en debug

- **Estado:** con profundidad 256, el parser usa unos 1,34 MiB de los 2 MiB de pila de
  un hilo en debug (unos 259 KiB en release). La prueba `profundidad_maxima` lo vigila.
- **Cuándo actuar:** si esa prueba empieza a fallar, o si la Fase 3 añade marcos o ramas
  a la recursión. (La validación contra el meta-esquema de la Fase 3 no añade nada: recorre
  el AST con una pila explícita, sin recursión; `crates/semantic/tests/meta.rs`,
  `profundidad_maxima` y `ast_muy_profundo_no_desborda`.) La solución robusta es una pila explícita o `stacker`, que sería una
  dependencia nueva.
- **Tope configurable:** `parse_with_max_depth` recorta la profundidad pedida a
  `MAX_DEPTH_LIMIT` = 256 (igual al valor por defecto), así que ningún valor del llamador
  puede llevar la recursión más allá del margen medido. Subir ese tope exige antes la pila
  explícita; ver `gramatica-esquema.md` §6.
- **Hilos con menos de 2 MiB:** la garantía solo vale en hilos con ≥ 2 MiB de pila. El
  hilo principal de Windows tiene 1 MiB, y ahí `parse(anidado(256))` en `dev` desborda
  (simulado en Linux con un hilo de 1 MiB; en release pasa). `schemac check` (Fase 3) ya
  cumple la regla: llama al parser y a la Fase 3 en un hilo con 8 MiB de pila explícita
  (`crates/cli/src/main.rs`, `STACK_SIZE`). El servidor aún no llama al parser, y libtest
  corre cada prueba en un hilo propio de 2 MiB. **Cuando el servidor (Fase 7) integre el
  parser, debe llamarlo dentro de un hilo creado con `std::thread::Builder::new().stack_size(…)` de al
  menos 2 MiB (o usar la pila explícita), nunca directamente en el hilo principal ni en un
  hilo de tamaño desconocido** (regla D7 del plan).

### 6. Límite de tamaño del esquema

- **Estado:** cada clave se copia a la tabla de claves vistas, y el parser no tiene límite
  de tamaño del esquema ni de claves por objeto. **`schemac` sí tiene un tope** desde la
  tarea 4.2: lee como mucho **16 MiB** y solo archivos regulares; pasarlo es un error de
  entrada con código 2 ([00-alcance.md](../00-alcance.md) §4,
  [fase3/bajada.md](../fase3/bajada.md) §5).
- **Cuándo actuar:** el tope del **servidor** (tamaño máximo de subida) sigue pendiente
  para la aplicación web (Fase 7). Va en el servidor, no en el parser, y se fija junto con
  el `RLIMIT_AS` de cada compilación ([fase3/pendientes.md](../fase3/pendientes.md) §1 y
  §2: la compilación hasta la IR usa ≈ 143 veces el tamaño del esquema).
