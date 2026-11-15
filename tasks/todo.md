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
- [x] Los dos esquemas de `tests/cases` y `pedido_xsd` bajan a una IR que pasa `verify`. La IR del pedido de `forma.rs` sale idéntica a la hecha a mano, salvo el orden de nodos que fije B3, con las mismas ubicaciones (`#/complexType/Pedido/element/item`, `#/builtin/int`, `#/simpleContent/xs:string`).
- [x] Cada entrada de `tests/schemas/xsd/semantic-error` da su error con línea y columna (manifiesto). Ningún esquema del corpus produce un error de `verify` (prueba que recorre todo el corpus y exige `Ok` o un error del usuario, nunca `InternalError`).
- [x] Análisis y bajada iterativos y O(n) en el tamaño del AST, con una prueba de tamaño: un tipo usado k veces es una sola arista por uso, no una copia.
- [x] Las tablas de símbolos se indexan por `(ns_id(), local())` con el *hasher* por defecto de `std` (nunca por el texto del URI, `expanded()`, ni con un *hasher* determinista), y el `targetNamespace` es el `NsId` que guardó la 5.2; prueba de regresión sin reloj (D1) con un URI de ~8 MiB y ~5·10⁵ referencias `type="p:T"` (`docs/fase2/pendientes.md` §11, 5.1-SEC3-I1; añadido el 3 nov).
- [x] **D39 (valores por defecto sin efecto; `docs/fase2/pendientes.md` §17, añadido el 3 nov):** se aceptan y se ignoran (no llegan al AST ni a la IR) `attributeFormDefault="unqualified"` en `xs:schema`, `form` con el valor que ya rige (`unqualified` en `xs:attribute`; en un `xs:element` local, el mismo que el `elementFormDefault` del esquema), `nillable="false"` y `abstract="false"`; los valores que cambian la validación (`attributeFormDefault="qualified"`, `form` distinto del vigente, `nillable="true"`, `abstract="true"`) siguen siendo `unsupported` con su categoría y motivo. Valores tras *collapse*, como el resto de la lista blanca (D37). Prueba por cada atributo con el valor aceptado y el rechazado; los casos de `tests/schemas/xsd/parse-error` que hoy rechazan el valor por defecto (`abstract.xsd`, `attribute-form-default.xsd`…) pasan a usar el valor que cambia la validación o se mueven a `parse-ok`, con el manifiesto al día; `gramatica-xsd.md` §5 y `pendientes.md` §17 al día.

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

**Estado (4 nov):** APROBADA CON OBSERVACIONES en la revisión 1 (code-reviewer solo, D35: 0 Critical, 2 Important —5.3-CR1-I1 documentación, corregida en el cierre; 5.3-CR1-I2 cobertura, pasa a la 5.7 por D38—, 6 Suggestion), sin rondas de corrección. Casillas verificadas por el Tester 1 (`crates/xsd/tests/lower.rs`: `pedido_baja_igual_que_la_ir_hecha_a_mano`, `esquemas_de_los_casos_bajan`, `errores_semanticos_con_linea_y_columna`, `corpus_entero_sin_error_interno`, `tipo_usado_k_veces_es_una_arista_por_uso`, `analisis_y_bajada_sin_recursion`, `uri_de_8_mib_y_medio_millon_de_referencias`, `d39_no_llega_a_la_ir`; `crates/xsd/tests/parser.rs`: `d39_valores_por_defecto_sin_efecto`). Decisiones del Builder: D40. Sin commit. Observaciones abiertas en `docs/fase3/pendientes.md` §3 y §33 y `docs/fase2/pendientes.md` §17; las de cobertura y de UPA son criterios de la 5.7.

## Tarea 5.4: `schemac check|ir` con XSD, corpus clasificado y doradas

**Descripción:** Integrar el frontal en `schemac` según U2, en el mismo hilo de 8 MiB (D7), con los códigos de D17 (0, 1, 2, 3). Añadir el caso XSD a `corpus_clasificado` y versionar las IR doradas y un DOT de ejemplo.

**Criterios de aceptación:**
- [x] `schemac check` y `schemac ir [--dot]` sobre un XSD:
  - 0 con la IR JSON v2 (`format: "xml"`, `targetNamespace`), con clase y estrategia;
  - 1 con los mismos errores que `check`;
  - 2 si no puede leer el archivo;
  - 3 ante un error interno (prueba con una bajada que falla, como `fallo_interno_da_codigo_3`).
  - un pánico en el hilo de `schemac check` da también **3**, igual que en `ir` (hoy da 2; `docs/fase3/pendientes.md` §8, decidido por el usuario el 3 nov, D33), con prueba y `bajada.md` §4 al día.

  Los esquemas JSON siguen dando exactamente la misma salida: las doradas de `tests/ir/` no cambian.
- [x] `valid-xml-pedido-01` y el esquema de `malformed-xml-billion-laughs-01` pasan por el equivalente XSD de `corpus_clasificado` (`pendientes.md` §22) usando el texto original del campo `schema`. El pedido no tiene ningún nodo (c), y sí uno (b) por `Attributes`.
- [x] Al menos 2 doradas XSD en `tests/ir/` (`make doradas`) y `docs/fase3/ir-pedido.dot` sincronizado por `make docs-check`.
- [x] La tabla ρ de `pendientes.md` §1 tiene una fila por cada clase nueva de error de XSD, medida con `schemac check` en release, o se justifica que no supera el ρ vigente.

**Verificación:**
- `make lint`, `make test`, `make docs-check`.
- Manual: `cargo run -p cli -- ir tests/schemas/xsd/parse-ok/valid-xml-pedido-01.xsd` (y `--dot`) muestra el documento con la `Choice` de los globales, el nodo (b) del atributo `id` y `#/builtin/positiveInteger` con `Minimum(1)` (corregido el 4 nov: decía `pedido.xsd`, que usa `xs:int`; `schemac ir` sobre `pedido.xsd` da la IR de `pedido_xsd`). Además, `cargo run -p cli -- check` sobre un XSD de `semantic-error` da el código 1 con línea y columna.

**Dependencias:** 5.3, U2.

**Archivos probables:** `crates/cli/{Cargo.toml,src/main.rs}`, `crates/cli/tests/{ir.rs,check.rs}`, `crates/xsd/tests/corpus.rs`, `tests/ir/*.json`, `Makefile`, `docs/fase3/bajada.md` §5, `docs/fase3/pendientes.md` §1.

**Tamaño:** M.

**Arrastra (es «la próxima tarea que toque la CLI»):**
- `pendientes.md` §23 (4.3-L1, `O_NONBLOCK` en `leer`);
- `pendientes.md` §28 (4.3-S3, no reserializar el esquema del caso);
- `pendientes.md` §29 (4.3-S4, si toca el corpus de `semantic`);
- `pendientes.md` §24 y §25, opcionales y cosméticos.

**Estado (4 nov):** APROBADA CON OBSERVACIONES en la revisión 1 (code-reviewer solo, D35: 0 Critical, 1 Important de documentación —5.4-CR1-I1, argumento de la tabla ρ, corregido en el cierre—, 6 Suggestion, 3 nits), sin rondas de corrección. Casillas verificadas por el Tester 1: `crates/cli/src/main.rs` (`panico_en_el_hilo_da_codigo_3`, `fallo_interno_xsd_da_codigo_3`, `formato_por_el_primer_byte_significativo`, `abrir_una_fifo_sin_escritor_no_bloquea`), `crates/cli/tests/check.rs` (`xsd_valido`, `errores_xsd_con_linea_y_columna`, `formato_por_el_contenido`, `profundidad_maxima_xsd`), `crates/cli/tests/ir.rs` (`ir_de_un_xsd`, `ir_xsd_determinista_doradas_y_dot`, `errores_xsd_iguales_que_check`), `crates/xsd/tests/corpus.rs` (`corpus_clasificado`, `ir_doradas_xsd`), doradas JSON idénticas a `HEAD`, tabla ρ reproducida. Decisiones del Builder: D41. Sin commit (encadenada con la 5.3; foto de la 5.3 en `target/sondas-5.4/base-5.3/`). Observaciones abiertas en `docs/fase3/pendientes.md` §1 y «Cierre de la 5.4».

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
- [x] Tabla de casos y proptest:
  - `xs:integer` rechaza `1.0`, `1e2` y `+-1`;
  - `xs:decimal` rechaza exponentes;
  - toda reescritura es un lexema RFC 8259 que `ir::verify` acepta (I8) y tiene el mismo valor;
  - la comparación coincide con un oráculo de números racionales en los casos de prueba y es antisimétrica y transitiva en el proptest;
  - `INF`, `NaN` y `1e39` en `xs:float` se rechazan con el motivo.
- [x] O(longitud del lexema) por operación, también con exponentes de 10⁵ cifras (sin pánico ni desbordamiento: saturación documentada, como en `semantic/src/number.rs`).

**Verificación:** `cargo test -p xsd` (o `-p ir`, según B4); `make lint`; `make test`.

**Dependencias:** U4 y B4. Es independiente de 5.1 a 5.4; puede adelantarse si hace falta.

**Archivos probables:** `crates/ir/src/num.rs` o `crates/xsd/src/number.rs` y sus pruebas.

**Tamaño:** S.

**Arrastra:** `pendientes.md` §3 (I-2, `xs:float`/`xs:double`); `ir.md` §11, pendiente 3.

**Estado (14 nov):** APROBADA CON OBSERVACIONES en la revisión 1 (code-reviewer solo, D35: 0 Critical, 2 Important de documentación —5.5-CR-I1, «desviación» de −0 inexistente en XSD 1.0 2.ª ed. y regla de NaN; 5.5-CR-I2, cotas 768/113 del corte—, ambos corregidos en el cierre; 6 Suggestion), sin rondas de corrección. Casillas verificadas por el Tester 1 (`crates/ir/tests/num.rs`: `integer_forma_lexica_y_reescritura`, `decimal_forma_lexica_y_reescritura`, `reescritura_valida_y_del_mismo_valor`, `reescritura_decimal_canonica`, `comparacion_coincide_con_oraculo`, `comparacion_antisimetrica_y_transitiva`, `comparacion_casos_conocidos`, `d24_inf_nan_y_desbordes_se_rechazan_con_motivo`, `exponentes_y_fracciones_de_cien_mil_cifras`, `redondeo_ieee_casos`, `redondeo_coincide_con_std`; prueba diferencial ad hoc de ~11 000 puntos medios, 0 discrepancias). Módulo `ir::num` (B4). Decisiones: D43. Commit `b37428a`. Observaciones abiertas en `docs/fase3/pendientes.md` §36 (5.5-P1, P2 y CR-S2 → 5.6; P3 y CR-S1 → F4.1; CR-S3…S5 → próxima tarea que toque `ir`).

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
- [x] Un esquema de `parse-ok` con tipos simples con nombre y anónimos, en cadena de 3 restricciones, con enumeración, rangos, longitudes y `pattern`, baja a una IR que pasa `verify`, con las ubicaciones de §5.2.
- [x] Hay un caso de `semantic-error` con línea y columna para cada una de estas obligaciones, y ninguno da un error interno:
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
- [x] Análisis iterativo y O(n) más el costo de 5.5 por valor. La lista de «Obligaciones del frontal XSD» de `ir.md` §4 se amplía con la regla general y las restricciones de esta tarea (`pendientes.md` §3).

**Verificación:**
- `cargo test -p xsd`; `make lint`; `make test`; `make docs-check`.
- Manual: `schemac ir` sobre el esquema de tipos simples muestra `Lexical`, `Ref` y las facetas reescritas.

**Dependencias:** 5.4, 5.5.

**Archivos probables:** `crates/xsd/src/{simple.rs,check.rs,lower.rs}`, `crates/xsd/tests/simple.rs`, `tests/schemas/xsd/*`, `docs/fase3/ir.md` §4, §5.2 y §7.

**Tamaño:** M. Es la más densa; si pasa de M, separar «valid restriction» y `default`/`fixed` en una 5.6b.

**Heredado de la 5.5 (14 nov, D38; detalle en `docs/fase3/pendientes.md` §36):**
- [x] 5.5-P1: los valores de facetas, enumeraciones y `default`/`fixed` de tipos numéricos y `xs:boolean` pasan por `ir::num` (`xsd_number`, `parse_boolean`); un `NumError` se informa como `XsdError` de código `value` en la posición del valor, con el texto recortado.
- [x] 5.5-P2: rangos de los predefinidos acotados (`xs:byte`, `xs:int`…) y «valid restriction» respecto de la base con `ir::num::compare` (también sobre `Num` de `Float`/`Double`, por la monotonía de 5.5-CR-S2).
- [x] 5.5-CR-S2: proptest de la monotonía del decimal más corto: x < y en binary32/binary64 ⇒ `compare(num(x), num(y)) = Less`.

**Arrastra:** `pendientes.md` §3 (L-3, I-1, y la viñeta «Para la 5.6» de la 5.3); §22 (los tres `parse-ok` con `xs:simpleType` —`anotaciones`, `atributos-ajenos`, `tipos-simples`— entran en `corpus_clasificado` de `crates/xsd/tests/corpus.rs`, y la prueba comprueba que no se omite ninguno); §33 (5.3-CR1-S2 ocurrencias > 2⁶⁴, 5.3-CR1-S3 `Debug` derivado, si toca `check.rs`); §34 (ayudas de prueba duplicadas, opcional). Añadido el 4 nov.

**Estado (14 nov):** APROBADA en la revisión 2 tras **1 ronda de corrección** (devuelta por el code-reviewer de la revisión 1: 5.6-CR-I1, costo cuadrático al comparar cada valor contra una cota larga —10,8 s con 4 000 valores y una cota de 10⁶ cifras—, corregido analizando cada cota una sola vez con `ir::num::Decimal`/`compare_parsed`, prueba sin reloj `cota_larga_se_analiza_una_sola_vez` que falla con los dos mutantes cuadráticos; 5.6-CR-I2, lista exacta de desviaciones frente a XSD 1.0, en documentación). Revisión 2: 0 Critical, 0 Important, 3 Suggestion (anotadas). Casillas verificadas por los Testers de las rondas 0 y 1 (`crates/xsd/tests/simple.rs`: `tipos_simples_baja_con_las_ubicaciones_de_5_2`, `origenes_de_los_tipos_anonimos`, `cadena_de_cien_mil_restricciones_con_ciclo_al_final`, `cota_larga_heredada_por_una_cadena`, `cota_larga_se_analiza_una_sola_vez`, `restriccion_valida_respecto_de_la_base`, `enumeraciones_y_valores_por_defecto_son_del_tipo`; `crates/xsd/tests/lower.rs`: `errores_semanticos_con_linea_y_columna` con 29 casos nuevos en `tests/schemas/xsd/semantic-error`, `corpus_entero_sin_error_interno`; `crates/xsd/tests/corpus.rs::corpus_clasificado`; `crates/ir/tests/num.rs`: `decimal_mas_corto_es_monotono`, `comparacion_analizada_coincide`; dorada nueva `tests/ir/xsd/tipos-simples.json`). Decisiones: D44 (modifica D40 y amplía D43). Commit `b37428a` (junto con la 5.5). Observaciones abiertas en `docs/fase3/pendientes.md` §37.

## Tarea 5.7: *Unique Particle Attribution* y cierre del frontal

**Descripción:**
- **UPA** sobre el árbol de partículas, iterativa, con la semántica de U5 y el límite de U6; el error lleva la posición de las dos partículas en conflicto.
- **Cierre del frontal:**
  - documento de teoría de la fase;
  - correcciones de `ir.md` (§4, §5.2 con los orígenes de S-5, §7 fila Q, §11);
  - `00-alcance.md` §3 y §4 (4.3-S1 y las desviaciones aprobadas en U3 a U7, U9);
  - cerrar `pendientes.md` §3, el resto de §22 (la parte XSD de la 5.4 quedó resuelta el 4 nov; faltan los tres `parse-ok` con `xs:simpleType`, que entran con la 5.6) y §26, y anotar los pendientes nuevos (`pattern` en enumeraciones y `default`/`fixed`; Glushkov con contadores para la Fase 4).

**Criterios de aceptación:**
- [x] Los casos `(a?, a)`, `(a | a)`, `(a*, a)` y un `choice` anidado ambiguo se rechazan con la posición de las dos partículas. Los modelos deterministas del corpus (pedido y `pedido_xsd` con `choice` y `subpedido` opcional) pasan. Pasar el límite de U6 es un error `limit`.
- [x] El costo de UPA está documentado y acotado por el límite, y lo comprueba una prueba sin reloj (D1): un modelo en el límite termina; uno por encima se rechaza antes de calcular conjuntos.
- [x] `docs/fase3/xsd.md` (o el nombre que elija el Builder) explica el análisis y la bajada con su costo. `make docs-check` en verde y casilla de la semana 2 lista para marcar.
- [x] **Heredado de la revisión de la 5.3 (4 nov, D38; detalle en `docs/fase2/pendientes.md` y `docs/fase3/pendientes.md`):**
  - 5.3-CR1-I2 (cobertura): casos `semantic-error/edc-ref-y-local.xsd` (un `ref` frente a un local del mismo nombre con otro tipo) y `semantic-error/edc-anonimos.xsd` (dos tipos anónimos iguales, distintos por XSD) en el manifiesto, y un caso aceptado con el mismo `ref` repetido en `correspondencia_de_cada_construccion`;
  - 5.3-CR1-S4: un nombre repetido en `xs:all` (`c/all-dup.xsd`: mismo nombre y tipo, pasa EDC y `verify`) lo rechaza la UPA, con prueba;
  - 5.3-CR1-S1: `xs:element` local con `ref` y `form`/`nillable` del valor sin efecto (D39) se rechaza como `syntax` (XSD 1.0 src-element 2.2), igual que antes de D39, con prueba;
  - 5.3-CR1-I1: la desviación «EDC por nombre local» (D40) en `00-alcance.md` §3, con D23, D34 y D39.

**Verificación:**
- `make lint`, `make test`, `make docs-check`.
- Manual: `schemac check` sobre un XSD con `(a?, a)` da 1 con línea y columna, y `schemac ir` sobre el pedido sigue igual.

**Dependencias:** 5.3 (puede ir antes que 5.6 si U8 lo pide), U5, U6.

**Archivos probables:** `crates/xsd/src/upa.rs`, `crates/xsd/tests/upa.rs`, `docs/fase3/{xsd.md,ir.md,pendientes.md}`, `docs/00-alcance.md`.

**Tamaño:** M.

**Heredado de la 5.6 (14 nov, D38; detalle en `docs/fase3/pendientes.md` §37):**
- [x] 5.6-P4 (con CR-S3, CR-S4 y CR2-S2): pasar a `00-alcance.md` §3 la lista exacta de desviaciones de los tipos simples (`minExclusive` nuevo frente al `maxInclusive` de la base con `<`; `minExclusive` frente a `maxExclusive` en el mismo paso con `<`; cotas no comparadas con la enumeración de la base; enumeración frente a su propio paso; `minLength`/`maxLength` sobre `length` por las cláusulas 1.2 y 2.2). Es interfaz compartida (D42): se propone a las dos personas.
- [x] 5.6-T1-1 (no aplica mientras `default`/`fixed` de `xs:element` sigan rechazados): si se admiten `default`/`fixed` en `xs:element`, añadir esa forma a `cota_larga_se_analiza_una_sola_vez`.
- [x] 5.6-CR-S5 (decidido: va a la Fase 7): tope de errores por etapa, igual para las tres.

**Estado (15 nov):** APROBADA CON OBSERVACIONES en la revisión 2 tras **1 ronda de corrección** (devuelta por el code-reviewer de la revisión 1: 5.7-CR-I1, la UPA solo garantizaba determinismo débil y la Fase 4 necesita el fuerte con contadores —`(a{1,2}){2,2}`—, corregido con el rechazo conservador de las vueltas ambiguas con contador, `ErrorKind::AmbiguousCounter`, O(1) por paso; I-2 precisión de D26; I-3 e I-4 alcance veraz y recortado según la indicación del usuario). Revisión 2: 0 Critical, 1 Important de documentación (guardas de contador con cuerpo anulable en 5.7-P1, corregido en el cierre), 3 Suggestion. Casillas verificadas por los Testers de las rondas 0 y 1 (`crates/xsd/tests/upa.rs`: `modelos_ambiguos_con_las_dos_posiciones`, `ambiguos_a_traves_de_grupos_y_ref`, `modelos_deterministas_pasan`, `tope_de_particulas_por_modelo`, `abstraccion_conservadora_de_d25`, `nombre_repetido_en_all`, `vueltas_ambiguas_con_contador`, `contadores_sin_vuelta_ambigua_pasan`; unitarias de `src/upa.rs`: `tope_antes_de_calcular_conjuntos`, `vueltas_con_contador_sin_costo_extra`; `parser.rs::ref_con_valores_sin_efecto_de_d39`; 12 casos nuevos en el corpus; fuzzing diferencial de los Testers y del Reviewer contra oráculos propios de UPA débil y determinismo fuerte, 0 discrepancias en ~15 000 modelos). Decisiones: D45 (enmienda D25 y precisa D26; aprobada por Leonardo el 15 nov, falta el visto bueno de Bayron). `docs/00-alcance.md` §3-§4 actualizado (16 cambios). Commit `21dbe84`. Observaciones abiertas en `docs/fase3/pendientes.md` §38.

## Punto de control D: cierre de la casilla XSD

- [x] `make lint`, `make test`, `make docs-check` en verde.
- [x] Todo el corpus XSD sale como IR clasificada («Terminado cuando» de la Fase 3 para XSD).
- [x] Plan general: casilla marcada, D21 y siguientes registradas, «Retomar aquí» apuntando a la Fase 4.

**Estado (15 nov):** hecho. `make lint`, `make test` (442 pasan, 0 fallan, 1 ignorada por D1) y `make docs-check` en verde sobre `21dbe84`. `schemac ir` (release) sobre los 12 `parse-ok` de `tests/schemas/xsd/` da código 0 con todos los nodos clasificados (clase y estrategias); los 95 `parse-error` y los 78 `semantic-error` dan código 1 (ningún 3); los 2 casos XML de `tests/cases` pasan por `corpus_clasificado`. Casilla XSD de la semana 2 marcada en el plan general.


# Fase 4 — F4.1: definición formal de M, construcción composicional y pesos tropicales (plan del Planner del 15 nov, aprobado por Leonardo el 15 nov con F4-U1…U8 = D47)

Detalle y decisiones en `tasks/plan.md` («Plan de F4.1»). Una subtarea por vez, en orden F4.1a → F4.1b → F4.1c. Revisión (D35): solo `code-reviewer` en las tres (el simulador es de pruebas; el payload no confiable entra en F4.6).

## Tarea F4.1a: Modelo formal de M, pesos tropicales y esqueleto JSON (va primero)

**Descripción:**
- `crates/automaton`: tipos de M (Σ como clases de eventos del lexer, Q, Γ con marcos y registros, δ densa y total, q₀, Z₀, F), el semianillo tropical y la tabla de costo por `RuleKind`.
- Constructor JSON sobre `&VerifiedIr`:
  - marco «cualquiera», que es la gramática JSON de `docs/fase2/gramatica-json.md` como DPDA;
  - `type` (con `integer` por el registro del lexema), `Reject`, `true`;
  - `$ref` sin hermanas como alias;
  - `properties`/`additionalProperties` con trie de claves;
  - `prefixItems`/`items`;
  - `required` y `dependentRequired` con máscara (D5, S-1);
  - claves declaradas repetidas con peso ∞ (D2).
- Lo demás sale como `Pending` con la tarea que lo cubrirá (F4-U5).
- Simulador std sobre eventos, en los dos modos (conformidad total y fallo temprano).
- `location_clipped`.
- `schemac automaton [--dot] <esquema>` (F4-U7: resumen JSON de tamaños + DOT completo).
- `docs/fase4/automata.md` (definición formal, Σ y la pasada única, Γ y registros, δ con la propiedad de pila visible, pesos y costo, construcción por regla, complejidad) y `docs/fase4/pendientes.md`.

**Criterios de aceptación:**
- [ ] Con el esquema `true`, el simulador acepta con w = 0 todos los payloads JSON bien formados del corpus, y da w = ∞ en el offset del evento que lo detecta con `{"a" 1}`, `[1,]`, `{,}`, `]`, `1 2`, `{"a":1}}` y la entrada vacía; el resultado es idéntico alimentando el lexer en bloques de 1, 7 y 4096 bytes.
- [ ] Corpus: `valid-recursive-tree-01` da 0; `invalid-missing-required-01` da 1 violación, de la regla `required` del nodo correcto, en el offset 64; los casos `non-dpda` dan `Pending` con la tarea que los cubrirá (F4.4/F4.5), nunca aceptación.
- [ ] Dorada verificable a mano: el DPDA de `{"type":"object","properties":{"a":{"type":"integer"}},"required":["a"],"additionalProperties":false}`, en `tests/` y como `docs/fase4/*.dot` generado por `make docs` y comprobado por `make docs-check`; el número de estados y transiciones que da `automata.md`, contado a mano, coincide con el de la prueba.
- [ ] Propiedades: leyes del semianillo por proptest (asociatividad, conmutatividad, distributividad, 0 y ∞); δ total (cada (q, clase) alcanzable tiene una transición); entero: `1.0`, `1e2` y `10e-1` lo son, `1.5` y `1e-1` no, exponentes de 10⁵ cifras sin pánico; en fallo temprano, la violación informada es la de menor costo c.
- [ ] D7: se construyen en un hilo de 256 KiB una cadena de 10⁵ `$ref` y un esquema recursivo productivo; el simulador recorre un payload de 10⁵ niveles sin recursión.

**Verificación:** `cargo test -p automaton`, `make lint`, `make test`, `make docs-check`. Manual: `schemac automaton --dot` sobre la dorada, pasado por `dot -Tsvg`, y el resumen JSON.

**Dependencias:** PC-C, PC-D (hechas); F4-U1…U5, U7.

**Archivos probables:** `crates/automaton/{Cargo.toml, src/lib.rs, src/weight.rs, src/model.rs, src/dfa.rs, src/json.rs, src/sim.rs, src/export.rs}`, `crates/automaton/tests/{json.rs,corpus.rs}`, `crates/cli/src/main.rs` y `crates/cli/tests/automaton.rs`, `Makefile` (`docs`, `docs-check`), `docs/fase4/{automata.md,pendientes.md}`.

**Tamaño:** M, en el borde alto. Si pasa de M, la CLI, la dorada y el DOT van a F4.1b.

**Arrastra:** `docs/fase3/pendientes.md` §4 (`location_clipped`); §5 S-1 (bits de `DependentRequired`; el hash de claves no declaradas queda documentado para F4.6); §27 (cerrar con la corrección de `clasificacion.md` §4.2); `ir.md` §7 (filas Σ y Γ corregidas); la dependencia `automaton → semantic` (F4-B1: `semantic` solo como dependencia de pruebas).

## Tarea F4.1b: Registros y escalares JSON; conjunción por E₌ con tope

**Descripción:**
- Contadores: `minProperties`/`maxProperties`, `minItems`/`maxItems`, `minLength`/`maxLength` (en puntos de código, tras los escapes).
- `enum`/`const` de escalares: trie de cadenas, números por valor, `true`/`false`/`null`; `enum: []` (D11).
- `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum` y `multipleOf` exactos sobre el registro del lexema, con las rutinas numéricas `no_std` en `runtime-rs` (F4-U4, F4-B12).
- Marcos por conjunto canónico de `NodeId` (F4-U2): `$ref` con hermanas y `allOf` como conjunción, con el tope de 4 096 estados por producto, configurable y contado antes de reservar (L-1).
- Literales compuestos → `Pending` (F4.5).

**Criterios de aceptación:**
- [ ] El comparador coincide con `ir::num::compare` en un proptest de lexemas de RFC 8259, con exponentes largos y negativos.
- [ ] Tabla de `multipleOf` contra un oráculo racional: `0.01` con `0.1`/`0.10`/`1e-2`/`0.005`; `3` con `1e2` y `3e2`; `2.5`; exponentes de 10⁴ cifras. Un divisor por encima del tope de F4-U6 da `limit` con posición.
- [ ] `minLength`/`maxLength` cuentan `"😀"` y `"ñ"` como 1; un `enum` de cadenas con prefijos compartidos y la cadena vacía funciona; `1`, `1.0` y `1e0` son el mismo valor en `enum`/`const`; `enum` y `const` juntos funcionan.
- [ ] Conjunción: el ejemplo de `ir.md` §6.2 (`allOf` + `additionalProperties: false`) rechaza `{"a":1}` con 1 violación; `{"$ref":"#/$defs/a","minimum":3}` exige las dos cosas; la cadena de diamantes de 10⁴ nodos (`referencias_compartidas_tamano_lineal`) se compila con un número lineal de marcos (comprobando el resultado, no el tiempo, D1); con el tope bajado a 8, un producto mayor da `limit` antes de reservar.
- [ ] Todos los casos JSON del corpus sin `Pending` dan el `valid`, `violations` y `offset` esperados.

**Verificación:** `cargo test -p automaton -p runtime-rs`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** F4.1a; F4-U2, U4, U6.

**Archivos probables:** `crates/runtime-rs/src/{lib.rs,num.rs}`, `crates/automaton/src/{json.rs,conj.rs}`, `crates/automaton/tests/{escalares.rs,conjuncion.rs}`, `docs/fase4/automata.md`.

**Tamaño:** M.

**Arrastra:** `docs/fase3/pendientes.md` §5 L-1 (el mecanismo del tope, que F4.5 reutiliza; el caso de los 15 primos va a F4.4/F4.5) y S-3; §22, parte JSON (cerrar sin cambiar el alcance).

## Tarea F4.1c: XML: elementos, Glushkov con contadores, atributos y textos

**Descripción:**
- Marco del documento, con la `Choice` de los globales y la declaración `<?xml …?>`.
- Marcos de elemento con despacho por nombre local.
- Glushkov de cada `Content` sin determinizar (5.7-P1): transiciones de vuelta y de hermana; guardas de contador como comprobaciones; salida sin exigir el mínimo si el cuerpo es anulable; comprobación de determinismo como defensa en profundidad (`Internal`).
- Máscara de `xs:all`.
- `Attributes`: trie, máscara de requeridos, `fixed` comparado en el espacio de valores (5.6-P3); `default` no afecta a la validación.
- Textos: espacios léxicos como `Dfa` con *collapse*; `enum` por valor; rangos exactos y, en `Float`/`Double`, umbrales IEEE con NaN e INF (5.5-P3, CR-S1); longitudes.
- Contenido vacío sin espacios; contenido de solo elementos que ignora los espacios.
- Presupuesto de transiciones (F4-U6).

**Criterios de aceptación:**
- [ ] `valid-xml-pedido-01` da 0.
- [ ] `(a?){2,3}`: `<r/>`, `<r><a/></r>` y tres `a` son válidos; cuatro `a` dan 1 violación. `(a{1,2}, b){2,2}` exige sus mínimos.
- [ ] `xs:all` acepta cualquier orden; el hijo requerido que falta y el repetido dan violación.
- [ ] Atributos: el requerido que falta, el no declarado y el `fixed` distinto por valor dan violación; iguales por valor y por tanto válidos: `fixed="1"` en `xs:boolean` frente a `true`, y `+01` frente a `1` en `xs:int`; NaN en `fixed`; `xmlns` se ignora.
- [ ] `xs:float maxInclusive="0.1"` acepta `0.10000000000000000001` y rechaza el flotante siguiente; NaN viola toda faceta de rango; `INF` viola un máximo finito y cumple un mínimo.
- [ ] Una IR hecha a mano que viola la UPA da `Internal`, sin pánico. Con el presupuesto bajado, `(e₁|…|e₄₀₉₅)*` da `limit` antes de reservar.
- [ ] Un XSD con 10⁴ secuencias anidadas se construye en un hilo de 256 KiB, y el simulador recorre un XML de 10⁵ niveles.

**Verificación:** `cargo test -p automaton`, `make lint`, `make test`, `make docs-check`. Manual: `schemac automaton --dot` sobre el pedido.

**Dependencias:** F4.1b (comparador, trie, contadores); F4-U6.

**Archivos probables:** `crates/automaton/src/{xml.rs,glushkov.rs,text.rs}`, `crates/automaton/tests/{xml.rs,glushkov.rs}`, `docs/fase4/automata.md`.

**Tamaño:** M, en el borde alto. Si se pasa: F4.1c1 (elementos, Glushkov y atributos) y F4.1c2 (textos y `fixed`). Si no hay tiempo, D28 (2): facetas de rango de `xs:float`/`xs:double` como `Pending`.

**Arrastra:** `docs/fase3/pendientes.md` §36 (5.5-P3, CR-S1); §37 (5.6-P3, y 5.6-CR-S6: las enumeraciones repetidas por valor las absorbe el comparador, con una prueba); §38 (5.7-P1 y su presupuesto).
