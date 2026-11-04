# Pendientes de la Fase 1 (análisis léxico)

Lo cerrado está archivado en [historial.md](historial.md) (3 nov 2026).

Observaciones que dejaron abiertas las rondas de auditoría de `crates/lexer`
y que se anotan aquí para no perderlas. Las decisiones de diseño viven en el
«Registro de decisiones» del plan (D1–D31); aquí solo se remite a ellas.

## Vigilar más adelante

### 1. Un crate de diagnósticos para `short`

- **Estado:** `lexer::Short` (texto del usuario recortado y escapado para los mensajes de
  error del compilador) vive en `lexer` desde la tarea 4.2 porque es la única dependencia
  común de `schema-parser`, `semantic` e `ir` (CR-S-5 de la revisión de la 4.2). Cambia
  un poco el papel del crate: `lexer` se incrusta en el validador generado (T5.6), y el
  tope de 32 caracteres es política de mensajes del compilador, no del tokenizador.
  Lo dicen la cabecera de `crates/lexer/src/lib.rs` y [lexico.md](lexico.md) §5.
- **Cuándo actuar:** cuando haya más política de diagnósticos compartida (por ejemplo, los
  mensajes del validador de la Fase 4 o el formato de errores del servidor de la Fase 7):
  moverla a un crate `diagnostics` del que dependan las fases, y dejar `lexer` solo con el
  tokenizador.

### 3. `Pos::advance` no cuenta un `\r` suelto como fin de línea (5.2-CR2-S5)

- **Estado:** `lexer::Pos::advance` (`crates/lexer/src/lib.rs:58-67`) solo cambia de línea con
  `\n`. XML 1.0 §2.11 trata también un `\r` suelto (no seguido de `\n`) como fin de línea, así
  que en un documento con fines de línea solo CR (Mac clásico) todo queda en la línea 1 y las
  columnas crecen sin parar: la posición de los errores es correcta en offset, pero no en
  línea y columna. Afecta al lector de esquemas XSD y al validador de payloads XML; en JSON
  (RFC 8259) `\r` es solo un espacio, así que habría que decidir si el cambio se aplica a los dos
  formatos o solo a XML.
- **Cuándo actuar:** con la próxima tarea que toque el lexer. El arreglo no necesita memoria
  dinámica: recordar si el byte anterior fue `\r` (un `\r` cambia de línea; un `\n` justo
  detrás de un `\r` no cambia otra vez). Cambia el tamaño de `Pos` o del estado del lexer en un
  booleano, así que hay que revisar la tabla de tamaños de [lexico.md](lexico.md).

Las dos notas opcionales que dejó el hallazgo 1 (D9) ya estaban resueltas al revisarlas
en la tarea 4.2 (27 oct); se conservan abajo como historia.

## Resueltos

### 1. Restas sin saturar en el lexer XML — RESUELTO → [historial.md](historial.md) §1

### 2. `back()` aproximado en el límite saturado — RESUELTO → [historial.md](historial.md) §2
