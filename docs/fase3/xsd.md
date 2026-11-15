# Fase 3 — Análisis y bajada del subconjunto XSD a la IR

Tareas 5.3, 5.6 y 5.7 (cierre del frontal XSD). Entrada: el AST de un esquema XSD que dio el parser de la Fase 2 ([gramatica-xsd.md](../fase2/gramatica-xsd.md)). Salida: la IR común de [ir.md](ir.md), **siempre verificada** (`VerifiedIr`, D15), o la lista de los errores del esquema con línea y columna. Es el equivalente XSD de la cadena meta-esquema → tabla de símbolos → ciclos → bajada de JSON Schema ([referencias.md](referencias.md), [bajada.md](bajada.md)).

Implementación: [`crates/xsd/src/symbols.rs`](../../crates/xsd/src/symbols.rs) (tablas de símbolos y tipos predefinidos), [`check.rs`](../../crates/xsd/src/check.rs) (`xsd::analyze`: recorrido, resolución y restricciones), [`simple.rs`](../../crates/xsd/src/simple.rs) (tipos simples, §10), [`upa.rs`](../../crates/xsd/src/upa.rs) (*Unique Particle Attribution* y tope de partículas, §11) y [`lower.rs`](../../crates/xsd/src/lower.rs) (`xsd::lower`). El resumen de la teoría de todo el frontal (lector, parser, análisis y bajada, con su costo) está en §12.

```
bytes ─parse─▶ AST ─analyze─▶ Analysis ─lower─▶ Ir ─verify─▶ VerifiedIr
               (Fase 2)  │                          └─ falla ─▶ InternalError (D15)
                         └─ falla ─▶ Vec<XsdError> (todos los de la primera etapa que falla)
```

**Alcance.** La tarea 5.3 hizo los tipos complejos (con nombre y anónimos), elementos globales y locales, `ref`, grupos `xs:sequence`/`xs:choice`/`xs:all` con ocurrencias, atributos y `xs:simpleContent` + `xs:extension`; la 5.6, los `xs:simpleType` del usuario (con nombre y anónimos), sus facetas, enumeraciones y `default`/`fixed` como valores del tipo, con los números de la 5.5 ([numeros-xsd.md](numeros-xsd.md)), en §10; la 5.7, *Unique Particle Attribution* conservadora, la comprobación de las vueltas con contador y el tope de 4 096 partículas por modelo (D25 y su enmienda, D26), en §11. Queda fuera que enumeraciones, `default` y `fixed` cumplan las `pattern` de la base (D29, tarea `pattern` → AFD). La integración en `schemac` es de la tarea 5.4 (§9).

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

**Por etapas (B2).** Como `semantic::analyze`, el análisis devuelve **todos** los errores de la primera etapa que falla, ordenados por posición (offset): (1) componentes y nombres, (2) tipos simples (§10), (3) modelos de contenido. Las etapas 2 y 3 necesitan los tipos resueltos en la 1, y repetir sus consecuencias confundiría. Las restricciones de la etapa 2 están en la tabla de §10.

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
| 1 | `xs:restriction` con `base` y un `xs:simpleType` hijo a la vez, o sin ninguno (*src-simple-type* 2) | `syntax` | `<` del tipo anónimo; `<` de `xs:restriction` |
| 1 | `base` de `xs:restriction` sin resolver o fuera de §3.2; que nombra un tipo complejo | `unresolved`, `unsupported`; `unresolved` | valor de `base` |
| 2 | Tipos simples: derivación circular, facetas, enumeraciones, `default`/`fixed` | `cycle`, `facet`, `value` | §10 |
| 3 | I11: `minOccurs` > `maxOccurs` (también con `maxOccurs="0"` y `minOccurs` por defecto), comparados tras saturar en `u64::MAX` (ver abajo) | `content-model` | valor de `maxOccurs` si está; si no, de `minOccurs` |
| 3 | I12, *all-group limited* (§3.8.6): `xs:all` con `minOccurs` ∉ {0, 1} o `maxOccurs` ≠ 1; sus elementos con `minOccurs` o `maxOccurs` ∉ {0, 1} | `content-model` | valor del atributo |
| 3 | I12, *Element Declarations Consistent*: dos partículas del mismo modelo con el mismo **nombre local** y distinto tipo (también en grupos anidados y con `ref`); desviación de XSD 1.0, abajo | `content-model` | `<` de la segunda (el mensaje da la primera) |
| 3 | D26: un modelo de contenido con más de 4 096 partículas (elementos y grupos, la raíz incluida), antes de la UPA (§11) | `limit` | `<` de la partícula 4 097 |
| 3 | *Unique Particle Attribution* conservadora (D25, §11): dos partículas con el mismo nombre local que pueden corresponder al mismo elemento | `content-model` | `<` de la segunda (el mensaje da la primera) |

**EDC por nombre local (desviación, 5.3-CR1-I1; D40).** XSD 1.0 (*cos-element-consistent*) compara el nombre **expandido** {name, target namespace}: con `targetNamespace="urn:x"` y `elementFormDefault` por defecto (`unqualified`), un elemento local `<xs:element name="a" type="xs:string"/>` (sin espacio de nombres) y un `<xs:element ref="t:a"/>` a un global `a` de tipo `xs:int` (en `urn:x`) pueden convivir en un mismo modelo. Aquí es `content-model`: la IR nombra elementos y atributos solo por su nombre local, en el único espacio de nombres admitido ([ir.md](ir.md) §5.2, fila `xs:schema`; `00-alcance.md` §1.2), e I12 exige que dos partículas con el mismo nombre local tengan el mismo tipo (el nombre del hijo decide el marco del autómata). Rechazar es lo necesario para no producir una IR que rompa I12; la 5.7 la documenta en `00-alcance.md` §3 ([../fase2/pendientes.md](../fase2/pendientes.md) §17).

**Ocurrencias mayores que 2⁶⁴ (5.3-CR1-S2) — resuelto en la 5.6.** `minOccurs` y `maxOccurs` se saturan en `u64::MAX` (§4, `ir.md` §5.1); si los dos saturan, I11 compara sus lexemas (sin signo ni ceros a la izquierda, por longitud y después lexicográficamente), así que `minOccurs="18446744073709551617" maxOccurs="18446744073709551616"` sí da el error.

Lo que ya rechaza el parser no se repite aquí: `minOccurs`/`maxOccurs` en un elemento global (`parse-error/min-occurs-global.xsd`), `ref` en un elemento global, `ref` con `nillable` o `form` aunque lleven el valor sin efecto de D39 (*src-element* 2.2, `syntax` en el valor del segundo de la pareja; 5.3-CR1-S1, tarea 5.7, `parse-error/ref-con-nillable.xsd`, `ref-con-form.xsd`), `use="prohibited"`, `mixed="true"`, los atributos fuera de la lista blanca y los valores de D39 que cambian la validación ([gramatica-xsd.md](../fase2/gramatica-xsd.md) §4-§5). D14 no lo puede rechazar el parser: depende de tres atributos y del hijo. La «identidad» de un tipo en EDC es la de su declaración: dos tipos anónimos son distintos aunque sean iguales, como en XSD.

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
| `xs:attribute` | `Attribute { name, required, ty, default, fixed }` en `Attributes`; `ty` el nodo `builtin/T`, el de un `xs:simpleType` (§10), o `builtin/anySimpleType` sin tipo; `default`/`fixed` tal como los dejó el lector (la 5.6 comprueba que son valores del tipo, sin reescribirlos) |
| predefinido `xs:T` | Nodo `Text`, padre el documento, paso `Entry("builtin", "T")`: `Lexical` y, en los enteros acotados, `Minimum`/`Maximum` (tabla de [ir.md](ir.md) §5.2) |
| envoltorio de `xs:T` | Nodo `Element` con solo `SimpleContent(builtin/T)`, padre el documento, paso `Entry("simpleContent", "xs:T")`, con la etiqueta fija `xs:` sea cual sea el prefijo del esquema |
| `xs:attribute` global; `attributeFormDefault`, `form`, `nillable`, `abstract` con el valor sin efecto (D39); `xs:annotation`, `id` (D23) | nada |

**Rangos.** Cada declaración lleva el rango de su `xs:complexType` (el documento, el de `xs:schema`); `Content`, el del grupo raíz; `SimpleContent`, el de `xs:simpleContent`; `Attributes`, desde el primer `xs:attribute` hasta el final del último; cada partícula y cada `Attribute`, el de su construcción. Los nodos sintéticos no tienen construcción propia: llevan, en su origen y en sus reglas, el rango de su **primer uso** (el valor de `type` o `base`, o el `xs:attribute` entero si no tiene tipo).

**Orden de los `NodeId` (B3).** (i) El documento, 0. (ii) Las declaraciones en **preorden de declaración** (`Analysis::decls`): las globales (`xs:complexType` y `xs:simpleType` con nombre, tipos anónimos de los elementos globales) en el orden del documento, y cada una seguida, antes que su hermana siguiente, de las que contiene, recursivamente: en un tipo complejo, los tipos anónimos de su modelo de contenido y después los de sus atributos; en un tipo simple, su base anónima; el tipo simple anónimo de un elemento es dos nodos, su envoltorio y su texto (§10). La declaración *i* del preorden es el nodo 1 + *i*. (iii) Los nodos sintéticos, en el orden de su **primer uso** al bajar (i) y (ii) en orden de `NodeId` (en cada tipo complejo: las partículas en preorden, después la base de `xs:simpleContent` y después los atributos; en un tipo simple, su base); el envoltorio de `xs:T` va seguido, si también es su primer uso, de `builtin/T`. Sin `xs:simpleType`, el orden es el de la 5.3 (las doradas no cambian). Cada sintético se crea una sola vez y solo si se usa (como en `pedido_xsd`). Para `pedido.xsd`: `#`, `#/complexType/Pedido`, `#/complexType/Pedido/element/item`, `#/simpleContent/xs:string`, `#/builtin/string`, `#/simpleContent/xs:decimal`, `#/builtin/decimal`, `#/builtin/int`.

El preorden de (ii) se obtiene sin recursión: una pila de declaraciones pendientes se inicializa con los globales del revés; al sacar un tipo se le da el número siguiente, se aplana su modelo y se apilan del revés los tipos anónimos que contiene. Como un tipo se numera al salir de la pila y sus hijos entran encima de sus hermanos, sale exactamente el preorden. El número de un tipo anónimo se anota en la partícula que lo declara al dárselo (la partícula ya existe, en la arena de su padre).

**Aplanado del modelo.** El árbol de grupos de un tipo se aplana en una arena en preorden con otra pila: cada partícula se añade al sacarla y se anota como hija de su grupo (que ya está en la arena, porque salió antes); los hijos se apilan del revés, así que se anotan en orden. Es la forma que exige I11 (el primer hijo de un grupo es la partícula siguiente y cada hijo empieza donde acaba el subárbol del anterior), y la bajada la copia con los mismos índices.

## 5. Corrección

**Proposición.** Si `analyze` devuelve un `Analysis`, `lower` devuelve una IR que pasa `verify` (I1-I14), y su semántica ([ir.md](ir.md) §2) es la del esquema en el subconjunto: un documento cumple el nodo 0 si y solo si es válido contra el esquema.

*Esbozo por invariantes.* I1: el padre de un tipo es el documento o un tipo anterior en el preorden; el de un sintético, el documento. I2: todo destino es 1 + *i* con *i* un tipo existente, o un sintético creado antes de usarlo; la constante colgante `usize::MAX` solo aparece con un `TypeRef` sin resolver, que un `Analysis` sin errores no tiene. I3-I5: los tipos complejos y los envoltorios son `Element` y solo llevan `Content`, `SimpleContent` y `Attributes`, a lo sumo una de cada y nunca `Content` con `SimpleContent` (un `ComplexContent` es de elementos o simple); las partículas apuntan a `Element` (un tipo o un envoltorio) y los atributos y `SimpleContent` a `Text` (un predefinido; un tipo complejo como tipo de atributo o como base es un error de la etapa 1). I6: el documento es `Element` con `Content` y sin más; ninguna partícula apunta a 0 (los tipos empiezan en 1). I7-I9: no hay listas de `allOf`/`type` ni literales; los `Num` son las cotas fijas de los predefinidos, lexemas de RFC 8259. I10: etapa 1 (atributos repetidos). I11: el aplanado es un preorden por construcción, y `minOccurs` ≤ `maxOccurs` es de la etapa 3. I12: etapa 3 (`xs:all` solo puede ser raíz por la gramática). I4 e I13 en los tipos simples, y I14 (los `Ref` entre textos), en §10. I13: los predefinidos tienen exactamente un `Lexical`, sin facetas que no apliquen; los atributos, sin `default` con `fixed` ni requeridos con `default` (etapa 1). I14: la única arista sobre la misma instancia es el `Ref` de un tipo simple a su base, y la etapa 2 rechaza los ciclos. La semántica coincide con XSD por la correspondencia de §4 y [ir.md](ir.md) §5.2: el documento es la elección de los globales; cada partícula valida su elemento con el tipo resuelto, y un `ref` con el de su declaración global.

La prueba `corpus_entero_sin_error_interno` comprueba la primera parte sobre todo el corpus XSD: cada esquema da una IR verificada o un error del usuario, nunca el error interno.

## 6. Costo y pila

Sea *n* el tamaño del AST (construcciones, nombres y valores).

- **Recorrido** (preorden y aplanado): cada tipo entra y sale una vez de la pila de tipos, cada grupo y cada elemento una vez de la pila de su modelo. O(n).
- **Tablas**: una inserción por declaración global y una consulta por `type`/`ref`/`base`, O(1) esperado más el hash del nombre local (§2). La prueba `uri_de_8_mib_y_medio_millon_de_referencias` usa un URI de 8 MiB como `targetNamespace` y como prefijo de 5·10⁵ referencias `type='p:T'` (desde la 5.7, repartidas en 125 tipos de 4 000 partículas, por el tope de D26): con una clave por texto serían ~4·10¹² bytes hasheados; comprueba el resultado (127 nodos y 125 + 5·10⁵ aristas), no el tiempo (D1).
- **Comprobaciones**: una pasada por los atributos de cada tipo (tabla de nombres para I10) y otra por su arena (ocurrencias, `xs:all`, tabla de nombres para EDC). La UPA es la única parte que no es lineal: O(*n*·*m*) por modelo, acotada por el tope de D26 (§11). Los errores se ordenan al final: O(*k* log *k*) con *k* errores.
- **Bajada**: un nodo por tipo complejo, una partícula por partícula y un atributo por atributo, más a lo sumo 13 predefinidos y 12 envoltorios. **Cada uso de un tipo es una arista, nunca una copia** (`tipo_usado_k_veces_es_una_arista_por_uso`: con 10 y 4 000 usos —10⁴ hasta la 5.7, que no caben en el tope de D26— el número de nodos es el mismo y las aristas crecen en uno por uso). Después, `verify`, O(n) sobre una IR válida ([ir.md](ir.md) §8).

**Sin recursión (D7).** Análisis y bajada usan solo las dos pilas explícitas de §4 y bucles. El AST sí tiene `Clone`, `Debug`, `PartialEq` y `Drop` recursivos (acotados por la profundidad de 256, D36), así que quien tiene el AST lo libera en un hilo con `stack_size` (D7, [../fase2/pendientes.md](../fase2/pendientes.md) §5). La prueba `analisis_y_bajada_sin_recursion` parsea la cadena más profunda (`element`/`complexType`/`sequence`, 84 tipos anónimos anidados) en un hilo de 2 MiB, la analiza y la baja en otro de 64 KiB y la libera de vuelta en el primero.

## 7. Pruebas

`crates/xsd/tests/lower.rs`:

- `pedido_baja_igual_que_la_ir_hecha_a_mano`: `parse-ok/pedido.xsd` da la IR de `pedido_xsd` (copia de `crates/ir/tests/forma.rs`) comparada nodo a nodo por ubicación (reglas, destinos y padre como ubicaciones, rangos aparte), con el orden de nodos de B3 y los rangos de declaraciones, reglas y del primer uso de `xs:int`.
- `esquemas_de_los_casos_bajan` (los dos XSD de `tests/cases`, con el tipo anónimo del `pedido` global colgando del documento), `corpus_entero_sin_error_interno` (todo `parse-ok`, `parse-error`, `semantic-error` y `tests/cases`) y `errores_semanticos_con_linea_y_columna` (manifiesto `tests/schemas/errores-semanticos-xsd.json`: código, línea, columna, texto del mensaje y, con `count`, el número de errores de la etapa; 78 casos en `tests/schemas/xsd/semantic-error` (12 de la 5.7: UPA, vueltas con contador y EDC por `ref` y con anónimos), al menos uno por fila de §3 y de §10).
- `correspondencia_de_cada_construccion` (ocurrencias saturadas, `ref` compartiendo el tipo —también el mismo `ref` dos veces en un modelo, 5.3-CR1-I2—, `xs:anySimpleType`, contenido vacío y `Choice([])`, `targetNamespace`, atributos globales sin bajar), `orden_de_nodos_b3`, `d39_no_llega_a_la_ir`, `errores_por_etapas_en_orden_del_documento`, `errores_con_nombres_largos_se_recortan`, `tipo_usado_k_veces_es_una_arista_por_uso`, `uri_de_8_mib_y_medio_millon_de_referencias` y `analisis_y_bajada_sin_recursion` (§6).

Las de la UPA (5.7), en §11.5. `crates/xsd/src/check.rs`: `ocurrencias_como_entero_no_negativo_saturado` y `ocurrencias_saturadas_se_comparan_por_lexema` (5.6, 5.3-CR1-S2; también en `errores_por_etapas_en_orden_del_documento`). Las pruebas de los tipos simples (5.6) están en §10.5.

## 8. Decisiones y pendientes

Las decisiones de esta tarea están registradas como **D40** en el plan general (y los valores por defecto sin efecto del parser, como **D39**):

- API `xsd::analyze`/`xsd::lower`; `Analysis` con campos privados es la única entrada de `lower` (para XSD resuelve el `AnalyzedTable` de [pendientes.md](pendientes.md) §13); un tipo sin resolver da una arista colgante (error interno), nunca pánico (§1).
- B2: dos etapas (tres desde D44, §10.6), componentes y nombres; modelos de contenido; todos los errores de la primera que falla, por offset; los códigos y las posiciones de §3.
- B3: documento, tipos complejos en preorden de declaración y sintéticos por primer uso (envoltorio antes que su predefinido), con el rango de su primer uso; el tipo anónimo de un global cuelga del documento (§4).
- Resolución por `(NsId, nombre local)` con `RandomState` (D31): primero el esquema, después §3.2; `xs:anySimpleType` no se puede nombrar; `xs:anyType`, fecha y hora y el resto de XSD 1.0 son `unsupported` (§2).
- Los atributos globales se comprueban y no se bajan (§2); contenido vacío según XSD §3.4.2, cláusulas 2.1.1-2.1.3, y documento `Choice([])` sin globales (§4).
- D39 en el parser: lista blanca solo con el valor sin efecto ([gramatica-xsd.md](../fase2/gramatica-xsd.md) §5).
- EDC por nombre local, desviación de XSD 1.0 (§3).

Las de la 5.6 (tipos simples), en §10.6; las de la 5.7, en §11.6.

Abiertos: [pendientes.md](pendientes.md) §33 (lo que queda de la revisión de la 5.3), §36 (comparadores IEEE de la Fase 4), §37 (lo que deja la 5.6) y §38 (lo que deja la 5.7: `pattern` en enumeraciones y `default`/`fixed`, Glushkov con contadores); [../fase2/pendientes.md](../fase2/pendientes.md) §12 (memoria del AST, Fase 7). §3 queda cerrado con la 5.7.

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
- **Clasificación**: la única regla (c) del subconjunto XSD es la faceta `pattern` (`00-alcance.md` §3.1), que sube a (c) su texto y los que llegan a él por `Ref` (5.6); un nodo es (b) si tiene `attributes`, facetas de longitud o un `content` con `xs:all` o con contador; si no, (a). El pedido de `tests/cases` (`valid-xml-pedido-01`) tiene un solo nodo (b), el tipo anónimo de `pedido`, por el `attributes` de `id`.

**Doradas y DOT.** [`tests/ir/xsd`](../../tests/ir/xsd) tiene las IR doradas de `pedido`, `valid-xml-pedido-01`, `espacio-por-defecto` (este, con `targetNamespace`) y, desde la 5.6, `tipos-simples` (`Lexical`, `Ref` y las facetas reescritas), de los XSD de `tests/schemas/xsd/parse-ok` con el mismo nombre; `make doradas` las regenera junto a las de JSON. Van en un subdirectorio porque `ir_doradas` (`crates/semantic/tests/lower.rs`) recorre los `.json` de `tests/ir` buscando su fuente en `tests/schemas/parse-ok`. [ir-pedido.dot](ir-pedido.dot) y [ir-pedido.svg](ir-pedido.svg) son la IR de `pedido.xsd` (variable `IR_DOT_XSD` del `Makefile`; `make docs-check` comprueba el `.dot`; descrito en [bajada.md](bajada.md) §5.2).

**Pruebas** (además de las de la CLI, [bajada.md](bajada.md) §7): `crates/xsd/tests/corpus.rs`, `corpus_clasificado` (**todo** XSD de `parse-ok`, también los tres con `xs:simpleType` que daban «pendiente» hasta la 5.6 —un error del usuario hace fallar la prueba, 5.4-CR1-S1—, y los dos casos de `tests/cases` con el texto de su campo `schema`, que es byte a byte su copia en `parse-ok`, cumplen la definición de la clasificación nodo a nodo y solo tienen nodos (c) por `pattern`: `anotaciones` y `tipos-simples`; el pedido, un (b) por `Attributes`; `malformed-xml-billion-laughs-01`, ninguno) e `ir_doradas_xsd` (desde la 5.6 también la dorada `tipos-simples`).

**Amplificación** ([pendientes.md](pendientes.md) §1): la peor clase de error XSD medida da ≈ 21 veces la entrada con una ruta de 8 bytes (un `xs:element` local con `name="\x7f"`: nombre que no es NCName y D14), muy por debajo del ρ vigente de JSON Schema (≈ 131). Una etiqueta da a lo sumo un error por valor de atributo más uno por la etiqueta (D14 o «sin name ni ref»), no dos en total (5.4-CR1-I1): con `minOccurs` y `maxOccurs` inválidos son 4 errores, pero cada atributo añade más entrada que mensaje y el cociente baja (13,2 medido); a 16 MiB esa variante da 18,7 veces de salida y 46 de memoria; con entradas de 16 MiB, la compilación XSD llega a ≈ 59 veces la entrada de memoria y ≈ 15,5 de salida (`<choice/>` repetido), también por debajo de JSON Schema (≈ 158 y ≈ 93).

## 10. Tipos simples del usuario (tarea 5.6)

Implementación: [`crates/xsd/src/simple.rs`](../../crates/xsd/src/simple.rs) (etapa 2 del análisis), el recorrido y la resolución de las bases en [`check.rs`](../../crates/xsd/src/check.rs) (etapa 1) y la bajada en [`lower.rs`](../../crates/xsd/src/lower.rs) (`simple_node`). Los números pasan por `ir::num` ([numeros-xsd.md](numeros-xsd.md); 5.5-P1, 5.5-P2).

### 10.1 Derivación

Un `xs:simpleType` es siempre una restricción (`xs:list` y `xs:union` están fuera, §3.3 del alcance). Su **base** es `base="B"` (resuelto como un `type`, §2: primero el esquema, después §3.2) o un `xs:simpleType` hijo (base anónima), exactamente uno de los dos (*src-simple-type* 2, etapa 1); un tipo complejo no puede ser base (`unresolved`). Cada tipo tiene a lo sumo una base, así que las bases forman un **grafo funcional** G sobre los tipos simples del usuario, con salida a lo sumo 1 y hojas los predefinidos.

**Derivación circular** (*st-props-correct* 2, etapa 2, `cycle`). El `Lexical` de un tipo es el de su base, así que fijarlo exige seguir la cadena de bases, que solo termina si G no tiene ciclos. Por eso los ciclos se buscan **antes** de seguirla (D15, `ir.md` §4): cada tipo tiene un estado (sin ver, en el camino, hecho); desde cada tipo sin ver se avanza por su base marcando «en el camino» y guardando el camino en una pila explícita; si se llega a un tipo «en el camino», los tipos del camino desde él forman un ciclo; al terminar, todo el camino pasa a «hecho». Como la salida es ≤ 1, cada tipo entra una vez en un camino: **O(n)** sin recursión (D7). Cada ciclo da **un** error, en el primer valor de `base` (por posición) de sus miembros: todo ciclo tiene uno, porque las bases anónimas son aristas de padre a hijo en el árbol del AST y un ciclo de solo esas aristas es imposible. Los tipos que derivan de un ciclo sin estar en él no dan otro error (serían consecuencias). La prueba `cadena_de_cien_mil_restricciones_con_ciclo_al_final` analiza 10⁵ restricciones con un ciclo de 10 al final en un hilo de 64 KiB.

**Facetas efectivas.** Sin ciclos, los tipos se procesan en **orden topológico** de G (la base antes que el derivado: se sube por la cadena de cada tipo sin calcular hasta uno calculado y se calculan al bajar, otra vez O(n)). Las facetas efectivas de un tipo son las de su base con las propias del paso encima: una cota inferior y una superior (cada una inclusiva o exclusiva), `length`, `minLength`, `maxLength` y la enumeración que rige. Son de **tamaño fijo**: cada cota y la enumeración **apuntan al tipo que las declara** (o al predefinido acotado), no se copian (`cota_larga_heredada_por_una_cadena`). Y cada cota se **analiza una sola vez**, al declararla (o, en los predefinidos acotados, al empezar la etapa): se guarda como `ir::num::Decimal` (signo, cifras significativas y exponente) y se compara con `compare_parsed`, que cuesta O(min(|valor|, |cota|)) sin volver a recorrer la cota (5.6-CR-I1). Así *k* valores cortos (enumeraciones de un derivado, `default`/`fixed` de *k* atributos, o *k* pasos con su propia cota del otro lado) contra una cota de 1 MiB cuestan O(*k* + 1 MiB), no O(*k* · 1 MiB) (`cota_larga_se_analiza_una_sola_vez`, que lo comprueba contando los bytes analizados, sin reloj). Basta una cota por lado porque la de un paso, si pasa *valid restriction* (abajo), nunca es más floja que la heredada. Un tipo con algún error queda **envenenado**: sus derivados y los atributos que lo usan no se comprueban (`errores_sin_consecuencias_y_en_orden`).

### 10.2 Comprobaciones de la etapa 2

| Restricción | `code()` | Posición |
|---|---|---|
| Derivación circular de `base`, directa o indirecta | `cycle` | primer valor de `base` del ciclo |
| I13: faceta que no se aplica al primitivo (longitud fuera de `xs:string`, rango fuera de los numéricos, en `xs:boolean` solo `pattern`; XSD 1.0 Parte 2 §4.1.5) | `facet` | `<` de la faceta |
| I4: faceta repetida en un paso (salvo `xs:enumeration` y `xs:pattern`, que se acumulan) | `facet` | `<` de la segunda (el mensaje da la primera) |
| I4: `length` con `minLength` o `maxLength` en el mismo paso; `min*` (o `max*`) inclusivo y exclusivo en el mismo paso | `facet` | `<` de la segunda |
| Valor de `length`/`minLength`/`maxLength` que no es `xs:nonNegativeInteger` (saturado en `u64::MAX`, como las ocurrencias) | `value` | valor |
| Valor de una faceta de rango que no tiene la forma léxica del primitivo, o `INF`/`NaN`/desborde en `xs:float`/`xs:double` (D24): cada `NumErrorKind` de `ir::num` salvo `NotNumeric`, que el frontal no produce | `value` | valor |
| *Facet valid restriction* respecto de las efectivas de la base, también los rangos de los predefinidos (`minInclusive="300"` sobre `xs:byte`) | `facet` | valor de la faceta nueva |
| Coherencia del mismo paso: `minInclusive` ≤ `maxInclusive` (con alguna exclusiva, `<`), `minLength` ≤ `maxLength` | `facet` | valor de la segunda |
| `xs:enumeration` que no es un valor de la **base**: forma léxica, cotas, longitudes y enumeración de la base (`1.0` en `xs:integer`, `300` en `xs:byte`) | `value` | valor |
| `default`/`fixed` que no es un valor del tipo del atributo (el mismo criterio, con las facetas del propio tipo) | `value` | valor |

**Valid restriction de las cotas** (Parte 2 §4.3.7-§4.3.10, reglas 1-4 de cada faceta). Sea (l, l̂) la cota inferior efectiva de la base y (u, û) la superior (^ = exclusiva). Una cota **inclusiva** nueva v debe ser un valor del espacio de la base por los dos lados: v ≥ l (v > l si l̂) y v ≤ u (v < u si û). Una **exclusiva** nueva no afloja la de su lado (`minExclusive` v ≥ l, `maxExclusive` v ≤ u, sea la de la base inclusiva o exclusiva) y queda estrictamente dentro por el otro (`minExclusive` v < u, `maxExclusive` v > l). Las del mismo paso: `min` ≤ `max` si las dos son inclusivas y `<` si alguna es exclusiva. En cada caso el código es igual de estricto que XSD 1.0 (2.ª ed.) o más, y lo único que rechaza de más son espacios de valores vacíos. **Las dos únicas desviaciones** (lecturas conservadoras, decisión 5): (i) un `minExclusive` nuevo frente al `maxInclusive` de la base, donde la regla 2 de *minExclusive valid restriction* admite v ≤ u y aquí se exige v < u; (ii) `minExclusive` y `maxExclusive` en el mismo paso, donde *minExclusive <= maxExclusive* admite la igualdad y aquí se exige `<`. Lo demás es literal de XSD: `minExclusive` < `maxExclusive` de la base (regla 4), *minInclusive <= maxInclusive*, *minInclusive < maxExclusive* y *minExclusive < maxInclusive* en el mismo paso. Los valores se comparan con `ir::num::compare_parsed` sobre las formas analizadas (el mismo orden que `compare`), exacta en `xs:integer`/`xs:decimal` y válida también en `xs:float`/`xs:double` porque el decimal más corto que guarda `xsd_number` es **monótono** (5.5-CR-S2, prueba `decimal_mas_corto_es_monotono` de `crates/ir/tests/num.rs`). El valor de una faceta de rango tiene que tener la forma léxica del primitivo, pero no se le exige estar en la enumeración de la base (XSD 1.0 dice «en el espacio de valores de la base», lo que con `minExclusive` igual al de la base contradice la regla 1: se toma la lectura que no se contradice).

**Valid restriction de las longitudes** (§4.3.1-§4.3.3). `length` = n: igual al `length` de la base, ≥ su `minLength`, ≤ su `maxLength`. `minLength` y `maxLength`: no aflojan los de la base (≥ y ≤) y quedan entre los de la base (`minLength` ≤ `maxLength` heredado y al revés). Si la base tiene `length`, un `minLength`/`maxLength` nuevo solo se admite si repite el `minLength`/`maxLength` **efectivo** de la base (no su `length`): son las cláusulas 1.2 (`minLength`) y 2.2 (`maxLength`) de *length and minLength or maxLength* (2.ª ed., §4.3.1.4), que piden un antecesor con ese mismo `minLength` (o `maxLength`) y sin `length`.

**Valores de un tipo** (`in_space`). En `xs:string` (*whiteSpace* `preserve`) el valor es el texto tal como lo deja el lector, y su longitud se cuenta en puntos de código; en los demás, el *collapse* y la forma léxica los hace `ir::num` (`xsd_number`, `parse_boolean`). Después: las cotas efectivas, las longitudes y la pertenencia a la enumeración que rige, comparando valores normalizados (la cadena; el lexema de `xsd_number`, canónico en `xs:integer`/`xs:decimal` e igual para dos escrituras del mismo binary32/64), con una tabla hash por tipo con enumeración. `pattern` no se comprueba (D29: queda para la tarea `pattern` → AFD). En `default`/`fixed` de `xs:float`/`xs:double`, `INF`, `-INF` y `NaN` **son** valores del tipo (D24 solo rige las facetas y las enumeraciones, que bajan a `Num`): NaN queda fuera de cualquier cota y de cualquier enumeración, `INF` fuera de una superior y `-INF` de una inferior (XSD 1.0 2.ª ed. §3.2.4); `+INF` (de XSD 1.1) es un error de forma léxica; un desborde (`1e39` en `xs:float`) se rechaza con el motivo de D24.

### 10.3 Bajada

| AST | IR |
|---|---|
| `xs:simpleType name="T"` | Nodo `Text`, padre el documento, paso `Entry("simpleType", "T")` |
| tipo anónimo del elemento `e` | Dos declaraciones seguidas: el envoltorio `Element` (padre el tipo complejo que contiene `e`, o el documento si `e` es global; paso `Entry("element", "e")`; solo `SimpleContent(texto)`) y el texto (padre el envoltorio, paso `Keyword("simpleType")`) |
| tipo anónimo del atributo `a` del tipo complejo C | Nodo `Text`, padre C, paso `Entry("attribute", "a")` |
| base anónima de la restricción del tipo S | Nodo `Text`, padre S, paso `Keyword("restriction")` |
| tipo anónimo de un atributo **global** | nada (se comprueba, como el atributo, pero no se baja: §2) |
| reglas de un texto | `Lexical(primitivo)`; `Ref(base)` salvo que la base sea un predefinido que solo tiene `Lexical` (`string`, `boolean`, `decimal`, `integer`, `float`, `double`); las facetas propias en orden fijo: `Enum`, cota inferior (`Minimum`/`ExclusiveMinimum`), cota superior, `MinLength`, `MaxLength`, `Pattern`; `length` = n da `MinLength(n)` y `MaxLength(n)` |
| valores | Las cotas, el `Num` de `xsd_number` (`+10` → `10`, `100.0` → `100`, en `xs:float` el decimal más corto); las enumeraciones, `LitToken::String` (el texto sin tocar) o `LitToken::Number` (el lexema de `xsd_number`) |
| varias `xs:pattern` de un paso | Un solo `Pattern`, `(p1)|(p2)|…` (una sola, tal cual) |
| `type="T"` de un elemento, con T un `xs:simpleType` con nombre | El envoltorio sintético `simpleContent/T` (sin `xs:`), padre el documento, creado en el primer uso con su rango y compartido por todos los usos |
| `type="T"` de un atributo, `base="T"` de `xs:simpleContent` o de otra restricción | Arista al texto de T |

**Rangos.** Cada texto y cada envoltorio de un anónimo, el de su `xs:simpleType`; `Lexical`, el de `xs:restriction`; `Ref`, el del valor de `base` (o el del `xs:simpleType` anónimo); cada faceta, el de su elemento; `Enum` y `Pattern`, desde la primera hasta el final de la última. Un predefinido al que apunta un `Ref` lleva, si es su primer uso, el rango de ese `base`.

**Corrección** (continúa §5). I3, I5: los textos solo llevan reglas admitidas en `Text` y sus `Ref` van a textos; los envoltorios solo `SimpleContent` a un texto. I4: a lo sumo una regla de cada clase, porque la etapa 2 rechaza las facetas repetidas, `length` con `minLength`/`maxLength` y dos cotas del mismo lado, y todas las enumeraciones y todas las `pattern` de un paso dan una sola regla. I8: los `Num` salen de `xsd_number`. I13: cada texto tiene exactamente un `Lexical`, el de su base (el `Ref` conserva el espacio léxico); las facetas aplican al primitivo y las enumeraciones son escalares del tipo (enteras en `Integer`, por la forma léxica de `xs:integer`), ninguna en `xs:boolean`. I14: los `Ref` siguen las aristas de G, que la etapa 2 garantiza acíclico. La semántica es la de XSD: un texto cumple su tipo si cumple las facetas del paso y, por el `Ref`, las de toda la cadena (la restricción es una conjunción), y varias `pattern` de un paso son una disyunción.

### 10.4 Costo

Recorrido: cada `xs:simpleType` entra una vez en la pila de declaraciones. Etapa 1: una consulta de tabla por `base`. Etapa 2: O(n) para los ciclos y el orden topológico; cada faceta y cada `default`/`fixed` se comprueba una vez contra a lo sumo dos cotas efectivas, tres longitudes y una tabla hash, con el costo de `ir::num` por valor: la forma léxica y el análisis del valor, O(|valor|), y cada comparación con una cota ya analizada, O(min(|valor|, |cota|)) ≤ O(|valor|) ([numeros-xsd.md](numeros-xsd.md) §5); cada cota se analiza una vez, al declararla, O(|cota|). Así la suma sobre todos los valores es O(n): ningún valor paga la longitud de una cota ajena (5.6-CR-I1). Las tablas de enumeraciones, O(total de enumeraciones). Bajada: un nodo por declaración y una regla por faceta (dos por `length`), más un envoltorio sintético por tipo con nombre usado por elementos. En total **O(n)** más el costo de `ir::num` por valor, sin recursión.

### 10.5 Pruebas

`crates/xsd/tests/simple.rs`: `tipos_simples_baja_con_las_ubicaciones_de_5_2` (IR de `parse-ok/tipos-simples.xsd`: con nombre y anónimos, una cadena de 3 restricciones, enumeraciones, rangos, longitudes y dos `pattern`; orden de nodos, reglas, destinos y rangos), `cadena_de_cien_mil_restricciones_con_ciclo_al_final`, `cota_larga_heredada_por_una_cadena`, `cota_larga_se_analiza_una_sola_vez` (5.6-CR-I1: una cota de 10⁶ cifras frente a 1000 enumeraciones, 1000 `default` y una cadena de 1000 pasos; el contador `ir::num::contador`, *feature* `contadores` solo en las pruebas, da ≈ 1,0·10⁶ bytes analizados por forma, y la versión cuadrática ≈ 1,0·10⁹), `restriccion_valida_respecto_de_la_base` (tabla de aceptadas y rechazadas), `enumeraciones_y_valores_por_defecto_son_del_tipo`, `errores_sin_consecuencias_y_en_orden`, `errores_de_tipos_simples_se_recortan`, `origenes_de_los_tipos_anonimos` y `tipo_de_atributo_global_se_comprueba_y_no_se_baja`. El corpus `semantic-error` tiene un caso con línea y columna por fila de §10.2 (y uno por cada `NumErrorKind` que puede salir). `crates/ir/tests/num.rs`: `decimal_mas_corto_es_monotono` (5.5-CR-S2) y `comparacion_analizada_coincide` (`compare_parsed` da el orden de `compare`). Dorada `tests/ir/xsd/tipos-simples.json`.

### 10.6 Decisiones

Candidatas al registro del plan (tomadas por el Builder de la 5.6; las conservadoras, por delegación del orquestador):

1. Tres etapas (B2): componentes y nombres, **tipos simples**, modelos de contenido; códigos nuevos `cycle`, `facet` y `value` (este ya existía) con las posiciones de §10.2.
2. Un error por ciclo, en el primer `base` del ciclo por posición; los derivados de un ciclo o de un tipo con errores no se comprueban.
3. Los `xs:simpleType` entran en el preorden de declaraciones de B3 (un anónimo de elemento son dos nodos: envoltorio y texto); sin `xs:simpleType` el orden no cambia.
4. `Ref` solo si la base no es un predefinido que se reduce a `Lexical`; facetas en orden fijo; varias `pattern` como `(p1)|(p2)`.
5. Dos lecturas conservadoras de XSD 1.0 (2.ª ed.), que solo rechazan espacios de valores vacíos que XSD admite: un `minExclusive` nuevo **<** el `maxInclusive` de la base (la regla 2 admite ≤) y `minExclusive` **<** `maxExclusive` en el mismo paso (XSD admite ≤). Lo demás de §10.2 es literal de XSD, también `minLength`/`maxLength` sobre una base con `length` (solo si repiten el `minLength`/`maxLength` efectivo de la base, cláusulas 1.2 y 2.2).
6. El valor de una faceta de rango no tiene que estar en la enumeración de la base (sí en su forma léxica y dentro de sus cotas, por *valid restriction*).
7. `default`/`fixed` se comprueban como valores del tipo y bajan sin tocar; en `xs:float`/`xs:double` admiten `INF`, `-INF` y `NaN`, y rechazan `+INF` y los desbordes.
8. `ErrorKind::Pending` desaparece; `NotSimpleType` gana el campo `context`.
9. Cada cota se analiza una sola vez (5.6-CR-I1): API aditiva de `ir::num`, `Decimal::new` y `compare_parsed`, y el contador de pruebas `ir::num::contador` tras la *feature* `contadores` (no cambia la IR, el JSON v2 ni ningún resultado).

## 11. *Unique Particle Attribution* y tope de partículas (tarea 5.7)

Implementación: [`crates/xsd/src/upa.rs`](../../crates/xsd/src/upa.rs), llamada desde la etapa 3 de [`check.rs`](../../crates/xsd/src/check.rs) **solo si las etapas 1 y 2 pasaron** (sus errores no se informarían, y la UPA necesita los nombres de los `ref` resueltos). Decisiones del plan: D25 (UPA conservadora) y D26 (tope de partículas).

### 11.1 La restricción

XSD 1.0 (Parte 1, §3.8.6, *Unique Particle Attribution*) exige que, al validar un contenido de izquierda a derecha, cada elemento hijo se pueda atribuir a **una sola** partícula del modelo sin mirar más allá. En términos de autómatas: el autómata de Glushkov del modelo (una posición por partícula de elemento, transiciones etiquetadas con su nombre) es **determinista**. Es lo que necesita la Fase 4, que compila cada modelo de contenido a su Glushkov ([ir.md](ir.md) §7, fila Q): una sola transición por (estado, nombre). `verify` no lo comprueba ([ir.md](ir.md) §4, «Lo que no es invariante»): es una *Schema Component Constraint*, un error del esquema del usuario, y lo da el frontal con su posición.

### 11.2 Abstracción conservadora (D25)

Las ocurrencias con contador (`a{2,5}`) hacen que el determinismo exacto dependa de los valores de los contadores (Kilpeläinen y Tuhkanen; XSD 1.0 admite `a{2,2}, a`). D25 lo evita comprobándolo sobre una **abstracción** del modelo como expresión regular sobre nombres locales:

| Partícula | En la abstracción |
|---|---|
| `minOccurs="0"` | `?` (anulable) |
| `maxOccurs` > 1 o `unbounded` | `+` (o `*` si además es anulable): la partícula se repite |
| `maxOccurs="0"` | `?`: la partícula sigue en el modelo |
| `xs:choice` sin hijos | ε (anulable) |
| `xs:all` (siempre raíz, I12) | tras cualquier hijo puede venir cualquier otro; anulable si lo son todos |
| nombre | el **local** (como EDC, D40, y como la IR, `00-alcance.md` §1.2) |

**Proposición.** Si la abstracción es determinista, el **autómata de posiciones** (Glushkov) del modelo exacto también lo es, lea como lea la Fase 4 `maxOccurs="0"` y el `xs:choice` vacío: cada elemento del contenido se atribuye a una sola partícula, que es lo que pide XSD 1.0. *Esbozo:* cada fila solo **agranda** las relaciones del Glushkov: una partícula repetida añade *first*(*p*) a *follow*(*x*) para las últimas posiciones *x* de *p* (con contadores, esa arista existe en algún valor del contador o no existe, nunca hay una que la abstracción no tenga); «anulable» solo añade caminos; ε en lugar del lenguaje vacío y `?` en lugar de «ausente» añaden posiciones alcanzables y anulabilidad. Las posiciones son las mismas, así que *first*(raíz) y cada *follow*(*x*) del modelo exacto están contenidos en los de la abstracción, y un conflicto en el exacto (dos posiciones del mismo nombre en un mismo conjunto) lo es también en la abstracción. Comparar por nombre local en lugar de expandido solo identifica más nombres: más conflictos, no menos. ∎

El precio es rechazar modelos que XSD 1.0 admite: `a{2,2}, a` (`semantic-error/upa-contador.xsd`), `a{0,0}, a` (XSD 1.0 trata una partícula con `maxOccurs="0"` como si no existiera; aquí cuenta como `?`) o `a*, <choice/>, a`. Es la desviación 1 de `00-alcance.md` §3.4.

**Vueltas con contador (enmienda de D25; I-1 de la revisión de la 5.7).** Que el autómata de posiciones sea determinista (determinismo *débil*) no basta cuando la Fase 4 lleva un contador por partícula en el marco (D16; [ir.md](ir.md) §7, fila Γ): cada transición tiene que decir también **qué contadores** cambia (determinismo *fuerte*; Kilpeläinen y Tuhkanen; Gelade, Gyssens y Martens). En `(a{1,2}){2,2}` (lenguaje a{2,4}, que XSD 1.0 admite), tras la primera `a`, la segunda puede seguir en la misma vuelta del grupo (contador de `a` a 2) o empezar la siguiente (contador del grupo a 2, el de `a` a 1): la transición `a → a` es una sola en el autómata de posiciones y dos distintas sobre los contadores, y ninguna regla fija acierta siempre. Igual en `(a{1,2}, b?)*`. **Regla (conservadora):** al enumerar *follow*(*x*), cada nodo guarda el nivel de la subida (§11.3) que lo añadió; si vuelve a entrar desde un nivel más alto *j* (por la vuelta del antecesor de ese nivel) habiendo entrado desde *i* < *j*, y alguna partícula de la subida entre los niveles *i* y *j*, ambos incluidos, tiene contador (`minOccurs` ≥ 2 o `maxOccurs` acotado > 1; la regla de `ir::classify`), los dos caminos actualizan distinto ese contador: error `content-model` (`ErrorKind::AmbiguousCounter`) en el `<` del nodo que vuelve a entrar (un elemento, o un grupo si entra entero), con dos posiciones en el mensaje: la partícula que se repite (la del nivel *j*) y la del contador (la más alta de la subida entre *i* y *j*; puede ser la misma, o el propio nodo). Cada nodo se informa a lo sumo una vez. Por simplicidad la regla mira los niveles *i*..*j* enteros: si el nodo entró en el nivel *i* como hermana (no por la vuelta), la partícula de *i* se sale en los dos caminos y su contador no los distingue, así que puede salir un **segundo error con la causa mal atribuida** (en `(a{0,2}, b?)*`, el de `a` es real y el de `b` culpa al contador de `a`); el modelo se rechaza igual, sin rechazos de más ([pendientes.md](pendientes.md) §38, 5.7-CR2-S1). Un grupo sin posiciones en su *first* (`<xs:sequence/>`) que vuelve a entrar no cuenta: no hay transición que actualizar. *Por qué basta:* el camino desde el nivel *i* sale de las partículas de la subida por debajo de *i* y repite la de *i*; el del nivel *j* sale además de las de *i* a *j* − 1 y repite la de *j*. Los dos tocan igual los contadores por debajo de *i* (salen de ellas) y por dentro del nodo (entran de nuevo); solo difieren en las partículas de *i* a *j*. Si ninguna tiene contador, las dos transiciones son la misma (las partículas sin contador las lleva el estado del Glushkov), y con una sola forma de llegar a cada posición desde *x*, cada transición fija sus contadores. Así, lo que pasa la UPA y esta regla, la Fase 4 lo construye determinista también sobre los contadores, sin determinizar ([pendientes.md](pendientes.md) §38). Es conservadora: rechaza también `((a, b){2,2})*`, donde los valores del contador sí deciden (con la vuelta interior a 2 hay que salir), y `(a+){2,3}`. No cambia nada sin contador por medio (`(a*)*`, `((a+)+)*`) ni sin una vuelta que entre dos veces (`(a{2,3}, b)*`, `(a, b{2,5})*`): prueba `contadores_sin_vuelta_ambigua_pasan`.

### 11.3 Algoritmo

El modelo ya está aplanado en una arena en preorden (§4), con los hijos de cada grupo; en una pasada de atrás hacia delante se calcula si cada partícula es **anulable** (los hijos van después de su grupo), y en otra el padre y la posición de cada partícula entre sus hermanas. El modelo es determinista si ni *first*(raíz) ni *follow*(*x*), para cada posición *x*, contienen dos posiciones distintas con el mismo nombre. Los conjuntos **no se guardan**: se enumeran uno a uno y se comparan con una tabla por nombre (nombre → marca del conjunto en curso y primera posición con ese nombre), que se reutiliza sin vaciarla cambiando la marca.

- *first*(*v*): pila explícita desde *v*; un elemento es su propia posición; una secuencia aporta sus hijos hasta el primero no anulable (incluido); `xs:choice` y `xs:all`, todos sus hijos. Cada nodo lleva la marca del conjunto en el que ya entró su *first*, así que no se recorre dos veces en un mismo conjunto.
- *follow*(*x*): se sube desde *x* mientras *x* sea una de las últimas posiciones del subárbol actual *c*: si *c* se repite, *first*(*c*); si el padre es una secuencia, *first* de cada hermana siguiente hasta la primera no anulable (si la hay, *x* no es última del padre y se para); si es un `xs:all`, *first* de las demás hijas; si es un `xs:choice`, nada. Es la definición de *follow* de Glushkov por casos (concatenación, unión, clausura), recorrida de abajo arriba.

**Error.** Un conflicto es una pareja (*p*, *q*) del mismo nombre en un conjunto; se informa en el `<` de la que va **después** en el documento (*q*, el mayor índice del preorden), con la posición de la primera en el mensaje (`ErrorKind::Ambiguous`, código `content-model`, como EDC). Cada partícula se informa **a lo sumo una vez**, así que un modelo da como mucho una línea por partícula: `(a | a | a)` da dos, y un `xs:choice` de 1 000 `a`, 999 (prueba `un_error_por_particula_en_orden`). La tabla por nombre guarda la primera posición en orden del documento, así que la pareja informada es la más temprana conocida. **A propósito**, un conflicto entre dos partículas del mismo nombre y distinto tipo, como `(a:int | a:string)`, da dos errores en la misma posición, el de EDC y el de UPA: son dos restricciones distintas de XSD 1.0 y se informan las dos (S-2 de la revisión de la 5.7).

### 11.4 Costo y tope (D26)

Sea *n* el número de partículas del modelo y *m* ≤ *n* el de posiciones. **Partícula** es aquí lo que es en XSD: un elemento o un grupo (`xs:sequence`, `xs:choice`, `xs:all`), **la raíz incluida** (precisión de D26, que decía «partículas (posiciones)»). Cada uno de los *m* + 1 conjuntos se enumera en O(*n*): cada nodo entra en la pila a lo sumo dos veces (desde su grupo o como hermana o nodo de la subida) y se recorre una sola vez por la marca; volver a entrar se comprueba en O(1) (el nivel guardado y, por nivel de la subida, el más alto con contador); y la subida tiene a lo sumo *n* pasos. En total, **a lo sumo 3·*n*·(*m* + 1) pasos** y memoria O(*n*), sin recursión (D7). El peor caso es real: en `(e₁ | … | eₖ)*` cada *follow* es el modelo entero (*n*·*m* pasos).

Por eso D26 fija **4 096 partículas por modelo** (`upa::MAX_PARTICLES`, `00-alcance.md` §4), el mismo tope que la determinización acotada, y lo comprueba **antes** de calcular ningún conjunto: un modelo con más da el error `limit` (`ErrorKind::TooManyParticles`) en el `<` de la partícula 4 097, y la UPA no da ni un paso. Cuentan los grupos (son partículas en XSD y nodos que la enumeración recorre: 10⁶ `xs:sequence` vacíos entre dos elementos costarían 10⁶ por conjunto sin ser posiciones). Con el tope, un modelo cuesta a lo sumo 3·4 096·4 096 ≈ 5·10⁷ pasos (*m* + 1 ≤ *n*), y el esquema entero, como Σ 3·*n*ᵢ·(*m*ᵢ + 1) ≤ 3·4 096·Σ *n*ᵢ, a lo sumo **3·4 096·*N* pasos**, con *N* el total de partículas: lineal en el tamaño del esquema, pero con una constante de ≈ 1,2·10⁴ pasos por partícula en el peor caso (S-5 de la revisión). Medido a mano en release (fuera de `cargo test`, D1), el peor esquema de 16 MiB (100 modelos así en el tope) tarda ≈ 7-28 s en `schemac check` según la máquina y la carga (tres medidas: 7,5, 23 y 28 s; la regla de las vueltas con contador cuesta O(1) por paso), aceptable con el tope de D26 ([pendientes.md](pendientes.md) §38, 5.7-P3: para el arnés de la Fase 6 y el servidor). El tope es de **tiempo**: se comprueba sobre el AST ya construido, así que no baja el pico de memoria del AST ([../fase2/pendientes.md](../fase2/pendientes.md) §12, Fase 7).

### 11.5 Pruebas

`crates/xsd/tests/upa.rs`: `modelos_ambiguos_con_las_dos_posiciones` (`(a?, a)`, `(a | a)`, `(a+, a)`, `(a*, a)` y `(b, (c | (d | c)))`, con la columna de las dos partículas), `ambiguos_a_traves_de_grupos_y_ref` (la vuelta de un grupo repetido, un grupo opcional con un primer hijo opcional y un `ref` frente a un local del mismo nombre y tipo), `modelos_deterministas_pasan` (entre ellos el modelo de `pedido_xsd`; los del corpus los recorre `corpus_clasificado`), `abstraccion_conservadora_de_d25` (`a{2,2}, a`, `a{3,5}, a?`, `a{0,0}, a` y `a*, <choice/>, a` se rechazan; `a{1,2}, b, a` pasa), `nombre_repetido_en_all` (5.3-CR1-S4), `vueltas_ambiguas_con_contador` (`(a{1,2}){2,2}` y `(a{1,2}, b?)*` con las tres posiciones; `(a+){2,3}`, `(a*){2,3}`, `((a, b){2,2})*`), `contadores_sin_vuelta_ambigua_pasan`, `un_error_por_particula_en_orden`, `upa_en_la_etapa_de_modelos` (UPA con I11 por offset; un error de la etapa 1 la oculta), `tope_de_particulas_por_modelo` (4 096 pasan, 4 097 dan `limit` en el `<` de la última; los grupos cuentan; el tope es por modelo) y `mensajes_recortados`.

`crates/xsd/src/upa.rs` (unitarias, con un contador de pasos que solo existe en `cfg(test)`, sin reloj, D1): `tope_antes_de_calcular_conjuntos` (el peor caso en el tope, un `xs:choice` repetido de 4 095 nombres distintos, termina con más de *n*·*m* y como mucho 3·*n*·(*m* + 1) pasos; con 4 096 hijos da `limit` con **cero** pasos), `vueltas_con_contador_sin_costo_extra` (el mismo modelo con cada hijo repetido: con la elección `unbounded` todo vuelve a entrar y pasa; con `maxOccurs="2"`, 4 095 errores de vuelta con contador; los dos dentro de la misma cota), `sin_upa_si_falla_una_etapa_anterior` y `secuencia_larga_es_lineal`.

Corpus `semantic-error`: `upa-opcional`, `upa-choice`, `upa-estrella`, `upa-choice-anidado`, `upa-contador` (D25), `upa-contador-anidado` y `upa-contador-vuelta` (vueltas con contador), `upa-nombre-local` (desviación 2: `ref="t:a"` y un local `a` con `targetNamespace`), `upa-contador-grupos` (el `parse-ok/grupos.xsd` anterior a la ronda 1, que tenía vueltas ambiguas con contador: el de `parse-ok` se cambió para que sea determinista también con contadores, conservando `xs:all`, grupos anidados con ocurrencias, un elemento con contador y contenido vacío), `upa-all-repetido` (5.3-CR1-S4), y los de EDC que pedía 5.3-CR1-I2, `edc-ref-y-local` y `edc-anonimos`. `parse-error`: `ref-con-nillable` y `ref-con-form` (5.3-CR1-S1, `crates/xsd/tests/parser.rs`, `ref_con_valores_sin_efecto_de_d39`). Para no pasar el tope, `tipo_usado_k_veces_es_una_arista_por_uso` usa 4 000 usos y `uri_de_8_mib_y_medio_millon_de_referencias` reparte las 5·10⁵ referencias en 125 tipos.

### 11.6 Decisiones

Candidatas al registro (tomadas por el Builder de la 5.7; las conservadoras, por delegación):

1. La UPA va en la etapa 3 y solo corre si las etapas 1 y 2 pasaron; el tope de D26 se comprueba en la misma etapa, antes que ella, modelo a modelo.
2. Abstracción de D25 ampliada a `maxOccurs="0"` (como `?`) y al `xs:choice` vacío (como ε): las dos solo rechazan de más (§11.2).
3. Nombres por nombre **local**, como EDC (D40).
4. El tope de D26 cuenta **todas** las partículas (elementos y grupos, la raíz incluida), y el error va en el `<` de la partícula 4 097 del preorden.
5. Error `content-model` (`ErrorKind::Ambiguous`) en la segunda partícula con la primera en el mensaje; cada partícula se informa a lo sumo una vez. Tope: `limit` (`ErrorKind::TooManyParticles`).
6. 5.3-CR1-S1 en el **parser**, no en el análisis: D39 no deja rastro en el AST, así que `ref` con `nillable`/`form` sin efecto se rechaza al llegar el segundo atributo de la pareja, `syntax` en su valor (el error sigue siendo el primero del documento, D37).
7. **Enmienda de D25 (I-1):** además de la UPA sobre la abstracción, se rechazan las vueltas ambiguas con contador (regla de §11.2), `content-model` en el nodo que vuelve a entrar con la partícula que se repite y la del contador en el mensaje. Pendiente de aprobar por las dos personas (D42).
8. **Precisión de D26 (I-2):** partícula = elemento o grupo, la raíz incluida.

## 12. Teoría del frontal XSD: resumen y costo

El frontal XSD (D21) lleva un documento de esquema a la IR común en cuatro pasos, todos de tiempo de compilación, sin recursión sobre la entrada salvo la del parser, acotada por la profundidad (D7):

| Paso | Qué hace | Teoría | Costo |
|---|---|---|---|
| Lector ([../fase2/lector-xsd.md](../fase2/lector-xsd.md)) | Tokens XML bajo demanda sobre el lexer de la Fase 1; buena formación, espacios de nombres (URI internados, `NsId`), profundidad ≤ 256 | autómata finito del lexer + pila de elementos abiertos | O(n) en bytes, memoria O(n) |
| Parser ([../fase2/gramatica-xsd.md](../fase2/gramatica-xsd.md)) | Descendente recursivo LL(1) del subconjunto (tabla `xsd.ll1`), lista blanca de atributos, D6 (rechazo con categoría), AST propio (D3) | gramática LL(1) con FIRST/FOLLOW disjuntos | O(n), recursión ≤ 256 marcos |
| Análisis (§2-§3, §10, §11) | Etapa 1: tabla de símbolos por `(NsId, nombre local)` y resolución; etapa 2: tipos simples (ciclos de `base`, facetas, valores); etapa 3: I11, I12 (EDC, *all-group limited*), D26 y UPA | grafo funcional de bases (ciclos y orden topológico iterativos); Glushkov de cada modelo sobre la abstracción de D25 | O(n) más `ir::num` por valor, más O(*n*ᵢ·*m*ᵢ) ≤ 3·4 096·*N* de la UPA |
| Bajada (§4, §10.3) | Un nodo por declaración, sintéticos por primer uso, cada uso de un tipo es una arista; `verify` siempre (D15) | correspondencia de [ir.md](ir.md) §5.2 | O(n) |

Las garantías que el frontal da a la Fase 4 son las de §5 (la IR pasa `verify` y tiene la semántica del esquema), más dos que `verify` no comprueba: los valores de `default`/`fixed` y de las facetas son del tipo (§10, salvo `pattern`, D29) y **cada modelo de contenido es determinista** sobre la abstracción de D25 (§11): su autómata de posiciones lo es (UPA de XSD) y, además, ninguna posición se alcanza por dos vueltas que cambien distinto un contador (enmienda de D25), con a lo sumo 4 096 partículas. Con las dos cosas, la Fase 4 puede construir el Glushkov con contadores de cada modelo sin determinizar: una transición **de vuelta** repite la partícula de su nivel (guarda «no llegó al máximo») y una **de hermana** sale de ella sin repetirla; salir de una partícula con contador exige su mínimo **salvo que su cuerpo sea anulable**, porque las vueltas que faltan pueden ser vacías (`(a?){2,3}` acepta `<r><a/></r>`; [pendientes.md](pendientes.md) §38, 5.7-P1).
