# Fase 6 — Arnés de métricas: DPDA frente a la línea base DOM (F6.5, D65)

Código: `crates/bench` (`src/lib.rs`, binario `src/bin/metricas.rs`, pruebas
`tests/arnes.rs`). Compara el autómata de pila de la Fase 4 (M de `automaton`,
ejecutado por el núcleo `no_std` de `runtime-rs`) con la línea base DOM de F6.7
(`serde_json` + `jsonschema`, D48), como pide el «Terminado cuando» de la Fase 6:
«el arnés produce una tabla comparativa DPDA vs línea base para todo el corpus».

## 1. Cómo se ejecuta

```sh
cargo run -p bench --release --bin metricas                       # corpus + bombeo
cargo run -p bench --release --bin metricas -- --corpus           # solo el corpus
cargo run -p bench --release --bin metricas -- --bombeo --csv b.csv
cargo run -p bench --release --bin metricas -- --repeticiones 21  # 11 por defecto
```

Imprime una tabla Markdown (la de §4) y, con `--csv`, un CSV para graficar (E1).
Las mediciones de tiempo viven aquí y no en `cargo test` (D1); las pruebas de
`tests/arnes.rs` solo comprueban, sin reloj, que lo medido es correcto.

## 2. Qué mide y cómo

| Columna | Cómo se obtiene |
| --- | --- |
| Bytes, Prof. | Tamaño del payload y anidamiento máximo (`bench::profundidad`, una pasada sin recursión; salta cadenas JSON y comentarios, CDATA y PI de XML). |
| Resultado | Veredicto y violaciones: peso tropical de M (D4) y errores de `jsonschema` en la línea base; «malformado o límite» = peso ∞. |
| MB/s, ns/byte | Mediana de `--repeticiones` validaciones (tras una de calentamiento), con `Instant`. Solo la validación: el esquema se compila antes (también M). **Ciclos/byte** = ns/byte × GHz de la máquina; no se leen contadores de ciclos (exigiría `unsafe`, D52). |
| Asignaciones, pico de memoria | Una validación aparte con `dhat` como asignador global del binario (modo `testing`, sin archivos): bloques reservados y pico de montón. En las pasadas de tiempo no hay perfilador activo y su coste es mínimo. |
| Estados, transiciones | `Automaton::summary()` (F4-U7): \|Q\| y entradas de δ. |

- **DPDA:** `bench::validar_dpda` reserva la pila de marcos con el tamaño de la
  memoria **fija** que dan los límites por defecto (`Automaton::state_words`, F4-U18) y alimenta
  `JsonValidator`/`XmlValidator` en bloques de 4 KiB (`sv_feed`). Esa pila es
  **toda** la memoria del DPDA: una sola asignación, de tamaño fijo, independiente de la
  longitud y de la profundidad. No construye árbol (invariante del proyecto).
- **Línea base:** `JsonBaseline::validate` (DOM completo y `jsonschema`), dentro de
  `with_stack` (D7). Solo JSON: la de XML necesita la feature `libxml2` y Docker
  (D54), y la memoria de libxml2 (malloc de C) no la vería `dhat`
  (`docs/fase6/pendientes.md` §2).
- **Corpus:** todos los casos de `tests/cases/` con payload concreto (todos tienen M
  desde la Fase 4, también los `non-dpda`).
- **Bombeo** (`bench::bombeo`, plan general «Pruebas de bombeo, cómo hacerlas»):
  - `[`ᵏ`]`ᵏ (u·vᵏ·x·yᵏ·z con v = `[`, y = `]`) con el esquema recursivo de
    arreglos de arreglos, k = 1, 10, 100, 255 (el tope es 256, D7);
  - arreglos de n enteros, n = 10³ … 10⁶ (profundidad fija 1);
  - arreglos de n objetos `{"a":1}` con `required` y `additionalProperties: false`,
    n = 10³ … 10⁵ (profundidad 2).

## 3. Corrección de lo medido (pruebas, sin reloj)

`tests/arnes.rs`: profundidad JSON y XML; los generadores producen lo que dicen;
el DPDA y la línea base aceptan todas las familias de bombeo y cuentan la misma
violación en un objeto sin `a`; en cada caso del corpus con M, el DPDA da el
`valid` esperado y coincide con la línea base; formato de la tabla y del CSV.

## 4. Resultados (20 nov 2026, tras la Fase 4 de `38a8678`)

Windows 11, release, 11 repeticiones, sin carga controlada. Los tiempos de los
payloads de pocos bytes están dominados por el arranque de cada validación (poner
a cero el estado del ejecutor) y por la resolución del reloj: no son significativos.

| Caso | Validador | Bytes | Prof. | Resultado | MB/s | ns/byte | Asignaciones | Pico de memoria | Estados | Transiciones |
| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| extreme-big-number-01 | DPDA | 1025 | 0 | malformado o límite | 22.1 | 45.27 | 1 | 72.0 KiB | 14 | 45 |
| extreme-big-number-01 | línea base DOM | 1025 | 0 | malformado o límite | 320.3 | 3.12 | 13 | 1.4 KiB | — | — |
| extreme-depth-01 | DPDA | 514 | 257 | malformado o límite | 76.7 | 13.04 | 1 | 72.0 KiB | 13 | 43 |
| extreme-depth-01 | línea base DOM | 514 | 257 | malformado o límite | 76.7 | 13.04 | 8 | 262 B | — | — |
| invalid-additional-properties-01 | DPDA | 18 | 1 | 1 violaciones | 0.6 | 1638.89 | 1 | 74.0 KiB | 23 | 75 |
| invalid-additional-properties-01 | línea base DOM | 18 | 1 | 1 violaciones | 15.0 | 66.67 | 13 | 1.2 KiB | — | — |
| invalid-enum-01 | DPDA | 17 | 1 | 1 violaciones | 0.6 | 1782.35 | 1 | 74.0 KiB | 38 | 260 |
| invalid-enum-01 | línea base DOM | 17 | 1 | 1 violaciones | 13.1 | 76.47 | 13 | 1.0 KiB | — | — |
| invalid-missing-required-01 | DPDA | 67 | 3 | 1 violaciones | 24.8 | 40.30 | 1 | 74.0 KiB | 50 | 326 |
| invalid-missing-required-01 | línea base DOM | 67 | 3 | 1 violaciones | 33.5 | 29.85 | 21 | 2.4 KiB | — | — |
| invalid-pattern-01 | DPDA | 21 | 1 | 1 violaciones | 12.4 | 80.95 | 1 | 74.0 KiB | 38 | 260 |
| invalid-pattern-01 | línea base DOM | 21 | 1 | 1 violaciones | 16.2 | 61.90 | 15 | 1.1 KiB | — | — |
| invalid-range-01 | DPDA | 12 | 1 | 2 violaciones | 0.4 | 2416.67 | 1 | 72.0 KiB | 17 | 62 |
| invalid-range-01 | línea base DOM | 12 | 1 | 2 violaciones | 7.5 | 133.33 | 16 | 918 B | — | — |
| invalid-type-01 | DPDA | 16 | 1 | 1 violaciones | 10.0 | 100.00 | 1 | 74.0 KiB | 27 | 102 |
| invalid-type-01 | línea base DOM | 16 | 1 | 1 violaciones | 12.3 | 81.25 | 13 | 1.0 KiB | — | — |
| invalid-unique-items-01 | DPDA | 11 | 1 | 1 violaciones | 0.4 | 2800.00 | 1 | 72.0 KiB | 16 | 60 |
| invalid-unique-items-01 | línea base DOM | 11 | 1 | 1 violaciones | 10.0 | 100.00 | 9 | 524 B | — | — |
| invalid-unique-items-02 | DPDA | 36 | 2 | 1 violaciones | 15.7 | 63.89 | 1 | 72.0 KiB | 16 | 60 |
| invalid-unique-items-02 | línea base DOM | 36 | 2 | 1 violaciones | 21.2 | 47.22 | 16 | 1.8 KiB | — | — |
| invalid-xml-missing-child-01 | DPDA | 39 | 2 | 1 violaciones | 17.7 | 56.41 | 1 | 72.0 KiB | 174 | 8218 |
| invalid-xml-range-01 | DPDA | 55 | 2 | 1 violaciones | 20.4 | 49.09 | 1 | 72.0 KiB | 174 | 8218 |
| malformed-json-duplicate-key-01 | DPDA | 24 | 1 | malformado o límite | 0.8 | 1254.17 | 1 | 74.0 KiB | 24 | 77 |
| malformed-json-duplicate-key-01 | línea base DOM | 24 | 1 | malformado o límite | 18.5 | 54.17 | 15 | 701 B | — | — |
| malformed-json-invalid-utf8-01 | DPDA | 10 | 1 | malformado o límite | 7.1 | 140.00 | 1 | 74.0 KiB | 23 | 75 |
| malformed-json-invalid-utf8-01 | línea base DOM | 10 | 1 | malformado o límite | 16.7 | 60.00 | 6 | 152 B | — | — |
| malformed-json-leading-zero-01 | DPDA | 12 | 1 | malformado o límite | 8.0 | 125.00 | 1 | 74.0 KiB | 27 | 102 |
| malformed-json-leading-zero-01 | línea base DOM | 12 | 1 | malformado o límite | 17.1 | 58.33 | 8 | 144 B | — | — |
| malformed-json-syntax-01 | DPDA | 7 | 1 | malformado o límite | 5.4 | 185.71 | 1 | 72.0 KiB | 13 | 43 |
| malformed-json-syntax-01 | línea base DOM | 7 | 1 | malformado o límite | 10.0 | 100.00 | 8 | 134 B | — | — |
| malformed-xml-billion-laughs-01 | DPDA | 153 | 1 | malformado o límite | 90.0 | 11.11 | 1 | 72.0 KiB | 108 | 4823 |
| malformed-xml-unclosed-01 | DPDA | 44 | 2 | malformado o límite | 18.3 | 54.55 | 1 | 72.0 KiB | 174 | 8218 |
| non-dpda-allof-not-01 | DPDA | 8 | 1 | válido | 5.3 | 187.50 | 1 | 76.0 KiB | 23 | 75 |
| non-dpda-allof-not-01 | línea base DOM | 8 | 1 | válido | 20.0 | 50.00 | 3 | 665 B | — | — |
| non-dpda-anyof-ambiguous-01 | DPDA | 8 | 1 | válido | 5.3 | 187.50 | 1 | 78.0 KiB | 26 | 92 |
| non-dpda-anyof-ambiguous-01 | línea base DOM | 8 | 1 | válido | 20.0 | 50.00 | 3 | 665 B | — | — |
| non-dpda-anyof-none-01 | DPDA | 8 | 1 | 1 violaciones | 5.3 | 187.50 | 1 | 78.0 KiB | 26 | 92 |
| non-dpda-anyof-none-01 | línea base DOM | 8 | 1 | 1 violaciones | 4.0 | 250.00 | 22 | 3.0 KiB | — | — |
| non-dpda-not-violated-01 | DPDA | 3 | 0 | 1 violaciones | 2.3 | 433.33 | 1 | 74.0 KiB | 13 | 43 |
| non-dpda-not-violated-01 | línea base DOM | 3 | 0 | 1 violaciones | 2.5 | 400.00 | 13 | 1.0 KiB | — | — |
| non-dpda-oneof-both-01 | DPDA | 1 | 0 | 1 violaciones | 0.7 | 1400.00 | 1 | 76.0 KiB | 14 | 45 |
| non-dpda-oneof-both-01 | línea base DOM | 1 | 0 | 1 violaciones | 1.4 | 700.00 | 8 | 480 B | — | — |
| non-dpda-oneof-valid-01 | DPDA | 2 | 0 | válido | 1.4 | 700.00 | 1 | 76.0 KiB | 14 | 45 |
| non-dpda-oneof-valid-01 | línea base DOM | 2 | 0 | válido | 20.0 | 50.00 | 1 | 32 B | — | — |
| non-dpda-pattern-01 | DPDA | 7 | 0 | válido | 4.4 | 228.57 | 1 | 72.0 KiB | 17 | 63 |
| non-dpda-pattern-01 | línea base DOM | 7 | 0 | válido | 35.0 | 28.57 | 2 | 37 B | — | — |
| non-dpda-unique-items-01 | DPDA | 9 | 1 | válido | 5.6 | 177.78 | 1 | 72.0 KiB | 17 | 62 |
| non-dpda-unique-items-01 | línea base DOM | 9 | 1 | válido | 30.0 | 33.33 | 2 | 160 B | — | — |
| valid-recursive-tree-01 | DPDA | 82 | 5 | válido | 25.6 | 39.02 | 1 | 74.0 KiB | 41 | 308 |
| valid-recursive-tree-01 | línea base DOM | 82 | 5 | válido | 54.7 | 18.29 | 14 | 2.9 KiB | — | — |
| valid-xml-pedido-01 | DPDA | 207 | 2 | válido | 40.6 | 24.64 | 1 | 74.0 KiB | 216 | 10054 |
| bombeo [^k ]^k, k=1 | DPDA | 2 | 1 | válido | 1.5 | 650.00 | 1 | 72.0 KiB | 16 | 60 |
| bombeo [^k ]^k, k=1 | línea base DOM | 2 | 1 | válido | 10.0 | 100.00 | 1 | 32 B | — | — |
| bombeo [^k ]^k, k=10 | DPDA | 20 | 10 | válido | 11.1 | 90.00 | 1 | 72.0 KiB | 16 | 60 |
| bombeo [^k ]^k, k=10 | línea base DOM | 20 | 10 | válido | 12.5 | 80.00 | 13 | 1.5 KiB | — | — |
| bombeo [^k ]^k, k=100 | DPDA | 200 | 100 | válido | 32.8 | 30.50 | 1 | 72.0 KiB | 16 | 60 |
| bombeo [^k ]^k, k=100 | línea base DOM | 200 | 100 | válido | 12.7 | 79.00 | 106 | 15.4 KiB | — | — |
| bombeo [^k ]^k, k=255 | DPDA | 510 | 255 | válido | 38.1 | 26.27 | 1 | 72.0 KiB | 16 | 60 |
| bombeo [^k ]^k, k=255 | línea base DOM | 510 | 255 | válido | 12.3 | 80.98 | 262 | 37.8 KiB | — | — |
| arreglo de enteros, n=1000 | DPDA | 2001 | 1 | válido | 30.2 | 33.08 | 1 | 72.0 KiB | 17 | 62 |
| arreglo de enteros, n=1000 | línea base DOM | 2001 | 1 | válido | 95.3 | 10.49 | 10 | 32.0 KiB | — | — |
| arreglo de enteros, n=10000 | DPDA | 20001 | 1 | válido | 30.8 | 32.50 | 1 | 72.0 KiB | 17 | 62 |
| arreglo de enteros, n=10000 | línea base DOM | 20001 | 1 | válido | 65.3 | 15.32 | 14 | 512.0 KiB | — | — |
| arreglo de enteros, n=100000 | DPDA | 200001 | 1 | válido | 27.0 | 37.00 | 1 | 72.0 KiB | 17 | 62 |
| arreglo de enteros, n=100000 | línea base DOM | 200001 | 1 | válido | 55.5 | 18.03 | 17 | 4.0 MiB | — | — |
| arreglo de enteros, n=1000000 | DPDA | 2000001 | 1 | válido | 26.9 | 37.21 | 1 | 72.0 KiB | 17 | 62 |
| arreglo de enteros, n=1000000 | línea base DOM | 2000001 | 1 | válido | 56.7 | 17.64 | 20 | 32.0 MiB | — | — |
| arreglo de objetos, n=1000 | DPDA | 8001 | 2 | válido | 39.4 | 25.40 | 1 | 74.0 KiB | 27 | 94 |
| arreglo de objetos, n=1000 | línea base DOM | 8001 | 2 | válido | 21.6 | 46.19 | 2010 | 650.2 KiB | — | — |
| arreglo de objetos, n=10000 | DPDA | 80001 | 2 | válido | 35.6 | 28.05 | 1 | 74.0 KiB | 27 | 94 |
| arreglo de objetos, n=10000 | línea base DOM | 80001 | 2 | válido | 15.6 | 64.25 | 20014 | 6.5 MiB | — | — |
| arreglo de objetos, n=100000 | DPDA | 800001 | 2 | válido | 35.3 | 28.31 | 1 | 74.0 KiB | 27 | 94 |
| arreglo de objetos, n=100000 | línea base DOM | 800001 | 2 | válido | 13.7 | 73.03 | 200017 | 64.4 MiB | — | — |

## 5. Lectura

- **Memoria de tamaño fijo frente a O(n).** El DPDA hace **una sola asignación**
  en todos los casos: su estado, de **72-78 KiB**, que fijan los límites por
  defecto de `docs/00-alcance.md` §4 (pila de 256 marcos y reservas de claves y de
  `uniqueItems`, `Automaton::state_words`, F4-U18), como el `sv_state_t` del
  validador generado. No depende de la longitud ni de la profundidad: es igual para
  2 bytes que para 2 MB de enteros o 800 KB de objetos. La línea base guarda el
  DOM: 32 MiB para 10⁶ enteros (≈ 16× la entrada) y **64,4 MiB con 200 017
  asignaciones** para 10⁵ objetos (≈ 80× la entrada). Para payloads de menos de
  ≈ 100 KB el DOM pesa menos que el estado fijo del DPDA; a partir de ahí la
  diferencia crece linealmente. Es la consecuencia práctica del lema de bombeo: el
  lenguaje {[ᵏ]ᵏ} no es regular y necesita la pila, pero acotar la profundidad
  (D7, 256) deja la memoria del DPDA acotada por una constante.
- **Tiempo lineal.** Con payloads de más de unos KB, el DPDA mantiene
  ≈ 27-40 MB/s (25-37 ns/byte) en todo el rango (2 KB a 2 MB) y en el bombeo de
  profundidad: el tiempo crece linealmente con n. La línea base va más rápido con
  arreglos de enteros (≈ 55-95 MB/s; `serde_json` está muy optimizado), pero cae a
  ≈ 12-22 MB/s con objetos y con anidamiento, donde construir y recorrer el DOM
  domina; ahí el DPDA es 2-3× más rápido. El DPDA aún es el intérprete de tablas
  (F4.6); el código generado (F5) se añadirá como otra columna.
- **Veredictos.** Los dos coinciden en todos los casos del corpus JSON (lo
  comprueba `veredictos_del_corpus`) y el DPDA cubre ya los `non-dpda` (`anyOf`,
  `oneOf`, `not`, `pattern`, `uniqueItems`, Fase 4) y los XML.
- **Tamaño de M.** De 13 estados y 43 transiciones a 216 y 10 054 (el XSD del
  pedido, con Glushkov y contadores).
- Lo que falta para la tabla completa del PDF: el código generado en C y Rust
  (F5.x, otra columna) y la línea base XML en el mismo proceso (no medible con
  `dhat`, §2 de pendientes). Las gráficas contra k y n son de E1, con el CSV de
  este arnés.
