# Vistas de la web: autómata, árbol de derivación y comparación (F7.6, D70)

El plan pide que el frontend tenga cuatro vistas: editor de esquema, autómata, árbol de derivación y panel de
comparación en vivo. F7.4 hizo el editor. F7.6 añade las otras tres, cada una con su endpoint en el servidor.

```text
cargo run -p server --release          # API en http://127.0.0.1:8080
cd web && npm install && npm run dev   # http://localhost:5173 (reenvía /api al servidor)
```

## 1. Endpoints nuevos

Son aditivos: `POST /api/schemas` y `GET /api/health` no cambian. **Tocan la API del servidor, así que quedan
pendientes del visto bueno de Leonardo (D42).** Los tres pasan por el mismo camino que `POST /api/schemas`
(`api::procesar`):

- tope de subida (413);
- presupuesto de cuerpos en vuelo y plazo de lectura (503/408);
- permiso de concurrencia sin cola (503);
- hilo propio con pila de 8 MiB (D7);
- tope de tiempo (504) y pánico → 500.

| Endpoint | Cuerpo | 200 | Errores |
| --- | --- | --- | --- |
| `POST /api/automaton` | El esquema crudo (JSON Schema o XSD, D22) | `{format, states, transitions, start, groups, truncated, nodes, edges}` | 422 `schema` (como `/api/schemas`); 422 `automaton` con `status` `pending` o `limit` |
| `POST /api/derivation` | El payload crudo (JSON o XML) | El árbol de F5.5: `{name, kind, start, end, lexeme?, pointer?, children?}` | 422 `derivation` con `kind` (`lexical`, `syntax`, `too-many-nodes`, `too-deep`), `offset` y `message` |
| `POST /api/compare` | `{"schema": "…", "payload": "…"}` (JSON con dos cadenas) | `{format, payloadBytes, dpda, baseline, agree}` | 400 `request` (cuerpo que no es ese JSON); 422 como `/api/automaton` |

**Grafo.** Sale de `Automaton::write_dot` (`crates/automaton`, determinista). El servidor lo traduce a nodos y
aristas, sin duplicar la forma de las etiquetas del constructor:

- nodos `qN` con su nombre, su grupo y `accept`;
- los especiales «↩ marco» y «↩ escalar», que son los destinos que da la pila;
- aristas ya agrupadas por destino y operación, con la etiqueta de `schemac automaton --dot` (clases, apila,
  desapila, contadores, comprobaciones).

Se envían como mucho 400 nodos y 1 500 aristas, con `truncated: true` si M es mayor. El XSD de pedido tiene
216 estados y 10 054 transiciones, y con más elementos Cytoscape deja de ser legible.

**Comparación.** Hay dos validadores:

- **DPDA:** M sobre `runtime-rs` en modo total, en bloques de 4 KiB. Devuelve `valid`, `violations` (`null` = ∞),
  `errorKind`, el primer error (offset, línea, columna, regla), `compileNs`, `validateNs`, `memoryBytes` (los
  `state_words` fijos de los límites) y el tamaño de M.
- **Línea base DOM** (`serde_json` + `jsonschema`, D48): devuelve `valid`, `kind`, `violations`, hasta 20 errores
  con puntero, el error de lectura y los tiempos. Para XSD no hay línea base en el servidor, porque la de XML
  necesita libxml2 (D54). Entonces `baseline` es `{"error": …}` y `agree` es `null`.

Los tiempos son de **una sola** medición, solo orientativos. Las medidas con repeticiones (MB/s, ns/byte, pico,
asignaciones) del corpus y del bombeo están en [../fase6/metricas.md](../fase6/metricas.md).

Código: `server/src/vistas.rs` y `server/src/api.rs` (`procesar`, que ahora comparte `POST /api/schemas`).
Pruebas: `server/tests/vistas.rs`, 10 pruebas:

- el grafo contiene todos los estados y cada arista une nodos enviados;
- el XSD se recorta;
- el árbol de JSON y de XML;
- los errores con su offset;
- la comparación válida, inválida (2 = 2), malformada y XSD sin línea base;
- un cuerpo mal formado da 400;
- el tope de subida en las tres rutas;
- un `GET` da 405.

Las 26 de `server/tests/api.rs` siguen verdes tras el refactor.

## 2. Frontend

Hay cuatro pestañas en `web/src/App.tsx`. El editor queda montado al cambiar de pestaña (solo se oculta), así
Monaco conserva el texto. La pestaña Árbol y la de Comparación comparten el payload.

- **Autómata** (`Automata.tsx`, Cytoscape 3.30.2):
  - disposición `breadthfirst` desde el estado inicial;
  - colores por grupo, borde verde en el inicio y doble en la aceptación;
  - al pulsar un nodo o una arista se ven su nombre y su etiqueta.
- **Árbol de derivación** (`Arbol.tsx`, d3-hierarchy 3.1.2):
  - `tree().nodeSize` calcula las posiciones y React dibuja el SVG;
  - no terminales en elipse, terminales en caja con su lexema y ε en gris;
  - al pulsar un nodo se ven sus bytes, su lexema y su puntero JSON.
- **Comparación** (`Comparacion.tsx`):
  - una tabla con DPDA y línea base lado a lado: veredicto, violaciones, primer error, tiempos, memoria y tamaño
    de M;
  - barras de tiempo de validación;
  - la lista completa de violaciones de la línea base.

**Seguridad (XSS).** Todo lo que llega del servidor se muestra como texto: React escapa y Cytoscape dibuja en un
lienzo. Ninguna vista usa `innerHTML`. Una prueba manda `<img onerror>` y `<b>` y comprueba que no se crean
(`pendientes.md` §5).

Pruebas (vitest, 18 nuevas, 32 en total):

- `vistas.test.ts`: los intérpretes del grafo, del árbol y de la comparación (incluidas formas inesperadas y
  aristas huérfanas), `elementos` de Cytoscape, el diseño del árbol (niveles y orden de hermanos) y las unidades;
- `Comparacion.test.tsx`: la tabla con `fetch` simulado, el XSS, los errores y el árbol en SVG con su detalle.

## 3. Pendientes

- **F7.6-P1:** visto bueno de Leonardo a los tres endpoints (D42).
- **F7.6-P2** (`pendientes.md` §15): `/api/compare` corre `jsonschema` sin tope de CPU propio. Un esquema hostil
  ocupa un permiso de concurrencia hasta terminar, aunque el cliente ya recibió el 504 (lo mismo que §12 para
  `POST /api/schemas`; `docs/fase6/pendientes.md` §14). Lo acotan el tope de subida (1 MiB) y la concurrencia
  (2). El arreglo es el de §1: un proceso aparte con límite de CPU.
- Los `.svg` de los árboles de `docs/fase5` (F5.5) siguen pendientes de `make docs` con Graphviz.
