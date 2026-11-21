# Mitigaciones (F6.4, D79)

Qué ataques conocidos contra validadores de JSON y XML no funcionan aquí, por qué, y qué prueba lo comprueba. Casi
todo lo hicieron tareas anteriores; F6.4 lo junta, lo prueba de punta a punta en
`crates/automaton/tests/mitigaciones.rs` y añade el fuzzing ([fuzzing.md](fuzzing.md)).

| Ataque | Mitigación | Dónde | Pruebas |
| --- | --- | --- | --- |
| **Billion laughs** (entidades que se expanden en cadena) | No hay DTD: el lexer XML rechaza `<!DOCTYPE` en su `D`, antes de leer ninguna declaración (D8). Solo existen las 5 entidades predefinidas y las referencias de carácter. | `crates/lexer/src/xml.rs` | `mitigaciones.rs::dtd_y_entidades`, `malformed-xml-billion-laughs-01`, `lexer/tests/xml.rs` |
| **XXE** (entidad externa `SYSTEM`) y entidades de parámetro | Lo mismo: sin DTD no hay entidades externas; nada abre archivos ni red. Una referencia a otra entidad (`&xxe;`) es error léxico. | ídem | `dtd_y_entidades` |
| Muchas referencias de carácter | `maxXmlRefs` (100 000) por documento. | `runtime-rs/src/core.rs` | `automaton/tests/limites.rs::referencias_xml` |
| DTD en el esquema XSD | El frontal XSD usa el mismo lexer: un XSD con DOCTYPE no se lee. | `crates/xsd` | `dtd_y_entidades` |
| **ReDoS** (`^(a+)+$` con `aaaa…!`) | `pattern` se compila a un AFD mínimo dentro de M (F4.4b): un paso de δ por carácter, sin vuelta atrás. Los patrones clásicos de ReDoS dan pocos estados y la cadena atacante se valida en una pasada. | `crates/automaton/src/pattern.rs` | `mitigaciones.rs::pattern_como_afd`, `automaton/tests/pattern.rs` |
| Explosión al determinizar (`(a\|b)*a(a\|b){24}`) | Presupuesto de estados y de expansión de `{m,n}` (F4-U16): `limit` al compilar, nunca un AFD enorme ni una espera. | `crates/ir/src/regex.rs`, `automaton` | `pattern_como_afd` |
| Retrorreferencias, *lookaround*, cuantificadores perezosos | Se rechazan en la Fase 3: no son regulares o no tienen sentido en un AFD. | `crates/ir/src/regex.rs` | `pattern_como_afd`, `tests/schemas/semantic-error/pattern-*.json` |
| **`$ref` cíclico** que no consume entrada (`{"$ref":"#"}`, `a → b → a`, `allOf`/`not` sobre sí mismo) | Se rechaza en la Fase 3 (ciclos no productivos). | `crates/semantic` | `mitigaciones.rs::ref_ciclico`, `tests/schemas/semantic-error/ciclo-*.json` |
| `$ref` recursivo productivo con un payload muy hondo | Un marco recursivo de M; la pila la acota `maxDepth` (256) con `limit`, en memoria fija. Nada recorre el payload con recursión (D7). | `runtime-rs/src/core.rs` | `ref_ciclico` (10⁵ niveles), `extreme-depth-01` |
| Números enormes (`1e999999999999999999`, 10⁵ cifras) | Solo se guarda el lexema hasta `maxLexeme`; más largo es `limit` si una comprobación lo lee. El exponente no se expande: se compara por partes (`num.rs`). | `runtime-rs/src/num.rs` | `mitigaciones.rs::numeros_enormes`, `extreme-big-number-01` |
| Cadenas, claves y payloads enormes | `maxString`, `maxKeys`, `keyPool`, `maxUnique`, `maxPayload` (F6.2). | `limits.rs` | `limites.rs` (núcleo y binarios) |
| Muchos `pattern` distintos, cada uno dentro de su tope (E3B-2, E3B-3) | Tope de trabajo de `pattern` por **esquema** (2²³ palabras): `limit` en 1-2 s en vez de minutos u horas. | `automaton/src/pattern.rs`, `json.rs`, `xml.rs` | `trabajo_de_pattern_por_esquema` |
| Cadena de `anyOf`/`oneOf` con puertas que crecen de forma combinatoria (E3B-1) | La unión de conjuntos deduplica y busca destinos con mapas (O(g)) y cobra cada puerta copiada al presupuesto de los cierres: `limit` en ≤ 2 s (antes, 206 s con 2,4 KB). | `automaton/src/conj.rs` | `cadena_de_anyof_acotada` |
| Exponentes de 19-25 cifras en `uniqueItems` (E3A-1) | El C acumulaba el exponente en `int64_t` (UB al multiplicar por 10); con `not` sobre `uniqueItems` aceptaba lo que Rust da como `limit`. Ahora en `uint64_t`, igual que Rust. | `runtime-c/sv_nucleo.c` (`number_hash`) | `diferencial.rs` (esquemas y bordes nuevos; con el código anterior UBSan la hace fallar) |
| Colisiones de hash para fabricar «claves repetidas» | SipHash-1-3 con clave; un choque solo da un falso repetido (rechazo), nunca acepta un malformado. La clave por defecto es pública (`docs/fase4/runtime.md` §4). | `runtime-rs/src/hash.rs` | `runtime-rs` |
| Memoria del validador | Sin asignación en el camino de validación (`sin_reservas.rs`, `nm -u` del C); `sv_state_t` de tamaño fijo. | — | `automaton/tests/sin_reservas.rs`, `codegen/tests/c.rs` |
| Esquemas hostiles en el servidor | Topes de subida, tiempo, concurrencia y presupuesto de construcción (F4-U6). | `server/` (pista B), `automaton` | `docs/fase7/servidor.md` |

## Lo que no cubre

- **Tiempo de compilación del esquema.** El presupuesto de F4-U6 acota lo que se construye, pero no hay un tope de
  tiempo de pared dentro de `automaton`. El servidor pone el suyo (10 s).
- **El C bajo fuzzing directo.** El C no se enlaza con libFuzzer: haría falta FFI, y eso exige `unsafe` (prohibido
  salvo D52). En su lugar, lo que encuentra el fuzzer se reproduce en la prueba diferencial con ASan/UBSan
  ([fuzzing.md](fuzzing.md) §3).
- **Revisión de seguridad.** La revisión consolidada con `security-auditor` es E3 (D35).
