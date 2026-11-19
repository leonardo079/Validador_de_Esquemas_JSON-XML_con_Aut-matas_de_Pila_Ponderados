# Pendientes de la Fase 6 (líneas base y métricas)

Observaciones que dejaron abiertas las tareas F6.7a (línea base JSON, `crates/baseline`;
teoría en [linea-base.md](linea-base.md) §1-§6) y F6.7b (línea base XML con libxml2,
feature `libxml2`; [linea-base.md](linea-base.md) §7) y que se resuelven en otra tarea. Las
decisiones de diseño viven en el «Registro de decisiones» del plan; aquí solo se
remite a ellas.

## Vigilar más adelante

### 1. Divergencias de escapes, `\d` y `encoding` frente al DPDA (F6.7a-P1)

- **Estado:** la línea base hereda el comportamiento de `serde_json` y del motor
  `regex`, que no siempre es el del proyecto:
  - un escape `\ud800` suelto en una cadena: `serde_json` lo rechaza como error
    sintáctico en la comilla que sigue (*unexpected end of hex escape*, offset 7 en
    `"\ud800"`; `escape_de_sustituto_suelto_es_malformado`). `00-alcance.md` §1.1 solo
    habla de sustitutos codificados en UTF-8, no de escapes ([linea-base.md](linea-base.md)
    §4, fila 12);
  - `\d` en `pattern`: en el crate `regex` es Unicode, pero `jsonschema` traduce las
    clases de ECMA-262 y la línea base lo trata como ASCII, igual que el AFD del
    proyecto (`00-alcance.md` §2.4; `d_en_pattern_es_ascii`). Queda comparar `\w`,
    `\s` y `.` sobre caracteres no ASCII (fila 13);
  - UTF-8 *overlong* (`C0 80`, `E0 80 80`…) y sustitutos codificados en UTF-8
    (`ED A0 80`): `00-alcance.md` §1.1 los declara error léxico; `serde_json` valida
    UTF-8 con `std` y también debería rechazarlos, pero no se comparó el offset con el
    lexer del proyecto (Tester ronda 0, hueco 2). El UTF-8 inválido simple sí está
    fijado: offset 2 en un valor y 3 en una clave
    (`utf8_invalido_en_un_valor_y_en_una_clave`);
  - en XML, un `encoding` distinto de UTF-8: libxml2 lo acepta; el proyecto lo
    rechaza (`00-alcance.md` §1.2). **Resuelto en F6.7b, ronda 1 (16 nov):** la
    primera versión lo aceptaba (y UTF-16 con BOM daba hasta un `Limit` falso, CR-I1,
    O1). Ahora la línea base sigue el alcance: libxml2 lee siempre UTF-8
    (`encoding: Some("UTF-8")`, `ignore_enc`, SEC-M1) y `politica_de_entrada` da
    `Malformed` a una entrada que empieza como UTF-16, UTF-32 o EBCDIC (offset 0) o
    que declara otro `encoding` (offset del valor); `utf-8` en cualquier caja se
    acepta ([linea-base.md](linea-base.md) §7.2 y §7.5, fila 2;
    `entradas_que_no_son_utf8_son_malformado`, `utf8_declarado_en_cualquier_caja_se_acepta`,
    `latin1_sin_declarar_es_malformado`, `xsd_que_no_es_utf8_no_compila`). Queda
    comparar con el intérprete **en qué byte** da el error el proyecto (la línea base
    lo da en el valor de `encoding`).
- **Arreglo propuesto:** F4.6 compara estos casos con el intérprete y, si divergen,
  los añade a `DIVERGENCIAS` (o a su lista de exclusiones) con el motivo. En XML no
  hay tabla de divergencias (`corpus_xml.rs`): si al entrar al corpus un caso
  `lexical` de `encoding` el offset del proyecto no es el valor, habría que añadirla.
- **Dónde:** `crates/baseline/src/json.rs:193` (`construir`: motor de `pattern`),
  `crates/baseline/src/json.rs:233` (`leer`: escapes y UTF-8),
  `crates/baseline/src/xml.rs:98` (`opciones`), `:326` (`politica_de_entrada`).
- **Lo que cambia para F4.6 (CR1-S2):** con B35, un caso XML que declare un
  `encoding` distinto de UTF-8 es `Malformed` en la línea base, con el offset en el
  **primer byte del valor** de `encoding` (30 en `<?xml version="1.0" encoding="…"?>`),
  y uno que empiece como UTF-16, UTF-32 o EBCDIC, en el offset 0 (o donde lo corte
  libxml2, §7.2 de [linea-base.md](linea-base.md)). La primera versión de la línea base
  los aceptaba. Es lo mismo que hace el lector XSD del proyecto
  (`crates/xsd/src/reader.rs:1073`); queda comprobar el lexer de payloads en F4.6.
- **Tarea:** F4.6 (JSON, y el offset en XML). La parte XML de F6.7b está **resuelta**.

### 2. Memoria de libxml2 fuera del asignador de Rust (F6.7a-P2)

- **Estado:** libxml2 reserva con `malloc` de C, que un `GlobalAlloc` de Rust no ve.
- **Arreglo propuesto:** F6.5 mide el pico del proceso (`VmHWM` de
  `/proc/self/status` en Linux) para la línea base XML. Para contar asignaciones de
  Rust sin escribir `unsafe` (prohibido en el workspace), un crate como `stats_alloc`
  o `dhat` en lugar de un asignador propio.
- **Cómo se mediría (F6.7b, 15 nov):** la línea base XML ya existe y solo corre en
  Linux (Docker, [linea-base.md](linea-base.md) §7.3), donde `VmHWM` está disponible.
  Lo que reserva libxml2 (el árbol, el esquema compilado, los diagnósticos) no pasa
  por Rust, así que el arnés debe medir cada etapa en un **proceso aparte** (o leer
  `VmHWM` antes y después, sabiendo que solo crece): `compile`, `parse` y
  `validate_dom` por separado, igual que en JSON. libxml2 permite además instalar sus
  propias funciones de reserva (`xmlMemSetup`), pero desde Rust exige `unsafe`, que el
  workspace prohíbe; no se propone.
- **Dónde:** — (el arnés de F6.5, `crates/bench`, aún no existe);
  `crates/baseline/src/xml.rs:199` (`compile`), `:251` (`parse`), `:268`
  (`validate_dom`).
- **Tarea:** F6.5. La parte de F6.7b (dejar dicho cómo se mide) está **resuelta**.

### 3. Tope de tamaño del payload en la línea base (F6.7a-P3)

- **Estado:** la línea base necesita el payload entero en memoria y además construye
  el árbol: O(n) bytes de entrada más el árbol, estimado entre 16 y más de 80 veces n
  según la forma del payload ([linea-base.md](linea-base.md) §1). No tiene tope de
  tamaño; el DPDA no lo necesita (`00-alcance.md` §4: «sin límite, streaming»).
- **Arreglo propuesto:** el arnés (F6.5) y el experimento de bombeo (E1) fijan un
  tope de bytes para la línea base (p. ej. una fracción de la RAM libre) y, por
  encima, registran «sin medición» en lugar de dejar que el proceso muera por
  memoria. Va en el arnés, no en `JsonBaseline::parse`, que recibe un `&[u8]` ya
  leído.
- **Dónde:** `crates/baseline/src/json.rs:124` (`JsonBaseline::parse`).
- **En XML (F6.7b):** igual. libxml2 recibe el payload entero (`&[u8]`) y además
  tiene un tope propio de `i32::MAX` bytes, que la línea base da como `Kind::Limit`
  ([linea-base.md](linea-base.md) §7.5, fila 16). El árbol de libxml2 (nodos de C de
  unos 120 bytes) lo mide F6.5 por proceso (§2).
- **Tarea:** F6.5, E1.

### 4. Unificar los lectores del corpus (F6.7a-P4, opcional)

- **Estado:** hay tres lectores de `tests/cases`: `crates/lexer/tests/common/mod.rs`
  (con `validar_formato`), `crates/semantic/tests/common/mod.rs` (texto original del
  esquema) y `baseline::corpus` (tipado, B7). Desde la ronda 1, `baseline::corpus`
  rechaza además los dos puntos y la barra invertida en `schemaFile`/`payloadFile`
  (B22), un poco más estricto que `validar_formato`.
- **Arreglo propuesto:** que las pruebas de `lexer` y `semantic` lean con
  `baseline::corpus` como dev-dependencia, dejando `validar_formato` donde está (y
  añadiéndole esa regla). Opcional: no hay defecto, solo duplicación.
- **Dónde:** `crates/lexer/tests/common/mod.rs:151` (`validar_formato`),
  `crates/semantic/tests/common/mod.rs:53` (`casos_json`),
  `crates/baseline/src/corpus.rs:138` (`casos_en`).
- **Tarea:** cualquiera que toque esos lectores (pista A o B).

### 5. Recuento de violaciones frente al peso del DPDA (F6.7a-P5)

- **Estado:** `violations` de la línea base es el número de errores de
  `iter_errors` de `jsonschema` (U7), hasta `Limits::max_violations` (100; con más,
  `Outcome::truncated` y la prueba del corpus falla). Coincide con `expected` en todo
  el corpus de hoy, y la tabla `DIVERGENCIAS` está vacía, pero el recuento de
  `jsonschema` no es el del semianillo tropical en `required` (uno por clave),
  `additionalProperties` (uno por palabra clave), `type` + `enum` (dos) y los
  combinadores ([linea-base.md](linea-base.md) §4, fila 8; los fija
  `recuento_de_violaciones_de_jsonschema`).
- **Arreglo propuesto:** cuando F4.2 fije el peso de esas palabras clave, revisar si
  algún caso nuevo del corpus diverge y añadirlo a `DIVERGENCIAS` con el motivo (la
  prueba falla si una entrada deja de divergir).
- **El tope también se aplica al oráculo (CR1-S1):** `run_case` compila con
  `Limits::default()`, así que guarda como mucho 100 violaciones. Un caso futuro con
  `expected.violations` > 100 haría fallar `la_linea_base_recorre_el_corpus_json` por
  `truncated` (B26): un fallo ruidoso, no silencioso. Además, F4.6 no podría subir el
  tope sin reescribir `run_case`. Hoy ningún caso pasa de 100. Hay dos opciones:
  1. que `run_case` use `Limits { max_violations: u32::MAX, ..Limits::default() }`,
     porque el corpus es de confianza (el tope existe para entradas hostiles, §14);
  2. que el tope entre en el formato de `limits` de los casos (§7).
- **Dónde:** `crates/baseline/tests/corpus_json.rs:19` (`DIVERGENCIAS`),
  `crates/baseline/src/corpus.rs:329` (`run_case`, `compile_text` con los límites por
  defecto).
- **Tarea:** F4.2, F4.6 (y E1 si se elige la opción 2).

### 6. XML en Windows solo a través de Docker (F6.7a-P6) — resuelto en F6.7b

- **Estado:** el crate `libxml` necesita libxml2, `pkg-config` y libclang, que no hay
  en la máquina Windows. Sin la feature `libxml2`, `run_case` devuelve
  `Err(Unavailable::XmlDisabled)` para los casos XML (lo prueba
  `sin_libxml2_un_caso_xml_no_esta_disponible`).
- **Arreglo propuesto:** la feature `libxml2` (apagada por defecto) y el objetivo
  `make test-baseline-xml` en Docker (U4).
- **Resuelto en F6.7b (15 nov).** La feature existe (`crates/baseline/Cargo.toml`) y
  sin ella el workspace compila y pasa sus pruebas en Windows sin libxml2. Con ella,
  `make test-baseline-xml` (Linux) o `scripts/test-baseline-xml.ps1` (Windows sin
  `make`) construyen una imagen fijada y corren pruebas y clippy sin red
  ([linea-base.md](linea-base.md) §7.3, con la salida en verde). Sigue siendo cierto
  que en Windows la línea base XML solo corre en Docker: F4.6 en la máquina de Bayron
  compara los casos XML con `expected` (U8), no con libxml2.
- **Dónde:** `crates/baseline/src/corpus.rs:347` (`validar_xml`, rama XML sin la
  feature); `crates/baseline/Cargo.toml` (la feature); `Makefile`
  (`test-baseline-xml`); `crates/baseline/docker/Dockerfile`;
  `scripts/test-baseline-xml.{sh,ps1}`.
- **Tarea:** F6.7b (resuelta).

### 7. Formato del campo `limits` de los casos (F6.7a-P7)

- **Estado:** `tests/README.md` define `limits` («límites a usar en lugar de los por
  defecto») pero no su forma, y ningún caso lo usa. `run_case` devuelve
  `Err(Unavailable::CaseLimits)` ante un caso con `limits` en lugar de ignorarlo
  (ignorarlo daría por buena una comparación con otros límites; lo prueba
  `run_case_sin_resultado_por_generador_limites_o_esquema`).
- **Arreglo propuesto:** quien añada el primer caso con `limits` (previsiblemente
  `extreme`/`pumping`, E1) fija su forma en `tests/README.md` y en `validar_formato`
  (p. ej. `{"maxDepth": 16}`), y `run_case` la traduce a `baseline::Limits` con
  `JsonBaseline::with_limits`. `tests/README.md` es formato compartido: lo aprueban
  las dos personas.
- **Tope de violaciones (CR1-S1, ver §5):** hoy `run_case` aplica siempre
  `Limits::default()`, con 100 violaciones como máximo. Al fijar el formato hay que
  decidir entre dos opciones: que `run_case` use `max_violations: u32::MAX` para el
  corpus, que es de confianza, o que `limits` pueda declarar el tope (p. ej.
  `{"maxViolations": 500}`).
- **Dónde:** `crates/baseline/src/corpus.rs:323` (`run_case`, rama de `limits`) y
  `crates/baseline/src/corpus.rs:329` (compilación con `Limits::default()`).
- **Tarea:** E1 o F6.5 (el primero que lo necesite); F4.6 para el tope del oráculo.

### 8. Offset de las claves duplicadas y de la profundidad frente al DPDA (F6.7a-P8)

- **Estado:** la línea base sitúa una clave duplicada en la comilla que cierra la
  clave repetida, y el exceso de profundidad en el `[`/`{` que lo provoca
  ([linea-base.md](linea-base.md) §3.4). La ronda 1 generalizó la corrección
  (CR-I1: `{"a":1,"a"}`, `,` tras el `[` 257). El corpus no tiene todavía casos de esas
  clases, así que la prueba del corpus no los compara; el DPDA podría detectarlos en
  otro byte (la duplicada, por ejemplo, al terminar de leer la clave). Tampoco se
  comparó con el lexer cómo cuentan las líneas un CR+LF y un CR suelto (la línea base:
  solo LF cambia de línea; `crlf_y_cr_suelto_en_linea_y_columna`).
- **Arreglo propuesto:** al añadir casos `malformed` de clave duplicada o `limit` de
  profundidad, F4.6 compara el offset del intérprete con el de la línea base; si
  difieren a propósito, `expected.offset` sigue al DPDA y la comparación de offsets de
  `corpus_json.rs` (hoy solo en `lexical`/`syntax`) se mantiene así.
- **Dónde:** `crates/baseline/src/json.rs:493` (`corregir`),
  `crates/baseline/src/json.rs:516` (`linea_y_columna`).
- **Tarea:** F4.6.

### 9. Números fuera de la precisión de `f64` (F6.7a-P9)

- **Estado:** sin `arbitrary_precision` (U3), la línea base pasa los números por
  `f64` cuando no caben en `u64`/`i64` o tienen parte fraccionaria:
  - `1e400` en un payload es `Kind::Unsupported` (U9);
  - un entero mayor que 2⁶⁴ o un decimal de más de 17 cifras se comparan mal con
    `minimum`, `maximum` y `multipleOf` ([linea-base.md](linea-base.md) §4, filas
    2-4; `numeros_mas_alla_de_f64`, `decimal_largo_se_redondea_a_f64`);
  - **SEC-L4:** lo mismo vale para `enum`, `const` y `uniqueItems`: dos enteros
    fuera de `i64`/`u64` que redondean al mismo `f64` (p. ej.
    `12345678901234567890123` y `12345678901234567890124`) son iguales para la línea
    base y distintos para el proyecto;
  - **CR-S6:** un número fuera de `f64` en el **esquema** (`{"maximum": 1e400}`) hace
    que `compile_text` falle y `run_case` dé `Unavailable::Compile`, aunque el
    proyecto acepte el esquema (`esquema_con_numero_fuera_de_f64_no_compila`). Un caso
    así en el corpus haría fallar `la_linea_base_recorre_el_corpus_json`.

  El proyecto compara el lexema decimal exacto (`00-alcance.md` §1.1).
- **B19 es frágil (CR-S1):** detectar `1e400` depende del texto `number out of range`
  de `serde_json` 1.0.154 (fijado por `Cargo.lock`). Si una versión nueva lo cambia,
  `numero_que_no_cabe_en_f64_no_esta_soportado` falla, así que no se pierde en
  silencio. Alternativa sin texto: ante un error `Syntax`, releer el lexema numérico
  que termina en el offset y ver si `parse::<f64>()` da infinito.
- **Arreglo propuesto:** F4.6 excluye los casos `Unsupported` y trata como
  divergencia declarada (con motivo) los casos con números más allá de la precisión de
  `f64`, en payloads y en esquemas.
- **Dónde:** `crates/baseline/src/json.rs:451` (clasificación de `1e400`),
  `crates/baseline/src/json.rs:101` (`compile_text`).
- **Tarea:** F4.6.

### 10. Esquemas que el proyecto rechaza y la línea base acepta (F6.7a-P10)

- **Estado:** `jsonschema` compila esquemas que `schemac` rechaza
  ([linea-base.md](linea-base.md) §4, filas 14-18):
  - palabras clave fuera del subconjunto (`if`/`then`, `unevaluatedProperties`…);
  - palabras clave desconocidas, que ignora;
  - ciclos de `$ref` no productivos (D12), que trata como `true`;
  - un `$schema` de otro draft (se fuerza 2020-12);
  - un `$ref` a un meta-esquema oficial (`https://json-schema.org/draft/2020-12/schema`),
    que resuelve sin red porque los trae incluidos (CR-S5;
    `ref_al_meta_esquema_compila_sin_red`).

  Al revés, rechaza un esquema con un número fuera de `f64` que el proyecto acepta
  (§9, CR-S6).
- **Arreglo propuesto:** F4.6 y F6.5 solo comparan esquemas que pasan `schemac check`;
  la línea base no sustituye al análisis de la Fase 3.
- **Dónde:** `crates/baseline/src/json.rs:193` (`construir`).
- **Tarea:** F4.6, F6.5.

### 11. Costo del hilo de compilación en las mediciones (F6.7a-P11)

- **Estado:** `JsonBaseline::compile` y `compile_text` crean un hilo con
  `STACK_SIZE` por esquema (B12, D7, [linea-base.md](linea-base.md) §2), así que el
  tiempo de compilación medido incluye crear y unir un hilo (decenas de µs).
- **Arreglo propuesto:** F6.5 mide la compilación sobre esquemas reales (donde domina
  la construcción del validador) o resta el costo de un hilo vacío, y lo anota en la
  tabla comparativa. Si hace falta medirla sin hilo, añadir de forma aditiva
  `compile_in(&Stack, &Value)` y `compile_text_in(&Stack, &[u8])`, que compilan en el
  hilo de `with_stack` del llamador (CR-S3).
- **Dónde:** `crates/baseline/src/json.rs:90` (`compile`),
  `crates/baseline/src/json.rs:98` (`compile_text`).
- **Tarea:** F6.5.

### 12. Dependencia `baseline` del workspace aún sin usar (F6.7a-P12)

- **Estado:** `Cargo.toml` del workspace declara `baseline` en
  `workspace.dependencies` para F4.6 (`runtime-rs`, dev-dependencia) y F6.5 (`bench`);
  hasta entonces cargo avisa «unused workspace dependency», como ya hace con
  `runtime-rs`. No es un error de `make lint`.
- **Arreglo propuesto:** desaparece cuando F4.6 o F6.5 lo usen con
  `baseline.workspace = true`.
- **Dónde:** `Cargo.toml:34`.
- **Tarea:** F4.6, F6.5.

### 13. Límites de `00-alcance.md` §4 que la línea base no aplica (F6.7a-P13, CR-I3)

- **Estado:** el proyecto da `limit` con una cadena de más de 1 MiB, un lexema
  numérico de más de 1 024 bytes o más de 1 024 claves no declaradas en un objeto. La
  línea base no aplica ninguno: los tres payloads son válidos, salvo el número que no
  cabe en `f64`, que es `Unsupported` ([linea-base.md](linea-base.md) §4, filas 20-23;
  `limites_del_alcance_que_la_linea_base_no_aplica`). Cuando entren casos «Extremos»
  de esas clases al corpus, `la_linea_base_recorre_el_corpus_json` fallará: U7 compara
  siempre `valid` y `kind`, y `DIVERGENCIAS` solo admite el campo `violations`.
- **Arreglo propuesto:** una de dos, antes de añadir esos casos:
  1. ampliar `Limits` en el visitante. `visit_str`/`visit_string` ven la longitud de
     la cadena ya decodificada, y el número de claves de cada objeto se puede contar en
     `visit_map`, pero el lexema numérico no es visible sin `arbitrary_precision`;
  2. acordar cómo trata `corpus_json.rs` los casos `limit` que la línea base no puede
     dar (p. ej. aceptar `kind` en `DIVERGENCIAS`).

  Cambiar U7 requiere la aprobación de las dos personas (D42).
- **Dónde:** `crates/baseline/src/json.rs:322` y `:326` (`visit_str`,
  `visit_string`), `crates/baseline/src/json.rs:343` (`visit_map`),
  `crates/baseline/tests/corpus_json.rs:19` (`DIVERGENCIAS`).
- **En XML (F6.7b):** tampoco se aplican el tope de 100 000 referencias de entidad y
  de carácter por documento ni el de 1 MiB por cadena. Sí se aplican los de libxml2 sin
  `huge` (texto de 10 MB, nombres de 50 000 bytes), como `Kind::Limit`
  ([linea-base.md](linea-base.md) §7.5, fila 11; §24). `corpus_xml.rs` compara también
  la clase siempre, así que vale el mismo arreglo.
- **Tarea:** E1, F4.6.

### 14. Esquemas hostiles: costo de CPU sin acotar en `jsonschema` (F6.7a-P14, SEC-M1) — bloqueante para F7

- **Estado:** desde la ronda 1, la lista de violaciones (`Limits::max_violations`,
  100) y cada mensaje (`MAX_MESSAGE_BYTES`, 1 KiB) tienen tope (SEC-M1 escenario 2,
  SEC-M2, SEC-L1). Lo que no se puede acotar desde la línea base es el trabajo de
  `jsonschema`:
  - `iter_errors` reúne **todos** los errores en un vector interno antes de entregar
    el primero;
  - un esquema de ~2 KB con `$ref` en abanico (`dI` = `allOf` de dos `$ref` a `dI+1`,
    k = 24) cuesta 2^k evaluaciones: según el auditor, 42 s y 2²⁴ errores internos;
    con k = 30, minutos y decenas de GB ([linea-base.md](linea-base.md) §3.6).

  Con esquemas del corpus (de confianza) no pasa; con esquemas de usuario, sí.
- **Arreglo propuesto:** el servidor (F7) **no** usa `JsonBaseline` con esquemas o
  payloads de usuario sin un límite externo de CPU y memoria: proceso aparte o el
  sandbox con tiempo máximo y `RLIMIT_AS`. Si F7 expone la línea base, que use
  `Validator::is_valid` (no reúne errores y corta cada `allOf` en la primera rama
  falsa; un `anyOf` en abanico sigue siendo exponencial) y el mismo análisis de la
  Fase 3 que el DPDA antes de compilar. F6.5 lo anota en la tabla comparativa.
- **`max_violations` sin tope superior (SEC1-L-B):** `Limits::effective_max_violations`
  solo sube el 0 a 1. Con `u32::MAX`, o con un valor que llegue de una configuración
  externa, se anulan los topes de SEC-M1/M2. Arreglo: recortarlo, igual que la
  profundidad (p. ej. a 10 000, solo configurable a la baja), o que F7 no lo exponga
  nunca a la entrada del usuario.
- **Dónde:** `crates/baseline/src/json.rs:141` (`validate_dom`, `iter_errors`),
  `crates/baseline/src/lib.rs:190` (`effective_max_violations`);
  `crates/baseline/src/xml.rs:199` (`compile`, sin tope de tamaño ni de partículas),
  `:86` (cerrojo `LIBXML2`), `:576` (`carga_otro_documento`, donde se contarían).
- **En XML (F6.7b):** libxml2 tampoco acota el costo de compilar un XSD hostil.
  Medido por los auditores de la ronda 0 en `--release`:
  - **SEC-M2:** un `xs:sequence` de N `xs:element minOccurs="0" maxOccurs="10000"`:
    N = 200 → 0,34 s; 400 (22 KB) → 0,62 s; 800 (46 KB) → 4,5 s; 1 600 (92 KB) →
    26,9 s; 2 000 (107 KB) → más de 40 s. Unas 6 veces más por cada duplicación.
  - **CR-S5:** una cadena de 100 000 `xs:simpleType` (cada `restriction base` es el
    anterior, ~4 MB) → 288 s, frente a 0,59 s con 10 000: cuadrático, sin caída de pila.
  - No son problema: `xs:pattern` con repeticiones anidadas, `maxOccurs` grandes
    anidados, `minOccurs="1000"`, `(a|aa)*c` (milisegundos).

  Además, la validación de restricciones de identidad (`xs:unique`, `xs:key`), que el
  proyecto no admite, puede ser cuadrática (sin medir), y el crate `libxml` reúne todos
  los errores de validación sin tope (§21). Mientras compila, `compile` tiene tomado
  el cerrojo global `LIBXML2`, así que nada más valida en el proceso: en un servidor
  serializaría todas las peticiones (CR-S4).
- **Criterio para F7 (XML):** F7 no expone `XmlBaseline` con esquemas o payloads de
  usuario sin un límite externo de CPU y memoria por proceso (el sandbox). Si alguna
  vez lo hiciera, además: un **tope de bytes y de partículas del XSD antes de
  `compile`** (por ejemplo, contar los `xs:element` y grupos en el recorrido de
  `carga_otro_documento`, con el mismo límite de 4 096 que D26; con N = 800 ya son
  4,5 s), un tope de bytes del payload antes de `parse` (§3, §21) y un proceso por
  petición en lugar del cerrojo global.
- **Tarea:** F7 (servidor y sandbox), F6.5.

### 15. Lectura de `schemaFile`/`payloadFile` sin tope ni control de enlaces (F6.7a-P15, SEC-L2)

- **Estado:** `leer_relativo` solo admite componentes `Normal`, y desde la ronda 1
  rechaza también los dos puntos y la barra invertida (B22): en Windows `a:b` abre un
  flujo NTFS, y en Linux `C:/x` o `..` seguido de barra invertida serían nombres
  normales. Pero `std::fs::read` sigue enlaces simbólicos y lee archivos de cualquier
  tamaño. Hoy no se puede explotar, porque el corpus es del repo y lo revisa
  `validar_formato`.
- **Nombres de dispositivo de Windows (SEC1-L-A):** `CON`, `NUL`, `AUX`, `PRN`,
  `COM1`…`COM9`, `LPT1`…`LPT9` (con o sin extensión: `NUL.txt`) son componentes
  `Normal`, así que pasan el filtro, y `std::fs::read` puede bloquear o devolver
  basura. Solo importa con un corpus ajeno.
- **Arreglo propuesto:** si F7 (o el script de carga a Atlas) reutiliza
  `baseline::corpus` con archivos ajenos, antes de leer hay que hacer cuatro cosas:
  - rechazar los nombres reservados de Windows en cualquier componente;
  - comprobar con `symlink_metadata` que no es un enlace;
  - exigir que `metadata().is_file()` sea cierto;
  - exigir un tope de `len()`.
- **Dónde:** `crates/baseline/src/corpus.rs:249` (`leer_relativo`) y
  `crates/baseline/src/corpus.rs:257` (`std::fs::read`).
- **Tarea:** F7 (si reutiliza `corpus.rs`).

### 16. `with_stack` hace pánico si no puede crear el hilo (F6.7a-P16, SEC-L3)

- **Estado:** `with_stack` llama a `expect` al crear el hilo de 8 MiB. Bajo presión de
  memoria, con muchas compilaciones concurrentes (cada `compile` crea uno, B12), eso
  es un pánico, no un error.
- **Arreglo propuesto:** si F6.5 o F7 corren muchas en paralelo, una variante
  `try_with_stack(...) -> std::io::Result<R>` aditiva, o un grupo fijo de hilos con
  `STACK_SIZE`. Hoy está documentado en `# Panics`.
- **Dónde:** `crates/baseline/src/lib.rs:226`.
- **En XML (F6.7b):** `XmlBaseline::compile` no crea hilo: compila en el de
  `with_stack` que recibe como testigo (no es `Send`). Solo `with_stack` puede hacer
  pánico, una vez por bloque de trabajo.
- **Tarea:** F6.5, F7.

### 17. `uniqueItems` con arreglos de objetos, posiblemente cuadrático (F6.7a-P17)

- **Estado:** el auditor midió `uniqueItems` con 10⁵ enteros en 5 ms, pero no con
  arreglos de objetos, donde `jsonschema` puede comparar por pares. El proyecto lo
  acota a 4 096 elementos con un hash (`00-alcance.md` §4); la línea base, no.
- **Arreglo propuesto:** F6.5 lo mide en el arnés (k objetos, k creciente) y, si es
  cuadrático, lo anota en la tabla comparativa como diferencia de costo, no de
  resultado.
- **Dónde:** — (`jsonschema` 0.61.2, `src/keywords/unique_items.rs`; dependencia, no
  código del repo).
- **Tarea:** F6.5.

### 18. `cargo audit` de las dependencias nuevas (F6.7a-P18)

- **Estado:** la F6.7a añadió 59 paquetes a `Cargo.lock` (`jsonschema` y sus
  dependencias: `regex`, `fancy-regex`, `referencing`, `num-*`…). No hay `cargo-audit`
  ni `cargo-deny` en la máquina, y no se consultó ninguna base de avisos.
- **Arreglo propuesto:** ejecutar `cargo audit` (o `cargo deny check advisories`) antes
  de cerrar la Fase 6, en la revisión consolidada de seguridad (D35), y anotar el
  resultado aquí.
- **Dónde:** `Cargo.lock`.
- **F6.7b** añade al `Cargo.lock` `libxml` 0.3.23 y las dependencias de su
  `build.rs` (`bindgen` 0.73, `clang-sys`, `pkg-config`, `vcpkg`…), que solo se
  compilan con la feature, y usa la libxml2 2.9.14 de Debian 13 (con sus parches de
  seguridad, §25). `cargo audit` no cubre la biblioteca de C: hay que mirar también los
  avisos de Debian para `libxml2`.
- **Tarea:** cierre de la Fase 6 (revisión consolidada).

### 19. El `Drop` de `JsonBaseline` es recursivo y corre fuera de `with_stack` (F6.7a-P19, CR1-I1)

- **Estado:** `JsonBaseline` es `Send + Sync` (U2), así que se libera en el hilo de
  quien lo tenga, a menudo el principal. El validador de `jsonschema` es un árbol con
  la forma del esquema, y su liberación es recursiva. Está acotado por `MAX_DEPTH_LIMIT`
  (256). Medido por el code-reviewer en `dev` con 256 contenedores anidados:

  | Esquema | Desborda con | Pasa con |
  |---|---|---|
  | `{"not": …}`, `unevaluatedItems` | 256 KiB | 384 KiB |
  | `items`, `additionalProperties`, `if`, `else`, `contains`, `propertyNames` | — | 256 KiB |
  | `allOf`, `properties` (128 niveles) | — | 256 KiB |

  Cabe en el hilo principal de Windows (1 MiB) con un margen de unas 3 veces
  ([linea-base.md](linea-base.md) §3.5). No es un defecto hoy, pero un hilo de pila
  pequeña que suelte un `JsonBaseline` profundo (un *worker* de tokio con poca pila, por
  ejemplo) podría desbordar y abortar el proceso.
- **Arreglo propuesto:** si F6.5 o F7 sueltan validadores en hilos de pila
  desconocida, liberar el validador en un hilo con `STACK_SIZE`. Por ejemplo, guardarlo
  en un `ManuallyDrop<Validator>` e implementar `Drop for JsonBaseline` de modo que lo
  mueva a `with_stack(|_| drop(…))`. Así se paga un hilo por cada liberación: solo
  merece la pena si se libera fuera de un hilo conocido. Otra opción es documentar que
  se suelte dentro de `with_stack`.
- **Dónde:** `crates/baseline/src/json.rs:41` (`JsonBaseline`, su documentación lo
  dice desde el cierre de la F6.7a).
- **En XML (F6.7b) no aplica:** `XmlBaseline` y `XmlDom` no son `Send`, así que se
  liberan siempre en el hilo de `with_stack` donde se crearon.
- **Tarea:** F6.5, F7.

### 20. Prueba permanente del recorte de mensajes con multibyte en el límite (F6.7a-P20, CR1-S2)

- **Estado:** `acotado` corta en un límite de carácter y nunca pasa de
  `MAX_MESSAGE_BYTES`. Lo verificaron con sondas el Tester de la ronda 1 (con `x`, `é`,
  `€` y `😀`: nunca más de 1 024 bytes, nunca un carácter cortado, siempre `…` al final;
  1 024 bytes exactos no se recortan y 1 025 sí) y el code-reviewer (con `é` en el
  límite: 1 023 bytes + `…`). Pero las pruebas del repo solo recortan texto ASCII
  (`el_mensaje_de_una_violacion_tiene_tope`, `el_mensaje_de_compile_error_tiene_tope`).
  Es cobertura de un comportamiento ya correcto: no abrió ronda (D38).
- **Arreglo propuesto:** añadir una prueba con `pattern` (o instancia) de `é`, `€` y
  `😀` colocados de modo que caigan en el límite. Debe exigir que el mensaje tenga
  ≤ 1 024 bytes, termine en `…` y sea UTF-8 válido (que lo es por ser `String`), y que
  un texto de exactamente 1 024 bytes no se recorte.
- **Dónde:** `crates/baseline/src/json.rs:381` (`acotado`),
  `crates/baseline/tests/json.rs` (sección «Topes de la lista de violaciones y de los
  mensajes»).
- **Tarea:** F6.5.

### 21. El crate `libxml` reúne todos los errores de validación sin tope (F6.7b-P1)

- **Estado:** `XmlBaseline::validate_dom` corta la lista en `Limits::max_violations`
  (100) con `truncated`, y cada mensaje en `MAX_MESSAGE_BYTES`
  ([linea-base.md](linea-base.md) §7.2). Pero el manejador de errores de
  `SchemaValidationContext` (crate `libxml` 0.3.23, `src/schemas/common.rs`) guarda en
  un `Vec` **todos** los `StructuredError` de libxml2 2.9.14, cada uno con su mensaje
  completo, antes de que la línea base los vea: un payload de n elementos inválidos da
  n errores en memoria (medido: 1 000 `<n>x</n>` dan 1 000). Es O(n), como el árbol,
  pero con una constante mayor. Los diagnósticos de la lectura sí tienen tope en el
  crate (100, `MAX_DIAGNOSTICS`).
- **Medición (SEC-M3, ronda 0):** `<r>` con N `<i/>` y un atributo requerido ausente
  en cada uno, pico de memoria del proceso (`VmHWM`, DOM incluido): N = 100 000
  (400 KB) → 36 MB; 1 000 000 (4 MB) → 322 MB y 0,95 s; 3 000 000 (12 MB) → 962 MB y
  3,2 s, siempre con 100 violaciones en el resultado (`truncated`). Unas **80 veces** el
  tamaño del payload (unos 320 bytes por error): 100 MB de payload pedirían unos 8 GB.
  Solo el 3-4 % llega al resultado. `max_violations` acota lo que se devuelve, no el
  trabajo ni la memoria.
- **Arreglo propuesto:** con el corpus no importa. El único remedio sin `unsafe` es
  el **tope de tamaño del payload antes de `parse`** de §3 (o un límite de memoria del
  proceso); es criterio de F7 si alguna vez expusiera la línea base (no debe, §14) y
  de F6.5 al medir payloads grandes con muchos errores. La alternativa, validar sin
  recoger errores (`xmlSchemaValidateStream` o un `xmlSchemaSetValidStructuredErrors`
  propio que solo cuente y guarde los primeros), exige `unsafe` o un cambio en el crate
  `libxml` (fuera del repo).
- **Dónde:** `crates/baseline/src/xml.rs:274` (`validate_document`) y `:292`
  (`truncated`).
- **Tarea:** F6.5, F7.

### 22. `pointer` vacío en la línea base XML (F6.7b-P2)

- **Estado:** libxml2 informa la línea del error de validación, pero no el nodo en una
  forma que el crate `libxml` exponga (`StructuredError` omite `xmlError.node`). Por
  eso `Violation::pointer` es `""` en XML, la línea va al principio de `message`
  (`línea 1: …`) y `corpus_xml.rs` no compara `expected.pointer`
  ([linea-base.md](linea-base.md) §7.4). Los casos XML del corpus de hoy no declaran
  `pointer`.
- **Arreglo propuesto:** cuando entre al corpus un caso XML `invalid` con `pointer`,
  F4.6 lo compara con el intérprete y no con la línea base. Si hiciera falta la ruta,
  se podría reconstruir un XPath simple buscando en el árbol el elemento de esa línea
  (ambiguo si hay varios en la misma línea) o pedir al crate `libxml` que exponga el
  nodo.
- **Dónde:** `crates/baseline/src/xml.rs:268` (`validate_dom`),
  `crates/baseline/tests/corpus_xml.rs:16` (`comparar`).
- **Tarea:** F4.6.

### 23. Offsets de los errores de libxml2 frente al DPDA (F6.7b-P3)

- **Estado:** salvo la codificación (offset 0 o el valor de `encoding`), el
  `<!DOCTYPE` (en la `D`) y el exceso de profundidad (en el `<`), que los sitúa la
  línea base, el offset de un `Malformed` sale de la línea y la
  columna de libxml2, que informa donde detecta el error: una etiqueta de cierre que
  no casa, tras leer su nombre (`etiqueta_sin_cerrar_es_malformado_con_linea_columna_y_offset`
  solo exige la línea 2 y una columna coherente con el offset). El DPDA puede
  detectarlo en otro byte. Un `\r` suelto no cambia de línea al calcular el offset.
  (La nota de la primera versión sobre otras codificaciones ya no aplica: desde la
  ronda 1 libxml2 solo lee UTF-8, §1.)
- **Arreglo propuesto:** como en §8: al añadir casos XML `malformed` al corpus, F4.6
  compara el offset del intérprete con el de la línea base; si difieren a propósito,
  `expected.offset` sigue al DPDA y `corpus_xml.rs` pasa a comparar el offset solo en
  los casos que sitúa la propia línea base (o con una tabla de divergencias).
- **Dónde:** `crates/baseline/src/xml.rs:698` (`offset_de`),
  `crates/baseline/tests/corpus_xml.rs:16` (`comparar`, offset de `lexical`/`syntax`).
- **Tarea:** F4.6.

### 24. Límites de libxml2 reconocidos por el texto del mensaje (F6.7b-P4)

- **Estado:** libxml2 2.9.14 no da un código propio a sus límites sin `huge`
  (profundidad: `XML_ERR_INTERNAL_ERROR`). `es_limite` los reconoce por el mensaje y los
  da como `Kind::Limit`; el resto, como `Malformed`:
  - «Excessive depth» (profundidad);
  - «too big» (desde la ronda 1, CR-I2: «Comment too big found», «CData section too
    big found», «PI … too big found», de más de 10 MB; antes salían `Malformed`);
  - «too long» («AttValue length too long», de más de 10 MB; «Name too long», de más
    de 50 000 bytes);
  - «Huge input lookup» y «xmlSAX2Characters: huge text node»: un **nodo de texto** de
    10 MB o más es `Limit` con uno de los dos (TR1-O1, CR1; medido por el Tester de la
    ronda 1 en Docker, debug, 2.9.14):

    | Texto | Resultado |
    |---|---|
    | `x` × 1 000 000 … 9 999 900 | válido |
    | `x` × 9 999 999, 10 000 000, 10 000 001 o 20 MB, de una tirada | `Limit`, «Huge input lookup» |
    | 125 001 líneas de 80 bytes (`\n` o `\r\n`); 10 000 001 espacios; palabras separadas por espacios (12 MB) | `Limit`, «Huge input lookup» |
    | 6 MB de texto + comentario + 6 MB de texto | `Limit`, «Huge input lookup» |
    | `ñ` × 5 000 001 (10 MB) o × 5,1 M (`ñ` × 4,9 M, 9,8 MB: válido) | `Limit`, «huge text node» |
    | 11 MB con `&lt;`, `&#65;` o `&amp;` intercalados cada 100 … 100 000 bytes | `Limit`, «huge text node» |
    | dos CDATA de 6 MB seguidos | `Limit`, «huge text node» |
    | 6 MB de texto + CDATA de 6 MB; 10 MB solo de `&amp;` o de `&#65;` | válido |
    | 3 000 000 de `<b>zz</b>` (≈ 27 MB, sin un nodo grande) | válido |

    El umbral de «Huge input lookup» es `XML_MAX_LOOKUP_LIMIT` = 10 000 000 bytes por
    búsqueda (9 999 999 bytes de texto ya lo alcanzan y 9 999 900 no; presumiblemente
    porque la búsqueda cuenta también los bytes de alrededor), y el de «huge text node» es `XML_MAX_TEXT_LENGTH` = 10 000 000 bytes de
    nodo acumulado. Un texto hecho solo de referencias o texto + CDATA no llega a
    ninguno de los dos. La primera versión de esta documentación decía que «huge text
    node» no salía en esta versión: era falso. La revisión de la ronda 0 vio un texto
    de 10 MB válido; con `x` × 9 999 9xx lo es.

  Lo prueban `comentario_cdata_e_instruccion_de_mas_de_10_mb_son_limite`,
  `atributo_de_mas_de_10_mb_y_nombre_de_mas_de_50_000_bytes_son_limite`,
  `texto_de_mas_de_10_mb_es_limite` («Huge input lookup», de una tirada y en líneas)
  y, con los textos literales, la prueba unitaria `es_limite_con_los_mensajes_de_libxml2`.
  Ninguna prueba contra libxml2 real ejercita «huge text node». Es frágil, como §9: otra versión de libxml2
  puede cambiar los textos. La profundidad no depende de esto, porque la línea base la
  comprueba antes (`profundidad_excedida`, B32).
- **Arreglo propuesto:** al cambiar la versión de libxml2 de la imagen (§25), repasar
  los mensajes de `parserInternals.c`/`parser.c` y volver a pasar las cuatro pruebas.
  **Criterio pendiente (TR1-O1):** una prueba contra libxml2 real que ejercite «huge
  text node», p. ej. una variante de `texto_de_mas_de_10_mb_es_limite` con `ñ` ×
  5 000 001 (o 11 MB con un `&amp;` cada 1 000 bytes) que exija `Limit` y ese mensaje.
  Si E1 o los casos «Extremos» necesitan el tope de 1 MiB por cadena de
  `00-alcance.md` §4, aplicarlo en la línea base con un recorrido propio, como la
  profundidad (§13).
- **Dónde:** `crates/baseline/src/xml.rs:518` (`es_limite`).
- **Tarea:** F6.5 (la prueba de «huge text node»); E1; al cambiar libxml2, quien
  actualice la imagen.

### 25. Mantenimiento de la imagen Docker y de libxml2 (F6.7b-P5)

- **Estado:** `crates/baseline/docker/Dockerfile` fija `rust:1.100.0-slim-trixie` por
  digest, pero `libxml2-dev`, `libclang-dev` y `pkg-config` vienen sin versión del
  espejo de Debian 13 (el 15 nov: libxml2 2.9.14, paquete
  `2.12.7+dfsg+really2.9.14-2.1+deb13u3`; libclang 19). Una reconstrucción puede traer
  un parche de seguridad de libxml2 con otro comportamiento. Además
  `rust-toolchain.toml` dice `stable`: cuando las máquinas pasen a la 1.101, la imagen
  seguirá en la 1.100.0 (por `RUSTUP_TOOLCHAIN`), y clippy puede dar avisos distintos
  en Docker y en `make lint`.
- **Arreglo propuesto:** al cambiar de versión de Rust en el equipo, actualizar a la vez
  la etiqueta, el digest y `RUSTUP_TOOLCHAIN` del `Dockerfile` y la etiqueta de la imagen
  (`validador-baseline-xml:N`) en el `Makefile` y en `scripts/test-baseline-xml.ps1`.
  El script imprime la versión de libxml2 en cada ejecución: si cambia, repasar §24 y
  volver a pasar `make test-baseline-xml`. Si hiciera falta reproducibilidad total,
  `snapshot.debian.org` con una fecha fija.
- **SEC-L4 (ronda 0):** la rama 2.9.x de libxml2 ya no tiene mantenimiento upstream;
  Debian la parchea. Hay avisos de 2025 sobre esquemas y Schematron en 2.9.x sin
  cuantificar (no hay `cargo audit` ni base de avisos de C en la máquina, §18). Por
  eso: reconstruir la imagen con regularidad (`docker build --pull --no-cache`) para
  recoger los parches de Debian, y no usar la línea base sobre XSD de usuarios sin el
  sandbox (§14).
- **Contenedor sin límites de recursos (SEC1-L2, ronda 1):** el paso de pruebas corre
  sin red, sin capacidades y sin escalada (B38), pero sin `--memory`, `--pids-limit` ni
  `--read-only`. Con el corpus es teórico; con payloads o XSD grandes u hostiles no
  (§14: un XSD de 92 KB cuesta 27 s; §21: 12 MB de payload, 962 MB de memoria).
  Arreglo: antes de que F6.5 o F4.6 pasen payloads mutados o grandes por el mismo
  script, añadir al `docker run` del paso de pruebas, a la par en el `Makefile` y en
  el `.ps1`, `--memory 2g --pids-limit 512` (o lo que mida F6.5) y, si cargo lo
  tolera con `CARGO_TARGET_DIR` y el registro en volúmenes, `--read-only` con
  `--tmpfs /tmp`.
- **Volúmenes escribibles compartidos (SEC1-L3, ronda 1):** el paso de pruebas compila
  y ejecuta `build.rs` de terceros (`libxml`, `bindgen`, `clang-sys`) como root, sin
  red ni capacidades, con `validador-cargo-registry` y `validador-baseline-xml-target`
  escribibles y persistentes entre ejecuciones. Un `build.rs` malicioso podría
  envenenar esa caché para la siguiente ejecución. Lo mitigan los checksums de
  `Cargo.lock` y `--locked` (cargo comprueba cada paquete descargado). Solo
  documentación: si se quiere más, borrar los volúmenes al cambiar `Cargo.lock`
  (`docker volume rm validador-cargo-registry validador-baseline-xml-target`) o montar
  el registro en solo lectura en el paso sin red.
- **Dónde:** `crates/baseline/docker/Dockerfile:15`, `Makefile:22`
  (`BASELINE_XML_IMAGE`) y `:29` (`docker run` del paso de pruebas),
  `scripts/test-baseline-xml.ps1:15` y `:29`.
- **Tarea:** cierre de la Fase 6 (revisión consolidada) y cada cambio de versión de
  Rust; SEC1-L2, F6.5 o F4.6 (la primera que pase payloads grandes por el script).

### 26. Confirmar con Leonardo la forma de `XmlBaseline` y `XmlDom` (F6.7b-P6, S1)

- **Estado:** la API de `baseline` (U2) está pendiente de confirmar con Leonardo
  (D48). F6.7b le añade dos cosas que debe ver:
  - **B27:** `XmlBaseline::compile(stack: &Stack, xsd: &[u8])` pide el testigo, a
    diferencia de `JsonBaseline::compile`, porque `XmlBaseline` no es `Send`;
  - **B34:** `XmlDom` no da acceso al documento de libxml2. La primera versión tenía
    `XmlDom::document()`, que se quitó en la ronda 1 (CR-S3).

  También le afecta **CR-S2**: si F4.6 activa la feature `libxml2` desde un crate del
  workspace, debe hacerlo con una feature de paso apagada por defecto
  ([linea-base.md](linea-base.md) §7.1).
- **`make test-baseline-xml` en Linux (TR1-O3):** el objetivo del `Makefile` solo se
  ha comprobado con `make -n` (en un contenedor); las ejecuciones reales fueron con
  `scripts/test-baseline-xml.ps1`, que hace lo mismo flag por flag. Leonardo debe
  correr `make test-baseline-xml` en su máquina Linux y confirmarlo en el punto de
  control tras F6.7a y F6.7b (con SELinux, montaje `:ro,z`, §7.3).
- **Arreglo propuesto:** incluirlo en la confirmación de U2, U8 y U10 con Leonardo
  (punto de control tras F6.7a y F6.7b de `tasks/todo.md`) y registrar B27-B38 en el
  plan general al cerrar, como D50.
- **Dónde:** `crates/baseline/src/xml.rs:199` (`compile`), `:168` (`XmlDom`).
- **Tarea:** punto de control de F6.7 (con Leonardo: U2 con B27 y B34, y
  `make test-baseline-xml` en Linux); F4.6.

### 27. `NO_UTF8` no reconoce dos órdenes de UCS-4 (F6.7b-P7, SEC1-L1)

- **Estado:** `politica_de_entrada` rechaza en el offset 0 las entradas que empiezan
  como UTF-16, UTF-32 o EBCDIC según la tabla `NO_UTF8`, pero le faltan dos comienzos
  del apéndice F de XML 1.0: `00 00 00 3C` (UCS-4 big-endian sin BOM) y `00 00 3C 00`
  (UCS-4 en orden 2143). Esas entradas pasan la política y las corta libxml2, que lee
  siempre UTF-8, con «Document is empty» (`Malformed`), sin fuga ni iconv: el mensaje
  es menos útil y, en teoría, libxml2 intenta detectar UCS-4. (Por lo mismo, UTF-16LE
  sin BOM con un espacio delante también lo corta libxml2, en el offset 1: CR1-S3,
  [linea-base.md](linea-base.md) §7.2.)
- **Arreglo propuesto:** añadir a `NO_UTF8` `(b"\x00\x00\x00\x3C", "UTF-32")` y
  `(b"\x00\x00\x3C\x00", "UTF-32")` (antes de `(b"\x00\x3C", …)`) y los dos casos a
  `entradas_que_no_son_utf8_son_malformado`, con offset 0.
- **Dónde:** `crates/baseline/src/xml.rs:70` (`NO_UTF8`),
  `crates/baseline/tests/xml.rs` (`entradas_no_utf8`).
- **Tarea:** la próxima que toque `xml.rs` (F6.5 o F4.6).
