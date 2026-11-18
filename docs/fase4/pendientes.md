# Pendientes de la Fase 4 (autómata de pila ponderado)

Observaciones que dejaron abiertas las tareas F4.1a (modelo formal de M, pesos, núcleo
`no_std` y esqueleto JSON) y F4.1b (registros, escalares y conjunción por E₌); teoría en
[automata.md](automata.md). Lo resuelto se marca y se conserva.
Las decisiones de diseño viven en el «Registro de decisiones» del plan; aquí solo se
remite a ellas.

## Vigilar más adelante

### 1. Claves no declaradas repetidas (F4.1a-P1)

- **Estado:** M detecta con su máscara las claves **declaradas** repetidas (peso ∞, D2);
  una clave sin bit repetida (`{"x": 1, "x": 2}` con `x` fuera de `properties`,
  `required` y `dependentRequired`) se acepta.
- **Arreglo propuesto:** la capa de buena formación del ejecutor (F4-B7): tabla acotada
  por `max_claves_adicionales` (1 024 por objeto, reserva común de 4 096; F4-U16, F4-B22)
  en la memoria del llamador, con error en la repetida.
- **Dónde:** `crates/runtime-rs/src/core.rs` (`Core::step`, rama de `Trans::bit`).
- **Tarea:** F4.6.

### 2. Límites de F4-U6 en `00-alcance.md` §4 (F4.1a-P2; interfaz compartida, D42)

- **Estado:** el código aplica los límites de F4-U6 (aprobados por las dos personas en
  D47): 2²⁰ entradas de δ por esquema (`automaton::MAX_DELTA`, sobre |Q| · |Σ|, con una
  cota de |Q| contada antes de reservar), 4 096 nombres con bit por objeto
  (`automaton::MAX_NAMES`) y 256 bytes de `location_clipped` (`automaton::LOCATION_MAX`).
  `docs/00-alcance.md` §4 todavía no los lista (el divisor de `multipleOf` es de F4.1b).
- **El presupuesto es conservador (S6 / T-2 de la revisión):** se compara con 2²⁰ una
  **cota** superior de |Q| · |Σ| calculada antes de reservar, no el tamaño real: sobreestima
  como mucho un estado por nodo (el de número propio, que no todos tienen) más los prefijos
  compartidos del trie (se cuenta un estado por carácter de cada nombre). Por eso
  `BuildError::Limit::found` (hoy `crates/automaton/src/json.rs:849`, `delta_limit`) da esa cota y no el
  número de entradas de δ que tendría M; el mensaje («límite superado: N entradas de δ…»)
  debe decir «como mucho N» o dar el tamaño real. Se corrige cuando una tarea toque ese
  código (F4.1b, que añade el tope por producto, o F4.1c, que añade el presupuesto del
  Glushkov).
  **RESUELTO el mensaje (F4.1b):** `what` dice «entradas de las tablas como mucho (cota de
  estados × clases de δ, más comprobaciones, máscaras y lexemas)»
  (`crates/automaton/src/json.rs:849-850`, `delta_limit`), con la ubicación del conjunto que agota
  el presupuesto (o la raíz en la comprobación final); la cota es ahora
  exacta en los tries (prefijos contados sin construirlos) y solo sobreestima por los estados
  de número y de cadena que comparten varios caminos. F4.1b añade al §4 de `00-alcance.md`
  dos límites más para las dos personas (§22).
- **Arreglo propuesto:** añadir las tres filas a la tabla de §4, precisando que el
  presupuesto de transiciones se mide en entradas de la tabla densa (|Q| · |Σ|). Lo
  escribe quien cierre F4.1a con el visto bueno de las dos personas (no lo cambia un
  agente, D42).
- **Tarea:** cierre de F4.1a (orquestador).

### 3. Partición de caracteres por contexto (F4.1a-P3) — RESUELTO para F4.1b (partición común)

- **Estado:** F4-B3 pide clases de carácter separadas por contexto (clave o valor); en
  F4.1a hay una sola partición, la de los nombres de los objetos, que usan también las
  cadenas valor (sin reglas que distingan caracteres, da el mismo resultado).
- **Arreglo propuesto:** cuando las cadenas valor tengan tries o AFD (`enum` de cadenas en
  F4.1b, `pattern` en F4.4b), particionar por contexto: el estado ya sabe si lee una clave
  o un valor, así que basta con dos tablas de clases o con la partición común más fina;
  vigilar que |Σ| no crezca con el producto de las dos.
- **Dónde:** `crates/automaton/src/json.rs` (`Builder::new`, clases de carácter).
- **Tarea:** F4.1b (y F4.4b).
- **Resolución (F4.1b):** una sola partición, la común más fina de los nombres con bit y las
  cadenas de los `enum`/`const` ([automata.md](automata.md) §2): |Σ| crece con la suma de los
  dos alfabetos, no con su producto, y el estado ya distingue clave de valor. Sigue abierto
  solo para F4.4b: decidir si los AFD de `pattern` (rangos de clases enteros) piden tablas por
  contexto para no inflar |Σ|.

### 4. Tries sin minimizar (F4.1a-P4)

- **Estado:** el trie de los nombres de cada objeto es un `Dfa` completo con un estado por
  prefijo; dos nodos con las mismas reglas tienen subautómatas distintos.
- **Arreglo propuesto:** Hopcroft (F4-B15) sobre cada `Dfa` antes de incrustarlo, y
  medir el efecto en la dorada (que entonces cambia: regenerar con `make doradas` y
  `make docs`, y recontar [automata.md](automata.md) §8).
- **Tarea:** F4.3.

### 4b. Cada conjunto producto de cadenas copia el trie entero (F4.1b-P6; rechazo claro)

- **Estado:** la parte de δ del presupuesto rechaza esquemas razonables con varios
  productos de cadenas sobre el mismo `enum`: cada conjunto (p. ej. `{"$ref":
  "#/$defs/country", "minLength": 1}`) tiene su propio subautómata de cadena con una copia
  del trie de los literales. Con 8 propiedades así y un `enum` de 250 nombres de 10 letras,
  `limit` con 1 055 988 entradas (≈ 2 260 estados × 72 clases por copia); con 4 propiedades
  compila (531 k); sin `minLength` las propiedades comparten un solo conjunto. Es un rechazo
  claro (`limit` con la ubicación), acorde con el alcance mínimo.
- **Dónde:** `crates/automaton/src/json.rs` (`Builder::reserve` y `fill_string`, un trie por
  conjunto con `info.string`).
- **Arreglo propuesto:** compartir las filas del trie entre los conjuntos con la misma unión
  de palabras y llevar las comprobaciones de cierre de cada conjunto a una tabla (conjunto,
  salida del trie); medir el efecto en el informe («tamaño del autómata»).
- **Tarea:** F4.3 (minimización, que ya une subautómatas iguales) o F4.6.

### 5. Forma del reporte frente a F4-U18 (F4.1a-P5; interfaz compartida)

- **Estado:** `runtime_rs::Report` tiene `valid`, `violations`, `error_kind` y el primer
  error (offset, línea, columna, regla). Añade `ErrorKind::Internal` (tablas
  incoherentes, defensa en profundidad; F4-U18 solo lista lexical/syntax/schema/limit).
  Si hay un error de peso ∞ es el primero aunque antes hubiera violaciones, y el offset es
  el del comienzo del evento que lo detecta ([automata.md](automata.md) §5). Las opciones
  son `Options { mode }`, sin límites todavía (F4-U18 habla de `&Limits`).
- **Arreglo propuesto:** F4.6 completa el reporte (ubicación recortada, puntero de la
  instancia con F6.1) y los límites, y propone a las dos personas la forma final de
  `sv_report_t` con estas tres precisiones.
- **Tarea:** F4.6.

### 6. Profundidad acotada solo por la memoria del llamador (F4.1a-P6)

- **Estado:** el núcleo apila mientras quepa en `mem`; si no cabe, error `limit`
  (`pila_llena_da_limit`). No hay todavía un tope `SV_MAX_DEPTH` independiente de la
  memoria, y `automaton::validate` reserva tantos marcos como `{` y `[` tiene el payload.
- **Arreglo propuesto:** el tope de profundidad (256 por defecto, `00-alcance.md` §4) en
  las opciones del núcleo y `sv_state_t` de tamaño fijo con él.
- **Tarea:** F4.6 (y F5.1).

### 7. `AnalyzedTable` (sigue abierta, `fase3/pendientes.md` §13)

- **Estado:** `automaton` no llama a `semantic::lower` (recibe `&VerifiedIr`), así que §13
  sigue sin afectar a la Fase 4; las pruebas bajan siempre tras `analyze`.
- **Tarea:** la del servidor (Fase 7) o la primera que llame a `lower` sin `analyze`.

### 8. El único `#[allow(unsafe_code)]` (F4.1a-P7) — DECIDIDO por el usuario el 16 nov: opción A

- **Estado:** `crates/automaton/tests/sin_reservas.rs` implementa `GlobalAlloc` (rasgo
  `unsafe`) para contar reservas (F4-B25, criterio 6 de F4.1a); solo delega en `System`.
  Es código de prueba, nunca del validador.
- **Decisión (16 nov, opción A, la que recomendaban el Tester y el Reviewer):** es la
  **única excepción nombrada** del invariante `unsafe_code = "deny"`: un solo
  `#[allow(unsafe_code)]`, sobre el `unsafe impl GlobalAlloc for Contador` de ese archivo y
  sus tres funciones (`alloc`, `dealloc`, `realloc`), que solo delegan en `System`.
  Cualquier otro `unsafe` en el workspace es un hallazgo Critical.
- **Comprobación:** `make lint` depende del objetivo `unsafe-check` del `Makefile`, que con
  solo `find` y `grep -E` falla, imprimiendo archivo y línea, si aparece
  `allow(unsafe_code)`, `unsafe impl`, `unsafe fn`, `unsafe extern`, `unsafe trait` o un
  bloque `unsafe {` en cualquier `.rs` fuera de `sin_reservas.rs`, o si en ese archivo hay
  algo más que las líneas autorizadas (o más de un `unsafe impl GlobalAlloc`). Las líneas de
  comentario no cuentan. Es una comprobación por patrones de texto (no ve, por ejemplo, un
  `unsafe` y su `{` en líneas distintas); la red de fondo sigue siendo `unsafe_code = "deny"`
  del workspace, que el compilador aplica en todo lo que no lleva el `allow`.
- **Descartadas:** (B) una dependencia de desarrollo como `stats_alloc` (en contra del
  alcance mínimo); (C) quitar la prueba y quedarse con la garantía estática (`#![no_std]`
  sin `alloc`).
- **Ronda 1 de F4.1b:** el asignador cuenta también los bytes pedidos (una línea segura más
  en `alloc` y en `realloc`, sin ningún `unsafe` nuevo; `unsafe-check` sigue pasando), para
  comprobar sin reloj que el constructor no reserva antes del tope de un producto.
- **Vigilar:** `scripts/lint.ps1` (Windows sin `make`) no tiene todavía esta comprobación.

### Observaciones abiertas de la revisión de F4.1a (code-reviewer, 16 nov)

### 9. F4.1a-CR-I1: instancias escalares frente a las salidas y puertas de F4.5a — RESUELTO el diseño (F4.1b); las puertas siguen para F4.5a

- **Dónde:** `crates/runtime-rs/src/core.rs:272-276` (`Op::Enter`/`Op::Leave`),
  `crates/automaton/src/json.rs:1415-1464` (`Builder::entry`, E(N, a, r) con `Op::Move` para `true`/`false`/
  `null`), [automata.md](automata.md) §1 y §9.2.
- **Estado:** F4-U10 y F4-B20 (D51) ponen las salidas «en el marco» y las puertas «al
  desapilar», y F4-U11 habla de «marcos escalares». Con el escalar no apilado (decisión 2
  de [automata.md](automata.md) §9), una cadena o un número no tienen marco ni `Pop`, y
  `true`/`false`/`null` son una sola transición. `{"not": {"type": "string"}}` sobre `"x"`
  o sobre `true` (criterio de F4.5a) necesita evaluar la puerta en `Leave` y en la
  transición de entrada de un valor de un solo evento, y guardar las salidas por rama en un
  registro global del escalar.
- **Arreglo propuesto:** generalizar «desapilar» a **cerrar la instancia** (`Pop`, `Leave`
  o la entrada de un valor de un evento): las puertas se evalúan en esas tres transiciones,
  y las salidas de la instancia escalar abierta viven en una zona global de tamaño máximo
  por esquema (como el registro del escalar). F4.1b ya lo necesita para atribuir las
  comprobaciones escalares de cada componente del producto en marcos escalares.
- **Tarea:** F4.1b (zona global y atribución por componente) y F4.5a (puertas en `Leave`
  y en la entrada).
- **Resolución (F4.1b):** [automata.md](automata.md) §3.6. La zona global existe en el
  formato (`Tables::scalar_slots`, 0 palabras, `crates/runtime-rs/src/tables.rs`) y el núcleo
  la reserva al principio de `mem` (`Core::new` empieza la pila detrás,
  `crates/runtime-rs/src/core.rs`); «cerrar la instancia» = `Pop`, `Leave` o la transición
  de entrada de un valor de un evento; dentro de un producto cada comprobación escalar ya va
  atribuida a su componente. **Queda para F4.5a:** dar tamaño a la zona, el mapa
  `Trans::outs` de las transiciones `Enter` hacia ella y las puertas en esas tres
  transiciones.

### 10. F4.1a-CR-I4: `BuildError::Limit` sin pruebas — RESUELTO (F4.1b)

- **Dónde:** `crates/automaton/src/json.rs:479` (más de 4 096 nombres por objeto) y
  `crates/automaton/src/json.rs:408` y `:967` (presupuesto de F4-U6, parcial y final).
- **Arreglo propuesto:** pruebas: un objeto con 4 096 nombres compila y uno con 4 097 da
  `limit` con su ubicación; un esquema por encima de 2²⁰ entradas da `limit` sin reservar
  (con el asignador contador de `sin_reservas.rs` o comprobando que no se llega a reservar
  δ).
- **Tarea:** F4.1b.
- **Resolución:** `limite_de_nombres_por_objeto` (4 096 compilan, 4 097 dan `limit` en
  `#/properties/o`) y `presupuesto_de_delta_da_limit_sin_reservar` (≈ 10¹¹ entradas, 1 TB: si
  se reservara antes de comprobar, la prueba abortaría), en
  `crates/automaton/tests/escalares.rs`.

### 11. F4.1a-CR-S1: la máscara ocupa todo el marco — RESUELTO (F4.1b)

- **Dónde:** `crates/runtime-rs/src/tables.rs:193-203` (`Frame::words`, hoy con `mask_words`) y
  `crates/runtime-rs/src/core.rs:442-448` (`Core::contains`, que comparaba
  `masks[m .. m + words]` con todo el marco).
- **Estado:** hoy `Frame::words` son las palabras de la máscara y no hay otros registros.
- **Arreglo propuesto:** al añadir contadores, separar la máscara (`mask_words` o un
  desplazamiento dentro del marco) para que `Test::Mask` y `Test::Dependent` no lean
  contadores.
- **Tarea:** F4.1b.
- **Resolución:** `Frame::mask_words`; registros `[máscara] [contador]`; `Core::top` da las
  palabras de la máscara y `contains`, `bit` y `set_bit` solo leen esas
  ([automata.md](automata.md) §3.1). Prueba: `mascara_separada_del_contador` (también con
  dos palabras de máscara).

### 12. F4.1a-CR-S2: una clase fuera de rango lee la fila siguiente (seguridad)

- **Dónde:** `crates/runtime-rs/src/core.rs:171` (clase de carácter) y
  `crates/runtime-rs/src/core.rs:238-240` (`Core::step`, índice `q · n_classes + a`).
- **Estado:** con tablas incoherentes, una clase `≥ n_classes − CHAR` (de `ascii` o
  `ranges`) cae en la fila del estado siguiente en lugar de dar `Fault::Internal`. Con las
  tablas que construye `automaton` no ocurre, pero el núcleo no debe confiar en ellas.
- **Arreglo propuesto:** `if a >= t.n_classes { Internal }` en `step`, con una prueba de
  tablas corruptas.
- **Tarea:** F4.6 (revisión de seguridad, D35).

### 13. F4.1a-CR-S3: la pila que reserva `automaton::validate`

- **Dónde:** `crates/automaton/src/lib.rs:176-186` (`validate`).
- **Estado:** reserva (número de `{` y `[` + 1) × (marco máximo + 1) palabras: hasta unos
  520 B por byte del payload con 4 096 nombres. Vale para pruebas, no para el servidor.
- **Arreglo propuesto:** tomar la profundidad de `Limits` (256 por defecto) y dar `limit`
  al pasarla; F4-B24 destina `validate` al servidor y al arnés F6.5.
- **Tarea:** F4.6 (y Fase 7).

### 14. F4.1a-CR-S4: pánicos y bucles defensivos en el constructor — RESUELTO (F4.1b)

- **Dónde (en F4.1a):** `crates/automaton/src/json.rs`, `fill_node` y `fill_object`
  (`expect("reservado")`, ya no existen) y `resolve_aliases` (hoy en `crates/automaton/src/conj.rs:66`, sin
  guarda ante un ciclo de solo `Ref`, que I14 excluye).
- **Arreglo propuesto:** devolver `BuildError::Internal` en lugar de pánico, y cortar el
  bucle de alias si da más de n pasos (defensa en profundidad, F4-B11).
- **Tarea:** F4.1b.
- **Resolución:** los `expect` desaparecen (los subautómatas se toman con `Option::take` y
  un hueco vacío no rellena nada); la resolución de alias pasa a `crates/automaton/src/conj.rs`
  (`resolve_aliases`), corta a los n pasos con `Internal`, y el DFS del cierre da `Internal`
  si encuentra un nodo en la pila. Pruebas unitarias `ciclo_de_alias_es_internal` y
  `ciclo_en_el_cierre_es_internal` (con una IR sin verificar).

### 15. F4.1a-CR-S5: el campo `pending` del resumen — RESUELTO (F4.1b; interfaz compartida)

- **Dónde:** `crates/automaton/src/model.rs:96` (`Summary::deferred`, antes `pending`) y
  `crates/automaton/src/export.rs:28`.
- **Estado:** significa «reglas con estrategia de respaldo» y se confunde con
  `BuildError::Pending` (que hace fallar la construcción).
- **Arreglo propuesto:** renombrarlo (`deferred` o `fallback`) en la próxima tarea que toque
  `export.rs`, y en todo caso antes de que la web de Bayron consuma el resumen (F4-U7,
  interfaz compartida); regenerar `tests/automata/` con `make doradas`.
- **Tarea:** la próxima que toque `export.rs` (F4.1b o F4.5).
- **Resolución:** `Summary::deferred` y la clave `"deferred"` del JSON; `tests/automata/
  objeto-cerrado.json` regenerado con `make doradas` (solo cambia esa clave). Cambio de
  F4-U7 (interfaz compartida, D42): se comunica a Bayron para su visto bueno (§22).

### 16. F4.1a-CR-S8: códigos fijos de `Op`, `Fault` y `Test`

- **Dónde:** `crates/runtime-rs/src/tables.rs:66-157` (`Fault`, `Op`, `Test`).
- **Arreglo propuesto:** `#[repr(u8)]` con discriminantes explícitos, para que el emisor C
  (F5.2) y las tablas `const` de F5.1 usen los mismos códigos.
- **Tarea:** F5.2 (o F5.1).

### 17. F4.1a-CR-S9: `Options` con campos públicos

- **Dónde:** `crates/runtime-rs/src/core.rs:33-35`.
- **Estado:** los literales `Options { mode }` de las pruebas dejarán de compilar al añadir
  los límites.
- **Arreglo propuesto:** `Options::new(mode)` o `Default` (con `..Default::default()`).
- **Tarea:** F4.6.

### 18. F4.1a-CR-S10: `integer` frente a `ir::num` y la línea base — RESUELTO el proptest (F4.1b); la divergencia sigue para la Fase 6

- **Dónde:** `crates/runtime-rs/src/core.rs` (`Number::is_integer`).
- **Arreglo propuesto:** un proptest de `Number::is_integer` frente a `ir::num` sobre
  lexemas de RFC 8259 (con exponentes largos y negativos). La divergencia conocida
  `1e-99999999999999999999` con `type: integer` (el DPDA da 1 violación; la línea base, que
  pasa por `f64`, da 0) va a `DIVERGENCIAS` de `crates/baseline/tests/corpus_json.rs` y a
  `docs/fase6/pendientes.md` §5 cuando el caso entre al corpus (T-4).
- **Tarea:** F4.1b (proptest) y Fase 6 (corpus y divergencia).
- **Resolución (F4.1b):** `entero_coincide_con_ir_num` (`crates/runtime-rs/tests/num.rs`):
  `runtime_rs::num::is_integer` (los registros de `Number`, ahora en `num.rs`) frente a un
  oráculo con `ir::num::compare` (x ≠ 0 de n cifras significativas es entero ⇔ |x| ≥ 10ⁿ⁻¹),
  con exponentes de hasta 40 cifras, con ceros a la izquierda y negativos.

### 19. F4.1a-CR-S11: cosméticos del DOT (nit) — RESUELTO el nombre (F4.1b); la agrupación sigue

- **Dónde:** `crates/runtime-rs/src/tables.rs:41` (`class::NAMES` llama «dígito» a
  `NUMBER_BYTE`, que es cualquier byte del lexema) y `crates/automaton/src/export.rs:72`
  (la agrupación de aristas por estado es O(k²)).
- **Arreglo propuesto:** renombrar a «byte num» (cambia el `.dot` de la dorada: `make docs`);
  la agrupación cuadrática es aceptable mientras el DOT sea solo para la documentación, y
  se cambia por un mapa si el DOT pasa al servidor.
- **Tarea:** la próxima que toque `tables.rs` o `export.rs`.
- **Resolución (F4.1b):** `class::NAMES` dice «byte num»; `docs/fase4/automata-objeto-cerrado.dot`
  y `.svg` regenerados con `make docs`. La agrupación cuadrática de `export.rs` sigue igual
  (aceptable para la documentación).

### 20. Para las dos personas (F4-U18 y F4-U6)

- Las tres precisiones del reporte (`ErrorKind::Internal`, el error de peso ∞ como primer
  error y el offset al comienzo del evento) están en §5; las filas de F4-U6 que faltan en
  `docs/00-alcance.md` §4, en §2. Se proponen juntas al cerrar F4.1a (D42).

### Abiertas por la tarea F4.1b (registros, escalares y conjunción)

### 21. El límite del lexema solo se aplica cuando hace falta (F4.1b-P1)

- **Estado:** el núcleo guarda los primeros 1 024 bytes del lexema (`LEXEME_MAX`,
  `crates/runtime-rs/src/core.rs:76`) y da `limit` solo si una comprobación que lo lee
  (rangos, `multipleOf`, números de `enum`/`const`) encuentra uno más largo; un número más
  largo sin esas comprobaciones (o solo con `integer`, que va por registros saturados) se
  acepta. `00-alcance.md` §4 dice «longitud máxima del lexema de un número: 1 024 bytes» sin
  más.
- **Arreglo propuesto:** decidir en F4.6, con los demás límites del ejecutor (`Limits`,
  F4-U18), si el límite se aplica a todo número (más estricto, uniforme con el resto de
  límites y con el oráculo) o solo cuando hace falta (lo de ahora, que acepta más), y
  documentarlo en el reporte.
- **Criterio de «exponentes de 10⁴ cifras» (ronda 1 de F4.1b):** se cumple en la rutina,
  que no tiene tope (`runtime_rs::num::is_multiple`, `crates/runtime-rs/tests/num.rs`,
  `tabla_de_multiple_of`). En el autómata, un **payload** con un exponente de 10⁴ cifras pasa
  de 1 024 bytes y da `limit` por diseño, con `multipleOf` igual que con `minimum`
  (`lexema_largo_da_limit_solo_si_hace_falta`, `crates/automaton/tests/escalares.rs`); un
  **divisor del esquema** con un exponente de 10⁴ cifras sí se decide exacto en el autómata
  (misma prueba).
- **Tarea:** F4.6.

### 22. Precisiones de límites para las dos personas (F4.1b-P2; interfaz compartida, D42)

- **Estado:** el código aplica, además de F4-U6:
  1. Divisor de `multipleOf`: parte coprima con 10 < 2⁶⁰ (F4-U6) **y** exponentes de 2 y de 5
     de sus cifras significativas a ≤ 63 y b ≤ 27 (y ≤ 38 cifras significativas); si no,
     `limit` con la ubicación (`crates/automaton/src/json.rs:859`, `divisor_limit`;
     `crates/runtime-rs/src/num.rs`, `Divisor::of`). Es conservador (rechaza, p. ej.,
     `multipleOf: 2⁶⁴`, que es exacto en teoría) y deja los tres restos en registros de 64
     bits.
  2. Presupuesto de trabajo de los cierres conjuntivos: 2²⁴ pasos (`MAX_CLOSURE_WORK`,
     `crates/automaton/src/conj.rs:26`), `limit` con la ubicación del conjunto que lo agota.
  3. El tope de 4 096 estados se aplica a cada subautómata producto (`BuildError::ProductLimit`,
     con el número alcanzado y si el marco es escalar), configurable con
     `BuildOptions::max_product_states`; es el de la fila «Estados en la determinización
     acotada» de §4, que F4-U16 ya extiende a todo producto.
  4. El renombrado `pending` → `deferred` del resumen de `schemac automaton` (F4-U7, §15).
  5. **Precisión de F4-U6 (ronda 2 de F4.1b):** el presupuesto de 2²⁰ entradas por esquema
     cubre las entradas de δ (|Q| · |Σ|) **más** las comprobaciones, las palabras de máscaras,
     las dependencias y los lexemas (`budget`, `crates/automaton/src/json.rs:783`), contados
     antes de reservar; si se pasa, `limit` con la ubicación. Sin tope nuevo. Rechaza, por
     ejemplo, un `allOf` de 2 000 `const` de cadena (≈ 4·10⁶ comprobaciones al cerrar la
     cadena); un `enum` de 2 000 cadenas sin producto sigue compilando.
- **Arreglo propuesto:** proponer a Bayron las filas 1-2 en `docs/00-alcance.md` §4 junto
  con las de §2 de este archivo, y confirmar 3 y 4. No lo cambia un agente (D42).
- **Tarea:** cierre de F4.1b (orquestador, con las dos personas).

### 23. Productos escalares por encima del tope (F4.1b-P3)

- **Estado:** un producto de cadena o número que pasa del tope da `ProductLimit` con
  `scalar: true`, como uno de objeto. Con las piezas de F4.1b no ocurre en la práctica (un
  producto crece con la suma de sus componentes); ocurrirá con los AFD de `pattern`.
- **Arreglo propuesto:** F4-U11: simulación paralela de las componentes en los marcos
  escalares (cada una con su AFD y su salida), en lugar del error; el caso de los 15 primos.
- **Tarea:** F4.4b/F4.5a.

### 24. Casos del corpus de las reglas de F4.1b (F4.1b-P4)

- **Estado:** el corpus JSON (`tests/cases`) solo tiene un caso `invalid` y uno `valid`
  que no son `non-dpda`; ninguno usa contadores, rangos, `multipleOf`, `enum`/`const` ni
  `allOf`. `casos_que_compilan_dan_lo_esperado` (`crates/automaton/tests/corpus.rs`) cumple el
  criterio 5 de F4.1b sobre lo que hay, y las pruebas de `escalares.rs` y `conjuncion.rs`
  cubren las reglas, pero no se comparan con la línea base.
- **Arreglo propuesto:** añadir casos `valid`/`invalid` con esas reglas (y uno con
  `1e-99999999999999999999` y `type: integer`, la divergencia conocida de §18) y declarar en
  `DIVERGENCIAS` de `crates/baseline/tests/corpus_json.rs` los que difieran (D47).
- **Tarea:** F4.6 (oráculo contra la línea base) o la Fase 6.

### 25. Lexemas de `enum` sin compartir (F4.1b-P5) — RESUELTO (ronda 2 de F4.1b)

- **Estado:** cada comprobación numérica guarda su propio lexema en `Tables::nums`/`lexemes`
  (`Builder::lexeme`, `crates/automaton/src/json.rs`), aunque se repita en otro conjunto; con
  productos que comparten un `enum` grande, las tablas crecen con el número de conjuntos que
  lo leen.
- **Arreglo propuesto:** memorizar los rangos de `nums` por (nodo, regla) si las tablas de un
  esquema real crecen; F5.1 los emite como `const`.
- **Tarea:** F4.3 o F5.1 (si se mide).
- **Resolución (ronda 2, I1 del code-reviewer):** era peor que «menor»: con muchos conjuntos
  que leen el mismo `enum`, `lexemes` pasaba de 4 GiB y `Lexeme::off` daba la vuelta
  (veredicto incorrecto sin error). Ahora cada `enum` numérico se ordena una vez por regla en
  el análisis (`Analysis::numbers`) y su rango en `nums` se guarda una vez
  (`Builder::enum_ranges`); cotas y divisores, una vez por regla (`rule_lexeme`,
  `divisor_ids`); los índices pasan por `u32::try_from` (`Builder::idx`, `limit` si no caben).
  Las comprobaciones de entrada se comparten por (conjunto, clase) y el resto multiplicativo
  cuenta en el presupuesto (§22, punto 5). Medidas en release (`schemac automaton`) de los
  esquemas del code-reviewer, antes → después: `pref.json` 700 MB / 1,0 s → 12 MB / 0,03 s;
  `strlits-3500-0.json` 308 MB / 0,55 s → `limit`, 8 MB / 0,01 s; `numenum-4000-20000.json`
  2,2 GB / 29 s → 27 MB / 1,2 s; `bigbound.json` 485 MB / 0,1 s → 9 MB / 0,02 s. Pruebas sin
  reloj en `crates/automaton/tests/sin_reservas.rs` (ronda 2).

### Observaciones abiertas de la revisión de la ronda 3 de F4.1b (code-reviewer, 18 nov)

### 26. F4.1b-CR3-S1: los productos de objetos hashean nombres enteros por conjunto (rendimiento)

- **Dónde:** `crates/automaton/src/json.rs:464-548` (`Analysis::set`, objeto: la unión con
  `idx.contains_key`, un `HashMap<&str, NodeId>` de declarados nuevo por componente y por
  conjunto en `:516`, `declared.get(n)`) y `crates/automaton/src/json.rs:1652-1667`
  (`fill_object`, `bits.get`).
- **Estado:** el presupuesto de los cierres cuenta nombres × componentes, pero cada paso
  cuesta O(ℓ) (la longitud del nombre): tiempo O(2²⁰ + n + W · ℓ), unos 5 s en el peor
  esquema de 16 MiB ([automata.md](automata.md) §7). La memoria no tiene el factor ℓ.
- **Arreglo propuesto:** guardar una vez por nodo el mapa de declarados y los ids de nombre
  de cada componente; mejor, internar los nombres a `u32` una vez (cada conjunto hashea
  enteros y desaparece el factor ℓ); como mínimo, descontar `Σ |nombre| × componentes` del
  presupuesto (`spend`).
- **Tarea:** F4.1b-CR-S6 (partir `Analysis::set` antes de `xml.rs`, en F4.1c) o F4.3.

### 27. F4.1b-CR3-S3: clase inesperada en `entry` (nit)

- **Dónde:** `crates/automaton/src/json.rs:1425-1428` (`position(..).unwrap_or(0)`).
- **Estado:** una clase fuera de `class::VALUE_STARTS` iría en silencio al bit de `{`. Es
  inalcanzable (`entry` solo se llama con esas clases).
- **Arreglo propuesto:** `self.internal(..)` en ese caso, por coherencia con el resto del
  constructor.
- **Tarea:** la próxima que toque `json.rs`.

### 28. F4.1b-CR3-S4: el contador de pruebas de `entry_fails` (nit)

- **Dónde:** `crates/automaton/src/json.rs:793-797` (el contador `#[cfg(test)]`
  `tests::LITERALES` dentro de `entry_fails`).
- **Arreglo propuesto:** si se parte el módulo, que el contador (y la prueba
  `enum_compartido_se_recorre_una_vez`) acompañen a la función.
- **Tarea:** F4.1b-CR-S6.

### 29. F4.1b-CR3-S2: memorias por (nodo, regla) frente a las salidas de F4.5a

- **Estado:** con F4-B20 la clave de un tipo de marco pasa a ser la lista de (`NodeId`,
  salida), así que el mismo nodo aparecerá en más conjuntos. Las memorias de F4.1b
  (máscaras de entrada, `enum` ordenados, conjuntos de cadenas, divisores y lexemas;
  `Analysis::{entry_masks, enums, strsets, divisors, lexeme_rules}` y
  `Builder::{lexeme_ids, divisor_ids, enum_ranges}`, `crates/automaton/src/json.rs`) deben
  seguir indexadas por (nodo, regla), sin depender de la salida.
- **Arreglo propuesto (criterio de F4.5a):** mantener esas memorias por (nodo, regla); contar
  en `extra` las comprobaciones por salida antes de reservar; una prueba sin reloj como
  `enum_compartido_se_recorre_una_vez` con un `enum` compartido por muchas ramas de `anyOf`.
- **Tarea:** F4.5a.

