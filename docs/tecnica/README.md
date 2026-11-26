# Documentación técnica

**Validación en tiempo real de esquemas JSON/XML mediante autómatas de pila ponderados.**
Proyecto final de Lenguajes Formales y Autómatas 2026-2, UPTC Sogamoso.

Esta carpeta es el **entregable 1** del enunciado (§5): «definición formal del lenguaje, diagramas de autómatas (AFD,
DPDA), gramáticas regulares y libres de contexto, arquitectura del compilador y árboles de derivación». Está
organizada **fase por fase**, como el §3 del enunciado.

**Cómo se escribió.** Se rehízo el 25 nov 2026 leyendo el **código** del repositorio, no los documentos anteriores
de `docs/faseN/`. Toda cifra (estados, clases, tamaños de M, métricas) sale de una ejecución hecha al escribirla. Los
diagramas y las tablas LL(1) que se enlazan desde `docs/faseN/` son **generados** por el propio código (`schemac
dot`, `schemac ir --dot`, `schemac automaton --dot`, `arbol --dot`, `scripts/ll1.py`). Antes de enlazarlos se volvieron
a generar y se compararon: todos coinciden byte a byte con la versión del repositorio. Las gráficas de bombeo de
[graficas/](graficas/) se midieron y dibujaron de nuevo para este documento.

## Índice

| # | Documento | Enunciado §3 |
| --- | --- | --- |
| 1 | [fase1-lexico.md](fase1-lexico.md) | alfabeto, tokens, AFD en streaming, gramáticas regulares, lexer del esquema |
| 2 | [fase2-sintactico.md](fase2-sintactico.md) | GLC de JSON, XML y del metalenguaje; parser LL(1); árboles de derivación |
| 3 | [fase3-semantico.md](fase3-semantico.md) | meta-esquema, `$ref`/`$defs`, ciclos, IR, clasificación (a)/(b)/(c) |
| 4 | [fase4-automata.md](fase4-automata.md) | M = (Q, Σ, Γ, δ, q₀, Z₀, F, w), pesos tropicales, construcción, determinismo, minimización |
| 5 | [fase5-codigo.md](fase5-codigo.md) | emisores C y Rust, tablas densas, API C-ABI, generadores de payloads |
| 6 | [fase6-tiempo-real.md](fase6-tiempo-real.md) | streaming, JSON Pointer, límites, métricas frente a DOM, bombeo, mitigaciones |
| 7 | [fase7-web.md](fase7-web.md) | API REST y WebSocket, web, sandbox, MongoDB Atlas, despliegue en AWS |
| — | [arquitectura.md](arquitectura.md) | arquitectura del compilador |

## Dónde está cada pieza del entregable 1

| Lo que pide el enunciado | Dónde |
| --- | --- |
| Definición formal del lenguaje | §1 de este archivo; Fase 4 §1-§3 |
| Diagramas de AFD | Fase 1: [UTF-8](../fase1/afd-utf8.svg), [JSON](../fase1/afd-json.svg), [XML](../fase1/afd-xml.svg), [DTD](../fase1/afd-dtd.svg) |
| Diagramas de DPDA | Fase 4: [objeto cerrado](../fase4/automata-objeto-cerrado.svg), [árbol recursivo](../fase4/automata-arbol-recursivo.svg), [pedido XSD](../fase4/automata-valid-xml-pedido-01.svg) |
| Gramáticas regulares | Fase 1 §2-§3 (UTF-8, números, cadenas, literales) |
| Gramáticas libres de contexto | Fase 2 §2-§6 (JSON, XML, JSON Schema, XSD, DTD) con sus tablas LL(1) |
| Arquitectura del compilador | [arquitectura.md](arquitectura.md) |
| Árboles de derivación | Fase 2 §7: [pedido.json](../fase5/arbol-pedido.json.svg), [pedido.xml](../fase5/arbol-pedido.xml.svg) |

## 1. Definición formal del lenguaje

**Alfabeto.** Σ_b = {0, …, 255}: el validador lee bytes. La decodificación UTF-8 es parte del análisis léxico.

**Payloads JSON.** L_JSON ⊂ Σ_b* son las cadenas que:

- son UTF-8 válido, con un BOM opcional al principio;
- dan, tras el análisis léxico (Fase 1), una sucesión de tokens derivable desde `value` en la GLC de la Fase 2 §2;
- no tienen claves repetidas en un mismo objeto.

La parte léxica es regular. La sintáctica es libre de contexto y **no regular** ({`[`ᵏ `]`ᵏ}). La restricción de
claves no es libre de contexto y se comprueba aparte, con una máscara de bits y un hash acotado.

**Payloads XML.** L_XML ⊂ Σ_b* son los documentos XML 1.0 bien formados en UTF-8, **sin** `<!DOCTYPE` ni entidades
distintas de las cinco predefinidas. Sobre un alfabeto de nombres no acotado, la igualdad de las etiquetas de
apertura y cierre **no es libre de contexto** (Fase 2 §3.1, por bombeo). Con el conjunto finito de nombres que fija el
esquema sí lo es.

**Esquemas.** 𝒮_J son los JSON Schema 2020-12 que usan solo las palabras clave soportadas y pasan la Fase 3
(meta-esquema, `$ref` locales resueltos, sin ciclos no productivos). 𝒮_X son los esquemas del subconjunto de XSD 1.0
(Fase 3 §8) y 𝒮_D los del subconjunto de DTD externas (Fase 3 §9). Para S ∈ 𝒮_J,

  L(S) = { w ∈ L_JSON : la instancia que denota w satisface S según 2020-12 },

y lo mismo sobre L_XML para 𝒮_X y 𝒮_D. Las tres familias bajan a la misma IR (Fase 3 §5).

**El autómata.** Para cada S el compilador construye un **autómata de pila ponderado determinista**

  M_S = (Q, Σ, Γ, δ, q₀, Z₀, F, w)

con Σ = las clases de eventos del lexer, Γ = marcos con registros acotados, δ total y determinista, F = {aceptación
con pila vacía} y w con valores en el **semianillo tropical** (ℕ ∪ {∞}, mín, +, ∞, 0). El validador es M_S ∘ T, donde
T es el lexer (un transductor finito determinista), y sigue siendo un DPDA (Fase 4 §2). Como es determinista, cada
w tiene un único recorrido, y su peso W(w) es la suma de los pesos de sus transiciones:

| W(w) | Significado |
| --- | --- |
| 0 | w ∈ L(S) |
| ∞ | w ∉ L_JSON (o L_XML), o w pasa un límite |
| k ≥ 1 | w está bien formado y viola k comprobaciones de S |

**Precisiones.**

1. **Límites.** Lo que se reconoce de verdad es L(S) ∩ {w que cabe en los límites Λ} (Fase 6 §4). Los registros
   saturan, así que Γ es finito y la pila es la única memoria no acotada (de profundidad ≤ `maxDepth`).
2. **Lo que no es DPDA puro** (`anyOf`, `oneOf`, `not`, `allOf`, `pattern`, `uniqueItems`) entra con una estrategia
   acotada: producto con tope, AFD con tope o verificación diferida (Fase 4 §4-§5). Si el tope se pasa, el
   **esquema** se rechaza al compilar; el validador nunca explota.
3. **Desviaciones de alcance.** Lo que el subconjunto no soporta se rechaza con su motivo, nunca se ignora
   (Fase 3 §2). En XSD, la UPA usa una abstracción conservadora que rechaza algunos modelos que XSD 1.0 admite
   (Fase 3 §8).

## 2. Complejidad (resumen)

| | Tiempo | Memoria |
| --- | --- | --- |
| Validar un payload de n bytes y profundidad d | **O(n)**: O(1) eventos por byte y O(1) por evento | **O(d)** de pila dentro de una memoria fija (72–78 KiB por defecto) |
| Compilar un esquema de tamaño s | O(s) en el análisis; la construcción de M está acotada por los topes de la Fase 4 §9 | O(\|Q\|·\|Σ\|) para δ, con tope de 2²⁰ entradas |

El análisis detallado, las mediciones y las pruebas de bombeo están en la [Fase 6](fase6-tiempo-real.md).

## 3. Cómo reproducir lo de este documento

```
cargo build --release -p cli -p derivation
target/release/schemac dot json                                    # AFD
target/release/schemac ir tests/schemas/parse-ok/objeto-cerrado.json       # IR
target/release/schemac automaton tests/schemas/parse-ok/arbol-recursivo.json  # tamaños de M
target/release/schemac compile <esquema> --lang c --out gen          # validador C
target/release/arbol --dot docs/fase5/ejemplos/pedido.json          # árbol de derivación
python3 scripts/ll1.py docs/fase2/json.ll1                          # tabla LL(1)
cargo run -p bench --release --bin metricas -- --bombeo             # métricas
make docs-check                                                     # los diagramas siguen al día
```
