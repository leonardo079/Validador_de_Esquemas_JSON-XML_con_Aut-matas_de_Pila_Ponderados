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
└── large/           # caché local de payloads generados (ignorado por git; ningún caso la referencia)
```

Un caso = un archivo `cases/<categoría>/<_id>.json`. El nombre del archivo debe coincidir con `_id`.

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
| `schema` | sí* | JSON Schema como objeto JSON, o XSD como cadena. |
| `schemaFile` | sí* | Alternativa a `schema`: ruta relativa a este archivo (p. ej. `pedido.xsd`). |
| `payload` | sí† | El documento a validar, como cadena (se valida byte a byte en UTF-8). |
| `payloadFile` | sí† | Alternativa: ruta relativa a la carpeta del caso (sin `..`), a un archivo **versionado**. Útil para payloads medianos o binarios legibles. |
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

Las pruebas del lexer recorren todo el corpus: tokenizan cada payload entero y en bloques de 1, 7 y 4096 bytes, exigen el mismo resultado, y si `errorKind` es `lexical` comprueban que el error ocurra exactamente en `offset`.

### Métricas

Las métricas (`metrics`: MB/s, ciclos/byte, memoria, asignaciones, profundidad) **no se guardan en el repositorio**: el ejecutor (T8.9) las añade al documento en la base NoSQL en cada corrida.

## Cómo agregar un caso

1. Copiar un caso existente de la misma categoría.
2. Ajustar `_id` (y el nombre del archivo), `description`, `schema`, `payload` y `expected`.
3. Verificar a mano `pointer` y `offset`; si el caso es grande, usar `generator` en lugar de incrustar el payload.
