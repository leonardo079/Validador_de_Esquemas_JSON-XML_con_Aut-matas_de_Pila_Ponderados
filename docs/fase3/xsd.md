# Fase 3 — Análisis y bajada del subconjunto XSD a la IR

Tarea 5.3 (semana 3). Entrada: el AST de un esquema XSD que dio el parser de la Fase 2 ([gramatica-xsd.md](../fase2/gramatica-xsd.md)). Salida: la IR común de [ir.md](ir.md), **siempre verificada** (`VerifiedIr`, D15), o la lista de los errores del esquema con línea y columna. Es el equivalente XSD de la cadena meta-esquema → tabla de símbolos → ciclos → bajada de JSON Schema ([referencias.md](referencias.md), [bajada.md](bajada.md)).

Implementación: [`crates/xsd/src/symbols.rs`](../../crates/xsd/src/symbols.rs) (tablas de símbolos y tipos predefinidos), [`check.rs`](../../crates/xsd/src/check.rs) (`xsd::analyze`: recorrido, resolución y restricciones) y [`lower.rs`](../../crates/xsd/src/lower.rs) (`xsd::lower`).

```
bytes ─parse─▶ AST ─analyze─▶ Analysis ─lower─▶ Ir ─verify─▶ VerifiedIr
               (Fase 2)  │                          └─ falla ─▶ InternalError (D15)
                         └─ falla ─▶ Vec<XsdError> (todos los de la primera etapa que falla)
```

**Alcance de esta tarea.** Tipos complejos (con nombre y anónimos), elementos globales y locales, `ref`, grupos `xs:sequence`/`xs:choice`/`xs:all` con ocurrencias, atributos y `xs:simpleContent` + `xs:extension` sobre un tipo predefinido. Quedan fuera, con un error que lo dice: los `xs:simpleType` del usuario y sus facetas (`pendiente (tarea 5.6)`; tampoco se comprueba todavía que `default`/`fixed` sean valores del tipo), la forma léxica de los valores (5.5), *Unique Particle Attribution* y el tope de partículas (5.7, D25, D26). La integración en `schemac` es de la tarea 5.4 (§9).

## 1. Entrada y salida

`xsd::analyze(&Schema) -> Result<Analysis<'_>, Vec<XsdError>>` y `xsd::lower(&Analysis<'_>) -> Result<VerifiedIr, InternalError>`. A diferencia de `semantic::lower`, que acepta cualquier tabla ([pendientes.md](pendientes.md) §13), `lower` solo acepta un `Analysis`, que solo se obtiene de `analyze` sin errores (campos privados): **el tipo garantiza que la bajada parte de un esquema analizado**. `Analysis` guarda referencias al AST, así que no lo copia ni lo sobrevive.

## 2. Tabla de símbolos

XSD 1.0 tiene un **espacio de símbolos por clase de componente global**: elementos, tipos y atributos (también grupos, fuera del subconjunto). Los tipos complejos y los simples **comparten** el suyo (un `type="T"` no dice cuál de los dos busca). Sea *tns* el `targetNamespace` (o ninguno). Cada declaración global con `name="n"` entra en su espacio con la clave (*tns*, *n*); repetir una clave es el error `duplicate-name`, en el nombre de la segunda y con la posición de la primera en el mensaje.

**Clave.** La clave es la **identidad** del URI y el nombre local, `(Option<&NsId>, &str)`: `(Name::ns_id(), Name::local())` en una referencia y `(Schema::target_namespace, name)` en una declaración, con el `NsId` que internó la 5.2 para el `targetNamespace` ya recortado ([lector-xsd.md](../fase2/lector-xsd.md) §3). Un `NsId` se compara por puntero y se hashea por la dirección de su URI, en O(1): una consulta cuesta O(|nombre local|), nunca O(|URI|). Indexar por el texto del URI (`Name::expanded()`) sería O(referencias · |URI|), alcanzable con un URI de varios MiB ([../fase2/pendientes.md](../fase2/pendientes.md) §11). Como la dirección no es aleatoria, las tablas usan el *hasher* aleatorio de `std` (`RandomState`), nunca uno determinista (5.1-SEC3-I1).

**Resolución** de un nombre de tipo *q* = (*u*, *l*) (`type` de un elemento o de un atributo, `base` de `xs:extension`):

1. si (*u*, *l*) está en la tabla de tipos, ese tipo;
2. si no, y *u* es el espacio de XML Schema: si *l* es uno de los doce predefinidos de `00-alcance.md` §3.2, ese predefinido; si es otro nombre de XSD 1.0 (`anyType`, los de fecha y hora, `token`, `anySimpleType`…), error `unsupported` con su categoría (D6); si no, `unresolved`;
3. si no, `unresolved` («no hay ningún tipo "p:T" en el espacio de nombres "urn:…"», con el URI recortado).

Un `ref="g"` solo se busca en la tabla de **elementos** (un tipo con el mismo nombre no cuenta). Sin espacio de nombres por defecto, `type="T"` en un esquema con `targetNamespace="urn:t"` es (ninguno, `T`), no (`urn:t`, `T`): da `unresolved` (`semantic-error/type-de-otro-espacio.xsd`). `xs:anySimpleType` no se puede nombrar (no está en §3.2): es solo el tipo implícito de un `xs:attribute` sin tipo.

**Atributos globales** (5.2-CR-S2). Sin `ref` de atributo (fuera del subconjunto) no se pueden usar: tienen su tabla (nombres únicos) y se comprueban como cualquier `xs:attribute`, pero no se bajan a la IR.

## 3. Restricciones comprobadas

Todas son *Schema Component Constraints* o *XML Representation Constraints* de XSD 1.0 que caen en el subconjunto y que la IR necesita ([ir.md](ir.md) §4, «Obligaciones del frontal XSD»; [pendientes.md](pendientes.md) §3). Se informan con su posición en el esquema del usuario, nunca como el error interno de `verify`.

**Por etapas (B2).** Como `semantic::analyze`, el análisis devuelve **todos** los errores de la primera etapa que falla, ordenados por posición (offset): la etapa 2 necesita los tipos resueltos en la 1, y repetir sus consecuencias confundiría.

| Etapa | Restricción | `code()` | Posición |
|---|---|---|---|
| 1 | `name` no es un NCName | `value` | primer carácter del valor |
| 1 | `minOccurs`/`maxOccurs` que no es `xs:nonNegativeInteger` (signo opcional, `-` solo con valor 0) o, en `maxOccurs`, `unbounded` | `value` | valor |
| 1 | `xs:element` local sin `name` ni `ref` | `syntax` | `<` del elemento |
| 1 | `name` con `ref` (src-element 2.1); `ref` con `type` o con tipo anónimo (2.2); `type` con tipo anónimo (3) | `syntax` | valor de `ref`; valor de `type` o `<` del tipo anónimo; `<` del tipo anónimo |
| 1 | `xs:element` sin `type`, `ref` ni tipo anónimo: `xs:anyType` (D14); `type="xs:anyType"` | `unsupported` | `<` del elemento; valor |
| 1 | Nombre global repetido en su espacio (elementos, tipos, atributos globales) | `duplicate-name` | `name` de la segunda (el mensaje da la primera) |
| 1 | I10: dos `xs:attribute` con el mismo nombre en un tipo (o en su `xs:extension`) | `duplicate-name` | ídem |
| 1 | I13: `default` y `fixed` a la vez (src-attribute 1); `use="required"` con `default` (2); `type` con `xs:simpleType` (4) | `syntax` | el valor que va segundo; valor de `default`; `<` del tipo anónimo |
| 1 | `type`, `base` o `ref` sin resolver; `ref` que no nombra un elemento global | `unresolved` | valor |
| 1 | Tipo de un atributo que es complejo | `unresolved` | valor de `type` |
| 1 | Tipo predefinido fuera de §3.2 (fecha y hora, `token`…) | `unsupported` | valor |
| 1 | D27 (U7): `xs:simpleContent` + `xs:extension` cuya `base` es un tipo complejo | `unsupported` | valor de `base` |
| 1 | `xs:simpleType` (global o anónimo): pendiente de la tarea 5.6 | `unsupported` | `<` de `xs:simpleType` |
| 2 | I11: `minOccurs` > `maxOccurs` (también con `maxOccurs="0"` y `minOccurs` por defecto), comparados tras saturar en `u64::MAX` (ver abajo) | `content-model` | valor de `maxOccurs` si está; si no, de `minOccurs` |
| 2 | I12, *all-group limited* (§3.8.6): `xs:all` con `minOccurs` ∉ {0, 1} o `maxOccurs` ≠ 1; sus elementos con `minOccurs` o `maxOccurs` ∉ {0, 1} | `content-model` | valor del atributo |
| 2 | I12, *Element Declarations Consistent*: dos partículas del mismo modelo con el mismo **nombre local** y distinto tipo (también en grupos anidados y con `ref`); desviación de XSD 1.0, abajo | `content-model` | `<` de la segunda (el mensaje da la primera) |

**EDC por nombre local (desviación, 5.3-CR1-I1; D40).** XSD 1.0 (*cos-element-consistent*) compara el nombre **expandido** {name, target namespace}: con `targetNamespace="urn:x"` y `elementFormDefault` por defecto (`unqualified`), un elemento local `<xs:element name="a" type="xs:string"/>` (sin espacio de nombres) y un `<xs:element ref="t:a"/>` a un global `a` de tipo `xs:int` (en `urn:x`) pueden convivir en un mismo modelo. Aquí es `content-model`: la IR nombra elementos y atributos solo por su nombre local, en el único espacio de nombres admitido ([ir.md](ir.md) §5.2, fila `xs:schema`; `00-alcance.md` §1.2), e I12 exige que dos partículas con el mismo nombre local tengan el mismo tipo (el nombre del hijo decide el marco del autómata). Rechazar es lo necesario para no producir una IR que rompa I12; la 5.7 la documenta en `00-alcance.md` §3 ([../fase2/pendientes.md](../fase2/pendientes.md) §17).

**Ocurrencias mayores que 2⁶⁴ (5.3-CR1-S2).** `minOccurs` y `maxOccurs` se saturan en `u64::MAX` antes de comparar (§4, `ir.md` §5.1), así que `minOccurs="18446744073709551617" maxOccurs="18446744073709551616"` no da el error de I11: los dos valen `u64::MAX`. Es inocuo (un mínimo saturado ya es insatisfacible: ningún documento tiene 2⁶⁴ − 1 hijos), pero acepta un esquema que XSD rechaza; queda anotado en [pendientes.md](pendientes.md) §33.

Lo que ya rechaza el parser no se repite aquí: `minOccurs`/`maxOccurs` en un elemento global (`parse-error/min-occurs-global.xsd`), `ref` en un elemento global, `use="prohibited"`, `mixed="true"`, los atributos fuera de la lista blanca y los valores de D39 que cambian la validación ([gramatica-xsd.md](../fase2/gramatica-xsd.md) §4-§5). D14 no lo puede rechazar el parser: depende de tres atributos y del hijo. La «identidad» de un tipo en EDC es la de su declaración: dos tipos anónimos son distintos aunque sean iguales, como en XSD.

Los mensajes son de una línea y recortan los textos del usuario (nombres, QName y URI) a 33 caracteres con `lexer::short`, como los del parser (`XsdError::new`); la prueba `errores_con_nombres_largos_se_recortan` lo comprueba con un nombre y un URI de 1 MiB.

## 4. Bajada

La correspondencia es la de [ir.md](ir.md) §5.2. Resumen de lo que fija esta tarea:

| AST | IR |
|---|---|
| `xs:schema` | `Format::Xml { target_namespace }` con el URI recortado; nodo 0, el **documento**: `Element` con `Content` = `Choice` de una partícula `Occurs::ONCE` por elemento global, en orden del documento (convención de [ir.md](ir.md) §5.2; sin elementos globales, `Choice([])`: ningún documento es válido). Origen: el `xs:schema`, `Step::Root` |
| `xs:complexType name="T"` | Nodo `Element`, padre el documento, paso `Entry("complexType", "T")` |
| tipo anónimo de un elemento global `e` | Nodo `Element`, padre **el documento**, paso `Entry("element", "e")` (B3; S-5, primera mitad) |
| tipo anónimo de un elemento local `e` | Nodo `Element`, padre el tipo cuyo modelo contiene `e`, paso `Entry("element", "e")` |
| modelo de contenido | `Content(ContentModel)`: la arena en preorden que aplanó el análisis, con los mismos índices; `Occurs` saturados en `u64::MAX`; `unbounded` = `None`. Sin `Content` si el contenido es vacío (§3.4.2, cláusulas 2.1.1-2.1.3: sin modelo, `xs:sequence`/`xs:all` sin hijos o `xs:choice` sin hijos con `minOccurs="0"`); un `xs:choice` vacío con `minOccurs` ≥ 1 es `Choice([])` |
| `xs:element name="e" type="T"` | `Term::Element { name: "e", ty }` con `ty` el nodo de T (complejo) o el envoltorio `simpleContent/xs:T` (predefinido) |
| `xs:element ref="g"` | `Term::Element` con el nombre de `g` y **el mismo** `ty` que su declaración global |
| `xs:simpleContent` + `xs:extension base="xs:T"` | `SimpleContent(builtin/T)` y después `Attributes` |
| `xs:attribute` | `Attribute { name, required, ty, default, fixed }` en `Attributes`; `ty` el nodo `builtin/T`, o `builtin/anySimpleType` sin tipo; `default`/`fixed` tal como los dejó el lector (su *whiteSpace* y su tipo, 5.6) |
| predefinido `xs:T` | Nodo `Text`, padre el documento, paso `Entry("builtin", "T")`: `Lexical` y, en los enteros acotados, `Minimum`/`Maximum` (tabla de [ir.md](ir.md) §5.2) |
| envoltorio de `xs:T` | Nodo `Element` con solo `SimpleContent(builtin/T)`, padre el documento, paso `Entry("simpleContent", "xs:T")`, con la etiqueta fija `xs:` sea cual sea el prefijo del esquema |
| `xs:attribute` global; `attributeFormDefault`, `form`, `nillable`, `abstract` con el valor sin efecto (D39); `xs:annotation`, `id` (D23) | nada |

**Rangos.** Cada declaración lleva el rango de su `xs:complexType` (el documento, el de `xs:schema`); `Content`, el del grupo raíz; `SimpleContent`, el de `xs:simpleContent`; `Attributes`, desde el primer `xs:attribute` hasta el final del último; cada partícula y cada `Attribute`, el de su construcción. Los nodos sintéticos no tienen construcción propia: llevan, en su origen y en sus reglas, el rango de su **primer uso** (el valor de `type` o `base`, o el `xs:attribute` entero si no tiene tipo).

**Orden de los `NodeId` (B3).** (i) El documento, 0. (ii) Los tipos complejos en **preorden de declaración**: los globales (`xs:complexType` con nombre y tipos anónimos de los elementos globales) en el orden del documento, y cada uno seguido, antes que su hermano siguiente, de los tipos anónimos de su modelo de contenido, recursivamente; el tipo *i* del preorden es el nodo 1 + *i*. (iii) Los nodos sintéticos, en el orden de su **primer uso** al bajar (i) y (ii) en orden de `NodeId` (en cada nodo: las partículas en preorden, después la base de `xs:simpleContent` y después los atributos); el envoltorio de `xs:T` va seguido, si también es su primer uso, de `builtin/T`. Cada sintético se crea una sola vez y solo si se usa (como en `pedido_xsd`). Para `pedido.xsd`: `#`, `#/complexType/Pedido`, `#/complexType/Pedido/element/item`, `#/simpleContent/xs:string`, `#/builtin/string`, `#/simpleContent/xs:decimal`, `#/builtin/decimal`, `#/builtin/int`.

El preorden de (ii) se obtiene sin recursión: una pila de tipos pendientes se inicializa con los globales del revés; al sacar un tipo se le da el número siguiente, se aplana su modelo y se apilan del revés los tipos anónimos que contiene. Como un tipo se numera al salir de la pila y sus hijos entran encima de sus hermanos, sale exactamente el preorden. El número de un tipo anónimo se anota en la partícula que lo declara al dárselo (la partícula ya existe, en la arena de su padre).

**Aplanado del modelo.** El árbol de grupos de un tipo se aplana en una arena en preorden con otra pila: cada partícula se añade al sacarla y se anota como hija de su grupo (que ya está en la arena, porque salió antes); los hijos se apilan del revés, así que se anotan en orden. Es la forma que exige I11 (el primer hijo de un grupo es la partícula siguiente y cada hijo empieza donde acaba el subárbol del anterior), y la bajada la copia con los mismos índices.

## 5. Corrección

**Proposición.** Si `analyze` devuelve un `Analysis`, `lower` devuelve una IR que pasa `verify` (I1-I14), y su semántica ([ir.md](ir.md) §2) es la del esquema en el subconjunto: un documento cumple el nodo 0 si y solo si es válido contra el esquema.

*Esbozo por invariantes.* I1: el padre de un tipo es el documento o un tipo anterior en el preorden; el de un sintético, el documento. I2: todo destino es 1 + *i* con *i* un tipo existente, o un sintético creado antes de usarlo; la constante colgante `usize::MAX` solo aparece con un `TypeRef` sin resolver, que un `Analysis` sin errores no tiene. I3-I5: los tipos complejos y los envoltorios son `Element` y solo llevan `Content`, `SimpleContent` y `Attributes`, a lo sumo una de cada y nunca `Content` con `SimpleContent` (un `ComplexContent` es de elementos o simple); las partículas apuntan a `Element` (un tipo o un envoltorio) y los atributos y `SimpleContent` a `Text` (un predefinido; un tipo complejo como tipo de atributo o como base es un error de la etapa 1). I6: el documento es `Element` con `Content` y sin más; ninguna partícula apunta a 0 (los tipos empiezan en 1). I7-I9: no hay listas de `allOf`/`type` ni literales; los `Num` son las cotas fijas de los predefinidos, lexemas de RFC 8259. I10: etapa 1 (atributos repetidos). I11: el aplanado es un preorden por construcción, y `minOccurs` ≤ `maxOccurs` es de la etapa 2. I12: etapa 2 (`xs:all` solo puede ser raíz por la gramática). I13: los predefinidos tienen exactamente un `Lexical`, sin facetas que no apliquen; los atributos, sin `default` con `fixed` ni requeridos con `default` (etapa 1). I14: no hay aristas sobre la misma instancia (sin `xs:simpleType` no hay `Ref` entre textos). La semántica coincide con XSD por la correspondencia de §4 y [ir.md](ir.md) §5.2: el documento es la elección de los globales; cada partícula valida su elemento con el tipo resuelto, y un `ref` con el de su declaración global.

La prueba `corpus_entero_sin_error_interno` comprueba la primera parte sobre todo el corpus XSD: cada esquema da una IR verificada o un error del usuario, nunca el error interno.

## 6. Costo y pila

Sea *n* el tamaño del AST (construcciones, nombres y valores).

- **Recorrido** (preorden y aplanado): cada tipo entra y sale una vez de la pila de tipos, cada grupo y cada elemento una vez de la pila de su modelo. O(n).
- **Tablas**: una inserción por declaración global y una consulta por `type`/`ref`/`base`, O(1) esperado más el hash del nombre local (§2). La prueba `uri_de_8_mib_y_medio_millon_de_referencias` usa un URI de 8 MiB como `targetNamespace` y como prefijo de 5·10⁵ referencias `type='p:T'`: con una clave por texto serían ~4·10¹² bytes hasheados; comprueba el resultado (3 nodos y 1 + 5·10⁵ aristas), no el tiempo (D1).
- **Comprobaciones**: una pasada por los atributos de cada tipo (tabla de nombres para I10) y otra por su arena (ocurrencias, `xs:all`, tabla de nombres para EDC). Los errores se ordenan al final: O(*m* log *m*) con *m* errores.
- **Bajada**: un nodo por tipo complejo, una partícula por partícula y un atributo por atributo, más a lo sumo 13 predefinidos y 12 envoltorios. **Cada uso de un tipo es una arista, nunca una copia** (`tipo_usado_k_veces_es_una_arista_por_uso`: con 10 y 10⁴ usos el número de nodos es el mismo y las aristas crecen en uno por uso). Después, `verify`, O(n) sobre una IR válida ([ir.md](ir.md) §8).

**Sin recursión (D7).** Análisis y bajada usan solo las dos pilas explícitas de §4 y bucles. El AST sí tiene `Clone`, `Debug`, `PartialEq` y `Drop` recursivos (acotados por la profundidad de 256, D36), así que quien tiene el AST lo libera en un hilo con `stack_size` (D7, [../fase2/pendientes.md](../fase2/pendientes.md) §5). La prueba `analisis_y_bajada_sin_recursion` parsea la cadena más profunda (`element`/`complexType`/`sequence`, 84 tipos anónimos anidados) en un hilo de 2 MiB, la analiza y la baja en otro de 64 KiB y la libera de vuelta en el primero.

## 7. Pruebas

`crates/xsd/tests/lower.rs`:

- `pedido_baja_igual_que_la_ir_hecha_a_mano`: `parse-ok/pedido.xsd` da la IR de `pedido_xsd` (copia de `crates/ir/tests/forma.rs`) comparada nodo a nodo por ubicación (reglas, destinos y padre como ubicaciones, rangos aparte), con el orden de nodos de B3 y los rangos de declaraciones, reglas y del primer uso de `xs:int`.
- `esquemas_de_los_casos_bajan` (los dos XSD de `tests/cases`, con el tipo anónimo del `pedido` global colgando del documento), `corpus_entero_sin_error_interno` (todo `parse-ok`, `parse-error`, `semantic-error` y `tests/cases`) y `errores_semanticos_con_linea_y_columna` (manifiesto `tests/schemas/errores-semanticos-xsd.json`: código, línea, columna, texto del mensaje y, con `count`, el número de errores de la etapa; 39 casos en `tests/schemas/xsd/semantic-error`, al menos uno por fila de §3).
- `correspondencia_de_cada_construccion` (ocurrencias saturadas, `ref` compartiendo el tipo, `xs:anySimpleType`, contenido vacío y `Choice([])`, `targetNamespace`, atributos globales sin bajar), `orden_de_nodos_b3`, `d39_no_llega_a_la_ir`, `errores_por_etapas_en_orden_del_documento`, `errores_con_nombres_largos_se_recortan`, `tipo_usado_k_veces_es_una_arista_por_uso`, `uri_de_8_mib_y_medio_millon_de_referencias` y `analisis_y_bajada_sin_recursion` (§6).

`crates/xsd/src/check.rs`: `ocurrencias_como_entero_no_negativo_saturado`.

## 8. Decisiones y pendientes

Las decisiones de esta tarea están registradas como **D40** en el plan general (y los valores por defecto sin efecto del parser, como **D39**):

- API `xsd::analyze`/`xsd::lower`; `Analysis` con campos privados es la única entrada de `lower` (para XSD resuelve el `AnalyzedTable` de [pendientes.md](pendientes.md) §13); un tipo sin resolver da una arista colgante (error interno), nunca pánico (§1).
- B2: dos etapas, componentes y nombres; modelos de contenido; todos los errores de la primera que falla, por offset; los códigos y las posiciones de §3.
- B3: documento, tipos complejos en preorden de declaración y sintéticos por primer uso (envoltorio antes que su predefinido), con el rango de su primer uso; el tipo anónimo de un global cuelga del documento (§4).
- Resolución por `(NsId, nombre local)` con `RandomState` (D31): primero el esquema, después §3.2; `xs:anySimpleType` no se puede nombrar; `xs:anyType`, fecha y hora y el resto de XSD 1.0 son `unsupported` (§2).
- Los atributos globales se comprueban y no se bajan (§2); contenido vacío según XSD §3.4.2, cláusulas 2.1.1-2.1.3, y documento `Choice([])` sin globales (§4).
- D39 en el parser: lista blanca solo con el valor sin efecto ([gramatica-xsd.md](../fase2/gramatica-xsd.md) §5).
- EDC por nombre local, desviación de XSD 1.0 (§3).

Abiertos: [pendientes.md](pendientes.md) §3 (5.5, 5.6 y 5.7) y §33 (observaciones de la revisión de la 5.3); [../fase2/pendientes.md](../fase2/pendientes.md) §12 (memoria, Fase 7 y 5.7) y §17 (alcance §3, 5.7).

## 9. En `schemac` (tarea 5.4)

`schemac check <esquema>` y `schemac ir [--dot] <esquema>` aceptan un XSD sin opción ni extensión: el formato sale del contenido (D22; si el primer byte significativo tras un BOM y espacios es `<`, es XSD), con el mismo hilo de 8 MiB (D7), el mismo tope de 16 MiB y los mismos códigos de salida que con JSON Schema ([bajada.md](bajada.md) §5):

```
bytes ─formato─▶ XSD ─xsd::parse─▶ AST ─xsd::analyze─▶ Analysis ─xsd::lower─▶ VerifiedIr ─▶ write_json / write_dot
                   │                  └─ Vec<XsdError> (código 1)            └─ InternalError (código 3)
                   └─ JSON ─▶ schema_parser ─▶ semantic (sin cambios)
```

- **`check`**: 0 con `ruta: esquema válido (subconjunto de XSD 1.0); N tipos complejos` (`Analysis::type_count`: los tipos complejos con nombre y anónimos, que son los nodos de declaración de la IR); 1 con el error de la Fase 2 (el lector y el parser se detienen en el primero) o todos los de la primera etapa de `xsd::analyze` que falla (§3), cada uno `ruta: línea L, columna C: …` en una línea; 3 si el análisis hace pánico (D33).
- **`ir`**: 0 con la IR JSON v2 (`format: "xml"`, `targetNamespace` recortado o `null`, `class` y `strategies` por nodo, [clasificacion.md](clasificacion.md) §5) o con el DOT; 1 con los mismos mensajes que `check`; 3 si `verify` falla o hay un pánico.
- **AST en el hilo** (D36): `xsd::parse`, `analyze`, `lower` y la exportación corren dentro del hilo de 8 MiB, y el AST se libera allí; del hilo solo sale texto.
- **Clasificación**: el subconjunto XSD no tiene reglas (c) (`00-alcance.md` §3.1): un nodo es (b) si tiene `attributes` o un `content` con `xs:all` o con contador; si no, (a). El pedido de `tests/cases` (`valid-xml-pedido-01`) tiene un solo nodo (b), el tipo anónimo de `pedido`, por el `attributes` de `id`.

**Doradas y DOT.** [`tests/ir/xsd`](../../tests/ir/xsd) tiene las IR doradas de `pedido`, `valid-xml-pedido-01` y `espacio-por-defecto` (este, con `targetNamespace`), de los XSD de `tests/schemas/xsd/parse-ok` con el mismo nombre; `make doradas` las regenera junto a las de JSON. Van en un subdirectorio porque `ir_doradas` (`crates/semantic/tests/lower.rs`) recorre los `.json` de `tests/ir` buscando su fuente en `tests/schemas/parse-ok`. [ir-pedido.dot](ir-pedido.dot) y [ir-pedido.svg](ir-pedido.svg) son la IR de `pedido.xsd` (variable `IR_DOT_XSD` del `Makefile`; `make docs-check` comprueba el `.dot`; descrito en [bajada.md](bajada.md) §5.2).

**Pruebas** (además de las de la CLI, [bajada.md](bajada.md) §7): `crates/xsd/tests/corpus.rs`, `corpus_clasificado` (todo XSD de `parse-ok` que pasa el análisis, al menos 7, y los dos casos de `tests/cases` con el texto de su campo `schema`, que es byte a byte su copia en `parse-ok`, cumplen la definición de la clasificación nodo a nodo y no tienen nodos (c); el pedido, un (b) por `Attributes`; `malformed-xml-billion-laughs-01`, ninguno) e `ir_doradas_xsd`.

**Amplificación** ([pendientes.md](pendientes.md) §1): la peor clase de error XSD medida da ≈ 21 veces la entrada con una ruta de 8 bytes (un `xs:element` local con `name="\x7f"`: nombre que no es NCName y D14), muy por debajo del ρ vigente de JSON Schema (≈ 131). Una etiqueta da a lo sumo un error por valor de atributo más uno por la etiqueta (D14 o «sin name ni ref»), no dos en total (5.4-CR1-I1): con `minOccurs` y `maxOccurs` inválidos son 4 errores, pero cada atributo añade más entrada que mensaje y el cociente baja (13,2 medido); a 16 MiB esa variante da 18,7 veces de salida y 46 de memoria; con entradas de 16 MiB, la compilación XSD llega a ≈ 59 veces la entrada de memoria y ≈ 15,5 de salida (`<choice/>` repetido), también por debajo de JSON Schema (≈ 158 y ≈ 93).
