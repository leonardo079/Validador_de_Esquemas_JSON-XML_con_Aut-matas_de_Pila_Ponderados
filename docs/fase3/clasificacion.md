# Fase 3 — Clasificación (a)/(b)/(c) y estrategia

Tarea 4.3 de la semana 2, la última de la Fase 3 para JSON Schema: cada regla y cada nodo de la IR verificada ([ir.md](ir.md)) quedan marcados con su **clase** de reconocibilidad y con la **estrategia** con que la Fase 4 los implementará, y `schemac ir` lo muestra ([bajada.md](bajada.md) §5). Con esto se cumple el «Terminado cuando» de la Fase 3: cada esquema del corpus sale como IR con cada nodo marcado con su clase y estrategia.

Implementación: [`crates/ir/src/classify.rs`](../../crates/ir/src/classify.rs). Vive en `ir` y no en `semantic` porque opera sobre la IR: el frontal XSD la obtiene sin pasar por la bajada de JSON Schema (D13).

## 1. Dónde está y qué no se guarda

La clase y la estrategia **no se guardan en el nodo** (D13). Hay dos piezas:

- **Una sola tabla**, `RuleKind::classification`, un `match` exhaustivo sobre las 31 clases de regla (`RuleKind::ALL`): da la clase y la estrategia de cada `RuleKind` y, si sus datos pueden subirla, aquella a la que sube (§3). `Rule::assignment` aplica la tabla a una regla concreta.
- **`classify(&VerifiedIr) -> Classification`**: la clase y las estrategias de cada nodo, indexadas por `NodeId` (§4). Solo acepta una IR verificada (D15), porque su corrección y su terminación se apoyan en I2 e I14.

`write_classification_table` escribe la tabla en Markdown; el bloque de §3 es su salida literal y la prueba `tabla_escrita_es_la_del_codigo` lo compara con el código.

## 2. Definiciones

**Clases** (`docs/00-alcance.md`, columna «Cat.»), con el orden (a) < (b) < (c):

- **(a)** directa en el DPDA: transiciones o guardas sobre un token.
- **(b)** con registros o contadores acotados en el marco de pila (máscara de bits, contador).
- **(c)** no determinista o no libre de contexto: se resuelve con producto de autómatas, determinización acotada, AFD o verificación diferida.

**Estrategias** (`ir::Strategy`; la clave entre paréntesis es la de la exportación y, en las tres últimas de (c), la del campo `strategy` de los casos `non-dpda`, `tests/README.md`):

| Estrategia | Clave | Qué hace la Fase 4 ([ir.md](ir.md) §7) |
|---|---|---|
| transición | `transition` | estados y transiciones del subautómata del marco: trie de claves o de literales, comparador decimal, autómata de Glushkov, AFD del espacio léxico; un `$ref` es una llamada ε o su destino expandido |
| registro o contador | `register` | un registro de tamaño fijo en el marco: contador de puntos de código, elementos, miembros u ocurrencias; máscara de bits de requeridos, de `xs:all` o de atributos |
| producto acotado | `product` | producto de los subautómatas que leen la misma instancia, con tope de estados contado antes de reservar ([pendientes.md](pendientes.md) §5); en `not`, con el veredicto invertido al cerrar la instancia ([ir.md](ir.md) §6.1) |
| determinización acotada | `bounded-determinization` | determinista si las ramas se distinguen por el primer token; si no, determinización acotada a 4 096 estados (`00-alcance.md` §4) y, al pasar del tope, verificación diferida |
| AFD | `dfa` | `pattern` compilado a AFD sin *backtracking* |
| verificación diferida | `deferred` | se decide al cerrar la instancia: hash acotado de los elementos (`uniqueItems`, 4 096) o el subesquema de `dependentSchemas` aplicado si apareció su propiedad (D5) |

## 3. La tabla

Una fila por `RuleKind`, en el orden de `RuleKind::ALL`. Bloque **generado** desde `crates/ir/src/classify.rs` (`write_classification_table`); si cambia el código, se copia aquí la tabla que imprime la prueba `tabla_escrita_es_la_del_codigo` al fallar.

<!-- tabla-clasificacion:inicio (generada; no editar) -->
| `RuleKind` | Regla | Clase | Estrategia |
|---|---|---|---|
| `Reject` | `false` | (a) | transición (`transition`) |
| `Ref` | `$ref` | (a) | transición (`transition`) |
| `Type` | `type` | (a) | transición (`transition`) |
| `Enum` | `enum` | (a) | transición (`transition`) |
| `Const` | `const` | (a) | transición (`transition`) |
| `Minimum` | `minimum` | (a) | transición (`transition`) |
| `Maximum` | `maximum` | (a) | transición (`transition`) |
| `ExclusiveMinimum` | `exclusiveMinimum` | (a) | transición (`transition`) |
| `ExclusiveMaximum` | `exclusiveMaximum` | (a) | transición (`transition`) |
| `MultipleOf` | `multipleOf` | (a) | transición (`transition`) |
| `MinLength` | `minLength` | (b) | registro o contador (`register`) |
| `MaxLength` | `maxLength` | (b) | registro o contador (`register`) |
| `Pattern` | `pattern` | (c) | AFD (`dfa`) |
| `Items` | `items` | (a) | transición (`transition`) |
| `MinItems` | `minItems` | (b) | registro o contador (`register`) |
| `MaxItems` | `maxItems` | (b) | registro o contador (`register`) |
| `UniqueItems` | `uniqueItems` | (c) | verificación diferida (`deferred`) |
| `Properties` | `properties` | (a) | transición (`transition`) |
| `Required` | `required` | (b) | registro o contador (`register`) |
| `MinProperties` | `minProperties` | (b) | registro o contador (`register`) |
| `MaxProperties` | `maxProperties` | (b) | registro o contador (`register`) |
| `DependentRequired` | `dependentRequired` | (c) | registro o contador (`register`) |
| `DependentSchemas` | `dependentSchemas` | (c) | verificación diferida (`deferred`) |
| `AllOf` | `allOf` | (c) | producto acotado (`product`) |
| `AnyOf` | `anyOf` | (c) | determinización acotada (`bounded-determinization`) |
| `OneOf` | `oneOf` | (c) | determinización acotada (`bounded-determinization`) |
| `Not` | `not` | (c) | producto acotado (`product`) |
| `Attributes` | `attributes` | (b) | registro o contador (`register`) |
| `Content` | `content` | (a); (b) con `xs:all` o una partícula con contador | transición (`transition`); registro o contador (`register`) |
| `SimpleContent` | `simpleContent` | (a) | transición (`transition`) |
| `Lexical` | `lexical` | (a) | transición (`transition`) |
<!-- tabla-clasificacion:fin -->

### 3.1 Correspondencia con `00-alcance.md` §2 y §3

La prueba `tabla_coincide_con_el_alcance` (`crates/ir/tests/classify.rs`) recorre `RuleKind::ALL` con un `match` exhaustivo que asocia cada clase de regla a sus filas del alcance, **lee** `docs/00-alcance.md` y compara la clase de la tabla con la columna «Cat.» de cada fila. Una `RuleKind` nueva no compila sin sus filas, y un cambio de clase en el alcance hace fallar la prueba. Las filas:

| `RuleKind` | Filas de `00-alcance.md` |
|---|---|
| `Reject` | esquemas booleanos (§2.1) |
| `Ref` | `$ref` (§2.1); en un texto, `xs:simpleType` + `xs:restriction` (§3.1, primera clase de la celda) |
| `Type`, `Const` | §2.2 |
| `Enum` | `enum` (§2.2); `xs:simpleType` + `xs:restriction` (faceta `enumeration`) |
| `Minimum`…`ExclusiveMaximum` | §2.3; `xs:simpleType` + `xs:restriction` (facetas de rango) |
| `MultipleOf` | §2.3 |
| `MinLength`, `MaxLength` | §2.4; «facetas de longitud (b)» de `xs:simpleType` + `xs:restriction` (segunda clase de la celda) |
| `Pattern` | `pattern` (§2.4) y faceta `pattern` (§3.1) |
| `Items` | `items`, `prefixItems` (§2.5) |
| `MinItems`, `MaxItems`, `UniqueItems` | §2.5 |
| `Properties` | `properties`, `additionalProperties` (§2.6) |
| `Required`, `MinProperties`, `MaxProperties`, `DependentRequired`, `DependentSchemas` | §2.6 |
| `AllOf`, `AnyOf`, `OneOf`, `Not` | §2.7 |
| `Attributes` | `xs:attribute` (§3.1) |
| `Content` | `xs:element`, `xs:complexType`, `xs:sequence`, `xs:choice` (a); al subir, `xs:all` y `minOccurs`, `maxOccurs` (b) (§3.1) |
| `SimpleContent` | `xs:simpleContent` + `xs:extension` (§3.1) |
| `Lexical` | `xs:simpleType` + `xs:restriction` (§3.1, primera clase de la celda) |

La prueba `estrategias_de_la_tabla` fija además la estrategia de cada clase de regla según el texto del alcance y D5.

### 3.2 Decisiones de la tabla

Cierran los dos puntos que [ir.md](ir.md) §9 dejó para esta tarea y uno que el alcance deja implícito:

1. **`Content` sube a (b) solo con registros.** Un modelo de contenido es (a) (autómata de Glushkov, determinista por UPA) y sube a (b) si tiene un `xs:all` (máscara de bits) o una partícula que necesita contador: `minOccurs` ≥ 2 o `maxOccurs` acotado > 1, sea un elemento o un grupo. `?`, `*`, `+`, una vez y `maxOccurs="0"` los expresa el Glushkov con estados y no necesitan registro (D16, [ir.md](ir.md) §7, fila Γ). La fila `minOccurs`, `maxOccurs` del alcance (§3.1) lo dice así desde la tarea 4.3: (b) cuando la ocurrencia necesita contador; sin contador no hay registro, y la clase (b) la da el registro. Es la única regla cuya clase depende de sus datos (`Rule::assignment`), y por eso es la única fila con dos clases.
   `Attributes`, en cambio, es (b) **siempre**, aunque todos los atributos sean opcionales. El alcance pone `xs:attribute` en (b) sin condición («los requeridos usan máscara de bits»), mientras que para `minOccurs`/`maxOccurs` describe un mecanismo (el contador) que solo existe en algunos casos y que D16 ya acotó; con todos los atributos opcionales la máscara de requeridos queda vacía, pero bajar esa regla a (a) sería contradecir una fila del alcance sin una decisión que lo respalde, así que manda el alcance (fuente de verdad 3). Si se quisiera la misma subida condicional que en `Content`, primero habría que cambiar esa fila.
2. **`Lexical` es (a)** también en un tipo predefinido usado sin restricción: el alcance (§3.2) no da clase a los predefinidos; `Lexical` es el AFD de su espacio léxico y se le da la de la fila `xs:simpleType` + `xs:restriction`, de la que es el primitivo.
3. **`dependentRequired` es (c) con estrategia de registro.** El alcance la clasifica (c) y D5 fija su implementación: la máscara de bits del objeto al cerrarlo. Manda el alcance en la clase (fuente de verdad 3) y D5 en la estrategia; la combinación (c) + registro es intencionada y la prueba la fija. Un caso `non-dpda` que solo use `dependentRequired` no tendría ninguna de las tres estrategias de respaldo del campo `strategy`, así que no se declara.

`not` se clasifica como un `allOf` de una rama con el veredicto invertido ([ir.md](ir.md) §6.1): producto acotado. `$ref` es (a) como dice el alcance, aunque con reglas hermanas la Fase 4 lo combina con ellas como un producto ([ir.md](ir.md) §7); la asimetría con `allOf` es del alcance y no cambia la clase (ver §8, pendiente 1).

## 4. La clase de un nodo

### 4.1 Definición

Sea E₌ el conjunto de aristas sobre la **misma instancia** (`RuleKind::same_instance`: `$ref`, `allOf`, `anyOf`, `oneOf`, `not`, `dependentSchemas`; [ciclos.md](ciclos.md) §2). Para cada nodo *v*, con reglas *r*₁…*r*ₖ:

- **clase** c(*v*) = máx( {clase(*r*ᵢ)} ∪ { c(*w*) : *v* →₌ *w* } ), con máx ∅ = (a);
- **estrategias** S(*v*) = {estrategia(*r*ᵢ)} ∪ ⋃ { S(*w*) : *v* →₌ *w* }.

Las aristas que **consumen entrada** (`properties`, `additionalProperties`, `items`, `prefixItems`, el modelo de contenido, los atributos y el contenido simple) **no** suben la clase: su destino valida otra instancia, en otro marco de la pila. Un objeto con `properties: {a: {pattern: …}}` es (a); su propiedad `a` es (c).

Un nodo sin reglas (el esquema `true`) es (a) con S vacío: el DPDA solo recorre la instancia.

### 4.2 Por qué es la máxima

La instancia de *v* la lee, en un solo marco, el producto del subautómata de las reglas de *v* y de los de cada destino de E₌ ([ir.md](ir.md) §6.1 y §7). El marco de ese producto es la tupla de los marcos de sus componentes, así que necesita los registros de todos ellos (si una componente es (b), el producto también) y hereda la estrategia de cada componente (si una sigue un AFD o una verificación diferida, el producto también). Y a la inversa, una componente (a) no baja la clase de otra (c): la clase del producto es la máxima. Por la misma razón la unión de estrategias es exacta: cada una la exige alguna componente.

Las aristas de `dependentSchemas` son la excepción al producto: también son de E₌ (validan la misma instancia, y por eso cuentan para I14), pero **no entran en el producto**, porque su destino solo se aplica si aparece la propiedad, y eso se sabe al leer la clave; se evalúan de forma **diferida** ([ir.md](ir.md) §7, fila «verificación diferida»). Su destino aporta sus estrategias a esa verificación diferida, y la clase del nodo la fija ya la propia regla `DependentSchemas`, que es (c); así que la ecuación de §4.1 da el mismo resultado que si el destino estuviera en el producto.

Ejemplos (pruebas `not_y_any_of_sobre_a_y_sobre_c` y `clasificacion_de_not_y_any_of_sobre_a_y_sobre_c`):

| Esquema | Nodo del combinador | Rama |
|---|---|---|
| `{"not": {"type": "string"}}` | (c): transición, producto acotado | (a): transición |
| `{"not": {"pattern": "^a"}}` | (c): producto acotado, AFD | (c): AFD |
| `{"anyOf": [{"type": "string"}, {"minLength": 2}]}` | (c): transición, registro, determinización acotada | (a) y (b) |
| `{"anyOf": [{"type": "string"}, {"uniqueItems": true}]}` | (c): transición, determinización acotada, verificación diferida | (a) y (c) |
| `{"$defs": {"p": {"pattern": "x"}}, "$ref": "#/$defs/p"}` | raíz (c): transición, AFD | `p` (c) |

Sobre (a) el combinador ya es (c) por su propia regla (el alcance clasifica `not` y `anyOf` como (c) por el producto, no por las ramas); sobre (c), además, toma las estrategias de la rama: `not` sobre un subesquema (c) sigue la estrategia de ese subesquema e invierte su veredicto ([ir.md](ir.md) §6.1). Las ramas conservan su propia clase: el mismo nodo puede ser rama de un `anyOf` y destino de un `$ref` sin combinador.

### 4.3 Corrección del cálculo

La definición es un sistema de ecuaciones sobre el grafo (N, E₌). **E₌ no tiene ciclos en una `VerifiedIr`**: es exactamente el invariante I14 ([ir.md](ir.md) §4), que el verificador comprueba con un DFS sobre esas aristas, y en JSON lo garantiza antes la Fase 3, que rechaza los ciclos no productivos (D12, [ciclos.md](ciclos.md)). La recursión productiva sí crea ciclos en el grafo completo (el árbol de [`ir-arbol-recursivo.svg`](ir-arbol-recursivo.svg): 1 → 3 → 4 → 1), pero todo ciclo pasa por una arista que consume entrada (`properties`, `items`), que no está en E₌. Así (N, E₌) es un grafo dirigido acíclico, y el sistema tiene **solución única**, que se obtiene en orden topológico inverso: por inducción en ese orden, cuando se calcula *v* ya están calculados todos sus sucesores.

`classify` hace exactamente eso: un DFS en **postorden** sobre E₌ con una pila explícita `Vec<(nodo, siguiente arista)>` en el montón (D7), con las aristas en formato CSR. Un nodo se cierra cuando ha visto todas sus aristas, y en ese momento su resultado se combina con el de su padre en la pila; una arista a un nodo ya cerrado combina su resultado directamente. Al cerrarse *v* todos sus sucesores están cerrados (en un grafo acíclico no hay aristas a nodos abiertos), así que su valor es el de la ecuación.

**Robustez.** Si E₌ tuviera un ciclo (imposible en una `VerifiedIr`; posible solo si se construyera una IR sin verificar, que la firma no admite), la arista a un nodo todavía abierto se ignora: cada nodo entra en la pila una vez, el recorrido termina igual y el resultado es una cota inferior. No hace falta más, porque el tipo lo descarta.

### 4.4 Costo

Cada nodo entra y sale de la pila una vez y cada arista de E₌ se examina una vez; combinar dos resultados es O(1) (el máximo de dos clases y el «o» de dos máscaras de 6 bits). Construir la clase propia de cada nodo y el CSR recorre cada regla y cada destino una vez (`Rule::assignment` de un `Content` recorre sus partículas una vez). Total **O(n + m)**, con n el tamaño de la IR y m sus aristas, y memoria O(n + m).

El resultado de cada nodo se **comparte por `NodeId`**: nunca se expande E₌ en árbol (D16). La prueba `d16_diamantes_sin_expandir` clasifica una cadena de 10⁴ nodos, cada uno con `allOf` al siguiente dos veces (2^(10⁴ − 1) caminos desde la raíz), y `d7_cadena_larga_de_ref` una cadena de 10⁵ `$ref` en un hilo de 256 KiB. Las dos comprueban el resultado, no el tiempo (D1).

## 5. Cómo lo muestra `schemac ir`

La exportación llama a `classify` y escribe el resultado; la IR no cambia. Detalle del formato en [bajada.md](bajada.md) §5.1 y §5.2.

- **JSON, versión 2** (`ir::IR_JSON_VERSION`): la cabecera lleva `classCount` (`{"a", "b", "c"}`: nodos de cada clase); cada nodo, `class` (`"a"`, `"b"` o `"c"`) y `strategies` (claves de §2, en el orden de la tabla de §2); cada regla, `class` y `strategy` (su `Rule::assignment`).
- **DOT:** la cabecera cuenta los nodos de cada clase; la última línea de la etiqueta de cada nodo es su clase y sus estrategias (`(c) transición, registro o contador, determinización acotada`), y los nodos (c) llevan el borde grueso (`style=bold`).

Ejemplo, el esquema del caso `tests/cases/non-dpda/non-dpda-anyof-ambiguous-01.json` (dos objetos que solo se distinguen por la clave que traen):

```json
    {
      "id": 0,
      …
      "class": "c",
      "strategies": ["transition", "register", "bounded-determinization"],
      "rules": [
        {"rule": "anyOf", "span": {…}, "class": "c", "strategy": "bounded-determinization", "targets": [1, 2]}
      ]
    }
```

## 6. Correspondencia con [ir.md](ir.md) §7

La estrategia de cada regla es la pieza del DPDA que [ir.md](ir.md) §7 saca de ella: las reglas de transición son las de la fila «Q, estados de control de un marco» (salvo `Pattern`, que es AFD); las de registro, las de la fila «Γ, símbolos de pila» (contadores, máscaras de `Required`, `DependentRequired`, `xs:all` y atributos requeridos, contadores de ocurrencias), que se comprueban en la fila «δ que desapila»; producto acotado y determinización acotada, la fila «combinación sobre la misma instancia»; verificación diferida, la fila del mismo nombre (`DependentSchemas`, `UniqueItems`, y `anyOf`/`oneOf` que pasan del tope). La clase del nodo dice qué necesita su marco: (a) solo estados, (b) además registros de tamaño fijo, (c) además alguna de las estrategias de (c). La Fase 4 parte de esta clasificación para elegir la construcción de cada marco.

## 7. Pruebas

`crates/ir/tests/classify.rs`:

- `tabla_coincide_con_el_alcance`, `estrategias_de_la_tabla`, `tabla_escrita_es_la_del_codigo` (§3).
- `contenido_sube_a_b_solo_con_all_o_contador`: `sequence` con `?`, `*`, `+`, una vez y `maxOccurs="0"` es (a), también una sola partícula `minOccurs="1" maxOccurs="1"`; `xs:all`, `maxOccurs="5"`, `minOccurs="2"`, los bordes `minOccurs="0" maxOccurs="2"` y `minOccurs="2" maxOccurs="2"` y un grupo con `maxOccurs="3"` son (b); el documento que lo contiene sigue (a) (la arista del modelo consume entrada).
- `nodo_toma_la_maxima_de_sus_reglas`, `sube_por_misma_instancia_y_no_por_aristas_que_consumen` (`$ref`, `dependentSchemas` y (b) suben; `properties` e `items` no), `not_y_any_of_sobre_a_y_sobre_c`, `not_anidado` (`not(not(type))`: (c) con producto acotado en los dos niveles), `recursion_productiva_termina`.
- `d7_cadena_larga_de_ref`, `d16_diamantes_sin_expandir`, `d16_diamantes_por_ref_y_dependent_schemas` (los diamantes pasan por `$ref` + `allOf` y por `dependentSchemas` de dos entradas; §4.4) y `claves_y_nombres`.

`crates/ir/tests/export.rs`: `json_exacto` y `dot_exacto` (formato exacto, versión 2) y `clase_y_estrategia_en_la_salida` (un `anyOf` con una rama (c) y otra (b)).

`crates/semantic/tests/lower.rs`: `clasificacion_de_not_y_any_of_sobre_a_y_sobre_c` (§4.2, sobre esquemas reales).

`crates/semantic/tests/corpus.rs`: `corpus_clasificado` — todo el corpus que pasa la Fase 3 (los de `tests/schemas/parse-ok` y los esquemas JSON de `tests/cases`, con su texto original) sale clasificado y cada nodo cumple la ecuación de §4.1 comprobada nodo a nodo; cada caso de `tests/cases/non-dpda` tiene al menos un nodo (c), con la estrategia de su campo `strategy` si lo declara; y los casos `valid` y los esquemas de `tests/schemas/parse-ok` cuyos subesquemas no usan ninguna palabra clave (c) (combinadores, `pattern`, `uniqueItems`, `dependentRequired`, `dependentSchemas`; se buscan en el AST del parser, solo en posición de palabra clave) no tienen ningún nodo (c).

`crates/cli/tests/ir.rs`: `ir_senala_el_nodo_c_y_su_estrategia` (`schemac ir` y `--dot` sobre el esquema que la prueba lee del caso `non-dpda-anyof-ambiguous-01`).

## 8. Decisiones y pendientes

Decisiones de esta tarea (candidatas al registro del plan):

1. Tabla única `RuleKind::classification` con la clase base y la subida; solo `Content` sube, a (b), con `xs:all` o una partícula con contador (§3.2, 1). `Lexical` es (a) (§3.2, 2). `dependentRequired` es (c) con estrategia de registro (§3.2, 3). `not` es producto acotado; `anyOf`/`oneOf`, determinización acotada.
2. Seis estrategias con claves estables; las tres de respaldo coinciden con el campo `strategy` de los casos `non-dpda`.
3. La clase de un nodo es la máxima de sus reglas y de los nodos alcanzables por E₌, y sus estrategias la unión; un nodo sin reglas es (a) sin estrategias (§4).
4. El formato JSON de `schemac ir` pasa a la **versión 2**: añade campos (`classCount`, `class`, `strategies`, `strategy`) sin quitar ni cambiar ninguno, pero la constante dice que la versión cambia si cambia la forma, y la web de la Fase 7 aún no existe, así que subirla no rompe a ningún lector. El DOT añade la línea de la clase y el borde grueso de los nodos (c).

Pendientes (también en [pendientes.md](pendientes.md) §22):

1. **`$ref` con reglas hermanas (Fase 4).** Es (a) por el alcance, pero la Fase 4 lo compila como un producto con sus hermanas ([ir.md](ir.md) §7), igual que un `allOf` de dos ramas, que es (c). Una cadena de `$ref` con hermanas puede crecer como un producto: el tope de estados de [pendientes.md](pendientes.md) §5 (L-1) ya lo cubre. Si la Fase 4 mide que ese producto se comporta como el de `allOf`, se propone al usuario cambiar la fila de `$ref` en el alcance.
2. **Frontal XSD.** La tabla ya clasifica las reglas de XSD y la prueba del alcance cubre sus filas, pero ningún XSD real del corpus pasa todavía por `classify`; el frontal XSD añade a `corpus_clasificado` sus esquemas (y el caso `valid-xml-pedido-01`).
