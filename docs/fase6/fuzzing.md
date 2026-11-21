# Fuzzing con cargo-fuzz (F6.4, D79)

`fuzz/` es un crate aparte (con su propio `[workspace]`), porque libFuzzer necesita nightly y `make test` no debe
pedirlo. Se corre a mano, no en `make test`:

```text
make fuzz            # 60 s por objetivo (SEG=300 make fuzz para más)
scripts/fuzz.sh 300  # lo mismo
```

Necesita `rustup toolchain install nightly` y `cargo install cargo-fuzz`. El script genera las semillas, corre los
cuatro objetivos y pasa lo encontrado por la prueba diferencial con el C.

## 1. Objetivos

| Objetivo | Entrada | Qué comprueba |
| --- | --- | --- |
| `validar_json` | un payload | Con 5 esquemas fijos que cubren todas las estrategias (`$ref` recursivo, números con `multipleOf`, `pattern`, `enum`, `uniqueItems`, `dependentRequired`, `additionalProperties: false`, `anyOf`/`oneOf`/`not`/`allOf`, `prefixItems`): las propiedades de §2 y, contra la **línea base** `serde_json` + `jsonschema` (D48), el mismo veredicto y un puntero que está entre los suyos o debajo de uno. Se excluyen los `limit` y lo que la línea base no puede decidir (`Unsupported`). |
| `validar_xml` | un payload | Con el pedido y un XSD con `sequence`, `choice`, `all`, contadores, atributos con `pattern` y `fixed`: las propiedades de §2. libxml2 no entra (feature y Docker, F6.7b). |
| `esquema_json` | un JSON Schema | Las Fases 2-4 no entran en pánico ni desbordan la pila (hilo de 8 MiB, D7). Si construye M, M cumple §2 con 10 payloads fijos. |
| `esquema_xsd` | un XSD | Lo mismo con el frontal XSD. |

Las semillas (`scripts/fuzz_semillas.py`) son los payloads del corpus y los esquemas de `tests/schemas`. El corpus
que crece y los fallos van a `fuzz/corpus/` y `fuzz/artifacts/` (no se versionan).

## 2. Propiedades de todo reporte (`fuzz/comun/mod.rs`)

- El mismo reporte en bloques de 1, 7 y 4 096 bytes (determinismo).
- Nunca `internal`. `valid` ⇔ sin primer error ⇔ sin clase de error.
- El puntero (F6.1) cabe en `POINTER_MAX` y es vacío o empieza por `/`.
- El fallo temprano da el mismo veredicto. Sin error de peso ∞, la misma primera violación con peso 1 y el mismo
  puntero. Con uno, o el mismo reporte o una violación anterior a él (F6.3-P1, opción a).

## 3. El C

El núcleo C no se puede enlazar con libFuzzer sin FFI, y FFI exige `unsafe` (prohibido salvo D52). Por eso
`scripts/fuzz.sh` termina pasando el corpus de `validar_json` y `validar_xml` por la prueba diferencial de F5.3:

```text
SV_EXTRA_JSON=fuzz/corpus/validar_json SV_EXTRA_XML=fuzz/corpus/validar_xml \
    cargo test -p codegen --test diferencial
```

Cada payload (como mucho 1 000 por formato, de hasta 64 KiB) se valida con **cada** esquema del corpus de su
formato, en el intérprete, el Rust generado y el C generado con ASan/UBSan, en bloques de 1, 7 y 4 096 y los dos
modos. Sin las variables, la prueba es la de siempre.

## 4. Resultado de la primera corrida (21 nov)

| Objetivo | Tiempo | Ejecuciones | Corpus final | Fallos |
| --- | --- | --- | --- | --- |
| `validar_json` | 120 s | 6 393 | 340 | 0 |
| `validar_xml` | 100 s | 21 504 | 195 | 0 |
| `esquema_json` | 100 s | 86 726 | 858 | 0 |
| `esquema_xsd` | 100 s | 56 417 | 841 | 0 |

La reproducción en el C de los 340 + 195 payloads con cada esquema de su formato dio el mismo reporte en las tres
implementaciones, sin avisos de los sanitizadores.

Es una **prueba de humo**, no una campaña: unos minutos en total, y `validar_json` solo hizo 6 393 ejecuciones.

**Segunda corrida (21 nov, `SEG=600 make fuzz`, durante E5 y E4):** 600 s por objetivo, sin fallos.

| Objetivo | Ejecuciones | Corpus final |
| --- | ---: | ---: |
| `validar_json` | 55 669 | 1 060 |
| `validar_xml` | 132 773 | 724 |
| `esquema_json` | 226 223 | 2 412 |
| `esquema_xsd` | 221 592 | 2 345 |

La reproducción en el C (como mucho 1 000 payloads por formato) dio el mismo reporte en las tres implementaciones.

**Tercera corrida (21 nov, E3, `SEG=120 make fuzz`)**, tras los cambios de E3 en el núcleo C (`number_hash`, E3A-1)
y en la construcción (E3B-1 a E3B-3): sin fallos, con 13 289, 14 939, 17 078 y 38 093 ejecuciones y corpus de
1 242, 787, 2 519 y 2 632 entradas; la reproducción en el C, igual en las tres implementaciones. Conviene repetirla
tras cada cambio del núcleo (`docs/fase6/pendientes.md` §32). El script sigue con los demás
objetivos si uno falla y sale con 1 al final, con el fallo en `fuzz/artifacts/`.

Los dos fallos que salieron al montar los objetivos eran de las propias pruebas: un esquema fijo con
`propertyNames` (fuera del subconjunto) y una propiedad del fallo temprano mal escrita (exigía la misma clase
cuando una violación precede a un error léxico, que es F6.3-P1). `validar_json` es el más lento (≈ 50 por
segundo), porque compara con la línea base cinco esquemas por entrada y valida en bloques de 1 byte.
