# Alcance: subconjunto soportado de JSON Schema, XSD y DTD

> Versión inicial (T0.6). Se refina en la Fase 3 (T3.5, T3.10) y se consolida en la documentación técnica (T9.1).

Este documento fija la **lista blanca** de palabras clave y construcciones que acepta `schemac`. Todo lo que no aparece aquí está fuera de alcance y el compilador lo **rechaza** con un error que nombra la palabra clave y su posición. No hay un modo que la ignore: ignorar, por ejemplo, `if`/`then` haría aceptar payloads que el esquema rechaza sin que el usuario lo note. Las anotaciones de §2.1 sí se aceptan porque no cambian el resultado de la validación.

La columna **Cat.** indica la categoría de reconocibilidad:

- **(a)** directa en el DPDA: se traduce a transiciones o guardas sobre un token.
- **(b)** con registros o contadores acotados en el marco de pila (máscara de bits, contador).
- **(c)** no determinista o no libre de contexto: se resuelve con producto de autómatas, determinización acotada o verificación diferida (T3.6–T3.8).

El compilador aplica estas clases con una sola tabla en el código, comprobada contra esta columna por una prueba: [fase3/clasificacion.md](fase3/clasificacion.md).

---

## 1. Payloads aceptados

### 1.1 JSON

- Sintaxis de **RFC 8259**. Codificación **solo UTF-8**; un BOM inicial se ignora. UTF-8 inválido (incluidos *overlongs* y sustitutos U+D800–U+DFFF) es error léxico.
- Un único valor de nivel superior de cualquier tipo.
- **Claves duplicadas** en un objeto: error de payload malformado. Se detectan exactamente para las claves declaradas en `properties` (máscara de bits); para claves no declaradas se usa una tabla hash acotada por `max_claves_adicionales` y, si se excede, se rechaza el payload por límite.
- Los números se comparan sobre el **lexema decimal exacto**, no sobre un `f64`; así `minimum`, `maximum` y `multipleOf` no pierden precisión. Un número es `integer` si su parte fraccionaria es cero (`1.0` es entero, como en 2020-12).
- `minLength`/`maxLength` cuentan **puntos de código** Unicode tras decodificar los escapes, no bytes.

### 1.2 XML

- XML 1.0 bien formado, **solo UTF-8**; un BOM inicial se ignora. Se acepta la declaración `<?xml version="1.0" encoding="UTF-8"?>` únicamente al inicio del documento; un `encoding` distinto de UTF-8 es error. Lo verifica el parser XML (la posición es una propiedad sintáctica): el lexer le entrega el nombre y el contenido de cada instrucción de procesamiento.
- **Prohibido:** `<!DOCTYPE>`, DTD interna o externa, entidades generales o de parámetro declaradas por el usuario (mitigación de XXE y *billion laughs*). Un `<!DOCTYPE` es error léxico. La prohibición se refiere al **payload**: una DTD se puede dar como **esquema** aparte, igual que un XSD (§3bis, D86), y el documento que se valida contra ella tampoco lleva `<!DOCTYPE`.
- Se aceptan las cinco entidades predefinidas (`&lt; &gt; &amp; &quot; &apos;`) y referencias de carácter `&#N;` / `&#xN;`.
- Se aceptan comentarios, secciones CDATA e instrucciones de procesamiento con nombre (`<?nombre …?>`); el validador ignora su contenido. `<??>` sin nombre es error léxico.
- Espacios de nombres: un único `targetNamespace` (o ninguno). Las declaraciones `xmlns` se aceptan; los elementos se comparan por nombre local dentro de ese espacio de nombres.
- El texto formado solo por espacios entre elementos se ignora en contenido de solo elementos. No se admite contenido mixto.

---

## 2. JSON Schema (subconjunto del Draft 2020-12)

### 2.1 Núcleo y referencias

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `$schema` | Solo `https://json-schema.org/draft/2020-12/schema` (o ausente), y solo en la raíz. Se admite también con un `#` final vacío (`…/schema#`), que según la RFC 3986 es el mismo recurso. Otro valor → error. | — |
| `$id` | Solo en la raíz, como URI base. No se descargan recursos remotos. | — |
| `$defs` (y `definitions` como alias) | Tabla de símbolos de subesquemas. | — |
| `$ref` | Solo referencias **locales**: `#`, `#/$defs/x`, JSON Pointer dentro del documento, `#ancla` (la cadena vacía equivale a `#`). El destino debe ser un subesquema, no un literal (`const`, `enum`, `default`…). Recursión permitida si es productiva (T3.4): un ciclo de `$ref` y aplicadores sobre la misma instancia (`allOf`, `anyOf`, `oneOf`, `not`, `dependentSchemas`) que no pase por `properties`, `additionalProperties`, `items` o `prefixItems` es un error ([fase3/ciclos.md](fase3/ciclos.md)). | (a) |
| `$anchor` | Anclas locales, únicas en el documento. | — |
| `$comment`, `title`, `description`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` | Anotaciones: se aceptan y se ignoran. | — |
| Esquemas booleanos `true` / `false` | `true` acepta cualquier valor; `false` rechaza. | (a) |

### 2.2 Cualquier tipo

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `type` | Cadena o arreglo de tipos: `null`, `boolean`, `object`, `array`, `number`, `integer`, `string`. | (a) |
| `enum` | Arreglo de literales de cualquier tipo; se compila a un trie/AFD sobre el valor canónico. | (a) |
| `const` | Equivale a `enum` de un solo elemento. | (a) |

### 2.3 Números

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `minimum`, `maximum` | Comparación decimal exacta. | (a) |
| `exclusiveMinimum`, `exclusiveMaximum` | Solo la forma numérica de 2020-12 (no la booleana de Draft 4). | (a) |
| `multipleOf` | Valor > 0; aritmética decimal exacta sobre el lexema. | (a) |

### 2.4 Cadenas

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `minLength`, `maxLength` | Contador de puntos de código en el marco de la cadena. | (b) |
| `pattern` | Subconjunto de ECMA-262 compilado a AFD **sin backtracking**: literales, clases `[...]`, `.`, `\d \w \s`, anclas `^ $` (solo al principio o al final de una alternativa de primer nivel), alternancia, grupos, cuantificadores `* + ? {m,n}`, escapes de control y `\uHHHH`, con semántica de puntos de código (`u`). **Rechazados:** retrorreferencias, *lookahead*/*lookbehind*, cuantificadores perezosos o posesivos, `\b`, `\p{…}`, grupos con nombre, `\cX`, `\u{…}` (mitigación de ReDoS; F4-U14, [fase4/pattern.md](fase4/pattern.md)). | (c) |
| `format` | Solo anotación: se ignora (no se valida). | — |

### 2.5 Arreglos

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `items` | Esquema para los elementos (los que siguen a `prefixItems`). | (a) |
| `prefixItems` | Esquemas posicionales. | (a) |
| `minItems`, `maxItems` | Contador en el marco del arreglo. | (b) |
| `uniqueItems` | Verificación diferida con hash acotado de los elementos al cerrar el arreglo. | (c) |

### 2.6 Objetos

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `properties` | Despacho por nombre de clave mediante trie. | (a) |
| `required` | Máscara de bits en el marco del objeto; se comprueba al leer `}`. | (b) |
| `additionalProperties` | `false`, `true` o un esquema: una clave que no está en el trie de `properties` toma la rama por defecto (rechazo o subautómata). | (a) |
| `minProperties`, `maxProperties` | Contador en el marco del objeto. | (b) |
| `dependentRequired` | Comprobación al cerrar el objeto con la máscara de bits. | (c) |
| `dependentSchemas` | Verificación diferida. | (c) |

### 2.7 Combinadores

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `allOf` | Producto (intersección) de autómatas. | (c) |
| `anyOf` | Determinista si las ramas se distinguen por el primer token; si no, determinización acotada (límite de estados configurable) y, al excederlo, verificación diferida. | (c) |
| `oneOf` | Como `anyOf`, contando que acepte **exactamente una** rama. | (c) |
| `not` | Complemento de un subautómata determinista: se invierte su veredicto al cerrar la instancia; si el subesquema es de categoría (c), sigue la estrategia de ese subesquema (producto acotado o verificación diferida) y se invierte el veredicto que entrega ([fase3/ir.md](fase3/ir.md) §6.1). | (c) |

### 2.8 Fuera de alcance

`if`/`then`/`else`, `contains`/`minContains`/`maxContains`, `patternProperties`, `propertyNames`, `unevaluatedProperties`, `unevaluatedItems`, `$dynamicRef`, `$dynamicAnchor`, `$vocabulary`, `contentEncoding`, `contentMediaType`, `contentSchema`, `$ref` remotos, validación de `format`, la forma booleana de `exclusiveMinimum`/`exclusiveMaximum` y la forma de arreglo de `items` (Draft 2019-09 y anteriores).

El compilador rechaza cada una de estas palabras clave (y cualquier otra que no esté en la lista blanca) con un mensaje que dice su categoría y por qué no se soporta; las reglas exactas de valor de las soportadas (rangos, nombres de `type`, repetidos…) están en [fase3/meta-esquema.md](fase3/meta-esquema.md).

Candidatos a añadirse si sobra tiempo: `contains` (contador + producto), `patternProperties` y `propertyNames` (AFD sobre las claves).

---

## 3. XSD (subconjunto de XML Schema 1.0)

Las construcciones XSD se analizan a un **AST propio de XSD** y bajan a la **misma IR** que JSON Schema (T2.8; decisión D3 del plan): el parser XSD no reutiliza el AST de JSON Schema. La regla *Unique Particle Attribution* de XSD obliga a que los modelos de contenido sean expresiones regulares deterministas, lo que encaja directamente con la construcción de un AFD por modelo de contenido. El compilador la comprueba de forma conservadora (§3.4, D25) y rechaza con su posición los modelos que no la cumplen.

### 3.1 Construcciones soportadas

| Construcción | Soporte | Cat. |
|---|---|---|
| `xs:schema` | Atributos `targetNamespace`, `elementFormDefault`; `attributeFormDefault` solo con `unqualified`, que no tiene efecto (§3.4). | — |
| `xs:element` | Global o local; `name`, `type`, `ref`, `minOccurs`, `maxOccurs` (entero o `unbounded`); `nillable="false"`, `form` igual al vigente y, en los globales, `abstract="false"`, que no tienen efecto (§3.4). | (a) |
| `xs:complexType` | Con nombre o anónimo. | (a) |
| `xs:sequence` | Concatenación ordenada de partículas. | (a) |
| `xs:choice` | Unión de partículas. | (a) |
| `xs:all` | Partículas en cualquier orden, cada una a lo sumo una vez: máscara de bits. | (b) |
| `minOccurs`, `maxOccurs` | En elementos y en `xs:sequence`, `xs:choice` y `xs:all` (en `xs:all` y en sus elementos, solo 0 o 1: *all-group limited*). Contador acotado en el marco de pila cuando hace falta (no se expande el modelo); el resto, estados del autómata de Glushkov. | (b) cuando la ocurrencia necesita contador: `minOccurs` ≥ 2 o `maxOccurs` acotado > 1; una vez, `?`, `*`, `+` y `maxOccurs="0"` van en el autómata (a) (D16) |
| `xs:attribute` | `name`, `type`, `use` (`required`/`optional`), `default`, `fixed`; `form="unqualified"`, sin efecto (§3.4). Los requeridos usan máscara de bits. | (b) |
| `xs:simpleContent` + `xs:extension` | Texto de tipo simple más atributos. La `base` tiene que ser un tipo simple: extender un tipo complejo de contenido simple es derivar un tipo complejo de otro y se rechaza (§3.3, D27). | (a) |
| `xs:simpleType` + `xs:restriction` | Con nombre o anónimo; `base` o un `xs:simpleType` anónimo como base. Facetas: `enumeration`, `minInclusive`, `maxInclusive`, `minExclusive`, `maxExclusive`, `length`, `minLength`, `maxLength`. Las comprobaciones de XSD 1.0 sobre los tipos simples están en [fase3/xsd.md](fase3/xsd.md) §10. | (a); facetas de longitud (b) |
| Faceta `pattern` | Igual que `pattern` de JSON Schema con la sintaxis de XSD (anclado implícito, `^ $` literales). **Desviación:** `\d` = `[0-9]`. **Rechazados:** `\D \w \W \i \I \c \C \p \P` y la resta de clases (F4-U14). | (c) |

### 3.2 Tipos predefinidos

`xs:string`, `xs:boolean`, `xs:decimal`, `xs:integer`, `xs:long`, `xs:int`, `xs:short`, `xs:byte`, `xs:nonNegativeInteger`, `xs:positiveInteger`, `xs:double`, `xs:float`.

### 3.3 Fuera de alcance

`xs:import`, `xs:include`, `xs:redefine`, `xs:any`, `xs:anyAttribute`, `xs:group`, `xs:attributeGroup`, `substitutionGroup`, `abstract="true"`, `nillable="true"`/`xsi:nil` (`abstract="false"` y `nillable="false"` se aceptan, §3.4), `xsi:type`, contenido mixto (`mixed="true"`), `xs:element` sin `type`, `ref` ni tipo anónimo (su tipo implícito es `xs:anyType`, que admite cualquier contenido, también mixto, elementos no declarados y cualquier atributo: equivale a `xs:any`, `xs:anyAttribute` y `mixed="true"`, ya excluidos; [fase3/ir.md](fase3/ir.md) §5.2), `xs:list`, `xs:union`, `xs:complexContent` (extensión/restricción de tipos complejos), `xs:simpleContent` + `xs:extension` con un tipo complejo como `base` (también es derivar un tipo complejo de otro, D27), restricciones de identidad (`xs:key`, `xs:keyref`, `xs:unique`, no libres de contexto), facetas `totalDigits`/`fractionDigits`/`whiteSpace`, y tipos de fecha y hora. También: `use="prohibited"` (solo tiene efecto al restringir un tipo complejo), `version` en `xs:schema`, `fixed` en las facetas, `attributeFormDefault="qualified"` y `form` distinto del vigente (§3.4).

### 3.4 Lo que se acepta y se ignora, y desviaciones de XSD 1.0

**Se aceptan y se ignoran** (no cambian la validación; no llegan a la IR):

- `xs:annotation`, `xs:documentation`, `xs:appinfo` (con cualquier XML bien formado dentro), su atributo `source` y el atributo `id` de cualquier construcción, como las anotaciones de JSON Schema de §2.1 (D23).
- Los atributos de un espacio de nombres distinto del de XSD (`xml:lang`, `xsi:schemaLocation`, `jaxb:version`…) en cualquier construcción (D34).
- Los valores por defecto sin efecto (D39): `attributeFormDefault="unqualified"`, `form` igual al vigente (`unqualified` en `xs:attribute`; en un `xs:element` local, el `elementFormDefault` del esquema), `nillable="false"`, `abstract="false"` y `mixed="false"`. Los otros valores (`qualified`, `form` distinto del vigente, `true`) se rechazan como fuera del subconjunto (§3.3). Como en XSD 1.0 (*src-element* 2.2), un `xs:element` con `ref` no admite `nillable` ni `form`, aunque lleven el valor sin efecto.
- Los `xs:attribute` globales: se comprueban como cualquier declaración, pero no se pueden usar (no hay `ref` de atributo) y no llegan a la IR (D40).

**Desviaciones de XSD 1.0.** Las tres primeras y las dos lecturas conservadoras de los tipos simples (primera viñeta de la 4) y D24 solo **rechazan** esquemas que XSD admite. Las dos últimas viñetas de la 4 **aceptan** esquemas que XSD rechaza, sin cambiar la validación de los documentos: una restricción es la conjunción de sus facetas con las de la base, así que el valor que no se comprueba nunca hace válido un documento que la base rechace.

1. ***Unique Particle Attribution* conservadora (D25 y su enmienda).** El determinismo de cada modelo de contenido se comprueba sobre la abstracción en la que toda partícula con `maxOccurs` > 1 es `+` o `*`, `maxOccurs="0"` cuenta como `?` (XSD 1.0 la trata como si no existiera) y un `xs:choice` vacío como ε. Además, ninguna partícula puede volver a empezar por dos vueltas que cambien de forma distinta un contador (`minOccurs` ≥ 2 o `maxOccurs` acotado > 1), para que el autómata con contadores sea determinista. Se rechazan modelos que XSD admite: `a{2,2}, a`, `a{0,0}, a` y `(a{1,2}){2,2}` (un `xs:sequence` con `minOccurs="2" maxOccurs="2"` alrededor de un elemento `a` con `maxOccurs="2"`). El error lleva la posición de las partículas implicadas: las dos en conflicto en la UPA, y en una vuelta con contador la del nodo que vuelve a empezar, la que se repite y la del contador ([fase3/xsd.md](fase3/xsd.md) §11).
2. **Nombres locales en *Element Declarations Consistent* y en la UPA (D40, D25).** XSD 1.0 compara el nombre expandido {name, target namespace}; aquí, el nombre local, como en el resto del validador (§1.2). Con `targetNamespace` y `elementFormDefault="unqualified"`, un elemento local `a` y un `ref` al global `a` en el mismo modelo, de tipos distintos o en posiciones que compiten, son válidos en XSD y aquí son error.
3. **Tope de partículas por modelo (D26):** §4.
4. **Tipos simples (D44, D24, D29)** ([fase3/xsd.md](fase3/xsd.md) §10.2, §10.6):
   - (rechaza de más) un `minExclusive` nuevo tiene que ser **menor** que el `maxInclusive` de la base (XSD admite igual), y `minExclusive` y `maxExclusive` del mismo paso, `minExclusive` < `maxExclusive` (XSD admite igual): los dos casos tienen un espacio de valores vacío;
   - (rechaza de más) `INF`, `-INF`, `NaN` y los valores que desbordan al redondear a binary32/binary64 (`1e39` en `xs:float`) se rechazan en las facetas y enumeraciones de `xs:float`/`xs:double` (D24);
   - (acepta de más) el valor de una faceta de rango tiene que tener la forma léxica del tipo y estar dentro de las cotas de la base, pero **no** se compara con la enumeración de la base, que XSD incluye en su espacio de valores;
   - (acepta de más; omisión conocida, D29) no se comprueba que las enumeraciones, `default` y `fixed` cumplan las facetas `pattern` de la base (sin un motor de expresiones regulares en el frontal; queda para la compilación de `pattern` a AFD).

Las comprobaciones de los tipos simples que siguen a XSD 1.0 al pie de la letra están en [fase3/xsd.md](fase3/xsd.md) §10.

---

## 3bis. DTD (subconjunto de XML 1.0 §3; D86)

Una DTD **externa**, dada como archivo de esquema aparte (como un `.xsd`), se analiza con un frontal propio
(`crates/dtd`, AST propio, D3) y baja a la **misma IR** que JSON Schema y XSD, con `Format::Xml` sin
`targetNamespace`. No hay campos nuevos en la IR ni en el JSON v2 (D17).

El formato se detecta por contenido (D22 enmendada por D86, enmienda E1). Tras el BOM, los espacios, una
`<?xml …?>` inicial y los comentarios `<!-- … -->`:

- `<!DOCTYPE` (en cualquier caja) es **XSD**, para que el frontal XSD lo rechace como hoy (D8);
- cualquier otro `<!` es **DTD**;
- otro `<` es XSD;
- lo demás, o nada, es JSON.

Si la `<?xml` o un comentario no se cierran, es XSD y el error lo da el lexer XML en su posición. El coste es
O(prólogo), sin recursión ni reservas. Lo hace una sola función en Rust, `dtd::formato`, que usan `schemac` y el
servidor; la web tiene su copia en TypeScript, solo para la etiqueta «Formato detectado», y las dos se prueban con la
misma tabla de casos (`tests/schemas/formato.json`; enmienda E2). El campo `format` de la API sigue siendo el de la
IR: `"xml"` para XSD y para DTD.

Como no hay `<!DOCTYPE` en el payload, no hay raíz declarada: el documento puede tener como raíz **cualquier
elemento declarado**, igual que con los elementos globales de XSD. Toda `<!ELEMENT>` es global.

### 3bis.1 Construcciones soportadas

| Construcción | Soporte | Cat. |
|---|---|---|
| `<!ELEMENT n EMPTY>` | Sin hijos ni texto. | (a) |
| `<!ELEMENT n (#PCDATA)>` | Solo texto (`xs:string`). | (a) |
| `<!ELEMENT n (modelo)>` | Hijos con `,` `\|` y los sufijos `?` `*` `+`, anidables. Tiene que ser determinista (XML 1.0, apéndice E); si no lo es, se rechaza con la posición de las dos partículas en conflicto. | (a) |
| `<!ATTLIST n a CDATA …>` | Texto libre. | (a) |
| `<!ATTLIST n a (v1\|v2\|…) …>` | Enumeración. | (a) |
| `#REQUIRED` / `#IMPLIED` | Requerido (máscara de bits) / opcional. | (b) |
| `"valor"` / `#FIXED "valor"` | Valor por defecto (el validador no lo inserta) / valor fijo. El valor se comprueba contra la enumeración al compilar. | (a) |
| Varias `<!ATTLIST>` del mismo elemento | Se unen. | — |
| Comentarios, instrucciones de procesamiento y una declaración `<?xml … encoding="UTF-8"?>` al inicio | Se aceptan y se ignoran. | — |

### 3bis.2 Fuera de alcance (error con posición y motivo)

`ANY` (equivale a `xs:any` con contenido mixto), contenido mixto `(#PCDATA|a|b)*` (§1.2), atributos `ID`,
`IDREF`, `IDREFS` (unicidad y referencias globales, como `xs:key`/`xs:keyref`), `ENTITY`, `ENTITIES`, `NOTATION`
y `<!NOTATION>`, `NMTOKEN`/`NMTOKENS` (primera versión), y nombres con `:`.

### 3bis.3 Prohibido por seguridad (el espíritu de D8)

Son errores léxicos, en el `<!` de la declaración o en el `%`/`&` de la referencia (enmienda E4):

- `<!ENTITY …>`, tanto generales como de parámetro;
- `<!DOCTYPE` dentro del archivo DTD;
- las secciones condicionales `<![INCLUDE[`/`<![IGNORE[`;
- una referencia de parámetro `%nombre;` fuera de un literal (dentro de un valor entre comillas, `%` es un carácter
  más, como en el `AttValue` de XML);
- dentro de un literal, `&nombre;` que no sea una de las cinco predefinidas ni una referencia de carácter.

Los identificadores externos (`SYSTEM`, `PUBLIC`) solo pueden ir dentro de esas declaraciones (o de `<!NOTATION`,
§3bis.2), así que quedan excluidos sin prohibir las palabras: `SYSTEM` y `PUBLIC` son nombres válidos de elemento y
de atributo. Nada abre archivos ni red y nada se expande: la IR sigue siendo lineal en el tamaño del esquema.

### 3bis.4 Desviaciones de XML 1.0 (todas rechazan de más)

1. Un nombre usado en un modelo de contenido o en una `<!ATTLIST>` sin su `<!ELEMENT>` es error `unresolved` (en
   XML es una advertencia).
2. Un atributo declarado dos veces para el mismo elemento es error (en XML gana la primera declaración).
3. Los valores de un atributo enumerado se comparan **sin** la normalización de XML §3.3.3 (quitar y colapsar
   espacios), porque `whiteSpace` está fuera de alcance en la IR: `a=" x "` se rechaza aunque XML lo acepta.

---

## 4. Límites por defecto

Todos son configurables en tiempo de compilación o en `sv_limits_t` (T5.2, T6.4). Superar un límite es un error controlado, distinto de un error de esquema.

| Límite | Valor por defecto |
|---|---|
| Profundidad máxima de anidamiento | 256 |
| Tamaño máximo del payload | sin límite (streaming) |
| Longitud máxima de una cadena | 1 MiB |
| Longitud máxima del lexema de un número | 1 024 bytes |
| Claves no declaradas por objeto (detección de duplicados) | 1 024, y 4 096 entre todos los objetos abiertos (F4-U16); lo mismo para los atributos no declarados de una etiqueta XML |
| Elementos para `uniqueItems` | 4 096 |
| Estados en la determinización acotada (T3.7) | 4 096 |
| Posiciones de un `pattern` tras expandir `{m,n}` / grupos anidados | 4 096 / 256 (F4-U16) |
| Partículas por modelo de contenido XSD (partícula = elemento o grupo, la raíz incluida; D26) | 4 096. Pasarlo es un error `limit` en la partícula 4 097. Lo mismo en los modelos de una DTD (§3bis) |
| Paréntesis anidados en un modelo de contenido DTD | 256, como la profundidad de anidamiento (D7) |
| Referencias de entidad/carácter XML por documento | 100 000 |
| Tiempo máximo de validación | sin límite en la librería; 10 s en la aplicación web |
| Subida máxima del servidor (cuerpo de cada petición de la aplicación web) | 1 MiB (1 048 576 bytes, `MAX_SCHEMA_BYTES`). Pasarla es 413 `too-large` ([fase7/servidor.md](fase7/servidor.md) §3; D82) |
| Tamaño máximo del esquema que lee `schemac` (`check`, `ir`) | 16 MiB (16 777 216 bytes); solo archivos regulares. Pasarlo es un error de entrada, código 2 ([fase3/bajada.md](fase3/bajada.md) §5) |

---

## 5. Pendiente de decidir en la Fase 3

Nada. Los tres puntos que quedaban se resolvieron con el diseño de la IR común ([fase3/ir.md](fase3/ir.md) §6): `not` sobre (a)/(b) es complemento real (inversión del veredicto al cerrar la instancia); `additionalProperties` sigue la semántica de 2020-12 y no ve las `properties` de las ramas de `allOf`; y `xs:sequence` y los objetos JSON son reglas distintas de la IR, que no se traducen entre sí.
