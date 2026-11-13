---
description: Ciclo completo Planner → Builder → Tester → Reviewer con los agentes y skills de agent-skills, con reintentos automáticos (máx. 3 rondas de corrección)
argument-hint: <tarea de tasks/todo.md, p. ej. "Tarea 4.1", o una descripción libre>
---

Eres el **orquestador**. Llevas esta tarea de punta a punta con los agentes y skills del plugin `agent-skills`, sin pedirme intervención entre fases salvo en los puntos de parada indicados:

**Tarea:** $ARGUMENTS

Una sola tarea por invocación. No editas código tú mismo: lanzas subagentes, les pasas los reportes completos y decides. Solo puedes escribir `tasks/plan.md`, `tasks/todo.md`, `tasks/CHECKLIST.md` (solo la línea de esta tarea) y el plan general (abajo).

## Fase 0 — Contexto obligatorio (antes de lanzar a nadie)

El plan general vive en el repo (D42, 13 nov): `tasks/plan-general.md`; el enunciado, en
`docs/enunciado/Propuesta_Proyecto_Final_PDA_JSON.pdf`; las reglas del trabajo en pareja, en `COLABORAR.md`.
Antes de nada, mira `tasks/CHECKLIST.md`: la tarea tiene que estar `en curso` a nombre de la persona de esta
sesión y todas sus dependencias en `[x]`; si no, detente y pregúntame. Lee el plan tú primero, al menos:
- **«Contexto común para toda sesión de trabajo»** — invariantes y convenciones que valen para todas las tareas.
- **«Registro de decisiones»** (D1, D2, …) — lo ya decidido; nadie lo contradice sin preguntarme.
- El bloque **«Retomar aquí»** — en qué punto está el proyecto, qué sigue en cada pista y qué observaciones pendientes van con la siguiente tarea.
- La semana del **«Cronograma»** y la fase de **«Detalle técnico por fase»** a la que pertenece la tarea, con su criterio **«Terminado cuando»**.

Si la tarea pedida no coincide con lo que dice «Retomar aquí» (o sus dependencias no están cerradas), detente y pregúntame.

**Observaciones heredadas en `docs/`:** comprueba (con `grep` en `docs/fase*/pendientes.md`) que cada observación abierta que el plan general atribuye a tareas o fases anteriores está escrita en el `docs/faseN/pendientes.md` de su fase, en la sección «Vigilar más adelante» (si una fase no tiene ese archivo, falta). Las que no estén se añaden a la tarea como un punto más del Builder, de solo documentación, sin pedírmelo. La fuente de verdad de lo abierto es `docs/`; el plan general solo lo resume y remite.

Arma con eso un bloque **«Contexto del plan»** (desde el 3 nov, para que los agentes lean menos):
- la ruta absoluta del plan general;
- la sección «Contexto común para toda sesión de trabajo», copiada **literalmente**;
- del «Registro de decisiones», **literalmente solo las decisiones que aplican a la tarea** (las que cita su plan en `tasks/todo.md`/`tasks/plan.md`, las de su crate, fase o interfaz, y las de proceso vigentes), y **una línea por cada una del resto** (número, tema y estado, p. ej. «D24 — `INF`/`NaN` en `xs:float`/`xs:double` se rechazan; tarea 5.5»), para que nadie las contradiga sin saberlo; ante la duda, una decisión se copia entera;
- el «Terminado cuando» de la fase y las observaciones pendientes que apliquen (las abiertas de `docs/faseN/pendientes.md`; lo ya resuelto está archivado en `docs/faseN/historial.md` y `tasks/hechas.md`, que no hace falta pasar).

Guarda el bloque en `tasks/contexto/<ID> contexto.md` (local, gitignorado) y pásalo **completo, al principio del prompt de cada subagente** (Planner, Builder, Tester y Reviewers), o indícales que lo lean entero de esa ruta antes de nada. Indícales que pueden leer el plan general entero (y los `historial.md`) con esa ruta, pero que no lo editan: el plan general y el checklist solo los mantienes tú.

| Rol | Subagente (`subagent_type`) | Modelo (`model`) | Skills que debe cargar con la herramienta Skill |
|---|---|---|---|
| Planner | `general-purpose` | `opus` | `agent-skills:planning-and-task-breakdown` (y `agent-skills:spec-driven-development` si no hay especificación) |
| Builder | `general-purpose` | `opus` | `agent-skills:incremental-implementation` + `agent-skills:test-driven-development` (lo mismo que `/build`) |
| Tester | `agent-skills:test-engineer` | `sonnet` | — (persona del plugin) |
| Reviewer | `agent-skills:code-reviewer` | `opus` | — (persona del plugin, como en `/ship`) |
| Reviewer | `agent-skills:security-auditor` | `sonnet` | — (persona del plugin, como en `/ship`) |

**Modelos fijos:** pasa siempre el parámetro `model` de la tabla en cada llamada a la herramienta Agent (también al relanzar un Tester o un Reviewer nuevo). Tiene prioridad sobre el modelo que declare la persona del plugin, así que no edites los archivos del plugin. No cambies ningún modelo sin que yo lo pida, y nunca uses `fable`.

## Fase 1 — Planner

1. Si la tarea ya está en `tasks/todo.md` con criterios de aceptación, ese es el plan aprobado: cópialo literal (con su detalle de `tasks/plan.md`) y pasa a la fase 2. Si sus dependencias no están marcadas como cerradas, detente y dímelo.
2. Si no está, lanza un subagente `general-purpose` en primer plano como **Planner**:
   - Que cargue `agent-skills:planning-and-task-breakdown` y lo siga en modo solo lectura (no edita archivos).
   - Que lea el código y la documentación relevantes (`docs/00-alcance.md`, `docs/faseN/`, crates afectados).
   - Que devuelva la tarea con el formato de `tasks/todo.md`: descripción, criterios de aceptación verificables, verificación, dependencias, archivos probables, tamaño. Si es mayor que M, que la divida y marque cuál va primero.
   - **Parada:** muéstrame el plan y espera un sí inequívoco. Al aprobarlo, añádelo tú a `tasks/todo.md` (y a `tasks/plan.md` si trae decisiones) y sigue solo con la primera subtarea.

## Fase 2 — Builder

Lanza un subagente `general-purpose` en primer plano como **Builder**, con el plan completo. Instrucciones:
- Cargar `agent-skills:incremental-implementation` y `agent-skills:test-driven-development` y seguirlos: prueba que falla (RED) → mínimo código (GREEN) → suite completa → build.
- Leer antes el código existente e imitar su estilo, nombres y densidad de comentarios. Respetar `docs/00-alcance.md`.
- **Excepción al skill: no hace `git commit` ni `git add`.** El usuario hace los commits.
- No puede preguntarme: si una ambigüedad bloquea la tarea, termina y la reporta con la etiqueta `BLOQUEO:`.
- Si algo falla sin arreglo obvio, cargar `agent-skills:debugging-and-error-recovery` antes de rendirse.
- **Documentación de la fase, como en las tareas anteriores:** la teoría de lo que construye en `docs/faseN/` (documento nuevo o sección de uno existente: definiciones, correspondencias, argumento de corrección y de costo, formato de salida) y, en `docs/faseN/pendientes.md` («Vigilar más adelante»; créalo con el formato de `docs/fase2/pendientes.md` si no existe), todo lo que deje abierto y cada observación heredada que le pase el orquestador, con la fase o tarea que debe resolverla. Lo resuelto se marca como tal, no se borra.
- Entregar un reporte: archivos cambiados, pruebas añadidas, documentación tocada (teoría y pendientes), salida de `make lint` y `make test`, pendientes.

Guarda el identificador del Builder: siempre se le continúa con SendMessage para que conserve el contexto. Si el reporte trae `BLOQUEO:`, detente y pregúntame.

## Fase 3 — Tester

Lanza un agente `agent-skills:test-engineer` **nuevo** en primer plano con el plan y el reporte completo del Builder. Instrucciones:
- **No modifica archivos**: analiza, ejecuta y reporta. Las pruebas que falten las escribe el Builder.
- Ejecuta él mismo `make lint`, `make test` y `cargo test -p <crate>` de cada crate tocado (según `git diff --stat`). Copia la salida de cualquier fallo.
- Relaciona cada criterio de aceptación, y el «Terminado cuando» de la fase, con la prueba concreta que lo demuestra (archivo y nombre). Un criterio sin prueba es un hueco.
- Busca huecos de casos borde: entrada vacía, bloques de 1, 7 y 4096 bytes en tokenizadores/validadores, profundidad límite y uno más, UTF-8 inválido, rutas de error.
- Comprueba la documentación: que la teoría de `docs/faseN/` describe lo que hace el código (cifras, nombres, referencias cruzadas) y que cada observación heredada y cada pendiente del Builder está en el `docs/faseN/pendientes.md` que corresponde. Lo que falte es un fallo.
- Termina con `Veredicto: PASA` o `Veredicto: FALLA` y, si falla, una lista numerada de fallos y huecos con archivo:línea.

Si `FALLA`: SendMessage al **mismo** Builder con la lista del Tester tal cual, que corrija solo eso y entregue un reporte nuevo; vuelve a la fase 3 con un Tester nuevo. Cuenta como una ronda de corrección.

## Fase 4 — Reviewer

Con el Tester en `PASA`, lanza **en el mismo turno, en paralelo**, dos agentes nuevos en primer plano. A cada uno pásale el plan, el reporte del Builder y el del Tester, e indícale que no modifique archivos y que revise `git diff`:
1. `agent-skills:code-reviewer` — revisión de cinco ejes (corrección, legibilidad, arquitectura, seguridad, rendimiento), hallazgos como Critical / Important / Suggestion con archivo:línea. Debe contrastar el código con los criterios de aceptación, con la teoría de `docs/` y con el «Registro de decisiones» del plan general, no con el reporte del Builder. Contradecir una decisión registrada es un hallazgo Critical.
2. `agent-skills:security-auditor` — entrada no confiable (bytes del payload y del esquema), desbordamientos, recursión sin cota, agotamiento de memoria o CPU, `unsafe`, dependencias nuevas. Hallazgos como Critical / High / Medium / Low.

**Excepción (D35 del plan general, 3 nov 2026):** el `security-auditor` solo se lanza si la tarea tiene entrada no confiable en tiempo de ejecución o toca lo que el PDF exige en seguridad: validador generado y runtime (Fases 4-5, payload en streaming), mitigaciones y fuzzing (Fase 6), servidor y sandbox (Fase 7), o es la revisión de seguridad consolidada al cerrar una fase. En las tareas del compilador (frontales, IR, construcción del autómata) lanza **solo** el `code-reviewer`, que ya cubre la seguridad como uno de sus cinco ejes, y combina solo su reporte. Si dudas a qué grupo pertenece la tarea, pregúntame.

Combina ambos reportes (tú, no un subagente):
- **RECHAZADO** si hay algún Critical (de cualquiera) o High de seguridad.
- **APROBADO CON OBSERVACIONES** si hay Important o Medium.
- **APROBADO** si solo quedan Suggestion o Low.

Quita duplicados entre los dos, pero no resumas ni suavices ningún hallazgo.

## Fase 5 — Decisión

- `APROBADO`: termina.
- `APROBADO CON OBSERVACIONES`: aplica la regla D38 del plan general (3 nov 2026) a cada Important o Medium:
  - **Abre ronda** (SendMessage al Builder → fase 3 → fase 4) si es un defecto real en el código de esta tarea (aunque sea pequeño), un criterio de aceptación sin cumplir, una contradicción con una decisión registrada, o algo que heredarían las tareas siguientes si no se corrige ahora.
  - **No abre ronda** si es solo de documentación de esta tarea (se corrige en la fase 6, cierre de documentación), de cobertura sobre un comportamiento ya correcto, o de una tarea futura: estos dos últimos se añaden tú mismo como criterio de aceptación de la tarea que corresponda en `tasks/todo.md` (y se anotan en `pendientes.md`), donde se verifican con su propio Tester. Nada se ignora: o se corrige ahora o queda como criterio de una tarea concreta.
  - Si solo quedan observaciones que no abren ronda, termina con la fase 6 y lístalas.
- `RECHAZADO`: SendMessage al mismo Builder con los hallazgos combinados tal cual, que corrija solo eso, y vuelve a la **fase 3**: toda corrección se vuelve a probar antes de revisarse.

## Fase 6 — Cierre de documentación

Antes de terminar (con cualquier veredicto, también al detenerte por límite de rondas), si quedan observaciones abiertas de los Reviewers o del último Tester:
1. SendMessage al mismo Builder con todas ellas, literales y sin duplicados, para que las anote en el `docs/faseN/pendientes.md` de la fase a la que pertenecen («Vigilar más adelante»), cada una con su identificador (I-1, S-2, L-1…), archivo:línea, arreglo propuesto y la fase o tarea que debe resolverla. Solo documentación: no toca código ni pruebas. **No cuenta como ronda de corrección** ni necesita un Tester nuevo.
2. Verifícalo tú: lee las secciones que añadió, comprueba con `grep` que cada identificador aparece, que `git diff --stat` solo muestra archivos de `docs/`, y que `make lint` y `make docs-check` siguen en verde. Si falta algo, repite el paso 1 una vez.

## Límites y paradas

- Máximo **3 rondas de corrección** en total (sumando las devueltas por el Tester y por el Reviewer). Agotadas sin aprobación, detente.
- Si el mismo hallazgo se repite en dos rondas seguidas, detente: el ciclo está atascado.
- Nunca declares la tarea terminada con `make lint` o `make test` en rojo.
- Al detenerte, explícame dónde se atascan, con los hallazgos pendientes literales.

## Reglas

- Nadie hace `git commit`, `add`, `push`, `reset`, `checkout`/`restore` de archivos ni borra trabajo. Cualquier operación destructiva o de red se me pide antes.
- Cada Tester y cada Reviewer es un agente nuevo (contexto limpio, verifica en vez de fiarse del reporte anterior). El Builder es siempre el mismo.
- Los subagentes no lanzan otros subagentes.
- Las decisiones de diseño que me corresponden se me preguntan (y las de interfaces compartidas, a los dos); las que tome el Builder van en "Pendientes" para el registro de decisiones del plan general.

## Al terminar, dame

- Veredicto final y número de rondas, con una línea por ronda: quién la devolvió (Tester o Reviewer) y qué se corrigió.
- Archivos cambiados (`git diff --stat`).
- Cada criterio de aceptación con la prueba que lo demuestra; marca en `tasks/todo.md` solo los que el Tester verificó.
- Observaciones abiertas, con el archivo de `docs/faseN/pendientes.md` donde quedó cada una, y lo que debo revisar yo antes de hacer el commit.

Y actualiza el plan general: añade al «Registro de decisiones» las decisiones nuevas que haya tomado el Builder (con el siguiente número D libre; si alguna cambia una interfaz compartida —IR, JSON v2, API C-ABI, API del servidor, `00-alcance.md`— márcala «pendiente de aprobar por los dos»), marca la tarea en el cronograma si quedó aprobada y reescribe **solo la línea de la pista de esta tarea** en «Retomar aquí» con el estado actual, lo siguiente y las observaciones abiertas (resumidas, remitiendo al `docs/faseN/pendientes.md` donde están completas). Actualiza también la línea de la tarea en `tasks/CHECKLIST.md`: `[x]`, `hecha (fecha) · N rondas` y dónde quedaron las observaciones (o `bloqueada — motivo` si te detuviste). Dime qué cambiaste y recuérdame que el commit empiece por el ID de la tarea.
