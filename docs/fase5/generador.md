# Generador de payloads válidos desde la IR (F5.4, D66)

El enunciado pide un generador de payloads válidos e inválidos a partir de la IR, con semilla para que sea
reproducible. F5.4 hace la parte de los **válidos en JSON**. Los inválidos y malformados (mutaciones) van en F6.3.

```text
cargo run -p bench --release --bin generar                  # muestra 20 payloads
cargo run -p bench --release --bin generar -- --escribir    # los guarda en tests/cases/valid/
cargo run -p bench --release --bin generar -- --n 40 --semilla 7
```

Código: `crates/bench/src/generar.rs` (biblioteca) y `crates/bench/src/bin/generar.rs` (CLI).
Pruebas: `crates/bench/tests/generar.rs`.

## 1. Cómo genera

- **Azar con semilla.** Usa splitmix64 (`Azar`) y no depende de ningún crate. Con la misma semilla y la misma IR
  sale el mismo payload.
- **Recorrido de la IR.** Parte de `NodeId::ROOT`. En cada punto junta las reglas de los nodos que aplican:
  expande `$ref` y `allOf`, y de `anyOf`/`oneOf` elige una rama. Después construye el valor en este orden:
  1. `const`: copia el literal (`literal_json`, sin recursión).
  2. `enum`: elige uno de sus literales.
  3. `type`: la intersección de los tipos, prefiriendo el que piden las demás reglas.
  4. El valor según el tipo:
     - número: respeta `minimum`, `maximum`, las cotas exclusivas y `multipleOf`, que se escribe con los decimales
       de su lexema;
     - cadena: respeta `minLength`, `maxLength` y `pattern`;
     - arreglo: respeta `prefixItems`, `items`, `minItems`, `maxItems` y `uniqueItems`;
     - objeto: añade `required`, un subconjunto al azar de `properties`, la clausura de `dependentRequired`, y
       ajusta a `minProperties`/`maxProperties` según `additionalProperties`.
- **`pattern` por construcción.** La cadena sale de un paseo al azar por el autómata de Glushkov del patrón
  (`ir::regex::Regex::glushkov`): empieza en una posición de *first*, sigue *follow* y para en una posición de
  *last*. En cada posición elige un carácter de su clase, con preferencia por ASCII imprimible. Así la cadena casa
  con el patrón aunque este sea muy restrictivo (`^[A-Z]{3}-[0-9]{3}$`). Después se comprueban las longitudes y
  los demás patrones; si algo falla, se prueba otro paseo (hasta 400).
- **Profundidad.** Pasado el nivel `PROFUNDIDAD` (6), el generador prefiere valores escalares. Así no crece sin
  fin en esquemas recursivos (`arbol-recursivo`).

## 2. Garantía de validez

El generador no intenta cubrir a mano cada interacción entre reglas (`not`, `if/then/else`, `oneOf` con ramas
que se solapan…). `generar_valido` prueba semillas consecutivas y acepta un payload solo si lo dan por válido
**los dos validadores**: el DPDA (M de `automaton` sobre `runtime-rs`) y la línea base DOM (D48). Cada caso que
entra al corpus está, por tanto, comprobado por dos implementaciones independientes.

## 3. Los 20 casos del corpus

Son los `tests/cases/valid/valid-gen-*.json` (`phase: 5`), dos por esquema:

| Origen | Esquemas |
| --- | --- |
| Propios de la CLI (`EXTRA`) | `pedido` (dos `pattern`, `multipleOf` 0.01, `enum`), `dependencias` (`dependentRequired`), `alternativas` (`oneOf`), `tupla` (`prefixItems` + `items: false`), `conjunto` (`uniqueItems`), `combinados` (`allOf` + `anyOf` + `const`) |
| `tests/schemas/parse-ok/` | `anotaciones`, `arbol-recursivo`, `con-bom`, `objeto-cerrado` |

El generador salta algunos esquemas:

- `vacio`, `booleano-true` y `booleano-false`, porque son triviales.
- `combinadores`, `escalares`, `fuera-del-subconjunto` y `literales`, porque no tienen M (la Fase 3 los rechaza
  o M aún no los cubre).
- `limites-meta-esquema`, porque con 500 intentos no salió ningún payload válido.

Los casos pasan `validar_formato` y todas las pruebas de corpus del workspace (lexer en bloques de 1, 7 y
4096 bytes, semantic, automaton, baseline). Para regenerarlos, usar la orden con `--escribir`; la salida es
idéntica mientras no cambien la IR ni la semilla.

## 4. Límites

- **Solo JSON.** Con una IR de XSD, `generar` devuelve `None`. El generador XML queda abierto (§5).
- Un `const` o `enum` compuesto (objeto o arreglo) se copia bien, pero M todavía no lo cubre (F4.5b, recorte 2),
  así que esos esquemas no entran al corpus.
- La distribución no es uniforme: el generador busca casos válidos y variados, no hace muestreo estadístico.

## 5. Pendientes

- **F5.4-P1 (XML).** Generar documentos XML válidos desde la IR de un XSD: recorrer el Glushkov con contadores
  del modelo de contenido igual que el de `pattern`, más atributos y textos con su tipo. Va como criterio
  opcional de F6.3, o se recorta si no hay tiempo; el corpus ya tiene casos XML escritos a mano.
- **F5.4-P2.** F6.3 parte de estos válidos para mutarlos (inválidos y malformados).
