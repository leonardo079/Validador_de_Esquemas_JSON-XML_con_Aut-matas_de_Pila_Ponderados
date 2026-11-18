# Checklist del proyecto

Estado compartido de todas las tareas. **Una línea por tarea.** Es lo primero que se mira antes de
empezar y lo último que se toca al terminar. Las reglas completas están en `COLABORAR.md`; el detalle de cada
tarea, en `tasks/plan-general.md` (cronograma y «Detalle técnico por fase») y, cuando ya tiene plan
aprobado, en `tasks/todo.md`.

## Cómo se usa

1. **Tomar una tarea:** `git pull --rebase`; elegir una línea `libre` cuyas dependencias estén todas en `[x]`;
   cambiar `Resp.` por tu nombre y `Estado` por `en curso (fecha)`; hacer commit solo de este archivo
   (`checklist: tomo F4.1`) y `git push` **antes de escribir código**. Si el push falla porque el otro tomó la
   misma tarea, gana el que subió primero: elige otra.
2. **Una sola tarea `en curso` por persona.**
3. **Cerrar una tarea** (aprobada en `/autodev`, `make lint` y `make test` en verde): marcar `[x]`, poner
   `hecha (fecha) · N rondas` y, si quedaron observaciones, dónde están. Esta edición va **en el mismo commit
   de la tarea**, cuyo mensaje empieza por el ID (`F4.1: construcción composicional del DPDA`). Así
   `git log --grep F4.1` da el commit sin escribir el hash aquí.
4. **Bloqueada o abandonada:** `Estado: bloqueada — motivo` o devolverla a `libre` (borrando `Resp.`), con commit y push.
5. Tareas nuevas que salgan de una revisión (D38) se añaden como línea nueva en la fase que corresponda, con ID libre.
6. En un conflicto de `git` sobre este archivo, se conservan las dos líneas editadas: cada una es de una tarea distinta.

Estados: `libre` · `en curso (fecha)` · `bloqueada — motivo` · `hecha (fecha) · N rondas`.
Pista: **A** = compilador (Fases 4-5) · **B** = plataforma (Fases 6-7, cuentas, XSD independiente) · **AB** = ambos.

> **Atraso (13 nov):** el hito de la semana 3 (prueba diferencial, 6 nov) no se cumplió; congelamiento previsto
> el 19 nov y entrega el 22 nov. Aplican los recortes de «Riesgos y recortes de alcance» del plan.
> **Acordado (S1, 13 nov):** entrega el domingo 22 nov (el cronograma no cambia); pista A Leonardo, pista B Bayron.

## Hecho hasta el 13 nov

- [x] **F1** · Fase 1 completa: alcance, AFD UTF-8/JSON/XML, lexers reanudables, DOT — *Resp.:* Leonardo · hecha (20 oct) · hallazgos 1-8 cerrados
- [x] **F2-JSON** · Fase 2 para JSON Schema: gramáticas, LL(1), parser → AST — *Resp.:* Leonardo · hecha (21 oct)
- [x] **F3-JSON** · Fase 3 para JSON Schema: meta-esquema, `$ref`/`$defs`, ciclos — *Resp.:* Leonardo · hecha (22 oct) · `c1a1a96`, `1777bbc`
- [x] **T4.0** · Observaciones de la tarea 3 — *Resp.:* Leonardo · hecha (22 oct) · 2 rondas
- [x] **T4.1** · IR común (`crates/ir`, D13-D16) — *Resp.:* Leonardo · hecha (26 oct) · 3 rondas · `12ea049`
- [x] **T4.2** · Bajada JSON → IR y `schemac ir` (D17-D18) — *Resp.:* Leonardo · hecha (27 oct) · 3 rondas · `edd75e5`
- [x] **T4.3** · Clasificación (a)/(b)/(c) (D19-D20) — *Resp.:* Leonardo · hecha (28 oct) · 1 ronda · `edd75e5`
- [x] **T5.1** · Lector XML con espacios de nombres (D30-D31) — *Resp.:* Leonardo · hecha (29 oct) · 3 rondas · `746a9c3`, `b171e48`
- [x] **T5.2** · Gramática LL(1) XSD y parser → AST (D34, D36-D37) — *Resp.:* Leonardo · hecha (3 nov) · 2 rondas · `dd6fbec`
- [x] **T5.3** · Nombres, restricciones y bajada XSD → `VerifiedIr` (D39-D40) — *Resp.:* Leonardo · hecha (4 nov) · 0 rondas · `dac26c8`
- [x] **T5.4** · `schemac check|ir` con XSD, doradas, ρ (D22, D33, D41) — *Resp.:* Leonardo · hecha (4 nov) · 0 rondas · `dac26c8`

## Cuentas y equipo

- [ ] **S0** · Clúster MongoDB Atlas M0 y cuenta AWS con alerta de presupuesto (compartir accesos) — *Pista:* B · *Depende:* — · *Resp.:* — · *Estado:* libre
- [x] **S1** · Acordar fecha real de entrega y reparto de pistas; reprogramar el cronograma del plan si cambia — *Pista:* AB · *Depende:* — · *Resp.:* ambos · *Estado:* hecha (13 nov) · entrega dom 22 nov, cronograma sin cambios; pista A Leonardo, pista B Bayron

## Tarea 5 — frontal XSD (resto)

- [x] **PC-C** · Punto de control C: `make lint`/`test`/`docs-check` en verde, revisar `schemac ir` sobre el pedido, detección de formato (D22) y texto de uso — *Pista:* A · *Depende:* T5.4 · *Resp.:* Leonardo · *Estado:* hecha (14 nov) · lint/test (393 pruebas)/docs-check en verde; 5.5-5.7 pasan a la pista A y van antes de la Fase 4 (decisión de Leonardo, 14 nov)
- [x] **T5.5** · Números XSD: forma léxica, reescritura a RFC 8259, comparación exacta, redondeo IEEE (D24) — *Pista:* A · *Depende:* — (independiente) · *Resp.:* Leonardo · *Estado:* hecha (14 nov) · 0 rondas · `b37428a` · observaciones en `docs/fase3/pendientes.md` §36 · criterios en `tasks/todo.md`
- [x] **T5.6** · Tipos simples: `xs:simpleType`/`xs:restriction`, facetas, enumeraciones, `default`/`fixed` — *Pista:* A · *Depende:* PC-C, T5.5 · *Resp.:* Leonardo · *Estado:* hecha (14 nov) · 1 ronda · `b37428a` · observaciones en `docs/fase3/pendientes.md` §37 · criterios en `tasks/todo.md`
- [x] **T5.7** · UPA (D25-D26) y cierre del frontal (docs, alcance, pendientes) — *Pista:* A · *Depende:* PC-C · *Resp.:* Leonardo · *Estado:* hecha (15 nov) · 1 ronda · `21dbe84` · alcance §3-§4, enmienda D25 y precisión D26 aprobados por Leonardo; falta visto bueno de Bayron (D45) · observaciones en `docs/fase3/pendientes.md` §38 · criterios en `tasks/todo.md`
- [x] **PC-D** · Punto de control D: todo el corpus XSD sale como IR clasificada — *Pista:* A · *Depende:* T5.6, T5.7 · *Resp.:* Leonardo · *Estado:* hecha (15 nov) · 12 `parse-ok` como IR clasificada; 173 casos de error con código 1

## Fase 4 — autómata de pila ponderado (`crates/automaton`, `crates/runtime-rs`)

- [x] **F4.1a** · Modelo formal de M (Σ = eventos del lexer, Γ con marcos y registros, δ densa y total), pesos tropicales y costo, esqueleto JSON (`type`, `$ref` alias, `properties`, `items`, `required`/`dependentRequired`, claves duplicadas), núcleo `no_std` del ejecutor en `runtime-rs`, `schemac automaton [--dot]`, `docs/fase4/` (D47, D51) — *Pista:* A · *Depende:* PC-C, PC-D · *Resp.:* Leonardo · *Estado:* hecha (16 nov) · 0 rondas · `d0624d2` · observaciones en `docs/fase4/pendientes.md`; criterios heredados en F4.1b y F4.6
- [x] **F4.1b** · Contadores, `enum`/`const` escalares, rangos y `multipleOf` exactos (rutinas `no_std` en `runtime-rs`); `$ref` con hermanas y `allOf` como conjunción con tope de 4 096 (D47) — *Pista:* A · *Depende:* F4.1a · *Resp.:* Leonardo · *Estado:* hecha (18 nov) · 3 rondas · sin commit · observaciones en `docs/fase4/pendientes.md`; criterios heredados en F4.1c, F4.5a y F4.6
- [ ] **F4.1c** · XML: documento, elementos, Glushkov con contadores, `xs:all`, atributos, textos con tipos simples (NaN/INF), presupuesto de transiciones (D47) — *Pista:* A · *Depende:* F4.1b · *Resp.:* — · *Estado:* libre
- [x] **F4.2** · ~~Pesos en el semianillo tropical~~ — fusionada en F4.1a (D46, 15 nov) — *Pista:* A · *Resp.:* — · *Estado:* fusionada
- [ ] **F4.3** · Verificación de determinismo de δ y Hopcroft en todas las partes finitas (`Dfa`) (D51) — *Pista:* A (o Bayron, en paralelo) · *Depende:* F4.1a · *Resp.:* — · *Estado:* libre · criterios en `tasks/todo.md`
- [ ] **F4.4a** · Analizador de `pattern` en `ir::regex` (ECMA y XSD) y rechazo anti-ReDoS en la Fase 3 (D51) — *Pista:* A (o Bayron, en paralelo) · *Depende:* — · *Resp.:* — · *Estado:* libre · criterios en `tasks/todo.md`
- [ ] **F4.4b** · `pattern` → AFD minimizado dentro de M (D51) — *Pista:* A · *Depende:* F4.1c, F4.3, F4.4a · *Resp.:* — · *Estado:* libre
- [ ] **F4.5a** · Combinadores: producto con salidas y puertas al desapilar (`anyOf`, `oneOf`, `not`, `dependentSchemas`), tope de 4 096 (D51) — *Pista:* A · *Depende:* F4.1b, F4.4b · *Resp.:* — · *Estado:* libre
- [ ] **F4.5b** · `uniqueItems` diferido (hash canónico) y literales compuestos de `enum`/`const` (D51) — *Pista:* A · *Depende:* F4.5a · *Resp.:* — · *Estado:* libre
- [ ] **F4.6** · Ejecutor completo sobre el núcleo `no_std`: lexer, límites, reporte, bloques y buena formación; oráculo contra `expected` y la línea base `baseline` (D48, D51) — *Pista:* A · *Depende:* F4.1c (F6.7a para la comparación JSON) · *Resp.:* — · *Estado:* libre
- [ ] **PC-E** · Punto de control E: un esquema JSON y el pedido XSD pasan por esquema → M → validador Rust y el corpus pasa — *Pista:* A · *Depende:* F4.6, F5.1 · *Resp.:* — · *Estado:* libre

## Fase 5 — generación de código (`crates/codegen`, `crates/runtime-c`)

- [ ] **F5.1** · Emisor Rust: tablas de M como `const` + el núcleo `no_std` (D51), pila acotada `SV_MAX_DEPTH`, sin asignación dinámica — *Pista:* A · *Depende:* F4.6 · *Resp.:* — · *Estado:* libre
- [ ] **F5.2** · Emisor C11 con la API C-ABI (`sv_init`/`sv_feed`/`sv_finish`); header con cbindgen — *Pista:* A · *Depende:* F5.1 · *Resp.:* — · *Estado:* libre
- [ ] **F5.3** · Prueba diferencial intérprete / C / Rust en todo el corpus (bloques de 1, 7 y 4096 bytes) — *Pista:* A · *Depende:* F5.2 · *Resp.:* — · *Estado:* libre
- [ ] **F5.4** · Generador de payloads válidos desde la IR, con semilla; 20 primeros casos al corpus — *Pista:* AB · *Depende:* PC-C · *Resp.:* — · *Estado:* libre
- [ ] **F5.5** · Exportar árboles de derivación de payloads (JSON para d3-hierarchy, DOT para docs) — *Pista:* AB · *Depende:* — · *Resp.:* — · *Estado:* libre

## Fase 6 — validación en tiempo real y métricas (`crates/bench`)

- [ ] **F6.1** · Reporte de error con puntero JSON, offset y línea/columna desde la pila — *Pista:* A · *Depende:* F5.1 · *Resp.:* — · *Estado:* libre
- [ ] **F6.2** · Límites configurables: profundidad, tamaño, cadena, dígitos, tiempo — *Pista:* A · *Depende:* F5.1 · *Resp.:* — · *Estado:* libre
- [ ] **F6.3** · Generador de payloads inválidos y malformados (mutaciones) — *Pista:* B · *Depende:* F5.4 · *Resp.:* — · *Estado:* libre
- [ ] **F6.4** · Mitigaciones (DTD/entidades, `pattern` como AFD, `$ref` cíclico) y fuzzing con cargo-fuzz — *Pista:* AB · *Depende:* F5.3 · *Resp.:* — · *Estado:* libre
- [ ] **F6.5** · Arnés de métricas: MB/s, ciclos/byte, pico de memoria, asignaciones, profundidad, estados — *Pista:* B · *Depende:* F6.7 · *Resp.:* — · *Estado:* libre
- [ ] **F6.6** · Benchmark criterion del parser de esquemas con N y 4N claves (sustituye la prueba ignorada, D1) — *Pista:* B · *Depende:* — · *Resp.:* — · *Estado:* libre
- [ ] **F6.7** · Líneas base: `serde_json` + `jsonschema` y libxml2 (DOM + XSD) sobre el corpus — *Pista:* B · *Depende:* — · *Resp.:* Bayron · *Estado:* en curso (13 nov) · partida en **F6.7a (JSON): hecha (15 nov) · 1 ronda** (D48-D50; observaciones en `docs/fase6/pendientes.md` §1-§20, §14 bloqueante para F7) y **F6.7b (XML, libxml2 en Docker): siguiente**; criterios en `tasks/todo.md`

## Fase 7 — aplicación web (`server/`, `web/`, `sandbox/`, `infra/`)

- [ ] **F7.1** · Backend axum: `POST /api/schemas` con `check`/`ir` en hilo propio con `stack_size` (D7), topes de subida, CPU y memoria (`docs/fase3/pendientes.md` §1-§2) — *Pista:* B · *Depende:* — · *Resp.:* — · *Estado:* libre
- [ ] **F7.2** · Persistencia en MongoDB Atlas y carga de `tests/cases/` con script local (D32) — *Pista:* B · *Depende:* S0, F7.1 · *Resp.:* — · *Estado:* libre
- [ ] **F7.3** · Sandbox Docker (`--network none`, límites, no root, FS de solo lectura) para compilar y ejecutar el código generado — *Pista:* B · *Depende:* — · *Resp.:* — · *Estado:* libre
- [ ] **F7.4** · Frontend: editor Monaco, compilar, ver la IR (JSON v2, D17; tratar como no confiable) — *Pista:* B · *Depende:* F7.1 · *Resp.:* — · *Estado:* libre
- [ ] **F7.5** · Descarga del código, `POST /api/validate` y WebSocket de validación por bloques — *Pista:* B · *Depende:* F5.3, F7.3 · *Resp.:* — · *Estado:* libre
- [ ] **F7.6** · Vistas del autómata (Cytoscape), árbol de derivación y panel de métricas DPDA vs línea base — *Pista:* B · *Depende:* F7.4, F5.5, F6.5 · *Resp.:* — · *Estado:* libre
- [ ] **F7.7** · Despliegue en EC2 con Docker Compose y Caddy (primera versión lo antes posible) — *Pista:* B · *Depende:* S0, F7.1 · *Resp.:* — · *Estado:* libre

## Entrega (semana 5)

- [ ] **E1** · Pruebas de bombeo u·vᵏ·x·yᵏ·z y casos extremos; gráficas de tiempo, pila y memoria contra k — *Pista:* AB · *Depende:* F6.5 · *Resp.:* — · *Estado:* libre
- [ ] **E2** · Cargar todos los casos y sus métricas en Atlas — *Pista:* B · *Depende:* F7.2, F6.5 · *Resp.:* — · *Estado:* libre
- [ ] **E3** · Revisión de seguridad consolidada de las Fases 4-7 (D35) — *Pista:* AB · *Depende:* F6.4, F7.5 · *Resp.:* — · *Estado:* libre
- [ ] **E4** · Informe final: complejidad, explosión al determinizar, decisiones, vulnerabilidades, bombeo — *Pista:* AB · *Depende:* E1 · *Resp.:* — · *Estado:* libre
- [ ] **E5** · Cerrar documentación técnica (diagramas finales, DPDA de 2 esquemas, árboles de derivación) — *Pista:* AB · *Depende:* F5.3 · *Resp.:* — · *Estado:* libre
- [ ] **E6** · Congelar código — *Pista:* AB · *Depende:* — · *Resp.:* ambos · *Estado:* libre · previsto 19 nov
- [ ] **E7** · Diapositivas, guion, video de respaldo y dos ensayos de la demo — *Pista:* AB · *Depende:* F7.7 · *Resp.:* ambos · *Estado:* libre
- [ ] **E8** · WASM (opcional, solo si todo lo demás está cerrado) — *Pista:* A · *Depende:* F5.3 · *Resp.:* — · *Estado:* libre
