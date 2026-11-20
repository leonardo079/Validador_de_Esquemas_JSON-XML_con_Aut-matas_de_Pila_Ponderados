# Árbol de derivación de un payload (F5.5, D69)

El enunciado pide exportar el árbol de derivación de payloads de ejemplo: en JSON para la vista del frontend
(d3-hierarchy, F7.6) y en DOT para la documentación. F5.5 lo construye con las gramáticas LL(1) de la Fase 2:

- [docs/fase2/json.ll1](../fase2/json.ll1), con su tabla en [ll1-json.md](../fase2/ll1-json.md);
- [docs/fase2/xml.ll1](../fase2/xml.ll1), con su tabla en [ll1-xml.md](../fase2/ll1-xml.md).

```text
cargo run -p derivation --bin arbol -- docs/fase5/ejemplos/pedido.json          # JSON para d3
cargo run -p derivation --bin arbol -- --dot docs/fase5/ejemplos/pedido.xml     # DOT
make docs                                                                       # regenera docs/fase5/arbol-*.dot y .svg
```

Código: `crates/derivation` (`derivar`, `a_json`, `a_dot`). Pruebas: `crates/derivation/tests/arbol.rs`.

## 1. Cómo se construye

1. **Tokens.** El lexer de la Fase 1 (`lexer::json`, `lexer::xml`) lee el payload. Sus eventos se agrupan en los
   terminales de la gramática:
   - JSON: `{ } [ ] : ,`, `STRING`, `NUMBER`, `true`, `false` y `null`;
   - XML: `STAG`, `ATTR`, `>`, `/>`, `ETAG`, `TEXT` y `PI`.

   El formato se decide por el primer byte significativo, como `schemac` (D22).
2. **Parser predictivo LL(1) con pila explícita**, sin recursión (D7). La pila guarda pares (símbolo, padre). Al
   sacar un no terminal se elige su producción con el token de adelanto, mirando la tabla LL(1), y se crea su
   nodo. Al sacar un terminal se empareja con el token y se guarda el lexema. Los hijos de cada nodo quedan en el
   orden de la **derivación más a la izquierda**, la que recorre un DPDA predictivo, sin retroceso porque la
   gramática es LL(1).
3. **Nodos.** Hay no terminales, terminales (con el lexema recortado a 48 caracteres y el rango de bytes) y ε,
   para las producciones vacías. Los no terminales cubren desde su primer terminal hasta el último.
4. **JSON Pointer** de cada nodo `value` (RFC 6901, con `~0` y `~1`), para enlazar la vista con los errores del
   validador.
5. **XML:** la gramática no impone que `</b>` cierre `<b>`, porque esa condición no es libre de contexto con un
   alfabeto de nombres no acotado (`docs/fase2/gramatica-xml.md`). La comprobación la hace una pila de nombres
   aparte, como en el validador.

## 2. Límites

| Límite | Valor | Por qué |
| --- | --- | --- |
| Nodos (`MAX_NODOS`, `--max-nodos`) | 5 000 | Un árbol más grande no se puede leer en pantalla. Error `Grande`. |
| Profundidad (`MAX_PROFUNDIDAD`) | 256 | El límite del proyecto (`docs/00-alcance.md` §4). Además, sin él los punteros JSON sumarían O(n²) bytes: con 10⁴ niveles de `[`, 100 MB de salida. Error `Profundo` en el byte del nivel 257. |

El árbol solo sirve para mostrar y explicar. **El validador no lo construye** (invariante: O(n) en bytes, O(d)
en pila y sin árbol del payload).

## 3. Formato JSON (para d3-hierarchy)

```json
{"name": "value", "kind": "nt", "start": 0, "end": 76, "pointer": "",
 "children": [{"name": "object", "kind": "nt", "start": 0, "end": 76, "children": [
   {"name": "{", "kind": "t", "start": 0, "end": 1, "lexeme": "{"}, …]}]}
```

- `kind`: `nt` (no terminal), `t` (terminal) o `eps`.
- `start` y `end`: los bytes `[start, end)` del payload que cubre el nodo.
- `lexeme` (solo en terminales) y `pointer` (solo en los `value` de JSON).
- `children` (se omite en las hojas).

`d3.hierarchy(datos)` lo lee directamente. La salida se escribe sin recursión.

## 4. Ejemplos

| Payload | Árbol (DOT) |
| --- | --- |
| [ejemplos/pedido.json](ejemplos/pedido.json) | [arbol-pedido.json.dot](arbol-pedido.json.dot) |
| [ejemplos/pedido.xml](ejemplos/pedido.xml) | [arbol-pedido.xml.dot](arbol-pedido.xml.dot) |

`make docs-check` comprueba que los `.dot` coinciden con lo que genera el código. Los `.svg` salen de `make docs`
en una máquina con Graphviz; en la de Bayron no hay `dot`, así que no se versionaron en esta tarea.

En el DOT, los no terminales van en elipse, los terminales en caja con su lexema y ε en gris; `ordering=out`
mantiene a los hijos en el orden de la derivación.

## 5. Pruebas

`crates/derivation/tests/arbol.rs`, 11 pruebas deterministas:

- cada no terminal se expande con una producción de la gramática;
- la frontera (los terminales en orden) cubre el payload, dejando fuera solo espacios, el BOM y, en XML,
  comentarios y delimitadores de CDATA;
- punteros con escapes, rangos y errores léxicos y sintácticos con su offset;
- etiquetas XML que no cierran, el tope de nodos y 256 niveles (en JSON y en XML) sin recursión, más el error en
  el nivel 257;
- el JSON para d3 se reparsea y tiene un objeto por nodo, y el DOT trae una arista por hijo;
- el recorte del lexema;
- todo payload `valid` del corpus tiene árbol.

## 6. Pendiente

- **F5.5-P1:** el endpoint `POST /api/schemas/{id}/derivation` del plan (o `POST /api/derivation`, porque el
  árbol no depende del esquema) y la vista con d3-hierarchy van en **F7.6**. Tocan la API del servidor, así que
  necesitan el visto bueno de Leonardo (D42).
