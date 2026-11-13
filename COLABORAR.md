# Cómo trabajamos

Guía para las dos personas del equipo (y para sus sesiones de Claude Code). Desde el 13 nov 2026 trabajamos en
simultáneo, cada uno desde su cuenta (D42 de `tasks/plan-general.md`).

## Qué leer y en qué orden

| # | Archivo | Para qué |
| --- | --- | --- |
| 1 | `docs/enunciado/Propuesta_Proyecto_Final_PDA_JSON.pdf` | Lo que exige el curso. Manda sobre todo lo demás. |
| 2 | `tasks/plan-general.md` | Cómo y cuándo se hace. Leer como mínimo «Contexto común para toda sesión», «Registro de decisiones» (D1-D42) y «Retomar aquí». |
| 3 | `tasks/CHECKLIST.md` | Quién lleva qué y qué está hecho. |
| 4 | `docs/00-alcance.md` | Subconjunto soportado, clase (a)/(b)/(c) de cada palabra clave, límites. Si choca con el plan en algo técnico, manda el alcance. |
| 5 | `docs/faseN/` y `docs/faseN/pendientes.md` | Teoría de cada fase y lo que dejó abierto cada revisión («Vigilar más adelante»). Lo cerrado está en `docs/faseN/historial.md`. |
| 6 | `tasks/todo.md`, `tasks/plan.md` | Criterios de aceptación de las tareas ya planificadas. Lo cerrado está en `tasks/hechas.md`. |

## Puesta en marcha (una vez)

1. Clonar: `git clone https://github.com/leonardo079/Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados.git`
   (el dueño del repo tiene que añadir al compañero como colaborador en GitHub → Settings → Collaborators).
2. Herramientas: Rust estable con `rustup` (el repo fija `rust-toolchain.toml`: rustfmt, clippy, llvm-tools),
   `make`, Graphviz (`dot`, para `make docs`), `clang-format` (para el C del runtime), Python 3 (`scripts/ll1.py`).
3. Comprobar que todo está en verde: `make lint && make test && make docs-check`.
4. Claude Code: al abrir el repo, Claude Code lee `CLAUDE.md`, y `.claude/settings.json` propone instalar el plugin
   `agent-skills` (marketplace `addyosmani/agent-skills`) que usa `/autodev`. Si no lo propone:
   `/plugin marketplace add addyosmani/agent-skills` y `/plugin install agent-skills@addy-agent-skills`.
   Los comandos `/autodev` y `/tarea` y los agentes `desarrollador` y `auditor` vienen en `.claude/`.
5. Modelos: solo `opus` y `sonnet` (en `/autodev`: Planner, Builder y code-reviewer en opus; Tester y
   security-auditor en sonnet). Nunca `fable`.

## Ciclo de una tarea

1. `git pull --rebase`.
2. En `tasks/CHECKLIST.md`, tomar una tarea `libre` con todas sus dependencias en `[x]`: poner tu nombre y
   `en curso (fecha)`. Commit solo de ese archivo (`checklist: tomo F4.1`) y `git push` **antes de empezar**.
3. En Claude Code: `/autodev F4.1` (o `/autodev Tarea 5.5`). Si la tarea ya está en `tasks/todo.md`, usa esos
   criterios; si no, el Planner propone el plan y **lo aprueba quien lleva la tarea**.
4. `/autodev` corre Builder → Tester → Reviewer con un máximo de 3 rondas de corrección (reglas abajo) y al
   final actualiza `tasks/todo.md`, el plan general (decisiones nuevas, casilla del cronograma, «Retomar aquí»)
   y la línea de la tarea en `tasks/CHECKLIST.md`.
5. Revisar `git diff`, `make lint`, `make test` (y `make docs-check` si se tocó `docs/` o una gramática).
6. Commit con todo (código, docs, `tasks/`) y mensaje que empiece por el ID: `F4.1: construcción composicional`.
   `git pull --rebase`, resolver conflictos, volver a pasar `make test` si el rebase trajo código, y `git push`.

## Reglas que no cambian

**Invariantes del código** (detalle en «Contexto común» del plan):

- Validación O(n) en bytes y O(d) en pila; el validador generado no construye árbol del payload.
- Sin asignación dinámica en el camino de validación (lexer `no_std` sin `alloc`; `sv_state_t` de tamaño fijo).
- Determinismo: una transición por (q, clase, X); mismo resultado con bloques de 1, 7 y 4096 bytes.
- `unsafe_code = "deny"`, clippy con `-D warnings`, finales de línea LF.
- Pruebas deterministas: nada de umbrales de tiempo en `cargo test` (D1); las mediciones van a `crates/bench`.
- Todo lo que recorre entrada del usuario es iterativo o va en un hilo con `stack_size` explícito (D7).
- Los `.dot` y `ll1-*.md` de `docs/` se generan con `make docs`; no se editan a mano.

**Proceso:**

- **Los agentes nunca hacen `git commit`, `add` ni `push`.** Cada persona hace los commits de sus tareas.
- **Una tarea a la vez por persona**, y no se pasa a la siguiente hasta que la actual quede aprobada.
- **GitHub solo guarda los commits (D32):** sin issues, PR, Projects ni Actions. Toda verificación es local:
  `make lint`, `make test`, `make docs-check`, y `cargo test -p semantic --release` si se toca `semantic`.
- **Revisión (D35):** las tareas del compilador (frontales, IR, construcción del autómata) las revisa solo
  `agent-skills:code-reviewer`; `agent-skills:security-auditor` entra en runtime y validador generado, fuzzing,
  servidor y sandbox (Fases 4-7) y en una revisión consolidada al cerrar cada fase.
- **Rondas (D38):** un Critical o un High de seguridad rechaza. Un Important o Medium abre ronda si es un defecto
  real, un criterio sin cumplir, una contradicción con una decisión registrada o algo que heredarían las tareas
  siguientes; si es solo documentación se corrige en el cierre; si es cobertura de algo ya correcto o de una tarea
  futura, pasa como criterio de la tarea que corresponda en `tasks/todo.md` y en `pendientes.md`. Nada se ignora.
- Lo que una tarea deja abierto va a `docs/faseN/pendientes.md` («Vigilar más adelante»), con identificador,
  archivo:línea, arreglo propuesto y la tarea que lo debe cerrar.

## Trabajo en paralelo: cómo no pisarnos

- **Pistas:** A = compilador (Fase 4 → Fase 5), B = plataforma (líneas base, servidor, web, sandbox, despliegue,
  XSD 5.5-5.7). Dentro de una pista las tareas van en orden; entre pistas solo se cruzan donde el checklist
  lo dice en `Depende`.
- **Interfaces compartidas:** un cambio en la IR o `VerifiedIr` (`crates/ir`), el JSON v2 de `schemac ir` (D17),
  la API C-ABI del validador, la API del servidor o `docs/00-alcance.md` lo aprueban **los dos** antes de entrar,
  y se registra como decisión nueva.
- **Números de decisión:** antes de añadir una decisión, `git pull`; se toma el siguiente número libre. Si al hacer
  `pull` el otro ya usó ese número, renumeras la tuya (y sus referencias en `docs/` y `tasks/`).
- **Archivos donde más chocamos:** `tasks/CHECKLIST.md` (se conservan las dos líneas), `tasks/todo.md` (cada
  tarea en su propia sección), «Retomar aquí» del plan (una línea por pista), `Cargo.toml` del workspace y
  `Cargo.lock` (tras resolver el `.toml`, `cargo build` regenera el lock).
- **Commits pequeños y frecuentes:** `pull --rebase` al empezar el día, al tomar una tarea y antes de cada push.
- El contexto que `/autodev` arma para cada tarea se guarda en `tasks/contexto/` y no se versiona.
