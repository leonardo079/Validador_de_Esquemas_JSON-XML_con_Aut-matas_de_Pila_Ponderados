# Pendientes de la Fase 2 (parser de esquemas)

Lo cerrado está archivado en [historial.md](historial.md) (3 nov 2026).

Observaciones que dejaron abiertas las rondas de auditoría del parser LL(1)
de `crates/schema-parser`. Las decisiones de diseño viven en el «Registro de
decisiones» del plan (D1–D31); aquí solo se remite a ellas.

## Resueltos

### 1. Prueba de rendimiento que depende del reloj — RESUELTO (fuera de la CI)

- **Resuelto:** `muchas_claves_en_tiempo_lineal` está marcada `#[ignore = "…"]` y ya no corre en `cargo test` ni en la CI. Decisión del plan: las pruebas que corren en la CI (local, D32: `make test`) deben ser deterministas, y las mediciones de tiempo van al crate `bench` (criterion), no a `cargo test`. La prueba conserva su método (razón t(4N)/t(N) < 10 con N = 10⁴ en `properties`, `const`, `$defs` y `dependentRequired`, lotes calibrados de ≥ ~50 ms, mínimo de 3 muestras intercaladas y hasta 3 intentos) y se corre a mano, sin carga y en release: `cargo test -p schema-parser --release --test parser -- --ignored muchas_claves_en_tiempo_lineal`. Sin carga detecta la búsqueda cuadrática de forma estable (26,4–27,2 en los tres intentos, release).
- **Parte determinista:** la comprobación de que una clave repetida al final de un objeto de 4·10⁴ claves se reporta en su offset exacto se separó en `clave_repetida_al_final_de_objeto_grande`, que corre siempre.
- **Por qué no corre en la CI (mide tiempo de pared):** con la CPU sobresuscrita (32 `yes` en 8 núcleos) las razones del parser lineal fueron de 1,1 a 15,9 (falsos fallos) y, con la regresión, el auditor midió 8,99 en un intento (falso aprobado). Los reintentos reducen lo primero a costa de lo segundo.
- **Descartado:** medir tiempo de CPU del hilo con `clock_gettime`/`libc` (el workspace tiene `unsafe_code = "deny"` y la CI corría también en Windows; desde D32 no hay CI y la verificación es local). Una comprobación determinista que cuente operaciones (comparaciones de claves) exigiría instrumentar `json_schema.rs` con un contador expuesto solo para pruebas, y aun así una regresión que cambie la estructura de claves podría no actualizar el contador; no se consideró barata.
- **Descartado también:** leer el tiempo de CPU del hilo de `/proc/thread-self/schedstat` (sin `unsafe` ni dependencias). Se probó y se retiró: solo existe en Linux y sigue siendo una medición de tiempo dentro de `cargo test`, contra la decisión del plan (las mediciones de tiempo van a `bench`). Con carga sí era estable (lineal 4,1–5,6; cuadrática 12,6–12,8), por si sirve como referencia para el benchmark.
- **Queda abierto:** sin la prueba en la CI, una regresión cuadrática en la detección de claves repetidas solo se detecta corriendo la prueba ignorada o con el benchmark del crate `bench` (Fase 6, criterion), que todavía no existe. Cuando exista, conviene añadirle objetos de N y 4N claves.
- **Dónde:** `crates/schema-parser/tests/parser.rs`, prueba `muchas_claves_en_tiempo_lineal`.
- **Problema (original):** exigía un tope absoluto de 10 s; en una CI muy cargada
  podía fallar sin que hubiera un error real.
- **Corrección propuesta originalmente:** medir el tiempo con N y con 4N claves y
  comprobar que la razón es menor que 8. La segunda auditoría mostró que una sola
  estimación no aguanta carga alta; de ahí los lotes, el umbral 10 y los reintentos.

### 2. Claves repetidas dentro de `const` y `enum` — DECIDIDO (D2) → [historial.md](historial.md) §2

### 3. «AST común» con XSD — DECIDIDO (D3)

- **Decisión (D3 del plan):** cada lenguaje tiene su propio AST (JSON Schema y XSD) y los
  dos bajan a una IR común. El parser XSD no reutiliza el AST de JSON Schema: la Fase 3
  (meta-esquema, `$ref`/`$defs`) es específica de JSON Schema, y XSD tiene orden de
  `sequence`, atributos frente a elementos y `minOccurs`/`maxOccurs`.
- **Hecho:** la documentación de `crates/schema-parser/src/lib.rs` y `docs/00-alcance.md`
  §3 y §5 ya no hablan de un «AST común». Queda acordar con la Persona B la forma de la
  IR antes de empezar el parser XSD (D3).

### 4. Revisar el manifiesto de errores — HECHO → [historial.md](historial.md) §4

### 7. Palabras clave fuera del subconjunto — RESUELTO (Fase 3, D6) → [historial.md](historial.md) §7

### 15. Clave de la tabla de §2 de `lector-xsd.md` — RESUELTO (5.1-CR3-S3) → [historial.md](historial.md) §15

## Vigilar más adelante

### 5. Margen de pila en debug

- **Estado:** con profundidad 256, el parser usa unos 1,34 MiB de los 2 MiB de pila de
  un hilo en debug (unos 259 KiB en release). La prueba `profundidad_maxima` lo vigila.
- **Cuándo actuar:** si esa prueba empieza a fallar, o si la Fase 3 añade marcos o ramas
  a la recursión. (La validación contra el meta-esquema de la Fase 3 no añade nada: recorre
  el AST con una pila explícita, sin recursión; `crates/semantic/tests/meta.rs`,
  `profundidad_maxima` y `ast_muy_profundo_no_desborda`.) La solución robusta es una pila explícita o `stacker`, que sería una
  dependencia nueva.
- **Parser XSD (tarea 5.2):** el parser del subconjunto XSD (`crates/xsd`) también es
  recursivo, con el mismo tope (256, solo a la baja) que corta el lector en el `<` del
  elemento que lo pasa. Medido en `dev`: ~1015 KiB a profundidad 256 en el peor caso
  (cadena `xs:element`/`xs:complexType`/`xs:sequence`, ~3,9 KiB por nivel), ~1 MiB de
  margen en un hilo de 2 MiB; 371 KiB en release ([gramatica-xsd.md](gramatica-xsd.md) §8).
  Lo vigila `profundidad_maxima_en_un_hilo_de_2_mib` (`crates/xsd/tests/parser.rs`). **Cuándo
  actuar:** si esa prueba falla o si la 5.3 añade recursión sobre el AST (la 5.3 debe
  recorrerlo con pila explícita, D7).
- **Análisis y bajada XSD (tarea 5.3) — RESUELTO:** `xsd::analyze` y `xsd::lower` no recurren
  sobre el AST: una pila de tipos pendientes da el preorden de las declaraciones y otra aplana cada
  modelo de contenido en una arena ([../fase3/xsd.md](../fase3/xsd.md) §4 y §6). Lo vigila
  `analisis_y_bajada_sin_recursion` (`crates/xsd/tests/lower.rs`: la cadena más profunda se analiza
  y se baja en un hilo de 64 KiB). Las pruebas de la 5.3 que comparan IR (no AST) no necesitan el
  hilo; las que construyen el AST profundo lo liberan en un hilo de 2 MiB.
- **Recursión del AST (5.2-SEC-L2, 5.2-CR-S1):** los `Clone`, `Debug` y `PartialEq` derivados y el
  `Drop` del AST XSD son recursivos, acotados por la profundidad (256). Todo consumidor recursivo
  (5.3, 5.4) y cualquier `clone()`, `{:?}` o `==` de un `xsd::ast::Schema` debe correr en el hilo
  con `stack_size` ≥ 2 MiB (D7). El `==` del AST compara los `Name` por contenido y los `NsId` por
  identidad, así que solo tiene sentido dentro de un mismo análisis (documentado en `Schema`).
- **Tope configurable:** `parse_with_max_depth` recorta la profundidad pedida a
  `MAX_DEPTH_LIMIT` = 256 (igual al valor por defecto), así que ningún valor del llamador
  puede llevar la recursión más allá del margen medido. Subir ese tope exige antes la pila
  explícita; ver `gramatica-esquema.md` §6.
- **Hilos con menos de 2 MiB:** la garantía solo vale en hilos con ≥ 2 MiB de pila. El
  hilo principal de Windows tiene 1 MiB, y ahí `parse(anidado(256))` en `dev` desborda
  (simulado en Linux con un hilo de 1 MiB; en release pasa). `schemac check` (Fase 3) ya
  cumple la regla: llama al parser y a la Fase 3 en un hilo con 8 MiB de pila explícita
  (`crates/cli/src/main.rs`, `STACK_SIZE`). El servidor aún no llama al parser, y libtest
  corre cada prueba en un hilo propio de 2 MiB. **Cuando el servidor (Fase 7) integre el
  parser, debe llamarlo dentro de un hilo creado con `std::thread::Builder::new().stack_size(…)` de al
  menos 2 MiB (o usar la pila explícita), nunca directamente en el hilo principal ni en un
  hilo de tamaño desconocido** (regla D7 del plan).

### 6. Límite de tamaño del esquema

- **Estado:** cada clave se copia a la tabla de claves vistas, y el parser no tiene límite
  de tamaño del esquema ni de claves por objeto. **`schemac` sí tiene un tope** desde la
  tarea 4.2: lee como mucho **16 MiB** y solo archivos regulares; pasarlo es un error de
  entrada con código 2 ([00-alcance.md](../00-alcance.md) §4,
  [fase3/bajada.md](../fase3/bajada.md) §5).
- **Cuándo actuar:** el tope del **servidor** (tamaño máximo de subida) sigue pendiente
  para la aplicación web (Fase 7). Va en el servidor, no en el parser, y se fija junto con
  el `RLIMIT_AS` de cada compilación ([fase3/pendientes.md](../fase3/pendientes.md) §1 y
  §2: la compilación de JSON Schema hasta la IR exportada (JSON v2) usa ≈ 158 veces el
  tamaño del esquema; la cifra v1 era ≈ 143). El frontal XSD suma su lector, con una
  amplificación de memoria de 29× a 33× de la entrada en los peores casos (§12).

### 8. CDATA de solo espacios fuera del elemento raíz (tarea 5.1; 5.1-SEC2-I1)

- **Estado:** el lector de esquemas (`crates/xsd`, [lector-xsd.md](lector-xsd.md) §8) acepta
  `<![CDATA[ ]]>` antes o después de la raíz si su contenido son solo espacios, aunque XML 1.0
  no lo admite en `Misc`: el lexer entrega su contenido como `TextChar` de un byte, igual que el
  texto literal. Con cualquier otro contenido sí es `TextOutsideRoot`.
- **Cuándo actuar:** si se quiere el rechazo exacto, hace falta que el lexer distinga el texto
  de una sección CDATA (un evento de apertura de CDATA, por ejemplo); toca la tabla y las
  cifras de `docs/fase1/lexico.md`. Afecta igual al validador de payloads (Fase 4). Riesgo
  bajo: el documento sigue siendo inequívoco.

### 9. Normalización de los valores según su tipo XSD (tareas 5.5 y 5.6)

- **Estado:** el lector entrega los valores de atributo y el texto con la normalización de
  XML 1.0 (fines de línea §2.11 y atributos CDATA §3.3.3), no la del tipo XSD. Los atributos
  del esquema con tipo de `whiteSpace` *collapse* (`minOccurs`, `maxOccurs`, `use`, `name`,
  los valores de facetas numéricas, `xs:boolean`…) llegan con sus espacios de los extremos.
  `Attr::as_qname()` ya quita los espacios de los extremos del QName (*collapse*).
  El valor de un `Attr` es de solo lectura (`Attr::value()`): la 5.2 aplica *collapse* sobre
  una copia (o con `into_value()`), nunca modificando el `Attr`, porque `as_qname()` guarda
  rangos del valor original.
- **Cuándo actuar:** al leer cada atributo del esquema (5.2) o al interpretar su valor (5.5,
  5.6): aplicar *collapse* según el tipo del atributo en el esquema de esquemas, y *replace*/
  *collapse* a `xs:enumeration`, `default` y `fixed` según el tipo simple base.
- **Hecho en la 5.2 (la parte del esquema de esquemas):** el parser aplica *collapse* sobre una
  copia (`Attr::value()`) a `name`, `minOccurs`, `maxOccurs`, `use`, `mixed`,
  `elementFormDefault`, `targetNamespace` y las facetas de rango y de longitud, y guarda
  `default`, `fixed`, `xs:enumeration` y `xs:pattern` sin tocar (`into_value()`); nunca modifica
  el `Attr` ([gramatica-xsd.md](gramatica-xsd.md) §6). **Queda (5.5 y 5.6):** el *whiteSpace*
  de `default`, `fixed` y `xs:enumeration` según el tipo simple base.

### 10. Normalización de fines de línea y de atributos en los payloads XML (Fase 4)

- **Estado:** el lexer XML no normaliza (`docs/fase1/lexico.md` §7). El lector de esquemas lo
  hace sobre los eventos, sin búfer: un carácter cuyo rango mide un byte es literal (se
  normaliza) y uno de rango mayor viene de una referencia (se conserva), y un `\n` literal en el
  offset siguiente a un `\r` literal es el resto de un `\r\n`.
- **Cuándo actuar:** en el validador de payloads (Fase 4, autómata de texto y atributos): debe
  aplicar la misma regla antes de `pattern`, `length` o la comparación de enumeraciones, o
  `"a\r\nb"` y `"a\nb"` darán resultados distintos. La técnica no necesita memoria dinámica:
  un registro con el offset del último `\r`.

### 11. Resolución de los QName de valores en el parser XSD (tarea 5.2) — RESUELTO (lector en la 5.1, parser en la 5.2, tablas de la 5.3)

- **Resuelto en la 5.3** ([../fase3/xsd.md](../fase3/xsd.md) §2 y §6): las tres tablas de símbolos
  (`crates/xsd/src/symbols.rs`) se indexan por `(Option<&NsId>, &str)`, es decir
  `(Name::ns_id(), Name::local())` en las referencias y `(Schema::target_namespace, name)` en las
  declaraciones, con el `NsId` que guardó la 5.2; nunca por el texto del URI ni por `expanded()`;
  `HashMap` con el *hasher* aleatorio de `std` (`RandomState`), sin *hasher* determinista. Prueba de
  regresión sin reloj (D1): `uri_de_8_mib_y_medio_millon_de_referencias` (URI de 8 MiB como
  `targetNamespace` y prefijo de 5·10⁵ referencias `type='p:T'`; comprueba que la IR tiene 3 nodos
  y 1 + 5·10⁵ aristas). Lo que sigue queda como historial.

- **Resuelto en la 5.2** ([gramatica-xsd.md](gramatica-xsd.md) §6): el parser llama a
  `as_qname()` una sola vez por atributo `type`/`base`/`ref` y guarda el `Name` resuelto con el
  rango de su valor (`Spanned<Name>`, `at()` = primer carácter); recorta (*collapse*) el
  `targetNamespace`, lo interna con `Reader::intern` mientras el lector existe y guarda su
  `NsId` en `Schema::target_namespace`; `intern` no se llama para ningún otro valor; la
  documentación de `Reader::intern` dice que interna el valor tal como se le pasa, ya
  normalizado. Pruebas: `target_namespace_recortado_da_el_mismo_ns_id` (`' urn:t '` y
  `type='t:T'` dan el mismo `NsId`) y `qname_resuelto_con_el_ambito_de_su_etiqueta` (un hijo
  que redeclara el prefijo), en `crates/xsd/tests/parser.rs`. Lo que sigue es el historial; las
  obligaciones de la 5.3 (tablas por `(ns_id(), local())`, prueba de regresión con un URI
  largo, *hasher* aleatorio) siguen abiertas.

- **Antes (ronda 1):** `Reader::resolve_qname` solo valía mientras se consumían los tokens de
  la etiqueta del atributo. Un parser que lee por adelantado (como el de JSON Schema, cuyo
  `bump()` pide el token siguiente al consumir uno) habría resuelto `type="p:T"` con las
  declaraciones de un hijo (`<xs:complexType xmlns:p="otro">`), sin error.
- **Resuelto (ronda 2 de la 5.1):** el lector resuelve todo valor de atributo como QName al
  cerrar su etiqueta y lo guarda en el `Attr`; `Attr::as_qname()` devuelve el resultado, sea
  cual sea el momento en que se pida ([lector-xsd.md](lector-xsd.md) §3.3). `resolve_qname`
  ya no existe. Prueba: `qname_resuelto_tras_leer_por_adelantado_un_hijo_que_redeclara`.
- **Queda (5.2 y 5.3):** el parser debe llamar a `as_qname()` en los valores de `type`, `base`
  y `ref` y guardar en el AST el `Name` con la posición del error (primer carácter del valor).
- **Obligación de la 5.3 (no repetir el error cuadrático de la ronda 1):** las tablas de
  símbolos se indexan por **(identidad del URI, nombre local)**, es decir
  `(Name::ns_id(), Name::local())`, nunca por el texto del URI (`Name::expanded()` o una
  `String`): con un URI de ~8 MiB y ~5·10⁵ referencias `type="p:T"`, hashear el URI en cada
  consulta es O(referencias · |URI|), alcanzable desde la Fase 7. El `NsId` solo es comparable
  dentro de un mismo `Reader` ([lector-xsd.md](lector-xsd.md) §3). Hace falta una prueba de
  regresión con un URI largo y muchas referencias, sin reloj (D1).
- **5.1-CR3-I1 — `targetNamespace` lo interna la 5.2, no la 5.3** (`docs/fase2/pendientes.md`
  §11 y `lector-xsd.md` §3, ya corregidos; `crates/xsd/src/reader.rs:1143-1147`,
  `Reader::intern`). `Reader::intern` necesita el lector vivo (`&mut self`) y en la 5.3 ya no
  existe. Además `targetNamespace` es `xs:anyURI` (*collapse*) y los URI de `xmlns:p` no se
  recortan: con `xmlns:t='urn:t' targetNamespace=' urn:t '`, `r.intern(value())` da un `NsId`
  distinto del de `type='t:T'`. **Arreglo (tarea 5.2):** el parser recorta (*collapse*) el
  `targetNamespace`, lo interna con `Reader::intern` mientras el lector existe y guarda el
  `Option<NsId>` en la raíz del AST; la 5.3 lo usa con `(ns_id(), local())`. En la 5.2, además,
  corregir la documentación de `Reader::intern` (`reader.rs:1143-1146`) para que diga que
  interna el valor **ya normalizado**. Alternativa en código: `Reader::into_namespaces()`.
- **5.1-SEC3-I1 — `NsId` hashea por dirección** (`reader.rs`, `impl Hash for NsId`). Es
  correcto con `RandomState` (la tabla de `std`). **Tarea 5.3:** prohibido usar `NsId` como
  clave en tablas con un *hasher* determinista (Fx, ahash con semilla fija): las direcciones
  de las reservas no son aleatorias y la distribución quedaría a merced del asignador.
- **5.1-SEC3-I2 — `Reader::intern` retiene los URI hasta el fin del lector, sin límite.**
  **Tarea 5.2:** llamarlo solo para `targetNamespace`, nunca por cada valor de atributo (un
  esquema hostil haría crecer la tabla con cada valor distinto).
- **5.1-SEC3-I3 — `as_qname` copia el QName** (O(|valor|) por llamada en el caso resuelto).
  **Tarea 5.2:** llamarlo una sola vez por atributo y guardar el `Name`; llamadas repetidas
  sobre un valor de 16 MiB serían cuadráticas.

### 12. El frontal XSD no tiene límites de memoria propios (Fase 7 y 5.7; 5.1-SEC3-L2, 5.2-SEC-M1 y 5.2-SEC-M2) — resuelta solo la profundidad

- **Seguimiento:** la auditoría final de la 5.1 lo dejó abierto como **5.1-SEC3-L2** (igual
  que SEC-M1 de la ronda 2): amplificación de memoria de 29× a 33× sin tope en
  `crates/xsd/src/reader.rs` (`Assembler::attrs`, la cola de tokens, `Scope::map`/`undo`).
  Arreglo: límites en el lector o en la 5.2 (profundidad, atributos por etiqueta,
  declaraciones), y en la Fase 7 tope de concurrencia de compilaciones o `RLIMIT_AS` por
  compilación, junto con el tope de subida.

- **Estado:** el lector (`crates/xsd`) es lineal en tiempo y memoria, pero con una constante
  grande. Medido con 16 MiB (el tope de `schemac`), la memoria de pico llega a unas 30 veces la
  entrada: una etiqueta con 1,4 M atributos (31×) o con 1,2 M atributos con prefijo (32×),
  5,6 M niveles sin cerrar (29×), 0,7 M declaraciones `xmlns:pK` (33×). Detalle en [lector-xsd.md](lector-xsd.md) §5, «Memoria». No
  tiene `limit` ni ningún tope configurable.
- **Cuándo actuar:**
  - **Tarea 5.2 — RESUELTO solo para la profundidad:** el parser fija el tope en el lector (`Reader::with_max_depth`,
    `MAX_DEPTH_LIMIT` = 256), que rechaza el elemento que lo pasa en su `<` (error `limit`) y
    no lee más: la pila de elementos abiertos del lector no pasa de 256. Prueba
    `corte_por_profundidad_sin_leer_el_resto` (10⁶ niveles, cuenta los bloques pedidos y
    comprueba que el lector se detiene con 256 abiertos); [gramatica-xsd.md](gramatica-xsd.md)
    §8.
  - **Fase 7 (servidor):** los esquemas vienen de terceros. Son requisito del servidor los
    límites de atributos por etiqueta, de declaraciones de espacio de nombres y de tamaño de
    subida, o en su lugar `RLIMIT_AS` por compilación más el tope de subida, junto con lo de
    [../fase3/pendientes.md](../fase3/pendientes.md) §1-§2. El factor ρ (memoria / tamaño
    del esquema) de la compilación XSD completa lo medirá la tarea 5.4 y debe incluir estos
    peores casos del lector. **Medido en la tarea 5.4** ([../fase3/pendientes.md](../fase3/pendientes.md)
    §1, «Camino de éxito», tabla XSD): con 16 MiB, `schemac check`/`ir` llega a ≈ 59× con
    `<choice/>` repetido, ≈ 54× con `<element/>`, ≈ 45× con `xmlns:pK` y ≈ 32× con una
    etiqueta de atributos; todo por debajo de los ≈ 158× de JSON Schema, que siguen
    dimensionando `RLIMIT_AS`. Los límites propios del lector siguen pendientes (Fase 7, 5.7).
  - Si se añaden límites al lector, que pasarlos sea un error `limit` con posición
    (decisión B2), nunca un pánico.
- **Lo que añade la 5.2 (auditoría, 5.2-SEC-M1 y 5.2-SEC-M2; decisión del orquestador:
  documentar, no limitar en la 5.2).** Pico medido en release con entradas de 16 MiB
  ([gramatica-xsd.md](gramatica-xsd.md) §8):
  - **AST sin tope (SEC-M1), hasta ≈ 34×:** `Element`/`Group` ocupan ≈ 300-400 B cada uno;
    1,5 M `<choice/>` llegan a 601 MiB, 1,67 M `<element/>` a 543 MiB, 1,5 M `<sequence/>` a
    495 MiB, 508 k globales a 208 MiB, facetas a 74-90 MiB. El tiempo es lineal. D26 (4 096
    partículas) se comprueba en la 5.7, después de construir el AST.
  - **Atributos por etiqueta sin tope (SEC-M2), ≈ 40×:** `<schema` con 1,9 M atributos llega a
    ≈ 673 MiB, y el primer error sale solo después de que el lector arma la etiqueta entera
    (`Assembler::attrs`, `close_start_tag`). **D34 no cambia el pico** (la etiqueta se arma entera antes de
    que el parser vea el primer atributo); solo hace que esa entrada sea válida si los atributos
    son de otro espacio de nombres (1,28 M con prefijo, válidos: 522 MiB ≈ 32×; 1,49 M sin
    prefijo, error en el primero: 510 MiB ≈ 31×).
  - **Arreglos propuestos:**
    - tope de atributos por etiqueta en el lector (≈ 1 024), con error `limit` en el atributo
      N + 1, y tope de declaraciones `xmlns` vivas: **Fase 7**, o la próxima tarea que toque el
      lector;
    - tope de nodos del AST, o de hijos por grupo, coherente con D26: **tarea 5.7**;
    - `RLIMIT_AS` por compilación y tope de concurrencia de compilaciones: **Fase 7**.

### 13. Recorte de los textos de error garantizado por el tipo (tarea 5.2; 5.1-CR3-S1 y 5.1-CR3-S2) — RESUELTO → [historial.md](historial.md) §13

### 14. Detalles del lector sin tarea fija (5.1-T4-1 y 5.1-CR2-S3)

- **5.1-T4-1** (`crates/xsd/src/reader.rs:819`, `saturating_add`, y `span_len`): no hay prueba
  directa de la saturación ni de `span_len`; el desbordamiento es inalcanzable con entradas
  reales (offsets `u64`). Abierta a propósito. **Cuándo:** si se toca el lector.
- **5.1-CR2-S3** (`crates/xsd/src/reader.rs`: `check_xml_decl(data, first, fin)`,
  `problema`, `bad`…): identificadores mezclados de inglés y español, frente al inglés de
  `crates/lexer` y `crates/schema-parser`. Cosmético. **Cuándo:** la próxima tarea que toque
  `reader.rs`. **RESUELTO en la 5.2:** `fin` → `end`, `problema` → `problem`; los
  identificadores de `reader.rs` quedan en inglés (los comentarios siguen en español, como en
  el resto del proyecto).

### 16. Restricciones del esquema XSD que el parser deja a la 5.3 (tarea 5.2) — RESUELTO (tarea 5.3), salvo lo de `xs:restriction` (5.6)

- **Resuelto en la 5.3** ([../fase3/xsd.md](../fase3/xsd.md) §3, corpus
  `tests/schemas/xsd/semantic-error` con su manifiesto `errores-semanticos-xsd.json`): `name` o
  `ref` en un `xs:element` local, `type` o tipo anónimo, D14, `name` como NCName, `minOccurs` y
  `maxOccurs` como entero no negativo o `unbounded`, `minOccurs` ≤ `maxOccurs`, las ocurrencias de
  `xs:all` y de sus elementos, `default` con `fixed`, `default` con `use="required"`, nombres
  globales repetidos, referencias, los tipos de fecha y hora (al resolver) y los `xs:attribute`
  globales (5.2-CR-S2: se comprueban y no se bajan, lo coherente con XSD). **Sigue abierto, tarea
  5.6:** `base` o `xs:simpleType` (exactamente uno) en `xs:restriction`; mientras, la 5.3 rechaza
  todo `xs:simpleType` con «pendiente (tarea 5.6)».

- **Estado:** el parser de la 5.2 aplica la gramática, la lista blanca de atributos, los
  obligatorios simples y los valores que necesita ([gramatica-xsd.md](gramatica-xsd.md) §4),
  pero deja en el AST, sin comprobar: `name` o `ref` (exactamente uno) en un `xs:element`
  local; `type` o tipo anónimo, no los dos; `xs:element` sin tipo (D14); `base` o
  `xs:simpleType` (exactamente uno) en `xs:restriction`; `name` como NCName; `minOccurs` y
  `maxOccurs` como entero no negativo o `unbounded`, y `minOccurs` ≤ `maxOccurs`; las
  ocurrencias de `xs:all` y de sus elementos (0 o 1); `default` y `fixed` a la vez en un
  atributo, y `default` con `use="required"`; nombres globales repetidos; referencias; los
  tipos de fecha y hora de `00-alcance.md` §3.3 (son nombres de tipo, se rechazan al resolver).
- **Cuándo actuar:** tarea 5.3 (y 5.5/5.6 la forma de los valores de faceta), cada una con la
  posición que guarda el AST (`Spanned::at()` para los valores, `span` para las construcciones).
- **Declaraciones globales de `xs:attribute` (5.2-CR-S2):** el parser las acepta (XSD las admite), pero sin
  `ref` en `xs:attribute` (fuera del subconjunto) no se pueden usar. La 5.3 decide si las ignora
  o las rechaza; ignorarlas es lo coherente con XSD.

### 17. `00-alcance.md` §3 no refleja aún las decisiones de la lista blanca (tareas 5.2 y 5.7)

- **Estado:** la lista blanca de la 5.2 ([gramatica-xsd.md](gramatica-xsd.md) §5) concreta cosas
  que el alcance no dice: `id`, `xs:annotation`, `xs:documentation`, `xs:appinfo` y `source`
  se aceptan y se ignoran (D23); `mixed="false"` se acepta; `use="prohibited"`, `version` en
  `xs:schema` y `fixed` en las facetas de rango y de longitud son `unsupported` (en `xs:enumeration` y
  `xs:pattern`, `syntax`: `xs:noFixedFacet`); se aceptan `xs:attribute` globales,
  `minOccurs`/`maxOccurs` en `xs:sequence`, `xs:choice` y `xs:all`, y una base anónima en
  `xs:restriction`.
- **Resuelto por D34 (3 nov, ronda 1 de la 5.2):** los atributos de otro espacio de nombres que el
  de XSD (`xml:lang`, `xsi:*`, extensiones) se aceptan y se ignoran en cualquier construcción y no
  llegan al AST; uno calificado con el propio espacio de XSD (`xs:type="…"`) es `syntax`. Ya no
  hay excepción para `xml:lang` en `xs:documentation`.
- **Asimetría — RESUELTO por D39 (tarea 5.3):** el parser acepta e ignora
  `attributeFormDefault="unqualified"`, `form` igual al vigente, `nillable="false"` y
  `abstract="false"`, y rechaza como `unsupported` (con categoría y motivo, en el valor) los que
  cambian la validación ([gramatica-xsd.md](gramatica-xsd.md) §5, prueba
  `d39_valores_por_defecto_sin_efecto`; los casos de `parse-error` usan el valor que cambia la
  validación). **Sigue abierto:** `00-alcance.md` §3 no lo refleja (tarea 5.7, con D23 y D34).
- **Asimetría para decidir (5.2-CR2-S4; decisión del usuario):** `mixed="false"` se acepta, pero
  `attributeFormDefault="unqualified"`, `form="unqualified"`, `nillable="false"` y
  `abstract="false"` se rechazan (`unsupported`), aunque tampoco tienen efecto: son los valores
  por defecto de XSD. `attributeFormDefault="unqualified"` es muy frecuente en los XSD generados
  por herramientas. **Propuesta:** aceptar e ignorar esos atributos cuando llevan el valor por
  defecto sin efecto (también `elementFormDefault`, que ya se acepta, y `form` igual al
  `elementFormDefault` vigente), y rechazar solo los valores que cambian la validación
  (`attributeFormDefault="qualified"`, `form="qualified"`, `nillable="true"`,
  `abstract="true"`). Queda para que decida el usuario.
- **EDC por nombre local (5.3-CR1-I1; D40):** el análisis de la 5.3 compara *Element Declarations
  Consistent* por nombre **local**, no por {name, target namespace} como XSD 1.0
  (*cos-element-consistent*): con `targetNamespace` y `elementFormDefault` por defecto, un local
  `a` y un `ref` al global `a` de otro tipo en el mismo modelo son válidos en XSD y aquí
  `content-model`. Es necesario porque la IR nombra por nombre local
  ([../fase3/ir.md](../fase3/ir.md) §5.2; `00-alcance.md` §1.2) e I12 lo exige
  ([../fase3/xsd.md](../fase3/xsd.md) §3). La 5.7 la documenta en `00-alcance.md` §3 como
  desviación.
- **Cuándo actuar:** tarea 5.7 (actualizar `00-alcance.md` §3 con D23, D34, D39, esta lista y la
  desviación de EDC por nombre local).

### 18. Posición del error de texto en contenido de solo elementos (tarea 5.2)

- **Estado:** `TextNotAllowed` va en el primer carácter del `TEXT`, que es máximo, así que
  puede ser un espacio o un salto de línea anterior al primer carácter que no lo es
  (`<xs:element/> hola` da la columna del espacio). El lector normaliza los fines de línea, así
  que la posición del primer carácter no blanco no se puede reconstruir avanzando la del token
  sobre el texto entregado.
- **Cuándo actuar:** si la web (Fase 7) necesita señalar el carácter exacto: guardar en el
  lector la posición del primer carácter no blanco de cada `TEXT`. Cosmético.

### 19. Cierre de la 5.2: cobertura y detalles sin tarea propia (revisión final, 3 nov)

Observaciones de la revisión final de la 5.2 (APROBADO CON OBSERVACIONES). El comportamiento es
correcto en todas; lo que falta es cobertura o pulido.

- **5.2-CR3-S1 — RESUELTO (tarea 5.3):** 11 casos nuevos en `parse-error` con su entrada en
  `errores-xsd.json` (`block-en-element`, `final-en-element`, `final-en-tipo-complejo`, `version`,
  `fixed-en-{min-inclusive,min-exclusive,max-exclusive,length,min-length,max-length}` y
  `texto-con-comentario-malo`, con el error en el byte que sigue a `--`, columna 70).
- **5.2-CR3-S1 + menor 1 del Tester 3 (mutaciones vivas en `crates/xsd/src/subset.rs`).** Sin
  prueba: `:284-285` (`block`/`final` en `xs:element`, `final` en `xs:complexType`); `:292-302`
  (`fixed` en `minInclusive`, `minExclusive`, `maxExclusive`, `length`, `minLength` y `maxLength`;
  solo hay un caso, con `maxInclusive`); `:307` (`version` en `xs:schema`). **Arreglo:** 4-5
  entradas en `tests/schemas/errores-xsd.json`, o un bucle sobre las nueve facetas en
  `lista_blanca_y_celdas_aceptadas`. Añadir también `texto-con-comentario-malo.xsd` (`lexical` en
  el segundo `-` del `--`), que fija el tramo de la excepción (2) de
  [gramatica-xsd.md](gramatica-xsd.md) §7 (5.2-CR3-I1). **Destino:** tarea 5.3 (toca el corpus
  XSD).
- **5.2-CR2-S3 — RESUELTO (tarea 5.3):** `esquemas_xml_de_los_casos_parsean` compara cada copia
  byte a byte con el esquema del caso (se quitó el salto de línea final que tenían las copias).
- **5.2-CR2-S3** (`crates/xsd/tests/parser.rs:155-158`): de las copias en `parse-ok` de los
  esquemas de `tests/cases` solo se comprueba que existen. **Arreglo:** compararlas byte a byte con
  el texto del caso. **Destino:** 5.3 o 5.4 (corpus).
- **5.2-N1 — RESUELTO (tarea 5.3):** `InvalidValue` muestra siempre `clip(a.value())`.
- **5.2-N1** (`crates/xsd/src/parser.rs:431-437`): `InvalidValue` de `mixed`/`use`/
  `elementFormDefault` muestra el valor recortado (*collapse*) y el de `targetNamespace`, el valor
  crudo. **Arreglo:** unificar con `clip(a.value())`. Cosmético; **destino:** la próxima tarea que
  toque `parser.rs`.
- **5.2-N2 — RESUELTO (tarea 5.3):** el mensaje de `UnknownAttribute` termina en «(y los de otro
  espacio de nombres, que se ignoran)» en todas las construcciones.
- **5.2-N2** (`crates/xsd/src/subset.rs:229` y el mensaje de `UnknownAttribute`): en
  `xs:documentation` dice «admite: source» sin mencionar que los atributos de otro espacio de
  nombres se aceptan (D34). **Arreglo:** añadir esa mención al mensaje (o a la lista mostrada).
  Cosmético; **destino:** la próxima tarea que toque `subset.rs`/`error.rs`.
- **5.2-N3 — RESUELTO (tarea 5.3):** documentación de `Reader::intern` reescrita.
- **5.2-N3** (`crates/xsd/src/reader.rs:1161-1168`): pulir la redacción de la documentación de
  `Reader::intern` («`uri` debe llegar ya normalizado (*collapse*)…»). Cosmético; **destino:** la
  próxima tarea que toque `reader.rs`.
- **5.2-CR-S4 — RESUELTO (tarea 5.3):** una sola función, `reader::value_start`, que usan
  `Attr::as_qname`, `Spanned::at` y el parser.
- **5.2-CR-S4** (`crates/xsd/src/parser.rs:115-119`): `value_at` repite `Spanned::at`
  (`crates/xsd/src/ast.rs:45-49`) y el cálculo de `as_qname`. **Arreglo:** una sola función.
  **Destino:** la próxima tarea que toque `parser.rs`.
- **5.2-CR-S5** (`crates/xsd/src/subset.rs:47-71`): las categorías de D6 son `&'static str`;
  `semantic` usa `enum Category` con `describe()`. **Arreglo:** un `enum` con un código estable que
  la web pueda leer. **Destino:** Fase 7.
- **5.2-CR-S6 — REVISADO en la 5.3, se mantiene:** que `open` devolviera los obligatorios ya
  extraídos no quita la rama: el tipo de cada valor (`Spanned<String>`, `Spanned<Name>`) lo decide
  el llamador, así que la extracción sigue siendo un `Option`. La alternativa sin rama sería un
  `unwrap`/`unreachable!`, que el proyecto evita en el camino de la entrada; las tres ramas
  devuelven el mismo error que ya informó `open`. Sin cambio; se cierra.
- **5.2-CR-S6** (`crates/xsd/src/parser.rs:708-717`, `818-827` y `909-918`): tres ramas
  defensivas iguales de `MissingAttribute`, inalcanzables. **Arreglo:** que `open` devuelva los
  obligatorios ya extraídos. Nit; **destino:** la próxima tarea que toque `parser.rs`.
- **5.2-CR-S7 — RESUELTO (tarea 5.3):** singular con tope 1 («1 elemento anidado») y
  documentado en `parse_with_max_depth` que con 0 el propio `xs:schema` es `limit` (prueba
  `profundidad_1_en_singular_y_0_rechaza_el_esquema`).
- **5.2-CR-S7:** con `max_depth = 1` el mensaje dice «1 elementos anidados», y
  `parse_with_max_depth(.., 0)` rechaza el propio `xs:schema` sin un aviso propio. **Arreglo:**
  singular en el mensaje y documentar (o recortar a 1) el valor 0. Cosmético; **destino:** la
  próxima tarea que toque `parser.rs`/`error.rs`.
- **5.2-SEC-L3** (`crates/xsd/src/error.rs:20-25`): `XsdError` tiene `kind` y `at` públicos, así que
  «único constructor» (`XsdError::new`, §13) solo se cumple por convención fuera del crate.
  **Arreglo:** campos `pub(crate)` con accesores. **Destino:** Fase 7 (API pública).
- **5.2-CR-S2 — RESUELTO (tarea 5.3):** los `xs:attribute` globales se comprueban (nombre,
  `default`/`fixed`, tipo, nombres únicos) y no se bajan (§16).
- **5.2-CR-S2:** los `xs:attribute` globales se aceptan pero no se pueden usar sin `ref`; ya está en
  §16 («Declaraciones globales de `xs:attribute`»), destino 5.3.
