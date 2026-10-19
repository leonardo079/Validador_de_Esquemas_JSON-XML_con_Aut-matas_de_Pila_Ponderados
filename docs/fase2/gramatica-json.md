# Fase 2 — Gramática libre de contexto de JSON

La sintaxis de JSON no es regular: el lenguaje de los arreglos balanceados `[ᵏ ]ᵏ` ya exige contar, y por el lema de bombeo ningún AFD lo reconoce. Se describe con una gramática libre de contexto (GLC) sobre los **tokens** que entrega el lexer de la Fase 1, no sobre caracteres.

Terminales: `{ } [ ] : ,` `STRING` `NUMBER` `true` `false` `null`.

## 1. Gramática tal como se escribe naturalmente

Siguiendo la RFC 8259, con las listas escritas por recursión:

```
value    → object | array | STRING | NUMBER | true | false | null
object   → { } | { members }
members  → members , pair | pair
pair     → STRING : value
array    → [ ] | [ elements ]
elements → elements , value | value
```

Esta gramática genera exactamente los documentos JSON y no es ambigua, pero **no es LL(1)** (informe completo en [ll1-json-izquierda.md](ll1-json-izquierda.md)):

- `members → members , pair` y `elements → elements , value` tienen **recursión por la izquierda**: un parser descendente entraría en un ciclo infinito, y en la tabla las dos alternativas de `members` comparten FIRST = {`STRING`}.
- `object → { } | { members }` y `array → [ ] | [ elements ]` comparten el **prefijo** `{` (o `[`): con un token de anticipación no se puede elegir la alternativa.

## 2. Transformación a LL(1)

**Eliminación de la recursión izquierda.** Una regla `A → A α | β` genera `β α*`; se reescribe como `A → β A'`, `A' → α A' | ε`:

```
members      → pair members_tail
members_tail → , pair members_tail | ε
```

y lo mismo para `elements`.

**Factorización por la izquierda.** Una regla `A → α β₁ | α β₂` se reescribe como `A → α A'`, `A' → β₁ | β₂`. Para `object`:

```
object → { object_rest
object_rest → } | members }
```

Las dos alternativas de `object_rest` terminan en `}`, y la primera es la segunda con la lista vacía. Si se permite que la lista derive ε, ambas se funden en una sola y desaparece `object_rest`:

```
object  → { members }
members → pair members_tail | ε
```

Lo mismo se hace con `array` y `elements`.

## 3. Gramática LL(1) resultante

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

Archivo fuente: [json.ll1](json.ll1). Los conjuntos FIRST y FOLLOW y la tabla LL(1) los calcula `scripts/ll1.py` y están en **[ll1-json.md](ll1-json.md)**.

### Por qué es LL(1)

Una gramática es LL(1) si para cada no terminal A y cada par de alternativas α, β:

1. FIRST(α) ∩ FIRST(β) = ∅, y
2. si β ⇒* ε, entonces FIRST(α) ∩ FOLLOW(A) = ∅.

Se comprueba en cada no terminal con alternativas:

| No terminal | Alternativas y sus FIRST | FOLLOW | ¿Disjuntos? |
|---|---|---|---|
| `value` | `{` · `[` · `STRING` · `NUMBER` · `true` · `false` · `null` | — | Sí: siete terminales distintos |
| `members` | {`STRING`} · ε | {`}`} | Sí: `STRING` ∉ {`}`} |
| `members_tail` | {`,`} · ε | {`}`} | Sí |
| `elements` | FIRST(value) · ε | {`]`} | Sí: `]` ∉ FIRST(value) |
| `elements_tail` | {`,`} · ε | {`]`} | Sí |

El script llega a la misma conclusión: ninguna celda de la tabla tiene dos producciones.

## 4. Ejemplo de derivación

Derivación por la izquierda de `{"a": [1, true]}`, que es exactamente la secuencia de decisiones del parser LL(1) (cada paso consulta la tabla con el no terminal más a la izquierda y el siguiente token):

```
value
⇒ object                                         (value, {)
⇒ { members }                                    (object, {)
⇒ { pair members_tail }                          (members, STRING)
⇒ { STRING : value members_tail }                (pair, STRING)
⇒ { STRING : array members_tail }                (value, [)
⇒ { STRING : [ elements ] members_tail }         (array, [)
⇒ { STRING : [ value elements_tail ] members_tail }       (elements, NUMBER)
⇒ { STRING : [ NUMBER elements_tail ] members_tail }      (value, NUMBER)
⇒ { STRING : [ NUMBER , value elements_tail ] members_tail }  (elements_tail, ,)
⇒ { STRING : [ NUMBER , true elements_tail ] members_tail }   (value, true)
⇒ { STRING : [ NUMBER , true ] members_tail }    (elements_tail, ])
⇒ { STRING : [ NUMBER , true ] }                 (members_tail, })
```

Árbol de derivación:

```
value
└── object
    ├── {
    ├── members
    │   ├── pair
    │   │   ├── STRING "a"
    │   │   ├── :
    │   │   └── value
    │   │       └── array
    │   │           ├── [
    │   │           ├── elements
    │   │           │   ├── value ── NUMBER 1
    │   │           │   └── elements_tail
    │   │           │       ├── ,
    │   │           │       ├── value ── true
    │   │           │       └── elements_tail ── ε
    │   │           └── ]
    │   └── members_tail ── ε
    └── }
```

El compilador exportará estos árboles en JSON (para la web) y DOT (para la documentación) en la semana 2.

## 5. Relación con el autómata de pila

El parser LL(1) dirigido por tabla **es** un autómata de pila: la pila guarda los símbolos pendientes de la forma sentencial y cada paso es una transición determinista (no terminal en la cima, token actual) → producción. El validador de la Fase 4 parte de esta misma idea, pero la pila no guarda símbolos de la gramática de JSON sino **marcos del esquema**: qué nodo del esquema se está validando y sus registros (máscara de `required`, contadores de `minItems`…). La profundidad de la pila es la profundidad de anidamiento del documento, de ahí el costo O(d) en memoria.
