# Fase 2 — Gramática de XML bien formado

> **Cifras manuales.** Los archivos `ll1-*.md` los genera `scripts/ll1.py` (`make docs`) y la CI los comprueba (`make docs-check`). En cambio, el número de terminales de §1 y la tabla de §2 (FIRST y FOLLOW) de este documento son un resumen escrito a mano de esa salida: quien cambie la gramática (`xml.ll1`) debe actualizarlos en el mismo cambio y compararlos con el `ll1-xml.md` regenerado.

## 1. Tokens

La gramática trabaja sobre los eventos del lexer de la Fase 1, agrupados en siete terminales:

| Terminal | Eventos del lexer | Ejemplo |
|---|---|---|
| `STAG` | `StartTagOpen` + `NameChar`⁺ | `<pedido` |
| `ATTR` | `AttrBegin` + `NameChar`⁺ + `AttrValueBegin` … `AttrValueEnd` | `id="7"` |
| `>` | `StartTagClose` | `>` |
| `/>` | `EmptyTagClose` | `/>` |
| `ETAG` | `EndTagOpen` + `NameChar`⁺ + `EndTagClose` | `</pedido>` |
| `TEXT` | `TextChar`⁺ (máximo) | `Ñandú &amp; Cía` |
| `PI` | `PiOpen` + `NameChar`⁺ + `PiChar`* + `PiClose` | `<?xml version="1.0"?>` |

Los comentarios no producen eventos: el lexer los reconoce y los descarta. Las instrucciones de procesamiento, incluida la declaración `<?xml …?>`, sí: forman el terminal `PI`.

## 2. Gramática LL(1)

```
document     → misc element misc
misc         → TEXT misc | PI misc | ε
element      → STAG attrs element_rest
element_rest → /> | > content ETAG
attrs        → ATTR attrs | ε
content      → TEXT content | PI content | element content | ε
```

Archivo fuente: [xml.ll1](xml.ll1). FIRST, FOLLOW y la tabla LL(1) están en **[ll1-xml.md](ll1-xml.md)**; el script no encuentra conflictos.

La gramática ya se escribió sin recursión izquierda (`attrs` y `content` son recursivas por la derecha) y con `element_rest` factorizado: la alternativa vacía `/>` y la con contenido `> content ETAG` comparten el prefijo `STAG attrs`.

Comprobación de las condiciones LL(1):

| No terminal | FIRST de cada alternativa | FOLLOW | ¿Disjuntos? |
|---|---|---|---|
| `misc` | {`TEXT`} · {`PI`} · ε | {`STAG`, `$`} | Sí |
| `element_rest` | {`/>`} · {`>`} | — | Sí |
| `attrs` | {`ATTR`} · ε | {`/>`, `>`} | Sí |
| `content` | {`TEXT`} · {`PI`} · {`STAG`} · ε | {`ETAG`} | Sí |

**Restricciones que no expresa la gramática** y que se verifican aparte: el texto de `misc` (fuera del elemento raíz) solo puede tener espacios; dos atributos de la misma etiqueta no pueden llamarse igual; una `PI` de nombre `xml` (la declaración) solo puede ser el primer token del documento y, si trae `encoding`, debe ser `UTF-8`.

## 3. El emparejamiento de nombres no es libre de contexto

La gramática anterior acepta `<a></b>`: no exige que el nombre de `ETAG` sea el de su `STAG`. Esa omisión no es un descuido; **sobre un alfabeto de nombres arbitrario, el emparejamiento no se puede expresar con ninguna GLC.**

**Proposición.** Sea L el lenguaje de los documentos XML bien formados sobre caracteres. L no es libre de contexto.

**Herramientas.**

- *Cierre bajo intersección con regulares.* Si L es libre de contexto y R es regular, L ∩ R es libre de contexto (un autómata de pila para L y un AFD para R corren en producto; la pila es la del primero).
- *Lema de bombeo para lenguajes libres de contexto.* Si M es libre de contexto, existe p ≥ 1 tal que toda z ∈ M con |z| ≥ p se escribe z = u v w x y con |vwx| ≤ p y |vx| ≥ 1, y u vⁱ w xⁱ y ∈ M para todo i ≥ 0.

**Demostración.** Supongamos que L es libre de contexto. Sea R el lenguaje regular `<(a|b)⁺></(a|b)⁺>`. Un documento de R es bien formado exactamente cuando el nombre de cierre es igual al de apertura, así que

  M = L ∩ R = { `<w></w>` : w ∈ {a, b}⁺ }

y M es libre de contexto por la propiedad de cierre. Sea p la constante del lema de bombeo para M y tomemos

  z = `<` aᵖbᵖ `></` aᵖbᵖ `>` ∈ M,

con |z| = 4p + 5 ≥ p. Llamemos A al nombre de apertura y B al de cierre (los dos son aᵖbᵖ). Sea z = u v w x y una descomposición cualquiera con |vwx| ≤ p y |vx| ≥ 1. Hay cuatro casos:

1. **v o x contienen un símbolo `<`, `/` o `>`.** Toda cadena de R tiene exactamente dos `<`, un `/` y dos `>`. Con i = 0 se borra al menos uno, así que u w y ∉ R y por tanto u w y ∉ M.
2. **v y x están dentro de A** (alguno puede ser vacío). Con i = 0, A pierde |v| + |x| ≥ 1 símbolos y B no cambia: los nombres tienen longitudes distintas y u w y ∉ M.
3. **v y x están dentro de B.** Simétrico al caso 2.
4. **v ≠ ε está dentro de A y x ≠ ε está dentro de B.** Entonces vwx contiene la subcadena `></` que separa los nombres. Como |vwx| ≤ p, v está dentro de los últimos p símbolos de A, que son todos b, y x está dentro de los primeros p símbolos de B, que son todos a. Con i = 0 queda A' = aᵖ b^(p−|v|) y B' = a^(p−|x|) bᵖ. A' empieza con p letras a y B' con menos de p, así que A' ≠ B' y u w y ∉ M.

Como v y x no contienen marcas (caso 1 descartado), cada uno está completamente dentro de A o completamente dentro de B, y los casos 2 a 4 agotan las posibilidades. En todos, u v⁰ w x⁰ y ∉ M, lo que contradice el lema de bombeo. Luego M no es libre de contexto, y tampoco lo es L. ∎

**Por qué la cadena es aᵖbᵖ y no aᵖ.** Con z = `<aᵖ></aᵖ>` la demostración falla: tomando v = aᵏ al final de A y x = aᵏ al principio de B, la cadena bombeada es `<a^(p+(i−1)k)></a^(p+(i−1)k)>`, que pertenece a M para todo i, y no hay contradicción. Las b obligan a que, en el caso 4, lo que se bombea a cada lado caiga en bloques distintos (b a la izquierda, a a la derecha), y eso rompe la igualdad de los nombres.

**Por qué se interseca con R.** Bombear directamente sobre L no funciona: con v = `>` se obtiene `<w>></w>`, que **es** XML bien formado porque `>` está permitido en el texto. La intersección con R descarta esas cadenas y deja un lenguaje con forma fija.

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
