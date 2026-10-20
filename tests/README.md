# Casos de prueba

Fuente de verdad de la suite. Cada fase agrega aquí sus casos desde el inicio; el script de siembra (T8.8) los carga en la base NoSQL y el ejecutor (T8.9) los corre contra el validador generado y la línea base.

## Organización

```
tests/
├── cases/
│   ├── valid/       # conformes: simples, anidados, recursivos
│   ├── invalid/     # bien formados pero violan el esquema
│   ├── malformed/   # sintaxis incorrecta, UTF-8 inválido, claves duplicadas…
│   ├── non-dpda/    # anyOf/oneOf ambiguos, pattern, uniqueItems…
│   ├── pumping/     # u·vᵏ·x·yᵏ·z con k creciente
│   └── extreme/     # payloads enormes, números gigantes, billion laughs…
├── schemas/         # esquemas sueltos para el parser (Fase 2) y el análisis semántico (Fase 3) (ver abajo)
└── large/           # caché local de payloads generados (ignorado por git; ningún caso la referencia)
```

Un caso = un archivo `cases/<categoría>/<_id>.json`. El nombre del archivo debe coincidir con `_id`. Todo `.json` del primer nivel de una carpeta de categoría se lee como un caso; los archivos que referencian `schemaFile`/`payloadFile` con extensión `.json` van en una subcarpeta (p. ej. `cases/valid/esquemas/`), que el recorrido ignora. En `cases/` solo hay carpetas de categoría.

## Formato de un caso

```json
{
  "_id": "invalid-missing-required-01",
  "category": "invalid",
  "format": "json",
  "description": "Falta la propiedad requerida \"email\" en el segundo usuario.",
  "phase": 4,
  "schema": { "type": "object", "...": "..." },
  "payload": "{\"users\": [...]}",
  "expected": {
    "valid": false,
    "violations": 1,
    "errorKind": "schema",
    "pointer": "/users/1",
    "offset": 64
  }
}
```

| Campo | Obligatorio | Descripción |
|---|---|---|
| `_id` | sí | `<categoría>-<tema>-<NN>`, en minúsculas con guiones. Único. |
| `category` | sí | `valid`, `invalid`, `malformed`, `non-dpda`, `pumping` o `extreme`. Coincide con la carpeta. |
| `format` | sí | `json` o `xml`. |
| `description` | sí | Qué prueba el caso, en una frase. |
| `phase` | no | Fase del plan que introdujo el caso (para filtrar). |
| `schema` | sí* | JSON Schema como objeto o booleano (`true`/`false`), o XSD como cadena. |
| `schemaFile` | sí* | Alternativa a `schema`: ruta relativa a la carpeta del caso, formada solo por nombres (sin `./`, `..` ni rutas absolutas), a un archivo que debe existir (p. ej. `pedido.xsd`). Si es `.json`, va en una subcarpeta (p. ej. `esquemas/arbol.json`). |
| `payload` | sí† | El documento a validar, como cadena (se valida byte a byte en UTF-8). |
| `payloadFile` | sí† | Alternativa: ruta relativa a la carpeta del caso, formada solo por nombres (sin `./`, `..` ni rutas absolutas), a un archivo **versionado**. Útil para payloads medianos o binarios legibles. Si es `.json`, va en una subcarpeta. |
| `payloadBase64` | sí† | Alternativa para bytes que no son UTF-8 válido (casos `malformed`). |
| `generator` | sí† | Alternativa para `pumping`/`extreme` y todo payload grande: `{ "name": "...", "params": { ... } }`; el ejecutor genera el payload. Los payloads grandes nunca se versionan ni se referencian por archivo. |
| `strategy` | no | Solo `non-dpda`: estrategia de respaldo esperada (`product`, `bounded-determinization`, `deferred`). |
| `limits` | no | Límites a usar en lugar de los por defecto de `docs/00-alcance.md` §4. |
| `expected` | sí | Resultado esperado (ver abajo). |

\* Exactamente uno de `schema` / `schemaFile`. † Exactamente uno de `payload` / `payloadFile` / `payloadBase64` / `generator`.

### `expected`

| Campo | Descripción |
|---|---|
| `valid` | `true` si el payload es conforme. |
| `violations` | Número de violaciones en modo de conformidad total (peso del recorrido en el semianillo tropical). `0` ⇔ `valid`. Se omite con `errorKind` `lexical` o `syntax`: esos errores detienen el análisis. |
| `errorKind` | Solo si `valid` es `false`: `lexical`, `syntax`, `schema` o `limit`. |
| `pointer` | JSON Pointer (o XPath simple para XML) del **primer** error. |
| `offset` | Posición en bytes (desde 0) donde se **detecta** el primer error. Por ejemplo, un `required` faltante se detecta en la `}` que cierra el objeto. |

**Comprobación automática del formato.** Al leer el corpus (`crates/lexer/tests/common/mod.rs`, función `validar_formato`, que corre en cada `cargo test`), cada caso debe cumplir: `_id` igual al nombre del archivo sin `.json` y con la forma `<categoría>-<tema>-<NN>` (minúsculas, dígitos y guiones; NN de al menos dos dígitos); `category` una de las seis e igual a su carpeta; `format` `json` o `xml`; `description` no vacía; `phase` entero si aparece; exactamente uno de `schema`/`schemaFile` (objeto o booleano para `json`, cadena XSD para `xml`); `schemaFile` y `payloadFile` relativas y formadas solo por nombres (sin `./`, `..` ni rutas absolutas), en una subcarpeta si son `.json`, y existentes; exactamente una forma de payload; `strategy` solo en `non-dpda` y con uno de sus tres valores; las reglas de la tabla `expected` anterior (`valid` booleano, `errorKind` solo con `valid: false` y en su dominio, `violations` 0 ⇔ `valid` y omitido con `lexical`/`syntax`, `offset` entero, `pointer` cadena); ningún campo fuera de las tablas, ni en el primer nivel ni dentro de `expected`; `_id` único, y en `cases/` solo las seis carpetas de categoría. Un incumplimiento hace fallar la prueba con el archivo y el campo. Lo que exige ejecutar el caso (que `pointer` y `offset` sean correctos salvo el `offset` léxico) lo comprueban las fases que lo validan.

Las pruebas del lexer recorren todo el corpus: tokenizan cada payload entero y en bloques de 1, 7 y 4096 bytes, exigen el mismo resultado, y si `errorKind` es `lexical` comprueban que el error ocurra exactamente en `offset`.

### Métricas

Las métricas (`metrics`: MB/s, ciclos/byte, memoria, asignaciones, profundidad) **no se guardan en el repositorio**: el ejecutor (T8.9) las añade al documento en la base NoSQL en cada corrida.

## Corpus de esquemas (`schemas/`)

Prueba el parser del metalenguaje (Fase 2, `crates/schema-parser/tests/corpus.rs`) y el análisis semántico (Fase 3: meta-esquema y referencias) con el texto exacto de cada esquema, para poder fijar línea y columna:

- `schemas/parse-ok/*.json`: deben parsear.
- `schemas/parse-error/*.json`: deben fallar. Cada archivo tiene una entrada en `schemas/errores.json` con `file`, `code` (`lexical`, `syntax`, `duplicate-key` o `limit`), `line` y `column` (base 1, columnas en puntos de código), un fragmento opcional del mensaje (`contains`) y `description`. La prueba falla si sobra o falta una entrada.

Los esquemas JSON de `cases/` también pasan por el parser.

La Fase 3 (`crates/semantic/tests/corpus.rs`: meta-esquema y, por etapas, tabla de símbolos con la resolución de `$ref` y ciclos no productivos) usa el mismo corpus:

- `schemas/parse-ok/*.json` y los esquemas JSON de `cases/` deben pasarla, salvo dos esquemas de `parse-ok` que existen para probar el parser y fallan a propósito en la Fase 3 (`combinadores.json` y `fuera-del-subconjunto.json`; sus errores exactos están fijados en la prueba).
- `schemas/semantic-error/*.json`: parsean pero deben fallar en la Fase 3. Cada archivo tiene una entrada en `schemas/errores-semanticos.json` con los mismos campos que `errores.json` (`code` es `unsupported`, `meta-schema`, `reference`, `duplicate-anchor` o `cycle`; `line` y `column` son las del **primer** error por posición) y, opcional, `count`: el número total de errores que se esperan (la Fase 3 los informa todos).

## Semillas de proptest (`*.proptest-regressions`)

Cuando una prueba de propiedades falla, proptest guarda la semilla del caso mínimo en un archivo `<prueba>.proptest-regressions` junto a la prueba (por ejemplo `crates/semantic/tests/symbols.proptest-regressions`) y la vuelve a probar antes que los casos nuevos en cada ejecución. **Se versionan**, como recomienda proptest: así cualquiera que ejecute las pruebas (y la CI) repite los contraejemplos que ya encontraron un error, y el costo es una línea por caso. No se editan a mano; si una semilla deja de tener sentido porque cambió el generador, se borra la línea y se explica en el commit. Las semillas actuales de `crates/semantic/tests/` salieron de mutaciones deliberadas hechas para comprobar que las pruebas detectan errores: `symbols.proptest-regressions`, de quitar el orden de documento del preorden; `cycles.proptest-regressions`, de tratar `properties` como aplicador sobre la misma instancia y de no tratar así a `not`. Son contraejemplos reales de esos errores.

## Cómo agregar un caso

1. Copiar un caso existente de la misma categoría.
2. Ajustar `_id` (y el nombre del archivo), `description`, `schema`, `payload` y `expected`.
3. Verificar a mano `pointer` y `offset`; si el caso es grande, usar `generator` en lugar de incrustar el payload.
