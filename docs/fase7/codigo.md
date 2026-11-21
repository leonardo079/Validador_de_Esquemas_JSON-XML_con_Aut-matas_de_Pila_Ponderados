# Código generado en la web: descarga, sandbox y streaming (F7.5, D81)

El plan pide tres cosas en la Fase 7:

- descargar el código generado (Rust o C);
- validar un payload con él;
- un WebSocket que valide por bloques y devuelva el progreso en vivo.

Además, la compilación y la ejecución del código generado tienen que ocurrir **solo dentro del sandbox** (F7.3).
F7.5 se apoya en los emisores de la pista A (F5.1, Rust; F5.2, C; D74-D76).

| Pieza | Dónde |
| --- | --- |
| ZIP sin crates (*stored* + CRC-32) | `server/src/zip.rs` |
| Código, ZIP y validación en el sandbox | `server/src/generado.rs` |
| Sesión WebSocket | `server/src/flujo.rs` |
| Rutas | `server/src/api.rs` (`codigo`, `validar`, `flujo`) |
| Web | pestaña «Código generado» (`web/src/Generado.tsx`, clientes en `web/src/vistas.ts`) |
| Pruebas | `server/tests/codigo.rs` (10, más 1 con Docker); `web/src/generado.test.ts` (4) |

## 1. `POST /api/code?lang=rust|c`

El cuerpo es el esquema crudo (D22). La respuesta es un `application/zip` con
`Content-Disposition: attachment; filename="validador-<json|xml>-<lang>.zip"`. Contiene:

- los archivos de `codegen::rust::emit` o `codegen::c::emit`: `main.rs` o `main.c`, las tablas y la copia del
  núcleo y del lexer;
- un `LEEME.txt` con la orden para compilar y ejecutar.

El ZIP es reproducible: fecha fija y mismo orden, así que el mismo esquema da los mismos bytes. Los errores son 400
si `lang` no es `rust` ni `c`, y 422 si el esquema no compila (como `/api/schemas`) o si M no se puede construir.
Ejemplos: el de `objeto-cerrado` en C ocupa 135 KB (9 archivos) y en Rust 213 KB. Los lee `unzip`.

## 2. `POST /api/validate`

El cuerpo es `{"schema": "…", "payload": "…", "lang": "c"|"rust"}`. El servidor:

1. emite el código;
2. lo compila y lo ejecuta en un contenedor nuevo del sandbox (F7.3: sin red ni root, solo lectura, con límites),
   con el payload por stdin;
3. lee la línea JSON del binario (`docs/fase5/codegen.md` §3);
4. la compara con el intérprete (M sobre `runtime-rs`) campo a campo: `valid`, `violations`, `errorKind` y `offset`.

```json
{"format":"json","lang":"c","status":"ok","exitCode":1,"sandboxMs":1830,"files":8,
 "generated":{"valid":false,"violations":1,"errorKind":"schema","offset":6,"pointer":"/a","rule":"#/properties/a",…},
 "interpreter":{…},"agree":true}
```

- `status` puede ser `ok`, `compile-timeout`, `run-timeout` o `killed`. Si el código no compila o Docker falla, la
  respuesta es 502 `sandbox`.
- **Sin sandbox** (el servidor arrancó sin `SANDBOX=docker`), la respuesta es 503 `no-sandbox`; el resto de la API
  sigue igual.
- El plazo es el del sandbox (compilar, ejecutar y holgura: 85 s), no los 10 s de una compilación de esquema
  (`api::procesar_con_plazo`).

**Verificado con Docker** (`validate_compila_y_ejecuta_en_el_sandbox`): en C y en Rust, con un payload válido, uno
inválido y uno malformado, el binario generado y el intérprete coinciden, y los códigos de salida son 0, 1 y 2. Las
seis ejecuciones tardan 12 s.

Pendiente F7.3-P2 (`pendientes.md` §14): resuelto, porque los dos emisores producen archivos planos sin
dependencias, que es lo que acepta el sandbox.

## 3. `WS /ws/validate`

Valida en streaming con el validador incremental de `runtime-rs`, el mismo núcleo que el código generado: memoria
fija, sin árbol y sin guardar el payload.

| Paso | Cliente → servidor | Servidor → cliente |
| --- | --- | --- |
| 1 | el esquema (texto) | `{"type":"ready","format","states","transitions","memoryBytes"}`, o `error` y cierra |
| 2 | un bloque binario (≤ 1 MiB) | `{"type":"progress","chunk","bytes","stopped"}` |
| 3 | `fin` (texto) | `{"type":"report","bytes","chunks","report":{…}}` y cierra |

- El protocolo va **por turnos**: el cliente manda el siguiente bloque cuando recibe el `progress` del anterior,
  así que nunca hay más de un bloque en cola.
- `stopped: true` indica que el validador ya encontró un error de peso ∞ (malformado o límite): lo que llegue
  después no cambia el veredicto.
- El validador corre en un hilo propio con pila de 8 MiB (D7). Hay como mucho 4 sesiones a la vez (503 `busy` sin
  cola), 64 MiB por sesión y 30 s de inactividad.

**Comprobado:**

- sin red (`flujo::conversar`, el mismo hilo que la sesión): bloques de 1, 3 y el payload entero dan el mismo
  reporte; un error léxico detiene el validador en su byte; un esquema malo da `error`; también XML;
- con el servidor real y un cliente Node, en directo y **a través de Caddy**: 16 001 bytes en 4 bloques de 4 KiB,
  4 progresos y un reporte con 285 violaciones, la primera en `/6/a`.

## 4. Web: pestaña «Código generado»

- **Descargar el código (ZIP)** en C11 o en Rust.
- **Compilar y validar en el sandbox:** una tabla con el código generado y el intérprete lado a lado (resultado,
  regla, código de salida, tiempo del sandbox y si coinciden).
- **Validación en streaming:** un tamaño de bloque configurable, una barra de progreso de lo enviado, el aviso cuando
  el validador se detiene, la memoria fija de M y el veredicto final.

En desarrollo, Vite reenvía también `/ws` (`vite.config.ts`). En el despliegue, Caddy pasa `/ws/*` al servidor
(`infra/Caddyfile`).

## 5. Despliegue y pendientes

- **F7.5-P1** (`pendientes.md` §16): en la EC2, `/api/validate` queda **apagado** (503) por defecto. Para
  encenderlo, el contenedor `server` necesitaría tres cosas:
  1. el cliente `docker` en su imagen;
  2. el socket `/var/run/docker.sock` montado, que equivale a ser root en la máquina (`sandbox.md` §5);
  3. `SANDBOX=docker`, y la imagen del sandbox (1,3 GB) en la `t3.micro`, donde además `rustc -O` tarda decenas de
     segundos.

  Para la entrega, la descarga, el WebSocket y el intérprete funcionan en la URL pública, y la validación con el
  código generado se enseña en local (`SANDBOX=docker cargo run -p server`) o con `make test-sandbox`.
- **F7.3-P1** (`pendientes.md` §13): sigue abierto. Cada `/api/validate` vuelve a compilar; guardar el binario por
  esquema queda para después de la entrega.
- **Visto bueno (D82, 21 nov):** Leonardo aprueba las tres rutas con estos cambios ([pendientes.md](pendientes.md)
  §25):
  - el mismo `lang` por defecto en `/api/code` y `/api/validate`, y 400 si se repite;
  - un campo `code` en los errores del WebSocket y cierres 1008, 1009 y 1013;
  - `sandbox` en `GET /api/health`, y `/api/validate` mira el sandbox antes de validar el cuerpo;
  - 500 `codegen` si el código generado no compila;
  - documentar cuándo `generated` y `agree` son `null`.
