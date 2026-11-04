# Fase 2 — Gramática del subconjunto XSD y parser → AST XSD

> **Cifras manuales.** [ll1-xsd.md](ll1-xsd.md) lo genera `scripts/ll1.py` (`make docs`) y `make docs-check` lo comprueba (local, D32). El número de clases y terminales de §1, el resumen FIRST/FOLLOW de §3 y la tabla de pila de §8 son un resumen escrito a mano: quien cambie la gramática ([xsd.ll1](xsd.ll1)) o el parser debe actualizarlos en el mismo cambio.

Tarea 5.2. Un documento XSD es un documento XML, así que podría analizarse con la gramática de [gramatica-xml.md](gramatica-xml.md) y revisar después qué hay dentro. Como en JSON Schema ([gramatica-esquema.md](gramatica-esquema.md)), el metalenguaje es más estrecho: el **nombre** de cada elemento de XSD decide qué hijos puede tener (`xs:sequence` contiene partículas, `xs:simpleType` una `xs:restriction`, …). Esa información se mete en la gramática para que el parser la compruebe mientras lee, en una sola pasada y con la posición exacta del primer token que no encaja.

Implementación: `crates/xsd` ([`parser.rs`](../../crates/xsd/src/parser.rs), [`subset.rs`](../../crates/xsd/src/subset.rs), [`ast.rs`](../../crates/xsd/src/ast.rs), [`error.rs`](../../crates/xsd/src/error.rs)), un parser LL(1) descendente recursivo escrito a mano, una función por construcción, sobre los tokens del lector de la tarea 5.1 ([lector-xsd.md](lector-xsd.md)). Produce el **AST propio de XSD** (D3). Pruebas: [`crates/xsd/tests/parser.rs`](../../crates/xsd/tests/parser.rs).

## 1. Terminales

Los terminales son los tokens del lector (`STAG` `ATTR` `>` `/>` `ETAG` `TEXT`; [lector-xsd.md](lector-xsd.md) §1) más **23 clases** `S_*`. Antes de la tabla se descartan las instrucciones de procesamiento y los `TEXT` de solo espacios, igual que el lexer descarta los comentarios: no aparecen en la gramática. En total, 29 terminales.

Una `S_*` no es un token nuevo del lector: es un `STAG` del espacio de nombres de XML Schema (`http://www.w3.org/2001/XMLSchema`, sea cual sea el prefijo, también el espacio por defecto) **clasificado por su nombre local**, lo mismo que las `K_*` de JSON Schema son un `STRING` en posición de clave clasificado por su valor. La partición la decide una tabla (`classify` en `subset.rs`):

| Clase | Elemento |
|---|---|
| `S_SCHEMA` | `xs:schema` |
| `S_ELEMENT` | `xs:element` |
| `S_COMPLEXTYPE` | `xs:complexType` |
| `S_SEQUENCE`, `S_CHOICE`, `S_ALL` | `xs:sequence`, `xs:choice`, `xs:all` |
| `S_ATTRIBUTE` | `xs:attribute` |
| `S_SIMPLECONTENT`, `S_EXTENSION` | `xs:simpleContent`, `xs:extension` |
| `S_SIMPLETYPE`, `S_RESTRICTION` | `xs:simpleType`, `xs:restriction` |
| `S_ENUMERATION` … `S_PATTERN` (9) | una por faceta: `enumeration`, `minInclusive`, `maxInclusive`, `minExclusive`, `maxExclusive`, `length`, `minLength`, `maxLength`, `pattern` |
| `S_ANNOTATION`, `S_DOCUMENTATION`, `S_APPINFO` | `xs:annotation` y sus hijos (D23, U3) |
| — | cualquier otro `STAG`: de XSD fuera del subconjunto (`xs:import`, `xs:any`…), del espacio de XSD sin ser de XSD 1.0 (`xs:elemento`), o de otro espacio de nombres |

La clasificación depende del **contexto**: dentro de `xs:documentation` y `xs:appinfo` (el no terminal `any`), todo `STAG` es un `STAG` cualquiera, también `<xs:element>`. Fuera de `any`, un `STAG` sin clase `S_*` no está en ninguna fila de la tabla: es una celda vacía, y el parser dice qué es (§4). Las `ETAG` no llevan clase: el lector ya garantiza que cierran su apertura ([lector-xsd.md](lector-xsd.md) §2).

Formalmente, la gramática trabaja sobre el alfabeto refinado, y borrar las clases (el homomorfismo `S_* ↦ STAG`) convierte cada derivación en una derivación de la gramática XML de [xml.ll1](xml.ll1) (con `misc` y los `TEXT` de espacios ya descartados): **todo esquema que el parser acepta es un documento XML bien formado**, cosa que además garantiza el lector.

## 2. Gramática LL(1)

```
document       → S_SCHEMA attrs schema_rest
schema_rest    → /> | > top_items ETAG
top_items      → top_item top_items | ε
top_item       → annotation | element | complex_type | simple_type | attribute
attrs          → ATTR attrs | ε
annot_opt      → annotation | ε
annotation     → S_ANNOTATION attrs annot_rest
annot_rest     → /> | > annot_items ETAG
annot_items    → S_DOCUMENTATION attrs any_rest annot_items | S_APPINFO attrs any_rest annot_items | ε
any_rest       → /> | > any ETAG
any            → STAG attrs any_rest any | TEXT any | ε
element        → S_ELEMENT attrs element_rest
element_rest   → /> | > annot_opt element_type ETAG
element_type   → complex_type | simple_type | ε
complex_type   → S_COMPLEXTYPE attrs ct_rest
ct_rest        → /> | > annot_opt ct_content ETAG
ct_content     → simple_content | model_opt attr_decls
model_opt      → sequence | choice | all | ε
attr_decls     → attribute attr_decls | ε
sequence       → S_SEQUENCE attrs group_rest
choice         → S_CHOICE attrs group_rest
group_rest     → /> | > annot_opt particles ETAG
particles      → particle particles | ε
particle       → element | sequence | choice
all            → S_ALL attrs all_rest
all_rest       → /> | > annot_opt all_items ETAG
all_items      → element all_items | ε
attribute      → S_ATTRIBUTE attrs attr_rest
attr_rest      → /> | > annot_opt simple_opt ETAG
simple_opt     → simple_type | ε
simple_content → S_SIMPLECONTENT attrs > annot_opt extension ETAG
extension      → S_EXTENSION attrs ext_rest
ext_rest       → /> | > annot_opt attr_decls ETAG
simple_type    → S_SIMPLETYPE attrs > annot_opt restriction ETAG
restriction    → S_RESTRICTION attrs restr_rest
restr_rest     → /> | > annot_opt simple_opt facets ETAG
facets         → facet facets | ε
facet          → facet_tag attrs facet_rest
facet_tag      → S_ENUMERATION | S_MININCLUSIVE | … | S_PATTERN
facet_rest     → /> | > annot_opt ETAG
```

Símbolo inicial: `document`; 40 no terminales. Archivo fuente: [xsd.ll1](xsd.ll1). Los conjuntos FIRST y FOLLOW y la tabla LL(1) los calcula `scripts/ll1.py` (`make docs`) y están en **[ll1-xsd.md](ll1-xsd.md)**.

La gramática sigue los modelos de contenido de XSD 1.0 (Parte 1, la representación XML de cada componente) restringidos al subconjunto: `xs:annotation` solo como primer hijo (o en cualquier lugar entre las declaraciones globales); en `xs:complexType`, `xs:simpleContent` o un modelo seguido de los atributos; `xs:all` solo como modelo de un tipo (no dentro de `xs:sequence` ni `xs:choice`) y solo con elementos; `xs:simpleContent` y `xs:simpleType` sin la alternativa `/>`, porque XSD exige su derivación; `xs:restriction` con un `xs:simpleType` anónimo opcional (la base anónima) y después las facetas. Las listas son `X → item X | ε`, sin recursión por la izquierda; en el parser son bucles.

## 3. Verificación LL(1)

Condiciones, para cada no terminal A con alternativas α, β: (1) FIRST(α) ∩ FIRST(β) = ∅; (2) si β ⇒* ε, FIRST(α) ∩ FOLLOW(A) = ∅. Resumen de [ll1-xsd.md](ll1-xsd.md), donde está la tabla completa:

| No terminal | FIRST de cada alternativa | FOLLOW | ¿Disjuntos? |
|---|---|---|---|
| `*_rest` (los de alternativa `/>`) | {`/>`} · {`>`} | según el padre | Sí |
| `top_items` | {`S_ANNOTATION` `S_ELEMENT` `S_COMPLEXTYPE` `S_SIMPLETYPE` `S_ATTRIBUTE`} · ε | {`ETAG`} | Sí |
| `top_item` | una clase por alternativa | | Sí |
| `attrs` | {`ATTR`} · ε | {`/>` `>`} | Sí |
| `annot_opt` | {`S_ANNOTATION`} · ε | {`ETAG`, y el FIRST del resto de cada fila: `S_COMPLEXTYPE`, `S_SEQUENCE`, `S_EXTENSION`, facetas…} | Sí: `S_ANNOTATION` no está en FOLLOW |
| `annot_items` | {`S_DOCUMENTATION`} · {`S_APPINFO`} · ε | {`ETAG`} | Sí |
| `any` | {`STAG`} · {`TEXT`} · ε | {`ETAG`} | Sí |
| `element_type` | {`S_COMPLEXTYPE`} · {`S_SIMPLETYPE`} · ε | {`ETAG`} | Sí |
| `ct_content` | {`S_SIMPLECONTENT`} · {`S_SEQUENCE` `S_CHOICE` `S_ALL` `S_ATTRIBUTE`} ∪ ε | {`ETAG`} | Sí |
| `model_opt` | {`S_SEQUENCE`} · {`S_CHOICE`} · {`S_ALL`} · ε | {`S_ATTRIBUTE` `ETAG`} | Sí |
| `attr_decls` | {`S_ATTRIBUTE`} · ε | {`ETAG`} | Sí |
| `particles` / `particle` | {`S_ELEMENT` `S_SEQUENCE` `S_CHOICE`} · ε | {`ETAG`} | Sí |
| `all_items` | {`S_ELEMENT`} · ε | {`ETAG`} | Sí |
| `simple_opt` | {`S_SIMPLETYPE`} · ε | {`ETAG`, las 9 facetas} | Sí |
| `facets` / `facet_tag` | las 9 clases de faceta · ε | {`ETAG`} | Sí: una clase por alternativa |

Como en `keyword` de JSON Schema, la disjunción se apoya en que las clases son una **partición** de los `STAG` de XSD. El script llega a la misma conclusión: ninguna celda de [ll1-xsd.md](ll1-xsd.md) tiene dos producciones.

## 4. Lo que la gramática no expresa

La gramática fija qué construcciones van dentro de cuáles. Lo que depende de los **atributos** (que son un `ATTR*` sin estructura) o de comparar valores lo comprueba el parser aparte (5.2), o queda para la tarea 5.3 (tabla de símbolos y restricciones estructurales) y las 5.5/5.6 (facetas):

| Comprobación | Dónde |
|---|---|
| XML bien formado, espacios de nombres, `<!DOCTYPE` (D8) | lector (5.1) |
| Profundidad ≤ 256 (D7) | lector, a petición del parser (§8) |
| Celdas vacías con su propio mensaje: construcciones de §3.3 (`unsupported`, con categoría y motivo, D6), elementos de otro espacio de nombres o desconocidos de XSD, texto en contenido de solo elementos (`syntax`) | parser (5.2) |
| Lista blanca de atributos por construcción y contexto, global o local (§5); los de otro espacio de nombres se ignoran (D34) | parser (5.2) |
| Atributos obligatorios: `name` en las declaraciones globales y en `xs:attribute`, `base` en `xs:extension`, `value` en las facetas | parser (5.2) |
| Valores que la gramática necesita: `use` (`required`/`optional`; `prohibited` es `unsupported`), `mixed` (`false`; `true` es `unsupported`), `elementFormDefault`, `targetNamespace` no vacío | parser (5.2) |
| QName de `type`, `base` y `ref` bien formado y con prefijo declarado | lector (`as_qname`), al pedirlo el parser |
| `name` o `ref` en un `xs:element` local (no los dos); `type` o tipo anónimo, no los dos; `xs:element` sin tipo (D14); `base` o `xs:simpleType` en `xs:restriction` | 5.3 |
| Forma de `name` (NCName), de `minOccurs`/`maxOccurs` (entero o `unbounded`), `minOccurs` ≤ `maxOccurs`, ocurrencias de `xs:all` | 5.3 |
| Nombres globales únicos, referencias resueltas, ciclos de `base`, D27, `default`/`fixed` del tipo, UPA | 5.3 y 5.7 |
| Forma léxica de los valores de faceta y de `enumeration`, `whiteSpace` por tipo | 5.5 y 5.6 |

La 5.2 no consulta ninguna tabla: los `Name` de `type`/`base`/`ref` llegan al AST resueltos por el lector pero sin buscar su declaración.

## 5. Lista blanca de atributos y correspondencia con `00-alcance.md` §3

Atributos sin espacio de nombres admitidos por construcción (`Ctx::allowed` en `subset.rs`). `id` se acepta y se ignora en todas (D23), y las declaraciones `xmlns`/`xmlns:p` no son atributos para el esquema. Un atributo fuera de la lista es `unsupported` (con categoría y motivo) si XSD lo define en esa construcción y el subconjunto no lo admite, y `syntax` («no admitido en …; admite: …») si XSD tampoco lo define ahí.

| Construcción | Admite (§3.1) | Rechazados con categoría (§3.3, D6) |
|---|---|---|
| `xs:schema` | `targetNamespace`, `elementFormDefault` | `attributeFormDefault` (calificación de nombres), `blockDefault`, `finalDefault` (control de derivación), `version` (metadato) |
| `xs:element` global | `name`\*, `type` | `abstract`, `substitutionGroup` (sustitución), `nillable` (valores nulos), `default`, `fixed` (valor de un elemento), `block`, `final` |
| `xs:element` local | `name`, `type`, `ref`, `minOccurs`, `maxOccurs` | `form`, `nillable`, `default`, `fixed`, `block` |
| `xs:complexType` | `name`\* (solo global), `mixed` (solo `false`) | `abstract`, `block`, `final`; `mixed="true"` (contenido mixto) |
| `xs:sequence`, `xs:choice`, `xs:all` | `minOccurs`, `maxOccurs` | — |
| `xs:attribute` local | `name`\*, `type`, `use`, `default`, `fixed` | `ref` (referencia a un atributo global), `form`; `use="prohibited"` |
| `xs:attribute` global | `name`\*, `type`, `default`, `fixed` | — |
| `xs:simpleContent` | — | — |
| `xs:extension`, `xs:restriction` | `base` (obligatorio en `xs:extension`) | — |
| `xs:simpleType` | `name`\* (solo global) | `final` |
| facetas | `value`\* | `fixed` en las de rango y de longitud (control de derivación); en `xs:enumeration` y `xs:pattern` (`xs:noFixedFacet`) XSD no lo define, así que es `syntax` |
| `xs:annotation` | — | — |
| `xs:documentation`, `xs:appinfo` | `source` (ignorado) | — |

\* obligatorio. **D34:** un atributo de **otro espacio de nombres** que el de XSD (`xml:lang`, `xsi:type`, `xsi:nil`, `x:version`…) se acepta y se ignora en cualquier construcción, como las anotaciones de D23, y no llega al AST: el esquema de esquemas los admite (`anyAttribute namespace="##other"`) y no cambian la validación. Uno calificado con el **propio** espacio de XSD (`xs:type="xs:int"` en un `xs:element`) no es de XSD ni de otro espacio: es `syntax`, con el mismo mensaje que un atributo desconocido. Siguen siendo error un atributo sin espacio de nombres que XSD no define en esa construcción (`syntax`) o que el subconjunto excluye (`unsupported`), y un elemento ajeno al espacio de XSD fuera de `xs:documentation`/`xs:appinfo` (`syntax`). Prueba `atributos_de_otro_espacio_de_nombres_se_ignoran` (el mismo AST con y sin ellos, también con 10⁴ en una etiqueta) y `parse-ok/atributos-ajenos.xsd`.

Elementos de §3.3 rechazados como celda vacía con categoría y motivo (`UNSUPPORTED_ELEMENTS` en `subset.rs`): `xs:import`, `xs:include`, `xs:redefine` (composición de esquemas); `xs:any`, `xs:anyAttribute` (comodín); `xs:group`, `xs:attributeGroup` (grupo con nombre); `xs:complexContent` y `xs:restriction` dentro de `xs:simpleContent` (derivación de tipos complejos, D27); `xs:list`, `xs:union` (lista o unión); `xs:key`, `xs:keyref`, `xs:unique`, `xs:selector`, `xs:field` (clase (c), no libre de contexto); `xs:totalDigits`, `xs:fractionDigits`, `xs:whiteSpace` (faceta fuera del subconjunto); `xs:notation`; y `xs:override`, `xs:assert`, `xs:alternative`, `xs:openContent` (XML Schema 1.1). Los tipos de fecha y hora de §3.3 son **nombres de tipo**: los rechaza la 5.3 al resolver `type`/`base`.

## 6. El AST

Tipos en [`ast.rs`](../../crates/xsd/src/ast.rs): `Schema` (raíz) con sus `TopLevel` (`Element`, `ComplexType`, `SimpleType`, `Attribute`); `ComplexType` con `ComplexContent::Simple` (`SimpleContent`: `base` y atributos) o `ComplexContent::Elements` (modelo `Group` opcional y atributos); `Group` con su `Compositor`, ocurrencias y `Particle`s (`Element` o `Group`); `SimpleType` con su `Restriction` (`base`, base anónima y `Facet`s). Cada construcción lleva el `Span` de su elemento entero (del `<` al `>` de su cierre o al `/>`); cada valor de atributo es un `Spanned<T>` con el rango del valor y sus comillas, y `at()` da su primer carácter (la posición de los errores sobre el valor, como en `as_qname`).

Qué se guarda de cada atributo:

- **`type`, `base`, `ref`:** el `Name` que resolvió el lector con el ámbito de **su** etiqueta. El parser llama a `Attr::as_qname()` **una sola vez** por atributo (cada llamada copia el QName; 5.1-SEC3-I3) y guarda el resultado; un parser que lee por adelantado no cambia la resolución (prueba `qname_resuelto_con_el_ambito_de_su_etiqueta`, con un hijo que redeclara el prefijo). La 5.3 los buscará con `(ns_id(), local())` como clave.
- **`targetNamespace`:** se recorta (*collapse*: es `xs:anyURI`), se interna con `Reader::intern` mientras el lector existe y su `NsId` queda en `Schema::target_namespace` (5.1-CR3-I1). Es **el único** valor que se interna (5.1-SEC3-I2: la tabla de URI crece sin límite). Con `xmlns:t='urn:t' targetNamespace=' urn:t '`, el `NsId` es el mismo que el del prefijo de `type='t:T'` (prueba `target_namespace_recortado_da_el_mismo_ns_id`). Vacío tras recortar es un error `value` (XSD no admite `targetNamespace=""`).
- **`name`, `minOccurs`, `maxOccurs`, `use`, `mixed`, `elementFormDefault` y las facetas de rango y de longitud:** *collapse* (sin espacios en los extremos, cada secuencia interior reducida a uno) sobre una **copia** del valor (`Attr::value()`), nunca modificando el `Attr` (§9 de [pendientes.md](pendientes.md)). Se guarda el lexema: su forma la comprueban la 5.3 (`minOccurs`/`maxOccurs`, `name`) y las 5.5/5.6 (facetas).
- **`default`, `fixed`, `xs:enumeration`, `xs:pattern`:** el valor tal como lo entrega el lector (normalización de XML 1.0), con `Attr::into_value()`: su `whiteSpace` depende del tipo (o, en `pattern`, de nadie) y lo aplican las 5.5/5.6.
- **`id`, `source`, `xs:annotation` y su contenido:** nada (D23); **los atributos de otro espacio de nombres**, tampoco (D34).

El AST no tiene `Drop` recursivo **profundo**: su profundidad es la del documento, que el lector acota a 256 elementos (§8), y su liberación se mide junto con el parser.

## 7. Corrección

**Proposición.** Si `parse` devuelve un AST, la sucesión de tokens (sin PI ni espacios) pertenece a L(G) para la gramática G de §2, y cumple las restricciones de §4 que son de la 5.2. Si devuelve un error, es el del primer token (en el orden de la entrada) que no admite ninguna derivación de G o que viola una de esas restricciones; un atributo obligatorio que falta se sabe, y se informa, en el `>` o `/>` de su etiqueta.

*Esbozo.* Cada función del parser es la de su no terminal: decide la producción con el token de anticipación y la fila de [ll1-xsd.md](ll1-xsd.md) (las listas, con un bucle que sale cuando el token no está en el FIRST del elemento, es decir, cuando está en el FOLLOW, que es `ETAG` o el FIRST de lo que sigue en la fila). Como la tabla no tiene conflictos, la derivación es la única posible, y una celda vacía es exactamente un error. Cada atributo se comprueba contra la lista blanca y se **convierte** (QName, *collapse*, palabras de `use`/`mixed`/`elementFormDefault`, internado de `targetNamespace`) **al llegar**, en el orden del documento, no en el orden en que cada construcción pide después sus valores; los obligatorios se comprueban al llegar al `>` o `/>`, que es donde se sabe que faltan y donde se informa el error. Todo eso ocurre **antes** de consumir el `>` o `/>`, así que el error de una etiqueta sale antes que cualquiera de lo que la sigue (prueba `primer_error_en_orden_del_documento`, con `<xs:element ref='u:x' type='v:y'/>`, que informa `u`, `<xs:attribute use='zz' … type='v:y'/>`, que informa `use`, y `<xs:element nillable='true'/>` global, que informa `nillable` antes que la falta de `name`); los errores del lector salen cuando el parser pide el token donde ocurren. Esto deja **dos excepciones** al «primer error de la entrada», las dos porque el lector entrega un token solo cuando está completo: (1) un error de buena formación o de espacios de nombres de una etiqueta sale antes que un error del parser sobre esa misma etiqueta (el lector la comprueba entera al leer su `>`); (2) un error léxico entre el comienzo de un texto y el token que lo termina sale antes que `TextNotAllowed` sobre ese texto. Ese tramo incluye el propio texto, los comentarios y las secciones CDATA que lo continúan, y los bytes de marca tras el `<` siguiente hasta que el lexer sabe que es una etiqueta, un cierre o una PI; el `TEXT` es máximo y el lector solo lo entrega entonces. Ejemplos (con el texto en la columna 56): `hola &#0;` informa la referencia inválida, y `hola <1/>`, `hola <!X`, `hola <![CDATX[` y `hola <!-- a -- b -->` informan el error léxico de la marca (columnas 62, 63, 68 y 70), no el texto (caso `parse-error/texto-con-error-lexico.xsd`; el del comentario queda pendiente, [pendientes.md](pendientes.md) §19).

`any` es la única producción recursiva que el parser no sigue con recursión: el lector garantiza que lo que hay entre la apertura de `xs:documentation` y su cierre es una sucesión de elementos bien anidados, texto y PI, que es exactamente `any` ⇒* (un lenguaje de Dyck con `STAG … (/> | > … ETAG)` como paréntesis). Para saber dónde termina basta un **contador** de elementos abiertos: sube en cada `STAG`, baja en cada `/>` y cada `ETAG`, y el `ETAG` con el contador a cero es el de `xs:documentation`. Es la misma pila de la derivación, guardada como un número porque todos sus símbolos son iguales.

## 8. Costo, profundidad y pila

**Tiempo: O(n).** El lector es O(n) ([lector-xsd.md](lector-xsd.md) §5). El parser hace O(1) trabajo por token más el que se cobra a los bytes del propio token: clasificar un `STAG` compara su nombre local con constantes (O(1): `==` de `str` compara primero las longitudes) y su URI con `XS_NS` (O(32)); cada atributo se busca en una lista blanca de a lo sumo 6 nombres; el *collapse*, `as_qname` y `into_value` cuestan O(|valor|) **una vez** por atributo; un `TEXT` se recorre una vez para ver si es de espacios. Ningún paso consulta otra parte de la entrada ni una tabla que crezca con el documento (`Reader::intern` se llama una vez). Los errores copian como mucho 33 caracteres del texto del usuario (§9). El tiempo no se mide en `cargo test` (D1); la prueba `uri_largo_y_muchas_referencias_en_una_pasada` usa un URI de 1 MiB como `targetNamespace` y como prefijo de 5·10⁴ referencias `type='t:T'`, un tamaño con el que un costo O(referencias · |URI|) no terminaría en un tiempo razonable, y comprueba que todas comparten el `NsId` del `targetNamespace`.

**Memoria: O(n)** para el AST (los nombres y lexemas guardados, más un nodo por construcción), más la del lector, pero con una **constante grande y sin tope propio**. Un `Element` o un `Group` ocupa unos 300-400 bytes, frente a los 10-15 bytes de `<element/>` o `<choice/>` en la entrada. Pico medido por la auditoría de la 5.2 en release, con entradas de 16 MiB (el tope de `schemac`):

| Entrada de 16 MiB | Pico | Factor |
|---|---|---|
| 1,5 M `<choice/>` en una secuencia | 601 MiB | ≈ 34× |
| 1,67 M `<element/>` locales | 543 MiB | ≈ 31× |
| 1,5 M `<sequence/>` | 495 MiB | ≈ 28× |
| 508 k declaraciones globales | 208 MiB | ≈ 12× |
| facetas | 74-90 MiB | ≈ 5× |
| `<xs:schema` con 1,9 M atributos (los arma el lector antes de que el parser vea el primero) | 673 MiB | ≈ 40× |

El tiempo es lineal en todos. D26 (4 096 partículas por modelo) lo comprueba la 5.7, **después** de construir el AST, y D34 no cambia el pico de la última fila (la etiqueta se arma entera antes de que el parser vea el primer atributo); solo hace que esa entrada sea válida si los atributos son de otro espacio de nombres (medido: 1,28 M atributos con prefijo, válidos, 522 MiB ≈ 32×; 1,49 M sin prefijo, error en el primero, 510 MiB ≈ 31×). Por decisión del orquestador (como con el lector en la 5.1) **no se limita en esta tarea**: los arreglos propuestos y su tarea de destino están en [pendientes.md](pendientes.md) §12.

**Profundidad (D7).** `parse` y `parse_with_max_depth` fijan en el lector el tope de anidamiento de elementos (`Reader::with_max_depth`, `MAX_DEPTH_LIMIT` = 256, solo configurable a la baja; un valor mayor se recorta y el error informa el máximo efectivo). El lector rechaza el elemento número 257 de una cadena de anidados en su `<` (error `limit`), **antes de leer su nombre**, y se detiene: la pila de elementos abiertos del lector nunca guarda más de 256 nombres y el resto del documento no se lee ([pendientes.md](pendientes.md) §12). La prueba `corte_por_profundidad_sin_leer_el_resto` entrega 10⁶ niveles de `<xs:sequence>` en bloques de uno por nivel y comprueba que el análisis pide exactamente 256 bloques (la cabecera y 255 niveles), y que el lector, al fallar, tiene 256 elementos abiertos. La profundidad cuenta **todos** los elementos, también los de dentro de `xs:documentation`.

**Pila.** La recursión del parser sigue el anidamiento de las construcciones (un marco de función por elemento abierto, más los de `open`/`close`/`bump`, que vuelven enseguida), así que está acotada por el tope. Se midió la pila mínima con la que el parser termina (análisis y liberación del AST) en un hilo de tamaño controlado, con búsqueda binaria en un programa aparte (`target/sondas-5.2/`, fuera del repositorio), para cada cadena de anidamiento que recorre la recursión:

| Cadena (profundidad 256) | `dev` | `release` |
|---|---|---|
| `xs:element` > `xs:complexType` > `xs:sequence` > … | 1015 KiB (~3,9 KiB por nivel) | 371 KiB (~1,4 KiB) |
| `xs:sequence` > `xs:sequence` > … (y `xs:choice`) | 903 KiB (~3,4 KiB) | 363 KiB (~1,4 KiB) |
| `xs:simpleType` > `xs:restriction` > … (bajo `xs:element` o `xs:attribute`) | 491 KiB (~1,8 KiB) | 199-205 KiB (~0,8 KiB) |
| contenido de `xs:documentation` (contador, sin recursión) | — | — |

El peor caso, `dev` con la cadena `element`/`complexType`/`sequence`, usa ~1 MiB de los 2 MiB que Rust y tokio dan a cada hilo: queda ~1 MiB de margen (con el parser de JSON Schema el margen en `dev` es de ~0,66 MiB; [gramatica-esquema.md](gramatica-esquema.md) §6). Como allí, el error viaja en una caja (`Result<T, Box<XsdError>>`) y la construcción de los mensajes está en funciones `#[cold]`. La prueba `profundidad_maxima_en_un_hilo_de_2_mib` analiza y libera cada cadena a la profundidad máxima en un hilo de exactamente 2 MiB y comprueba que un nivel más es `limit`; `profundidad_configurable_acotada` lo hace con `u32::MAX`, `MAX_DEPTH_LIMIT + 1` y 10⁵ sobre 10⁵ niveles. Sin el corte del lector, las pruebas de profundidad desbordan la pila y abortan el proceso (comprobado quitando el corte). La garantía vale en hilos con **≥ 2 MiB de pila**: `schemac` y el servidor deben llamar al parser en un hilo con `stack_size` explícito (D7; [pendientes.md](pendientes.md) §5).

## 9. Errores

El parser se detiene en el **primer** error y lo devuelve como `XsdError { kind, at }`, con la posición (offset, línea, columna en puntos de código) delante del mensaje, de una línea, con el texto del usuario recortado y escapado con `lexer::short` (D18). Todo error se construye con `XsdError::new(kind, at)` (`error.rs`), **único constructor dentro del crate**, que recorta los textos a `SHORT_MAX + 1` = 33 caracteres; quien lo llama pasa ya una copia recortada (`clip(&nombre)`), nunca el nombre entero (5.1-CR3-S1/S2).

```
línea 2, columna 3: elemento <xs:import> no soportado (composición de esquemas): el esquema se compila desde un solo documento, sin leer ni resolver otros
línea 4, columna 45: atributo "form" no soportado en xs:element local (calificación de nombres): los elementos se comparan por nombre local en un único espacio de nombres (00-alcance.md §1.2)
línea 4, columna 46: texto en el contenido de xs:sequence, que solo admite elementos (y espacios entre ellos)
línea 4, columna 7: se esperaba xs:element, xs:sequence, xs:choice o </xs:sequence>, se encontró <xs:all>
```

| `code()` | Clases (además de las del lector, [lector-xsd.md](lector-xsd.md) §7) | Posición |
|---|---|---|
| `limit` | `TooDeep` | el `<` del elemento que pasa el tope |
| `syntax` | `Unexpected` (celda vacía; el mensaje lista lo que admitía la fila), `TextNotAllowed`, `ForeignElement`, `UnknownElement`, `UnknownAttribute`, `MissingAttribute` | el `<` del elemento; el primer carácter del texto (que puede ser un espacio: el `TEXT` es máximo); el nombre del atributo; el `>` o `/>` de la etiqueta a la que le falta el atributo (donde se sabe que falta) |
| `unsupported` | `UnsupportedElement`, `UnsupportedAttribute`, `UnsupportedValue` (D6: categoría y motivo) | el `<`; el nombre del atributo; el primer carácter del valor |
| `value` | `InvalidValue` | el primer carácter del valor (el que sigue a la comilla) |
| `namespace` | QName de `type`/`base`/`ref` inválido o con prefijo sin declarar (lo da `as_qname`) | el primer carácter del valor |

**Orden.** El error es el primero de la entrada, con dos excepciones (§7), las dos porque el lector entrega cada token entero: (1) un error de buena formación o de espacios de nombres de una etiqueta (lo comprueba el lector al leer su `>`) sale antes que cualquier error del parser sobre esa misma etiqueta, aunque esté más adelante en ella; (2) un error léxico entre el comienzo de un texto y el token que lo termina sale antes que `TextNotAllowed` sobre ese texto. Ese tramo incluye el propio texto, los comentarios y las secciones CDATA que lo continúan, y los bytes de marca tras el `<` siguiente hasta que el lexer sabe que es una etiqueta, un cierre o una PI; el `TEXT` es máximo y el lector solo lo entrega entonces (`parse-error/texto-con-error-lexico.xsd`).

## 10. Correspondencia con el código

| No terminal | Función en `parser.rs` |
|---|---|
| `document`, `schema_rest`, `top_items`, `top_item` | `Parser::document`, `Parser::schema` |
| `attrs` y la lista blanca | `Parser::open`, `Parser::attr` (más `Ctx` en `subset.rs`) |
| `*_rest` (`/>` o `>`) y el `ETAG` | `Parser::tag_end`, `Parser::close` |
| `annot_opt`, `annotation`, `annot_items` | `Parser::annotation_opt`, `Parser::annotation` |
| `any_rest`, `any` | `Parser::any_content` (contador, sin recursión) |
| `element`, `element_rest`, `element_type` | `Parser::element` |
| `complex_type`, `ct_rest`, `ct_content`, `model_opt`, `attr_decls` | `Parser::complex_type`, `Parser::attr_decls` |
| `sequence`, `choice`, `group_rest`, `particles`, `particle`, `all`, `all_rest`, `all_items` | `Parser::group` |
| `attribute`, `attr_rest`, `simple_opt` | `Parser::attribute` |
| `simple_content`, `extension`, `ext_rest` | `Parser::simple_content` |
| `simple_type`, `restriction`, `restr_rest`, `facets` | `Parser::simple_type`, `Parser::restriction` |
| `facet`, `facet_tag`, `facet_rest` | `Parser::facet` |

Cada función decide con `self.look` (un token) y, si el token no está en la fila de la tabla, construye el error con `Parser::unexpected` y lo que la fila admitía.

## 11. Corpus

- `tests/schemas/xsd/parse-ok/`: esquemas que deben dar un AST: `pedido.xsd` (el equivalente de `pedido_xsd` de `crates/ir/tests/forma.rs`, que la 5.3 comparará con esa IR), los esquemas de los dos casos XML de `tests/cases` (copiados con su `_id`), anotaciones en todas las posiciones (D23), todas las facetas y la base anónima, grupos y contenido vacío, el espacio de XSD como espacio por defecto, otro prefijo, un esquema vacío y atributos de otros espacios de nombres en varias construcciones (`atributos-ajenos.xsd`, D34).
- `tests/schemas/xsd/parse-error/` + `tests/schemas/errores-xsd.json`: 82 esquemas erróneos con el código, la línea, la columna y un texto del mensaje esperados, en el estilo de `tests/schemas/errores.json`: cada construcción y atributo de §3.3, texto en contenido de solo elementos, elementos ajenos y desconocidos, celdas vacías de la gramática, la lista blanca global y local, valores, QName, el orden de los errores dentro de una etiqueta y el orden entre un texto y un error léxico (§7), profundidad y dos errores del lector. Las posiciones se calcularon con un script que no usa el parser (una marca en el texto de cada caso).

Lo ejecuta `crates/xsd/tests/parser.rs`, que además comprueba el mismo resultado con bloques de 1, 7 y 4096 bytes en todo el corpus y que los casos XML de `tests/cases` parsean.
