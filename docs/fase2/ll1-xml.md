<!-- Generado por `python3 scripts/ll1.py docs/fase2/xml.ll1`. No editar a mano. -->

## Gramática

```
document     → misc element misc
misc         → TEXT misc | ε
element      → STAG attrs element_rest
element_rest → /> | > content ETAG
attrs        → ATTR attrs | ε
content      → TEXT content | element content | ε
```

Símbolo inicial: `document`. Terminales: `/>`, `>`, `ATTR`, `ETAG`, `STAG`, `TEXT`.

## Conjuntos FIRST y FOLLOW

| No terminal | FIRST | FOLLOW |
|---|---|---|
| `document` | `STAG`, `TEXT` | `$` |
| `misc` | `TEXT`, `ε` | `STAG`, `$` |
| `element` | `STAG` | `ETAG`, `STAG`, `TEXT`, `$` |
| `element_rest` | `/>`, `>` | `ETAG`, `STAG`, `TEXT`, `$` |
| `attrs` | `ATTR`, `ε` | `/>`, `>` |
| `content` | `STAG`, `TEXT`, `ε` | `ETAG` |

## Tabla LL(1)

Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como
siguiente token. Una celda vacía es un error sintáctico.

| | `/>` | `>` | `ATTR` | `ETAG` | `STAG` | `TEXT` | `$` |
|---|---|---|---|---|---|---|---|
| `document` |  |  |  |  | misc element misc | misc element misc |  |
| `misc` |  |  |  |  | ε | TEXT misc | ε |
| `element` |  |  |  |  | STAG attrs element_rest |  |  |
| `element_rest` | /> | > content ETAG |  |  |  |  |  |
| `attrs` | ε | ε | ATTR attrs |  |  |  |  |
| `content` |  |  |  | ε | element content | TEXT content |  |

## Conflictos

Ninguno: cada celda tiene a lo sumo una producción, así que **la gramática es LL(1)**.
