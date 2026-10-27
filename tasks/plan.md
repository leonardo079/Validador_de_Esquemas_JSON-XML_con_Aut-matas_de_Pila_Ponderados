# Plan de implementación: tarea 4 de la semana 2 — IR común y clasificación (a)/(b)/(c)

Fecha: 20 oct 2026. Fuente: «Retomar aquí» del plan general
(`../Plan de desarrollo — Validador JSON XML con DPDA ponderado.md`), D3 y la
Fase 3 de `docs/00-alcance.md`. Este archivo detalla solo la tarea 4; el
cronograma, las decisiones y los invariantes siguen en el plan general.

## Resumen

Hoy la Fase 3 termina en una `SymbolTable` verificada (meta-esquema, `$ref`
resueltos, sin ciclos no productivos). Falta lo que cierra la Fase 3 según su
criterio de terminado: *cada esquema del corpus sale como IR con cada nodo
marcado con su clase y estrategia*. La IR es además el contrato entre los
dos frontales (JSON Schema ahora, XSD después) y el constructor del DPDA de
la Fase 4, así que su forma se audita antes de construir nada encima (D3).

## Grafo de dependencias

```
4.0 observaciones de la tarea 3 (semantic)        — independiente, se cierra primero
4.1 diseño de la IR (crate ir + docs/fase3/ir.md)  — contrato; lo revisa el usuario
 ├── 4.2 bajada JSON Schema → IR + `schemac ir`     (semantic, cli)
 │    └── 4.3 clasificación (a)/(b)/(c) y estrategia sobre la IR (ir, cli, docs)
 └── (semana 2, después) parser XSD → AST XSD → IR — fuera de este plan
          (Fase 4) constructor del DPDA — consume la IR clasificada
```

## Decisiones de arquitectura propuestas (las confirma el auditor en 4.1)

- **Crate nuevo `crates/ir`**, sin dependencias salvo `lexer` (por `Span`).
  `semantic` (bajada JSON), el futuro frontal XSD y `automaton` dependen de
  él; ninguno depende de los otros frontales. La clasificación vive en `ir`
  porque opera sobre la IR: el frontal XSD la obtiene gratis. Se registra
  como **D13** en el plan general al aprobar 4.1.
- **IR como grafo en arena** (`Vec<Node>` + `NodeId`), no árbol: un `$ref`
  es una arista a un nodo ya existente, así la recursión productiva se
  conserva sin duplicar subesquemas y el tamaño de la IR es O(n).
- **Mínimos que la IR debe expresar (D3):** nodo, restricciones tipadas por
  palabra clave, clase y estrategia (derivables de la clase de cada regla,
  `RuleKind`; la tabla clase/estrategia vive aparte y la construye 4.3),
  orden de hijos (`xs:sequence`),
  atributo frente a elemento, `minOccurs`/`maxOccurs`, contenido simple.
  Números como lexema exacto; la ubicación de origen (`Span` y puntero o
  ruta) para los errores de las fases siguientes.
- **Los pendientes de `docs/00-alcance.md` §5** se resuelven en
  `docs/fase3/ir.md` dentro de 4.1: correspondencia `xs:sequence` ↔ objeto
  JSON, `additionalProperties` con `allOf` (semántica 2020-12: no ve las
  `properties` de las ramas) y la estrategia de `not`.
- **Sin recursión sobre el AST ni la IR** (D7): bajada y clasificación
  iterativas.

## Lista de tareas

Cada tarea pasa por el ciclo desarrollador → auditor (`/tarea`, máximo 3
rondas) y el usuario hace el commit antes de empezar la siguiente.

### Fase A: deuda y contrato
- [x] 4.0 Cerrar las 6 observaciones de la tarea 3 (S)
- [ ] 4.1 Diseño de la IR: tipos, invariantes y teoría (M)

### Punto de control A (usuario)
- [ ] `make lint`, `make test`, `make docs-check` en verde
- [ ] El usuario aprueba la forma de la IR y D13 antes de bajar nada a ella

### Fase B: JSON Schema de extremo a extremo hasta la IR clasificada
- [ ] 4.2 Bajada JSON Schema → IR y `schemac ir` (M)
- [ ] 4.3 Clasificación (a)/(b)/(c) y estrategia (M)

### Punto de control B (cierre de la Fase 3 para JSON)
- [ ] Todo el corpus sale como IR con clase y estrategia en cada nodo
- [ ] Plan general actualizado: casillas, D13, «Retomar aquí» apuntando al parser XSD

## Riesgos y mitigaciones

| Riesgo | Impacto | Mitigación |
| --- | --- | --- |
| La IR no alcanza para XSD y hay que rehacerla con la bajada JSON ya hecha | Alto | 4.1 exige construir a mano, en pruebas, la IR de un XSD con `sequence`, `choice`, atributos y `maxOccurs="unbounded"` antes de aprobar el diseño |
| La IR se queda corta para la Fase 4 (falta información para el DPDA) | Medio | 4.1 recorre la construcción composicional del DPDA de `docs/` y comprueba que cada transición sale de un dato de la IR |
| Explosión de tamaño por `$ref` | Medio | Grafo con aristas compartidas; prueba de tamaño O(n) con referencias en cadena |
| La clasificación diverge de `docs/00-alcance.md` | Medio | Una prueba recorre todas las variantes de restricción (`match` exhaustivo) y compara con una tabla única citada por la documentación |
| Semana 2 se alarga y la semana 3 (crítica) empieza tarde | Alto | 4.0 es pequeña; si 4.1 necesita una tercera ronda, se avisa antes de seguir |

## Preguntas abiertas

1. ¿Crate `ir` aparte (recomendado) o módulo dentro de `semantic`? El crate
   separado evita que el frontal XSD dependa del análisis de JSON Schema.
2. Formato de `schemac ir`: se propone JSON estable (lo usará la web en la
   Fase 7) y, si sobra tiempo, DOT.
3. Este `tasks/` queda sin seguimiento en git. ¿Se versiona, se añade a
   `.gitignore` o se mueve junto al plan general?
