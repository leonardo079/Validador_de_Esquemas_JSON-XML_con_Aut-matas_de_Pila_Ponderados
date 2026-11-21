# Documentación técnica

Entregable 1 del enunciado (`docs/enunciado/Propuesta_Proyecto_Final_PDA_JSON.pdf`, §5): «definición formal del
lenguaje, diagramas de autómatas (AFD, DPDA), gramáticas regulares y libres de contexto, arquitectura del compilador
y árboles de derivación». Este archivo es la entrada: resume la definición formal (§2) y dice dónde está cada pieza.
El detalle de cada fase está en `docs/faseN/`; lo que sigue abierto, en `docs/faseN/pendientes.md`.
El informe final (entregable 4: complejidad, tamaño del autómata, decisiones, errores, vulnerabilidades y
bombeo) está en [informe-final.md](informe-final.md).

## 1. Mapa del entregable

| Lo que pide el enunciado | Dónde | Cómo se mantiene |
| --- | --- | --- |
| Definición formal del subconjunto soportado | [00-alcance.md](00-alcance.md) (lista blanca, clase (a)/(b)/(c), límites) y [fase3/clasificacion.md](fase3/clasificacion.md) | la tabla de clases la compara una prueba con el código |
| Definición formal del lenguaje y del autómata | §2 de este archivo; [fase4/automata.md](fase4/automata.md) §1-§5 (M, Σ, Γ, δ, pesos) | — |
| AFD del tokenizador (UTF-8, números, cadenas con escapes, literales, etiquetas) | [fase1/lexico.md](fase1/lexico.md); diagramas [afd-utf8](fase1/afd-utf8.svg), [afd-json](fase1/afd-json.svg), [afd-xml](fase1/afd-xml.svg) | `.dot` generado por `schemac dot`; `make docs-check` |
| Gramáticas regulares | [fase1/lexico.md](fase1/lexico.md) §2-§4 (UTF-8, número, cadena, tokens XML, lineales por la derecha) | — |
| Gramáticas libres de contexto con tabla LL(1) | [fase2/gramatica-json.md](fase2/gramatica-json.md), [gramatica-xml.md](fase2/gramatica-xml.md), [gramatica-esquema.md](fase2/gramatica-esquema.md) (metalenguaje), [gramatica-xsd.md](fase2/gramatica-xsd.md); tablas [ll1-json](fase2/ll1-json.md), [ll1-xml](fase2/ll1-xml.md), [ll1-esquema](fase2/ll1-esquema.md), [ll1-xsd](fase2/ll1-xsd.md) | tablas generadas por `scripts/ll1.py`; `make docs-check` |
| DPDA de al menos dos esquemas | §3: [objeto-cerrado](fase4/automata-objeto-cerrado.svg), [arbol-recursivo](fase4/automata-arbol-recursivo.svg) (JSON) y [pedido](fase4/automata-valid-xml-pedido-01.svg) (XSD) | `.dot` generado por `schemac automaton --dot`; `make docs-check` |
| Arquitectura del compilador | [arquitectura.md](arquitectura.md) | dependencias tomadas de `cargo metadata` |
| Árboles de derivación de payloads de ejemplo | [fase5/arbol-derivacion.md](fase5/arbol-derivacion.md); [pedido.json](fase5/arbol-pedido.json.svg), [pedido.xml](fase5/arbol-pedido.xml.svg) | `.dot` generado por `arbol --dot`; `make docs-check` |

## 2. Definición formal del lenguaje

**Alfabeto de entrada.** Σ_b = {0, …, 255}: el validador lee bytes, no caracteres. La decodificación UTF-8 es
parte del análisis léxico (un AFD, [fase1/lexico.md](fase1/lexico.md) §2).

**Lenguaje de los payloads JSON.** L_JSON ⊂ Σ_b\* son las cadenas que son UTF-8 válido (con un BOM opcional al
principio) y cuyo análisis léxico da una sucesión de tokens derivable desde `value` en la GLC de
[fase2/gramatica-json.md](fase2/gramatica-json.md), **sin claves repetidas** en un mismo objeto (alcance §1.1). La
parte léxica es regular; la sintáctica es libre de contexto y no regular (los corchetes balanceados `[ᵏ ]ᵏ`). La
restricción de claves no es libre de contexto. Se comprueba con la máscara de bits de las claves declaradas y, para
las demás, con una tabla hash acotada (§4 de [fase4/runtime.md](fase4/runtime.md)).

**Lenguaje de los payloads XML.** L_XML ⊂ Σ_b\* son los documentos XML 1.0 bien formados en UTF-8 sin DTD
(alcance §1.2). Sobre un alfabeto de nombres arbitrario, el emparejamiento de etiquetas no es libre de contexto
([fase2/gramatica-xml.md](fase2/gramatica-xml.md) §3, con la demostración por bombeo). Lo es porque el esquema fija un
conjunto finito de nombres (§4 del mismo archivo).

**Esquemas.** 𝒮_J son los JSON Schema 2020-12 que usan solo la lista blanca de [00-alcance.md](00-alcance.md) §2 y
pasan la Fase 3: el meta-esquema, la resolución de `$ref` y la ausencia de ciclos que no consumen entrada. 𝒮_X es
lo mismo con el subconjunto de XSD 1.0 de §3. Para S ∈ 𝒮_J,

  L(S) = { w ∈ L_JSON : la instancia que denota w satisface S según 2020-12 }

y, para S ∈ 𝒮_X, lo mismo sobre L_XML con la validación de XSD 1.0, con las desviaciones de alcance §3.4.

**El autómata.** El compilador construye, para cada S, un autómata de pila ponderado determinista

  M_S = (Q, Σ, Γ, δ, q₀, Z₀, F, w)

([fase4/automata.md](fase4/automata.md) §1):

- **Σ:** las clases de eventos que emite el lexer, que es un transductor finito determinista (Mealy) de Σ_b\* a
  Σ\*.
- **Γ:** marcos con registros acotados (máscara de claves, contadores, salidas de combinadores).
- **δ:** total y determinista: una transición por (q, a, X).
- **w:** pesos en el semianillo tropical (ℕ ∪ {∞}, mín, +, ∞, 0).

El validador es la composición del lexer con M_S, que sigue siendo un DPDA (§2 del mismo archivo). Como es
determinista, cada w tiene **un** recorrido, y su peso W(w) es la suma de los pesos de sus transiciones. Lo que el
compilador garantiza, y lo que comprueban el corpus, la línea base y el fuzzing, es:

| W(w) | Significado |
| --- | --- |
| 0 | w ∈ L(S) |
| ∞ | w ∉ L_JSON (o L_XML), o w pasa un límite |
| k ∈ ℕ, k ≥ 1 | w está bien formado y viola k restricciones de S (en conformidad total; `required` con dos claves ausentes cuenta 1, F4-U3 y D4) |

**Las tres precisiones.** Hay que añadir tres a la tabla:

1. **Límites.** Con los límites Λ ([00-alcance.md](00-alcance.md) §4, [fase6/limites.md](fase6/limites.md)), lo que
   se reconoce es L(S) ∩ {w que cabe en Λ}. Los registros se saturan, así que su conjunto de valores es finito y M_S
   es un autómata de pila en sentido estricto, con la pila como única memoria no acotada (automata.md §1). Con la
   profundidad acotada por `maxDepth`, la pila cabe en una memoria fija: a lo sumo `maxDepth` marcos de tamaño fijo.
2. **Clase (c).** `pattern` es un AFD dentro de M_S, así que queda en el lenguaje regular. `anyOf`, `oneOf`, `not`
   y `dependentSchemas` son salidas por rama combinadas al cerrar la instancia: los autómatas de pila visible son
   cerrados bajo las operaciones booleanas (automata.md §18). `uniqueItems` no es libre de contexto y se resuelve
   con verificación diferida: un hash por elemento, comparados al cerrar el arreglo (automata.md §19). Un choque de
   hash da un falso «repetido» (un rechazo), con probabilidad ≤ n²/2⁶⁵ con una clave secreta. Bajo `not`, ese
   rechazo se invierte y el choque haría aceptar.
3. **Claves no declaradas repetidas.** Se detectan con la misma familia de hash, y un choque da un falso rechazo.
   La clave por defecto de la biblioteca es pública. Los binarios de `schemac compile` toman una aleatoria al
   arrancar, así que quien escribe el payload no puede fabricar choques ([fase4/runtime.md](fase4/runtime.md) §4,
   [fase6/pendientes.md](fase6/pendientes.md) §31).

**Clasificación.** (a) directas en δ, (b) registros acotados en el marco y (c) producto, determinización acotada,
AFD o verificación diferida. Están en [00-alcance.md](00-alcance.md) (columna «Cat.») y en
[fase3/clasificacion.md](fase3/clasificacion.md), con la estrategia de cada `RuleKind`.

**Complejidad.**
- **Validación:** O(n) en bytes, O(1) por evento respecto del payload, pila O(d) con marcos de tamaño fijo y cero
  reservas de memoria (automata.md §7, [fase4/runtime.md](fase4/runtime.md)).
- **Construcción:** acotada por un presupuesto de 2²⁰ entradas para δ y las demás tablas (|Q|·|Σ| más las que
  crecen con los productos), contado antes de reservar.
- **Mediciones:** [fase6/metricas.md](fase6/metricas.md); pruebas del lema de bombeo y gráficas de tiempo, pila y
  memoria contra k en [fase6/bombeo.md](fase6/bombeo.md) (E1).

## 3. Diagramas de autómatas

### AFD (Fase 1)

| Diagrama | Qué muestra |
| --- | --- |
| [afd-utf8](fase1/afd-utf8.svg) | RFC 3629: rechaza *overlongs*, sustitutos y > U+10FFFF por la forma del autómata |
| [afd-json](fase1/afd-json.svg) | Tokenizador JSON completo: literales `true`/`false`/`null`, números (gramática de §3 de `lexico.md`), cadenas con escapes y `\u` con pares sustitutos (transiciones punteadas: dependen de un registro) |
| [afd-xml](fase1/afd-xml.svg) | Tokens XML: etiquetas de apertura y cierre, atributos, referencias, comentarios, CDATA, instrucciones de procesamiento y referencias de entidad o de carácter. De `<!` solo salen `-` y `[`: un `<!DOCTYPE` va al error (D8) |

### DPDA (Fase 4)

Se dibujan con `schemac automaton --dot <esquema>`:

- Cada clúster es un **grupo** de Q: el subautómata de un conjunto de nodos de la IR o de «cualquiera».
- Una etiqueta `a / op [comprobación c=costo]` es una transición sobre la clase de evento a. `✗` marca una
  comprobación que falla y suma su peso.
- `apila X ↩q` empuja el marco X con estado de retorno q, y `↩ marco` es desapilar.

| Esquema | Tamaño | Qué ilustra |
| --- | --- | --- |
| [objeto-cerrado](fase4/automata-objeto-cerrado.svg) ([esquema](../tests/schemas/parse-ok/objeto-cerrado.json)) | 24 estados, 77 transiciones | La dorada contada a mano en [automata.md](fase4/automata.md) §8: trie de claves, máscara de `required`, `additionalProperties: false`. |
| [arbol-recursivo](fase4/automata-arbol-recursivo.svg) ([esquema](../tests/schemas/parse-ok/arbol-recursivo.json)) | 41 estados, 308 transiciones | Recursión por `$ref` (abajo). |
| [pedido](fase4/automata-valid-xml-pedido-01.svg) ([esquema](../tests/schemas/xsd/parse-ok/valid-xml-pedido-01.xsd)) | 216 estados, 10 054 transiciones | XSD: marcos de elemento con despacho por nombre, autómata de Glushkov con contadores del modelo de contenido, atributos y textos con tipo simple. |

**Cómo se ve la recursión en `arbol-recursivo`.** El esquema es un nodo
`{"value": integer, "children": [nodo…]}` con `required: ["value"]` y `additionalProperties: false`.

- Q es finito y cada grupo se dibuja una sola vez: el objeto `#/$defs/node` (q13…q36), el arreglo `children`
  (q38-q40) y el número `value` (q37).
- Lo que no está acotado es la pila. En q33, «valor de `children`», `[` apila el marco del arreglo con retorno q36
  y va a q38. En q38 o en q39, `{` apila **otra vez** el marco de `#/$defs/node` con retorno q40 y vuelve a q13,
  el mismo estado en que empezó la raíz (q0 → q13).
- Cada `}` o `]` desapila y sigue en el estado de retorno guardado en el marco: el control vuelve al grupo de
  arriba.
- La comprobación `required` se lee en el `}` (q13 o q36 → `↩ marco`, `[required máscara c=2]`).
- Un valor de tipo incorrecto en `value` (q31) o una clave desconocida (q35) se recorren con los marcos de
  «cualquiera» y suman una violación (`type ✗`, `false ✗`) sin dejar de leer.

## 4. Gramáticas

| Nivel | Gramática | LL(1) |
| --- | --- | --- |
| Léxico (regular) | UTF-8, número, cadena y tokens XML como gramáticas lineales por la derecha y expresiones regulares ([fase1/lexico.md](fase1/lexico.md)) | — (AFD) |
| JSON | [gramatica-json.md](fase2/gramatica-json.md): la natural (recursiva por la izquierda, [ll1-json-izquierda](fase2/ll1-json-izquierda.md) muestra los conflictos) y la factorizada | [ll1-json](fase2/ll1-json.md) |
| XML bien formado | [gramatica-xml.md](fase2/gramatica-xml.md), con la prueba de que el emparejamiento de nombres no es libre de contexto y cómo lo resuelve el esquema | [ll1-xml](fase2/ll1-xml.md) |
| Metalenguaje de esquemas | [gramatica-esquema.md](fase2/gramatica-esquema.md) (JSON Schema como lenguaje sobre los tokens JSON) | [ll1-esquema](fase2/ll1-esquema.md) |
| XSD | [gramatica-xsd.md](fase2/gramatica-xsd.md) y el lector con espacios de nombres ([lector-xsd.md](fase2/lector-xsd.md)) | [ll1-xsd](fase2/ll1-xsd.md) |

El parser del esquema es el descendente LL(1) de esas tablas. El AST solo existe al compilar: el validador generado
no construye árbol (Fase 5).

## 5. Árboles de derivación

[fase5/arbol-derivacion.md](fase5/arbol-derivacion.md) explica el generador (crate `derivation`, iterativo, salida
DOT y JSON para d3). Los ejemplos son [pedido.json](fase5/ejemplos/pedido.json) →
[árbol](fase5/arbol-pedido.json.svg) y [pedido.xml](fase5/ejemplos/pedido.xml) → [árbol](fase5/arbol-pedido.xml.svg).
La aplicación web dibuja el de cualquier payload (`POST /api/derivation`, [fase7/vistas.md](fase7/vistas.md)).

## 6. Índice por fase

| Fase | Documentos |
| --- | --- |
| 1. Léxico | [lexico.md](fase1/lexico.md) |
| 2. Sintáctico | [gramatica-json.md](fase2/gramatica-json.md), [gramatica-xml.md](fase2/gramatica-xml.md), [gramatica-esquema.md](fase2/gramatica-esquema.md), [gramatica-xsd.md](fase2/gramatica-xsd.md), [lector-xsd.md](fase2/lector-xsd.md) |
| 3. Semántico | [meta-esquema.md](fase3/meta-esquema.md), [referencias.md](fase3/referencias.md), [ciclos.md](fase3/ciclos.md), [ir.md](fase3/ir.md), [bajada.md](fase3/bajada.md), [clasificacion.md](fase3/clasificacion.md), [xsd.md](fase3/xsd.md), [numeros-xsd.md](fase3/numeros-xsd.md); IR dibujada: [arbol-recursivo](fase3/ir-arbol-recursivo.svg), [pedido](fase3/ir-pedido.svg) |
| 4. Autómata | [automata.md](fase4/automata.md), [runtime.md](fase4/runtime.md), [pattern.md](fase4/pattern.md) |
| 5. Código | [codegen.md](fase5/codegen.md) (Rust), [codegen-c.md](fase5/codegen-c.md) (C), [generador.md](fase5/generador.md), [arbol-derivacion.md](fase5/arbol-derivacion.md) |
| 6. Tiempo real | [puntero.md](fase6/puntero.md), [limites.md](fase6/limites.md), [mitigaciones.md](fase6/mitigaciones.md), [fuzzing.md](fase6/fuzzing.md), [mutaciones.md](fase6/mutaciones.md), [linea-base.md](fase6/linea-base.md), [metricas.md](fase6/metricas.md), [bombeo.md](fase6/bombeo.md) (gráficas en [graficas/](fase6/graficas/datos.md)) |
| 7. Aplicación | [servidor.md](fase7/servidor.md), [vistas.md](fase7/vistas.md), [sandbox.md](fase7/sandbox.md), [codigo.md](fase7/codigo.md), [atlas.md](fase7/atlas.md), [despliegue.md](fase7/despliegue.md) |

Las decisiones (D1-D79) están en [`tasks/plan-general.md`](../tasks/plan-general.md), «Registro de decisiones». Lo
cerrado de cada revisión está en `docs/faseN/historial.md`.

## 7. Regenerar y comprobar

```text
make docs        # regenera los .dot, los .svg y las tablas ll1-*.md (requiere Graphviz)
make docs-check  # comprueba que los .dot y ll1-*.md versionados son los que da el código
```

Los `.svg` no entran en `docs-check`: `dot -Tsvg` escribe la versión de Graphviz y coordenadas que dependen de las
fuentes del sistema. Se regeneran con `make docs` desde el `.dot` comprobado. Las cifras escritas a mano (tamaños
de los autómatas en las tablas, FIRST/FOLLOW resumidos) se actualizan a mano junto con el cambio que las mueve.
