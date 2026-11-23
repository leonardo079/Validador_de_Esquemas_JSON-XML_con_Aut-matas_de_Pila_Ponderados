# Casos de prueba en MongoDB Atlas (F7.2, D72)

El plan dice: «Todos los casos viven en una colección de MongoDB Atlas y se ejecutan desde la aplicación; la carpeta
`tests/cases/` es solo la semilla que se sube». F7.2 hace las tres cosas:

- carga el corpus en Atlas;
- lo lista desde el servidor;
- lo ejecuta con el DPDA desde la web.

| Pieza | Dónde |
| --- | --- |
| Modelo del documento y conexión | `crates/atlas` (`documento`, `leer`, `coleccion`) |
| Carga (script local, D32) | `cargo run -p atlas` (binario `cargar-casos`) |
| API | `GET /api/tests[?category=…]` y `POST /api/tests/run[?category=…]` (`server/src/casos.rs`) |
| Web | Pestaña «Casos de prueba (Atlas)» (`web/src/Casos.tsx`) |

**Driver:** `mongodb` 3.10.2 sin sus features por defecto, con `bson-3`, `rustls-tls` y `dns-resolver`. Las features por defecto traen `bson` 2, que activa `serde_json/preserve_order` en **todo** el workspace (Cargo unifica features). Eso rompía D49 y tres pruebas de la línea base y del lexer, que dependen del orden y del tamaño del mapa de `serde_json`; con `bson` 3 no pasa (`cargo tree -e features -i serde_json`).

Clúster: Atlas M0 en us-east-1 (S0), base `validador` y colección `cases`. La URI va en `.env` (`MONGODB_URI=…`),
que git ignora, no entra en ninguna imagen (`.dockerignore`) y nunca se imprime.

## 1. El documento

Hay un documento por caso, con `_id` igual al id del caso:

```json
{
  "_id": "invalid-range-01", "version": 1, "category": "invalid", "format": "json",
  "description": "…", "phase": 4, "source": "tests/cases/invalid/invalid-range-01.json",
  "schema": "{ \"type\": \"array\", … }",
  "payload": {"kind": "text", "value": "[150, 7, -1]"},
  "expected": {"valid": false, "violations": 2, "errorKind": "schema", "pointer": "/0", "offset": 1}
}
```

- `schema` es el **texto exacto** del esquema, el mismo que leen las pruebas de corpus. Así no cambian el orden de
  las claves ni los lexemas de los números.
- `payload` puede ser de tres formas:
  - `{"kind": "text"}` si es UTF-8;
  - `{"kind": "base64"}` si no lo es (`malformed-json-invalid-utf8-01`);
  - `{"kind": "generator", "name", "params"}` si lo produce un generador.
- `source` es la ruta relativa al repo, nunca la absoluta de la máquina que carga.

## 2. Cargar

```text
cargo run -p atlas --release              # (en Windows con Smart App Control: sin --release)
cargo run -p atlas -- --podar             # además borra de Atlas los casos que ya no están en el corpus
cargo run -p atlas -- --en-seco           # solo lee y cuenta, sin conectar
```

La carga hace *upsert* por `_id`, así que es idempotente. El 20 nov:

| Carga | Resultado |
| --- | --- |
| Primera | 78 casos nuevos: 22 valid, 30 invalid, 16 malformed, 8 non-dpda y 2 extreme |
| Segunda | 78 sin cambios |
| Tras corregir `source` | 78 actualizados |
| 22 nov, con los XML de F5.4b | 8 nuevos, 2 actualizados y 76 sin cambios: 86 en Atlas; 86/86 pasan desde la URL pública |

Crea un índice por `category`.

## 3. API

| Ruta | 200 | Errores |
| --- | --- | --- |
| `GET /api/tests[?category=c]` | `{total, cases: [{id, category, format, description, phase, expected}]}`, sin esquema ni payload | 400 `request` si `category` no es `[a-z-]{1,32}`; 503 `no-database` sin `MONGODB_URI`; 502 `database` si Atlas falla o no responde en 10 s (el detalle va al registro) |
| `POST /api/tests/run[?category=c]` | `{total, passed, failed, skipped, errors, results: [{id, category, status, expected, got, mismatch, reason?}]}` | Los mismos, y los de `procesar` (503 `busy`, 504) |

Los casos se ejecutan en el hilo de `api::procesar`: permiso de concurrencia, pila de 8 MiB y tope de tiempo. Cada
caso pasa por la tubería del servidor (`compilar::ir_de` → `automaton::build`) y se valida en modo total. Después se
compara con su `expected`, igual que las pruebas de corpus:

- se comparan siempre `valid`, y `errorKind`, `violations` y `offset` si el caso los trae;
- `skipped`: el payload lo produce un generador, o una regla aún no tiene estrategia (`pending`);
- `error`: el esquema no compila o el documento no se puede leer.

Hay como mucho 1 000 casos por consulta. El servidor conecta con Atlas solo si existe `MONGODB_URI`; si falta o es
inválida, arranca igual, sin base de datos.

**Resultado el 20 nov**, con el servidor en Windows y con la pila de Docker detrás de Caddy: **78 casos, 78 pasan**.

## 4. Pruebas (sin red, D1)

- `crates/atlas/tests/documento.rs` (5):
  - todo el corpus hace ida y vuelta (caso → documento → BSON → JSON → caso) con el esquema byte a byte, el payload,
    `expected`, la descripción y `source` relativa;
  - el payload no UTF-8 va en base64;
  - base64 de ida y vuelta, con sus rechazos;
  - `leer` rechaza documentos incompletos;
  - la URI se lee del entorno o del `.env`.
- `server/tests/casos.rs` (4):
  - sin base de datos, las rutas dan 503;
  - una `category` no válida da 400;
  - el corpus entero, ejecutado desde sus documentos, no falla;
  - un `expected` falso da `failed` con los campos distintos, y un documento roto da `error`.

## 5. Despliegue

`infra/docker-compose.yml` pasa `../.env` al servicio `server` (`env_file`, opcional). `infra/desplegar.ps1` copia
el `.env` a la EC2 por `scp`, con permisos 600. **En Atlas, Network Access tiene que incluir la Elastic IP de la
EC2** (`despliegue.md` §2, paso 3).

## 6. Métricas (E2, D73)

Las medidas de F6.5 (`docs/fase6/metricas.md`) van a una segunda colección, `metrics`. Así volver a cargar los casos
no las borra.

```text
cargo run -p bench --release --bin metricas -- --csv target/metricas/metricas.csv
cargo run -p atlas --release -- --metricas target/metricas/metricas.csv        # casos y métricas
cargo run -p atlas --release -- --solo-metricas target/metricas/metricas.csv   # solo métricas
```

Hay un documento por medida, con `_id` = `"<caso>|<validador>"`:

- `case`: el `_id` del caso en `cases`, o `null` en el bombeo;
- `family`: `corpus` o `bombeo`;
- `label` y `validator` (`DPDA` o `línea base DOM`);
- `bytes`, `depth`, `valid` y `violations`;
- `seconds` (mediana), `mbPerSecond`, `nsPerByte`, `allocations` y `peakHeapBytes`;
- `states` y `transitions` (solo el DPDA);
- `measuredAt`, la fecha de la carga, y `source`.

Los valores que no caben en JSON (`inf`) se guardan como `null`. Hay un índice por `case`.

**Cargado el 21 nov:** 173 métricas, 151 del corpus (78 casos, los JSON con las dos columnas) y 22 de bombeo. La
segunda carga dejó las 173 sin cambios. Para cruzarlas con los casos en Atlas:
`db.metrics.find({case: "invalid-range-01"})` o un `$lookup` desde `cases` por `_id` = `case`.

Pruebas: `crates/atlas/tests/documento.rs` (3 más, 8 en total): lectura del CSV (comillas dobladas, comas dentro
del nombre del bombeo, campos vacíos, `inf`), rechazo de un CSV ajeno o incompleto, y que la cabecera coincida con la
que escribe `bench::csv`.

## 7. Pendientes

- **F7.2-P1:** guardar en Atlas los esquemas compilados por su `id` (SHA-256, `pendientes.md` §2) no está hecho.
  Ninguna vista lo necesita, porque el servidor no tiene estado (D70).
- **E2:** hecho (§6, D73).
- Los endpoints son API nueva: **aprobados por Leonardo (D82, 21 nov)**, con un cambio aditivo: un campo
  `truncated` cuando hay más de 1 000 casos ([pendientes.md](pendientes.md) §25).
