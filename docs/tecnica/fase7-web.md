# Fase 7 — Aplicación web

> Enunciado, §3: «Backend (API REST + WebSocket) y frontend con editor de esquema, compilación y descarga del código
> generado, envío de payloads, visualización del autómata y del árbol de derivación, y panel de comparación en vivo.
> Compilación y ejecución en sandbox; despliegue en AWS o Azure.» §4: «Todos los casos (esquema, payload, resultado
> esperado y métricas) se almacenan en una base de datos NoSQL en la nube.»

Código: `server/` (axum), `web/` (React + Vite), `sandbox/` (imagen Docker), `crates/atlas` (MongoDB) e `infra/`
(Docker Compose, Caddy y el script de despliegue).

## 1. Arquitectura

```
 navegador ──HTTPS──▶ Caddy (contenedor web: estáticos + proxy, TLS de Let's Encrypt, cabeceras, tope de 1 MiB)
                          │  /api/*  /ws/*
                          ▼
                     servidor axum (contenedor server: 600 MiB, solo lectura, sin capacidades)
                      │   │          │
                      │   │          └──docker.sock──▶ sandbox: un contenedor nuevo por trabajo (C o Rust)
                      │   └── MongoDB Atlas (casos y métricas)
                      └── compilador en proceso: Fases 1-5 (mismos crates que schemac)
```

El servidor no reimplementa nada: usa los mismos crates que `schemac` (`schema-parser`, `semantic`, `xsd`, `dtd`,
`ir`, `automaton`, `codegen`), y valida con el mismo núcleo `runtime-rs` que el código generado.

## 2. API

| Método y ruta | Qué hace |
| --- | --- |
| `GET /api/health` | estado del servidor, de la base de datos y del sandbox |
| `POST /api/schemas` | compila el esquema (formato detectado por contenido): errores con línea y columna, o la IR v2 y el resumen de M; `id` = SHA-256 del esquema |
| `POST /api/automaton` | el grafo de M para Cytoscape (estados con su grupo, transiciones y sus comprobaciones) |
| `POST /api/derivation` | el árbol de derivación de un payload (Fase 2 §7) en JSON para d3-hierarchy |
| `POST /api/compare` | valida un payload con el DPDA y con la línea base DOM (`serde_json` + `jsonschema`; solo JSON, porque libxml2 no está en el servidor) y devuelve los dos reportes con sus tiempos de compilar y validar, la memoria del DPDA y el tamaño de M |
| `POST /api/code` | descarga el validador generado, C o Rust, como ZIP (`server/src/zip.rs`, sin comprimir, con CRC-32) |
| `POST /api/validate` | compila el código generado **en el sandbox**, lo ejecuta con el payload y lo compara con el intérprete |
| `GET /api/tests` | lista los casos de prueba guardados en MongoDB Atlas |
| `POST /api/tests/run` | ejecuta los casos con el DPDA y compara con su `expected` |
| `GET /ws/validate` | WebSocket de validación por bloques (§3) |

Todas las respuestas son JSON, y las de error llevan un campo `error` con un código estable (`too-large`, `busy`, `timeout`,
`body-timeout`, `schema`, `automaton`, `codegen`, `sandbox`, `database`…).

**Topes del servidor** (`server::Config`, configurables por variables de entorno):

| Variable | Por defecto |
| --- | --- |
| `MAX_UPLOAD_BYTES` | 1 MiB por petición (413 `too-large`) |
| `COMPILE_TIMEOUT_MS` | 10 s por compilación |
| `MAX_CONCURRENT_COMPILES` | 2 a la vez (503 si no hay permiso) |
| `MAX_IR_BYTES` | 4 MiB de IR en la respuesta |
| `MAX_INFLIGHT_BODY_BYTES` | 32 MiB de cuerpos en vuelo entre todas las peticiones |
| `BODY_READ_TIMEOUT_MS` | 10 s para leer el cuerpo |

Cada compilación corre en un **hilo propio con 8 MiB de pila**, porque los parsers recursivos están acotados a 256
niveles solo en hilos de al menos 2 MiB.

## 3. WebSocket: validación en vivo por bloques

Protocolo de `/ws/validate` (`server/src/flujo.rs`):

1. El cliente manda el **esquema** como primer mensaje de texto. El servidor responde
   `{"type":"ready","format","states","transitions","memoryBytes"}` o `{"type":"error",…}` y cierra.
2. El cliente manda el payload en **bloques binarios**. Tras cada uno, el servidor responde
   `{"type":"progress","chunk","bytes","stopped"}`, donde `stopped` indica que ya hay un error de peso ∞ y lo que
   llegue después no cambia el veredicto.
3. El cliente manda el texto `fin` y el servidor responde `{"type":"report","bytes","chunks","report":{…}}`, con
   los mismos campos que el binario generado.

El servidor **no guarda el payload**: valida con el núcleo incremental de memoria fija, en un hilo con pila
explícita, y cada sesión tiene un tope de CPU.

## 4. Sandbox para el código generado

`POST /api/validate` compila y ejecuta código que salió del compilador a partir de un esquema del usuario. Cada
trabajo corre en un contenedor **nuevo** de la imagen `validador-sandbox:1` (`sandbox/Dockerfile`: Rust 1.100 fijado
por *digest*, más `cc`) con estas opciones de `docker run` (`server/src/sandbox.rs`):

| Opción | Valor |
| --- | --- |
| red | `--network none` |
| usuario | `--user 10001:10001` (sin root) |
| sistema de archivos | `--read-only` y un `tmpfs` de 256 MiB en `/tmp` como única zona escribible |
| privilegios | `--cap-drop ALL`, `--security-opt no-new-privileges`, `--ipc none` |
| memoria | `--memory 512m --memory-swap 512m` (sin *swap*) |
| CPU y procesos | `--cpus 1.000`, `--pids-limit 64` |
| `ulimit` | `nofile=256`, `fsize` = tamaño del tmpfs, `core=0` |
| tiempo | `timeout` dentro (60 s para compilar, 10 s para ejecutar) y, fuera, un plazo con 15 s de holgura tras el cual se hace `docker kill` |

Los fuentes y el payload entran como un **tar por la entrada estándar**, sin montajes. Las órdenes de compilación
son fijas (`sandbox/sandbox-run.sh`: `cc -std=c11 -O2 -Wall -Wextra` o `rustc --edition 2021 -O main.rs`), y de
fuera solo llegan el lenguaje, los nombres de archivo validados y los bytes. La salida se corta en 64 KiB.

## 5. Frontend

`web/`: React 18, Vite, el editor **Monaco** (servido localmente), **Cytoscape** (con `cytoscape-dagre`) para el
autómata y **d3-hierarchy** para el árbol. Hay seis pestañas, dibujadas como estados de un autómata (la activa con
doble círculo):

| Pestaña | Qué muestra |
| --- | --- |
| Esquema | editor Monaco con ejemplos de JSON Schema, XSD y DTD; el formato detectado; los errores marcados en el editor; el resumen de M y la IR |
| Autómata | M como grafo (Cytoscape); cada estado lleva el color de su grupo (subesquema o elemento) |
| Árbol de derivación | el árbol LL(1) del payload (d3) |
| DPDA frente a línea base | el mismo payload validado por los dos, con veredictos, errores, tiempos y memoria lado a lado |
| Código generado | descarga del ZIP C o Rust y validación compilada en el sandbox, comparada con el intérprete |
| Casos de prueba | los casos de Atlas y su ejecución |

Todo lo que viene del servidor se muestra **como texto**, nunca como HTML. La detección de formato tiene una copia
en TypeScript (`web/src/api.ts`) que se prueba con la misma tabla que la de Rust (`tests/schemas/formato.json`).

## 6. Base de datos NoSQL: MongoDB Atlas

`crates/atlas` carga el corpus de `tests/cases/` en la colección **`cases`** (un documento por caso, `_id` = id del
caso, con *upsert*, así que repetir la carga no duplica). Con `--metricas <csv>` carga las mediciones de la Fase 6
en la colección **`metrics`**. La conexión usa `MONGODB_URI`, que está en `.env` y nunca en el repositorio. Sin esa
variable, el servidor arranca y `/api/tests` responde 503.

```
cargo run -p atlas                          # casos
cargo run -p atlas -- --metricas <csv>      # métricas
```

**El corpus.** Cada caso tiene `_id`, `category`, `format`, `description`, el esquema (`schema` o `schemaFile`), el
payload (`payload`, `payloadFile`, `payloadBase64` o `generator`), `expected` (`valid`, `violations`, `errorKind`,
`offset`, `pointer`) y, en los `non-dpda`, la estrategia esperada. Hoy hay 94:

| Categoría | Casos | Qué cubre (enunciado §4) |
| --- | ---: | --- |
| `valid` | 27 | simples, anidados y recursivos (`$ref`, árboles) en JSON, XSD y DTD |
| `invalid` | 37 | tipos incorrectos, obligatorios faltantes, propiedades adicionales, `enum` y rangos |
| `malformed` | 20 | sintaxis incorrecta, UTF-8 inválido, claves duplicadas, etiquetas sin cerrar, `<!DOCTYPE` |
| `non-dpda` | 8 | `anyOf`/`oneOf` ambiguos, `not`, `pattern` y `uniqueItems` con su estrategia |
| `extreme` | 2 | número enorme y profundidad sobre el límite |
| `pumping` | 0 en archivos | se generan con el arnés de la Fase 6 (`metricas --bombeo`) |

## 7. Despliegue en AWS

`infra/` despliega en una **EC2 de AWS** (us-east-1) con Docker Compose:

- **`server`**: la imagen del servidor con `mem_limit: 600m`, `pids_limit: 128`, `cpus: 1.5`, raíz de solo lectura,
  `cap_drop: ALL`, `restart: unless-stopped` y logs con tope (10 MB × 3).
- **`web`**: Caddy con los estáticos de Vite. Pide el certificado TLS a Let's Encrypt para el dominio
  `<ip>.sslip.io` y añade cabeceras de seguridad (HSTS, `nosniff`, `X-Frame-Options: DENY`, una CSP estricta sin
  `unsafe-eval`). También corta los cuerpos de más de 1 MiB con 413 antes de llegar al servidor y pone plazos de
  lectura y escritura.
- **Sandbox** (`docker-compose.sandbox.yml`, `desplegar.ps1 -Sandbox`): construye `validador-sandbox:1` en la EC2
  y monta `docker.sock` en el servidor para que pueda lanzar contenedores. Es un riesgo aceptado: quien controle el
  servidor controla Docker en esa máquina.

```
powershell -File infra/desplegar.ps1 -Ip <ip> -Llave <clave.pem> [-Sandbox]
```

El script compila las imágenes en local (la instancia es pequeña para compilar Rust), las sube por SSH y levanta
Compose.

**Estado al escribir este documento (25 nov 2026):** `https://54-157-96-49.sslip.io/api/health` responde
`{"database":true,"sandbox":true,"status":"ok","version":"0.1.0"}`.
