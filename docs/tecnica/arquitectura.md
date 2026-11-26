# Arquitectura del compilador

Workspace de Cargo con 15 paquetes (`Cargo.toml`), más el núcleo C (`crates/runtime-c`), la web (`web/`), la
imagen del sandbox (`sandbox/`) y el despliegue (`infra/`).

## 1. Tubería

```
                Fase 1            Fase 2               Fase 3                         Fase 4            Fase 5
 esquema ─▶ dtd::formato ─▶ lexer ─▶ parser LL(1) ─▶ AST ─▶ análisis ─▶ bajada ─▶ VerifiedIr ─▶ automaton::build ─▶ M ─▶ codegen ─▶ validador C / Rust
  (bytes)    (JSON/XSD/DTD)                                (meta-esquema,    (Ir::verify)        (tablas densas,          (tablas + núcleo
                                                            $ref, ciclos,                        determinismo,             no_std copiado)
                                                            UPA, Glushkov)                       Hopcroft)

                                                                     Fase 6
 payload ─▶ lexer (no_std) ─▶ eventos ─▶ núcleo runtime-rs / runtime-c (δ, pila en memoria fija, pesos) ─▶ reporte
  (bloques)
```

Lo que va a la izquierda de `VerifiedIr` es de **tiempo de compilación**: puede reservar memoria y construir el
AST, pero no recursa sin cota sobre la entrada. Lo que va en la última línea es el **validador**: `no_std`, sin
reservas, sin recursión y O(1) por byte.

## 2. Paquetes

| Paquete | Fase | Responsabilidad | Depende de (internos) |
| --- | --- | --- | --- |
| `lexer` | 1 | AFD de UTF-8, JSON y XML en streaming; `no_std` sin `alloc`; exportación DOT | — |
| `schema-parser` | 2 | parser LL(1) de JSON Schema → AST | `lexer` |
| `semantic` | 3 | meta-esquema, tabla de símbolos, `$ref`, ciclos, bajada JSON Schema → IR | `ir`, `schema-parser` |
| `xsd` | 2-3 | lector XML con espacios de nombres, parser LL(1) de XSD, análisis (símbolos, tipos simples, UPA) y bajada | `ir`, `lexer` |
| `dtd` | 1-3 | lexer, parser, análisis (determinismo por Glushkov) y bajada de DTD; detección de formato | `ir`, `lexer` |
| `ir` | 3 | IR en arena, verificador I1–I14, clasificación (a)/(b)/(c), números XSD, expresiones regulares | `lexer` |
| `automaton` | 4 | construcción de M, Glushkov con contadores, `pattern` → AFD, Hopcroft, verificación del determinismo, DOT | `ir`, `lexer`, `runtime-rs` |
| `runtime-rs` | 4-6 | formato de las tablas y núcleo `no_std` del ejecutor: pila, pesos, límites, capa de buena formación, reporte | `lexer` |
| `runtime-c` | 5 | port C11 del núcleo (no es un paquete de Cargo: lo copia `codegen`) | — |
| `codegen` | 5 | emisores Rust y C11 | `automaton`, `lexer`, `runtime-rs` |
| `cli` (`schemac`) | 1-5 | `check`, `ir`, `automaton`, `compile`, `dot` | casi todos |
| `derivation` (`arbol`) | 2 | árboles de derivación LL(1) de payloads, JSON y DOT | `lexer` |
| `baseline` | 6 | líneas base DOM: `serde_json` + `jsonschema`, libxml2 | `dtd`, `schema-parser` |
| `bench` | 5-6 | métricas, bombeo, gráficas, generadores y mutaciones de payloads, *benchmark* del parser | varios |
| `atlas` | 7 | carga de casos y métricas en MongoDB Atlas | `baseline` |
| `server` | 7 | API REST y WebSocket (axum), sandbox, ZIP | casi todos |

(Dependencias internas tomadas de `cargo metadata`.)

**Decisiones de estructura:**

- **Un AST por lenguaje y una IR común.** JSON Schema, XSD y DTD tienen cada uno su AST. Los tres bajan a la misma
  IR, y desde ahí la Fase 4 y la 5 son comunes. Añadir la DTD no tocó `automaton` ni `codegen`.
- **Un solo ejecutor.** El intérprete de las pruebas, el validador Rust generado y el servidor usan el **mismo**
  `runtime-rs`; el C es un port función por función, y la prueba diferencial vigila que los tres coincidan.
- **`lexer` y `runtime-rs` son `no_std` sin `alloc`**, porque se copian dentro del validador generado.
- **El compilador nunca confía en sí mismo.** La IR se verifica antes de construir M (I1–I14) y M se verifica
  después (`determinism::check`). Un fallo ahí es un error interno (código 3), no del usuario.

## 3. Invariantes del código

| Invariante | Cómo se hace cumplir |
| --- | --- |
| Validación O(n) en bytes y O(d) en pila; sin árbol del payload | diseño del núcleo; `bench` lo mide |
| Sin asignación dinámica al validar | `no_std` sin `alloc`; `crates/automaton/tests/sin_reservas.rs` cuenta las reservas |
| Determinismo: una transición por (q, a) | δ es una tabla; `determinism::check` en cada `build` |
| El resultado no depende del tamaño de los bloques | pruebas con bloques de 1, 7 y 4 096 bytes |
| Sin `unsafe` | `unsafe_code = "deny"` en el workspace; `make unsafe-check` permite solo el asignador contador de `sin_reservas.rs` |
| Nada recursa sin cota sobre la entrada | pilas explícitas; los parsers recursivos, acotados a 256 y en hilos de 8 MiB |
| Pruebas deterministas | sin umbrales de tiempo en `cargo test`; los tiempos solo en `bench` |
| Diagramas y tablas LL(1) generados desde el código | `make docs` y `make docs-check` |

## 4. Herramientas

| Orden | Qué hace |
| --- | --- |
| `make lint` | `cargo fmt --check`, `clippy -D warnings`, `clang-format` del C y `unsafe-check` |
| `make test` | `cargo test --workspace` (unas 930 funciones de prueba entre unitarias e integración) |
| `make docs` / `make docs-check` | regenera o compara los `.dot`, `.svg` y `ll1-*.md` |
| `make test-baseline-xml` | la línea base libxml2 en Docker, sin red |
| `make test-sandbox` | el sandbox con contenedores reales |
| `make fuzz` | cargo-fuzz (*nightly*) y reproducción en C |
