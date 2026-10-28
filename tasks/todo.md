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
- [x] `docs/fase3/ir.md` define formalmente la IR, sus invariantes y cómo se representa cada palabra clave de `00-alcance.md` §2 y cada construcción de §3.1; resuelve los tres puntos de §5 (que se quitan de ahí)
- [x] Pruebas que construyen a mano la IR de un esquema JSON recursivo (`$ref` a la raíz por `properties`) y de un XSD con `sequence`, `choice`, atributo requerido y `maxOccurs="unbounded"`, y pasan `Ir::verify`; casos que rompen cada invariante fallan con un error claro
- [x] Sin recursión (D7), sin `unsafe`, tamaño O(n) en número de subesquemas; el documento muestra de qué dato de la IR sale cada pieza de la construcción del DPDA de la Fase 4

**Verificación:**
- [x] `make lint`, `make test`, `cargo doc -p ir --no-deps` sin avisos
- [x] Revisión del usuario en el punto de control A (aprobado el 26 oct; `VerifiedIr` pasa a la 4.2)

**Estado (26 oct):** APROBADO CON OBSERVACIONES (0 Critical, 0 High; 2 Important de documentación de fases futuras). 3 rondas de corrección en la segunda invocación; sin commit. Casillas marcadas: las verificó el Tester de la ronda 3, y el punto de control A lo aprobó el usuario. Observaciones abiertas en el plan general («Pendientes abiertos tras 4.1»).

**Arrastra de 4.0:** los pendientes (a)-(d) de «Pendientes abiertos tras 4.0» del plan general (cota de amplificación ≲ 12× —11,35× medido— y «≈ 9×» como estimación, cifras de `ciclos.md:100-102`, prueba en release de las salvaguardas de `path_in`, requisitos conjuntos del servidor en `docs/fase3/pendientes.md` §2); se corrigen junto con 4.1.

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
- [x] Todos los esquemas válidos del corpus (`tests/schemas/parse-ok`, salvo las 2 excepciones, y `tests/cases`) bajan a una IR que pasa `Ir::verify`; cada nodo conserva su `Span` y su puntero
- [x] Un `$ref` no duplica su destino: el número de nodos es el de subesquemas; la recursión productiva queda como ciclo en el grafo; la salida es determinista (prueba dorada de al menos 3 esquemas)
- [x] `schemac ir` da código 0 con la IR, 1 con los mismos errores que `check`, 2 si no puede leer; bajada iterativa, O(n)
- [x] `VerifiedIr` (punto de control A, 26 oct): `Ir::verify(self) -> Result<VerifiedIr, (Ir, Vec<VerifyError>)>`; `VerifiedIr` sin campos mutables; la bajada llama siempre a `verify` en el camino normal y sus errores se presentan como un único error interno (`docs/fase3/ir.md` §4, pendientes 1 y 6 de §11); las fases siguientes solo aceptan `&VerifiedIr`
- [x] Exportar la IR a DOT (`schemac ir --dot`, o generada por `make docs`) y versionar en `docs/fase3/` el diagrama de al menos un esquema de ejemplo con recursión productiva; `make docs-check` verifica que el `.dot` está sincronizado, igual que los AFD de la Fase 1

**Verificación:**
- [x] `make lint`, `make test`
- [x] Manual: `cargo run -p cli -- ir tests/schemas/parse-ok/arbol-recursivo.json` (y `--dot`) muestra la IR (corregido el 27 oct: `tests/cases/valid/*.json` son casos de prueba, no esquemas)

**Estado (27 oct):** APROBADA en la revisión final (0 Critical, 0 Important, 0 High, 0 Medium), tras 3 rondas de corrección (Tester F1 de documentación → revisión con CR-I-1 en los literales → Tester con fallo de compilación en Windows); sin commit. Casillas marcadas: las verificó el Tester 4. Observaciones abiertas en `docs/fase3/pendientes.md` §16-§21 (4.2-S1…4.2-I1) y §15 (4.2-I2).

**Arrastra de 4.1 (hecho):** SA-L1 (los mensajes de `VerifyError` repiten nombres y lexemas del usuario sin escapar ni recortar: llevar `short` a un sitio compartido; registrar solo el recuento y los N primeros errores internos); en `docs/fase3/ir.md`: decisión 9 y pendiente 6 ya decididos (punto A, 26 oct; el 6 remite al criterio `VerifiedIr`), fila `Content` de §9 marcada «propuesta, se confirma en 4.3», «2^(10⁴ − 1) caminos» en §7-§8; decir qué código de salida da el error interno de `verify` en `schemac ir`. L-3 de la revisión final de 4.1: anotar en `docs/fase3/pendientes.md` («Vigilar más adelante») las obligaciones del frontal XSD (`ir.md` §4), `location_clipped` para las Fases 4/5 (`ir.md` §11, pendiente 2) y el tope de estados para todo producto (L-1).

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
- [x] Una sola tabla en el código, citada por `docs/fase3/clasificacion.md`, da clase y estrategia; una prueba con `match` exhaustivo comprueba que coincide con las columnas «Cat.» de `00-alcance.md` (incluidas las de XSD) y una prueba de la documentación comprueba que la tabla escrita es la del código
- [x] La clase de un nodo es la máxima de sus restricciones y la de los nodos que aplica sobre la misma instancia; documentado y probado con `not`/`anyOf` sobre (a) y sobre (c)
- [x] Todo el corpus sale clasificado; cada caso de `tests/cases/non-dpda` tiene al menos un nodo (c) y los `valid` sin combinadores ni `pattern`/`uniqueItems` no tienen ninguno

**Verificación:**
- [x] `make lint`, `make test`, `make docs-check`
- [x] Manual: `schemac ir` sobre un caso `non-dpda` señala el nodo (c) y su estrategia (28 oct: con el esquema extraído del campo `schema` de `non-dpda-allof-not-01`, porque los casos no son esquemas; la raíz y el `not` salen (c) con producto acotado, en JSON y en DOT con borde grueso)

**Estado (28 oct):** APROBADA en la revisión final (0 Critical, 0 High, 0 Medium; el único Important era del plan general y lo corrigió el orquestador: tabla de la Fase 3 y D19-D20), tras 1 ronda de corrección (borde `maxOccurs=2` y observaciones de pruebas y documentación); sin commit. Casillas verificadas por los Testers 1 y 2. Observaciones abiertas en `docs/fase3/pendientes.md` §22-§32 (4.3-L1, 4.3-I1…I3, 4.3-S1…S7).

**Arrastra de 4.2 (hecho):** 4.2-S3 (`docs/fase3/pendientes.md` §18): el corpus de `tests/cases` se prueba reserializado con `serde_json` (claves ordenadas, números por `f64`); al recorrer el corpus en 4.3, usar el texto original del esquema o activar `preserve_order` y `arbitrary_precision`. Las demás observaciones de 4.2 (S1, S2, S4, L1, I1) van con la próxima tarea que toque la CLI o `crates/ir`, y la 4.3 toca las dos: se corrigen aquí (§16, §17, §19 con el comentario de `main.rs:44-46`, §20, §21; añadido por el orquestador el 28 oct).

**Dependencias:** 4.2

**Archivos probables:** `crates/ir/src/classify.rs`, `crates/ir/tests/classify.rs`, `crates/semantic/tests/corpus.rs`, `crates/cli/src/main.rs`, `docs/fase3/clasificacion.md`

**Tamaño:** M

## Punto de control B (cierre de la Fase 3 para JSON)
- [x] `make lint`, `make test`, `make docs-check` en verde (28 oct)
- [x] Plan general: casilla «Módulo de clasificación» marcada, D13 registrada, «Retomar aquí» apunta al parser XSD
- [ ] Revisión del usuario antes de empezar el parser XSD
