# Pendientes de la Fase 7 (aplicación web)

Observaciones que dejó abiertas la tarea F7.1 (backend axum: `POST /api/schemas` y
`GET /api/health`; teoría y contrato en [servidor.md](servidor.md)) y que se resuelven
en otra tarea. Los identificadores CR-…, SEC-… y TR1-… son los de las revisiones de la
ronda 1 de F7.1. Las decisiones de diseño viven en el «Registro de decisiones» del plan;
aquí solo se remite a ellas.

## Vigilar más adelante

### 1. Tope de memoria por proceso (F7.1-P1; SEC-M2)

- **Estado:** cada compilación corre en un hilo con 8 MiB de pila. El servidor acota la
  memoria con el tope de subida (1 MiB), el semáforo de compilaciones (2), el tope de la
  IR (4 MiB) y, desde la ronda 2, el presupuesto de cuerpos en vuelo (32 MiB) y el plazo
  de lectura (10 s, SEC-H1). Desde la ronda 3, cada cuerpo se lee a un búfer contiguo del
  tamaño reservado (SEC2-H1), así que los 32 MiB acotan de verdad lo que retienen los
  cuerpos, sin copia final ni amplificación por frames. El pico es ≈ 350 MiB de
  compilaciones más ≤ 32 MiB de cuerpos, los búferes de lectura de hyper y el proceso base:
  ≈ 380 MiB en total ([servidor.md](servidor.md) §3). La cifra de ≈ 350 MiB de la ronda 0
  no contaba los cuerpos en vuelo, que entonces no tenían tope. Un **fallo de asignación** o
  un **desbordamiento de pila** abortan el proceso entero, y ni `join` ni `catch_unwind` los
  capturan (`docs/fase3/pendientes.md` §2, requisitos 3 y 4): se caen todas las peticiones
  en curso. Es el riesgo residual que declara U4. Con las entradas de ≤ 1 MiB del auditor
  no se pudo provocar. En las rondas 1 y 2 sí: primero con el cuerpo sin presupuesto
  (SEC-H1) y después con `chunked` de 1 byte (SEC2-H1). Desde la ronda 3, no (TR2-O1).
- **Arreglo propuesto:**
  - F7.7, **obligatorio** (SEC-M2): `mem_limit` (p. ej. 1 GiB) y `restart: unless-stopped`
    en Docker Compose, para que el contenedor se reinicie en lugar de dejar la máquina sin
    memoria;
  - F7.3: compilar en un **proceso hijo** con `RLIMIT_AS` ≈ 165 × subida + el proceso
    base. Sin `unsafe` exige lanzar el hijo con `sh -c 'ulimit -v N; exec …'` (solo
    Linux) y pasarle el esquema por la entrada estándar o por un archivo temporal. Con
    eso, un aborto solo tumba esa compilación (el servidor la vería como un 500).
- **Dónde:** `server/src/api.rs:300` (`spawn` del hilo de compilación).
- **Tarea:** F7.7 (`mem_limit`; **hecho en D71**: 600 MiB y `restart: unless-stopped`, `despliegue.md` §3). **F7.3 (D68):** el proceso hijo se resolvió con el sandbox Docker para el código generado. Para la compilación de esquemas se descarta, porque arrancar un contenedor por petición no compensa; queda el `mem_limit` de F7.7 ([sandbox.md](sandbox.md) §5).

### 2. `id` persistente (F7.1-P2)

- **Estado:** `id` es el SHA-256 hexadecimal del cuerpo (U5). Es determinista y no
  guarda nada: hoy no hay ningún `GET /api/schemas/{id}/…` que lo use.
- **Arreglo propuesto:** F7.2 guarda el esquema con `_id` = `id` (un `upsert`, porque el
  mismo esquema da el mismo `id`), y las rutas de código, autómata y derivación lo usan.
  Si entonces el recurso tiene `GET`, valorar si `POST` pasa a 201 (U1 eligió 200
  porque no hay recurso que leer). Eso es un cambio de la interfaz compartida (D42).
- **Dónde:** `server/src/api.rs:309` (`let id = hex(…)`).
- **Tarea:** F7.2.

### 3. IR en streaming (F7.1-P3, opcional)

- **Estado:** la IR se escribe entera en un `String` con tope (4 MiB) dentro del hilo, y
  el cuerpo del 200 se arma con otra copia en el manejador: hasta ≈ 2 × 4 MiB por
  respuesta. Pasado el tope sale `ir: null`, `irOmitted: true` e `irBytes: null`.
  Además, el permiso del semáforo se suelta al terminar la compilación, antes de que se
  escriba la respuesta, así que esas copias de la IR no cuentan en la concurrencia:
  como mucho 8 MiB por petición en vuelo. Cuántas respuestas hay en vuelo no tiene tope
  (CR-I3, §8).
- **Arreglo propuesto:** la opción de `docs/fase3/pendientes.md` §1: escribir
  `write_json` directamente en el cuerpo con un adaptador `fmt::Write` → `io::Write` (o
  un `Body` en streaming), decidiendo qué se hace si hay un pánico a mitad de la
  respuesta (ya empezó con 200). Solo si la web necesita IR de más de 4 MiB.
- **Dónde:** `server/src/compilar.rs:233` (`ir::write_json` con `Tope`),
  `server/src/api.rs:388` (`valido`).
- **Tarea:** F7.4 o posterior, si hace falta.

### 4. CORS (F7.1-P4)

- **Estado:** el servidor no envía cabeceras CORS (B9). En producción Caddy sirve la web
  y la API en el mismo origen (F7.7), así que no hacen falta.
- **Arreglo propuesto:** en desarrollo, que el `vite dev` de F7.4 use su proxy hacia
  `127.0.0.1:8080`. Solo si eso no basta, `tower-http` con `CorsLayer` limitado al origen
  de Vite y con una feature o variable de entorno que lo deje apagado en producción.
- **Dónde:** `server/src/api.rs:68` (`Router`).
- **Tarea:** F7.4, si hace falta.

### 5. Lo que hereda la web de la Fase 3 (§9, §10 y §15 de `docs/fase3/pendientes.md`) — RESUELTO en F7.4 salvo §9

- **Resuelto en F7.4 (19 nov, D56):** la web muestra todo como texto (React, sin
  `innerHTML` ni `dangerouslySetInnerHTML`), lo prueba
  `web/src/Resultado.test.tsx` («un nombre de propiedad hostil en la IR se muestra como
  texto») con `<img src=x onerror="alert(1)">` en la IR, el `summary`, el mensaje del
  autómata y los errores del 422; la IR se muestra **cruda** (`irCruda`, `web/src/api.ts`),
  sin `JSON.parse`, así que los contadores `u64::MAX` no se redondean (§10; prueba «la IR
  se muestra cruda»). Los contadores del resumen del autómata sí pasan por `JSON.parse`,
  pero son tamaños de tablas (≤ 2²⁰). Sigue abierto §9 (aviso de `allOf` +
  `additionalProperties: false`): sin canal de avisos en el contrato.

- **Estado:** la IR de la respuesta es el JSON v2 de `schemac ir`, tal cual:
  - §9: no hay aviso para «`allOf` + `additionalProperties: false`». El contrato de U1
    no tiene canal de avisos; si se añade, sería un campo nuevo (`warnings`) con
    `apiVersion` sin cambiar, porque solo se añade;
  - §10: los contadores pueden valer hasta `u64::MAX`, que `JSON.parse` redondea;
  - §15: claves, nombres y literales van enteros y sin escapar (U+202E, U+2028, `<`,
    `</script>`, marcado HTML). El auditor lo confirmó en la ronda 1 (INFO).
- **Arreglo propuesto:** F7.4 muestra la IR con `textContent` (nunca `innerHTML`), lee los
  contadores con `BigInt` o como «sin límite» si valen `u64::MAX`, y carga la respuesta con
  `fetch` + `JSON.parse`. Criterio para F7.4: una prueba con un esquema que tenga
  `<img src=x onerror=alert(1)>` como nombre de propiedad, para ver que se muestra como
  texto. El aviso de §9, si se quiere, es local a un nodo de la IR, O(1).
- **Dónde:** `server/src/api.rs:388` (`valido`, que incrusta la IR);
  `docs/fase3/pendientes.md` §9, §10, §15.
- **Tarea:** F7.4.

### 6. Unificar la tubería en un crate `pipeline` (F7.1-P5, U2; CR-I1, CR-S1)

- **Estado:** `server/src/compilar.rs` repite la tubería de `crates/cli/src/main.rs`
  (`formato`, `analizar_json`/`analizar_xsd`, `bajar`, `construir`), porque U2 dejó
  `crates/cli` sin tocar. Qué vigila la paridad, exactamente (CR-I1):
  - **la IR**: las doradas compartidas de `tests/ir` y `tests/ir/xsd`, que comprueban a la
    vez `crates/cli` y el servidor (`json_valido_da_200_con_la_ir_dorada`,
    `xsd_valido_da_200_con_automata_pendiente`). El resumen del autómata, la dorada de
    `tests/automata`;
  - **`formato` y el `summary`**: desde la ronda 2, `misma_tuberia_que_schemac`
    (`server/src/compilar.rs`, CR-S1) lee el fuente de `crates/cli` con `include_str!` y
    exige que el cuerpo de `formato` sea el mismo línea a línea, sin la sangría, y que las
    tres cadenas de los resúmenes coincidan. Si alguien cambia una copia y no la otra,
    falla. La tabla de `formato_por_el_primer_byte_significativo` y el literal del
    `summary` de `json_valido_da_200_con_la_ir_dorada` son copias, no comparaciones con
    `schemac`;
  - **no se vigila** el resto de la tubería (el orden de las etapas, qué errores se
    devuelven). Es la misma porque se copió, y las pruebas de 422 lo cubren solo en parte.
- **Arreglo propuesto:** un crate `pipeline` con `formato`, la compilación hasta la
  `VerifiedIr` y los resúmenes, que usen `schemac` y el servidor. `crates/cli` cambia (lo
  aprueba quien lo lleve, D42) y las doradas no. Con él, `misma_tuberia_que_schemac`
  sobra.
- **Dónde:** `server/src/compilar.rs:48` (`formato`), `:203` (`compilar_o_fallar`);
  `crates/cli/src/main.rs:133` (`formato`), `:422` (`bajar`), `:487` (`construir`).
- **Tarea:** la próxima que toque las dos tuberías (pista A o B), o tras la entrega.

### 7. El servidor es el segundo llamador de `semantic::lower` (§13 de la Fase 3)

- **Estado:** `semantic::lower` acepta cualquier `&SymbolTable`, también una que no pasó
  `analyze` (`docs/fase3/pendientes.md` §13). El servidor la obtiene siempre de
  `semantic::analyze`, y lo dice un comentario en el código. Con eso basta hoy, pero lo
  garantiza la disciplina, no el tipo.
- **Arreglo propuesto:** el de §13: que `analyze` devuelva un `AnalyzedTable` que solo se
  obtiene de ella y que `lower` lo exija. Cambia la API de `semantic` (pista A).
- **Dónde:** `server/src/compilar.rs:220`; `crates/semantic/src/lower.rs:55`.
- **Tarea:** la próxima que toque `semantic::lower`, o el crate `pipeline` (§6).

### 8. Conexiones lentas, memoria de entrada y salida, y lo que va a Caddy (F7.1-P6; SEC-H1, CR-I3, SEC-M1)

- **SEC-H1 (High), resuelto en el servidor en la ronda 2.** Antes, el cuerpo se leía
  entero antes del semáforo, sin plazo ni tope de conexiones: N conexiones con 1 MiB a
  medias retenían N MiB (el auditor llevó a `OOMKilled` un contenedor de 300 MiB con 400
  conexiones). Ahora:
  - un presupuesto global de cuerpos en vuelo (`MAX_INFLIGHT_BODY_BYTES`, 32 MiB, bloques
    de 1 KiB) se reserva **antes** de leer. Sin sitio, 503 `busy` sin leer el cuerpo;
  - un plazo de lectura (`BODY_READ_TIMEOUT_MS`, 10 s) termina con un 408 `body-timeout`.

  Sonda de la ronda 2 ([servidor.md](servidor.md) §3): con 1 000 conexiones a medias en
  300 MiB, el pico es ≈ 38 MB en release (≈ 41 MB en debug), con 967 × 503 y 33 × 408, y el servidor sigue vivo. Pruebas:
  `presupuesto_de_cuerpos_en_vuelo_da_503`, `la_reserva_sigue_al_content_length` y
  `cuerpo_que_no_llega_da_408`.
- **SEC2-H1 (High), resuelto en la ronda 3.** La sonda de la ronda 2 usaba pocos frames
  grandes y no cubría un cuerpo `chunked` en trozos de 1 byte: `Bytes::from_request` hace
  `collect()`, que guarda un `Bytes` por frame, así que una subida de 1 MiB llegaba a
  ≈ 40 MiB y 8 conexiones tumbaban un contenedor de 300 MiB. Ahora el cuerpo se lee frame
  a frame y se copia a un `Vec` con capacidad = lo reservado (`server/src/api.rs:192`),
  dentro del mismo plazo: lo retenido nunca pasa de lo reservado. Prueba (SEC2-L1):
  `esquema_en_frames_de_un_byte_da_200` (`server/tests/api.rs:593`).
- **Sigue abierto:**
  - **CR2-I1, el presupuesto como blanco contra la disponibilidad:** unas 32 conexiones
    `chunked` (o ≈ 33 con `Content-Length` ≈ 1 MB) que mandan un byte y se callan agotan
    los 32 MiB, y durante el plazo de lectura (10 s) todas las subidas reciben 503 `busy`,
    también un `{}`; el ataque se puede repetir cada 10 s. El servidor ya no cae: es el
    precio buscado de cambiar un OOM por un 503. Arreglo: `request_buffers 1MB` en el
    `reverse_proxy` de Caddy (lee el cuerpo entero antes de reenviarlo, así el servidor
    solo ve subidas rápidas), junto con `read_body` y el límite de conexiones por IP;
  - **SEC2-L2, conexiones keep-alive ociosas sin plazo:** tras una respuesta, hyper no
    tiene plazo para la siguiente petición (mismo límite que el plazo de cabeceras de
    abajo); Caddy (`idle_timeout`) lo resuelve en F7.7;
  - **plazo de cabeceras y tope de conexiones.** `axum::serve` (axum 0.8.9,
    `serve/mod.rs:391`) arma su `hyper_util::server::conn::auto::Builder` sin temporizador
    ni opciones, así que `header_read_timeout` no se puede activar sin reescribir el bucle
    de `accept` con `hyper-util` a mano. Una conexión que no manda cabeceras se queda
    abierta indefinidamente (≥ 50 s medidos por el auditor), aunque ocupa ≈ 15 KB y ningún
    permiso: 2 048 de esas son ≈ 30 MB. Lo mismo con un `Content-Length` dentro del tope que
    no termina de llegar, solo que ese sí se corta a los 10 s con el 408;
  - **CR-I3, respuestas en vuelo sin tope:** cada 200 retiene hasta ≈ 8 MiB (dos copias
    de la IR) hasta que el cliente lo lee, en conexiones ilimitadas. Arreglo: `write_timeout`
    en Caddy, o un `tower::limit::ConcurrencyLimit` sobre el `Router`;
  - **SEC-M1, sin limitador por cliente:** pocos clientes con esquemas de 1 MiB que
    compilan en ≈ 0,5 s ocupan los 2 permisos y el resto recibe 503 (36 de 40 peticiones
    legítimas en la prueba del auditor). Arreglo: limitador por IP en Caddy. Opcional en el
    servidor: un permiso reservado para esquemas pequeños (< 64 KiB) o una cola corta con
    espera acotada (1-2 s).
- **Arreglo propuesto (F7.7, Caddy y Compose):** `read_header`, `read_body`,
  `idle_timeout` y `request_buffers 1MB` (`reverse_proxy`) en Caddy;
  límite de conexiones y limitador por IP; `request_body max_size 1MB`; `write_timeout`;
  `mem_limit` y `restart: unless-stopped` (§1); `logging.options.max-size` (§11);
  `cargo audit` con red (el auditor no pudo contrastar RUSTSEC). Si el servidor se expone
  sin Caddy, servir con `hyper-util` y `header_read_timeout`, y con un `Listener` propio
  que limite las conexiones.
- **Dónde:** `server/src/main.rs:44` (`axum::serve`); `server/src/api.rs:252` (reserva
  del presupuesto), `:258` (plazo de lectura), `:285` (permiso de compilación).
- **Tarea:** F7.7.

### 9. Visto bueno de la interfaz compartida (F7.1-P7, D42)

- **Estado:** el contrato de `POST /api/schemas` (U1), el `id` SHA-256 (U5) y el estado
  del autómata (U6) son interfaz compartida: los consumen F7.4 y F7.5 y los aprueban las
  dos personas (D42). F7.1 los implementa como se aprobaron el 18 nov, con estas
  precisiones del Builder:
  - B10: `irBytes: null` si la IR se omite;
  - B11: `no-thread` como código del 503 cuando no se puede crear el hilo;
    `method-not-allowed`, `not-found` y `body` para el resto de errores;
  - B12: en el 200, la IR va al final del cuerpo;
  - ronda 1: B18 (413 por `Content-Length` sin leer el cuerpo), B19 (máximos de las
    variables de entorno: subida ≤ 16 MiB, tiempo ≤ 10 min, concurrencia ≤ 64) y el orden
    alfabético de las claves de los errores;
  - ronda 2: B22 (408 `body-timeout` y variable `BODY_READ_TIMEOUT_MS`), B23 (503 `busy`
    también cuando se agota el presupuesto de cuerpos, y variable
    `MAX_INFLIGHT_BODY_BYTES`), B24 (`X-Content-Type-Options: nosniff` en todas las
    respuestas) y que los únicos 400 son de lectura de la petición (CR-I2);
  - ronda 3: B27 (el cuerpo se lee frame a frame en un búfer contiguo de lo reservado,
    SEC2-H1; dev-dependency y dependencia `http-body-util` 0.1.5, ya en el árbol de axum).
    No cambia el contrato.

  U3 propone además una fila en `00-alcance.md` §4 («subida máxima del servidor: 1 MiB»),
  que hoy solo está en [servidor.md](servidor.md) §3. CR-S3: la feature `json` de axum no
  se usa (todo va con `serde_json`) y trae `serde_path_to_error`. La pide el criterio de
  F7.1, así que se queda; se puede quitar al revisar el contrato con Leonardo.
- **Arreglo propuesto:** que Leonardo revise [servidor.md](servidor.md) §2 y §3 y, con su
  visto bueno, añadir la fila al alcance, decidir lo de la feature `json` y registrar la
  decisión.
- **Dónde:** [servidor.md](servidor.md) §2-§3; `server/src/api.rs:388` (`valido`);
  `server/Cargo.toml` (`axum`).
- **Tarea:** cierre de F7.1 (antes de F7.4).

### 10. Los 400 y 431 de hyper no son JSON (F7.1-P8; TR1-O1)

- **Estado:** un `Content-Length` que no es un número, o dos `Content-Length` distintos,
  los rechaza hyper al leer las cabeceras, antes de que la petición llegue al `Router`: la
  respuesta es un 400 vacío con `connection: close`, y no lleva el JSON de U1 ni `nosniff`.
  Un valor enorme (`u64::MAX`) da un 431 igual de vacío (TR1-O1), y lo mismo unas 300
  cabeceras (auditor). Comprobado contra el binario en release (rondas 1 y 2). No se
  cuelga ni llega a compilar. Si una petición así llegara al manejador, por ejemplo en las
  pruebas con `oneshot`, sin hyper delante, el valor raro no cuenta y decide el cuerpo
  leído con tope (`content_length_raro_o_que_miente`).
- **Arreglo propuesto:** ninguno en el servidor; axum no expone un gancho para los errores
  de parseo de hyper. En F7.7, Caddy delante normaliza o rechaza esas peticiones con su
  propia respuesta. Si se quisiera JSON también ahí, habría que servir con `hyper-util` a
  mano en lugar de `axum::serve`.
- **Dónde:** `server/src/main.rs:44` (`axum::serve`); `server/src/api.rs:165`
  (`anunciado`).
- **Tarea:** F7.7 (Caddy), o ninguna.

### 11. Registro sin tope global (SEC-L3)

- **Estado:** el registro es stderr (`docs/fase3/pendientes.md` §14). Cada 500 escribe
  como mucho 10 líneas acotadas, pero no hay tope de tasa: un cliente que provoque
  errores internos sin parar llena el disco del registro de Docker.
- **Arreglo propuesto:** `logging.options.max-size` (y `max-file`) en Docker Compose.
- **Dónde:** `server/src/api.rs:118` (`registrar`).
- **Tarea:** F7.7.

### 12. El hilo que supera el plazo sigue corriendo (SEC-L1)

- **Estado:** tras un 504 el hilo no se puede matar: sigue hasta terminar y conserva su
  permiso de compilación ([servidor.md](servidor.md) §4). El auditor midió ≤ 0,6 s el peor
  esquema de 1 MiB, así que en la práctica no llega al plazo de 10 s, y los hilos vivos
  quedan acotados por la concurrencia (2, más uno que esté terminando).
- **Arreglo propuesto:** el proceso hijo de F7.3 (§1), al que sí se le puede mandar una
  señal.
- **Dónde:** `server/src/api.rs:300`.
- **Tarea:** F7.3. **Resuelto para el código generado (F7.3, D68):** compilar y ejecutar corre en un contenedor aparte, con `timeout` dentro y `docker kill` al pasar el plazo de pared ([sandbox.md](sandbox.md) §2). La compilación de *esquemas* sigue en un hilo (§1).

### 13. Binario compilado en caché por esquema (F7.3-P1)

- **Estado:** cada `ejecutar` del sandbox compila desde cero: alrededor de 1 s en C y varios segundos en Rust
  con `-O`, más el arranque del contenedor.
- **Arreglo propuesto:** en F7.5, compilar una vez por `id` de esquema y guardar el binario. Hay dos maneras:
  - un segundo modo de `sandbox-run`, que solo ejecute un binario recibido en el tar;
  - un contenedor de larga vida por esquema.

  Las restricciones de [sandbox.md](sandbox.md) §2 se mantienen en los dos casos.
- **Dónde:** `server/src/sandbox.rs` (`ejecutar`), `sandbox/sandbox-run.sh`.
- **Tarea:** F7.5.

### 14. El código generado tiene que compilar sin dependencias (F7.3-P2)

- **Estado:** el sandbox compila sin red un `main.rs` suelto o un conjunto de `.c`/`.h`, sin Cargo.
- **Arreglo propuesto:**
  - si F5.1 emite un crate que depende de `runtime-rs` y `lexer`, copiar sus fuentes en la imagen (etapa
    `COPY` con su propio `Cargo.toml` de workspace) y usar `cargo build --offline`;
  - o que el emisor incluya el núcleo en el mismo archivo.
- **Dónde:** `sandbox/Dockerfile`, `sandbox/sandbox-run.sh`.
- **Tarea:** F5.1 (decide la forma, pista A) y F7.5.

### 15. `/api/compare` sin tope de CPU propio para la línea base (F7.6-P2)

- **Estado:** `POST /api/compare` (F7.6) valida con `jsonschema` dentro del hilo de la petición. Un esquema
  hostil (`docs/fase6/pendientes.md` §14) se pasa del tope de tiempo: el cliente recibe 504, pero el hilo sigue y
  conserva su permiso (como §12). Lo acotan el tope de subida (1 MiB) y la concurrencia (2).
- **Arreglo propuesto:** el de §1, un proceso aparte con límite de CPU (o el sandbox de F7.3), o apagar la línea
  base para esquemas grandes.
- **Dónde:** `server/src/vistas.rs` (`comparar`).
- **Tarea:** F7.7, o se acepta para la entrega académica.
