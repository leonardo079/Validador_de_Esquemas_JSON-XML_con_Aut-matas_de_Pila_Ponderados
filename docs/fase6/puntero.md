# Puntero del primer error (F6.1, D77)

El reporte dice **dónde** falla el payload de dos formas: la posición en bytes, línea y columna (F4.6) y, desde F6.1,
la instancia en la que se detectó, como un JSON Pointer (RFC 6901) en JSON y una ruta de elementos en XML. Se
calcula en la misma pasada que la validación, con memoria fija y sin construir el árbol del payload.

| Pieza | Archivo |
| --- | --- |
| Núcleo Rust | `crates/runtime-rs/src/core.rs` (`Pointer`, `POINTER_MAX`, `seg_*`, `snapshot`) |
| Núcleo C | `crates/runtime-c/sv_nucleo.c` (las mismas funciones; `sv_report_t.pointer`) |
| Línea JSON de los binarios | `crates/codegen/src/rust.rs` y `c.rs` (`pointer`, `pointerTruncated`) |
| Pruebas | `crates/automaton/tests/puntero.rs`, `corpus.rs`; `crates/codegen/tests/diferencial.rs` |

## 1. Qué es

- **JSON:** `""` es la raíz; cada nivel añade `/` y la clave (con `~` → `~0` y `/` → `~1`) o el índice del
  elemento en decimal. La clave es la decodificada (`"A"` da `A`).
- **XML:** `/` y el nombre de cada elemento abierto, tal como está escrito (con prefijo): `/pedido/linea/cantidad`.
  Es un XPath simple sin posiciones: calcular `linea[3]` exigiría contar los hermanos de cada nombre, y eso no cabe
  en memoria fija. La línea y la columna dicen cuál de ellos es.

La instancia del error es **la que está abierta en el evento que lo detecta**:

| El error se detecta en… | Puntero |
| --- | --- |
| un valor escalar o la apertura de un contenedor | ese valor (`/a/0`) |
| una clave (repetida, o no permitida por el trie), la coma o el cierre `}`/`]` (`required`, `minItems`, `maxProperties`…) | el contenedor (`/a`) |
| el valor de un miembro no declarado con `additionalProperties: false` | el miembro (`/b`) |
| un atributo, el `>` o la etiqueta de cierre (falta un hijo) | el elemento |
| el texto de un elemento | el elemento |
| un elemento que el modelo de contenido no admite (se ve al terminar su nombre; el offset es el de su `<`) | ese elemento (`/r/x`), como el miembro de `additionalProperties: false` |
| un error léxico a mitad del nombre de un elemento | el padre |
| el fin de la entrada (incompleta) | donde se quedó: tras `[1,` es `/1` |

`jsonschema` atribuye `additionalProperties: false` al objeto (`""`), no al miembro. Las dos lecturas valen: la
regla falla en el subesquema `false` aplicado al valor de `b`. La prueba con la línea base admite por eso que el
puntero del DPDA sea igual a uno de los suyos **o esté debajo** de uno.

## 2. Cómo se calcula

El núcleo guarda la ruta de la instancia abierta en un búfer fijo `path` de `POINTER_MAX` = 1 024 bytes:

- **Clave:** se copia mientras se lee (ya escapada), como segmento *pendiente*: no cuenta hasta el `:`. Así un
  error en la propia clave es del objeto.
- **Arreglo:** el `[` abre el segmento `/0`; cada coma suma 1 al índice **en su sitio** (de `/9` a `/10` crece un
  byte). Durante la coma el segmento está oculto: lo que se detecta en ella es del arreglo.
- **Cierre:** quita el último segmento antes de δ (el error del `}` es del objeto).
- **XML:** el nombre del elemento es el segmento pendiente hasta que termina; la etiqueta de cierre o `/>` lo
  quitan después de δ.
- **Primer error:** al fijarse `Report::first` (la primera violación o, después, el error de peso ∞ que la
  sustituye), se copia la ruta activa al reporte.

No hace falta guardar nada por marco: los segmentos se separan por `/`, que nunca aparece dentro de uno (en JSON
está escapado y un nombre XML no lo admite), y el índice de un arreglo es el texto de su último segmento. Por
evento cuesta O(1) más los bytes que se escriben, que son los de la clave: la validación sigue siendo O(n).

**Recorte.** Si un segmento no cabe, no se guarda ni él ni ninguno más profundo: se cuentan en `pextra`. El
puntero del reporte es entonces el antepasado más profundo que cabe y `Pointer::truncated()` es `true`. Al volver
por encima de los niveles que no cupieron, el puntero vuelve a ser exacto. El corte es siempre entre segmentos,
así que el texto sigue siendo UTF-8. La memoria del núcleo crece en 2 KiB (la ruta y la copia del reporte), fuera
de `state_words`.

## 3. Dónde aparece

- `runtime_rs::Report::pointer` (`as_str`, `as_bytes`, `truncated`); `Pointer::ROOT` si no hay error.
- `sv_report_t.pointer`, `pointer_len` y `pointer_truncated` en C.
- La línea JSON de los binarios generados, entre `column` y `rule`:

  ```json
  {"valid":false,"violations":1,"errorKind":"schema","offset":64,"line":1,"column":65,"pointer":"/users/1","pointerTruncated":false,"rule":"#/properties/users/items","keyword":"required"}
  ```

  Sin error, `"pointer":null,"pointerTruncated":false`. Es un campo aditivo del contrato de F7.5
  (`docs/fase5/pendientes.md` §5).

El servidor (`/api/compare`, `/api/tests/run`) todavía no lo muestra: es de la pista B (`docs/fase6/pendientes.md`
§29).

## 4. Pruebas

- **Corpus:** los 7 casos con `expected.pointer` (5 JSON y los 2 XML a los que F6.1 añadió `pointer`) dan ese
  puntero, en el intérprete y en los binarios Rust y C. En todo caso JSON con violaciones, el puntero está entre
  los de `jsonschema` o debajo de uno.
- **`tests/puntero.rs`:** anidados, índices de 1 a 4 cifras, escapes RFC 6901 y claves con `\u`, errores de
  contenedor, léxicos, de sintaxis y de fin de entrada, el de peso ∞ que sustituye a una violación, el recorte y la
  vuelta a un puntero exacto, el índice que deja de caber al crecer, XML (texto, atributo, hijo que falta,
  cierre que no empareja, elementos vacíos), y una propiedad con proptest: el puntero es el del primer número
  negativo de un valor aleatorio (claves con `~`, `/`, no ASCII y de 100-400 bytes), recortado al prefijo de
  segmentos que cabe. Todo en bloques de 1, 7 y 4 096 bytes.
- **Diferencial:** el puntero entra en el reporte que se compara entre intérprete, Rust y C (con ASan/UBSan), con
  payloads añadidos para él: claves con `~` y `/`, índices de dos cifras, un puntero de más de 1 024 bytes, el
  índice que deja de caber al pasar de `/9` a `/10` y un error en una coma con segmentos recortados (las dos ramas
  del recorte del C).
- `Pointer` se compara por su texto y su marca, no por los bytes viejos del búfer (`igualdad_por_el_puntero_visible`).
