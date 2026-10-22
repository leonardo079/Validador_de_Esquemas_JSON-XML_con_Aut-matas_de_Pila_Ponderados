# Pendientes de la Fase 3 (análisis semántico del esquema)

Observaciones que dejaron abiertas las rondas de auditoría de `crates/semantic`
y que se resuelven en otra fase. Las decisiones de diseño viven en el
«Registro de decisiones» del plan; aquí solo se remite a ellas.

## Vigilar más adelante

### 1. Truncar la lista de errores mostrada

- **Estado:** la Fase 3 informa todos sus errores, sin tope, ordenados por posición
  ([ciclos.md](ciclos.md) §5, «Número de errores: sin tope»). Lo guardado es O(n), pero
  el texto que se muestra es varias veces la entrada. Medido con `schemac check` en
  release, con una ruta corta delante de cada línea (ver [ciclos.md](ciclos.md) §5):
  - escenario H (m ciclos de 9 aristas bajo una clave de 200 bytes), el peor medido:
    unos 1 015 bytes por error, **≈ 9 veces la entrada** en la variante de 2,2 MB del
    auditor y 7,2 veces en la de 2,8 MB (20,3 MB de mensajes);
  - escenario trivial (autoaristas bajo una clave larga): unos 350 bytes por error,
    ≈ 6 veces la entrada.
  Cada byte de ruta de archivo suma un byte por error.
- **Cuándo actuar:** en el servidor de la aplicación web (Fase 7; está en la tarea del
  backend de la semana 4 del plan). La Fase 3 sigue devolviéndolos todos, igual que
  `schemac`; quien presenta la lista la trunca. La tarea del backend debe tener un
  **criterio de aceptación explícito** que cubra las tres cosas juntas, porque sin ellas
  una subida pequeña produce una respuesta ~9 veces mayor:
  - límite de tamaño de subida (`docs/fase2/pendientes.md` §6), que con el factor de
    arriba acota también el tamaño de la respuesta;
  - truncar la lista a N errores, con «… y K más»;
  - tope de CPU (tiempo) por compilación.
  Opcional: una variante `check_cycles_limited(table, n)` que deje de construir errores
  tras los primeros n, para no pagar su costo cuando no se van a mostrar.

### 2. `debug_assert!` en código que llamará el servidor

- **Estado:** `path_in` (`crates/semantic/src/cycles.rs`) afirma con `debug_assert!` dos
  invariantes de la búsqueda del camino de un ciclo. En release no hacen nada (la
  salvaguarda devuelve un camino vacío y el error de la componente se informa igual);
  en debug, si un cambio futuro rompiera el invariante, harían pánico.
- **Cuándo actuar:** al integrar la Fase 3 en el servidor (Fase 7). El servidor se
  despliega compilado en release, o aísla cada compilación de esquema en su propio hilo
  (ya hace falta uno con pila explícita, D7) o con `std::panic::catch_unwind`, de modo
  que un pánico afecte solo a esa petición y nunca tumbe el proceso.
