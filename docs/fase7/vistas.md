# Vistas de la web: autómata, árbol de derivación y comparación (F7.6, D70)

El plan pide que el frontend tenga cuatro vistas: editor de esquema, autómata, árbol de derivación y panel de
comparación en vivo. F7.4 hizo el editor. F7.6 añade las otras tres, cada una con su endpoint en el servidor.

```text
cargo run -p server --release          # API en http://127.0.0.1:8080
cd web && npm install && npm run dev   # http://localhost:5173 (reenvía /api al servidor)
```

## 1. Endpoints nuevos

Son aditivos: `POST /api/schemas` y `GET /api/health` no cambian. **Tocan la API del servidor; Leonardo les dio
el visto bueno el 20 nov (D42, D70).** Los tres pasan por el mismo camino que `POST /api/schemas`
(`api::procesar`):

- tope de subida (413);
- presupuesto de cuerpos en vuelo y plazo de lectura (503/408);
- permiso de concurrencia sin cola (503);
- hilo propio con pila de 8 MiB (D7);
- tope de tiempo (504) y pánico → 500.

| Endpoint | Cuerpo | 200 | Errores |
| --- | --- | --- | --- |
| `POST /api/automaton` | El esquema crudo (JSON Schema, XSD o DTD; D22, D86) | `{format, states, transitions, start, groups, truncated, nodes, edges}` | 422 `schema` (como `/api/schemas`); 422 `automaton` con `status` `pending` o `limit` |
| `POST /api/derivation` | El payload crudo (JSON o XML) | El árbol de F5.5: `{name, kind, start, end, lexeme?, pointer?, children?}` | 422 `derivation` con `kind` (`lexical`, `syntax`, `too-many-nodes`, `too-deep`), `offset` y `message` |
| `POST /api/compare` | `{"schema": "…", "payload": "…"}` (JSON con dos cadenas) | `{format, payloadBytes, dpda, baseline, agree, measurement}` | 400 `request` (cuerpo que no es ese JSON); 422 como `/api/automaton` |

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

`compileNs` y `validateNs` son de **una sola** pasada, solo orientativos. La medición con repeticiones va en
`measurement` (enunciado, objetivo 3: «medir en vivo rendimiento, memoria y asignaciones frente a validadores
basados en AST»):

- la hace el binario `medir` (`crates/bench/src/bin/medir.rs`), que usa `dhat` como asignador global y corre
  como **proceso aparte** (`server/src/medir.rs`): cada medición tiene su propio montón y el servidor no necesita
  asignador global ni `unsafe` (D52). La imagen Docker lo copia junto al servidor;
- devuelve `{bytes, profundidad, dpda, lineaBase, motivoLineaBase}`; `dpda` y `lineaBase` llevan `valido`,
  `violaciones`, `segundos` (mediana de 3 a 15 repeticiones en unos 400 ms), `mbPorSegundo`, `nsPorByte`,
  `asignaciones` y `picoBytes`. Para XML `lineaBase` es `null` y `motivoLineaBase` dice por qué (D54);
- tiene un tope de 7 s, menor que el plazo de la petición; si falta el binario o se pasa del tope,
  `measurement` es `{"error": …}` y el resto de la respuesta no cambia.

Con 10 000 objetos (la «prueba de carga» de la web, 800 KB) el DPDA hace 1 asignación con 74 KiB de pico; la
línea base, unas 70 000 asignaciones y unos 7 MiB. En rendimiento la línea base es más rápida (unos 70 MB/s
frente a unos 35 MB/s en la máquina de pruebas): la ventaja del DPDA es la memoria fija y el streaming. Las medidas del corpus y del bombeo siguen en
[../fase6/metricas.md](../fase6/metricas.md).

Código: `server/src/vistas.rs` y `server/src/api.rs` (`procesar`, que ahora comparte `POST /api/schemas`).
Pruebas: `server/tests/vistas.rs`, 11 pruebas:

- el grafo contiene todos los estados y cada arista une nodos enviados;
- el XSD se recorta;
- el árbol de JSON y de XML;
- los errores con su offset;
- la comparación válida, inválida (2 = 2), malformada y XSD sin línea base;
- la medición en vivo: el DPDA hace como mucho 2 asignaciones y la línea base más de 2 000 con 2 000 objetos;
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

- **F7.6-P1:** visto bueno de Leonardo a los tres endpoints (D42). **Resuelto (20 nov).**
- **F7.6-P3:** el grafo sale de reinterpretar el texto de `Automaton::write_dot`, así que un cambio en las etiquetas
  del DOT rompe la vista (lo detecta `server/tests/vistas.rs`). Arreglo propuesto: una salida estructurada de nodos
  y aristas en `automaton::export`, cuando haya tiempo (pista A).
- **F7.6-P4:** `/api/compare` recibe el payload como cadena JSON, así que no admite payloads con UTF-8 inválido
  (algunos casos `malformed` del corpus). Es una limitación de la vista, no del validador.
- **F7.6-P2** (`pendientes.md` §15): `/api/compare` corre `jsonschema` sin tope de CPU propio. Un esquema hostil
  ocupa un permiso de concurrencia hasta terminar, aunque el cliente ya recibió el 504 (lo mismo que §12 para
  `POST /api/schemas`; `docs/fase6/pendientes.md` §14). Lo acotan el tope de subida (1 MiB) y la concurrencia
  (2). El arreglo es el de §1: un proceso aparte con límite de CPU.
- Los `.svg` de los árboles de `docs/fase5` (F5.5) siguen pendientes de `make docs` con Graphviz.
