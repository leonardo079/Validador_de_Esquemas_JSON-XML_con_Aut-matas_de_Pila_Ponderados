<!-- Generado por `python3 scripts/ll1.py docs/fase2/json.ll1`. No editar a mano. -->

## Gramática

```
value         → object | array | STRING | NUMBER | true | false | null
object        → { members }
members       → pair members_tail | ε
members_tail  → , pair members_tail | ε
pair          → STRING : value
array         → [ elements ]
elements      → value elements_tail | ε
elements_tail → , value elements_tail | ε
```

Símbolo inicial: `value`. Terminales: `,`, `:`, `NUMBER`, `STRING`, `[`, `]`, `false`, `null`, `true`, `{`, `}`.

## Conjuntos FIRST y FOLLOW

| No terminal | FIRST | FOLLOW |
|---|---|---|
| `value` | `NUMBER`, `STRING`, `[`, `false`, `null`, `true`, `{` | `,`, `]`, `}`, `$` |
| `object` | `{` | `,`, `]`, `}`, `$` |
| `members` | `STRING`, `ε` | `}` |
| `members_tail` | `,`, `ε` | `}` |
| `pair` | `STRING` | `,`, `}` |
| `array` | `[` | `,`, `]`, `}`, `$` |
| `elements` | `NUMBER`, `STRING`, `[`, `false`, `null`, `true`, `{`, `ε` | `]` |
| `elements_tail` | `,`, `ε` | `]` |

## Tabla LL(1)

Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como
siguiente token. Una celda vacía es un error sintáctico.

| | `,` | `NUMBER` | `STRING` | `[` | `]` | `false` | `null` | `true` | `{` | `}` |
|---|---|---|---|---|---|---|---|---|---|---|
| `value` |  | NUMBER | STRING | array |  | false | null | true | object |  |
| `object` |  |  |  |  |  |  |  |  | { members } |  |
| `members` |  |  | pair members_tail |  |  |  |  |  |  | ε |
| `members_tail` | , pair members_tail |  |  |  |  |  |  |  |  | ε |
| `pair` |  |  | STRING : value |  |  |  |  |  |  |  |
| `array` |  |  |  | [ elements ] |  |  |  |  |  |  |
| `elements` |  | value elements_tail | value elements_tail | value elements_tail | ε | value elements_tail | value elements_tail | value elements_tail | value elements_tail |  |
| `elements_tail` | , value elements_tail |  |  |  | ε |  |  |  |  |  |

## Conflictos

Ninguno: cada celda tiene a lo sumo una producción, así que **la gramática es LL(1)**.
