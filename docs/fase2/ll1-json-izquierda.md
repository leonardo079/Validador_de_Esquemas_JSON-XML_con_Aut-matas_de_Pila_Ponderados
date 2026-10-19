<!-- Generado por `python3 scripts/ll1.py docs/fase2/json-izquierda.ll1`. No editar a mano. -->

## Gramática

```
value    → object | array | STRING | NUMBER | true | false | null
object   → { } | { members }
members  → members , pair | pair
pair     → STRING : value
array    → [ ] | [ elements ]
elements → elements , value | value
```

Símbolo inicial: `value`. Terminales: `,`, `:`, `NUMBER`, `STRING`, `[`, `]`, `false`, `null`, `true`, `{`, `}`.

## Conjuntos FIRST y FOLLOW

| No terminal | FIRST | FOLLOW |
|---|---|---|
| `value` | `NUMBER`, `STRING`, `[`, `false`, `null`, `true`, `{` | `,`, `]`, `}`, `$` |
| `object` | `{` | `,`, `]`, `}`, `$` |
| `members` | `STRING` | `,`, `}` |
| `pair` | `STRING` | `,`, `}` |
| `array` | `[` | `,`, `]`, `}`, `$` |
| `elements` | `NUMBER`, `STRING`, `[`, `false`, `null`, `true`, `{` | `,`, `]` |

## Tabla LL(1)

Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como
siguiente token. Una celda vacía es un error sintáctico.

| | `NUMBER` | `STRING` | `[` | `false` | `null` | `true` | `{` |
|---|---|---|---|---|---|---|---|
| `value` | NUMBER | STRING | array | false | null | true | object |
| `object` |  |  |  |  |  |  | { } / { members } ⚠ |
| `members` |  | members , pair / pair ⚠ |  |  |  |  |  |
| `pair` |  | STRING : value |  |  |  |  |  |
| `array` |  |  | [ ] / [ elements ] ⚠ |  |  |  |  |
| `elements` | elements , value / value ⚠ | elements , value / value ⚠ | elements , value / value ⚠ | elements , value / value ⚠ | elements , value / value ⚠ | elements , value / value ⚠ | elements , value / value ⚠ |

## Conflictos

- (`array`, `[`): `[ ]` / `[ elements ]`
- (`elements`, `NUMBER`): `elements , value` / `value`
- (`elements`, `STRING`): `elements , value` / `value`
- (`elements`, `[`): `elements , value` / `value`
- (`elements`, `false`): `elements , value` / `value`
- (`elements`, `null`): `elements , value` / `value`
- (`elements`, `true`): `elements , value` / `value`
- (`elements`, `{`): `elements , value` / `value`
- (`members`, `STRING`): `members , pair` / `pair`
- (`object`, `{`): `{ }` / `{ members }`

**La gramática no es LL(1).**
