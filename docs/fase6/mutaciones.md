# Payloads inválidos y malformados por mutación (F6.3, D67)

El enunciado pide generar payloads válidos e inválidos a partir de la IR, con semilla. F5.4 genera los
válidos ([../fase5/generador.md](../fase5/generador.md)). F6.3 los **muta** para obtener casos `invalid`
(bien formados pero que violan el esquema) y `malformed` (no son JSON).

```text
cargo run -p bench --release --bin mutar                    # muestra 20 inválidos y 10 malformados
cargo run -p bench --release --bin mutar -- --escribir      # los guarda en tests/cases/
cargo run -p bench --release --bin mutar -- --invalidos 30 --malformados 15 --semilla 7
```

Código: `crates/bench/src/mutar.rs` (biblioteca) y `crates/bench/src/bin/mutar.rs` (CLI).
Pruebas: `crates/bench/tests/mutar.rs`.

## 1. Tramos

`tramos(p)` recorre el payload válido sin recursión y devuelve cada valor con su rango de bytes, su clase
(objeto, arreglo, cadena, número o literal), dónde empieza su clave si es un miembro y si es elemento de un
arreglo. Las mutaciones eligen un tramo al azar, con `Azar`, el splitmix64 de F5.4. Con el mismo payload y la
misma semilla, la mutación es siempre la misma.

## 2. Mutaciones

| Semánticas (dejan JSON bien formado) | Qué rompen típicamente |
| --- | --- |
| `tipo`: cambia un valor por otro de otra clase | `type`, `oneOf`, `items` |
| `rango`: cambia un número por −10⁶, 10⁶, 0.5, 7.001 o −1 | `minimum`/`maximum`, `integer`, `multipleOf` |
| `cadena`: cambia una cadena por `""`, 64 `z`, `"!?"` o `"ab12"` | `minLength`/`maxLength`, `pattern`, `enum` |
| `sin-miembro`: borra un miembro con su coma | `required`, `dependentRequired`, `minProperties` |
| `miembro-extra`: añade `"zz_extra": 1` | `additionalProperties`, `maxProperties` |
| `elementos`: duplica o quita un elemento | `uniqueItems`, `minItems`/`maxItems`, `prefixItems` + `items: false` |

| Sintácticas y léxicas | Error esperado |
| --- | --- |
| `truncado`: corta el payload | `syntax` al final, o `lexical` si queda una cadena abierta |
| `coma`: añade una coma colgante antes de `}` o `]` | `syntax` |
| `cero-inicial`: cambia un número por `012` | `lexical` |
| `escape`: añade `\q` al inicio de una cadena | `lexical` |
| `literal`: cambia un escalar por `ture`, `nul`, `fals`, `+1` o `1.` | `lexical` |
| `clave-duplicada`: repite un miembro | `syntax` (D2) |
| `utf8`: añade el byte 0xFF dentro de una cadena | `lexical` (el caso se escribe con `payloadBase64`) |

## 3. Oráculo doble

`clasificar` fija el `expected` del caso con el reporte del DPDA en modo total (`violations`, `errorKind`,
`offset` del primer error). Solo acepta el caso si se cumplen todas estas condiciones:

- la línea base DOM (D48) da la misma clase (`schema` o malformado);
- si la clase es `schema`, la línea base cuenta el mismo número de violaciones, sin recorte;
- si es malformado, la línea base da el mismo offset;
- el DPDA da el mismo reporte en bloques de 1, 7 y 4096 bytes;
- el DPDA da el mismo veredicto en fallo temprano (con `schema`, peso 1 y el mismo primer error);
- si es malformado, el resultado se repite con el esquema `true`, porque un error léxico o sintáctico no
  depende del esquema.

Estas son las mismas condiciones que `crates/automaton/tests/corpus.rs`, `crates/baseline/tests/corpus_json.rs`
y las pruebas del lexer exigen a todo caso del corpus. Por eso los 30 casos entran sin tocar esas pruebas.

## 4. Los 30 casos

La CLI recorre los `valid` JSON del corpus que tienen M en dos rondas. En cada ronda saca como mucho un inválido
por fuente; los malformados son uno por fuente y solo en la primera ronda. Las mutaciones rotan de una fuente a
otra para que salgan variadas.

- **20 `invalid-mut-*`**, en `tests/cases/invalid/`. Cubren `type`, `required`, `additionalProperties`,
  `minItems`, `maxItems`, `uniqueItems`, `minimum`/`maximum` y `oneOf`, de 1 o 2 violaciones.
- **10 `malformed-json-mut-*`**, en `tests/cases/malformed/`. Cubren `truncado`, `coma`, `escape`, `literal`,
  `clave-duplicada` y `cero-inicial`, con errores léxicos y sintácticos.

Todos llevan `phase: 6` y en la descripción la mutación, la fuente y la semilla. Para regenerarlos, usar la
orden con `--escribir`; la salida es idéntica byte a byte.

## 5. Lo que encontró

- **F6.3-P1** (`pendientes.md` §28): con `{"type":"string"}` y el payload `1.`, el fallo temprano da `schema` y
  el modo total da `lexical`. No es un fallo del validador, porque el fallo temprano para en la primera
  violación. Pero contradice la redacción del criterio 6 de F4.6. Hoy `clasificar` descarta esos payloads, y
  lo decide la pista A.

## 6. Límites

- Solo JSON, como F5.4: el generador XML es F5.4-P1.
- Los casos en que los dos validadores no coinciden se descartan en silencio. Esas divergencias conocidas están
  en `linea-base.md`.
