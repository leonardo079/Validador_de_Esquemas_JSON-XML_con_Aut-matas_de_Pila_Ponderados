<!-- Generado por `python3 scripts/ll1.py docs/fase2/xml.ll1`. No editar a mano. -->

## Gramática

```
document     → misc element misc
misc         → TEXT misc | PI misc | ε
element      → STAG attrs element_rest
element_rest → /> | > content ETAG
attrs        → ATTR attrs | ε
content      → TEXT content | PI content | element content | ε
```

Símbolo inicial: `document`. Terminales: `/>`, `>`, `ATTR`, `ETAG`, `PI`, `STAG`, `TEXT`.

## Conjuntos FIRST y FOLLOW

| No terminal | FIRST | FOLLOW |
|---|---|---|
| `document` | `PI`, `STAG`, `TEXT` | `$` |
| `misc` | `PI`, `TEXT`, `ε` | `STAG`, `$` |
| `element` | `STAG` | `ETAG`, `PI`, `STAG`, `TEXT`, `$` |
| `element_rest` | `/>`, `>` | `ETAG`, `PI`, `STAG`, `TEXT`, `$` |
| `attrs` | `ATTR`, `ε` | `/>`, `>` |
| `content` | `PI`, `STAG`, `TEXT`, `ε` | `ETAG` |

## Tabla LL(1)

Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como
siguiente token. Una celda vacía es un error sintáctico.

| | `/>` | `>` | `ATTR` | `ETAG` | `PI` | `STAG` | `TEXT` | `$` |
|---|---|---|---|---|---|---|---|---|
| `document` |  |  |  |  | misc element misc | misc element misc | misc element misc |  |
| `misc` |  |  |  |  | PI misc | ε | TEXT misc | ε |
| `element` |  |  |  |  |  | STAG attrs element_rest |  |  |
| `element_rest` | /> | > content ETAG |  |  |  |  |  |  |
| `attrs` | ε | ε | ATTR attrs |  |  |  |  |  |
| `content` |  |  |  | ε | PI content | element content | TEXT content |  |

## Conflictos

Ninguno: cada celda tiene a lo sumo una producción, así que **la gramática es LL(1)**.
