# Fase 1 — Análisis léxico de DTD (DTD.1, D86)

El tercer lenguaje de esquema (`docs/00-alcance.md` §3bis) tiene su propio tokenizador:
`crates/dtd/src/lexer.rs`. Es un **AFD de tabla con salida** (máquina de Mealy) en producto con el AFD UTF-8, como
los de JSON y XML ([lexico.md](lexico.md)), y lee en **streaming**: el mismo resultado con cualquier partición de
la entrada.

No está en `crates/lexer` porque una DTD solo se lee al compilar el esquema, nunca dentro del validador generado.
`crates/lexer` es `no_std` y el emisor C copia sus tablas. Del crate `lexer` reutiliza `Pos`, `Span`, el
decodificador UTF-8 y el exportador DOT.

| AFD | Estados (sin el de error) | Clases | Columnas distintas |
| --- | --- | --- | --- |
| DTD | 26 | 28 | 25 |

Las tres cifras las comprueba `cargo test -p dtd` (`tests/tabla.rs`), que las calcula desde la tabla. El diagrama
[afd-dtd.svg](afd-dtd.svg) sale de la misma tabla (`schemac dot dtd`, `make docs`), y `make docs-check` compara su
`.dot` con el versionado.

## 1. Alfabeto y clases de bytes

El alfabeto son los 256 bytes. La tabla no distingue cada byte: los agrupa en 28 clases, porque las transiciones
de muchos bytes son iguales en todos los estados.

| Clases | Bytes | Papel |
| --- | --- | --- |
| `␠`, `\t \n \r` | espacio y fines de línea | separadores |
| `< > ! ? - [` | | marcas: `<!`, `<?`, `<!--`, `<![` |
| `( ) \| , * +` | | modelo de contenido y enumeraciones |
| `#` | | `#PCDATA`, `#REQUIRED`, `#IMPLIED`, `#FIXED` |
| `%` | | referencia de parámetro: error fuera de un literal |
| `& ;` | | referencias dentro de un literal |
| `" '` | | literales |
| `0–9`, `x`, `a–f A–F` | | dígitos de las referencias de carácter (`&#65;`, `&#x41;`) |
| `letra _ :`, `.` | resto de letras ASCII | nombres |
| `80–FF` | bytes no ASCII | nombres, literales, comentarios (decodificados por el AFD UTF-8) |
| `control` | 00–1F salvo `\t \n \r` | prohibidos por la producción `Char` de XML |
| `otro ASCII` | `= / @ $ ] …` | solo dentro de literales, comentarios e instrucciones |

Hay 3 columnas repetidas (28 clases, 25 columnas distintas):

- `␠` y `\t \n \r` tienen la misma columna;
- `[`, `%` y `otro ASCII` también, porque las tres van al error en todos los estados salvo dentro de literales,
  comentarios e instrucciones.

Se mantienen separadas porque el **tipo** del error cambia: `<![` es `ConditionalSectionForbidden` y `%` fuera de
un literal es `ParameterEntityForbidden` (§5). Esa clasificación la hace una función aparte de la tabla
(`error_kind`).

## 2. Tokens

Con `L` = letra ASCII, `_` o `:`, `N` = `L` ∪ dígito ∪ `-` ∪ `.` ∪ no ASCII, `S` = espacio o fin de línea:

| Token | Expresión regular | Evento(s) |
| --- | --- | --- |
| apertura de declaración | `<!` `L`+ | `DeclOpen`, la palabra clave como nombre |
| nombre o *nmtoken* | `N`+ | `NameBegin`, `NameChar`… |
| palabra clave con `#` | `#` `L` `N`* | `HashBegin`, `NameChar`… |
| puntuación | `(` \| `)` \| `\|` \| `,` \| `?` \| `*` \| `+` | uno por símbolo |
| literal | `"` ([^"<&] \| ref)* `"` \| `'` ([^'<&] \| ref)* `'` | `LiteralBegin`, `LiteralChar`…, `LiteralEnd` |
| referencia (en literal) | `&` (`lt` \| `gt` \| `amp` \| `quot` \| `apos`) `;` \| `&#` [0-9]+ `;` \| `&#x` [0-9a-fA-F]+ `;` | el carácter resuelto |
| cierre | `>` | `DeclClose` (rango: el `>`; el parser arma el de la declaración, de `<!` a `>`) |
| instrucción | `<?` `L` `N`* (`S` ([^?] \| `?`+[^?>])* `?`*)? `?>` | `PiOpen`, `NameChar`…, `PiChar`…, `PiClose` |
| comentario | `<!--` ([^-] \| `-`[^-])* `-->` | ninguno: se descarta |
| separador | `S`+ | ninguno |

Las **palabras clave son contextuales**: `ELEMENT`, `EMPTY` o `CDATA` llegan como nombres y el parser las
clasifica por su posición ([../fase2/gramatica-dtd.md](../fase2/gramatica-dtd.md) §2). Así `<!ELEMENT EMPTY EMPTY>`
declara un elemento llamado `EMPTY`, como en XML, y `SYSTEM` y `PUBLIC` son nombres válidos (enmienda E4).

Un *nmtoken* puede empezar por dígito, `-` o `.` (`(1|2)` en una enumeración). El lexer los acepta en todas
partes y el parser exige un `Name` (que empiece por letra, `_`, `:` o no ASCII) donde corresponde.

**Separación obligatoria.** XML exige espacio entre ciertos tokens. El AFD lo hace cumplir con dos estados:

- tras un nombre solo puede venir puntuación, un sufijo, espacio o `>`. `<!ELEMENT a(b)>`, `CDATA"x"` y `a#` son
  error;
- tras `)` o un sufijo (estado «`)` o sufijo»), tampoco un nombre ni `(`. `(b)c` es error;
- tras un literal (estado «fin literal»), solo espacio o `>`. `"x"c` es error.

## 3. Gramática regular de ejemplo: el literal con comillas dobles

Los no terminales son estados de [afd-dtd.svg](afd-dtd.svg):

```text
Lit     → "  Dentro
Dentro  → "  Fin  |  &  Ref  |  c  Dentro          (c ∈ Σ − {", &, <, control})
Ref     → l Rnom  |  #  Rhash
Rnom    → l Rnom  |  ;  Dentro                     (el nombre se compara al llegar `;`)
Rhash   → d Rdec  |  x  Rx
Rdec    → d Rdec  |  ;  Dentro
Rx      → h Rhex
Rhex    → h Rhex  |  ;  Dentro
Fin     → ε
```

Es lineal por la derecha, así que el lenguaje es regular. Que el nombre sea una de las 5 entidades predefinidas y
que el valor de `&#N;` esté en `Char` no se expresa en la tabla: lo comprueban **registros acotados** (§5), que
equivalen a más estados por la construcción producto.

## 4. El autómata

Estados, por grupo:

| Grupo | Estados |
| --- | --- |
| Entre declaraciones (único de aceptación) | `entre declaraciones` |
| Marcas | `<`, `<!`, `palabra clave` |
| Cuerpo de una declaración | `declaración`, `nombre`, `#`, `) o sufijo` |
| Literales | `literal"`, `literal'`, `fin literal` |
| Referencias | `&`, `&nombre`, `&#`, `&#dec`, `&#x`, `&#xhex` |
| Comentarios | `<!-`, `comentario`, `com -`, `com --` |
| Instrucciones | `<?`, `<?nombre`, `<?nombre?`, `<?nombre …`, `<?… ?` |

Más el estado de error, que es un sumidero y no se dibuja. Tras `;`, una referencia vuelve al literal del que
salió: la tabla dice comillas dobles y la vuelta a comillas simples, que depende del registro `ref_ret`, se dibuja
punteada.

## 5. Lo prohibido por seguridad (D8, D86 §3bis.3)

| Construcción | Cómo se detecta | Error, y dónde |
| --- | --- | --- |
| `<!ENTITY`, `<!DOCTYPE` (en cualquier caja) | Registro de la palabra clave (8 bytes; una más larga no puede ser ninguna de las dos), comparado al terminar la palabra: con espacio, con `>`, con otro byte o al acabar la entrada | `EntityForbidden`, `DoctypeForbidden`, en el `<` |
| `<![INCLUDE[`, `<![IGNORE[` | Transición `<!` × `[` al error | `ConditionalSectionForbidden`, en el `<` |
| `%nombre;` fuera de un literal | Transición `%` al error en los estados entre tokens | `ParameterEntityForbidden`, en el `%` |
| `&nombre;` no predefinida (en un literal) | Registro del nombre (4 bytes: `quot`, `apos`) | `UndefinedEntity`, en el `&` |
| `&` o `<` fuera de un literal | No hay transición | `UnexpectedByte` |

Dentro de un literal, `%` es un carácter más, como en el `AttValue` de XML. `SYSTEM` y `PUBLIC` no se prohíben:
solo pueden ir dentro de `<!ENTITY`, `<!DOCTYPE` o `<!NOTATION`, y las dos primeras ya son error en su `<`
(enmienda E4).

Con esto, nada en una DTD puede **expandirse** ni **abrir un recurso**. Cada byte produce a lo sumo un evento (más
`NameBegin` al empezar un nombre, y el `?` retenido de una instrucción), así que el tamaño de lo que recibe el
parser es lineal en la entrada: no hay *billion laughs* ni XXE.

## 6. Streaming y complejidad

- **Tiempo O(n):** una consulta a la tabla por byte, más O(1) del decodificador UTF-8 y de los registros.
- **Memoria O(1):** el estado del lexer ocupa unos pocos registros de tamaño fijo (`DtdLexer` no reserva memoria):
  la posición, el decodificador, la palabra clave (8 bytes), el nombre de entidad (4 bytes) y el valor de la
  referencia (acotado a U+10FFFF).
- **Reanudable:** `feed` acepta bloques de cualquier tamaño y `finish` señala el final. Un error deja el lexer
  detenido y lo devuelve siempre igual.

## 7. Verificación (`cargo test -p dtd`)

- `tests/lexer.rs`:
  - casos de cada token y de cada error, con su byte;
  - los rangos de los eventos;
  - el mismo resultado (eventos, rangos y error) con bloques de 1, 7 y 4 096 bytes;
  - un **oráculo**: un tokenizador de referencia escrito aparte, con código estructurado y sin tabla, que tiene que
    dar los mismos eventos y el mismo error (clase y byte) que el AFD en 20 000 entradas armadas al azar con
    trozos de DTD válidos, prohibidos y rotos, más otras 20 000 declaraciones bien formadas probadas también por
    bloques.

  El oráculo encontró una incoherencia en el desarrollo: U+FFFE al principio de la entrada daba `InvalidChar` y en
  cualquier otro sitio entre declaraciones `UnexpectedByte`. Ahora es `UnexpectedByte` siempre.
- `tests/tabla.rs`:
  - las cifras de este documento;
  - que el `.dot` versionado es el de la tabla;
  - que el error es un sumidero, que solo «entre declaraciones» acepta y que cada byte tiene clase.

## 8. Simplificaciones

- Como en el lexer XML, los nombres aceptan **cualquier** carácter no ASCII (XML restringe algunos rangos) y no se
  normalizan los fines de línea.
- Una declaración `<?xml …?>` se tokeniza como cualquier instrucción. Que vaya al inicio y diga `UTF-8` lo comprueba
  el parser (DTD.2), igual que en XML.
