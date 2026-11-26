# Fase 5 — Generación de código

> Enunciado, §3: «Backend que emite C y Rust: tablas de transición densas o saltos directos, pila de profundidad
> acotada y configurable, sin asignación dinámica, API C-ABI/FFI y opcionalmente WASM; generador de payloads válidos
> e inválidos (serialización JSON/XML) a partir del esquema para alimentar las pruebas.»

Código: `crates/codegen` (`rust.rs`, `c.rs`, `fuentes.rs`), `crates/runtime-c` (núcleo C) y, para los
generadores de payloads, `crates/bench/src/{generar,generar_xml,mutar}.rs`.

## 1. Comando

```
schemac compile <esquema> --lang <rust|c> --out <dir> [--limit nombre=valor]...
```

`schemac` construye M (Fase 4) y escribe en `<dir>` un validador **autocontenido**:

- con `rust`, compila con `rustc --edition 2021 -O main.rs`, sin Cargo ni red;
- con `c`, compila con `cc -std=c11 -O2 -Wall -Wextra -o validador *.c`.

Esas órdenes son las mismas que usa el sandbox de la Fase 7. La salida es **determinista**: solo depende de M, porque
se recorren las tablas en orden, sin mapas, fechas ni rutas del anfitrión.

## 2. Estrategia: tablas densas

Se emiten **tablas densas**, no saltos directos. δ es un arreglo plano `DELTA[q · |Σ| + a]` de índices de
transición, y el resto de las tablas de M (`TRANS`, `CHECKS`, `FRAMES`, `MASKS`, `NUMS`, `GATES`…) son arreglos de
registros de tamaño fijo enlazados por índices `u32`, sin punteros. Es el mismo formato en Rust y en C.

Razones:

1. **Un solo ejecutor.** El código generado no reescribe la lógica: las tablas van delante del **mismo** núcleo que
   se probó en la Fase 4 (`runtime-rs`, o su port a C). Un esquema nuevo solo cambia los datos.
2. **Tiempo predecible.** Cada evento es una indexación en `DELTA` y una en `TRANS`. Con saltos directos el código
   crecería con |Q|·|Σ| y dependería más del optimizador.
3. **Tamaño acotado.** El presupuesto de 2²⁰ entradas de la Fase 4 acota también el tamaño del código emitido.

Encabezado de un `tablas.c` real (esquema `arbol-recursivo`):

```c
/* Tablas densas de M (D46 (1)) para payloads json: 41 estados, |Σ| = 26, 308 transiciones,
 * 4 tipos de marco. */
#include "validador.h"
static const uint32_t DELTA[1066] = {
    1, 0, 2, 0, 0, 0, 5, 6, 7, 3, 0, 4, 0, 0, 0, 0,
    …
```

## 3. Emisor Rust

| Archivo | Contenido |
| --- | --- |
| `main.rs` | raíz `std`: lee stdin por bloques y escribe una línea JSON en stdout con el código de salida |
| `validador.rs` | fachada `no_std`: `STATE_WORDS`, `SV_MAX_DEPTH`, `Validador`, `validar` |
| `tablas.rs` | las tablas de M como `static` |
| `lexer*.rs`, `runtime_rs*.rs` | copias de las fuentes de `crates/lexer` y `crates/runtime-rs` |

Las fuentes se incrustan con `include_str!` al compilar `codegen`, así que la copia es siempre la del árbol actual.
Para que compilen como módulos de un solo crate sin Cargo se reescriben tres cosas, de forma mecánica: se quita el
`#![no_std]` de cada `lib.rs`, cada `mod x;` recibe un `#[path = "<prefijo>_x.rs"]` (archivos planos, que es lo
único que admite el sandbox) y las rutas `crate::…` y `lexer::…` se ajustan. La prueba `copia_sincronizada` vigila
que la lista de archivos incrustados sea exactamente la de los dos `src/`, y otra prueba compila la parte de
validación como `#![no_std]` sin `alloc`.

## 4. Emisor C11 y API C-ABI

| Archivo | Contenido |
| --- | --- |
| `main.c` | stdin por bloques → una línea JSON y el código de salida (los mismos que en Rust) |
| `validador.h`, `validador.c` | la API C-ABI |
| `tablas.c` | las tablas de M como arreglos `static const` |
| `sv_lexico.h`, `sv_lexico.c` | las tablas de los AFD de la Fase 1, volcadas del crate `lexer` |
| `sv_nucleo.h`, `sv_nucleo.c` | el núcleo C (`crates/runtime-c`), port **función por función** del de `runtime-rs` |

**API** (de un `validador.h` emitido):

```c
#define SV_FORMATO "json"
#define SV_MAX_DEPTH 256          /* profundidad máxima con los límites emitidos */
#define SV_STATE_WORDS 9472u      /* palabras de memoria del núcleo */

typedef struct {
    sv_validator_t v;
    uint64_t mem[SV_STATE_WORDS]; /* pila de marcos + reservas de la capa de buena formación */
} sv_state_t;                     /* tamaño FIJO: se puede declarar en la pila o como global */

void sv_limits_emitted(sv_limits_t* lim);
void sv_init(sv_state_t* s, const sv_limits_t* lim);   /* lim NULL = los límites emitidos */
int  sv_feed(sv_state_t* s, const uint8_t* chunk, size_t len); /* 0 sigue, -1 se detuvo */
void sv_abort_limit(sv_state_t* s);                    /* corta por tiempo (lo vigila el llamador) */
int  sv_finish(sv_state_t* s, sv_report_t* out);       /* devuelve el código de salida */
int  sv_report_json(const sv_report_t* r, char* buf, size_t cap);
int  sv_exit_code(const sv_report_t* r);
```

- **Sin `malloc`.** Toda la memoria es `sv_state_t`, de tamaño conocido al compilar. Para `arbol-recursivo`,
  9 472 palabras de 8 bytes son 74 KiB, que es el pico de memoria que mide la Fase 6.
- **Sin recursión.** El núcleo C recorre igual que el de Rust, con la pila en `mem`.
- **Pila acotada y configurable.** `SV_MAX_DEPTH` y `SV_STATE_WORDS` salen de los límites con que se emite
  (`--limit maxDepth=…`). Pasar la profundidad es un error `limit` controlado, nunca un desbordamiento.

**Códigos de salida** (`sv_exit_code`, iguales en Rust): 0 válido; 1 inválido por el esquema; 2 malformado o
límite (`lexical`, `syntax`, `limit`); 3 error interno.

## 5. Ejemplo de uso

```
$ schemac compile tests/schemas/parse-ok/arbol-recursivo.json --lang c --out gen
… validador C en gen (8 archivos; compilar con `cc -std=c11 -O2 -Wall -Wextra -o validador *.c`)
$ cd gen && cc -std=c11 -O2 -Wall -Wextra -o validador *.c
$ printf '{"value": 1, "children": [{"value": "x"}, {"children": []}]}' | ./validador
{"valid":false,"violations":2,"errorKind":"schema","offset":36,"line":1,"column":37,
 "pointer":"/children/0/value","pointerTruncated":false,"rule":"#/$defs/node/properties/value","keyword":"type"}
$ echo $?
1
```

Las dos violaciones son el `"x"` que no es entero y el segundo hijo sin `value`. El reporte da la **primera**
(offset, línea, columna, puntero en la instancia y regla del esquema). El validador Rust del mismo esquema da
exactamente la misma línea. Esta salida se comprobó al escribir este documento, compilando los dos con `gcc` y
`rustc`.

## 6. Equivalencia: prueba diferencial

`crates/codegen/tests/diferencial.rs` ejecuta **tres** implementaciones del mismo M sobre las mismas entradas:

1. el intérprete de referencia (`automaton::validate_with`);
2. el validador Rust generado, compilado con `rustc` en *debug* (con los desbordamientos comprobados);
3. el validador C generado, compilado con `-fsanitize=address,undefined -fno-sanitize-recover=all` cuando el
   compilador lo admite.

Las entradas son cada payload del corpus y mutaciones deterministas de cada uno (truncar, borrar, duplicar o cambiar
un byte por llaves, comillas, `<`, `&` o bytes no ASCII). Cada entrada se prueba en bloques de **1, 7 y 4 096
bytes** y en los dos modos. Los tres reportes deben coincidir **campo a campo**: válido, peso, clase de error,
offset, línea, columna y regla.

## 7. Generadores de payloads

Los tres generadores son deterministas con semilla. Usan **dos oráculos independientes**: el DPDA y la línea base
DOM de la Fase 6 (`serde_json` + `jsonschema`; libxml2 en XML). Un caso solo entra si los dos coinciden.

| Binario | Qué genera | Cómo |
| --- | --- | --- |
| `generar` | JSON **válidos** | recorre la IR desde la raíz; en cada instancia junta las reglas del nodo, de sus `$ref` y `allOf` y de **una** rama al azar de cada `anyOf`/`oneOf`, y produce un valor que las cumple: literales de `enum`/`const`, rangos y `multipleOf`, longitudes, `pattern` (candidatos comprobados con `ir::regex`), propiedades requeridas, arreglos con `minItems`/`uniqueItems`… La profundidad está acotada (`PROFUNDIDAD`). |
| `generar_xml` | XML **válidos, inválidos y malformados** desde la IR de un XSD | válidos: un elemento global, sus atributos requeridos y algunos opcionales, contenido según el modelo (`sequence` en orden, una rama de `choice`, `all` en orden al azar, cada partícula entre `minOccurs` y `minOccurs + 2`) y textos de su tipo; inválidos: un sabotaje (omitir un hijo o un atributo obligatorio, un texto fuera de su tipo); malformados: truncar, cambiar el nombre de cierre, repetir un atributo, una entidad no declarada o un `<!DOCTYPE` |
| `mutar` | JSON **inválidos y malformados** a partir de los válidos | mutaciones semánticas (cambiar un tipo, salir de un rango, borrar o añadir un miembro) para casos `invalid`, y sintácticas (truncar, coma colgante, cero a la izquierda, escape o UTF-8 inválidos, clave repetida) para casos `malformed` |

```
cargo run -p bench --release --bin generar     [-- --escribir]
cargo run -p bench --release --bin generar_xml [-- --escribir]
cargo run -p bench --release --bin mutar       [-- --escribir]
```

Con `--escribir`, los casos aceptados se añaden a `tests/cases/` (prefijos `valid-gen-*`, `invalid-mut-*`,
`malformed-json-mut-*`, `*-xml-*`), que es la semilla de la base NoSQL de la Fase 7.

## 8. WASM

No está hecho. La tarea opcional E8 sigue `libre` en `tasks/CHECKLIST.md`. Como el núcleo es `no_std` sin `alloc`,
compilar `runtime-rs` más las tablas a `wasm32-unknown-unknown` sería directo, pero no se implementó ni se probó.
