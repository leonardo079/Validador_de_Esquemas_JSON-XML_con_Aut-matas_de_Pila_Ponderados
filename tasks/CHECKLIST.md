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

- [x] **S0** · Clúster MongoDB Atlas M0 y cuenta AWS con alerta de presupuesto (compartir accesos) — *Pista:* B · *Depende:* — · *Resp.:* Bayron · *Estado:* hecha (20 nov) · Atlas M0 us-east-1 (proyecto validador-pda), AWS Free plan con budgets cero-gasto y 5 USD, accesos compartidos con Leonardo
- [x] **S1** · Acordar fecha real de entrega y reparto de pistas; reprogramar el cronograma del plan si cambia — *Pista:* AB · *Depende:* — · *Resp.:* ambos · *Estado:* hecha (13 nov) · entrega dom 22 nov, cronograma sin cambios; pista A Leonardo, pista B Bayron

## Tarea 5 — frontal XSD (resto)

- [x] **PC-C** · Punto de control C: `make lint`/`test`/`docs-check` en verde, revisar `schemac ir` sobre el pedido, detección de formato (D22) y texto de uso — *Pista:* A · *Depende:* T5.4 · *Resp.:* Leonardo · *Estado:* hecha (14 nov) · lint/test (393 pruebas)/docs-check en verde; 5.5-5.7 pasan a la pista A y van antes de la Fase 4 (decisión de Leonardo, 14 nov)
- [x] **T5.5** · Números XSD: forma léxica, reescritura a RFC 8259, comparación exacta, redondeo IEEE (D24) — *Pista:* A · *Depende:* — (independiente) · *Resp.:* Leonardo · *Estado:* hecha (14 nov) · 0 rondas · `b37428a` · observaciones en `docs/fase3/pendientes.md` §36 · criterios en `tasks/todo.md`
- [x] **T5.6** · Tipos simples: `xs:simpleType`/`xs:restriction`, facetas, enumeraciones, `default`/`fixed` — *Pista:* A · *Depende:* PC-C, T5.5 · *Resp.:* Leonardo · *Estado:* hecha (14 nov) · 1 ronda · `b37428a` · observaciones en `docs/fase3/pendientes.md` §37 · criterios en `tasks/todo.md`
- [x] **T5.7** · UPA (D25-D26) y cierre del frontal (docs, alcance, pendientes) — *Pista:* A · *Depende:* PC-C · *Resp.:* Leonardo · *Estado:* hecha (15 nov) · 1 ronda · `21dbe84` · alcance §3-§4, enmienda D25 y precisión D26 aprobados por Leonardo; falta visto bueno de Bayron (D45) · observaciones en `docs/fase3/pendientes.md` §38 · criterios en `tasks/todo.md`
- [x] **PC-D** · Punto de control D: todo el corpus XSD sale como IR clasificada — *Pista:* A · *Depende:* T5.6, T5.7 · *Resp.:* Leonardo · *Estado:* hecha (15 nov) · 12 `parse-ok` como IR clasificada; 173 casos de error con código 1

## Fase 4 — autómata de pila ponderado (`crates/automaton`, `crates/runtime-rs`)

- [x] **F4.1a** · Modelo formal de M (Σ = eventos del lexer, Γ con marcos y registros, δ densa y total), pesos tropicales y costo, esqueleto JSON (`type`, `$ref` alias, `properties`, `items`, `required`/`dependentRequired`, claves duplicadas), núcleo `no_std` del ejecutor en `runtime-rs`, `schemac automaton [--dot]`, `docs/fase4/` (D47, D51) — *Pista:* A · *Depende:* PC-C, PC-D · *Resp.:* Leonardo · *Estado:* hecha (16 nov) · 0 rondas · `d0624d2` · observaciones en `docs/fase4/pendientes.md`; criterios heredados en F4.1b y F4.6
- [x] **F4.1b** · Contadores, `enum`/`const` escalares, rangos y `multipleOf` exactos (rutinas `no_std` en `runtime-rs`); `$ref` con hermanas y `allOf` como conjunción con tope de 4 096 (D47) — *Pista:* A · *Depende:* F4.1a · *Resp.:* Leonardo · *Estado:* hecha (18 nov) · 3 rondas · `33807b8` · observaciones en `docs/fase4/pendientes.md`; criterios heredados en F4.1c, F4.5a y F4.6
- [x] **F4.1c** · XML: documento, elementos, Glushkov con contadores, `xs:all`, atributos, textos con tipos simples (NaN/INF), presupuesto de transiciones (D47) — *Pista:* A · *Depende:* F4.1b · *Resp.:* Leonardo · *Estado:* hecha (19 nov) · 3 rondas + 1 extra (D57) · observaciones en `docs/fase4/pendientes.md` §33-§56; criterios heredados en F4.3 y F4.6; D57 (1)-(4) pendiente del visto bueno de Bayron
- [x] **F4.2** · ~~Pesos en el semianillo tropical~~ — fusionada en F4.1a (D46, 15 nov) — *Pista:* A · *Resp.:* — · *Estado:* fusionada
- [x] **F4.3** · Verificación de determinismo de δ y Hopcroft en todas las partes finitas (`Dfa`) (D51) — *Pista:* A (o Bayron, en paralelo) · *Depende:* F4.1a · *Resp.:* Leonardo · *Estado:* hecha (19 nov) · 0 rondas (sin `/autodev`; `make lint`/`test`/`docs-check` en verde) · D60 · `docs/fase4/automata.md` §17; heredados de F4.1c recortados a `docs/fase4/pendientes.md` §63 · criterios en `tasks/todo.md`
- [x] **F4.4a** · Analizador de `pattern` en `ir::regex` (ECMA y XSD) y rechazo anti-ReDoS en la Fase 3 (D51) — *Pista:* A (o Bayron, en paralelo) · *Depende:* — · *Resp.:* Leonardo · *Estado:* hecha (20 nov) · 0 rondas (sin `/autodev`; `make lint`/`test`/`docs-check` en verde) · D61 · `docs/fase4/pattern.md`; D29 recortado a `docs/fase4/pendientes.md` §65 · criterios en `tasks/todo.md`
- [x] **F4.4b** · `pattern` → AFD minimizado dentro de M (D51) — *Pista:* A · *Depende:* F4.1c, F4.3, F4.4a · *Resp.:* Leonardo · *Estado:* hecha (20 nov) · 0 rondas (sin `/autodev`; `make lint`/`test`/`docs-check` en verde) · D62 · `docs/fase4/pattern.md` §6; observaciones en `docs/fase4/pendientes.md` §67-§68
- [x] **F4.5a** · Combinadores: producto con salidas y puertas al desapilar (`anyOf`, `oneOf`, `not`, `dependentSchemas`), tope de 4 096 (D51) — *Pista:* A · *Depende:* F4.1b, F4.4b · *Resp.:* Leonardo · *Estado:* hecha (20 nov) · 0 rondas (sin `/autodev`; `make lint`/`test`/`docs-check` en verde) · D63 · `docs/fase4/automata.md` §18; recortes 3 y 5 en `docs/fase4/pendientes.md` §70-§71
- [x] **F4.5b** · `uniqueItems` diferido (hash canónico) y literales compuestos de `enum`/`const` (D51) — *Pista:* A · *Depende:* F4.5a · *Resp.:* Leonardo · *Estado:* hecha (20 nov) · 0 rondas (sin `/autodev`; `make lint`/`test`/`docs-check` en verde) · D64 · `docs/fase4/automata.md` §19; literales compuestos en el recorte 2 (`docs/fase4/pendientes.md` §72, decisión de las dos personas)
- [x] **F4.6** · Ejecutor completo sobre el núcleo `no_std`: lexer, límites, reporte, bloques y buena formación; oráculo contra `expected` y la línea base `baseline` (D48, D51) — *Pista:* A · *Depende:* F4.1c (F6.7a para la comparación JSON) · *Resp.:* Leonardo · *Estado:* hecha (19 nov) · 0 rondas (sin `/autodev`, decisión del usuario; `make lint`/`test`/`docs-check` en verde) · D59 · `docs/fase4/runtime.md`; observaciones en `docs/fase4/pendientes.md` §59-§62 (§59 pendiente del visto bueno de Bayron)
- [x] **PC-E** · Punto de control E: un esquema JSON y el pedido XSD pasan por esquema → M → validador Rust y el corpus pasa — *Pista:* A · *Depende:* F4.6, F5.1 · *Resp.:* Leonardo · *Estado:* hecha (20 nov) · los 32 esquemas del corpus (JSON y XSD, con `valid-xml-pedido-01`) compilados con `rustc --edition 2021 -O main.rs`; todo payload del corpus da `expected` e igual al intérprete (`crates/codegen/tests/rust.rs`, `main_real_en_todo_el_corpus` y `banco_equivale_al_interprete`); sigue abierta la decisión §72 (literales compuestos, `docs/fase4/pendientes.md`)

## Fase 5 — generación de código (`crates/codegen`, `crates/runtime-c`)

- [x] **F5.1** · Emisor Rust: tablas de M como `const` + el núcleo `no_std` (D51), pila acotada `SV_MAX_DEPTH`, sin asignación dinámica — *Pista:* A · *Depende:* F4.6 · *Resp.:* Leonardo · *Estado:* hecha (20 nov) · 0 rondas (sin `/autodev`; `make lint`/`test`/`docs-check` en verde) · D74 · `schemac compile <esquema> --lang rust --out <dir>`; doc en `docs/fase5/codegen.md`; observaciones en `docs/fase5/pendientes.md` §1-§8 (revisión de seguridad a E3; contrato para F7.5 en §5, cierra F7.3-P2) · criterios en `tasks/todo.md`
- [x] **F5.2** · Emisor C11 con la API C-ABI (`sv_init`/`sv_feed`/`sv_finish`); header con cbindgen (al final, escrito por el emisor, sin cbindgen: D75) — *Pista:* A · *Depende:* F5.1 · *Resp.:* Leonardo · *Estado:* hecha (21 nov) · 0 rondas (sin `/autodev`) · D75 · doc en `docs/fase5/codegen-c.md`; observaciones en `docs/fase5/pendientes.md` §9-§11 (visto bueno de Bayron a la API C-ABI, §10)
- [x] **F5.3** · Prueba diferencial intérprete / C / Rust en todo el corpus (bloques de 1, 7 y 4096 bytes) — *Pista:* A · *Depende:* F5.2 · *Resp.:* Leonardo · *Estado:* hecha (21 nov) · 0 rondas (sin `/autodev`) · D76 · `crates/codegen/tests/diferencial.rs`
- [x] **F5.4** · Generador de payloads válidos desde la IR, con semilla; 20 primeros casos al corpus — *Pista:* AB · *Depende:* PC-C · *Resp.:* Bayron · *Estado:* hecha (20 nov) · D66 · sin rondas (sin agentes) · `cargo run -p bench --release --bin generar`; doc en `docs/fase5/generador.md`; pendiente XML (F5.4-P1)
- [x] **F5.4b** · Generador XML desde la IR de un XSD (F5.4-P1): válidos, inválidos y malformados; casos XML recursivos — *Pista:* AB · *Depende:* F5.4 · *Resp.:* Leonardo · *Estado:* hecha (22 nov) · 0 rondas (sin `/autodev`) · `crates/bench/src/generar_xml.rs`, binario `generar_xml`; oráculos DPDA y libxml2 (Docker), de acuerdo en todo lo generado; 8 casos XML nuevos (86 en total; Atlas por recargar); `docs/fase5/generador.md` §6
- [x] **F5.5** · Exportar árboles de derivación de payloads (JSON para d3-hierarchy, DOT para docs) — *Pista:* AB · *Depende:* — · *Resp.:* Bayron · *Estado:* hecha (20 nov) · D69 · sin rondas (sin agentes) · `cargo run -p derivation --bin arbol`; doc en `docs/fase5/arbol-derivacion.md`; endpoint y vista en F7.6

## Fase 6 — validación en tiempo real y métricas (`crates/bench`)

- [x] **F6.1** · Reporte de error con puntero JSON, offset y línea/columna desde la pila — *Pista:* A · *Depende:* F5.1 · *Resp.:* Leonardo · *Estado:* hecha (21 nov) · 0 rondas (sin `/autodev`) · D77 · doc en `docs/fase6/puntero.md`; para Bayron `docs/fase6/pendientes.md` §29 y §31 · criterios en `tasks/todo.md`
- [x] **F6.2** · Límites configurables: profundidad, tamaño, cadena, dígitos, tiempo — *Pista:* A · *Depende:* F5.1 · *Resp.:* Leonardo · *Estado:* hecha (21 nov) · 0 rondas (sin `/autodev`) · D78 · `schemac compile --limit`; doc en `docs/fase6/limites.md`; formato de `limits` del corpus por acordar (§30)
- [x] **F6.3** · Generador de payloads inválidos y malformados (mutaciones) — *Pista:* B · *Depende:* F5.4 · *Resp.:* Bayron · *Estado:* hecha (20 nov) · D67 · sin rondas (sin agentes) · `cargo run -p bench --release --bin mutar`; doc en `docs/fase6/mutaciones.md`; F6.3-P1 en `docs/fase6/pendientes.md` §28 (pista A)
- [x] **F6.4** · Mitigaciones (DTD/entidades, `pattern` como AFD, `$ref` cíclico) y fuzzing con cargo-fuzz — *Pista:* AB · *Depende:* F5.3 · *Resp.:* Leonardo · *Estado:* hecha (21 nov) · 0 rondas (sin `/autodev`) · D79 · `make fuzz`; docs en `docs/fase6/mitigaciones.md` y `fuzzing.md`; §32 a E3
- [x] **F6.5** · Arnés de métricas: MB/s, ciclos/byte, pico de memoria, asignaciones, profundidad, estados — *Pista:* B · *Depende:* F6.7 · *Resp.:* Bayron · *Estado:* hecha (20 nov) · D65 · `cargo run -p bench --release --bin metricas`; resultados en `docs/fase6/metricas.md`
- [x] **F6.6** · Benchmark criterion del parser de esquemas con N y 4N claves (sustituye la prueba ignorada, D1) — *Pista:* B · *Depende:* — · *Resp.:* Bayron · *Estado:* hecha (19 nov) · D58 · `crates/bench/benches/parser_claves.rs`; razones 4,6-5,4 (lineal); `docs/fase2/pendientes.md` §1
- [x] **F6.7** · Líneas base: `serde_json` + `jsonschema` y libxml2 (DOM + XSD) sobre el corpus — *Pista:* B · *Depende:* — · *Resp.:* Bayron · *Estado:* hecha (18 nov) · **F6.7a (JSON): 15 nov · 1 ronda** (D48-D50) y **F6.7b (XML, libxml2 en Docker): 18 nov · 1 ronda** (D54); observaciones en `docs/fase6/pendientes.md` §1-§27 (§14 y §21 bloqueantes para F7; §26 por confirmar con Leonardo); criterios en `tasks/todo.md`

## Fase 7 — aplicación web (`server/`, `web/`, `sandbox/`, `infra/`)

- [x] **F7.1** · Backend axum: `POST /api/schemas` con `check`/`ir` en hilo propio con `stack_size` (D7), topes de subida, CPU y memoria (`docs/fase3/pendientes.md` §1-§2) — *Pista:* B · *Depende:* — · *Resp.:* Bayron · *Estado:* hecha (19 nov) · 3 rondas (D55) · observaciones en `docs/fase7/pendientes.md` §1-§12 (tope duro de memoria → F7.3/F7.7; Caddy → F7.7); contrato por confirmar con Leonardo (§9)
- [x] **F7.2** · Persistencia en MongoDB Atlas y carga de `tests/cases/` con script local (D32) — *Pista:* B · *Depende:* S0, F7.1 · *Resp.:* Bayron · *Estado:* hecha (20 nov) · D72 · sin rondas (sin agentes) · 78 casos en Atlas, 78/78 pasan con `POST /api/tests/run`; doc en `docs/fase7/atlas.md`; endpoints pendientes del visto bueno de Leonardo (D42)
- [x] **F7.3** · Sandbox Docker (`--network none`, límites, no root, FS de solo lectura) para compilar y ejecutar el código generado — *Pista:* B · *Depende:* — · *Resp.:* Bayron · *Estado:* hecha (20 nov) · D68 · sin rondas (sin agentes) · doc en `docs/fase7/sandbox.md`; pruebas con Docker `scripts/test-sandbox.ps1`; F7.3-P1/P2 en `docs/fase7/pendientes.md` §13-§14
- [x] **F7.4** · Frontend: editor Monaco, compilar, ver la IR (JSON v2, D17; tratar como no confiable) — *Pista:* B · *Depende:* F7.1 · *Resp.:* Bayron · *Estado:* hecha (19 nov) · D56 · `web/README.md`; `docs/fase7/pendientes.md` §5 resuelto salvo §9 de la Fase 3
- [x] **F7.5** · Descarga del código, `POST /api/validate` y WebSocket de validación por bloques — *Pista:* B · *Depende:* F5.3, F7.3 · *Resp.:* Bayron · *Estado:* hecha (21 nov) · D81 · sin rondas (sin agentes) · doc en `docs/fase7/codigo.md`; probado con Docker (C y Rust = intérprete) y el WebSocket tras Caddy; endpoints pendientes del visto bueno de Leonardo (D42); F7.5-P1 en `docs/fase7/pendientes.md` §16
- [x] **F7.6** · Vistas del autómata (Cytoscape), árbol de derivación y panel de métricas DPDA vs línea base — *Pista:* B · *Depende:* F7.4, F5.5, F6.5 · *Resp.:* Bayron · *Estado:* hecha (20 nov) · D70 · sin rondas (sin agentes) · doc en `docs/fase7/vistas.md`; endpoints `/api/automaton`, `/api/derivation` y `/api/compare` pendientes del visto bueno de Leonardo (D42); F7.6-P2 en `docs/fase7/pendientes.md` §15
- [x] **F7.7** · Despliegue en EC2 con Docker Compose y Caddy (primera versión lo antes posible) — *Pista:* B · *Depende:* S0, F7.1 · *Resp.:* Bayron · *Estado:* hecha (22 nov) · D71 · sin rondas (sin agentes) · **https://54-157-96-49.sslip.io** (EC2 `t3.micro` us-east-1, Elastic IP 54.157.96.49); comprobado en producción: web y cabeceras, HTTPS, 86/86 casos de Atlas, ZIP de código y WebSocket; doc en `docs/fase7/despliegue.md` §2-§3 · sandbox del código generado también en la EC2 (`t3.small`, `desplegar.ps1 -Sandbox`, D85)

## Entrega (semana 5)

- [x] **E1** · Pruebas de bombeo u·vᵏ·x·yᵏ·z y casos extremos; gráficas de tiempo, pila y memoria contra k — *Pista:* AB · *Depende:* F6.5 · *Resp.:* Bayron · *Estado:* hecha (21 nov) · D80 · sin rondas (sin agentes) · `docs/fase6/bombeo.md` (7 pruebas del lema, 5 gráficas SVG en `docs/fase6/graficas/`)
- [x] **E2** · Cargar todos los casos y sus métricas en Atlas — *Pista:* B · *Depende:* F7.2, F6.5 · *Resp.:* Bayron · *Estado:* hecha (21 nov) · D73 · sin rondas (sin agentes) · 78 casos y 173 métricas en Atlas (`cargo run -p atlas -- --metricas <csv>`); doc en `docs/fase7/atlas.md` §6
- [x] **E3** · Revisión de seguridad consolidada de las Fases 4-7 (D35) — *Pista:* AB · *Depende:* F6.4, F7.5 · *Resp.:* Leonardo (revisión, pista A y verificación); Bayron (servidor, D83) · *Estado:* hecha (21 nov) · tres `security-auditor` (validador y código generado; compilador con esquemas hostiles; servidor y plataforma) · pista A corregida (E3B-1, E3B-2, E3B-3, E3A-1, §61, clave aleatoria §31) · servidor corregido por Bayron (D83) y verificado con las sondas de la auditoría; §19 no bastaba y se corrigió en la verificación (`ConReserva` por trozos) · quedan Medium/Low en `docs/fase7/pendientes.md` §20, §22, §24 · informes en `tasks/contexto/E3 *.md`
- [x] **E4** · Informe final: complejidad, explosión al determinizar, decisiones, vulnerabilidades, bombeo — *Pista:* AB · *Depende:* E1 · *Resp.:* Leonardo · *Estado:* hecha (21 nov) · 0 rondas (sin `/autodev`; revisión de exactitud con sonnet, hallazgos corregidos) · `docs/informe-final.md`; lo abierto de seguridad (clave de hash pública, hilos que no se cancelan, `docker.sock`) a E3
- [x] **E5** · Cerrar documentación técnica (diagramas finales, DPDA de 2 esquemas, árboles de derivación) — *Pista:* AB · *Depende:* F5.3 · *Resp.:* Leonardo · *Estado:* hecha (21 nov) · 0 rondas (sin `/autodev`; revisión de exactitud con sonnet, 8 hallazgos corregidos) · índice y definición formal en `docs/README.md`, arquitectura en `docs/arquitectura.md`, tercer DPDA (`arbol-recursivo`) y SVG de los árboles de derivación
- [ ] **E6** · Congelar código — *Pista:* AB · *Depende:* — · *Resp.:* ambos · *Estado:* libre · previsto 19 nov
- [ ] **E7** · Diapositivas, guion, video de respaldo y dos ensayos de la demo — *Pista:* AB · *Depende:* F7.7 · *Resp.:* ambos · *Estado:* en curso (21 nov) · diapositivas (18, en claude.ai) y guion `docs/presentacion/guion.md` hechos por Leonardo; faltan la URL pública (F7.7), el video de respaldo y los dos ensayos
- [ ] **E8** · WASM (opcional, solo si todo lo demás está cerrado) — *Pista:* A · *Depende:* F5.3 · *Resp.:* — · *Estado:* libre

## DTD como tercer lenguaje de esquema (rama `DTD`, D86; `main` no se toca hasta cerrar)

Propuesta de Leonardo en `tasks/propuesta-dtd.md`. Todas las tareas las lleva Bayron; Leonardo revisa.

- [x] **DTD.0** · Registrar D86 y la sección §3bis de `docs/00-alcance.md` con el visto bueno de los dos — *Pista:* AB · *Depende:* — · *Resp.:* Bayron · *Estado:* hecha (22 nov) · D86 · sin código
- [x] **DTD.1** · Lexer DTD (AFD, tabla, `.dot`, pruebas por bloques de 1/7/4096 y oráculo) y gramática LL(1) — *Pista:* B (excepción, D86) · *Depende:* DTD.0 · *Resp.:* Bayron · *Estado:* hecha (23 nov) · sin rondas (sin agentes, decisión del usuario; verificado con lint y pruebas) · crate `crates/dtd` (`lexer.rs`: 26 estados, 28 clases), `schemac dot dtd`, `docs/fase1/lexico-dtd.md` y `afd-dtd.dot/.svg`, `docs/fase2/dtd.ll1` (LL(1) sin conflictos), `gramatica-dtd.md` y `ll1-dtd.md`; 22 pruebas (oráculo con 20 000 casos)
- [x] **DTD.2** · Parser LL(1), AST y rechazo de lo que está fuera del subconjunto con categoría y motivo — *Pista:* B (excepción, D86) · *Depende:* DTD.1 · *Resp.:* Bayron · *Estado:* hecha (23 nov) · sin rondas (sin agentes, decisión del usuario; verificado con lint y pruebas) · `crates/dtd/src/{parser,ast}.rs`: una función por no terminal, grupos con pila explícita y tope de 256 (D7), clases `Lexical`/`Syntax`/`OutOfScope`/`Limit` con posición y motivo; 10 pruebas (ida y vuelta con 2 000 casos, un millón de paréntesis en 256 KiB de pila); doc en `docs/fase2/gramatica-dtd.md` §5-§7
- [x] **DTD.3** · Análisis, determinismo y bajada a la IR verificada; doradas de IR para 2-3 esquemas — *Pista:* B (excepción, D86) · *Depende:* DTD.2 · *Resp.:* Bayron · *Estado:* hecha (23 nov) · sin rondas (sin agentes, decisión del usuario; verificado con lint y pruebas) · `crates/dtd/src/{check,lower}.rs` y `dtd::compile`: tabla de símbolos, todos los errores en orden, determinismo por Glushkov (diferencial con la UPA de XSD, 3 000 modelos), bajada con la forma de XSD y sin campos nuevos; doradas `tests/ir/dtd/{pedido,biblioteca}.json`; de extremo a extremo con `automaton::build`; 45 pruebas de `dtd`; doc en `docs/fase3/dtd.md`
- [x] **DTD.4** · Detección de formato común (D22 enmendada) en `schemac`, servidor y web; ejemplo DTD en la web — *Pista:* B (excepción, D86) · *Depende:* DTD.3 · *Resp.:* Bayron · *Estado:* hecha (23 nov) · sin rondas (sin agentes, decisión del usuario; verificado con lint, pruebas y la pila en Docker; security-auditor pendiente si se pide) · `dtd::formato` única en Rust (CLI y servidor), copia TS probada con la misma tabla `tests/schemas/formato.json` (29 casos); `schemac` y el servidor compilan DTD con `format: "xml"` (la API no cambia); web con etiqueta DTD, ejemplo y `.dtd`; doc en `docs/fase3/dtd.md` §5
- [x] **DTD.5** · Casos en `tests/cases`, fila en `docs/fase6/mitigaciones.md`, objetivo de fuzzing `esquema_dtd`, carga en Atlas y diferencial DTD frente a XSD **por veredictos** (2-3 pares equivalentes; payloads válidos, inválidos y malformados generados desde las dos IR; mismo veredicto y misma clase de error; enmienda E3) — *Pista:* B (excepción, D86) · *Depende:* DTD.4 · *Resp.:* Bayron · *Estado:* hecha (23 nov), salvo la carga en Atlas, que va tras fusionar con `main` y desplegar · revisión: security-auditor · sin rondas (sin agentes, decisión del usuario) · 8 casos DTD en `tests/cases` (94 en total); lectores del corpus por `dtd::formato` (automaton, codegen, bench, xsd, baseline); 14 esquemas DTD rechazados con su código y posición; 3 filas en `mitigaciones.md`; objetivo `esquema_dtd` (compila; sin correr: nightly); diferencial por veredictos en 2 422 documentos; doc en `docs/fase3/dtd.md` §6
- [ ] **DTD.6** · Documentación final: `00-alcance.md`, `docs/README.md`, `informe-final.md` y `arquitectura.md` — *Pista:* B (excepción, D86) · *Depende:* DTD.5 · *Resp.:* Bayron · *Estado:* libre
