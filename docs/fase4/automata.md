# Fase 4 — El autómata de pila ponderado M

Primera tarea de la Fase 4 (F4.1a, D47 y D51 del plan): la definición formal de M, los pesos en el semianillo tropical, el núcleo `no_std` que lo ejecuta y el esqueleto de la construcción para JSON Schema. Las tareas siguientes amplían la misma construcción sin cambiar su forma: F4.1b (contadores, `enum`/`const` escalares, rangos, `multipleOf`, `$ref` con hermanas y `allOf`), F4.1c (XML), F4.3 (determinismo y Hopcroft), F4.4 (`pattern`), F4.5 (combinadores) y F4.6 (límites, reporte y capa de buena formación del ejecutor).

Implementación:

- [`crates/automaton`](../../crates/automaton/src/lib.rs): `build(&VerifiedIr) -> Result<Automaton, BuildError>` ([`json.rs`](../../crates/automaton/src/json.rs)), el modelo como tablas ([`model.rs`](../../crates/automaton/src/model.rs)), los pesos y el costo ([`weight.rs`](../../crates/automaton/src/weight.rs)), los AFD de las partes finitas ([`dfa.rs`](../../crates/automaton/src/dfa.rs)), `location_clipped` ([`location.rs`](../../crates/automaton/src/location.rs)) y las salidas de `schemac automaton` ([`export.rs`](../../crates/automaton/src/export.rs)).
- [`crates/runtime-rs`](../../crates/runtime-rs/src/lib.rs): el formato de las tablas ([`tables.rs`](../../crates/runtime-rs/src/tables.rs)), el núcleo ([`core.rs`](../../crates/runtime-rs/src/core.rs)), el lexer delante ([`json.rs`](../../crates/runtime-rs/src/json.rs)) y el semianillo ([`weight.rs`](../../crates/runtime-rs/src/weight.rs)). Es `#![no_std]` sin `alloc` y solo depende de `lexer`.
- `schemac automaton [--dot] <esquema>`: resumen de tamaños en JSON o M completo en DOT (F4-U7).

Dependencias (F4-B1, D13): `automaton` depende de `ir`, `lexer` y `runtime-rs`; `semantic`, `schema-parser`, `xsd` y `baseline` solo son dependencias de pruebas (antes `automaton` dependía de `semantic`, en contra de D13).

```
lexer ─▶ runtime-rs (núcleo no_std) ─┐
  │                                   ├─▶ automaton ─▶ codegen (Fase 5)
  └────▶ ir ──────────────────────────┘
```

## 1. Definición formal

M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) con

- **Q**: estados de control. Cada estado pertenece a un **grupo**: el subautómata de un nodo de la IR (su objeto, su arreglo o su número) o uno de los de «cualquiera». Son los clústeres del DOT.
- **Σ**: clases de **eventos del lexer** (§2), no bytes.
- **Γ**: marcos. Un símbolo de pila es X = (tipo de marco, estado de retorno, registros). El tipo de marco es el de un objeto o arreglo de un nodo (o de «cualquiera»); los registros, su máscara de claves (§3). Z₀ es la pila vacía: la raíz no se apila.
- **δ**: sobre el **control extendido** (q, r, ρ) (abajo): (Q × R × P) × Σ × Γ → Q × Op × (actualización de ρ y de la máscara de la cima) × (comprobaciones), con Op ∈ {mover, apilar(X), desapilar, abrir escalar, cerrar escalar, error}. La entrada de la tabla densa solo depende de (q, a); r interviene en «cerrar escalar» (el estado al que se vuelve) y ρ y la máscara de X en las comprobaciones (`integer` lee los registros del número, `required` la máscara). Es **total**: toda entrada que la gramática de JSON no admite va a la transición de error, de peso ∞.
- **q₀**: el estado inicial de la raíz; **F** = {fin de la raíz}, con la pila vacía y sin escalar abierto.
- **w**: δ → ℕ ∪ {∞} (D4): el número de comprobaciones de la transición que fallan, o ∞ en la de error (§5).

La configuración es (q, r, ρ, γ): el estado, el registro del escalar abierto (su estado de retorno y los registros del número), los registros globales y la pila γ ∈ Γ\*. El peso de un recorrido es el producto ⊗ (la suma) de los pesos de sus transiciones; el payload es conforme si llega a F con peso 0.

Por qué el escalar no se apila: en JSON hay a lo sumo un escalar abierto a la vez (una cadena o un número no contienen otros valores), así que su estado de retorno es **un** registro finito, no una pila. Formalmente el control es el par (q, r) con r ∈ R = Q: el producto de dos conjuntos finitos, que sigue siendo finito. A cambio, la pila solo crece con `{` y `[` (§4.3) y su altura es la profundidad de anidamiento de objetos y arreglos.

**Los registros son finitos por la saturación.** P, el conjunto de valores de ρ, es finito porque cada registro está acotado: los del número (§3.3) son `u64` saturados y el exponente se satura en 2⁶²; las máscaras tienen un número fijo de bits por esquema. Así (q, r, ρ) recorre un conjunto finito y M es un autómata de pila en sentido estricto, con la pila como única memoria no acotada. La saturación no es un detalle: sin ella, «el número es entero» no es un lenguaje regular sobre los bytes del lexema (compara un exponente escrito en decimal con el recuento unario de las cifras de la fracción y de los ceros finales). Con ella la respuesta es exacta mientras esos recuentos no saturen, es decir, para payloads de menos de 2⁶² bytes (unos 4,6 EB; el offset de `Pos` es `u64`, D9), y la saturación del exponente no cambia el resultado (§3.3).

## 2. Σ y la pasada única

El lexer de la Fase 1 (`lexer::json::JsonLexer`, un AFD de Mealy `no_std`) va **delante** de M como un transductor finito determinista: lee bytes y emite eventos. Σ son las clases de esos eventos (F4-U1):

| Clase | Evento |
|---|---|
| 0–8 | `{` `}` `[` `]` `:` `,` `true` `false` `null` |
| 9, 10 | apertura y cierre de una cadena |
| 11, 12, 13 | inicio de un número, un byte del lexema (la parte va a los registros del número, no a δ), fin del número |
| 14 + k | un carácter de una cadena (ya sin escapes ni UTF-8) de la clase k |

Las clases de carácter se calculan **por esquema** (F4-B3): cada punto de código que aparece en algún nombre con bit de un objeto (§3.1) tiene su clase, y todos los demás comparten la clase 0 («otro»). Así |Σ| = 14 + 1 + (puntos de código distintos de los nombres). La clase de un carácter sale de una tabla densa de 128 entradas en ASCII y de una búsqueda binaria en los rangos ordenados del resto (O(log k), con k fijo por esquema). En F4.1a las cadenas que son valores usan la misma partición (no tienen reglas que distingan caracteres); F4.1b y F4.4b la refinarán por contexto ([pendientes.md](pendientes.md) §3).

**Una pasada.** Cada byte entra una vez al lexer, que hace O(1) por byte, y emite como mucho un número constante de eventos por byte; cada evento entra una vez a δ (§4), que hace O(1) respecto del payload. La composición del transductor del lexer con M es otro DPDA: su estado es el par (estado del lexer, estado de M) y cada byte dispara una transición del lexer seguida de las de M sobre los eventos emitidos, todas deterministas.

Un byte puede emitir **más de un** evento: el byte que termina un número también es el siguiente token (`1}` da `NumberEnd` y después `EndObject`; `1,` da `NumberEnd` y `Comma`), y el último byte de un literal emite el literal. La composición sigue siendo un DPDA porque, de los eventos que emite un byte, **como mucho uno toca la pila**: `NumberEnd` (y los literales `true`/`false`/`null`) son internos para la pila (§4.3), y el evento estructural que los sigue es uno solo. El movimiento compuesto por un byte es, por tanto, una secuencia acotada (de longitud fija) de transiciones deterministas de M con a lo sumo una operación de pila: se puede leer como una sola transición del DPDA compuesto que lee el byte, cambia el control y hace esa operación de pila, o, equivalentemente, como una transición que lee el byte seguida de un número acotado de ε-transiciones deterministas (sin conflicto con las que leen, porque el lexer decide cuáles se disparan). No se pliega en un solo autómata (multiplicaría los estados de M por los 32 del lexer): se deja como composición, que es lo que harán también los emisores de la Fase 5 (dos niveles de tablas densas, F4-U1). Como el lexer y M guardan todo su estado entre bloques, el resultado no depende de cómo se parta la entrada (pruebas con bloques de 1, 7 y 4096 bytes).

## 3. Γ y registros

### 3.1 Marcos

Un objeto o un arreglo del payload es una instancia que **se apila** con su primer evento (`{`, `[`) y se desapila con el último (`}`, `]`) (F4-B5). En memoria cada marco son sus registros seguidos de una cabecera `(estado de retorno << 32) | tipo de marco` (una palabra de 64 bits). El tipo fija cuántas palabras de registros tiene (`Frame::words`), así que el marco de tamaño fijo es el de la mayor disposición: `sv_state_t` = profundidad máxima × marco máximo, de tamaño fijo por esquema.

Registros de un marco de objeto (F4-B4, S-1 de [fase3/pendientes.md](../fase3/pendientes.md) §5):

- **Máscara de nombres**: un bit por cada nombre de `properties` ∪ `required` ∪ claves y listas de `dependentRequired`, en ese orden y sin repetir. Como mucho 4 096 nombres por objeto (F4-U6; 64 palabras, 512 B). Al cerrar la clave de un nombre se fija su bit; si ya estaba, la clave declarada está **repetida** y es un error de peso ∞ (D2). Al cerrar el objeto, `required` es una comprobación «la máscara contiene la de los requeridos», y `dependentRequired` (D5), «para cada propiedad cuyo bit está, la máscara contiene la de su lista».
- Las claves **no declaradas** repetidas no tienen bit: las detecta la capa de buena formación del ejecutor con una tabla acotada por `max_claves_adicionales` (F4-B7, F4-U16), en F4.6.

Los arreglos de F4.1a no tienen registros: el índice dentro de `prefixItems` está en el estado (§6.3). F4.1b añadirá los contadores de miembros, elementos y puntos de código (`u64` saturados).

### 3.2 El escalar abierto

Una cadena o un número **no** se apilan (§1): `abrir escalar` guarda el estado de retorno en el registro global del escalar y va al subautómata del escalar; `cerrar escalar` vuelve a ese estado. `true`, `false` y `null` son un solo evento: su transición comprueba y vuelve directamente.

### 3.3 Registros del número

Para `type: integer` no hace falta guardar el lexema: basta con cinco registros de tamaño fijo, que se actualizan con cada byte del número (F4-B12):

- si alguna cifra de la mantisa D (parte entera y fracción) no es 0;
- z, los ceros finales de D (saturado);
- f, las cifras de la fracción (saturado);
- |e|, el exponente (saturado en 2⁶², mucho más que cualquier número de cifras alcanzable) y su signo.

El valor es D · 10^(e − f). Es entero si D = 0 o si **e + z ≥ f**: desplazar la coma e posiciones a la derecha deja en la fracción las f − e últimas cifras de D, y son todas cero si y solo si z ≥ f − e. Ejemplos: `1.0` (z = 1, f = 1, e = 0), `1e2` y `10e-1` (z = 1, e = −1) son enteros; `1.5` y `1e-1` no. La saturación no cambia la respuesta: si e satura arriba, e > f; si satura abajo, e + z < 0 ≤ f. Por eso un exponente de 10⁵ cifras se decide sin pánico ni memoria. F4.1b añadirá el registro del lexema (≤ 1 024 bytes, `00-alcance.md` §4) para las comparaciones de `minimum`… y `multipleOf`.

## 4. δ

### 4.1 Forma de las tablas

M se construye directamente como tablas (F4-U4) en el formato que lee el núcleo, `runtime_rs::Tables<'a>` (*slices* prestados, enlaces por índice `u32`, sin punteros: el mismo formato vale para los `const` de Rust y los arreglos de C de la Fase 5):

| Tabla | Contenido |
|---|---|
| `delta` | densa: `delta[q * n_classes + a]` = índice en `trans` |
| `trans` | (op, `next`, `ret`, `frame`, `bit`, comprobaciones, `outs`) |
| `checks` | (prueba, regla, costo c, `slot`, argumentos), ordenadas por c dentro de cada transición |
| `frames` | (`words`, `slots`, `gates`) |
| `masks`, `deps` | máscaras de `required` y de las listas de `dependentRequired` |
| `ascii`, `ranges` | clases de carácter |
| `rules`, `text` | regla de cada comprobación (nodo, `RuleKind`) y su ubicación recortada a 256 B (§6.5) |

Campos reservados para los combinadores de F4.5a, ya presentes: `Check::slot` (siempre 0), `Frame::slots` y `Frame::gates` (vacíos) y `Trans::outs`, el mapa de salidas de cada transición que apila (siempre la identidad). El núcleo de F4.1a no los lee.

Las partes finitas se construyen antes como AFD completos con salidas (F4-B14): `Dfa { n_classes, start, trans (por filas), out }`, con estado muerto explícito. En F4.1a lo son los tries de claves (`Dfa::trie`: un estado por prefijo y el muerto); F4.3 los minimizará con Hopcroft y F4.4b producirá los de `pattern` con la misma forma.

### 4.2 Totalidad y determinismo

δ es total por construcción: la tabla densa empieza con todas sus entradas en la transición 0, el error de sintaxis, y la construcción solo sobrescribe las que la gramática admite. Es determinista también por construcción: hay una sola entrada por (q, a), y la cima de la pila solo interviene al desapilar (el estado de retorno de X) y en las comprobaciones de la máscara. No hay transiciones ε: un `$ref` sin hermanas es un alias resuelto al compilar (§6.4). La prueba `delta_total_y_estados_alcanzables` comprueba que toda entrada es una transición válida y que todos los estados se alcanzan desde q₀.

### 4.3 Propiedad de pila visible

La operación de pila la fija la **clase** del evento, no el estado: `{` y `[` apilan (o son error), `}` y `]` desapilan (o son error), y ninguna otra clase toca la pila. Σ se parte en Σ\_llamada = {`{`, `[`}, Σ\_retorno = {`}`, `]`} y Σ\_interna = el resto: M es un autómata de pila **visible** (Alur y Madhusudan), la clase cerrada bajo intersección y complemento en la que se apoyan los productos y las puertas de F4.1b y F4.5 ([fase3/ir.md](../fase3/ir.md) §6.1). La comprueba la misma prueba sobre cada entrada de δ. Por la misma razón la altura de la pila es la profundidad de anidamiento: O(d).

## 5. Pesos y costo

Dos pesos distintos sobre las mismas transiciones (F4-U3, D4):

**w ∈ ℕ ∪ {∞}**, en el semianillo tropical (ℕ ∪ {∞}, ⊕ = mín, ⊗ = +, 0̄ = ∞, 1̄ = 0). w(t) es el número de comprobaciones de t que fallan: **1 por `Constraint` violada por instancia** (`required` con dos claves ausentes es 1). ∞ es malformado o límite y absorbe ⊗, por eso el corpus omite `violations` en los errores `lexical` y `syntax`. El peso del recorrido es ⊗ de sus transiciones; ⊕ solo aparece entre alternativas (F4.5: `anyOf` = mín de las ramas). Representación (`runtime_rs::Weight`): un `u64` con `u64::MAX` = ∞ y los finitos saturados en `u64::MAX − 1`. Saturar es el homomorfismo de truncamiento de ℕ ∪ {∞} en [0, M] ∪ {∞}, así que se conservan las leyes (lo comprueban por proptest `suma_y_producto_asociativos`, `…_conmutativos`, `producto_distribuye_sobre_la_suma` y `neutros_y_absorbente`) y muchas violaciones nunca se confunden con un malformado.

**c: `RuleKind` → ℕ**, el costo estático de evaluar una comprobación (`automaton::weight::cost`, `match` exhaustivo, F4-B6). Ordena las comprobaciones de cada transición, baratas primero; a igualdad, el orden de las reglas en el esquema.

| c | Reglas |
|---|---|
| 1 | `type`, `false`, `Lexical`; las estructurales (`$ref`, `items`, `properties`, atributos, contenido) |
| 2 | `required`, `min`/`maxProperties`, `min`/`maxItems`, `min`/`maxLength` |
| 3 | `dependentRequired` |
| 4 | `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum` |
| 5, 6 | `const`, `enum` |
| 8 | `multipleOf` |
| 10 | `pattern` |
| 12 | `uniqueItems` |
| 16 | `allOf`, `anyOf`, `oneOf`, `not`, `dependentSchemas` |

**Dos modos sobre el mismo M** (`runtime_rs::Mode`):

- **Conformidad total**: sigue tras cada violación y las suma todas, con recuperación uniforme: una clave prohibida (`additionalProperties: false`) suma 1 y su valor se recorre con el marco «cualquiera»; un `type` incumplido no suspende las demás reglas del nodo (un arreglo donde se exigía un objeto con `required` cuenta 1, el del tipo). Es el modo en que se leen `expected.violations` del corpus.
- **Fallo temprano**: se detiene en la primera transición con w > 0 e informa de su comprobación de **menor costo** (la primera que falla, porque van ordenadas): en `{"dependentRequired": {"a": ["b"]}, "required": ["c"]}` con `{"a": 1}`, `}` incumple las dos y se informa `required` (c = 2) aunque en el esquema venga después (`fallo_temprano_informa_la_de_menor_costo`).

Los dos modos pueden dar **clases de error distintas** con el mismo payload: si un payload malformado o truncado tiene una violación antes del error de sintaxis, el fallo temprano se detiene en la violación y da `schema` (no llega a ver el malformado), y la conformidad total sigue hasta el error y da `syntax` con peso ∞ (p. ej. `{"items": {"type": "string"}}` con `[1, 2, ]`). Es lo esperado del fallo temprano; F4.6 compara los dos modos sobre el corpus y lo documenta en el reporte.

El reporte (`runtime_rs::Report`, base de `sv_report_t`, F4-U18) da `valid`, `violations` (el peso), `error_kind` (`lexical`, `syntax`, `schema`, `limit`; e `internal` si las tablas fueran incoherentes) y el **primer** error: offset, línea, columna y la regla. El offset es el del **comienzo del evento** que lo detecta: `}` para `required`, la comilla de apertura de la clave repetida, el principio del valor para `type` y `false`, el principio del número para `integer`. Si hay un error de peso ∞, el primero es ese (el payload no es JSON, o no cabe), aunque antes hubiera violaciones.

## 6. Construcción por regla

`automaton::build` es iterativa (D7, F4-B9): ningún paso recurre sobre la IR ni sobre el payload, y la construcción cabe en un hilo de 256 KiB (`d7_cadena_de_ref_y_payload_profundo`).

### 6.1 Composición

Cada nodo de la IR alcanzable produce los subautómatas de su objeto, su arreglo y su número, si sus reglas los necesitan; si no, usa los de «cualquiera». Un **valor** de un nodo N con retorno r empieza con una **transición compuesta** E(N, a, r) para cada una de las 7 clases a que empiezan un valor: lleva las comprobaciones del primer evento (`type` que no admite a, `false`) y la operación que abre la instancia (apilar el marco de objeto o arreglo de N, abrir el escalar, o volver a r con `true`/`false`/`null`). Un estado que espera un valor de N (el de la raíz, el de tras `:` de una clave, el de un elemento de arreglo) tiene esas 7 entradas. Es la composición: el padre apila el subautómata del hijo, que vuelve al padre al desapilar. Los estados de todos los nodos se reservan antes de rellenar ninguna fila, así que un hijo que es un antepasado (recursión por `$ref`) ya existe cuando se apila.

### 6.2 El marco «cualquiera»

Es la gramática LL(1) de JSON de [fase2/gramatica-json.md](../fase2/gramatica-json.md) §3 como DPDA, el esquema `true`:

| Grupo | Estados | Transiciones |
|---|---|---|
| raíz | q₀ (espera el valor), fin (∈ F) | q₀: E(raíz, a, fin) para las 7 clases de valor |
| objeto | `{` primero, `{` tras `,`, clave, tras clave, tras `:`, tras valor | primero: `"` → clave, `}` → desapila; tras `,`: `"` → clave; clave: cada carácter → clave, `"` final → tras clave; tras clave: `:` → tras `:`; tras `:`: E(cualquiera, a, tras valor); tras valor: `,` → tras `,`, `}` → desapila |
| arreglo | `[` primero, `[` tras `,`, tras valor | primero: `]` → desapila, E(cualquiera, a, tras valor); tras `,`: E(cualquiera, a, tras valor); tras valor: `,` → tras `,`, `]` → desapila |
| escalares | cadena, número | cadena: cada carácter → cadena, `"` final → cierra; número: byte → número, fin → cierra |

Las producciones `members_tail → , pair members_tail | ε` y `elements_tail` son los estados «tras valor» (`,` o el cierre) y «tras `,`» (que no admite el cierre: `[1,]` es error en el `]`). La diferencia entre «primero» y «tras `,`» es la que hay entre `members` (que puede ser ε) y `pair`.

### 6.3 Las reglas de F4.1a

| Regla | Subautómata |
|---|---|
| `true`, `{}` (sin reglas) | «cualquiera» |
| `false` (`Reject`) | comprobación que falla siempre en E(N, a, r), para las 7 clases; el valor se recorre con «cualquiera» |
| `type` | comprobación que falla en E(N, a, r) si el tipo de a no está en el conjunto. Solo un número queda sin decidir si el conjunto tiene `integer` y no `number`: N tiene entonces su propio estado de número, cuyo cierre comprueba «entero» con los registros del número (§3.3) |
| `$ref` sin hermanas | alias (§6.4) |
| `properties`, `additionalProperties`, `required`, `dependentRequired` | un marco de objeto propio: «primero», «tras `,`», un estado por estado del **trie** de los nombres (§3.1) más «clave desconocida» (el muerto del trie), dos por nombre (tras la clave, tras `:`) y dos para la clave desconocida, y «tras valor». Al cerrar la clave en el estado del nombre j se fija su bit; tras `:` se esperan valores del destino de j (el de `properties`, o el de `additionalProperties` si j solo está en `required`/`dependentRequired`, o «cualquiera»); la clave desconocida, del de `additionalProperties` o «cualquiera». `}` desapila con las comprobaciones de `required` y `dependentRequired` |
| `prefixItems`, `items` | un marco de arreglo propio con el índice en el estado: «primero» espera el elemento 0; «tras valor i» va con `,` a «valor min(i + 1, p)», con p = número de `prefixItems`; el elemento i < p es del nodo `prefixItems[i]` y los demás del de `items` (o «cualquiera») |

Dos nodos distintos con las mismas reglas producen subautómatas distintos; los une la minimización de F4.3. Lo que no está en la tabla se rechaza en compilación (§6.6).

### 6.4 `$ref` y E₌

Un nodo cuya única regla es `Ref(t)` es un **alias** de t (F4-U2): antes de construir, cada nodo se resuelve a su destino final (memorizado, una vez por nodo: una cadena de 10⁵ `$ref` es O(n)), o a «cualquiera» si no tiene reglas. No hay llamadas ε ni copias: el destino tiene un solo subautómata, y todas las aristas que llegan a él lo comparten. Es la opción «expandir E₌ memorizado» de [fase3/ir.md](../fase3/ir.md) §7, que cierra [fase3/pendientes.md](../fase3/pendientes.md) §27: la instancia de un nodo la lee un solo marco, el de su conjunto canónico de `NodeId`. En F4.1a ese conjunto tiene como mucho un nodo; F4.1b lo generaliza al cierre conjuntivo por `$ref` con hermanas y `allOf` (producto con tope, componentes = `NodeId` distintos, S-3).

### 6.5 Ubicación recortada

`automaton::location_clipped(&VerifiedIr, id, max)` (F4-B8, cierra [fase3/pendientes.md](../fase3/pendientes.md) §4) es `VerifiedIr::location` recortada a como mucho `max` bytes (mínimo 16): si no cabe, su principio, `…` y su final. Sube por los padres (O(profundidad)) y solo escribe los caracteres que caben, así que cuesta O(profundidad + max) aunque las claves midan megabytes; nunca parte un carácter ni un escape `~0`/`~1`. Las tablas guardan la de cada nodo con reglas comprobadas, con `max` = 256 (F4-U6), y la usan los mensajes de `Pending` y del reporte. `location` sin recortar queda para pruebas y depuración.

### 6.6 Lo no implementado

Antes de construir nada, `build` recorre todo lo alcanzable desde la raíz (por todas las aristas; lo inalcanzable no cuenta) y, si alguna regla no se compila todavía, devuelve `BuildError::Pending { node, rule, task, location }` con la tarea **más tardía** de las que encuentra (la que fija cuándo compilará el esquema; a igualdad, la primera en anchura). `schemac automaton` la escribe como «estrategia pendiente: pattern (F4.4b) en #/properties/x» y sale con 1 (F4-U5):

| Regla | Tarea |
|---|---|
| `minimum`…`exclusiveMaximum`, `multipleOf`, `min`/`maxLength`, `min`/`maxItems`, `min`/`maxProperties`, `enum`/`const` escalares, `$ref` con hermanas, `allOf` | F4.1b |
| XML (cualquier IR de XSD) | F4.1c |
| `pattern` | F4.4b |
| `anyOf`, `oneOf`, `not`, `dependentSchemas` | F4.5a |
| `uniqueItems`, `enum`/`const` con un literal compuesto | F4.5b |

Los cuatro casos `non-dpda` del corpus dan `Pending` de F4.4b o F4.5 (`non_dpda_da_pending`); `non-dpda-allof-not-01` da `not` (F4.5a) y no `allOf` (F4.1b), por la regla de la tarea más tardía.

Además, `BuildError::Limit` si un objeto tiene más de 4 096 nombres o si |Q| · |Σ| (con una cota superior de |Q| contada **antes** de reservar δ) pasa de 2²⁰ entradas (F4-U6), y `BuildError::Internal` como defensa en profundidad (F4-B11; F4.1a no lo produce).

## 7. Complejidad

Sea n el tamaño de la IR alcanzable, L la suma de las longitudes de los nombres con bit y k = |Σ|.

- **Construcción**: |Q| ≤ 13 + Σ_N (L_N + 2·nombres_N + 7) sobre los nodos con reglas de objeto + Σ_N (2·p_N + 3) sobre los de arreglo + 1 por nodo con estado de número propio (es la cota que se comprueba antes de reservar); |δ| = |Q| · k, acotado por 2²⁰. Tiempo y memoria O(|Q| · k + n); la resolución de alias y los recorridos son O(n).
- **Validación**: O(1) por evento respecto del payload (una consulta a δ, las comprobaciones de la transición, acotadas por el esquema, una operación de pila de O(palabras del marco)); con el lexer, O(n) en bytes. Pila: O(d) marcos de tamaño fijo. Cero reservas: el núcleo usa la memoria del llamador (`cero_reservas_al_validar`, con un asignador contador en un binario de prueba aparte, F4-B25) y es `#![no_std]` sin `alloc`; ningún bucle recurre, así que un payload de 10⁵ niveles se recorre en un hilo de 256 KiB.

## 8. La dorada, contada a mano

Esquema `tests/schemas/parse-ok/objeto-cerrado.json`:

```json
{"type":"object","properties":{"a":{"type":"integer"}},"required":["a"],"additionalProperties":false}
```

IR: nodo 0 (`type: object`, `properties` {a → 1} con `additional` = 2, `required` [a]), nodo 1 (`type: integer`), nodo 2 (`false`). Nombres con bit del nodo 0: [a] (un bit, una palabra de máscara).

**Clases**: las 14 fijas, «otro» y `'a'`: |Σ| = **16**.

**Estados** (24) y **transiciones** (entradas de δ distintas del error, 77):

| Grupo | Estados | Transiciones por estado | Total |
|---|---|---|---|
| raíz | q₀, fin | q₀: 7 clases de valor; fin: 0 | 2 / 7 |
| cualquiera: objeto | primero, tras `,`, clave, tras clave, tras `:`, tras valor | 2 (`"`, `}`) + 1 (`"`) + 3 (otro, `'a'`, `"` final) + 1 (`:`) + 7 + 2 (`,`, `}`) | 6 / 16 |
| cualquiera: arreglo | primero, tras `,`, tras valor 0 | 8 (`]` y 7 de valor) + 7 + 2 (`,`, `]`) | 3 / 17 |
| cualquiera: escalares | cadena, número | 3 (otro, `'a'`, `"` final) + 2 (byte, fin) | 2 / 5 |
| objeto `#` (nodo 0) | primero, tras `,`, clave «», clave «a», clave desconocida, tras «a», valor «a», tras clave desconocida, valor desconocido, tras valor | 2 + 1 + 3 + 3 + 3 + 1 + 7 + 1 + 7 + 2 | 10 / 30 |
| número `#/properties/a` (nodo 1) | entero | 2 (byte, fin con «entero») | 1 / 2 |
| **Total** | | | **24 / 77** |

Detalle de la clave del nodo 0: el trie de [a] tiene tres estados (ε, «a» y el muerto, que es «clave desconocida»); cada uno tiene una entrada por cada clase de carácter (2) y la del cierre de la clave (1): ε va con `'a'` a «a», con «otro» al muerto y con `"` a «tras clave desconocida»; «a» va con cualquier carácter al muerto y con `"` a «tras «a»» fijando el bit 0. Los tipos de marco son 3 (objeto y arreglo de «cualquiera», sin registros, y el objeto del nodo 0 con una palabra): el marco máximo ocupa 16 bytes.

Comprobaciones: las 7 entradas de q₀ salvo `{` llevan «`type` ✗» del nodo 0; las de «valor «a»» salvo la del número, «`type` ✗» del nodo 1, y la del número va al estado «entero»; las 7 de «valor desconocido», «`false` ✗» del nodo 2; los dos `}` del nodo 0, «`required`: máscara». La prueba `dorada_contada_a_mano` comprueba estos números grupo a grupo; `schemac automaton` da el resumen versionado en `tests/automata/objeto-cerrado.json` y `schemac automaton --dot`, el grafo [automata-objeto-cerrado.dot](automata-objeto-cerrado.dot) ([SVG](automata-objeto-cerrado.svg)), que generan `make docs` y comprueba `make docs-check`.

## 9. Decisiones de esta tarea

Las de F4-U1…U19 están en el plan (D47, D51). Las que tomó el Builder (F4-B), candidatas al registro:

1. **F4-B1.** `semantic` pasa a dependencia de pruebas de `automaton`; `automaton` depende de `ir`, `lexer` y `runtime-rs` (desde F4.1a, porque construye `runtime_rs::Tables`). `runtime-rs` depende solo de `lexer` (ambos `no_std` sin `alloc`).
2. **Escalar no apilado** (precisa F4-B5): solo objetos y arreglos se apilan; el escalar abierto vive en un registro global (§1, §3.2). Da la propiedad de pila visible (§4.3) y una altura de pila igual a la profundidad. Consecuencia para F4.5a (F4-U10, F4-B20, F4-U11): donde D51 dice que las salidas viven «en el marco» y las puertas se evalúan «al desapilar», «desapilar» se generaliza a **cerrar la instancia**, que es `Pop` en un objeto o arreglo, `Leave` en una cadena o un número, y la propia transición de entrada en un valor de un solo evento (`true`, `false`, `null`); las salidas de una instancia escalar viven en una zona global de tamaño máximo por esquema, junto al registro del escalar abierto ([pendientes.md](pendientes.md) §9).
3. **Entero sin lexema** (precisa F4-B12): cinco registros saturados (§3.3), exactos con exponentes de cualquier longitud; el lexema queda para F4.1b.
4. **Transición compuesta E(N, a, r)** para empezar un valor (§6.1): una sola transición de M por evento, sin redespacho.
5. **Pending de la tarea más tardía** entre las reglas alcanzables (§6.6).
6. **Presupuesto F4-U6 sobre |Q| · |Σ|** con una cota superior de |Q| calculada antes de reservar.
7. **Peso y reporte**: `Weight` finito saturado en `u64::MAX − 1` (§5); el primer error es el de peso ∞ si lo hay; el offset es el del comienzo del evento que detecta el error; `ErrorKind::Internal` para tablas incoherentes (no está en F4-U18).
8. **F4-B3 en F4.1a**: una sola partición de caracteres (la de los nombres) para claves y valores.
9. **F4-B6**: la tabla de costos de §5.
10. **F4-B25**: el contador de reservas es el único `#[allow(unsafe_code)]` del workspace, en un binario de prueba (`GlobalAlloc` es un rasgo `unsafe`). **Decidido por el usuario el 16 nov (opción A):** es la única excepción nombrada del invariante `unsafe_code = "deny"`, y `make lint` la vigila con el objetivo `unsafe-check` del `Makefile`, que falla ante cualquier otro `unsafe` ([pendientes.md](pendientes.md) §8).
11. **Salida de `schemac automaton`** (F4-U7): JSON con `automatonVersion` 1, `format`, `states`, `transitions`, `frames`, `classes`, `registerBytes`, `maxFrameBytes`, `products` y `pending` (estos dos, 0 hasta F4.1b y F4.5); códigos 0, 1 (esquema, pendiente o límite), 2 (lectura, argumentos) y 3 (interno).

## 10. Pruebas

- `crates/automaton/tests/json.rs`: el marco «cualquiera» con `true` (bien formados con w = 0; `{"a" 1}`, `[1,]`, `{,}`, `]`, `1 2`, `{"a":1}}` y la entrada vacía con ∞ en su offset), la dorada contada a mano, `type` e `integer` (con exponentes de 10⁵ cifras), `false`, `properties`/`additionalProperties` (con escapes y caracteres no ASCII en las claves), claves declaradas repetidas (∞), `required`/`dependentRequired` (también con máscaras de más de 64 bits), fallo temprano, `prefixItems`/`items`, alias, `Pending` con su tarea, δ total y pila visible, la pila llena (`limit`) y D7. Todo payload se valida con bloques de 1, 7 y 4096 bytes y se exige el mismo reporte.
- `crates/automaton/tests/corpus.rs`: el corpus de `tests/cases` (§6.6 y criterios 1-2 de F4.1a).
- `crates/automaton/tests/weight.rs`: leyes del semianillo por proptest y la tabla de costo.
- `crates/automaton/tests/partes.rs`: el trie como `Dfa` completo y `location_clipped`.
- `crates/automaton/tests/sin_reservas.rs`: 0 reservas en el camino de validación.
- `crates/runtime-rs/src/core.rs` (unitarias): el entero sin lexema y la saturación del exponente.
- `crates/cli/tests/automaton.rs`: `schemac automaton`, la dorada y el `.dot` versionados y los códigos de salida.
