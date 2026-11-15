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
