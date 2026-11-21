# Límites configurables (F6.2, D78)

Los límites de `docs/00-alcance.md` §4 ya los aplicaba el núcleo desde F4.6 (`runtime_rs::Limits`), pero los
binarios generados los traían fijos (`docs/fase5/pendientes.md` §2). F6.2 les da nombre, los lleva a
`schemac compile`, añade el del lexema numérico («dígitos») y el tiempo máximo de los binarios.

| Pieza | Archivo |
| --- | --- |
| Límites del núcleo | `crates/runtime-rs/src/limits.rs` (`max_lexeme` nuevo), `crates/runtime-c/sv_nucleo.h` (`sv_limits_t`) |
| Opciones del emisor | `crates/codegen/src/lib.rs` (`Opciones`, `NOMBRES_DE_LIMITES`, `MAX_STATE_WORDS`) |
| CLI | `schemac compile … --limit nombre=valor` |
| Pruebas | `crates/codegen/tests/limites.rs`, `crates/automaton/tests/limites.rs`, `crates/cli/tests/compile.rs` |

## 1. Los límites

| Nombre (`--limit`) | Campo | Defecto | Qué cuenta |
| --- | --- | --- | --- |
| `maxDepth` | `max_depth` | 256 | objetos y arreglos anidados; elementos XML con contenido |
| `maxString` | `max_string` | 1 MiB | bytes UTF-8 decodificados de una cadena, nombre, valor de atributo o texto |
| `maxLexeme` | `max_lexeme` | 1 024 | bytes del lexema de un número que lee una comprobación numérica (≤ 1 024: el búfer es fijo) |
| `maxKeys` | `max_keys` | 1 024 | claves no declaradas de un objeto (atributos sin bit de una etiqueta) guardadas para ver repetidas |
| `keyPool` | `key_pool` | 4 096 | esas claves entre todos los objetos abiertos |
| `maxUnique` | `max_unique` | 4 096 | hashes de elementos de los `uniqueItems` abiertos |
| `maxXmlRefs` | `max_xml_refs` | 100 000 | referencias de entidad o de carácter de un documento |
| `maxPayload` | `max_payload` | sin límite | bytes del payload |
| `maxTimeMs` | — (solo los binarios) | sin límite | milisegundos de reloj desde que empieza `main` |

Pasar cualquiera es un error `limit` de peso ∞ en el evento que lo pasa (código de salida 2), nunca un pánico ni una
escritura fuera de la memoria. `maxLexeme` mayor que 1 024 cuenta como 1 024: el lexema se guarda en un búfer fijo
del núcleo, y un número más largo sin comprobación numérica no se guarda (`[1e99999…]` con `true` pasa).

**Tiempo.** El alcance dice «sin límite en la librería; 10 s en la aplicación web». El núcleo es `no_std` y no
tiene reloj, así que el tiempo lo vigila quien lo alimenta: el `main` emitido lo mira **después de leer cada
bloque** de 64 KiB y antes de entregarlo (el fin de la entrada no lo mira) y, si se agotó, llama a `abort_limit` (Rust: `JsonValidator::abort_limit`; C: `sv_abort_limit`), que da
`limit` donde se quedó la entrada. Como la validación es O(n), entre dos miradas pasa a lo sumo lo que cuesta un
bloque. Un `read` bloqueado (stdin que no llega) no lo ve; para eso está el tope del sandbox (`docs/fase7/sandbox.md`).

## 2. Cómo llegan al binario

```text
schemac compile pedido.xsd --lang c --out salida/ --limit maxDepth=32 --limit maxTimeMs=10000
```

- **Rust:** `validador.rs` declara `pub const LIMITES: Limits` con los valores, y `STATE_WORDS` y `SV_MAX_DEPTH`
  salen de ellos. `main.rs` lleva `const TIEMPO_MS: Option<u64>`.
- **C:** `validador.c` define `sv_limits_emitted(sv_limits_t*)`, que es lo que usa `sv_init(s, NULL)`;
  `SV_STATE_WORDS` y `SV_MAX_DEPTH` salen de los mismos límites. `main.c` lleva `#define TIEMPO_MS` (−1 sin tiempo)
  y usa `timespec_get` (C11).
- **Memoria:** `state_words` crece con `maxDepth` × marco, `keyPool` y `maxUnique`. Con límites que pidan más de
  `MAX_STATE_WORDS` = 16 Mi palabras (128 MiB), `emit_con` devuelve el error y `schemac compile` da 1 con el motivo
  sin escribir nada, igual que con `maxDepth=0`. Bajar los límites reduce la memoria del binario.
- Un nombre desconocido o repetido, sin `=`, o un valor que no son solo cifras (`+5`, `-1`, vacío) dan 2
  (argumentos).
- Sin `--limit`, lo emitido es lo de antes con los valores por defecto (`emit` = `emit_con` con
  `Opciones::default()`).

## 3. El campo `limits` del corpus (propuesta para las dos personas)

`tests/README.md` deja el formato de `limits` «sin fijar», y hoy ningún caso lo usa (la línea base y
`POST /api/tests/run` de la pista B los tratarían como no disponibles o fallidos). Se propone que sea un objeto con
**los mismos nombres** de la tabla de §1, solo con los que cambian:

```json
"limits": { "maxDepth": 3, "maxLexeme": 5 }
```

Es una interfaz compartida (formato del corpus, D42): queda pendiente del visto bueno de Bayron
(`docs/fase6/pendientes.md` §30), y hasta entonces F6.2 no añade casos con `limits` al corpus. Las mismas
comprobaciones están en `crates/codegen/tests/limites.rs`.

## 4. Pruebas

- **Binarios con límites propios** (`tests/limites.rs`), JSON y XML, compilados de verdad en Rust y en C
  (`-Werror`): cada payload da la línea JSON y el código del intérprete con esos mismos límites, y se comprueba que
  cada uno da lo que se busca (en su valor pasa, en su valor + 1 da `limit`): profundidad, cadena, lexema, claves,
  reserva de claves, `uniqueItems`, tamaño del payload, referencias XML y lexema de un texto XML.
- **Tiempo con `maxTimeMs=0`:** `limit` en el offset 0 y código 2, en los dos lenguajes; con la entrada vacía, el
  reporte de siempre (el fin no mira el tiempo); el `main.c` con tiempo compila con `-Wconversion -pedantic -Werror`.
  No se mide tiempo de pared (D1): con 0 el resultado es determinista.
- **Núcleo** (`automaton/tests/limites.rs`): `max_lexeme` en JSON, XML y el hash de `uniqueItems`, el tope de
  1 024, y `abort_limit` (en la posición actual; no cambia un reporte ya detenido).
- **CLI** (`cli/tests/compile.rs`): `--limit` llega a lo emitido en Rust y en C; errores de argumentos (2) y de
  memoria (1, sin escribir nada).
