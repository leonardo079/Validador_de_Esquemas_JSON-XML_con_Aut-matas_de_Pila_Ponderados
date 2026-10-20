# Fase 3 — Tabla de símbolos y resolución de `$ref`

Segunda tarea del análisis semántico. Entrada: el AST de un esquema que ya pasó el meta-esquema ([meta-esquema.md](meta-esquema.md)). Salida: la **tabla de símbolos** (cada subesquema con un id estable y su puntero JSON, las entradas de `$defs`/`definitions` y las anclas) y el **destino de cada `$ref`**, que forman el grafo de referencias que usarán la detección de ciclos no productivos (tarea siguiente) y la IR. O bien, la lista de todos los errores con línea y columna.

Implementación: [`crates/semantic/src/symbols.rs`](../../crates/semantic/src/symbols.rs) (`SymbolTable::build`) y [`pointer.rs`](../../crates/semantic/src/pointer.rs) (decodificación del fragmento). `semantic::analyze` encadena meta-esquema y tabla; `schemac check` la ejecuta.

## 1. Subesquemas y caminos

Sea *S* el AST. El conjunto **N(S)** de subesquemas y el camino **π(s)** de cada uno (una sucesión de segmentos, cadenas Unicode) se definen por inducción, siguiendo las clases `K_*` de la gramática ([gramatica-esquema.md](../fase2/gramatica-esquema.md) §1):

- la raíz *r* ∈ N(S), con π(*r*) = ε;
- si *s* ∈ N(S) tiene la palabra clave *k* de clase `K_SCHEMA` (`items`, `additionalProperties`, `not`) con valor *t*, entonces *t* ∈ N(S) y π(*t*) = π(*s*)·*k*;
- si *k* es de clase `K_MAP` (`properties`, `$defs`, `definitions`, `dependentSchemas`) y su valor tiene la entrada *e* : *t*, entonces π(*t*) = π(*s*)·*k*·*e*;
- si *k* es de clase `K_LIST` (`prefixItems`, `allOf`, `anyOf`, `oneOf`) y *t* es su elemento *i* (desde 0), entonces π(*t*) = π(*s*)·*k*·dec(*i*), con dec(*i*) el decimal sin ceros a la izquierda.

Nada más es un subesquema: los valores de `const`, `enum`, `default`, `examples` y de las palabras clave fuera del subconjunto son **literales** JSON aunque tengan forma de esquema, y los de las demás palabras clave (`minimum`, `required`, `$ref`…) no son esquemas.

π es inyectiva: dos subesquemas distintos tienen caminos distintos. Se apoya en dos premisas: (1) el parser rechaza las claves repetidas en un mismo objeto (D2), así que una palabra clave o una entrada de mapa nombra un único valor; y (2) la clase de una palabra clave es función de su nombre (`K_SCHEMA`, `K_MAP` y `K_LIST` son disjuntas, `gramatica-esquema.md` §1), así que el número de segmentos de cada paso (uno tras `K_SCHEMA`, dos tras `K_MAP` o `K_LIST`) queda determinado por el segmento de la palabra clave y un camino no puede partirse en pasos de dos maneras. Por eso el camino identifica al subesquema, y el **puntero JSON** de *s* es p(*s*) = concatenación de `/`·esc(*σ*) para cada segmento *σ* de π(*s*), con esc(*σ*) la codificación de la RFC 6901 (`~` → `~0`, `/` → `~1`).

## 2. Tabla de símbolos

T(S) = (N, id, padre, A, D, ρ):

- **id: N → {0, …, |N|−1}** numera los subesquemas en **preorden en orden de documento**. Como el texto de un subárbol es contiguo y cada esquema empieza antes que sus hijos, el preorden coincide con el orden del primer byte (`{` o `true`/`false`) de cada subesquema en la entrada: id(*s*) < id(*t*) ⇔ inicio(*s*) < inicio(*t*). Los ids son **estables**: dependen solo del texto, no de semillas de hash ni del orden de recorrido interno.
- **padre(*t*) = (*s*, paso)**: el paso es el último uno o dos segmentos de π(*t*). p(*t*) se reconstruye subiendo por los padres en O(profundidad + |p(*t*)|); `fragment_clipped` da una versión recortada en O(profundidad + tope), sin construirlo entero. Se guarda el padre y no el puntero completo para que la tabla ocupe O(n): guardar cada puntero costaría la suma de sus longitudes, que es cuadrática en el peor caso (una clave larga cerca de la raíz con muchos descendientes).
- **A: nombres → N**, la tabla de anclas: A(*a*) = *s* si *s* tiene `"$anchor": a`. Debe ser una función. En 2020-12 (Core §8.2.2) repetir un nombre de fragmento dentro de un recurso tiene un efecto **indefinido** y las implementaciones *pueden* (MAY) dar error; la norma no lo exige. El subconjunto elige ese error que la especificación permite (alcance §2.1: anclas «únicas en el documento»); como `$id` solo se admite en la raíz, el documento es un único recurso. Un `$anchor` repetido es el error `duplicate-anchor` en la **segunda aparición en el texto** (se ordenan por posición, no por orden de visita).
- **D ⊆ N**, las entradas de `$defs` y `definitions` (alias, `00-alcance.md` §2.1) de cualquier nivel, en orden de documento, cada una con su puntero p(*d*).
- **ρ: N ⇀ N**, el destino del `$ref` de cada subesquema que lo tiene (a lo sumo uno por esquema, D2).

## 3. Resolución de un `$ref`

Un `$ref` local *u* es `""` o `#`·*f*. La cadena vacía es una referencia al mismo documento (RFC 3986 §4.4) y equivale a `#`: se acepta (D11; ver [meta-esquema.md](meta-esquema.md) §4). La resolución tiene dos niveles:

1. **Fragmento (RFC 3986).** *f* se decodifica cambiando cada `%XX` por su byte; el resultado *f*′ debe ser UTF-8 válido. Un `%` sin dos dígitos hexadecimales o bytes que no forman UTF-8 son error. No se exige que *f* use solo los caracteres que la RFC admite sin codificar (`#/properties/ñ` se acepta): el meta-esquema oficial tampoco lo comprueba, porque `format: uri-reference` es una anotación, y D11 pide no ser más estricto.
2. **Puntero o ancla.** Si *f*′ = ε, el destino es la raíz. Si *f*′ empieza por `/`, es un puntero JSON (RFC 6901): se parte por `/` en segmentos σ₁…σₘ y en cada uno `~1` → `/` y `~0` → `~`, de izquierda a derecha (así `~01` es `~1`, y `%7E1` es `/` porque el `%` se decodifica antes, RFC 6901 §6); un `~` seguido de otra cosa es error. Si no, *f*′ debe ser un nombre de ancla (`^[A-Za-z_][-A-Za-z0-9._]*$`) y el destino es A(*f*′).

**Recorrido del puntero.** Es un autómata determinista sobre el alfabeto de segmentos, con estados *esquema s* (en un subesquema) y *valor (s, k)* (en el objeto o arreglo de esquemas de la palabra clave *k* de *s*), estado inicial *esquema r* y finales los estados *esquema*:

| Estado | Segmento σ | Siguiente estado |
|---|---|---|
| esquema *s* | *k*, palabra clave de *s* de clase `K_SCHEMA` | esquema (valor de *k*) |
| esquema *s* | *k* de clase `K_MAP` o `K_LIST` | valor (*s*, *k*) |
| esquema *s* | *k* de clase `K_VALUE`, `K_VALUES` u `K_OTHER` | error: entra en un **literal** |
| esquema *s* | *k* de cualquier otra clase | error: **no es un esquema** |
| esquema *s* | *s* no tiene la palabra clave σ | error: **no existe** |
| valor (*s*, *k*), *k* ∈ `K_MAP` | nombre de una entrada | esquema (esa entrada) |
| valor (*s*, *k*), *k* ∈ `K_LIST` | índice *i* < longitud, `0` o sin ceros a la izquierda | esquema (elemento *i*) |
| valor (*s*, *k*) | otro | error: **no existe** |

Si al acabar los segmentos el estado es *valor (s, k)* (por ejemplo `#/properties` o `#/allOf`), el destino no es un esquema y también es error. Por la inyectividad de π, el recorrido de p(*t*) termina exactamente en *t*: **resolver el fragmento de un subesquema devuelve ese subesquema** (la prueba de propiedades `fragmento_de_cada_nodo_resuelve_a_el` lo comprueba sobre esquemas generados con nombres que necesitan escapes).

El puntero recorre el **documento**, no la semántica: no atraviesa otros `$ref` (si `#/$defs/a` es `{"$ref": "#/$defs/b"}`, `#/$defs/a/properties/x` no existe aunque `b` tenga esa propiedad), igual que en 2020-12. Por eso resolver un `$ref` nunca sigue otro, y **una referencia circular se resuelve como cualquier otra**: el árbol recursivo de `valid-recursive-tree-01` (`items → #/$defs/node`, que contiene ese `items`), `A → B → A` o `{"$ref": "#"}` dan aristas de ρ sin bucle alguno. Decidir si un ciclo es productivo (consume entrada) es la tarea siguiente ([ciclos.md](ciclos.md)); `analyze` rechaza los que no lo son, pero `SymbolTable::build` los resuelve igual.

## 4. Salida para las tareas siguientes

`SymbolTable` (tomando prestado el AST, que vive durante toda la compilación):

| Método | Contenido |
|---|---|
| `ids()`, `nodes()`, `node(id)` | Subesquemas en preorden; cada `Node` tiene el `&Schema`, `parent: Option<(SchemaId, Step)>` y `ref_target: Option<SchemaId>` |
| `refs()` | Las aristas de ρ: `Ref { from, target, at }`, en orden de documento del `$ref` |
| `defs()` | Entradas de `$defs`/`definitions` |
| `anchor(nombre)` | A |
| `pointer(id)`, `fragment(id)` | p(*s*) y su forma de fragmento (`#…`, con `%XX` donde la RFC 3986 lo exige). Miden lo que el camino entero, claves incluidas |
| `fragment_clipped(id, max)` | El fragmento recortado a `max` bytes (principio, `…`, final) sin construirlo entero: O(profundidad + max). Es el que deben usar los mensajes de error ([ciclos.md](ciclos.md) §5) |

El grafo de referencias es G = (N, E_estr ∪ E_ref), con E_estr = {(padre(*t*), *t*)} y E_ref = {(*s*, ρ(*s*))}. La detección de ciclos ([ciclos.md](ciclos.md)) usa su subgrafo G₀ de aristas que no consumen entrada.

## 5. Errores

`code()` es `reference` (un `$ref` sin resolver, señalado en el inicio de su valor) o `duplicate-anchor` (en la segunda aparición del valor del `$anchor`, con la posición de la primera en el mensaje). El motivo de un `reference` (`RefProblem`) es uno de: escape `%` mal formado o no UTF-8, escape `~` inválido, fragmento que no es puntero ni ancla, ancla desconocida, segmento que no existe (con su número y su texto), puntero que entra en un literal (`const`, `enum`, `default`, `examples`, palabra clave fuera del subconjunto) o destino que no es un esquema. Se informan **todos** los errores, ordenados por posición. Ejemplo:

```
línea 3, columna 11: no se puede resolver "$ref": "#/$defs/b": el destino no existe: no se encuentra el segmento 2 del puntero, "b"
```

`analyze` no resuelve nada si el meta-esquema falla: un `$ref` remoto o una palabra clave fuera del subconjunto ya tienen su error, y repetirlos como referencias sin resolver confundiría.

## 6. Costo y pila

- **Construcción:** un preorden con pila explícita; cada subesquema y cada palabra clave se insertan una vez en tres tablas hash (palabra clave por esquema, entrada por mapa, hijo por posición), con claves que **toman prestadas** las cadenas del AST: O(n) esperado en el tamaño del AST. Las anclas se ordenan por posición: O(a log a).
- **Resolución:** O(|*u*|) por `$ref` (decodificar y un acceso hash por segmento), así que O(n) en total. Las tablas usan el hash por defecto de `std` (SipHash con semilla aleatoria), como el parser, para que un esquema hostil no pueda forzar colisiones.
- **Pila (D7):** nada es recursivo. El preorden usa una pila en el montón, la resolución es un bucle sobre los segmentos y `pointer` sube por los padres con un bucle. La prueba `ast_muy_profundo_no_desborda` construye a mano un AST de 2·10⁵ niveles (mucho más de lo que admite el parser) con un `$ref` de 2·10⁵ segmentos y lo resuelve en un hilo de 256 KiB; `profundidad_maxima` hace lo mismo a la profundidad máxima del parser en un hilo de 2 MiB.

## 7. Pruebas

- `crates/semantic/tests/symbols.rs`: ids en preorden de documento; `defs()` con sus punteros (`$defs`, `definitions`, anidados, nombres con `/`); el caso `valid-recursive-tree-01`; referencias circulares, `#` y `""`; un `$ref` a cada lugar donde hay subesquemas; escapes de la RFC 6901 y de la RFC 3986; anclas (también dentro de literales, que no cuentan); cada `RefProblem` con su posición; `analyze` se detiene en el meta-esquema; ids estables entre dos análisis; profundidad; propiedad de ida y vuelta.
- `crates/semantic/src/pointer.rs`: ejemplos de la RFC 6901 §5 y §6, errores, ida y vuelta de codificar y decodificar segmentos arbitrarios.
- `tests/schemas/semantic-error/` (archivos `ref-*` y `ancla-repetida*`) con sus entradas en `errores-semanticos.json`; `crates/semantic/tests/corpus.rs` ejecuta `analyze` sobre todo el corpus.
- `crates/cli/tests/check.rs`: `schemac check` resume la tabla e informa un `$ref` sin resolver.
