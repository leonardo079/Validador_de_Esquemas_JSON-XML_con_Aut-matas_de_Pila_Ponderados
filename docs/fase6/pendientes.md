# Pendientes de la Fase 6 (líneas base y métricas)

Observaciones que dejó abiertas la tarea F6.7a (línea base JSON, `crates/baseline`;
teoría en [linea-base.md](linea-base.md)) y que se resuelven en otra tarea. Las
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
    rechaza (`00-alcance.md` §1.2).
- **Arreglo propuesto:** F4.6 compara estos casos con el intérprete y, si divergen,
  los añade a `DIVERGENCIAS` (o a su lista de exclusiones) con el motivo; la parte
  XML la documenta F6.7b.
- **Dónde:** `crates/baseline/src/json.rs:193` (`construir`: motor de `pattern`),
  `crates/baseline/src/json.rs:233` (`leer`: escapes y UTF-8).
- **Tarea:** F4.6 (JSON), F6.7b (XML).

### 2. Memoria de libxml2 fuera del asignador de Rust (F6.7a-P2)

- **Estado:** libxml2 reserva con `malloc` de C, que un `GlobalAlloc` de Rust no ve.
- **Arreglo propuesto:** F6.5 mide el pico del proceso (`VmHWM` de
  `/proc/self/status` en Linux) para la línea base XML. Para contar asignaciones de
  Rust sin escribir `unsafe` (prohibido en el workspace), un crate como `stats_alloc`
  o `dhat` en lugar de un asignador propio.
- **Dónde:** — (el arnés de F6.5, `crates/bench`, aún no existe; la línea base XML es
  F6.7b).
- **Tarea:** F6.5 (con F6.7b).

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
  `crates/baseline/src/corpus.rs:327` (`run_case`, `compile_text` con los límites por
  defecto).
- **Tarea:** F4.2, F4.6 (y E1 si se elige la opción 2).

### 6. XML en Windows solo a través de Docker (F6.7a-P6)

- **Estado:** el crate `libxml` necesita libxml2, `pkg-config` y libclang, que no hay
  en la máquina Windows. Sin la feature `libxml2`, `run_case` devuelve
  `Err(Unavailable::XmlDisabled)` para los casos XML (lo prueba
  `sin_libxml2_un_caso_xml_no_esta_disponible`).
- **Arreglo propuesto:** la feature `libxml2` (apagada por defecto) y el objetivo
  `make test-baseline-xml` en Docker (U4).
- **Dónde:** `crates/baseline/src/corpus.rs:316` (`run_case`, rama XML);
  `crates/baseline/Cargo.toml` (la feature, por añadir).
- **Tarea:** F6.7b.

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
- **Dónde:** `crates/baseline/src/corpus.rs:324` (`run_case`, rama de `limits`) y
  `crates/baseline/src/corpus.rs:327` (compilación con `Limits::default()`).
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
- **Dónde:** `crates/baseline/src/json.rs:492` (`corregir`),
  `crates/baseline/src/json.rs:515` (`linea_y_columna`).
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
- **Dónde:** `crates/baseline/src/json.rs:450` (clasificación de `1e400`),
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
  `crates/baseline/src/lib.rs:181` (`effective_max_violations`).
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
- **Dónde:** `crates/baseline/src/lib.rs:217`.
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
- **Dónde:** `crates/baseline/src/json.rs:380` (`acotado`),
  `crates/baseline/tests/json.rs` (sección «Topes de la lista de violaciones y de los
  mensajes»).
- **Tarea:** F6.5.
