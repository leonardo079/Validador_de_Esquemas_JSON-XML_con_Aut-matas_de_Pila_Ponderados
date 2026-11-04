<!-- Generado por `python3 scripts/ll1.py docs/fase2/xsd.ll1`. No editar a mano. -->

## Gramática

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
facet_tag      → S_ENUMERATION | S_MININCLUSIVE | S_MAXINCLUSIVE | S_MINEXCLUSIVE | S_MAXEXCLUSIVE | S_LENGTH | S_MINLENGTH | S_MAXLENGTH | S_PATTERN
facet_rest     → /> | > annot_opt ETAG
```

Símbolo inicial: `document`. Terminales: `/>`, `>`, `ATTR`, `ETAG`, `STAG`, `S_ALL`, `S_ANNOTATION`, `S_APPINFO`, `S_ATTRIBUTE`, `S_CHOICE`, `S_COMPLEXTYPE`, `S_DOCUMENTATION`, `S_ELEMENT`, `S_ENUMERATION`, `S_EXTENSION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN`, `S_RESTRICTION`, `S_SCHEMA`, `S_SEQUENCE`, `S_SIMPLECONTENT`, `S_SIMPLETYPE`, `TEXT`.

## Conjuntos FIRST y FOLLOW

| No terminal | FIRST | FOLLOW |
|---|---|---|
| `document` | `S_SCHEMA` | `$` |
| `schema_rest` | `/>`, `>` | `$` |
| `top_items` | `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SIMPLETYPE`, `ε` | `ETAG` |
| `top_item` | `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SIMPLETYPE` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SIMPLETYPE` |
| `attrs` | `ATTR`, `ε` | `/>`, `>` |
| `annot_opt` | `S_ANNOTATION`, `ε` | `ETAG`, `S_ALL`, `S_ATTRIBUTE`, `S_CHOICE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_ENUMERATION`, `S_EXTENSION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN`, `S_RESTRICTION`, `S_SEQUENCE`, `S_SIMPLECONTENT`, `S_SIMPLETYPE` |
| `annotation` | `S_ANNOTATION` | `ETAG`, `S_ALL`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_CHOICE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_ENUMERATION`, `S_EXTENSION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN`, `S_RESTRICTION`, `S_SEQUENCE`, `S_SIMPLECONTENT`, `S_SIMPLETYPE` |
| `annot_rest` | `/>`, `>` | `ETAG`, `S_ALL`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_CHOICE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_ENUMERATION`, `S_EXTENSION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN`, `S_RESTRICTION`, `S_SEQUENCE`, `S_SIMPLECONTENT`, `S_SIMPLETYPE` |
| `annot_items` | `S_APPINFO`, `S_DOCUMENTATION`, `ε` | `ETAG` |
| `any_rest` | `/>`, `>` | `ETAG`, `STAG`, `S_APPINFO`, `S_DOCUMENTATION`, `TEXT` |
| `any` | `STAG`, `TEXT`, `ε` | `ETAG` |
| `element` | `S_ELEMENT` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_CHOICE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SEQUENCE`, `S_SIMPLETYPE` |
| `element_rest` | `/>`, `>` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_CHOICE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SEQUENCE`, `S_SIMPLETYPE` |
| `element_type` | `S_COMPLEXTYPE`, `S_SIMPLETYPE`, `ε` | `ETAG` |
| `complex_type` | `S_COMPLEXTYPE` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SIMPLETYPE` |
| `ct_rest` | `/>`, `>` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SIMPLETYPE` |
| `ct_content` | `S_ALL`, `S_ATTRIBUTE`, `S_CHOICE`, `S_SEQUENCE`, `S_SIMPLECONTENT`, `ε` | `ETAG` |
| `model_opt` | `S_ALL`, `S_CHOICE`, `S_SEQUENCE`, `ε` | `ETAG`, `S_ATTRIBUTE` |
| `attr_decls` | `S_ATTRIBUTE`, `ε` | `ETAG` |
| `sequence` | `S_SEQUENCE` | `ETAG`, `S_ATTRIBUTE`, `S_CHOICE`, `S_ELEMENT`, `S_SEQUENCE` |
| `choice` | `S_CHOICE` | `ETAG`, `S_ATTRIBUTE`, `S_CHOICE`, `S_ELEMENT`, `S_SEQUENCE` |
| `group_rest` | `/>`, `>` | `ETAG`, `S_ATTRIBUTE`, `S_CHOICE`, `S_ELEMENT`, `S_SEQUENCE` |
| `particles` | `S_CHOICE`, `S_ELEMENT`, `S_SEQUENCE`, `ε` | `ETAG` |
| `particle` | `S_CHOICE`, `S_ELEMENT`, `S_SEQUENCE` | `ETAG`, `S_CHOICE`, `S_ELEMENT`, `S_SEQUENCE` |
| `all` | `S_ALL` | `ETAG`, `S_ATTRIBUTE` |
| `all_rest` | `/>`, `>` | `ETAG`, `S_ATTRIBUTE` |
| `all_items` | `S_ELEMENT`, `ε` | `ETAG` |
| `attribute` | `S_ATTRIBUTE` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SIMPLETYPE` |
| `attr_rest` | `/>`, `>` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_SIMPLETYPE` |
| `simple_opt` | `S_SIMPLETYPE`, `ε` | `ETAG`, `S_ENUMERATION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN` |
| `simple_content` | `S_SIMPLECONTENT` | `ETAG` |
| `extension` | `S_EXTENSION` | `ETAG` |
| `ext_rest` | `/>`, `>` | `ETAG` |
| `simple_type` | `S_SIMPLETYPE` | `ETAG`, `S_ANNOTATION`, `S_ATTRIBUTE`, `S_COMPLEXTYPE`, `S_ELEMENT`, `S_ENUMERATION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN`, `S_SIMPLETYPE` |
| `restriction` | `S_RESTRICTION` | `ETAG` |
| `restr_rest` | `/>`, `>` | `ETAG` |
| `facets` | `S_ENUMERATION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN`, `ε` | `ETAG` |
| `facet` | `S_ENUMERATION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN` | `ETAG`, `S_ENUMERATION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN` |
| `facet_tag` | `S_ENUMERATION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN` | `/>`, `>`, `ATTR` |
| `facet_rest` | `/>`, `>` | `ETAG`, `S_ENUMERATION`, `S_LENGTH`, `S_MAXEXCLUSIVE`, `S_MAXINCLUSIVE`, `S_MAXLENGTH`, `S_MINEXCLUSIVE`, `S_MININCLUSIVE`, `S_MINLENGTH`, `S_PATTERN` |

## Tabla LL(1)

Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como
siguiente token. Una celda vacía es un error sintáctico.

| | `/>` | `>` | `ATTR` | `ETAG` | `STAG` | `S_ALL` | `S_ANNOTATION` | `S_APPINFO` | `S_ATTRIBUTE` | `S_CHOICE` | `S_COMPLEXTYPE` | `S_DOCUMENTATION` | `S_ELEMENT` | `S_ENUMERATION` | `S_EXTENSION` | `S_LENGTH` | `S_MAXEXCLUSIVE` | `S_MAXINCLUSIVE` | `S_MAXLENGTH` | `S_MINEXCLUSIVE` | `S_MININCLUSIVE` | `S_MINLENGTH` | `S_PATTERN` | `S_RESTRICTION` | `S_SCHEMA` | `S_SEQUENCE` | `S_SIMPLECONTENT` | `S_SIMPLETYPE` | `TEXT` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `document` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | S_SCHEMA attrs schema_rest |  |  |  |  |
| `schema_rest` | /> | > top_items ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `top_items` |  |  |  | ε |  |  | top_item top_items |  | top_item top_items |  | top_item top_items |  | top_item top_items |  |  |  |  |  |  |  |  |  |  |  |  |  |  | top_item top_items |  |
| `top_item` |  |  |  |  |  |  | annotation |  | attribute |  | complex_type |  | element |  |  |  |  |  |  |  |  |  |  |  |  |  |  | simple_type |  |
| `attrs` | ε | ε | ATTR attrs |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `annot_opt` |  |  |  | ε |  | ε | annotation |  | ε | ε | ε |  | ε | ε | ε | ε | ε | ε | ε | ε | ε | ε | ε | ε |  | ε | ε | ε |  |
| `annotation` |  |  |  |  |  |  | S_ANNOTATION attrs annot_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `annot_rest` | /> | > annot_items ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `annot_items` |  |  |  | ε |  |  |  | S_APPINFO attrs any_rest annot_items |  |  |  | S_DOCUMENTATION attrs any_rest annot_items |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `any_rest` | /> | > any ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `any` |  |  |  | ε | STAG attrs any_rest any |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | TEXT any |
| `element` |  |  |  |  |  |  |  |  |  |  |  |  | S_ELEMENT attrs element_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `element_rest` | /> | > annot_opt element_type ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `element_type` |  |  |  | ε |  |  |  |  |  |  | complex_type |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | simple_type |  |
| `complex_type` |  |  |  |  |  |  |  |  |  |  | S_COMPLEXTYPE attrs ct_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `ct_rest` | /> | > annot_opt ct_content ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `ct_content` |  |  |  | model_opt attr_decls |  | model_opt attr_decls |  |  | model_opt attr_decls | model_opt attr_decls |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | model_opt attr_decls | simple_content |  |  |
| `model_opt` |  |  |  | ε |  | all |  |  | ε | choice |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | sequence |  |  |  |
| `attr_decls` |  |  |  | ε |  |  |  |  | attribute attr_decls |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `sequence` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | S_SEQUENCE attrs group_rest |  |  |  |
| `choice` |  |  |  |  |  |  |  |  |  | S_CHOICE attrs group_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `group_rest` | /> | > annot_opt particles ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `particles` |  |  |  | ε |  |  |  |  |  | particle particles |  |  | particle particles |  |  |  |  |  |  |  |  |  |  |  |  | particle particles |  |  |  |
| `particle` |  |  |  |  |  |  |  |  |  | choice |  |  | element |  |  |  |  |  |  |  |  |  |  |  |  | sequence |  |  |  |
| `all` |  |  |  |  |  | S_ALL attrs all_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `all_rest` | /> | > annot_opt all_items ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `all_items` |  |  |  | ε |  |  |  |  |  |  |  |  | element all_items |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `attribute` |  |  |  |  |  |  |  |  | S_ATTRIBUTE attrs attr_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `attr_rest` | /> | > annot_opt simple_opt ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `simple_opt` |  |  |  | ε |  |  |  |  |  |  |  |  |  | ε |  | ε | ε | ε | ε | ε | ε | ε | ε |  |  |  |  | simple_type |  |
| `simple_content` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | S_SIMPLECONTENT attrs > annot_opt extension ETAG |  |  |
| `extension` |  |  |  |  |  |  |  |  |  |  |  |  |  |  | S_EXTENSION attrs ext_rest |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `ext_rest` | /> | > annot_opt attr_decls ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `simple_type` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | S_SIMPLETYPE attrs > annot_opt restriction ETAG |  |
| `restriction` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | S_RESTRICTION attrs restr_rest |  |  |  |  |  |
| `restr_rest` | /> | > annot_opt simple_opt facets ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| `facets` |  |  |  | ε |  |  |  |  |  |  |  |  |  | facet facets |  | facet facets | facet facets | facet facets | facet facets | facet facets | facet facets | facet facets | facet facets |  |  |  |  |  |  |
| `facet` |  |  |  |  |  |  |  |  |  |  |  |  |  | facet_tag attrs facet_rest |  | facet_tag attrs facet_rest | facet_tag attrs facet_rest | facet_tag attrs facet_rest | facet_tag attrs facet_rest | facet_tag attrs facet_rest | facet_tag attrs facet_rest | facet_tag attrs facet_rest | facet_tag attrs facet_rest |  |  |  |  |  |  |
| `facet_tag` |  |  |  |  |  |  |  |  |  |  |  |  |  | S_ENUMERATION |  | S_LENGTH | S_MAXEXCLUSIVE | S_MAXINCLUSIVE | S_MAXLENGTH | S_MINEXCLUSIVE | S_MININCLUSIVE | S_MINLENGTH | S_PATTERN |  |  |  |  |  |  |
| `facet_rest` | /> | > annot_opt ETAG |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |

## Conflictos

Ninguno: cada celda tiene a lo sumo una producción, así que **la gramática es LL(1)**.
