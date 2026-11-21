# Fase 4 — El ejecutor de M (F4.6)

El ejecutor es el núcleo `no_std` de [`crates/runtime-rs`](../../crates/runtime-rs/src/lib.rs) (F4-U9, D51) con los lexers de la Fase 1 delante: lee las tablas de M ([automata.md](automata.md) §4), usa memoria del llamador y recibe el payload por bloques. F4.6 le añadió los **límites** de [`docs/00-alcance.md`](../00-alcance.md) §4, el **reporte** de F4-U18 y la **capa de buena formación** (F4-B7): lo que M no comprueba por diseño. Es el mismo núcleo que emitirá la Fase 5 con las tablas como `const` (F5.1) y que se portará a C (F5.2). Decisiones: D59 del plan.

## 1. API (F4-U18)

| Pieza | Qué es |
|---|---|
| `Options { mode, limits }` | `Options::new(mode)` o `Options::default()` (conformidad total) con `Limits::default()` (F4.1a-CR-S9). |
| `Limits` | Los límites de §3; `Limits::DEFAULT` = los del alcance. |
| `Tables::state_words(&Limits)` | Palabras `u64` que el llamador debe prestar (§2). |
| `JsonValidator::new(tables, &opts, mem)` / `XmlValidator::new(…)` | El lexer de su formato delante del núcleo. |
| `feed(&[u8]) -> Flow` | Un bloque de cualquier tamaño: `Continue` o `Stop` (error de peso ∞ o fallo temprano: el resto no cambia el resultado). |
| `finish() -> Report` | Fin de la entrada: cierra el número pendiente y da el reporte. |
| `automaton::validate(a, payload, mode, chunk)` / `validate_with(a, payload, &opts, chunk)` | Envoltura `std` (F4-B24): reserva `state_words` palabras **antes** de validar (F4.1a-CR-S3, §13/§45 de [pendientes.md](pendientes.md)) y parte el payload en bloques de `chunk` bytes. |

`Report`: `valid` (bien formado y peso 0), `violations` (el peso: `u64` saturado o ∞), `error_kind` (`lexical`, `syntax`, `schema`, `limit`, o `internal` si las tablas son incoherentes) y el **primer** error `ErrorAt { offset, line, column, rule }`. La regla da el `RuleKind`, el `NodeId` y la ubicación del esquema recortada a 256 bytes (`location_clipped`, guardada en las tablas: `Tables::location`). Si hay un error de peso ∞ es el primero, aunque antes hubiera violaciones. Desde F6.1, `Report::pointer` es la instancia del primer error ([../fase6/puntero.md](../fase6/puntero.md)); los límites con nombre, `max_lexeme` y el tiempo de los binarios son de F6.2 ([../fase6/limites.md](../fase6/limites.md)).

## 2. Memoria y complejidad

`mem` se reparte así, sin reservar nada durante la validación (`sin_reservas.rs`: cero reservas entre `new` y `finish` en todo el corpus, en los dos modos, en bloques de 1, 7 y 4096 bytes, y con 10⁵ niveles con `max_depth` = 10⁵ en un hilo de 256 KiB):

```
[ zona escalar (scalar_slots) | pila de marcos → … ← reserva de hashes ]
```

- **Pila de marcos** detrás de la zona de las instancias escalares con salidas (`Tables::scalar_slots`, F4.5a): cada marco son `Frame::words` palabras de registros (con sus salidas al final), el acumulador del hash canónico y la clave del miembro abierto (`uniqueItems`, F4.5b), una palabra de buena formación y la cabecera `(retorno << 32) | tipo` (`FRAME_OVERHEAD` = 4; 2 hasta F4.5b).
- **Reserva de hashes** desde el final hacia la pila: una entrada de 64 bits por clave JSON no declarada o atributo XML sin bit (§4).
- `state_words(l)` = `scalar_slots` + `l.max_depth` × (marco máximo + `FRAME_OVERHEAD`) + `l.key_pool` + `l.max_unique`, saturado. Si `mem` es más corta, el marco o la entrada que no caben dan `limit`, nunca un pánico (`memoria_de_los_limites`).

Con los límites por defecto y el marco más grande de un esquema de 4 096 nombres (64 palabras de máscara y un contador): 256 × 69 + 8 192 palabras ≈ 202 KiB; con el marco «cualquiera», 256 × 4 + 8 192 palabras = 72 KiB. Los registros globales de `Core` (el lexema de 1 024 bytes y unas decenas de palabras) no dependen del payload.

**O(n) en bytes y O(d) en pila.** Cada byte entra una vez al lexer, que da como mucho un evento por byte (un carácter de una referencia o de un escape da un evento para varios bytes). Por evento, el núcleo hace una consulta a δ, las comprobaciones de la transición (acotadas por el esquema) y a lo sumo una operación de pila; la capa de buena formación añade O(1) por byte (el hash de la clave o del nombre) y, al terminar una clave o un atributo sin bit, una búsqueda lineal en la región de su objeto o etiqueta, de como mucho `max_keys` = 1 024 entradas: una constante del alcance, no del payload (F4-B22). La pila crece solo con `{`, `[` o un elemento con contenido, y `max_depth` la acota.

## 3. Límites

| Límite (`Limits`) | Defecto | Qué cuenta | Dónde da `limit` |
|---|---|---|---|
| `max_depth` | 256 | Objetos y arreglos abiertos (JSON); elementos con contenido abiertos (XML; `<a/>` no apila) | En el evento que apilaría el 257.º: el `{` o `[`, o el `<` de la etiqueta del elemento |
| `max_string` | 1 MiB | Bytes UTF-8 (ya sin escapes ni referencias) de un **tramo de caracteres**: una cadena o clave JSON, un nombre, un valor de atributo o un tramo de texto XML | En el carácter que lo pasa |
| `LEXEME_MAX` (constante) | 1 024 bytes | Lexema de un número (o de un texto numérico XML sin espacios) | Al cerrarlo, **solo si una comprobación lo lee** (rangos, `multipleOf`, `enum` numérico; [pendientes.md](pendientes.md) §21) |
| `max_keys` | 1 024 | Claves no declaradas de un objeto; atributos sin bit de una etiqueta | En la comilla de cierre de la clave o en el comienzo del atributo que no cabe |
| `key_pool` | 4 096 | Las mismas, entre todos los objetos abiertos (F4-U16) | Igual |
| `max_unique` | 4 096 | Hashes de elementos guardados por los arreglos con `uniqueItems` abiertos (F4.5b) | En el cierre del elemento que no cabe |
| `max_xml_refs` | 100 000 | Referencias de entidad o de carácter de un documento XML | En el `&` de la que pasa |
| `max_payload` | sin límite | Bytes del payload | En el primer byte de más, sin leerlo |

Cada uno se prueba en su valor y en su valor + 1 con el offset esperado (`crates/automaton/tests/limites.rs`). Un límite es peso ∞: en fallo temprano y en conformidad total detiene el análisis. F4.5a hará que atraviese `not` sin invertirse.

**Los dos modos** (F4.1a-CR-S7, [automata.md](automata.md) §5): dan el mismo veredicto en todo el corpus (`todo_el_corpus_da_lo_esperado`); en fallo temprano se detiene en la primera violación con peso 1 y la regla de menor costo. Pueden dar clases distintas con un payload malformado **después** de una violación: el fallo temprano no llega a verlo (`schema`), la conformidad total sí (`syntax`). No pasa en el corpus.

## 4. Capa de buena formación (F4-B7)

Lo que M no ve por diseño lo comprueba el núcleo junto a δ, en la misma pasada. Un error de buena formación es `syntax` (∞), como el de M.

**Claves JSON repetidas (D2, [pendientes.md](pendientes.md) §1).** M detecta con su máscara las claves **declaradas** repetidas. Para las demás, el núcleo sabe qué cadena es una clave (la que sigue a `{` o a `,` dentro de un objeto: la palabra de buena formación de cada marco JSON dice si es un objeto) y calcula su hash SipHash-1-3 con la clave `Limits::hash_key` mientras la lee. Si la transición que la cierra no fija un bit, el hash entra en la **región** del objeto en la reserva (su comienzo va en la palabra de buena formación del marco); si ya estaba, es una repetida. Al desapilar el objeto se libera su región, así que un hijo puede repetir las claves del padre. El error va en la **comilla de cierre** de la repetida, declarada o no, con su línea y columna en la fuente (un escape `a` cuenta sus seis columnas): es donde se sabe que lo es, y coincide con la línea base (D50, B14).

**Atributos XML sin bit repetidos.** Igual, con una región por etiqueta de apertura que se libera en su `>` o `/>`: los atributos no declarados, los de un elemento saltado y `xmlns`/`xmlns:p` repetidos. El error va en el comienzo del atributo.

**Nombre de cierre (XML 1.0 §3, [pendientes.md](pendientes.md) §33).** El hash del QName de apertura (prefijo incluido) va en la palabra de buena formación del marco del elemento; en el `>` de la etiqueta de cierre se compara con el del nombre de cierre. Cubre **todo** elemento con contenido, también los saltados (M solo compara la parte local de los que tienen marco propio). El error va en el `<` de la etiqueta de cierre.

**Forma de los nombres (Namespaces in XML §3).** Un nombre de elemento o de atributo con `:` al principio, al final o más de uno no es un QName: `syntax` en el evento que termina el nombre.

**Fines de línea (XML 1.0 §2.11 y §3.3.3, [pendientes.md](pendientes.md) §35).** En el texto y en los valores de atributo, `\r\n` y `\r` llegan a M como un solo `\n`; en un valor, además, `\t` y `\n` llegan como espacio. Una referencia de carácter (`&#13;`, `&#9;`) no se normaliza: el núcleo la distingue porque el rango de su evento es más largo que el carácter que da. Así `length`, `enum` y `fixed` de `xs:string` ven el valor normalizado.

**Colisiones.** Un choque de SipHash-1-3 entre dos nombres distintos del mismo objeto o etiqueta da un falso «repetido» (rechaza un payload bien formado). La excepción es el nombre de cierre XML: un choque entre el QName de apertura y el de cierre **acepta** una etiqueta de cierre que no empareja (E3A-5; con la clave pública se puede fabricar, `docs/fase6/pendientes.md` §31). Con una clave que el atacante no conoce, la probabilidad es ≤ n²/2⁶⁵ con n ≤ 1 024 (< 2⁻⁴⁵). La clave por defecto es pública; quien valide payloads hostiles y no quiera que se puedan fabricar choques pone una propia en `Limits::hash_key` (o `sv_limits_t.hash_key` en C); los binarios de `schemac compile` usan la de por defecto (`docs/fase6/pendientes.md` §31).

**Lo que queda fuera.** Los espacios de nombres del payload se comparan por nombre local, sin resolver el URI ([pendientes.md](pendientes.md) §34, decisión de las dos personas); `xml:lang` y `xsi:*` cuentan como atributos no declarados.

## 5. Oráculos

- **El corpus** (`crates/automaton/tests/corpus.rs`, `todo_el_corpus_da_lo_esperado`): todo caso con bytes cuyo esquema compila, JSON o XML, da el `valid`, `errorKind`, `violations` y `offset` de `expected`, con el mismo reporte en bloques de 1, 7 y 4096 bytes y el mismo veredicto en fallo temprano. F4.6 añadió once casos: `invalid-type-01`, `invalid-additional-properties-01`, `invalid-enum-01`, `invalid-range-01`, `invalid-xml-range-01`, `invalid-xml-missing-child-01`, `malformed-json-duplicate-key-01`, `malformed-json-syntax-01`, `malformed-xml-unclosed-01`, `extreme-depth-01` y `extreme-big-number-01`.
- **La línea base de F6.7a** (`json_coincide_con_la_linea_base`, D48): en JSON, el veredicto de M (válido, malformado, violación o límite) y el recuento coinciden con los de `serde_json` + `jsonschema` en cada caso que compila. Única divergencia declarada (también en `DIVERGENCIAS` de `crates/baseline/tests/corpus_json.rs`, que desde F4.6 admite el campo `errorKind`): `extreme-big-number-01`, `limit` en el DPDA y `Unsupported` en la línea base, porque `serde_json` no representa 10¹⁰²⁴ en `f64`. En XML el oráculo es `expected`; con la feature `libxml2` (F6.7b), también libxml2.
- **Proptest** (`aleatorio.rs`): payloads JSON y XML aleatorios y mutaciones de un byte no hacen entrar en pánico al ejecutor y dan el mismo reporte en los tres tamaños de bloque; en JSON, el ejecutor da `syntax` si y solo si algún objeto repite una clave, y nunca acepta lo que `serde_json` rechaza.
