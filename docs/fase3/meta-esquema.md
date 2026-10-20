# Fase 3 — Validación del esquema contra el meta-esquema (subconjunto)

Primera tarea del análisis semántico. Entrada: el AST que produce el parser LL(1) de la Fase 2 (`crates/schema-parser`). Salida: nada si el esquema cumple el **meta-esquema 2020-12 restringido al subconjunto** de [`docs/00-alcance.md`](../00-alcance.md), o la lista de **todos** sus errores con línea y columna. Las tareas siguientes de la Fase 3 (tabla de símbolos de `$defs`, resolución de `$ref`, ciclos no productivos, IR y clasificación) parten de un AST que ya pasó esta validación.

Implementación: [`crates/semantic/src/meta.rs`](../../crates/semantic/src/meta.rs) (función `check_meta_schema`), con [`number.rs`](../../crates/semantic/src/number.rs) (propiedades exactas de un número a partir de su lexema), [`unsupported.rs`](../../crates/semantic/src/unsupported.rs) (categoría y motivo de cada palabra clave rechazada) y [`error.rs`](../../crates/semantic/src/error.rs). En la línea de comandos: `schemac check esquema.json`.

## 1. Qué es el meta-esquema y qué parte se usa

En JSON Schema, el lenguaje de los esquemas válidos se define con otro esquema, el **meta-esquema** (`https://json-schema.org/draft/2020-12/schema`): un esquema es válido si, visto como un documento JSON, el meta-esquema lo acepta. El oficial se compone de siete vocabularios (`core`, `applicator`, `unevaluated`, `validation`, `meta-data`, `format-annotation`, `content`) y además declara, por compatibilidad, cuatro palabras clave antiguas (`definitions`, `dependencies`, `$recursiveAnchor`, `$recursiveRef`).

El proyecto no incluye el meta-esquema oficial ni un validador genérico para aplicárselo: las reglas que corresponden al subconjunto están escritas a mano en `meta.rs`, una por palabra clave (§3). Esto es equivalente a validar contra un meta-esquema **restringido** M′ que se obtiene del oficial así:

1. Se quitan las palabras clave fuera del subconjunto y se añade `"additionalProperties": false` a cada objeto-esquema, de modo que **cualquier otra clave es un error** (en el oficial, una clave desconocida se acepta y se ignora). Ese es el rechazo de la decisión D6.
2. Se endurecen algunas reglas según el alcance (§4).

| Vocabulario oficial | Cubierto | Fuera (se rechaza) |
|---|---|---|
| `core` | `$schema`, `$id`, `$anchor`, `$ref`, `$defs`, `$comment` | `$dynamicRef`, `$dynamicAnchor`, `$vocabulary` |
| `applicator` | `prefixItems`, `items`, `properties`, `additionalProperties`, `dependentSchemas`, `allOf`, `anyOf`, `oneOf`, `not` | `contains`, `patternProperties`, `propertyNames`, `if`, `then`, `else` |
| `unevaluated` | — | `unevaluatedItems`, `unevaluatedProperties` |
| `validation` | `type`, `const`, `enum`, `multipleOf`, `maximum`, `exclusiveMaximum`, `minimum`, `exclusiveMinimum`, `maxLength`, `minLength`, `pattern`, `maxItems`, `minItems`, `uniqueItems`, `maxProperties`, `minProperties`, `required`, `dependentRequired` | `maxContains`, `minContains` |
| `meta-data` | `title`, `description`, `default`, `deprecated`, `readOnly`, `writeOnly`, `examples` | — |
| `format-annotation` | `format` (solo anotación, como en el oficial) | — |
| `content` | — | `contentEncoding`, `contentMediaType`, `contentSchema` |
| compatibilidad | `definitions` (alias de `$defs`) | `dependencies`, `$recursiveAnchor`, `$recursiveRef` |

## 2. Por qué es análisis semántico y no parte de la gramática LL(1)

La gramática de la Fase 2 ([gramatica-esquema.md](../fase2/gramatica-esquema.md)) ya hace la mitad del trabajo del meta-esquema: la clase `K_*` de cada palabra clave decide la **forma** de su valor (número, cadena, esquema, arreglo de esquemas…), y eso es exactamente la parte `"type": …` de cada regla del meta-esquema. Lo que queda depende del **valor** y no del tipo de token, y hay tres razones para no meterlo en la gramática:

1. **Hay restricciones que no son libres de contexto.** `required` exige cadenas sin repetir (`"uniqueItems": true`). El lenguaje de los arreglos **con** una cadena repetida no es libre de contexto: su intersección con el lenguaje regular `["a*b*", "a*b*"]` es { `["aⁱbʲ", "aⁱbʲ"]` }, que no lo es (lema de bombeo con `["aᵖbᵖ", "aᵖbᵖ"]`: la subcadena bombeada no puede alterar las dos copias por igual), y los libres de contexto son cerrados bajo intersección con regulares. Si los arreglos **sin** repetidas formaran un lenguaje LL(1), serían deterministas libres de contexto; estos son cerrados bajo complemento y bajo intersección con regulares, así que su complemento intersecado con el lenguaje (regular) de los arreglos de cadenas, que es el de los arreglos con repetidas, también sería libre de contexto, lo que acabamos de descartar. Ningún parser LL(1) puede decidirlo: hace falta una tabla aparte, igual que para las claves repetidas del parser (`gramatica-esquema.md` §4).

   *Nivel del argumento.* Lo anterior razona sobre **caracteres**, es decir, sobre el sistema lexer + parser visto como un solo reconocedor de bytes. El parser LL(1) trabaja sobre **tokens**, y ahí el problema es aún más claro: `STRING` es un único terminal, así que `["a", "a"]` y `["a", "b"]` son la misma palabra `[ STRING , STRING ]` y ninguna gramática sobre ese alfabeto puede aceptar una y rechazar la otra. Para que la gramática las distinguiera habría que refinar `STRING` en un terminal por cada cadena posible, un alfabeto infinito. En los dos niveles la conclusión es la misma: la unicidad es un atributo del valor del token y se comprueba fuera de la gramática.
2. **Otras sí son regulares, pero meterlas en la gramática la empeora.** «`minLength` es un entero no negativo» o «`type` es uno de siete nombres» son lenguajes regulares sobre el lexema: se podrían reconocer refinando `NUMBER` en `NUM_ENTERO_NO_NEG`, `NUM_POSITIVO`, … y `STRING` en un terminal por nombre de tipo, como se hizo con las `K_*`. Pero cada refinamiento multiplica los terminales y las celdas de la tabla, y el error pasaría a ser «se esperaba `NUM_ENTERO_NO_NEG`» en lugar de «se esperaba un entero no negativo, se encontró `-1`». Es la separación habitual en un compilador: la gramática fija la estructura y el análisis semántico comprueba los atributos (aquí, el valor de cada token).
3. **El rechazo de las palabras clave fuera del subconjunto se hace aquí a propósito.** La gramática podría rechazar `K_OTHER` en la tabla (bastaría con quitar esa alternativa de `keyword`), pero el parser se detiene en el primer error y solo podría decir «palabra clave no esperada». En la Fase 3 se informa **cada** palabra clave rechazada, con su categoría y su motivo (D6), y se distinguen las de §2.8 de las desconocidas.

La Fase 3 tampoco es un autómata sobre la entrada: recorre el AST, que ya existe en tiempo de compilación (el AST solo existe para el esquema, nunca para el payload).

## 3. Qué se comprueba y dónde

Columna «Capa»: **2** = lo comprueba el parser LL(1) (no se repite aquí); **3** = esta tarea; **3+** = otra tarea de la Fase 3 o posterior.

| Palabra clave | Regla del meta-esquema oficial | Capa | Comprobación y error |
|---|---|---|---|
| todas | `type` del valor (cadena, número, esquema, …) | 2 | `syntax` en el primer token que no encaja |
| todas | claves sin repetir en un objeto | 2 | `duplicate-key` (D2) |
| fuera del subconjunto | (el oficial las acepta) | 3 | `unsupported` en el nombre: categoría y motivo (§5) |
| `$schema` | `uriString` | 3 | `unsupported` si no es `https://json-schema.org/draft/2020-12/schema`, con o sin `#` final (en el valor), o si no está en la raíz (en el nombre) |
| `$id` | `uriReferenceString`, `pattern: ^[^#]*#?$` | 3 | `meta-schema` si tiene un fragmento no vacío; `unsupported` si no está en la raíz |
| `$anchor` | `anchorString`: `^[A-Za-z_][-A-Za-z0-9._]*$` | 3 | `meta-schema` en el valor |
| `$ref` | `uriReferenceString` | 3 | `unsupported` si no es local: ni empieza por `#` ni es la cadena vacía (que equivale a `#`, RFC 3986 §4.4) |
| `$ref` | que el destino exista | 3 (tabla de símbolos) | `reference` en el valor ([referencias.md](referencias.md)) |
| `$defs`, `definitions`, `properties`, `dependentSchemas` | objeto nombre → esquema | 2 | — |
| `type` | `simpleTypes` o arreglo `minItems: 1`, `uniqueItems` | 3 | `meta-schema` en el nombre inválido, en la segunda aparición de un repetido o en el `[` vacío |
| `enum` | arreglo (`items: true`), sin `minItems` | 2 | forma en 2; `[]` se acepta, como en el oficial (D11) |
| `const`, `default`, `examples` | cualquier valor / arreglo | 2 | — (su contenido es un literal, no se recorre) |
| `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum` | `type: number` | 2 | — (la forma booleana de Draft 4 la rechaza el parser con una pista) |
| `multipleOf` | `type: number`, `exclusiveMinimum: 0` | 3 | `meta-schema` si es cero o negativo |
| `minLength`, `maxLength`, `minItems`, `maxItems`, `minProperties`, `maxProperties` | `nonNegativeInteger` | 3 | `meta-schema` si es negativo o tiene parte fraccionaria |
| `pattern` | `type: string`, `format: regex` | 2 / 3+ | forma en 2; el subconjunto de regex (sin retrorreferencias ni lookaround) se comprueba al compilarlo a AFD (tarea `pattern` → AFD) |
| `format` | `type: string` | 2 | — (anotación; no se valida, `docs/00-alcance.md` §2.4) |
| `items`, `additionalProperties`, `not` | esquema | 2 | — (la forma de arreglo de `items` la rechaza el parser con una pista) |
| `prefixItems`, `allOf`, `anyOf`, `oneOf` | `schemaArray`: `minItems: 1` | 3 | `meta-schema` en el `[` vacío |
| `uniqueItems`, `deprecated`, `readOnly`, `writeOnly` | `type: boolean` | 2 | — |
| `required` | `stringArray`: cadenas, `uniqueItems` | 2 / 3 | forma en 2; repetidas en 3, en la segunda aparición |
| `dependentRequired` | objeto → `stringArray` | 2 / 3 | forma en 2; repetidas dentro de cada arreglo en 3 |
| `$comment`, `title`, `description` | `type: string` | 2 | — |
| todas | profundidad ≤ 256 | 2 | `limit` (D7) |

**Números exactos.** «Entero» sigue a 2020-12: la parte fraccionaria es cero, así que `1.0`, `1e2` y `1.50e1` son enteros, y `-0` es un entero no negativo. Se decide sobre el lexema decimal, sin pasar por `f64` (`number.rs`): el valor es D × 10ˢ con D los dígitos de la mantisa sin ceros a la derecha y s = exponente − cifras decimales + ceros quitados; es cero si D solo tiene ceros, y entero si s ≥ 0. Así `1e-400` no es entero y `1e400` sí, aunque ninguno de los dos cabe en un `f64` sin redondear. Una prueba de propiedades lo compara con aritmética entera exacta.

## 4. Diferencias con el meta-esquema oficial

Regla (decisión D11 del plan): la validación **no es más estricta que el meta-esquema oficial salvo donde el alcance o el registro de decisiones lo dicen expresamente**. Las únicas diferencias, de cualquier capa (2 o 3), cada una con su apoyo:

- *Capa 2.* Claves repetidas en cualquier objeto del esquema, también dentro de los literales de `const`, `enum`, `default` y `examples` (error `duplicate-key` del parser; `{"const": {"a": 1, "a": 2}}` falla en la segunda `"a"`). Apoyo: decisión D2 del plan y el enunciado (PDF), que lista las «claves duplicadas» como caso malformado. El meta-esquema oficial no puede verlo: trabaja sobre el modelo de datos ya parseado, donde un objeto no tiene claves repetidas.
- *Capa 2.* Anidamiento de `{`/`[` de como mucho 256 niveles en todo el esquema (error `limit`). Apoyo: decisión D7 del plan y el límite de profundidad de `docs/00-alcance.md` §4. El oficial no pone límite.

- Las palabras clave desconocidas o fuera del subconjunto se **rechazan** (el oficial las acepta y las ignora). Alcance: introducción y §2.8 («todo lo que no aparece aquí … el compilador lo rechaza»), decisión D6. Ignorarlas haría aceptar payloads que el esquema rechaza sin que el usuario lo note.
- `$schema` solo admite el URI de 2020-12 (alcance §2.1: «Otro valor → error»), con o sin un `#` final: un fragmento vacío no cambia el recurso (RFC 3986 §6.2.3) y `…/schema#` es la forma que se copia de Draft 7. Cualquier otro fragmento se rechaza.
- `$schema` solo en la raíz (alcance §2.1). Esto no es más estricto que la especificación sino que la aplica: 2020-12 prohíbe `$schema` fuera de la raíz de un recurso, y como `$id` solo se admite en la raíz no hay recursos embebidos. El meta-esquema oficial no puede comprobarlo porque no ve la posición.
- `$id` solo en la raíz (alcance §2.1: «Solo en la raíz, como URI base»).
- `$ref` debe ser local: empezar por `#` o ser la cadena vacía (alcance §2.1: «Solo referencias locales: `#`, `#/$defs/x`, JSON Pointer dentro del documento, `#ancla`», y §2.8: «`$ref` remotos»). La cadena vacía es una referencia al mismo documento (RFC 3986 §4.4), equivalente a `#`, así que es local y se acepta: rechazarla sería más estricto que el alcance (D11). En cambio, un `$ref` absoluto o relativo que, resuelto contra el `$id` de la raíz, coincide con el propio documento (`https://ejemplo.co/s.json#/x` con ese `$id`) sería local según la RFC 3986, pero se rechaza: el alcance excluye los `$ref` remotos sin resolver URI, y distinguir esos casos exigiría resolver referencias contra la URI base; escribirlo como `#…` es equivalente. El mensaje dice «solo se admiten referencias locales».
- Los `$anchor` deben ser únicos en el documento (error `duplicate-anchor`, [referencias.md](referencias.md) §2). No es más estricto que 2020-12, que exige anclas únicas en cada recurso; como `$id` solo se admite en la raíz, el documento es un solo recurso.

Las demás comprobaciones de §3 (`$id` sin fragmento, `$anchor`, nombres y unicidad de `type`, `multipleOf` > 0, enteros no negativos, unicidad en `required` y `dependentRequired`, `minItems: 1` en `prefixItems`/`allOf`/`anyOf`/`oneOf`) son reglas del meta-esquema oficial, no añadidos.

Igual que el oficial, a propósito:

- `enum` vacío se acepta (D11): el oficial no pone `minItems` (la especificación solo dice que el arreglo *debería* tener un elemento). No acepta ningún valor, igual que el esquema `false`. Una versión anterior de esta tarea lo rechazaba; se quitó por D11.
- `enum` con valores repetidos se acepta (la unicidad es solo una recomendación y no cambia el resultado).
- `format` es una anotación y no se valida; tampoco se valida la sintaxis URI de `$schema`, `$id` y `$ref` más allá de lo indicado (en el oficial son `format: uri`/`uri-reference`, que también son anotaciones).

No se comprueba porque **no es** del meta-esquema: la satisfacibilidad (`minLength` > `maxLength`, `minimum` > `maximum`, `required` de una propiedad prohibida…). Un esquema insatisfacible es un esquema válido que no acepta ningún payload.

## 5. Mensajes de las palabras clave rechazadas (D6)

El mensaje nombra la palabra clave, su **categoría** y el **motivo**; la tabla está en `unsupported.rs`:

| Palabras clave | Categoría | Motivo (resumen) |
|---|---|---|
| `if`, `then`, `else` | (c) | el resultado de `if` elige entre `then` y `else`: tres subesquemas evaluados a la vez (producto) |
| `contains` | (c) | cuantificador existencial sobre los elementos con contador de coincidencias (producto); candidato a añadirse |
| `minContains`, `maxContains` | (b) | cuentan las coincidencias de `contains`, que está fuera |
| `patternProperties` | (c) | una clave puede coincidir con varias expresiones y debe cumplir todos sus esquemas; candidato |
| `propertyNames` | (c) | aplica un subesquema a cada nombre de clave (producto con el trie); candidato |
| `unevaluatedProperties`, `unevaluatedItems` | (c) | dependen de las anotaciones de todos los demás aplicadores, también de las ramas que fallan |
| `$dynamicRef`, `$dynamicAnchor` | núcleo | se resuelven según el ámbito dinámico, no al compilar |
| `$vocabulary` | núcleo | solo tiene sentido en un meta-esquema propio |
| `contentEncoding`, `contentMediaType`, `contentSchema` | anotación | el validador no decodifica el contenido; se rechaza para que no parezca validado |
| `additionalItems`, `dependencies`, `$recursiveRef`, `$recursiveAnchor` | borrador anterior | indica la palabra clave de 2020-12 que la reemplaza |
| cualquier otra | desconocida | no es de 2020-12 ni del subconjunto; si está a distancia de edición ≤ 2 de una soportada se sugiere (`minLenght` → «¿quiso decir "minLength"?») |

Ejemplo:

```
línea 3, columna 3: palabra clave "if" no soportada (clase (c), no determinista o no libre de contexto):
aplicación condicional: el resultado de `if` decide si el valor debe cumplir `then` o `else`, así que
hay que evaluar los tres subesquemas a la vez (producto de autómatas)
```

(en una sola línea; aquí se partió para leerlo). Las formas de §2.8 que no son palabras clave se rechazan en su capa: `$ref` remoto aquí (`unsupported`, «forma de "$ref" fuera del subconjunto»), y la forma booleana de `exclusiveMinimum`/`exclusiveMaximum` y la de arreglo de `items` en el parser (`syntax`, con una pista).

El valor de una palabra clave rechazada no se recorre (puede ser cualquier JSON), y tampoco el de `const`, `enum`, `default` y `examples`, que son literales: `{"const": {"if": 1}}` es válido. Un nombre de propiedad igual a una palabra clave (`"properties": {"if": …}`) tampoco es una palabra clave.

## 6. Errores, orden y costo

- Cada error es un `SemanticError { kind, at }` con el mismo formato que los del parser: `línea L, columna C: …`, en una sola línea, con los textos del usuario escapados y recortados a 32 caracteres (`schema_parser::error::short`). `code()` es `unsupported` (palabra clave o forma fuera del subconjunto) o `meta-schema` (valor que viola el meta-esquema restringido).
- **Posición:** el nombre de la palabra clave cuando el problema es la palabra clave (`unsupported`, `$id`/`$schema` fuera de la raíz); el valor en los demás casos; dentro de un arreglo, el elemento culpable (la **segunda** aparición de una cadena repetida, como en las claves repetidas del parser); en un arreglo vacío, su `[`.
- **Todos los errores, no solo el primero**, ordenados por offset. El parser se detiene en el primero porque después de un error sintáctico el resto de la entrada no tiene estructura fiable; aquí el AST está completo y cada comprobación es independiente, así que informarlos todos de una vez le ahorra al usuario una ronda por error.
- **Costo:** O(n) en el tamaño del AST (cada palabra clave se visita una vez; la unicidad usa una tabla hash, O(1) esperado por cadena), más O(e log e) para ordenar los e errores. La sugerencia de las desconocidas compara solo nombres de hasta 32 caracteres con las 42 palabras clave: O(1) por palabra clave.
- **Pila (D7):** el recorrido es **iterativo**, con una pila explícita en el montón, así que no consume pila del hilo en proporción a la profundidad del esquema. Las comprobaciones son locales a cada palabra clave (solo se arrastra si el esquema es la raíz), por eso el orden de visita no importa y basta con ordenar al final. La profundidad sigue acotada por el parser (256); `schemac check` llama a parser y validación en un hilo con 8 MiB de pila explícita, nunca en el hilo principal.

## 7. Pruebas

- `crates/semantic/tests/meta.rs`: una prueba por cada palabra clave de `00-alcance.md` §2.8 (y por las de borradores anteriores) que comprueba código, posición, categoría y mensaje; las formas de §2.8; anotaciones de la lista blanca en la raíz y en subesquemas; rangos numéricos en el borde; nombres de `type`; repetidos; núcleo (`$schema`, `$id`, `$anchor`, `$ref`); orden de los errores; profundidad máxima (256) en un hilo de 2 MiB y un AST de 2·10⁵ niveles construido a mano en un hilo de 256 KiB; propiedades: ninguna clave se ignora en silencio, y la validación no falla y ordena sus errores sobre bytes arbitrarios y sobre esquemas generados.
- `crates/semantic/tests/corpus.rs`: los esquemas de `tests/schemas/parse-ok` y de `tests/cases` pasan, salvo dos de `parse-ok` que existen para probar el parser y fallan aquí a propósito (`combinadores.json` usa `"oneOf": []`, `fuera-del-subconjunto.json` usa `if`/`then`/`x-extension`; sus errores exactos están fijados en la prueba); los de `tests/schemas/semantic-error/` fallan con el código, la línea, la columna, el fragmento de mensaje y el número de errores de `tests/schemas/errores-semanticos.json`.
- `crates/semantic/src/number.rs` y `unsupported.rs`: pruebas unitarias y de propiedades del análisis de lexemas y de la sugerencia.
- `crates/cli/tests/check.rs`: `schemac check` con esquemas válidos, erróneos en cada fase, profundos e inexistentes, y con una ruta que no es UTF-8 (`ruta_no_utf8`: código 2, sin pánico).

## 8. Pendiente

- Magnitud de los contadores: `"maxLength": 1e400` es válido según el meta-esquema. La IR (tarea posterior) tendrá que acotarlo al tamaño de sus registros o tratarlo como «sin límite»; no es un error del esquema.
- `pattern`: la comprobación del subconjunto de regex va con la tarea `pattern` → AFD (semana 3), que es la que tiene el analizador de expresiones regulares.
- Resolución de `$ref` y de `$anchor` (que el destino exista, anclas repetidas): **hecha** en la tarea de la tabla de símbolos, [referencias.md](referencias.md).
