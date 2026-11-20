# web

Frontend de la Fase 7 (F7.4, D56): React 18 + TypeScript + Vite, con el editor Monaco
servido desde el propio bundle (sin CDN). Es la vista «editor de esquema» del plan: se
edita un JSON Schema (subconjunto de 2020-12) o un XSD (subconjunto de 1.0), se compila
contra `POST /api/schemas` del servidor (F7.1, contrato en `docs/fase7/servidor.md` §2) y
se muestra el resultado:

- **200**: resumen de `schemac check`, `id` (SHA-256), resumen del autómata de pila
  (estados, transiciones, marcos…) o el motivo de que aún no se construya (`pending`, todo
  XSD hasta F4.1c) y la IR v2 **cruda**, tal como la escribió `ir::write_json` (sin pasar
  por `JSON.parse`, para no redondear contadores de hasta `u64::MAX`).
- **422**: la lista de errores (como mucho 100, con el total), marcados en el editor;
  pulsar un error lleva el cursor a su línea y columna.
- **413, 408, 503, 504, 500, red caída**: el mensaje del servidor y, si lo hay, el
  `Retry-After`.

Todo lo que viene del servidor se muestra **como texto** (React escapa por defecto; no
hay `innerHTML` ni `dangerouslySetInnerHTML`): la IR y los mensajes traen texto del
usuario sin escapar (`docs/fase7/pendientes.md` §5). Lo comprueba
`src/Resultado.test.tsx` con `<img src=x onerror=…>` como nombre de propiedad.

Desde F7.6 (D70) hay otras tres pestañas: **autómata** (Cytoscape, `POST /api/automaton`),
**árbol de derivación** (d3-hierarchy, `POST /api/derivation`) y **comparación** DPDA / línea
base (`POST /api/compare`). Detalle en `docs/fase7/vistas.md`.

## Uso

Requiere Node ≥ 20. Desde `web/`:

```sh
npm install          # una vez (package-lock.json fija las versiones)
npm run dev          # http://localhost:5173 ; /api se reenvía a 127.0.0.1:8080
npm test             # vitest: respuestas, panel y vistas de F7.6 (32 pruebas)
npm run build        # tsc + vite build → dist/ (lo sirve Caddy en F7.7)
```

En otra terminal, desde la raíz del repo, el servidor: `cargo run -p server --release`.
Para apuntar el proxy a otro servidor: `VITE_API_TARGET=http://host:puerto npm run dev`.

## Archivos

| Archivo | Qué hace |
| --- | --- |
| `src/api.ts` | Cliente de `POST /api/schemas`: `compilar` (con `fetch` inyectable) e `interpretar` (pura, nunca lanza), y `formatoProbable` (D22). |
| `src/Resultado.tsx` | Panel de resultados (200, 422, fallos). |
| `src/App.tsx` | Barra (ejemplos, abrir archivo, compilar con Ctrl+Enter) y editor Monaco con los errores marcados. |
| `src/monaco.ts` | Monaco local con sus *workers* de Vite; sin esquemas remotos. |
| `src/ejemplos.ts` | Esquemas de ejemplo para la demo. |
| `src/vistas.ts` | F7.6: clientes de `/api/automaton`, `/api/derivation` y `/api/compare` (intérpretes puros). |
| `src/Automata.tsx` | F7.6: grafo de M con Cytoscape. |
| `src/Arbol.tsx` | F7.6: árbol de derivación con d3-hierarchy en SVG. |
| `src/Comparacion.tsx` | F7.6: tabla y barras DPDA frente a línea base. |
