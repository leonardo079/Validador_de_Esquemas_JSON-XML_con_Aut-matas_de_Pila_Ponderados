# Tareas: semana 3, tarea 5 (frontal XSD)

Detalle y decisiones en `tasks/plan.md`. Una tarea por vez con `/autodev`; el
usuario hace el commit entre tareas. Las tareas cerradas (4.0-4.3, puntos de control A
y B, 5.1 y 5.2) están archivadas literalmente en `tasks/hechas.md`.

# Tarea 5: frontal XSD (plan del Planner del 28 oct, aprobado el 29 oct con U1-U9 = D21-D29)

Detalle en `tasks/plan.md`. Una subtarea por vez; 5.1 y 5.2 están cerradas (en `tasks/hechas.md`).

## Tarea 5.3: Resolución de nombres, restricciones estructurales y bajada de tipos complejos a `VerifiedIr`

**Descripción:**
- **Tabla de símbolos:**
  - elementos globales, por un lado, y tipos complejos y simples, por otro, con nombres únicos en cada espacio;
  - resolución de `type`, `ref` y `base` contra la tabla o los predefinidos de §3.2;
  - `ref` solo a elementos globales.
- **Restricciones de componente del subconjunto que la IR exige** (con posición, antes de bajar):
  - D14 (`xs:element` sin `type`, `ref` ni tipo anónimo);
  - `name` con `ref`, o `type` con un tipo anónimo;
  - ~~`minOccurs`/`maxOccurs` en un elemento global~~ (ya lo rechaza el parser de la 5.2 como `syntax`, `parse-error/min-occurs-global.xsd`);
  - `minOccurs` > `maxOccurs` (I11);
  - atributos repetidos (I10);
  - *all-group limited* (I12);
  - *Element Declarations Consistent* (I12);
  - `default` junto a `fixed`, o `default` en un atributo requerido (I13);
  - `use="prohibited"`;
  - U7.
- **Bajada** según `ir.md` §5.2: documento con `Choice` de los globales, tipos compartidos, predefinidos y envoltorios `simpleContent/xs:…` creados una vez, orígenes de §5.2 y B3, contadores `u64` saturados.
- La bajada llama siempre a `verify` (D15).
- Mientras no esté la 5.6, `xs:simpleType` se rechaza con un error explícito «pendiente (tarea 5.6)». Los valores de `default`/`fixed` todavía no se comprueban contra su tipo (lo cierra la 5.6).

**Criterios de aceptación:**
- [ ] Los dos esquemas de `tests/cases` y `pedido_xsd` bajan a una IR que pasa `verify`. La IR del pedido de `forma.rs` sale idéntica a la hecha a mano, salvo el orden de nodos que fije B3, con las mismas ubicaciones (`#/complexType/Pedido/element/item`, `#/builtin/int`, `#/simpleContent/xs:string`).
- [ ] Cada entrada de `tests/schemas/xsd/semantic-error` da su error con línea y columna (manifiesto). Ningún esquema del corpus produce un error de `verify` (prueba que recorre todo el corpus y exige `Ok` o un error del usuario, nunca `InternalError`).
- [ ] Análisis y bajada iterativos y O(n) en el tamaño del AST, con una prueba de tamaño: un tipo usado k veces es una sola arista por uso, no una copia.
- [ ] Las tablas de símbolos se indexan por `(ns_id(), local())` con el *hasher* por defecto de `std` (nunca por el texto del URI, `expanded()`, ni con un *hasher* determinista), y el `targetNamespace` es el `NsId` que guardó la 5.2; prueba de regresión sin reloj (D1) con un URI de ~8 MiB y ~5·10⁵ referencias `type="p:T"` (`docs/fase2/pendientes.md` §11, 5.1-SEC3-I1; añadido el 3 nov).
- [ ] **D39 (valores por defecto sin efecto; `docs/fase2/pendientes.md` §17, añadido el 3 nov):** se aceptan y se ignoran (no llegan al AST ni a la IR) `attributeFormDefault="unqualified"` en `xs:schema`, `form` con el valor que ya rige (`unqualified` en `xs:attribute`; en un `xs:element` local, el mismo que el `elementFormDefault` del esquema), `nillable="false"` y `abstract="false"`; los valores que cambian la validación (`attributeFormDefault="qualified"`, `form` distinto del vigente, `nillable="true"`, `abstract="true"`) siguen siendo `unsupported` con su categoría y motivo. Valores tras *collapse*, como el resto de la lista blanca (D37). Prueba por cada atributo con el valor aceptado y el rechazado; los casos de `tests/schemas/xsd/parse-error` que hoy rechazan el valor por defecto (`abstract.xsd`, `attribute-form-default.xsd`…) pasan a usar el valor que cambia la validación o se mueven a `parse-ok`, con el manifiesto al día; `gramatica-xsd.md` §5 y `pendientes.md` §17 al día.

**Verificación:** `cargo test -p xsd`; `make lint`; `make test`.

**Dependencias:** 5.2, U7.

**Archivos probables:** `crates/xsd/src/{symbols.rs,check.rs,lower.rs}`, `crates/xsd/tests/lower.rs`, `tests/schemas/xsd/semantic-error/*`, `tests/schemas/errores-semanticos-xsd.json`.

**Tamaño:** M.

**Arrastra:**
- `pendientes.md` §3 (S-5, primera mitad; la segunda la cierra D27 rechazando la construcción);
- `docs/fase2/pendientes.md` §11 (tablas por `NsId`);
- `docs/fase2/pendientes.md` §16 (restricciones que el parser de la 5.2 deja a la 5.3, incl. 5.2-CR-S2: `xs:attribute` globales) y §19 (5.2-CR3-S1: casos de corpus de las mutaciones vivas de `subset.rs` y `texto-con-comentario-malo.xsd`; 5.2-CR2-S3: comparar las copias de `parse-ok` con `tests/cases`);
- `docs/fase2/pendientes.md` §5: recorrer el AST con pila explícita; `clone`/`{:?}`/`==` del `Schema` solo en el hilo con `stack_size` (D7).
- `pendientes.md` §32 (4.3-S7: cosméticos de `ir`, solo si la tarea toca `crates/ir`).

## Tarea 5.4: `schemac check|ir` con XSD, corpus clasificado y doradas

**Descripción:** Integrar el frontal en `schemac` según U2, en el mismo hilo de 8 MiB (D7), con los códigos de D17 (0, 1, 2, 3). Añadir el caso XSD a `corpus_clasificado` y versionar las IR doradas y un DOT de ejemplo.

**Criterios de aceptación:**
- [ ] `schemac check` y `schemac ir [--dot]` sobre un XSD:
  - 0 con la IR JSON v2 (`format: "xml"`, `targetNamespace`), con clase y estrategia;
  - 1 con los mismos errores que `check`;
  - 2 si no puede leer el archivo;
  - 3 ante un error interno (prueba con una bajada que falla, como `fallo_interno_da_codigo_3`).
  - un pánico en el hilo de `schemac check` da también **3**, igual que en `ir` (hoy da 2; `docs/fase3/pendientes.md` §8, decidido por el usuario el 3 nov, D33), con prueba y `bajada.md` §4 al día.

  Los esquemas JSON siguen dando exactamente la misma salida: las doradas de `tests/ir/` no cambian.
- [ ] `valid-xml-pedido-01` y el esquema de `malformed-xml-billion-laughs-01` pasan por el equivalente XSD de `corpus_clasificado` (`pendientes.md` §22) usando el texto original del campo `schema`. El pedido no tiene ningún nodo (c), y sí uno (b) por `Attributes`.
- [ ] Al menos 2 doradas XSD en `tests/ir/` (`make doradas`) y `docs/fase3/ir-pedido.dot` sincronizado por `make docs-check`.
- [ ] La tabla ρ de `pendientes.md` §1 tiene una fila por cada clase nueva de error de XSD, medida con `schemac check` en release, o se justifica que no supera el ρ vigente.

**Verificación:**
- `make lint`, `make test`, `make docs-check`.
- Manual: `cargo run -p cli -- ir tests/schemas/xsd/parse-ok/pedido.xsd` (y `--dot`) muestra el documento con la `Choice` de los globales, el nodo (b) del atributo `id` y `#/builtin/positiveInteger` con `Minimum(1)`. Además, `cargo run -p cli -- check` sobre un XSD de `semantic-error` da el código 1 con línea y columna.

**Dependencias:** 5.3, U2.

**Archivos probables:** `crates/cli/{Cargo.toml,src/main.rs}`, `crates/cli/tests/{ir.rs,check.rs}`, `crates/xsd/tests/corpus.rs`, `tests/ir/*.json`, `Makefile`, `docs/fase3/bajada.md` §5, `docs/fase3/pendientes.md` §1.

**Tamaño:** M.

**Arrastra (es «la próxima tarea que toque la CLI»):**
- `pendientes.md` §23 (4.3-L1, `O_NONBLOCK` en `leer`);
- `pendientes.md` §28 (4.3-S3, no reserializar el esquema del caso);
- `pendientes.md` §29 (4.3-S4, si toca el corpus de `semantic`);
- `pendientes.md` §24 y §25, opcionales y cosméticos.

## Punto de control C: primer XSD de extremo a extremo (imprescindible para la semana 3)

- [ ] `make lint`, `make test`, `make docs-check` en verde.
- [ ] El usuario revisa `schemac ir` sobre el pedido, la detección de formato (U2) y el texto de uso.
- [ ] Decidir según U8 si 5.5 a 5.7 siguen antes de la Fase 4 o se aplica el recorte.

## Tarea 5.5: Números XSD: forma léxica, reescritura a RFC 8259 y comparación exacta

**Descripción:** Funciones puras, sin AST:
- forma léxica de `xs:integer`, `xs:decimal`, `xs:float`/`xs:double` y `xs:boolean`, con los espacios colapsados (`collapse`);
- reescritura a un lexema de RFC 8259 (`+1` → `1`, `.5` → `0.5`, `1.` → `1`);
- comparación decimal exacta, sin `f64`, para cualquier longitud de exponente;
- redondeo IEEE de float y double, con el rechazo de U4.

**Criterios de aceptación:**
- [ ] Tabla de casos y proptest:
  - `xs:integer` rechaza `1.0`, `1e2` y `+-1`;
  - `xs:decimal` rechaza exponentes;
  - toda reescritura es un lexema RFC 8259 que `ir::verify` acepta (I8) y tiene el mismo valor;
  - la comparación coincide con un oráculo de números racionales en los casos de prueba y es antisimétrica y transitiva en el proptest;
  - `INF`, `NaN` y `1e39` en `xs:float` se rechazan con el motivo.
- [ ] O(longitud del lexema) por operación, también con exponentes de 10⁵ cifras (sin pánico ni desbordamiento: saturación documentada, como en `semantic/src/number.rs`).

**Verificación:** `cargo test -p xsd` (o `-p ir`, según B4); `make lint`; `make test`.

**Dependencias:** U4 y B4. Es independiente de 5.1 a 5.4; puede adelantarse si hace falta.

**Archivos probables:** `crates/ir/src/num.rs` o `crates/xsd/src/number.rs` y sus pruebas.

**Tamaño:** S.

**Arrastra:** `pendientes.md` §3 (I-2, `xs:float`/`xs:double`); `ir.md` §11, pendiente 3.

## Tarea 5.6: Tipos simples: `xs:simpleType`/`xs:restriction`, facetas, enumeraciones y `default`/`fixed`

**Descripción:** Quitar el rechazo temporal de 5.3. Hay que:
- detectar la derivación circular de `base` de forma iterativa, **antes** de seguir la cadena (D7, D15);
- fijar el `Lexical` de cada restricción;
- comprobar con posición que cada faceta se aplica al primitivo (I13) y que no hay faceta repetida en un paso ni `length` junto a `minLength`/`maxLength` (I4);
- rechazar `min*` y `max*` inclusivo y exclusivo a la vez;
- comprobar que cada faceta es una restricción válida de la base («valid restriction»);
- comprobar las enumeraciones: forma léxica del tipo y dentro del espacio de valores de la base, también los rangos de los predefinidos (`xs:int`), y ninguna en `xs:boolean`;
- comprobar que `default`/`fixed` son valores del tipo, salvo las facetas `pattern` (U9);
- unir con `|` varias `pattern` de un mismo paso;
- bajar según `ir.md` §5.2: `Ref` a la base y orígenes `#/simpleType/…`, anónimos incluidos.

**Criterios de aceptación:**
- [ ] Un esquema de `parse-ok` con tipos simples con nombre y anónimos, en cadena de 3 restricciones, con enumeración, rangos, longitudes y `pattern`, baja a una IR que pasa `verify`, con las ubicaciones de §5.2.
- [ ] Hay un caso de `semantic-error` con línea y columna para cada una de estas obligaciones, y ninguno da un error interno:
  - ciclo directo e indirecto de `base`;
  - una cadena de 10⁵ restricciones con un ciclo al final, sin desbordamiento y en un hilo pequeño;
  - una faceta que no se aplica al tipo;
  - una faceta repetida;
  - `length` junto a `minLength`;
  - `minInclusive` junto a `minExclusive`;
  - una restricción inválida de la base;
  - una enumeración `1.0` en `xs:integer`;
  - `300` en una restricción de `xs:byte`;
  - un `fixed` que no es del tipo.
- [ ] Análisis iterativo y O(n) más el costo de 5.5 por valor. La lista de «Obligaciones del frontal XSD» de `ir.md` §4 se amplía con la regla general y las restricciones de esta tarea (`pendientes.md` §3).

**Verificación:**
- `cargo test -p xsd`; `make lint`; `make test`; `make docs-check`.
- Manual: `schemac ir` sobre el esquema de tipos simples muestra `Lexical`, `Ref` y las facetas reescritas.

**Dependencias:** 5.4, 5.5.

**Archivos probables:** `crates/xsd/src/{simple.rs,check.rs,lower.rs}`, `crates/xsd/tests/simple.rs`, `tests/schemas/xsd/*`, `docs/fase3/ir.md` §4, §5.2 y §7.

**Tamaño:** M. Es la más densa; si pasa de M, separar «valid restriction» y `default`/`fixed` en una 5.6b.

**Arrastra:** `pendientes.md` §3 (L-3, I-1).

## Tarea 5.7: *Unique Particle Attribution* y cierre del frontal

**Descripción:**
- **UPA** sobre el árbol de partículas, iterativa, con la semántica de U5 y el límite de U6; el error lleva la posición de las dos partículas en conflicto.
- **Cierre del frontal:**
  - documento de teoría de la fase;
  - correcciones de `ir.md` (§4, §5.2 con los orígenes de S-5, §7 fila Q, §11);
  - `00-alcance.md` §3 y §4 (4.3-S1 y las desviaciones aprobadas en U3 a U7, U9);
  - cerrar `pendientes.md` §3, §22 (parte XSD) y §26, y anotar los pendientes nuevos (`pattern` en enumeraciones y `default`/`fixed`; Glushkov con contadores para la Fase 4).

**Criterios de aceptación:**
- [ ] Los casos `(a?, a)`, `(a | a)`, `(a*, a)` y un `choice` anidado ambiguo se rechazan con la posición de las dos partículas. Los modelos deterministas del corpus (pedido y `pedido_xsd` con `choice` y `subpedido` opcional) pasan. Pasar el límite de U6 es un error `limit`.
- [ ] El costo de UPA está documentado y acotado por el límite, y lo comprueba una prueba sin reloj (D1): un modelo en el límite termina; uno por encima se rechaza antes de calcular conjuntos.
- [ ] `docs/fase3/xsd.md` (o el nombre que elija el Builder) explica el análisis y la bajada con su costo. `make docs-check` en verde y casilla de la semana 2 lista para marcar.

**Verificación:**
- `make lint`, `make test`, `make docs-check`.
- Manual: `schemac check` sobre un XSD con `(a?, a)` da 1 con línea y columna, y `schemac ir` sobre el pedido sigue igual.

**Dependencias:** 5.3 (puede ir antes que 5.6 si U8 lo pide), U5, U6.

**Archivos probables:** `crates/xsd/src/upa.rs`, `crates/xsd/tests/upa.rs`, `docs/fase3/{xsd.md,ir.md,pendientes.md}`, `docs/00-alcance.md`.

**Tamaño:** M.

## Punto de control D: cierre de la casilla XSD

- [ ] `make lint`, `make test`, `make docs-check` en verde.
- [ ] Todo el corpus XSD sale como IR clasificada («Terminado cuando» de la Fase 3 para XSD).
- [ ] Plan general: casilla marcada, D21 y siguientes registradas, «Retomar aquí» apuntando a la Fase 4.
