# Tareas: semana 2, tarea 4 (IR común y clasificación)

Detalle y decisiones en `tasks/plan.md`. Una tarea por vez con `/tarea`; el
usuario hace el commit entre tareas.

## Tarea 4.0: Cerrar las observaciones de la tarea 3

**Descripción:** Corregir las observaciones BAJAS de la ronda 2 de la tarea 3
(`../Retomar - semana 2 tarea 3 ronda 2.md`) y las dos notas de la revisión
del coordinador, para que la IR empiece sobre una Fase 3 sin deuda.

**Criterios de aceptación:**
- [x] Un proptest y un caso fijo (`ñ` justo en el corte) fallan si `fragment_clipped` parte un escape `%XX`, `~0` o `~1`; la mutación del auditor (añadir byte a byte) los hace fallar
- [x] Fórmula de costo de `check_cycles` corregida a O(m·9·(h + 80)) en el código y en `docs/fase3/ciclos.md`; «truncar la lista de errores» registrado en `docs/` con el factor medido (~9× la entrada); cifra 0,39/0,41 s unificada
- [x] `debug_assert!` en las dos salidas anómalas de `path_in`; comentarios de `symbols.rs:18` y `error.rs:13-15` al día; estado de D7 en el plan general dice que `schemac` ya cumple
- [x] (Opcional) constante de `fragment_clipped` reducida sin `String` por carácter, con la medición antes y después de H20000

**Verificación:**
- [x] `make lint` y `make test`
- [x] `cargo test -p semantic` con la mutación aplicada a mano: falla

**Dependencias:** ninguna

**Archivos probables:** `crates/semantic/src/{symbols,cycles,error}.rs`, `crates/semantic/tests/symbols.rs`, `docs/fase3/ciclos.md`, plan general

**Tamaño:** S

## Tarea 4.1: Diseño de la IR común

**Descripción:** Crear el crate `crates/ir` con los tipos de la IR (grafo en
arena, restricciones tipadas, modelo de contenido con orden y ocurrencias,
atributo/elemento, ubicación de origen), su verificador de invariantes y la
teoría en `docs/fase3/ir.md`, incluidas las decisiones pendientes de
`docs/00-alcance.md` §5. Todavía no baja nada a ella: la expresividad se
prueba construyendo IR a mano.

**Criterios de aceptación:**
- [ ] `docs/fase3/ir.md` define formalmente la IR, sus invariantes y cómo se representa cada palabra clave de `00-alcance.md` §2 y cada construcción de §3.1; resuelve los tres puntos de §5 (que se quitan de ahí)
- [ ] Pruebas que construyen a mano la IR de un esquema JSON recursivo (`$ref` a la raíz por `properties`) y de un XSD con `sequence`, `choice`, atributo requerido y `maxOccurs="unbounded"`, y pasan `Ir::verify`; casos que rompen cada invariante fallan con un error claro
- [ ] Sin recursión (D7), sin `unsafe`, tamaño O(n) en número de subesquemas; el documento muestra de qué dato de la IR sale cada pieza de la construcción del DPDA de la Fase 4

**Verificación:**
- [ ] `make lint`, `make test`, `cargo doc -p ir --no-deps` sin avisos
- [ ] Revisión del usuario en el punto de control A

**Dependencias:** 4.0 (solo por orden de commits)

**Archivos probables:** `Cargo.toml`, `crates/ir/{Cargo.toml,src/lib.rs,src/verify.rs}`, `crates/ir/tests/forma.rs`, `docs/fase3/ir.md`, `docs/00-alcance.md` §5

**Tamaño:** M

## Punto de control A
- [ ] `make lint`, `make test`, `make docs-check` en verde
- [ ] El usuario aprueba la IR; se registra D13 (crate `ir`, grafo en arena) en el plan general

## Tarea 4.2: Bajada de JSON Schema a la IR y `schemac ir`

**Descripción:** A partir del AST y la `SymbolTable` ya verificados, construir
la IR de un esquema JSON (`$ref` como arista al nodo destino, booleanos,
anotaciones descartadas, números como lexema) y exponerla con
`schemac ir <esquema.json>` en JSON estable, en el mismo hilo con pila
explícita que `schemac check` (D7).

**Criterios de aceptación:**
- [ ] Todos los esquemas válidos del corpus (`tests/schemas/parse-ok`, salvo las 2 excepciones, y `tests/cases`) bajan a una IR que pasa `Ir::verify`; cada nodo conserva su `Span` y su puntero
- [ ] Un `$ref` no duplica su destino: el número de nodos es el de subesquemas; la recursión productiva queda como ciclo en el grafo; la salida es determinista (prueba dorada de al menos 3 esquemas)
- [ ] `schemac ir` da código 0 con la IR, 1 con los mismos errores que `check`, 2 si no puede leer; bajada iterativa, O(n)

**Verificación:**
- [ ] `make lint`, `make test`
- [ ] Manual: `cargo run -p cli -- ir tests/cases/valid/<uno>.json` muestra la IR

**Dependencias:** 4.1

**Archivos probables:** `crates/semantic/{Cargo.toml,src/lower.rs,src/lib.rs}`, `crates/semantic/tests/lower.rs`, `crates/cli/src/main.rs`, `tests/ir/*.json` (doradas)

**Tamaño:** M

## Tarea 4.3: Clasificación (a)/(b)/(c) y estrategia

**Descripción:** Módulo de `ir` que marca cada restricción y cada nodo con su
clase y su estrategia de implementación (transición, registro o contador,
producto acotado, determinización acotada, AFD, verificación diferida), según
`docs/00-alcance.md` §2 y §3; `schemac ir` lo muestra. Con esto se cumple el
criterio de terminado de la Fase 3 para JSON Schema.

**Criterios de aceptación:**
- [ ] Una sola tabla en el código, citada por `docs/fase3/clasificacion.md`, da clase y estrategia; una prueba con `match` exhaustivo comprueba que coincide con las columnas «Cat.» de `00-alcance.md` (incluidas las de XSD) y una prueba de la documentación comprueba que la tabla escrita es la del código
- [ ] La clase de un nodo es la máxima de sus restricciones y la de los nodos que aplica sobre la misma instancia; documentado y probado con `not`/`anyOf` sobre (a) y sobre (c)
- [ ] Todo el corpus sale clasificado; cada caso de `tests/cases/non-dpda` tiene al menos un nodo (c) y los `valid` sin combinadores ni `pattern`/`uniqueItems` no tienen ninguno

**Verificación:**
- [ ] `make lint`, `make test`, `make docs-check`
- [ ] Manual: `schemac ir` sobre un caso `non-dpda` señala el nodo (c) y su estrategia

**Dependencias:** 4.2

**Archivos probables:** `crates/ir/src/classify.rs`, `crates/ir/tests/classify.rs`, `crates/semantic/tests/corpus.rs`, `crates/cli/src/main.rs`, `docs/fase3/clasificacion.md`

**Tamaño:** M

## Punto de control B (cierre de la Fase 3 para JSON)
- [ ] `make lint`, `make test`, `make docs-check` en verde
- [ ] Plan general: casilla «Módulo de clasificación» marcada, D13 registrada, «Retomar aquí» apunta al parser XSD
- [ ] Revisión del usuario antes de empezar el parser XSD
