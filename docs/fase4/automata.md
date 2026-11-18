# Fase 4 — El autómata de pila ponderado M

Primeras tareas de la Fase 4 (D47 y D51 del plan). F4.1a: la definición formal de M, los pesos en el semianillo tropical, el núcleo `no_std` que lo ejecuta y el esqueleto de la construcción para JSON Schema. F4.1b: los registros (contadores de miembros, elementos y puntos de código, y el lexema del número), los escalares (`enum`/`const`, rangos y `multipleOf` exactos) y la conjunción por E₌ (marcos por conjunto canónico de `NodeId`: `$ref` con hermanas y `allOf` como producto, con tope). Las tareas siguientes amplían la misma construcción sin cambiar su forma: F4.1c (XML), F4.3 (determinismo y Hopcroft), F4.4 (`pattern`), F4.5 (combinadores) y F4.6 (límites, reporte y capa de buena formación del ejecutor).

Implementación:

- [`crates/automaton`](../../crates/automaton/src/lib.rs): `build(&VerifiedIr) -> Result<Automaton, BuildError>` y `build_with` con el tope de productos ([`json.rs`](../../crates/automaton/src/json.rs)), el cierre conjuntivo ([`conj.rs`](../../crates/automaton/src/conj.rs)), el modelo como tablas ([`model.rs`](../../crates/automaton/src/model.rs)), los pesos y el costo ([`weight.rs`](../../crates/automaton/src/weight.rs)), los AFD de las partes finitas ([`dfa.rs`](../../crates/automaton/src/dfa.rs)), `location_clipped` ([`location.rs`](../../crates/automaton/src/location.rs)) y las salidas de `schemac automaton` ([`export.rs`](../../crates/automaton/src/export.rs)).
- [`crates/runtime-rs`](../../crates/runtime-rs/src/lib.rs): el formato de las tablas ([`tables.rs`](../../crates/runtime-rs/src/tables.rs)), el núcleo ([`core.rs`](../../crates/runtime-rs/src/core.rs)), las rutinas numéricas exactas ([`num.rs`](../../crates/runtime-rs/src/num.rs)), el lexer delante ([`json.rs`](../../crates/runtime-rs/src/json.rs)) y el semianillo ([`weight.rs`](../../crates/runtime-rs/src/weight.rs)). Es `#![no_std]` sin `alloc` y solo depende de `lexer`.
- `schemac automaton [--dot] <esquema>`: resumen de tamaños en JSON o M completo en DOT (F4-U7).

Dependencias (F4-B1, D13): `automaton` depende de `ir`, `lexer` y `runtime-rs`; `semantic`, `schema-parser`, `xsd` y `baseline` solo son dependencias de pruebas (antes `automaton` dependía de `semantic`, en contra de D13).

```
lexer ─▶ runtime-rs (núcleo no_std) ─┐
  │                                   ├─▶ automaton ─▶ codegen (Fase 5)
  └────▶ ir ──────────────────────────┘
```

## 1. Definición formal

M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) con

- **Q**: estados de control. Cada estado pertenece a un **grupo**: el subautómata de un conjunto canónico de nodos de la IR (su objeto, su arreglo, su número o su cadena; §6.4) o uno de los de «cualquiera». Son los clústeres del DOT.
- **Σ**: clases de **eventos del lexer** (§2), no bytes.
- **Γ**: marcos. Un símbolo de pila es X = (tipo de marco, estado de retorno, registros). El tipo de marco es el de un objeto o arreglo de un conjunto canónico de nodos (o de «cualquiera»); los registros, su máscara de claves y su contador (§3). Z₀ es la pila vacía: la raíz no se apila.
- **δ**: sobre el **control extendido** (q, r, ρ) (abajo): (Q × R × P) × Σ × Γ → Q × Op × (actualización de ρ y de la máscara de la cima) × (comprobaciones), con Op ∈ {mover, apilar(X), desapilar, abrir escalar, cerrar escalar, error}. La entrada de la tabla densa solo depende de (q, a); r interviene en «cerrar escalar» (el estado al que se vuelve) y ρ y la máscara de X en las comprobaciones (`integer` lee los registros del número, `required` la máscara). Es **total**: toda entrada que la gramática de JSON no admite va a la transición de error, de peso ∞.
- **q₀**: el estado inicial de la raíz; **F** = {fin de la raíz}, con la pila vacía y sin escalar abierto.
- **w**: δ → ℕ ∪ {∞} (D4): el número de comprobaciones de la transición que fallan, o ∞ en la de error (§5).

La configuración es (q, r, ρ, γ): el estado, el registro del escalar abierto (su estado de retorno, los registros del número, su lexema y el contador de puntos de código de la cadena), los registros globales y la pila γ ∈ Γ\*. El peso de un recorrido es el producto ⊗ (la suma) de los pesos de sus transiciones; el payload es conforme si llega a F con peso 0.

Por qué el escalar no se apila: en JSON hay a lo sumo un escalar abierto a la vez (una cadena o un número no contienen otros valores), así que su estado de retorno es **un** registro finito, no una pila. Formalmente el control es el par (q, r) con r ∈ R = Q: el producto de dos conjuntos finitos, que sigue siendo finito. A cambio, la pila solo crece con `{` y `[` (§4.3) y su altura es la profundidad de anidamiento de objetos y arreglos.

**Los registros son finitos por la saturación.** P, el conjunto de valores de ρ, es finito porque cada registro está acotado: los del número (§3.3) son `u64` saturados y el exponente se satura en 2⁶²; el lexema guardado tiene como mucho 1 024 bytes (§3.4); los contadores de miembros, elementos y puntos de código son `u64` saturados; las máscaras tienen un número fijo de bits por esquema. Así (q, r, ρ) recorre un conjunto finito y M es un autómata de pila en sentido estricto, con la pila como única memoria no acotada. La saturación no es un detalle: sin ella, «el número es entero» no es un lenguaje regular sobre los bytes del lexema (compara un exponente escrito en decimal con el recuento unario de las cifras de la fracción y de los ceros finales). Con ella la respuesta es exacta mientras esos recuentos no saturen, es decir, para payloads de menos de 2⁶² bytes (unos 4,6 EB; el offset de `Pos` es `u64`, D9), y la saturación del exponente no cambia el resultado (§3.3).

## 2. Σ y la pasada única

El lexer de la Fase 1 (`lexer::json::JsonLexer`, un AFD de Mealy `no_std`) va **delante** de M como un transductor finito determinista: lee bytes y emite eventos. Σ son las clases de esos eventos (F4-U1):

| Clase | Evento |
|---|---|
| 0–8 | `{` `}` `[` `]` `:` `,` `true` `false` `null` |
| 9, 10 | apertura y cierre de una cadena |
| 11, 12, 13 | inicio de un número, un byte del lexema (la parte va a los registros del número, no a δ), fin del número |
| 14 + k | un carácter de una cadena (ya sin escapes ni UTF-8) de la clase k |

Las clases de carácter se calculan **por esquema** (F4-B3): cada punto de código que aparece en algún nombre con bit de un objeto (§3.1) o en alguna cadena de un `enum`/`const` (§6.3) tiene su clase, y todos los demás comparten la clase 0 («otro»). Así |Σ| = 14 + 1 + (puntos de código distintos de los nombres y de las cadenas literales). La clase de un carácter sale de una tabla densa de 128 entradas en ASCII y de una búsqueda binaria en los rangos ordenados del resto (O(log k), con k fijo por esquema). Claves y valores comparten **una sola partición**, la común más fina (decisión de F4.1b, [pendientes.md](pendientes.md) §3): |Σ| crece con la **suma** de los alfabetos de nombres y literales, nunca con su producto, y el estado ya distingue si se lee una clave o un valor. F4.4b decidirá si los AFD de `pattern` necesitan tablas por contexto.

**Una pasada.** Cada byte entra una vez al lexer, que hace O(1) por byte, y emite como mucho un número constante de eventos por byte; cada evento entra una vez a δ (§4), que hace O(1) respecto del payload. La composición del transductor del lexer con M es otro DPDA: su estado es el par (estado del lexer, estado de M) y cada byte dispara una transición del lexer seguida de las de M sobre los eventos emitidos, todas deterministas.

Un byte puede emitir **más de un** evento: el byte que termina un número también es el siguiente token (`1}` da `NumberEnd` y después `EndObject`; `1,` da `NumberEnd` y `Comma`), y el último byte de un literal emite el literal. La composición sigue siendo un DPDA porque, de los eventos que emite un byte, **como mucho uno toca la pila**: `NumberEnd` (y los literales `true`/`false`/`null`) son internos para la pila (§4.3), y el evento estructural que los sigue es uno solo. El movimiento compuesto por un byte es, por tanto, una secuencia acotada (de longitud fija) de transiciones deterministas de M con a lo sumo una operación de pila: se puede leer como una sola transición del DPDA compuesto que lee el byte, cambia el control y hace esa operación de pila, o, equivalentemente, como una transición que lee el byte seguida de un número acotado de ε-transiciones deterministas (sin conflicto con las que leen, porque el lexer decide cuáles se disparan). No se pliega en un solo autómata (multiplicaría los estados de M por los 32 del lexer): se deja como composición, que es lo que harán también los emisores de la Fase 5 (dos niveles de tablas densas, F4-U1). Como el lexer y M guardan todo su estado entre bloques, el resultado no depende de cómo se parta la entrada (pruebas con bloques de 1, 7 y 4096 bytes).

## 3. Γ y registros

### 3.1 Marcos

Un objeto o un arreglo del payload es una instancia que **se apila** con su primer evento (`{`, `[`) y se desapila con el último (`}`, `]`) (F4-B5). En memoria cada marco son sus registros seguidos de una cabecera `(estado de retorno << 32) | tipo de marco` (una palabra de 64 bits). El tipo fija cuántas palabras de registros tiene (`Frame::words`), así que el marco de tamaño fijo es el de la mayor disposición: `sv_state_t` = zona escalar (§3.6) + profundidad máxima × marco máximo, de tamaño fijo por esquema.

**Disposición de los registros** (F4.1b, F4.1a-CR-S1): `[máscara (Frame::mask_words palabras)] [contador (0 o 1 palabra)]`. La máscara va primero y el contador, si el tipo de marco lo tiene, en la palabra `mask_words`. `Test::Mask`, `Test::Dependent` y la fijación de bits solo leen las `mask_words` primeras palabras, así que nunca confunden el contador con nombres (`mascara_separada_del_contador`, también con 70 nombres: dos palabras de máscara y una de contador).

Registros de un marco de objeto (F4-B4, S-1 de [fase3/pendientes.md](../fase3/pendientes.md) §5):

- **Máscara de nombres**: un bit por cada nombre de `properties` ∪ `required` ∪ claves y listas de `dependentRequired`, en ese orden y sin repetir. Como mucho 4 096 nombres por objeto (F4-U6; 64 palabras, 512 B). Al cerrar la clave de un nombre se fija su bit; si ya estaba, la clave declarada está **repetida** y es un error de peso ∞ (D2). Al cerrar el objeto, `required` es una comprobación «la máscara contiene la de los requeridos», y `dependentRequired` (D5), «para cada propiedad cuyo bit está, la máscara contiene la de su lista».
- Las claves **no declaradas** repetidas no tienen bit: las detecta la capa de buena formación del ejecutor con una tabla acotada por `max_claves_adicionales` (F4-B7, F4-U16), en F4.6.

- **Contador de miembros** (`minProperties`/`maxProperties`, F4.1b): un `u64` saturado que suma 1 la transición que abre cada clave (`Trans::count`, la `"` desde «primero» o «tras `,`»); al cerrar el objeto, `Test::CountMin`/`CountMax` lo comparan con la cota (guardada en `Check::arg` | `Check::len` << 32). Solo existe si algún componente del marco tiene esas reglas: la dorada no cambia.

Los arreglos: el índice dentro de `prefixItems` está en el estado (§6.3), saturado en el mayor `prefixItems` de los componentes. El **contador de elementos** (`minItems`/`maxItems`) es la única palabra de registros del marco de arreglo: lo suma la transición compuesta que empieza cada elemento (E(N, a, r) con `count`, **antes** de apilar el marco del elemento, así que suma en el arreglo y no en el hijo), y se comprueba al desapilar con las mismas pruebas.

### 3.2 El escalar abierto

Una cadena o un número **no** se apilan (§1): `abrir escalar` guarda el estado de retorno en el registro global del escalar y va al subautómata del escalar; `cerrar escalar` vuelve a ese estado. `true`, `false` y `null` son un solo evento: su transición comprueba y vuelve directamente.

### 3.3 Registros del número

Para `type: integer` no hace falta guardar el lexema: basta con cinco registros de tamaño fijo (en [`num.rs`](../../crates/runtime-rs/src/num.rs), `Number`), que se actualizan con cada byte del número (F4-B12):

- si alguna cifra de la mantisa D (parte entera y fracción) no es 0;
- z, los ceros finales de D (saturado);
- f, las cifras de la fracción (saturado);
- |e|, el exponente (saturado en 2⁶², mucho más que cualquier número de cifras alcanzable) y su signo.

El valor es D · 10^(e − f). Es entero si D = 0 o si **e + z ≥ f**: desplazar la coma e posiciones a la derecha deja en la fracción las f − e últimas cifras de D, y son todas cero si y solo si z ≥ f − e. Ejemplos: `1.0` (z = 1, f = 1, e = 0), `1e2` y `10e-1` (z = 1, e = −1) son enteros; `1.5` y `1e-1` no. La saturación no cambia la respuesta: si e satura arriba, e > f; si satura abajo, e + z < 0 ≤ f. Por eso un exponente de 10⁵ cifras se decide sin pánico ni memoria. Un proptest los compara con un oráculo hecho con `ir::num::compare` (F4.1a-CR-S10, `entero_coincide_con_ir_num`): x ≠ 0 con n cifras significativas es 0.D · 10^E, y es entero ⇔ E ≥ n ⇔ |x| ≥ 10ⁿ⁻¹, una comparación que `ir::num` hace con exponentes de cualquier longitud.

### 3.4 El lexema del número y la comparación exacta

`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` y los números de `enum`/`const` comparan **por valor** sobre el lexema (`00-alcance.md` §1.1, D16), nunca sobre un `f64`. El núcleo guarda el lexema del número abierto en un **registro global** de `LEXEME_MAX` = 1 024 bytes (`00-alcance.md` §4; F4-B4), dentro de `Core`: tamaño fijo y sin reservas. Cada byte del número se copia mientras cabe y la longitud se cuenta saturada. Si una comprobación necesita el lexema y no cupo, el error es `limit` (∞) en el comienzo del número (`lexema_largo_da_limit_solo_si_hace_falta`); `integer` no lo necesita (§3.3), así que un número largo sin otras comprobaciones sigue sin límite (aplicar el límite a todo número es de F4.6, [pendientes.md](pendientes.md) §21).

Las cotas, los divisores y los valores del esquema se guardan como sus lexemas (`Tables::nums`, `lexemes`), y la comparación es `runtime_rs::num::compare(a, b)`, una rutina `no_std` sobre **dos lexemas** con el mismo orden total que `ir::num::compare`:

- Cada lexema se lee como (−1)^s · 0.D · 10^E, con D las cifras de la mantisa desde la primera no nula y E = exp + |parte entera| − ceros a la izquierda. El cero (todas las cifras nulas, con cualquier signo y exponente) es 0: `-0` = `0`.
- Se comparan el signo; si coincide y no es 0, E (a mayor E, mayor magnitud, porque D empieza por una cifra no nula); y si E coincide, D como fracción, rellenando con ceros (así los ceros finales no cuentan y un prefijo es menor). Si el signo es negativo, se invierte.
- El exponente escrito puede tener **cualquier longitud** (el del esquema, hasta 16 MiB). Hace falta el signo de E_a − E_b = (exp_a − exp_b) + (k_a − k_b), con k = |parte entera| − ceros a la izquierda, acotado por la longitud del lexema. `exp_a − exp_b` se calcula exacta si su valor absoluto es menor que 2¹⁰⁰: con los dos de hasta 31 cifras, en `i128`; si no, con signos distintos el resultado pasa de 10³¹ > 2¹⁰⁰, y con el mismo signo se restan las magnitudes cifra a cifra desde la menos significativa (sin buffer) y, si queda una cifra no nula a partir de la 31.ª, pasa de 2¹⁰⁰. Por encima se **satura** en ±2¹⁰⁰ con su signo. Como |k_a − k_b| < 2⁶⁴ ≪ 2¹⁰⁰, saturar no cambia el signo de la suma: la comparación es exacta para cualquier par de lexemas.

Cuesta O(|a| + |b|), sin memoria dinámica. Un `enum` de números se guarda **ordenado por valor y sin repetir** (con `ir::num::compare` en compilación: `1`, `1.0` y `1e0` son uno) y se busca por bisección: O(log n) comparaciones. Lo comprueban los proptest `comparador_coincide_con_ir_num` (lexemas de RFC 8259 con exponentes de hasta 40 cifras, con ceros a la izquierda y negativos) y `comparador_con_exponentes_cercanos` (exponentes de 20 a 40 cifras que difieren en la última, que ejercitan la resta exacta), y `comparador_casos_frontera` (exponentes de 10⁴ cifras).

**NaN e INF.** JSON no los tiene. En XML (F4.1c) cada cota IEEE de `xs:float`/`xs:double` se compila a su umbral decimal exacto (5.5-CR-S1) y el comparador sigue siendo este.

### 3.5 `multipleOf` exacto

`runtime_rs::num::is_multiple(x, d, div)` (F4-B12). Con x = Dₓ · 10^eₓ y d = D_d · 10^e_d, donde D son las cifras significativas **sin ceros finales** (así que ningún D es múltiplo de 10), y k = eₓ − e_d (la diferencia exacta o saturada de §3.4):

- si x = 0, es múltiplo;
- si k < 0, x/d = Dₓ / (D_d · 10^|k|) no es entero, porque haría falta 10 | Dₓ;
- si k ≥ 0, x/d = Dₓ · 10^k / D_d. En compilación D_d se factoriza como 2ᵃ · 5ᵇ · M′ con M′ coprimo con 10 (`Divisor::of`). Como 10^k es coprimo con M′, x/d es entero ⇔ **M′ | Dₓ, 2^(a−k) | Dₓ y 5^(b−k) | Dₓ** (un exponente negativo cuenta como 0).

Los tres restos se calculan por Horner sobre las cifras de Dₓ: módulo M′ en `u64` (M′ < 2⁶⁰, así que r · 10 + 9 < 2⁶⁴), módulo 2⁶⁴ con la aritmética modular de `u64` (basta para 2^(a−k) con a ≤ 63) y módulo 5²⁷ en `u128` (5²⁷ < 2⁶³). Es exacto con exponentes de cualquier longitud en x y en d: `1e<10⁴ nueves>` es múltiplo de `0.01` y `1e-<10⁴ nueves>` no; con d = `1e<10⁴ nueves>`, `1e100` no lo es. O(|x| + |d|). Lo comprueban la tabla `tabla_de_multiple_of` (contra el oráculo racional hecho a mano: `0.01` con `0.1`, `0.10`, `1e-2` y `0.005`; `3` con `1e2` y `3e2`; `2.5`; `0.0625`; exponentes de 10⁴ cifras) y el proptest `multiplo_coincide_con_el_oraculo` (contra la división exacta en `u128`). En el autómata la payload pasa además por el lexema de §3.4 (≤ 1 024 bytes).

**Tope del divisor** (F4-U6, precisado): la parte coprima con 10 debe ser < 2⁶⁰ y, además, a ≤ 63 y b ≤ 27, para que cada factor quepa en un registro de 64 bits. Un divisor fuera (o con más de 38 cifras significativas) da `BuildError::Limit` con la ubicación de la regla (`divisor_por_encima_del_tope_da_limit`: 2⁶¹ − 1, primo, da 61 bits; 2⁶⁰ − 93 compila). La precisión sobre a y b es nueva y se propone a las dos personas (§9).

### 3.6 Escalares: contador de longitud y zona de salidas

El **contador de puntos de código** de la cadena abierta es otro registro global (`u64` saturado): se pone a 0 con la `"` de apertura y suma 1 con cada evento de carácter, que el lexer ya da **sin escapes ni UTF-8** (`"😀"`, `"\ud83d\ude00"`, `"ñ"` y `"\u00f1"` miden 1). Al cerrar la cadena, `Test::LengthMin`/`LengthMax` lo comparan con la cota. Las claves también lo mueven, pero solo lo leen las comprobaciones del cierre de un valor.

**Salidas de una instancia escalar** (diseño para F4.5a, F4.1a-CR-I1). Con el escalar no apilado (§1), una cadena o un número no tienen marco, y `true`/`false`/`null` son una sola transición. Las salidas por rama de F4-U10 (los contadores de violaciones de cada rama de `anyOf`/`oneOf`/`not`) de la instancia escalar abierta viven en una **zona global** de `Tables::scalar_slots` palabras al principio de la memoria del llamador, delante de la pila: de tamaño máximo por esquema, porque hay a lo sumo un escalar abierto (0 palabras hasta F4.5a). Las puertas se evalúan al **cerrar la instancia**, que generaliza «desapilar» a tres transiciones: `Pop` (objeto o arreglo), `Leave` (cadena o número) y la propia transición compuesta de entrada en un valor de un solo evento (`true`, `false`, `null`). La transición que abre el escalar (`Enter`) llevará en `Trans::outs` el mapa de salidas hacia la zona, como la que apila lo lleva hacia el marco. El núcleo de F4.1b ya reserva la zona (`Core::new` empieza la pila en la palabra `scalar_slots`) y no la lee; dentro de un producto, cada comprobación escalar ya va atribuida a su componente (su regla, §6.4), que es lo que F4.5a convertirá en la salida de su rama.

## 4. δ

### 4.1 Forma de las tablas

M se construye directamente como tablas (F4-U4) en el formato que lee el núcleo, `runtime_rs::Tables<'a>` (*slices* prestados, enlaces por índice `u32`, sin punteros: el mismo formato vale para los `const` de Rust y los arreglos de C de la Fase 5):

| Tabla | Contenido |
|---|---|
| `delta` | densa: `delta[q * n_classes + a]` = índice en `trans` |
| `trans` | (op, `next`, `ret`, `frame`, `bit`, `count`, comprobaciones, `outs`) |
| `checks` | (prueba, regla, costo c, `slot`, argumentos), ordenadas por c dentro de cada transición |
| `frames` | (`words`, `mask_words`, `slots`, `gates`) |
| `masks`, `deps` | máscaras de `required` y de las listas de `dependentRequired` |
| `nums`, `lexemes` | lexemas del esquema: cotas, valores de `enum`/`const` (ordenados) y divisores |
| `divisors` | (lexema, M′, a, b) de cada `multipleOf` (§3.5) |
| `scalar_slots` | palabras de la zona global de salidas del escalar (§3.6; 0) |
| `ascii`, `ranges` | clases de carácter |
| `rules`, `text` | regla de cada comprobación (nodo, `RuleKind`) y su ubicación recortada a 256 B (§6.5) |

Campos reservados para los combinadores de F4.5a, ya presentes: `Check::slot` (siempre 0), `Frame::slots` y `Frame::gates` (vacíos), `Trans::outs`, el mapa de salidas de cada transición que apila o abre un escalar (siempre la identidad), y `Tables::scalar_slots` (0). El núcleo no los lee.

Pruebas de las comprobaciones (`Test`): `Fails` (decidida al compilar), `Integer`, `Mask`, `Dependent` (F4.1a); `CountMin`/`CountMax` (contador del marco de la cima), `LengthMin`/`LengthMax` (contador global de la cadena), `NumMin`, `NumExMin`, `NumMax`, `NumExMax` (§3.4), `NumEnum` (bisección en los valores ordenados) y `MultipleOf` (§3.5), de F4.1b. Las numéricas devuelven `limit` si el lexema no cupo; todas, `internal` si las tablas son incoherentes (nunca un pánico).

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

`automaton::build` es iterativa (D7, F4-B9): ningún paso recurre sobre la IR ni sobre el payload (los cierres son DFS con pila explícita y los conjuntos se descubren con una lista de trabajo), y la construcción cabe en un hilo de 256 KiB (`d7_cadena_de_ref_y_payload_profundo`).

### 6.1 Composición

Cada **conjunto canónico** N de nodos alcanzable (§6.4; en un esquema sin `$ref` con hermanas ni `allOf`, un solo nodo) produce los subautómatas de su objeto, su arreglo, su número y su cadena, si las reglas de sus componentes los necesitan; si no, usa los de «cualquiera». Un **valor** de N con retorno r empieza con una **transición compuesta** E(N, a, r) para cada una de las 7 clases a que empiezan un valor: lleva las comprobaciones del primer evento de cada componente (`type` que no admite a, `false`, un `enum`/`const` sin ningún literal del tipo de a) y la operación que abre la instancia (apilar el marco de objeto o arreglo de N, abrir el escalar de N, o volver a r con `true`/`false`/`null`). Un estado que espera un valor de N (el de la raíz, el de tras `:` de una clave, el de un elemento de arreglo) tiene esas 7 entradas. Es la composición: el padre apila el subautómata del hijo, que vuelve al padre al desapilar. Los estados de todos los nodos se reservan antes de rellenar ninguna fila, así que un hijo que es un antepasado (recursión por `$ref`) ya existe cuando se apila.

### 6.2 El marco «cualquiera»

Es la gramática LL(1) de JSON de [fase2/gramatica-json.md](../fase2/gramatica-json.md) §3 como DPDA, el esquema `true`:

| Grupo | Estados | Transiciones |
|---|---|---|
| raíz | q₀ (espera el valor), fin (∈ F) | q₀: E(raíz, a, fin) para las 7 clases de valor |
| objeto | `{` primero, `{` tras `,`, clave, tras clave, tras `:`, tras valor | primero: `"` → clave, `}` → desapila; tras `,`: `"` → clave; clave: cada carácter → clave, `"` final → tras clave; tras clave: `:` → tras `:`; tras `:`: E(cualquiera, a, tras valor); tras valor: `,` → tras `,`, `}` → desapila |
| arreglo | `[` primero, `[` tras `,`, tras valor | primero: `]` → desapila, E(cualquiera, a, tras valor); tras `,`: E(cualquiera, a, tras valor); tras valor: `,` → tras `,`, `]` → desapila |
| escalares | cadena, número | cadena: cada carácter → cadena, `"` final → cierra; número: byte → número, fin → cierra |

Las producciones `members_tail → , pair members_tail | ε` y `elements_tail` son los estados «tras valor» (`,` o el cierre) y «tras `,`» (que no admite el cierre: `[1,]` es error en el `]`). La diferencia entre «primero» y «tras `,`» es la que hay entre `members` (que puede ser ε) y `pair`.

### 6.3 Las reglas

| Regla | Subautómata |
|---|---|
| `true`, `{}` (sin reglas) | «cualquiera» |
| `false` (`Reject`) | comprobación que falla siempre en E(N, a, r), para las 7 clases; el valor se recorre con «cualquiera» |
| `type` | comprobación que falla en E(N, a, r) si el tipo de a no está en el conjunto. Solo un número queda sin decidir si el conjunto tiene `integer` y no `number`: N tiene entonces su propio estado de número, cuyo cierre comprueba «entero» con los registros del número (§3.3) |
| `$ref` sin hermanas | alias (§6.4) |
| `properties`, `additionalProperties`, `required`, `dependentRequired` | un marco de objeto propio: «primero», «tras `,`», un estado por estado del **trie** de los nombres (§3.1) más «clave desconocida» (el muerto del trie), dos por nombre (tras la clave, tras `:`) y dos para la clave desconocida, y «tras valor». Al cerrar la clave en el estado del nombre j se fija su bit; tras `:` se esperan valores del destino de j (el de `properties`, o el de `additionalProperties` si j solo está en `required`/`dependentRequired`, o «cualquiera»); la clave desconocida, del de `additionalProperties` o «cualquiera». `}` desapila con las comprobaciones de `required` y `dependentRequired` |
| `prefixItems`, `items` | un marco de arreglo propio con el índice en el estado: «primero» espera el elemento 0; «tras valor i» va con `,` a «valor min(i + 1, p)», con p = número de `prefixItems`; el elemento i < p es del nodo `prefixItems[i]` y los demás del de `items` (o «cualquiera») |
| `minProperties`, `maxProperties` (F4.1b) | el marco de objeto (propio aunque no haya `properties`) con contador de miembros (§3.1): la `"` de cada clave suma 1; `}` lo compara |
| `minItems`, `maxItems` (F4.1b) | el marco de arreglo con contador de elementos (§3.1): E(·, a, ·) de cada elemento suma 1; `]` lo compara |
| `minLength`, `maxLength` (F4.1b) | el subautómata de cadena de N: al cerrarla, el contador global de puntos de código (§3.6) |
| `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` (F4.1b) | el estado de número propio de N: al cerrarlo, la comparación exacta (§3.4) o el resto exacto (§3.5) sobre el lexema |
| `enum`, `const` de escalares (F4.1b) | por tipo: `true`, `false` y `null` se deciden en E(N, a, r) (falla si el literal no está); un objeto o un arreglo falla en la entrada (los literales compuestos son de F4.5b); un tipo sin ningún literal falla en la entrada. Las cadenas, con el **trie** de la unión de las cadenas literales de todos los componentes como subautómata de cadena de N (con la cadena vacía y prefijos compartidos; el estado muerto es «fuera de los literales»): al cerrar la cadena en el estado s, cada `enum`/`const` cuya lista no contiene la palabra de s falla, decidido al compilar. Los números, con `Test::NumEnum` (bisección en los valores ordenados, §3.4). `enum: []` (D11) falla en todas las entradas. Un `enum` y un `const` en el mismo nodo son dos reglas, cada una su violación |

Dos conjuntos distintos con las mismas reglas producen subautómatas distintos; los une la minimización de F4.3. Lo que no está en la tabla se rechaza en compilación (§6.6).

### 6.4 `$ref`, `allOf` y E₌: el cierre conjuntivo

**Qué marco lee cada instancia** (F4-U2; [conj.rs](../../crates/automaton/src/conj.rs)). Las aristas `$ref` y `allOf` de E₌ aplican su destino a la **misma** instancia. El cierre conjuntivo C(v) es el conjunto de los nodos que **aportan reglas propias** (alguna regla que no es `$ref` ni `allOf`) entre v y los alcanzables desde v por esas aristas. La instancia de un valor de v la lee **un solo marco**, el del conjunto canónico C(v) (lista ordenada de `NodeId`, guardada una vez): sus componentes. Un conjunto de un nodo es el marco de F4.1a; uno vacío es «cualquiera»; uno de varios es un **producto**: `$ref` con hermanas (el nodo y su destino) y `allOf` (las ramas, y el nodo si tiene más reglas). No hay llamadas ε ni copias: es la opción «expandir E₌ memorizado» de [fase3/ir.md](../fase3/ir.md) §7, que cierra [fase3/pendientes.md](../fase3/pendientes.md) §27 y la parte JSON de §22.

**Corrección.** El lenguaje de un valor de v es la intersección de los lenguajes de los componentes de C(v) (cada uno, el de sus reglas propias): por definición de `$ref` y de `allOf` (2020-12, Core §8.2.3.1 y §10.2.1.1), la instancia es válida para v si lo es para cada nodo alcanzado por esas aristas, y un nodo sin reglas propias no restringe nada. Como intersección y conjunto no distinguen repeticiones, los componentes son los `NodeId` **distintos**, no los caminos (S-3): `allOf: [X, X]` cuenta las reglas de X una vez, y la cadena de diamantes da un solo nodo. El producto de los subautómatas de los componentes reconoce esa intersección: los subautómatas apilan con los mismos eventos (la operación de pila la fija la clase, §4.3), así que el producto es otro autómata de pila visible, con marcos que son tuplas de marcos (Alur y Madhusudan), y su peso es la suma de los de sus componentes (F4-U13: `allOf` y `$ref` con hermanas suman las violaciones de dentro). Cada comprobación lleva la regla **de su componente** (`Check::rule` = nodo y regla), así que el reporte atribuye cada violación a su nodo (`el_producto_atribuye_cada_comprobacion`) y cada `Constraint` cuenta la suya.

**Cómo se construye el producto sin multiplicar estados.** Las piezas de F4.1b son tries, índices de arreglo, contadores y comprobaciones al cerrar:

- *Objeto*: el producto alcanzable de los tries de nombres de los componentes, que leen la misma clave, tiene un estado por prefijo distinto de la unión de los nombres: es el **trie de la unión**, con un bit por nombre de la unión. Cada componente conserva su semántica: tras `"k":` el hijo es la **unión de los cierres** de los destinos que cada componente asigna a k (su `properties[k]`, o su `additionalProperties`, o nada), y una clave desconocida para la unión, la unión de los `additionalProperties`. Así `additionalProperties` sigue sin ver las `properties` de las ramas (ejemplo de [fase3/ir.md](../fase3/ir.md) §6.2: `{"a": 1}` da 1 violación, la de `false`). `required` y `dependentRequired` de cada componente son máscaras sobre los bits de la unión, y los contadores se comparten (cuentan lo mismo).
- *Arreglo*: el índice se satura en el mayor `prefixItems`; el hijo del índice i es la unión de los de cada componente.
- *Número y cadena*: un solo estado de número con las comprobaciones de todos los componentes al cerrar, y el trie de la unión de las cadenas literales con una comprobación por literal (§6.3).

**Tamaño.** Los conjuntos se descubren con una lista de trabajo desde C(raíz), una vez cada uno (memoria por conjunto). C(v) se calcula una vez por destino distinto con un DFS iterativo sobre las aristas conjuntivas, tras resolver los **alias** (un nodo sin reglas propias con un solo destino conjuntivo distinto, como `$ref` sin hermanas o `allOf: [X, X]`, tiene el cierre de su destino; las cadenas se recorren una vez, con compresión de caminos). La cadena de diamantes de `referencias_compartidas_tamano_lineal` (10⁴ definiciones con `allOf` a la siguiente dos veces, 2^(10⁴ − 1) caminos) se compila con los 2 marcos de «cualquiera» y un estado de número; si cada definición tiene además un `maximum`, el conjunto tiene 10⁴ componentes y M, 10⁴ comprobaciones al cerrar el número: lineal, no 2^m (se comprueba el resultado, no el tiempo, D1). En general, el número de conjuntos está acotado por las ranuras de hijo de los subautómatas (cada conjunto nuevo es el hijo de una clave o de un índice), que cuentan en el presupuesto de δ. Un grafo hostil (muchos destinos distintos con cierres grandes) podría costar trabajo cuadrático: el trabajo de los cierres (nodos y aristas recorridos y tamaños de las uniones) tiene un presupuesto de 2²⁴ pasos (`MAX_CLOSURE_WORK`) y pasarlo es `limit`.

**Tope por producto** (L-1, F4-U11; `BuildOptions::max_product_states`, 4 096 por defecto, configurable). Los estados de cada subautómata producto (de un conjunto con ≥ 2 componentes que aportan a ese subautómata) se cuentan **en el análisis, antes de reservar**: el trie de la unión se cuenta sin construirlo (con las palabras ordenadas, cada una añade sus caracteres menos el prefijo común con la anterior, más la raíz y el muerto). El número solo depende de los nombres de la unión (o del mayor `prefixItems`) y del número de componentes, así que el tope se comprueba nada más reunir los nombres, **antes** de calcular los hijos; justo después, el trabajo de calcularlos (nombres × componentes, o índices × componentes en un arreglo) se descuenta del presupuesto de los cierres, también antes de hacerlo. Los hijos se calculan nombre a nombre con un solo vector temporal: el análisis de un producto ocupa la memoria de su entrada (los nombres y los declarados de cada componente), nunca una matriz nombres × componentes (ronda 1 de F4.1b: `tope_de_producto_antes_de_reservar` comprueba con el asignador contador que un producto de 4 096 nombres y 2 001 componentes da `ProductLimit` pidiendo ≈ 1,5 MB, donde el orden anterior pedía ≈ 69 MB; `presupuesto_de_los_cierres_da_limit`, que 1 000 nombres × 17 001 componentes agotan los 2²⁴ pasos). Si pasa del tope, `BuildError::ProductLimit { states, limit, scalar, location }`: el número alcanzado y si el marco es escalar (cadena o número), donde F4.5a podrá recurrir a la simulación paralela; en un marco con hijos (objeto o arreglo) es `limit` definitivo (`tope_de_estados_por_producto`: con el tope a 8, el producto de dos objetos con `a` y `b` tiene 13 estados). El resumen de `schemac automaton` cuenta los subautómatas producto en `products`. Con las piezas de F4.1b, los **estados** de un producto crecen con la suma de sus componentes, no con su producto; la explosión de estados llega con los AFD de `pattern` (el `allOf` de `^(a{p})*$` con los 15 primeros primos tiene ∏ p estados), que F4.4b y F4.5 meterán en este mismo mecanismo.

**Las demás tablas** (ronda 2 de F4.1b). El tope y el presupuesto de δ acotan los estados, no las comprobaciones ni los lexemas, que sí pueden crecer como un producto. Se acotan así:

- *Lo que se comparte, una vez*: las comprobaciones de la entrada de un valor de cada conjunto se guardan una vez por (conjunto, clase), y todas las transiciones de entrada con retornos distintos (un índice de `prefixItems` por retorno, cada valor de un objeto) apuntan al mismo trozo de `checks` (antes, una copia por retorno: `prefixItems` de 2 000 `$ref` a un `allOf` de 200 ramas daba 2,4·10⁶ comprobaciones; ahora 1 600). Cada cota, divisor y `enum` numérico de una regla se ordena (en el análisis) y se copia a `nums`/`lexemes` (en el constructor) una sola vez por (nodo, regla), aunque lo lean muchos conjuntos; así `lexemes` no pasa del tamaño de los lexemas del esquema.
- *Lo que sigue siendo multiplicativo, en el presupuesto*: el cierre de cada estado del trie de una cadena producto lleva una comprobación por literal que no contiene su palabra (estados × literales), y los cierres de objetos, números y arreglos, una por regla de cada componente, con sus máscaras. El análisis los cuenta antes de reservar (`extra`, como mucho estados × (literales + longitudes) en las cadenas) y el presupuesto de F4-U6 pasa a cubrir **entradas de δ + comprobaciones + palabras de máscaras + dependencias + lexemas**: |Q| · |Σ| + `extra` ≤ 2²⁰, o `limit` con la ubicación (`presupuesto_ampliado_da_limit_antes_de_reservar`: un `allOf` de 2 000 `const` da ≈ 4·10⁶ y `limit` pidiendo ≈ 1,5 MB). Es una precisión de F4-U6 para las dos personas ([pendientes.md](pendientes.md) §22), no un tope nuevo.
- *Índices de 32 bits*: las longitudes de las tablas pasan por `u32::try_from`; si alguna no cupiera (con el presupuesto no ocurre salvo en `text` y `lexemes`, que solo dependen del tamaño del esquema), la construcción da `limit` en lugar de truncar un índice que señalaría otra entrada válida.

### 6.5 Ubicación recortada

`automaton::location_clipped(&VerifiedIr, id, max)` (F4-B8, cierra [fase3/pendientes.md](../fase3/pendientes.md) §4) es `VerifiedIr::location` recortada a como mucho `max` bytes (mínimo 16): si no cabe, su principio, `…` y su final. Sube por los padres (O(profundidad)) y solo escribe los caracteres que caben, así que cuesta O(profundidad + max) aunque las claves midan megabytes; nunca parte un carácter ni un escape `~0`/`~1`. Las tablas guardan la de cada nodo con reglas comprobadas, con `max` = 256 (F4-U6), y la usan los mensajes de `Pending` y del reporte. `location` sin recortar queda para pruebas y depuración.

### 6.6 Lo no implementado

Antes de construir nada, `build` recorre todo lo alcanzable desde la raíz (por todas las aristas; lo inalcanzable no cuenta) y, si alguna regla no se compila todavía, devuelve `BuildError::Pending { node, rule, task, location }` con la tarea **más tardía** de las que encuentra (la que fija cuándo compilará el esquema; a igualdad, la primera en anchura). `schemac automaton` la escribe como «estrategia pendiente: pattern (F4.4b) en #/properties/x» y sale con 1 (F4-U5):

| Regla | Tarea |
|---|---|
| XML (cualquier IR de XSD) | F4.1c |
| `pattern` | F4.4b |
| `anyOf`, `oneOf`, `not`, `dependentSchemas` | F4.5a |
| `uniqueItems`, `enum`/`const` con un literal compuesto | F4.5b |

Desde F4.1b, `minimum`…`exclusiveMaximum`, `multipleOf`, `min`/`maxLength`, `min`/`maxItems`, `min`/`maxProperties`, `enum`/`const` escalares, `$ref` con hermanas y `allOf` se compilan. Los cuatro casos `non-dpda` del corpus dan `Pending` de F4.4b o F4.5 (`non_dpda_da_pending`); `non-dpda-allof-not-01` da `not` (F4.5a).

Además, todos antes de reservar δ:

- `BuildError::Limit` si un objeto (la unión de los nombres de un producto) tiene más de 4 096 nombres (`limite_de_nombres_por_objeto`: 4 096 compilan, 4 097 no); si |Q| · |Σ| más las entradas de las tablas que crecen con los productos (comprobaciones, máscaras, dependencias y lexemas, §6.4) pasa de 2²⁰ (F4-U6, precisado en F4.1b), con una **cota** superior de |Q| (el mensaje dice «como mucho»), comprobada también durante el análisis con las clases vistas hasta entonces, para cortar un grafo de conjuntos que crece sin parar (`presupuesto_de_delta_da_limit_sin_reservar`: unas 10¹¹ entradas, 1 TB, sin pedir la memoria); si un divisor de `multipleOf` pasa del tope (§3.5); o si los cierres pasan su presupuesto de trabajo (§6.4).
- `BuildError::ProductLimit` si un producto pasa del tope de estados (§6.4).
- `BuildError::Internal` como defensa en profundidad (F4-B11): un ciclo de `$ref`/`allOf` (que I14 excluye) en la resolución de alias o en el DFS del cierre (`ciclo_de_alias_es_internal` y `ciclo_en_el_cierre_es_internal`, con una IR sin verificar), en lugar de un cuelgue; y ya no hay `expect` en el constructor (F4.1a-CR-S4).

## 7. Complejidad

Sea n el tamaño de la IR alcanzable, L la suma de las longitudes de los nombres con bit y k = |Σ|.

- **Construcción**: con N los conjuntos alcanzables, |Q| ≤ 13 + Σ_N (T_N + 2·nombres_N + 5) sobre los que tienen objeto, con T_N los estados del trie de la unión de sus nombres (≤ L_N + 2) + Σ_N (2·p_N + 2) sobre los de arreglo (p_N ≥ 1; 3 si p_N = 0) + 1 por estado de número propio + T′_N por subautómata de cadena (el trie de sus literales). Es la cota que se comprueba antes de reservar: |Q| · k + E ≤ 2²⁰, con E las entradas de las otras tablas que crecen con los productos (comprobaciones, máscaras, dependencias y lexemas, §6.4). Lo acotado y por qué: los **estados y δ** por el presupuesto y por el tope de cada producto; las **comprobaciones, máscaras y lexemas** por el mismo presupuesto (las compartidas, una vez por conjunto y clase o por regla); el **trabajo de los cierres** W ≤ 2²⁴ (O(n) con alias y memoria por destino en los esquemas normales); y los **lexemas** en bytes y las ubicaciones (`text`), por el tamaño del esquema (cada uno se copia una vez). **Tiempo** (ronda 3 de F4.1b): todo lo que recorre literales, lexemas o nombres de una regla se hace **una vez por (nodo, regla)** y se reutiliza en cada conjunto que la contiene: la máscara de 7 bits de las clases en las que la regla falla al empezar un valor (`entry_mask`: un `enum` se recorre una vez por clase, no una vez por conjunto), el `enum` numérico ordenado, el conjunto de cadenas, el análisis del divisor y la copia de cada lexema. Así cada conjunto cuesta O(Σ reglas de sus componentes) (≤ 31 · W en total) más su parte de las tablas, que el presupuesto acota. En tiempo, el total es **O(2²⁰ + n + W · ℓ)**, con n el tamaño de la IR (que incluye los literales) y ℓ la longitud del nombre más largo: el presupuesto de los cierres cuenta los pasos nombres × componentes de los productos de objetos, pero cada paso hashea el nombre entero (la unión de nombres, el mapa de declarados de cada componente, que se construye por componente y por conjunto, y los bits de la máscara). Está acotado: el presupuesto de δ impone conjuntos × ℓ ≲ 2¹⁶ (cada nombre de un conjunto aporta al menos ℓ estados por |Σ| ≥ 15) y el tamaño del esquema, componentes × ℓ ≤ 2²⁴, así que conjuntos × componentes × ℓ ≲ 2³². Medido en release (revisión de la ronda 3): unos 5 s con un esquema de 16 MiB construido para ello (5,58 s y 362 MB frente a 0,59 s de `schemac ir` sobre el mismo esquema; otro de 140 conjuntos × 33 000 componentes con nombres de 440 bytes, 4,89 s). El servidor necesita, por tanto, el tope de tiempo de compilación de [fase3/pendientes.md](../fase3/pendientes.md) §38 (5.7-P3); internar los nombres a enteros quitaría el factor ℓ ([pendientes.md](pendientes.md) §26). Lo comprueba sin reloj `enum_compartido_se_recorre_una_vez` (`crates/automaton/src/json.rs`): 2 000 conjuntos que comparten un `enum` de 10⁴ valores visitan como mucho 7 · 10⁴ literales (sin el memo, 9,6 · 10⁸). Memoria O(2²⁰ + n + W) (sin el factor ℓ: los mapas de declarados son temporales de cada conjunto).
- **Validación**: O(1) por evento respecto del payload (una consulta a δ, las comprobaciones de la transición, acotadas por el esquema, una operación de pila de O(palabras del marco)); con el lexer, O(n) en bytes. Las comprobaciones numéricas al cerrar un número cuestan O(|lexema| + |cota|) cada una (O(log n) comparaciones en un `enum`), con el lexema ≤ 1 024 bytes y la cota fija por esquema: O(1) amortizado por byte del número. Pila: O(d) marcos de tamaño fijo. Cero reservas: el núcleo usa la memoria del llamador (`cero_reservas_al_validar`, con un asignador contador en un binario de prueba aparte, F4-B25) y es `#![no_std]` sin `alloc`; ningún bucle recurre, así que un payload de 10⁵ niveles se recorre en un hilo de 256 KiB.

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

## 9. Decisiones de F4.1a y F4.1b

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
11. **Salida de `schemac automaton`** (F4-U7): JSON con `automatonVersion` 1, `format`, `states`, `transitions`, `frames`, `classes`, `registerBytes`, `maxFrameBytes`, `products` y `deferred` (antes `pending`, renombrado en F4.1b por F4.1a-CR-S5; 0 hasta F4.5); códigos 0, 1 (esquema, pendiente o límite), 2 (lectura, argumentos) y 3 (interno).

Las que tomó el Builder en F4.1b, candidatas al registro (las que tocan una interfaz compartida, marcadas, se proponen a las dos personas, D42):

12. **Marco por conjunto canónico = cierre conjuntivo de nodos que aportan reglas propias** (§6.4), con alias generalizados (`$ref` sin hermanas y `allOf` con un solo destino distinto) y memoria por destino; el producto de objetos es el trie de la unión con hijos por unión de cierres, y el de cadenas, el trie de la unión de los literales: el producto alcanzable sin explosión de estados.
13. **Tope por subautómata producto** (de un conjunto con ≥ 2 componentes que aportan a ese subautómata), contado en el análisis con el trie contado sin construirlo; error propio `BuildError::ProductLimit { states, limit, scalar, location }` (distinto de `Limit`, para que F4.5a sustituya el caso escalar por la simulación paralela); `BuildOptions { max_product_states }` y `build_with`.
14. **Presupuesto de trabajo de los cierres** (`MAX_CLOSURE_WORK` = 2²⁴ pasos, `limit`): defensa ante grafos de E₌ hostiles. **[Compartida: límite nuevo, `00-alcance.md` §4]**.
15. **Lexema en un registro global de 1 024 bytes** dentro de `Core` (`LEXEME_MAX`), y `limit` solo cuando una comprobación lo necesita (§3.4); `integer` sigue sin lexema.
16. **Comparador sobre dos lexemas** (`runtime_rs::num::compare`), con la diferencia de exponentes exacta hasta 2¹⁰⁰ y saturada por encima; las cotas del esquema se guardan como lexemas, sin normalizar; `enum` de números ordenado y por bisección.
17. **`multipleOf` por factorización del divisor** 2ᵃ · 5ᵇ · M′ y tres restos por Horner; **precisión de F4-U6**: además de M′ < 2⁶⁰, a ≤ 63 y b ≤ 27 (y ≤ 38 cifras significativas), si no `limit` con la ubicación. **[Compartida: `00-alcance.md` §4]**.
18. **Contadores**: uno por marco de objeto (miembros, sumado al abrir cada clave) y uno por marco de arreglo (elementos, sumado en la transición compuesta de entrada del elemento, antes de apilar), en la palabra tras la máscara (`Frame::mask_words`, F4.1a-CR-S1); `Trans::count`; la cota de 64 bits en `Check::arg`/`len`. El contador de puntos de código de la cadena es global.
19. **`enum`/`const` escalares por tipo**: `true`/`false`/`null` y los tipos sin literal en la transición de entrada; cadenas por trie (decididas al compilar, `Test::Fails` en el cierre); números por `Test::NumEnum`.
20. **Una sola partición de caracteres** para claves y valores (la común más fina; [pendientes.md](pendientes.md) §3).
21. **Zona global de salidas del escalar** al principio de la memoria del llamador (`Tables::scalar_slots`, 0 palabras) y «cerrar la instancia» = `Pop`, `Leave` o la entrada de un valor de un evento (§3.6), para F4.5a.
22. **`Task::Registers` desaparece** (F4.1b ya no deja nada pendiente); el DOT llama «byte num» a la clase `NUMBER_BYTE` (F4.1a-CR-S11) y marca `/ +1` en las transiciones que cuentan.

## 10. Pruebas

- `crates/automaton/tests/json.rs`: el marco «cualquiera» con `true` (bien formados con w = 0; `{"a" 1}`, `[1,]`, `{,}`, `]`, `1 2`, `{"a":1}}` y la entrada vacía con ∞ en su offset), la dorada contada a mano, `type` e `integer` (con exponentes de 10⁵ cifras), `false`, `properties`/`additionalProperties` (con escapes y caracteres no ASCII en las claves), claves declaradas repetidas (∞), `required`/`dependentRequired` (también con máscaras de más de 64 bits), fallo temprano, `prefixItems`/`items`, alias, `Pending` con su tarea, δ total y pila visible, la pila llena (`limit`) y D7. Todo payload se valida con bloques de 1, 7 y 4096 bytes y se exige el mismo reporte.
- `crates/automaton/tests/corpus.rs`: el corpus de `tests/cases` (§6.6, criterios 1-2 de F4.1a y criterio 5 de F4.1b: todo caso JSON que compila da su `valid`, `violations` y `offset`).
- `crates/automaton/tests/escalares.rs` (F4.1b): contadores de miembros y elementos, máscara separada del contador, longitudes en puntos de código, `enum` de cadenas con prefijos y la vacía, números por valor en `enum`/`const`, `enum` y `const` juntos, `enum: []`, rangos exactos, `multipleOf` y su tope, el lexema largo, el límite de nombres y el presupuesto de δ sin reservar.
- `crates/automaton/tests/conjuncion.rs` (F4.1b): el ejemplo de `fase3/ir.md` §6.2, `$ref` con hermanas, la atribución por componente, ramas repetidas (S-3), productos de objetos, de arreglos y con recursión, la cadena de diamantes de 10⁴ nodos y el tope de estados por producto.
- `crates/automaton/src/conj.rs` (unitarias): ciclos de alias y de cierre dan `Internal`; un diamante da sus `NodeId` distintos.
- `crates/automaton/src/json.rs` (unitaria, ronda 3): `enum_compartido_se_recorre_una_vez`, con un contador de literales visitados solo en pruebas.
- `crates/runtime-rs/tests/num.rs` (F4.1b): el comparador y el entero sin lexema frente a `ir::num` (proptest), `multipleOf` frente al oráculo racional (proptest y tabla) y el tope del divisor.
- `crates/automaton/tests/weight.rs`: leyes del semianillo por proptest y la tabla de costo.
- `crates/automaton/tests/partes.rs`: el trie como `Dfa` completo y `location_clipped`.
- `crates/automaton/tests/sin_reservas.rs` (ronda 2): `comprobaciones_de_entrada_compartidas`, `enum_numerico_compartido_una_vez` y `presupuesto_ampliado_da_limit_antes_de_reservar`, con el asignador contador: menos de 256 bytes pedidos por byte del esquema, o `limit` antes de reservar.
- `crates/automaton/tests/sin_reservas.rs`: 0 reservas en el camino de validación, también con contadores, lexema, `enum`, rangos, `multipleOf` y productos (`cero_reservas_con_registros_y_productos`); y, con el mismo asignador (que cuenta también los bytes), que el tope de un producto y el presupuesto de los cierres saltan antes de reservar lo proporcional al producto (`tope_de_producto_antes_de_reservar`, `presupuesto_de_los_cierres_da_limit`).
- `crates/runtime-rs/src/num.rs` (unitarias): la saturación del exponente y la diferencia de exponentes saturada.
- `crates/cli/tests/automaton.rs`: `schemac automaton`, la dorada y el `.dot` versionados y los códigos de salida.
