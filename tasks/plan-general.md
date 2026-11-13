# Plan de desarrollo — Validador JSON/XML con DPDA ponderado

Oct 19, 2026 · @Leo · actualizado Nov 13, 2026 (desde hoy vive en el repo como `tasks/plan-general.md` y el estado de las tareas se lleva en `tasks/CHECKLIST.md`, D42; historia archivada en el Anexo A)

## Resumen y roles

El compilador debe funcionar de extremo a extremo (esquema → DPDA ponderado → C y Rust que validan) al cierre de la semana 3; la semana 4 cierra validación en tiempo real y la aplicación web, y la semana 5 queda para despliegue, pruebas de bombeo, informe y demo.

**Supuestos:** trabajo del lunes 19 de octubre al domingo 22 de noviembre de 2026; entrega y demo al final de la semana 5. Los lunes 2 y 16 de noviembre son festivos en Colombia, así que esas semanas tienen un día menos. Si la fecha de entrega real es otra, se corre el cronograma completo.

**Decisión que ahorra más tiempo:** JSON Schema y el subconjunto de XSD se traducen a una misma representación intermedia (IR). Cada lenguaje tiene su propio AST (solo en tiempo de compilación) y ambos bajan a la IR común; desde ahí el constructor del DPDA, el generador de código y las métricas son uno solo para ambos formatos.

| Rol | Responsable de | Fases |
| --- | --- | --- |
| Persona A — Compilador y teoría | Definiciones formales, gramáticas, parser LL(1), análisis semántico, construcción del DPDA, manejo de errores, seguridad, informe final | 1 (teoría), 2, 3, 4, 6 (errores y límites) |
| Persona B — Runtime y plataforma | Tokenizador streaming, generación de código C/Rust, métricas y líneas base, backend, frontend, NoSQL, sandbox, despliegue | 1 (implementación), 5, 6 (instrumentación), 7 |
| Ambos | Definir el subconjunto soportado, casos de prueba, revisión cruzada de cada PR, demo | Todas |

Regla de trabajo: ~~cada PR lo revisa la otra persona antes de entrar a `main`~~ (sin efecto desde el 3 nov, D32: no hay PR; la revisión es la auditoría Tester → Reviewers de cada tarea y el commit lo hace el usuario), y el viernes de cada semana se hace una integración de 1 hora para verificar el hito.

## Contexto común para toda sesión de trabajo

**Reparto por personas:** desde el 21 oct 2026 se ignora la división en Persona A / Persona B de la tabla de roles y del cronograma; las etiquetas "Persona A/B" solo indican el tema. **Desde el 13 nov (D42) trabajan dos personas en simultáneo, cada una desde su cuenta:** cada tarea se toma en `tasks/CHECKLIST.md` (responsable y estado) antes de empezar, cada persona lleva una sola tarea a la vez con `/autodev`, y las reglas de trabajo en equipo están en `COLABORAR.md`.

Leer esta sección al empezar cualquier sesión (de una persona o de un agente). Cada semana del cronograma tiene además su bloque **Contexto para la sesión** con el estado de partida, lo que hay que leer y los pendientes heredados.

**Fuentes de verdad, en este orden:**

1. `docs/enunciado/Propuesta_Proyecto_Final_PDA_JSON.pdf` (enunciado del curso): lo que se exige.
2. Este plan (`tasks/plan-general.md`): cómo y cuándo se hace, y el registro de decisiones. El estado de cada tarea (quién la lleva, si está hecha) está en `tasks/CHECKLIST.md`.
3. `docs/00-alcance.md` en el repositorio: subconjunto soportado, clase (a)/(b)/(c) de cada palabra clave y límites por defecto. Si el plan y el alcance chocan en algo técnico, manda el alcance y se corrige el plan.
4. `docs/faseN/` (teoría de cada fase) y `docs/faseN/pendientes.md` (lo que dejó abierto la última auditoría).

**Invariantes que ninguna sesión puede romper:**

- Validación O(n) en bytes y O(d) en pila; el validador generado no construye árbol del payload.
- Sin asignación dinámica en el camino de validación: el lexer es `#![no_std]` sin `alloc`; el runtime usa `sv_state_t` de tamaño fijo. Si cambia el tamaño de una estructura documentada, se actualiza la tabla de tamaños en `docs/fase1/lexico.md`.
- Determinismo: una sola transición por (q, clase, X); el resultado no depende del tamaño de los bloques (pruebas con bloques de 1, 7 y 4096 bytes).
- `unsafe_code = "deny"` en el workspace; clippy con `-D warnings`; finales de línea LF (`.gitattributes`), necesarios para que las columnas coincidan en Windows.
- **Regla de rondas (D38, 3 nov):** un Important abre ronda si es un defecto real, un criterio sin cumplir, una contradicción con una decisión o algo que heredarían las tareas siguientes; si es documentación se corrige en el cierre, y si es cobertura o de una tarea futura pasa como criterio de la tarea que corresponda. Nada se ignora.
- **Auditor de seguridad solo donde aplica (D35, 3 nov):** en `/autodev`, las tareas del compilador se revisan solo con `agent-skills:code-reviewer`; `agent-skills:security-auditor` va en las tareas de runtime, fuzzing, servidor y sandbox (Fases 4-7) y en una revisión consolidada al cerrar cada fase.
- **GitHub solo guarda los commits (D32, 3 nov):** no se usan GitHub Projects, issues, PR ni GitHub Actions. Git local sí: el usuario hace un commit entre tareas y los agentes usan `git status`/`git diff` en solo lectura para ver lo que cambió en la tarea. Ninguna sesión consulta la CI, crea tareas en GitHub ni da algo por verificado «en la CI»: la verificación es local (`make lint`, `make test`, `make docs-check` y, si se toca `semantic`, `cargo test -p semantic --release`). Toda mención a «la CI» en este plan o en `docs/` se lee como esas comprobaciones locales.
- Las pruebas son deterministas: nada de umbrales de tiempo de pared en `cargo test` (ver D1).
- Los diagramas DOT/SVG y las tablas LL(1) de `docs/` se generan desde el código y las gramáticas (`make docs`); no se editan a mano. `make docs-check` (local, D32) verifica los `.dot` y los `ll1-*.md`; los `.svg` y las cifras escritas a mano en los `.md` (número de estados y clases, resúmenes FIRST/FOLLOW) los actualiza quien cambie el lexer o una gramática.

**Antes de dar una tarea por terminada:** `make lint` y `make test` en verde, documentación de la fase actualizada si cambió el comportamiento, auditoría (`/autodev`: Builder → Tester → Reviewer, máximo 3 rondas) y la fila de la tarea marcada en `tasks/CHECKLIST.md`. Los agentes nunca hacen `git commit` ni `push`; los commits los hace la persona responsable de la tarea después de revisar (D42).

## Registro de decisiones

Decisiones tomadas el 20 oct 2026 a partir de la auditoría del estado actual y de lo que pide el enunciado. Cada una indica qué dice el PDF que la justifica. Si una decisión cambia, se edita aquí y se anota la fecha.

| # | Decisión | Justificación (enunciado) | Estado |
| --- | --- | --- | --- |
| D1 | Las mediciones de tiempo van al crate `bench` (criterion) y al arnés de la Fase 6, no a `cargo test`. La prueba `muchas_claves_en_tiempo_lineal` queda `#[ignore]` justificado y se corre a mano. No se usa tiempo de CPU: ni con `libc` (exigiría `unsafe` y no funciona en Windows, donde también corre la CI) ni leyendo `/proc/thread-self/schedstat` (solo Linux). | El PDF pide demostrar O(n) en el informe y medir MB/s y bombeo con métricas comparativas: eso es trabajo del arnés, no de una prueba unitaria. | Hecha (hallazgo 2); falta el benchmark de la semana 4 |
| D2 | Las claves repetidas se rechazan en cualquier objeto del esquema, incluidos los valores de `const` y `enum`. En los payloads, el validador generado también las rechaza como malformado (`docs/00-alcance.md` §1). | "Claves duplicadas" aparece en el PDF como caso **malformado**. | Tomada |
| D3 | AST propio por lenguaje (JSON Schema y XSD) y una IR común a la que ambos bajan. El parser XSD no reutiliza el AST de JSON Schema: la Fase 3 (meta-esquema, `$ref`/`$defs`) es específica de JSON Schema, y XSD tiene orden de `sequence`, atributos frente a elementos y `minOccurs`/`maxOccurs`. La IR debe poder expresar esas restricciones. | "Compilador que traduce un esquema JSON (con extensión a un subconjunto de XSD)"; "el AST existe solo en tiempo de compilación". | Tomada; el diseño de la IR lo propone el desarrollador en la tarea de la IR y lo revisa el auditor (sin reparto por personas, 21 oct) |
| D4 | Semianillo tropical (ℕ ∪ {∞}, mín, +, ∞, 0). El peso acumulado cuenta violaciones, que son enteras. ℕ ∪ {∞} es un subsemianillo de ℝ≥0 ∪ {∞}, así que no se pierde nada de la teoría. | "El peso estima el costo de cada transición … y acumula el número de violaciones". | Tomada (el código ya lo usa) |
| D5 | `dependentRequired` se comprueba al cerrar el objeto con la máscara de bits; `dependentSchemas` es verificación diferida. | El PDF pone las dependencias en la clase (c) con "verificación diferida"; coincide con `docs/00-alcance.md`. | Tomada |
| D6 | Las palabras clave fuera del subconjunto se rechazan siempre en la Fase 3 con un mensaje que diga su clase y por qué no se soporta. Hoy el parser las deja pasar como `Kw::Unsupported`. | "Clasificar el resto de palabras clave". | Tomada; se implementa en la Fase 3 |
| D7 | La profundidad máxima sigue siendo configurable (`parse_with_max_depth`, `SV_MAX_DEPTH`) pero con un tope máximo seguro documentado; pasar de él es un error, nunca un desbordamiento de pila. En el parser de esquemas el tope es `MAX_DEPTH_LIMIT` = 256 (solo configurable a la baja; valores mayores se recortan) y está garantizado en hilos con ≥ 2 MiB de pila: `schemac` y el servidor deben llamar al parser en un hilo con `stack_size` explícito, nunca en el hilo principal (1 MiB en Windows). `SV_MAX_DEPTH` del runtime es independiente: usa una pila explícita de tamaño fijo y se acota por memoria, no por recursión. | "Pila de profundidad acotada y configurable". | Hecha en el parser (hallazgo 3). `schemac` ya cumple la regla del hilo: `schemac check` llama al parser y a la Fase 3 en un hilo de 8 MiB (`crates/cli/src/main.rs`, `STACK_SIZE`). Pendiente solo en el servidor (Fase 7), cuando integre el parser |
| D8 | `<!DOCTYPE` se rechaza en el lexer (en mayúsculas y minúsculas, con el error `DoctypeForbidden`); solo se aceptan las 5 entidades predefinidas. | Mitigar "XXE y expansión de entidades" y probar "expansión de entidades XML" como caso extremo. | Hecha (el mensaje de `<!doctype` se corrigió con el hallazgo 4; el lector XSD de la 5.1 la conserva: `DoctypeForbidden` en el offset del caso billion-laughs) |
| D10 | En JSON, un prefijo de BOM (`EF` o `EF BB`) seguido de otro byte se reporta como `UnexpectedByte` en el offset 0, igual que cualquier byte no ASCII fuera de cadena; no como `InvalidUtf8` en el offset 1. Decidido por el usuario el 21 oct 2026 (opción B de `docs/fase1/lexico.md` §7). | Reportar "error léxico con offset" correcto para los malformados. | Hecha (hallazgo 8, ronda 3) |
| D11 | `enum: []` se acepta, como en el meta-esquema oficial 2020-12 (equivale a un esquema que no acepta ningún valor, igual que `false`). La validación semántica no es más estricta que el oficial salvo donde el alcance lo dice expresamente (palabras clave fuera del subconjunto, `$schema`/`$id` en la raíz, `$ref` local). Decidido por el usuario el 21 oct 2026. | El PDF pide un subconjunto de Draft 2020-12, no un dialecto propio. | Hecha (semana 2, tarea 1) |
| D12 | Los ciclos no productivos de `$ref` se rechazan aunque sean inalcanzables desde la raíz (por ejemplo, dos `$defs` sin usar que se apuntan entre sí). 2020-12 deja ese caso indefinido; rechazarlo hace que la detección no dependa de qué parte del esquema se compila y deja la mitigación sin huecos. El usuario delegó la elección el 21 oct 2026. | El PDF pide "detección de ciclos no productivos" en la Fase 3 y reportar "`$ref` cíclicos" como vulnerabilidad mitigada, sin distinguir alcanzables de inalcanzables. | Hecha (semana 2, tarea 3) |
| D9 | Línea y columna de `Pos` son `u32` saturantes (`u32::MAX` = "u32::MAX o más"); el `offset` es `u64` exacto y es la referencia en entradas de varios GB. | "Payloads de gran tamaño en streaming". | Hecha (hallazgo 1, aprobado) |
| D13 | IR común en un crate aparte, `crates/ir`, que solo depende de `lexer`: grafo en arena (`Vec<Node>` + `NodeId`, `nodes[0]` la raíz; en XML, el documento), restricciones tipadas (`Rule`, 31 clases en `RuleKind::ALL`), modelo de contenido en preorden con `Occurs`, y verificador iterativo `Ir::verify` de los invariantes I1–I14 (`docs/fase3/ir.md` §4). La clase (a)/(b)/(c) y la estrategia no se guardan en el nodo: se derivan de `RuleKind` en una tabla aparte (tarea 4.3). En JSON, `NodeId` = `SchemaId` y se bajan también las definiciones inalcanzables (coherente con D12). Aprobada por el usuario en el punto de control A el 26 oct. | D3: una IR común que exprese orden, atributo/elemento y ocurrencias; "el AST existe solo en tiempo de compilación". | Hecha (tarea 4.1, commit `12ea049`) |
| D14 | Se excluye del subconjunto XSD el `xs:element` sin `type`, `ref` ni tipo anónimo: su tipo implícito es `xs:anyType`, equivalente a `xs:any` + `xs:anyAttribute` + `mixed="true"`, ya excluidos (`00-alcance.md` §3.3). Se rechaza con mensaje claro (D6). Aprobada por el usuario el 26 oct. | "Subconjunto de XSD". | Tomada; la aplica el frontal XSD |
| D15 | `Ir::verify` se llama siempre en el camino normal antes de la Fase 4 y sus errores se presentan al usuario como un único error interno (la lista completa, solo al registro y a las pruebas). En la 4.2, `verify(self) -> Result<VerifiedIr, (Ir, Vec<VerifyError>)>` y las fases siguientes solo aceptan `&VerifiedIr`. El frontal XSD detecta con posición los errores del esquema del usuario (ciclos de `base` de forma iterativa, UPA, `default`/`fixed`, I12, la parte de I13 del usuario) y `verify` queda como defensa en profundidad. Decidido por el usuario el 26 oct. | Robustez frente a esquemas maliciosos (Fase 7); "detección de ciclos". | Documentada en `ir.md` §4; `VerifiedIr` es criterio de la 4.2 |
| D16 | Decisiones de diseño de la IR aceptadas en bloque en el punto de control A (26 oct; detalle en `docs/fase3/ir.md` §6 y §11): `not` es complemento real (se invierte el veredicto al cerrar la instancia; sobre un subesquema (c), sigue su estrategia e invierte); `additionalProperties` con la semántica de 2020-12 (no ve las `properties` de `allOf`); `xs:sequence` y objetos JSON no se traducen entre sí; `const` es regla propia; números como lexema RFC 8259, comparados por valor en la Fase 4; contadores `u64` saturados; literales en preorden con claves ordenadas; en XML la raíz es el documento (`Choice` de los elementos globales, por convención del frontal); contenido vacío sin espacios; `xs:attribute` sin tipo = `Lexical(String)`; enumeraciones de un tipo entero, enteras; `xs:boolean` sin `xs:enumeration`; contador de ocurrencias solo si `minOccurs` ≥ 2 o `maxOccurs` acotado > 1 (`?`, `*`, `+` van en el Glushkov); E₌ se evalúa compartiendo por `NodeId`, nunca expandiendo en árbol; ubicaciones de nodos sintéticos con `builtin`, `simpleContent/xs:…` y `simpleType`. | D3, D4, D7; XSD 1.0 y JSON Schema 2020-12. | Tomadas |
| D17 | Interfaz de `schemac ir [--dot] <esquema.json>` (tarea 4.2, la consumirá la web en la Fase 7): JSON estable con padre y paso por nodo (**v1** en la 4.2; **v2** desde la 4.3, que solo añade campos: `classCount` en la cabecera, `class` y `strategies` por nodo, `class` y `strategy` por regla; en el DOT, una línea con la clase de cada nodo, el recuento en la cabecera y borde grueso en los nodos (c)) (no la ruta, que sería cuadrática), cotas como lexema en cadena, contadores como entero `u64` saturado, y literales de `enum`/`const` como **lista plana de tokens en preorden** (`["number","1e400"]`, `["key",k]`, `["object",n]`…), sin pérdida ni recursión en el lector; DOT con las aristas de E₌ discontinuas. Formato del esquema detectado por el contenido (D22): primer byte significativo `<` → XSD, si no JSON; el uso pasa a `check <esquema>` / `ir [--dot] <esquema>`. Códigos de salida: 0 IR; 1 los mismos errores que `check`; 2 no se puede leer, argumentos inválidos, no se pudo crear el hilo o escribir la salida; **3 error interno** (`verify` falla o pánico en el hilo; `check` conservaba 2 ante un pánico hasta D33 (4 nov), hoy también 3); tubería cerrada → 0 en silencio. El esquema que lee `schemac` es un archivo regular de como mucho **16 MiB** (`00-alcance.md` §4). Detalle en `docs/fase3/bajada.md` §5. Propuesta por el orquestador el 27 oct; v2 el 28 oct (tarea 4.3, `docs/fase3/clasificacion.md` §5); detección de formato por contenido añadida por D22 (29 oct). Revisada y aprobada por el usuario en el punto de control B (29 oct). | "Aplicación web" (Fase 7) y robustez frente a entradas maliciosas. | Hecha (tarea 4.2, commit `edd75e5`); aprobada por el usuario el 29 oct; XSD integrado en la tarea 5.4 (commit `dac26c8`; D22, D33, D41) |
| D18 | Decisiones internas de la bajada (tarea 4.2; `docs/fase3/bajada.md` §1, §4 y §8): `lower` exige una tabla que ya pasó `analyze` (la opción `AnalyzedTable` queda en `pendientes.md` §13); hijos agrupados por padre (CSR) y cada hijo comparado con el paso que espera su palabra clave, si no la arista queda colgante y acaba en error interno; las reglas dobles (`Items`, `Properties`) van en la posición de la primera palabra clave de la pareja; `short` (recorte y escape de texto del usuario en mensajes) vive en `lexer`, única dependencia común; el «registro» de D15 es stderr con el prefijo `schemac: registro:` (recuento y 8 primeros) mientras no haya otro canal; la bajada es O(n) en el grafo y O(k log k) por objeto literal al ordenar claves. | D7, D13, D15. | Hechas (tarea 4.2) |
| D19 | Clasificación (a)/(b)/(c) y estrategia (tarea 4.3; `docs/fase3/clasificacion.md` §3-§4 y §8): una sola tabla en `crates/ir` (`RuleKind::classification`, `match` exhaustivo) con clase base, estrategia y clase de subida, comprobada por prueba contra la columna «Cat.» de `00-alcance.md` y contra la tabla escrita de `clasificacion.md`; solo `Content` sube, a (b), si tiene `xs:all` o una partícula con contador (`minOccurs` ≥ 2 o `maxOccurs` acotado > 1; D16), y `00-alcance.md` §3.1 lo dice ya en la fila `minOccurs`/`maxOccurs`; `Lexical` es (a); `Attributes` siempre (b), como manda el alcance; `dependentRequired` es (c) con estrategia de registro (clase del alcance, estrategia de D5); `not` y `allOf` → producto acotado, `anyOf` y `oneOf` → determinización acotada; 6 estrategias con claves estables (`transition`, `register`, `product`, `bounded-determinization`, `dfa`, `deferred`); clase de un nodo = máxima de sus reglas y de los nodos alcanzables por E₌ (acíclico por I14), estrategias = unión, las aristas que consumen entrada no propagan, un nodo sin reglas es (a); cálculo iterativo O(n + m) sobre `&VerifiedIr`, compartido por `NodeId`. Tomadas por el Builder; aprobadas por el usuario en el punto de control B el 29 oct, incluido el cambio de la fila `minOccurs`/`maxOccurs` de `00-alcance.md` §3.1. | Fase 3: "clasificar el resto de palabras clave"; D5, D13, D15, D16. | Hechas (tarea 4.3, commit `edd75e5`); aprobadas por el usuario el 29 oct |
| D20 | Detalles de la CLI y de las pruebas cerrados en la tarea 4.3 (`docs/fase3/bajada.md` §5 y §7, `pendientes.md` §16-§21): `schemac` comprueba la ruta con `fs::metadata` antes de abrirla y otra vez sobre el descriptor (las rutas de la CLI son de confianza; la carrera entre ambas, 4.3-L1, la cerró la 5.4 con `O_NONBLOCK` e `is_file()` sobre el descriptor, D41); `ejecutar_ir` recibe los destinos de salida y error; todo diagnóstico pasa por `avisar`, que ignora el error de escritura; las opciones (`--…`) nunca se toman como ruta. Las pruebas del corpus usan el texto original del esquema de cada caso, recortado con `schema_parser` por el rango del valor, en lugar de activar `preserve_order`/`arbitrary_precision` de `serde_json` (sus features se unifican en todo el workspace). | D7, D17; robustez frente a entradas maliciosas. | Hechas (tarea 4.3, commit `edd75e5`) |
| D21 | (U1) El frontal XSD vive en un crate nuevo `crates/xsd` (lector XML con espacios de nombres, gramática, AST, análisis y bajada) que depende solo de `lexer` e `ir`; no depende de `semantic` ni de `schema-parser` (D13: ningún frontal depende del otro). Se corrige el árbol de «Arquitectura y repositorio». Aprobada por el usuario el 29 oct (plan de la tarea 5, `Retomar - semana 2 tarea 5 plan.md`). | D3; `ir.md` §1 («`xsd` → `ir`»). | Tomada; la empieza la tarea 5.1 |
| D22 | (U2) `schemac` y el servidor (Fase 7) distinguen XSD de JSON por el contenido: si el primer byte significativo (tras un BOM y espacios) es `<`, es XSD; si no, JSON. El uso pasa a `check <esquema>` / `ir [--dot] <esquema>` y `check` informa «esquema válido (subconjunto de XSD 1.0)». Modifica D17. Aprobada por el usuario el 29 oct. | El servidor recibe bytes, no nombres de archivo (Fase 7). | Hecha (tarea 5.4, commit `dac26c8`) |
| D23 | (U3) `xs:annotation`, `xs:documentation`, `xs:appinfo` y el atributo `id` se aceptan y se ignoran, como las anotaciones de JSON (`00-alcance.md` §2.1); el contenido de `documentation`/`appinfo` es XML bien formado cualquiera. Cambia `00-alcance.md` §3. Aprobada por el usuario el 29 oct. | "Subconjunto de XSD"; casi todo XSD real los usa. | Tomada; tareas 5.2 y 5.7 (alcance) |
| D24 | (U4) `INF`, `-INF`, `NaN` y los valores que desbordan al redondear a binary32/64 (`1e39` en `xs:float`) se rechazan en facetas y enumeraciones de `xs:float`/`xs:double` con un mensaje que lo diga; no se amplían `Num` ni I8 (pendiente 3 de `ir.md` §11). Aprobada por el usuario el 29 oct. | D6 (mensaje claro); evita tocar la IR, `verify`, el JSON v2 y la Fase 4. | Tomada; tarea 5.5 |
| D25 | (U5) UPA conservadora: el determinismo se comprueba sobre la abstracción en la que toda partícula con `maxOccurs` > 1 es `+` o `*`. Es correcta (si la abstracción es determinista, el modelo con contadores también) y rechaza algunos modelos que XSD 1.0 admite (`a{2,2}, a`); la desviación se documenta en `00-alcance.md` §3. Coincide con el Glushkov con contadores de la Fase 4. Aprobada por el usuario el 29 oct. | Determinismo del DPDA; menos riesgo que Kilpeläinen. | Tomada; tarea 5.7 |
| D26 | (U6) Límite nuevo en `00-alcance.md` §4: como mucho 4 096 partículas (posiciones) por modelo de contenido, igual que el tope de la determinización acotada; pasarlo es un error de límite con posición, comprobado antes de calcular FIRST/FOLLOW. Aprobada por el usuario el 29 oct. | Robustez frente a esquemas hostiles (FIRST/FOLLOW y Glushkov cuadráticos). | Tomada; tarea 5.7 |
| D27 | (U7) Se rechaza `xs:simpleContent` + `xs:extension` cuya `base` es un tipo complejo de contenido simple (derivar un tipo complejo de otro, como `xs:complexContent`, fuera del alcance §3.3). Cierra la segunda mitad de S-5 (`docs/fase3/pendientes.md` §3). Aprobada por el usuario el 29 oct. | "Subconjunto de XSD"; D6. | Tomada; tarea 5.3 |
| D28 | (U8) Si el 1 nov no está el punto de control C, no se recortan `choice` ni `all`; se recorta en este orden: (1) UPA (5.7) justo después del hito, con el Glushkov de la Fase 4 como defensa; (2) facetas sobre `xs:float`/`xs:double` (rechazadas mientras tanto); (3) `xs:annotation` (rechazada). Se mantienen los tipos simples con `enumeration` y rangos (5.5 y 5.6). Aprobada por el usuario el 29 oct. | Riesgos y recortes de alcance. | **Aplicada el 3 nov** (el punto de control C no llegó el 1 nov): 5.7 (UPA) pasa a después del hito de la semana 3, con el Glushkov de la Fase 4 como defensa; 5.2-5.6 siguen en orden; si hace falta más recorte, facetas de `float`/`double` y luego `xs:annotation` |
| D29 | (U9) Cambios del alcance aprobados: la fila `minOccurs`/`maxOccurs` de §3.1 (4.3-S1, `pendientes.md` §26), las desviaciones de D23, D25, D26 y D27, y una omisión conocida: comprobar que enumeraciones, `default` y `fixed` cumplen las facetas `pattern` de la base queda pendiente para la tarea `pattern` → AFD (Fase 4), sin mini-motor de expresiones regulares en el frontal. Aprobada por el usuario el 29 oct. | "Toda *Schema Component Constraint*" del subconjunto (D15), acotado. | Tomada; tareas 5.6 y 5.7 (alcance y pendientes) |
| D30 | Lector XML de esquemas (tarea 5.1; `docs/fase2/lector-xsd.md`): tokens bajo demanda `STag`/`Attr`/`Gt`/`EmptyClose`/`ETag`/`Text`/`Pi`/`Eof`; la etiqueta de apertura se entrega entera al leer `>` (`xmlns` puede ir detrás del nombre que lo usa) y, si tiene varios errores, gana el de menor offset salvo un error léxico dentro de ella; el lector pasa un byte por vez al lexer y nunca se adelanta (independencia del bloque por construcción; `Reader::new` no parte la entrada); comprueba también raíz única y etiquetas sin abrir/cerrar, así que toda salida que acaba en `Eof` es bien formada; `xmlns`/`xmlns:p` salen como `ATTR` en `XMLNS_NS`; `<?xml` se valida por posición (1,1) tras un BOM, `version` 1.x y `encoding` UTF-8, y se entrega sin normalizar; normaliza fines de línea (XML 1.0 §2.11, también en las PI) y valores CDATA de atributo (§3.3.3); `&#32;` fuera de la raíz se rechaza; sin límites propios (los pone la 5.2 para la profundidad, D7, y la Fase 7 para el resto; `docs/fase2/pendientes.md` §12); `crates/xsd` aún no depende de `ir`. Tomadas por el Builder. | D7, D8, D21; XML 1.0 y Namespaces in XML 1.0. | Hechas (tarea 5.1, commits `746a9c3`, `b171e48`) |
| D31 | Espacios de nombres y API del lector (tarea 5.1): los URI se internan (un `Arc<str>` por URI distinto, vive con el lector) y su identidad pública es `NsId` (igualdad `Arc::ptr_eq`, hash de la dirección, comparable solo dentro de un mismo `Reader`); la clave de las tablas de la 5.3 es `(ns_id(), local())`, nunca el texto del URI ni `expanded()` (evita el O(referencias·\|URI\|) que encontró el Tester 1); `Reader::intern` para `targetNamespace`, que interna la **5.2** ya recortado (*collapse*) mientras el lector existe; todo valor de atributo se resuelve como QName de forma ansiosa al cerrar su etiqueta y se consulta con `Attr::as_qname()` (error solo si el parser lo pide, en el primer carácter del valor; un valor con prefijo `xmlns` da `ReservedNamespace`); campos de `Name` y `Attr.value` privados; los textos del usuario en los errores se recortan a 33 caracteres al crearlos (el mensaje con `short` no cambia); códigos de error `lexical`, `syntax`, `duplicate-attribute` y `namespace` (amplían B2). Tomadas por el Builder en las rondas 1-3. | D7, D15, D18; robustez frente a esquemas hostiles (Fase 7). | Hechas (tarea 5.1, commits `746a9c3`, `b171e48`) |
| D32 | GitHub se usa solo como remoto de los commits: se descartan el tablero de GitHub Projects (`scripts/crear_issues.py`), los issues, los PR con revisión y GitHub Actions como verificación. Las comprobaciones son locales: `make lint`, `make test`, `make docs-check`, más `cargo test -p semantic --release` cuando se toque `semantic` (corre las salvaguardas de `docs/fase3/pendientes.md` §2). Consecuencias: Windows ya no se prueba (D7 conserva la regla del hilo con `stack_size`, pero nadie la comprueba en Windows); la carga del corpus a Atlas (semana 5) se hace con un script local; `docs/fase3/pendientes.md` §6 (CI) se cierra como «sin efecto». Los archivos de `.github/workflows/` se quedan en el repo sin que nadie los vigile. **Git local sí se sigue usando**, en solo lectura, para delimitar los cambios de cada tarea: el usuario hace un commit entre tareas, y Testers, Reviewers y orquestador comparan el árbol con ese commit (`git status`, `git diff --stat` para elegir los `cargo test -p <crate>`, `git diff` para revisar); nadie más que el usuario hace `add`, `commit` o `push` (el `push` a GitHub, si lo hace, es solo copia de seguridad). Decidido por el usuario el 3 nov. | El PDF no exige CI ni gestión en GitHub; ahorra trabajo que nadie hacía. | Tomada (3 nov) |
| D33 | Un pánico en el hilo de `schemac check` da el código de salida **3** (error interno), igual que en `schemac ir`; hoy da 2. Cierra `docs/fase3/pendientes.md` §8. Decidido por el usuario el 3 nov. | Coherencia de la interfaz de D17 (la consumirá la Fase 7). | Hecha (tarea 5.4, commit `dac26c8`) |
| D34 | Los atributos de un espacio de nombres distinto del de XSD (p. ej. `jaxb:version`, `xsi:schemaLocation`, `x:foo`) en los elementos de XSD se aceptan y se ignoran, como las anotaciones de D23 (XSD 1.0 los admite en todo componente y no cambian su significado); no llegan al AST ni a la IR. Siguen siendo error un atributo sin espacio de nombres que XSD no define en esa construcción (`syntax`) o que el subconjunto excluye (`unsupported`), y un elemento ajeno al espacio de XSD fuera de `xs:appinfo`/`xs:documentation`. Sustituye la decisión 4 del Builder de la 5.2 («atributo de extensión» → `unsupported`). Decidido por el usuario el 3 nov. | "Subconjunto de XSD"; coherencia con D23. | Tomada; se implementa en la ronda de corrección de la 5.2 si la hay, o como primer criterio de la 5.3 |
| D35 | El Reviewer `agent-skills:security-auditor` solo se lanza en las tareas con entrada no confiable en tiempo de ejecución o con lo que exige el PDF en seguridad: validador generado y runtime (Fases 4-5, payload en streaming), mitigaciones y fuzzing (Fase 6), servidor y sandbox (Fase 7); además, una revisión de seguridad consolidada al cerrar cada fase (material para «vulnerabilidades detectadas y mitigadas» del informe). En las tareas del compilador (resto de la tarea 5, partes de la Fase 4 que solo construyen el autómata) la fase de revisión la hace solo `agent-skills:code-reviewer`, que cubre la seguridad como uno de sus cinco ejes. Prevalece sobre la tabla de roles de `/autodev`. Decidido por el usuario el 3 nov; se aplica desde la revisión de la ronda 1 de la 5.2. | Proyecto académico: el PDF pide mitigar XXE, entidades, `$ref` cíclicos, ReDoS y límites, y un sandbox, no auditar cada tarea. | Tomada (3 nov) |
| D36 | Parser del subconjunto XSD (tarea 5.2; `docs/fase2/gramatica-xsd.md`, `xsd.ll1`): descendente recursivo LL(1) como `schema-parser` (40 no terminales, 23 clases `S_*`, 29 terminales), error en `Box` y mensajes `#[cold]`; el tope de profundidad (256, `MAX_DEPTH_LIMIT`, solo a la baja) lo aplica el **lector** (`Reader::with_max_depth`): `limit` en el `<` del elemento 257, sin leer más (cuenta también lo que hay dentro de `xs:documentation`, que se salta con un contador); pila medida en debug ≈ 1015 KiB en el peor caso (≈ 1 MiB de margen sobre 2 MiB), 371 KiB en release; los `Clone`/`Debug`/`PartialEq`/`Drop` derivados del AST son recursivos (≤ 256): todo consumidor recursivo en el hilo con `stack_size` (D7), y `==` del AST solo dentro de un mismo análisis (mezcla contenido e identidad de `NsId`). Las construcciones de §3.3 son celdas vacías con categoría y motivo (`subset.rs`). Tomadas por el Builder. | D3, D6, D7, D21. | Hechas (tarea 5.2, commit `dd6fbec`) |
| D37 | Errores y valores del parser XSD (tarea 5.2): códigos `syntax` (celda vacía, texto, elemento ajeno o desconocido, atributo desconocido o calificado con el URI de XSD con cualquier prefijo, obligatorio que falta), `unsupported` (§3.3 con categoría y motivo), `value`, `limit`, y `namespace`/`lexical` del lector; un atributo sin espacio de nombres fuera de la lista blanca es `unsupported` si XSD lo define en esa construcción y `syntax` si no (por eso `fixed` en `xs:enumeration`/`xs:pattern` es `syntax` y en las facetas de rango y de longitud `unsupported`); la lista blanca depende del contexto global/local; cada atributo se convierte al llegar, en el orden del documento (*collapse* sobre una copia de `name`, ocurrencias, `use`, `mixed`, `elementFormDefault`, `targetNamespace` y facetas de rango y longitud; `default`, `fixed`, `enumeration` y `pattern` sin tocar); los obligatorios (`name` en globales y en todo `xs:attribute`, `base` en `xs:extension`, `value` en facetas) se informan en el `>`/`/>`; el error informado es el primero del documento salvo dos excepciones documentadas en §7/§9 (errores de buena formación o de espacios de nombres de la etiqueta, y un error léxico entre el comienzo de un texto y el token que lo termina); `mixed="false"`/`"0"` se aceptan; `mixed="true"`, `use="prohibited"`, `version` se rechazan; se aceptan `xs:attribute` globales (su uso lo decide la 5.3), ocurrencias en los grupos y base anónima de `xs:restriction`. Tomadas por el Builder. | D6, D34; XSD 1.0 (esquema de esquemas). | Hechas (tarea 5.2, commit `dd6fbec`) |
| D38 | Regla de rondas en `/autodev` (fase 5): un Important o Medium **abre ronda** si es un defecto real en el código de la tarea (aunque sea pequeño), un criterio de aceptación sin cumplir, una contradicción con una decisión registrada o algo que heredarían las tareas siguientes; **no abre ronda** si es solo documentación de la tarea (se corrige en el cierre de documentación), cobertura sobre un comportamiento ya correcto o de una tarea futura (estos dos pasan como criterio de aceptación de la tarea que corresponda en `tasks/todo.md` y se anotan en `pendientes.md`). Nada se ignora. Los Critical y High siguen rechazando como antes. Escrita en `.claude/commands/autodev.md`. Decidido por el usuario el 3 nov. | Ahorro de rondas que solo añadían casos de prueba o pulían frases, sin dejar defectos para después. | Tomada (3 nov) |
| D39 | Valores por defecto sin efecto en XSD: se aceptan y se ignoran (no llegan al AST ni a la IR) `attributeFormDefault="unqualified"`, `form` con el valor que ya rige (`unqualified` en `xs:attribute`; en un `xs:element` local, el mismo que el `elementFormDefault` del esquema), `nillable="false"` y `abstract="false"`, como ya se hacía con `mixed="false"`; los valores que cambian la validación (`attributeFormDefault="qualified"`, `form` distinto del vigente, `nillable="true"`, `abstract="true"`) se siguen rechazando como `unsupported`. Cierra la asimetría 5.2-CR2-S4 (`docs/fase2/pendientes.md` §17). Modifica D37 en ese punto. Decidido por el usuario el 3 nov («`form` igual al vigente» es la lectura del orquestador de «los que cambian la validación se siguen rechazando»). | "Subconjunto de XSD"; `attributeFormDefault="unqualified"` es muy frecuente en los XSD generados por herramientas. | Tomada (3 nov); criterio de la tarea 5.3 en `tasks/todo.md`; el alcance (§3) lo actualiza la 5.7 |
| D40 | Análisis y bajada XSD (tarea 5.3; `docs/fase3/xsd.md`): API `xsd::analyze(&Schema) -> Result<Analysis, Vec<XsdError>>` y `xsd::lower(&Analysis) -> Result<VerifiedIr, InternalError>` (siempre con `verify`, D15); `Analysis` con campos privados es la única entrada de `lower` (para XSD resuelve `AnalyzedTable`, `docs/fase3/pendientes.md` §13); un `TypeRef` sin resolver da arista colgante (error interno), nunca pánico. **Etapas (B2):** 1 componentes y nombres (NCName, forma de ocurrencias, combinaciones de atributos, D14, nombres repetidos e I10, resolución, tipos fuera de §3.2, D27, `xs:simpleType` «pendiente (tarea 5.6)»), 2 modelos de contenido (I11, *all-group limited*, EDC); todos los errores de la primera etapa que falla, por offset. **Códigos:** `duplicate-name`, `unresolved` (también tipo complejo en un atributo), `unsupported` (D14, D27, tipos fuera de §3.2, pendiente 5.6), `syntax` (`name`/`ref`, `type`/anónimo, `default`/`fixed`, requerido con `default`, `type` con `xs:simpleType`), `value` (NCName, ocurrencias), `content-model` (I11, I12, EDC). **Posiciones:** en el valor del atributo culpable; `default`/`fixed` en el segundo; mín > máx en el valor de `maxOccurs` (si falta, el de `minOccurs`); D14 y «sin name ni ref» en el `<` del elemento; tipo anónimo en conflicto y `xs:simpleType` pendiente en su `<`; duplicados y EDC en la segunda aparición, con la primera en el mensaje. **Orden B3:** documento, tipos complejos en preorden de declaración (cada global seguido de los anónimos de su modelo), sintéticos por primer uso (envoltorio antes que su `builtin`), con el rango de su primer uso; tipo anónimo de un global con padre el documento y paso `Entry("element", nombre)` (S-5). **Resolución:** tabla del esquema y después los predefinidos de §3.2, por `(NsId, local)` con `RandomState` (D31); `xs:anySimpleType` no se puede nombrar (solo tipo implícito de un atributo sin tipo); `xs:anyType`, fecha/hora y resto de XSD 1.0 → `unsupported`. **Atributos globales:** espacio propio, se comprueban, no se bajan. **D39 en el parser:** lista blanca solo con el valor sin efecto, sin rastro en el AST; el otro valor válido es `UnsupportedValue` en el valor, uno inválido `InvalidValue`. **Contenido vacío** según XSD §3.4.2 cl. 2.1.1-2.1.3 (sin `Content`); sin elementos globales, documento `Choice([])`. **EDC por nombre local** (no expandido), como la IR (`ir.md` §5.2) e I12: desviación de XSD 1.0 cos-element-consistent (5.3-CR1-I1; la 5.7 la lleva a `00-alcance.md` §3). Nits de la 5.2 cerrados (CR-S4, N1-N3, CR-S7); 5.2-CR-S6 se mantiene. Tomadas por el Builder. | D6, D7, D14, D15, D16, D27, D31, D36, D37, D39; XSD 1.0. | Hechas (tarea 5.3, commit `dac26c8`) |
| D41 | Integración XSD en `schemac` (tarea 5.4; `docs/fase3/bajada.md` §4-§5, `docs/fase3/xsd.md` §9): `formato(bytes)` salta un BOM completo `EF BB BF` y ` \t\r\n` y decide por `<` (D22); un BOM incompleto no es BOM (JSON, `UnexpectedByte` en el offset 0, D10); el BOM no se quita de la entrada; `check` XSD válido informa «esquema válido (subconjunto de XSD 1.0); N tipos complejos»; `check` pasa por `ejecutar_check` como `ir` por `ejecutar_ir`, y `en_hilo` devuelve siempre 3 ante un pánico (D33); `abrir` añade `O_NONBLOCK` (Unix, `libc` solo `cfg(unix)` y solo la constante, sin `unsafe`) y decide con `is_file()` sobre el descriptor, conservando el `fs::metadata` previo (cierra 4.3-L1); AST, `Analysis` e IR se crean y liberan dentro del hilo de 8 MiB (D7, D36); doradas XSD en `tests/ir/xsd/<n>.json` con fuente `tests/schemas/xsd/parse-ok/<n>.xsd` (tres: `pedido`, `valid-xml-pedido-01`, `espacio-por-defecto`), segundo bucle en `make doradas`, y `IR_DOT_XSD := pedido` para `docs/fase3/ir-pedido.dot`; tabla ρ de XSD (peor cociente ≈ 21,6 con r = 8, ρ vigente sin cambios; memoria a 16 MiB ≤ ≈ 59×); §24 (temporales) cerrado como aceptado sin `tempfile`. Tomadas por el Builder. | D7, D10, D17, D22, D33, D36; robustez frente a entradas hostiles (Fase 7). | Hechas (tarea 5.4, commit `dac26c8`) |
| D42 | Trabajo en paralelo de dos personas (13 nov): el plan general pasa al repo (`tasks/plan-general.md`; la copia de fuera queda obsoleta), igual que el enunciado (`docs/enunciado/`), `.claude/agents/` y `.claude/commands/` (para que `/autodev` funcione igual en ambas cuentas) y un `CLAUDE.md` con las reglas. El estado de las tareas se lleva en `tasks/CHECKLIST.md`: una línea por tarea con responsable y estado; se toma una tarea (commit + push del checklist) antes de empezarla y se marca al cerrarla, en el mismo commit de la tarea. Cada persona hace sus propios commits (los agentes siguen sin hacer `commit`/`push`), sobre `main` con `git pull --rebase` antes de empezar y antes de subir; mensajes con el ID de la tarea. Las decisiones que cambian una interfaz que usa la otra persona (IR y `VerifiedIr`, JSON v2 de `schemac ir`, API C-ABI, API del servidor, `00-alcance.md`) las aprueban los dos. Los números D nuevos se toman con el plan recién actualizado; si al hacer `pull` otro ya usó ese número, se renumera la propia. Detalle en `COLABORAR.md`. | El PDF pide un proyecto en pareja; sin un estado compartido, dos `/autodev` en paralelo se pisan. | Tomada (13 nov) |

## Estado actual y pendientes

**Retomar aquí** (una línea por pista; cada persona edita solo la suya al cerrar una tarea; el estado tarea por tarea está en `tasks/CHECKLIST.md`):

- **General (13 nov):** Fases 1-3 cerradas para JSON Schema; frontal XSD con 5.1-5.4 commiteadas (`746a9c3`, `b171e48`, `dd6fbec`, `dac26c8` «Final fase 3», que junta 5.3 y 5.4). Fases 4-7 sin empezar: `automaton`, `codegen`, `runtime-rs`, `runtime-c`, `bench`, `server/`, `web/`, `sandbox/` e `infra/` siguen vacíos. **Vamos atrasados:** el hito de la semana 3 (6 nov, prueba diferencial) no se cumplió y el plan congela el código el 19 nov y entrega el 22 nov; aplican los recortes de «Riesgos y recortes de alcance» (solo tablas densas, `oneOf` como verificación diferida, frontend simple) y D28 (5.5-5.7 detrás de la Fase 4). **Acordado (S1, 13 nov):** entrega el domingo 22 nov, cronograma sin cambios; pista A (compilador) Leonardo, pista B (plataforma) Bayron. **Procedimiento:** `/autodev` con D32, D35, D38 y D42; contexto de cada tarea en `tasks/contexto/` (local).
- **Pista A, compilador (13 nov):** siguiente, PC-C (punto de control C: `make lint`/`test`/`docs-check`, revisar `schemac ir` sobre `tests/schemas/xsd/parse-ok/pedido.xsd` y `valid-xml-pedido-01.xsd` con y sin `--dot`, la detección de formato D22 y el texto de uso) y después F4.1 (definición formal y construcción composicional del DPDA sobre `VerifiedIr`; leer antes `docs/fase3/ir.md` entero, §7 y §11). Observaciones que hereda la Fase 4: `docs/fase3/pendientes.md` §5 y la sección «Vigilar más adelante» (tope de todo producto, `location_clipped`, máscara de `DependentRequired`, `$ref` con hermanas como producto §22).
- **Pista B, plataforma (13 nov):** se puede empezar ya, sin esperar al compilador: S0 (Atlas y AWS), F6.7 (líneas base, que además son el oráculo del criterio de la Fase 4), F6.6 (benchmark D1), F7.1 (servidor sobre `schemac check`/`ir`; requisitos de `docs/fase3/pendientes.md` §1-§2 y D7), F7.3 (sandbox) y T5.5 (números XSD, criterios en `tasks/todo.md`). Observaciones abiertas de la tarea 5: de la 5.3, `docs/fase3/pendientes.md` §33 y §3 y `docs/fase2/pendientes.md` §17 (criterios de 5.6 y 5.7 en `tasks/todo.md`); de la 5.4, `docs/fase3/pendientes.md` §1 (tabla ρ XSD) y «Cierre de la 5.4» (cobertura de `corpus_clasificado` → 5.6; `O_NOCTTY`, IR rota con `Format::Xml`, glob vacío del `Makefile` y nits → próxima tarea de la CLI).

**Pendientes abiertos tras 5.2** (resumen; detalle en `docs/fase2/pendientes.md`, fuente de verdad): profundidad resuelta, pero sin topes de memoria en el frontal: el AST llega a ≈ 34× la entrada y una etiqueta con millones de atributos a ≈ 31-40× (§12; tope de nodos o hijos coherente con D26 en la 5.7, tope de atributos y `xmlns` en el lector, `RLIMIT_AS` y concurrencia en la Fase 7); restricciones que el parser deja a la 5.3 (§16: combinaciones `name`/`ref` y `type`/anónimo, D14, NCName, ocurrencias, `default`/`fixed`, nombres repetidos, fechas, `xs:attribute` globales); `00-alcance.md` §3 sin D23/D34/lista blanca (§17, 5.7); valores por defecto sin efecto decididos como D39 (criterio de la 5.3); posición del error de texto (§18, cosmético); cobertura y detalles del cierre (§19: casos de corpus de mutaciones vivas de `subset.rs` y `texto-con-comentario-malo.xsd` y copias de `parse-ok` para la 5.3; nits de `parser.rs`/`subset.rs`/`reader.rs`; categorías como `enum` y `XsdError` con campos `pub(crate)` en la Fase 7); recorrido del AST con pila explícita en la 5.3 (§5); `\r` suelto en `Pos::advance` (`docs/fase1/pendientes.md` §3, próxima tarea del lexer).

Lo abierto de tareas anteriores está completo en la sección «Vigilar más adelante» de `docs/fase1/pendientes.md`, `docs/fase2/pendientes.md` y `docs/fase3/pendientes.md` (fuente de verdad; lo cerrado, en `docs/faseN/historial.md`). Los resúmenes históricos («Pendientes abiertos tras 4.0 … 5.1», la auditoría del 20 oct y sus notas) están en el **Anexo A** al final de este plan.

## Herramientas y entorno

Todo el núcleo se escribe en Rust (compilador, runtime de referencia y backend), así se comparte código entre compilador y servidor; C solo aparece como lenguaje de salida y en la línea base de XML.

| Área | Herramienta | Para qué |
| --- | --- | --- |
| Compilador | Rust estable (rustup, cargo, clippy, rustfmt) | Compilador `schemac`, código Rust generado |
| Salida C | GCC o Clang en modo C11, Make | Compilar el validador C generado; `-fsanitize=address,undefined` en pruebas |
| FFI | cbindgen | Generar el header C del API C-ABI del validador Rust |
| WASM (opcional) | target `wasm32-unknown-unknown`, wasm-bindgen | Ejecutar el validador generado en el navegador |
| Diagramas formales | Graphviz (DOT exportado por el compilador), JFLAP | AFD y DPDA en la documentación; JFLAP para verificar a mano autómatas pequeños |
| Documentación | LaTeX en Overleaf (o Markdown + Mermaid) | Definiciones formales, gramáticas, árboles de derivación, informe |
| Pruebas | `cargo test`, proptest, insta, cargo-fuzz (libFuzzer) | Unitarias, propiedades, snapshots del código generado, fuzzing del tokenizador y validadores |
| Métricas | criterion, `perf stat`, `/usr/bin/time -v`, valgrind massif | MB/s, ciclos por byte, pico de memoria |
| Conteo de asignaciones | `GlobalAlloc` propio en Rust, `-Wl,--wrap=malloc` en C | Demostrar 0 asignaciones dinámicas durante la validación |
| Líneas base | serde\_json + crate `jsonschema`; libxml2 (DOM + XSD) | Validadores genéricos basados en árbol para comparar |
| Backend | axum + tokio, driver oficial `mongodb` | API REST + WebSocket |
| Frontend | React + Vite + TypeScript, Monaco Editor, Cytoscape.js, d3-hierarchy, Recharts | Editor de esquema, autómata, árbol de derivación, panel de métricas |
| Base NoSQL | MongoDB Atlas, clúster gratuito M0 en región AWS | Casos de prueba y resultados de métricas |
| Sandbox | Docker sin red (`--network none`), límites de memoria, CPU y procesos, timeout, usuario no root, FS de solo lectura | Compilar y ejecutar el código generado |
| Despliegue | AWS EC2 (Ubuntu) + Docker Compose + Caddy (HTTPS automático) | Aplicación en la nube; Azure VM es equivalente si prefieren Azure |
| Repositorio | GitHub (solo remoto de los commits, D32) | Guardar el historial; sin Actions, Projects, issues ni PR |

Día 1, ambos: instalar el toolchain, crear el repositorio con CI mínimo, la cuenta de Atlas y la cuenta de AWS (o Azure) con alerta de presupuesto.

## Arquitectura y repositorio

El compilador es una tubería de siete etapas, una por fase del enunciado; el AST del esquema solo existe en tiempo de compilación y el validador generado nunca construye árbol del payload.

&#91;embedded content: arquitectura del compilador · 7 fases\]

Las fases 1 a 5 corren una sola vez por esquema; en producción solo corre el validador generado, que recibe bytes y entrega el reporte de la Fase 6.

**Estructura del repositorio (monorepo, workspace de Cargo):**

```
Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados/
├── crates/
│   ├── lexer/          # Fase 1: AFD UTF-8, tokens JSON y XML (streaming, reanudable, no_std)
│   ├── schema-parser/  # Fase 2: LL(1) de JSON Schema → AST (el de XSD vive en xsd/, D21)
│   ├── semantic/       # Fase 3 (JSON Schema): meta-esquema, $ref/$defs, ciclos, bajada → IR
│   ├── ir/             # IR común, verificador y clasificación (a)/(b)/(c) (D13, D19)
│   ├── xsd/            # Frontal XSD: lector XML con espacios de nombres, LL(1), AST, análisis y bajada → IR (D21)
│   ├── automaton/      # Fase 4: construcción, pesos tropicales, determinismo, minimización
│   ├── codegen/        # Fase 5: emisores C y Rust, generador de payloads
│   ├── runtime-rs/     # Runtime Rust no_std; intérprete de referencia (oráculo)
│   ├── runtime-c/      # Runtime C11 del validador generado
│   ├── bench/          # Fase 6: arnés de métricas y líneas base
│   └── cli/            # schemac compile esquema.json --lang c|rust --out dir/
├── server/             # Fase 7: axum (REST + WebSocket), cliente MongoDB
├── web/                # Fase 7: React + Vite
├── sandbox/            # Dockerfile del contenedor de compilación/ejecución
├── tests/cases/        # Casos de prueba semilla por categoría (se suben a MongoDB)
├── infra/              # docker-compose.yml, Caddyfile, scripts de despliegue
├── scripts/            # ll1.py (crear_issues.py sin uso, D32)
└── docs/               # Alcance, teoría por fase, diagramas DOT, informe
```

**API C-ABI del validador generado** (misma forma en Rust con `extern "C"`):

```c
#define SV_MAX_DEPTH 256        /* configurable al compilar (docs/00-alcance.md §4) */
typedef struct { /* estado, pila acotada, contadores, máscaras */ } sv_state_t;

void sv_init(sv_state_t *s, const sv_limits_t *lim);
int  sv_feed(sv_state_t *s, const uint8_t *chunk, size_t len);  /* 0 = sigue, <0 = error */
int  sv_finish(sv_state_t *s, sv_report_t *out);  /* violaciones, puntero JSON, offset */
```

`sv_state_t` tiene tamaño fijo conocido en tiempo de compilación: el llamador lo pone en la pila o en memoria estática, por eso no hay `malloc`.

## Cronograma semana a semana

La semana 3 es la crítica: si el viernes 6 de noviembre no hay validación de extremo a extremo, se aplican los recortes de la sección de riesgos antes de empezar la semana 4.

&#91;embedded content: cronograma por fase · 5 semanas\]

Las fases 1 a 3 se solapan en las dos primeras semanas porque A escribe la teoría mientras B implementa; de la semana 3 en adelante cada uno lleva su propia fase.

### Semana 1 (19–25 oct): subconjunto, Fase 1 y gramáticas

Hito: tokenizadores JSON y XML en streaming pasan sus pruebas con cualquier tamaño de bloque, y está escrito el documento del subconjunto soportado.

> **Contexto para la sesión (semana 1)**
> - **Estado al 20 oct:** las tareas técnicas de la semana están hechas (alcance, AFD, gramáticas, lexers reanudables, DOT). Quedan las tareas de cuentas y tablero, y corregir los hallazgos 2 a 8 de la auditoría.
> - **Leer primero:** `docs/00-alcance.md`, `docs/fase1/lexico.md`, `crates/lexer/src/{lib,utf8,json,xml}.rs`, `tests/README.md`.
> - **Decisiones vigentes:** D8 (DOCTYPE), D9 (posiciones saturantes).
> - **Pendientes heredados:** hallazgos 4, 5, 6 y la parte de la Fase 1 del hallazgo 8 (ver la tabla de la auditoría).
> - **Cómo verificar:** `make lint`, `make test`, `make docs` sin diferencias en `git diff docs/`.

**Ambos (lunes y martes)**

- [x] Escribir la tabla del subconjunto: cada palabra clave de Draft 2020-12 y de XSD con su clase (a) directa, (b) registro/contador, (c) fuera del DPDA puro, y su estrategia — clasificación alineada con `docs/00-alcance.md`
- [x] Crear repositorio, workspace de Cargo y CI en GitHub Actions (lint + wasm, tests en Ubuntu y Windows, cobertura)
- [x] ~~Verificar que la CI esté en verde en GitHub y crear el tablero en GitHub Projects (`scripts/crear_issues.py`)~~ — descartada el 3 nov (D32)
- [ ] Crear clúster en MongoDB Atlas y cuenta en AWS con alerta de presupuesto

**Persona A**

- [x] Definir el alfabeto Σ = bytes 0–255 y las clases de bytes
- [x] Escribir gramáticas regulares y dibujar AFD de: UTF-8, números JSON, cadenas con escapes (`\uXXXX`), literales `true/false/null`, tokens XML (etiquetas, atributos, texto, comentarios, CDATA, referencias de entidad)
- [x] Escribir la gramática libre de contexto de JSON y de XML bien formado; calcular FIRST y FOLLOW y demostrar que la de JSON es LL(1)

**Persona B**

- [x] Implementar el AFD de UTF-8 y el tokenizador JSON reanudable (el estado sobrevive entre bloques)
- [x] Implementar el tokenizador XML reanudable
- [x] Pruebas: cada payload del corpus tokenizado con bloques de 1, 7 y 4096 bytes debe dar la misma secuencia de tokens
- [x] Exportar los AFD del código a DOT para que coincidan con los diagramas de A

### Semana 2 (26 oct – 1 nov): Fases 2 y 3

Hito: un esquema JSON o XSD entra al compilador y sale como IR verificada, o como lista de errores con ubicación.

> **Contexto para la sesión (semana 2)**
> - **Estado al 20 oct:** el parser LL(1) de JSON Schema está hecho y adelantado (`crates/schema-parser`, gramática en `docs/fase2/esquema.ll1`, tabla en `ll1-esquema.md`). Errores con línea y columna, claves repetidas detectadas, profundidad acotada a 256. Hay 6 casos en `tests/cases` y un manifiesto de esquemas erróneos en `tests/schemas/errores.json`.
> - **Leer primero:** `docs/fase2/gramatica-esquema.md`, `docs/fase2/pendientes.md`, `crates/schema-parser/src/{lib,json_schema,error}.rs`, `docs/00-alcance.md` §2 (palabras clave) y §4 (límites).
> - **Decisiones vigentes:** D1 (sin tiempo de pared en la CI), D2 (claves repetidas), D3 (AST por lenguaje, IR común), D6 (palabras clave no soportadas), D7 (profundidad configurable con tope).
> - **Pendientes heredados:** hallazgos 2, 3 y 7; en `pendientes.md`, vigilar el margen de pila en debug (§5) y el límite de tamaño del esquema (§6, va en el servidor).
> - **Al integrar el parser en `schemac` (Fase 3) o en el servidor (Fase 7):** llamarlo dentro de `std::thread::Builder::new().stack_size(…)` con ≥ 2 MiB, no en el hilo principal (D7); en Windows el hilo principal tiene 1 MiB y 256 niveles en debug usan unos 1,34 MiB.
> - **Antes de empezar XSD:** tener definida y auditada la forma de la IR (D3). La IR tiene que representar nodo, clase (a)/(b)/(c), estrategia, orden de hijos (para `sequence`) y distinción atributo/elemento.

**Persona A**

- [x] Gramática del metalenguaje de esquemas (subconjunto) y parser descendente LL(1) que produce el AST
- [x] Validación del esquema contra el meta-esquema 2020-12 (restringido al subconjunto) — `crates/semantic` (`check_meta_schema`), `schemac check`, `docs/fase3/meta-esquema.md`; aprobada el 21 oct en 2 rondas
- [x] Tabla de símbolos para `$defs` y resolución de `$ref` por puntero JSON — `crates/semantic` (`SymbolTable`, `pointer`), `docs/fase3/referencias.md`; RFC 6901/3986, anclas únicas, `$ref: ""` = `#`; aprobada el 21 oct en 1 ronda (commit `c1a1a96`)
- [x] Detección de ciclos no productivos (DFS sobre referencias que no consumen entrada) — `crates/semantic/src/cycles.rs` (Tarjan iterativo, errores con camino y fragmentos recortados, costo lineal medido), `docs/fase3/ciclos.md`, D12; aprobada el 22 oct en 2 rondas (la 1 rechazada por memoria cuadrática en los errores)
- [x] Cerrar las observaciones de la tarea 3 (tarea 4.0) — prueba de que `fragment_clipped` no parte escapes (oráculo exacto, UTF-8 de 1-4 bytes), fórmula O(m·9·(h + 80)), `debug_assert!` en `path_in`, `fragment_clipped` ~4× más rápido, `docs/fase3/pendientes.md` nuevo; aprobada el 22 oct en 2 rondas (sin commit)
- [x] Diseño de la IR común (tarea 4.1) — crate `ir` (`lib.rs`, `verify.rs`: I1–I14 iterativo, O(n + m log m)), 63 pruebas en `crates/ir/tests/forma.rs`, `docs/fase3/ir.md`, D13–D16; punto de control A aprobado el 26 oct; aprobada con observaciones el 26 oct (segunda invocación, 3 rondas; sin commit)
- [x] Bajada de JSON Schema a la IR y `schemac ir` (tarea 4.2) — `semantic::lower` iterativo, `VerifiedIr` (D15), `schemac ir [--dot]` (D17), 5 doradas en `tests/ir/`, `docs/fase3/bajada.md` e `ir-arbol-recursivo.dot`; aprobada el 27 oct en 3 rondas (commit `edd75e5`)

**Persona B**

- [ ] Parser del subconjunto XSD hacia su propio AST y bajada a la IR común (D3). Avance: **5.1 lector XML con espacios de nombres** (`crates/xsd`, `docs/fase2/lector-xsd.md`, D30-D31; aprobada con observaciones el 29 oct en 3 rondas; commits `746a9c3`, `b171e48`); **5.2 gramática LL(1) y parser → AST** (`docs/fase2/{xsd.ll1,gramatica-xsd.md}`, corpus `tests/schemas/xsd/`, D34, D36-D37; aprobada con observaciones el 3 nov en 2 rondas; commit `dd6fbec`); **5.3 tabla de símbolos, restricciones y bajada de tipos complejos a `VerifiedIr`** (`crates/xsd/src/{symbols,check,lower}.rs`, `docs/fase3/xsd.md`, 39 casos `semantic-error`, D39-D40; aprobada con observaciones el 4 nov sin rondas de corrección; commit `dac26c8`); **5.4 `schemac check|ir` con XSD** (D22, D33, D41, `corpus_clasificado` XSD, 3 doradas `tests/ir/xsd/`, `docs/fase3/ir-pedido.dot`, tabla ρ XSD; aprobada con observaciones el 4 nov sin rondas de corrección; commit `dac26c8`); falta el **punto de control C**; siguen (punto C), 5.5, 5.6 y 5.7 (aplazada por D28) en `tasks/todo.md`. Obligaciones del frontal (`docs/fase3/ir.md` §4, D15): rechazar con posición toda *Schema Component Constraint* y toda `src-*` del subconjunto antes de bajar, entre ellas los ciclos de `base` (iterativo, antes de seguir la cadena), UPA, `default`/`fixed` como valores del tipo, atributos repetidos (I10), `minOccurs` > `maxOccurs` (I11), faceta repetida o `length` con `minLength`/`maxLength` en un paso (I4), `min*`/`max*` inclusivo y exclusivo a la vez, I12 e I13; validar la forma léxica XSD de facetas y enumeraciones antes de reescribirlas a RFC 8259; `xs:float`/`xs:double` se comparan tras redondear al binario IEEE (revisión final de 4.1, I-2; hoy `ir.md` §7 dice comparación decimal)
- [x] Módulo de clasificación de palabras clave (a)/(b)/(c) sobre la IR (tarea 4.3) — `crates/ir/src/classify.rs` (tabla única `RuleKind::classification`, `classify(&VerifiedIr)` iterativo O(n + m)), `schemac ir` con clase y estrategia (JSON v2, D17), 4 casos `non-dpda`, `docs/fase3/clasificacion.md`, D19-D20; aprobada el 28 oct en 1 ronda (commit `edd75e5`). Cierra el criterio «Terminado cuando» de la Fase 3 para JSON Schema
- [ ] Exportar árboles de derivación de payloads de ejemplo (JSON para el frontend, DOT para la documentación)
- [ ] Generador de payloads válidos v1 a partir de la IR; cargar los primeros 20 casos al corpus

### Semana 3 (2–8 nov, lunes festivo): Fases 4 y 5

Hito (viernes 6 nov): esquema → DPDA → C y Rust generados validan correctamente el corpus válido e inválido de JSON y al menos un XSD simple.

> **Contexto para la sesión (semana 3)**
> - **Estado de partida esperado:** IR verificada para JSON Schema y XSD (hito de la semana 2). Al 20 oct, `automaton`, `codegen` y `runtime-rs` son solo el comentario del crate, y `crates/runtime-c/include/schema_validator.h` está vacío.
> - **Leer primero:** la sección "Arquitectura y repositorio" (API C-ABI), Fase 4 y Fase 5 de este plan, `docs/00-alcance.md` §2 (estrategia de cada palabra clave, combinadores en §2.7), §3 (subconjunto XSD), §4 (tope de 4 096 estados, `SV_MAX_DEPTH`), y `docs/fase3/ir.md` entero (§7 dice de qué dato de la IR sale cada pieza del DPDA; §11 y «Pendientes abiertos tras 4.1» de este plan, lo que la Fase 4 tiene que resolver).
> - **Decisiones vigentes:** D4 (semianillo ℕ ∪ {∞}), D5 (dependencias), D7 (profundidad con tope).
> - **Riesgo principal:** si el viernes 6 nov no pasa la prueba diferencial, aplicar los recortes de la sección de riesgos antes de empezar la semana 4.

**Persona A**

- [ ] Definición formal de M = (Q, Σ, Γ, δ, q0, Z0, F, w) y construcción composicional por nodo de la IR
- [ ] Pesos en el semianillo tropical (ℕ ∪ {∞}, mín, +) (D4): costo por transición y conteo de violaciones
- [ ] Verificación de determinismo de δ y minimización (Hopcroft) de las partes finitas
- [ ] `pattern` → AFD (sin retrocesos ni lookaround) y producto acotado para `allOf`/`anyOf`/`oneOf`, con tope de estados para **todo** producto (también `allOf`, `Ref` con hermanas y `not` sobre (c); caso de prueba: `allOf` de `pattern` `^(a{p})*$` con los 15 primeros primos), contando estados y transiciones antes de reservar; las componentes del producto son el conjunto de `NodeId` distintos alcanzables por E₌ (no los caminos), y los combinadores se evalúan como circuito sobre sus veredictos (con multiplicidad en `oneOf`); `location_clipped` en mensajes y codegen (`ir.md` §11, pendiente 2). Incluye rechazar en la Fase 3 los `pattern` con lookaround o retrorreferencias: hoy pasan la validación semántica (`docs/fase3/meta-esquema.md` §8) y es la mitigación de ReDoS que exige el PDF

**Persona B**

- [ ] Intérprete de referencia del DPDA en `crates/runtime-rs/` (sirve de oráculo)
- [ ] Emisor Rust: tablas densas por clase de byte, pila de profundidad acotada
- [ ] Emisor C con el mismo API C-ABI; header con cbindgen para el lado Rust
- [ ] Prueba diferencial: intérprete, C generado y Rust generado deben dar el mismo veredicto en todo el corpus

### Semana 4 (9–15 nov): Fases 6 y 7

Hito: con `docker compose up` se puede editar un esquema, compilarlo, descargar el código, validar payloads y ver métricas en vivo.

> **Contexto para la sesión (semana 4)**
> - **Estado de partida esperado:** prueba diferencial (intérprete vs C vs Rust) pasando en el corpus (hito de la semana 3). `server/` solo imprime la versión; `web/`, `sandbox/` e `infra/` tienen solo un README "Pendiente".
> - **Leer primero:** Fase 6 y Fase 7 de este plan, "Casos de prueba y base NoSQL", `docs/00-alcance.md` §1 (claves duplicadas en payloads) y §4 (límites).
> - **Decisiones vigentes:** D1 (aquí se construye el arnés de métricas que sustituye a cualquier prueba de tiempo en `cargo test`), D2 (claves duplicadas en payloads), D8 (DOCTYPE), D9 (posiciones en payloads de varios GB).
> - **Pendiente heredado:** límite de tamaño de subida en el servidor (`docs/fase2/pendientes.md` §6).

**Persona A**

- [ ] Reporte de error con puntero JSON, offset en bytes y línea/columna
- [ ] Límites configurables: profundidad, tamaño total, longitud de cadena, dígitos de número, tiempo
- [ ] Generador de payloads inválidos y malformados (mutaciones sobre los válidos)
- [ ] Mitigaciones: DTD y entidades externas desactivadas, `pattern` solo como AFD, `$ref` cíclico rechazado; fuzzing con cargo-fuzz

**Persona B**

- [ ] Arnés de métricas: MB/s, ciclos por byte, pico de memoria, asignaciones, profundidad máxima de pila, estados y transiciones
- [ ] Benchmark criterion en `crates/bench` del parser de esquemas con objetos de N y 4N claves (`properties`, `const`, `$defs`, `dependentRequired`): sustituye a la prueba ignorada `muchas_claves_en_tiempo_lineal` como detector de una regresión cuadrática (D1, hallazgo 2)
- [ ] Líneas base: serde\_json + `jsonschema` y libxml2
- [ ] Backend: endpoints REST, WebSocket de validación por bloques, sandbox Docker, persistencia en MongoDB. Incluye truncar o agrupar la lista de errores de compilación que se devuelve (la Fase 3 informa todos; en el peor caso la salida es ρ(r) ≈ 126 + 0,67·r veces la subida, con 118,1× medido: `docs/fase3/pendientes.md` §1), el tope de tamaño de subida, un tope de CPU y de **memoria** por compilación (proceso hijo con `RLIMIT_AS` o cgroup en el sandbox: un fallo de asignación aborta el proceso y `join`/`catch_unwind` no lo capturan), y los requisitos conjuntos de `pendientes.md` §2 (hilo propio con `join`/`catch_unwind`, nunca `panic = "abort"`, `stack_size` explícito)
- [ ] Frontend: editor Monaco, compilar y descargar, enviar payload, autómata (Cytoscape), árbol de derivación, panel de comparación

### Semana 5 (16–22 nov, lunes festivo): despliegue, pruebas, informe y demo

Hito: aplicación desplegada en AWS con el corpus completo en Atlas, informe entregado y demo ensayada.

> **Contexto para la sesión (semana 5)**
> - **Estado de partida esperado:** aplicación funcionando con `docker compose up` (hito de la semana 4) y una primera versión ya desplegada desde el lunes 9 nov.
> - **Leer primero:** "Entregables y checklist final", "Pruebas de bombeo, cómo hacerlas", sección 5 del PDF (entregables) y este registro de decisiones, que alimenta la sección "decisiones de diseño" del informe.
> - **Para el informe:** cada decisión D1–D9 y cada hallazgo de auditoría corregido es material para "decisiones de diseño, manejo de errores y casos especiales" y "vulnerabilidades detectadas y mitigadas".

**Persona A**

- [ ] Pruebas de bombeo u·vᵏ·x·yᵏ·z y casos extremos; gráficas de tiempo, pila y memoria contra k
- [ ] Informe final: complejidad, explosión al determinizar, decisiones de diseño, vulnerabilidades
- [ ] Cerrar documentación técnica (diagramas finales, árboles de derivación)

**Persona B**

- [ ] Despliegue en EC2 con Docker Compose y Caddy; Atlas en la misma región
- [ ] Cargar todos los casos de prueba y sus métricas en Atlas con un script local (sin CI, D32)
- [ ] WASM opcional, solo si todo lo anterior está cerrado

**Ambos**

- [ ] Congelar código el jueves 19 nov
- [ ] Diapositivas y guion de la demo; dos ensayos completos con cronómetro

## Detalle técnico por fase

Cada fase termina con un criterio verificable; no se pasa a la siguiente sin cumplirlo o sin anotar el recorte.

### Fase 1 — Análisis léxico

- Alfabeto: Σ = {0x00…0xFF}. Agrupar bytes en clases de equivalencia (dígito, comilla, barra, espacio, letra, byte de continuación UTF-8, etc.) para que las tablas sean pequeñas.
- AFD de UTF-8: rechaza secuencias sobrelargas, sustitutos (U+D800–DFFF) y bytes > U+10FFFF.
- Número JSON: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`. Cadena: comillas, escapes `\" \\ \/ \b \f \n \r \t \uXXXX`, sin controles sin escapar.
- XML: nombre, `<`, `</`, `/>`, `>`, `=`, valor de atributo, texto, comentario, CDATA y las 5 entidades predefinidas.
- Streaming: el tokenizador guarda su estado (estado del AFD + bytes parciales) al terminar un bloque y continúa con el siguiente.
- Lexer del propio esquema: el de JSON se reutiliza para JSON Schema y el de XML para XSD.
- Terminado cuando: la misma entrada partida en bloques de cualquier tamaño produce los mismos tokens.

### Fase 2 — Análisis sintáctico

- GLC de JSON: `value → object | array | STRING | NUMBER | true | false | null`, `object → { members? }`, `members → pair (, pair)*`, etc., reescrita sin recursión izquierda y con tabla LL(1) sin conflictos.
- GLC de XML bien formado: `element → EmptyTag | STag content ETag`. Explicar en el informe que la coincidencia de nombres de apertura y cierre requiere pila; con el esquema el conjunto de nombres es finito, así que cabe en Γ.
- Metalenguaje: gramática del subconjunto de JSON Schema (objeto de palabras clave) y del subconjunto de XSD (`element`, `complexType`, `sequence`, `choice`, `all`, `attribute`, `simpleType`, `restriction`).
- Parser descendente LL(1) escrito a mano (funciones recursivas) que construye el AST del esquema; un AST por lenguaje, ambos bajan a la IR común (D3). Las claves repetidas se rechazan en cualquier objeto del esquema, también dentro de `const`/`enum` (D2). La profundidad es configurable con un tope máximo seguro (D7).
- Árboles de derivación: el compilador exporta el árbol de un payload de ejemplo en JSON (para d3-hierarchy) y DOT (para la documentación).
- Terminado cuando: el corpus de esquemas parsea y los erróneos reportan línea y columna.

### Fase 3 — Análisis semántico

- Validar el esquema contra el meta-esquema 2020-12 restringido al subconjunto; las palabras clave no soportadas se rechazan siempre con mensaje claro (no hay modo que las ignore); las anotaciones de la lista blanca se aceptan.
- Tabla de símbolos: cada entrada de `$defs` con su puntero JSON; `$ref` se resuelve contra ella.
- Ciclos: grafo de referencias; un ciclo es no productivo si se puede volver al mismo nodo sin consumir un token (por ejemplo `A → $ref B → $ref A`). Ese se rechaza; la recursión a través de `properties` o `items` es válida y usa la pila.
- Clasificación de palabras clave:

| Clase | Palabras clave | Cómo se implementa |
| --- | --- | --- |
| (a) Directas en el DPDA | `type`, `properties`, `additionalProperties` (`false`, `true` o esquema), `items`, `prefixItems`, `enum`, `const`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` | Transiciones del autómata; las claves de `properties` y los valores de `enum` como trie |
| (b) Con registros o contadores | `required` (máscara de bits), `minItems`, `maxItems`, `minProperties`, `maxProperties`, `minLength`, `maxLength`, `minOccurs`/`maxOccurs` en XSD cuando necesitan contador (`minOccurs` ≥ 2 o `maxOccurs` acotado > 1; `?`, `*`, `+` van en (a), D16; corregido el 28 oct según `00-alcance.md` §3.1) | Registros acotados en el frame de pila del nodo |
| (c) No deterministas o no libres de contexto | `allOf`, `anyOf`, `oneOf`, `not`, `pattern`, `uniqueItems`, `dependentRequired`, `dependentSchemas` | Producto de autómatas acotado; `not` como complemento de un subautómata determinista o verificación diferida; `pattern` como AFD; `uniqueItems` con verificación diferida (tabla hash de tamaño fijo, con reporte si se desborda); `dependentRequired` verificado al cerrar el objeto con la máscara de bits y `dependentSchemas` con verificación diferida (D5) |

- Terminado cuando: cada esquema del corpus sale como IR con cada nodo marcado con su clase y estrategia.

### Fase 4 — Autómata de pila ponderado

- M = (Q, Σ, Γ, δ, q0, Z0, F, w), con w: δ → (ℕ ∪ {∞}, mín, +, ∞, 0) (D4; subsemianillo de ℝ≥0 ∪ {∞}, suficiente porque el peso cuenta violaciones).
- Dos usos del peso: el costo estima cuánto cuesta cada comprobación para ordenar las baratas primero (fallo temprano: `type` antes que `enum`); la acumulación cuenta violaciones, y el peso del recorrido (0 = conforme) va al reporte de conformidad.
- Construcción composicional: cada nodo de la IR produce un sub-autómata; un objeto anidado o un `$ref` recursivo empuja un símbolo de Γ = (nodo, estado de retorno, registros) y lo desapila al cerrar.
- Determinismo: comprobar que para cada (q, a, X) hay a lo sumo una transición y que no hay conflictos con transiciones ε.
- Minimización: Hopcroft sobre las partes finitas (tries de claves, AFD de `pattern` y de números); se documenta que no se minimiza el DPDA completo.
- Explosión: `anyOf`/`oneOf` se determinizan por producto solo hasta un tope de estados (4 096 por defecto, configurable); si se supera, se marca la rama como verificación diferida y se reporta en el informe.
- Terminado cuando: el intérprete de referencia acepta y rechaza el corpus igual que el validador de línea base.

### Fase 5 — Generación de código

- Dos estrategias de emisión: tablas densas `delta[estado][clase]` y saltos directos (`switch`/`goto` en C, `match` en Rust); elegir una por defecto y medir la otra si hay tiempo.
- Pila de profundidad acotada (`SV_MAX_DEPTH`, 256 por defecto) en arreglo estático; desbordarla es un error reportado, nunca un fallo de memoria.
- Sin asignación dinámica: se verifica con el contador de asignaciones en cada prueba.
- API C-ABI de la sección de arquitectura; WASM opcional.
- Generador de payloads válidos e inválidos, en JSON y XML, a partir de la IR, con semilla para reproducibilidad.
- Terminado cuando: la prueba diferencial (intérprete vs C vs Rust) pasa en todo el corpus.

### Fase 6 — Validación en tiempo real

- `sv_feed` acepta bloques de cualquier tamaño; el backend reenvía los bloques del WebSocket sin juntarlos.
- Error con ruta: la pila guarda el segmento de cada nivel (índice de clave o de arreglo), así se arma el puntero JSON sin memoria dinámica, más offset en bytes y línea/columna.
- Malformados: UTF-8 inválido, claves duplicadas (bit por propiedad conocida; para claves desconocidas hash acotado), etiquetas sin cerrar, fin de entrada prematuro.
- Límites: profundidad, tamaño, longitud de cadena, dígitos de número, tiempo.
- Instrumentación por ejecución: MB/s, ciclos por byte, pico de memoria, asignaciones, profundidad máxima alcanzada, número de estados y transiciones. Misma medición para la línea base DOM.
- Toda medición de tiempo vive aquí y en `crates/bench`, nunca como umbral en `cargo test` (D1).
- Terminado cuando: el arnés produce una tabla comparativa DPDA vs línea base para todo el corpus.

### Fase 7 — Aplicación web

| Endpoint | Función |
| --- | --- |
| `POST /api/schemas` | Compilar un esquema; responde id, errores, tamaño del autómata |
| `GET /api/schemas/{id}/code?lang=c\|rust` | Descargar el código generado (zip) |
| `GET /api/schemas/{id}/automaton` | Grafo del autómata para Cytoscape |
| `POST /api/schemas/{id}/derivation` | Árbol de derivación de un payload |
| `POST /api/validate` | Validar un payload completo con DPDA y línea base |
| `WS /ws/validate/{id}` | Enviar bloques y recibir progreso, errores y métricas en vivo |
| `GET /api/tests`, `POST /api/tests/run` | Listar y ejecutar casos de prueba de MongoDB |

- La compilación y ejecución del código generado ocurren solo dentro del contenedor sandbox.
- Frontend en cuatro vistas: editor de esquema, autómata, árbol de derivación y panel de comparación en vivo.
- Terminado cuando: la demo completa corre en la URL pública.

## Casos de prueba y base NoSQL

Todos los casos viven en una colección de MongoDB Atlas y se ejecutan desde la CI y desde la aplicación; la carpeta `tests/cases/` es solo la semilla que se sube.

| Categoría | Ejemplos mínimos | Resultado esperado | Formato |
| --- | --- | --- | --- |
| Válidos | Esquema plano; objetos anidados 3 niveles; árbol recursivo con `$ref`; arreglos con `items` | Acepta, 0 violaciones | JSON y XML |
| Inválidos | Tipo incorrecto; `required` faltante; propiedad adicional con `additionalProperties: false`; valor fuera de `enum`; número fuera de `minimum`/`maximum` | Rechaza con puntero JSON y número de violaciones | JSON y XML |
| Malformados | Sintaxis rota; UTF-8 inválido; claves duplicadas; etiqueta XML sin cerrar | Error léxico o sintáctico con offset | JSON y XML |
| Fuera del DPDA puro | `anyOf`/`oneOf` ambiguos; `pattern`; `uniqueItems` | Resultado correcto e indicación de la estrategia de respaldo usada | JSON |
| Bombeo | u·vᵏ·x·yᵏ·z con k creciente; arreglos muy largos; carga sostenida | Tiempo lineal en bytes, pila lineal en profundidad | JSON y XML |
| Extremos | Payload de varios GB en streaming; profundidad sobre el límite; números con miles de dígitos; expansión de entidades XML | Termina con error controlado, sin caída ni crecimiento de memoria | JSON y XML |

**Documento de un caso en la colección `test_cases`:**

```json
{
  "_id": "invalid-required-01",
  "category": "invalid",
  "format": "json",
  "schema": { "type": "object", "required": ["id"], "properties": { "id": { "type": "integer" } } },
  "payload": { "inline": "{\"name\": \"x\"}" },
  "expected": { "valid": false, "violations": 1, "errors": [{ "pointer": "", "code": "required" }] },
  "metrics": [
    { "run_at": "2026-11-12T10:00:00Z", "validator": "dpda-c", "mb_s": 0, "cycles_per_byte": 0,
      "peak_mem_kb": 0, "allocs": 0, "max_depth": 1, "states": 0, "transitions": 0 }
  ]
}
```

**Semilla frente a documento de la base:** los archivos de `tests/cases/` siguen el formato de `tests/README.md` (que la CI verifica en `crates/lexer/tests/common/mod.rs::validar_formato`), no exactamente el documento de arriba. El script que sube la semilla a Atlas traduce entre los dos: `payload` como cadena → `payload.inline`; `expected.pointer`/`offset` → `expected.errors[]`; `generator: {name, params}` → la receta de abajo; y añade `metrics`, que nunca se versiona en el repositorio.

Los payloads grandes no se guardan en el documento (límite de 16 MB por documento): se guarda la receta del generador (`{"generator": "pump", "u": "", "v": "[", "x": "1", "y": "]", "k": 100000, "seed": 7}`) y el arnés lo produce en streaming.

**Pruebas de bombeo, cómo hacerlas:**

1. Elegir pares de anidamiento: v = `[`, y = `]`; v = `{"a":`, y = `}`; v = `<a>`, y = `</a>`.
2. Barrer k = 1, 10, 100, … hasta pasar `SV_MAX_DEPTH` y medir tiempo, profundidad de pila y memoria para cada k.
3. Repetir con arreglos de longitud n creciente (profundidad fija) para separar el efecto de n y de d.
4. Graficar: el tiempo debe crecer lineal con n, la pila con d, y la memoria del DPDA debe quedar constante mientras la de la línea base crece.
5. En el informe, conectar con el lema de bombeo: el lenguaje {vᵏ x yᵏ} no es regular, por eso hace falta la pila; y la profundidad acotada es lo que convierte el DPDA en memoria O(d).

## Entregables y checklist final

Los cinco entregables se arman a lo largo de las semanas, no al final: cada fase deja su parte de documentación cuando se cierra.

**1. Documentación técnica** (A, con diagramas generados por B)

- [ ] Definición formal del lenguaje y del subconjunto soportado
- [ ] Diagramas de AFD (UTF-8, números, cadenas, literales, tokens XML) y de DPDA de al menos 2 esquemas
- [ ] Gramáticas regulares y libres de contexto (JSON, XML, metalenguaje), con tabla LL(1)
- [ ] Diagrama de arquitectura del compilador y árboles de derivación de payloads de ejemplo

**2. Código fuente** (ambos)

- [ ] Compilador y validadores generados, modulares, con documentación (`cargo doc` y README por módulo)
- [ ] Suite completa de pruebas en verde en local (`make lint`, `make test`, `make docs-check`; sin GitHub Actions, D32)

**3. Aplicación funcional** (B)

- [ ] Backend y frontend desplegados en AWS (o Azure) con URL pública
- [ ] Casos de prueba cargados en MongoDB Atlas

**4. Informe final** (A, con métricas de B)

- [ ] Análisis de complejidad: validación O(n) en bytes y O(d) en pila, con las gráficas de bombeo
- [ ] Tamaño del autómata (estados y transiciones) y riesgo de explosión al determinizar
- [ ] Decisiones de diseño, manejo de errores y casos especiales
- [ ] Vulnerabilidades y mitigaciones: ReDoS en `pattern`, desbordamiento de pila por anidamiento, XXE y expansión de entidades, `$ref` cíclicos, ejecución de código generado
- [ ] Resultados de las pruebas de bombeo

**5. Presentación** (ambos)

- [ ] Demo en vivo sobre la URL pública (con video de respaldo grabado por si falla la red)
- [ ] Explicación de las 7 fases
- [ ] Casos de prueba de cada categoría con métricas comparativas DPDA vs línea base

## Riesgos y recortes de alcance

Con dos personas, el mayor riesgo es que la Fase 4 se alargue y se coma la aplicación web; los recortes de esta tabla están ordenados para proteger primero lo que el enunciado exige.

| Riesgo | Señal temprana | Mitigación o recorte |
| --- | --- | --- |
| Construcción del DPDA se atrasa | Viernes 6 nov sin prueba diferencial pasando | Emitir primero solo tablas densas (sin saltos directos); dejar `oneOf` siempre como verificación diferida |
| Explosión de estados en `anyOf`/`oneOf` | El producto supera el tope de estados | Verificación diferida y reporte del tamaño en el informe (es un resultado válido, no un fallo) |
| Subconjunto XSD demasiado grande | Semana 2 sin XSD → IR | Limitar a `element`, `complexType`, `sequence`, `attribute`, tipos simples con `enumeration` y rangos; `choice` y `all` solo si sobra tiempo |
| `pattern` con sintaxis no regular | Expresiones con retrocesos o lookaround | Rechazarlas en Fase 3 con mensaje claro; documentarlo como mitigación de ReDoS |
| Frontend consume la semana 4 | Miércoles 11 nov sin vistas básicas | Autómata y árbol con renderizado simple de Cytoscape y d3, sin estilos; el panel de métricas es lo único que no se recorta |
| Despliegue falla cerca de la demo | Problemas de red o de cuenta en la nube | Desplegar una primera versión ya el lunes 9 nov; video de respaldo de la demo |
| Mediciones ruidosas | MB/s varía más del 10 % entre corridas | Repetir 10 veces y reportar mediana; medir en la misma instancia para DPDA y línea base; ninguna prueba de la CI depende del reloj (D1) |
| WASM | — | Es opcional en el enunciado: solo se hace si todo lo demás está cerrado el 18 nov |

## Anexo A — Historia del plan (archivada el 3 nov 2026)

Párrafos de historia que salieron del cuerpo del plan el 3 nov 2026 (archivado aprobado por el usuario), copiados literalmente. Lo que siga abierto de ellos está en `docs/faseN/pendientes.md`.

### Auditoría del estado actual (20 oct 2026)

Veredicto: **aprobado con observaciones**. `make lint` y `make test` pasan; la Fase 1 cumple su criterio de terminado y la Fase 2 lo cumple para JSON Schema. No hay nada atrasado frente al cronograma. Los hallazgos se corrigen uno por uno con el ciclo desarrollador → auditor:

| # | Hallazgo | Severidad | Estado |
| --- | --- | --- | --- |
| 1 | Desbordamiento de `Pos::line`/`Pos::column` con payloads de varios GB | Medio | Corregido y aprobado (D9) |
| 2 | Prueba de tiempo lineal dependiente del reloj de pared | Medio | Corregido y aprobado con observaciones (D1): la prueba queda `#[ignore]`; hasta que exista el benchmark de la semana 4, la CI no detecta una regresión cuadrática en las claves repetidas |
| 3 | `parse_with_max_depth` con `u32::MAX` desborda la pila | Bajo | Corregido y aprobado con observaciones (D7): tope 256 con recorte; `schemac` ya cumple la regla del hilo de ≥ 2 MiB (22 oct); queda solo el servidor (Fase 7) |
| 4 | `<!doctype` en minúsculas da `UnexpectedByte` | Bajo | Corregido y aprobado con observaciones (D8): clase nueva `d`, δ idéntica a la anterior; la definición de clases de `lexico.md` §1 pasa al hallazgo 8 |
| 5 | La CI no verifica que `docs/` esté sincronizado | Bajo | Corregido y aprobado con observaciones: job `docs` con `make docs-check` (`.dot` y `ll1-*.md`); los `.svg` y las cifras escritas a mano (estados, clases, FIRST/FOLLOW resumidos) siguen siendo responsabilidad manual |
| 6 | No se comprueba que `_id` coincida con el nombre del archivo y la categoría con la carpeta en `tests/cases` | Bajo | Corregido y aprobado con observaciones (2 rondas): `validar_formato` comprueba las reglas de `tests/README.md`; los `.json` referenciados van en subcarpeta |
| 7 | `docs/fase2/pendientes.md` con decisiones abiertas ya resueltas aquí (D2, D3) y manifiesto de errores por revisar | Bajo | Corregido y aprobado con observaciones: §2-3 cerrados con D2/D3, «AST común» corregido también en `00-alcance.md` §3 y §5, las 28 entradas de `errores.json` verificadas por dos métodos independientes |
| 8 | Imprecisiones de documentación: "seis terminales" en `gramatica-xml.md` (son siete), "límites" en la clase (a) del README, definición de clases de bytes en `lexico.md`, transiciones a error dependientes de registro sin mostrar en los DOT | Bajo | Corregido y aprobado con observaciones (3 rondas): textos corregidos, definición de clases como refinamiento de δ con cifras verificadas por `cargo test`, leyenda de los DOT con errores dependientes de registro, D10 implementado |

### Pendientes abiertos tras 4.0 … 5.1 y notas de la auditoría

**Pendientes abiertos tras 5.1** (resumen; detalle completo en `docs/fase2/pendientes.md` §8-§14, fuente de verdad): CDATA de solo espacios fuera de la raíz aceptado (§8, 5.1-SEC2-I1, toca el lexer); normalización `whiteSpace` por tipo XSD sobre una copia del valor (§9, 5.2/5.5/5.6); misma normalización de fines de línea y atributos en los payloads de la Fase 4 (§10); QName de valores y `targetNamespace` en la 5.2, `NsId` como clave solo con hasher aleatorio en la 5.3 (§11; 5.1-CR3-I1, 5.1-SEC3-I1/I2/I3); el lector no tiene límites propios y amplifica la memoria 29-33× (≈ 550 MB con 16 MiB): profundidad en la 5.2, límites o `RLIMIT_AS` y tope de concurrencia en la Fase 7 (§12, 5.1-SEC3-L2 = SEC-M1); recorte de errores garantizado por el tipo (§13, 5.1-CR3-S1/S2, 5.2); sin prueba directa de `saturating_add`/`span_len` y mezcla inglés/español en `reader.rs` (§14, 5.1-T4-1, 5.1-CR2-S3). Siguen abiertas las de 4.3, 4.2, 4.1 y 4.0 (párrafos siguientes y `docs/fase3/pendientes.md`).

**Pendientes abiertos tras 4.3** (resumen; el detalle completo, con archivo:línea, arreglo y fase de destino, está en `docs/fase3/pendientes.md`, que es la fuente de verdad, junto con `docs/fase2/pendientes.md` §5-§6 y `docs/fase1/pendientes.md`): de 4.3, §22-§32: `$ref` con reglas hermanas es (a) pero la Fase 4 lo compila como producto (§22, CR1-S6), el frontal XSD debe pasar por `corpus_clasificado` (§22), la tabla escrita de `clasificacion.md` la vigila una prueba y no `make docs-check` (§22), un solo caso `valid` JSON sin (c) (§22, T-4); carrera residual entre `fs::metadata` y `open` en `leer` (4.3-L1, §23; arreglo `O_NONBLOCK`, tarea 5.4, que es la próxima de la CLI); JSON de la IR con `<`, `>`, U+2028/2029 sin escapar, la web debe tratarlo como no confiable (4.3-I1, §15); temporales de prueba con nombre predecible (4.3-I2, §24); `/proc/self/mem` (4.3-I3, §25); columna «Soporte» de `minOccurs`/`maxOccurs` en el alcance (4.3-S1, §26; aprobada en D29, la aplica la 5.7); «un solo marco» en `clasificacion.md` §4.2 (4.3-S2, §27, Fase 4); pruebas y cosméticos 4.3-S3…S7 (§28-§32). De 4.2 quedan abiertas §1-§15 (las §16-§21 se cerraron en 4.3). Camino de éxito de `schemac ir` con JSON v2: salida ≈ 93× y memoria ≈ 158× la entrada (peor caso con 16 MiB: ≈ 1,5 GiB de salida y ≈ 2,5 GiB de memoria); la Fase 7 necesita `RLIMIT_AS` y además salida en streaming o tope de bytes de salida (§1-§2, requisito 4). Siguen abiertas las de 4.1 (frontal XSD §3, `location_clipped` §4, Fase 4 §5, CI §6) y las de 4.0 (§7).

**Pendientes abiertos tras 4.2** (historia; §16-§21 cerradas en 4.3 y las cifras de memoria sustituidas por las de v2 en «tras 4.3»; el detalle completo, con archivo:línea, arreglo y fase de destino, está en `docs/fase3/pendientes.md` §1-§21, `docs/fase2/pendientes.md` §5-§6 y `docs/fase1/pendientes.md`, que son la fuente de verdad): de 4.2, §16-§21: argumentos `--…` según su posición (4.2-S1), una prueba que imprime el DOT en `cargo test` (4.2-S2), corpus reserializado (4.2-S3, va con 4.3), `use` sin agrupar en `verify.rs` (4.2-S4), abrir una FIFO sin escritor bloquea `schemac` (4.2-L1; requisito Fase 7: el servidor recibe bytes, no rutas; corregir también el comentario de `main.rs:44-46`), `eprintln!` con stderr roto (4.2-I1), y la web debe usar `textContent` (4.2-I2/SA-I1, §15). Camino de éxito de `schemac ir`: salida ≈ 78× y memoria ≈ 143× la entrada, por encima de ρ ≈ 131 (§1; `RLIMIT_AS` en §2, requisito 4). Siguen abiertas las de 4.1 (frontal XSD §3, `location_clipped` §4, Fase 4 §5, CI §6) y las de 4.0 (§7).

**Pendientes abiertos tras 4.1** (historia; todos están ahora en `docs/fase3/pendientes.md` §2-§7, y SA-L1, L-3 y los textos de `ir.md` se cerraron en 4.2): (I-1, frontal XSD) `ir.md` §4 «Obligaciones del frontal XSD» se queda corta: I4, I10 e I11 también los puede romper un XSD del usuario (atributos repetidos, `minOccurs` > `maxOccurs`, faceta repetida o `length` con `minLength`/`maxLength`), además de `min*`/`max*` inclusivo y exclusivo a la vez; ampliar la lista y la regla general (ya anotado en la tarea XSD del cronograma); (I-2, frontal XSD / Fase 4) `ir.md` §7 fila Q y §5.1 comparan los números en decimal exacto, pero `xs:float`/`xs:double` se comparan en IEEE binary32/64 (XSD 1.0 Parte 2 §3.2.4-3.2.5): corregir §5.2, §7 y el pendiente 3 de §11, o documentar la desviación en `00-alcance.md` §3.2; (S-1, Fase 4) `ir.md` §7 fila Γ: la máscara necesita también bits para `DependentRequired` (D5), falta la tabla hash acotada de claves no declaradas (D2, `max_claves_adicionales`) y «registros de las reglas (b)» no es exacto; (S-2, Fase 4) los errores de límite atraviesan `not` sin invertirse y la inversión exige veredicto exacto (`uniqueItems` con hash); (S-3, Fase 4) componentes del producto = conjunto de `NodeId` de E₌, combinadores como circuito (anotado en la semana 3); (S-5, frontal XSD) origen de un tipo anónimo dentro de un `xs:element` global y `xs:simpleContent`+`xs:extension` sobre un tipo complejo de contenido simple; (L-1 y L-2, Fases 4 y 7) tope de estados para todo producto y tope de memoria por compilación (anotados en la semana 3 y en el backend); (L-3) hecho: plan corregido (backend ρ, `ir.md` en «Leer primero» de la semana 3), falta anotar en `docs/fase3/pendientes.md` «Vigilar más adelante» las obligaciones del frontal XSD, `location_clipped` y el tope de producto (con la 4.2); (SA-L1, 4.2) mensajes de `VerifyError` sin escapar ni recortar; (SA-L5, CI) sin bloque `permissions` y acciones por etiqueta, preexistente; (CI) el paso `cargo test -p semantic --release` (`ci.yml:54-56`) no se ha visto correr en GitHub; (4.0, abiertas a propósito) `encode_char` podría devolver `&str` y `Seg::Num` reserva un `String` por índice.

**Pendientes abiertos tras 4.0** (cerrados dentro de 4.1, salvo (e); historia): (a) `docs/fase3/pendientes.md:13-17` y `ciclos.md:111` presentan el «≈ 9×» como medido, pero es una estimación (1 015 B por error ÷ 110 B por ciclo); lo medido directo es 7,2×, y una variante compacta con `not` llega a ≈ 9,8×: documentar una cota (≲ 12× con ruta corta; 11,35× medido — corregido el 26 oct) y citar 9,8× como medida; (b) `ciclos.md:100` atribuye los 60 B por ciclo a la tabla, pero son de `t16k.json` (959 KB), y `ciclos.md:102` dice «reservada una sola vez», cierto solo con `max` ≤ 1024; (c) las salvaguardas de release de `path_in` (`cycles.rs:215-217`, `:229-231`) no tienen prueba: falta una gemela con `#[cfg(not(debug_assertions))]` o anotarlo; (d) `pendientes.md` §2 debe dar como requisitos conjuntos, no alternativos, que cada compilación del servidor vaya en su hilo con `join`/`catch_unwind` y que el perfil nunca use `panic = "abort"` (release no es una medida de seguridad); (e) abiertas a propósito: `encode_char` podría devolver `&str`, y `Seg::Num` reserva un `String` por índice en `emit_head`/`emit_tail`.

Observaciones de la tarea 3 que cerró 4.0 (historia): las BAJAS de la ronda 2 de la tarea 3 (tal cual en `Retomar - semana 2 tarea 3 ronda 2.md`): (1) ninguna prueba impide que `fragment_clipped` parta un escape `%XX`/`~0`/`~1`; (2) constante alta de `fragment_clipped` (~10 µs por fragmento, 98 % del tiempo de la Fase 3 en el peor caso); (3) fórmula de costo de `check_cycles` mal factorizada y la tarea "truncar la lista de errores" sin registrar; (4) `path_in` falla en silencio si se rompe el invariante (añadir `debug_assert!`). Revisión del coordinador (20 oct, sin hallazgos nuevos de corrección): (5) comentarios desactualizados: `crates/semantic/src/symbols.rs:18` dice que los ciclos son «la tarea siguiente» y `crates/semantic/src/error.rs:13-15` no menciona que `check_cycles` también devuelve todos los errores; (6) el estado de D7 sigue diciendo «pendiente» para `schemac`, pero `schemac check` ya analiza en un hilo de 8 MiB (`crates/cli/src/main.rs`, `STACK_SIZE`): solo queda el servidor.

**Estado (21 oct):** los 8 hallazgos están cerrados. Se sigue con las tareas de la semana 2, una por una. Nada de esto está commiteado: revisar `git diff` antes del commit. Queda una nota menor: el comentario de `crates/lexer/src/json.rs:690-693` ("el contador solo retrasa la decisión") no vale cuando la entrada acaba en el prefijo de BOM (ver `lexico.md` §7); se corrige junto con la primera tarea de la semana 2.

Notas abiertas del hallazgo 1 (opcionales): `xml.rs:679` y `xml.rs:696` restan `here.offset - 1` sin saturar (el invariante lo garantiza); `lexico.md` no menciona que `back()` es aproximado en el límite saturado.
