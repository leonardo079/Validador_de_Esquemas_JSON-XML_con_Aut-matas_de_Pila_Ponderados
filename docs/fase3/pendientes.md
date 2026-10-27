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
  debe cumplir los **tres requisitos a la vez** (ninguno sustituye a otro):
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
