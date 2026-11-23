<!-- Generado por `python3 scripts/ll1.py docs/fase2/dtd.ll1`. No editar a mano. -->

## Gramática

```
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

Símbolo inicial: `dtd`. Terminales: `COMMA`, `DECL_ATTLIST`, `DECL_ELEMENT`, `GT`, `K_ANY`, `K_EMPTY`, `K_FIXED`, `K_IMPLIED`, `K_NOTATION`, `K_PCDATA`, `K_REQUIRED`, `LITERAL`, `LPAR`, `NAME`, `OPT`, `PI`, `PIPE`, `PLUS`, `RPAR`, `STAR`, `T_CDATA`, `T_TOKENIZED`.

## Conjuntos FIRST y FOLLOW

| No terminal | FIRST | FOLLOW |
|---|---|---|
| `dtd` | `DECL_ATTLIST`, `DECL_ELEMENT`, `PI`, `ε` | `$` |
| `item` | `DECL_ATTLIST`, `DECL_ELEMENT`, `PI` | `DECL_ATTLIST`, `DECL_ELEMENT`, `PI`, `$` |
| `elementdecl` | `DECL_ELEMENT` | `DECL_ATTLIST`, `DECL_ELEMENT`, `PI`, `$` |
| `contentspec` | `K_ANY`, `K_EMPTY`, `LPAR` | `GT` |
| `group_body` | `K_PCDATA`, `LPAR`, `NAME` | `GT` |
| `children_rest` | `COMMA`, `PIPE`, `ε` | `RPAR` |
| `seq_rest` | `COMMA`, `ε` | `RPAR` |
| `choice_rest` | `PIPE`, `ε` | `RPAR` |
| `cp` | `LPAR`, `NAME` | `COMMA`, `PIPE`, `RPAR` |
| `occ` | `OPT`, `PLUS`, `STAR`, `ε` | `COMMA`, `GT`, `PIPE`, `RPAR` |
| `mixed` | `PIPE`, `RPAR` | `GT` |
| `mixed_names` | `PIPE`, `RPAR` | `GT` |
| `mixed_star` | `STAR`, `ε` | `GT` |
| `attlistdecl` | `DECL_ATTLIST` | `DECL_ATTLIST`, `DECL_ELEMENT`, `PI`, `$` |
| `attdefs` | `NAME`, `ε` | `GT` |
| `attdef` | `NAME` | `GT`, `NAME` |
| `atttype` | `K_NOTATION`, `LPAR`, `T_CDATA`, `T_TOKENIZED` | `K_FIXED`, `K_IMPLIED`, `K_REQUIRED`, `LITERAL` |
| `enum_rest` | `PIPE`, `RPAR` | `K_FIXED`, `K_IMPLIED`, `K_REQUIRED`, `LITERAL` |
| `defaultdecl` | `K_FIXED`, `K_IMPLIED`, `K_REQUIRED`, `LITERAL` | `GT`, `NAME` |

## Tabla LL(1)

Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como
siguiente token. Una celda vacía es un error sintáctico.

| | `COMMA` | `DECL_ATTLIST` | `DECL_ELEMENT` | `GT` | `K_ANY` | `K_EMPTY` | `K_FIXED` | `K_IMPLIED` | `K_NOTATION` | `K_PCDATA` | `K_REQUIRED` | `LITERAL` | `LPAR` | `NAME` | `OPT` | `PI` | `PIPE` | `PLUS` | `RPAR` | `STAR` | `T_CDATA` | `T_TOKENIZED` | `$` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `dtd` |  | item dtd | item dtd |  |  |  |  |  |  |  |  |  |  |  |  | item dtd |  |  |  |  |  |  | ε |
| `item` |  | attlistdecl | elementdecl |  |  |  |  |  |  |  |  |  |  |  |  | PI |  |  |  |  |  |  |  |
| `elementdecl` |  |  | DECL_ELEMENT NAME contentspec GT |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `contentspec` |  |  |  |  | K_ANY | K_EMPTY |  |  |  |  |  |  | LPAR group_body |  |  |  |  |  |  |  |  |  |  |
| `group_body` |  |  |  |  |  |  |  |  |  | K_PCDATA mixed |  |  | cp children_rest RPAR occ | cp children_rest RPAR occ |  |  |  |  |  |  |  |  |  |
| `children_rest` | COMMA cp seq_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | PIPE cp choice_rest |  | ε |  |  |  |  |
| `seq_rest` | COMMA cp seq_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |  |  |  |  |
| `choice_rest` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | PIPE cp choice_rest |  | ε |  |  |  |  |
| `cp` |  |  |  |  |  |  |  |  |  |  |  |  | LPAR cp children_rest RPAR occ | NAME occ |  |  |  |  |  |  |  |  |  |
| `occ` | ε |  |  | ε |  |  |  |  |  |  |  |  |  |  | OPT |  | ε | PLUS | ε | STAR |  |  |  |
| `mixed` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | PIPE NAME mixed_names |  | RPAR mixed_star |  |  |  |  |
| `mixed_names` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | PIPE NAME mixed_names |  | RPAR STAR |  |  |  |  |
| `mixed_star` |  |  |  | ε |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | STAR |  |  |  |
| `attlistdecl` |  | DECL_ATTLIST NAME attdefs GT |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `attdefs` |  |  |  | ε |  |  |  |  |  |  |  |  |  | attdef attdefs |  |  |  |  |  |  |  |  |  |
| `attdef` |  |  |  |  |  |  |  |  |  |  |  |  |  | NAME atttype defaultdecl |  |  |  |  |  |  |  |  |  |
| `atttype` |  |  |  |  |  |  |  |  | K_NOTATION LPAR NAME enum_rest |  |  |  | LPAR NAME enum_rest |  |  |  |  |  |  |  | T_CDATA | T_TOKENIZED |  |
| `enum_rest` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | PIPE NAME enum_rest |  | RPAR |  |  |  |  |
| `defaultdecl` |  |  |  |  |  |  | K_FIXED LITERAL | K_IMPLIED |  |  | K_REQUIRED | LITERAL |  |  |  |  |  |  |  |  |  |  |  |

## Conflictos

Ninguno: cada celda tiene a lo sumo una producción, así que **la gramática es LL(1)**.
