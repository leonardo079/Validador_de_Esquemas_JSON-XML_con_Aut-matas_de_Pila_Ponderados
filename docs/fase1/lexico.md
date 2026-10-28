# Fase 1 — Análisis léxico

Este documento define formalmente los tokens de JSON y XML como lenguajes regulares, da sus expresiones regulares y gramáticas regulares, y explica cómo se construyen los AFD que implementa el crate [`lexer`](../../crates/lexer). Los diagramas de esta carpeta se generan desde las mismas tablas que ejecuta el código (`make docs`). La CI (`make docs-check`) regenera los `.dot` y falla si difieren de los versionados, así que los `.dot` no pueden desincronizarse del código sin que la CI lo detecte. Los `.svg` se dibujan con Graphviz a partir de esos `.dot` y no se verifican en la CI, porque su salida cambia con la versión de Graphviz y las fuentes instaladas; tras cambiar un autómata hay que ejecutar `make docs` para actualizarlos.

| Autómata | Estados | Clases | Diagrama | Código |
|---|---|---|---|---|
| UTF-8 | 9 (1 de error) | 12 | [afd-utf8.svg](afd-utf8.svg) | [`utf8.rs`](../../crates/lexer/src/utf8.rs) |
| Tokenizador JSON | 32 (1 de error) | 31 | [afd-json.svg](afd-json.svg) | [`json.rs`](../../crates/lexer/src/json.rs) |
| Tokenizador XML | 42 (1 de error) | 29 | [afd-xml.svg](afd-xml.svg) | [`xml.rs`](../../crates/lexer/src/xml.rs) |

**Qué cifras se comprueban y cuáles son manuales.** Las cifras de estados y clases de esta tabla, la tabla de particiones de §1 y la frase «de 256 a 31 columnas en JSON y a 29 en XML» las comprueba `cargo test`: la prueba `particion_de_clases` de cada módulo las calcula desde las tablas y falla si el documento no contiene la fila exacta. Son **manuales**, y debe actualizarlas quien cambie el AFD correspondiente en el mismo cambio: las tablas de clases de §1 (bytes y papel de cada clase), las tablas de tamaños de §5 (medidas en x86-64; la prueba `memoria_constante` solo exige un tope) y las descripciones de estados y transiciones de §2–§4.

---

## 1. Alfabeto y clases de bytes

El alfabeto de entrada es **Σ = {0x00, …, 0xFF}**, los 256 valores de un byte. El validador consume bytes, no caracteres: así la decodificación UTF-8 también es parte del autómata y no un paso previo que reserve memoria.

Una tabla de transiciones sobre Σ tendría 256 columnas por estado. Para reducirla se agrupan los bytes en **clases**. La relación de partida es la equivalencia por δ: a ≡ b si δ(q, a) = δ(q, b) para todo estado q. Es una congruencia, y un AFD sobre sus clases reconoce exactamente el mismo lenguaje. Las clases que usa el código son un **refinamiento** de esa equivalencia: si a y b están en la misma clase, entonces a ≡ b, pero no al revés. Se separan además clases δ-equivalentes por dos motivos:

- **La salida de la máquina de Mealy.** Los tokenizadores no solo aceptan: en cada transición ejecutan una acción (saltar, emitir un token, entregar un carácter al decodificador UTF-8) y, si van al error, informan un tipo de error. El **tipo de acción** y el tipo de error deben ser función de (estado, clase); el **dato** que emite la acción puede leer el byte o un registro (qué token de puntuación, qué carácter, el valor de un `\uXXXX`). Dónde se traza esa línea es una decisión de modelado: con otra elección, las seis de puntuación de JSON contarían como separadas por la salida y no por legibilidad. Por eso en JSON ␠ va aparte de `{ } [ ] : ,` (se salta, ellos se emiten) y «otro ASCII» aparte de 80–FF (estos van al AFD UTF-8), y en XML `d` va aparte de `a–c e f B E F` (`<!d` es `DoctypeForbidden`; `<!a`, `UnexpectedByte`).
- **Legibilidad.** Algunas clases tienen el mismo δ y la misma acción, y van separadas solo para que la tabla y los diagramas se lean mejor: en JSON, las seis de puntuación `{ } [ ] : ,` (la acción lee el byte para elegir el token); en XML, ␠ frente a `\t \n \r`, que se nombran igual que en JSON. La línea y la columna no influyen: `Pos::advance` las calcula a partir del byte, no de la clase.

| Autómata | Clases declaradas | Distintas por δ | Distintas por δ y acción | Además por tipo de error |
|---|---|---|---|---|
| UTF-8 | 12 | 12 | — | — |
| JSON | 31 | 24 | 26 | 26 |
| XML | 29 | 27 | 27 | 28 |

Las cifras las calcula y fija la prueba `particion_de_clases` de cada módulo (`utf8.rs`, `json.rs`, `xml.rs`) a partir de las tablas. Con estas clases la tabla pasa de 256 a 31 columnas en JSON y a 29 en XML. El cálculo de la clase de un byte es un acceso a un arreglo `CLASS[256]`, así que no cuesta nada en tiempo de ejecución.

Clases del tokenizador JSON (`json::CLASS_NAMES`):

| Clase | Bytes | Clase | Bytes |
|---|---|---|---|
| ␠ | `0x20` | `0` | `0` |
| `\t \n \r` | espacio fuera de cadenas; prohibido dentro | `1–9` | `1`…`9` |
| `{ } [ ] : ,` | una clase por símbolo | `a b e f l n r s t u` | una clase por letra: aparecen en literales, escapes, exponente o hexadecimal |
| `"` `\` `/` | una clase por símbolo | `E` | exponente y hexadecimal |
| `-` `+` `.` | una clase por símbolo | `c d A–D F` | solo dígitos hexadecimales |
| control | `0x00–0x1F` salvo `\t \n \r` | otro ASCII | resto de `0x21–0x7F` |
| 80–FF | bytes no ASCII (los procesa el AFD UTF-8) | | |

Clases del AFD UTF-8 (`utf8::CLASS_NAMES`). Separan los bytes iniciales según la longitud de la secuencia y los bytes de continuación según el rango que exige cada caso especial de la sección 2. Del tercer byte en adelante, las tres clases de continuación son válidas siempre:

| Clase | Bytes | Papel |
|---|---|---|
| `00–7F` | ASCII | carácter de 1 byte |
| `80–8F` | continuación | válida como segundo byte salvo tras `E0` y `F0` (sobrelargas) |
| `90–9F` | continuación | válida como segundo byte salvo tras `E0` (sobrelarga) y `F4` (> U+10FFFF) |
| `A0–BF` | continuación | válida como segundo byte salvo tras `ED` (sustitutos) y `F4` (> U+10FFFF) |
| `C0 C1 F5–FF` | prohibidos | nunca aparecen en UTF-8 válido |
| `C2–DF` | inicial de 2 bytes | |
| `E0` | inicial de 3 bytes | exige `A0–BF` (evita sobrelargas) |
| `E1–EC EE EF` | inicial de 3 bytes | |
| `ED` | inicial de 3 bytes | exige `80–9F` (evita sustitutos) |
| `F0` | inicial de 4 bytes | exige `90–BF` (evita sobrelargas) |
| `F1–F3` | inicial de 4 bytes | |
| `F4` | inicial de 4 bytes | exige `80–8F` (evita > U+10FFFF) |

Clases del tokenizador XML (`xml::CLASS_NAMES`):

| Clase | Bytes | Papel |
|---|---|---|
| ␠, `\t \n \r` | espacio | separador en etiquetas; los tres de control se permiten en el texto |
| `<` `>` `/` `?` `!` `=` | una clase por símbolo | marcado |
| `"` `'` | una clase por símbolo | delimitan valores de atributo |
| `&` `#` `;` | una clase por símbolo | referencias |
| `-` `.` | una clase por símbolo | comentarios; caracteres de nombre no iniciales |
| `[` `]` | una clase por símbolo | CDATA y la prohibición de `]]>` |
| `0–9` | dígitos | nombre (no inicial), referencias decimales y hexadecimales |
| `x` | letra | `&#x` |
| `a–c e f B E F` | letras hexadecimales | nombre y referencia hexadecimal |
| `A` `C` `D` `T` | una clase por letra | `<![CDATA[`; `A C D` también son hexadecimales y `<!D` delata un DOCTYPE |
| `d` | letra hexadecimal | transita igual que `a–c e f B E F`; tiene clase propia solo para que `<!d` (DOCTYPE en minúsculas) se reporte como `DoctypeForbidden` |
| letra `_ :` | resto de letras ASCII, `_`, `:` | inicio de nombre |
| control | `0x00–0x1F` salvo `\t \n \r` | prohibidos en todo documento XML |
| otro ASCII | resto de `0x21–0x7F` | texto y valores |
| 80–FF | bytes no ASCII | los procesa el AFD UTF-8; en nombres se aceptan |

---

## 2. UTF-8 (RFC 3629)

### Expresión regular

Escrita en hexadecimal, con `T = [80–BF]` (byte de continuación):

```
UTF8-char = [00–7F]
          | [C2–DF] T
          | E0 [A0–BF] T   | [E1–EC] T T | ED [80–9F] T | [EE–EF] T T
          | F0 [90–BF] T T | [F1–F3] T T T | F4 [80–8F] T T
UTF8      = UTF8-char*
```

Las restricciones sobre el segundo byte no son decorativas: `E0 [A0–BF]` excluye las formas **sobrelargas** de 3 bytes, `ED [80–9F]` excluye los **sustitutos** U+D800–U+DFFF, `F0 [90–BF]` excluye las sobrelargas de 4 bytes y `F4 [80–8F]` excluye los valores **> U+10FFFF**. Los bytes `C0`, `C1` y `F5–FF` no aparecen en ninguna alternativa.

### Gramática regular (lineal por la derecha)

Cada no terminal corresponde a un estado del AFD (el nombre entre paréntesis es el de [afd-utf8.svg](afd-utf8.svg)):

```
S  → [00–7F] S | [C2–DF] T1 | E0 X | [E1–EC] T2 | [EE–EF] T2 | ED Y
   | F0 Z | [F1–F3] T3 | F4 W | ε                          (inicio, de aceptación)
T1 → [80–BF] S                                              (falta 1)
T2 → [80–BF] T1                                             (falta 2)
T3 → [80–BF] T2                                             (falta 3)
X  → [A0–BF] T1                                             (E0·)
Y  → [80–9F] T1                                             (ED·)
Z  → [90–BF] T2                                             (F0·)
W  → [80–8F] T2                                             (F4·)
```

La construcción es la estándar AFD → gramática lineal derecha: cada transición q —a→ p da la producción `Q → a P`, y cada estado de aceptación da `Q → ε`.

### Verificación

La prueba de propiedad `coincide_con_std` compara el AFD con `core::str::from_utf8` sobre miles de secuencias de bytes aleatorias; `secuencias_invalidas` cubre las categorías de error del *UTF-8 decoder stress test* de Markus Kuhn (continuaciones sueltas, sobrelargas, sustitutos, > U+10FFFF, bytes prohibidos, secuencias truncadas o interrumpidas).

---

## 3. Tokens de JSON (RFC 8259)

### Expresiones regulares

| Token | Expresión regular |
|---|---|
| Puntuación | `{` · `}` · `[` · `]` · `:` · `,` |
| Literales | `true` · `false` · `null` |
| Número | `-? (0 \| [1-9][0-9]*) (\.[0-9]+)? ([eE][+-]?[0-9]+)?` |
| Cadena | `" (U \| \\ (["\\/bfnrt] \| u H H H H))* "` |
| Espacio | `[ \t\n\r]+` (se descarta) |

donde `H = [0-9a-fA-F]` y `U` es cualquier carácter Unicode codificado en UTF-8 salvo `"`, `\` y los controles U+0000–U+001F.

### Gramática regular del número

Los no terminales son los estados de [afd-json.svg](afd-json.svg):

```
N    → - M | 0 Z | [1-9] I                  (valor)
M    → 0 Z | [1-9] I                        (-)
Z    → . D | [eE] E | ε                     (0)      sin ceros a la izquierda
I    → [0-9] I | . D | [eE] E | ε           (int)
D    → [0-9] F                              (int.)
F    → [0-9] F | [eE] E | ε                 (frac)
E    → [+-] G | [0-9] X                     (e)
G    → [0-9] X                              (e±)
X    → [0-9] X | ε                          (exp)
```

Los estados `0`, `int`, `frac` y `exp` son de aceptación. Un número no tiene delimitador propio: termina con el primer byte que no puede continuarlo (`,` `]` `}` espacio…). El tokenizador mira ese byte, emite el fin del número y **lo vuelve a procesar** desde el estado `valor`: es la regla de la coincidencia más larga con un byte de anticipación. Los bytes que solo podrían continuar un número mal formado (`01`, `1.2.3`, `1e5e`, `-0-`) se rechazan en el acto en lugar de cortarse en dos tokens.

### Gramática regular de la cadena

```
S0  → " C                                   (valor → cadena)
C   → " | U C | \ Esc                       (cadena)
Esc → ["\\/bfnrt] C | u H1                  (\)
H1  → H H2      H2 → H H3      H3 → H H4    (\u, \uX, \uXX)
H4  → H C                                   (\uXXX)
```

**Pares sustitutos.** En JSON un carácter fuera del plano básico se escribe como `😀`: un sustituto alto (D800–DBFF) seguido obligatoriamente de uno bajo (DC00–DFFF). Esa restricción también es regular (los rangos son finitos), pero expresarla en la tabla obligaría a distinguir los valores de cada dígito hexadecimal y multiplicaría los estados. Se implementa con un **registro acotado** de 16 bits que guarda el valor del escape: al cuarto dígito, si el valor es un sustituto alto, el autómata pasa al estado `alto`, que exige `\u` y cuatro dígitos más (`alto\u…alto\uXXX`). Un AFD con un registro de k valores es equivalente a un AFD con k veces más estados (construcción producto), así que el lenguaje sigue siendo regular; es la misma idea que usará la Fase 4 con las máscaras de bits y los contadores. En el diagrama esa transición es la única punteada.

### Literales

```
L → t T1 | f F1 | n N1
T1 → r T2    T2 → u T3    T3 → e
F1 → a F2    F2 → l F3    F3 → s F4    F4 → e
N1 → u N2    N2 → l N3    N3 → l
```

### El tokenizador completo

El AFD del tokenizador se obtiene uniendo los AFD de cada token desde un estado común `valor` y cerrando con la estrella de Kleene: al aceptar un token se vuelve a `valor`. El lenguaje reconocido es

```
(espacio | puntuación | literal | número | cadena)*
```

con la regla de la coincidencia más larga para los números. La tabla es una función **total**: para cada par (estado, clase) hay exactamente una entrada, y las entradas no válidas van a un estado sumidero de error. Por eso el autómata es determinista por construcción.

Dentro de las cadenas, los bytes no ASCII los consume el AFD UTF-8 de la sección 2: los dos autómatas corren en **producto** (el estado efectivo es el par (estado JSON, estado UTF-8)).

---

## 4. Tokens de XML (subconjunto de XML 1.0)

### Expresiones regulares

Con `S = [ \t\n\r]+`, `Char` = caracteres permitidos por XML 1.0 (`\t \n \r`, U+0020–U+D7FF, U+E000–U+FFFD, U+10000–U+10FFFF):

| Token | Expresión regular |
|---|---|
| Nombre | `NameStart (NameStart \| [0-9] \| - \| .)*`, con `NameStart = [A-Za-z_:] \| no ASCII` |
| Referencia | `&(lt\|gt\|amp\|quot\|apos);` · `&#[0-9]+;` · `&#x[0-9a-fA-F]+;` |
| Valor de atributo | `"([^<&"] \| Ref)*"` · `'([^<&'] \| Ref)*'` |
| Etiqueta de apertura | `< Nombre (S Nombre S? = S? Valor)* S? (> \| />)` |
| Etiqueta de cierre | `</ Nombre S? >` |
| Texto | `([^<&] \| Ref)*`, sin contener `]]>` |
| Comentario | `<!-- ((Char − -) \| -(Char − -))* -->` |
| CDATA | `<![CDATA[` (Char* sin contener `]]>`) `]]>` |
| Instrucción de procesamiento | `<?` Nombre (S (Char* sin contener `?>`))? `?>` (incluye `<?xml …?>`) |

Las etiquetas completas son regulares porque son planas: un número arbitrario de atributos se reconoce con un ciclo (`etiqueta␠ → atributo → … → fin valor → etiqueta␠`). Lo que **no** es regular es el anidamiento y el emparejamiento de `<a>` con `</a>`; eso es tarea de la gramática libre de contexto y de la pila (ver [`../fase2/gramatica-xml.md`](../fase2/gramatica-xml.md)).

### Gramáticas regulares de ejemplo

Comentario (prohíbe `--` en el contenido):

```
K0 → < K1     K1 → ! K2     K2 → - K3     K3 → - C
C  → (Char − -) C | - D                   (comentario)
D  → (Char − -) C | - E                   (com -)
E  → >                                    (com --)
```

Referencia de carácter hexadecimal:

```
R → & R1      R1 → # R2      R2 → x R3      R3 → H R4      R4 → H R4 | ;
```

Las restricciones de valor (la entidad es una de las 5 predefinidas; el número es un `Char` válido) se resuelven con registros acotados: el nombre de la entidad cabe en 4 bytes y el valor numérico se acota a U+10FFFF, así que el lenguaje sigue siendo regular.

### Lo que se excluye y por qué

- **`<!DOCTYPE`**: el autómata lo rechaza en el byte que sigue a `<!` si es `D` o `d` (error `DoctypeForbidden`), así que cualquier combinación de mayúsculas (`<!DOCTYPE`, `<!doctype`, `<!DocType`) da el mismo error en el mismo offset. XML exige `DOCTYPE` en mayúsculas, pero reportar la variante en minúsculas como `UnexpectedByte` sería un mensaje menos claro para lo que es, sin duda, un intento de DTD. Sin DTD no se pueden declarar entidades, y sin entidades declaradas no existen la **inyección de entidades externas (XXE)** ni la **expansión recursiva (*billion laughs*)**. La prueba `doctype_y_entidades_prohibidas` incluye los dos ataques.
- **Entidades no predefinidas** (`&x;`): error `UndefinedEntity`.

### Instrucciones de procesamiento

El lexer exige el nombre (`<??>` es error `InvalidPi`) y entrega el nombre y el contenido como eventos (`PiOpen`, `NameChar`, `PiChar`, `PiClose`) en lugar de descartarlos. Así el parser puede rechazar una declaración `<?xml …?>` que no esté al inicio o cuyo `encoding` no sea UTF-8: la posición de la declaración es una propiedad sintáctica, no léxica. Los comentarios sí se descartan en el lexer porque no afectan la validación.

---

## 5. Streaming y complejidad

**Tiempo O(n).** Por cada byte el tokenizador hace un acceso a `CLASS`, uno a la tabla y una acción de costo constante (emitir un evento, actualizar un registro). La única excepción es el fin de un número, que reprocesa el byte una vez; así que cada byte provoca **a lo sumo dos** accesos a la tabla.

**Memoria O(1).** Todo el estado cabe en una estructura de tamaño fijo, independiente de la entrada (prueba `memoria_constante`):

| Estructura | Tamaño |
|---|---|
| `Utf8Decoder` | 8 bytes |
| `JsonLexer` | 96 bytes |
| `XmlLexer` | 112 bytes |

**Posiciones en entradas de varios GB.** `Pos` guarda `offset: u64`, `line: u32` y `column: u32`. El offset es exacto siempre (2⁶⁴ bytes no son alcanzables). Línea y columna **saturan** en `u32::MAX` en lugar de dar la vuelta o provocar pánico: una línea de más de 2³² − 1 caracteres reporta `column = u32::MAX`, que se lee como "u32::MAX o más"; el offset sigue localizando el byte exacto. Se prefirió saturar a ampliar a `u64` para no agrandar `Pos` (16 bytes) ni los lexers (pruebas `columna_satura_en_u32_max`, `linea_satura_en_u32_max`). En el límite, la columna de un `]` o `?` que el lexer XML retiene y emite después (`back` en `xml.rs`) es aproximada: si la columna actual está saturada se reporta `u32::MAX`, aunque el carácter estuviera hasta 2 columnas antes; el offset sigue siendo exacto (prueba `back_respeta_columna_saturada`).

Como el lexer no reserva memoria ni guarda bytes de la entrada, es `no_std` y se puede incrustar en el validador generado (la CI lo comprueba compilándolo para `wasm32-unknown-unknown`).

**Reanudable.** Al terminar un bloque el estado (estado del AFD, registros, posición) queda en la estructura y el siguiente bloque continúa donde iba, aunque un token, un escape `\uXXXX` o una secuencia UTF-8 queden partidos. Las pruebas de propiedad `trozos_arbitrarios` (JSON y XML) verifican que tokenizar la entrada entera y tokenizarla partida en trozos aleatorios da exactamente los mismos eventos y errores.

**Salida sin copias.** En lugar de tokens con su texto, el tokenizador es una **máquina de Mealy** que emite eventos a un `Sink`: las cadenas llegan carácter a carácter ya decodificadas y los números byte a byte con su parte (signo, entero, fracción, exponente). Así la Fase 4 puede aplicar tries, `pattern`, `minLength` o comparaciones decimales exactas directamente sobre el flujo.

**Texto del usuario en los mensajes (`short`).** El crate `lexer` aloja también `lexer::Short` (`crates/lexer/src/short.rs`): recorta a 32 caracteres y escapa los controles del texto del usuario que repiten los mensajes de error del compilador (parser, Fase 3 y verificador de la IR, tarea 4.2). No es parte del tokenizador ni lo usa el validador generado; vive aquí porque `lexer` es la única dependencia común de esas fases, y es `no_std` y sin reservas como el resto. Un crate de diagnósticos aparte queda como mejora ([pendientes.md](pendientes.md)).

---

## 6. Verificación

| Prueba | Qué comprueba |
|---|---|
| `tests/utf8.rs` | Casos límite de UTF-8; igualdad con `core::str::from_utf8` (propiedad); memoria constante |
| `tests/json.rs` | Números, escapes, sustitutos, UTF-8, errores con posición, rangos de bytes; bloques de 1, 7 y 4096 bytes; trozos aleatorios (propiedad); **oráculo**: todo JSON serializado por `serde_json` produce los mismos átomos en el mismo orden (propiedad) |
| `tests/corpus.rs` | Formato de cada caso de `tests/cases` según `tests/README.md` (`_id` = nombre del archivo, `category` = carpeta, `format`, `expected`…), aplicado por `common::corpus()` antes de usar el caso; pruebas unitarias de cada regla incumplida |
| `tests/*.rs` (`corpus_en_bloques`) | Cada payload de `tests/cases` (JSON y XML) tokenizado entero y en bloques de 1, 7 y 4096 bytes da el mismo resultado; los casos con error léxico declarado fallan en el offset declarado |
| `tests/xml.rs` | Etiquetas, atributos, referencias, CDATA, comentarios, DOCTYPE/XXE/*billion laughs*, errores con posición; bloques de 1, 7 y 4096 bytes; trozos aleatorios (propiedad); **oráculo**: documentos generados al azar, con y sin referencias de carácter, producen exactamente sus nombres, atributos y textos (propiedad) |

El oráculo de XML encontró un error real durante el desarrollo: un documento cuyo primer carácter empieza por el byte `0xEF` se confundía con un BOM. Proptest guarda ese caso en `tests/xml.proptest-regressions` y lo reejecuta siempre.

## 7. Simplificaciones

- Los nombres XML aceptan cualquier carácter no ASCII; la especificación excluye algunos rangos.
- No se normalizan los fines de línea (`\r\n` → `\n`) ni los espacios de los valores de atributo.
- La columna de las posiciones cuenta puntos de código, no unidades UTF-16.

### Decisión tomada (D10): prefijo de BOM seguido de otro byte (JSON)

El tokenizador JSON reconoce el BOM con un contador (`bom`, 0–3) antes del AFD. Al inicio, si llegan `EF` o `EF BB` y después un byte que no continúa el BOM, el error es **`UnexpectedByte` en el offset 0** (línea 1, columna 1), aunque la secuencia sea UTF-8 válido: `EF BC 81` (U+FF01), `EF BB 81` y `EF 31` dan los tres `(UnexpectedByte, 0)`. Es lo mismo que da cualquier byte no ASCII fuera de cadena en su primer byte (`[1,EF BC 81]` da `(UnexpectedByte, 3)`; `C3 B1`, `(UnexpectedByte, 0)`). El contador solo retrasa la decisión hasta ver el byte siguiente, así que el error se informa en `Pos::START` y no en la posición del byte que rompe el prefijo; el resultado no depende del tamaño de bloque.

**Fin de entrada.** Si la entrada termina justo después del prefijo (`EF` o `EF BB` y nada más), `finish` informa **`InvalidUtf8` en el offset 0**: es una secuencia UTF-8 truncada, como cualquier otra al final. Antes de D10 esto ya salía en el offset 0, mientras que el prefijo seguido de otro byte salía en ese byte (offset 1 o 2); ahora los dos casos señalan el offset 0. **Diferencia conocida:** `C3` solo al inicio no pasa por el contador del BOM y da `(UnexpectedByte, 0)` en el acto, mientras que `EF` solo da `(InvalidUtf8, 0)`, porque `EF` podría ser el inicio de un BOM y el lexer espera al fin de la entrada para decidir.

**Razón.** Antes, el error era `InvalidUtf8` en el byte siguiente: un mensaje engañoso para un carácter válido y un offset que no era el del carácter. XML no tiene este caso: decodifica el carácter con el AFD UTF-8 y solo después decide si es un BOM. Lo fija la prueba `prefijo_de_bom_seguido_de_otro_byte` (bloques de 1, 7 y 4096 bytes, con línea y columna).
