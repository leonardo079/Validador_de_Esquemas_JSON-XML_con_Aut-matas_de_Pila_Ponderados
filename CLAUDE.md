# Instrucciones para Claude Code

Proyecto en pareja: dos personas trabajan a la vez, cada una desde su cuenta. Las reglas de trabajo están en
`COLABORAR.md` (importado abajo) y el plan, con el registro de decisiones, en `tasks/plan-general.md`.

@COLABORAR.md

## Reglas para Claude en este repo

- Al empezar una sesión de trabajo, leer en `tasks/plan-general.md` «Contexto común para toda sesión de trabajo»,
  el «Registro de decisiones» y «Retomar aquí», y mirar `tasks/CHECKLIST.md`. Si la tarea pedida no está tomada
  por la persona de esta sesión en el checklist, o sus dependencias no están en `[x]`, preguntar antes de seguir.
- **Nunca hacer `git commit`, `git add`, `git push`, `reset`, `checkout`/`restore` de archivos ni borrar trabajo.**
  Git solo en lectura (`status`, `diff`, `log`). Los commits los hace la persona.
- Una tarea por vez, con `/autodev`; no avanzar a la siguiente hasta que la actual esté aprobada.
- Modelos de subagentes: solo `opus` o `sonnet`; nunca `fable`.
- No consultar ni proponer GitHub Actions, issues, PR ni Projects (D32).
- Nadie contradice una decisión registrada (D1-D42…) sin preguntar. Las decisiones que tocan una interfaz
  compartida (IR, JSON v2 de `schemac ir`, API C-ABI, API del servidor, `docs/00-alcance.md`) necesitan el
  visto bueno de las dos personas: proponerlas, no aplicarlas.
- Al cerrar una tarea, actualizar su línea en `tasks/CHECKLIST.md` (marca, fecha, rondas, dónde quedaron las
  observaciones), el «Retomar aquí» del plan (solo la línea de su pista) y `tasks/todo.md`. No editar líneas del
  checklist que pertenecen a la otra persona.
