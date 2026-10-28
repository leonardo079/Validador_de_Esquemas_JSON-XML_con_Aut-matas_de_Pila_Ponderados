# Pendientes de la Fase 3 (análisis semántico del esquema)

Observaciones que dejaron abiertas las rondas de auditoría de `crates/semantic`
y que se resuelven en otra fase. Las decisiones de diseño viven en el
«Registro de decisiones» del plan; aquí solo se remite a ellas.

## Vigilar más adelante

### 1. Truncar la lista de errores mostrada

- **Estado:** la Fase 3 informa todos sus errores, sin tope, ordenados por posición
  ([ciclos.md](ciclos.md) §5, «Número de errores: sin tope»). Lo guardado es O(n), pero
  el texto que se muestra es varias veces la entrada. Esta sección acota ese texto para
  **todas** las clases de error de `schemac check`.
- **Etapas.** `schemac check` devuelve los errores de una sola etapa: la primera que
  falla.
  - El lexer y el parser (Fase 2) se detienen en el primer error: **un solo mensaje**,
    de tamaño acotado. Lleva un texto fijo de ≤ 93 bytes de pista, el `expected` y el
    `found` recortados a 32 caracteres escapados (`short`, ≤ 10 bytes por carácter) y el
    prefijo de posición: ≲ 750 bytes más la ruta, sea cual sea la entrada (el Tester de
    4.1 midió 725 bytes sin ruta con dos cadenas de 40 × U+10FFFF).
  - La Fase 3 (`semantic::analyze`) devuelve **todos** los errores de su primera etapa
    que falla: meta-esquema, luego tabla de símbolos, luego ciclos. Nunca se suman dos
    etapas.
  - Los errores de `Ir::verify` (la IR que sale de la bajada, [ir.md](ir.md) §4) **no se
    listan al usuario**: son fallos del frontal, no del esquema, y `schemac` y el servidor
    los colapsan en **un único error interno** de tamaño fijo. Por eso no tienen fila en
    la tabla de abajo; la lista completa solo va al registro y a las pruebas.

  Por tanto la salida es ≤ ρ(r)·n + K, con n los bytes de la entrada, K unos cientos de
  bytes (el error de la Fase 2, o los bordes de un arreglo) y ρ(r) el mayor cociente de
  la tabla de abajo.
- **Cociente por construcción.** Cada error sale de una construcción de la entrada, y
  las construcciones de errores distintos no comparten bytes. El cociente de una
  construcción es (M + e·(r + 1)) / b, donde:
  - b son los bytes que la construcción **añade** a la entrada (las llaves de un objeto
    que comparte con otras no se cuentan, lo que sobrestima el cociente);
  - e son los errores que provoca;
  - M son los bytes de esos mensajes sin la ruta, tomando el mayor mensaje de la clase y
    posiciones de 7 cifras (entradas de menos de 10 MB; cada cifra más suma 1 o 2 bytes
    por error);
  - r es la longitud de la ruta que `schemac` pone delante de cada línea (`t.json: ` son
    8 bytes); cada error añade además un salto de línea.

  Para la Fase 3 entera:

  **ρ(r) = (376 + 2·(r + 1)) / 3 ≈ 126 + 0,67·r**, unas 131 veces con una ruta de 8
  bytes. Lo fija el elemento `""` de `type`. Es una cota **holgada**: M = 376 toma dos
  veces el mayor de los dos mensajes; los dos reales suman ≤ 351 bytes, lo que daría
  ≈ 118 + 0,67·r.

  Los valores de b, e y M salen del formato real de los mensajes
  (`crates/semantic/src/error.rs`, `meta.rs`, `symbols.rs`, `cycles.rs`), medidos con
  `schemac check` en release sobre documentos de m = 2 000 repeticiones:

  | Clase de error | Construcción mínima | b | e | M | Cociente, r = 0 | Cociente, r = 8 |
  |---|---|---|---|---|---|---|
  | meta-esquema: nombre de `type` en un arreglo | elemento `""` (nombre inválido **y** repetido) | 3 | 2 | 376 | **126,0** | **131,3** |
  | meta-esquema: repetido en `dependentRequired` | elemento `""` | 3 | 1 | 169 | 56,7 | 59,3 |
  | meta-esquema: repetido en `required` | elemento `""` | 3 | 1 | 160 | 53,7 | 56,3 |
  | meta-esquema: palabra clave rechazada (D6) | `"if":0,` | 7 | 1 | 292 | 41,9 | 43,0 |
  | ídem, desconocida | `"a":0,` | 6 | 1 | 201 | 33,7 | 35,0 |
  | meta-esquema: `$id` fuera de la raíz y con fragmento | `"$id":"##",` | 11 | 2 | 364 | 33,3 | 34,7 |
  | meta-esquema: `$ref` no local | `"$ref":"x",` | 11 | 1 | 296 | 27,0 | 27,7 |
  | meta-esquema: nombre de `type` (cadena) | `"type":"",` | 10 | 1 | 188 | 18,9 | 19,7 |
  | meta-esquema: `$schema` fuera de la raíz | `"$schema":"",` | 13 | 1 | 213 | 16,5 | 17,1 |
  | meta-esquema: `$anchor` inválido | `"$anchor":"",` | 13 | 1 | 209 | 16,2 | 16,8 |
  | meta-esquema: arreglo vacío | `"type":[],` / `"allOf":[],` | 10 / 11 | 1 | 155 / 149 | 15,6 / 13,6 | 16,4 / 14,4 |
  | meta-esquema: número fuera de rango | `"minItems":-1,` / `"multipleOf":0,` | 14 / 15 | 1 | 141 / 143 | 10,1 / 9,6 | 10,7 / 10,1 |
  | tabla de símbolos: `$ref` sin resolver | `{"$ref":"#%"},` (el peor de los 7 motivos) | 14 | 1 | 154 | 11,1 | 11,6 |
  | tabla de símbolos: ancla repetida | `{"$anchor":"a"},` | 16 | 1 | 94 | 5,9 | 6,4 |
  | ciclos no productivos | 8 aristas como elemento de `allOf` (abajo) | ≥ 84 | 1 | ≤ 974 | ≲ 11,6 | ≲ 12 |
  | Fase 2 (lexer y parser) | cualquiera | — | 1 | ≲ 750 | constante | constante |

  Por qué no hay construcciones más baratas:
  - un elemento de arreglo de cadenas no baja de 3 bytes (`""` y la coma);
  - un elemento no provoca más de 2 errores (nombre inválido y repetido, solo en `type`);
  - una palabra clave no puede repetirse en un objeto (D2), así que cada aparición paga
    su nombre;
  - los textos del usuario van recortados a 32 caracteres, así que alargarlos sube b más
    que M;
  - los escapes de `short` dan como mucho **6** bytes de salida por byte de entrada (un
    DEL, 0x7F, válido sin escapar en una cadena JSON, sale como `\u{7f}`), muy por
    debajo del cociente base: con elementos `"\x7f"` en `type` el Tester de 4.1 midió
    91,6 veces, menos que con `""`.
- **Medido** con la ruta `t.json: ` (8 bytes), en una sola línea:
  - el peor caso, `{"type":["","",…]}` con 10⁵ elementos: 0,30 MB → 35,4 MB, **118,1
    veces**;
  - `required`: 53,9 veces;
  - `dependentRequired`: 57,2 veces (lo midió el Tester de 4.1);
  - palabras clave rechazadas: con 52 claves de una letra más `if`, `then` y `else` por
    objeto, 34,6 veces; con la ruta `kw.json: ` (9 bytes), un error de `if` ocupa
    292 bytes por 7 de entrada (41,7 veces).
- **Ciclos, en detalle.**
  - **Formato del mensaje.** Un error de ciclo ocupa a lo sumo:
    - 137 bytes de texto fijo;
    - el prefijo `línea L, columna C: `, de 21 a 39 bytes;
    - 9 × 80 bytes de fragmentos;
    - 8 etiquetas, cada una de 8 bytes de separadores más la palabra clave;
    - ` …` y ` (N aristas)`.

    Eso da 974 bytes con etiquetas `$ref`/`not` y columnas de 7 cifras, y unos
    1 115 bytes en el extremo de 8 etiquetas `dependentSchemas`.
  - **Entrada mínima.** El ciclo más barato que llena los 9 fragmentos es un elemento
    de un `allOf`, sin clave: `{"$anchor":"a","not":{"not":…{"$ref":"#a"}…}}` con 7
    `not`, de 82 + 2k bytes (k es la longitud del ancla, así que ≥ 84). Las etiquetas
    largas cuestan en la entrada más de lo que añaden al mensaje. La autoarista más
    barata, `{"$anchor":"a","$ref":"#a"}`, cuesta ≥ 28 bytes para ≤ 337 de mensaje.
    Llegar a 12 exige anclas de una letra, que son como mucho 53, y con tan pocas
    dominan el prefijo compartido y la posición: medido, 5,2 y 8,0 veces con m = 50.
  - **Medido**, con ciclos como elementos de un `allOf` bajo 240 `not` de prefijo
    compartido: ciclos de 8 aristas, 11,35 veces con m = 3 000 y 11,2 con m = 2·10⁴;
    autoaristas, 11,3 y 10,9 veces.
  - **Variantes anteriores**, menos densas: la del auditor con `$defs`, 9,8 veces; el
    escenario H de [ciclos.md](ciclos.md) §5, 7,2 veces; `t16k.json`, ≈ 6 veces. El
    «≈ 9 veces» de la variante de 2,2 MB del escenario H es una **estimación**
    (1 015 ÷ 110), no una medida.
- **Camino de éxito de `schemac ir` (SA-L2 de la revisión de la 4.2).** La tabla de
  arriba solo acota los errores. Con un esquema válido, `schemac ir` escribe la IR entera,
  que es más grande que la entrada, y la construye en memoria antes de escribirla.
  Medido en release con `{"allOf":[{},{},…]}`, la entrada más densa en nodos (un
  subesquema `{}` cada 3 bytes). **Remedido el 28 oct tras la tarea 4.3**, que añade la
  clase y la estrategia de cada nodo y regla (formato JSON v2); entre paréntesis, la
  medida del 27 oct con el formato v1 (después del cambio de los literales a listas de
  tokens):

  | Entrada | Salida JSON | Salida DOT | Memoria máx. `ir` | Memoria `ir --dot` | Memoria `check` |
  |---|---|---|---|---|---|
  | 0,30 MB | 27,0 MB (**90×**; v1: 75×) | 9,4 MB (31×; v1: 30×) | 54 MiB (v1: 50) | 37 MiB | 27 MiB |
  | 8,1 MB | 752 MB (**93×**; v1: 78×) | 278 MB (**34×**; v1: 33×) | 1 221 MiB (**≈ 158×**; v1: 143×) | 856 MiB (111×) | 645 MiB (83×) |

  Unos 278 bytes de JSON por nodo `{}` de 3 bytes (230 en v1: `class` y `strategies`
  suman ≈ 48 por nodo). La memoria de la entrada pequeña está dominada por el tamaño
  base del proceso; la cifra que escala es la de 8 MB. La **salida** (≈ 93×) queda por
  debajo de ρ(r) ≈ 131, pero la **memoria** (≈ 158× la entrada, frente a ≈ 83× de
  `check`) **supera** ρ(r): la dominan la IR, el AST y el `String` con la salida entera
  (la clasificación solo añade un vector de un par de bytes por nodo y el CSR de E₌).
  Por eso el requisito 4 de §2 dimensiona `RLIMIT_AS` con la memoria medida (≈ 158×),
  **no con ρ**, que solo acota el texto de salida. Con el tope de 16 MiB de `schemac`
  (`00-alcance.md` §4) una compilación puede necesitar ≈ 2,7 GB y escribir ≈ 1,6 GB.
  **Consecuencia para el servidor:** el tope de subida y el `RLIMIT_AS` del requisito 4
  de §2 se fijan juntos (memoria ≈ 165 × tope de subida, más el proceso base); por
  ejemplo, 1 MiB de subida necesita ≈ 175 MiB. **Opción para la
  Fase 7:** escribir `write_json`/`write_dot` directamente en la respuesta con un
  adaptador `fmt::Write` → `io::Write`, sin el `String` intermedio (quita la salida de la
  memoria), decidiendo qué se hace si hay un pánico a medio escribir (la respuesta ya
  empezó).
- **Cuándo actuar:** en el servidor de la aplicación web (Fase 7; está en la tarea del
  backend de la semana 4 del plan). La Fase 3 sigue devolviéndolos todos, igual que
  `schemac`, y el formato de los mensajes no cambia: quien presenta la lista la trunca o
  la agrupa (por ejemplo, el motivo una sola vez por clase). La tarea del backend debe
  tener un **criterio de aceptación explícito** que cubra las tres cosas juntas, porque
  sin ellas una subida pequeña produce una respuesta de hasta ρ(r) ≈ 126 + 0,67·r veces
  su tamaño (r es lo que el servidor ponga delante de cada mensaje):
  - límite de tamaño de subida (`docs/fase2/pendientes.md` §6), que con el factor de
    arriba acota también el tamaño de la respuesta;
  - truncar la lista a N errores, con «… y K más»;
  - tope de CPU (tiempo) por compilación.
  Opcional: una variante `check_cycles_limited(table, n)` que deje de construir errores
  tras los primeros n, para no pagar su costo cuando no se van a mostrar.

### 2. `debug_assert!` en código que llamará el servidor

- **Estado:** `path_in` (`crates/semantic/src/cycles.rs`) afirma con `debug_assert!` dos
  invariantes de la búsqueda del camino de un ciclo. En release no hacen nada: la
  salvaguarda devuelve un camino vacío y el error de la componente se informa igual (lo
  fijan `camino_a_otra_componente_es_vacio_en_release` y
  `cadena_de_pred_rota_es_vacia_en_release`, que solo corren con
  `cargo test -p semantic --release`; `make test` compila en debug y corre las gemelas
  `#[should_panic]`). En debug, si un cambio futuro rompiera el invariante, harían pánico.
- **Cuándo actuar:** al integrar la Fase 3 en el servidor (Fase 7). La tarea del backend
  debe cumplir los **cuatro requisitos a la vez** (ninguno sustituye a otro):
  1. cada compilación de esquema corre en su propio hilo con `stack_size` explícito (D7)
     y el servidor la espera con `join()` (o `std::panic::catch_unwind`), convirtiendo un
     pánico en un error 500 de **esa** petición, como ya hace `schemac`
     (`crates/cli/src/main.rs`, hilo `schemac-check` y la rama `Ok(Err(_))` del `join`);
  2. el perfil con que se compila el servidor nunca usa `panic = "abort"`: con él un
     pánico tumba el proceso aunque esté en otro hilo (lo recuerda un comentario en el
     `Cargo.toml` del workspace);
  3. compilar en release es recomendable por rendimiento, pero **no es una medida de
     seguridad**: quita los `debug_assert!`, no los pánicos de `assert!`, índices o
     `unwrap`. Y un desbordamiento de pila aborta el proceso aunque ocurra en un hilo
     aparte (no se puede capturar con `join` ni `catch_unwind`), por eso el tope de
     profundidad del parser y la pila explícita de las fases siguientes (D7) son
     obligatorios.
  4. **tope de memoria por compilación** (L-2 de la revisión final de 4.1): cada
     compilación corre en un **proceso hijo** con `RLIMIT_AS` o en un cgroup con límite
     de memoria. Un fallo de asignación aborta el proceso entero, igual que un
     desbordamiento de pila: ni `join` ni `catch_unwind` lo capturan, así que el aislamiento
     por hilo de los requisitos 1 y 2 no basta. La cifra: la compilación completa hasta la
     IR exportada usa ≈ 158 veces el tamaño del esquema (≈ 83 veces solo hasta la Fase 3;
     medido en §1, «Camino de éxito de `schemac ir`», con el formato v2 de la tarea 4.3),
     así que `RLIMIT_AS` ≈ 165 × tope de subida + el proceso base. Se dimensiona con esa
     memoria y no con ρ(r) ≈ 131 de §1, que acota el texto de salida y queda por debajo
     de ella. **Además de `RLIMIT_AS`**, el servidor debe **consumir la salida en
     streaming** (escribir la IR directamente en la respuesta, la opción de §1) **o fijar
     un límite de bytes de salida**: con el tope de 16 MiB de `schemac`, el peor caso
     medido es ≈ 1,5 GiB de salida JSON y ≈ 2,5 GiB de memoria por compilación (≈ 1,6 GB y
     ≈ 2,7 GB, §1), que ningún cliente web necesita recibir.

### 3. Obligaciones del frontal XSD (tarea XSD de la semana 2)

Observaciones de la revisión final de 4.1 (L-3, I-1, I-2, S-5). `verify` es defensa en
profundidad ([ir.md](ir.md) §4): todo lo que un XSD del **usuario** pueda romper lo tiene
que rechazar antes el frontal, **con su posición**, no como el error interno de `verify`
(código 3 de `schemac ir`, [bajada.md](bajada.md) §4).

- **Lista de [ir.md](ir.md) §4, «Obligaciones del frontal XSD»** (L-3): derivación
  circular de tipos simples (iterativa, D7), I12, la parte de I13 que viene del usuario,
  *Unique Particle Attribution* y que `default`/`fixed` sean valores del tipo.
- **La lista se queda corta** (I-1): I4, I10 e I11 también los rompe un XSD del usuario:
  atributos repetidos (I10), `minOccurs` > `maxOccurs` (I11), una faceta repetida en una
  misma restricción o `length` junto a `minLength`/`maxLength` (I4, porque `length` baja
  a las dos reglas), y además `min*`/`max*` inclusivo y exclusivo a la vez. **Regla
  general:** el frontal rechaza con su posición toda *Schema Component Constraint* y toda
  restricción `src-*` de XSD 1.0 que caiga en el subconjunto; ampliar la lista de
  [ir.md](ir.md) §4 con ella.
- **`xs:float`/`xs:double`** (I-2; frontal XSD y Fase 4): se comparan en IEEE
  binary32/binary64 (XSD 1.0 Parte 2 §3.2.4–3.2.5), no en decimal exacto como dicen hoy
  [ir.md](ir.md) §5.1 y §7 (fila Q). Hay que corregir §5.2, §7 y el pendiente 3 de §11, o
  documentar la desviación en `00-alcance.md` §3.2. No se corrigió en la 4.2.
- **Orígenes sin fijar** (S-5): el de un tipo anónimo dentro de un `xs:element`
  **global**, y qué hacer con `xs:simpleContent` + `xs:extension` sobre un tipo complejo
  de contenido simple.

### 4. Ubicación recortada para los mensajes del validador (Fases 4 y 5)

- **Estado:** `Ir::location` construye la ruta entera, sin tope ([ir.md](ir.md) §11,
  pendiente 2). La 4.2 no la usa en ningún mensaje: `schemac ir` escribe el padre y el
  paso de cada nodo, no su ruta ([bajada.md](bajada.md) §5.1).
- **Cuándo actuar:** al escribir los mensajes de error del payload (Fase 4) y el codegen
  (Fase 5): usar `location_clipped(id, max)`, análogo a `SymbolTable::fragment_clipped`,
  en O(profundidad + max), con `max` fijado en `00-alcance.md` §4; y añadir su fila a la
  tabla ρ de §1.

### 5. Construcción del DPDA (Fase 4)

Observaciones de la revisión final de 4.1 sobre [ir.md](ir.md) §6–§7:

- **Tope de estados para todo producto** (L-1): no solo `anyOf`/`oneOf`; también `allOf`,
  `Ref` con reglas hermanas y `not` sobre un subesquema (c). Contar estados y transiciones
  **antes** de reservar. Caso de prueba: un `allOf` de `pattern` `^(a{p})*$` con los 15
  primeros primos (el producto tiene ∏ p estados).
- **Registros de la fila Γ** (S-1): a la máscara le faltan los bits de `DependentRequired`
  (D5); falta la tabla hash acotada de claves no declaradas (D2,
  `max_claves_adicionales`); y decir que los registros salen «de las reglas (b)» no es
  exacto.
- **`not` y los límites** (S-2): un error de límite atraviesa `not` sin invertirse, y la
  inversión exige un veredicto exacto (con `uniqueItems` por hash no lo es).
- **Componentes del producto** (S-3): son el conjunto de `NodeId` **distintos** alcanzados
  por E₌, no los caminos (la cadena de `cadena_de_referencias_tamano_lineal`,
  [bajada.md](bajada.md) §7, tiene 2^(10⁴ − 1) caminos y 10⁴ nodos); los combinadores se
  evalúan como un circuito, con multiplicidad en `oneOf`.

### 6. CI

- **Permisos y versiones de las acciones** (SA-L5, anterior a la 4.1): los flujos de
  `.github/workflows/` no tienen bloque `permissions: contents: read`, y las acciones
  (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`…)
  están fijadas por etiqueta, no por SHA.
- **Paso de release** (`.github/workflows/ci.yml`, `cargo test -p semantic --release`, que
  corre las salvaguardas de §2): no se ha visto correr en GitHub; confirmarlo en el primer
  push.

### 7. Abiertas a propósito en la tarea 4.0

- `pointer::encode_char` (`crates/semantic/src/pointer.rs`) devuelve `&[u8]`; podría
  devolver `&str` y ahorrar la conversión byte a byte de `emit_head`/`emit_tail`.
- `Seg::Num` reserva un `String` por índice (`i.to_string()`) cada vez que
  `emit_head` y `emit_tail` (`crates/semantic/src/symbols.rs`, `fragment_clipped`)
  recorren un paso `Index`.

### 8. Código de salida de un pánico en `schemac check` (tarea 4.2)

- **Estado:** `schemac ir` da **3** para un error interno, sea un fallo de `verify` o un
  pánico en su hilo ([bajada.md](bajada.md) §4). `schemac check` conserva el **2** que ya
  daba a un pánico («no se pudo leer o analizar»). Unificarlo (pánico = 3 en los dos) cambia
  la interfaz de `check`; queda propuesto para cuando lo decida el usuario.

### 9. Aviso para «`allOf` + `additionalProperties: false`» (Fase 7, o cuando haya avisos)

- **Estado:** el pendiente 5 de [ir.md](ir.md) §11 propone un **aviso** (no un error) para
  ese patrón, que casi nunca hace lo que se quiere ([ir.md](ir.md) §6.2). La 4.2 no lo
  implementa: `schemac` no tiene todavía un canal de avisos (solo errores, con su código de
  salida) y añadirlo cambia la interfaz.
- **Cuándo actuar:** cuando exista ese canal, a más tardar en la aplicación web (Fase 7),
  que muestra el resultado de la compilación. La comprobación es local a un nodo de la IR
  (`AllOf` y `Properties { additional: Some(n) }` con `n` = `Reject`), O(1) por nodo.

### 10. Formato JSON de `schemac ir` en JavaScript (Fase 7)

- **Estado:** los contadores (`minLength`, `maxItems`…) salen como enteros JSON y pueden
  valer hasta 18446744073709551615 (`u64::MAX`, saturado; [bajada.md](bajada.md) §5.1),
  más que `Number.MAX_SAFE_INTEGER`. `JSON.parse` los redondea.
- **Cuándo actuar:** en la web (Fase 7), al leer la IR: tratarlos con `BigInt` (o como «sin
  límite» si valen `u64::MAX`), o cambiar el formato a cadenas subiendo `irVersion`.

### 11. Ordenación de las claves de los literales (Fase 6)

- **Estado:** la bajada es O(n) salvo la ordenación de las claves de cada objeto literal de
  `enum`/`const` (I9), O(k log k) por objeto de k miembros, O(n log n) en el peor caso
  ([bajada.md](bajada.md) §6). Es inherente a I9 con comparaciones.
- **Cuándo actuar:** si el benchmark del crate `bench` (Fase 6) muestra que pesa en esquemas
  con literales grandes, cambiarla por una ordenación por radix sobre los bytes de las
  claves, lineal en su longitud total.

### 12. Etiquetas DOT de la IR sin escapes de RFC 6901 (cosmético)

- **Estado:** el DOT de `schemac ir --dot` etiqueta cada nodo con su paso desde el padre,
  por ejemplo `properties/a/b~c`, sin los escapes `~0`/`~1` que sí pone `Ir::location`
  (`#/properties/a~1b~0c`). La etiqueta es un texto para leer, no un puntero: los nombres
  van recortados con `short` y no se pueden resolver ([bajada.md](bajada.md) §5.2). Por eso
  una clave con `/` se lee ambigua en el diagrama.
- **Cuándo actuar:** si un diagrama lo necesita, escapar el nombre con RFC 6901 antes de
  `short`. No afecta al JSON de `schemac ir`, que separa `keyword` y `entry`.

### 13. Tipo `AnalyzedTable` para la entrada de la bajada (Fase 4)

- **Estado:** `semantic::lower` exige una tabla que ya pasó `analyze`, pero acepta
  cualquier `&SymbolTable`, también la de `SymbolTable::build` sola (CR-S-2 de la revisión
  de la 4.2). Con ella, un error del esquema del usuario (un ciclo no productivo, que
  rompe I14) saldría como error interno (código 3) en lugar de como error del esquema con
  su posición. Hoy solo lo documenta `lower` ([bajada.md](bajada.md) §1).
- **Cuándo actuar:** antes de que otro llamador use `lower` (el constructor del DPDA de la
  Fase 4, o el servidor de la Fase 7): que `analyze` devuelva un tipo `AnalyzedTable` que
  solo se pueda obtener de ella y que `lower` lo exija, igual que `VerifiedIr` para la
  Fase 4.

### 14. El «registro» de D15 es stderr (Fase 7)

- **Estado:** D15 manda la lista de errores de `verify` «solo al registro». `schemac` no
  tiene registro aparte: el error único y, debajo, el registro acotado (el recuento y los
  8 primeros, con el prefijo `schemac: registro:`) salen los dos por stderr
  ([bajada.md](bajada.md) §4).
- **Cuándo actuar:** en el servidor (Fase 7): el usuario recibe solo el error único y el
  registro va al log del servidor. Si `schemac` llega a tener un modo de registro (un
  archivo, `--log`), las líneas `schemac: registro:` se mueven allí.

### 15. Texto del usuario en bruto en el JSON de la IR (Fase 7, frontal web)

- **Identificadores:** SA-I1 (revisión de la ronda 2), **4.2-I2** (revisión final de la
  4.2) y **4.3-I1** (revisión de seguridad de la 4.3): la misma observación.
- **Estado:** el JSON de `schemac ir` es fiel al esquema: claves, nombres y cadenas van
  enteros, sin recortar, con solo los escapes de RFC 8259 (SA-I1 de la revisión de la
  4.2). Pueden llevar U+202E (texto bidireccional), U+2028, U+0085, DEL o marcado HTML.
- **Cuándo actuar:** en la tarea del frontal de la Fase 7: la web que muestre la IR inserta
  ese texto con `textContent` (o el equivalente que no interpreta marcado), **nunca con
  `innerHTML`**, y conviene marcar los caracteres de control y bidireccionales al
  mostrarlos.
- **4.3-I1 (añadido en la 4.3):** el JSON deja además `<`, `>`, U+2028 y U+2029 sin escapar
  (es JSON válido: RFC 8259 no los obliga a escapar). Si la web lo incrusta en un
  `<script>` (U+2028/U+2029 terminaban literales de cadena en JavaScript anterior a
  ES2019, y `</script>` cerraría el bloque) o lo inserta con `innerHTML`, debe tratarlo
  como texto no confiable: cargarlo con `fetch` + `JSON.parse`, o escapar `<`, `>`, `&`,
  U+2028 y U+2029 como `\u…` al incrustarlo.

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

### 19. 4.2-L1: abrir una FIFO sin escritor bloquea `schemac` — RESUELTO (tarea 4.3; el requisito de la Fase 7 sigue)

- **Resuelto:** `leer` comprueba con `fs::metadata` que la ruta es un archivo regular
  **antes** de abrirla, y otra vez sobre el descriptor; el comentario de `MAX_SCHEMA_BYTES`
  y [bajada.md](bajada.md) §5 lo dicen, y documentan que las rutas de la CLI son de
  confianza (entre las dos comprobaciones queda una carrera si alguien cambia la ruta por
  una FIFO). `fifo_sin_escritor_no_bloquea` (`crates/cli/tests/es.rs`, solo Unix, FIFO
  creada con `mkfifo` y una guarda de 60 s que mata el proceso) da 2 en `check` e `ir`; sin
  el arreglo se cuelga y falla. **Sigue en pie** el requisito para el servidor de la Fase 7:
  recibe bytes, nunca rutas de los usuarios. La carrera que queda entre las dos
  comprobaciones es **4.3-L1** (§23).

- **Dónde:** `crates/cli/src/main.rs:124` (`leer`: `File::open` antes de `is_file()`).
- **Estado:** `File::open` sobre una FIFO sin escritor se bloquea antes de llegar a la
  comprobación `is_file()`: `mkfifo f; timeout 5 schemac ir f` termina con 124 (colgado).
  «Una FIFO nunca se carga» es cierto en memoria, no en tiempo. La frase ya está corregida
  en [bajada.md](bajada.md) §5; la del comentario de `MAX_SCHEMA_BYTES`
  (`crates/cli/src/main.rs:44-46`) hay que corregirla con el arreglo, porque este cierre solo
  toca `docs/`. Solo importa si un proceso de confianza pasa a `schemac` rutas que eligen
  los usuarios.
- **Arreglo propuesto:** abrir con `O_NONBLOCK` (`OpenOptionsExt::custom_flags` en Unix), o
  llamar a `std::fs::metadata(path)` antes de `open` y rechazar lo que no sea un archivo
  regular, manteniendo `is_file` sobre el descriptor. Documentar además que las rutas de
  la CLI son de confianza.
- **Cuándo:** la próxima tarea que toque la CLI. Requisito para el servidor de la Fase 7:
  recibe **bytes**, nunca rutas de usuarios.

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

### Abiertas por la tarea 4.3 (clasificación)

### 22. Clasificación: `$ref` con hermanas y XSD (Fase 4 y frontal XSD)

- **`$ref` con reglas hermanas (Fase 4; CR1-S6).** Es (a) por el alcance (`00-alcance.md` §2.1),
  pero la Fase 4 lo compila como un producto con sus hermanas ([ir.md](ir.md) §7), igual
  que un `allOf` de dos ramas, que es (c). Una cadena de `$ref` con hermanas crece como un
  producto; el tope de estados de §5 (L-1) ya lo cubre. Si la Fase 4 mide que se comporta
  como el producto de `allOf`, proponer al usuario cambiar la fila de `$ref` en el alcance
  ([clasificacion.md](clasificacion.md) §8, pendiente 1). Mientras, manda el alcance.
- **Frontal XSD (tarea XSD de la semana 2).** La tabla ya clasifica las reglas de XSD y
  `tabla_coincide_con_el_alcance` cubre sus filas de §3.1, pero ningún XSD real pasa todavía
  por `classify`: el frontal añade sus esquemas (y el caso `valid-xml-pedido-01`) a
  `corpus_clasificado` (`crates/semantic/tests/corpus.rs`, o su equivalente en el crate del
  frontal).
- **Regenerar la tabla escrita (CR1-S7, en parte).** El bloque de [clasificacion.md](clasificacion.md) §3 no
  lo regenera `make docs`: lo comprueba la prueba `tabla_escrita_es_la_del_codigo`, que al
  fallar imprime la tabla que hay que copiar. Si se quiere en `make docs`/`make docs-check`,
  añadir un subcomando (por ejemplo `schemac tabla-clasificacion`) cuando haya otro motivo
  para tocar la CLI.
- **Un solo caso `valid` JSON sin palabras clave (c) (T-4; corpus de la Fase 8 o la próxima
  tarea que añada casos).**
  - **Dónde:** `tests/cases/valid/` (`valid-recursive-tree-01` es el único caso `valid` en
    JSON); `corpus_clasificado` (`crates/semantic/tests/corpus.rs`).
  - **Estado:** el criterio «`valid` sin palabras clave (c) → ningún nodo (c)» se comprueba
    sobre ese único caso; desde la ronda 1 de la 4.3 también sobre los esquemas de
    `tests/schemas/parse-ok` (al menos 5), así que ya no depende de uno solo.
  - **Arreglo propuesto:** añadir casos `valid` con objetos, arreglos, números y cadenas
    sin combinadores cuando se amplíe el corpus.

### Abiertas por la revisión final de la 4.3

### 23. 4.3-L1: carrera entre `fs::metadata` y `File::open` en `leer` (próxima tarea que toque la CLI)

- **Dónde:** `crates/cli/src/main.rs:152-164` (`leer`).
- **Estado:** el arreglo de §19 comprueba que la ruta es un archivo regular con
  `fs::metadata(path)` y después la abre con `File::open(path)`. Entre las dos llamadas otro
  proceso puede cambiar la ruta por una FIFO sin escritor, y el `open` se bloquea para
  siempre: la comprobación `is_file()` sobre el descriptor llega tarde. Es solo una
  denegación de servicio de esa invocación; las rutas de la CLI son de confianza
  ([bajada.md](bajada.md) §5) y el servidor de la Fase 7 pasa bytes, no rutas. Seguridad,
  Low.
- **Arreglo propuesto:** en Unix, abrir con `OpenOptionsExt::custom_flags(O_NONBLOCK)` (el
  `open` de una FIFO no espera al escritor) y decidir solo con `is_file()` sobre el
  descriptor, sin la primera comprobación; en Windows basta lo de hoy. `O_NONBLOCK` vale
  distinto en Linux y macOS: tomarlo de `libc` o del sistema, sin `unsafe`.
- **Cuándo:** la próxima tarea que toque la CLI.

### 24. 4.3-I2: directorios temporales con nombre predecible en las pruebas de la CLI (opcional)

- **Dónde:** `crates/cli/tests/es.rs:30` (`dir_temporal`: `temp_dir()` +
  `schemac-es-{nombre}-{pid}` + `create_dir_all`), `crates/cli/tests/ir.rs:139`, `:155` y
  `:181`, `crates/cli/tests/check.rs:67`.
- **Estado:** los nombres son predecibles, así que en un `/tmp` compartido otro usuario
  podría adelantarse con un enlace simbólico. Solo afecta a pruebas locales. Seguridad,
  Info.
- **Arreglo propuesto:** usar `tempfile` (directorio con nombre aleatorio y permisos 0700)
  como dev-dependency, o aceptarlo y dejarlo escrito.
- **Cuándo:** opcional; la próxima tarea que toque las pruebas de la CLI.

### 25. 4.3-I3: `schemac ir /proc/self/mem` da un error de E/S, no «no es un archivo regular» (cosmético)

- **Dónde:** `crates/cli/src/main.rs:152-170` (`leer`).
- **Estado:** procfs declara sus archivos como regulares, así que pasan las dos
  comprobaciones de `is_file()`; la lectura falla con un error de E/S y `schemac` termina
  con código 2 y ese mensaje. El código es el correcto; solo cambia el texto. Seguridad,
  Info.
- **Arreglo propuesto:** ninguno obligatorio; si se quiere, nombrar el caso en
  [bajada.md](bajada.md) §5.
- **Cuándo:** cosmético; cuando se toque `leer`.

### 26. 4.3-S1: la fila `minOccurs`/`maxOccurs` del alcance, columna «Soporte» (frontal XSD o punto de control B)

- **Dónde:** `docs/00-alcance.md:129` (§3.1).
- **Estado:** la celda «Cat.» ya dice cuándo la ocurrencia es (b), pero la columna
  «Soporte» sigue diciendo «Contador acotado en el marco de pila» sin matiz, y la celda
  «Cat.» solo nombra `?`, `*` y `+` como (a), no «una vez» ni `maxOccurs="0"`, que también
  lo son ([clasificacion.md](clasificacion.md) §3.2, decisión 1).
- **Arreglo propuesto:** «Soporte»: «Contador acotado en el marco de pila cuando hace
  falta (no se expande el modelo); el resto, estados del Glushkov»; y en «Cat.», añadir
  «una vez y `maxOccurs="0"`» a la lista de (a). Es un cambio del alcance (fuente de verdad
  3): lo revisa el usuario.
- **Cuándo:** la tarea del frontal XSD, o antes, con la revisión del usuario del punto de
  control B.

### 27. 4.3-S2: «en un solo marco» en `clasificacion.md` §4.2 supone que E₌ se expande (Fase 4)

- **Dónde:** [clasificacion.md](clasificacion.md) §4.2, primera frase.
- **Estado:** dice que la instancia de *v* la lee «en un solo marco, el producto» de sus
  reglas y de los destinos de E₌. Eso es así si la Fase 4 expande E₌; si compila `$ref` como
  llamada ε ([ir.md](ir.md) §7 deja abiertas las dos opciones), los registros del destino
  viven en el marco que apila la llamada. La clase y las estrategias del nodo no cambian.
- **Arreglo propuesto:** matizar: «(si E₌ se expande; con llamada ε, el marco del destino
  aporta los mismos registros y estrategias)».
- **Cuándo:** la Fase 4, al elegir entre expansión y llamada ε.

### 28. 4.3-S3: la prueba `ir_senala_el_nodo_c_y_su_estrategia` reserializa el esquema del caso (próxima tarea que toque la CLI)

- **Dónde:** `crates/cli/tests/ir.rs:179` (`caso["schema"].to_string()`).
- **Estado:** lee el caso con `serde_json` y lo reescribe, que es lo que §18 quiso evitar
  (claves ordenadas, números por `f64`). No falla porque sus aserciones no dependen del
  orden de las claves ni de lexemas.
- **Arreglo propuesto:** recortar el texto original del valor `schema` como hace
  `crates/semantic/tests/common/mod.rs` (`texto_del_esquema`), añadiendo `schema-parser`
  como dev-dependency de `cli` o moviendo esa ayuda a un sitio común.
- **Cuándo:** la próxima tarea que toque la CLI.

### 29. 4.3-S4: `menciona_palabra_c` hace `unwrap` sin nombre de archivo (próxima tarea que toque el corpus)

- **Dónde:** `crates/semantic/tests/corpus.rs:193` (`parse(texto).unwrap()`).
- **Estado:** hoy no puede fallar (los esquemas ya pasaron la Fase 3 antes de llegar
  aquí), pero si fallara el mensaje no diría qué archivo.
- **Arreglo propuesto:** pasar el nombre y usar `unwrap_or_else(|e| panic!("{nombre}: {e}"))`.
- **Cuándo:** la próxima tarea que toque el corpus de la Fase 3.

### 30. 4.3-S5: `write_json` y `write_dot` clasifican cada uno la IR (Fase 7)

- **Dónde:** `crates/ir/src/export.rs` (`write_json` y `write_dot` llaman a `classify`).
- **Estado:** cada exportación calcula la clasificación, O(n). Hoy `schemac ir` solo
  escribe un formato por invocación, así que no cuesta nada.
- **Arreglo propuesto:** si la Fase 7 quiere los dos formatos de una misma IR, recibir la
  `Classification` como parámetro (o una variante `write_*_with`).
- **Cuándo:** la Fase 7 (servidor).

### 31. 4.3-S6: `estrategias_de_la_tabla` repite el `match` del código (abierta a propósito)

- **Dónde:** `crates/ir/tests/classify.rs:215-235` (`estrategias_de_la_tabla`).
- **Estado:** la prueba repite casi literalmente la asignación de estrategias de
  `RuleKind::classification`: es en parte tautológica. Se deja porque el alcance no tiene
  columna de estrategia contra la que comparar, y la prueba protege de cambios
  accidentales en la tabla (la estrategia de cada regla sale del texto del alcance y de
  D5).
- **Arreglo propuesto:** ninguno; si el alcance añade una columna de estrategia, leerla
  como hace `tabla_coincide_con_el_alcance` con «Cat.».
- **Cuándo:** abierta a propósito.

### 32. 4.3-S7: cosméticos de `classify.rs` y `export.rs` (próxima tarea que toque `crates/ir`)

- **Dónde:** `crates/ir/src/classify.rs:256-265` (`NodeClass::join`), `Classification::node`
  y `crates/ir/src/export.rs:111` (`Rule::assignment` por regla).
- **Estado:** `join` compara a mano en lugar de `self.class.max(o.class)`;
  `Classification::node` hace pánico si se usa con otra IR (documentado en `# Panics`);
  la exportación recalcula `Rule::assignment` de cada regla, que `classify` ya calculó
  (O(n) en total, sin costo apreciable).
- **Arreglo propuesto:** usar `Ord::max`; si `Classification` se pasa entre fases, ligarla
  a su IR (por ejemplo, devolverla junto a la `VerifiedIr`); guardar las asignaciones por
  regla si algún día pesan.
- **Cuándo:** la próxima tarea que toque `crates/ir`.
