<!-- Generado por `python3 scripts/ll1.py docs/fase2/esquema.ll1`. No editar a mano. -->

## Gramática

```
schema        → { keywords } | true | false
keywords      → keyword keywords_tail | ε
keywords_tail → , keyword keywords_tail | ε
keyword       → K_STRING : STRING | K_NUMBER : NUMBER | K_BOOL : boolean | K_SCHEMA : schema | K_MAP : schema_map | K_LIST : [ schemas ] | K_VALUE : value | K_VALUES : array | K_TYPE : type_value | K_REQUIRED : string_array | K_DEPREQ : dep_map | K_OTHER : value
boolean       → true | false
type_value    → STRING | string_array
string_array  → [ strings ]
strings       → STRING strings_tail | ε
strings_tail  → , STRING strings_tail | ε
schema_map    → { entries }
entries       → STRING : schema entries_tail | ε
entries_tail  → , STRING : schema entries_tail | ε
schemas       → schema schemas_tail | ε
schemas_tail  → , schema schemas_tail | ε
dep_map       → { deps }
deps          → STRING : string_array deps_tail | ε
deps_tail     → , STRING : string_array deps_tail | ε
value         → object | array | STRING | NUMBER | true | false | null
object        → { members }
members       → pair members_tail | ε
members_tail  → , pair members_tail | ε
pair          → STRING : value
array         → [ elements ]
elements      → value elements_tail | ε
elements_tail → , value elements_tail | ε
```

Símbolo inicial: `schema`. Terminales: `,`, `:`, `K_BOOL`, `K_DEPREQ`, `K_LIST`, `K_MAP`, `K_NUMBER`, `K_OTHER`, `K_REQUIRED`, `K_SCHEMA`, `K_STRING`, `K_TYPE`, `K_VALUE`, `K_VALUES`, `NUMBER`, `STRING`, `[`, `]`, `false`, `null`, `true`, `{`, `}`.

## Conjuntos FIRST y FOLLOW

| No terminal | FIRST | FOLLOW |
|---|---|---|
| `schema` | `false`, `true`, `{` | `,`, `]`, `}`, `$` |
| `keywords` | `K_BOOL`, `K_DEPREQ`, `K_LIST`, `K_MAP`, `K_NUMBER`, `K_OTHER`, `K_REQUIRED`, `K_SCHEMA`, `K_STRING`, `K_TYPE`, `K_VALUE`, `K_VALUES`, `ε` | `}` |
| `keywords_tail` | `,`, `ε` | `}` |
| `keyword` | `K_BOOL`, `K_DEPREQ`, `K_LIST`, `K_MAP`, `K_NUMBER`, `K_OTHER`, `K_REQUIRED`, `K_SCHEMA`, `K_STRING`, `K_TYPE`, `K_VALUE`, `K_VALUES` | `,`, `}` |
| `boolean` | `false`, `true` | `,`, `}` |
| `type_value` | `STRING`, `[` | `,`, `}` |
| `string_array` | `[` | `,`, `}` |
| `strings` | `STRING`, `ε` | `]` |
| `strings_tail` | `,`, `ε` | `]` |
| `schema_map` | `{` | `,`, `}` |
| `entries` | `STRING`, `ε` | `}` |
| `entries_tail` | `,`, `ε` | `}` |
| `schemas` | `false`, `true`, `{`, `ε` | `]` |
| `schemas_tail` | `,`, `ε` | `]` |
| `dep_map` | `{` | `,`, `}` |
| `deps` | `STRING`, `ε` | `}` |
| `deps_tail` | `,`, `ε` | `}` |
| `value` | `NUMBER`, `STRING`, `[`, `false`, `null`, `true`, `{` | `,`, `]`, `}` |
| `object` | `{` | `,`, `]`, `}` |
| `members` | `STRING`, `ε` | `}` |
| `members_tail` | `,`, `ε` | `}` |
| `pair` | `STRING` | `,`, `}` |
| `array` | `[` | `,`, `]`, `}` |
| `elements` | `NUMBER`, `STRING`, `[`, `false`, `null`, `true`, `{`, `ε` | `]` |
| `elements_tail` | `,`, `ε` | `]` |

## Tabla LL(1)

Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como
siguiente token. Una celda vacía es un error sintáctico.

| | `,` | `K_BOOL` | `K_DEPREQ` | `K_LIST` | `K_MAP` | `K_NUMBER` | `K_OTHER` | `K_REQUIRED` | `K_SCHEMA` | `K_STRING` | `K_TYPE` | `K_VALUE` | `K_VALUES` | `NUMBER` | `STRING` | `[` | `]` | `false` | `null` | `true` | `{` | `}` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `schema` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | false |  | true | { keywords } |  |
| `keywords` |  | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail | keyword keywords_tail |  |  |  |  |  |  |  |  | ε |
| `keywords_tail` | , keyword keywords_tail |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |
| `keyword` |  | K_BOOL : boolean | K_DEPREQ : dep_map | K_LIST : [ schemas ] | K_MAP : schema_map | K_NUMBER : NUMBER | K_OTHER : value | K_REQUIRED : string_array | K_SCHEMA : schema | K_STRING : STRING | K_TYPE : type_value | K_VALUE : value | K_VALUES : array |  |  |  |  |  |  |  |  |  |
| `boolean` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | false |  | true |  |  |
| `type_value` |  |  |  |  |  |  |  |  |  |  |  |  |  |  | STRING | string_array |  |  |  |  |  |  |
| `string_array` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | [ strings ] |  |  |  |  |  |  |
| `strings` |  |  |  |  |  |  |  |  |  |  |  |  |  |  | STRING strings_tail |  | ε |  |  |  |  |  |
| `strings_tail` | , STRING strings_tail |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |  |  |  |  |  |
| `schema_map` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | { entries } |  |
| `entries` |  |  |  |  |  |  |  |  |  |  |  |  |  |  | STRING : schema entries_tail |  |  |  |  |  |  | ε |
| `entries_tail` | , STRING : schema entries_tail |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |
| `schemas` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε | schema schemas_tail |  | schema schemas_tail | schema schemas_tail |  |
| `schemas_tail` | , schema schemas_tail |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |  |  |  |  |  |
| `dep_map` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | { deps } |  |
| `deps` |  |  |  |  |  |  |  |  |  |  |  |  |  |  | STRING : string_array deps_tail |  |  |  |  |  |  | ε |
| `deps_tail` | , STRING : string_array deps_tail |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |
| `value` |  |  |  |  |  |  |  |  |  |  |  |  |  | NUMBER | STRING | array |  | false | null | true | object |  |
| `object` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | { members } |  |
| `members` |  |  |  |  |  |  |  |  |  |  |  |  |  |  | pair members_tail |  |  |  |  |  |  | ε |
| `members_tail` | , pair members_tail |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |
| `pair` |  |  |  |  |  |  |  |  |  |  |  |  |  |  | STRING : value |  |  |  |  |  |  |  |
| `array` |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | [ elements ] |  |  |  |  |  |  |
| `elements` |  |  |  |  |  |  |  |  |  |  |  |  |  | value elements_tail | value elements_tail | value elements_tail | ε | value elements_tail | value elements_tail | value elements_tail | value elements_tail |  |
| `elements_tail` | , value elements_tail |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | ε |  |  |  |  |  |

## Conflictos

Ninguno: cada celda tiene a lo sumo una producción, así que **la gramática es LL(1)**.
