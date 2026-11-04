# Tareas hechas (archivo)

Tareas cerradas que salieron de `tasks/todo.md` el 3 nov 2026 (archivado aprobado por el usuario), copiadas literalmente. Lo pendiente sigue en `tasks/todo.md`; el detalle del plan de la tarea 5, en `tasks/plan.md`.

# Semana 2, tareas 4 (IR común y clasificación)

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
- [x] Revisión del usuario antes de empezar el parser XSD (29 oct: D17, D19 con el cambio de `00-alcance.md` §3.1 y D20 aprobadas)

# Tarea 5: frontal XSD (subtareas cerradas)

## Tarea 5.1: Lector XML de documentos de esquema y espacios de nombres

**Descripción:** Adaptar los eventos de `lexer::xml` a tokens bajo demanda (`STAG`, `ATTR`, `>`, `/>`, `ETAG`, `TEXT`, `PI`, igual que `schema-parser/src/token.rs` con el lexer JSON). Añade lo que la gramática XML deja fuera: el nombre de cierre igual al de apertura, atributos únicos por etiqueta, `<?xml …?>` solo como primer token y con `encoding` UTF-8 si lo trae, texto fuera del elemento raíz formado solo por espacios, y ámbito de las declaraciones `xmlns` con resolución de QName a (URI, nombre local). Es solo para tiempo de compilación: puede reservar memoria.

**Criterios de aceptación:**
- [x] Un documento bien formado da la sucesión de tokens esperada, y los nombres resueltos por espacio de nombres con cualquier prefijo (`xs:`, `xsd:`, espacio por defecto). Cada uno de estos casos da un error con línea y columna:
  - `<a></b>`;
  - un atributo repetido;
  - `<?xml` que no va al inicio o con `encoding="latin1"`;
  - texto que no es espacio fuera de la raíz;
  - un prefijo sin declarar;
  - `<!DOCTYPE` (el error `DoctypeForbidden` del lexer, en el offset del caso billion-laughs).
- [x] El resultado es el mismo con entradas de bloques de 1, 7 y 4096 bytes (proptest), y el costo es O(n) en bytes.
- [x] Sin recursión (pila explícita de etiquetas y ámbitos) y sin pánicos ante una entrada hostil (proptest con bytes arbitrarios).

**Verificación:** `cargo test -p xsd`; `make lint`; `make test`.

**Dependencias:** U1.

**Archivos probables:** `Cargo.toml` (workspace), `crates/xsd/{Cargo.toml,src/lib.rs,src/reader.rs,src/error.rs}`, `crates/xsd/tests/reader.rs`.

**Tamaño:** M.

**Estado (29 oct):** APROBADA CON OBSERVACIONES en la revisión final (0 Critical, 0 High, 0 Medium; 1 Important de documentación, ya anotado), tras 3 rondas de corrección (Tester 1: tiempo cuadrático con URI largos → URI internados; revisión 1: `resolve_qname` dependía del momento de llamada → resolución ansiosa `Attr::as_qname()`, memoria de `<?xml`; revisión 2: `Attr.value` público con rangos internos → privado, `NsId` como clave). Sin commit. Casillas verificadas por el Tester 4 (44 pruebas en `crates/xsd/tests/reader.rs`). Observaciones abiertas en `docs/fase2/pendientes.md` §8-§14 (5.1-CR3-I1, -S1, -S2, 5.1-SEC3-L2/I1/I2/I3, 5.1-T4-1, 5.1-CR2-S3, 5.1-SEC2-I1); las de §11 y §13 son obligaciones de la 5.2.

## Tarea 5.2: Gramática LL(1) del subconjunto XSD y parser → AST XSD

**Descripción:** Gramática del metalenguaje XSD sobre los tokens de 5.1. El `STAG` del espacio de nombres de XSD se refina en una clase por construcción (`S_SCHEMA`, `S_ELEMENT`, `S_COMPLEXTYPE`, `S_SEQUENCE`, `S_CHOICE`, `S_ALL`, `S_ATTRIBUTE`, `S_SIMPLECONTENT`, `S_EXTENSION`, `S_SIMPLETYPE`, `S_RESTRICTION`, una por faceta y, según U3, `S_ANNOTATION`), como las clases `K_*` de `esquema.ll1`. El parser construye el AST propio de XSD (D3) con el `Span` de cada construcción y aplica la lista blanca de atributos por construcción (`00-alcance.md` §3.1).

**Criterios de aceptación:**
- [x] `docs/fase2/xsd.ll1` sin conflictos LL(1); `ll1-xsd.md` lo generan `make docs` y `make docs-check` (añadir `xsd` a los bucles del `Makefile`); `docs/fase2/gramatica-xsd.md` con tokens, gramática, FIRST/FOLLOW resumidos y las restricciones que la gramática no expresa.
- [x] Todo `tests/schemas/xsd/parse-ok` (con los dos esquemas de `tests/cases` y el `pedido_xsd` de `forma.rs`) da un AST. Cada entrada de `parse-error` da el error y la línea/columna del manifiesto, entre ellas:
  - cada construcción de §3.3 (`xs:import`, `xs:any`, `xs:group`, `mixed="true"`, `xs:complexContent`, `xs:list`, `xs:union`, `xs:key`, `totalDigits`…), rechazada con su categoría y su motivo (D6);
  - un atributo no admitido (`form`, `nillable`, `abstract`, `default` en `xs:element`);
  - texto en contenido de solo elementos;
  - un elemento ajeno al espacio de nombres de XSD.
- [x] Pasar el tope de profundidad (256) es un error `limit`, nunca un desbordamiento de pila: probado en un hilo de 2 MiB en debug, con el margen medido y documentado como en `gramatica-esquema.md` §6. El análisis es O(n).
- [x] Obligaciones que deja la 5.1 (`docs/fase2/pendientes.md` §11-§13; añadidas el 3 nov):
  - el corte por profundidad ocurre en cuanto se pasa el tope, sin seguir leyendo el documento (la pila de elementos abiertos del lector no pasa de 256; §12, D7), con prueba de un documento de 10⁶ niveles;
  - `targetNamespace` se recorta (*collapse*), se interna con `Reader::intern` mientras el lector existe y su `Option<NsId>` se guarda en la raíz del AST; `intern` no se llama para ningún otro valor; la doc de `Reader::intern` dice que interna el valor ya normalizado (5.1-CR3-I1, 5.1-SEC3-I2); prueba con `targetNamespace=' urn:t '` y `type='t:T'` que dan el mismo `NsId`;
  - `as_qname()` se llama una sola vez por atributo `type`/`base`/`ref` y el AST guarda el `Name` resuelto con su posición (5.1-SEC3-I3, §11); prueba de lectura anticipada con un hijo que redeclara el prefijo;
  - *collapse* de los valores de atributo del esquema sobre una copia (`value()`/`into_value()`), nunca modificando el `Attr` (§9);
  - `XsdError::new(kind, at)` en `error.rs`, único constructor dentro del crate, que recorta los textos con `clipped()`; los errores se construyen con `clip(&nombre)` sin clonar el nombre entero (5.1-CR3-S1/S2, §13).

**Verificación:** `cargo test -p xsd`; `make lint`; `make test`; `make docs-check`.

**Dependencias:** 5.1, U3.

**Archivos probables:** `docs/fase2/{xsd.ll1,ll1-xsd.md,gramatica-xsd.md}`, `Makefile`, `crates/xsd/src/{ast.rs,parser.rs}`, `crates/xsd/tests/parser.rs`, `tests/schemas/xsd/{parse-ok,parse-error}/*`, `tests/schemas/errores-xsd.json`.

**Tamaño:** M.

**Arrastra:** `fase2/pendientes.md` §5 (margen de pila), §9, §11, §12 (profundidad) y §13 (ver el último criterio).

**Estado (3 nov):** APROBADA CON OBSERVACIONES en la revisión final (0 Critical; el único Important era de documentación y se corrigió en el cierre), tras 2 rondas de corrección (revisión 1: D34 sin aplicar, orden de errores dentro de la etiqueta, cobertura de la lista blanca; revisión 2: `fixed` en `enumeration`/`pattern`, segunda excepción de orden, cobertura). Desde la revisión 2 solo revisa el code-reviewer (D35). Casillas verificadas por el Tester 3 (`crates/xsd/tests/parser.rs`, 22 pruebas; corpus `tests/schemas/xsd/` con 10 `parse-ok` y 82 `parse-error`). Sin commit. Observaciones abiertas en `docs/fase2/pendientes.md` §5, §9, §11, §12, §16-§19 y `docs/fase1/pendientes.md` §3.
