# Fase 2 — Gramática de XML bien formado

## 1. Tokens

La gramática trabaja sobre los eventos del lexer de la Fase 1, agrupados en seis terminales:

| Terminal | Eventos del lexer | Ejemplo |
|---|---|---|
| `STAG` | `StartTagOpen` + `NameChar`⁺ | `<pedido` |
| `ATTR` | `AttrBegin` + `NameChar`⁺ + `AttrValueBegin` … `AttrValueEnd` | `id="7"` |
| `>` | `StartTagClose` | `>` |
| `/>` | `EmptyTagClose` | `/>` |
| `ETAG` | `EndTagOpen` + `NameChar`⁺ + `EndTagClose` | `</pedido>` |
| `TEXT` | `TextChar`⁺ (máximo) | `Ñandú &amp; Cía` |

Los comentarios, las instrucciones de procesamiento y la declaración `<?xml …?>` no producen eventos: el lexer los reconoce y los descarta, así que no aparecen en la gramática.

## 2. Gramática LL(1)

```
document     → misc element misc
misc         → TEXT misc | ε
element      → STAG attrs element_rest
element_rest → /> | > content ETAG
attrs        → ATTR attrs | ε
content      → TEXT content | element content | ε
```

Archivo fuente: [xml.ll1](xml.ll1). FIRST, FOLLOW y la tabla LL(1) están en **[ll1-xml.md](ll1-xml.md)**; el script no encuentra conflictos.

La gramática ya se escribió sin recursión izquierda (`attrs` y `content` son recursivas por la derecha) y con `element_rest` factorizado: la alternativa vacía `/>` y la con contenido `> content ETAG` comparten el prefijo `STAG attrs`.

Comprobación de las condiciones LL(1):

| No terminal | FIRST de cada alternativa | FOLLOW | ¿Disjuntos? |
|---|---|---|---|
| `misc` | {`TEXT`} · ε | {`STAG`, `$`} | Sí |
| `element_rest` | {`/>`} · {`>`} | — | Sí |
| `attrs` | {`ATTR`} · ε | {`/>`, `>`} | Sí |
| `content` | {`TEXT`} · {`STAG`} · ε | {`ETAG`} | Sí |

**Restricciones que no expresa la gramática** y que se verifican aparte: el texto de `misc` (fuera del elemento raíz) solo puede tener espacios, y dos atributos de la misma etiqueta no pueden llamarse igual.

## 3. El emparejamiento de nombres no es libre de contexto

La gramática anterior acepta `<a></b>`: no exige que el nombre de `ETAG` sea el de su `STAG`. Esa omisión no es un descuido; **sobre un alfabeto de nombres arbitrario, el emparejamiento no se puede expresar con ninguna GLC.**

**Proposición.** Sea L el lenguaje de los documentos XML bien formados sobre caracteres. L no es libre de contexto.

**Demostración.** Supongamos que L es libre de contexto. Los lenguajes libres de contexto son cerrados bajo intersección con lenguajes regulares y bajo homomorfismos.

1. Sea R el lenguaje regular `<(a|b)⁺></(a|b)⁺>`. Entonces L ∩ R = { `<w></w>` : w ∈ {a,b}⁺ }, porque en un documento bien formado el nombre de cierre es igual al de apertura.
2. Sea h el homomorfismo que borra `<` y `>` y lleva `/` a `#` (y deja `a`, `b` igual). Entonces h(L ∩ R) = { w#w : w ∈ {a,b}⁺ }.
3. El lenguaje de copia { w#w } **no** es libre de contexto. Por el lema de bombeo: dada la constante p, se toma z = aᵖbᵖ#aᵖbᵖ. Cualquier partición z = uvxyz' con |vxy| ≤ p y |vy| ≥ 1 deja vxy dentro de una sola mitad o a caballo del `#` tocando solo bᵖ#aᵖ; en el primer caso bombear desequilibra una mitad respecto de la otra, en el segundo cambia los b de la izquierda y los a de la derecha de forma que las mitades ya no coinciden (o desaparece el `#`). En ningún caso uv²xy²z' pertenece al lenguaje.

Como 3 contradice 1 y 2, L no es libre de contexto. ∎

Intuitivamente: una pila puede recordar un nombre, pero lo devuelve **al revés** (es una pila), así que sirve para reconocer palíndromos `w#wᴿ` y no copias `w#w`.

## 4. Cómo se resuelve: el esquema fija un alfabeto finito de nombres

El validador nunca necesita aceptar nombres arbitrarios: el esquema XSD enumera los elementos permitidos. Sea N = {n₁, …, nₖ} ese conjunto finito. Si el lexer reconoce cada nombre de N (un trie, que es un AFD) y lo entrega como un terminal propio, `STAGₙ` y `ETAGₙ`, la gramática se vuelve:

```
element      → STAG_n₁ attrs rest_n₁ | … | STAG_nₖ attrs rest_nₖ
rest_nᵢ      → /> | > content ETAG_nᵢ          (para cada nᵢ ∈ N)
```

Esta gramática sí es libre de contexto (una cantidad finita de producciones) y sigue siendo LL(1): las alternativas de `element` empiezan por terminales distintos. Un nombre fuera de N es directamente un error de esquema.

En el autómata de pila de la Fase 4 esto se traduce en que el **alfabeto de pila Γ incluye N**: al leer `STAGₙ` se apila n, y al leer `ETAGₘ` se compara m con la cima y se desapila. Como N es finito, Γ es finito y el autómata es un DPDA legítimo. En la práctica la pila guarda el identificador del nodo del esquema, que ya determina el nombre esperado.

## 5. Ejemplo de derivación

`<p id="7"><c>Ana</c><v/></p>` se tokeniza como `STAG ATTR > STAG > TEXT ETAG STAG /> ETAG`:

```
document
⇒ misc element misc
⇒ element misc                                   (misc, STAG → ε)
⇒ STAG attrs element_rest misc
⇒ STAG ATTR attrs element_rest misc
⇒ STAG ATTR element_rest misc                    (attrs, > → ε)
⇒ STAG ATTR > content ETAG misc
⇒ STAG ATTR > element content ETAG misc
⇒ STAG ATTR > STAG attrs element_rest content ETAG misc
⇒ STAG ATTR > STAG element_rest content ETAG misc
⇒ STAG ATTR > STAG > content ETAG content ETAG misc
⇒ STAG ATTR > STAG > TEXT content ETAG content ETAG misc
⇒ STAG ATTR > STAG > TEXT ETAG content ETAG misc            (content, ETAG → ε)
⇒ STAG ATTR > STAG > TEXT ETAG element content ETAG misc
⇒ STAG ATTR > STAG > TEXT ETAG STAG attrs element_rest content ETAG misc
⇒ STAG ATTR > STAG > TEXT ETAG STAG /> content ETAG misc
⇒ STAG ATTR > STAG > TEXT ETAG STAG /> ETAG misc
⇒ STAG ATTR > STAG > TEXT ETAG STAG /> ETAG                  (misc, $ → ε)
```
