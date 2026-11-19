# Plan de implementación: tarea 5 de la semana 2 — frontal XSD (parser → AST → IR)

Fecha: 28 oct 2026. Fuente: «Retomar aquí» del plan general, D3, D13-D16, D19,
`docs/fase3/ir.md` §4-§5.2 y §11, `docs/fase3/pendientes.md` §3, §22 y §26,
`docs/00-alcance.md` §1.2, §3 y §4.

## Resumen
Hoy solo JSON Schema llega a la IR. Esta tarea añade el segundo frontal (D3): un lector XML
de esquemas con espacios de nombres, una gramática LL(1) del subconjunto XSD con su AST
propio, el análisis que rechaza con posición toda *Schema Component Constraint* del
subconjunto (D15; `verify` queda como defensa en profundidad) y la bajada a la IR común de
`ir.md` §5.2, expuesta por `schemac check|ir`. Cierra el hito de la semana 2 («un esquema
JSON o XSD sale como IR verificada») y la entrada XSD del hito de la semana 3.

## Grafo de dependencias
5.1 lector XML + espacios de nombres
 └── 5.2 gramática LL(1) XSD + parser → AST (docs/fase2)
      └── 5.3 nombres, restricciones estructurales (I10-I12, D14) y bajada de tipos complejos
           └── 5.4 schemac check|ir con XSD, corpus_clasificado, doradas, ρ
                │   ── Punto de control C (imprescindible para la semana 3) ──
                ├── 5.6 tipos simples, facetas, ciclos de base, default/fixed ◀── 5.5 números XSD (independiente)
                └── 5.7 UPA y cierre del frontal (docs, alcance, pendientes)
                    ── Punto de control D (casilla cerrada) ──
            (Fase 4) constructor del DPDA — consume la IR verificada y clasificada

## Decisiones de arquitectura
- Crate `crates/xsd` (lexer + ir), sin depender de `semantic` (D13, `ir.md` §1) — U1.
- Formato del esquema detectado por el contenido (`<` inicial) en `schemac` y el servidor — U2 (toca D17).
- Parser LL(1) descendente al estilo de `schema-parser`, con tope de profundidad 256 en un hilo con pila explícita (D7);
  análisis y bajada iterativos, O(n) salvo UPA, acotada por un límite de partículas (U6).
- Errores con `code()` estable y por etapas, como `semantic::analyze`; un error de `verify` es siempre interno (D15, código 3).
- Bajada determinista según `ir.md` §5.2; predefinidos y envoltorios de contenido simple creados una vez, al usarse.
- Comparación decimal exacta en `ir::num` (la reutiliza la Fase 4); `xs:float`/`xs:double` redondeados a IEEE (I-2).

## Riesgos y mitigaciones
| Riesgo | Impacto | Mitigación |
| --- | --- | --- |
| La semana 2 termina sin XSD → IR (señal del plan general) | Alto | 5.1-5.4 primero (punto C); 5.5-5.7 aplazables según U8 |
| UPA o FIRST/FOLLOW cuadráticos ante un esquema hostil | Alto | Límite de partículas por modelo (U6), comprobado antes de calcular; prueba sin reloj (D1) |
| «Toda SCC del subconjunto» es más de lo que cabe | Medio | Lista explícita en `ir.md` §4; `pattern` en valores aplazado a la tarea `pattern` → AFD (U9) |
| Un XSD del usuario llega a `verify` y sale como error interno (código 3) | Medio | Cada tarea que baja una construcción comprueba en el frontal los invariantes que la tocan; prueba de corpus «nunca `InternalError`» |
| Espacios de nombres y QName (prefijos arbitrarios, espacio por defecto, `targetNamespace`) | Medio | 5.1 los resuelve por URI; casos con `xsd:`, espacio por defecto y `tns:` en el corpus |
| Desbordamiento de pila en debug con anidamiento profundo | Medio | Tope 256 medido en un hilo de 2 MiB (`fase2/pendientes.md` §5), o parser iterativo (B1) |
| Romper la salida JSON de `schemac` al integrar XSD | Bajo | Las doradas de `tests/ir/` no deben cambiar (criterio de 5.4) |

## Decisiones del usuario
U1-U9 aprobadas el 29 oct como D21-D29 del plan general (todas según la recomendación del Planner).

## Decisiones que puede tomar el Builder (B1-B7; se registran en el plan general al tomarlas)

- **B1. Parser y AST.** Imitar `schema-parser`: descendente recursivo LL(1), con tope de profundidad 256 (`MAX_DEPTH_LIMIT`, solo configurable a la baja), garantizado en un hilo de 2 MiB y medido en debug, como en `fase2/pendientes.md` §5. O bien un parser iterativo. En los dos casos, el AST sin `Drop` recursivo profundo.
- **B2. Errores.** Un tipo propio con `code()` estable (`lexical`, `syntax`, `unsupported`, `duplicate-name`, `unresolved`, `cycle`, `facet`, `value`, `content-model`, `upa`, `limit`…) y mensajes de una línea con `lexer::short`.
  - La Fase 2 se detiene en el primer error.
  - El análisis informa por etapas, todos los errores de la primera etapa que falla, igual que `semantic::analyze`: nombres y resolución → tipos simples → modelos de contenido.
- **B3. Bajada determinista.**
  - Orden de los `NodeId`: el documento, después las declaraciones en preorden, después los nodos sintéticos (`builtin`, `simpleContent/xs:…`) por orden de primer uso.
  - Los predefinidos se crean solo si se usan, como en `pedido_xsd`.
  - Origen de un tipo anónimo dentro de un `xs:element` global (S-5, primera mitad): padre el documento y paso `Entry("element", nombre)`.
- **B4. Comparación decimal exacta y aritmética de lexemas** (para facetas «valid restriction», rangos de los predefinidos y `default`/`fixed`). Recomiendo ponerla en `crates/ir` (`ir::num`), porque la Fase 4 también compara por valor. Alternativa: en `crates/xsd`.
  - `xs:float`/`xs:double`: guardar en `Num` el decimal más corto del valor redondeado (`f32`/`f64` de `std`, sin `unsafe`), o el lexema exacto con una nota de que la Fase 4 redondea los dos lados. En cualquier caso hay que corregir `ir.md` §5.2, §7 (fila Q) y §11.3.
- **B5. Espacios de nombres.** Ámbito completo de las declaraciones `xmlns`/`xmlns:p` con una pila.
  - Resolución de los QName (`type`, `base`, `ref`) por (URI, nombre local).
  - Los tipos complejos y los simples comparten un mismo espacio de símbolos; los elementos tienen otro.
  - Normalización de espacios de los valores de atributo según su tipo XSD (el lexer no la hace).
- **B6. Pruebas.**
  - Corpus en `tests/schemas/xsd/{parse-ok,parse-error,semantic-error}`, con manifiestos `errores-xsd.json` y `errores-semanticos-xsd.json` (línea y columna), en el estilo de `tests/schemas/errores.json`.
  - El equivalente de `corpus_clasificado` en `crates/xsd/tests/corpus.rs`, que lee los casos `format: "xml"` de `tests/cases`.
- **B7. Algoritmo de UPA y de *Element Declarations Consistent* (EDC).** Iterativo, sin recursión sobre el árbol de partículas (D7).

---

# Plan de F4.1 (Fase 4): definición formal de M, construcción composicional y pesos tropicales

Fecha: 15 nov 2026. Planner del 15 nov; aprobado por Leonardo el 15 nov con las recomendaciones (F4-U1…U8 = D47 del plan general). Las etiquetas llevan el prefijo `F4-` para no confundirlas con las U1-U9/B1-B7 de la tarea 5 de arriba. F4-U3, F4-U6 y F4-U7 son interfaz compartida (D42): provisionales hasta que Bayron las apruebe (al cerrar F4.1a).

## Decisiones del usuario (aprobadas)

- **F4-U1. Alfabeto:** Σ = clases de **eventos del lexer** (el lexer `no_std` va delante como transductor finito determinista). Estructurales como clases fijas; `StringChar`/`NameChar`/`AttrValueChar`/`TextChar` agrupados en clases de punto de código calculadas por esquema (partición inducida por tries, espacios léxicos y AFD de `pattern`). Una pasada: cada byte entra una vez al lexer y cada evento una vez a δ; O(1) amortizado por byte. Componer el transductor con el DPDA da un DPDA (se documenta; no se pliega para no multiplicar estados). «Tablas densas por clase de byte» de F5.1 = dos niveles densos (lexer y δ); el C generado (F5.2) lleva también el lexer.
- **F4-U2. `$ref` y E₌:** expansión en compilación memorizada por **conjunto canónico de `NodeId`** (cierre conjuntivo C(v): v y los destinos de `Ref` y `AllOf`; finito por I14; componentes = `NodeId` distintos, S-3). Marco = producto de los autómatas de sus reglas; despacho de claves e índices componente a componente (§6.2); conjunto vacío → marco «cualquiera»; cada `Constraint` cuenta su propia violación. Sin ε. `$ref` sin hermanas = alias. `$ref` con hermanas y `allOf` pasan a F4.1b; F4.5 se queda con `anyOf`, `oneOf` (diferida), `not` (inversión al desapilar), literales compuestos, `uniqueItems`, `dependentSchemas` y el respaldo diferido al pasar el tope. Cierra §27 y la parte JSON de §22.
- **F4-U3. Pesos (D4)** (interfaz compartida, provisional): w(t) ∈ ℕ ∪ {∞} = número de comprobaciones que fallan en t: **1 por `Constraint` violada por instancia** (`ir.md` §7). ∞ = malformado o límite (absorbe ⊗; por eso el corpus omite `violations` en `lexical`/`syntax`; un límite atraviesa `not` sin invertirse, S-2). Peso del recorrido = ⊗ (suma); ⊕ = mín solo entre alternativas (F4.5: `anyOf` = mín de las ramas). `not` y `oneOf` como veredicto al desapilar. **Costo:** segundo peso estático c: `RuleKind` → ℕ (tabla con `match` exhaustivo) que ordena las comprobaciones de cada transición (baratas primero; fallo temprano). **Dos modos** sobre el mismo M: conformidad total (sigue tras una violación con recuperación uniforme: clave prohibida suma 1 y el hijo va con el marco «cualquiera»; elemento XML inesperado suma 1, se salta su subárbol y el Glushkov no se mueve; un `type` incumplido no suspende las demás reglas) y fallo temprano (se detiene en la primera transición con w > 0). `expected.violations` del corpus se lee en modo total, «por regla» (avisar a Bayron: `tests/README.md` y su ejecutor).
- **F4-U4. Representación:** F4.1 construye M ya como **tablas**: `δ[q][clase] → TransId` total (error de peso ∞ por defecto); transición = (q', operación de pila, operaciones de registros, comprobaciones ordenadas por c); marcos con disposición de registros fija y metadatos para el DOT; simulador std sobre eventos para probar L(M). F4.6 = intérprete `no_std` de esas tablas en `runtime-rs` (lexer, límites, reporte, bloques); F5.1 = tablas como `const` + ese intérprete; F5.2 lo porta a C. Las rutinas numéricas `no_std` (comparación decimal, `is_integer`, `multipleOf`) viven en `runtime-rs` desde F4.1b.
- **F4-U5. Lo no implementado:** rechazo en compilación `BuildError::Pending { nodo, regla, tarea }` con ubicación recortada; `schemac automaton` da código 1 («estrategia pendiente: pattern (F4.4) en #/properties/x»). Reparto: `pattern` → F4.4; `anyOf`, `oneOf`, `not`, `enum`/`const` compuestos, `uniqueItems`, `dependentSchemas` → F4.5.
- **F4-U6. Límites nuevos en `00-alcance.md` §4** (interfaz compartida, provisional): presupuesto de transiciones por esquema 2²⁰ (contado antes de reservar, `limit`); tope de `location_clipped` 256 bytes; divisor de `multipleOf` con parte coprima con 10 < 2⁶⁰ (si no, `limit` con posición); como mucho 4 096 nombres por objeto (`properties` ∪ `required` ∪ `dependentRequired`; máscara de 512 B). Se proponen a Bayron al cerrar F4.1a.
- **F4-U7. Salida:** `schemac automaton [--dot] <esquema>`: JSON pequeño de tamaños (estados, transiciones, marcos, clases, bytes de registros y de marco máximo, productos, pendientes) + DOT completo (interfaz compartida, provisional). El JSON del grafo para Cytoscape se diseña con Bayron en la Fase 7.
- **F4-U8. Orden:** F4.1a → F4.1b → F4.1c. F4.3 y F4.4 pueden empezar tras F4.1a (solo necesitan el tipo `Dfa`). Si F4.1c se atrasa: D28 (2), facetas de rango de `xs:float`/`xs:double` como pendientes.

## Decisiones que puede tomar el Builder (registrarlas en `docs/fase4/`)

- **F4-B1. Dependencias:** `automaton` depende de `ir` y `lexer` (y de `runtime-rs` desde F4.1b); `semantic` pasa a dependencia de pruebas (hoy `crates/automaton/Cargo.toml` depende de `semantic`, en contra de D13/D43). §13 (`AnalyzedTable`) sigue abierta: `automaton` no llama a `lower`.
- **F4-B2. Tipos:** `StateId(u32)`, `FrameId(u32)`, `TransId(u32)`, `ClassId(u16)` sobre arenas `Vec`; δ total; un `automaton::dfa::Dfa` propio (estados × clases → estado, con salidas) para tries y espacios léxicos, que F4.3 minimiza y F4.4 produce desde `pattern`.
- **F4-B3. Clases de caracteres por esquema:** rangos ordenados → clase, separados por contexto (clave o valor en JSON; nombre, valor de atributo o texto en XML).
- **F4-B4. Registros:** por marco: máscara de declared ∪ required ∪ claves y listas de `dependentRequired` (S-1); contadores de miembros, ítems y ocurrencias en `u64` saturado; máscara de `xs:all` y de atributos; id del nombre del elemento. Globales (un solo escalar abierto a la vez): el lexema del número (≤ 1 024 bytes) y el contador de puntos de código.
- **F4-B5. Pila:** una instancia = un marco; se apila con el primer evento de su valor (aplicándolo) y se desapila con el último; la altura depende solo del payload.
- **F4-B6. Costo:** valores concretos de c por `RuleKind`; empates por el orden de las reglas.
- **F4-B7. Capa de buena formación del runtime (F4.6), fuera de M:** claves no declaradas repetidas (hash acotado por `max_claves_adicionales`; las declaradas repetidas las detecta la máscara con peso ∞); atributos XML repetidos y nombres de etiquetas de elementos saltados.
- **F4-B8.** `location_clipped(&VerifiedIr, id, max)` como función de `automaton` sobre `Origin` (público); no cambia la API de `ir`.
- **F4-B9. Construcción iterativa (D7):** lista de trabajo de marcos por conjunto canónico (`Vec<NodeId>` ordenado con hash) y pilas explícitas.
- **F4-B10.** DOT: un clúster por marco.
- **F4-B11.** `BuildError::{Pending, Limit, Internal}` (`Internal` para la defensa en profundidad, p. ej. un Glushkov no determinista).
- **F4-B12. Números:** comparación decimal exacta sobre el registro del lexema; entero = fracción nula tras desplazar por el exponente saturado; `multipleOf` = resto módulo M′ en `u64` más valuaciones de 2 y 5 frente al exponente; `xs:float`/`xs:double` con umbrales decimales m± calculados en compilación con `ir::num` (5.5-CR-S1) y la regla de NaN/INF (5.5-P3).
- **F4-B13. XML:** trie de nombres + tabla Glushkov (x, nombre) → (y, actualización, comprobaciones) (determinismo fuerte por la enmienda de D25: un destino y una actualización por par; las guardas solo suman peso); declaración `<?xml …?>` en el marco del documento; nombres por su parte local; `xmlns`/`xmlns:*` no cuentan como atributos.

## Riesgos y recortes

1. Calendario (congelamiento 19 nov; faltan F4.1a-c, F4.3, F4.4, F4.5, F4.6, F5.1-F5.3). Mitigación: F4-U2, U4, U8. Orden de recorte que cumple el PDF: (1) JSON de resumen en vez del grafo para la web; (2) F4.3 limitado a determinismo y Hopcroft sobre los `Dfa`; (3) respaldo de `anyOf`/`allOf` por encima del tope como simulación paralela de ramas; (4) D28 (2) para las facetas de `xs:float`/`xs:double`. No se recortan los emisores C ni Rust.
2. Explosión de marcos y tamaño de `sv_state_t`: tope por producto contado antes de reservar, presupuesto global (F4-U6), escalares globales (F4-B4), informe del tamaño del marco máximo.
3. Glushkov cuadrático (5.7-P1): presupuesto de transiciones y tabla (x, nombre).
4. Corrección numérica: proptest contra `ir::num` y los casos frontera de `numeros-xsd.md` §6.
5. Huecos fuera de M hasta F4.6 (claves no declaradas repetidas, atributos repetidos, etiquetas saltadas): criterio de F4.6.
6. Interfaces compartidas sin aprobar (F4-U3, U6, U7; además D45 y los cambios del alcance pendientes de Bayron): provisionales hasta cerrar F4.1a.
7. El C generado lleva también el lexer (F5.2): sus tablas ya son densas.
8. XML: espacios de nombres en el payload: solo nombre local; si no basta para `00-alcance.md` §1.2, desviación a proponer al cerrar F4.1c (ante la duda, rechazar).
9. F4.6 depende de F6.7 (Bayron); si no llega, el oráculo se compara con los `expected` del corpus.

---

# F6.7: líneas base (pista B, Bayron; Planner del 13 nov, aprobado por Bayron el 13 nov)

Criterios en `tasks/todo.md` («Tarea F6.7»). F6.7a (JSON, crate `crates/baseline`) primero; F6.7b (XML con libxml2, feature `libxml2`, en Docker) después.

## Entorno de la máquina de Bayron (13 nov)

Rust 1.100.0 `x86_64-pc-windows-msvc` (fijado por `rust-toolchain.toml`), MSVC Build Tools 2022. **No hay** `make`, `xmllint`/libxml2, `pkg-config`, vcpkg, MSYS2, `clang`/libclang, `gcc`, `clang-format` ni `dot`; el `python3` del PATH es el stub de WindowsApps (Python 3.14.7 real en `AppData\Local\Python\bin`). Docker instalado con el daemon parado. **Smart App Control activo:** bloquea de forma intermitente ejecutables recién compilados (`os error 4551`); si una prueba falla así, se repite. Sin `make`: `scripts/lint.ps1` y `cargo test --workspace`.

## Decisiones del usuario (aprobadas el 13 nov con la opción recomendada)

- **U1.** Crate nuevo `crates/baseline`, no módulos en `bench` (F6.5 hará que `bench` dependa de `runtime-rs`/`codegen`, y F4.6 en `runtime-rs` necesita la línea base como dev-dependencia: en `bench` saldría el ciclo `runtime-rs` → `bench` → `runtime-rs`; `bench` ya depende de `automaton`; los ~65 crates de `jsonschema` solo entran donde hacen falta). Toca el `Cargo.toml` del workspace y el árbol de «Arquitectura y repositorio». Avisar a Leonardo.
- **U2.** API pública de `baseline` (la consume F4.6; **confirmar con Leonardo**):
  `pub enum Kind { Malformed, Schema, Limit, Unsupported }`;
  `pub struct Violation { pub pointer: String, pub keyword: String, pub message: String }`;
  `pub struct Malformed { pub line: u32, pub column: u32, pub offset: Option<u64>, pub message: String }`;
  `pub struct Outcome { pub valid: bool, pub kind: Option<Kind>, pub violations: Vec<Violation>, pub malformed: Option<Malformed> }` con `violation_count()`;
  `pub struct Limits { pub max_depth: u32 /* 256 */ }`;
  `JsonBaseline::compile(&serde_json::Value)` / `compile_text(&[u8]) -> Result<Self, CompileError>`, `parse(&self, &Stack, &[u8]) -> Result<JsonDom, Outcome>`, `validate_dom(&self, &Stack, &JsonDom) -> Outcome`, `validate(&self, &Stack, &[u8]) -> Outcome`; lo mismo para `XmlBaseline` con la feature `libxml2` (`!Send`: se crea dentro del hilo);
  `pub fn with_stack<R: Send>(f: impl FnOnce(&Stack) -> R + Send) -> R` lanza un hilo con `STACK_SIZE` y es el único modo de obtener el testigo `Stack` (el tipo hace cumplir D7 sin un hilo por llamada);
  `corpus::{casos(), Case { id, category, format, schema_text, schema_value, payload: Payload::{Bytes, Generator}, expected: Expected, strategy }}` y `run_case(&Case, &Stack) -> Result<Outcome, Unavailable>`.
- **U3.** Features de `serde_json` (D20, se unifican en el workspace): se acepta `float_roundtrip` (la impone `jsonschema`), se añade `unbounded_depth` (solo añade `disable_recursion_limit()`) y la profundidad la controla el contador propio del visitante (256). **No** se activa `arbitrary_precision`. Se registra como decisión nueva al cerrar; afecta a las pruebas de los dos (avisar a Leonardo).
- **U4.** XML con el crate `libxml` (0.3.23) tras la feature opcional `libxml2`, ejecutado en Docker/Linux. Alternativas descartadas: `xmllint` como subproceso; libxml2 nativo en Windows con vcpkg + LLVM.
- **U5.** Claves duplicadas (D2): visitante de deserialización propio que construye el `Value` y falla con `Error::custom` en el duplicado (línea y columna de `serde_json`).
- **U6.** D8 en XML: capa de política que rechaza cualquier nodo DTD como `Malformed` (si no, libxml2 da `billion-laughs` por válido).
- **U7.** `violations` = número de errores de `iter_errors`; se compara con el corpus con la tabla explícita `DIVERGENCIAS`; `valid` y `kind` se comparan siempre.
- **U8.** El corpus como oráculo certificado: F6.7 garantiza línea base = `expected`; F4.6 compara contra la línea base en JSON y contra `expected` en XML cuando falta libxml2. Cambia cómo se verifica el «Terminado cuando» de la Fase 4: **aprobación de Leonardo**.
- **U9.** Números que no caben en `f64` (`1e400`): `Kind::Unsupported` (carencia de la línea base; F4.6 los excluye), no `Malformed` (contradiría `00-alcance.md` §1.1).
- **U10.** F6.7 se parte en F6.7a y F6.7b; el `Depende` de F4.6 pasa a «F6.7a (JSON), F6.7b (XML)». La línea de F4.6 es de Leonardo: avisarle.

## Decisiones que puede tomar el Builder (se registran en el plan general al tomarlas)

- **B1.** Nombres de módulos y archivos internos (`json.rs`, `xml.rs`, `corpus.rs`, `limits.rs`) y de documentos en `docs/fase6/`.
- **B2.** Valor de `STACK_SIZE`: ≥ 8 MiB como `schemac`; comprobarlo con un `$ref` recursivo a profundidad 256.
- **B3.** Offset a partir de línea y columna de `serde_json` (bytes, base 1) con tabla de inicios de línea; columna 0 en EOF → offset = longitud.
- **B4.** El BOM se quita antes de parsear y se suman 3 a los offsets.
- **B5.** Recuperador explícito que siempre falla aunque las features de red estén apagadas (defensa doble); `PatternOptions::regex()` con `size_limit` por defecto.
- **B6.** Los casos `generator` se informan con `eprintln!` y se omiten, como en `lexer`; soporte de `schemaFile`/`payloadFile` con las mismas reglas de ruta.
- **B7.** `baseline::corpus` lee el corpus con tipos pero **no** duplica `validar_formato` (sigue en `crates/lexer/tests/common`); el texto original del esquema se saca con `schema_parser` como en `crates/semantic/tests/common`.
- **B8.** Pruebas XML serializadas con un `Mutex` estático (estado global de libxml2); `no_error`/`no_warning` a `true` para recoger `StructuredError` sin escribir en stderr.
- **B9.** Imagen Docker fijada (p. ej. `rust:1.100-<distro>`), volumen de `target` aparte, caché de cargo opcional.
- **B10.** Errores de `iter_errors` en el orden de `jsonschema`; comparación por `pointer ∈ conjunto`.
- **B11.** Texto de los mensajes de fallo de la prueba: archivo, campo, esperado y obtenido.

## Hallazgos del Planner que el Builder debe tener en cuenta

- `jsonschema` 0.61.2 (MSRV 1.85): features por defecto `resolve-http`, `resolve-file`, `tls-aws-lc-rs`, `idna`; con `default-features = false` quedan ~65 crates todo Rust, sin `cc`, reqwest, rustls ni tokio. Un `$ref` remoto falla al compilar («`resolve-http` feature or a custom resolver is required»). Hacia `serde_json` solo activa `float_roundtrip`. `PatternOptions::regex()` es el motor lineal; por defecto usa `fancy-regex`, que retrocede.
- Prototipo del Planner sobre los 8 casos JSON: `valid` coincide en los 8; `invalid-missing-required-01` da 1 error con puntero `/users/1`; los dos léxicos dan columnas 8 y 11 en la línea 1 (columnas en bytes, base 1) → offsets 7 y 10. Código de referencia (fuera del repo): `C:\Users\bayro\AppData\Local\Temp\claude\bcc3d1797eb97101\10873621-16eb-4921-aa19-8a45143ad21d\scratchpad\proto\src\main.rs`.
- Divergencias medidas: claves duplicadas (gana la última); `1e400` «number out of range»; enteros > 2⁶⁴ vía `f64`; BOM rechazado por `serde_json`; profundidad 128 de `serde_json`; recuento (`required` 1 por clave, `additionalProperties` 1 por palabra clave, `type`+`enum` 2, `allOf` aplana, `anyOf`/`oneOf`/`not` 1); orden de errores según el esquema; `jsonschema` acepta *lookahead* (con `fancy-regex`) e `if`/`then`.
- `libxml` 0.3.23 (MSRV 1.88): `build.rs` hace `panic!("Could not find libxml2.")` sin pkg-config/vcpkg (por eso la feature); usa bindgen (libclang). `ParserOptions::default()` trae `recover: true` (poner `false`) y `no_net: false`. API de esquemas segura (`SchemaParserContext::from_buffer`, `SchemaValidationContext::validate_document`). Sin HUGE, libxml2 limita la profundidad a 256. `malformed-xml-billion-laughs-01` es XML bien formado (DTD interna pequeña): hace falta U6. `unsafe_code = "deny"` es por crate: no afecta al `unsafe` interno de `libxml`.
- Corpus: 10 casos (8 JSON: 1 `valid`, 1 `invalid`, 2 `malformed` léxicos, 4 `non-dpda` válidos; 2 XML: `valid-xml-pedido-01` y `billion-laughs`); ningún `generator`, `schemaFile` ni `payloadFile`; un `payloadBase64`.

## Pendientes iniciales para `docs/fase6/pendientes.md` («Vigilar más adelante»)

- Comparar con F4.6 las divergencias de escapes `\ud800` sueltos, de `\d` (Unicode en `regex`, ASCII en ECMA-262) y de `encoding` distinto de UTF-8 en XML.
- F6.5: la memoria de libxml2 (`malloc` de C) no la ve un `GlobalAlloc` de Rust: medir el pico del proceso (`VmHWM` en Linux); un contador de asignaciones propio exige `unsafe`, así que conviene un crate como `stats_alloc` o `dhat`.
- E1 y casos extremos: la línea base necesita el payload entero en memoria; fijarle un tope de tamaño.
- Unificar los lectores de corpus de `lexer` y `semantic` sobre `baseline::corpus` (opcional).
- Revisar el recuento de violaciones cuando F4.2 fije el peso de `additionalProperties`, `required` y `type` + `enum`.
- XML en Windows solo a través de Docker.

---

# Plan del resto de la Fase 4 (F4.3, F4.4a/b, F4.5a/b, F4.6)

Fecha: 15 nov 2026 (Planner); aprobado por Leonardo el 16 nov con las recomendaciones (F4-U9…U19 = D51). Las marcadas **[compartida]** necesitan también el visto bueno de Bayron (D42); mientras tanto valen como provisionales.

## Decisiones del usuario (aprobadas)

- **F4-U9. Un solo ejecutor** (enmienda F4-U4): el «simulador» de F4.1a se escribe directamente como núcleo `no_std` en `runtime-rs` (lee `runtime_rs::Tables<'a>` aplanadas con *slices*, usa memoria del llamador, recibe eventos); F4.1b/c, F4.4b y F4.5 añaden sus operaciones ahí; F4.6 pone delante el lexer, los límites, el reporte, la capa de buena formación y los bloques. Ahorra un M y elimina divergencias; F5.1 = tablas `const` + ese núcleo; F5.2 porta un solo núcleo.
- **F4-U10. Combinadores con salidas y puertas:** el producto de F4.1b se generaliza con **salidas** (contadores por rama en el marco): `allOf` y `$ref` con hermanas cuentan en la salida del padre; `anyOf`, `oneOf`, `not` y `dependentSchemas` abren una salida por rama; los hijos que apila una componente cuentan en su salida; al desapilar, **puertas** (ANY, ONE, NOT, IMPLIES(bit de la clave)) dan el veredicto. Exacto, sin búfer, O(1) por evento con el producto tabulado; no cambia la IR. Justificación: los autómatas de pila visible son cerrados bajo intersección y complemento (`ir.md` §6.1).
- **F4-U11. Al pasar el tope de 4 096 estados** [compartida: texto de §2.7]: en marcos escalares (cadena, número), simulación paralela de las componentes (cada una con su AFD y su salida; cubre los 15 primos); en marcos con hijos, `limit` con el tamaño alcanzado, que va al informe. Se aparta de D46 (4) en ese último caso.
- **F4-U12. `oneOf`** (enmienda D46 (4)): igual que `anyOf` (producto acotado con la puerta «exactamente una», con multiplicidad, S-3). No cambia `00-alcance.md` §2.7 ni la estrategia `bounded-determinization` de D19.
- **F4-U13. Peso de los combinadores** [compartida: precisa F4-U3]: `anyOf`, `oneOf` y `not` cuentan **1** violación (su `Constraint`); `allOf`, `$ref` con hermanas y `dependentSchemas` (si salta) suman las de dentro; ⊕ = mín dentro de la puerta (homomorfismo ℕ∪{∞} → 𝔹). Acerca el conteo a `jsonschema` (F6.7).
- **F4-U14. Subconjunto de `pattern`** [compartida: `00-alcance.md` §2.4, §3.1, §3.4]: puntos de código (semántica `u`); literales, `.`, clases `[...]`/`[^...]` con rangos, alternancia, grupos, `* + ? {m} {m,} {m,n}`, escapes de control y `\uHHHH`. ECMA: búsqueda sin anclar; `^`/`$` solo al principio o al final de una alternativa de primer nivel; `\d`=[0-9], `\w`=[A-Za-z0-9_], `\s` con la lista de ECMA; `.` excluye `\n \r U+2028 U+2029`; se rechazan retrorreferencias, *lookaround*, `\b`/`\B`, perezosos/posesivos, grupos con nombre, `\p{}`, `\cX`, `\u{}`, anclas en medio. XSD: anclado implícito; `^`/`$` literales; `.` excluye `\n \r`; `\s \S` exactos; `\d` = [0-9] como desviación conservadora (§3.4); se rechazan `\D \w \W \i \I \c \C \p \P` y la resta de clases (con un mensaje que propone `[0-9]`).
- **F4-U15. `ir::regex`:** analizador + AST en arena + `matches` por simulación de posiciones, como `ir::num` (D43); `xsd` sigue dependiendo solo de `lexer` e `ir` (D21); `Rule::Pattern(String)` no cambia (`automaton` lo vuelve a analizar; si falla, `Internal`). Toca `crates/ir`, no la IR ni el JSON v2.
- **F4-U16. Límites nuevos o precisados** [compartida: `00-alcance.md` §4]: `pattern` ≤ 4 096 posiciones tras expandir `{m,n}` (Fase 3) y ≤ 4 096 estados del AFD, anidamiento ≤ 256; claves no declaradas ≤ 1 024 por objeto y una reserva común de 4 096 entre todos los objetos abiertos (igual para atributos por etiqueta); `uniqueItems` 4 096 elementos entre todos los arreglos abiertos; hash de 64 bits con clave (probabilidad de colisión documentada); el tope de 4 096 de la «determinización acotada» vale para todo producto y todo AFD. `sv_state_t` ≈ 64 KiB + 256 × marco.
- **F4-U17. `uniqueItems` y literales compuestos:** `uniqueItems` por hash canónico en *streaming* (números por valor, objetos sin orden), SipHash-1-3 con clave en `Limits`, comprobado al leer `]` (estrategia `deferred`); bajo `not` se admite y se documenta (falso «repetido» con probabilidad ≤ 2⁻⁴¹). `enum`/`const` compuestos traducidos en compilación a su autómata exacto (objeto: `const` por clave, todas requeridas, sin adicionales; arreglo: `prefixItems` + longitud exacta) con la puerta ANY.
- **F4-U18. Reporte y API del ejecutor** [compartida: forma de `sv_report_t` y la API C-ABI]: `new(tables, &Limits, mem: &mut [u64])`, `feed(&[u8]) -> Continue|Stop` (0 / <0), `finish() -> Report`; `Report`: `valid`, `violations` (u64 saturado), `error_kind` (lexical/syntax/schema/limit) y el **primer** error (offset, línea, columna, `RuleKind`, ubicación del esquema recortada a 256 B guardada en las tablas); el modo (total / fallo temprano) en las opciones; la estrategia de respaldo en el resumen de `schemac automaton` (amplía F4-U7), no en `sv_report_t`.
- **F4-U19. Orden:** F4.6 justo después de F4.1c y antes que F4.4b/F4.5 (con U9 ya no depende de los combinadores): camino completo pronto (esquema → M → ejecutor → F5.1, PC-E). F4.6 se cierra contra `expected` y contra la línea base de F6.7a. Reparto con la Fase 6: puntero de la instancia → F6.1; exponer límites en `sv_limits_t`, tiempo y el campo `limits` del corpus → F6.2.

## Decisiones que puede tomar el Builder (registrarlas en `docs/fase4/`)

- **F4-B14.** Forma de `Dfa`, fijada en F4.1a: `{ n_classes, start, trans: Vec<u32> por filas, out: Vec<Out> }`, completo, con estado muerto explícito; Hopcroft devuelve una numeración canónica (BFS desde el inicial, clases en orden).
- **F4-B15.** Hopcroft iterativo (transiciones inversas por clase, «la mitad menor»); Moore O(n²k) solo como oráculo de pruebas.
- **F4-B16.** `determinism::check` corre en cada `build` (O(|δ|)); un fallo da `Internal`.
- **F4-B17.** AST de las expresiones regulares en arena, sin `Box`; análisis con pila explícita; posiciones contadas con multiplicación saturada antes de expandir.
- **F4-B18.** Patrón → AFD: Glushkov (sin ε) + subconjuntos sobre las clases del patrón; anclaje por alternativa (sin `^` se reinyecta el inicial en cada subconjunto; sin `$`, estado «encontrado» absorbente); en XSD, el patrón ve el texto tras el *collapse* cuando el tipo lo pide.
- **F4-B19.** Códigos de error de `pattern` en la Fase 3: JSON `meta-schema` (mal formada), `unsupported` (fuera del subconjunto), `limit` (demasiado grande); XSD `facet`/`value`, `unsupported`, `limit`; posición de la cadena; constructo citado recortado.
- **F4-B20.** Salidas como contadores u32 saturados; puertas en orden fijo de compilación; las salidas que llegan a la raíz solo por conjunciones son «globales» (detienen el fallo temprano); clave de memorización de un tipo de marco = lista canónica de pares (`NodeId`, etiqueta de salida); la transición que apila lleva el mapa de salidas.
- **F4-B21.** SipHash-1-3 propio `no_std`, clave fija por defecto en `Limits`; forma canónica de los números para el hash (signo, cifras sin ceros sobrantes, exponente ajustado; si no cabe en i64, `limit`).
- **F4-B22.** Reservas: un solo arreglo de u64 usado como pila de regiones; claves y atributos con búsqueda lineal en su región (≤ 1 024) y error en la repetida; `uniqueItems` con heapsort de la región al leer `]`.
- **F4-B23.** Unidad del límite de cadena (bytes UTF-8 decodificados) y contador de referencias XML; si el lexer no lo expone sin coste, pendiente para F6.2.
- **F4-B24.** `automaton::validate(&VerifiedIr, &[u8], Mode) -> Report` (std) para pruebas, arnés F6.5 y servidor.
- **F4-B25.** Prueba de «sin reservas» con un asignador global contador en un binario de prueba aparte.
- **F4-B26.** Documentos: `docs/fase4/{automata.md, pattern.md, runtime.md, pendientes.md}`.

## Ajustes a F4.1a-c (aprobados; ya reflejados en `tasks/todo.md`)

1. F4-U9: el «simulador std» de F4.1a se escribe como núcleo `no_std` en `runtime-rs` (lee `Tables<'a>` aplanadas que construye `automaton`, usa la memoria del llamador; criterio añadido: 0 reservas en el camino de validación); F4.1b y F4.1c añaden registros, Glushkov y contadores en ese núcleo.
2. El formato de las tablas reserva desde F4.1a los campos de F4.5a: `slot` en cada comprobación (siempre 0 en F4.1), `slots` y `gates` en cada tipo de marco (vacíos), mapa de salidas en cada transición que apila (identidad).
3. La forma de `Dfa` (F4-B14) se fija en F4.1a.
4. F4.1b: el producto atribuye cada comprobación a su componente; el error de tope devuelve el número alcanzado y si el marco es escalar; la prueba «con el tope bajado a 8, `limit`» usa un producto de **marcos de objeto**.
5. Etiquetas de `Pending`: `pattern` → F4.4b; `anyOf`/`oneOf`/`not`/`dependentSchemas` → F4.5a; `uniqueItems` y literales compuestos → F4.5b.
6. F4.1c: el transductor de *collapse* es una pieza reutilizable (F4.4b lo usa delante del AFD del `pattern`).
7. Documentación: con F4-U13, `automata.md` dice «`anyOf`/`oneOf`/`not`: 1 violación, veredicto al desapilar».

## Orden, paralelo y calendario

Pista A: F4.1a → F4.1b → F4.1c → F4.6 → F5.1 → [PC-E] → F4.4b → F4.5a → F4.5b → F5.2 → F5.3. En paralelo: F4.4a (sin dependencias) y F4.3 (tras el commit de F4.1a), idealmente Bayron; si no, Leonardo entre F4.1c y F4.4b. Calendario objetivo: dom 15 F4.1a (y F4.4a); lun 16 F4.1b, F4.1c (y F4.3); mar 17 F4.6, F5.1, F4.4b; mié 18 F4.5a, F4.5b, F5.2 empieza como muy tarde a las 18 h; jue 19 F5.2, F5.3 y congelamiento. Si no se cumple: congelamiento el viernes 20.

## Riesgos y orden de recorte

Riesgos: calendario (≈ 11 M en 4 días; depende de U9 y de la ayuda de Bayron); F5.2 (portar el núcleo a C) va último (mantener el núcleo pequeño y basado en tablas; si el mié 18 a las 18 h F4.5a no está, F5.2 empieza igual); explosión de tipos de marco (tope por producto, presupuesto 2²⁰, `limit` con tamaño); corrección Unicode de las expresiones regulares (oráculo `matches`, proptest); colisiones de hash (clave secreta, cota documentada); F6.7 tarde (comparar con `expected`); decisiones compartidas sin aprobar (U11, U13, U14, U16, U18: provisionales; proponerlas a Bayron en un solo mensaje); corpus pequeño (casos nuevos en F4.4b-F4.6).
Orden de recorte (ninguno deja de cumplir el PDF): (1) D29 (`pattern` en enumeraciones y `default`/`fixed`); (2) literales compuestos de `enum`/`const` rechazados con mensaje [compartida: §2.2]; (3) simulación paralela en marcos escalares → `limit` con el tamaño; (4) Hopcroft solo sobre los AFD de `pattern`; (5) resumen mínimo de `schemac automaton`. No se recortan: determinismo + Hopcroft, `pattern` → AFD con rechazo anti-ReDoS, producto acotado de `anyOf`/`allOf`, `not`/`oneOf` como puertas, `uniqueItems` diferido, el ejecutor sobre el corpus, los emisores Rust y C.

---

# F7.1: backend axum (pista B, Bayron; Planner del 18 nov, aprobado por Bayron el 18 nov)

Criterios en `tasks/todo.md` («Tarea F7.1»).

## Hallazgos del Planner
- `server/` hoy solo imprime la versión y depende de `codegen` sin usarlo. `crates/cli/src/main.rs` es solo binario; su tubería usa API pública: `formato` (D22, ~10 líneas); `schema_parser::parse` → `semantic::analyze` → `semantic::lower`, o `xsd::parse` → `xsd::analyze` → `xsd::lower`; `ir::write_json(&VerifiedIr, &mut impl fmt::Write)`; `automaton::build(&ir)` → `write_summary_json`. Los mensajes de `cli` son `String`; `ParseError`, `SemanticError` y `XsdError` tienen `line()`, `column()`, `code()` y su `Display` empieza por `"línea {l}, columna {c}: "`.
- `automaton::build` hoy solo JSON; XSD da `BuildError::Pending` hasta F4.1c, y algunas reglas JSON también (F4-U5). El resumen F4-U7 escribe `"format": "json"` fijo.
- Memoria ≈ 158× la entrada hasta la IR (≈ 83× hasta la Fase 3); reservar ≈ 165× el tope de subida + proceso base; salida de la IR hasta ≈ 93× la entrada; peor XSD de 16 MiB ≈ 7-28 s (§38).
- Versiones: axum 0.8.9 (MSRV 1.80), tokio 1.54.1 (MSRV 1.85), tower 0.5.3, http-body-util 0.1.5, sha2 0.11.1; prototipo en Docker con `axum` `default-features = false, features = ["http1","tokio","json"]`: 413 para 1 MiB + 1 y 200 para `{}`; 54 crates, sin openssl, `-sys` ni `cc`. En Windows, Smart App Control bloqueó en debug el DLL de `serde_derive` y el binario de pruebas; `--release` pasó. No hace falta tower-http.
- Prototipo fuera del repo: `C:\Users\bayro\AppData\Local\Temp\claude\bcc3d1797eb97101\10873621-16eb-4921-aa19-8a45143ad21d\scratchpad\proto`.

## Decisiones del usuario (aprobadas el 18 nov con la opción recomendada)
- **U1 — Contrato de la API (interfaz compartida, D42: visto bueno de Leonardo).** Entrada: cuerpo crudo, formato por contenido (D22). Respuestas: 200 `{apiVersion, id, format, summary, ir|null, irBytes, irOmitted, automaton}`; 422 `{error:"schema", format, errors[], total, truncated}`; 413, 500, 503 (con `Retry-After`) y 504, todas en JSON con un campo `error`; sin 400; 200 y no 201 (no hay `GET` del recurso).
- **U2 — Sin tocar `crates/cli`.** El servidor llama a las bibliotecas y copia `formato` con prueba de paridad; pendiente: unificar en un crate `pipeline`.
- **U3 — Tope de subida 1 MiB, configurable.** Se propone a Leonardo una fila en `00-alcance.md` §4 («subida máxima del servidor: 1 MiB»); hasta su OK, solo en `docs/fase7/servidor.md`.
- **U4 — Memoria:** hilo + semáforo de 2 + subida de 1 MiB + tope de IR (pico ≈ 350 MiB + proceso base); tope duro con `mem_limit` (p. ej. 1 GiB) y `restart: unless-stopped` en Compose (F7.7). No cubre el aborto por fallo de asignación o desbordamiento de pila (tumba el proceso; el contenedor se reinicia): riesgo residual declarado y pendiente para F7.3 (proceso hijo con `RLIMIT_AS`, que sin `unsafe` exige `sh -c 'ulimit -v N; exec …'`, archivos temporales y solo Linux).
- **U5 — `id` = SHA-256 hexadecimal del contenido** (`sha2` 0.11.1, Rust puro); determinista y sin estado; F7.2 lo usa como `_id` (visto bueno de Leonardo).
- **U6 — `automaton` como objeto con `status` `ok`/`pending`/`limit`;** un esquema válido con autómata no construible sigue siendo 200; `BuildError::Internal` → 500 (D15); el resumen de `write_summary_json` se incrusta tal cual.
- **U7 — Dockerfile en F7.7;** F7.1 deja la configuración por variables de entorno (`SERVER_ADDR=0.0.0.0:8080` en el contenedor).
- **U8 — IR en la respuesta si cabe en 4 MiB;** si no, `ir: null` e `irOmitted: true`.

## Decisiones que puede tomar el Builder
- B1. `AppState` con un `Arc<dyn Fn(&[u8]) -> Compilado + Send + Sync>` para inyectar compilaciones que hacen pánico o se bloquean en las pruebas.
- B2. `message` sin el prefijo de posición con `strip_prefix(&format!("línea {}, columna {}: ", e.line(), e.column()))`, texto completo si no coincide.
- B3. Pruebas con `#[tokio::test]` (feature `macros`) o runtime a mano.
- B4. Nombres de módulos y estructuras de respuesta; serializar con `serde_json` o armar el JSON a mano para incrustar la IR y el resumen sin volver a parsearlos.
- B5. Log con `writeln!` a stderr ignorando el error; sin `tracing`.
- B6. Apagado con `axum::serve(...).with_graceful_shutdown(ctrl_c)`.
- B7. `fmt::Write` con tope para la IR y el resumen.
- B8. `Retry-After: 5`; mensajes de 413, 503 y 504 en español.
- B9. Sin CORS (Caddy sirve web y API en el mismo origen, F7.7); proxy de Vite si F7.4 lo necesita.

## Riesgos
Smart App Control (Windows: `--release` o Docker); `Cargo.lock` sube de 136 a ~175 paquetes (zona de choque con Leonardo: pull antes); el servidor es el segundo llamador de `semantic::lower` (§13, `AnalyzedTable`): llamar siempre a `analyze` antes y comentarlo; revisión con `code-reviewer` y `security-auditor` (D35).
