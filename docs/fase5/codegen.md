# Emisor Rust: el validador generado (F5.1, D74)

La Fase 5 traduce M (Fase 4) a código que valida sin el compilador. F5.1 emite **Rust**: las tablas densas de M
como `static` (D46 (1): solo tablas, sin saltos directos) delante del mismo núcleo `no_std` que usa el
intérprete de referencia (`runtime-rs`, F4-U9/D51) y del lexer de la Fase 1. No hay un segundo ejecutor escrito
a mano: el código que valida es el mismo que se prueba en la Fase 4, copiado. F5.2 hará lo mismo en C11.

| Pieza | Archivo |
| --- | --- |
| Emisor | `crates/codegen/src/rust.rs` (`codegen::rust::emit`) |
| Fuentes copiadas y su reescritura | `crates/codegen/src/fuentes.rs` |
| CLI | `schemac compile <esquema> --lang rust --out <dir>` (`crates/cli/src/main.rs`) |
| Pruebas | `crates/codegen/tests/rust.rs`, `crates/cli/tests/compile.rs` |

## 1. Uso

```text
schemac compile pedido.xsd --lang rust --out salida/
cd salida && rustc --edition 2021 -O main.rs
./main < payload.xml        # una línea JSON en stdout; código 0/1/2/3
```

Las opciones van en cualquier orden. `--lang c` responde «aún no está implementado (F5.2)» con el código 2.
Códigos de `schemac compile`: 0 con los archivos escritos (el directorio se crea si no existe; los archivos del
mismo nombre se sobrescriben y los demás no se tocan); 1 si el esquema tiene errores, una regla pendiente o un
límite (los de `schemac automaton`); 2 por argumentos, lectura del esquema o escritura de los archivos; 3 si hay un
error interno.

Desde Rust: `codegen::rust::emit(&automaton::build(&ir)?) -> Vec<codegen::Archivo { nombre, contenido }>`.

## 2. Qué se emite

Archivos planos con nombres `[a-z0-9_]+.rs`, sin subdirectorios (el tar del sandbox no los admite, D68):

| Archivo | Contenido |
| --- | --- |
| `main.rs` | Raíz del crate (`std`). Lee stdin en bloques de 64 KiB, los pasa al validador, escribe el reporte y sale con su código (§3). |
| `validador.rs` | Fachada, solo con `core`: `FORMATO`, `SV_MAX_DEPTH`, `STATE_WORDS`, `Validador` (`new(&mut [u64; STATE_WORDS], Mode)`, `feed`, `finish`), `validar(payload)`, `validar_por_bloques(payload, bloque, modo)`, `validar_en(mem, …)`, `escribir_json`, `codigo_de_salida`. |
| `tablas.rs` | `pub static TABLAS: Tables<'static>` y un `static` por tabla (`DELTA`, `TRANS`, `CHECKS`, `FRAMES`, …, `TEXT`), más `PALABRAS` (la palabra clave de cada regla, para el reporte). |
| `lexer.rs`, `lexer_*.rs` | Copia de `crates/lexer/src`. |
| `runtime_rs.rs`, `runtime_rs_*.rs` | Copia de `crates/runtime-rs/src`. |

**Tablas.** `emit` recorre `Automaton::tables()` y escribe cada campo de `runtime_rs::Tables` como un `static` de
literales: los `u32` de 16 en 16 (`u32::MAX` se escribe `X`), los bytes como `*b"…"` con escapes y las
transiciones y comprobaciones con dos `const fn` (`t(…)`, `c(…)`) para que el archivo no crezca con los nombres
de campo. Cada struct se desestructura de forma **exhaustiva** en el emisor: si `runtime-rs` añade un campo o una
variante, `codegen` deja de compilar en lugar de emitir tablas incompletas. El pedido (`valid-xml-pedido-01`:
216 estados, |Σ| = 61, 10 054 transiciones) da un `tablas.rs` de 83 KB; las fuentes completas ocupan unos 274 KB.

**Memoria fija.** `STATE_WORDS` = `Tables::state_words(&Limits::DEFAULT)`, calculado al emitir: zona del escalar,
`SV_MAX_DEPTH` (= 256, `Limits::DEFAULT.max_depth`, con un `const assert` en `validador.rs`) marcos del tipo más
grande y las reservas de claves (4 096) y de `uniqueItems` (4 096). En el pedido son 9 472 palabras (74 KiB).
`main` la toma de un `static Mutex<[u64; STATE_WORDS]>`: va en `.bss` (cero al cargar, sin reservas y fuera de
la pila del hilo principal); el `Mutex` existe solo porque un `static mut` exigiría `unsafe`. `validar` y
`validar_por_bloques` la ponen en la pila (para esquemas normales, decenas de KiB); con memoria propia,
`validar_en` o `Validador::new`.

**Clave de SipHash.** `main` valida con `Validador::con_clave` y una clave aleatoria por ejecución, la de
`RandomState` de `std` (§31 de `docs/fase6/pendientes.md`). `Validador::new` y `validar` usan la clave por
defecto de `LIMITES`, que es pública: quien use la biblioteca con payloads hostiles debería pasar la suya.

**Copia del runtime.** `fuentes.rs` incrusta los `.rs` de `lexer` y `runtime-rs` con `include_str!`, así que la
copia es siempre la del árbol con que se compiló `schemac`. Se reescriben solo tres cosas, de forma mecánica:
se quita `#![no_std]` (es un atributo de crate); cada `mod x;` de un `lib.rs` lleva delante
`#[path = "<módulo>_x.rs"]`; y las rutas `crate::…` pasan a `crate::lexer::…` o `crate::runtime_rs::…` (y en
`runtime-rs`, `lexer::…` pasa a `crate::lexer::…`). `main.rs` lleva `#![allow(dead_code, unused_imports)]`:
`dot.rs`, `short.rs` y el validador del otro formato se copian enteros aunque no se usen, para no editar la
copia; `rustc` los descarta.

**Determinismo.** La salida depende solo de M: se recorren las tablas en orden, sin mapas, fechas ni rutas del
anfitrión. Mismo esquema, mismos bytes, también reconstruyendo M.

## 3. Formato de la salida del binario

Una línea JSON en stdout, terminada en `\n`, con los campos del DPDA de `POST /api/compare` (D70) más `keyword`:

```json
{"valid":false,"violations":1,"errorKind":"schema","offset":64,"line":1,"column":65,"pointer":"/users/1","pointerTruncated":false,"rule":"#/properties/users/items","keyword":"required"}
```

| Campo | Valor |
| --- | --- |
| `valid` | bien formado y peso 0 |
| `violations` | peso del recorrido en conformidad total; `null` si es ∞ (malformado o límite) |
| `errorKind` | `lexical`, `syntax`, `schema`, `limit`, `internal` o `null` |
| `offset`, `line`, `column` | dónde se detectó el primer error (como `Report::first`), o `null` |
| `pointer`, `pointerTruncated` | la instancia del primer error (JSON Pointer, o ruta de elementos en XML) y si se recortó a 1 024 bytes; `null` y `false` sin error (F6.1, `docs/fase6/puntero.md`) |
| `rule` | ubicación recortada de la regla en el esquema (`#/…`, ≤ 256 bytes), o `null` en un error de peso ∞ |
| `keyword` | palabra clave de la regla (`required`, `maximum`, …), o `null` |

`rule` y `keyword` se escapan como cadenas JSON (`"`, `\`, controles; un byte no UTF-8 sale como U+FFFD), porque
la ubicación puede contener nombres de propiedades del esquema.

**Códigos de salida:**

| Código | Significado |
| --- | --- |
| 0 | válido |
| 1 | inválido: bien formado con violaciones (`schema`) |
| 2 | malformado (`lexical`, `syntax`) o un límite (`limit`) |
| 3 | error interno: tablas incoherentes (`internal`), o no se pudo leer stdin o escribir stdout (como el 3 de `schemac`, D33); el motivo de E/S va a stderr y stdout queda vacío |

Ninguno choca con los que reserva el sandbox (100-103, 124-127; `docs/fase7/sandbox.md` §3).

## 4. Qué garantiza y cómo se prueba

Las pruebas compilan de verdad lo emitido con el `rustc` del repositorio, en `CARGO_TARGET_TMPDIR`. Ninguna mide
tiempo (D1); las cinco tardan unos 12 s.

1. **Equivalencia con el intérprete** (`banco_equivale_al_interprete`). Un solo `rustc` en modo *debug*, con los
   desbordamientos aritméticos comprobados, compila un banco con las tablas y la fachada de **cada** esquema
   distinto del corpus (32) sobre una copia común del runtime. Para cada payload del corpus, en bloques de 1, 7 y
   4 096 bytes y en los dos modos, el `{:?}` del `Report` es idéntico al de `automaton::validate_with`. También
   comprueba la línea JSON y el código de salida, y que la copia del runtime sea la misma para todos los
   esquemas. Se eligió un banco porque un `rustc` por esquema en debug costaría más, e `include!` exigiría un
   `build.rs` o archivos generados versionados.
2. **El `main.rs` real con la orden del sandbox** (`main_real_en_todo_el_corpus`). Para cada esquema del corpus,
   `rustc --edition 2021 -O main.rs`, sin ningún aviso, con los nombres que exige el sandbox. Cada payload entra
   por stdin, y la línea JSON es igual a la del intérprete y cumple el `expected` del caso (`valid`, `errorKind`,
   `violations`, `offset`). Salen los códigos 0, 1 y 2, este último también por un `limit`. Con stdin ilegible (un
   directorio) sale 3. Es la evidencia de PC-E.
3. **Salida reproducible** (`salida_reproducible`): dos emisiones, y una tercera sobre M reconstruida, dan los
   mismos bytes.
4. **Sin reservas, por construcción** (`nucleo_sin_std_ni_alloc`). `validador.rs`, `tablas.rs` y la copia del
   runtime compilan como un crate `#![no_std]` sin `alloc` (`rustc --crate-type rlib`), en JSON y en XML. Ese
   código no tiene asignador al que llamar, lo que es más fuerte que contar reservas en unos casos. Un testigo con
   un `Vec` no compila, así que la prueba distingue. Fuera del camino de validación, `main` usa `std`: el búfer
   interno de `Stdin` (una reserva de 8 KiB al primer uso) y la cadena de la línea JSON, que se arma **después**
   de `finish`. El contador de `crates/automaton/tests/sin_reservas.rs` (la única excepción `unsafe`, D52) no se
   reutilizó: `codegen` depende de `automaton` y la dependencia inversa sería un ciclo. Además, contar dentro del
   binario generado exigiría un segundo `unsafe`.
5. **Vigía de la copia** (`copia_sincronizada`): los archivos incrustados son exactamente los `.rs` de
   `crates/lexer/src` y `crates/runtime-rs/src`, con el mismo contenido; cada `mod` de sus `lib.rs` lleva su
   `#[path]` hacia un archivo emitido y no queda ningún `#![no_std]`. Un módulo nuevo en cualquiera de los dos
   crates hace fallar la prueba hasta añadirlo a `fuentes.rs`.
6. **CLI** (`crates/cli/tests/compile.rs`): JSON y XSD escriben los archivos (con las opciones en otro orden, los
   mismos bytes); `--lang c` da 2 con «F5.2»; argumentos inválidos dan 2; un esquema con errores da 1 sin
   escribir nada; una salida que no es un directorio da 2.

Las invariantes de la Fase 4 se heredan del núcleo copiado: O(n) en bytes, O(d) en pila, sin recursión, el mismo
resultado con cualquier partición en bloques (`docs/fase4/runtime.md`).

## 5. Límites

- **Límites.** Por defecto son los de `Limits::DEFAULT` (`docs/00-alcance.md` §4), incluidos en `STATE_WORDS` y
  `SV_MAX_DEPTH`. Desde F6.2 se cambian al emitir con `schemac compile --limit nombre=valor`, también el tiempo
  máximo (`docs/fase6/limites.md`); el binario no lee opciones.
- **Solo conformidad total en `main`.** El fallo temprano está en la fachada (`Mode::EarlyFail`), pero `main` no lo
  expone.
- **Memoria.** `STATE_WORDS` crece con el marco más grande × 256. Con el tope del sandbox (512 MiB) sobra para
  cualquier esquema del corpus (74 KiB en el pedido); un esquema con marcos de miles de palabras pediría decenas
  de MiB en `.bss`. `validar`/`validar_por_bloques` ponen esa memoria en la pila: con `STATE_WORDS` grande, usar
  `validar_en` o `Validador::new`.
- **Tamaño de la copia.** Se copian enteros los dos crates (unos 6 000 líneas) aunque el binario use una parte.
  Compilar con `-O` tarda alrededor de 1 s; el binario sin `strip` ocupa unos 4,6 MB.
- **El puntero del error** (`pointer`, `pointerTruncated`) está en la línea desde F6.1 (`docs/fase6/puntero.md`).

## 6. Cómo lo usará el sandbox (F7.5)

`server::sandbox::ejecutar` (F7.3, D68) recibe `Lenguaje::Rust` y la lista de archivos. Con este emisor:

```rust
let archivos = codegen::rust::emit(&automaton::build(&ir)?);
let pares: Vec<(&str, &[u8])> = archivos.iter()
    .map(|a| (a.nombre.as_str(), a.contenido.as_bytes()))
    .collect();
// Trabajo { lenguaje: Lenguaje::Rust, archivos: &pares, payload }
```

- Los nombres ya cumplen `comprobar` (`[a-z0-9_]+.rs`, con `main.rs`, sin repetidos) y la orden fija de la imagen
  (`rustc --edition 2021 -O -o /tmp/out/validador /tmp/src/main.rs`) los compila sin Cargo ni red. Esto cierra
  F7.3-P2 (`docs/fase7/pendientes.md` §14): no hace falta copiar fuentes en la imagen.
- Con `Resultado::Terminado { codigo, stdout, .. }`, `codigo` es 0/1/2/3 (§3) y `stdout` es una sola línea JSON
  con los campos del DPDA de `/api/compare`, así que la web puede mostrarla igual.
- Unos 274 KB de fuentes por trabajo: muy por debajo del tope de 16 MiB de la entrada.
- La caché del binario por esquema (F7.3-P1) sigue siendo de F7.5; el binario no depende del payload.
