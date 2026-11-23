# Fase 2 — Gramática y parser de DTD (DTD.1-DTD.2, D86)

La sintaxis del subconjunto DTD de `docs/00-alcance.md` §3bis es una **gramática libre de contexto LL(1)** sobre
los tokens del lexer ([../fase1/lexico-dtd.md](../fase1/lexico-dtd.md)). Está en [dtd.ll1](dtd.ll1). Su tabla
LL(1), con FIRST, FOLLOW y la comprobación de conflictos, la genera `scripts/ll1.py` en [ll1-dtd.md](ll1-dtd.md)
(`make docs`; `make docs-check` comprueba que la versionada coincide). **No tiene conflictos.**

El parser descendente que la ejecuta y su AST propio (D3) son de la tarea DTD.2: `crates/dtd/src/parser.rs` y
`ast.rs` (§6).

## 1. La gramática

```text
dtd           → item dtd | ε
item          → PI | elementdecl | attlistdecl
elementdecl   → DECL_ELEMENT NAME contentspec GT
contentspec   → K_EMPTY | K_ANY | LPAR group_body
group_body    → K_PCDATA mixed | cp children_rest RPAR occ
children_rest → COMMA cp seq_rest | PIPE cp choice_rest | ε
seq_rest      → COMMA cp seq_rest | ε
choice_rest   → PIPE cp choice_rest | ε
cp            → NAME occ | LPAR cp children_rest RPAR occ
occ           → OPT | STAR | PLUS | ε
mixed         → RPAR mixed_star | PIPE NAME mixed_names
mixed_names   → PIPE NAME mixed_names | RPAR STAR
mixed_star    → STAR | ε
attlistdecl   → DECL_ATTLIST NAME attdefs GT
attdefs       → attdef attdefs | ε
attdef        → NAME atttype defaultdecl
atttype       → T_CDATA | T_TOKENIZED | K_NOTATION LPAR NAME enum_rest | LPAR NAME enum_rest
enum_rest     → PIPE NAME enum_rest | RPAR
defaultdecl   → K_REQUIRED | K_IMPLIED | K_FIXED LITERAL | LITERAL
```

Cada no terminal corresponde a una producción de XML 1.0 §3: `elementdecl` [45], `contentspec` [46], `children` y
`cp` [47]-[50], `Mixed` [51], `AttlistDecl` [52], `AttDef` [53], `AttType` [54]-[59] y `DefaultDecl` [60].

**Por qué es LL(1) sin reescribir nada importante.** En XML, `choice` y `seq` empiezan igual: `( cp …`. La
gramática factoriza ese prefijo común. Tras el primer `cp`, el siguiente token decide: `,` es secuencia, `|` es
elección y `)` es un grupo de un solo elemento (`children_rest`). Una vez elegido, `seq_rest` solo admite `,` y
`choice_rest` solo `|`, así que mezclar separadores en el mismo grupo (`(a, b | c)`) es un error sintáctico, como
en XML. Lo mismo pasa con `#PCDATA`, que solo puede ir primero (`group_body`).

## 2. Palabras clave contextuales

En XML las palabras de una DTD no son reservadas: `<!ELEMENT EMPTY EMPTY>` declara un elemento llamado `EMPTY` sin
contenido. El lexer entrega todas como `NAME` (o `#` + nombre), y el parser las **clasifica por la celda** de la
tabla en la que está:

| Posición (no terminal en la cima) | Nombre | Terminal |
| --- | --- | --- |
| `item`, tras `<!` | `ELEMENT`, `ATTLIST` | `DECL_ELEMENT`, `DECL_ATTLIST` (cualquier otra es «declaración fuera de alcance») |
| `contentspec` | `EMPTY`, `ANY` | `K_EMPTY`, `K_ANY` |
| tras `#` | `PCDATA`, `REQUIRED`, `IMPLIED`, `FIXED` | `K_PCDATA`, `K_REQUIRED`, `K_IMPLIED`, `K_FIXED` |
| `atttype` | `CDATA` | `T_CDATA` |
| `atttype` | `ID`, `IDREF`, `IDREFS`, `ENTITY`, `ENTITIES`, `NMTOKEN`, `NMTOKENS` | `T_TOKENIZED` |
| `atttype` | `NOTATION` | `K_NOTATION` |
| cualquier otra | cualquiera | `NAME` |

La clasificación solo depende del no terminal en la cima de la pila, no de lo que se ha leído antes. Por eso la
gramática sigue siendo LL(1): cada celda decide con un token de anticipación ya clasificado. En ninguna celda
compiten un `NAME` y una palabra clave (`contentspec` no tiene celda `NAME`, y `atttype` tampoco), así que no hay
ambigüedad.

## 3. Lo que está en la gramática pero fuera del subconjunto

`ANY`, el contenido mixto (`mixed`) y los tipos `T_TOKENIZED` y `K_NOTATION` son sintaxis válida de XML. Están en
la gramática para que el parser los **reconozca y los rechace por alcance**, con el motivo
(`docs/00-alcance.md` §3bis.2), en lugar de dar un error sintáctico confuso. Las declaraciones `<!NOTATION` no
tienen fila: el parser las rechaza en su `<!` como «declaración fuera de alcance». `<!ENTITY` y `<!DOCTYPE` ni
siquiera llegan al parser: son error léxico.

## 4. Lo que la gramática no expresa

Como en las demás fases 2 del proyecto, estas restricciones son **semánticas** (Fase 3, DTD.3), no sintácticas:

- que cada nombre usado tenga su `<!ELEMENT>` y que no haya dos con el mismo nombre;
- que el modelo de contenido sea determinista (XML 1.0, apéndice E);
- que un atributo no se declare dos veces para el mismo elemento;
- que un valor por defecto o `#FIXED` pertenezca a la enumeración;
- que un `NAME` en posición de nombre de elemento o atributo empiece como un `Name` y no como un *nmtoken*
  (`1a`), y que no lleve `:` (§3bis.2).

La declaración `<?xml …?>` es una `PI` para la gramática. Que vaya al inicio y declare `UTF-8` lo comprueba el
parser al aplicar `item → PI`, igual que en XML.

## 5. Profundidad

Los grupos se anidan (`cp → LPAR cp …`), así que lo que el parser tiene pendiente crece con la profundidad de
paréntesis. Es la **única** producción recursiva de la gramática. El parser la recorre con una **pila explícita**
de grupos abiertos, en el heap, en lugar de llamarse a sí mismo (D7). La pila tiene un tope de 256 grupos
(`MAX_DEPTH`, `docs/00-alcance.md` §4): el paréntesis 257 es un error `Limit` en su byte.

Así ningún esquema puede desbordar la pila del hilo. La prueba
`un_millon_de_parentesis_no_desborda_la_pila` analiza un millón de `(` en un hilo con 256 KiB de pila y recibe el
`Limit` en el 257.

## 6. El parser (DTD.2)

**Entrada.** Los eventos del lexer se agrupan en tokens de la gramática (`Decl` con su palabra clave, `Name`,
`Hash`, puntuación, `Literal`, `Close`, `Pi`). Cada token lleva su `Span`. El lexer se para en el primer error
léxico, y el parser lo devuelve como `Lexical` con su clase y su posición.

**Una función por no terminal.** Siguen la tabla de [ll1-dtd.md](ll1-dtd.md): `dtd`, `elementdecl`, `contentspec`,
`group_body`, `occ`, `attlistdecl`, `atttype` y `defaultdecl`. `cp`, `children_rest`, `seq_rest` y `choice_rest`
van juntos en el bucle de `children`, con la pila de §5:

- tras un `cp`, `,` o `|` fijan el separador del grupo, que no se puede mezclar;
- `)` cierra el grupo, lee su sufijo y lo añade al grupo padre, o lo devuelve si era el exterior.

Cada decisión mira un solo token, clasificado por su posición (§2). El parser se para en el **primer** error.

**El AST** (`crates/dtd/src/ast.rs`) solo representa el subconjunto:

- `ElementDecl` con `ContentSpec::{Empty, Pcdata, Children(Particle)}`;
- la partícula es `Name`, `Seq` (`(a)` es una secuencia de uno) o `Choice` (dos opciones o más), con su `Occurs`;
- `AttlistDecl` con `AttDef { name, ty: Cdata | Enumeration, default: Required | Implied | Fixed | Default }`.

`(#PCDATA)*` es lo mismo que `(#PCDATA)`. Lo de fuera del subconjunto no tiene nodo: el parser lo rechaza antes.

**Errores.** Cuatro clases (`dtd::ErrorKind`), cada una con su posición y un mensaje:

| Clase | Cuándo | Ejemplo |
| --- | --- | --- |
| `Lexical` | Fase 1, con lo prohibido por seguridad | `<!ENTITY x 'y'>`, en el `<` |
| `Syntax` | Fuera de la gramática; el mensaje dice qué se esperaba y qué llegó | `<!ELEMENT a b>`: «se esperaba EMPTY, ANY o «(» tras el nombre del elemento, pero se encontró el nombre «b»» |
| `OutOfScope` | Sintaxis XML fuera de §3bis.2, con el motivo | `ANY`, `(#PCDATA\|b)*`, `ID`, `IDREF(S)`, `ENTITY`, `NMTOKEN(S)`, `NOTATION`, nombres con `:`, `encoding` distinto de UTF-8 |
| `Limit` | Más de 256 paréntesis anidados | el `(` número 257 |

También son `Syntax`:

- un `Name` que empieza por dígito, `-` o `.` en posición de nombre de elemento o de atributo (los *nmtokens*
  solo valen en las enumeraciones);
- `<?xml …?>` que no va al inicio.

Las demás instrucciones de procesamiento se ignoran.

## 7. Verificación (`crates/dtd/tests/parser.rs`)

- El AST del pedido completo (`<?xml?>`, comentario, elementos, listas de atributos, instrucción final) y sus
  posiciones.
- Grupos anidados con sufijos, el grupo de uno y las palabras clave contextuales (`<!ELEMENT EMPTY EMPTY>`, un
  atributo llamado `CDATA`, `SYSTEM` y `PUBLIC`, enumeraciones de *nmtokens*).
- Cada rechazo por alcance y cada error sintáctico, con su clase y su byte. Lo prohibido por seguridad llega como
  `Lexical`.
- El tope de profundidad: 256 grupos anidados se analizan enteros, el 257 es `Limit`, y un millón tampoco desborda.
- **Ida y vuelta** (*proptest*, 2 000 casos): DTD aleatorias con modelos anidados hasta 4 niveles, enumeraciones y
  literales con `<`, `&`, comillas, `%` y no ASCII se analizan; el AST escrito como texto y vuelto a analizar da
  el mismo AST.
