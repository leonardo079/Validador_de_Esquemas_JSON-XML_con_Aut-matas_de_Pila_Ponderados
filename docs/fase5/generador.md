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

- Con una IR de XSD, `generar` delega en el generador XML (§6).
- Un `const` o `enum` compuesto (objeto o arreglo) se copia bien, pero M todavía no lo cubre (F4.5b, recorte 2),
  así que esos esquemas no entran al corpus.
- La distribución no es uniforme: el generador busca casos válidos y variados, no hace muestreo estadístico.

## 5. Pendientes

- **F5.4-P1 (XML).** Resuelto el 22 nov: §6.
- **F5.4-P2.** F6.3 parte de estos válidos para mutarlos (inválidos y malformados).

## 6. Documentos XML: válidos, inválidos y malformados (F5.4-P1, 22 nov)

El enunciado pide un «generador de payloads válidos e inválidos (serialización JSON/XML) a partir del esquema».
[`crates/bench/src/generar_xml.rs`](../../crates/bench/src/generar_xml.rs) hace la parte XML con la IR de un XSD:

- **Válidos** (`generar_xml`). El documento elige un elemento global. Cada elemento escribe sus atributos (los
  requeridos siempre, los opcionales a veces, el valor `fixed` si lo hay) y después su contenido:
  - `xs:sequence` en orden, `xs:choice` con una rama y `xs:all` en un orden al azar;
  - cada partícula entre `minOccurs` y `minOccurs` + 2 veces;
  - o el texto de su `xs:simpleContent`.

  Los textos salen del tipo predefinido, con sus facetas, con las rutinas de números y cadenas de §1 y `pattern`
  en el dialecto XSD. Una enumeración solo se elige si cumple las demás facetas de la cadena de restricciones:
  en `tipos-simples.xsd`, `PocoPorcentaje` hereda {0, 5.50, 10, 50} y la acota a ≤ 10. La primera versión no lo
  miraba y el DPDA rechazó, con razón, los documentos con 50.
- **Inválidos** (`generar_xml_invalido`). La misma generación con un sabotaje en una de sus oportunidades,
  elegida con la semilla: un hijo obligatorio de menos, un atributo requerido de menos o un texto fuera de su
  tipo. El documento queda bien formado.
- **Malformados** (`malformar`). Cinco mutaciones de texto de un válido:
  - truncarlo (`lexical` si corta un token, `syntax` si no);
  - cambiar el nombre de la última etiqueta de cierre (`syntax`);
  - repetir un atributo (`syntax`);
  - una entidad no declarada (`lexical`);
  - un `<!DOCTYPE` delante (`lexical`).

La recursión está acotada: 18 elementos anidados, 64 grupos y 400 elementos por documento (D7). Un tipo
recursivo con un hijo obligatorio no tiene documento finito y da `None`.

**Dos oráculos independientes.** El generador sigue la semántica del XSD, no el autómata.
[`crates/bench/tests/generar_xml.rs`](../../crates/bench/tests/generar_xml.rs), con 40 semillas en cada XSD de
`tests/schemas/xsd/parse-ok`, comprueba:

- el DPDA acepta **todo** documento generado como válido. De los 14 XSD distintos, 12 dan documentos; los otros
  dos no tienen elementos globales. Las copias de los casos del corpus se cuentan una vez;
- los inválidos nunca salen malformados, y el DPDA los rechaza como `schema` en 10 de los 14 esquemas. Un
  sabotaje puede no romper nada, por ejemplo omitir la rama de un `xs:choice` que tiene otra;
- cada malformación da su clase de error;
- el generador es determinista con la semilla, y el árbol recursivo llega a cuatro niveles de profundidad o
  más;
- da `None` sin colgarse cuando no hay documento finito (un tipo con un hijo obligatorio de su mismo tipo) o
  cuando costaría demasiado (un `minOccurs` de 4·10⁹);
- con dos `xs:enumeration` en la cadena de restricciones, solo elige de su intersección.

La revisión de la tarea encontró y se corrigieron:
- un sabotaje que caía en una rama de `xs:choice` que luego fallaba: la etiqueta decía que estaba, pero el
  documento no lo llevaba;
- la primera enumeración de la cadena se tomaba sin cruzarla con las demás;
- un `minOccurs` enorme en un grupo opcional no se cortaba;
- un atributo opcional imposible de generar abortaba todo el elemento;
- el DOCTYPE delante de la declaración y la entidad tras una raíz cerrada sola probaban otra cosa;
- los blancos de control en los atributos no se escapaban;
- la partícula raíz contaba como oportunidad de sabotaje y dejaba el documento vacío.

Con la feature `libxml2`, en Docker, `libxml2_da_el_mismo_veredicto` exige que libxml2 2.9.14 dé el mismo
veredicto que el DPDA en todos los documentos generados (válidos, inválidos y malformados) de cada XSD que
compila: más de 500 documentos, todos de acuerdo.

```text
make test-baseline-xml                                          # imagen y pruebas de la línea base XML
docker run --rm -v "$PWD":/src:ro -v validador-cargo-registry:/usr/local/cargo/registry \
  -v validador-baseline-xml-target:/target -w /src --network none validador-baseline-xml:1 \
  cargo test -p bench --features libxml2 --test generar_xml --locked --offline
```

**Corpus.** `cargo run -p bench --release --bin generar_xml -- --escribir` escribe seis casos para el pedido y
el árbol recursivo: `valid-gen-xml-*`, `invalid-mut-xml-*` y `malformed-xml-mut-*`, con la copia de su XSD en
`parse-ok`. El `expected` fija solo lo que comparten el DPDA y libxml2:

- el veredicto, la clase y el puntero;
- en los inválidos, una violación. Se elige un sabotaje con una sola violación, porque con un hijo de menos
  libxml2 se detiene en el primer error del modelo y cuenta menos que la conformidad total.

El offset de los malformados no se fija, porque libxml2 da el final de la etiqueta y no el principio.
`make test-baseline-xml` pasa con ellos.

