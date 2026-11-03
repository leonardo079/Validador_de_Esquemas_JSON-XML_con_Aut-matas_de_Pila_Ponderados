# Plan de implementación: tarea 5 de la semana 2 — frontal XSD (parser → AST → IR)

Fecha: 28 oct 2026. Fuente: «Retomar aquí» del plan general, D3, D13-D16, D19,
`docs/fase3/ir.md` §4-§5.2 y §11, `docs/fase3/pendientes.md` §3, §22 y §26,
`docs/00-alcance.md` §1.2, §3 y §4.

## Resumen
Hoy solo JSON Schema llega a la IR. Esta tarea añade el segundo frontal (D3): un lector XML
de esquemas con espacios de nombres, una gramática LL(1) del subconjunto XSD con su AST
propio, el análisis que rechaza con posición toda *Schema Component Constraint* del
subconjunto (D15; `verify` queda como defensa en profundidad) y la bajada a la IR común de
`ir.md` §5.2, expuesta por `schemac check|ir`. Cierra el hito de la semana 2 («un esquema
JSON o XSD sale como IR verificada») y la entrada XSD del hito de la semana 3.

## Grafo de dependencias
5.1 lector XML + espacios de nombres
 └── 5.2 gramática LL(1) XSD + parser → AST (docs/fase2)
      └── 5.3 nombres, restricciones estructurales (I10-I12, D14) y bajada de tipos complejos
           └── 5.4 schemac check|ir con XSD, corpus_clasificado, doradas, ρ
                │   ── Punto de control C (imprescindible para la semana 3) ──
                ├── 5.6 tipos simples, facetas, ciclos de base, default/fixed ◀── 5.5 números XSD (independiente)
                └── 5.7 UPA y cierre del frontal (docs, alcance, pendientes)
                    ── Punto de control D (casilla cerrada) ──
            (Fase 4) constructor del DPDA — consume la IR verificada y clasificada

## Decisiones de arquitectura
- Crate `crates/xsd` (lexer + ir), sin depender de `semantic` (D13, `ir.md` §1) — U1.
- Formato del esquema detectado por el contenido (`<` inicial) en `schemac` y el servidor — U2 (toca D17).
- Parser LL(1) descendente al estilo de `schema-parser`, con tope de profundidad 256 en un hilo con pila explícita (D7);
  análisis y bajada iterativos, O(n) salvo UPA, acotada por un límite de partículas (U6).
- Errores con `code()` estable y por etapas, como `semantic::analyze`; un error de `verify` es siempre interno (D15, código 3).
- Bajada determinista según `ir.md` §5.2; predefinidos y envoltorios de contenido simple creados una vez, al usarse.
- Comparación decimal exacta en `ir::num` (la reutiliza la Fase 4); `xs:float`/`xs:double` redondeados a IEEE (I-2).

## Riesgos y mitigaciones
| Riesgo | Impacto | Mitigación |
| --- | --- | --- |
| La semana 2 termina sin XSD → IR (señal del plan general) | Alto | 5.1-5.4 primero (punto C); 5.5-5.7 aplazables según U8 |
| UPA o FIRST/FOLLOW cuadráticos ante un esquema hostil | Alto | Límite de partículas por modelo (U6), comprobado antes de calcular; prueba sin reloj (D1) |
| «Toda SCC del subconjunto» es más de lo que cabe | Medio | Lista explícita en `ir.md` §4; `pattern` en valores aplazado a la tarea `pattern` → AFD (U9) |
| Un XSD del usuario llega a `verify` y sale como error interno (código 3) | Medio | Cada tarea que baja una construcción comprueba en el frontal los invariantes que la tocan; prueba de corpus «nunca `InternalError`» |
| Espacios de nombres y QName (prefijos arbitrarios, espacio por defecto, `targetNamespace`) | Medio | 5.1 los resuelve por URI; casos con `xsd:`, espacio por defecto y `tns:` en el corpus |
| Desbordamiento de pila en debug con anidamiento profundo | Medio | Tope 256 medido en un hilo de 2 MiB (`fase2/pendientes.md` §5), o parser iterativo (B1) |
| Romper la salida JSON de `schemac` al integrar XSD | Bajo | Las doradas de `tests/ir/` no deben cambiar (criterio de 5.4) |

## Decisiones del usuario
U1-U9 aprobadas el 29 oct como D21-D29 del plan general (todas según la recomendación del Planner).

## Decisiones que puede tomar el Builder (B1-B7; se registran en el plan general al tomarlas)

- **B1. Parser y AST.** Imitar `schema-parser`: descendente recursivo LL(1), con tope de profundidad 256 (`MAX_DEPTH_LIMIT`, solo configurable a la baja), garantizado en un hilo de 2 MiB y medido en debug, como en `fase2/pendientes.md` §5. O bien un parser iterativo. En los dos casos, el AST sin `Drop` recursivo profundo.
- **B2. Errores.** Un tipo propio con `code()` estable (`lexical`, `syntax`, `unsupported`, `duplicate-name`, `unresolved`, `cycle`, `facet`, `value`, `content-model`, `upa`, `limit`…) y mensajes de una línea con `lexer::short`.
  - La Fase 2 se detiene en el primer error.
  - El análisis informa por etapas, todos los errores de la primera etapa que falla, igual que `semantic::analyze`: nombres y resolución → tipos simples → modelos de contenido.
- **B3. Bajada determinista.**
  - Orden de los `NodeId`: el documento, después las declaraciones en preorden, después los nodos sintéticos (`builtin`, `simpleContent/xs:…`) por orden de primer uso.
  - Los predefinidos se crean solo si se usan, como en `pedido_xsd`.
  - Origen de un tipo anónimo dentro de un `xs:element` global (S-5, primera mitad): padre el documento y paso `Entry("element", nombre)`.
- **B4. Comparación decimal exacta y aritmética de lexemas** (para facetas «valid restriction», rangos de los predefinidos y `default`/`fixed`). Recomiendo ponerla en `crates/ir` (`ir::num`), porque la Fase 4 también compara por valor. Alternativa: en `crates/xsd`.
  - `xs:float`/`xs:double`: guardar en `Num` el decimal más corto del valor redondeado (`f32`/`f64` de `std`, sin `unsafe`), o el lexema exacto con una nota de que la Fase 4 redondea los dos lados. En cualquier caso hay que corregir `ir.md` §5.2, §7 (fila Q) y §11.3.
- **B5. Espacios de nombres.** Ámbito completo de las declaraciones `xmlns`/`xmlns:p` con una pila.
  - Resolución de los QName (`type`, `base`, `ref`) por (URI, nombre local).
  - Los tipos complejos y los simples comparten un mismo espacio de símbolos; los elementos tienen otro.
  - Normalización de espacios de los valores de atributo según su tipo XSD (el lexer no la hace).
- **B6. Pruebas.**
  - Corpus en `tests/schemas/xsd/{parse-ok,parse-error,semantic-error}`, con manifiestos `errores-xsd.json` y `errores-semanticos-xsd.json` (línea y columna), en el estilo de `tests/schemas/errores.json`.
  - El equivalente de `corpus_clasificado` en `crates/xsd/tests/corpus.rs`, que lee los casos `format: "xml"` de `tests/cases`.
- **B7. Algoritmo de UPA y de *Element Declarations Consistent* (EDC).** Iterativo, sin recursión sobre el árbol de partículas (D7).
