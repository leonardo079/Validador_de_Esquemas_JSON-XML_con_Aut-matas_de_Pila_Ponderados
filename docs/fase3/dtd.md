# Fase 3 — Análisis semántico y bajada a la IR de DTD (DTD.3, D86)

La Fase 3 de la DTD recibe el AST del parser ([../fase2/gramatica-dtd.md](../fase2/gramatica-dtd.md)) y produce la
**misma IR** que JSON Schema y XSD ([ir.md](ir.md)), verificada (D15). Código en `crates/dtd/src/check.rs`
(análisis) y `lower.rs` (bajada). `dtd::compile` encadena toda la tubería: texto → tokens → AST → análisis → IR.

No hay campos nuevos en la IR, en el JSON v2 de `schemac ir` (D17) ni en la API: una DTD se ve, desde la Fase 4,
igual que un XSD sin espacio de nombres.

## 1. Tabla de símbolos y reglas

El análisis recorre las declaraciones una vez y **reúne todos los errores**, ordenados por posición, en lugar de
parar en el primero (como el frontal XSD).

| Regla | Error (código) | Dónde |
| --- | --- | --- |
| La DTD declara al menos un `<!ELEMENT>`; sin elementos no hay documento válido | `NoElements` (`empty-schema`) | inicio |
| Un elemento se declara una sola vez (XML: «Unique Element Type Declaration») | `Duplicate` (`duplicate-name`) | el nombre repetido; el mensaje cita la primera |
| `<!ATTLIST>` de un elemento sin `<!ELEMENT>` | `Unresolved` | el nombre del elemento |
| Un nombre del modelo de contenido sin `<!ELEMENT>` | `Unresolved` | el nombre |
| Varias `<!ATTLIST>` del mismo elemento se **unen**; un atributo repetido es error (en XML gana el primero, §3bis.4) | `Duplicate` | el atributo repetido |
| Una enumeración sin valores repetidos («No Duplicate Tokens») | `Duplicate` | el valor repetido |
| Un valor por defecto o `#FIXED` está en su enumeración, sin normalizar espacios (§3bis.4) | `InvalidValue` (`value`) | el literal |
| A lo sumo 4 096 partículas por modelo, la raíz incluida (D26) | `Limit` | la partícula 4 097 |
| El modelo de contenido es **determinista** (§2) | `NonDeterministic` (`content-model`) | la segunda partícula en conflicto |

Los nombres de las primeras dos columnas son los de `dtd::ErrorKind` y su `code()`. Son los mismos códigos que da
el frontal XSD, así que la CLI y el servidor los tratan igual.

**Aplanado sin recursión (D7).** Cada modelo se convierte en una **arena en preorden**, con una pila explícita: los
hijos de un grupo son índices posteriores y cada hijo empieza donde acaba el subárbol del anterior. Es justo la forma
que exige la IR (invariante I11), así que la bajada la reutiliza tal cual. El tope de 4 096 se comprueba al aplanar.

## 2. Determinismo: el autómata de Glushkov

XML 1.0 (apéndice E) exige que los modelos de contenido sean **deterministas**: al leer un hijo, tiene que estar
claro a qué partícula del modelo corresponde sin mirar más adelante. Es la misma condición que la *Unique Particle
Attribution* de XSD, y la que hace que cada modelo sea un **AFD** sin determinizar, que es lo que necesita la
Fase 4.

**La construcción.** Cada nombre del modelo es una **posición**. Para cada nodo n de la arena se calculan:

- `nullable(n)`: si n acepta la cadena vacía;
- `first(n)`: posiciones que pueden empezar una palabra de n;
- `last(n)`: posiciones que pueden terminarla;
- `follow(p)`: para cada posición p, las que pueden ir justo después.

| Nodo | `nullable` | `first` | `last` | `follow` |
| --- | --- | --- | --- | --- |
| nombre `x` en la posición i | no | {i} | {i} | — |
| `(c1 \| … \| ck)` | alguno | ∪ first(cj) | ∪ last(cj) | — |
| `(c1, …, ck)` | todos | first(c1) ∪ first(c2) si c1 es anulable ∪ … | igual, desde el final | last del prefijo (ampliado mientras los hijos son anulables) ⊆ follow ⊇ first(cj+1) |
| sufijo `?` | sí | igual | igual | — |
| sufijo `*` | sí | igual | igual | para p ∈ last(n): follow(p) ⊇ first(n) |
| sufijo `+` | igual que sin sufijo | igual | igual | igual que `*` |

**La condición.** El modelo es determinista si y solo si ni `first(raíz)` ni ningún `follow(p)` contienen **dos
posiciones con el mismo nombre**. Si dos compiten, el error nombra las dos con su línea y columna.

Ejemplos:

- `((a, b) | (a, c))` no es determinista: `first` = {a₁, a₂}.
- `(a, b?, b)` tampoco: follow(a) = {b₁, b₂}.
- `((a, b)*, a?)` tampoco: follow(b) = {a₁, a₂}.
- `((a, b) | (c, d))` y `(a, (b | c)*, d?)` sí lo son.

**Sin recursión y con coste acotado.** En preorden, los hijos van después del padre. Recorriendo la arena **de atrás
hacia adelante**, cada nodo se calcula con sus hijos ya calculados, sin pila. Los conjuntos son mapas de bits de n
posiciones. Con n ≤ 4 096 (D26), el cálculo es O(n²/64) palabras y la comprobación, O(n²) en el peor caso: unos
pocos megabytes y milisegundos.

**Diferencial con XSD.** Sin contadores (`?`, `*` y `+` no los necesitan), el determinismo de XML 1.0 y la UPA de
XSD son la misma regla. La prueba `el_determinismo_coincide_con_la_upa_de_xsd` genera 3 000 modelos al azar,
escribe cada uno en DTD y en XSD, y comprueba que `dtd::analyze` y `xsd::analyze` coinciden en si es determinista.
`xsd` solo es una dependencia de las pruebas: el frontal DTD no depende de él (D13).

## 3. La bajada a la IR

Con la forma de XSD ([ir.md](ir.md) §5.2):

| DTD | IR |
| --- | --- |
| la DTD | `Format::Xml { target_namespace: None }` |
| el documento | nodo 0, `Element`, con `Content` = `Choice` de una partícula `Element` por cada `<!ELEMENT>` (`Occurs::ONCE`): la raíz puede ser **cualquier** elemento declarado (D86) |
| `<!ELEMENT e …>` | un nodo `Element` por declaración, padre el documento, paso `Entry("element", e)` |
| `EMPTY` | sin `Content` ni `SimpleContent`: contenido vacío |
| `(#PCDATA)`, `(#PCDATA)*` | `SimpleContent` hacia `#/builtin/string` |
| modelo de hijos | `Content` con la arena del §1: `Sequence`, `Choice` y `Element { name, ty }`, donde `ty` es el nodo de la declaración de ese nombre (una arista, no una copia: la recursión `seccion → seccion` es un ciclo) |
| `?` `*` `+` | `Occurs` {0,1}, {0,∞}, {1,∞}; sin sufijo, {1,1} |
| `<!ATTLIST>` | `Attributes`, con los de todas las listas del elemento en orden |
| `CDATA` | `ty` = `#/builtin/string` |
| `(v1\|v2\|…)` | `ty` = un nodo `Text` propio con `Lexical(String)` y `Enum` de cadenas; padre el elemento, paso `Entry("attribute", nombre)` |
| `#REQUIRED` / `#IMPLIED` / `"v"` / `#FIXED "v"` | `required` / nada / `default` / `fixed` |

**Orden de los `NodeId`**, determinista:

1. el documento (0);
2. los elementos en orden de declaración (1 + su posición);
3. los sintéticos, en el orden de su primer uso al bajar los anteriores: `#/builtin/string`, creado una sola vez, y
   un `Text` por atributo enumerado.

Las IR doradas de `tests/ir/dtd` lo fijan: `pedido` tiene 9 nodos (documento, 6 elementos, `builtin/string` y la
enumeración de `estado`).

**Siempre verificada (D15).** `lower` termina con `Ir::verify`. Si la bajada tuviera un fallo, el usuario recibe el
error interno fijo, nunca una IR inválida.

## 4. Verificación (`cargo test -p dtd`)

- `tests/semantica.rs` (9 pruebas):
  - cada regla del §1 con su clase y su byte;
  - todos los errores juntos y en orden;
  - 15 modelos deterministas y 9 que no lo son, con el byte de la segunda partícula;
  - el tope de 4 096 partículas;
  - la diferencial con la UPA de XSD (3 000 modelos).
- `tests/bajada.rs` (4 pruebas):
  - la forma de la IR del pedido (nodos, pasos, reglas, atributos, arena en preorden);
  - las **IR doradas** de `tests/schemas/dtd/parse-ok/{pedido,biblioteca}.dtd` en `tests/ir/dtd/`: deterministas y
    con `format: "xml"`. Se regeneran con `DTD_DORADAS=escribir cargo test -p dtd --test bajada`, solo tras un
    cambio de la bajada revisado a mano;
  - **de extremo a extremo**: la IR construye el **autómata de pila** de la Fase 4 (`automaton::build`), que valida
    documentos XML como manda la DTD, con bloques de 1, 7 y 4 096 bytes. Acepta los válidos y la recursión de
    `seccion`, y rechaza:
    - un atributo requerido que falta;
    - un valor fuera de su enumeración y un `#FIXED` distinto;
    - el orden equivocado y las dos opciones de una elección;
    - texto en un elemento `EMPTY`;
    - atributos y elementos no declarados.

## 5. Lo que falta (DTD.4-DTD.6)

- **DTD.4:** que `schemac` y el servidor detecten la DTD por su contenido con `dtd::formato` (enmienda E1/E2) y
  llamen a `dtd::compile`, y la web.
- **DTD.5:** casos en `tests/cases`, mitigaciones, fuzzing, Atlas y la diferencial DTD frente a XSD **por
  veredictos** sobre payloads generados (E3).
