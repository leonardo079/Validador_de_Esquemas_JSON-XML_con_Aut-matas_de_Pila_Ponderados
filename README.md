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
 │ AFD streaming│   │ Parser LL(1) │   │ $ref, ciclos,│   │ DPDA         │   │ C / Rust /   │
 │ UTF-8, tokens│   │ → AST        │   │ clasificación│   │ ponderado    │   │ WASM         │
 └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘
                                                                                     │
                     Payload (bytes) ──▶ validador generado ──▶ conforme / violaciones + JSON Pointer
```

| Fase | Contenido |
|---|---|
| 1. Análisis léxico | Alfabeto de bytes UTF-8, tokens JSON y XML, AFD del tokenizador en streaming, gramáticas regulares. |
| 2. Análisis sintáctico | GLC de JSON, XML y del metalenguaje de esquemas; parser LL(1); árboles de derivación. |
| 3. Análisis semántico | Validación contra el meta-esquema, resolución de `$ref`/`$defs`, detección de ciclos no productivos, clasificación de palabras clave. |
| 4. Autómata ponderado | M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) con pesos en el semianillo tropical; construcción composicional; verificación de determinismo y minimización. |
| 5. Generación de código | Emisores C y Rust (tablas densas o saltos directos), pila acotada, API C-ABI/FFI, WASM opcional, generador de payloads. |
| 6. Validación en tiempo real | Streaming por bloques, errores con JSON Pointer y posición, límites, métricas (MB/s, ciclos/byte, memoria, asignaciones, profundidad). |
| 7. Aplicación web | API REST + WebSocket, editor de esquemas, visualización del autómata y del árbol de derivación, panel comparativo, sandbox, despliegue en la nube. |

### Clasificación de palabras clave

| Categoría | Palabras clave | Estrategia |
|---|---|---|
| (a) Directas en el DPDA | `type`, `properties`, `items`, `enum`, `const`, límites | Transiciones del autómata. |
| (b) Con registros o contadores | `required`, `minItems`, `maxItems` | Máscara de bits / contador en el marco de pila. |
| (c) No deterministas o no libres de contexto | `anyOf`, `oneOf`, `allOf`, `pattern`, `uniqueItems`, dependencias | Producto de autómatas, determinización acotada o verificación diferida. |

## Estructura del repositorio

> Estructura prevista; se irá completando durante el desarrollo.

```
├── crates/
│   ├── lexer/          # Fase 1
│   ├── schema-parser/  # Fase 2
│   ├── semantic/       # Fase 3
│   ├── automaton/      # Fase 4
│   ├── codegen/        # Fase 5
│   ├── runtime-c/      # Soporte C del validador generado
│   ├── runtime-rs/     # Soporte Rust no_std del validador generado
│   ├── bench/          # Fase 6: métricas y línea base DOM
│   └── cli/            # Herramienta de línea de comandos `schemac`
├── server/             # Backend REST + WebSocket
├── web/                # Frontend
├── sandbox/            # Imagen aislada para compilar/ejecutar código generado
├── tests/              # Casos de prueba (se cargan en la base NoSQL)
├── docs/               # Documentación técnica e informe
└── infra/              # Infraestructura como código y despliegue
```

## Tecnologías

- **Compilador y validadores:** Rust (compilador), C11 y Rust `no_std` (código generado), WASM (opcional).
- **Backend:** Rust + axum (REST + WebSocket).
- **Frontend:** React + TypeScript + Vite, Monaco Editor, Cytoscape.js, D3.
- **Base de datos:** NoSQL en la nube (MongoDB Atlas, DynamoDB o Cosmos DB).
- **Nube:** AWS o Azure.
- **Documentación:** Markdown, LaTeX y Graphviz.

## Requisitos

- [Rust](https://rustup.rs/) estable (1.80+)
- GCC o Clang con soporte C11
- Node.js 20+ y npm
- Docker y Docker Compose
- Graphviz (para generar los diagramas de autómatas)

## Uso

> Pendiente: se documentará a medida que se implementen las fases.

```bash
# Compilar todo el workspace
cargo build --workspace

# Ejecutar las pruebas
cargo test --workspace

# Compilar un esquema a un validador en C (previsto)
cargo run -p cli -- compile esquema.json --lang c --out build/

# Levantar la aplicación web en local (previsto)
docker compose up
```

## Desarrollo

```bash
make lint        # rustfmt --check + clippy -D warnings + clang-format
make test        # cargo test --workspace
make coverage    # cargo llvm-cov (requiere `cargo install cargo-llvm-cov`)
```

En Windows sin `make`: `powershell -File scripts/lint.ps1`. La CI (GitHub Actions) ejecuta lint, tests en Linux y Windows, comprobación de MSRV y cobertura en cada PR.

- Subconjunto soportado de JSON Schema y XSD: [docs/00-alcance.md](docs/00-alcance.md)
- Formato de los casos de prueba: [tests/README.md](tests/README.md)
- Issues del plan: `python scripts/crear_issues.py --plan <ruta al plan>` (simulación; `--crear` con `GITHUB_TOKEN` para crearlos)

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

🚧 En desarrollo.

## Autores

- Leonardo — [@leonardo079](https://github.com/leonardo079)
