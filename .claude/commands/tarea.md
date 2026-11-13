---
description: Ciclo automático desarrollador → auditor hasta que la tarea quede APROBADA (máx. 3 rondas)
argument-hint: <tarea del plan, p. ej. "Semana 2 · Fase 2 · parser LL(1) del metalenguaje">
---

Coordina a los subagentes `desarrollador` y `auditor` para esta tarea, sin pedirme intervención entre rondas:

**Tarea:** $ARGUMENTS

## Ciclo

1. **Desarrollo.** Lanza el agente `desarrollador` en primer plano (`run_in_background: false`) con la tarea completa. Indícale que no puede preguntarme directamente: si encuentra una ambigüedad que bloquea la tarea, debe terminar y ponerla en "Pendientes y riesgos" con la etiqueta `BLOQUEO:`.
2. Si el reporte trae un `BLOQUEO:`, detén el ciclo y pregúntame a mí.
3. **Auditoría.** Lanza un agente `auditor` nuevo en primer plano. Pásale la tarea y el reporte completo del desarrollador, sin resumirlo.
4. **Decisión según el veredicto:**
   - `APROBADO`: termina.
   - `APROBADO CON OBSERVACIONES`: si alguna observación es una corrección concreta y barata, haz una ronda más; si no, termina.
   - `RECHAZADO`: continúa al desarrollador que ya existe con SendMessage (para que conserve el contexto) y pásale los hallazgos completos del auditor, tal cual. Pídele que corrija solo eso y entregue un reporte nuevo. Vuelve al paso 3 con un auditor nuevo.
5. **Límite:** máximo 3 rondas de auditoría. Si en la tercera sigue `RECHAZADO`, o si el mismo hallazgo se repite dos rondas seguidas, detente y explícame dónde se atascan.

## Reglas

- No edites código tú mismo; solo coordinas. Las correcciones las hace el desarrollador.
- Nadie hace `git commit` ni `push`.
- No resumas ni suavices los hallazgos del auditor cuando se los pases al desarrollador.

## Al terminar, dame

- Veredicto final y número de rondas.
- Archivos cambiados (`git diff --stat`).
- Hallazgos que se corrigieron por ronda y observaciones que quedaron abiertas.
- Lo que debo revisar yo antes de hacer el commit.
