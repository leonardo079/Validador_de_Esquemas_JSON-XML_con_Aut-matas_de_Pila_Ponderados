# Fase 2 — Gramática de DTD (DTD.1, D86)

La sintaxis del subconjunto DTD de `docs/00-alcance.md` §3bis es una **gramática libre de contexto LL(1)** sobre
los tokens del lexer ([../fase1/lexico-dtd.md](../fase1/lexico-dtd.md)). Está en [dtd.ll1](dtd.ll1). Su tabla
LL(1), con FIRST, FOLLOW y la comprobación de conflictos, la genera `scripts/ll1.py` en [ll1-dtd.md](ll1-dtd.md)
(`make docs`; `make docs-check` comprueba que la versionada coincide). **No tiene conflictos.**

El parser descendente que la ejecuta, con su AST propio (D3), es la tarea DTD.2.

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

Los grupos se anidan (`cp → LPAR cp …`), así que la pila del parser crece con la profundidad de paréntesis. El
parser de DTD.2 será **iterativo**, con la pila de la tabla LL(1) en el heap, y con un tope de 256 paréntesis
anidados (`docs/00-alcance.md` §4, D7). Más que eso es un error `limit` en el paréntesis 257.
