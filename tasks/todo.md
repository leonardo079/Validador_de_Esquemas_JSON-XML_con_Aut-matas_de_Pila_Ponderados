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
- **Núcleo `no_std` del ejecutor en `runtime-rs` (F4-U9, D51)** —no un simulador std aparte—: lee `runtime_rs::Tables<'a>` aplanadas que construye y presta `automaton`, usa memoria del llamador, recibe eventos; los dos modos (conformidad total y fallo temprano). F4.1b/c, F4.4b, F4.5 y F4.6 añaden sus operaciones a este mismo núcleo.
- Formato de las tablas con los campos de F4.5a reservados desde ya: `slot` en cada comprobación (siempre 0 en F4.1), `slots` y `gates` en cada tipo de marco (vacíos), mapa de salidas en cada transición que apila (identidad).
- Forma de `Dfa` fijada (F4-B14: `{ n_classes, start, trans: Vec<u32> por filas, out: Vec<Out> }`, completo, con estado muerto explícito) para que F4.3 y F4.4b avancen en paralelo.
- Etiquetas de `Pending`: `pattern` → F4.4b; `anyOf`/`oneOf`/`not`/`dependentSchemas` → F4.5a; `uniqueItems` y literales compuestos → F4.5b.
- `location_clipped`.
- `schemac automaton [--dot] <esquema>` (F4-U7: resumen JSON de tamaños + DOT completo).
- `docs/fase4/automata.md` (definición formal, Σ y la pasada única, Γ y registros, δ con la propiedad de pila visible, pesos y costo, construcción por regla, complejidad) y `docs/fase4/pendientes.md`.

**Criterios de aceptación:**
- [x] Con el esquema `true`, el simulador acepta con w = 0 todos los payloads JSON bien formados del corpus, y da w = ∞ en el offset del evento que lo detecta con `{"a" 1}`, `[1,]`, `{,}`, `]`, `1 2`, `{"a":1}}` y la entrada vacía; el resultado es idéntico alimentando el lexer en bloques de 1, 7 y 4096 bytes.
- [x] Corpus: `valid-recursive-tree-01` da 0; `invalid-missing-required-01` da 1 violación, de la regla `required` del nodo correcto, en el offset 64; los casos `non-dpda` dan `Pending` con la tarea que los cubrirá (F4.4/F4.5), nunca aceptación.
- [x] Dorada verificable a mano: el DPDA de `{"type":"object","properties":{"a":{"type":"integer"}},"required":["a"],"additionalProperties":false}`, en `tests/` y como `docs/fase4/*.dot` generado por `make docs` y comprobado por `make docs-check`; el número de estados y transiciones que da `automata.md`, contado a mano, coincide con el de la prueba.
- [x] Propiedades: leyes del semianillo por proptest (asociatividad, conmutatividad, distributividad, 0 y ∞); δ total (cada (q, clase) alcanzable tiene una transición); entero: `1.0`, `1e2` y `10e-1` lo son, `1.5` y `1e-1` no, exponentes de 10⁵ cifras sin pánico; en fallo temprano, la violación informada es la de menor costo c.
- [x] D7: se construyen en un hilo de 256 KiB una cadena de 10⁵ `$ref` y un esquema recursivo productivo; el núcleo recorre un payload de 10⁵ niveles sin recursión.
- [x] F4-U9: el núcleo es `no_std` (sin `alloc`) y hace 0 reservas en el camino de validación (asignador contador en una prueba aparte, F4-B25).

**Verificación:** `cargo test -p automaton`, `make lint`, `make test`, `make docs-check`. Manual: `schemac automaton --dot` sobre la dorada, pasado por `dot -Tsvg`, y el resumen JSON.

**Dependencias:** PC-C, PC-D (hechas); F4-U1…U5, U7.

**Archivos probables:** `crates/runtime-rs/src/{lib.rs,tables.rs,core.rs}` (núcleo `no_std`), `crates/automaton/{Cargo.toml, src/lib.rs, src/weight.rs, src/model.rs, src/dfa.rs, src/json.rs, src/export.rs}`, `crates/automaton/tests/{json.rs,corpus.rs}`, `crates/cli/src/main.rs` y `crates/cli/tests/automaton.rs`, `Makefile` (`docs`, `docs-check`), `docs/fase4/{automata.md,pendientes.md}`.

**Tamaño:** M, en el borde alto. Si pasa de M, la CLI, la dorada y el DOT van a F4.1b.

**Arrastra:** `docs/fase3/pendientes.md` §4 (`location_clipped`); §5 S-1 (bits de `DependentRequired`; el hash de claves no declaradas queda documentado para F4.6); §27 (cerrar con la corrección de `clasificacion.md` §4.2); `ir.md` §7 (filas Σ y Γ corregidas); la dependencia `automaton → semantic` (F4-B1: `semantic` solo como dependencia de pruebas).

**Estado (16 nov):** APROBADA CON OBSERVACIONES en la revisión 1 (code-reviewer solo, D35: 0 Critical, 4 Important —I1, I2, I3 de documentación, corregidos en el cierre; I4 de cobertura, pasa a F4.1b por D38—, 12 Suggestion), **sin rondas de corrección**. Nada pasó a F4.1b: CLI, dorada y DOT incluidos. Casillas verificadas por el Tester 1 (`crates/automaton/tests/json.rs`: `true_acepta_json_bien_formado`, `true_rechaza_malformados_en_su_offset`, `dorada_contada_a_mano` —24 estados, 16 clases, 77 transiciones—, `delta_total_y_estados_alcanzables`, `entero_por_los_registros_del_numero`, `fallo_temprano_informa_la_de_menor_costo`, `d7_cadena_de_ref_y_payload_profundo`; `tests/corpus.rs`: `esquema_true_sobre_todo_el_corpus`, `arbol_recursivo_da_cero`, `falta_required_da_una_violacion_en_64`, `non_dpda_da_pending`; `tests/weight.rs` (proptest del semianillo); `tests/sin_reservas.rs::cero_reservas_al_validar`; `crates/cli/tests/automaton.rs`: `resumen_de_la_dorada`, `dot_de_la_dorada`; sondas adversarias de ~100 payloads × 30 esquemas contra `baseline`, sin aceptaciones en falso). Decisión del usuario (16 nov): el contador de reservas de `crates/automaton/tests/sin_reservas.rs` se mantiene como **única excepción nombrada** de `unsafe_code = "deny"`, vigilada por `make lint` (`unsafe-check`). Decisiones: D52. Commit `d0624d2`. Observaciones abiertas en `docs/fase4/pendientes.md` (§1-§20); las que heredan F4.1b y F4.6, como criterios en sus tareas.

## Tarea F4.1b: Registros y escalares JSON; conjunción por E₌ con tope

**Descripción:**
- Contadores: `minProperties`/`maxProperties`, `minItems`/`maxItems`, `minLength`/`maxLength` (en puntos de código, tras los escapes).
- `enum`/`const` de escalares: trie de cadenas, números por valor, `true`/`false`/`null`; `enum: []` (D11).
- `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum` y `multipleOf` exactos sobre el registro del lexema, con las rutinas numéricas `no_std` en `runtime-rs` (F4-U4, F4-B12).
- Marcos por conjunto canónico de `NodeId` (F4-U2): `$ref` con hermanas y `allOf` como conjunción, con el tope de 4 096 estados por producto, configurable y contado antes de reservar (L-1).
- Literales compuestos → `Pending` (F4.5b). El producto atribuye cada comprobación a su componente; el error de tope devuelve el número alcanzado y si el marco es escalar (D51).

**Criterios de aceptación:**
- [x] El comparador coincide con `ir::num::compare` en un proptest de lexemas de RFC 8259, con exponentes largos y negativos.
- [x] Tabla de `multipleOf` contra un oráculo racional: `0.01` con `0.1`/`0.10`/`1e-2`/`0.005`; `3` con `1e2` y `3e2`; `2.5`; exponentes de 10⁴ cifras. Un divisor por encima del tope de F4-U6 da `limit` con posición.
- [x] `minLength`/`maxLength` cuentan `"😀"` y `"ñ"` como 1; un `enum` de cadenas con prefijos compartidos y la cadena vacía funciona; `1`, `1.0` y `1e0` son el mismo valor en `enum`/`const`; `enum` y `const` juntos funcionan.
- [x] Conjunción: el ejemplo de `ir.md` §6.2 (`allOf` + `additionalProperties: false`) rechaza `{"a":1}` con 1 violación; `{"$ref":"#/$defs/a","minimum":3}` exige las dos cosas; la cadena de diamantes de 10⁴ nodos (`referencias_compartidas_tamano_lineal`) se compila con un número lineal de marcos (comprobando el resultado, no el tiempo, D1); con el tope bajado a 8, un producto de **marcos de objeto** mayor da `limit` antes de reservar.
- [x] Todos los casos JSON del corpus sin `Pending` dan el `valid`, `violations` y `offset` esperados.

**Verificación:** `cargo test -p automaton -p runtime-rs`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** F4.1a; F4-U2, U4, U6.

**Archivos probables:** `crates/runtime-rs/src/{lib.rs,num.rs}`, `crates/automaton/src/{json.rs,conj.rs}`, `crates/automaton/tests/{escalares.rs,conjuncion.rs}`, `docs/fase4/automata.md`.

**Tamaño:** M.

**Arrastra:** `docs/fase3/pendientes.md` §5 L-1 (el mecanismo del tope, que F4.5 reutiliza; el caso de los 15 primos va a F4.4/F4.5) y S-3; §22, parte JSON (cerrar sin cambiar el alcance).

**Heredado de la revisión de F4.1a (16 nov, D38; detalle en `docs/fase4/pendientes.md`):**
- [x] F4.1a-CR-I4 (cobertura): `BuildError::Limit` probado: 4 096 nombres por objeto compilan y 4 097 dan `limit`; un esquema por encima de 2²⁰ entradas de δ da `limit` sin reservar (`crates/automaton/src/json.rs:375`, `:395`).
- [x] F4.1a-CR-S1: al añadir contadores al marco, separar la máscara de los contadores (`mask_words` o un desplazamiento) para que `Test::Mask` no lea contadores (`tables.rs:146-154`, `core.rs:386-392`).
- [x] F4.1a-CR-S4: `expect("reservado")` (`json.rs:873,906`) y el bucle de `resolve_aliases` (`json.rs:159-176`) devuelven `BuildError::Internal` en vez de pánico o cuelgue.
- [x] F4.1a-CR-S10: proptest de `Number::is_integer` frente a `ir::num`.
- [x] F4.1a-CR-I1: las salidas de una instancia escalar (cadena, número, `true`/`false`/`null`) se diseñan como zona global de tamaño máximo por esquema y «cerrar la instancia» (`Pop`, `Leave` o la entrada de un valor de un evento), para que F4.5a no tenga que rediseñar (`docs/fase4/automata.md` §9.2).

**Estado (18 nov):** APROBADA en la revisión 4 tras **3 rondas de corrección**: ronda 1 (Tester: el tope de los productos de objetos se comprobaba después de reservar una matriz nombres × componentes —841 KB → 490 MB—; ahora tope y `spend` antes de los hijos, pruebas con el asignador contador y mutantes; además pruebas de `MAX_CLOSURE_WORK` y de `multipleOf` con lexemas largos); ronda 2 (code-reviewer: las tablas `checks`, `nums` y `lexemes` crecían multiplicativamente sin presupuesto —157 KB → 700 MB; 383 KB → 31 s y 2,3 GB— y casts `as u32` sin comprobar podían dar un veredicto incorrecto; ahora comprobaciones de entrada compartidas, una copia por (nodo, regla), lo multiplicativo descontado del presupuesto de δ de F4-U6 y `u32::try_from`); ronda 3 (code-reviewer: el tiempo crecía como conjuntos × |`enum`| —49 s y > 150 s—; ahora máscara de entrada y divisor memorizados por (nodo, regla): 0,3-0,4 s). Revisión 4: 0 Critical, 1 Important de documentación (factor ℓ de los nombres en la cota de tiempo de §7, corregido en el cierre), 4 Suggestion anotadas. Casillas verificadas por los Testers de las rondas 0-3 (`crates/runtime-rs/tests/num.rs`, `crates/automaton/tests/{escalares,conjuncion,sin_reservas,corpus}.rs`, `json::tests::enum_compartido_se_recorre_una_vez`; sondas diferenciales contra `baseline` de hasta 16 000 esquemas y ~113 000 payloads, calibradas, sin discrepancias). Decisiones: D53. Commit `33807b8`. Observaciones abiertas en `docs/fase4/pendientes.md` (§2-§29); lo que heredan F4.1c, F4.5a y F4.6, como criterios en sus tareas.

## Tarea F4.1c: XML: elementos, Glushkov con contadores, atributos y textos

**Descripción:**
- Marco del documento, con la `Choice` de los globales y la declaración `<?xml …?>`.
- Marcos de elemento con despacho por nombre local.
- Glushkov de cada `Content` sin determinizar (5.7-P1): transiciones de vuelta y de hermana; guardas de contador como comprobaciones; salida sin exigir el mínimo si el cuerpo es anulable; comprobación de determinismo como defensa en profundidad (`Internal`).
- Máscara de `xs:all`.
- `Attributes`: trie, máscara de requeridos, `fixed` comparado en el espacio de valores (5.6-P3); `default` no afecta a la validación.
- Textos: espacios léxicos como `Dfa` con *collapse* (el transductor de *collapse* es una pieza reutilizable: F4.4b lo usa delante del AFD de `pattern`, D51); `enum` por valor; rangos exactos y, en `Float`/`Double`, umbrales IEEE con NaN e INF (5.5-P3, CR-S1); longitudes.
- Contenido vacío sin espacios; contenido de solo elementos que ignora los espacios.
- Presupuesto de transiciones (F4-U6).

**Criterios de aceptación:**
- [x] `valid-xml-pedido-01` da 0. *(`corpus.rs::casos_xml_dan_lo_esperado`, `xml.rs::pedido_valido_e_invalidos`)*
- [x] `(a?){2,3}`: `<r/>`, `<r><a/></r>` y tres `a` son válidos; cuatro `a` dan 1 violación. `(a{1,2}, b){2,2}` exige sus mínimos. *(`glushkov.rs::opcional_con_contador`, `dos_contadores_con_reinicio`)*
- [x] `xs:all` acepta cualquier orden; el hijo requerido que falta y el repetido dan violación. *(`glushkov.rs::all_en_cualquier_orden`)*
- [x] Atributos: el requerido que falta, el no declarado y el `fixed` distinto por valor dan violación; iguales por valor y por tanto válidos: `fixed="1"` en `xs:boolean` frente a `true`, y `+01` frente a `1` en `xs:int`; NaN en `fixed`; `xmlns` se ignora. *(`xml.rs::atributos_fixed_por_valor_y_xmlns`, `fixed_infinito_en_flotantes`)*
- [x] `xs:float maxInclusive="0.1"` acepta `0.10000000000000000001` y rechaza el flotante siguiente; NaN viola toda faceta de rango; `INF` viola un máximo finito y cumple un mínimo. *(`xml.rs::float_umbral_ieee_nan_e_inf`, `text.rs::intervalos_de_redondeo`)*
- [x] Una IR hecha a mano que viola la UPA da `Internal`, sin pánico. Con el presupuesto bajado, `(e₁|…|e₄₀₉₅)*` da `limit` antes de reservar. *(`glushkov.rs::ir_que_viola_la_upa_da_internal`, `eleccion_ancha_da_limit`; `sin_reservas.rs::glushkov_ancho_da_limit_antes_de_reservar`)*
- [x] Un XSD con 10⁴ secuencias anidadas se construye en un hilo de 256 KiB, y el simulador recorre un XML de 10⁵ niveles. *(`glushkov.rs::d7_secuencias_anidadas_y_payload_profundo`, con IR a mano: el lector XSD limita a 256 niveles, `pendientes.md` §42)*

**Verificación:** `cargo test -p automaton`, `make lint`, `make test`, `make docs-check`. Manual: `schemac automaton --dot` sobre el pedido.

**Dependencias:** F4.1b (comparador, trie, contadores); F4-U6.

**Archivos probables:** `crates/automaton/src/{xml.rs,glushkov.rs,text.rs}`, `crates/automaton/tests/{xml.rs,glushkov.rs}`, `docs/fase4/automata.md`.

**Tamaño:** M, en el borde alto. Si se pasa: F4.1c1 (elementos, Glushkov y atributos) y F4.1c2 (textos y `fixed`). Si no hay tiempo, D28 (2): facetas de rango de `xs:float`/`xs:double` como `Pending`.

**Arrastra:** `docs/fase3/pendientes.md` §36 (5.5-P3, CR-S1); §37 (5.6-P3, y 5.6-CR-S6: las enumeraciones repetidas por valor las absorbe el comparador, con una prueba); §38 (5.7-P1 y su presupuesto).

**Heredado de la revisión de F4.1b (17 nov, D38; detalle en `docs/fase4/pendientes.md`):**
- [x] F4.1b-CR-S5: el marco admite hoy un solo contador (`Trans::count: bool`, disposición `[máscara][contador 0 o 1]`, `tables.rs:109-111`, `194-197`, `core.rs:398-416`); el Glushkov con contadores de 5.7-P1 necesita k contadores por elemento y su reinicio al volver a entrar: índice de contador en `Trans` y en `Check` más una operación de reinicio, documentado en `automata.md` §3.1. *(`glushkov.rs::dos_contadores_con_reinicio`, `opcional_con_contador`; `automata.md` §3.1)*
- [x] F4.1b-CR2-S3: el presupuesto de transiciones de F4.1c usa la misma cuenta que la precisión de F4-U6 de F4.1b (`docs/fase4/pendientes.md` §22.5): las transiciones y guardas del Glushkov entran en `extra` y se comprueban antes de reservar, para que la regla sea única en JSON y XML; y ningún recorrido por literal, cota o nombre se repite por conjunto (memo por (nodo, regla), como el arreglo de F4.1b ronda 3). *(`sin_reservas.rs`: 9 pruebas de `limit` antes de reservar y la propiedad `presupuesto_cubre_lo_emitido_en_parse_ok`; `automata.md` §15)*
- [x] F4.1b-CR-S6: antes de escribir `xml.rs`, extraer las utilidades genéricas de las tablas de `json.rs` (`Builder::{group, state, frame, trans, rule, check, checks, lexeme, trie_states}`, `trie_size`, `label`) a un módulo común, y partir `Analysis::set` (~235 líneas) en objeto, arreglo, número y cadena, para no duplicarlas. *(`emit.rs`; suite JSON, dorada y `.dot` sin cambios)*

**Estado:** hecha el 19 nov · 3 rondas + 1 ronda extra autorizada por el usuario (la 3.ª y la extra con un Builder nuevo) · revisada por `code-reviewer` y `security-auditor` (D35, decisión del usuario) · APROBADA. Reportes en `tasks/contexto/F4.1c reportes ronda {0,1,2,3,extra}.md`. Observaciones abiertas en `docs/fase4/pendientes.md` §33-§56 (con gravedad); lo que hereda cada tarea está como criterio en F4.3 y F4.6.

# Tarea F6.7: líneas base (pista B, Bayron; plan del Planner del 13 nov, aprobado el 13 nov con U1-U10)

Detalle, decisiones U1-U10 y B1-B11 en `tasks/plan.md` («F6.7: líneas base»). Una subtarea por vez: F6.7a primero.

## F6.7a: Línea base JSON (`serde_json` + `jsonschema`) sobre el corpus

**Descripción:** crate nuevo `crates/baseline` (U1). Funciones:
- lee el corpus (`baseline::corpus`);
- compila el esquema con `jsonschema` (Draft 2020-12 fijo, sin red, `format` como anotación, motor de `pattern` lineal);
- parsea el payload a un DOM `serde_json::Value` con un visitante propio que rechaza claves duplicadas (D2) y cuenta la profundidad (tope 256, `00-alcance.md` §4);
- valida y devuelve un `Outcome` comparable con `expected`.

La compilación del esquema, el parseo y la validación se pueden llamar por separado, para que F6.5 mida cada etapa. Todo lo recursivo corre en un hilo con `stack_size` explícito (D7). Desbloquea la mitad JSON de F4.6.

**Criterios de aceptación:**
- [x] El crate `crates/baseline` está en el workspace, sin `unsafe` y con la documentación pública escrita. Dependencias: `serde`, `serde_json = "1.0.154"`, `jsonschema = { version = "0.61.2", default-features = false }`, `base64 = "0.23.1"` y `schema-parser` (path).
  - `cargo tree -p baseline -e features -i serde_json` muestra solo `default`, `std`, `float_roundtrip` y `unbounded_depth` (U3).
  - `cargo tree -p baseline` no contiene `reqwest`, `rustls`, `aws-lc`, `ring`, `tokio` ni `cc`.
- [x] La API es la de U2. `JsonBaseline::compile` fija `draft202012`, `should_validate_formats(false)`, `PatternOptions::regex()` y un recuperador que siempre falla.
- [x] Prueba `crates/baseline/tests/corpus_json.rs`. Para cada caso JSON con bytes:
  - `valid` es igual;
  - `errorKind` se corresponde así: `lexical`/`syntax` → `Malformed`, `schema` → `Schema`, `limit` → `Limit`;
  - `violations` es igual, salvo los casos de una tabla explícita `DIVERGENCIAS` (id, campo, motivo); la prueba falla si una entrada de la tabla ya no diverge (U7);
  - el `pointer` esperado está entre los punteros de la línea base;
  - el offset de los errores léxicos y sintácticos es igual (hoy coinciden los 2);
  - exige que hayan corrido ≥ 8 casos, e informa y omite los de `generator`.
- [x] Tabla de casos unitarios. Todos los que recorren entrada corren dentro de `with_stack` (D7), y ninguna prueba mide tiempo (D1):
  - clave duplicada en la raíz, anidada y dentro de un arreglo → `Malformed` con offset; la misma clave en objetos distintos → válido (U5);
  - profundidad 256 → válido; 257 → `Limit`; 10⁵ `[` → `Limit` sin desbordar la pila;
  - BOM inicial → se ignora;
  - `1e400` → `Unsupported` (U9);
  - basura después del valor → `Malformed`;
  - `$ref` `http://` y `file://` → error de compilación sin red;
  - `pattern` con *lookahead* → error de compilación (prueba que el motor es el lineal);
  - `^(a+)+$` sobre `"aaaa…a!"` termina;
  - `format: "email"` no se valida.
- [x] Documentación:
  - `docs/fase6/linea-base.md`, parte JSON: DOM frente a streaming, costo O(n) en memoria y dos pasadas, configuración segura y tabla de divergencias (claves duplicadas, `1e400`, enteros > 2⁶⁴ vía `f64`, BOM, profundidad, recuento de violaciones, orden de errores, palabras clave que el proyecto rechaza y `jsonschema` acepta; U7 y U9);
  - `docs/fase6/pendientes.md` con el formato de `docs/fase2/pendientes.md`, con los pendientes iniciales de `tasks/plan.md`;
  - una línea en `tests/README.md` («la línea base recorre el corpus»).
- [x] `make lint` y `make test` en verde. En esta máquina, sin `make`: `scripts/lint.ps1` y `cargo test --workspace`.

**Verificación:** `cargo test -p baseline`; `cargo clippy -p baseline --all-targets -- -D warnings`; `make lint`; `make test`. Manual: `cargo tree` como en el primer criterio.

**Dependencias:** ninguna de código; U1, U2, U3, U5, U7 y U9 resueltas (13 nov).

**Archivos probables:** `Cargo.toml` (miembro y `workspace.dependencies`), `Cargo.lock`, `crates/baseline/{Cargo.toml,src/lib.rs,src/corpus.rs,src/json.rs}`, `crates/baseline/tests/corpus_json.rs`, `docs/fase6/{linea-base.md,pendientes.md}`, `tests/README.md`.

**Tamaño:** M. **Va primero.**

**Estado (15 nov):** APROBADA CON OBSERVACIONES tras **1 ronda de corrección** (devuelta por los Reviewers de la ronda 0: CR-I1 offsets de `corregir`, CR-I2 `JsonDom` recursivo fuera de `with_stack`, SEC-M1/M2 lista de violaciones y mensajes sin tope). Revisión de la ronda 1: code-reviewer APROBADO (1 Important de documentación, CR1-I1, cerrado en el cierre), security-auditor sin Critical/High/Medium. Casillas verificadas por el Tester de la ronda 1. Pruebas: `crates/baseline/tests/json.rs` (48), `tests/corpus_json.rs` (8) y 2 doc-tests; con las dos pistas mezcladas, 486 pruebas del workspace en verde. Decisiones D48-D50. Observaciones abiertas en `docs/fase6/pendientes.md` §1-§20 (§14 bloqueante para F7; §5 con el recuento de F4-U3; §19 `Drop` de `JsonBaseline`; §20 prueba multibyte del recorte, F6.5).

## F6.7b: Línea base XML (libxml2: DOM + XSD), tras la feature `libxml2`

**Descripción:**
- `libxml = { version = "0.3.23", optional = true }` detrás de la feature `libxml2` de `baseline`, apagada por defecto (U4), para que `make test` siga en verde en máquinas sin libxml2.
- `XmlBaseline::compile(xsd)`, `parse(payload)` (DOM) y `validate_dom`.
- Opciones del parser: `recover = false`, `no_net = true`, sin NOENT, DTDLOAD ni HUGE.
- Capa de política D8: cualquier nodo DTD → `Malformed` (U6).
- Se ejecuta en Linux o Docker con un objetivo del `Makefile`.

**Criterios de aceptación:**
- [x] Sin la feature, `cargo build --workspace --all-targets` y `cargo test --workspace` no piden libxml2 (verificado en la máquina Windows sin libxml2); sin la feature, `run_case` sobre un caso XML devuelve `Err(Unavailable)`, nunca un pánico.
- [x] Con la feature, `cargo test -p baseline --features libxml2` y `cargo clippy -p baseline --all-targets --features libxml2 -- -D warnings` pasan en Docker:
  - `valid-xml-pedido-01` → válido; `malformed-xml-billion-laughs-01` → `Malformed`;
  - casos unitarios: etiqueta sin cerrar → `Malformed` con línea, columna y offset; falta `id` requerido → `Schema` con 1 violación; `<total>abc</total>` → `Schema`; entidad externa `SYSTEM "file:///etc/passwd"` → `Malformed`, sin que el contenido del archivo aparezca en la salida ni en los errores; anidamiento de 300 niveles → error controlado sin caída; XSD con `xs:import` → error de compilación sin red.
- [x] Objetivo `make test-baseline-xml`: `docker run` con una imagen `rust` fijada, `apt-get install libxml2-dev libclang-dev pkg-config`, `CARGO_TARGET_DIR` aparte, prueba + clippy con la feature. Bayron lo ejecuta en verde; el comando y su salida quedan en la documentación.
- [x] `docs/fase6/linea-base.md`, parte XML: configuración segura, D8, cómo ejecutarlo y divergencias (`encoding` distinto de UTF-8 **rechazado** (B35, D54, coincide con `00-alcance.md` §1.2; antes se pensaba aceptarlo), offsets y XPath frente a `pointer`, recuento de errores de libxml2); pendientes nuevos en `pendientes.md`.
- [x] `make lint` y `make test` siguen en verde sin la feature.

**Verificación:** `make test-baseline-xml` (Docker); `cargo test --workspace` en Windows sin libxml2; `make lint`.

**Dependencias:** F6.7a (crate, `Outcome`, `corpus`), U4 y U6. Docker Desktop arrancado.

**Archivos probables:** `crates/baseline/{Cargo.toml,src/xml.rs,src/lib.rs}`, `crates/baseline/tests/corpus_xml.rs`, `Makefile`, `docs/fase6/{linea-base.md,pendientes.md}`.

**Tamaño:** M. Después de F6.7a.

**Estado (18 nov):** APROBADA tras **1 ronda de corrección** (devuelta por el Tester y los Reviewers de la ronda 0: codificación no UTF-8 con falso `Limit` y segunda capa de D8 sin prueba —CR-I1/SEC-M1/O1, CR-I3/O2—, y `es_limite` sin «too big» —CR-I2—). Revisión de la ronda 1: code-reviewer APROBADO, security-auditor APROBADA (0 Critical/High/Medium; 3 Low documentados). Casillas verificadas por el Tester de la ronda 1. Pruebas: `crates/baseline/tests/xml.rs` (41), `tests/corpus_xml.rs` (2), 3 unitarias en `src/xml.rs` y 3 doc-tests `compile_fail`, en Docker con `scripts/test-baseline-xml.ps1`; sin la feature, 575 pruebas en Windows con las dos pistas mezcladas (tras `77f02c2`). El `make test-baseline-xml` en Linux queda para Leonardo (punto de control). Decisión D54. Observaciones abiertas en `docs/fase6/pendientes.md` §21-§27 (§14 y §21 bloqueantes para F7; §26 por confirmar con Leonardo; §27 UCS-4).

## Punto de control tras F6.7a y F6.7b

- [ ] `make lint` y `make test` en verde en las dos máquinas; `make test-baseline-xml` en verde.
- [ ] Leonardo confirma que la API sirve para F4.6 (U2, U8).
- [ ] En el checklist, F4.6 pasa a depender de «F6.7a (JSON), F6.7b (XML)» (U10; la línea de F4.6 la edita Leonardo).

# Fase 4 — resto (F4.3, F4.4a/b, F4.5a/b, F4.6; plan del Planner del 15 nov, aprobado por Leonardo el 16 nov con F4-U9…U19 = D51)

Detalle y decisiones en `tasks/plan.md` («Plan del resto de la Fase 4»). Orden de la pista A: F4.1a → F4.1b → F4.1c → F4.6 → F5.1 → [PC-E] → F4.4b → F4.5a → F4.5b → F5.2 → F5.3. En paralelo (sin pisar la pista A; Bayron si tiene tiempo): F4.4a (sin dependencias) y F4.3 (tras el commit de F4.1a).

## Tarea F4.3: Verificación de determinismo de δ y minimización de Hopcroft de las partes finitas — HECHA (19 nov, D60)

**Descripción:**
- `automaton::minimize::hopcroft(&Dfa) -> Dfa`: partición inicial por salida (con el estado muerto), O(k·n log n), iterativo, numeración canónica (F4-B14, F4-B15).
- Se aplica a todo `Dfa` antes de volcarlo a δ: tries de claves y de `enum` de cadenas, espacios léxicos y, desde F4.4b, los AFD de `pattern`.
- `automaton::determinism::check(&M) -> Result<(), Vec<DetError>>`: (1) δ funcional y total, `TransId` en rango; (2) las clases de cada contexto son una partición de [0, 0x10FFFF] sin sustitutos; (3) pila visible: solo las clases de llamada apilan y solo las de retorno desapilan; (4) no hay ε (no existe la variante; se documenta); (5) un destino y una actualización por par en Glushkov y en el producto.
- El resumen de `schemac automaton` da los estados de los AFD antes y después de minimizar.
- `automata.md` §«Determinismo y minimización»: Myhill-Nerode; por qué no se minimiza el DPDA (en general no hay un DPDA ni un VPA mínimo canónico); costo.

**Criterios de aceptación:**
- [x] Proptest de AFD completos aleatorios (≤ 40 estados, ≤ 4 clases, ≤ 3 salidas): Hopcroft da el mismo número de estados que Moore y un autómata equivalente al original (salidas incluidas); es idempotente.
- [x] A mano: el AFD de subconjuntos de `(a|b)*abb` baja de 5 a 4 estados; el trie de `{"ab","cb"}` con el mismo destino comparte el sufijo y con destinos distintos no; los inalcanzables desaparecen.
- [x] `check` acepta todos los M de F4.1a-c y del corpus, y detecta sobre tablas hechas a mano un `TransId` fuera de rango, rangos que se solapan, un hueco en la partición y una transición que apila con una clase de retorno.
- [x] D7: un AFD en cadena de 10⁵ estados se minimiza en un hilo de 256 KiB.
- [x] La dorada de F4.1a se regenera si cambia y la cuenta a mano de `automata.md` se actualiza.

**Heredado de la revisión de F4.1c (19 nov, D38; detalle en `docs/fase4/pendientes.md`):**
- [ ] §39/§47/§53 (S1, S12): con la representación actual 2 000 literales en 200 elementos ya pasan de 2²⁰ (δ densa con |Σ| = 61, AFD del contenido simple copiado por marco, un trie de follow(x) por estado); `re2` (200 elementos de 40 tipos × 50 literales, 1 131 091 entradas) debe compilar: grupo global del texto de contenido simple con `Enter`/`Leave` como los atributos, trie compartido entre estados con la misma lista de aristas, partición de clases y Hopcroft. **Recortado** → `docs/fase4/pendientes.md` §63.
- [ ] §48 y §52 (S5, S8): partir `fill_frame` en `fill_empty`/`fill_simple`/`fill_model` y una sola contabilidad por pieza emitida (`elem` frente a `frame_cost`). **Recortado** → `docs/fase4/pendientes.md` §63.
- [ ] §56 (S13): `Builder::new` memoriza `classes_of` por conjunto de palabras (hoy por texto; `w4.xsd` 153 MB frente a 63 MB de `check`). **Recortado** → `docs/fase4/pendientes.md` §63.
- [ ] F4.1c-CRX-S4: una prueba que falle si el cobro de los nombres de follow(x) (`glushkov.rs:298`) se mueve detrás de la UPA. **Recortado** → `docs/fase4/pendientes.md` §63.
- [ ] F4.1c-CRX-S5 = F4E-I2: el tamaño del trie de atributos se memoriza por tipo en `ElemSpec` y Σ|nombre| de atributos se cobra una vez a los pasos. **Recortado** → `docs/fase4/pendientes.md` §63.
- [ ] F4D-M1 residual / F4D-I1: la cuenta conservadora de F4-U6 (≈ 60 por redondeo IEEE y los tres casos de §53) se ajusta si F4.3 cambia la representación. **Recortado** → `docs/fase4/pendientes.md` §63.

**Verificación:** `cargo test -p automaton`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** F4.1a (tipo `Dfa`, F4-B14).

**Archivos probables:** `crates/automaton/src/{minimize.rs,determinism.rs,lib.rs}`, `crates/automaton/tests/{minimize.rs,determinism.rs}`, `docs/fase4/{automata.md,pendientes.md}`.

**Tamaño:** S-M. **Revisión:** `code-reviewer` (D35).

## Tarea F4.4a: Analizador de `pattern` (dos dialectos) y rechazo en la Fase 3 — HECHA (20 nov, D61)

**Descripción:**
- `ir::regex` (F4-U15): `parse(src, Dialect::{Ecma, Xsd}) -> Result<Regex, RegexError{kind, at}>` sobre el subconjunto de F4-U14; AST en arena, análisis iterativo, anidamiento ≤ 256, ≤ 4 096 posiciones (F4-U16); `matches(&Regex, &str)` por simulación del conjunto de posiciones, O(|s|·m), sin retroceso (oráculo de F4.4b y de D29).
- `semantic` (meta-esquema): rechaza los patrones fuera del subconjunto o mal formados, con el constructo citado y el motivo (D6, F4-B19).
- `xsd`: analiza cada `xs:pattern` por separado antes de unirlas (5.6-P1, CR-S2) y aplica los rechazos propios de XSD.
- **(Si cabe; recorte 1)** D29: cada `xs:enumeration`, `default` y `fixed` se comprueba con `matches` contra las `pattern` de toda la cadena de bases; error `value` en la posición del valor; se quita la omisión de `00-alcance.md` §3.4 (interfaz compartida).
- `docs/fase4/pattern.md` (gramática, dialectos, motivo de cada rechazo, ReDoS) y cierre de `docs/fase3/meta-esquema.md` §8.

**Criterios de aceptación:**
- [x] Tabla de ≥ 30 filas con código y `at` esperados: ECMA acepta `^[A-Z]{3}-\d{4}$`, `^(?:a|b)+$`, `[^\n]`, `é`, `^a|b$`; ECMA rechaza `(a)\1`, `\k<n>`, `(?=a)`, `(?!a)`, `(?<=a)`, `(?<!a)`, `\b`, `a*?`, `a{2,}?`, `(?<n>a)`, `\p{L}`, `a^b`, `[z-a]`, `(a`, `a{3,2}`; XSD acepta `[A-Z]{3}`, `X+L`, `^a$` (literales); XSD rechaza `\w`, `\i`, `\p{Nd}`, `[a-z-[aeiou]]`, `(?:a)`.
- [x] `matches` cumple una tabla (patrón, cadena, resultado) que cubre el anclaje según el dialecto (`a` en ECMA encuentra `xay`; en XSD no), `.` frente a `\n` y a un astral (cuenta como uno), clases negadas y `{m,n}`.
- [x] Proptest sin pánico con cadenas arbitrarias; `a{999999999}` da `limit`; 10⁵ `(` en un hilo de 256 KiB dan un error, no un desbordamiento.
- [x] Esquemas nuevos en `tests/schemas/semantic-error/` (`pattern-lookahead`, `-retrorreferencia`, `-perezoso`, `-mal-formado`, `-demasiado-grande`) con su entrada en `errores-semanticos.json`; en el corpus XSD, `(a` y `b)` en el mismo paso dan dos errores; ningún `parse-ok` ni caso del corpus deja de pasar.
- [ ] (Si entra D29) `ZZZ` sobre `pattern="[a-z]"` da `value` en su posición. **Recortado** (recorte 1) → `docs/fase4/pendientes.md` §65.

**Verificación:** `cargo test -p ir -p semantic -p xsd`, `cargo test -p semantic --release`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** ninguna (puede empezar ya; no toca `automaton`).

**Archivos probables:** `crates/ir/src/{regex.rs,lib.rs}`, `crates/ir/tests/regex.rs`, `crates/semantic/src/meta.rs`, `crates/xsd/src/{simple.rs,lower.rs,error.rs}`, `tests/schemas/…` y sus `errores-*.json`, `docs/fase4/pattern.md`.

**Tamaño:** M (bajo sin D29). **Arrastra:** `docs/fase3/pendientes.md` §37 5.6-P1 y CR-S2 (y 5.6-P2); §38 5.7-P2; `meta-esquema.md` §8. **Revisión:** `code-reviewer`.

## Tarea F4.4b: `pattern` → AFD minimizado dentro de M — HECHA (20 nov, D62)

**Descripción:**
- `automaton::pattern`: `Regex` → Glushkov → subconjuntos sobre clases de puntos de código con el anclaje de F4-B18 → tope de 4 096 estados contado antes de reservar (`Limit` en la ubicación del `pattern`) → Hopcroft (F4.3) → `Dfa`.
- En M: en el marco de cadena en JSON (solo si la instancia es cadena) y en el texto en XSD (tras el *collapse* si el tipo lo pide); refina la partición de clases del contexto (F4-B3); varias `pattern` sobre la misma instancia se combinan con el producto de F4.1b; 1 violación por `Pattern` al cerrar. Se quita `Pending(pattern)`; el resumen da los estados de cada AFD.
- `pattern.md`: construcción, peor caso 2^m (`(a|b)*a(a|b){k}`), O(1) por carácter.

**Criterios de aceptación:**
- [x] Proptest: con ≥ 20 patrones (corpus, F4.4a y frontera), el AFD minimizado y `ir::regex::matches` coinciden en cadenas aleatorias con los caracteres del patrón, `\n` y un astral.
- [x] Tamaños: `^(a{p})*$` queda en p estados + el muerto para p ∈ {2, 3, 5, 7, 11}; `(a|b)*a(a|b){12}` da `limit` antes de reservar y con k = 8 compila; `^(a+)+$` queda en ≤ 3 estados y 10⁵ `a` seguidas de `!` se recorren con un evento por carácter (se cuentan eventos, D1).
- [x] Corpus: `non-dpda-pattern-01` da 0; caso nuevo `invalid-pattern-01`: 1 violación, `schema`, en el offset de la comilla de cierre.
- [x] XSD: `[A-Z]{3}` sobre `xs:string` acepta `ABC` y rechaza `ABCD` y ` ABC`; `[0-9]{3}` sobre `xs:int` acepta ` 123 `. JSON: `a` acepta `"xay"`; `^a` rechaza `"ba"`; `pattern` sobre un número da 0.
- [x] El resultado no cambia con bloques de 1, 7 y 4 096 bytes.

**Verificación:** `cargo test -p automaton`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** F4.1c (texto con *collapse*), F4.3, F4.4a.

**Archivos probables:** `crates/automaton/src/{pattern.rs,json.rs,text.rs,classes.rs}`, `crates/automaton/tests/pattern.rs`, `tests/cases/invalid/invalid-pattern-01.json`, `docs/fase4/pattern.md`.

**Tamaño:** M. **Arrastra:** D6; D46 (3); `docs/fase3/pendientes.md` §5 L-1. **Revisión:** `code-reviewer`.

## Tarea F4.5a: Combinadores — producto con salidas y puertas al desapilar — HECHA (20 nov, D63)

**Descripción:**
- Generaliza el producto de F4.1b según F4-U10, U12 y U13: salidas por rama para `anyOf`, `oneOf`, `not` y `dependentSchemas`; mapa de salidas en las transiciones que apilan; puertas ANY, ONE, NOT, IMPLIES al desapilar. Un límite o un malformado (∞) termina antes y nunca se invierte (S-2).
- Tope de 4 096 contado antes de reservar; al pasarlo, según F4-U11: simulación paralela en marcos escalares y `Limit` con el tamaño en los marcos con hijos.
- El resumen da la estrategia efectiva de cada regla (c) (`product`, `bounded-determinization` o `deferred`, con estados o «pasó el tope en N»). Fallo temprano: solo detienen las salidas globales.
- `automata.md` §«Combinadores»: cierre de los autómatas de pila visible bajo intersección y complemento; el circuito; el homomorfismo a 𝔹; tamaños. Medir `$ref` con hermanas como producto (§22) y, si procede, proponer el cambio de su fila.

**Criterios de aceptación:**
- [x] Corpus: `non-dpda-anyof-ambiguous-01` y `non-dpda-allof-not-01` dan 0 y la estrategia del resumen es la de su campo `strategy`. Casos nuevos en `non-dpda/`: `oneof` válido, `oneof` con las dos ramas (1), `anyof` sin ninguna (1), `not` violado (1). Veredictos sí; la estrategia del resumen queda en el recorte 5 (`pendientes.md` §70).
- [x] Semántica: `anyOf` con ramas que fallan 2 y 3 veces cuenta 1 y el mismo `allOf` cuenta 5; `not:{type:string}` da 0 sobre `1` y 1 sobre `"x"`; `not` de una cadena que pasa el límite da `limit`; `oneOf` con dos ramas iguales que aceptan da 1; `dependentSchemas {"a":{"required":["b"]}}`: `{"b":1}` → 0, `{"a":1}` → 1, `{"b":1,"a":1}` → 0; un `anyOf` anidado bajo `properties` de una rama cuenta en la rama correcta.
- [x] 15 primos (§5 L-1): `allOf` de 15 `^(a{p})*$`; el constructor se corta al pasar 4 096 (lo comprueba su contador, no el tiempo); resultado `deferred` (o `limit` con el recorte 3); `""` → 0, `"aa"` → 14, `"a"×30` → 12. Con el recorte 3: `limit` en el estado 4 097 (`quince_primos_dan_limit`).
- [x] Fallo temprano: en `anyOf` la rama que falla no lo detiene; en `allOf` se detiene en la violación de menor costo.
- [x] Proptest: `anyOf:[S]` y `not:{not:S}` dan el mismo veredicto (0 o ≥ 1) que S sobre payloads pequeños con esquemas de F4.1a-b. La cadena de diamantes de 10⁴ con `anyOf` compila con un número lineal de marcos.

**Verificación:** `cargo test -p automaton -p runtime-rs`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** F4.1b; F4.4b (solo para el caso de los 15 primos).

**Archivos probables:** `crates/automaton/src/{product.rs,gates.rs,export.rs}`, núcleo de `crates/runtime-rs` (salidas y puertas), `crates/automaton/tests/combinadores.rs`, `tests/cases/non-dpda/*`, `docs/fase4/automata.md`.

**Tamaño:** M (alto). **Arrastra:** §5 L-1, S-2, S-3; D5; D46 (4) enmendado por D51; §22 (parte JSON); `clasificacion.md` §4.2 y §8. **Revisión:** `code-reviewer` (el núcleo lo revisa `security-auditor` en F4.6).

**Heredado de la revisión de F4.1b (17 nov, D38):**
- [x] F4.1b-CR3-S2: con F4-B20 la clave de un tipo de marco pasa a ser (`NodeId`, salida) y el mismo nodo aparece en más conjuntos: máscaras de entrada, `enum` ordenados, conjuntos de cadenas, divisores y lexemas siguen memorizados por (nodo, regla) sin depender de la salida; las comprobaciones por salida entran en `extra` antes de reservar; prueba sin reloj como `enum_compartido_se_recorre_una_vez` con un `enum` compartido por muchas ramas de `anyOf`.
- [x] F4.1b-CR-S4: `Automaton::stack_words` (`model.rs:181-189`) suma `scalar_slots` (hoy fijado a 0 en `model.rs:115`), que pasa a ser un campo de `Automaton`, para que `validate` no se quede corta cuando la zona escalar tenga tamaño.

## Tarea F4.5b: `uniqueItems` diferido y literales compuestos — HECHA (20 nov, D64; literales en el recorte 2)

**Descripción:**
- `runtime-rs`: SipHash-1-3 y hash canónico en *streaming* (números por valor, cadenas por punto de código, arreglos en orden, objetos sin orden por suma modular de H(clave‖valor)), con un registro de hash por marco; una reserva fija usada como pila (F4-B22, F4-U16): cada arreglo con `uniqueItems` ordena su región al leer `]` y, si hay dos iguales, da 1 violación en ese `]`; más de 4 096 elementos → `limit`.
- Literales compuestos de `enum`/`const` traducidos a autómatas exactos con la puerta ANY (F4-U17), contando 1 violación; se quita su `Pending`.
- `automata.md` §«Verificación diferida»: por qué `uniqueItems` no es libre de contexto, cota de colisión y comportamiento bajo `not`.

**Criterios de aceptación:**
- [x] `non-dpda-unique-items-01` da 0 con estrategia `deferred`; casos nuevos: `[1, 2, 1.0]` → 1 violación en el `]`; objetos iguales con las claves en otro orden → 1.
- [x] `uniqueItems`: `[[1,2],[2,1]]` → 0; `["a","a"]` → 1; `[1,"1"]` → 0; `[true,1]` → 0; 4 097 elementos únicos → `limit` en el 4 097; arreglos anidados con `uniqueItems` comparten la reserva y cada uno comprueba solo su región.
- [ ] Literales: `const:{"a":[1,{"b":null}]}` acepta `{"a":[1.0,{"b":null}]}` y rechaza con 1 `{"a":[1,{"b":null}],"c":1}` y `{"a":[1]}`; `enum:[{"a":1},[1],2]` acepta los tres y rechaza `{"a":2}` con 1; un `const` de 300 claves compila con un número lineal de estados. **Recortado** (recorte 2) → `docs/fase4/pendientes.md` §72.
- [x] Proptest: valores iguales por valor (claves reordenadas, números reescritos) dan el mismo hash; `uniqueItems` coincide con un oráculo std (`serde_json::Value` normalizado) en arreglos pequeños. (`coincide_con_el_oraculo` en `unique.rs`).

**Verificación:** `cargo test -p automaton -p runtime-rs`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** F4.5a (puerta ANY), F4.1b (comparador numérico).

**Archivos probables:** `crates/runtime-rs/src/{hash.rs,pool.rs}` y el núcleo, `crates/automaton/src/{literal.rs,json.rs}`, `crates/automaton/tests/unique.rs`, `tests/cases/{non-dpda,invalid}/*`, `docs/fase4/automata.md`.

**Tamaño:** M (bajo). **Arrastra:** §5 S-1 (la reserva, que F4.6 reutiliza para las claves no declaradas) y S-2. **Revisión:** `code-reviewer` (la reserva la audita `security-auditor` en F4.6).

## Tarea F4.6: Ejecutor con lexer, límites, reporte y buena formación (oráculo; cierre de la Fase 4) — HECHA (19 nov, D59)

**Descripción** (con F4-U9):
- API de F4-U18 sobre el núcleo `no_std`, con los lexers `no_std` de JSON y XML delante (eventos → clases, una pasada). Memoria del llamador (`Tables::state_words(&Limits)`).
- Límites de `00-alcance.md` §4 al validar, con `Limits::default()` igual al alcance: profundidad 256; cadena 1 MiB; lexema 1 024; claves no declaradas 1 024 por objeto con la reserva común; `uniqueItems` 4 096; referencias XML 100 000 (F4-B23); tamaño del payload sin límite por defecto.
- Reporte de F4-U18, con `location_clipped` guardado en las tablas.
- Capa de buena formación (F4-B7): claves no declaradas y atributos repetidos (reserva de F4.5b o, si F4.5b no está, una reserva propia equivalente); nombre de cierre de un elemento saltado (hash en su marco); fin de entrada con marcos abiertos → `syntax`.
- `automaton::validate` (F4-B24).
- `docs/fase4/runtime.md` (API, memoria —y la tabla de tamaños de `docs/fase1/lexico.md` si cambia—, límites, reporte, argumento O(n)/O(d)); filas de la tabla ρ de `docs/fase3/pendientes.md` §1 para los mensajes del payload.
- Comparación con la línea base de Bayron (`crates/baseline`, D48; dependencia **solo de pruebas** de `automaton`, nunca de `runtime-rs`) en JSON, y con `expected` del corpus en XML mientras no esté F6.7b.
- Fuera de esta tarea: el puntero de la instancia es F6.1; `sv_limits_t`, el tiempo y el campo `limits` del corpus son F6.2.

**Criterios de aceptación:**
- [x] Todo el corpus (casos actuales y los de F4.4b-F4.5b que existan): `valid`, `errorKind`, `violations` y `offset` iguales a `expected`, con un `Report` idéntico en bloques de 1, 7 y 4 096 bytes. — `todo_el_corpus_da_lo_esperado` (los `non-dpda` siguen `Pending` hasta F4.4b-F4.5b)
- [x] Con F6.7a (hecha): el veredicto coincide con `baseline::JsonBaseline` en los casos JSON (las divergencias de conteo de F4-U3/U13 declaradas en `DIVERGENCIAS`); en XML, con `expected` (y con libxml2 si F6.7b está). — `json_coincide_con_la_linea_base`; divergencia declarada: `extreme-big-number-01` (§60)
- [x] Casos nuevos de las categorías del PDF que faltan: `invalid-type-01`, `invalid-additional-properties-01`, `invalid-enum-01`, `invalid-range-01`; dos `invalid-xml-*` (rango; hijo requerido); `malformed-json-duplicate-key-01` (clave no declarada repetida), `malformed-json-syntax-01` (`{"a" 1}`), `malformed-xml-unclosed-01`; `extreme-depth-01` (257 `[` → `limit` en el 257.º), `extreme-big-number-01` (1 025 cifras → `limit`).
- [x] Cero reservas entre `new` y `finish` (asignador contador, F4-B25) en todo el corpus y en 10⁵ niveles con `max_depth` = 10⁵, sin recursión. — `cero_reservas_en_todo_el_corpus_y_en_profundidad`
- [x] Cada límite se prueba en su valor y en su valor + 1, con el offset esperado; `limit` atraviesa `not` sin invertirse (cuando F4.5a esté). — `limites.rs`; lo de `not`, en F4.5a
- [x] El fallo temprano detiene `feed` en la violación de menor costo; la conformidad total sigue hasta el final; los dos dan el mismo veredicto en el corpus. — en el corpus (`todo_el_corpus_da_lo_esperado`) y `fallo_temprano_informa_la_de_menor_costo`
- [x] Proptest de payloads JSON y XML aleatorios y de mutaciones de un byte: sin pánico y con el mismo `Report` en los tres tamaños de bloque. — `aleatorio.rs`

**Verificación:** `cargo test -p runtime-rs -p automaton`, `make lint`, `make test`, `make docs-check`.

**Dependencias:** F4.1c (F4-U19). F6.7a (hecha) para la comparación JSON; F6.7b opcional.

**Archivos probables:** `crates/runtime-rs/src/{validator.rs,limits.rs,report.rs,wf.rs}`, `crates/automaton/src/validate.rs`, `crates/automaton/tests/{corpus.rs,limites.rs,sin_reservas.rs}`, unos 10 casos en `tests/cases/`, `docs/fase4/runtime.md`, `docs/fase3/pendientes.md` §1.

**Tamaño:** M (alto). Si se pasa: F4.6a (JSON, límites, reporte) y F4.6b (XML y buena formación XML).

**Arrastra:** F4-B7; §5 S-1 (D2); §4 e `ir.md` §11 pendiente 2 (`location_clipped` y ρ); «Terminado cuando» de la Fase 4; riesgo 9 del plan de F4.1.

**Revisión:** `code-reviewer` + `security-auditor` (D35: payload no confiable), sobre el núcleo entero: índices y saturaciones, reservas y regiones, DoS por claves o atributos, colisiones de hash, profundidad.

**Heredado de la revisión de F4.1a (16 nov, D38; detalle en `docs/fase4/pendientes.md`):**
- [x] F4.1a-CR-S2 (seguridad): `step` da `Internal` si la clase de carácter es ≥ `n_classes` con tablas incoherentes, en vez de leer la fila del estado siguiente (`core.rs:205,264-268`).
- [x] F4.1a-CR-S3: `automaton::validate` toma el tamaño de la memoria de `Limits` (hoy reserva (nº de `{`/`[` + 1) × (marco máximo + 1) palabras, hasta 520 B por byte con 4 096 nombres; `lib.rs:131-141`). — `validate_with`
- [x] F4.1a-CR-S9: `Options::new(mode)` o `Default` antes de añadir `Limits` (`core.rs:30-33`).
- [x] F4.1a-CR-S7: los dos modos se comparan sabiendo que, en fallo temprano, un payload truncado tras una violación da `schema` y en total `syntax`. — `runtime.md` §3

**Heredado de la revisión de F4.1b (17 nov, D38):**
- [ ] F4.1b-CR-S3 (rendimiento): al cerrar cada número, `compare` e `is_multiple` cuestan O(|cota del esquema|) (hasta 16 MiB): en compilación, quitar los ceros a la izquierda del exponente, decidir al compilar la comprobación si |E| de la cota pasa del rango alcanzable por un lexema de 1 024 bytes, y guardar en `DivisorRef` el e_d ya saturado, para que cada número cueste O(`LEXEME_MAX`). **Recortado** (orden de recorte): pasa a F6.5/E3, `docs/fase4/pendientes.md` §61.
- [x] F4.1b-CR-S7: S-3 cuenta un nodo repetido una sola vez (`allOf:[{$ref:x},{$ref:x}]` con x violado da 1; `jsonschema` da 2): declararlo en `DIVERGENCIAS` junto a las de D47 cuando entre al corpus. — ningún caso del corpus lo usa todavía; queda en la lista de `DIVERGENCIAS` si entra

**Heredado de la revisión de F4.1c (19 nov, D38; detalle en `docs/fase4/pendientes.md`):**
- [x] §33 = F4C-S3/I2 (Medium; antes de F6.7b para el oráculo): la capa de buena formación compara el nombre de cierre completo (prefijo incluido, hash del QName en el marco) en todo elemento, y rechaza `xmlns`/`xmlns:p` repetidos en la etiqueta y `:` inicial o final (`<p:a></q:a>`, `<a></p:a>`, `<:pedido>…</:pedido>` hoy dan peso 0).
- [ ] §34 (Medium): espacios de nombres del payload por nombre local, `<x:pedido xmlns:x="urn:otro">` aceptado, `xsi:*`/`xml:lang` suman 1; según lo que decidan las dos personas (desviación en `00-alcance.md` §1.2 o tabla de prefijos acotada). **Sin cambiar**: necesita la decisión de las dos personas (§34, §62).
- [x] §35 = F4C-S5 (Low): normalizar `\r\n`/`\r` → `\n` (XML 1.0 §2.11) en texto y `\t\n\r` → espacio en atributos antes de `length`/`enum`/`fixed` de `xs:string`. — falta contrastar con libxml2 en Docker
- [x] §13/§45 = F4C-S6/F4D-M3 (Medium; **High** en cuanto F6.5 o F7 llamen a `validate`): profundidad desde `Limits` (256) y `limit` antes del `vec!`; prueba con 1 MB de `<` sobre un XSD de marco grande (hoy pide 3,2 GB y aborta).
- [x] §44 = F4C-S4 (Low): `step` da `Internal` con clase ≥ `n_classes` y `Core::new` valida `n_classes ≥ class::CHAR + 1` (amplía F4.1a-CR-S2).
- [x] §46 = F4C-S7 (Low; también F5.2): `words.checked_add(1)` en `push` (32 bits).
- [x] §49 (Info, informe): el lexema de 1 024 bytes da `limit` en `xs:decimal`/`xs:integer` con más cifras y facetas numéricas. — documentado en `runtime.md` §3

## Punto de control PC-E (tras F4.6 + F5.1)

- [ ] Un esquema JSON y el pedido XSD pasan por esquema → M → validador Rust generado, y el corpus pasa.
- [ ] La Fase 4 queda cerrada al terminar F4.5b (sin ningún `Pending`). **Estado (20 nov):** F4.5b hecha; queda `Pending` solo para los literales compuestos de `enum`/`const` (recorte 2, §72), según lo que decidan las dos personas.

# Tarea F7.1: backend axum (pista B, Bayron; plan del Planner del 18 nov, aprobado el 18 nov con U1-U8)

Detalle, decisiones U1-U8 y B1-B9 en `tasks/plan.md` («F7.1: backend axum»).

## F7.1: `POST /api/schemas` y `GET /api/health` (axum)

**Descripción:**
- Crate `server` (biblioteca `src/lib.rs` con `router(Config)` y binario `src/main.rs`).
- `POST /api/schemas` recibe el esquema crudo en el cuerpo y detecta el formato por el contenido (D22).
- Las Fases 2 y 3, la bajada a `VerifiedIr`, la exportación JSON v2 (D17) y el resumen del autómata (F4-U7) corren en un hilo propio con `stack_size` de 8 MiB (D7).
- Topes: subida 1 MiB; tiempo 10 s → 504; concurrencia 2 compilaciones → 503 sin cola; errores devueltos: los 100 primeros más el total; IR: 4 MiB, por encima se omite.
- El tope de memoria por proceso queda para F7.7 (cgroup de Docker) y F7.3 (proceso hijo); el riesgo se declara (U4).
- `crates/cli` no se toca (U2). Nada de Mongo (F7.2), sandbox (F7.3), validate/WS (F7.5) ni Dockerfile (U7).

**Criterios de aceptación:**
- [x] `server/Cargo.toml`: axum 0.8.9 sin features por defecto (`http1`, `tokio`, `json`), tokio 1.54.1 con `rt-multi-thread`, `net`, `sync`, `time`, `signal` (más `macros` si se usa `#[tokio::test]`), serde_json 1.0.154 sin features nuevas (D49) y `schema-parser`, `semantic`, `xsd`, `ir`, `automaton` del workspace; `tower` 0.5.3 con `util` solo en `[dev-dependencies]`; se quita `codegen`, que no se usa; sin openssl ni TLS; `Cargo.lock` regenerado con `cargo build`; ningún perfil con `panic = "abort"` (§2.2).
- [x] `POST /api/schemas` con un JSON Schema válido → 200 (sin `Content-Type` obligatorio) con `apiVersion: 1`, `id`, `format: "json"|"xml"`, `summary` (el mismo texto que `schemac check`), `ir` (objeto IR v2 byte a byte igual al de `ir::write_json`, comprobado contra una dorada de `tests/ir/`), `irBytes` y `automaton` con `{status:"ok", summary:{…F4-U7…}}` o `{status:"pending"|"limit", message}`.
- [x] Un XSD válido (p. ej. `tests/schemas/xsd/parse-ok/pedido.xsd`) da 200 con `format: "xml"`, la IR y `automaton.status = "pending"` hasta F4.1c. No es un error del esquema.
- [x] Un esquema inválido da 422 con `{error:"schema", format, errors:[{code,line,column,message}], total, truncated}`: errores de la primera etapa que falla, como en `check`; `message` sin el prefijo de posición; con más de 100 errores, los 100 primeros y `total` cuenta todos (prueba con ≥ 150 errores, p. ej. `type` con 150 `""`); tamaño de la respuesta acotado (§1).
- [x] Cuerpo vacío o sin UTF-8 → tratado como JSON con su error léxico o de parser → 422; sin pánico ni 400.
- [x] Cuerpo de tope + 1 bytes → 413 con `{error:"too-large", limit}` en JSON (no el texto de axum); también cuando `Content-Length` lo anuncia.
- [x] Hilo: la compilación corre en `std::thread::Builder::new().stack_size(8 << 20)`; AST, `Analysis`/`SymbolTable` e IR se crean y liberan dentro del hilo, que solo devuelve `String`s y estructuras propias (D7, D36); el resultado vuelve por `tokio::sync::oneshot`; nunca se compila en un hilo de tokio.
- [x] Pánico → 500 con mensaje fijo, y la petición siguiente al mismo `Router` da 200 (§2.1); prueba con una compilación inyectada que hace pánico.
- [x] Error interno de `verify` o del constructor (`BuildError::Internal`) → 500 con un único mensaje fijo (D15); el detalle acotado (`InternalError::detail(8)`) va al log en stderr, sin `eprintln!` que pueda hacer pánico (§14).
- [x] Tope de tiempo configurable; prueba con tope de 50 ms y una compilación inyectada bloqueada en un canal hasta que la prueba la suelta → 504 `{error:"timeout"}`; la aserción no mide tiempo de pared (D1); el permiso del semáforo se mueve al hilo y solo se libera cuando el hilo termina (tras el 504, mientras siga bloqueado, otra petición con concurrencia 1 da 503; al soltarlo, 200).
- [x] Concurrencia: semáforo con `try_acquire_owned`; lleno → 503 `{error:"busy"}` con `Retry-After`; prueba determinista con concurrencia 1 (la primera petición se bloquea y avisa por canal; la segunda da 503).
- [x] No poder crear el hilo → 503, y el permiso se libera.
- [x] IR escrita en un `fmt::Write` con tope (4 MiB por defecto) que falla al pasarlo; si lo pasa, `ir: null`, `irOmitted: true` y 200 (§2.4); prueba con un tope bajo.
- [x] `GET /api/health` → 200 `{status:"ok", version}`.
- [x] Configuración por variables de entorno con valores por defecto: `SERVER_ADDR` (127.0.0.1:8080), `MAX_UPLOAD_BYTES`, `COMPILE_TIMEOUT_MS`, `MAX_CONCURRENT_COMPILES`, `MAX_IR_BYTES`; un valor inválido hace fallar el arranque con un mensaje; apagado ordenado con Ctrl-C.
- [x] Pruebas con `tower::ServiceExt::oneshot`, sin red ni puertos; ninguna depende del tiempo de pared (D1); `cargo test -p server` en verde en Windows (con `--release` si Smart App Control bloquea) y en Docker.
- [x] Una prueba comprueba que el `formato` del servidor coincide con D22, con la misma tabla de casos que `crates/cli` (`formato_por_el_primer_byte_significativo`).
- [x] `docs/fase7/servidor.md`: contrato de la API; topes y por qué (≈ 165 × subida, 2 compilaciones ≈ 350 MiB, 1 MiB de XSD ≲ 2 s); qué cubre el aislamiento por hilo y qué no (un aborto por falta de memoria o desbordamiento de pila tumba el proceso); cómo arrancarlo.
- [x] `docs/fase7/pendientes.md`: memoria por proceso (→ F7.3/F7.7), `id` persistente (→ F7.2), IR en streaming (opción de §1), CORS (→ F7.4 si hace falta), §9, §10 y §15 de la Fase 3 (→ F7.4), unificar la tubería en un crate `pipeline` (U2), segundo llamador de `semantic::lower` (§13).
- [x] Al cerrar, se actualizan `docs/fase3/pendientes.md` §1, §2, §38 y `docs/fase2/pendientes.md` §6, §12: lo cerrado (truncado, subida, CPU, hilo, pánico) se marca resuelto y se deja abierto lo que sigue (`RLIMIT_AS`/cgroup).
- [x] `make lint` (`scripts/lint.ps1` + `unsafe-check`, sin `unsafe` nuevo) y `make test` (`cargo test --workspace`) en verde; las doradas y las pruebas de `crates/cli` no cambian.

**Verificación:** `cargo test -p server` (o `--release`); `cargo test --workspace`; `scripts/lint.ps1`; clippy en Docker (`cargo fetch --locked` primero); arranque manual `cargo run -p server --release` y `curl --data-binary @tests/schemas/xsd/parse-ok/pedido.xsd http://127.0.0.1:8080/api/schemas`; `git diff --stat` para ver que `crates/cli` no se tocó.

**Dependencias:** ninguna de código. Visto bueno de Leonardo al contrato (U1), al tope (U3, si va a `00-alcance.md`) y al `id` (U5) antes de cerrar (D42).

**Archivos probables:** `server/{Cargo.toml,src/main.rs,src/lib.rs,src/compilar.rs,src/api.rs,tests/api.rs}`, `Cargo.lock`, `docs/fase7/{servidor.md,pendientes.md}`, `docs/fase3/pendientes.md`, `docs/fase2/pendientes.md`.

**Tamaño:** M (≈ 4-5 h). Recorte si se va de tiempo: el tope de la IR (B7) y `automaton` como F7.1b, con `automaton` siempre `null`.
**Estado (19 nov):** APROBADA tras **3 rondas de corrección**: ronda 1 (Tester: 413 por `Content-Length` no inmediato; máximos de configuración), ronda 2 (code-reviewer: prueba del 504 dependiente del reloj; security-auditor: memoria de cuerpos sin acotar → presupuesto de 32 MiB y plazo de lectura de 10 s), ronda 3 (security-auditor: amplificación con `chunked` de 1 byte → lectura frame a frame en un búfer de lo reservado). Por decisión del usuario (proyecto académico; menos agentes), la ronda 3 la verificó el orquestador sin Tester ni Reviewers nuevos: `lint OK` y 612 pruebas del workspace en verde en Windows, `cargo test -p server --release` 11 + 26, `unsafe-check` limpio; la sonda de SEC2-H1 contra el binario quedó sin correr (Docker apagado; la cota la prueba `api::tests::frames_de_un_byte_en_un_solo_bufer`). Decisión D55. Observaciones en `docs/fase7/pendientes.md` §1-§12; contrato por confirmar con Leonardo (§9).

# Tarea F7.4: frontend, editor de esquema (pista B, Bayron; 19 nov, D56)

Hecha directamente por el orquestador, sin Planner/Tester/Reviewers (decisión del usuario del 19 nov: proyecto académico, menos agentes).

- [x] `web/` con React 18 + TypeScript + Vite y Monaco servido desde el bundle (sin CDN); versiones fijadas en `package-lock.json`.
- [x] Compila contra `POST /api/schemas` (D55) enviando el esquema crudo; formato detectado como el servidor (D22).
- [x] 200: resumen, `id`, resumen del autómata (o `pending`/`limit` con su mensaje), IR v2 cruda (sin redondear `u64::MAX`, §10 de la Fase 3) o aviso de IR omitida.
- [x] 422: errores (≤ 100 y total) marcados en el editor; pulsar uno lleva el cursor a su línea y columna. 413/408/503/504/500/red: mensaje y `Retry-After`.
- [x] Todo como texto, sin `innerHTML`: prueba con `<img src=x onerror=…>` en la IR, el resumen y los errores (`web/src/Resultado.test.tsx`).
- [x] `npm test` (18 pruebas), `tsc --noEmit` y `npm run build` en verde; probado de punta a punta con `cargo run -p server --release` + `npm run dev` (página, `/api/health`, XSD 200 y esquema con errores 422 por el proxy).

**Estado (19 nov):** hecha. Pendiente para F7.6: vistas del autómata, árbol de derivación y métricas; para F7.5: descargar el código y validar payloads.

# Tarea F6.6: benchmark del parser de esquemas (pista B, Bayron; 19 nov, D58)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] `crates/bench/benches/parser_claves.rs` con criterion 0.5.1 (sin `plotters` ni `rayon`): objetos de N = 10⁴ y 4N claves en `properties`, `const`, `$defs` y `dependentRequired`, con `Throughput::Elements`.
- [x] Corre con `cargo bench -p bench --bench parser_claves` (y `-- --test`); no entra en `cargo test` (D1).
- [x] Primera medida: t(4N)/t(N) = 5,0 / 5,4 / 4,6 / 4,8 (lineal; la cuadrática daba 26-28), anotada en `docs/fase2/pendientes.md` §1.
- [x] `lint OK` (fmt + clippy con `--all-targets`, que compila el benchmark).

**Estado (19 nov):** hecha.

# Tarea F6.5: arnés de métricas (pista B, Bayron; 20 nov, D65)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] Binario `metricas` en `crates/bench`: DPDA (M sobre `runtime-rs`, bloques de 4 KiB, memoria fija de los límites) frente a la línea base DOM (D48) en todo el corpus y en el bombeo (`[`ᵏ`]`ᵏ, n enteros, n objetos).
- [x] Mide mediana de tiempo, MB/s, ns/byte (ciclos/byte = ns/byte × GHz), asignaciones y pico de montón (`dhat` solo en el binario, sin `unsafe`), profundidad y |Q|/δ; tabla Markdown y CSV (`--csv`, para E1).
- [x] 8 pruebas deterministas en `crates/bench/tests/arnes.rs` (profundidad, generadores, DPDA = línea base = `expected` en el corpus, formato del informe); ninguna mide tiempo (D1).
- [x] Resultados y lectura en `docs/fase6/metricas.md`; `docs/fase6/pendientes.md` §2 y §11 anotados.
- [x] `lint OK`; 720 pruebas del workspace en verde (8 binarios bloqueados por Smart App Control repetidos en `--release`).

**Estado (20 nov):** hecha. Pendiente para F5.x: añadir el código generado como otra columna de validador; para E1: las gráficas contra k y n con el CSV.

# Tarea F5.4: generador de payloads válidos desde la IR (pista B, Bayron; 20 nov, D66)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] `crates/bench/src/generar.rs`: generador con semilla (splitmix64) que recorre la IR (`$ref`, `allOf`, rama de `anyOf`/`oneOf`, `const`, `enum`, `type`, cotas numéricas y `multipleOf`, longitudes, `pattern` por paseo en el Glushkov, `prefixItems`/`items`/`uniqueItems`, `required`/`dependentRequired`/`min/maxProperties`).
- [x] `generar_valido` solo acepta lo que dan por válido el DPDA y la línea base DOM (D48).
- [x] CLI `cargo run -p bench --release --bin generar [-- --escribir --n N --semilla S]`; 20 casos `tests/cases/valid/valid-gen-*.json` (`phase: 5`), que pasan `validar_formato` y las pruebas de corpus de todos los crates.
- [x] 7 pruebas deterministas en `crates/bench/tests/generar.rs` (misma semilla da el mismo payload; validez frente al DPDA y la línea base en 8 esquemas × 5 semillas; `pattern` por construcción; `const`; escapes; `Azar`; XSD → `None`).
- [x] Doc en `docs/fase5/generador.md`; `lint OK`; 727 pruebas del workspace en verde.

**Estado (20 nov):** hecha. Pendiente: generador XML (F5.4-P1, opcional en F6.3); F6.3 parte de estos válidos para mutarlos.

# Tarea F6.3: payloads inválidos y malformados por mutación (pista B, Bayron; 20 nov, D67)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] `crates/bench/src/mutar.rs`: `tramos` iterativo; 6 mutaciones semánticas y 7 sintácticas o léxicas, deterministas con la semilla de F5.4.
- [x] `clasificar`, el oráculo doble: `expected` sale del DPDA (modo total). El caso solo se acepta si la línea base da la misma clase y el mismo recuento u offset, si el DPDA da lo mismo en bloques de 1, 7 y 4096 bytes y en fallo temprano, y, si es malformado, también con el esquema `true`.
- [x] CLI `cargo run -p bench --release --bin mutar [-- --escribir]`: 20 `invalid-mut-*` y 10 `malformed-json-mut-*` (`phase: 6`), regenerables byte a byte; `payloadBase64` si el payload no es UTF-8.
- [x] 6 pruebas en `crates/bench/tests/mutar.rs`: tramos; las semánticas dejan JSON válido y las sintácticas no; la misma semilla da la misma mutación; base64 con los vectores de RFC 4648; `clasificar` con casos conocidos.
- [x] Doc en `docs/fase6/mutaciones.md`; hallazgo F6.3-P1 en `docs/fase6/pendientes.md` §28 (decide la pista A); `lint OK`; 733 pruebas del workspace en verde.

**Estado (20 nov):** hecha. Pendiente: F6.3-P1, para Leonardo; generador XML (F5.4-P1), opcional.

# Tarea F7.3: sandbox Docker para el código generado (pista B, Bayron; 20 nov, D68)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] Imagen `sandbox/` (`validador-sandbox:1`): Rust 1.100.0 por digest con `rustc`, `cc`, `tar` y `timeout`; usuario 10001; punto de entrada `sandbox-run` con las órdenes de compilación fijas (C11 y Rust) y `timeout` para compilar y para ejecutar.
- [x] `server::sandbox`: un contenedor por trabajo con `--network none`, `--read-only` + tmpfs, sin root, memoria sin swap, CPU, `--pids-limit`, `--ulimit`, `--cap-drop ALL`, `no-new-privileges` e `--ipc none`; entrada por tar sin montajes; plazo de pared con `docker kill`; salida recortada; nombres y tamaño validados.
- [x] 3 pruebas sin Docker en `cargo test` (argumentos, `comprobar`, tar ustar) y 8 con contenedores reales (`scripts/test-sandbox.ps1` o `make test-sandbox`): C y Rust, error de compilación, bucle infinito, sin red/root y solo lectura, memoria, bomba de `fork`, salida. Todas en verde.
- [x] Doc en `docs/fase7/sandbox.md`; `docs/fase7/pendientes.md` §1 y §12 anotados y §13-§14 nuevos; `lint OK`; 736 pruebas del workspace en verde (9 ignoradas).

**Estado (20 nov):** hecha. Pendiente: F7.3-P1 (caché del binario por esquema, F7.5) y F7.3-P2 (código generado sin dependencias, F5.1, pista A).

# Tarea F5.5: árbol de derivación de payloads (pista B, Bayron; 20 nov, D69)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] Crate `crates/derivation`: parser predictivo LL(1) con pila explícita sobre `json.ll1` y `xml.ll1`, con los tokens del lexer de la Fase 1. Los nodos llevan su rango de bytes, el lexema y el JSON Pointer; en XML, una pila de nombres comprueba que cada etiqueta cierre la suya.
- [x] Topes de 5 000 nodos y de 256 niveles de profundidad (`00-alcance.md` §4). El de profundidad evita que los punteros sumen O(n²) bytes; se encontró con una prueba de 10⁵ niveles.
- [x] Salidas iterativas: JSON para `d3.hierarchy` y DOT. Binario `arbol`; ejemplos `docs/fase5/arbol-pedido.{json,xml}.dot`, que regeneran `make docs` y comprueba `make docs-check` (`DERIV_DOT`).
- [x] 11 pruebas en `crates/derivation/tests/arbol.rs`: gramática, frontera, punteros, rangos, errores con offset, topes, JSON para d3, DOT, recorte y todo el corpus `valid`. Doc en `docs/fase5/arbol-derivacion.md`; `lint OK`.

**Estado (20 nov):** hecha. Pendiente: endpoint y vista con d3 (F7.6, con el visto bueno de Leonardo para la API, D42); los `.svg` de los ejemplos, con `make docs` donde haya Graphviz.

# Tarea F7.6: vistas del autómata, del árbol de derivación y comparación (pista B, Bayron; 20 nov, D70)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] Servidor: `POST /api/automaton` (grafo de M para Cytoscape, recortado a 400 nodos y 1 500 aristas), `POST /api/derivation` (árbol de F5.5) y `POST /api/compare` (DPDA frente a la línea base DOM).
- [x] Los tres usan `api::procesar`, extraído de `POST /api/schemas`. Las 26 pruebas de la API de F7.1 siguen verdes y hay 10 nuevas en `server/tests/vistas.rs`.
- [x] Web: pestañas Autómata (Cytoscape 3.30.2), Árbol de derivación (d3-hierarchy 3.1.2, SVG con React) y Comparación (tabla y barras). Todo se muestra como texto; el XSS está probado. Hay 18 pruebas nuevas de vitest (32 en total) y `npm run build` pasa.
- [x] Prueba de humo con el servidor real y `curl` en las tres rutas.
- [x] Doc en `docs/fase7/vistas.md`, más `servidor.md`, `web/README.md` y `pendientes.md` §15; `lint OK`; 757 pruebas del workspace en verde.

**Estado (20 nov):** hecha. Pendiente: visto bueno de Leonardo a los tres endpoints (D42); F7.6-P2, la CPU de `jsonschema` en `/api/compare` (`pendientes.md` §15).

# Tarea F7.7: despliegue en EC2 con Docker Compose y Caddy (pista B, Bayron; preparado el 20 nov, D71)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] `infra/Dockerfile.server` (release, Debian slim, sin root) y `infra/Dockerfile.web` (Vite + Caddy 2.8.4); `.dockerignore` sin secretos.
- [x] `infra/docker-compose.yml`:
  - `server`: `mem_limit` 600 MiB sin swap, `restart`, `read_only`, `cap_drop`, `no-new-privileges`, `pids_limit` y logs acotados;
  - `web`: puertos 80 y 443, volúmenes de certificados.
- [x] `infra/Caddyfile`: HTTPS por `DOMINIO` (`sslip.io`), HSTS, CSP, `nosniff`, `X-Frame-Options`, plazos y tope de 1 MiB.
  - El tope usa `request_buffers` y rechaza por `Content-Length` antes de leer, porque la prueba local encontró 502 con 1,5 MB y una conexión colgada con 3 MB.
- [x] Probado en local tras Caddy: health, web y SPA, compilar, comparar, cabeceras, y 413 en milisegundos con y sin `chunked`.
- [x] `infra/ec2-preparar.sh` (Docker, swap y clon) e `infra/desplegar.ps1` (construir aquí, `docker save`/`scp`/`docker load`, `up`, comprobar HTTPS); guía en `docs/fase7/despliegue.md`.
- [ ] **21 nov:** crear la EC2 y la Elastic IP, ejecutar `ec2-preparar.sh` y `desplegar.ps1`, y comprobar la URL pública.

**Estado (20 nov):** preparado. Falta el despliegue real del sábado.

# Tarea F7.2: casos de prueba en MongoDB Atlas (pista B, Bayron; 20 nov, D72)

Hecha directamente por el orquestador, sin agentes (decisión del usuario).

- [x] Crate `crates/atlas`: documento por caso (esquema exacto, payload como texto, base64 o generador, `expected`, `source` relativa) y conexión con plazos cortos.
- [x] Carga local `cargo run -p atlas` (*upsert* idempotente, `--podar`, `--en-seco`): 78 casos en Atlas.
- [x] Servidor: `GET /api/tests` y `POST /api/tests/run` (en `api::procesar`), con 503 sin base, 502 si Atlas falla y 400 si la categoría no es válida. Arranca igual sin `MONGODB_URI`.
- [x] Web: pestaña «Casos de prueba (Atlas)» con filtro por categoría, lista y ejecución.
- [x] Compose pasa el `.env` al servidor (opcional) y `desplegar.ps1` lo copia a la EC2 con permisos 600.
- [x] Driver con `bson-3`, porque bson 2 activaba `serde_json/preserve_order` en todo el workspace y rompía D49 y tres pruebas.
- [x] 9 pruebas nuevas sin red (`crates/atlas/tests/documento.rs`, `server/tests/casos.rs`) y 2 de vitest (34 en total). Verificado contra Atlas: 78/78 pasan, en Windows y en Docker tras Caddy. `lint OK`; 766 pruebas del workspace en verde.

**Estado (20 nov):** hecha. Pendiente: visto bueno de Leonardo a `/api/tests` (D42); E2 (métricas en Atlas); en la EC2, añadir su IP en Network Access de Atlas.
