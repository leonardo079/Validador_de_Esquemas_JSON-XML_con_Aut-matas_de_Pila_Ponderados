# Emisor C11 y prueba diferencial (F5.2 y F5.3, D75 y D76)

F5.2 emite el mismo validador que F5.1 (`docs/fase5/codegen.md`) en C11: las tablas densas de M como arreglos
`static const` delante de un núcleo C que es el port, función por función, del ejecutor de `runtime-rs` y de los
lexers de la Fase 1. F5.3 comprueba que las tres implementaciones (el intérprete de referencia, el Rust generado y
el C generado) dan el mismo reporte.

| Pieza | Archivo |
| --- | --- |
| Emisor | `crates/codegen/src/c.rs` (`codegen::c::emit`) |
| Núcleo C (escrito a mano) | `crates/runtime-c/sv_nucleo.h`, `sv_nucleo.c` |
| CLI | `schemac compile <esquema> --lang c --out <dir>` |
| Pruebas | `crates/codegen/tests/c.rs` (F5.2), `crates/codegen/tests/diferencial.rs` (F5.3) |

## 1. Uso

```text
schemac compile pedido.xsd --lang c --out salida/
cd salida && cc -std=c11 -O2 -Wall -Wextra -o validador *.c
./validador < payload.xml   # la misma línea JSON y los mismos códigos 0/1/2/3 que el binario Rust
```

## 2. Qué se emite

Archivos planos `[a-z0-9_]+.c/.h`, los que admite el sandbox (D68):

| Archivo | Contenido |
| --- | --- |
| `main.c` | Lee stdin sin búfer (`setvbuf(_IONBF)`: la biblioteca no reserva el suyo) en bloques de 64 KiB. Usa un `sv_state_t` estático, escribe la línea JSON y sale con el código. |
| `validador.h` | La API C-ABI: `SV_FORMATO`, `SV_MAX_DEPTH`, `SV_STATE_WORDS`, `sv_state_t` (tamaño fijo), `sv_init`, `sv_feed`, `sv_finish`, `sv_report_json` y `sv_exit_code`. |
| `validador.c` | La fachada sobre `sv_tablas`, el escritor de la línea JSON (sin `printf`) y la palabra clave de cada regla. |
| `tablas.c` | `const sv_tables_t sv_tablas` y un arreglo por tabla (`NULL` si está vacía: C no admite arreglos de longitud 0). Se desestructura `Tables` de forma exhaustiva en el emisor, como en Rust. |
| `sv_lexico.h`, `sv_lexico.c` | Las tablas de los AFD UTF-8, JSON y XML **volcadas del crate `lexer`** (sus `TABLE`, `CLASS` y constantes `S_*`, `A_*`, `C_*` pasan a `pub`). Las constantes se leen de la fuente de Rust al emitir, así que el C usa los mismos números. |
| `sv_nucleo.h`, `sv_nucleo.c` | La copia de `crates/runtime-c`. |

**API** (plan general, «API C-ABI»; F4-U18):

```c
sv_state_t s;                      /* en la pila o estático: sin malloc */
sv_init(&s, NULL);                 /* NULL = límites por defecto, conformidad total */
while (hay_bloque) if (sv_feed(&s, p, n) < 0) break;   /* 0 sigue, -1 se detuvo */
sv_report_t r;
int codigo = sv_finish(&s, &r);    /* 0 válido, 1 inválido, 2 malformado o límite, 3 interno */
```

`sv_limits_t` lleva el modo y los límites de `runtime_rs::Options`. Con límites mayores que los de por defecto, lo
que no cabe en `SV_STATE_WORDS` es un error `limit`, nunca una escritura fuera de rango. Exponerlos con nombre y
documentarlos es F6.2.

**Sin cbindgen.** El plan preveía generar con cbindgen la cabecera de una C-ABI del validador *Rust*. Exponer
`extern "C"` con `#[no_mangle]` y punteros crudos exige `unsafe`, que está prohibido salvo la excepción de D52. El
validador C es C puro y su cabecera la escribe el emisor. Es una interfaz compartida (D42): queda pendiente del
visto bueno de Bayron.

## 3. El port: qué cambia respecto de Rust

Cada función de `core.rs`, `json.rs`, `xml.rs`, `num.rs`, `hash.rs` y de los lexers tiene su equivalente con el
mismo nombre y en el mismo orden. Donde Rust usa `get` (un índice que puede no existir), el C lo comprueba y da
un error interno. Hay dos diferencias, ninguna con efecto en el resultado:

- **`num` en `int64_t`.** Rust usa `i128`, con `exp_diff` saturado en ±2¹⁰⁰ y exacto hasta 31 cifras; el C usa
  ±2⁶² y 18 cifras. Del valor solo importan su signo y, cuando es pequeño, el valor exacto: si el resultado no es
  exacto, |valor| ≥ 10¹⁸ en los dos, muy por encima de los desplazamientos que se le suman (< 2³¹). El resto
  módulo 5²⁷ de `multipleOf` se calcula por duplicaciones para no pasar de 64 bits.
- **`uniqueItems` ordena con heapsort** (sin recursión) en lugar de `sort_unstable`. Solo se miran los iguales
  adyacentes, así que el orden no cambia nada.

## 4. Pruebas de F5.2 (`tests/c.rs`)

1. **El `main.c` real**, para cada uno de los 32 esquemas del corpus, con la orden exacta del sandbox y sin
   avisos. Cada payload por stdin da la línea JSON del intérprete, cumple `expected` y sale con su código. Con
   stdin ilegible da 3.
2. **Estricto:** gcc y clang, `-Werror -pedantic -Wshadow -Wconversion`, en todos los esquemas.
3. **Sin reservas por construcción:** los objetos del camino de validación (`validador.o`, `tablas.o`,
   `sv_nucleo.o`, `sv_lexico.o`) no tienen más símbolos externos que `memcmp`, `memcpy`, `memmove`, `memset` y
   `strlen` (`nm -u`). Un testigo con `malloc` muestra que la prueba lo vería.
4. **Salida reproducible**, también reconstruyendo M.
5. **Núcleo sincronizado:** la copia es idéntica a `crates/runtime-c`, y cada constante `SV_U8_*`, `SV_JS_*` y
   `SV_XL_*` que usa el núcleo está en `sv_lexico.h` con el valor de Rust.

## 5. La prueba diferencial (F5.3, `tests/diferencial.rs`)

- **Implementaciones:**
  - el intérprete (`automaton::validate_with`);
  - un banco Rust con las tablas y la fachada de cada esquema sobre una copia del runtime, compilado en *debug*
    (desbordamientos comprobados);
  - un banco C con las tablas de cada esquema (`sv_tablas_k`) sobre un solo núcleo, compilado con
    AddressSanitizer y UndefinedBehaviorSanitizer (`-fno-sanitize-recover=all`) si el compilador los tiene.
- **Entradas:**
  - los payloads del corpus y 10 mutaciones deterministas de cada uno (truncar, borrar, duplicar, cambiar un byte
    por uno de los que mueven a los lexers);
  - 12 esquemas numéricos sintéticos (cotas negativas, exclusivas, `multipleOf`, `enum`, `integer`, `xs:decimal`
    y `xs:double`) con arreglos de números generados: más de 18 y de 31 cifras, exponentes de más de 18 cifras y
    bordes exactos;
  - 2 esquemas de buena formación: claves repetidas con escapes, clave vacía, 1 024 y 1 025 claves, BOM,
    profundidad 256 y 257, espacios normalizados en atributos, `\r\n`, QName con dos `:`, referencias y 100 000
    y 100 001 referencias.
- **Comparación:** bloques de 1, 7 y 4 096 bytes y los dos modos. Los tres reportes deben ser idénticos campo a
  campo: válido, peso, clase y primer error con offset, línea, columna y regla.
- **Se valida con mutaciones:** se metieron defectos a propósito en `sv_nucleo.c`.
  - Aritmética: se detectaron las 8 que se pudieron aplicar.
  - Núcleo y lexers: 14 de 15. La que queda (seguir tras un repetido con el núcleo ya detenido) es casi
    equivalente.
  - Los casos sintéticos se añadieron precisamente por las mutaciones que sobrevivían al corpus solo.

Ninguna prueba mide tiempo (D1). Las de `c.rs` tardan unos 40 s y la diferencial unos 7 s.

## 6. Límites

- **Herramientas.** Las pruebas necesitan `cc` (y `nm`). Sin sanitizadores, el banco C corre sin ellos y lo
  avisa. En Windows sin `cc` fallarían (`docs/fase5/pendientes.md` §9).
- **Tamaño.** El núcleo C son unas 2 900 líneas. El pedido da 204 KB de fuentes y un binario de 130 KB, que
  compila con `-O2` en 1,3 s.
- **Fuera de esta tarea:** límites configurables con nombre (F6.2), puntero JSON (F6.1) y el fallo temprano en
  `main` (F7.5).
