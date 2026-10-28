# Pendientes de la Fase 1 (análisis léxico)

Observaciones que dejaron abiertas las rondas de auditoría de `crates/lexer`
y que se anotan aquí para no perderlas. Las decisiones de diseño viven en el
«Registro de decisiones» del plan (D1–D16); aquí solo se remite a ellas.

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

Las dos notas opcionales que dejó el hallazgo 1 (D9) ya estaban resueltas al revisarlas
en la tarea 4.2 (27 oct); se conservan abajo como historia.

## Resueltos

### 1. Restas sin saturar en el lexer XML — RESUELTO

- **Nota original (hallazgo 1, opcional):** `crates/lexer/src/xml.rs:679` y `:696`
  restaban `here.offset - 1` sin saturar; el invariante lo garantizaba (el byte retenido
  ya se consumió, así que `offset ≥ 1`).
- **Estado actual:** las líneas se movieron. Las dos restas están hoy en las acciones
  `A_CD_RB` y `A_CD_FLUSH2` (`crates/lexer/src/xml.rs:690` y `:712`) y usan
  `here.offset.saturating_sub(1)`; las posiciones de inicio usan `back(here, k)`
  (`xml.rs:796`), también saturante.

### 2. `back()` aproximado en el límite saturado — RESUELTO

- **Nota original (hallazgo 1, opcional):** `docs/fase1/lexico.md` no decía que la
  columna que da `back()` es aproximada cuando la columna está saturada en `u32::MAX`
  (D9).
- **Estado actual:** lo dicen `lexico.md` («Posiciones en entradas de varios GB»: la
  columna de un `]` o `?` retenido puede ser hasta 2 columnas menor que `u32::MAX`; el
  offset es exacto) y la documentación de `back` en `xml.rs`; lo fija la prueba
  `back_respeta_columna_saturada`.
