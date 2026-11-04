# Historial de la Fase 1 (análisis léxico)

Entradas cerradas (resueltas, decididas o sin efecto) que salieron de
[pendientes.md](pendientes.md) el 3 nov 2026, copiadas literalmente. Conservan su número
(§N), al que remiten el código y el resto de `docs/`. Lo abierto sigue en `pendientes.md`.

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
