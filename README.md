# Validador de Esquemas JSON/XML con Autómatas de Pila Ponderados

**Proyecto final — Lenguajes Formales y Autómatas 2026-2**
Ingeniería de Sistemas y Computación — UPTC Sogamoso

Compilador que traduce un esquema **JSON Schema** (subconjunto del Draft 2020-12, con extensión a un subconjunto de **XSD**) a un **autómata de pila determinista ponderado** y emite un validador nativo en **C** y **Rust**. El validador consume los bytes del payload en **una sola pasada**, en streaming, **sin construir un AST ni reservar memoria dinámica**.

## Motivación

Los microservicios gastan mucha CPU serializando y parseando contratos de API. Normalmente cada payload se convierte primero en un árbol genérico (AST/DOM) y después se valida de forma interpretada contra el esquema. Este proyecto valida directamente sobre los bytes de entrada con un autómata generado a partir del esquema, y compara su rendimiento con los validadores basados en AST.

## Objetivos

- Definir formalmente el subconjunto de JSON Schema que puede reconocer un DPDA con registros acotados, y clasificar el resto de palabras clave.
- Construir el prototipo del compilador: esquema → DPDA ponderado → código nativo C/Rust con validación en streaming.
- Medir en vivo rendimiento, memoria y asignaciones frente a validadores basados en AST, y contrastarlo con la complejidad teórica.
- Desplegar la solución como aplicación web en la nube con persistencia NoSQL.

## Arquitectura del compilador

```
Esquema (JSON Schema / XSD)
        │
        ▼
 ┌──────────────┐   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
 │ 1. Léxico    │──▶│ 2. Sintáctico│──▶│ 3. Semántico │──▶│ 4. Autómata  │──▶│ 5. Código    │
 │ AFD streaming│   │ Parser LL(1) │   │ $ref, ciclos,│   │ DPDA         │   │ C11 / Rust   │
 │ UTF-8, tokens│   │ → AST        │   │ clasificación│   │ ponderado    │   │ no_std       │
 └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘
                                                                                     │
                     Payload (bytes) ──▶ validador generado ──▶ conforme / violaciones + JSON Pointer
```

| Fase | Contenido |
|---|---|
| 1. Análisis léxico | Alfabeto de bytes UTF-8, tokens JSON y XML, AFD del tokenizador en streaming, gramáticas regulares. |
| 2. Análisis sintáctico | GLC de JSON, XML y del metalenguaje de esquemas; parser LL(1); árboles de derivación. |
| 3. Análisis semántico | Validación contra el meta-esquema, resolución de `$ref`/`$defs`, detección de ciclos no productivos, clasificación de palabras clave. |
| 4. Autómata ponderado | M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) con pesos en el semianillo tropical (ℕ ∪ {∞}, mín, +, ∞, 0); construcción composicional; verificación de determinismo y minimización. |
| 5. Generación de código | Emisores C y Rust con tablas densas, pila acotada, API C (`sv_init`/`sv_feed`/`sv_finish`), generador de payloads. WASM era opcional y no se hizo. |
| 6. Validación en tiempo real | Streaming por bloques, errores con JSON Pointer y posición, límites, métricas (MB/s, ciclos/byte, memoria, asignaciones, profundidad). |
| 7. Aplicación web | API REST + WebSocket, editor de esquemas, visualización del autómata y del árbol de derivación, panel comparativo, sandbox, despliegue en la nube. |

La documentación técnica (definición formal, AFD, DPDA, gramáticas, árboles de derivación) empieza en
[docs/README.md](docs/README.md); la arquitectura por crates, en [docs/arquitectura.md](docs/arquitectura.md).

### Clasificación de palabras clave

| Categoría | Palabras clave | Estrategia |
|---|---|---|
| (a) Directas en el DPDA | `type`, `properties`, `items`, `enum`, `const`, límites numéricos (`minimum`, `maximum`, `exclusive*`, `multipleOf`) | Transiciones del autómata. |
| (b) Con registros o contadores | `required`, `minLength`, `maxLength`, `minItems`, `maxItems`, `minProperties`, `maxProperties` | Máscara de bits / contador en el marco de pila. |
| (c) No deterministas o no libres de contexto | `anyOf`, `oneOf`, `allOf`, `pattern`, `uniqueItems`, dependencias | Producto de autómatas, determinización acotada o verificación diferida. |

## Estructura del repositorio

```
├── crates/
│   ├── lexer/          # Fase 1
│   ├── schema-parser/  # Fase 2
│   ├── semantic/       # Fase 3
│   ├── ir/             # Fase 3: IR común de JSON Schema y XSD
│   ├── xsd/            # Fases 2-3: frontal XSD (lector XML con espacios de nombres, …)
│   ├── automaton/      # Fase 4
│   ├── codegen/        # Fase 5
│   ├── runtime-c/      # Núcleo C11 del validador generado (port de runtime-rs)
│   ├── runtime-rs/     # Núcleo Rust no_std del validador (intérprete y generado)
│   ├── baseline/       # Fase 6: líneas base DOM (serde_json + jsonschema, libxml2)
│   ├── bench/          # Fases 5-6: métricas, benchmarks, generador y mutaciones
│   ├── derivation/     # Fase 5: árboles de derivación de payloads
│   ├── atlas/          # Fase 7: carga de casos y métricas en MongoDB Atlas
│   └── cli/            # Herramienta de línea de comandos `schemac`
├── fuzz/               # Fase 6: objetivos de cargo-fuzz (nightly, fuera del workspace)
├── server/             # Backend REST (axum)
├── web/                # Frontend (React + Vite)
├── sandbox/            # Imagen aislada para compilar/ejecutar código generado
├── tests/              # Casos de prueba (se cargan en MongoDB Atlas)
├── docs/               # Documentación técnica (índice en docs/README.md)
└── infra/              # Docker Compose, Caddy y despliegue en EC2
```

## Tecnologías

- **Compilador y validadores:** Rust (compilador), C11 y Rust `no_std` (código generado).
- **Backend:** Rust + axum (REST).
- **Frontend:** React + TypeScript + Vite, Monaco Editor, Cytoscape.js, D3.
- **Base de datos:** MongoDB Atlas.
- **Nube:** AWS EC2 con Docker Compose y Caddy (despliegue en curso, F7.7).
- **Documentación:** Markdown, Graphviz y Mermaid.

## Requisitos

- [Rust](https://rustup.rs/) estable
- GCC o Clang con soporte C11
- Node.js 20+ y npm
- Docker y Docker Compose
- Graphviz (para generar los diagramas de autómatas)

## Uso

```bash
# Compilar todo el workspace
cargo build --workspace

# Ejecutar las pruebas
cargo test --workspace

# Analizar un esquema (sintaxis, meta-esquema 2020-12 restringido, $ref y ciclos) y listar sus errores
cargo run -p cli -- check esquema.json

# Bajar el esquema a la IR verificada y escribirla en JSON estable (o en DOT con --dot),
# con la clase (a)/(b)/(c) y la estrategia de cada nodo (docs/fase3/clasificacion.md)
cargo run -p cli -- ir esquema.json
cargo run -p cli -- ir --dot esquema.json | dot -Tsvg > ir.svg

# Construir el autómata M: resumen de tamaños en JSON, o M completo en DOT
cargo run -p cli -- automaton esquema.json
cargo run -p cli -- automaton --dot esquema.json | dot -Tsvg > m.svg

# Compilar un esquema a un validador autocontenido en Rust o en C, con límites opcionales
cargo run -p cli -- compile esquema.json --lang c --out build/ --limit maxDepth=64
cc -std=c11 -O2 -Wall -Wextra -o build/validador build/*.c
build/validador < payload.json      # una línea JSON; código 0 válido, 1 violaciones, 2 malformado o límite

# Levantar la aplicación en local (servidor en 127.0.0.1:8080; la web en web/ con `npm run dev`)
cargo run -p server --release
docker compose -f infra/docker-compose.yml up -d   # o todo en contenedores, tras `build`
```

## Desarrollo

```bash
make lint        # rustfmt --check + clippy -D warnings + clang-format
make test        # cargo test --workspace
make coverage    # cargo llvm-cov (requiere `cargo install cargo-llvm-cov`)
make docs        # regenera diagramas DOT/SVG y tablas LL(1) de docs/ (requiere Graphviz)
make docs-check  # comprueba que los .dot y ll1-*.md versionados están sincronizados
make doradas     # regenera las IR doradas de tests/ir (tras revisar el cambio)
make fuzz        # cargo-fuzz, 60 s por objetivo (SEG=600 make fuzz); requiere nightly y cargo-fuzz
make test-baseline-xml  # línea base XML con libxml2 en Docker
make test-sandbox       # pruebas del sandbox con contenedores reales
```

Códigos de salida de `schemac check` y `schemac ir`: 0 bien (también si el lector cierra la salida antes de tiempo, como `| head`); 1 el esquema tiene errores (los mismos en los dos comandos); 2 no se pudo leer el archivo, no es un archivo regular o pasa de 16 MiB, los argumentos son inválidos, no se pudo escribir la salida, no se pudo crear el hilo de análisis o, solo en `check`, un fallo interno (pánico); 3 error interno del compilador en `schemac ir` (la IR bajada no cumple sus invariantes o un pánico en su hilo, [docs/fase3/bajada.md](docs/fase3/bajada.md) §4).

En Windows sin `make`: `powershell -File scripts/lint.ps1`. No hay CI: toda la verificación es local (D32).

- Subconjunto soportado de JSON Schema y XSD: [docs/00-alcance.md](docs/00-alcance.md)
- Formato de los casos de prueba: [tests/README.md](tests/README.md)
- Cómo trabajamos en pareja: [COLABORAR.md](COLABORAR.md); plan y decisiones: [tasks/plan-general.md](tasks/plan-general.md)

## Casos de prueba

Cada caso guarda esquema, payload, resultado esperado y métricas, y se almacena en la base NoSQL en la nube. Las categorías son:

- **Válidos:** esquemas simples, anidados y recursivos (`$ref`, árboles) en JSON y XML.
- **Inválidos:** tipos incorrectos, campos obligatorios faltantes, propiedades adicionales, violaciones de enum y rangos.
- **Malformados:** sintaxis incorrecta, UTF-8 inválido, claves duplicadas, etiquetas XML sin cerrar.
- **Fuera del DPDA puro:** `anyOf`/`oneOf` ambiguos, `pattern` y `uniqueItems`, con su estrategia de respaldo.
- **Pruebas de bombeo:** payloads u·vᵏ·x·yᵏ·z con anidamiento creciente, arreglos muy largos y carga sostenida.
- **Extremos:** payloads muy grandes en streaming, profundidad sobre el límite, números enormes y expansión de entidades XML.

## Entregables

1. Documentación técnica: definición formal del lenguaje, diagramas de autómatas (AFD, DPDA), gramáticas, arquitectura del compilador y árboles de derivación.
2. Código fuente: compilador y validadores generados, con suite completa de pruebas.
3. Aplicación web desplegada en la nube, con los casos de prueba en la base NoSQL.
4. Informe final: complejidad (O(n) en bytes, O(d) en pila), tamaño del autómata, decisiones de diseño, vulnerabilidades mitigadas y resultados de las pruebas de bombeo.
5. Presentación con demo en vivo y métricas comparativas.

## Estado

Fases 1-6 cerradas. De la Fase 7 faltan la descarga del código con validación por WebSocket (F7.5) y el
despliegue en EC2 (F7.7). El avance por tarea está en
[tasks/CHECKLIST.md](tasks/CHECKLIST.md).

## Autores

- Leonardo — [@leonardo079](https://github.com/leonardo079)
- Bayron -[@Bayronfc05](https://github.com/Bayronfc05)
