# Pendientes de la Fase 4 (autómata de pila ponderado)

Observaciones que dejó abiertas la tarea F4.1a (modelo formal de M, pesos, núcleo `no_std`
y esqueleto JSON; teoría en [automata.md](automata.md)) y que se resuelven en otra tarea.
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
  `BuildError::Limit::found` (`crates/automaton/src/json.rs:398`) da esa cota y no el
  número de entradas de δ que tendría M; el mensaje («límite superado: N entradas de δ…»)
  debe decir «como mucho N» o dar el tamaño real. Se corrige cuando una tarea toque ese
  código (F4.1b, que añade el tope por producto, o F4.1c, que añade el presupuesto del
  Glushkov).
- **Arreglo propuesto:** añadir las tres filas a la tabla de §4, precisando que el
  presupuesto de transiciones se mide en entradas de la tabla densa (|Q| · |Σ|). Lo
  escribe quien cierre F4.1a con el visto bueno de las dos personas (no lo cambia un
  agente, D42).
- **Tarea:** cierre de F4.1a (orquestador).

### 3. Partición de caracteres por contexto (F4.1a-P3)

- **Estado:** F4-B3 pide clases de carácter separadas por contexto (clave o valor); en
  F4.1a hay una sola partición, la de los nombres de los objetos, que usan también las
  cadenas valor (sin reglas que distingan caracteres, da el mismo resultado).
- **Arreglo propuesto:** cuando las cadenas valor tengan tries o AFD (`enum` de cadenas en
  F4.1b, `pattern` en F4.4b), particionar por contexto: el estado ya sabe si lee una clave
  o un valor, así que basta con dos tablas de clases o con la partición común más fina;
  vigilar que |Σ| no crezca con el producto de las dos.
- **Dónde:** `crates/automaton/src/json.rs` (`Builder::new`, clases de carácter).
- **Tarea:** F4.1b (y F4.4b).

### 4. Tries sin minimizar (F4.1a-P4)

- **Estado:** el trie de los nombres de cada objeto es un `Dfa` completo con un estado por
  prefijo; dos nodos con las mismas reglas tienen subautómatas distintos.
- **Arreglo propuesto:** Hopcroft (F4-B15) sobre cada `Dfa` antes de incrustarlo, y
  medir el efecto en la dorada (que entonces cambia: regenerar con `make doradas` y
  `make docs`, y recontar [automata.md](automata.md) §8).
- **Tarea:** F4.3.

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
- **Vigilar:** `scripts/lint.ps1` (Windows sin `make`) no tiene todavía esta comprobación.

### Observaciones abiertas de la revisión de F4.1a (code-reviewer, 16 nov)

### 9. F4.1a-CR-I1: instancias escalares frente a las salidas y puertas de F4.5a

- **Dónde:** `crates/runtime-rs/src/core.rs:297-301` (`Op::Enter`/`Op::Leave`),
  `crates/automaton/src/json.rs:756-762` (E(N, a, r) con `Op::Move` para `true`/`false`/
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

### 10. F4.1a-CR-I4: `BuildError::Limit` sin pruebas

- **Dónde:** `crates/automaton/src/json.rs:375` (más de 4 096 nombres por objeto) y
  `crates/automaton/src/json.rs:395` (más de 2²⁰ entradas de δ).
- **Arreglo propuesto:** pruebas: un objeto con 4 096 nombres compila y uno con 4 097 da
  `limit` con su ubicación; un esquema por encima de 2²⁰ entradas da `limit` sin reservar
  (con el asignador contador de `sin_reservas.rs` o comprobando que no se llega a reservar
  δ).
- **Tarea:** F4.1b.

### 11. F4.1a-CR-S1: la máscara ocupa todo el marco

- **Dónde:** `crates/runtime-rs/src/tables.rs:146-154` (`Frame::words`) y
  `crates/runtime-rs/src/core.rs:386-392` (`Core::contains`, que compara
  `masks[m .. m + words]` con todo el marco).
- **Estado:** hoy `Frame::words` son las palabras de la máscara y no hay otros registros.
- **Arreglo propuesto:** al añadir contadores, separar la máscara (`mask_words` o un
  desplazamiento dentro del marco) para que `Test::Mask` y `Test::Dependent` no lean
  contadores.
- **Tarea:** F4.1b.

### 12. F4.1a-CR-S2: una clase fuera de rango lee la fila siguiente (seguridad)

- **Dónde:** `crates/runtime-rs/src/core.rs:205` (clase de carácter) y
  `crates/runtime-rs/src/core.rs:264-268` (`Core::step`, índice `q · n_classes + a`).
- **Estado:** con tablas incoherentes, una clase `≥ n_classes − CHAR` (de `ascii` o
  `ranges`) cae en la fila del estado siguiente en lugar de dar `Fault::Internal`. Con las
  tablas que construye `automaton` no ocurre, pero el núcleo no debe confiar en ellas.
- **Arreglo propuesto:** `if a >= t.n_classes { Internal }` en `step`, con una prueba de
  tablas corruptas.
- **Tarea:** F4.6 (revisión de seguridad, D35).

### 13. F4.1a-CR-S3: la pila que reserva `automaton::validate`

- **Dónde:** `crates/automaton/src/lib.rs:131-141`.
- **Estado:** reserva (número de `{` y `[` + 1) × (marco máximo + 1) palabras: hasta unos
  520 B por byte del payload con 4 096 nombres. Vale para pruebas, no para el servidor.
- **Arreglo propuesto:** tomar la profundidad de `Limits` (256 por defecto) y dar `limit`
  al pasarla; F4-B24 destina `validate` al servidor y al arnés F6.5.
- **Tarea:** F4.6 (y Fase 7).

### 14. F4.1a-CR-S4: pánicos y bucles defensivos en el constructor

- **Dónde:** `crates/automaton/src/json.rs:873` y `crates/automaton/src/json.rs:906`
  (`expect("reservado")`) y `crates/automaton/src/json.rs:159-176` (`resolve_aliases`, sin
  guarda ante un ciclo de solo `Ref`, que I14 excluye).
- **Arreglo propuesto:** devolver `BuildError::Internal` en lugar de pánico, y cortar el
  bucle de alias si da más de n pasos (defensa en profundidad, F4-B11).
- **Tarea:** F4.1b.

### 15. F4.1a-CR-S5: el campo `pending` del resumen

- **Dónde:** `crates/automaton/src/model.rs:87-88` (`Summary::pending`) y
  `crates/automaton/src/export.rs:28`.
- **Estado:** significa «reglas con estrategia de respaldo» y se confunde con
  `BuildError::Pending` (que hace fallar la construcción).
- **Arreglo propuesto:** renombrarlo (`deferred` o `fallback`) en la próxima tarea que toque
  `export.rs`, y en todo caso antes de que la web de Bayron consuma el resumen (F4-U7,
  interfaz compartida); regenerar `tests/automata/` con `make doradas`.
- **Tarea:** la próxima que toque `export.rs` (F4.1b o F4.5).

### 16. F4.1a-CR-S8: códigos fijos de `Op`, `Fault` y `Test`

- **Dónde:** `crates/runtime-rs/src/tables.rs:62-127`.
- **Arreglo propuesto:** `#[repr(u8)]` con discriminantes explícitos, para que el emisor C
  (F5.2) y las tablas `const` de F5.1 usen los mismos códigos.
- **Tarea:** F5.2 (o F5.1).

### 17. F4.1a-CR-S9: `Options` con campos públicos

- **Dónde:** `crates/runtime-rs/src/core.rs:30-33`.
- **Estado:** los literales `Options { mode }` de las pruebas dejarán de compilar al añadir
  los límites.
- **Arreglo propuesto:** `Options::new(mode)` o `Default` (con `..Default::default()`).
- **Tarea:** F4.6.

### 18. F4.1a-CR-S10: `integer` frente a `ir::num` y la línea base

- **Dónde:** `crates/runtime-rs/src/core.rs` (`Number::is_integer`).
- **Arreglo propuesto:** un proptest de `Number::is_integer` frente a `ir::num` sobre
  lexemas de RFC 8259 (con exponentes largos y negativos). La divergencia conocida
  `1e-99999999999999999999` con `type: integer` (el DPDA da 1 violación; la línea base, que
  pasa por `f64`, da 0) va a `DIVERGENCIAS` de `crates/baseline/tests/corpus_json.rs` y a
  `docs/fase6/pendientes.md` §5 cuando el caso entre al corpus (T-4).
- **Tarea:** F4.1b (proptest) y Fase 6 (corpus y divergencia).

### 19. F4.1a-CR-S11: cosméticos del DOT (nit)

- **Dónde:** `crates/runtime-rs/src/tables.rs:40` (`class::NAMES` llama «dígito» a
  `NUMBER_BYTE`, que es cualquier byte del lexema) y `crates/automaton/src/export.rs:72`
  (la agrupación de aristas por estado es O(k²)).
- **Arreglo propuesto:** renombrar a «byte num» (cambia el `.dot` de la dorada: `make docs`);
  la agrupación cuadrática es aceptable mientras el DOT sea solo para la documentación, y
  se cambia por un mapa si el DOT pasa al servidor.
- **Tarea:** la próxima que toque `tables.rs` o `export.rs`.

### 20. Para las dos personas (F4-U18 y F4-U6)

- Las tres precisiones del reporte (`ErrorKind::Internal`, el error de peso ∞ como primer
  error y el offset al comienzo del evento) están en §5; las filas de F4-U6 que faltan en
  `docs/00-alcance.md` §4, en §2. Se proponen juntas al cerrar F4.1a (D42).
