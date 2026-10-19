# Alcance: subconjunto soportado de JSON Schema y XSD

> Versión inicial (T0.6). Se refina en la Fase 3 (T3.5, T3.10) y se consolida en la documentación técnica (T9.1).

Este documento fija la **lista blanca** de palabras clave y construcciones que acepta `schemac`. Todo lo que no aparece aquí está fuera de alcance: en **modo estricto** el compilador lo rechaza con error; en modo normal lo ignora y emite una advertencia (T3.9).

La columna **Cat.** indica la categoría de reconocibilidad:

- **(a)** directa en el DPDA: se traduce a transiciones o guardas sobre un token.
- **(b)** con registros o contadores acotados en el marco de pila (máscara de bits, contador).
- **(c)** no determinista o no libre de contexto: se resuelve con producto de autómatas, determinización acotada o verificación diferida (T3.6–T3.8).

---

## 1. Payloads aceptados

### 1.1 JSON

- Sintaxis de **RFC 8259**. Codificación **solo UTF-8**; un BOM inicial se ignora. UTF-8 inválido (incluidos *overlongs* y sustitutos U+D800–U+DFFF) es error léxico.
- Un único valor de nivel superior de cualquier tipo.
- **Claves duplicadas** en un objeto: error de payload malformado. Se detectan exactamente para las claves declaradas en `properties` (máscara de bits); para claves no declaradas se usa una tabla hash acotada por `max_claves_adicionales` y, si se excede, se rechaza el payload por límite.
- Los números se comparan sobre el **lexema decimal exacto**, no sobre un `f64`; así `minimum`, `maximum` y `multipleOf` no pierden precisión. Un número es `integer` si su parte fraccionaria es cero (`1.0` es entero, como en 2020-12).
- `minLength`/`maxLength` cuentan **puntos de código** Unicode tras decodificar los escapes, no bytes.

### 1.2 XML

- XML 1.0 bien formado, **solo UTF-8**. Se acepta la declaración `<?xml version="1.0" encoding="UTF-8"?>`.
- **Prohibido:** `<!DOCTYPE>`, DTD interna o externa, entidades generales o de parámetro declaradas por el usuario (mitigación de XXE y *billion laughs*). Un `<!DOCTYPE` es error léxico.
- Se aceptan las cinco entidades predefinidas (`&lt; &gt; &amp; &quot; &apos;`) y referencias de carácter `&#N;` / `&#xN;`.
- Se aceptan comentarios y secciones CDATA. Otras instrucciones de procesamiento se ignoran.
- Espacios de nombres: un único `targetNamespace` (o ninguno). Las declaraciones `xmlns` se aceptan; los elementos se comparan por nombre local dentro de ese espacio de nombres.
- El texto formado solo por espacios entre elementos se ignora en contenido de solo elementos. No se admite contenido mixto.

---

## 2. JSON Schema (subconjunto del Draft 2020-12)

### 2.1 Núcleo y referencias

| Palabra clave | Soporte | Cat. |
|---|---|---|
| `$schema` | Solo `https://json-schema.org/draft/2020-12/schema` (o ausente). Otro valor → error. | — |
| `$id` | Solo en la raíz, como URI base. No se descargan recursos remotos. | — |
| `$defs` (y `definitions` como alias) | Tabla de símbolos de subesquemas. | — |
| `$ref` | Solo referencias **locales**: `#`, `#/$defs/x`, JSON Pointer dentro del documento, `#ancla`. Recursión permitida si es productiva (T3.4). | (a) |
| `$anchor` | Anclas locales. | — |
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
| `pattern` | Subconjunto de ECMA-262 compilado a AFD **sin backtracking**: literales, clases `[...]`, `.`, `\d \w \s`, anclas `^ $`, alternancia, grupos, cuantificadores `* + ? {m,n}`. **Rechazados:** retrorreferencias, *lookahead*/*lookbehind*, cuantificadores perezosos o posesivos (mitigación de ReDoS). | (c) |
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
| `not` | Complemento de un subautómata determinista; si el subesquema es de categoría (c), verificación diferida. | (c) |

### 2.8 Fuera de alcance

`if`/`then`/`else`, `contains`/`minContains`/`maxContains`, `patternProperties`, `propertyNames`, `unevaluatedProperties`, `unevaluatedItems`, `$dynamicRef`, `$dynamicAnchor`, `$vocabulary`, `contentEncoding`, `contentMediaType`, `contentSchema`, `$ref` remotos, validación de `format`, la forma booleana de `exclusiveMinimum`/`exclusiveMaximum` y la forma de arreglo de `items` (Draft 2019-09 y anteriores).

Candidatos a añadirse si sobra tiempo: `contains` (contador + producto), `patternProperties` y `propertyNames` (AFD sobre las claves).

---

## 3. XSD (subconjunto de XML Schema 1.0)

Las construcciones XSD se traducen al **mismo AST intermedio** que JSON Schema (T2.8). La regla *Unique Particle Attribution* de XSD obliga a que los modelos de contenido sean expresiones regulares deterministas, lo que encaja directamente con la construcción de un AFD por modelo de contenido.

### 3.1 Construcciones soportadas

| Construcción | Soporte | Cat. |
|---|---|---|
| `xs:schema` | Atributos `targetNamespace`, `elementFormDefault`. | — |
| `xs:element` | Global o local; `name`, `type`, `ref`, `minOccurs`, `maxOccurs` (entero o `unbounded`). | (a) |
| `xs:complexType` | Con nombre o anónimo. | (a) |
| `xs:sequence` | Concatenación ordenada de partículas. | (a) |
| `xs:choice` | Unión de partículas. | (a) |
| `xs:all` | Partículas en cualquier orden, cada una a lo sumo una vez: máscara de bits. | (b) |
| `minOccurs`, `maxOccurs` | Contador acotado en el marco de pila (no se expande el modelo). | (b) |
| `xs:attribute` | `name`, `type`, `use` (`required`/`optional`), `default`, `fixed`. Los requeridos usan máscara de bits. | (b) |
| `xs:simpleContent` + `xs:extension` | Texto de tipo simple más atributos. | (a) |
| `xs:simpleType` + `xs:restriction` | Facetas: `enumeration`, `minInclusive`, `maxInclusive`, `minExclusive`, `maxExclusive`, `length`, `minLength`, `maxLength`. | (a); facetas de longitud (b) |
| Faceta `pattern` | Igual que `pattern` de JSON Schema (sintaxis de regex XSD equivalente al subconjunto de §2.4). | (c) |

### 3.2 Tipos predefinidos

`xs:string`, `xs:boolean`, `xs:decimal`, `xs:integer`, `xs:long`, `xs:int`, `xs:short`, `xs:byte`, `xs:nonNegativeInteger`, `xs:positiveInteger`, `xs:double`, `xs:float`.

### 3.3 Fuera de alcance

`xs:import`, `xs:include`, `xs:redefine`, `xs:any`, `xs:anyAttribute`, `xs:group`, `xs:attributeGroup`, `substitutionGroup`, `abstract`, `nillable`/`xsi:nil`, `xsi:type`, contenido mixto (`mixed="true"`), `xs:list`, `xs:union`, `xs:complexContent` (extensión/restricción de tipos complejos), restricciones de identidad (`xs:key`, `xs:keyref`, `xs:unique`, no libres de contexto), facetas `totalDigits`/`fractionDigits`/`whiteSpace`, y tipos de fecha y hora.

---

## 4. Límites por defecto

Todos son configurables en tiempo de compilación o en `sv_limits_t` (T5.2, T6.4). Superar un límite es un error controlado, distinto de un error de esquema.

| Límite | Valor por defecto |
|---|---|
| Profundidad máxima de anidamiento | 256 |
| Tamaño máximo del payload | sin límite (streaming) |
| Longitud máxima de una cadena | 1 MiB |
| Longitud máxima del lexema de un número | 1 024 bytes |
| Claves no declaradas por objeto (detección de duplicados) | 1 024 |
| Elementos para `uniqueItems` | 4 096 |
| Estados en la determinización acotada (T3.7) | 4 096 |
| Referencias de entidad/carácter XML por documento | 100 000 |
| Tiempo máximo de validación | sin límite en la librería; 10 s en la aplicación web |

---

## 5. Pendiente de decidir en la Fase 3

- Si `not` sobre subesquemas de categoría (a)/(b) se implementa como complemento real del autómata o siempre como verificación diferida.
- Semántica exacta de `additionalProperties` combinado con `allOf` (en 2020-12 no "ve" las `properties` de las ramas).
- Correspondencia exacta entre `xs:sequence` (orden fijo) y los objetos JSON (sin orden) en el AST común.
