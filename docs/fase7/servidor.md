# Servidor de la aplicación web: `POST /api/schemas` (F7.1)

El crate `server` (axum 0.8, tokio 1) expone la compilación de esquemas de `schemac`
sobre HTTP. F7.1 trae dos rutas: `POST /api/schemas` y `GET /api/health`. El resto
de la tabla de la Fase 7 del plan (código, autómata, derivación, validación,
WebSocket, pruebas) llega en F7.2-F7.5. Lo que queda abierto está en
[pendientes.md](pendientes.md).

- `server/src/lib.rs`: `Config` (topes y `Config::from_env`) y la reexportación de `router`.
- `server/src/api.rs`: manejadores, hilo de compilación, semáforo, tope de tiempo y forma
  de las respuestas.
- `server/src/compilar.rs`: la tubería de `schemac check`/`ir`/`automaton` sobre los
  bytes de la petición: `formato` (D22), errores con tope y el `fmt::Write` con tope para
  la IR.
- `server/src/main.rs`: lee el entorno, escucha y apaga en orden.
- `server/tests/api.rs`: pruebas con `tower::ServiceExt::oneshot`, sin red ni puertos.

`crates/cli` no se toca (U2): el servidor llama a las mismas bibliotecas que `schemac`
y copia `formato` y los textos del resumen. Dos pruebas vigilan esas copias:
`misma_tuberia_que_schemac` lee el fuente de `crates/cli` al correr y compara el cuerpo de
`formato` y las cadenas de los resúmenes (CR-S1, CR2-S1; TR2-O2), y
`formato_por_el_primer_byte_significativo` repite la tabla de casos de `crates/cli`. La
IR la vigilan las doradas compartidas ([pendientes.md](pendientes.md) §6).

## 1. Qué hace una compilación

El cuerpo de la petición es el esquema, en crudo. No hace falta `Content-Type` y no se
usa: el formato sale del contenido (D22). Si el primer byte significativo (tras un BOM
UTF-8 y los espacios `\t\n\r `) es `<`, el esquema es XSD; si no, JSON Schema. Un cuerpo
vacío o que no es UTF-8 se trata como JSON y da su error léxico o del parser (422), nunca
un 400.

En un hilo propio corren, en este orden:

- JSON Schema: `schema_parser::parse` → `semantic::analyze` → `semantic::lower`. El
  servidor es el segundo llamador de `lower` y le pasa siempre la tabla que devuelve
  `analyze` (`docs/fase3/pendientes.md` §13);
- XSD: `xsd::parse` → `xsd::analyze` → `xsd::lower`;
- con la `VerifiedIr`: `ir::write_json` (JSON v2, D17) en un escritor con tope y
  `automaton::build` con `write_summary_json` (F4-U7).

Los errores del esquema son los de `schemac check`: el primero de la Fase 2 (los dos
parsers se detienen en él) o todos los de la primera etapa de la Fase 3 que falla. Solo
los 100 primeros se pasan a texto.

## 2. Contrato de la API (U1)

Todas las respuestas del `Router` son `application/json` y llevan
`X-Content-Type-Options: nosniff` (SEC-L2). Las de error llevan un campo `error` con un
código estable y un `message` en español. El servidor **nunca responde 400 por el
esquema**: un esquema malo, vacío o que no es UTF-8 es un 422. Los únicos 400 son de
lectura de la petición: el de hyper por un `Content-Length` malformado, y el `body` de
axum cuando la conexión se corta a mitad del cuerpo (CR-I2). Un esquema compilado
responde 200, no 201, porque todavía no hay un `GET` del recurso (llegará con la
persistencia de F7.2).

### `GET /api/health`

```json
{"status":"ok","version":"0.1.0"}
```

### `POST /api/schemas` → 200 (esquema válido)

```sh
curl --data-binary @tests/schemas/parse-ok/objeto-cerrado.json http://127.0.0.1:8080/api/schemas
```

```json
{"apiVersion":1,
 "id":"bab8fca9554edf0f37a0ae172ad1527ff1275db87617f7bdc622b574eae5fe47",
 "format":"json",
 "summary":"esquema válido (subconjunto de JSON Schema 2020-12); 3 subesquemas, 0 entradas de $defs, 0 referencias resueltas",
 "irBytes":1697,
 "irOmitted":false,
 "automaton":{"status":"ok","summary":{
  "automatonVersion": 1,
  "format": "json",
  "states": 24,
  "...": "...",
  "deferred": 0
}},
 "ir":{
  "irVersion": 2,
  "format": "json",
  "...": "..."
}
}
```

Los saltos de línea de arriba están para leer. En la respuesta real, la cabecera hasta
`automaton` va en una sola línea, sin espacios. El resumen del autómata y la IR se
incrustan **tal cual** salen de `write_summary_json` y de `write_json`, con sus saltos de
línea y su sangría. La IR conserva su salto de línea final; al resumen se le quita. Es
JSON válido de todos modos, y lo comprueban las pruebas parseando el cuerpo.

| Campo | Contenido |
| --- | --- |
| `apiVersion` | `1`. Cambia si cambia la forma de la respuesta. |
| `id` | SHA-256 en hexadecimal (minúsculas) de los bytes del cuerpo (U5). Determinista y sin estado: el mismo esquema da el mismo `id`, y F7.2 lo usará como `_id`. |
| `format` | `"json"` o `"xml"`, como el campo `format` de la IR. |
| `summary` | El texto de `schemac check` sin la ruta: «esquema válido (subconjunto de JSON Schema 2020-12); N subesquemas, …» o «esquema válido (subconjunto de XSD 1.0); N tipos complejos». |
| `ir` | El objeto que escribe `ir::write_json`, **byte a byte** (lo comprueban las pruebas contra las doradas de `tests/ir` y `tests/ir/xsd`). Va al final del cuerpo. `null` si pasa del tope (§3). |
| `irBytes` | Bytes de `ir`; `null` si se omitió. |
| `irOmitted` | `true` si la IR pasó del tope y no se incluye. |
| `automaton` | `{"status":"ok","summary":{…}}` con el resumen de `write_summary_json` tal cual; `{"status":"pending","message":"estrategia pendiente: … (F4.x) en …"}` si una regla aún no se compila (todo XSD hasta F4.1c, y `pattern` o los combinadores de JSON hasta su tarea, F4-U5); `{"status":"limit","message":"límite superado: …"}` si se pasa un límite del constructor (`00-alcance.md` §4). Pendiente y límite **no** son errores del esquema: la respuesta sigue siendo 200 (U6). |

Los textos del usuario (claves, nombres, literales) van en la IR enteros y sin escapar
más que con RFC 8259. La web los inserta como texto, nunca con `innerHTML`
(`docs/fase3/pendientes.md` §15). Los contadores pueden valer hasta `u64::MAX`, que
`JSON.parse` redondea (§10). Las dos cosas son de F7.4 ([pendientes.md](pendientes.md) §5).

### `POST /api/schemas` → 422 (errores del esquema)

```sh
curl --data-binary '{"type": ["", "x"], "minItems": -1}' http://127.0.0.1:8080/api/schemas
```

```json
{"error":"schema",
 "errors":[{"code":"meta-schema","column":11,"line":1,
            "message":"valor inválido para \"type\" según el meta-esquema: se esperaba un nombre de tipo (null, boolean, object, array, number, integer o string), se encontró \"\""},
           "…"],
 "format":"json","total":3,"truncated":false}
```

El 422 y los demás errores se serializan con `serde_json::json!`, que ordena las claves
alfabéticamente (sin `preserve_order`, D49). El orden de las claves no forma parte del
contrato: el cliente lee el JSON por nombre.

- `code` es el `code()` estable del error: `lexical`, `syntax`, `duplicate-key`, `limit`,
  `meta-schema`, `content-model`… según el frontal.
- `line` y `column` son las de `schemac check`. `message` es su texto **sin** el prefijo
  «línea L, columna C: ». Si un mensaje no empezara por ese prefijo, iría entero (B2).
- `errors` trae como mucho los **100** primeros, en el orden de `schemac check`, y
  `total` los cuenta todos. `truncated` es `total > errors.length`.

### Errores de la petición o del servidor

| Estado | Cuerpo | Cuándo |
| --- | --- | --- |
| 413 | `{"error":"too-large","limit":1048576,"message":…}` | Un `Content-Length` anuncia más que el tope: se responde **al instante, sin leer el cuerpo**, y a un cliente con `Expect: 100-continue` no se le manda `100 Continue`, porque hyper solo lo envía cuando alguien lee el cuerpo. Sin `Content-Length` (`chunked`), o con uno que miente por lo bajo, el cuerpo se lee como mucho hasta tope + 1 bytes (`DefaultBodyLimit`) y entonces 413. |
| 400 o 431 (de hyper) | vacío, `connection: close` | `Content-Length` que no es un número, o dos `Content-Length` distintos (400); uno enorme, como `u64::MAX`, da 431 (TR1-O1). Lo rechaza hyper al leer las cabeceras, antes de que la petición llegue al `Router`, así que el cuerpo no es JSON. No se cuelga ([pendientes.md](pendientes.md) §10). |
| 500 | `{"error":"internal","message":"error interno del compilador"}` | Pánico en la compilación, IR que no pasa `verify` o `BuildError::Internal`. El mensaje es fijo (D15) y el detalle va al registro (§4). |
| 503 | `{"error":"busy",…}` + `Retry-After: 5` | Todas las compilaciones ocupadas, o el presupuesto de cuerpos en vuelo agotado (§3, SEC-H1; en ese caso el cuerpo no se lee). No hay cola. |
| 408 | `{"error":"body-timeout","message":…}` | El cuerpo no terminó de llegar en `BODY_READ_TIMEOUT_MS` (10 s; SEC-H1). Sin `Connection: close`: hyper cierra la conexión porque el cuerpo quedó sin leer (CR2-S4). |
| 503 | `{"error":"no-thread",…}` + `Retry-After: 5` | No se pudo crear el hilo de compilación. |
| 504 | `{"error":"timeout","message":…}` | La compilación pasó del tope de tiempo. |
| 404 | `{"error":"not-found",…}` | Ruta desconocida. |
| 405 | `{"error":"method-not-allowed",…}` | Método no permitido (p. ej. `GET /api/schemas`). |
| 400 | `{"error":"body",…}` | No se pudo leer el cuerpo (la conexión se cortó a mitad, u otro error del cuerpo). |

## 3. Topes y su porqué

Un esquema puede venir de cualquiera, y la compilación tiene amplificaciones medidas en
la Fase 3 (`docs/fase3/pendientes.md` §1-§2, `docs/fase2/pendientes.md` §12). Los
topes se eligen juntos:

| Tope | Por defecto | Variable | Por qué |
| --- | --- | --- | --- |
| Subida | 1 MiB (U3) | `MAX_UPLOAD_BYTES` | La compilación completa hasta la IR exportada usa ≈ 158 veces la entrada en el peor JSON Schema medido (≈ 59 veces en XSD): **≈ 165 × subida** con margen, unos 165 MiB por compilación con 1 MiB. Los esquemas reales pesan unos KB. `schemac` admite 16 MiB, pero en un servidor eso serían ≈ 2,7 GB por compilación. |
| Concurrencia | 2 | `MAX_CONCURRENT_COMPILES` | 2 × 165 MiB ≈ **350 MiB** de pico de compilaciones, más los cuerpos en vuelo (abajo) y el proceso base, y cabe en una máquina pequeña de EC2 (F7.7). Lleno → 503 enseguida (`try_acquire_owned`), sin cola, que también consumiría memoria. |
| Tiempo | 10 s | `COMPILE_TIMEOUT_MS` | El peor XSD de 16 MiB tarda ≈ 7-28 s en la UPA (5.7-P3, `docs/fase3/pendientes.md` §38), lineal en el número de modelos de 4 096 partículas (D26). Con 1 MiB caben como mucho ≈ 6 de esos modelos: **≲ 2 s**. 10 s deja margen a una máquina lenta. Pasado el tope, 504. |
| Errores devueltos | 100 + el total | (fijo) | El texto de los errores puede ser ρ(r) ≈ 126 + 0,67·r veces la entrada (118,1× medido). Con 100 mensajes de ≲ 1 KB el cuerpo del 422 queda en ≲ 100 KB, sea cual sea la subida. Solo esos 100 se pasan a texto. |
| IR | 4 MiB (U8) | `MAX_IR_BYTES` | La IR JSON llega a ≈ 93 veces la entrada (93 MiB con 1 MiB). Se escribe en un `fmt::Write` que falla al pasar el tope: deja de escribirla ahí, sin construirla entera, y la respuesta sale con `ir: null` e `irOmitted: true`. El resumen del autómata sigue. |
| Cuerpos en vuelo | 32 MiB | `MAX_INFLIGHT_BODY_BYTES` | SEC-H1. El cuerpo se lee entero antes de pedir el permiso de compilación, así que sin otro tope mil conexiones con 1 MiB a medias retenían 1 GiB (el auditor vio `OOMKilled` con 400 conexiones en 300 MiB). Antes de leer, cada petición reserva en un semáforo global los bloques de 1 KiB que su cuerpo puede llegar a ocupar: `min(Content-Length, tope)`, o el tope entero si es `chunked`, y al menos uno. Si no caben, 503 `busy` sin leer nada. La reserva se suelta al terminar de leer. 32 MiB son 32 subidas de 1 MiB a la vez. El cuerpo se lee frame a frame (`leer`, `api.rs:191`) a **un solo** `Vec` con capacidad igual a lo reservado, y cada frame se suelta al copiarlo (SEC2-H1): lo que retiene un cuerpo es lo que reservó, tenga los trozos que tenga. Antes, `Bytes::from_request` guardaba 32 B por frame, y con `chunked` de 1 byte 1 MiB ocupaba ≈ 40 MiB. Precio (CR2-I1): unas 32 conexiones lentas agotan el presupuesto y todas las subidas reciben 503 durante el plazo de lectura (10 s), de forma renovable; el servidor ya no cae ([pendientes.md](pendientes.md) §8). |
| Lectura del cuerpo | 10 s | `BODY_READ_TIMEOUT_MS` | SEC-H1. Una conexión que manda el cuerpo a medias y se calla suelta su reserva a los 10 s con un 408 `body-timeout`. 1 MiB en 10 s pide ≈ 100 KB/s, cualquier conexión real. |

**Memoria total del proceso.** Es la suma de dos partes:

- las compilaciones: ≈ 165 × subida × concurrencia = 165 × 1 MiB × 2 ≈ **350 MiB**. La IR
  y M del autómata entran en esa cifra. `automaton::build` corre después de que el AST y
  la tabla de símbolos se suelten al cerrar el brazo del `match` (`compilar.rs:205-231`), y
  M ocupa O(2²⁰ + n + W) (F4-U6). Las tablas tienen como mucho 2²⁰ entradas sea cual sea
  la subida; el peor caso medido es ≈ 362 MB con un esquema de 16 MiB hecho para ello
  (`docs/fase4/automata.md` §7), y con 1 MiB n y W son 16 veces menores (CR-S2);
- los cuerpos que se están leyendo: como mucho `MAX_INFLIGHT_BODY_BYTES` = **32 MiB** de
  búferes. Desde la ronda 3 no hay copia final, porque el búfer ya es contiguo, así que el
  pico transitorio de ≈ 2 × por cuerpo que señaló TR2-O3 (la copia de `collect().to_bytes()`)
  desaparece. A eso se suman los búferes de lectura de hyper por conexión, que no pasan por
  el presupuesto. Un cuerpo ya leído pasa al hilo de compilación, que es el primer
  apartado, o se suelta con el 503.

En total ≈ 380 MiB más el proceso base (≈ 30-40 MB con 2 000 conexiones inactivas sin
cuerpo, según el auditor), por debajo del `mem_limit` de 1 GiB de F7.7.

**Medido** con sondas manuales, binario release de Linux en un contenedor con
`--memory 300m`; pico `VmHWM` del servidor:

| Sonda | Antes | Después |
| --- | --- | --- |
| Ronda 2 (`h1.sh`): 1 000 conexiones con `Content-Length: 1000000` y 999 999 bytes enviados sin cerrar | `OOMKilled` (400 conexiones, ronda 1) | ≈ 38 MB; 33 × 408 a los 10 s y 967 × 503 sin leer el cuerpo |
| Ronda 3 (`h.pl` del auditor): 32 conexiones `chunked` con trozos de 1 byte (6,3 MB en el cable cada una) | 644 MB y luego `Killed` en 1 GiB | **≈ 53 MB**; 32 × 408 |
| Ídem con 4 conexiones | 160 MB | ≈ 10 MB |
| Ídem con 32 conexiones y trozos de 16 bytes | 116 MB | ≈ 54 MB |

La sonda de la ronda 2 usaba `Content-Length` y una sola escritura grande, que llega en
pocos frames, así que no cubría el `chunked` de 1 byte (SEC2-H1). En todas las sondas el
servidor sigue vivo; en la de la ronda 2, además, `/api/health` respondió durante la
sonda y una compilación después dio 200. Quedan fuera las respuestas en vuelo (hasta ≈ 8 MiB de la IR por conexión que no
lee su respuesta): sin tope en el servidor, van a F7.7 ([pendientes.md](pendientes.md)
§8, CR-I3).

Rangos válidos de las variables (`server/src/lib.rs`, `Config::from_env`):

| Variable | Rango | Por qué el máximo |
| --- | --- | --- |
| `SERVER_ADDR` | `ip:puerto` (`SocketAddr`) | — |
| `MAX_UPLOAD_BYTES` | 1 a 16 777 216 (16 MiB) | El tope de `schemac` (`00-alcance.md` §4). Más no lo compila ni la CLI, y serían ≈ 2,7 GB por compilación. |
| `COMPILE_TIMEOUT_MS` | 1 a 600 000 (10 min) | De sobra para el peor esquema de 16 MiB (≈ 7-28 s). |
| `MAX_CONCURRENT_COMPILES` | 1 a 64 | Cada compilación es un hilo con 8 MiB de pila y hasta ≈ 165 × subida de memoria; 64 ya no cabe en la máquina de F7.7. |
| `MAX_IR_BYTES` | 0 a `usize::MAX` | Sin máximo: con 0 nunca incluye la IR, y por arriba la acota la subida (≈ 93 × subida). |
| `MAX_INFLIGHT_BODY_BYTES` | `MAX_UPLOAD_BYTES` a 1 073 741 824 (1 GiB) | Nunca menos que una subida, que si no jamás cabría; 1 GiB es el `mem_limit` de F7.7. |
| `BODY_READ_TIMEOUT_MS` | 1 a 600 000 (10 min) | Como el tiempo de compilación. |

Una variable que no se entiende o que se sale de su rango hace fallar el arranque con un
mensaje que nombra la variable (código 2).

## 4. Aislamiento por hilo: qué cubre y qué no

Cada compilación corre en `std::thread::Builder::new().stack_size(8 << 20)`, nunca en un
hilo de tokio. El parser es recursivo y su tope de profundidad (256) está garantizado
con ≥ 2 MiB de pila (D7); en Windows el hilo principal solo tiene 1 MiB. El AST, la tabla
de símbolos o el `Analysis` y la IR se crean y se liberan dentro del hilo (los `Drop` del
AST XSD son recursivos, D36). El hilo solo devuelve texto (`Compilado`) por un
`tokio::sync::oneshot`, junto con el `id`, que también se calcula en él.

**Cubre:**

- **Pánico** (`assert!`, índice fuera de rango, `unwrap`; `docs/fase3/pendientes.md`
  §2.1): al desenrollar, el hilo suelta el `oneshot` y el manejador responde 500. Primero
  suelta el permiso del semáforo, así que la petición siguiente ya lo encuentra libre. El
  perfil nunca usa `panic = "abort"` (§2.2; lo recuerda un comentario en el `Cargo.toml`
  del workspace). Lo prueba `panico_da_500_y_el_servidor_sigue`.
- **Tiempo:** el manejador espera como mucho `COMPILE_TIMEOUT_MS`. Un hilo no se puede
  matar desde fuera, así que el que se pasa sigue hasta terminar. Se lleva el permiso del
  semáforo y sigue contando en la concurrencia: con 2 compilaciones colgadas el servidor
  da 503 en lugar de lanzar más. Lo prueba `tope_de_tiempo_da_504_y_el_permiso_sigue_en_el_hilo`.
- **Errores internos (D15):** el usuario recibe un único mensaje fijo. Al registro, que
  es stderr (`docs/fase3/pendientes.md` §14), van el mensaje, el recuento y los 8 primeros
  invariantes rotos (`InternalError::detail(8)`), con el prefijo `server: registro:`.
  Se escribe con `writeln!` ignorando el error, porque `eprintln!` haría pánico.

**No cubre** (riesgo residual declarado, U4; [pendientes.md](pendientes.md) §1):

- un **fallo de asignación** (sin memoria): Rust aborta el proceso entero, y ni `join` ni
  `catch_unwind` lo capturan. Los topes de §3 lo hacen improbable (≈ 380 MiB de pico con los cuerpos en vuelo), pero
  no lo impiden;
- un **desbordamiento de pila**: también aborta el proceso. Lo evitan el tope de
  profundidad del parser y las pilas explícitas de las fases siguientes (D7), no el hilo.

Con cualquiera de los dos cae el proceso entero, con las peticiones en curso. El tope
duro va en F7.7: `mem_limit` (p. ej. 1 GiB) y `restart: unless-stopped` en Docker
Compose. El aislamiento por proceso (proceso hijo con `RLIMIT_AS`) va en F7.3.

## 5. Arrancar y probar

```sh
cargo run -p server --release                     # escucha en 127.0.0.1:8080
SERVER_ADDR=0.0.0.0:8080 cargo run -p server --release   # en el contenedor (U7, F7.7)
```

PowerShell: `$env:SERVER_ADDR = '127.0.0.1:18080'; cargo run -p server --release`.

Al arrancar escribe en stderr `server: escuchando en http://… (subida ≤ …, … ms, …, cuerpos en vuelo ≤ … bytes, lectura ≤ … ms)`.
Ctrl-C (o SIGTERM en Unix, que es lo que manda `docker stop`) deja de aceptar
conexiones, espera a las peticiones en curso y termina con `server: apagado`. Una
compilación que siga corriendo en su hilo no retrasa el apagado: el proceso termina
igual. Verificado en Docker (ronda 1, 19 nov), con el binario de Linux como PID 1 del
contenedor: `docker kill --signal TERM` y `--signal INT` dan los dos `server: apagado` y
código de salida 0. En Windows, Ctrl-C lo atiende `tokio::signal::ctrl_c`; no se probó
a mano.

```sh
curl http://127.0.0.1:8080/api/health
curl --data-binary @tests/schemas/parse-ok/usuarios.json http://127.0.0.1:8080/api/schemas
curl --data-binary @tests/schemas/xsd/parse-ok/pedido.xsd http://127.0.0.1:8080/api/schemas
curl --data-binary '{"type": 1}' http://127.0.0.1:8080/api/schemas          # 422
head -c 1048577 /dev/zero | tr '\0' ' ' | curl -i --data-binary @- http://127.0.0.1:8080/api/schemas   # 413
```

Hay que usar `--data-binary`: `-d` quita los saltos de línea y cambia las posiciones
de los errores.

Pruebas: `cargo test -p server` (en Windows, `--release` si Smart App Control bloquea los
binarios de debug). Ninguna abre puertos ni mide tiempo de pared (D1). Las compilaciones
que hacen pánico, se bloquean o fallan por dentro se inyectan con `router_con` (B1); el
fallo al crear el hilo se provoca pidiendo una pila de 4 EiB (`Config::stack_size`, que
nunca baja de 8 MiB y no se lee del entorno).

## 6. Dependencias

axum `0.8.9` sin features por defecto (`http1`, `tokio`, `json`), tokio `1.54.1`
(`rt-multi-thread`, `net`, `sync`, `time`, `signal`; `macros` solo en las pruebas),
serde_json `1.0.154` sin features nuevas (D49) y sha2 `0.11.1`. En las pruebas, tower
`0.5.3` con `util`. Son 70 crates en el árbol del servidor, sin TLS (lo pone Caddy delante,
F7.7), sin openssl, sin `cc` y sin `serde_derive`. En Linux el único crate de enlace al
sistema es `libc`; en Windows, `windows-sys`. Los dos son bindings en Rust, sin compilar C.
El `Cargo.lock` del workspace pasa de 136 a 171 paquetes. `codegen` deja de ser
dependencia del servidor, porque no lo usaba.
