# Fase 2 — Lector XML de documentos de esquema (frontal XSD)

Tarea 5.1. Primera pieza del frontal XSD (`crates/xsd`, decisión D21): convierte los eventos del tokenizador XML de la Fase 1 en los **tokens** de la gramática de [gramatica-xml.md](gramatica-xml.md), comprueba la buena formación que esa gramática no expresa y resuelve los **espacios de nombres**. La gramática LL(1) del subconjunto XSD (tarea 5.2) leerá estos tokens, igual que el parser de JSON Schema lee los de `schema-parser/src/token.rs`.

Implementación: [`reader.rs`](../../crates/xsd/src/reader.rs) (`Reader`) y [`error.rs`](../../crates/xsd/src/error.rs) (`XsdError`). Pruebas: [`crates/xsd/tests/reader.rs`](../../crates/xsd/tests/reader.rs).

Es código de **tiempo de compilación**: reserva memoria (cola de tokens, cadenas, pilas). No forma parte del validador generado, que sigue sin asignación dinámica.

## 1. Tokens

`Reader::next_token` entrega un `Token` (`tok` y `span`) por llamada:

| Terminal | `Tok` | Eventos del lexer | Rango (`span`) |
|---|---|---|---|
| `STAG` | `STag(Name)` | `StartTagOpen` `NameChar`⁺ | de `<` al fin del nombre |
| `ATTR` | `Attr(Attr)` | `AttrBegin` `NameChar`⁺ `AttrValueBegin` `AttrValueChar`* `AttrValueEnd` | del nombre a la comilla de cierre |
| `>` | `Gt` | `StartTagClose` | el byte `>` |
| `/>` | `EmptyClose` | `EmptyTagClose` | los bytes `/>` |
| `ETAG` | `ETag(Name)` | `EndTagOpen` `NameChar`⁺ `EndTagClose` | `</nombre>` entero |
| `TEXT` | `Text(String)` | `TextChar`⁺ máximo | del primer al último carácter |
| `PI` | `Pi { target, data }` | `PiOpen` `NameChar`⁺ `PiChar`* `PiClose` | `<?…?>` entero |
| `$` | `Eof` | fin de la entrada | vacío, en la posición final |

- **Nombres.** `Name` da el QName escrito (`as_str()`: `xsd:element`), el URI de su espacio de nombres (`ns()`), su **identidad internada** (`ns_id()`, un `NsId`; §3), el prefijo (`prefix()`) y el nombre local (`local()`). `expanded()` devuelve el par (URI, nombre local) como texto, para mostrarlo o compararlo una vez, e `is(XS_NS, "element")` lo compara con una constante. **La clave de una tabla de símbolos es `(ns_id(), local())`, no `expanded()`**: con el texto del URI como clave, cada consulta vuelve a leer el URI entero, y un URI de varios MiB con muchas referencias da un costo O(referencias · |URI|) (el mismo error de la ronda 1, §5). Todos los campos de `Name` son privados: el texto escrito y el inicio del nombre local forman un invariante que `local()` y `prefix()` necesitan, y solo el lector crea identidades internadas. La igualdad `==` de dos `Name` compara el texto y el URI por contenido, así que vale entre lectores distintos. El `ETAG` lleva el mismo `Name` que su `STAG`.
- **Atributos.** `Attr` lleva el nombre resuelto (`name`), el valor (`value()`, o `into_value()` para quedárselo), el rango del valor con sus comillas (`value_span`) y el valor **ya resuelto como QName** contra el ámbito de su etiqueta, que se consulta con `Attr::as_qname()` (§3.3). El valor es de solo lectura porque esa resolución guarda rangos dentro de él: para normalizarlo (*collapse*), el parser trabaja sobre una copia. Las declaraciones `xmlns` y `xmlns:p` también son `ATTR`, con `ns()` = `http://www.w3.org/2000/xmlns/` (`XMLNS_NS`, la convención de DOM) y `Attr::is_namespace_decl()`: el parser las reconoce por su URI y las salta.
- **Texto.** Los comentarios no producen eventos, así que `a<!--x-->b` es un solo `TEXT` `ab`. CDATA y referencias llegan ya resueltas. Los textos de solo espacios también se entregan; el parser decide si los ignora (`00-alcance.md` §1.2).
- **Valores normalizados como en XML 1.0.** El lector aplica dos normalizaciones del propio XML que el lexer no hace: los fines de línea (§2.11: `\r\n` y `\r` literales pasan a `\n`, en texto, valores y contenido de las PI; la declaración `<?xml …?>` se entrega sin normalizar, §2.2) y la de los valores de atributo CDATA (§3.3.3: cada `\t`, `\n` o `\r` **literal** pasa a un espacio). Sin DTD, todo atributo es CDATA. Un carácter de una referencia (`&#9;`, `&#13;`) se conserva: el lector lo distingue porque su rango mide más de un byte. La normalización según el tipo XSD (`whiteSpace` *replace*/*collapse*) **no** se hace aquí (§8).

**Etiqueta de apertura como unidad.** Las declaraciones `xmlns` de una etiqueta pueden ir *después* del nombre que las usa (`<p:a xmlns:p="u">`), así que el nombre del `STAG` no se puede resolver hasta leer el `>`. El lector acumula la etiqueta entera y, al leer su `>` o `/>`, la comprueba y entrega `STAG ATTR* (> | />)` de una vez. La memoria que usa es la de una etiqueta.

## 2. Restricciones de buena formación que añade el lector

La gramática de [gramatica-xml.md](gramatica-xml.md) §2 genera un superconjunto de los documentos bien formados: no exige que el nombre de cierre sea el de apertura (§3 de ese documento demuestra que **ninguna** GLC puede exigirlo sobre nombres arbitrarios) y deja fuera tres restricciones más. El lector las comprueba todas, y además las de Namespaces in XML 1.0:

| Restricción | Fuente | Cómo se comprueba | Error (`code`) |
|---|---|---|---|
| El `ETAG` cierra el último elemento abierto y con el mismo nombre escrito | XML 1.0 WFC *Element Type Match* | Pila explícita de elementos abiertos | `MismatchedEndTag`, `UnopenedEndTag` (`syntax`) |
| Un solo elemento raíz, cerrado al final | XML 1.0 [1] `document` | `root_seen` y la pila vacía al final | `MultipleRootElements`, `UnclosedElement`, `NoRootElement` (`syntax`) |
| Fuera de la raíz, solo espacios literales (`S`) | XML 1.0 [27] `Misc` | Cada carácter de texto con la pila vacía | `TextOutsideRoot` (`syntax`) |
| Atributos únicos por etiqueta (nombre escrito) | XML 1.0 WFC *Unique Att Spec* | Conjunto *hash* por etiqueta | `DuplicateAttribute` (`duplicate-attribute`) |
| Atributos únicos por nombre expandido (`p:x` y `q:x` con el mismo URI) | Namespaces 1.0 §6.3 | Conjunto *hash* de (`NsId`, local): la identidad internada del URI (§3), no su texto | `DuplicateAttribute` (`duplicate-attribute`) |
| `<?xml …?>` solo como lo primero del documento | XML 1.0 [22] `prolog` | Línea 1, columna 1 (§2.1) | `XmlDeclNotFirst` (`syntax`) |
| `<?xml …?>` bien formada y en UTF-8 | XML 1.0 [23]-[32]; `00-alcance.md` §1.2 | §2.2 | `BadXmlDecl`, `UnsupportedEncoding` (`syntax`) |
| Ninguna otra PI se llama `[Xx][Mm][Ll]` | XML 1.0 [17] `PITarget` | Comparación sin mayúsculas | `ReservedPiTarget` (`syntax`) |
| Nombres de elemento y atributo son QName | Namespaces 1.0 §3 | §3.1 | `InvalidQName` (`namespace`) |
| Todo prefijo está declarado en el ámbito | Namespaces 1.0 NSC *Prefix Declared* | Pila de ámbitos (§3) | `UndeclaredPrefix` (`namespace`) |
| Reglas de `xml` y `xmlns` | Namespaces 1.0 NSC *Reserved Prefixes and Namespace Names*, *No Prefix Undeclaring* | §3.2 | `ReservedNamespace` (`namespace`) |

`<!DOCTYPE` no llega al lector: es el error léxico `DoctypeForbidden` del lexer (D8), que el lector entrega como `ErrorKind::Lex` con el código `lexical` y la posición de la `D` (offset 24 en `malformed-xml-billion-laughs-01`).

Que el `ETAG` coincida y que haya un solo elemento raíz ya los exigiría el parser de la 5.2 con la gramática (lo segundo) o no podría exigirlos (lo primero). Que los compruebe el lector hace que **toda sucesión de tokens que termina en `Eof` sea un documento bien formado**: el parser de la 5.2 puede contar con ello.

### 2.1 La declaración va primero

La declaración debe ser lo primero del documento, sin nada delante salvo un BOM (que no es parte del documento). Los comentarios no producen eventos, así que «primer token» no basta: `<!-- c --><?xml …?>` tendría la declaración como primer token. El lector usa la **posición**: la declaración está bien colocada si y solo si su `<` está en la línea 1, columna 1. Es correcto porque cada byte que no es de continuación UTF-8 suma una columna (`Pos::advance`), y el único carácter que el lexer descarta sin columna es el BOM inicial; así, línea 1 y columna 1 equivalen a offset 0, u offset 3 tras un BOM. El nombre de una PI se comprueba **en cuanto está completo** (con el primer carácter del contenido, o al cerrar si no tiene): una `<?xml` tardía es `XmlDeclNotFirst` sin esperar al `?>`, aunque nunca se cierre, y lo mismo `ReservedPiTarget`.

### 2.2 Forma de la declaración

`version` es obligatoria y va primero; `encoding` y `standalone` son opcionales y van en ese orden, separados por espacios; `=` admite espacios a los lados y los valores van entre comillas iguales (XML 1.0 [23]-[32]). `version` es `1.` seguido de dígitos (la producción `VersionNum`); `encoding` debe ser `UTF-8` sin distinguir mayúsculas (`00-alcance.md` §1.2); `standalone` es `yes` o `no`. Un error de forma o de codificación va en la posición exacta del carácter (`encoding="latin1"` en la columna de la `l`); lo que falta al final (`<?xml?>`), en el `<` de la declaración.

El contenido de la declaración se guarda con los bytes de la entrada (en una PI no hay referencias, y aquí no se normalizan los fines de línea) y se analiza por bytes, porque su sintaxis es ASCII. Solo se guarda la posición de su primer carácter: la de un error se reconstruye avanzando una copia de ella con `Pos::advance` sobre los bytes anteriores, una sola vez. Así la memoria de una declaración es la de su texto, no una posición de 16 bytes por carácter.

## 3. Espacios de nombres

**Definiciones** (Namespaces in XML 1.0). Un *QName* es `NCName` o `NCName:NCName`, donde un `NCName` es un nombre XML sin `:`. Una declaración `xmlns:p="u"` liga el prefijo `p` al URI `u`; `xmlns="u"` fija el espacio por defecto y `xmlns=""` lo quita. El *ámbito* de una declaración es el elemento donde aparece, con sus atributos y su contenido, salvo donde la tape una declaración interna del mismo prefijo. El prefijo `xml` está ligado siempre a `http://www.w3.org/XML/1998/namespace`. El *nombre expandido* de un QName es (URI, nombre local):

- **elemento** con prefijo `p`: el URI de `p` en el ámbito; sin prefijo, el espacio por defecto (o ninguno);
- **atributo** con prefijo `p`: el URI de `p`; sin prefijo, **ninguno** (el espacio por defecto no se aplica a los atributos);
- **valor** de un atributo de tipo `xs:QName` (`type`, `base`, `ref`, tarea 5.3): como un elemento, con el espacio por defecto (XSD 1.0, *QName resolution (Schema Document)*). Por eso `<xs:schema xmlns="http://www.w3.org/2001/XMLSchema">` puede escribir `type="string"`.

**Implementación.** Una tabla `prefijo → pila de URI` (la cadena vacía es el espacio por defecto) y un registro `undo` con los prefijos ligados en orden. Al abrir un elemento se guarda la marca `undo.len()` y se ligan sus declaraciones; al cerrarlo se deshace hasta su marca. Es una pila de ámbitos, sin recursión. Los URI se **internan** al ligarlos: un conjunto guarda un único `Arc<str>` por URI distinto, así que dos declaraciones del mismo URI comparten el mismo `Arc`. `NsId` envuelve ese `Arc` y se compara y se *hashea* por su dirección: O(1), sea cual sea la longitud del URI. Dentro del lector es la clave de los nombres expandidos repetidos. Fuera, `Name::ns_id()` la ofrece a las fases siguientes, y `Reader::intern(uri)` interna en la **misma tabla** un URI que llega como valor (el `targetNamespace` de `xs:schema`), de modo que su `NsId` coincide con el de los nombres del documento con ese URI.

**`targetNamespace` lo interna la 5.2, no la 5.3.** `Reader::intern` necesita el lector vivo (`&mut self`), y cuando empiece la 5.3 el lector ya se habrá descartado. Además `targetNamespace` es `xs:anyURI` (`whiteSpace` *collapse*), mientras que los URI de `xmlns:p` se internan tal cual: con `xmlns:t='urn:t' targetNamespace=' urn:t '`, internar el valor sin recortar da un `NsId` distinto del de `type='t:T'`; recortado primero, coincide. Por eso el parser de la 5.2 recorta (*collapse*) el `targetNamespace`, lo interna con `Reader::intern` mientras el lector existe y guarda el `Option<NsId>` en la raíz del AST; la 5.3 lo usa como clave con `(ns_id(), local())` ([pendientes.md](pendientes.md) §11, 5.1-CR3-I1).

**Alcance de la identidad.** Solo es comparable entre identidades del **mismo** `Reader`: las de dos lectores distintos son siempre distintas, aunque el URI sea el mismo. Un `NsId` mantiene vivo su `Arc`, así que su dirección no se reutiliza mientras exista. El O(1) depende de este internado: un URI que no pase por el lector (una `String` cualquiera) no tiene `NsId`, y compararlo o *hashearlo* cuesta O(|URI|).

**Corrección.** Invariante: para cada prefijo, la cima de su pila es la declaración más interna de ese prefijo entre los elementos abiertos (o la del prefijo `xml`), y la pila está vacía si ninguna lo declara. Abrir un elemento apila exactamente sus declaraciones; cerrarlo desapila exactamente esas (las que están por encima de su marca en `undo`, que son las suyas porque los elementos se cierran en orden inverso al de apertura, garantizado por la comprobación del `ETAG`). Así la cima es la definición de ámbito de arriba, y la resolución es la de la especificación.

### 3.1 Sintaxis de QName

Un nombre del lexer ya es un `Name` de XML; el lector exige además a lo sumo un `:`, no al principio ni al final, y que el nombre local empiece por una letra, `_` o un carácter no ASCII (no por dígito, `-` ni `.`). `<a:b:c/>`, `<:a/>` y `b:="1"` son `InvalidQName`. Con la misma simplificación que el lexer (`docs/fase1/lexico.md` §7), cualquier carácter no ASCII vale como letra.

### 3.2 Prefijos reservados

Son errores (`ReservedNamespace`): declarar el prefijo `xmlns`; ligar `xml` a otro URI que el suyo; ligar otro prefijo, o el espacio por defecto, al URI de `xml`; ligar cualquiera al URI de `xmlns`; `xmlns:p=""` (deshacer un prefijo solo existe en Namespaces 1.1); y un elemento con prefijo `xmlns`. Declarar `xmlns:xml` con su propio URI está permitido.

### 3.3 Resolución de los QName de los valores

Los valores de `type`, `base` y `ref` son QName que se resuelven con el ámbito de **su** etiqueta. Un parser LL(1) que lee por adelantado (como el de JSON Schema, cuyo `bump()` pide el token siguiente en cuanto consume uno) puede mirar el `type` de un elemento cuando el lector ya leyó la etiqueta del hijo, con sus propias declaraciones: `<xs:element type="p:T"><xs:complexType xmlns:p="otro">`. Si la resolución dependiera del ámbito vigente en el lector, `p:T` saldría ligado a `otro` sin ningún error.

Por eso el lector resuelve **todos** los valores de atributo al cerrar cada etiqueta, cuando el ámbito es exactamente el suyo, y guarda el resultado en el `Attr`; `Attr::as_qname()` lo devuelve. El resultado no depende de cuándo se pida ni de cuántos tokens haya leído el parser. El lector no sabe qué atributos son QName, así que el fallo solo es error si el parser lo pide: el valor se guarda como rangos dentro de `value` (o la marca «no es QName» / «prefijo sin declarar»), sin copias.

- Quita los espacios de los extremos (`xs:QName` tiene `whiteSpace` *collapse*).
- Sin prefijo toma el espacio por defecto (§3).
- Un valor que no es QName da `InvalidQName`, uno con un prefijo sin declarar `UndeclaredPrefix`, y uno con el prefijo `xmlns` `ReservedNamespace` (Namespaces in XML 1.0: `xmlns` está ligado a `http://www.w3.org/2000/xmlns/` y no se usa en QName). Todos van en el **primer carácter del valor**, el que sigue a la comilla de apertura (aunque sea un espacio que luego se quita).
- Costo: O(|valor|) por atributo, cobrado a sus propios bytes (§5).

El parser de la 5.2 debe guardar en el AST el `Name` de `type`, `base` y `ref` (`as_qname()`, una sola vez por atributo: cada llamada copia el QName) con su posición, y el `NsId` del `targetNamespace` ya recortado (§3); la 5.3 los buscará en sus tablas, con `(ns_id(), local())` como clave, cuando el lector y su ámbito ya no existan.

## 4. Correspondencia con la gramática XML

Sea G la gramática de [xml.ll1](xml.ll1). **Proposición:** si `next_token` entrega t₁ … tₖ `$` sin error, entonces t₁ … tₖ ∈ L(G), y la entrada es un documento XML bien formado (en el subconjunto de `00-alcance.md` §1.2) que cumple Namespaces in XML 1.0.

*Esbozo.* El lexer solo acepta etiquetas de apertura de la forma `STAG ATTR* (> | />)`, y el lector las entrega enteras y en ese orden, así que cada una deriva `element → STAG attrs element_rest` hasta `>` o `/>`. La pila de elementos abiertos empareja cada `>` con un único `ETAG` posterior y con el mismo nombre, y entre ambos solo hay `TEXT`, `PI` y elementos completos: eso es `content`. Fuera de la pila solo hay un elemento (el raíz, por `MultipleRootElements` y `NoRootElement`), texto de espacios y `PI`: eso es `misc element misc`. Las restricciones de §2 son, una a una, las condiciones de buena formación y de espacios de nombres que G no expresa. ∎

El recíproco (todo documento bien formado del subconjunto da sus tokens sin error) lo comprueban las pruebas con el esquema del corpus y con los prefijos `xs:`, `xsd:` y el espacio por defecto.

## 5. Costo

**Tiempo: O(n) en bytes** (esperado, por las tablas *hash*). Cada byte se entrega al lexer una vez, y el lexer hace O(1) por byte (Fase 1). Cada evento hace O(1) amortizado (añadir un carácter a una cadena, apilar). El trabajo no constante se **cobra a los bytes que lo provocan, y cada byte se cobra un número constante de veces**. La regla que lo garantiza: ninguna operación que se repite por atributo, por etiqueta o por uso de un prefijo lee un texto que pertenezca a otra parte de la entrada; solo lee sus propios bytes (un nombre, un prefijo, un valor) o compara con una constante.

| Operación | Costo | Se cobra a |
|---|---|---|
| Declaración `xmlns:p="u"`: internar `u` (un *hash* y, si es nuevo, una copia), comparar con los URI de `xml`/`xmlns`, ligar `p` | O(\|u\| + \|p\|); la comparación con una constante para en su longitud (36 o 29 bytes) | la propia declaración |
| Atributo: separar el QName, *hash* del nombre escrito, buscar su prefijo, *hash* de (identidad del URI, nombre local) | O(\|nombre\|) | el propio atributo |
| `STAG`: separar el QName, buscar su prefijo, copiar el `Name` para la pila | O(\|nombre\|) | el propio nombre |
| `ETAG`: comparar con el nombre abierto (`==` de `str` compara primero las longitudes) | O(\|nombre de cierre\|) | el propio `ETAG` |
| Copiar un URI en un `Name` o en un `ATTR`; comparar o *hashear* un `NsId` | O(1) (`Arc` y su dirección) | — |
| Deshacer un ámbito: desapilar cada ligadura (*hash* de su prefijo) | O(\|p\|) por ligadura, una vez | la declaración |
| Declaración `<?xml …?>` (a lo sumo tres pseudoatributos, una búsqueda de la comilla cada uno) | O(\|declaración\|) | la declaración |
| Error de una etiqueta (copia del nombre afectado) | O(\|nombre\|) | el nombre |
| Valor de atributo como QName (al cerrar la etiqueta): quitar espacios, separar, buscar el prefijo, copiar el `Arc` | O(\|v\|) | el propio valor |
| `Attr::as_qname()`: copiar el QName recortado (o el texto del error) | O(\|v\|) | quien llama |
| `Name::is(ns, local)`: comparar el URI con `ns` | O(\|ns\|) del argumento (una constante del parser, como `XS_NS`), no del URI del documento | quien llama |

Suma: O(n) para todo el documento, más O(|v|) por cada llamada a `as_qname`. Los URI internados viven hasta el fin del lector: O(n) de memoria.

**El error de la primera versión** (ronda 1 de pruebas): la clave del conjunto de nombres expandidos era (URI, nombre local) con el texto del URI, y cada atributo con prefijo volvía a calcular el *hash* del URI entero. Con un URI de 1 MiB y M atributos el costo era O(M · |URI|), cuadrático en la entrada (medido en release: 11 s con M = 20 000, 46 s con M = 80 000). Ahora la clave es (identidad del URI internado, nombre local). La prueba `uri_largo_y_muchos_atributos_con_prefijo` (URI de 1 MiB, 4·10⁴ atributos y un repetido con otro prefijo del mismo URI) tarda menos de un segundo en debug; con la clave antigua no termina en un minuto.

Las tablas son las de `std` (SipHash con clave aleatoria), así que una entrada no puede provocar colisiones a propósito. Los repetidos se detectan con conjuntos, no comparando cada par de atributos, y la búsqueda de un prefijo no recorre las declaraciones en vigor (la prueba `muchos_atributos_y_declaraciones` usa 5·10⁴ atributos y 5·10⁴ declaraciones en una etiqueta). El tiempo no se mide en `cargo test` (D1): las pruebas exigen el resultado correcto con tamaños para los que una versión cuadrática no terminaría en un tiempo razonable.

**Memoria: O(profundidad + atributos)**, medida en bytes de los nombres: la pila de elementos abiertos (sus nombres), las ligaduras en vigor, la etiqueta en curso (sus atributos), los URI internados y el texto o la PI en curso. La cola de tokens contiene a lo sumo los de un byte: los de una etiqueta (`STAG`, sus `ATTR` y su cierre) más el `TEXT` anterior. Todo es O(n), pero con una **constante grande**: cada atributo, elemento abierto o ligadura cuesta varias cabeceras de `String`, `Vec` y `Arc` y su entrada en las tablas *hash*, mucho más que los pocos bytes que ocupa en la entrada.

Medido en release (programa aparte, fuera del repositorio), con entradas de 16 MiB (el tope de `schemac`) y el consumidor descartando cada token; «pico extra» es el aumento de `VmHWM` durante la lectura:

| Entrada de 16,8 MB | Pico extra | Factor | Tiempo |
|---|---|---|---|
| Una etiqueta con 1,4 M atributos `aK='1'` | 521 MB | 31× | 0,5 s |
| Una etiqueta con 1,2 M atributos con prefijo `p:vK='1'` | 540 MB | 32× | 0,7 s |
| 5,6 M niveles `<a>` sin cerrar | 493 MB | 29× | 0,8 s |
| Una etiqueta con 0,7 M declaraciones `xmlns:pK='uK'` | 549 MB | 33× | 1,0 s |
| `<a xmlns='u'>` anidado sin cerrar | 170 MB | 10× | 1,1 s |
| `<?xml version='1.0' ` seguido de 16 MB sin cerrar | 17 MB | 1,0× | 0,2 s |
| Un texto de 16 MB | 17 MB | 1,0× | 0,2 s |

El tiempo es lineal en todos los casos. Antes de la ronda 2 de correcciones la declaración `<?xml` sin cerrar daba 18× (una `Pos` por carácter); ahora guarda solo su texto (§2.2). Las cifras son de la ronda 3 de correcciones. Las cuatro primeras filas son los peores casos: el lector **no tiene límites propios** (§8), así que la memoria por compilación la deben acotar el parser de la 5.2 (profundidad, D7) y el servidor de la Fase 7 ([pendientes.md](pendientes.md) §12).

**Sin recursión.** Elementos y ámbitos van en pilas explícitas (`Vec`), y ningún tipo es recursivo, así que tampoco lo es su liberación. 2·10⁵ niveles de anidamiento se leen en un hilo de prueba normal (`anidamiento_profundo_sin_recursion`). El tope de profundidad del parser (D7) lo pondrá la tarea 5.2.

## 6. Independencia del tamaño de bloque

`Reader::from_chunks` acepta la entrada como una sucesión de bloques de cualquier tamaño (también vacíos); `Reader::new` la toma en un solo bloque (partirla no aporta nada: el lector la recorre byte a byte de todos modos). El resultado es el mismo con cualquier partición:

1. el lexer de la Fase 1 emite los mismos eventos, con los mismos rangos, sea cual sea la partición (su propiedad de streaming, probada en la Fase 1);
2. el lector entrega al lexer **un byte cada vez**, sea cual sea el bloque, y decide cuándo parar (cola no vacía) solo a partir de los eventos;
3. el ensamblador es una función de la sucesión de eventos y de las posiciones del lexer (`position()` antes de cada byte, para el `>` y el `/>`), que tampoco dependen de la partición.

Lo comprueban tres pruebas de propiedades: bloques de 1, 7 y 4096 bytes contra la entrada entera, cortes arbitrarios, y bytes arbitrarios (que además no deben provocar pánico). Como las entradas generadas casi nunca pasan de 4096 bytes, una prueba determinista (`documento_multibloque_con_fronteras_en_medio_de_un_nombre`) lee un esquema de unos 20 KB, con marcas que cruzan las fronteras de 4096, 8192, 12288 y 16384 bytes, en bloques de 4096, con `Reader::new` y entero, y compara los tokens uno a uno.

## 7. Errores

`XsdError { kind, at }`: el primero en el orden de la entrada. El mensaje es de una línea, con la posición delante y el texto del usuario recortado y escapado con `lexer::short` (D18):

```
línea 1, columna 6: la etiqueta de cierre </b> no corresponde a <a>
línea 1, columna 31: codificación "latin1" no soportada: solo se admite UTF-8
línea 2, columna 3: error léxico: <!DOCTYPE no está permitido (sin DTD ni entidades declaradas)
```

| `code()` | Clases |
|---|---|
| `lexical` | `Lex(LexErrorKind)`: errores del tokenizador XML, incluido `DoctypeForbidden` |
| `syntax` | buena formación: `MismatchedEndTag`, `UnopenedEndTag`, `UnclosedElement`, `NoRootElement`, `MultipleRootElements`, `TextOutsideRoot`, `XmlDeclNotFirst`, `ReservedPiTarget`, `BadXmlDecl`, `UnsupportedEncoding` |
| `duplicate-attribute` | `DuplicateAttribute` |
| `namespace` | `InvalidQName`, `UndeclaredPrefix`, `ReservedNamespace` |

Las tareas 5.2-5.7 añadirán sus clases a `ErrorKind` (con códigos como `unsupported`, `unresolved`, `limit`…, decisión B2 del plan de la tarea).

**Posición de cada error.** El de un nombre (cierre distinto, prefijo sin declarar, QName inválido, atributo repetido), en el primer carácter de ese nombre: el del `ETAG` o del atributo, el segundo atributo repetido. Texto fuera de la raíz, en el carácter. Segundo elemento raíz, en su `<`. Elemento sin cerrar o sin raíz, en el fin de la entrada. Declaración XML, §2.2. QName de un valor (`as_qname`), en el primer carácter del valor (§3.3).

**Textos recortados.** Los nombres y valores que lleva un error (`MismatchedEndTag`, `DuplicateAttribute`, `InvalidQName`, `UndeclaredPrefix`…) se guardan recortados a `SHORT_MAX + 1` = 33 caracteres al crear el error. Es todo lo que muestra `lexer::short` en el mensaje (32 caracteres y `…`), así que el mensaje no cambia; a cambio, copiar el error, que el lector repite en cada llamada tras fallar, o pedir muchas veces `as_qname()` sobre un valor inválido de varios MiB, cuesta O(1) y no O(|valor|).

**Orden.** Una etiqueta de apertura se comprueba entera al leer su `>`: si tiene varios errores **de etiqueta** (nombres, atributos, espacios de nombres), se informa el de **menor offset** (por ejemplo, un prefijo sin declarar en el nombre antes que un atributo repetido). Un **error léxico** dentro de la etiqueta gana siempre, aunque esté más adelante: el lexer lo detecta antes de que llegue el `>` y la etiqueta nunca se comprueba. Así, `<a b='1' b='2' c=<` da `Lex(UnexpectedByte)` en la columna 18, no `DuplicateAttribute` en la 10. En el resto de casos el error es el primero de la entrada. Los tokens anteriores al error se entregan primero y después el error, que se repite en cada llamada siguiente; los tokens de la etiqueta con el error no se entregan.

## 8. Límites, simplificaciones y lo que queda para otras tareas

- **Sin límites propios.** La memoria y el tiempo son lineales en la entrada, y la entrada ya está acotada: `schemac` lee como mucho 16 MiB (`00-alcance.md` §4) y el servidor tendrá su tope (Fase 7). No hay error `limit` en el lector. La constante de la memoria es grande (hasta 33 veces la entrada, §5), así que los límites quedan como obligación de la 5.2 (profundidad, D7) y del servidor (atributos por etiqueta, declaraciones y tamaño, o `RLIMIT_AS`): [pendientes.md](pendientes.md) §12.
- **Nombres no ASCII.** Como el lexer, cualquier carácter no ASCII vale en un nombre (§3.1).
- **CDATA fuera de la raíz.** `<![CDATA[ ]]>` antes o después de la raíz no es `Misc` en XML 1.0, pero el lexer entrega su contenido como `TextChar` con rangos de un byte, igual que el texto literal, y el lector no lo distingue: una sección CDATA **de solo espacios** fuera de la raíz se acepta (con otro contenido, es `TextOutsideRoot`). Se anota en [pendientes.md](pendientes.md) §8.
- **Declaración XML sin normalizar.** El contenido de `<?xml …?>` se entrega con sus bytes (§2.2); el análisis trata `\r` como espacio. El de las demás PI sí se normaliza (§1).
- **Normalización por tipo XSD** (`whiteSpace` *replace*/*collapse* de los valores de atributo y del texto de `xs:enumeration`, `default`, `fixed`…): tareas 5.5 y 5.6; el valor que entrega el lector es el de XML 1.0 (§1). [pendientes.md](pendientes.md) §9.
