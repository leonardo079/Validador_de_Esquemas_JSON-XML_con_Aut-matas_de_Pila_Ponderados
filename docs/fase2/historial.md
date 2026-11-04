# Historial de la Fase 2 (parser de esquemas)

Entradas cerradas (resueltas, decididas o sin efecto) que salieron de
[pendientes.md](pendientes.md) el 3 nov 2026, copiadas literalmente. Conservan su número
(§N), al que remiten el código y el resto de `docs/`. Lo abierto sigue en `pendientes.md`.

### 2. Claves repetidas dentro de `const` y `enum` — DECIDIDO (D2)

- **Decisión (D2 del plan):** las claves repetidas se rechazan en cualquier objeto del
  esquema, incluidos los valores literales de `const`/`enum`, con el error
  `duplicate-key` en la segunda aparición. Es lo que ya hacía el parser; no cambia código.
- **Justificación:** el PDF del curso lista «claves duplicadas» como caso **malformado**,
  y en los payloads el validador generado también las rechaza (`docs/00-alcance.md` §1).
  Que la RFC 8259 solo diga que los nombres *deberían* ser únicos no obliga a aceptarlos:
  el proyecto aplica la misma regla al esquema y al payload. Ver `gramatica-esquema.md`
  §4 y el caso `tests/schemas/parse-error/clave-repetida-en-const.json`.

### 4. Revisar el manifiesto de errores — HECHO

- **Dónde:** `tests/schemas/errores.json`.
- **Revisión:** se verificaron a mano las 28 entradas (todas), con un script propio que
  no usa el parser: decodifica cada archivo de `parse-error/`, quita el BOM si lo hay,
  cuenta líneas por `\n` y columnas en puntos de código, y marca con `^` la posición
  declarada para comprobar a ojo que cae en el token que provoca el error (la segunda
  clave repetida, el `{` número 257 de `profundidad.json` en la columna 7·256 + 1 = 1793,
  el fin de archivo en `vacio`, `fin-inesperado` y `cadena-sin-cerrar`, etc.). Ninguna
  estaba mal; el manifiesto no cambió.

### 7. Palabras clave fuera del subconjunto — RESUELTO (Fase 3, D6)

- **Antes:** el parser las acepta y las deja en el AST como `Kw::Unsupported`, y nada
  las rechazaba: un esquema con `if`/`then` pasaba sin error.
- **Resuelto:** el parser sigue conservándolas (así puede informar todas, no solo la
  primera), y la validación contra el meta-esquema de la Fase 3
  (`semantic::check_meta_schema`) rechaza cada una con el código `unsupported`, en la
  posición de su nombre, con su categoría y el motivo (D6). No hay modo que las ignore.
  Ver [../fase3/meta-esquema.md](../fase3/meta-esquema.md) §5.
- **Corpus:** `tests/schemas/parse-ok/fuera-del-subconjunto.json` sigue en `parse-ok`
  (prueba el parser) y la Fase 3 lo rechaza con tres errores fijados en
  `crates/semantic/tests/corpus.rs`.

### 13. Recorte de los textos de error garantizado por el tipo (tarea 5.2; 5.1-CR3-S1 y 5.1-CR3-S2) — RESUELTO

- **Resuelto en la 5.2:** `pub(crate) fn XsdError::new(kind, at)` en `error.rs` llama a
  `ErrorKind::clipped()` (ahora privada del módulo) y es el único constructor dentro del crate
  (lo usan `err`, `keep`, el error léxico del lector y el parser). `keep`, `UnclosedElement`,
  `UndeclaredPrefix`, `ReservedPiTarget`, `UnsupportedEncoding` y los errores del parser reciben
  `clip(&nombre)`, sin clonar el nombre entero. Prueba `errores_con_nombres_largos_se_recortan`
  (nombres y valores de 1 MiB).

- **5.1-CR3-S1** (`crates/xsd/src/reader.rs:538-554` frente a `crates/xsd/src/error.rs:20-25`):
  el recorte de los textos de un error a 33 caracteres no lo garantiza el tipo; solo lo
  aplican `err` y `keep`, privadas de `reader.rs`, y los módulos de la 5.2 en adelante
  construirán `XsdError { kind, at }` directamente. **Arreglo (tarea 5.2):** un
  `pub(crate) fn XsdError::new(kind, at)` en `error.rs` que llame a `ErrorKind::clipped()`,
  usado por `err`/`keep` y documentado como único constructor.
- **5.1-CR3-S2** (= SEC3-L1, observación del Tester 4; `reader.rs:688, 702, 707, 754, 774,
  784, 901`): `keep` y `UnclosedElement` reciben el nombre entero clonado
  (`self.tag_name.clone()`, `a.name.clone()`, `o.name.raw.clone()`) y lo recortan después: una
  copia transitoria O(|nombre|), hasta 16 MiB, aunque `keep` descarte el error. La
  documentación de `ErrorKind` sugiere que nunca se copia un nombre largo. **Arreglo (tarea
  5.2, junto con CR3-S1):** construir los errores con `clip(&nombre)`, o aclarar la
  documentación de `ErrorKind`.

### 15. Clave de la tabla de §2 de `lector-xsd.md` — RESUELTO (5.1-CR3-S3)

- **Antes:** la tabla de §2 de [lector-xsd.md](lector-xsd.md) decía «Conjunto *hash* de
  (URI, local)» para los atributos repetidos por nombre expandido.
- **Resuelto (cierre de la 5.1):** la clave es `(NsId, local)`, la identidad internada del
  URI, no su texto; la tabla ya lo dice.
