# Fase 3 — Ciclos no productivos

Tercera tarea del análisis semántico. Entrada: la tabla de símbolos con todos los `$ref` resueltos ([referencias.md](referencias.md)). Salida: nada si no hay ciclos no productivos, o un error con línea, columna y el camino del ciclo por cada componente que los tenga.

Implementación: [`crates/semantic/src/cycles.rs`](../../crates/semantic/src/cycles.rs) (`check_cycles`, `application`). `semantic::analyze` la ejecuta después de la tabla de símbolos, y `schemac check` también.

## 1. Consumir entrada

Validar un payload contra un esquema es recorrer a la vez el payload y el esquema: cada subesquema se aplica a una **instancia** (un valor del payload). Una palabra clave con subesquemas los aplica a una de dos cosas:

- a la **misma instancia** que el esquema que la contiene: para pasar del esquema al subesquema no hace falta leer nada del payload;
- a una **instancia hija** (un miembro de un objeto o un elemento de un arreglo): para llegar a ella hay que leer al menos un token del padre (`{` y la clave con `:`, o `[` y quizá `,`). Esa arista **consume entrada**.

## 2. Clasificación de las palabras clave con subesquemas

Criterio: ¿el subesquema se aplica a la misma instancia o a una hija? (Core y Validation 2020-12, definición de cada aplicador.)

| Palabra clave | Se aplica a | Arista | Por qué |
|---|---|---|---|
| `$ref` | la misma instancia | no consume | En 2020-12 `$ref` es un aplicador más: el esquema referenciado se aplica a la instancia actual, junto con las palabras clave hermanas |
| `allOf`, `anyOf`, `oneOf` | la misma instancia | no consume | Cada rama valida el mismo valor |
| `not` | la misma instancia | no consume | Valida el mismo valor e invierte el resultado |
| `dependentSchemas` | la misma instancia (el objeto) | no consume | Si el objeto tiene la propiedad, el subesquema valida **el objeto entero**, no el valor de la propiedad |
| `properties` | el valor de un miembro | consume | Hay que leer `{`, la clave y `:` |
| `additionalProperties` | el valor de un miembro no declarado | consume | Ídem |
| `items`, `prefixItems` | un elemento | consume | Hay que leer `[` (y las `,` anteriores) |
| `$defs`, `definitions` | nada | ninguna | Son definiciones: no se aplican, solo se alcanzan por un `$ref` |

`application(kw)` devuelve esta clasificación (`SameInstance`, `ChildInstance`, `None`). La prueba `clasificacion_completa` comprueba que toda palabra clave cuya clase lleva esquemas (`K_SCHEMA`, `K_MAP`, `K_LIST`) esté clasificada, salvo `$defs`/`definitions`. Las palabras clave fuera del subconjunto no llegan aquí (las rechaza el meta-esquema), así que no hace falta clasificar `if`/`then`/`contains`… (`then` y `else` serían misma instancia, `contains` una hija).

**`$ref` junto a otras palabras clave.** El alcance (`00-alcance.md` §2.1) no restringe los hermanos de `$ref`, así que se sigue 2020-12: `$ref` es una arista que no consume aunque el objeto tenga más palabras clave. Por eso `{"type": "object", "properties": {"h": {"items": {"$ref": "#/$defs/n"}}}, "$ref": "#/$defs/n"}` como definición `n` es un ciclo (su propio `$ref` es una autoarista), aunque la recursión por `items` sea válida (`ciclo-con-hermanos.json`).

## 3. Definición formal

Sea T la tabla de símbolos con subesquemas N, padre y ρ. El **grafo que no consume** es G₀ = (N, E₀) con

E₀ = { (padre(*t*), *t*) : la palabra clave del paso a *t* aplica a la misma instancia } ∪ { (*s*, ρ(*s*)) : *s* tiene `$ref` }.

Un **ciclo no productivo** es un ciclo de G₀ (incluidas las autoaristas, como `{"$ref": "#"}` en la raíz o `x → #x`). Equivalentemente, *s* está en un ciclo no productivo si y solo si existe un camino no vacío de *s* a *s* en G₀: desde *s* se vuelve a *s* sin consumir ningún token.

Toda arista estructural va de un padre a un hijo, y el hijo tiene un id mayor (preorden, [referencias.md](referencias.md) §2). Un ciclo no puede estar hecho solo de aristas que aumentan el id, así que **todo ciclo de G₀ contiene al menos una arista de `$ref`**. Sin `$ref` no hay ciclos.

**Alcance del rechazo (decisión D12 del plan).** Se rechazan todos los ciclos de G₀, también los que no se alcanzan desde la raíz (dos definiciones de `$defs` que se referencian entre sí sin que nadie las use). Core 2020-12 («Guarding Against Infinite Recursion») dice que los esquemas no deberían usar una recursión infinita así y que el comportamiento es indefinido, así que el error está permitido. El plan pide rechazarlos (Fase 3: «Ese se rechaza»; el PDF pide mitigar los `$ref` cíclicos), y hacerlo sin mirar la alcanzabilidad da un criterio que solo depende del texto y protege a las fases siguientes, que pueden bajar todas las definiciones a la IR.

## 4. Por qué se rechazan: el autómata no terminaría

En la construcción composicional del DPDA (Fase 4), aplicar un subesquema a la misma instancia no lee ningún símbolo:

- un `$ref` es una «llamada»: una transición ε que apila el punto de retorno y salta al subautómata del destino;
- `allOf`/`anyOf`/`oneOf`/`not` combinan subautómatas que leen **la misma** entrada (producto, complemento), también sin leer nada para entrar en ellos.

Un ciclo de G₀ se convierte así en un **ciclo de transiciones ε**:

- Si cada vuelta apila un marco (`$ref`), el autómata puede hacer infinitas transiciones ε apilando sin leer ni un byte: la pila crece sin límite (en el validador generado, desbordaría `SV_MAX_DEPTH`) y la validación no termina, en lugar de ser O(n).
- Si el ciclo pasa por un combinador, la construcción del producto no termina: el producto de A con un subautómata que contiene a A otra vez se desenrolla sin fin.
- Lo que se pierde no es el determinismo: un DPDA puede tener ciclos de transiciones ε y seguir siendo determinista (a lo sumo una transición por (q, a, X) y ninguna ε en conflicto con una que lee). Se pierden la **terminación** (el autómata puede quedarse para siempre sin leer, o sin decidir al final de la entrada) y la **cota O(d) de la pila**, que son invariantes del proyecto (validación O(n) en bytes y O(d) en pila).

Que G₀ sea acíclico es una condición **necesaria** para lo anterior, no suficiente. Lo que sí garantiza: entre dos aristas que consumen, el camino de G₀ es simple, así que tiene menos de |N| aristas, y por cada nivel de anidamiento del payload se apilan a lo sumo |N| marcos de llamada. La altura de la pila de llamadas queda en O(|N| · d), con d la profundidad del payload; como |N| es una constante del esquema, es **O(d)**, el invariante del proyecto. Es la versión, para este proyecto, de la condición clásica de que una gramática sin ciclos A ⇒⁺ A se pueda analizar sin bucles. La recursión **productiva** (árboles por `properties` o `items`) es justamente la que usa la pila de forma acotada por la profundidad del payload.

Lo que no garantiza: que todo el esquema se pueda compilar a un solo DPDA. Un `allOf` (o un `anyOf`/`oneOf` ambiguo) de dos esquemas con recursión productiva exige el producto de dos autómatas de pila, que en general no es un autómata de pila: los lenguajes deterministas libres de contexto no son cerrados bajo intersección. Esos casos son de clase (c) y van al producto acotado o a la verificación diferida (`00-alcance.md` §2.7, Fases 3 y 4); esta tarea solo quita el obstáculo previo, los ciclos que no consumen.

Para el informe: el rechazo de los ciclos no productivos es la mitigación de los «`$ref` cíclicos» del PDF. Un esquema subido a la aplicación web no puede hacer que el compilador o el validador generado entren en un bucle infinito ni desborden la pila por recursión sin consumir entrada.

## 5. Algoritmo

1. **Grafo.** Las aristas de E₀ salen de la tabla en O(V + E): una por subesquema cuyo paso es de misma instancia y una por `$ref`. Se guardan en formato CSR (desplazamientos y destinos, ordenados por origen con un conteo), con la etiqueta de cada arista (la palabra clave, o `$ref`).
2. **Componentes fuertemente conexas.** Tarjan **iterativo**: la pila de llamadas del DFS es un `Vec<(nodo, siguiente arista)>` en el montón, así que el consumo de pila del hilo no depende del tamaño del grafo (D7). Una componente tiene un ciclo si tiene más de un nodo o un nodo con autoarista. O(V + E).
3. **Un error por componente con ciclo.** El **representante** es el `$ref` que aparece **primero en el texto** entre los que tienen origen y destino dentro de la componente (por lo anterior, siempre hay al menos uno). El error se señala en el valor de ese `$ref`. El camino que se muestra empieza en el esquema que contiene ese `$ref`, sigue por él y vuelve al inicio por el camino **más corto** dentro de la componente (BFS; los vecinos se recorren en el orden del CSR, así que el camino es determinista). Como las componentes son disjuntas, todas las búsquedas juntas visitan cada nodo y cada arista una vez: O(V + E).
4. **Construcción de los errores.** Cada error guarda a lo sumo `SHOWN_EDGES` + 1 = 9 fragmentos, y cada uno **ya recortado** a `FRAGMENT_BYTES` = 80 bytes con `SymbolTable::fragment_clipped` (su principio, `…` y su final). Ese método no construye nunca el puntero entero: sube por los padres, O(profundidad), y codifica solo los caracteres que caben, así que cuesta O(profundidad + 80) aunque el camino tenga claves de cualquier longitud. La primera versión guardaba los fragmentos enteros y los recortaba solo al mostrar el mensaje: con m ciclos bajo una clave de longitud L eso era Θ(m·L) bytes con una entrada de Θ(m + L) (hallazgo 1 de la ronda 1 de la auditoría: 1,27 GB y 12,2 s con L = 8·10⁴, m = 8·10³).
5. Los errores se ordenan por posición.

Mensaje (una línea):

```
línea 3, columna 20: ciclo de referencias que no consume entrada (validar el valor exigiría volver a validarlo
con el mismo esquema sin leer ningún token): #/$defs/a —$ref→ #/$defs/b —$ref→ #/$defs/a
```

Cada nodo se muestra con su fragmento (`#…`, [referencias.md](referencias.md) §4) y cada arista con su palabra clave. Si el ciclo tiene más de 8 aristas se muestran las 7 primeras, `…`, la última y el total (`(30001 aristas)`); cada fragmento tiene como mucho 80 bytes. `code()` es `cycle`.

**Costo garantizado.** Sea n el tamaño de la entrada, V y E los nodos y aristas de G₀, h la profundidad máxima de un subesquema (≤ 256 en lo que produce el parser, D7) y m el número de componentes con ciclo:

- tiempo: O(V + E) para el grafo, Tarjan y las búsquedas, más O(m · (h + 9·80)) para los fragmentos de los errores y O(m log m) para ordenarlos;
- memoria de la salida: O(m) errores de tamaño acotado (9 fragmentos de ≤ 80 bytes y 9 etiquetas cortas). Como cada componente con ciclo tiene un `$ref` propio, distinto del de las demás, y un `$ref` ocupa al menos 9 bytes de la entrada (`"$ref":""`), m ≤ n/9: **la salida es O(n)**, y no depende de la longitud de las claves.

**Número de errores: sin tope.** No se acota el número de ciclos informados. Por lo anterior la salida ya es lineal en la entrada, y la regla es la misma en todas las etapas de la Fase 3 (palabras clave rechazadas, referencias sin resolver, anclas repetidas): se informan todos, ordenados por posición. Limitar cuánto de esa lista se muestra es tarea de quien la presenta: el servidor (Fase 7) ya tiene pendiente un límite de tamaño de subida (`docs/fase2/pendientes.md` §6) y puede truncar la lista.

**Otros errores de la Fase 3.** Ninguno construye fragmentos ni punteros de nodos: `UnresolvedRef` guarda el `$ref` recortado a 32 caracteres y, en `NotFound`/`IntoLiteral`/`NotASchema`, un segmento o un nombre de palabra clave que sale del texto de **ese** `$ref` (no puede ser más largo que él); `DuplicateAnchor` guarda el nombre del ancla repetida, que está en la entrada en esa posición; los del meta-esquema guardan nombres o valores de su propia posición, recortados. En todos, lo guardado por error es como mucho proporcional a los bytes de la entrada que lo provocan, así que el total es O(n).

**Medición** (`schemac check` en release; memoria pico, `ru_maxrss` del proceso hijo; `/usr/bin/time` no está instalado en la máquina de desarrollo), con el escenario del hallazgo, m autoaristas `{"$anchor": "xi", "$ref": "#xi"}` bajo una clave de longitud L:

| L / m | Entrada | Antes: tiempo / memoria | Ahora: tiempo / memoria |
|---|---|---|---|
| 10⁴ / 10³ | 52 KB | 0,07 s / 23 MB | 0,02 s / 11,8 MB |
| 2·10⁴ / 2·10³ | 107 KB | 0,23 s / 82 MB | 0,04 s / 11,9 MB |
| 4·10⁴ / 4·10³ | 217 KB | 0,90 s / 314 MB | 0,09 s / 11,9 MB |
| 8·10⁴ / 8·10³ | 437 KB | 12,2 s / 1,27 GB (auditor) | 0,19 s / 17,7 MB |
| 1,6·10⁵ / 1,6·10⁴ | 895 KB | — | 0,39 s / 33,3 MB |

Antes, la memoria crecía ×4 al duplicar la entrada; ahora el tiempo y la memoria crecen ×2 (la memoria de las primeras filas es la base del proceso). Los mensajes que escribe `schemac` ocupan unos 530 bytes por error (≈ 10 veces la entrada en este escenario, que es el peor: 55 bytes por ciclo), con la ruta del archivo delante de cada línea. La prueba `errores_de_ciclo_de_tamano_lineal` lo fija sin medir tiempo (D1): con L = 2·10⁴ y m = 2·10³ comprueba que cada fragmento guardado tiene ≤ 80 bytes y que lo guardado en total y los mensajes no pasan de 4 y 8 veces el tamaño de la entrada (con fragmentos enteros serían ≥ 8·10⁷ bytes).

## 6. Pruebas

- `crates/semantic/tests/cycles.rs`:
  - recursión válida: el caso `valid-recursive-tree-01`; por `properties`, `items`, `prefixItems` y `additionalProperties`; A → B → A con `properties` en medio; un aplicador seguido de una hija; una definición que apunta a la raíz sin que la raíz la use; `$ref` con hermanos que consumen;
  - los esquemas circulares de `tests/symbols.rs` (que la tabla resuelve) se rechazan aquí;
  - mensaje y posición exactos;
  - dos componentes con ciclo encadenadas ({a, b} → {c, d}) con el camino completo de cada una: si la búsqueda del camino saliera de su componente, la segunda quedaría incompleta (mata la mutación de quitar `comp[w] == c`);
  - tamaño de los errores con m ciclos bajo una clave larga (`errores_de_ciclo_de_tamano_lineal`, §5);
  - D7: cadenas de 3·10⁴ `$ref` con ciclo y sin él en un hilo de 256 KiB (un DFS recursivo necesitaría varios MiB), y un ciclo de 256 aristas a la profundidad máxima del parser;
  - proptest: con definiciones generadas al azar que se referencian envueltas en `allOf`, `not` (no consumen), `properties` o `items` (consumen), el número de errores coincide con el número de componentes con ciclo calculado por fuerza bruta (clausura transitiva).
- `crates/semantic/tests/symbols.rs`: `fragment_clipped` es el fragmento entero si cabe y, si no, un principio y un final suyos con `…` sin pasar del tope (propiedad), y no depende de la longitud de una clave de 10⁵ caracteres.
- `crates/semantic/src/cycles.rs`: Tarjan contra la clausura transitiva en grafos aleatorios (mismo ciclo y misma componente), validez de los caminos de la BFS y clasificación completa.
- `tests/schemas/semantic-error/ciclo*.json`: directo, indirecto por `anyOf`/`oneOf`, por `allOf`, por `not`, por `dependentSchemas`, autoaristas (`#`, `""`, ancla), dos componentes y `$ref` con hermanos; con el camino esperado en `errores-semanticos.json`.
- `crates/cli/tests/check.rs`: `schemac check` informa un ciclo.

**Costo medido** (`schemac check`, cadenas de *n* definiciones): 1,0 s, 1,8 s y 3,8 s en debug para *n* = 2,5·10⁴, 5·10⁴ y 10⁵, con ciclo o sin él; en release, para 10⁵ (≈ 3,5 MB): parser 368 ms, meta-esquema 28 ms, tabla de símbolos 234 ms y ciclos 14 ms. Las proporciones al cuadruplicar *n* (×3,7 a ×4,9) confirman que el costo es lineal.
