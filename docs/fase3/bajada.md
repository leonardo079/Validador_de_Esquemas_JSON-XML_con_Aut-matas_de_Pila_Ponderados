# Fase 3 — Bajada de JSON Schema a la IR y `schemac ir`

Quinta tarea del análisis semántico (tarea 4.2 de la semana 2). Entrada: el AST de un esquema y su tabla de símbolos, ya verificados por la Fase 3 (meta-esquema, [referencias.md](referencias.md), [ciclos.md](ciclos.md)). Salida: la IR común de [ir.md](ir.md), **siempre verificada** (`VerifiedIr`, D15), o un único error interno. `schemac ir` la escribe en JSON estable o en DOT.

Implementación: [`crates/semantic/src/lower.rs`](../../crates/semantic/src/lower.rs) (`semantic::lower`), [`crates/ir/src/verify.rs`](../../crates/ir/src/verify.rs) (`VerifiedIr`, `InternalError`), [`crates/ir/src/export.rs`](../../crates/ir/src/export.rs) (`write_json`, `write_dot`) y [`crates/cli/src/main.rs`](../../crates/cli/src/main.rs) (`schemac ir`).

```
bytes ─parse─▶ AST ─analyze─▶ SymbolTable ─lower─▶ Ir ─verify─▶ VerifiedIr ─▶ write_json / write_dot
                    (Fase 3: meta, $ref, ciclos)        └─ falla ─▶ InternalError (código 3)
```

## 1. Correspondencia AST → IR

Sea T = (N, id, padre, A, D, ρ) la tabla de símbolos de [referencias.md](referencias.md) §2. La bajada define la IR I = (JSON, N′, σ, R, o) así:

- **Nodos.** N′ = {0, …, |N| − 1} y el nodo *i* es el subesquema de id *i*: **`NodeId` = `SchemaId`** (D13). Todos son de tipo `Json` (σ ≡ json, I6). También son nodos las entradas de `$defs`/`definitions` que nadie referencia (D13, coherente con D12): la IR no depende de qué parte del esquema es alcanzable.
- **Origen.** o(*i*) = (rango del subesquema, padre(*i*), paso) con el padre y el paso de la tabla, copiando los nombres (`Step::Keyword`, `Entry`, `Index`). Por eso `Ir::location(i)` es `#` seguido del puntero p(*i*) de la tabla, y la raíz tiene padre `None` y paso `Root` (I1).
- **Reglas.** R(*i*) sale de las palabras clave del subesquema, **en el orden en que aparecen**, según la tabla de [ir.md](ir.md) §5.1. Resumen:

| Subesquema o palabra clave | Regla |
|---|---|
| `true` | ninguna |
| `false` | `Reject`, con el rango del esquema |
| `$ref` | `Ref(ρ(i))`: **una arista** al nodo que resolvió la tabla, nunca una copia |
| `type` (cadena o arreglo) | `Type(TypeSet)` |
| `enum`, `const` | `Enum(literales)`, `Const(literal)` (§2) |
| `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` | la regla con `Num { lexeme }`: el **lexema exacto** del esquema, sin pasar por `f64` |
| `minLength`, `maxLength`, `minItems`, `maxItems`, `minProperties`, `maxProperties` | la regla con un `u64` exacto **saturado** (§3) |
| `pattern` | `Pattern(fuente)` |
| `prefixItems` y/o `items` | **una** regla `Items { prefix, rest }` |
| `properties` y/o `additionalProperties` | **una** regla `Properties { declared, additional }` (semántica de 2020-12, [ir.md](ir.md) §6.2) |
| `uniqueItems: true` / `false` | `UniqueItems` / ninguna |
| `required`, `dependentRequired` | `Required(nombres)`, `DependentRequired(propiedad → nombres)` |
| `dependentSchemas` | `DependentSchemas(propiedad → nodo)` |
| `allOf`, `anyOf`, `oneOf`, `not` | `AllOf(nodos)`, `AnyOf`, `OneOf`, `Not(nodo)` |
| `$defs`, `definitions` | ninguna (sus entradas ya son nodos) |
| `$schema`, `$id`, `$anchor`, `$comment`, `title`, `description`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly`, `format` | ninguna: anotaciones, o ya las consumieron el meta-esquema y la resolución de `$ref` |

**Rangos.** Cada regla lleva el rango de su palabra clave, desde el inicio de su nombre hasta el final de su valor (`"minimum": 0`), para que el validador cite la regla que falla. Las dos reglas dobles (`Items`, `Properties`) se colocan en la posición de la **primera** palabra clave de la pareja que aparece en el texto y llevan su rango: un rango que cubriera las dos abarcaría las palabras clave que haya entre ellas.

**Entrada exigida.** `lower` exige una tabla que **ya pasó `analyze`** (meta-esquema, `$ref` resueltos, sin ciclos no productivos). El tipo no lo impone (acepta cualquier `&SymbolTable`, también la de `SymbolTable::build` sola): con otra tabla, un error del esquema del usuario, por ejemplo el ciclo `{"$ref": "#"}`, rompería I14 y saldría como error interno (código 3) en lugar de como error del esquema con su posición. Un tipo `AnalyzedTable` que lo garantice, como `VerifiedIr`, queda como mejora ([pendientes.md](pendientes.md) §13).

**Destinos de las aristas.** Las palabras clave con subesquemas (`items`, `properties`, `allOf`…) apuntan a sus hijos en la tabla. Los hijos de un nodo son los nodos cuyo padre es él y, como los ids son un preorden en orden de documento, aparecen **en el orden de sus palabras clave y entradas**. La bajada los agrupa por padre una sola vez (un conteo, formato CSR) y, al bajar un nodo, cada palabra clave toma los siguientes de su grupo: tantos como subesquemas tenga su valor (uno, o los de su mapa o arreglo). Se toman también los de `$defs`, que no producen regla, para no desalinear los siguientes. Al tomar cada hijo se comprueba que su paso en la tabla es el que corresponde a la palabra clave: `Keyword(k)` para un subesquema, `Entry(k, e)` para la entrada *e* de un mapa e `Index(k, i)` para el elemento *i* de un arreglo. Si no lo es (los grupos estarían desalineados), la arista es colgante (abajo) en lugar de apuntar en silencio a otro subesquema, que I2 e I5 no detectarían.

**Defensa en profundidad.** Con una tabla de `analyze` todo `$ref` tiene destino y cada palabra clave encuentra sus hijos. Si no fuera así (un error de la bajada o de la tabla, o un hijo con otro paso), la bajada no hace pánico: pone una arista al `NodeId` imposible `usize::MAX`, que `verify` rechaza (I2), y el usuario ve el error interno (§4).

## 2. Literales

`enum` y `const` bajan a `Literal`: el valor JSON como lista de tokens en **preorden**, con las claves de cada objeto **ordenadas por punto de código** (I9; el orden de los bytes de UTF-8 coincide con el de los puntos de código) y los números con su lexema. El parser ya rechazó las claves repetidas (D2). Se recorre con una pila explícita de elementos «valor» y «clave»: un objeto apila sus miembros ordenados del revés, cada uno como valor y encima su clave, así que salen clave, valor, clave, valor… en orden.

## 3. Contadores

`"maxLength": 1e400` es válido para el meta-esquema. `number::to_u64_saturating` convierte el lexema de un entero no negativo (`5`, `5.0`, `1.5e1`, `1e400`) a `u64` **exacto**, saturando en `u64::MAX` ([ir.md](ir.md) §5.1): con el método de [`number.rs`](../../crates/semantic/src/number.rs), el valor es D × 10^*s* tras quitar los ceros a la derecha de D; si D tiene más de 20 cifras o 10^*s* no cabe, el resultado satura. Nunca pasa por `f64`. La saturación es exacta para lo que significa cada contador: un máximo de `u64::MAX` equivale a «sin límite» y un mínimo, a «insatisfacible», porque ninguna instancia tiene 2⁶⁴ − 1 caracteres, elementos o miembros.

## 4. Verificación siempre y error interno (D15)

`semantic::lower` construye la IR y llama **siempre** a `Ir::verify`, que la consume: si cumple I1–I14 devuelve un `VerifiedIr`; si no, devuelve la IR intacta con **todos** los invariantes rotos, y `lower` los envuelve en un `ir::InternalError`.

- **`VerifiedIr`** solo se obtiene de `verify` (su campo es privado) y solo da acceso de lectura (`ir()`, `nodes()`, `node(id)`, `location(id)`, `edge_count()`); para cambiarla hay que sacarla con `into_ir()` y verificarla de nuevo. Lo comprueba una prueba `compile_fail` de su documentación. La exportación a JSON y DOT y, a partir de la Fase 4, el constructor del DPDA solo aceptan `&VerifiedIr`.
- **`InternalError`** se muestra como **un único error** de texto fijo: `error interno del compilador: la IR no cumple sus invariantes`. Un fallo de `verify` es un error de la bajada, no del esquema del usuario ([ir.md](ir.md) §4, «Contrato»), así que no se listan sus invariantes como si fueran errores del esquema. La lista completa queda en `InternalError::errors()` para las pruebas; `InternalError::detail(n)` da la versión para el registro: el recuento y los *n* primeros, cada uno de una línea y con el texto del usuario recortado con `short` (SA-L1), así que mide O(*n*) sea cual sea el número de errores.
- **En `schemac ir`** el error interno se escribe en stderr como `ruta: error interno del compilador: la IR no cumple sus invariantes`, seguido del registro (`schemac: registro: …`, el recuento y los 8 primeros), y el **código de salida es 3**. Un pánico dentro del hilo de análisis de `schemac ir` también es un fallo interno y da 3. (`schemac check` conserva su código 2 para un pánico, como antes de esta tarea.)
- **El «registro» de D15 es stderr.** D15 dice que la lista va «solo al registro»; `schemac` no tiene todavía un registro aparte, así que el registro acotado sale por stderr, después del error único y con el prefijo `schemac: registro:` para distinguirlo. Cuando exista un canal de registro (el servidor de la Fase 7), esas líneas van allí y el usuario solo ve el error único ([pendientes.md](pendientes.md) §14).

## 5. `schemac ir`

```
schemac ir [--dot] <esquema.json>
```

Corre en el **mismo hilo con pila explícita** de 8 MiB que `schemac check` (D7): parser, Fase 3, bajada, clasificación ([clasificacion.md](clasificacion.md)) y exportación. Desde la tarea 4.3 la salida marca cada nodo y cada regla con su clase (a)/(b)/(c) y su estrategia (§5.1, §5.2).

**Entrada.** El esquema tiene que ser un **archivo regular** (no un directorio, una FIFO ni `/dev/zero`) de como mucho **16 MiB** (`docs/00-alcance.md` §4). Se lee como mucho un byte más que el tope (`Read::take`), así que una entrada enorme o infinita nunca se carga entera en memoria. El tipo se comprueba **antes de abrir** el archivo (`fs::metadata`) y otra vez sobre el descriptor abierto: abrir una FIFO sin escritor bloquearía esperándolo, y así se rechaza con código 2 sin abrirla (4.2-L1, [pendientes.md](pendientes.md) §19, resuelto en la 4.3). Entre las dos comprobaciones queda una carrera (cambiar la ruta por una FIFO justo después de la primera): las rutas de la CLI son **de confianza**, las elige quien ejecuta `schemac`; el servidor de la Fase 7 recibe bytes, no rutas. Vale igual para `check`.

**Salida.** Se escribe con `write_all` y `flush` sobre stdout, nunca con `print!` (que hace pánico si falla la escritura); los diagnósticos van a stderr con `writeln!` ignorando su error, nunca con `eprintln!` (4.2-I1, [pendientes.md](pendientes.md) §21). Si el lector cierra la tubería antes de tiempo (`schemac ir e.json | head`), `schemac` termina **en silencio con código 0**, la convención de Unix; cualquier otro error al escribir (disco lleno…) da un mensaje y **código 2**. Vale igual para `check`, `dot` y `version`.

Códigos de salida de `schemac ir` (y, entre paréntesis, las diferencias de `check`):

| Código | Cuándo | Salida |
|---|---|---|
| 0 | la IR verificada; también si el lector cerró la tubería | stdout: la IR en JSON (o DOT con `--dot`) |
| 1 | el esquema tiene errores | stderr: **los mismos mensajes que `schemac check`** (Fase 2, o la primera etapa de la Fase 3 que falla) |
| 2 | no se pudo leer el archivo, no es un archivo regular o supera los 16 MiB; argumentos inválidos; no se pudo escribir la salida; no se pudo crear el hilo de análisis (en `check`, también un pánico en su hilo) | stderr: el motivo, o el uso |
| 3 | error interno del compilador (§4): la IR no pasa `verify` o un pánico en el hilo de `ir` | stderr: el error único y el registro acotado |

### 5.1 Formato JSON

Determinista (solo depende del texto del esquema: nodos por `NodeId`, reglas y destinos en su orden, nunca el orden de una tabla hash), con una regla por línea para que los cambios se lean bien en un `diff`. Versión **2** (`ir::IR_JSON_VERSION`; cambia si cambia la forma). La versión 1 (tarea 4.2) era la misma sin `classCount`, `class`, `strategies` ni `strategy`; la 2 (tarea 4.3) solo **añade** esos campos ([clasificacion.md](clasificacion.md) §5):

```json
{
  "irVersion": 2,
  "format": "json",
  "targetNamespace": null,
  "nodeCount": 6,
  "edgeCount": 6,
  "classCount": {"a": 3, "b": 3, "c": 0},
  "nodes": [
    {
      "id": 3,
      "sort": "json",
      "span": {"line": 9, "column": 21, "start": 221, "end": 277},
      "parent": 1,
      "step": {"keyword": "properties", "entry": "children"},
      "class": "a",
      "strategies": ["transition"],
      "rules": [
        {"rule": "type", "span": {"line": 9, "column": 23, "start": 223, "end": 238}, "class": "a", "strategy": "transition", "types": ["array"]},
        {"rule": "items", "span": {"line": 9, "column": 40, "start": 240, "end": 275}, "class": "a", "strategy": "transition", "prefix": [], "rest": 4}
      ]
    }
  ]
}
```

- **Cabecera:** `format` (`json` o `xml`), `targetNamespace` (`null` en JSON), `nodeCount` y `edgeCount` (|N| y el número de destinos de todas las reglas, `Ir::edge_count`) y `classCount` (número de nodos de cada clase, `{"a", "b", "c"}`).
- **Nodo:** `id` (su posición), `sort` (`json`, `element` o `text`), `span` (línea y columna del inicio, en base 1, y los offsets en bytes `[start, end)`), `parent` (`null` en la raíz) y `step`: `null` en la raíz, `{"keyword"}`, `{"keyword", "entry"}` o `{"keyword", "index"}`. **No se escribe la ruta** de cada nodo: la suma de las longitudes de todas las rutas es cuadrática en el peor caso ([ir.md](ir.md) §3); se reconstruye subiendo por `parent`. Después, `class` (`"a"`, `"b"` o `"c"`: la máxima de sus reglas y de los nodos sobre la misma instancia) y `strategies` (las claves `transition`, `register`, `product`, `bounded-determinization`, `dfa`, `deferred`, en ese orden; vacío en un nodo sin reglas), definidas en [clasificacion.md](clasificacion.md) §2 y §4.
- **Regla:** `rule` (la clave estable de `RuleKind::key`: la palabra clave de JSON Schema, `false` para `Reject`; `attributes`, `content`, `simpleContent` y `lexical` en XSD), `span`, `class` y `strategy` (las de la regla en la tabla de [clasificacion.md](clasificacion.md) §3; un `content` es `b`/`register` si tiene `xs:all` o una partícula con contador), y sus datos:

| `rule` | Campos |
|---|---|
| `false`, `uniqueItems` | — |
| `$ref`, `not`, `simpleContent` | `target` |
| `type` | `types`: nombres en el orden `null`, `boolean`, `object`, `array`, `number`, `integer`, `string` |
| `enum` / `const` | `values` (un arreglo de literales) / `value` (un literal). Cada literal es **su lista de tokens en preorden**, la forma de la IR (I9), ver abajo |
| `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` | `value`: el lexema **como cadena**, para que un lector JSON no lo convierta a `f64` |
| `minLength`, `maxLength`, `minItems`, `maxItems`, `minProperties`, `maxProperties` | `value`: entero (hasta 18446744073709551615, el valor saturado; un lector en JavaScript debe leerlo como `BigInt` o como «sin límite») |
| `pattern` | `source` |
| `items` | `prefix` (ids), `rest` (id o `null`) |
| `properties` | `declared` (`[{"name", "target"}]`), `additional` (id o `null`) |
| `required` | `names` |
| `dependentRequired` | `entries` (`[{"name", "requires"}]`) |
| `dependentSchemas` | `entries` (`[{"name", "target"}]`) |
| `allOf`, `anyOf`, `oneOf` | `targets` |
| `attributes` | `attributes` (`[{"name", "required", "type", "default", "fixed", "span"}]`) |
| `content` | `particles`, en preorden: `{"term": "element", "name", "type"}` o `{"term": "sequence" \| "choice" \| "all", "children"}`, con `min`, `max` (`null` = `unbounded`) y `span` |
| `lexical` | `lexical`: `string`, `boolean`, `decimal`, `integer`, `float` o `double` |

**Literales.** Un literal es un arreglo JSON plano de tokens, cada uno un arreglo `[etiqueta]` o `[etiqueta, dato]`, con estas etiquetas estables en ASCII:

| Token | En el JSON | Significado |
|---|---|---|
| `Null` | `["null"]` | `null` |
| `Bool` | `["bool", true]` | `true` o `false` |
| `Number` | `["number", "1e400"]` | el número, con su **lexema exacto como cadena** |
| `String` | `["string", "é\t"]` | la cadena decodificada |
| `Array(n)` | `["array", n]` | un arreglo de *n* elementos: le siguen sus *n* valores |
| `Object(n)` | `["object", n]` | un objeto de *n* miembros: le siguen *n* pares `key`, valor, con las claves ordenadas por punto de código |
| `Key` | `["key", "a"]` | el nombre de un miembro |

Así `{"const": {"b": [1e400], "a": null}}` sale como `"value": [["object", 2], ["key", "a"], ["null"], ["key", "b"], ["array", 1], ["number", "1e400"]]`. Escribir el literal como un valor JSON anidado perdería información en cualquier lector con `f64` (`1e400` no es representable y `serde_json` lo rechaza; `0.10000000000000000000001` se redondea a `0.1`; JavaScript da `Infinity`), igual que las cotas, que por eso van como cadena. Además, la lista plana se lee **sin recursión** aunque el literal tenga cientos de niveles. Ver la dorada [`tests/ir/literales.json`](../../tests/ir/literales.json).

Las cadenas se escapan como en RFC 8259 §7 (comillas, barra inversa y controles); lo demás va en UTF-8. La salida mide O(tamaño de la IR): cada nombre y literal se escribe una vez.

### 5.2 DOT

`schemac ir --dot` escribe un `digraph`: un nodo por nodo de la IR, con su número, su paso desde el padre, las claves de sus reglas y, en la última línea, su clase y sus estrategias por nombre (`(c) transición, determinización acotada`; [clasificacion.md](clasificacion.md) §5); la raíz con doble borde y los nodos (c) con borde grueso. La cabecera cuenta los nodos de cada clase; una arista por destino de cada regla, etiquetada con la palabra clave y el nombre o el índice (`properties/children`, `allOf/0`, `@moneda`, `<pedido>`). Las aristas de **E₌** (misma instancia: `$ref`, combinadores, `dependentSchemas`) son **discontinuas** y las que consumen entrada, continuas; la recursión productiva se ve como un ciclo con al menos una arista continua. El texto del usuario se recorta y escapa con `short` (las etiquetas son de una línea y acotadas). Las etiquetas son texto para leer, **no punteros JSON**: el paso se escribe tal cual (`properties/a/b~c`), sin los escapes `~0`/`~1` de RFC 6901 que pone `Ir::location`, y un nombre recortado no se podría resolver de todos modos. La ruta exacta de cada nodo se reconstruye del JSON (`parent`, `step`); ver [pendientes.md](pendientes.md) §12.

El ejemplo versionado es la IR de [`tests/schemas/parse-ok/arbol-recursivo.json`](../../tests/schemas/parse-ok/arbol-recursivo.json): [ir-arbol-recursivo.dot](ir-arbol-recursivo.dot) y [ir-arbol-recursivo.svg](ir-arbol-recursivo.svg). El ciclo 1 → 3 → 4 → 1 (`properties/children`, `items`, `$ref`) es la recursión del árbol. Los nodos 0, 1 y 4 son (b) por el `required` de `$defs/node`, que sube por los `$ref` (misma instancia) pero no por `properties` ni por `items`. `make docs` regenera el `.dot` y el `.svg`; `make docs-check` comprueba que el `.dot` está sincronizado con el código (la variable `IR_DOT` del `Makefile` lista los esquemas que se dibujan).

## 6. Costo y terminación

**Terminación.** La bajada no sigue aristas: recorre los nodos de la tabla una vez, en orden, con un bucle; cada palabra clave toma un número fijo de hijos de su grupo. No hay nada que pueda no terminar aunque el esquema sea recursivo: un `$ref` circular es solo un número, el id de su destino, y el ciclo queda en el grafo de la IR. Por eso tampoco hace falta marca de visitados.

**O(n).** Sea *n* el tamaño del AST (subesquemas, palabras clave, nombres, literales):

- agrupar los hijos por padre: dos pasadas sobre los nodos, O(|N|);
- cada palabra clave se visita una vez y produce una regla del tamaño de su valor (sus nombres, lexemas y uno o varios ids), o nada;
- cada `$ref` es **una** arista: un subesquema referenciado *k* veces está una sola vez y tiene *k* aristas que llegan a él. Sin esto, una cadena de definiciones con dos referencias a la siguiente crecería como 2^*m*;
- cada literal produce un token por token de su valor;
- `verify` es O(n) sobre una IR válida ([ir.md](ir.md) §8), y la exportación también (§5.1).

La única parte que no es lineal es la **ordenación de las claves** de los objetos literales (I9), O(*k* log *k*) comparaciones por objeto de *k* miembros, que está acotada por O(*n* log *n*). El grafo (nodos, reglas, aristas) es exactamente lineal, que es lo que comprueba la prueba sin medir tiempos (D1).

**Sin recursión (D7).** El bucle de nodos, los literales con pila explícita, `verify` (DFS con pila en el montón) y la exportación (los literales son una lista plana de tokens) no consumen pila del hilo en proporción a la profundidad del esquema. El AST sí es recursivo (`Box<Schema>`) y lo construye un parser recursivo, por eso todo corre en el hilo de 8 MiB de `schemac` (D7); la IR no tiene tipos recursivos y su liberación tampoco recurre.

## 7. Pruebas

`crates/semantic/tests/lower.rs`:

- `corpus_baja_a_ir_verificada`: todos los esquemas válidos de `tests/schemas/parse-ok` (salvo las dos excepciones de `tests/corpus.rs`: `combinadores.json`, con `"oneOf": []`, y `fuera-del-subconjunto.json`, con palabras clave rechazadas por D6) y los JSON de `tests/cases` (con el **texto original** de su esquema, recortado del caso por su rango; antes de la 4.3 se reserializaba con `serde_json`, que ordena las claves y pasa los números por `f64`: [pendientes.md](pendientes.md) §18) bajan a una IR que pasa `verify`, con tantos nodos como subesquemas tiene la tabla y, en cada nodo, el `Span`, el padre y el puntero (`location` = `#` + `pointer`) de su subesquema.
- `casos_con_el_texto_original_del_esquema` (tarea 4.3): en `valid-recursive-tree-01` las reglas de `$defs/node` salen en el orden del caso (`type`, `properties`, `required`), no en el de las claves ordenadas, y un lexema como `0.10000000000000000000001` llega intacto. `clasificacion_de_not_y_any_of_sobre_a_y_sobre_c`: ver [clasificacion.md](clasificacion.md) §7.
- `ref_es_arista_sin_duplicar_el_destino`, `recursion_productiva_es_un_ciclo` (con un DFS sobre las aristas de la IR), `definiciones_inalcanzables_se_bajan` (D13).
- `esquemas_booleanos`, `anotaciones_se_descartan`, `correspondencia_de_cada_palabra_clave` (las 25 reglas de JSON Schema en un esquema, en orden, con los rangos de una regla simple y de una doble), `type_con_un_nombre`, `numeros_como_lexema_y_contadores_saturados`, `literales_en_preorden_con_claves_ordenadas`.
- `cadena_de_referencias_tamano_lineal`: 10⁴ definiciones, cada una con `allOf` a la siguiente dos veces (2^(10⁴ − 1) caminos); la IR tiene exactamente 1 + 3·10⁴ − 2 nodos y 1 + 4·(10⁴ − 1) aristas. Cuenta, no mide tiempos (D1).
- En `src/lower.rs`, `hijo_con_otro_paso_es_colgante`: un hijo cuyo paso no es el que espera la palabra clave (otra palabra clave, otra entrada, otro índice) da la arista colgante. No hay forma de construir una tabla desalineada sin API nueva, así que se prueba la función que toma el hijo con expectativas equivocadas sobre una tabla real.
- `bajada_sin_recursion`: 120 `not` anidados con un `const` de 120 arreglos anidados (241 niveles de `{`/`[`, 242 contando el escalar del fondo; cerca del tope de 256, que cuenta los `{`/`[`), analizados, bajados y verificados en un hilo de 256 KiB.
- `ir_doradas`: la salida de `write_json` de cada esquema con dorada en [`tests/ir`](../../tests/ir) (`arbol-recursivo`, `usuarios`, `limites-meta-esquema`, `unicode`, `literales`) es idéntica a la dorada, no cambia entre dos bajadas y la lee `serde_json`; en `literales` (`enum` con `1e400` y `0.10000000000000000000001`, `const` con objetos anidados y claves con escapes) los lexemas salen exactos. `make doradas` las regenera; solo se hace tras revisar el cambio a mano.

`crates/semantic/src/number.rs`: `enteros_a_u64_saturado` y la propiedad `u64_coincide_con_aritmetica_exacta`.

`crates/ir/tests/export.rs`: salida JSON y DOT exacta de una IR pequeña (versión 2, con clase y estrategia; `clase_y_estrategia_en_la_salida`), JSON válido con las 25 reglas de JSON Schema y con las de XSD, texto del usuario escapado y recortado, `literales_sin_perdida` (lexemas exactos de `1e400` y `0.10000000000000000000001`, objetos anidados y claves con escapes, leídos con `serde_json`), y `exportar_sin_recursion` (10⁵ niveles en un hilo de 256 KiB).

`crates/ir/tests/forma.rs`: las 63 pruebas de la tarea 4.1, adaptadas a `verify(self)` sin debilitarlas (verifican una copia y comprueban además que `verify` devuelve la misma IR, verificada o intacta con sus errores), más `verified_ir_de_solo_lectura`, `mensajes_escapan_y_recortan_texto_del_usuario` (SA-L1) y `error_interno_unico_y_acotado`.

`crates/cli/tests/ir.rs` y `crates/cli/src/main.rs`: códigos 0 (IR válida, igual a la dorada y determinista; `--dot` igual al `.dot` versionado), 1 (los mismos mensajes que `check` para errores de la Fase 2 y de cada etapa de la Fase 3), 2 (archivo que no existe, ruta no UTF-8, argumentos inválidos, también una opción desconocida antes o después de `--dot`) y 3 (`error_interno_unico_con_codigo_3`; `fallo_interno_da_codigo_3` y `panico_en_el_hilo_da_codigo_3` recorren `ejecutar_ir` hasta el código de salida con una bajada que falla o hace pánico, escribiendo en búferes y no en la salida de `cargo test`), un esquema a la profundidad máxima del parser (256) y uno que la pasa (257: código 1 y el mismo mensaje que `check`, `profundidad_257_igual_que_check`). `crates/cli/tests/es.rs`: un esquema de 16 MiB + 1 bytes da 2 y uno de 16 MiB exactos, 0 (`check` e `ir`); un directorio da 2; una FIFO sin escritor da 2 sin bloquearse (`fifo_sin_escritor_no_bloquea`, solo Unix, con una guarda de 60 s que mata el proceso); stdout en `/dev/full` da 2 con mensaje y sin pánico en `ir`, `check` y `dot` (solo Linux); y stdout a una tubería que el lector cierra enseguida da 0 en silencio (solo Unix; la salida, de varios MB, es mucho mayor que el búfer de la tubería, así que la escritura llega a fallar, y nunca se bloquea porque el lector ya no existe).

## 8. Decisiones y pendientes

1. `NodeId` = `SchemaId`, hijos por agrupación CSR en lugar de un índice nuevo en la tabla de símbolos.
2. Las reglas dobles (`Items`, `Properties`) van en la posición y con el rango de la primera palabra clave de la pareja.
3. Ante una tabla incoherente, arista a `usize::MAX` (error interno vía I2) en lugar de pánico.
4. Código de salida 3 para el error interno de `schemac ir` (y para un pánico en su hilo); `check` conserva su 2 de antes. Unificarlos es un cambio de interfaz que queda propuesto ([pendientes.md](pendientes.md) §8).
5. JSON de `schemac ir` con padre y paso en lugar de ruta, números de las cotas como cadena, contadores como entero, literales como lista de tokens en preorden con los números como lexema en cadena (ronda 2 de la revisión, CR-I-1); versión 1, que no se había publicado. La tarea 4.3 la sube a la **versión 2**, que añade la clase y la estrategia ([clasificacion.md](clasificacion.md) §8, decisión 4).
6. `short` vive en `lexer` (la única dependencia común de `schema-parser`, `semantic` e `ir`) como `lexer::Short`, `no_std` y sin reservar memoria; `schema_parser::error::short` lo envuelve.
7. Pendiente 5 de [ir.md](ir.md) §11 (aviso para «`allOf` + `additionalProperties: false`»): no se implementa; `schemac` no tiene todavía un canal de avisos. Anotado en [pendientes.md](pendientes.md) §9.
8. Tope de 16 MiB para el esquema que lee `schemac` (`docs/00-alcance.md` §4; `docs/fase2/pendientes.md` §6 no fijaba cifra) y solo archivos regulares; un error de escritura en stdout da 2, y una tubería cerrada, 0 en silencio.
9. Cada hijo tomado del grupo CSR se compara con el paso que espera su palabra clave; si no coincide, arista colgante.

Siguen abiertos y están en [pendientes.md](pendientes.md): el código de salida de un pánico en `check` (§8), los contadores mayores que 2⁵³ en JavaScript (§10), la ordenación de claves O(k log k) (§11), las etiquetas DOT sin escapes de RFC 6901 (§12), el tipo `AnalyzedTable` (§13), el registro de D15 en stderr (§14), la amplificación y la memoria del camino de éxito (§1 y §2) y el texto en bruto del JSON en la web (§15). Las decisiones 1, 2, 3, 5, 6, 8 y 9 de arriba están cerradas.
