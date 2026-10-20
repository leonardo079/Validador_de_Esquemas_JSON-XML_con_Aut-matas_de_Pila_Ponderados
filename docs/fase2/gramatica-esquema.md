# Fase 2 — Gramática del metalenguaje de esquemas (JSON Schema)

> **Cifras manuales.** Los archivos `ll1-*.md` los genera `scripts/ll1.py` (`make docs`) y la CI los comprueba (`make docs-check`). En cambio, el número de clases `K_*` de §1 y la tabla de §3 (FIRST y FOLLOW) de este documento son un resumen escrito a mano de esa salida: quien cambie la gramática (`esquema.ll1`) debe actualizarlos en el mismo cambio y compararlos con el `ll1-esquema.md` regenerado.

Un esquema JSON es un documento JSON, así que podría analizarse con la gramática de [gramatica-json.md](gramatica-json.md) y revisar después qué hay dentro. El metalenguaje es más estrecho: la **palabra clave** de cada miembro decide qué forma tiene su valor (`minimum` lleva un número, `properties` un objeto de esquemas, `allOf` un arreglo de esquemas, …). Esa información se mete en la gramática para que el parser la compruebe mientras lee, en una sola pasada y con la posición exacta del primer token que no encaja.

Implementación: `crates/schema-parser` ([`json_schema.rs`](../../crates/schema-parser/src/json_schema.rs)), un parser descendente recursivo escrito a mano con una función por no terminal. Lee los tokens del tokenizador JSON de la Fase 1.

## 1. Terminales

Los terminales son los tokens del lexer JSON (`{ } [ ] : ,` `STRING` `NUMBER` `true` `false` `null`) más doce **clases de palabra clave** `K_*`.

Una `K_*` no es un token nuevo del lexer: es un `STRING` que aparece **como clave de un objeto-esquema**, clasificado por su valor. La partición es finita y la decide una tabla (`KEYWORDS` en [`ast.rs`](../../crates/schema-parser/src/ast.rs)), lo mismo que hace un compilador para distinguir palabras reservadas de identificadores:

| Clase | Palabras clave | Forma del valor |
|---|---|---|
| `K_STRING` | `$schema` `$id` `$anchor` `$ref` `$comment` `title` `description` `format` `pattern` | `STRING` |
| `K_NUMBER` | `minimum` `maximum` `exclusiveMinimum` `exclusiveMaximum` `multipleOf` `minLength` `maxLength` `minItems` `maxItems` `minProperties` `maxProperties` | `NUMBER` |
| `K_BOOL` | `uniqueItems` `deprecated` `readOnly` `writeOnly` | `true` \| `false` |
| `K_SCHEMA` | `items` `additionalProperties` `not` | un esquema |
| `K_MAP` | `$defs` `definitions` `properties` `dependentSchemas` | objeto nombre → esquema |
| `K_LIST` | `prefixItems` `allOf` `anyOf` `oneOf` | arreglo de esquemas |
| `K_VALUE` | `const` `default` | cualquier valor JSON |
| `K_VALUES` | `enum` `examples` | arreglo de valores JSON |
| `K_TYPE` | `type` | `STRING` o arreglo de `STRING` |
| `K_REQUIRED` | `required` | arreglo de `STRING` |
| `K_DEPREQ` | `dependentRequired` | objeto nombre → arreglo de `STRING` |
| `K_OTHER` | cualquier otra cadena | cualquier valor JSON |

La clasificación depende del **contexto**: la misma cadena `"if"` es `K_OTHER` como clave de un esquema y un `STRING` cualquiera como nombre dentro de `properties` (ver `fuera-del-subconjunto.json` en el corpus). El parser solo la aplica en el no terminal `keyword`, que es el único cuya fila de la tabla tiene columnas `K_*`. Formalmente, la gramática trabaja sobre el alfabeto refinado; borrar las clases (el homomorfismo `K_* ↦ STRING`) convierte cada derivación en una derivación de la gramática de JSON, así que **todo esquema que el parser acepta es un documento JSON**.

`K_OTHER` existe para que las palabras clave fuera del subconjunto (`if`, `contains`, `x-algo`, …) **no se pierdan**: el parser las conserva en el AST como `Kw::Unsupported` con su nombre, su posición y su valor, y la Fase 3 las rechaza con un mensaje que las nombra (`docs/00-alcance.md`: nunca se ignoran en silencio). Como su valor puede ser cualquier cosa, la Fase 2 solo exige que sea JSON.

## 2. Gramática LL(1)

```
schema         → { keywords } | true | false
keywords       → keyword keywords_tail | ε
keywords_tail  → , keyword keywords_tail | ε
keyword        → K_STRING : STRING   | K_NUMBER : NUMBER     | K_BOOL : boolean
               | K_SCHEMA : schema   | K_MAP : schema_map    | K_LIST : [ schemas ]
               | K_VALUE : value     | K_VALUES : array      | K_TYPE : type_value
               | K_REQUIRED : string_array | K_DEPREQ : dep_map | K_OTHER : value
boolean        → true | false
type_value     → STRING | string_array
string_array   → [ strings ]
strings        → STRING strings_tail | ε
strings_tail   → , STRING strings_tail | ε
schema_map     → { entries }
entries        → STRING : schema entries_tail | ε
entries_tail   → , STRING : schema entries_tail | ε
schemas        → schema schemas_tail | ε
schemas_tail   → , schema schemas_tail | ε
dep_map        → { deps }
deps           → STRING : string_array deps_tail | ε
deps_tail      → , STRING : string_array deps_tail | ε
value … elements_tail   (las ocho reglas de json.ll1, sin cambios)
```

Símbolo inicial: `schema`. Archivo fuente: [esquema.ll1](esquema.ll1). Los conjuntos FIRST y FOLLOW y la tabla LL(1) los calcula `scripts/ll1.py` (`make docs`) y están en **[ll1-esquema.md](ll1-esquema.md)**.

Se escribió directamente sin recursión por la izquierda (cada lista es `X → item X_tail | ε`, `X_tail → , item X_tail | ε`, como `members` en la gramática de JSON) y sin prefijos comunes entre alternativas: la lista vacía es una alternativa ε de la propia lista, así que `{ }` y `{ keywords }` no compiten.

## 3. Verificación LL(1)

Condiciones, para cada no terminal A con alternativas α, β: (1) FIRST(α) ∩ FIRST(β) = ∅; (2) si β ⇒* ε, FIRST(α) ∩ FOLLOW(A) = ∅.

| No terminal | FIRST de cada alternativa | FOLLOW | ¿Disjuntos? |
|---|---|---|---|
| `schema` | {`{`} · {`true`} · {`false`} | {`,` `]` `}` `$`} | Sí; ninguna deriva ε |
| `keywords` | {doce `K_*`} · ε | {`}`} | Sí: `}` no es una `K_*` |
| `keywords_tail` | {`,`} · ε | {`}`} | Sí |
| `keyword` | {`K_STRING`} · {`K_NUMBER`} · … · {`K_OTHER`} | {`,` `}`} | Sí: doce clases distintas, una por alternativa |
| `boolean` | {`true`} · {`false`} | {`,` `}`} | Sí |
| `type_value` | {`STRING`} · {`[`} | {`,` `}`} | Sí |
| `strings` | {`STRING`} · ε | {`]`} | Sí |
| `strings_tail`, `schemas_tail` | {`,`} · ε | {`]`} | Sí |
| `entries`, `deps` | {`STRING`} · ε | {`}`} | Sí |
| `entries_tail`, `deps_tail` | {`,`} · ε | {`}`} | Sí |
| `schemas` | {`{` `true` `false`} · ε | {`]`} | Sí: `]` ∉ FIRST(schema) |
| `value` … `elements_tail` | como en [gramatica-json.md](gramatica-json.md) §3 | | Sí |

La disjunción de las alternativas de `keyword` se apoya en que las clases son una **partición** de las cadenas: cada nombre está en una sola fila de la tabla de §1 y `K_OTHER` es el complemento de la unión de las demás. El script llega a la misma conclusión: ninguna celda de [ll1-esquema.md](ll1-esquema.md) tiene dos producciones.

## 4. Qué comprueba la Fase 2 y qué deja a la Fase 3

La gramática fija la **forma** de cada valor; lo que no es libre de contexto, o depende del valor y no del tipo de token, es semántico:

| Comprobación | Fase |
|---|---|
| JSON bien formado (léxico y sintaxis) | 2 |
| Forma del valor de cada palabra clave (número, esquema, arreglo de esquemas…) | 2 |
| Formas excluidas por el alcance: `items` como arreglo, `exclusiveMinimum`/`exclusiveMaximum` booleanos | 2 (con una pista que lo explica) |
| Claves repetidas en un mismo objeto (palabras clave, entradas de un mapa, miembros de un valor JSON) | 2, como restricción aparte |
| Profundidad de anidamiento ≤ 256 | 2 |
| Palabras clave fuera del subconjunto (`K_OTHER`) | 3 (rechazo con nombre, posición, categoría y motivo) |
| Rangos: `minLength` entero ≥ 0, `multipleOf` > 0, … | 3 |
| Nombres de `type` válidos, sin repetidos en `type`, `required` y `dependentRequired`, arreglos no vacíos (`type` y arreglos de esquemas) | 3 |
| `$schema` con el URI de 2020-12, `$id` solo en la raíz, `$ref` local | 3 |
| `$ref` resoluble, `$anchor` sin repetir | 3 (tabla de símbolos, [../fase3/referencias.md](../fase3/referencias.md)) |
| Ciclos no productivos | 3 (tarea siguiente) |
| `pattern` dentro del subconjunto de regex | 3 (con la compilación de `pattern` a AFD) |

La tabla completa de lo que comprueba la Fase 3 contra el meta-esquema, y por qué no está en la gramática, está en [../fase3/meta-esquema.md](../fase3/meta-esquema.md).

**Claves repetidas.** La RFC 8259 dice que los nombres de un objeto *deberían* ser únicos, y 2020-12 no define qué significa un esquema con `"type"` dos veces. En lugar de quedarse con la primera o la última aparición, el parser lo rechaza en la segunda. La regla vale en **cualquier** objeto del esquema, también en los valores literales de `const` y `enum` (decisión D2 del plan): el PDF del curso lista las claves duplicadas como caso malformado, y el validador generado también las rechaza en los payloads (`docs/00-alcance.md` §1), así que el proyecto aplica la misma regla a esquema y payload. No lo expresa la gramática (como la unicidad de atributos en [gramatica-xml.md](gramatica-xml.md)): lo comprueba el parser al leer cada clave, contra el conjunto de claves ya vistas en el mismo objeto.

*Costo.* Cada objeto (objeto-esquema, mapa de `properties`/`$defs`/`dependentSchemas`/`dependentRequired` o valor JSON dentro de `const`, `enum`…) lleva una tabla hash (`HashSet<String>`) con sus claves: insertar y consultar cuesta O(1) esperado por clave, más O(ℓ) por copiar la clave de longitud ℓ, así que la comprobación es **lineal en el tamaño del esquema** y el análisis completo sigue siendo O(n). La tabla usa el hash por defecto de `std` (SipHash con semilla aleatoria por proceso), para que un esquema hostil no pueda fabricar colisiones y degradarla a cuadrática. Una primera versión comparaba cada clave con todas las anteriores (O(k²) para k claves): con 10⁵ claves en `properties` tardaba unos 18,6 s en release, lo que era una denegación de servicio barata porque los esquemas los sube el usuario en la web. Con la tabla, medido en release con `{"properties": {"p0": true, …}}`:

| Claves | Tamaño | Tiempo |
|---|---|---|
| 10⁴ | 125 KB | 4,8 ms |
| 10⁵ | 1,3 MB | 55 ms |
| 10⁶ | 14,2 MB | 0,62 s |

La prueba `muchas_claves_en_tiempo_lineal` no usa un tope absoluto de tiempo, que dependería de la máquina. Mide objetos de N = 10⁴ y 4N claves en `properties`, `const`, `$defs` y `dependentRequired` y exige t(4N)/t(N) < 10 (lineal ≈ 4; la búsqueda cuadrática reintroducida midió 16,8 en debug y 26,4–27,9 en release). Cada estimación de la razón usa lotes calibrados para que cada muestra dure al menos ~50 ms, el mismo tiempo para N (4 veces más análisis) que para 4N, y el mínimo de 3 muestras intercaladas por lado; la estimación se repite hasta 3 veces y la prueba falla solo si las tres superan el umbral. Como mide **tiempo de pared**, con la CPU sobresuscrita no es fiable en ninguna de las dos direcciones: con 32 procesos `yes` en 8 núcleos y el parser lineal intacto se midieron razones de 1,1 a 15,9 (falsos fallos posibles), y con la regresión cuadrática el auditor midió 8,99 en algún intento (falso aprobado). Por eso **está marcada `#[ignore]`**: las pruebas de `cargo test` y de la CI (runners compartidos, también Windows) deben ser deterministas, y las mediciones de tiempo no forman parte de ellas. Se corre a mano, en una máquina sin carga y en release, donde detecta la regresión de forma estable (26,4–27,2 en los tres intentos):

```text
cargo test -p schema-parser --release --test parser -- --ignored muchas_claves_en_tiempo_lineal
```

Con la regresión presente tarda en fallar unos 70–75 s en release. La parte determinista de la antigua prueba, que una clave repetida al final de un objeto de 4N claves se detecta en su offset exacto, pasó a `clave_repetida_al_final_de_objeto_grande` y corre siempre. La medición de rendimiento definitiva del parser corresponde al crate `bench` (criterion); ver [pendientes.md](pendientes.md) §1.

**Números.** Se guardan como el lexema decimal exacto (`Num::lexeme`), nunca como `f64`, para que la Fase 4 compare `minimum` o `multipleOf` sin perder precisión (`docs/00-alcance.md` §1.1).

## 5. Errores con línea y columna

El parser se detiene en el **primer** error y lo devuelve como `ParseError { kind, at }`, donde `at` es la posición (offset, línea, columna) del token que no encaja. La columna cuenta puntos de código, no bytes, y un BOM inicial no ocupa columna. El mensaje tiene la forma

```
línea 3, columna 12: se esperaba un esquema (objeto `{…}`, `true` o `false`) como valor de "items",
se encontró `[` (la forma de arreglo de `items` (Draft 2019-09) está fuera del subconjunto; use `prefixItems`)
```

| `code()` | Causa | Posición |
|---|---|---|
| `lexical` | error del tokenizador de la Fase 1 | la del lexer (el byte, o el inicio del escape o de la secuencia UTF-8) |
| `syntax` | celda vacía de la tabla LL(1); el mensaje lista lo que admitía la fila | primer byte del token encontrado; al final de la entrada, la posición siguiente al último byte |
| `duplicate-key` | clave repetida | la segunda aparición |
| `limit` | anidamiento mayor que el máximo | el `{` o `[` que lo supera |

Los textos del usuario que aparecen en un mensaje (cadenas, números, claves repetidas, nombres de palabra clave) se escapan con `escape_debug` y se recortan a 32 caracteres, así que el mensaje ocupa siempre **una línea** y no lleva caracteres de control hacia la web.

**Costo.** Detectar los errores no cambia la complejidad del análisis: el lexer es O(1) por byte, cada token se decide con una consulta a la tabla LL(1), y la comprobación de claves repetidas es O(1) esperado por clave (§4). El análisis entero es O(n) en el tamaño del esquema, y el primer error se reporta en cuanto se lee el token que lo provoca.

El parser pide los tokens de uno en uno: el lexer se alimenta byte a byte hasta completar el siguiente. Así, si un error sintáctico ocurre antes en la entrada que uno léxico, se reporta el sintáctico (`{"a" 1x}` falla en el `1`, columna 6, no en la `x`). El fin de un número solo se conoce al ver el byte siguiente; si ese byte es inválido, el lexer falla con el número ya emitido, y el parser entrega primero el número y después el error léxico.

## 6. Profundidad y pila

El parser es descendente **recursivo**: su pila de llamadas es la pila del autómata (§5 de [gramatica-json.md](gramatica-json.md)), y crece con el anidamiento de `{`/`[`. Un esquema hostil con un millón de `[` dentro de `const` desbordaría la pila del proceso, lo que sería una caída y no un error. Por eso cada `{` o `[` incrementa un contador acotado por `max_depth` (256 por defecto, el mismo valor por defecto de los payloads en `docs/00-alcance.md` §4; configurable a la baja con `parse_with_max_depth`, con tope `MAX_DEPTH_LIMIT` = 256), y superarlo es el error `limit`. `MAX_DEPTH_LIMIT` limita solo esta recursión del compilador sobre el esquema. La profundidad de los payloads (`SV_MAX_DEPTH`, Fase 5) es independiente: el validador generado usa una pila explícita de tamaño fijo, no recursión, así que su límite es configurable (`docs/00-alcance.md` §4) y solo lo acota la memoria que se reserve para esa pila.

Además, el límite tiene que caber en la pila real. Se midió la pila mínima con la que el parser termina en el peor caso (cadena de `not` anidados, la de más marcos por nivel), con un hilo de tamaño controlado:

| Compilación | Pila por nivel | A profundidad 256 |
|---|---|---|
| `release` | ~1,0 KiB | ~250 KiB |
| `dev` (sin optimizar) | ~5,4 KiB | ~1,34 MiB |

Las dos caben en los 2 MiB que Rust y tokio reservan por defecto para cada hilo. En la primera versión el modo `dev` gastaba ~12 KiB por nivel y desbordaba esos 2 MiB; para bajar a 5,4 KiB, el error viaja en una caja dentro del parser (`Result<T, Box<ParseError>>`), cada alternativa de `keyword` tiene su propia función y la construcción de mensajes está en funciones `#[cold]`. La tabla de claves de §4 añade un `HashSet` vacío (sin reservar memoria) al marco de `delimited`; medido de nuevo, el costo por nivel no cambió. La prueba `profundidad_maxima` lo vigila: analiza esquemas a la profundidad máxima en un hilo de exactamente 2 MiB.

**Tope de la profundidad configurable.** `parse_with_max_depth` recorta el valor pedido a `MAX_DEPTH_LIMIT` = 256. Sin el recorte, `parse_with_max_depth(…, u32::MAX)` con un millón de `[` desbordaba la pila (*thread has overflowed its stack … aborting*): una caída, no un error. Se eligió 256 porque es la mayor profundidad cuyo peor caso está medido y cabe en 2 MiB también en `dev` (1,34 MiB); 384 niveles ya pasarían de 2 MiB sin optimizar, así que subir el tope exige primero una pila explícita (ver [pendientes.md](pendientes.md) §5). Se recorta en lugar de devolver un error de configuración porque `ParseError` siempre señala una posición de la entrada y un parámetro fuera de rango no la tiene. El recorte no pasa desapercibido: si la entrada lo supera, el error `limit` (`TooDeep { max }`) informa el máximo efectivo, 256, no el pedido. Los valores menores que 256 se respetan tal cual. La garantía vale en hilos con al menos 2 MiB de pila: el hilo principal de Windows tiene 1 MiB, así que `schemac` y el servidor (y cualquier llamador desde un hilo de pila desconocida) deben llamarlo en un hilo creado con `std::thread::Builder::stack_size` de 2 MiB o más (ver [pendientes.md](pendientes.md) §5). La prueba `profundidad_configurable_acotada` lo verifica con `u32::MAX`, `MAX_DEPTH_LIMIT + 1` y 10⁵ sobre un millón de `[` y sobre 2·10⁵ `not` anidados, en un hilo de 2 MiB.

## 7. Ejemplo de derivación

Derivación por la izquierda de `{"type": "array", "items": {"minimum": 0}}`, con las decisiones de la tabla:

```
schema
⇒ { keywords }                                            (schema, {)
⇒ { keyword keywords_tail }                               (keywords, K_TYPE)
⇒ { K_TYPE : type_value keywords_tail }                   (keyword, K_TYPE)
⇒ { K_TYPE : STRING keywords_tail }                       (type_value, STRING)
⇒ { K_TYPE : STRING , keyword keywords_tail }             (keywords_tail, ,)
⇒ { K_TYPE : STRING , K_SCHEMA : schema keywords_tail }   (keyword, K_SCHEMA)
⇒ { K_TYPE : STRING , K_SCHEMA : { keywords } keywords_tail }                         (schema, {)
⇒ { K_TYPE : STRING , K_SCHEMA : { keyword keywords_tail } keywords_tail }            (keywords, K_NUMBER)
⇒ { K_TYPE : STRING , K_SCHEMA : { K_NUMBER : NUMBER keywords_tail } keywords_tail }  (keyword, K_NUMBER)
⇒ { K_TYPE : STRING , K_SCHEMA : { K_NUMBER : NUMBER } keywords_tail }                (keywords_tail, })
⇒ { K_TYPE : STRING , K_SCHEMA : { K_NUMBER : NUMBER } }                              (keywords_tail, })
```

Con `"items": [{"minimum": 0}]` la derivación se detiene en el paso 7: la celda (`schema`, `[`) está vacía, y el error señala el `[`.

## 8. Correspondencia con el código

| No terminal | Función en `json_schema.rs` |
|---|---|
| `schema` | `Parser::schema` |
| `keywords`, `keywords_tail` y las demás listas | `Parser::delimited` (un bucle en lugar de la recursión por la cola) |
| `keyword` | `Parser::keyword` + `Parser::keyword_value` (una rama por clase) |
| `schema_map` / `entries` | `Parser::schema_map` + `Parser::schema_entry` |
| `schemas` | `Parser::schema_list` |
| `dep_map` / `deps` | `Parser::dep_map` + `Parser::dep_entry` |
| `string_array` / `strings` | `Parser::string_array` |
| `value`, `object`, `pair`, `array` | `Parser::json`, `Parser::pair`, `Parser::json_array` |

Cada función decide con `self.look` (un token) y, si el token no está en la fila de la tabla, construye el error con lo que la fila admitía.

## 9. Corpus

- `tests/schemas/parse-ok/`: esquemas que deben parsear (booleanos, BOM, Unicode y escapes, todas las clases de palabra clave, recursión con `$defs`, palabras clave fuera del subconjunto).
- `tests/schemas/parse-error/` + `tests/schemas/errores.json`: esquemas erróneos con la clase, la línea y la columna esperadas, calculadas a mano.
- Los esquemas JSON de `tests/cases/` también deben parsear.

Lo ejecuta `crates/schema-parser/tests/corpus.rs`.
