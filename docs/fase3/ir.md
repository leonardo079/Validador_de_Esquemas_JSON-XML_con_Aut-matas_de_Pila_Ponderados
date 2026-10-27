# Fase 3 — Representación intermedia (IR) común

Cuarta tarea del análisis semántico (tarea 4.1 de la semana 2): la **forma** de la IR. Es el contrato entre los dos frontales (JSON Schema, que baja desde la tabla de símbolos de [referencias.md](referencias.md) en la tarea 4.2, y el subconjunto de XSD, que bajará desde su propio AST) y el constructor del DPDA de la Fase 4 (decisión D3 del plan). Esta tarea no baja nada todavía: la expresividad se prueba construyendo IR a mano (§10).

Implementación: [`crates/ir/src/lib.rs`](../../crates/ir/src/lib.rs) (tipos) y [`verify.rs`](../../crates/ir/src/verify.rs) (`Ir::verify`, invariantes de §4).

## 1. Dónde está

`crates/ir` es un crate aparte que **solo depende de `lexer`**, por `Span`. `semantic` (bajada de JSON Schema), el frontal XSD y `automaton` dependen de él; ninguno de los frontales depende del otro. La clasificación (a)/(b)/(c) de la tarea 4.3 vivirá en `ir` porque opera sobre la IR, así el frontal XSD la obtiene sin pasar por `semantic`.

```
schema-parser ─▶ semantic ─┐
                            ├─▶ ir ─▶ automaton (Fase 4) ─▶ codegen (Fase 5)
xsd (semana 2) ────────────┘    ▲
lexer (Span) ───────────────────┘
```

## 2. Definición formal

Una IR es una tupla **I = (F, N, σ, R, o)**:

- **F** ∈ {JSON, XML(*ns*)}: el formato de los payloads que valida. Un payload JSON nunca se valida contra una IR de XSD ni al revés (§6.3). En XML, *ns* es el único `targetNamespace` admitido (o ninguno; `00-alcance.md` §1.2): elementos y atributos se nombran por su nombre local.
- **N** = {0, …, |N| − 1}: los nodos. **0 es la raíz**: el esquema entero en JSON, el **documento** en XML.
- **σ: N → {json, elem, texto}**: a qué instancia se aplica cada nodo. Una instancia es un valor JSON, un elemento XML (sus atributos y su contenido) o un texto XML (el valor de un atributo o el contenido simple de un elemento).
- **R(*v*)**: la lista de reglas de *v*, cada una una pareja (regla tipada, `Span`). La regla tiene una **clase** κ ∈ K, el enum sin datos `RuleKind` (31 clases: casi una por palabra clave de JSON Schema que valida, con tres salvedades: `Items` junta `prefixItems` e `items`, `Properties` junta `properties` y `additionalProperties`, y `Reject` es el esquema `false`, no una palabra clave; más `Attributes`, `Content`, `SimpleContent` y `Lexical` de XSD; su clase (a)/(b)/(c) está en §9), y los datos que necesita.
- **o(*v*)** = (rango, padre, paso): la ubicación de origen en el esquema fuente.

**Aristas.** E = {(*v*, κ, *w*) : *w* es destino de una regla de clase κ de *v*}. Se parten según κ:

- E₌, aristas **sobre la misma instancia**: κ ∈ {`$ref`, `allOf`, `anyOf`, `oneOf`, `not`, `dependentSchemas`} (la clasificación de [ciclos.md](ciclos.md) §2; `RuleKind::same_instance`). En XSD, la restricción de un tipo simple por otro (`Ref` entre textos) también lo es.
- E→, aristas que **consumen**: `properties` y `additionalProperties`, `prefixItems` e `items`, las partículas del modelo de contenido (se lee `<nombre`), los atributos y el contenido simple (se lee el valor o el texto tras la etiqueta de apertura).

Un `$ref`, un `allOf` o el `type="T"` de un `xs:element` son **aristas a un nodo que ya existe**: un subesquema referenciado *k* veces está una sola vez. Por eso la IR es un grafo y no un árbol, y la recursión productiva (un árbol de objetos por `properties`, un tipo complejo que contiene un elemento de su propio tipo) es un ciclo de E que pasa por al menos una arista de E→.

**Semántica.** Una instancia *x* cumple el nodo *v*, *x* ⊨ *v*, si cumple **todas** las reglas de R(*v*) (conjunción; un nodo sin reglas acepta cualquier instancia de su tipo, como `true` o `{}`). Cada regla se define sobre *x* y, si tiene destinos, sobre la relación ⊨ de esos destinos: `Ref(w)` exige *x* ⊨ *w*; `Properties` exige *y* ⊨ *w*ₖ para cada miembro (*k*, *y*) de *x*; `Content` exige que la sucesión de nombres de los hijos de *x* pertenezca a la expresión regular de las partículas y que cada hijo cumpla el tipo de su partícula; etc. (§5). La definición es por inducción y está **bien fundada**: una arista de E→ va a una instancia estrictamente más pequeña (un miembro, un elemento, un hijo), y las de E₌ no forman ciclos (invariante I14), así que basta inducir sobre el par (tamaño de la instancia, rango de *v* en un orden topológico de E₌). Sin I14, `{"$ref": "#"}` no definiría nada: es la misma razón por la que el autómata no terminaría ([ciclos.md](ciclos.md) §4).

## 3. Tipos

| Concepto | Tipo en `crates/ir` | Notas |
|---|---|---|
| I | `Ir { format, nodes: Vec<Node> }` | arena; `nodes[0]` es la raíz |
| nodo | `NodeId(usize)` | índice en la arena; `NodeId::ROOT` = 0 |
| σ(*v*) | `Sort::{Json, Element, Text}` | |
| R(*v*) | `Node::rules: Vec<Constraint>`; `Constraint { rule: Rule, span }` | `span`: la palabra clave o la construcción, para los mensajes del validador |
| κ | `RuleKind` (`Rule::kind()`), `RuleKind::ALL` | lo usan la unicidad (I4), los tipos (I3) y la clasificación (4.3) |
| o(*v*) | `Origin { span, parent: Option<NodeId>, step: Step }` | `Step::{Root, Keyword, Entry, Index}`; `Ir::location(id)` reconstruye la ruta |
| número | `Num { lexeme }` | lexema decimal exacto de RFC 8259 (I8) |
| `type` | `TypeSet` | máscara de los 7 `JsonType` |
| literal | `Literal(Vec<LitToken>)` | valor JSON en preorden, sin árbol (I9) |
| modelo de contenido | `ContentModel { particles: Vec<Particle> }` | arena en preorden; `Particle { term, occurs, span }`, `Term::{Element, Sequence, Choice, All}` |
| `minOccurs`/`maxOccurs` | `Occurs { min: u64, max: Option<u64> }` | `None` = `unbounded` |
| atributo | `Attribute { name, required, ty, default, fixed, span }` | `ty` es un nodo de texto |
| espacio léxico | `Lexical::{String, Boolean, Decimal, Integer, Float, Double}` | el primitivo de un `xs:simpleType` |

**Origen sin rutas enteras.** Cada nodo guarda su padre en el árbol del esquema fuente y el paso desde él, no la ruta completa: guardar cada ruta costaría la suma de sus longitudes, cuadrática en el peor caso, la misma razón que en la tabla de símbolos ([referencias.md](referencias.md) §2). En JSON, el padre y el paso son los de la tabla, y `Ir::location` da el puntero JSON (`#/properties/hijos/items`, con los escapes de la RFC 6901). En XSD el paso es la construcción y su nombre (`#/complexType/Pedido/element/item`). La cadena de padres termina porque el padre es siempre un nodo anterior (I1).

**Ningún tipo es recursivo.** No hay `Box<Node>`, ni literales como árboles, ni grupos de partículas anidados: los literales son una lista de tokens en preorden y los modelos de contenido una arena con índices. Así ni el verificador ni la liberación de memoria (el `Drop` de un árbol de `Box` es recursivo) consumen pila del hilo en proporción a la profundidad del esquema (D7).

**Clase y estrategia (tarea 4.3).** No hay un campo `class` en `Node`. La clase de una regla es función de su `RuleKind` (y, en los combinadores, de las clases de sus destinos), así que se **deriva** de la IR; la tarea 4.3 la calculará en una tabla aparte indexada por `NodeId` (como `comp` en el Tarjan de [ciclos.md](ciclos.md)), junto con la estrategia de cada nodo. La IR queda inmutable después de la bajada y el verificador no tiene que comprobar campos a medio rellenar.

## 4. Invariantes

`Ir::verify` comprueba todos y devuelve **todos** los errores (`VerifyError { node, at, kind }`), en orden de nodo; dentro de un nodo, primero los del propio nodo (origen, formato, texto sin espacio léxico), después los de cada regla en el orden de las reglas (también los de I13) y al final los de ciclo (I14, que salen de un recorrido de todo el grafo); con el mismo estilo que los de la Fase 3: `code()` estable y un mensaje de una línea, `IR inválida: nodo 3 (línea 4, columna 12): …`. Cuesta O(n + m log m), con m el número de errores (§8).

**Contrato.** `Ir::verify` se llama **siempre, en el camino normal**, entre la bajada y la Fase 4 (en `schemac` y en el servidor, no solo en las pruebas): la Fase 4 solo recibe una IR que lo ha pasado, y para las propiedades de la tabla de abajo no necesita más comprobaciones. No cubre lo que «Lo que no es invariante» (al final de esta sección) deja fuera: satisfacibilidad, sintaxis del `pattern`, *Unique Particle Attribution* y que `default`/`fixed` sean valores del tipo del atributo. Un error del verificador es un **fallo del frontal** (la bajada produjo algo que no debía), no del esquema del usuario: los errores del usuario los da con su posición la Fase 3 (JSON Schema) o el frontal XSD (sus obligaciones, abajo). Por eso los errores de `verify` **no se listan al usuario**: `schemac` y el servidor los presentan como **un único error interno** de tamaño fijo («error interno del compilador: la IR no cumple sus invariantes»), y la lista completa solo va al registro, a las pruebas y a las herramientas de depuración. Por la misma razón no entran en la cota de amplificación de [pendientes.md](pendientes.md) §1.

| # | Invariante | `code()` | Quién lo necesita |
|---|---|---|---|
| I1 | Hay al menos un nodo; el 0 no tiene padre y su paso es `Root`; todo otro nodo tiene un padre **anterior** y un paso que no es `Root`. El origen forma un árbol con raíz en 0 | `origin` | `Ir::location` termina; mensajes de error |
| I2 | Todo `NodeId` de una regla está en rango | `dangling` | todos |
| I3 | Cada regla se admite en el tipo de su nodo (tabla de abajo) | `rule-sort` | la construcción de cada marco |
| I4 | A lo sumo una regla de cada clase por nodo (D2: no hay claves repetidas); un elemento no tiene `Content` y `SimpleContent` a la vez | `duplicate-rule` | un solo subautómata por palabra clave |
| I5 | El destino de cada arista es del tipo que exige la regla: en JSON, JSON; en XML, `Content` lleva a elementos y `Attributes`, `SimpleContent` y `Ref` a textos | `target-sort` | apilar el marco correcto |
| I6 | En JSON todos los nodos son JSON; en XML ninguno lo es, la raíz es un elemento (el documento) con `Content` y sin atributos ni contenido simple, y ninguna partícula tiene como tipo el documento | `format` | Σ y el marco inicial; el documento no se puede apilar dentro de sí |
| I7 | `allOf`, `anyOf`, `oneOf` y `type` no son vacíos; `Items` tiene `prefix` no vacío o `rest` (`enum: []` sí se admite, D11) | `empty` | producto y despacho por tipo |
| I8 | Todo `Num` es un lexema de RFC 8259; `multipleOf` > 0 | `number` | comparación decimal exacta sobre el lexema |
| I9 | Cada literal de `enum` y de `const` es exactamente un valor JSON en preorden, con las claves de cada objeto **ordenadas por punto de código y sin repetir** y números de RFC 8259 | `literal` | el trie de `enum` compara listas de tokens |
| I10 | Nombres únicos en `properties`, `required`, `dependentRequired` (claves y cada lista), `dependentSchemas` y atributos | `duplicate-name` | un bit por nombre en las máscaras |
| I11 | Cada modelo de contenido es un árbol en **preorden** con raíz en la partícula 0 (cada hijo de un grupo en rango, posterior a él y con un solo padre; toda partícula salvo la 0 tiene padre; el primer hijo de un grupo es la partícula siguiente y cada hijo empieza donde acaba el subárbol del anterior) y `minOccurs` ≤ `maxOccurs` | `content-model` | Glushkov sobre las partículas, sin recursión |
| I12 | `xs:all` solo como partícula raíz, con `minOccurs` ∈ {0, 1} y `maxOccurs` = 1, y con hijos que son elementos con `maxOccurs` ≤ 1 (XSD 1.0, *all-group limited*); dos partículas con el mismo nombre en un modelo tienen el mismo tipo (*Element Declarations Consistent*) | `all`, `element-consistency` | máscara de `xs:all`; el nombre del hijo decide el marco |
| I13 | Un texto tiene algún `Lexical` (si falta, `text`; un segundo es `duplicate-rule`, por I4); las facetas de longitud solo en `xs:string`, las de rango solo en tipos numéricos; cada `xs:enumeration` es un escalar del tipo del texto (y entero si el espacio léxico es `Integer`, comprobado sobre el lexema de RFC 8259 que deja el frontal, es decir, en el espacio de valores: `1.0` y `1e2` lo son, `1.5` no; la forma léxica de XSD la comprueba el frontal antes de reescribirla, ver abajo), y `xs:boolean` no tiene enumeraciones; un `Ref` entre textos conserva el espacio léxico. Un atributo no tiene `default` y `fixed` a la vez, ni `default` si es requerido | `text`, `attribute` | AFD del espacio léxico |
| I14 | El subgrafo E₌ es **acíclico** | `cycle` | terminación y cota O(d) de la pila ([ciclos.md](ciclos.md) §4); semántica bien fundada (§2) |

**Reglas admitidas por tipo de nodo** (I3, `RuleKind::allowed_in`):

| Tipo | Reglas |
|---|---|
| `Json` | todas las de JSON Schema: `false` (`Reject`), `$ref`, `type`, `enum`, `const`, las 5 numéricas, `minLength`, `maxLength`, `pattern`, `items`, `minItems`, `maxItems`, `uniqueItems`, `properties`, `required`, `minProperties`, `maxProperties`, `dependentRequired`, `dependentSchemas`, `allOf`, `anyOf`, `oneOf`, `not` |
| `Element` | `Attributes`, `Content`, `SimpleContent` (ninguna: contenido vacío y sin atributos) |
| `Text` | `Lexical` (obligatoria), `Ref` (restricción de otro tipo simple), `enum`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `minLength`, `maxLength`, `pattern` |

I14 repite a propósito la comprobación de la Fase 3 ([ciclos.md](ciclos.md)) sobre la IR: la Fase 4 depende de ella para terminar, el frontal XSD no pasa por `semantic` (y XSD prohíbe la derivación circular de tipos simples, que en la IR es un ciclo de `Ref` entre textos), y es un DFS iterativo O(V + E). Cada arista de retroceso del DFS da un error en su origen.

**Obligaciones del frontal XSD.** Los invariantes I12, I13 e I14 coinciden en parte con reglas de XSD que el **usuario** puede romper, y esos errores tienen que salir con su posición en el esquema, no como el error interno de `verify`. El frontal XSD los detecta antes de bajar, y `verify` queda como **defensa en profundidad**:

- la **derivación circular** de tipos simples (`base` que vuelve a sí mismo, directa o indirectamente): el frontal sigue la cadena de bases para fijar el `Lexical` de cada restricción (§5.2), así que debe detectar el ciclo **antes** de seguirla, de forma iterativa (marca de visitados y pila explícita, D7), o no terminaría;
- I12: *all-group limited* y *Element Declarations Consistent*;
- la parte de I13 que viene del usuario: facetas que no se aplican al tipo de la base, valores de `xs:enumeration` que no son del tipo (con la forma léxica de XSD, pendiente 4 de §11), `xs:enumeration` en `xs:boolean`, atributos con `default` y `fixed` a la vez o requeridos con `default`;
- y lo que `verify` no comprueba (abajo): *Unique Particle Attribution* y que `default`/`fixed` sean valores del tipo del atributo. Son restricciones sobre los componentes del esquema (XSD 1.0 Parte 1, *Schema Component Constraints*), es decir, errores del esquema del usuario, y el responsable de darlos con su posición es el frontal XSD.

**Lo que no es invariante.** Que el esquema sea satisfacible (`minimum` > `maximum`, `required` de una propiedad prohibida), igual que en el meta-esquema ([meta-esquema.md](meta-esquema.md) §4): un esquema insatisfacible es válido y no acepta ningún payload. Tampoco el determinismo de los modelos de contenido (*Unique Particle Attribution*): el responsable es el frontal XSD, que lo reporta con su posición (es una *Schema Component Constraint* de XSD 1.0, un error del esquema del usuario); la Fase 4 puede volver a comprobarlo al construir el autómata de Glushkov, donde un modelo no determinista se manifiesta, pero solo como defensa en profundidad. Ni que `default` y `fixed` sean valores del tipo del atributo: también es responsabilidad del frontal XSD. Ni la sintaxis del `pattern`: la comprueba la tarea `pattern` → AFD. Estas cuatro propiedades quedan **fuera de `verify`**.

## 5. Correspondencia con el alcance

### 5.1 JSON Schema (`00-alcance.md` §2)

En JSON la bajada es 1:1 con la tabla de símbolos: **cada subesquema es un nodo con el mismo número** (el `SchemaId` en preorden de [referencias.md](referencias.md) §2 es el `NodeId`), con su padre y su paso. Por eso un nodo por subesquema y el tamaño O(n).

| Palabra clave | En la IR |
|---|---|
| `$schema`, `$id` | Nada: los comprobó la Fase 3 y solo hay referencias locales |
| `$defs`, `definitions` | Cada entrada es un nodo (padre: el esquema que la contiene, paso `Entry("$defs", nombre)`) sin regla propia; se alcanza por las aristas `Ref`. Se bajan todas, también las inalcanzables (D12) |
| `$ref` | `Rule::Ref(destino)`, con el destino que resolvió la tabla de símbolos |
| `$anchor` | Nada: lo consumió la resolución de `$ref` |
| `$comment`, `title`, `description`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` | Nada (anotaciones) |
| `true` / `false` | Nodo sin reglas / `Rule::Reject` |
| `type` | `Rule::Type(TypeSet)` |
| `enum` | `Rule::Enum(literales)`, cada uno en preorden con las claves de los objetos ordenadas (I9) |
| `const` | `Rule::Const(literal)`, su propia regla. No se baja a un `Enum` de un valor: `{"enum": [1, 2], "const": 1}` es válido para el meta-esquema y su nodo tendría dos `Enum`, contra I4. La semántica es la de `00-alcance.md` §2.2 (equivale a un `enum` de un elemento): las dos reglas se cumplen a la vez, y la Fase 4 puede intersecar sus tries (comparando números por valor) o comprobarlas por separado |
| `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` | `Rule::Minimum(Num)`… con el lexema exacto |
| `minLength`, `maxLength` | `Rule::MinLength(u64)`, `Rule::MaxLength(u64)` (contadores: ver abajo) |
| `pattern` | `Rule::Pattern(fuente)`; dialecto ECMA-262 sin anclar, por estar en un nodo `Json` |
| `format` | Nada (anotación) |
| `prefixItems`, `items` | Una sola regla `Rule::Items { prefix, rest }`: `items` solo se aplica a los elementos que siguen a `prefixItems` **del mismo esquema**, así que van juntos |
| `minItems`, `maxItems` | `Rule::MinItems(u64)`, `Rule::MaxItems(u64)` |
| `uniqueItems` | `true` → `Rule::UniqueItems`; `false` → nada |
| `properties`, `additionalProperties` | Una sola regla `Rule::Properties { declared, additional }` (§6.2); `additionalProperties` sin `properties` es `declared` vacío |
| `required` | `Rule::Required(nombres)` |
| `minProperties`, `maxProperties` | `Rule::MinProperties(u64)`, `Rule::MaxProperties(u64)` |
| `dependentRequired` | `Rule::DependentRequired(propiedad → nombres)` |
| `dependentSchemas` | `Rule::DependentSchemas(propiedad → nodo)` (misma instancia) |
| `allOf`, `anyOf`, `oneOf` | `Rule::AllOf(nodos)`… (misma instancia) |
| `not` | `Rule::Not(nodo)` (misma instancia; estrategia en §6.1) |

**Magnitud de los contadores** (el pendiente de [meta-esquema.md](meta-esquema.md) §8). `"maxLength": 1e400` es válido según el meta-esquema. La bajada convierte cada entero no negativo del esquema (los de longitud, ítems, propiedades y ocurrencias) a `u64` **saturando** en `u64::MAX`. Es exacto, no una aproximación: ninguna instancia tiene 2⁶⁴ − 1 puntos de código, elementos o miembros, por la misma razón que el `offset` de `Pos` no se satura (D9: 2⁶⁴ bytes no son alcanzables). Un máximo saturado se comporta como «sin límite» y un mínimo saturado como «insatisfacible», que es lo que significan.

### 5.2 XSD (`00-alcance.md` §3.1 y §3.2)

| Construcción | En la IR |
|---|---|
| `xs:schema` | `Format::Xml { target_namespace }`. `elementFormDefault` no se representa: los elementos se comparan por nombre local en el único espacio de nombres (`00-alcance.md` §1.2) |
| elementos globales | El **documento** (nodo 0, `Element`) tiene `Content` con una `Choice` de una partícula por elemento global, cada una con `Occurs::ONCE`: un documento tiene exactamente un elemento raíz, y cualquier global puede serlo. Esta forma es una **convención del frontal**, no un invariante: I6 solo exige que el documento sea un elemento con `Content` y sin atributos ni contenido simple, así que un frontal con un error que produjera otra forma (por ejemplo `Sequence(a, b)`) pasaría `verify`; lo detectan las pruebas del frontal |
| `xs:element` local | Partícula `Term::Element { name, ty }` con sus `Occurs` |
| `xs:element ref="g"` | Partícula con el nombre de `g` y **el mismo** `ty` que su declaración global (arista compartida) |
| `type="T"` con T complejo | `ty` es el nodo de T, compartido por todas las declaraciones que lo usan; un tipo que contiene un elemento de su propio tipo es un ciclo de E→ (recursión productiva) |
| `type` simple (`xs:int`, un `simpleType`) | `ty` es un nodo `Element` cuya única regla es `SimpleContent(S)`; uno por tipo simple distinto, compartido. Su origen nombra el tipo, no un uso: padre el documento y paso `Entry("simpleContent", nombre del tipo)`, con el prefijo `xs:` si es predefinido (`#/simpleContent/xs:string`; `xs:` es una etiqueta fija, ver los tipos predefinidos abajo) y sin él si es del usuario (`#/simpleContent/string` para un `xs:simpleType name="string"`). Un nombre de tipo del usuario es un NCName, que no admite `:`, así que los dos nunca coinciden. Si el tipo simple es anónimo solo tiene un uso, y el envoltorio es el de la declaración: padre el nodo del tipo que la contiene y paso `Entry("element", nombre)`, como un `complexType` anónimo |
| `xs:complexType` con nombre o anónimo | Nodo `Element`. Con nombre: padre el documento, paso `Entry("complexType", nombre)`. Anónimo: padre el nodo del tipo que contiene la declaración, paso `Entry("element", nombre)` |
| `xs:sequence`, `xs:choice`, `xs:all` | `Term::Sequence`, `Term::Choice`, `Term::All` en la arena de partículas del `Content` (I11, I12) |
| `minOccurs`, `maxOccurs` | `Occurs { min, max }`, `unbounded` = `None`; saturados como en §5.1 |
| complexType sin partículas, con `xs:sequence` o `xs:all` sin hijos, o con `xs:choice` sin hijos y `minOccurs="0"` (XSD 1.0 Parte 1 §3.4.2, cláusulas 2.1.1 a 2.1.3) | Nodo `Element` sin `Content` ni `SimpleContent`: contenido vacío (sin hijos ni texto; comentarios e instrucciones de procesamiento se ignoran, `00-alcance.md` §1.2) |
| `xs:choice` sin hijos con `minOccurs` ≥ 1 (cláusula 2.2: partícula de lenguaje vacío), también anidado en otro grupo | `Content` con la partícula `Choice([])` y sus `Occurs`: **ningún** contenido es válido, ni siquiera el vacío (`<e/>` se rechaza). No es contenido vacío; I11 lo admite (prueba `i11_choice_vacio_con_min_occurs_1_pasa`) |
| `xs:attribute` | `Attribute { name, required, ty, default, fixed }` en `Rule::Attributes`; un atributo no declarado es un error del payload (no hay `xs:anyAttribute`). Las declaraciones `xmlns` no son atributos |
| `xs:simpleContent` + `xs:extension base="S"` | `Rule::SimpleContent(S)` y los atributos de la extensión en `Rule::Attributes` |
| `xs:simpleType` + `xs:restriction base="B"` | Nodo `Text` con `Lexical` (el primitivo de B), `Ref(B)` si B no es un primitivo, y las facetas. Restricción = conjunción: el texto cumple B y las facetas nuevas |
| `xs:simpleType` con nombre | Ese nodo `Text`, con padre el documento y paso `Entry("simpleType", nombre)` (`#/simpleType/Edad`), como un `complexType` con nombre; los predefinidos van aparte, bajo `builtin` (abajo) |
| `xs:simpleType` anónimo | El mismo nodo `Text`, con padre el nodo de la declaración que lo contiene y un paso que la nombra: dentro de `xs:attribute name="a"`, padre el tipo complejo dueño del atributo y paso `Entry("attribute", "a")`; dentro de `xs:element name="e"`, padre el nodo `Element` de contenido simple de `e` y paso `Keyword("simpleType")`; como base anónima de otra restricción, padre el texto derivado y paso `Keyword("restriction")` |
| `xs:element` sin `type`, `ref` ni tipo anónimo (`xs:anyType`) | No tiene representación: `xs:anyType` admite contenido mixto y cualquier elemento, que están fuera del subconjunto (`xs:any`, `mixed="true"`, `00-alcance.md` §3.3). El frontal XSD lo rechaza con un error que lo diga, como D6 |
| `xs:attribute` sin `type` ni tipo anónimo (`xs:anySimpleType`) | Nodo `Text` con `Lexical(String)` y sin facetas: cualquier texto (`anySimpleType` no tiene facetas). Uno solo, compartido, como los predefinidos |
| `enumeration` | `Rule::Enum` de literales escalares del tipo del texto (cadena en `xs:string`, número en los numéricos, entero en `Integer`; I13). `xs:boolean` no admite `enumeration` (XSD 1.0 Parte 2, §3.2.2.1: solo `pattern` y `whiteSpace`), e I13 lo rechaza |
| `minInclusive`, `maxInclusive`, `minExclusive`, `maxExclusive` | `Rule::Minimum`, `Maximum`, `ExclusiveMinimum`, `ExclusiveMaximum`; el frontal reescribe la forma léxica de XSD (`+1`, `.5`, `1.`) al lexema equivalente de RFC 8259 |
| `length`, `minLength`, `maxLength` | `length` = n es `MinLength(n)` y `MaxLength(n)`; en puntos de código, como en JSON |
| `pattern` | `Rule::Pattern(fuente)`; dialecto de XSD, **anclado** (la regex debe cubrir todo el texto), por estar en un nodo `Text`. Varias `pattern` en un mismo paso de restricción se unen con `|` (en XSD son disyunción); las de pasos distintos quedan en nodos distintos unidos por `Ref` (conjunción) |
| `use`, `default`, `fixed` | `required`, `default`, `fixed` de `Attribute`; `fixed` se compara con el valor del atributo en el espacio de valores de su tipo |

Tipos predefinidos (`00-alcance.md` §3.2), cada uno un nodo `Text` que la bajada crea una sola vez, con padre el documento y paso `Entry("builtin", nombre)` (`#/builtin/int`); `xs:anySimpleType` igual, con `Entry("builtin", "anySimpleType")`. El nombre va **sin prefijo** porque `builtin` ya es su propio espacio: ahí solo hay predefinidos. En el paso `Entry("simpleContent", …)` de su envoltorio, en cambio, comparten espacio con los tipos del usuario y llevan `xs:` (fila «`type` simple» de §5.2). Ese `xs:` es una **etiqueta fija** de la IR, no el prefijo del esquema: un esquema que declare el espacio de nombres de XML Schema con otro prefijo (`xsd:string`) da el mismo `#/simpleContent/xs:string`:

| Tipo | Reglas |
|---|---|
| `xs:string` | `Lexical(String)` |
| `xs:boolean` | `Lexical(Boolean)`; de las facetas del subconjunto solo admite `pattern` |
| `xs:decimal` | `Lexical(Decimal)` |
| `xs:integer` | `Lexical(Integer)` |
| `xs:long` | `Lexical(Integer)`, `Minimum(-9223372036854775808)`, `Maximum(9223372036854775807)` |
| `xs:int` | `Lexical(Integer)`, `Minimum(-2147483648)`, `Maximum(2147483647)` |
| `xs:short` | `Lexical(Integer)`, `Minimum(-32768)`, `Maximum(32767)` |
| `xs:byte` | `Lexical(Integer)`, `Minimum(-128)`, `Maximum(127)` |
| `xs:nonNegativeInteger` | `Lexical(Integer)`, `Minimum(0)` |
| `xs:positiveInteger` | `Lexical(Integer)`, `Minimum(1)` |
| `xs:double`, `xs:float` | `Lexical(Double)`, `Lexical(Float)` |

El espacio léxico fija también el tratamiento de los espacios (*whiteSpace*): `collapse` en todos salvo `xs:string` (`preserve`). Los valores especiales `INF`, `-INF` y `NaN` de `xs:float`/`xs:double` son parte de su espacio léxico, pero no se pueden escribir como `Num` ni como literal de `enum` (no son lexemas de RFC 8259): un `minInclusive="-INF"` o una enumeración de `NaN` no tienen representación hoy (pendiente 3 de §11).

## 6. Decisiones pendientes de `00-alcance.md` §5

### 6.1 `not`: complemento real, no verificación diferida

**Decisión.** `not` sobre un subesquema de clase (a) o (b) se implementa como **complemento real**: el veredicto del subautómata sobre la instancia se invierte cuando la instancia termina. Si el subesquema es de clase (c), `not` sigue la estrategia de ese subesquema (producto acotado o verificación diferida) e invierte el veredicto que esta entrega al final de la instancia. Nunca se difiere `not` por ser `not`.

**Por qué basta invertir.** Complementar un autómata finito determinista es intercambiar estados finales y no finales, siempre que sea **completo** (que no se bloquee en ninguna entrada). Pero el subautómata de S puede ser **de pila** (S puede tener `properties`, `items` o contenido), y en un DPDA intercambiar finales no da el complemento en general: puede haber bucles de transiciones ε que nunca terminan de leer la entrada, o pasar por estados finales y no finales con transiciones ε después del último símbolo, de modo que una entrada se acepta y se rechaza a la vez. Aquí no ocurre ninguna de las dos cosas, por tres razones que se cumplen sobre la instancia:

1. *Determinista y sin bucles ε:* las clases (a) y (b) son, por definición, transiciones o registros acotados de un autómata determinista, y las únicas transiciones que no consumen entrada son las aristas de E₌, que no forman ciclos (I14); así que el subautómata siempre lee la instancia entera.
2. *Completo:* en el autómata ponderado (D4) una violación no es una transición que falta sino una transición de peso 1; el subautómata nunca se bloquea y siempre llega al final de la instancia con un peso *w* ∈ ℕ.
3. *El final de la instancia es visible:* la instancia es un valor JSON o un elemento XML, y su final es un token (`}`, `]`, el fin del escalar, `</nombre>`) en el que el marco de la instancia se desapila. Ahí se decide, una sola vez: el veredicto no depende de qué estado alcance el autómata con transiciones ε después del último símbolo, sino del peso del marco al desapilar. Es la misma razón por la que el producto de abajo es un autómata de pila visible, cerrado bajo complemento.

El peso de `not` S sobre *x* es entonces 0 si *w*_S(*x*) > 0 y 1 si *w*_S(*x*) = 0: un bit del marco, sin construir ningún autómata complemento. Esta operación no es del semianillo (no es monótona), así que se aplica como veredicto al desapilar, no a lo largo de las transiciones; las violaciones internas de S no se añaden al reporte, porque dentro de un `not` son lo esperado.

**Lo que sí cuesta.** Lo caro de `not` no es el complemento sino el **producto**: S lee la misma instancia que las reglas hermanas del nodo, igual que una rama de `allOf`. Por eso `not` sigue siendo de clase (c) en el alcance (`00-alcance.md` §2.7) y la tarea 4.3 lo clasificará como un `allOf` de una rama con el veredicto invertido. Observación para la Fase 4: como los subautómatas apilan en los mismos tokens de apertura (el payload decide cuándo se apila y se desapila), el producto es un autómata de **pila visible** (Alur y Madhusudan, 2004) cuyos marcos son tuplas de marcos, y esa clase sí es cerrada bajo intersección y complemento, a diferencia de los deterministas libres de contexto en general ([ciclos.md](ciclos.md) §4). Esto vale si las aristas de E₌ se expanden al compilar (I14 garantiza que la expansión es finita), **compartiendo por `NodeId`** (un subautómata por nodo, memoizado; nunca una expansión en árbol, §7), en lugar de apilar una llamada ε por cada `$ref`; elegir entre las dos es decisión de la Fase 4.

### 6.2 `additionalProperties` con `allOf`

**Decisión.** Semántica de 2020-12 (Core §10.3.2.3): `additionalProperties` solo «ve» las `properties` del **mismo objeto-esquema**; no ve las de las ramas de `allOf`, ni las de un `$ref` hermano, ni las de `dependentSchemas`.

**Cómo lo garantiza la IR.** `properties` y `additionalProperties` son una sola regla, `Rule::Properties { declared, additional }`, dentro de un nodo; las ramas de `allOf` son otros nodos con sus propias reglas. La regla no tiene forma de referirse a nombres de otro nodo, así que la semántica sale de la estructura y no de una convención de la bajada. Ejemplo:

```json
{ "allOf": [ { "properties": { "a": { "type": "integer" } } } ],
  "additionalProperties": false }
```

rechaza `{"a": 1}`: para la raíz, `a` no está en `declared` (vacío), así que es adicional y `false` lo rechaza, aunque la rama lo declare. Es el comportamiento de 2020-12 y de cualquier validador conforme; lo que el usuario suele querer se escribe con `unevaluatedProperties`, que está fuera del subconjunto (`00-alcance.md` §2.8).

**En el DPDA.** En el producto de la raíz con su rama, cada componente tiene su propio trie de claves: al leer `"a":` la componente de la rama apila el marco de `{"type": "integer"}` y la de la raíz apila el de `false`. La clave se clasifica como declarada o adicional **por componente**.

### 6.3 `xs:sequence` (orden fijo) frente a los objetos JSON (sin orden)

**Decisión.** La IR **no traduce** un modelo al otro. Son dos reglas distintas, `Properties` y `Content`, que comparten el mecanismo de despacho pero no la semántica:

| | Objeto JSON (`Properties`, `Required`…) | Contenido XSD (`Content`) |
|---|---|---|
| Orden de los hijos | No importa | `xs:sequence`: el del esquema; `xs:choice`: uno de; `xs:all`: no importa |
| Repetición de un nombre | Prohibida (clave duplicada: payload malformado, D2) | Permitida según `maxOccurs` |
| Lenguaje de los nombres de hijos | Conjunto (cada nombre a lo sumo una vez) | Expresión regular sobre nombres |
| Control del marco | Máscara de bits (claves vistas, `required`): clase (b) | Estado del autómata de Glushkov de las partículas (determinista por UPA): clase (a); contadores de ocurrencias: clase (b) |
| Común | El nombre del hijo (clave o `<nombre`) se busca en un trie y elige el nodo que se apila |

El caso XSD más parecido a un objeto JSON es `xs:all` (cada hijo a lo sumo una vez, en cualquier orden, máscara de bits), y por eso comparten la estrategia (b). Un `xs:sequence` no se puede expresar con `properties` (perdería el orden y las repeticiones) ni un objeto JSON con `xs:sequence` (impondría un orden que JSON no tiene); traducir entre los dos cambiaría el lenguaje aceptado. Como cada IR lleva su formato (`Format`) y un payload JSON solo se valida contra una IR de JSON Schema y uno XML solo contra una de XSD, nunca hace falta.

Lo que D3 pide de la IR común no es que los dos lenguajes se parezcan sino que el resto de la tubería (constructor del DPDA, generador de código, métricas) sea uno solo; eso lo da el mecanismo común (nodos, aristas que consumen, marcos con registros, §7), no una traducción.

## 7. De qué dato de la IR sale cada pieza del DPDA

M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) de la Fase 4, construido por composición: cada nodo produce el subautómata de un marco, y una arista que consume apila el marco de su destino.

| Pieza de M | Dato de la IR | Cómo |
|---|---|---|
| Σ (alfabeto) | `Ir::format` | JSON: las clases de token del lexer JSON (`{ } [ ] , :`, literales, y cadenas y números carácter a carácter); XML: los eventos del lexer XML (apertura `<nombre`, atributo nombre/valor, `>`/`/>`, `</nombre>`, texto) |
| Q, estados de control de un marco | las reglas del nodo que se compilan a estados de control (clase entre paréntesis, §9) | `Type` (a): el primer token elige la rama; `Enum`, `Const` (a): trie sobre los tokens de cada `Literal` (números comparados por valor); `Minimum`…`MultipleOf` (a): comparador decimal sobre el lexema contra `Num::lexeme`; `Pattern` (c): AFD (tarea `pattern` → AFD), sin anclar en `Json` y anclado en `Text`; `Properties::declared` (a): trie de claves; `Content` (a; (b) por `xs:all` y por las partículas con contador de la fila Γ): autómata de Glushkov de `particles`; `Attributes` (b): trie de nombres (la máscara de requeridos va en Γ); `Lexical` (a): AFD del espacio léxico; `Reject` (a): un estado de peso 1 |
| Γ, símbolos de pila | `NodeId` del destino + registros (b) del nodo | marco = (nodo, estado de retorno, registros); la lista de registros sale de las reglas (b) del nodo: contador de puntos de código (`MinLength`/`MaxLength`), de elementos (`MinItems`/`MaxItems`; el índice para `Items::prefix`, acotado por su longitud), de miembros (`MinProperties`/`MaxProperties`), máscara de un bit por nombre de `declared` ∪ `Required` (`Required`, `DependentRequired`, claves duplicadas), máscara de `Term::All` y de `Attribute::required`, contador por partícula con `minOccurs` ≥ 2 o con `maxOccurs` acotado > 1 (saturado en su máximo, o en su mínimo si es `unbounded`); las demás partículas (`?`, `*`, `+`, una sola vez o `maxOccurs="0"`) no lo necesitan, porque el autómata de Glushkov ya expresa sus ocurrencias con estados. Todos tienen tamaño fijo por nodo: `sv_state_t` de tamaño fijo |
| q₀, Z₀ | `NodeId::ROOT` | marco inicial de la raíz con registros a cero |
| δ que apila | aristas de E→ | `Properties` tras leer `"clave":` (destino `declared[k]` o `additional`); `Items` tras `[` o `,` (`prefix[i]` o `rest`); `Content` tras `<nombre` (el `ty` de la partícula, único por I12); `Attributes` y `SimpleContent` al leer el valor o el texto |
| δ que desapila | el token de cierre de la instancia | comprueba los registros: `Required` y `DependentRequired` sobre la máscara (D5), mínimos de los contadores, `Occurs::min` (con el contador de la partícula si lo tiene; si no, lo da el estado del Glushkov) y estado final del Glushkov, máscara de atributos requeridos; aplica `Not` (§6.1) |
| combinación sobre la misma instancia | aristas de E₌ salvo `DependentSchemas`: `Ref` (a), también entre textos, y los combinadores `AllOf`, `AnyOf`, `OneOf`, `Not` (c) | producto de los subautómatas de los destinos (finito por I14), con determinización acotada a 4 096 estados para `anyOf`/`oneOf` (`00-alcance.md` §4). `Ref` es (a): sin reglas hermanas es el subautómata de su destino; con hermanas se combina con ellas como un producto. `Ref` como llamada ε ([ciclos.md](ciclos.md) §4) o expandido (§6.1). En los dos casos E₌ se evalúa **compartiendo por `NodeId`**: un subautómata por nodo, alcanzado por llamadas ε o memoizado al expandir, nunca una expansión en árbol. Una cadena como la de `referencias_compartidas_tamano_lineal` (§8: 10⁴ nodos, 2^(10⁴) caminos) tiene que compilarse en tamaño lineal; se añade al corpus de la Fase 4 |
| verificación diferida | `DependentSchemas` (D5) y `UniqueItems` | no entran en el producto: `dependentSchemas` aplica su nodo al objeto entero solo si la propiedad aparece, y eso se sabe al leer la clave; se verifica de forma diferida (D5), aunque su arista sigue en E₌ para I14 porque valida la misma instancia. `uniqueItems` guarda un hash acotado de cada elemento del marco del arreglo (tope de 4 096 elementos, `00-alcance.md` §4) y lo comprueba al cerrar `]`. `anyOf`/`oneOf` que pasan del tope de estados también acaban aquí |
| F | el marco de la raíz al final de la entrada | se acepta con peso 0; el peso total es el número de violaciones |
| w (D4) | cada `Constraint` | una violación de la regla suma 1; el costo para ordenar las comprobaciones (baratas primero: `type` antes que `enum`) sale de su `RuleKind` |
| ubicación en el reporte | `Constraint::span`, `Origin`, `location_clipped` (pendiente 2 de §11) | el error del payload cita la regla del esquema que falla y su línea y columna; la ruta, recortada a un tope fijo, nunca `Ir::location` entera |
| tope de los registros | `u64` saturados (§5.1) | |

Ninguna pieza necesita algo que la IR no tenga, y ninguna necesita el AST: el AST de cada lenguaje se puede liberar tras la bajada.

## 8. Tamaño y pila

**Tamaño O(n).** En JSON, |N| es el número de subesquemas (uno por `SchemaId`). En XSD, |N| ≤ declaraciones de tipos + un nodo de contenido simple por tipo simple distinto + 13 nodos predefinidos (los 12 tipos de `00-alcance.md` §3.2 y `xs:anySimpleType`) + el documento. Cada regla ocupa lo proporcional al texto que la produce (sus nombres, lexemas y destinos), cada literal tiene un token por token de la entrada, y cada partícula sale de una partícula del esquema. Las aristas no copian subgrafos: un `$ref` usado *k* veces es *k* aristas a un mismo nodo. La prueba `referencias_compartidas_tamano_lineal` construye una cadena de 10⁴ definiciones, cada una con `allOf` a la siguiente dos veces (2^(10⁴) caminos), y comprueba que la IR tiene 10⁴ + 1 nodos y 2·10⁴ − 1 aristas.

**Sin recursión (D7).** Ningún tipo es recursivo (§3). `Ir::verify` es un bucle por nodo y por regla; los literales se recorren con una pila explícita de marcos; el árbol de partículas se comprueba con un vector de «tiene padre» y, para el preorden, con los tamaños de subárbol calculados de atrás hacia delante (los hijos tienen índice mayor, así que ya están calculados al llegar a su grupo); el espacio léxico de cada nodo se precalcula en una pasada, así que comprobar un `Ref` entre textos es O(1) aunque la base tenga muchas reglas o rompa I4; I14 es un DFS con la pila `Vec<(nodo, siguiente arista)>` en el montón sobre las aristas de E₌ en formato CSR. `Ir::location` sube por los padres con un bucle. Costo total O(n + m log m), con n el tamaño de la IR y m el número de errores (al final se ordenan por nodo con un orden estable); con tablas hash para los nombres únicos. Sobre una IR válida m = 0, así que el camino normal es O(n).

## 9. Clasificación (tarea 4.3)

La IR guarda lo necesario para derivarla: la clase de cada regla es función de su `RuleKind`, y la estrategia de un combinador depende de las clases de sus destinos, que son nodos de la IR. `RuleKind::ALL` permite que la tabla de 4.3 se compruebe con un `match` exhaustivo. Se guardará aparte, indexada por `NodeId`.

Clase de cada una de las 31 `RuleKind`, tomada de las tablas de `00-alcance.md` §2 y §3.1 (la fila de donde sale, entre paréntesis):

| `RuleKind` | Clase | Fila de `00-alcance.md` |
|---|---|---|
| `Reject` | (a) | esquemas booleanos (§2.1) |
| `Ref` | (a); con reglas hermanas se combina con ellas como producto (§7) | `$ref` (§2.1); en un texto, `xs:simpleType` + `xs:restriction` (§3.1) |
| `Type`, `Enum`, `Const` | (a) | §2.2; `Enum` en un texto, `enumeration` de `xs:restriction` (§3.1) |
| `Minimum`, `Maximum`, `ExclusiveMinimum`, `ExclusiveMaximum`, `MultipleOf` | (a) | §2.3; las cuatro primeras en un texto, facetas de rango de `xs:restriction` (§3.1) |
| `MinLength`, `MaxLength` | (b) | §2.4; en un texto, facetas de longitud (§3.1) |
| `Pattern` | (c) | §2.4; faceta `pattern` (§3.1) |
| `Items` | (a) | `items`, `prefixItems` (§2.5) |
| `MinItems`, `MaxItems` | (b) | §2.5 |
| `UniqueItems` | (c) | §2.5 |
| `Properties` | (a) | `properties`, `additionalProperties` (§2.6) |
| `Required`, `MinProperties`, `MaxProperties` | (b) | §2.6 |
| `DependentRequired`, `DependentSchemas` | (c) | §2.6 (`dependentRequired` se comprueba con la máscara de bits al cerrar el objeto, D5, pero el alcance la clasifica (c)) |
| `AllOf`, `AnyOf`, `OneOf`, `Not` | (c) | §2.7 |
| `Attributes` | (b) | `xs:attribute` (§3.1) |
| `Content` | (a) con solo `xs:sequence`/`xs:choice`/`xs:element`; (b) si tiene `xs:all` o una partícula con ocurrencias que necesiten contador | `xs:sequence`, `xs:choice`, `xs:element`, `xs:complexType` (a); `xs:all`, `minOccurs`/`maxOccurs` (b) (§3.1) |
| `SimpleContent` | (a) | `xs:simpleContent` + `xs:extension` (§3.1) |
| `Lexical` | (a) | `xs:simpleType` + `xs:restriction` (§3.1): `Lexical` es el tipo primitivo del que parte la restricción |

Dos puntos que el alcance no fija y quedan **pendientes para 4.3** (no se inventan aquí): qué `Occurs` hacen que un `Content` pase a (b) (el alcance clasifica `minOccurs`/`maxOccurs` como (b) sin distinguir; según la regla de §7 (fila Γ), una partícula necesita contador si su `minOccurs` ≥ 2 o si su `maxOccurs` es acotado y > 1, por ejemplo `minOccurs="3" maxOccurs="unbounded"` o `maxOccurs="5"`; `?`, `*` y `+` no lo necesitan en el autómata de Glushkov; si solo las partículas con contador hacen (b) a un `Content` es la decisión de clase que se cierra en 4.3), y la clase de `Lexical` en un tipo predefinido usado sin restricción: `00-alcance.md` §3.2 no da clase a los predefinidos, y aquí se le da la de la fila de `xs:simpleType` porque `Lexical` es su primitivo.

## 10. Pruebas

`crates/ir/tests/forma.rs`:

- **IR de un esquema JSON recursivo** (`json_recursivo_por_properties_pasa`): `type`, `properties` con `$ref: "#"` directo (`hijo`) y por `items` (`hijos`), `required` y `additionalProperties: false`; pasa `verify`, tiene un nodo por subesquema (6) y 7 aristas, y su ubicación es `#/properties/hijos/items`.
- **IR de un XSD** (`xsd_con_sequence_choice_atributo_y_unbounded_pasa`): un pedido con `sequence`, `choice`, atributo requerido de tipo `xs:int`, `maxOccurs="unbounded"`, `simpleContent` con extensión y atributos (uno con `default`), un tipo complejo recursivo por el modelo de contenido (`subpedido`) y tipos predefinidos compartidos.
- **Un caso por invariante**, cada uno con el código esperado: `i1_*` (IR vacía, raíz con padre o con paso, padre que no es anterior), `i2_arista_colgante`, `i3_regla_de_otro_tipo_de_nodo`, `i4_regla_repetida`, `i5_destino_de_otro_tipo`, `i6_formato`, `i7_listas_vacias` (y `enum: []` y `properties: {}` sí pasan), `i8_lexemas_de_rfc_8259`, `i8_multiple_of_positivo`, `i9_literales_bien_formados` (vacío, sobra, falta, clave fuera de objeto, valor sin clave, claves desordenadas o repetidas, número inválido), `i10_nombres_repetidos`, `i11_arbol_de_particulas`, `i11_min_occurs_mayor_que_max_occurs` (y `maxOccurs="0"` pasa), `i12_all`, `i12_declaraciones_consistentes`, `i13_*` (texto sin espacio léxico o con dos, facetas según el espacio léxico, enumeraciones del tipo del texto, restricción que cambia el espacio léxico, `default` con `fixed` y requerido con `default`), `i14_ciclos_que_no_consumen` (`{"$ref": "#"}`, `allOf`/`not`, `dependentSchemas`/`anyOf`, restricción circular de tipos simples) y `one_of_colgante_y_con_ciclo` (I2 e I14 por `oneOf`) e `i14_recursion_productiva_y_diamantes_pasan`.
- **Todos los errores y el mensaje:** `informa_todos_los_errores_en_orden`, `errores_de_ciclo_en_orden_de_nodo` (los de ciclo, que se detectan al final, salen en su nodo), `mensaje_claro`.
- **Ronda de corrección 1:** `enum_y_const_en_el_mismo_nodo` (§5.1), `i11_orden_de_anchura_no_es_preorden`, `i6_nodo_json_en_ir_de_xml_y_contenido_simple_en_el_documento`, `i6_elemento_cuyo_tipo_es_el_documento`, `i4_simple_content_despues_de_content`, `i2_i5_simple_content`, `i8_lexemas_de_las_cotas_exclusivas`, `i12_all_con_max_occurs_2_o_hijo_unbounded`, `i13_cada_faceta_en_texto_equivocado` (incluidos los booleanos en enumeraciones no booleanas), `i13_enumeracion_entera` e `i13_base_con_muchas_reglas` (base inválida de 2·10⁴ reglas y 2·10⁴ restricciones que la referencian; comprueba el resultado, no el tiempo, D1).
- **Ronda de corrección 2:** `i11_preorden_con_grupo_profundo_y_hermano` (`seq[seq[seq[a, b]], c]` en preorden pasa y dos versiones desordenadas no: fija el cálculo de los tamaños de subárbol), `i13_rango_en_float_y_double`, `i10_repetido_en_la_segunda_lista_de_dependent_required`; `i13_cada_faceta_en_texto_equivocado` comprueba además que `xs:boolean` rechaza `enumeration` con su mensaje y admite `pattern`.
- **Ronda de corrección 3:** `one_of_colgante_y_con_ciclo`, `i3_tabla_completa_de_reglas_por_tipo_de_nodo` (`RuleKind::ALL` × `Sort` contra la tabla de §4), `errores_de_texto_en_orden_de_regla` (los errores de I13 salen en el orden de las reglas) y el índice de `Step::Index` en `ubicacion_escapa_segmentos` (`#/…/allOf/0`).
- **Ronda de corrección 4** (una prueba por mutante que sobrevivía): `i14_ciclo_que_no_pasa_por_el_inicio_del_recorrido` (ciclo 1 → 2 → 1 alcanzado desde 0: el DFS marca en gris cada nodo que apila), `i13_enumeracion_entera_casos_limite` (`-0e-5`, exponentes que no caben en `i64`, mantisas largas), `i13_enumeracion_entera_con_e_mayuscula`, `i13_enumeracion_entera_con_exponente_positivo` (`1e+2`), `i13_enumeracion_entera_con_lexema_invalido` (`literal` y `text`, sin pánico), `i8_multiple_of_cero_con_e_mayuscula` (`0E5`), `i11_arbol_roto_no_sigue` (con el árbol roto no se indexa fuera de rango; hijo con índice igual a la longitud), `i12_all_con_min_occurs_2`, `i6_raiz_de_texto_con_modelo_de_contenido`, `reglas_no_admitidas_no_cuentan_para_i13_ni_i14` y `ubicacion_de_ir_invalida_termina` (`location` corta también con la raíz como su propio padre).
- **Revisión, ronda 1:** `i11_choice_vacio_con_min_occurs_1_pasa` (§5.2: `Choice([])` con `minOccurs` ≥ 1, también anidado) y `mensajes_de_cada_clase_de_error` (las 16 variantes de `VerifyErrorKind`: prefijo de posición, principio del motivo y una sola línea); `xsd_con_sequence_choice_atributo_y_unbounded_pasa` comprueba además el origen de los nodos sintéticos (`#/builtin/int`, `#/simpleContent/xs:string`).
- **D7:** `d7_cadena_larga_de_ref` (10⁵ `$ref` encadenados, con y sin ciclo, `location` de 10⁵ segmentos y la liberación de la IR) y `d7_modelo_de_contenido_y_literal_profundos` (10⁵ grupos anidados y un literal de 10⁵ arreglos anidados), en un hilo de 256 KiB.
- **Tamaño:** `referencias_compartidas_tamano_lineal` (§8); `ubicacion_escapa_segmentos` (RFC 6901).

`crates/ir/src/lib.rs`: `RuleKind::ALL` en el orden de la declaración (el verificador indexa por `kind as usize`) y las aristas sobre la misma instancia coinciden con las de [ciclos.md](ciclos.md) §2.

## 11. Decisiones y pendientes

Decisiones de esta tarea (candidatas al registro del plan; las numera el usuario):

1. Crate `ir` aparte que solo depende de `lexer`; grafo en arena con `NodeId`; `$ref` y los tipos de XSD como aristas compartidas; clase y estrategia en una tabla aparte (§1, §3).
2. En JSON, `NodeId` = `SchemaId` (un nodo por subesquema, mismo preorden), y se bajan también las definiciones inalcanzables (D12).
3. `not` como complemento real por inversión del veredicto al cerrar la instancia (§6.1); `additionalProperties` con la semántica de 2020-12, garantizada por la forma de `Rule::Properties` (§6.2); `xs:sequence` y los objetos JSON no se traducen entre sí (§6.3).
4. Contadores en `u64` saturado (§5.1), que cierra el primer punto de [meta-esquema.md](meta-esquema.md) §8.
5. Literales en preorden con las claves ordenadas; los números conservan su lexema y se comparan por valor en la Fase 4 (§3, I9).
6. En XML, la raíz es el documento: un elemento cuyo contenido es la elección de los globales (§5.2).
7. El contenido vacío de XSD no admite texto, ni siquiera espacios (XSD 1.0 §3.4.4, *Element Locally Valid (Complex Type)* 2.1); en contenido de solo elementos los espacios entre elementos se ignoran (`00-alcance.md` §1.2).
8. `const` es su propia regla (`Rule::Const`), no un `Enum` de un valor, para que `enum` y `const` convivan en un nodo (§5.1).
9. Un `xs:element` sin tipo (`xs:anyType`) lo rechaza el frontal XSD (añadido a `00-alcance.md` §3.3; lo confirma el usuario en el punto de control A); un `xs:attribute` sin tipo (`xs:anySimpleType`) es un texto `Lexical(String)` sin facetas (§5.2).
10. I11 exige preorden de verdad (tamaños de subárbol), no solo un orden topológico.
11. Una `xs:enumeration` de un tipo entero tiene que ser entera en el espacio de valores, sobre el lexema de RFC 8259 que deja el frontal (I13; la forma léxica de XSD la comprueba el frontal, pendiente 4); las facetas de rango de un tipo entero no se comprueban así porque `minInclusive="0.5"` en `xs:integer` es un error del esquema que debe dar el frontal XSD con su posición.
12. `xs:boolean` no admite `xs:enumeration` (XSD 1.0 Parte 2, §3.2.2.1), e I13 lo rechaza; ningún texto admite un booleano en una enumeración.

Pendientes:

1. La bajada de JSON Schema (4.2) y el frontal XSD llaman a `Ir::verify` **siempre, en el camino normal**, antes de la Fase 4; un fallo es un error del frontal y se presenta al usuario como un único error interno (§4, «Contrato»).
2. **Requisito de las Fases 4 y 5.** `Ir::location` construye la ruta entera y no tiene tope: con claves largas una sola ruta puede medir cientos de KB, y un reporte del payload que la repita en cada violación, o un codegen que precalcule la de cada nodo, amplificaría la entrada sin cota. Los mensajes del validador y el codegen usarán `location_clipped(id, max)`, análogo a `SymbolTable::fragment_clipped` (`crates/semantic/src/symbols.rs`; [ciclos.md](ciclos.md) §5): sube por los padres en O(profundidad + max) y nunca construye la ruta entera. `max` se fija en `00-alcance.md` §4 cuando se implemente; `Ir::location` sin recortar queda para pruebas y depuración. Cuando existan los errores del payload, su fila se añade a la tabla ρ de [pendientes.md](pendientes.md) §1. No se implementa en 4.1.
3. `INF`, `-INF` y `NaN` de `xs:float`/`xs:double` en facetas y enumeraciones no tienen representación (no son lexemas de RFC 8259); si el frontal XSD los encuentra, debe rechazarlos con un error que lo diga, o hay que ampliar `Num`.
4. La comprobación de *Unique Particle Attribution* y de que `default`/`fixed` sean valores del tipo del atributo es responsabilidad del **frontal XSD**, que las reporta con su posición (*Schema Component Constraints* de XSD 1.0), fuera de `verify` (§4); la Fase 4 puede repetir la de UPA al construir Glushkov solo como defensa en profundidad. También la **forma léxica de XSD** de facetas y enumeraciones: el frontal la valida contra el tipo **antes** de reescribirla a RFC 8259 (§5.2), porque I13 comprueba el valor y no la forma: `xs:integer` no admite `1.0` ni `1e2` y `xs:decimal` no admite exponente, así que sin esa comprobación `<xs:enumeration value="1.0"/>` en `xs:integer` pasaría.
5. Un aviso (no error) para el patrón «`allOf` + `additionalProperties: false`» de §6.2, que casi nunca es lo que se quiere, podría añadirse en la bajada de 4.2.
6. **Opción propuesta, la decide el usuario:** un tipo verificado. Hoy `Ir::nodes` y `Num::lexeme` son públicos, así que una IR ya verificada se puede mutar antes de la Fase 4, y nada en los tipos obliga a verificarla. La opción es que `verify` consuma la IR y devuelva `Result<VerifiedIr, (Ir, Vec<VerifyError>)>`, que `VerifiedIr` solo dé acceso de lectura (con `Ir::node` sin pánico posible) y que `automaton` acepte solo `&VerifiedIr`. Cambia la API, así que no se hace en 4.1: hasta entonces vale el contrato de §4.
