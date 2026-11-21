# Pendientes de la Fase 4 (autómata de pila ponderado)

Observaciones que dejaron abiertas las tareas F4.1a (modelo formal de M, pesos, núcleo
`no_std` y esqueleto JSON), F4.1b (registros, escalares y conjunción por E₌), F4.1c (XML) y
F4.6 (ejecutor: límites, reporte y buena formación, [runtime.md](runtime.md));
teoría en
[automata.md](automata.md). Lo resuelto se marca y se conserva.
Las decisiones de diseño viven en el «Registro de decisiones» del plan; aquí solo se
remite a ellas.

## Vigilar más adelante

### 1. Claves no declaradas repetidas (F4.1a-P1) — RESUELTO (F4.6)

- **Resuelto en F4.6:** la capa de buena formación guarda el hash SipHash-1-3 de cada clave sin bit en la región de su objeto (1 024 por objeto, 4 096 entre todos) y da `syntax` en la comilla de cierre de la repetida ([runtime.md](runtime.md) §4; `clave_declarada_repetida_es_infinito`, `malformed-json-duplicate-key-01`, proptest `json_aleatorio`).

- **Gravedad:** Medium (un payload JSON mal formado, con una clave no declarada repetida, se acepta como válido hasta F4.6).
- **Estado:** M detecta con su máscara las claves **declaradas** repetidas (peso ∞, D2);
  una clave sin bit repetida (`{"x": 1, "x": 2}` con `x` fuera de `properties`,
  `required` y `dependentRequired`) se acepta.
- **Arreglo propuesto:** la capa de buena formación del ejecutor (F4-B7): tabla acotada
  por `max_claves_adicionales` (1 024 por objeto, reserva común de 4 096; F4-U16, F4-B22)
  en la memoria del llamador, con error en la repetida.
- **Dónde:** `crates/runtime-rs/src/core.rs` (`Core::step`, rama de `Trans::bit`).
- **Tarea:** F4.6.

### 2. Límites de F4-U6 en `00-alcance.md` §4 (F4.1a-P2; interfaz compartida, D42)

- **Gravedad:** Low (documentación del alcance; los límites ya se aplican).
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

- **Gravedad:** Low (tamaño de M, no corrección).
- **Estado:** el trie de los nombres de cada objeto es un `Dfa` completo con un estado por
  prefijo; dos nodos con las mismas reglas tienen subautómatas distintos.
- **Arreglo propuesto:** Hopcroft (F4-B15) sobre cada `Dfa` antes de incrustarlo, y
  medir el efecto en la dorada (que entonces cambia: regenerar con `make doradas` y
  `make docs`, y recontar [automata.md](automata.md) §8).
- **Tarea:** F4.3.

### 4b. Cada conjunto producto de cadenas copia el trie entero (F4.1b-P6; rechazo claro)

- **Gravedad:** Low (rechazo claro de esquemas razonables, sin defecto de memoria).
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

### 5. Forma del reporte frente a F4-U18 (F4.1a-P5; interfaz compartida) — RESUELTO en el código (F4.6); la forma de `sv_report_t` va a las dos personas

- **Resuelto en F4.6:** `Options { mode, limits }` con `Options::new`/`Default`, `Limits`, `Tables::state_words` y el reporte de [runtime.md](runtime.md) §1 (las tres precisiones de arriba se mantienen). Lo que cambia la interfaz compartida (F4-U18) está en §59.

- **Gravedad:** Low (forma del reporte, interfaz compartida).
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

### 6. Profundidad acotada solo por la memoria del llamador (F4.1a-P6) — RESUELTO (F4.6)

- **Resuelto en F4.6:** `Limits::max_depth` (256 por defecto) en el núcleo, independiente de la memoria; `automaton::validate` reserva `state_words(&limits)` ([runtime.md](runtime.md) §2-§3, `limites.rs::profundidad`).

- **Gravedad:** Low (siempre `limit` sin pánico; falta el tope de `Limits`).
- **Estado:** el núcleo apila mientras quepa en `mem`; si no cabe, error `limit`
  (`pila_llena_da_limit`). No hay todavía un tope `SV_MAX_DEPTH` independiente de la
  memoria, y `automaton::validate` reserva tantos marcos como `{` y `[` tiene el payload.
- **Arreglo propuesto:** el tope de profundidad (256 por defecto, `00-alcance.md` §4) en
  las opciones del núcleo y `sv_state_t` de tamaño fijo con él.
- **Tarea:** F4.6 (y F5.1).

### 7. `AnalyzedTable` (sigue abierta, `fase3/pendientes.md` §13)

- **Gravedad:** Low.
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

### 12. F4.1a-CR-S2: una clase fuera de rango lee la fila siguiente (seguridad) — RESUELTO (F4.6)

- **Resuelto en F4.6:** `Core::step` da `Internal` si la clase es ≥ `n_classes`, antes de calcular el índice.

- **Gravedad:** Low (solo con tablas incoherentes; veredicto erróneo, nunca lectura fuera de rango).
- **Dónde:** `crates/runtime-rs/src/core.rs:171` (clase de carácter) y
  `crates/runtime-rs/src/core.rs:238-240` (`Core::step`, índice `q · n_classes + a`).
- **Estado:** con tablas incoherentes, una clase `≥ n_classes − CHAR` (de `ascii` o
  `ranges`) cae en la fila del estado siguiente en lugar de dar `Fault::Internal`. Con las
  tablas que construye `automaton` no ocurre, pero el núcleo no debe confiar en ellas.
- **Arreglo propuesto:** `if a >= t.n_classes { Internal }` en `step`, con una prueba de
  tablas corruptas.
- **Tarea:** F4.6 (revisión de seguridad, D35).

### 13. F4.1a-CR-S3: la pila que reserva `automaton::validate` — RESUELTO (F4.6)

- **Resuelto en F4.6:** `automaton::validate` reserva `Tables::state_words(&Limits)` palabras, de los límites y no del payload (`validate_with` para otros límites); ver también §45.

- **Gravedad:** **Medium** (ronda 3, F4D-M3); **High** en cuanto F6.5 o F7 cableen `validate` con payloads de usuario.
- **Dónde:** `crates/automaton/src/lib.rs:187-213` (`validate`: `vec![0u64; a.stack_words(depth)]`
  en `:192` y `:204`).
- **Estado:** reserva (número de `{` y `[` + 1) × (marco máximo + 1) palabras: hasta unos
  520 B por byte del payload con 4 096 nombres. Vale para pruebas, no para el servidor.
  **Escenario de la ronda 3 de F4.1c (F4D-M3, security-auditor):** un XSD con 400 hijos
  `minOccurs="2" maxOccurs="3"` (`maxFrameBytes` 3 208) y un payload de 1 MB de `<` piden
  3,2 GB de una vez; con `ulimit -v 3000000`, el proceso **aborta**
  (`memory allocation of 3208003208 bytes failed`) en lugar de dar `limit`; con 16 MiB de
  payload serían ≈ 51 GB. El lexer rechazaría el payload en el segundo `<`, pero la reserva
  ocurre antes de leerlo.
- **Arreglo propuesto:** tomar la profundidad de `Limits` (256 por defecto) y dar `limit`
  al pasarla, **antes** del `vec!`; F4-B24 destina `validate` al servidor y al arnés F6.5.
  Prueba con ese payload de `<`.
- **Tarea:** F4.6, **antes** de que F6.5 o F7 cableen `validate` (y Fase 7).

### 14. F4.1a-CR-S4: pánicos y bucles defensivos en el constructor — RESUELTO (F4.1b)

- **Dónde (en F4.1a):** `crates/automaton/src/json.rs`, `fill_node` y `fill_object`
  (`expect("reservado")`, ya no existen) y `resolve_aliases` (hoy en `crates/automaton/src/conj.rs:67`, sin
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

- **Gravedad:** Low.
- **Dónde:** `crates/runtime-rs/src/tables.rs:66-157` (`Fault`, `Op`, `Test`).
- **Arreglo propuesto:** `#[repr(u8)]` con discriminantes explícitos, para que el emisor C
  (F5.2) y las tablas `const` de F5.1 usen los mismos códigos.
- **Tarea:** F5.2 (o F5.1).

### 17. F4.1a-CR-S9: `Options` con campos públicos — RESUELTO (F4.6)

- **Resuelto en F4.6:** `Options::new(mode)` y `Options::default()`; las pruebas ya no usan literales.

- **Gravedad:** Low (nit).
- **Dónde:** `crates/runtime-rs/src/core.rs:33-35`.
- **Estado:** los literales `Options { mode }` de las pruebas dejarán de compilar al añadir
  los límites.
- **Arreglo propuesto:** `Options::new(mode)` o `Default` (con `..Default::default()`).
- **Tarea:** F4.6.

### 18. F4.1a-CR-S10: `integer` frente a `ir::num` y la línea base — RESUELTO el proptest (F4.1b); la divergencia sigue para la Fase 6

- **Gravedad:** Low (divergencia documentada con la línea base).
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

- **Gravedad:** Low (nit).
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

- **Gravedad:** Low (proceso, D42).

- Las tres precisiones del reporte (`ErrorKind::Internal`, el error de peso ∞ como primer
  error y el offset al comienzo del evento) están en §5; las filas de F4-U6 que faltan en
  `docs/00-alcance.md` §4, en §2. Se proponen juntas al cerrar F4.1a (D42).

### Abiertas por la tarea F4.1b (registros, escalares y conjunción)

### 21. El límite del lexema solo se aplica cuando hace falta (F4.1b-P1) — DECIDIDO (F4.6, D59)

- **Decidido en F4.6:** se mantiene «solo cuando una comprobación lo lee»: un número largo sin rangos, `multipleOf` ni `enum` numérico no necesita guardarse (los registros de `integer` son saturados) y aplicarlo a todo número rechazaría payloads que el validador puede decidir sin el lexema. Documentado en [runtime.md](runtime.md) §3 (`limites.rs::lexema`, `extreme-big-number-01`).

- **Gravedad:** Low.
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

- **Gravedad:** Low (proceso, D42: falta llevar las filas a `00-alcance.md` §4).
- **Estado:** el código aplica, además de F4-U6:
  1. Divisor de `multipleOf`: parte coprima con 10 < 2⁶⁰ (F4-U6) **y** exponentes de 2 y de 5
     de sus cifras significativas a ≤ 63 y b ≤ 27 (y ≤ 38 cifras significativas); si no,
     `limit` con la ubicación (`crates/automaton/src/json.rs:859`, `divisor_limit`;
     `crates/runtime-rs/src/num.rs`, `Divisor::of`). Es conservador (rechaza, p. ej.,
     `multipleOf: 2⁶⁴`, que es exacto en teoría) y deja los tres restos en registros de 64
     bits.
  2. Presupuesto de trabajo de los cierres conjuntivos: 2²⁴ pasos (`MAX_CLOSURE_WORK`,
     `crates/automaton/src/conj.rs:27`), `limit` con la ubicación del conjunto que lo agota.
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

- **Gravedad:** Low (funcionalidad de F4.4b/F4.5a; hoy, `ProductLimit` claro).
- **Estado:** un producto de cadena o número que pasa del tope da `ProductLimit` con
  `scalar: true`, como uno de objeto. Con las piezas de F4.1b no ocurre en la práctica (un
  producto crece con la suma de sus componentes); ocurrirá con los AFD de `pattern`.
- **Arreglo propuesto:** F4-U11: simulación paralela de las componentes en los marcos
  escalares (cada una con su AFD y su salida), en lugar del error; el caso de los 15 primos.
- **Tarea:** F4.4b/F4.5a.

### 24. Casos del corpus de las reglas de F4.1b (F4.1b-P4)

- **Gravedad:** Low (cobertura).
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

- **Gravedad:** Low (rendimiento acotado).
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
- **F4.1c:** `Analysis::set` ya está partido (§32); la unión de objetos vive ahora en
  `Analysis::object` (`crates/automaton/src/json.rs:437`) con el mismo hasheo por nombre, que
  **sigue abierto** (internar los nombres a `u32`). **Tarea:** F4.3.

### 27. F4.1b-CR3-S3: clase inesperada en `entry` (nit)

- **RESUELTO (F4.1c):** `Builder::entry` (`crates/automaton/src/json.rs:1132`) da
  `Internal` con una clase que no empieza un valor.
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
- **RESUELTO (F4.1c):** el reparto de F4.1b-CR-S6 deja `entry_fails`, su contador
  `tests::LITERALES` y la prueba en `crates/automaton/src/json.rs` (lo común que pasa a
  `emit.rs` no recorre literales).
- **Tarea:** F4.1b-CR-S6.

### 29. F4.1b-CR3-S2: memorias por (nodo, regla) frente a las salidas de F4.5a

- **Gravedad:** Low (criterio de F4.5a).
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

### Heredadas de la revisión de F4.1b (añadidas en F4.1c)

### 30. F4.1b-CR-S5: un solo contador por marco — RESUELTO (F4.1c)

- **Texto de la tarea:** el marco admite hoy un solo contador (`Trans::count: bool`,
  disposición `[máscara][contador 0 o 1]`, `tables.rs:109-111`, `194-197`, `core.rs:398-416`);
  el Glushkov con contadores de 5.7-P1 necesita k contadores por elemento y su reinicio al
  volver a entrar: índice de contador en `Trans` y en `Check` más una operación de reinicio,
  documentado en `automata.md` §3.1.
- **Resolución:** `Trans::{counts, n_counts}` sobre `Tables::counter_ops`
  (`CounterOp { counter, op: Add | Restart }`, `crates/runtime-rs/src/tables.rs:139`, `:275`),
  `Check::counter`, `Core::counter(k)`/`Core::counters` (índice fuera del marco: `internal`),
  orden bit → comprobaciones → marca → contadores → pila; [automata.md](automata.md) §3.1
  (disposición `[máscara][k contadores]`, `CounterOp` con `Add` y `Restart`; corregida en la
  ronda 2 de F4.1c, I3: la ronda 0 la había dejado con el contador 0 o 1 y `Trans::count`).
  Pruebas: `dos_contadores_con_reinicio`, `opcional_con_contador`
  (`crates/automaton/tests/glushkov.rs`); JSON igual (doradas sin cambios).

### 31. F4.1b-CR2-S3: presupuesto común JSON/XML, sin recorridos repetidos por conjunto — RESUELTO (F4.1c, ronda 3)

- **Texto de la tarea:** el presupuesto de transiciones de F4.1c usa la misma cuenta que la
  precisión de F4-U6 de F4.1b (§22.5): las transiciones y guardas del Glushkov entran en
  `extra` y se comprueban antes de reservar, para que la regla sea única en JSON y XML; y
  ningún recorrido por literal, cota o nombre se repite por conjunto (memo por (nodo, regla),
  como el arreglo de F4.1b ronda 3).
- **Resolución:** `emit::budget`/`delta_limit` comunes, `BuildOptions::max_delta`; el análisis
  XML cuenta |Q| exacto y, **desde la ronda 2 (C1 de la revisión)**, en `extra` lo que de verdad
  se emite: por marco las comprobaciones de la etiqueta y del contenido (el AFD del contenido
  simple se repite por marco), por arista 3 · guardas + 2 · requeridos + las comprobaciones del
  hijo vacío, y por texto de atributo estados × comprobaciones de un cierre
  (`Analysis::{frame_cost, entry_cost, empty_cost}`, `crates/automaton/src/xml.rs:450`), antes
  de reservar, con comprobación parcial por marco; las comprobaciones de un cierre de texto se
  calculan una vez por (texto, salida). Los pares del Glushkov se cuentan con una pasada
  aritmética y los bloques solo se reservan tras el tope (ronda 2, I1). Los intervalos IEEE se
  cargan al presupuesto antes de calcularse (ronda 2, F4C-S1). En la ronda 0 la cuenta era
  por tipo de elemento y no por lo emitido (2,4 GB con `b3.xsd`), así que esta entrada no era
  cierta hasta la ronda 2. Modelos memorizados por nodo, textos por (cierre, `fixed`),
  `enum` por (nodo, regla), lexemas e intervalos una vez. [automata.md](automata.md) §12, §15.
  Pruebas: `eleccion_ancha_da_limit` (`crates/automaton/tests/glushkov.rs`),
  `glushkov_ancho_da_limit_antes_de_reservar`,
  `comprobaciones_por_arista_dan_limit_antes_de_reservar`,
  `enumeraciones_flotantes_dan_limit_antes_de_calcular` y
  `secuencias_anchas_sin_bloques_cuadraticos` (`crates/automaton/tests/sin_reservas.rs`).
- **Corrección de la ronda 3 (C2 de la revisión tras la ronda 2):** tras la ronda 2 esta entrada
  seguía sin ser del todo cierta. El presupuesto se comprobaba **una vez por marco**, y antes de
  esa comprobación `elem()` analizaba todos los atributos y el modelo, `frame_cost` todos los
  hijos, `run` todos los globales, y cada texto unía y ordenaba las palabras de **todos** los
  `enum` de su cierre (uno por cierre distinto: O(N² · W)); el tope de pares era por modelo y
  no acumulado; los umbrales IEEE de las cotas se calculaban por texto sin cargarse, y el trie
  del documento no se contaba. Desde la ronda 3 la granularidad es la de JSON (comprobación
  tras cada texto, atributo, modelo, tipo, global, hijo y marco), las palabras se memorizan por
  lista de reglas `enum`, los umbrales por (nodo, regla), los pares se acumulan entre modelos y
  cada sitio del análisis que trabaja en proporción al esquema está inventariado con lo que lo
  acota ([automata.md](automata.md) §15). La propiedad «lo contado cubre lo emitido» se
  comprueba en todo `parse-ok` y en los esquemas de `tests/{xml,glushkov}.rs`
  (`tests/comun/presupuesto.rs`). Detalle y medidas en §51.

### 32. F4.1b-CR-S6: extraer las utilidades de `json.rs` y partir `Analysis::set` — RESUELTO (F4.1c)

- **Texto de la tarea:** antes de escribir `xml.rs`, extraer las utilidades genéricas de las
  tablas de `json.rs` (`Builder::{group, state, frame, trans, rule, check, checks, lexeme,
  trie_states}`, `trie_size`, `label`) a un módulo común, y partir `Analysis::set` (~235
  líneas) en objeto, arreglo, número y cadena, para no duplicarlas.
- **Resolución:** `crates/automaton/src/emit.rs` (`Emitter`, `crates/automaton/src/emit.rs:139`,
  más `budget`, `delta_limit`, `table_index`, `trie_size`, `label`, `clip`, `plain`);
  `Analysis::{set, product, object, array, number, string}` en `json.rs:383-700`. Sin cambio de
  comportamiento: las pruebas JSON, la dorada `tests/automata/objeto-cerrado.json` y
  `docs/fase4/automata-objeto-cerrado.dot` no cambian.

### Abiertas por la tarea F4.1c (XML)

### 33. Buena formación XML que queda fuera de M (F4.1c-P1) — RESUELTO (F4.6)

- **Resuelto en F4.6:** el QName de apertura completo se compara por hash con el de cierre en **todo** elemento con contenido (también los saltados); `:` al principio, al final o repetido en nombres de elemento y de atributo, y los atributos sin bit repetidos (`xmlns`, `xmlns:p`, no declarados, de un elemento saltado) son `syntax` ([runtime.md](runtime.md) §4; `nombres_de_cierre_y_prefijos`). Queda §34.

- **Gravedad:** **Medium** (F4C-S3: XML mal formado aceptado con peso 0; diferencial con libxml2).
- **Estado:** M comprueba los atributos declarados repetidos (bit, ∞) y, de los elementos con
  marco propio, **solo la parte local** del nombre de cierre (I2/F4C-S3 de la revisión de la
  ronda 1): se aceptan con peso 0 `<p:a></q:a>`, `<a></p:a>`, `<a:b:pedido>…</c:pedido>` y
  `<:pedido>…</:pedido>` (el QName de cierre debe ser idéntico al de apertura, y un `:` inicial
  o final no forma un QName), y también `xmlns` o `xmlns:p` repetidos en la misma etiqueta. No
  comprueba nada del nombre de cierre de un elemento saltado (cualquier nombre cierra) ni los
  atributos repetidos sin bit (no declarados, de un elemento saltado, `xmlns*`); estos últimos
  no son explotables (ya suman ≥ 1).
- **Dónde:** `crates/automaton/src/xml.rs:2138` (`self.trie(&fs.end, true)`: `:` vuelve al
  inicio del trie de cierre), `:1393` (`Builder::trie`), el grupo «elemento saltado»
  (`Builder::fill_fixed`) y el atributo desconocido y `xmlns` en `fill_frame`;
  [automata.md](automata.md) §11.
- **Arreglo propuesto:** la capa de buena formación de F4-B7: el QName completo de apertura por
  hash en el marco de **todo** elemento (también los saltados) y comparación exacta en el
  cierre; rechazar `xmlns`/`xmlns:p` repetidos; rechazar `:` inicial o final (y más de un `:`)
  en nombres de elemento y de atributo; tabla de atributos sin bit por etiqueta (F4-U16).
- **Tarea:** F4.6.

### 34. Espacios de nombres del payload (F4.1c-P2; para las dos personas, D42)

- **Gravedad:** **Medium** (se acepta más que `00-alcance.md` §1.2).
- **Estado:** los elementos se comparan por nombre local (`:` vuelve al inicio del trie) y el
  prefijo no se resuelve contra su URI, así que hoy se acepta **más** de lo que dice
  `docs/00-alcance.md` §1.2 («los elementos se comparan por nombre local dentro de ese espacio
  de nombres»; S4 de la revisión): un elemento de cualquier espacio de nombres
  (`<x:pedido xmlns:x="urn:otro">` se valida como `pedido`; con `elementFormDefault="qualified"`
  en el esquema, también un `pedido` sin prefijo y sin `xmlns`, que en XSD no estaría en el
  `targetNamespace`). En los atributos: los que llevan prefijo nunca son declarados, así que
  `xsi:schemaLocation`, `xsi:noNamespaceSchemaLocation` y `xml:lang` **suman 1** (XSD los
  admite siempre los de `xsi:`); `xmlns` y `xmlns:…` se ignoran.
- **Arreglo propuesto (decisión de las dos personas, D42; un agente no cambia el alcance):**
  (a) escribir la desviación en `docs/00-alcance.md` §1.2 (nombre local sin comprobar el URI;
  `xsi:*`/`xml:*` como atributos no declarados), o (b) una tabla de prefijos acotada en el
  ejecutor (F4.6: `xmlns:p` en la pila de marcos, el URI del elemento comparado con el
  `targetNamespace`, y `xsi:` reconocido). Ante la duda, rechazar.
- **Tarea:** cierre de F4.1c (orquestador) o F4.6.

### 35. Normalización de fines de línea y de valores de atributo (F4.1c-P3, S3/F4C-S5) — RESUELTO (F4.6)

- **Resuelto en F4.6:** `Core::xml_text_char` normaliza `\r\n`/`\r` → `\n` en texto y valores, y `\t \n` → espacio en los valores; las referencias de carácter no se normalizan ([runtime.md](runtime.md) §4; `limites.rs::fines_de_linea_xml`). Falta contrastarlo con libxml2 en Docker (`make test-baseline-xml`).

- **Gravedad:** Low (F4C-S5: solo `xs:string` con longitudes, `enum` o `fixed`).
- **Estado:** ni el lexer XML (`crates/lexer/src/xml.rs:22`, simplificación de la Fase 1) ni el
  núcleo (`Core::text_char`, `crates/runtime-rs/src/core.rs:256`) normalizan: (1) los fines de
  línea `\r\n` y `\r` sueltos a `\n` (XML 1.0 §2.11), en el contenido de texto **y** en los
  valores; (2) `\t \n \r` de un valor de atributo a espacios (XML 1.0 §3.3.3). Afecta a
  `length`, `minLength`, `maxLength`, `enum` y `fixed` de `xs:string` (*preserve*):
  `<s>a\r\n</s>` cuenta 3 puntos de código (conforme, 2) y da un falso rechazo con
  `maxLength="2"` o un falso aceptado con `minLength`/`length`; `a="a\tb"` frente a un `enum`
  `"a b"`. En los demás tipos el *collapse* lo hace igual.
- **Arreglo propuesto:** normalizar en `Core::text_char` (un registro de un bit «el anterior fue
  `\r`» para `\r\n`; en un valor de atributo, `\t \n \r` → espacio, que además ve el AFD) o
  en el lexer, y probarlo contra libxml2 (F6.7b).
- **Tarea:** F4.6 (o F6.7b).

### 36. Precisiones de F4-U3 en XML (F4.1c-P4; interfaz compartida, D42)

- **Gravedad:** Low (precisión de interfaz compartida).
- **Estado** ([automata.md](automata.md) §11, §13, §14): un hijo inesperado suma 1 y se salta
  (F4-U3); además, un hijo repetido de `xs:all` suma 1 y **no** se salta (se valida con su
  tipo); un tramo de texto no permitido (contenido vacío o de solo elementos) suma 1 una vez;
  un hijo dentro de un contenido simple suma 1 y el resto del texto no se comprueba; un texto
  fuera de su espacio léxico cuenta **una** violación aunque la cadena de restricciones repita
  `Lexical`, y sus facetas no se evalúan; las violaciones de un elemento sin `Content`/
  `Attributes` se atribuyen a esas reglas implícitas (`Emitter::rule_of`). También (S2 de la
  revisión): un hijo que pasa del `maxOccurs` de su partícula (la guarda de máximo del Glushkov
  falla, §12 de [automata.md](automata.md)) suma 1 pero **no** se salta: la transición se hace
  igual y el hijo se valida con su tipo, así que su subárbol puede sumar más violaciones, y el
  recuento difiere del de libxml2 (que lo trata como inesperado) en F6.7b.
- **Arreglo propuesto:** proponerlas a las dos personas junto con el recuento de libxml2
  (F6.7b) y declarar en `DIVERGENCIAS` las que difieran.
- **Tarea:** cierre de F4.1c (orquestador) y F6.7b.

### 37. Presupuesto de pasos de los Glushkov (F4.1c-P5; para las dos personas)

- **Gravedad:** Low (precisión de interfaz compartida).
- **Estado:** la construcción de los autómatas de Glushkov (tamaños de first/last, caminos y
  pares) tiene un presupuesto de pasos con el mismo tope que los cierres (2²⁴,
  `MAX_CLOSURE_WORK`, §22.2), con su propio mensaje («pasos de la construcción XML (autómatas
  de Glushkov e intervalos IEEE)»), `crates/automaton/src/glushkov.rs` (`Work`). Desde la
  ronda 2 cuenta además cada paso de la pasada aritmética de los pares y cada bloque (I1), y
  2 048 pasos por intervalo de redondeo IEEE de un `enum` o un `fixed` de `xs:float`/
  `xs:double` (`ROUNDING_STEPS`, `crates/automaton/src/xml.rs:248`, F4C-S1): unas 8 000
  enumeraciones flotantes como mucho por esquema. **Ronda 3:** también 2 048 pasos por cada
  **cota** de `xs:float`/`xs:double`, una vez por (nodo, regla) (I4), y un paso por palabra de
  los `enum` de cadena que se unen para el trie de un texto (F4D-S1); el mensaje pasa a «pasos
  de la construcción XML (autómatas de Glushkov, intervalos IEEE y literales de los textos)».
  Además, el presupuesto de los **cierres** (`MAX_CLOSURE_WORK`, §22.2) pesa `FACET_WEIGHT` = 4
  pasos por regla de cada componente del cierre de un texto (F4D-M1). **Ronda 3, precisión de
  F4-U6 (F4D-M2):** el tope de los pares de follow, el presupuesto de δ entre `CHAR + 1` (cada
  par da al menos un estado de trie de |Σ| > `CHAR` entradas), se aplica a la **suma** de los
  pares de todos los modelos del esquema, no a cada modelo. **Ronda extra (I6 = F4E-M1):** el
  presupuesto de pasos cuenta también Σₓ Σ_{y ∈ follow(x)} |nombre(y)| bytes (y 3 × Σ |nombre|
  en `xs:all`), antes de la UPA y de los tries por estado; el mensaje pasa a «pasos de la
  construcción XML (autómatas de Glushkov con sus nombres, intervalos IEEE y literales de los
  textos)».
- **Arreglo propuesto:** añadirlo a la fila de `MAX_CLOSURE_WORK` que se propone para
  `docs/00-alcance.md` §4 (§22).
- **Tarea:** cierre de F4.1c (orquestador, con las dos personas).

### 38. Formato de las tablas para los emisores (F4.1c-P6)

- **Gravedad:** Low.
- **Estado:** F4.1c cambia el formato de `runtime_rs::Tables` (`counter_ops`, `intervals`,
  `Trans::{mark, counts, n_counts}`, `Check::counter`, `Test::{Unset, NumIn}`, clases
  `xclass` y `PI_BODY`) y añade `XmlValidator` y `Core::xml_event` (`pi_target`, registros del
  texto).
- **Arreglo propuesto:** F5.1 emite las tablas nuevas como `const`; F5.2 porta `xml_event`,
  `text_char` y la bandera `pi_target`; los códigos de `Op`, `Test` y `CountOp` con
  discriminantes fijos (F4.1a-CR-S8, §16).
- **Tarea:** F5.1, F5.2.

### 39. Tamaño de M en XML (F4.1c-P7)

- **Gravedad:** Low (tamaño de M).
- **Estado:** el pedido (`valid-xml-pedido-01`) da 257 estados y |Σ| = 61: la mayor parte son las
  partes fijas (documento, elemento saltado, instrucciones, AFD de la declaración) y los AFD de
  los tipos numéricos; un tipo usado con varios nombres repite sus estados por nombre (un marco
  por (tipo, nombre)), y el contenido simple repite el AFD del texto en cada marco. La partición
  de caracteres es de un punto de código por clase (dígitos incluidos).
- **Lo que esto rechaza (S12 de la revisión tras la ronda 3):** los `limit` en esquemas
  legítimos vienen de aquí, no de que la cuenta del presupuesto sea conservadora (lo contado es
  ≈ 1,03 × lo emitido): la δ es densa con |Σ| = 61 (una clase por punto de código) y el AFD del
  texto de un contenido simple se copia en **cada marco** sin minimizar. `re1` (200 elementos con
  un tipo de 10 literales) emite 651 771 entradas y compila; `re2` (200 elementos de 40 tipos con
  50 literales cada uno) emitiría 1 131 091 > 2²⁰ y da `limit` (1 050 438 en la comprobación
  parcial). Es aceptable para el alcance (F4-U6 aprobado, `limit` claro con la ubicación).
- **Arreglo propuesto:** Hopcroft sobre los AFD de texto y los tries (F4.3); agrupar en una clase
  los caracteres equivalentes para todos los AFD (partición inducida, F4-B3); el texto de un
  contenido simple en un **grupo global** con `Enter`/`Leave`, como los de atributo, en vez de
  copiarlo por marco; medirlo en el informe («tamaño del autómata»). **Criterio de F4.3:** `re2`
  (`scratchpad/rev4/re2.xsd` de la revisión: 200 elementos de 40 tipos × 50 literales) compila.
- **Tarea:** F4.3.

### 40. Corpus XML (F4.1c-P8)

- **Gravedad:** Low (cobertura).
- **Estado:** el corpus solo tiene dos casos XML (`valid-xml-pedido-01`,
  `malformed-xml-billion-laughs-01`, `casos_xml_dan_lo_esperado`); las reglas de F4.1c se
  prueban en `crates/automaton/tests/{xml,glushkov}.rs`, sin oráculo externo.
- **Arreglo propuesto:** los `invalid-xml-*` (rango, hijo requerido) y `malformed-xml-unclosed-01`
  de F4.6, y la comparación con libxml2 (F6.7b).
- **Tarea:** F4.6, F6.7b.

### 41. La comprobación de determinismo fuerte frente a la del frontal (F4.1c-P9)

- **Gravedad:** Low (`Internal` sin pánico; no visto).
- **Estado:** `glushkov::build` da `Internal` si dos derivaciones de un par (x, y) dan
  actualizaciones de contador distintas; el frontal (5.7) rechaza las vueltas ambiguas con
  contador con su propia regla. Si alguna lectura difiriera, un esquema aceptado por el
  frontal daría `Internal` (no se ha visto en `tests/schemas/xsd/parse-ok` ni en las pruebas).
- **Arreglo propuesto:** un proptest de modelos aleatorios pequeños: todo modelo que pasa
  `xsd::analyze` construye M sin `Internal`, y M acepta lo mismo que un oráculo por expansión
  (con contadores pequeños).
- **Tarea:** F4.3 o F4.6.

### 42. D7 en XSD solo con IR hecha a mano (F4.1c-P10)

- **Gravedad:** Info (nota para el informe).
- **Estado:** el criterio «10⁴ secuencias anidadas en un hilo de 256 KiB» se prueba con una IR
  hecha a mano (`d7_secuencias_anidadas_y_payload_profundo`): el lector XSD limita el
  anidamiento a 256 (D7) y D26 las partículas a 4 096, así que ningún XSD llega a ese modelo.
- **Tarea:** ninguna (nota para el informe).

### Revisión de F4.1c tras la ronda 1 (code-reviewer y security-auditor, 19 nov)

### 43. C1, F4C-S1, I1/F4C-S2 y S6 — RESUELTOS (ronda 2 de F4.1c; C1 y F4C-S1 completados en la ronda 3)

- **Precisión de la ronda 3:** la revisión tras la ronda 2 encontró que C1 estaba resuelto solo
  en la ruta reportada (la comprobación seguía siendo una por marco: C2, Critical) y F4C-S1 solo
  para `enum` y `fixed`, no para las cotas (I4), y que el tope de pares de I1 era por modelo
  (F4D-M2). Las tres cosas se cierran en la ronda 3 (§51).

- **C1 (Critical):** el presupuesto de F4-U6 no contaba las comprobaciones emitidas por arista y
  por marco (`b3.xsd`: 2,4 GB). Ahora `Analysis::frame_cost`/`entry_cost`/`empty_cost`
  (`crates/automaton/src/xml.rs:450`) las cuentan antes de reservar y `Builder::text_checks`
  se memoriza por (texto, salida). Prueba: `comprobaciones_por_arista_dan_limit_antes_de_reservar`.
- **F4C-S1 (High):** los intervalos de redondeo de `enum`/`fixed` flotantes se calculaban antes
  de contar nada. Ahora `Analysis::floats` suma a `extra` y comprueba el presupuesto, y carga
  `ROUNDING_STEPS` por valor al de pasos, **antes** del bucle (`xml.rs:1039`). Prueba:
  `enumeraciones_flotantes_dan_limit_antes_de_calcular`.
- **I1 / F4C-S2:** `glushkov::build` reservaba los bloques de pares antes del tope (251 MB con
  `a.xsd` y `a0.xsd`). Ahora una pasada aritmética (`blocks_of`,
  `crates/automaton/src/glushkov.rs:222`) cuenta pasos, bloques y pares, descuenta el trabajo y
  comprueba el tope antes de reservar; salta los hijos sin first/last. Prueba:
  `secuencias_anchas_sin_bloques_cuadraticos`.
- **S6 (nit):** `export.rs:104` usa `checked_add` + `get` para la lista de operaciones de
  contador.

### 44. F4C-S4: clase fuera de rango en `Core::step` (amplía §12) — RESUELTO (F4.6)

- **Resuelto en F4.6:** `step` da `Internal` con una clase ≥ `n_classes` y `Core::new` se detiene con `Internal` si `n_classes` ≤ `class::CHAR`.

- **Gravedad:** Low (F4C-S4).
- **Dónde:** `crates/runtime-rs/src/core.rs:317` (`state * n_classes + a` sin `a < n_classes`).
- **Estado:** con tablas incoherentes (`ascii[c]` ≥ `n_classes − 14`, o `n_classes` < 11, por
  `PI_BODY` = 10) una clase salta a la fila de otro estado: veredicto erróneo, nunca pánico ni
  lectura fuera de rango.
- **Arreglo propuesto:** `if a >= t.n_classes { fail(Internal) }` en `step` y validar en
  `Core::new` que `n_classes ≥ class::CHAR + 1`; prueba con tablas corruptas.
- **Tarea:** F4.6.

### 45. F4C-S6: `automaton::validate` reserva ∝ `<` × marco máximo (amplía §13) — RESUELTO (F4.6)

- **Resuelto en F4.6:** la memoria sale de `Limits` (256 marcos), no del número de `<`: 1 MB de `<` sobre un XSD de marco grande pide 256 × (marco + 2) palabras y da `limit` en el 257.º (`limites.rs::profundidad`).

- **Gravedad:** **Medium** (F4C-S6 y F4D-M3, ronda 3); **High** en cuanto F6.5 o F7 cableen `validate`.
- **Dónde:** `crates/automaton/src/lib.rs:203-204` (rama `Xml`: `depth` = número de `<` + 1,
  `stack_words(depth)`).
- **Estado:** la memoria crece con los `<` del payload por el marco máximo del esquema (un
  `xs:all` de 4 000 hijos ≈ 65 palabras por marco: 1 MB de `<` son cientos de MB); cada `push`
  pone a cero el marco entero. F4D-M3 (ronda 3): con 400 hijos con contador (`maxFrameBytes`
  3 208) y 1 MB de `<`, la reserva de 3,2 GB **aborta el proceso** bajo un límite de memoria
  (escenario completo en §13).
- **Arreglo propuesto:** la profundidad de `Limits` (256 por defecto) y `limit` al pasarla, antes
  del `vec!` y antes de que el arnés F6.5 o el servidor usen `validate`.
- **Tarea:** F4.6, antes de F6.5/F7 (y Fase 7).

### 46. F4C-S7: `words + 1` en 32 bits en `Core::push` — RESUELTO (F4.6)

- **Resuelto en F4.6:** `push` usa `checked_add(FRAME_OVERHEAD)`; F5.2 lo porta igual.

- **Gravedad:** Low (F4C-S7; solo en 32 bits).
- **Dónde:** `crates/runtime-rs/src/core.rs:583` (`checked_add(words + 1)`).
- **Estado:** con `Frame.words` = `u32::MAX` y `usize` de 32 bits (wasm32), `words + 1` desborda
  antes del `checked_add`.
- **Arreglo propuesto:** `words.checked_add(1)` dentro de la cadena `ok_or(Limit)`.
- **Tarea:** F4.6 (y F5.2, que porta el núcleo a C).

### 47. S1: la posición del Glushkov en el control cuesta Σₓ |trie(follow(x))| · |Σ| (precisión de F4-B13)

- **Gravedad:** Low (S1; tamaño de M, `limit` claro).
- **Dónde:** `crates/automaton/src/xml.rs` (`Builder::reserve`, un trie de nombres por estado
  entre hijos); [automata.md](automata.md) §9 (punto 26) y §12.
- **Estado:** un `xs:choice maxOccurs="unbounded"` de 100 hijos `elementoNNN` da 15 052
  estados; con 150, `limit` (≈ 1,7·10⁶ > 2²⁰). Es la precisión de F4-B13 anotada en §9.
- **Arreglo propuesto:** compartir el trie entre los estados entre hijos con la misma lista de
  aristas (nombres y destinos), que en un `xs:choice*` son todos; llevar el límite al informe.
- **Tarea:** F4.3 (con §39).

### 48. S5: partir `Builder::fill_frame`

- **Gravedad:** Low (nit).
- **Dónde:** `crates/automaton/src/xml.rs:1938` (`fill_frame`, ≈ 210 líneas con tres ramas).
- **Arreglo propuesto:** `fill_empty`, `fill_simple` y `fill_model`.
- **Tarea:** F4.3 o la próxima tarea que toque `xml.rs`.

### 49. Lexema de 1 024 bytes en los textos numéricos (Info I1 del security-auditor)

- **Gravedad:** Info.
- **Estado:** un texto `xs:decimal`/`xs:integer`/`xs:float`/`xs:double` de más de 1 024 bytes sin
  espacios con una faceta numérica, un `enum` o un `fixed` da `limit` (el registro del lexema,
  §21 y §3.4 de [automata.md](automata.md)); sin facetas se acepta.
- **Arreglo propuesto:** decidirlo con los límites de F4.6 (§21) y decirlo en el informe.
- **Tarea:** F4.6 (e informe).

### 50. Tope de tiempo de pared por compilación en el servidor

- **Gravedad:** **Medium** (DoS de CPU en el servidor; bloquea exponer la compilación sin tope).
- **Estado:** los presupuestos (2²⁰ entradas, 2²⁴ pasos) acotan memoria y trabajo, pero no
  el tiempo de pared con esquemas de 16 MiB hostiles (≈ 5 s en JSON, §7 de
  [automata.md](automata.md); la UPA del frontal, 7-28 s, 5.7-P3 de
  [fase3/pendientes.md](../fase3/pendientes.md) §38).
- **XML (F4E-M1, revisión tras la ronda 3):** un `xs:choice*` de 260 hijos con un prefijo común de
  57 000 caracteres (`sh260.xsd`, 14,8 MB) tardaba 11,4 s en dar `limit` (≈ 0,7 s/MB, el doble
  que los 5 s de JSON); `n3.xsd` 8,6 s; `b4.xsd` 8,8 s. Tras la ronda extra (§55): 0,30 s,
  0,35 s y 0,30 s, lo mismo que el frontal solo (`schemac check`); el resto de la construcción
  XML va a ≈ 0,07 s/MB (§54). Lo que queda para el tope de pared es la UPA del frontal y JSON.
- **Arreglo propuesto:** un tope de tiempo de compilación por petición en el sandbox del servidor.
- **Tarea:** F7.1 (enlazado con 5.7-P3).


### Revisión de F4.1c tras la ronda 2 (code-reviewer y security-auditor, 19 nov)

### 51. C2, F4D-S1, F4D-M2, I4/F4D-L2, I5, F4D-M1 y F4D-L1 — RESUELTOS (ronda 3 de F4.1c)

- **El problema de fondo** (C1 → C2, I1 → F4D-M2, F4C-S1 → I4): en el análisis XML
  (`Analysis`, `crates/automaton/src/xml.rs`) se reservaba o se trabajaba **antes** de contarlo
  contra el presupuesto de F4-U6 (D53 (5), §22.5) o el de pasos. En lugar de arreglar caso por
  caso, la ronda 3 inventarió **cada** sitio del análisis (y de `conj.rs`, `glushkov.rs` y
  `text.rs` en lo que usa XML) que reserva o trabaja en proporción al esquema y dejó escrito, en
  el código y en la tabla «sitio → qué lo acota» de [automata.md](automata.md) §15, que su coste
  se carga y se comprueba antes, o que está acotado por algo ya comprobado.
- **C2 (Critical) + F4D-S1 (High):** granularidad de JSON (comprobación tras cada texto,
  atributo, modelo, tipo, global, hijo de `frame_cost` y marco); el texto suma su emisión
  (estados × c(t)) una vez al crearse (`Analysis::text`, `xml.rs:770`) y `elem()` ya no la
  suma; las palabras de los `enum` de cadena se memorizan por (lista de reglas `enum`, `fixed`)
  (`Analysis::words`, `xml.rs:949`), con Σ |`enum`| cargado a los pasos y a `extra` y el
  presupuesto comprobado **antes** de copiar y ordenar; `text_checks` lee las palabras por
  índice, sin clonarlas. Pruebas: `cadena_de_enumeraciones_da_limit_antes_de_copiar`,
  `enumeraciones_derivadas_dan_limit_antes_de_copiar` (`tests/sin_reservas.rs`) y la unitaria
  `enum_derivado_se_copia_una_vez` (`src/xml.rs`, contador de palabras de solo pruebas).
- **F4D-M2 (C2 repro B):** el tope de pares de follow (presupuesto de δ entre `CHAR + 1`) se
  aplica a la **suma** de todos los modelos: `Analysis::model` pasa a `glushkov::build` lo que
  queda (`xml.rs:714`), que lo comprueba antes de materializar bloques, pares y `KEdge`
  (`Glushkov::pairs`). Prueba: `pares_de_glushkov_acumulados_dan_limit`.
- **I4 / F4D-L2:** el umbral IEEE de cada cota de `xs:float`/`xs:double` se calcula una vez
  por (nodo, regla) en `Analysis::text` (`float_bounds`, `xml.rs:854`), con `ROUNDING_STEPS`
  cargado antes; el `Builder` lo lee del memo. Prueba: `umbrales_flotantes_una_vez_por_regla`.
- **I5:** el trie del documento suma a `extra` un `Fails` por estado (`xml.rs:352`). Prueba: la
  propiedad del presupuesto (`tests/comun/presupuesto.rs`) en todo `parse-ok`
  (`presupuesto_cubre_lo_emitido_en_parse_ok`, que fallaba con `vacio.xsd`: 6 846 emitidas
  frente a 6 844 contadas) y en cada esquema de `tests/{xml,glushkov}.rs`.
- **F4D-M1:** `FACET_WEIGHT` = 4 pasos del presupuesto de los cierres por regla de cada
  componente del cierre de un texto, antes de recorrerlas (`xml.rs:795`); los conjuntos
  canónicos se guardan una vez (`Rc<[NodeId]>` compartido entre la lista y la tabla de ids,
  `conj.rs:134`; la suite JSON y la dorada no cambian). Prueba:
  `cierres_largos_dan_limit_antes_de_copiar`.
- **F4D-L1:** `saturating_add`/`saturating_mul` en `frame_cost`, `entry_cost`, `empty_cost`,
  `elem` y `upper`.
- **Medidas** (release, `schemac automaton`, tiempo y pico de memoria con `getrusage`; el
  frontal solo, `schemac check`, entre paréntesis cuando domina):

  | Esquema (hallazgo) | Antes | Después |
  |---|---|---|
  | `s2000.xsd`, 12 MB (C2 repro A) | `limit`, 29,2 s, 178 MB | `limit`, 0,58 s, 134 MB (`check`: 0,53 s, 118 MB) |
  | `audit_enum_derived_20000x4000.xsd`, 1,3 MB (F4D-S1) | `limit`, 16,7 s, 1 253 MB | `limit`, 0,06 s, 20 MB |
  | `m230.xsd`, 2,6 MB (F4D-M2) | `limit`, 2,36 s, 1 375 MB | `limit`, 0,19 s, 43 MB (`check`: 36 MB) |
  | `audit_choice400x200.xsd`, 3,4 MB (F4D-M2) | `limit`, 2,37 s, 1 374 MB | `limit`, 0,22 s, 56 MB (`check`: 47 MB) |
  | `audit_choice8000x40.xsd`, 14 MB (F4D-M2) | `limit`, 2,55 s, 1 187 MB | `limit`, 0,80 s, 262 MB (`check`: 0,72 s, 223 MB) |
  | `r170.xsd` (I4) | compila, 1,17 s, 17 MB | compila, 0,06 s, 17 MB |
  | `audit_floatfacets130.xsd` (F4D-L2) | compila, 1,35 s, 17 MB | compila, 0,06 s, 17 MB |
  | `audit_closure_chain.xsd`, 1,2 MB (F4D-M1) | `limit` de cierres, 1,10 s, 828 MB | `limit`, 0,07 s, 24 MB |
  | `vacio.xsd` (I5) | compila; cuenta 6 844 < 6 846 emitidas | compila; la cuenta cubre lo emitido |
  | `b3.xsd`, `d60k.xsd`, `a.xsd`, `a0.xsd` (ronda 2) | 0,05-0,17 s, 9-23 MB | igual |
  | pedido (`valid-xml-pedido-01`) | 257 estados | 257 estados |

  La frontera de `xs:choice maxOccurs="unbounded"` no cambia: 116 hijos `eN` compilan (16 264
  estados) y 117 dan `limit` (1 049 175, 4 entradas más que antes por los `Fails` del trie del
  documento, I5). Con nombres `elementoNNN` (más largos) la frontera ya estaba entre 100 y 110.

### 52. S8: dos contabilidades del mismo marco (`elem` y `frame_cost`)

- **Gravedad:** Low (mantenimiento; las dos cuentas son conservadoras y la propiedad del
  presupuesto vigila que cubran lo emitido).
- **Dónde:** `crates/automaton/src/xml.rs:582` (`elem`: guardas + operaciones + 4 por arista,
  2 por estado, máscaras) y `:450` (`frame_cost`: 3 · guardas + `entry_cost` por arista, por
  marco).
- **Estado:** la ronda 3 no las unificó: `frame_cost` cuenta por marco (lo que se repite por
  nombre) y `elem` por tipo (operaciones de contador, que se guardan una vez por lista), así que
  no salió natural con C2. Se solapan en parte (las guardas de cada arista).
- **Arreglo propuesto:** una sola función de cuenta por pieza emitida (`Count` por tipo y por
  marco), junto con partir `fill_frame` (§48).
- **Tarea:** F4.3 (con §48).

### 53. Cuentas conservadoras del presupuesto XML (ronda 3)

- **Gravedad:** Low (pueden rechazar con `limit` un esquema justo en el borde del presupuesto;
  nunca aceptan uno que no cabe).
- **Precisión (S12 de la revisión tras la ronda 3):** el sobrecoste es pequeño: en los esquemas
  medidos lo contado es ≈ 1,03 × lo emitido (y la cuenta de F4-U6 de un intervalo IEEE es ≈ 60
  veces lo que de verdad cuesta en tablas, F4D-I1, porque se cobra en pasos). Los `limit` en
  esquemas legítimos (`re2`) **no** vienen de estas cuentas sino de la δ densa con |Σ| = 61 y
  del AFD del contenido simple copiado por marco sin minimizar: §39, con su criterio en F4.3.
- **Dónde:** `crates/automaton/src/xml.rs:770` (`text`), `:949` (`words`), `:714` (`model`).
- **Estado:** (1) el texto de un contenido simple se cuenta al crearlo y además en cada marco
  que lo usa (`frame_cost`); (2) Σ |`enum`| de un fallo de memo de `words` cuenta los literales
  repetidos entre los `enum` de una cadena de restricciones (la unión es la base, D44); (3) el
  tope de pares acumulado cuenta las derivaciones repetidas de un mismo par (`(a*)*`), que es lo
  que se materializa.
- **Arreglo propuesto:** si en F4.3 (Hopcroft y tries compartidos, §39, §47) algún esquema real
  queda fuera por esto, contar el texto de contenido simple solo por marco y los pares
  deduplicados.
- **Tarea:** F4.3.

### 54. Techos de memoria de la compilación XML (para el informe)

- **Gravedad:** Info.
- **Estado:** con los presupuestos (2²⁰ entradas, 2²⁴ pasos de Glushkov/IEEE/literales y 2²⁴
  pasos de cierres), lo que la construcción XML puede pedir además del frontal está acotado: los
  conjuntos canónicos ≤ 2²⁴ `NodeId` (64 MB, una copia desde F4D-M1), las entradas de los
  textos ≤ 2²² (`FACET_WEIGHT`) más lo que cabe en el presupuesto, el primer modelo de Glushkov
  que agota el tope de pares (≈ 30 MB con 69 696 pares), y las tablas ≤ 2²⁰ entradas. En los
  repros de las revisiones, la memoria de la compilación queda por debajo de la del frontal
  (§51); el tiempo de pared lo acota §50.
- **Lexemas en bytes (S11):** el presupuesto de F4-U6 cuenta los lexemas **como entradas**
  (`nums`, D53 (5)), no en bytes, y la propiedad de `crates/automaton/tests/comun/presupuesto.rs`
  hace lo mismo. Los umbrales e intervalos IEEE dan lexemas de ≈ 770 B: 8 000 `enum` de
  `xs:double` son ≈ 12 MB de `lexemes`, acotados por los 2²⁴ pasos (`ROUNDING_STEPS`).
- **Peores cocientes medidos tras la ronda 3** (security-auditor, F4D-I1/F4D-I2): la cuenta de
  F4-U6 es conservadora, ≈ 60 entradas por cada intervalo de redondeo realmente emitido; la
  memoria del autómata es ≈ 6 × el XSD en el peor caso (`gdeep`: +41 MB para 6,8 MB) y siempre
  ≤ 50 MB por encima del frontal; el tiempo, ≈ 0,07 s/MB, salvo F4E-M1 (≈ 0,7 s/MB), cerrado en
  la ronda extra (§55).
- **Tarea:** informe (Fase 5/6), con §50.

### Revisión de F4.1c tras la ronda 3 (code-reviewer y security-auditor, 19 nov)

### 55. I6 = F4E-M1: bytes de los nombres de follow(x) sin contar — RESUELTO (ronda extra de F4.1c)

- **Dónde estaba:** `crates/automaton/src/xml.rs` (`elem`: `trie_size` de los nombres de cada
  estado entre hijos; `frame_cost` lo repetía), `crates/automaton/src/glushkov.rs` (UPA: ordena
  (nombre, destino) por estado) y `crates/automaton/src/emit.rs:72` (`trie_size`, prefijo común
  con `chars().zip`). El tope de pares acota pares, no la longitud de los nombres: un
  `xs:choice*` de 260 hijos con un prefijo común de 57 000 caracteres son 67 600 pares (bajo el
  tope) y ≈ 3,8·10⁹ bytes comparados antes de comprobar nada. La fila «`elem`, modelo» del
  inventario de [automata.md](automata.md) §15 decía que lo acotaba el tope de pares: no era
  cierto.
- **Arreglo:** `glushkov::build` carga Σₓ Σ_{y ∈ follow(x)} |nombre(y)| (más los destinos del
  estado inicial) al presupuesto de pasos con una pasada aritmética sobre los pares ya contados,
  antes de los efectos, la UPA y los tries (`crates/automaton/src/glushkov.rs:298`); `xs:all`
  carga 3 × Σ |nombre| antes de ordenar (`xml.rs:635`); los tamaños de los tries por estado se
  calculan una vez por modelo (`ModelSpec::tries`, `state_tries`, `xml.rs:103`) y `elem`,
  `frame_cost` y el trie del documento los leen de ahí; `trie_size` mide el prefijo común por
  bytes. No se internaron los nombres: con el cobro previo, la UPA ordena por estado los nombres
  de follow(x), que son Σ bytes ≤ 2²⁴, con ⌈log₂ k⌉ comparaciones cada uno (k = destinos por
  estado; ⌈log₂ k⌉ ≤ 17 con el tope de pares): O(2²⁴ · log k) bytes comparados.
  De paso (F4E-I1), sumas saturadas en `floats` (`3 · valores`), `glushkov::build` (`steps`,
  `n_blocks`) y `trie_size`.
- **Pruebas:** `nombres_largos_de_follow_se_cobran_antes` (unitaria de `src/xml.rs`, contador
  de bytes recorridos por `trie_size` solo en pruebas: < 1 MiB, antes 2,7·10⁸) y
  `nombres_largos_de_follow_dan_limit_antes_de_recorrerlos` (`tests/sin_reservas.rs`, prefijo de
  50 000 caracteres: `limit` con < 16 MiB pedidos). La propiedad del presupuesto sigue en verde.
- **Medidas** (release, `schemac automaton`): `sh260.xsd` 11,38 s / 47 MB → 0,30 s / 46 MB;
  `n3.xsd` 8,60 s / 48 MB → 0,35 s / 49 MB; `b4.xsd` 8,76 s / 49 MB → 0,30 s / 49 MB (el frontal
  solo: 0,28-0,31 s). Sin regresiones: pedido 257 estados; `choice*` de 116 hijos `xN` compila
  (16 264 estados) y 117 da `limit` (1 049 175); `re1` compila.

### 56. S13: `Builder::new` calcula las clases de las palabras por texto

- **Gravedad:** Low (acotado por el presupuesto; memoria transitoria).
- **Dónde:** `crates/automaton/src/xml.rs:1213` (`Builder::new`: `t.classes_of` de todas las
  palabras de cada texto, 8 B por carácter, aunque varios textos compartan el mismo
  `WordSet`).
- **Estado:** `w4.xsd` de la revisión (15,3 MB, 3 900 palabras de 3 900 caracteres con prefijo
  común) compila en 0,69 s y 153 MB, frente a 63 MB de `schemac check`.
- **Arreglo propuesto:** memorizar las clases (y el AFD del texto, que solo depende del espacio
  léxico y de las palabras) por `TextSpec::words`.
- **Tarea:** F4.3.

### Revisión de F4.1c tras la ronda extra (19 nov)

### 57. F4.1c-CRX-S4: la prueba de I6 no cubre la UPA

- **Gravedad:** Low (cobertura de un comportamiento correcto).
- **Dónde:** `crates/automaton/src/xml.rs:2179` (`nombres_largos_de_follow_se_cobran_antes`) y
  `crates/automaton/src/glushkov.rs:298` (`work.spend(bytes, ctx)`).
- **Estado:** el contador `CARACTERES` (`crates/automaton/src/emit.rs`) solo cuenta lo que
  recorre `trie_size`; si el cobro de `glushkov.rs:298` se moviera a después de la UPA, ninguna
  prueba fallaría (la UPA compararía los nombres antes de cobrarlos y `trie_size` seguiría sin
  recorrerlos).
- **Arreglo propuesto:** contar también, solo en pruebas, los bytes que compara el bucle de la
  UPA de `glushkov::build`, o comprobar en la prueba que el `Limit` es el del presupuesto de
  pasos («pasos de la construcción XML…») y no el de δ.
- **Tarea:** F4.3.

### 58. F4.1c-CRX-S5 = F4E-I2: el trie de los atributos se recalcula por marco

- **Gravedad:** Info (acotado por el presupuesto de δ: cada marco suma ese trie a |Q|).
- **Dónde:** `crates/automaton/src/xml.rs:384-390` (`Analysis::run`: `trie_size(&attr_names)`
  por cada marco (tipo, nombre), sin cobrar los bytes de los nombres).
- **Estado:** `at8000.xsd` (16 MB) tarda 0,48 s frente a 0,30 s de `schemac check`; `attrsglob`
  (400 000 globales) 1,02 s frente a 0,64 s.
- **Arreglo propuesto:** guardar el tamaño del trie de atributos en `ElemSpec` junto a
  `content_states` (una vez por tipo) y cargar Σ |nombre| de los atributos una vez al presupuesto
  de pasos.
- **Tarea:** F4.3, junto con §56.

### Abiertas por la tarea F4.6 (ejecutor)

### 59. Interfaz del ejecutor para las dos personas (F4.6-P1; D42)

- **Gravedad:** Low (interfaz compartida: F4-U18, `sv_report_t`, API C-ABI).
- **Estado:** F4.6 fijó, sin cambiar lo aprobado en D51/D52: (1) `Options { mode, limits }`
  con `Limits { max_depth, max_string, max_keys, key_pool, max_unique (F4.5b), max_xml_refs, max_payload,
  hash_key }` (F4-U18 decía `new(tables, &Limits, mem)`; el modo va en las opciones, como dice
  el mismo F4-U18); (2) `Tables::state_words(&Limits)` y `FRAME_OVERHEAD` = 2, 4 desde F4.5b (cada marco
  lleva una palabra de buena formación: `sv_state_t` = zona escalar + `max_depth` × (marco
  máximo + 2) + `key_pool` palabras); (3) la clave repetida (declarada o no) se informa en su
  comilla de **cierre** (antes, la declarada en la de apertura), como la línea base; (4) el
  límite de cadena cuenta bytes UTF-8 decodificados de todo tramo de caracteres, también
  nombres y textos XML (F4-B23); (5) `\r\n` normalizado en XML; (6) la clave de SipHash por
  defecto es pública (§4 de [runtime.md](runtime.md)).
- **Arreglo propuesto:** proponerlo a Bayron en el mismo mensaje que D57 (1)-(4) y §34; F6.2
  expone `Limits` en `sv_limits_t`.
- **Tarea:** F6.2 (y F5.1/F5.2 para portar la disposición de la memoria).

### 60. `DIVERGENCIAS` de la línea base con el campo `errorKind` (F4.6-P2)

- **Gravedad:** Info (prueba de la pista B).
- **Estado:** `extreme-big-number-01` da `limit` en el DPDA y `Unsupported` en
  `serde_json` (10¹⁰²⁴ no cabe en `f64`, D50 B19). F4.6 amplió la tabla `DIVERGENCIAS` de
  `crates/baseline/tests/corpus_json.rs` para admitir el campo `errorKind` (solo con el mismo
  `valid`; una entrada que ya no diverge sigue siendo un fallo) y declaró el caso.
- **Arreglo propuesto:** avisar a Bayron (es su crate).
- **Tarea:** —.

### 61. Costo de comparar con una cota larga (F4.1b-CR-S3; recorte de F4.6)

- **Gravedad:** Medium (rendimiento con esquemas hostiles: el esquema lo controla quien
  valida, no el payload). **High en el servidor desde F7.5 (E3A-2):** `WS /ws/validate` acepta el esquema
  del cliente (≤ 1 MiB) y valida en el proceso sin tope de CPU. Medido en E3: 2 000 números contra un
  `minimum` de 10⁶ cifras cuestan 1,6 s en C y 3,0 s en Rust (0,004 s con `minimum: 1`), lineal en los
  números: un bloque de 1 MiB serían minutos de CPU por sesión, con 4 sesiones a la vez.
- **Mitigación inmediata (pista B):** plazo de CPU por sesión en `server/src/flujo.rs` (como el de las
  compilaciones), en `docs/fase7/pendientes.md` §17.
- **Estado:** al cerrar cada número, `num::compare` e `is_multiple` cuestan
  O(|cota del esquema|), hasta 16 MiB. No se hizo en F4.6 (orden de recorte de
  `tasks/plan.md`: no cambia ningún veredicto).
- **Arreglo propuesto:** en compilación, quitar los ceros a la izquierda del exponente de la
  cota, decidir si |E| pasa del rango alcanzable por un lexema de 1 024 bytes (comprobación
  constante) y guardar en `DivisorRef` el exponente ya saturado, para que cada número cueste
  O(`LEXEME_MAX`).
- **Tarea:** F6.5 (medición) o E3 (revisión de seguridad); antes de F7.5 si el servidor valida
  con esquemas del usuario.

### 62. Espacios de nombres y `xsi:*` (sigue §34)

- **Estado:** F4.6 no cambió §34: necesita la decisión de las dos personas (D42). Mientras
  tanto, un elemento `<x:pedido xmlns:x="urn:otro">` se valida como `pedido` y `xsi:*` suma 1.
- **Tarea:** decisión de las dos personas; si es la opción (b), F6.2.

### Abiertas por la tarea F4.3 (determinismo y Hopcroft)

### 63. Lo heredado de F4.1c que F4.3 no hizo (recorte; §39, §47, §48, §52, §53, §56-§58)

- **Gravedad:** Medium (tamaño de M en XML con esquemas grandes; ningún veredicto cambia).
- **Estado:** F4.3 minimiza cada parte finita y verifica el determinismo (§17 de
  [automata.md](automata.md)), pero **no** cambia la representación XML que pedían los heredados
  de la revisión de F4.1c, por el orden de recorte del plan (lo que no cambia ningún veredicto
  va después del congelamiento): (1) grupo global del texto de contenido simple con
  `Enter`/`Leave` y trie compartido entre estados con la misma lista de aristas, para que
  `re2` (200 elementos de 40 tipos × 50 literales) compile (§39/§47/§53); (2) partir
  `fill_frame` y una sola contabilidad por pieza (§48/§52); (3) `classes_of` memorizado por
  conjunto de palabras (§56); (4) la prueba de que el cobro de los nombres de follow(x) no se
  mueve detrás de la UPA (§57); (5) el tamaño del trie de atributos memorizado por tipo (§58);
  (6) ajustar la cuenta conservadora de F4-U6 (F4D-M1 residual): no hace falta, porque la
  representación no cambió.
- **Arreglo propuesto:** el de cada sección citada.
- **Tarea:** después de PC-E (E4 lo usa para «tamaño del autómata y riesgo de explosión»), o
  una tarea nueva de la pista A si sobra tiempo.

### 64. Campos nuevos del resumen de `schemac automaton` (F4.3-P1; para Bayron)

- **Gravedad:** Info (interfaz F4-U7, aditiva).
- **Estado:** el resumen añade `dfaStates` y `dfaStatesMinimized` al final, sin cambiar
  `automatonVersion` (1). `maxFrameBytes` cuenta ya la palabra de buena formación de F4.6 (la
  dorada pasa de 16 a 24). El servidor (F7.1) copia el resumen tal cual y la web (F7.4) lo
  muestra; `web/src/*.test.*` tienen el resumen viejo como dato de prueba.
- **Arreglo propuesto:** avisar a Bayron; si la web lista los campos con nombre, añadir los dos
  nuevos.
- **Tarea:** F7.6 (vista del autómata).

### Abiertas por la tarea F4.4a (analizador de `pattern`)

### 65. D29: enumeraciones, `default` y `fixed` frente a `pattern` (recorte 1)

- **Gravedad:** Low (acepta de más en el esquema: una enumeración que no cumple el `pattern` de
  su base hace que ese valor nunca valide; omisión ya escrita en `00-alcance.md` §3.4).
- **Estado:** no se hizo (orden de recorte de `tasks/plan.md`, primer punto). Con
  `ir::regex::Regex::matches` es una llamada por valor en `crates/xsd/src/simple.rs`
  (`valid_restriction`), contra las `pattern` de toda la cadena de bases.
- **Arreglo propuesto:** error `value` en la posición del valor (`ZZZ` sobre
  `pattern="[a-z]"`) y quitar la omisión de `00-alcance.md` §3.4 (interfaz compartida, D42).
- **Tarea:** después de PC-E, si hay tiempo; si no, se queda como omisión documentada.

### 66. Mensajes de `pattern` y el código `limit` de la Fase 3 (F4.4a-P1; para Bayron)

- **Gravedad:** Info (interfaz de errores de `schemac check` y del 422 del servidor).
- **Estado:** la Fase 3 de JSON tiene un código nuevo, `limit` (`semantic::ErrorKind::Limit`),
  para un `pattern` demasiado grande; en XSD, `ErrorKind::PatternLimit` con el código `limit` que
  ya existía. La web pinta los códigos como texto, así que no se rompe nada.
- **Tarea:** avisar a Bayron.

### Abiertas por la tarea F4.4b (`pattern` → AFD)

### 67. Offset de las violaciones del cierre de una cadena (F4.4b-P1; para Bayron)

- **Gravedad:** Info (interfaz del reporte, F4-U18).
- **Estado:** desde F4.4b, toda violación que se detecta al cerrar una cadena JSON (`enum`,
  longitudes, `pattern`) se informa en la **comilla de cierre** (antes, en la de apertura,
  porque el `Span` de `StringEnd` empieza ahí); `invalid-enum-01` pasa de 10 a 15. La línea base
  no compara offsets de violaciones, así que no cambia nada en su prueba.
- **Tarea:** avisar a Bayron (el puntero de F6.1 y la web lo mostrarán).

### 68. Prueba del servidor con una regla pendiente (F4.4b-P2; para Bayron)

- **Gravedad:** Info (prueba de la pista B).
- **Estado:** `regla_pendiente_da_200_con_automata_pendiente` (`server/tests/api.rs`) usaba
  `pattern`, que ya compila; ahora usa `uniqueItems` (F4.5b). Cuando F4.5b esté hecha no quedará
  ninguna regla pendiente en JSON y esa prueba tendrá que usar otro estado (`limit`).
- **Tarea:** F4.5b (ajustarla) y avisar a Bayron.


### 69. Revisión del informe de F4.4b: clase «otro» y presupuesto — RESUELTO (20 nov)

- **7.1 (defecto):** `Partition::reps` tomaba `0xE000` como representante de la clase 0
  aunque estuviera cubierto (`^[^😀]$` aceptaba `"😀"`). Ahora busca el primer hueco que no sea
  un sustituto (`clase_otro_con_el_hueco_de_los_sustitutos`).
- **7.2 (presupuesto):** |Σ| se contaba con `chars.len()` sin las clases de los `pattern`; ahora
  la cuenta final es la partición exacta y las parciales suman los rangos, y el producto de una
  cadena JSON se acota (estados × clases ≤ `max_delta`) antes de reservar
  (`presupuesto_con_las_clases_de_los_pattern`).

### Abiertas por la tarea F4.5a (combinadores)

### 70. Simulación paralela en marcos escalares y estrategia en el resumen (recortes 3 y 5)

- **Gravedad:** Low (ningún caso del corpus cambia de veredicto; afecta a lo que compila).
- **Estado:** al pasar el tope de 4 096 un producto escalar da `limit` (`ProductLimit`,
  `scalar: true`) en vez de simular las componentes en paralelo (F4-U11); el resumen de
  `schemac automaton` no da la estrategia efectiva de cada regla (c) ni el campo `strategy` del
  corpus se compara (recorte 5: resumen mínimo; sigue `products`).
  Consecuencia: un `anyOf` (o `allOf`) de varias ramas con `pattern` sobre la misma cadena
  multiplica sus AFD y puede dar `limit` antes que la simulación paralela. Un conjunto formado
  solo por componentes con salida se nombra `#` en los mensajes (`label` no ubica los ids
  virtuales).
- **Arreglo propuesto:** F4-U11 tal cual (una zona de estados por componente en el marco
  escalar) y un campo `strategies` en el resumen.
- **Tarea:** después de la entrega, si se retoma.

### 71. Atribución del primer error que llega por una salida (F4.5a-P1; para Bayron)

- **Gravedad:** Info (interfaz del reporte).
- **Estado:** una violación dentro de una rama no es la «primera» del reporte hasta que llega al
  peso global: la puerta que salta se atribuye a la regla del combinador, en el offset del
  evento que cierra la instancia (`non-dpda-anyof-none-01`: el `}` en 7). Las comprobaciones
  de entrada de un valor cuentan en la instancia que abren (`SLOT_CHILD`).
- **Tarea:** F6.1 (el puntero de la instancia) y avisar a Bayron.

### Abiertas por la tarea F4.5b (`uniqueItems` y literales)

### 72. Literales compuestos de `enum`/`const` (recorte 2; interfaz compartida, D42)

- **Gravedad:** Medium (esquemas válidos según el alcance que no compilan).
- **Estado:** `enum`/`const` con objetos o arreglos siguen dando `estrategia pendiente: const
  (F4.5b)` (código 1 de `schemac automaton`; `automaton.status = "pending"` en el servidor).
  Compararlos por hash canónico se descartó (con la clave pública se fabrica un choque en ≈ 2³²
  pasos y se aceptaría un valor que no es el literal); el autómata exacto de F4-U17 no entró.
- **Arreglo propuesto (decisión de las dos personas):** (a) escribir en `00-alcance.md` §2.2
  que los literales compuestos se rechazan (es el recorte 2 del plan), o (b) F4-U17: traducir
  cada literal a reglas (`const` por clave, `required` de todas, `additionalProperties: false`;
  `prefixItems` + longitud exacta) con la puerta ANY de F4.5a.
- **Tarea:** decisión de las dos personas; (b) después de la entrega.

### 73. `FRAME_OVERHEAD` = 4 y el resumen (F4.5b-P1; para Bayron)

- **Gravedad:** Info.
- **Estado:** cada marco lleva dos palabras más (hash canónico y clave del miembro), así que
  `maxFrameBytes` de la dorada pasa de 24 a 40 y `state_words` suma `max_unique` (4 096). F5.1 y
  F5.2 portan esta disposición.
- **Tarea:** F5.1, F5.2; avisar a Bayron.

