# Pendientes de la Fase 2 (parser de esquemas)

Observaciones que dejaron abiertas las rondas de auditoría del parser LL(1)
de `crates/schema-parser`. Las decisiones de diseño viven en el «Registro de
decisiones» del plan (D1–D31); aquí solo se remite a ellas.

## Resueltos

### 1. Prueba de rendimiento que depende del reloj — RESUELTO (fuera de la CI)

- **Resuelto:** `muchas_claves_en_tiempo_lineal` está marcada `#[ignore = "…"]` y ya no corre en `cargo test` ni en la CI. Decisión del plan: las pruebas que corren en la CI deben ser deterministas, y las mediciones de tiempo van al crate `bench` (criterion), no a `cargo test`. La prueba conserva su método (razón t(4N)/t(N) < 10 con N = 10⁴ en `properties`, `const`, `$defs` y `dependentRequired`, lotes calibrados de ≥ ~50 ms, mínimo de 3 muestras intercaladas y hasta 3 intentos) y se corre a mano, sin carga y en release: `cargo test -p schema-parser --release --test parser -- --ignored muchas_claves_en_tiempo_lineal`. Sin carga detecta la búsqueda cuadrática de forma estable (26,4–27,2 en los tres intentos, release).
- **Parte determinista:** la comprobación de que una clave repetida al final de un objeto de 4·10⁴ claves se reporta en su offset exacto se separó en `clave_repetida_al_final_de_objeto_grande`, que corre siempre.
- **Por qué no corre en la CI (mide tiempo de pared):** con la CPU sobresuscrita (32 `yes` en 8 núcleos) las razones del parser lineal fueron de 1,1 a 15,9 (falsos fallos) y, con la regresión, el auditor midió 8,99 en un intento (falso aprobado). Los reintentos reducen lo primero a costa de lo segundo.
- **Descartado:** medir tiempo de CPU del hilo con `clock_gettime`/`libc` (el workspace tiene `unsafe_code = "deny"` y la CI corre también en Windows). Una comprobación determinista que cuente operaciones (comparaciones de claves) exigiría instrumentar `json_schema.rs` con un contador expuesto solo para pruebas, y aun así una regresión que cambie la estructura de claves podría no actualizar el contador; no se consideró barata.
- **Descartado también:** leer el tiempo de CPU del hilo de `/proc/thread-self/schedstat` (sin `unsafe` ni dependencias). Se probó y se retiró: solo existe en Linux y sigue siendo una medición de tiempo dentro de `cargo test`, contra la decisión del plan (las mediciones de tiempo van a `bench`). Con carga sí era estable (lineal 4,1–5,6; cuadrática 12,6–12,8), por si sirve como referencia para el benchmark.
- **Queda abierto:** sin la prueba en la CI, una regresión cuadrática en la detección de claves repetidas solo se detecta corriendo la prueba ignorada o con el benchmark del crate `bench` (Fase 6, criterion), que todavía no existe. Cuando exista, conviene añadirle objetos de N y 4N claves.
- **Dónde:** `crates/schema-parser/tests/parser.rs`, prueba `muchas_claves_en_tiempo_lineal`.
- **Problema (original):** exigía un tope absoluto de 10 s; en una CI muy cargada
  podía fallar sin que hubiera un error real.
- **Corrección propuesta originalmente:** medir el tiempo con N y con 4N claves y
  comprobar que la razón es menor que 8. La segunda auditoría mostró que una sola
  estimación no aguanta carga alta; de ahí los lotes, el umbral 10 y los reintentos.

### 2. Claves repetidas dentro de `const` y `enum` — DECIDIDO (D2)

- **Decisión (D2 del plan):** las claves repetidas se rechazan en cualquier objeto del
  esquema, incluidos los valores literales de `const`/`enum`, con el error
  `duplicate-key` en la segunda aparición. Es lo que ya hacía el parser; no cambia código.
- **Justificación:** el PDF del curso lista «claves duplicadas» como caso **malformado**,
  y en los payloads el validador generado también las rechaza (`docs/00-alcance.md` §1).
  Que la RFC 8259 solo diga que los nombres *deberían* ser únicos no obliga a aceptarlos:
  el proyecto aplica la misma regla al esquema y al payload. Ver `gramatica-esquema.md`
  §4 y el caso `tests/schemas/parse-error/clave-repetida-en-const.json`.

### 3. «AST común» con XSD — DECIDIDO (D3)

- **Decisión (D3 del plan):** cada lenguaje tiene su propio AST (JSON Schema y XSD) y los
  dos bajan a una IR común. El parser XSD no reutiliza el AST de JSON Schema: la Fase 3
  (meta-esquema, `$ref`/`$defs`) es específica de JSON Schema, y XSD tiene orden de
  `sequence`, atributos frente a elementos y `minOccurs`/`maxOccurs`.
- **Hecho:** la documentación de `crates/schema-parser/src/lib.rs` y `docs/00-alcance.md`
  §3 y §5 ya no hablan de un «AST común». Queda acordar con la Persona B la forma de la
  IR antes de empezar el parser XSD (D3).

### 4. Revisar el manifiesto de errores — HECHO

- **Dónde:** `tests/schemas/errores.json`.
- **Revisión:** se verificaron a mano las 28 entradas (todas), con un script propio que
  no usa el parser: decodifica cada archivo de `parse-error/`, quita el BOM si lo hay,
  cuenta líneas por `\n` y columnas en puntos de código, y marca con `^` la posición
  declarada para comprobar a ojo que cae en el token que provoca el error (la segunda
  clave repetida, el `{` número 257 de `profundidad.json` en la columna 7·256 + 1 = 1793,
  el fin de archivo en `vacio`, `fin-inesperado` y `cadena-sin-cerrar`, etc.). Ninguna
  estaba mal; el manifiesto no cambió.

### 7. Palabras clave fuera del subconjunto — RESUELTO (Fase 3, D6)

- **Antes:** el parser las acepta y las deja en el AST como `Kw::Unsupported`, y nada
  las rechazaba: un esquema con `if`/`then` pasaba sin error.
- **Resuelto:** el parser sigue conservándolas (así puede informar todas, no solo la
  primera), y la validación contra el meta-esquema de la Fase 3
  (`semantic::check_meta_schema`) rechaza cada una con el código `unsupported`, en la
  posición de su nombre, con su categoría y el motivo (D6). No hay modo que las ignore.
  Ver [../fase3/meta-esquema.md](../fase3/meta-esquema.md) §5.
- **Corpus:** `tests/schemas/parse-ok/fuera-del-subconjunto.json` sigue en `parse-ok`
  (prueba el parser) y la Fase 3 lo rechaza con tres errores fijados en
  `crates/semantic/tests/corpus.rs`.

### 15. Clave de la tabla de §2 de `lector-xsd.md` — RESUELTO (5.1-CR3-S3)

- **Antes:** la tabla de §2 de [lector-xsd.md](lector-xsd.md) decía «Conjunto *hash* de
  (URI, local)» para los atributos repetidos por nombre expandido.
- **Resuelto (cierre de la 5.1):** la clave es `(NsId, local)`, la identidad internada del
  URI, no su texto; la tabla ya lo dice.

## Vigilar más adelante

### 5. Margen de pila en debug

- **Estado:** con profundidad 256, el parser usa unos 1,34 MiB de los 2 MiB de pila de
  un hilo en debug (unos 259 KiB en release). La prueba `profundidad_maxima` lo vigila.
- **Cuándo actuar:** si esa prueba empieza a fallar, o si la Fase 3 añade marcos o ramas
  a la recursión. (La validación contra el meta-esquema de la Fase 3 no añade nada: recorre
  el AST con una pila explícita, sin recursión; `crates/semantic/tests/meta.rs`,
  `profundidad_maxima` y `ast_muy_profundo_no_desborda`.) La solución robusta es una pila explícita o `stacker`, que sería una
  dependencia nueva.
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

### 10. Normalización de fines de línea y de atributos en los payloads XML (Fase 4)

- **Estado:** el lexer XML no normaliza (`docs/fase1/lexico.md` §7). El lector de esquemas lo
  hace sobre los eventos, sin búfer: un carácter cuyo rango mide un byte es literal (se
  normaliza) y uno de rango mayor viene de una referencia (se conserva), y un `\n` literal en el
  offset siguiente a un `\r` literal es el resto de un `\r\n`.
- **Cuándo actuar:** en el validador de payloads (Fase 4, autómata de texto y atributos): debe
  aplicar la misma regla antes de `pattern`, `length` o la comparación de enumeraciones, o
  `"a\r\nb"` y `"a\nb"` darán resultados distintos. La técnica no necesita memoria dinámica:
  un registro con el offset del último `\r`.

### 11. Resolución de los QName de valores en el parser XSD (tarea 5.2) — RESUELTO EN EL LECTOR; queda usarlo

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

### 12. El lector de esquemas no tiene límites propios (tarea 5.2 y Fase 7; 5.1-SEC3-L2 = SEC-M1)

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
  - **Tarea 5.2:** el parser debe cortar en cuanto la profundidad pase su tope (D7,
    `MAX_DEPTH_LIMIT`), sin esperar al final del documento: así la pila de elementos abiertos
    del lector no pasa de ese tope.
  - **Fase 7 (servidor):** los esquemas vienen de terceros. Son requisito del servidor los
    límites de atributos por etiqueta, de declaraciones de espacio de nombres y de tamaño de
    subida, o en su lugar `RLIMIT_AS` por compilación más el tope de subida, junto con lo de
    [../fase3/pendientes.md](../fase3/pendientes.md) §1-§2. El factor ρ (memoria / tamaño
    del esquema) de la compilación XSD completa lo medirá la tarea 5.4 y debe incluir estos
    peores casos del lector.
  - Si se añaden límites al lector, que pasarlos sea un error `limit` con posición
    (decisión B2), nunca un pánico.

### 13. Recorte de los textos de error garantizado por el tipo (tarea 5.2; 5.1-CR3-S1 y 5.1-CR3-S2)

- **5.1-CR3-S1** (`crates/xsd/src/reader.rs:538-554` frente a `crates/xsd/src/error.rs:20-25`):
  el recorte de los textos de un error a 33 caracteres no lo garantiza el tipo; solo lo
  aplican `err` y `keep`, privadas de `reader.rs`, y los módulos de la 5.2 en adelante
  construirán `XsdError { kind, at }` directamente. **Arreglo (tarea 5.2):** un
  `pub(crate) fn XsdError::new(kind, at)` en `error.rs` que llame a `ErrorKind::clipped()`,
  usado por `err`/`keep` y documentado como único constructor.
- **5.1-CR3-S2** (= SEC3-L1, observación del Tester 4; `reader.rs:688, 702, 707, 754, 774,
  784, 901`): `keep` y `UnclosedElement` reciben el nombre entero clonado
  (`self.tag_name.clone()`, `a.name.clone()`, `o.name.raw.clone()`) y lo recortan después: una
  copia transitoria O(|nombre|), hasta 16 MiB, aunque `keep` descarte el error. La
  documentación de `ErrorKind` sugiere que nunca se copia un nombre largo. **Arreglo (tarea
  5.2, junto con CR3-S1):** construir los errores con `clip(&nombre)`, o aclarar la
  documentación de `ErrorKind`.

### 14. Detalles del lector sin tarea fija (5.1-T4-1 y 5.1-CR2-S3)

- **5.1-T4-1** (`crates/xsd/src/reader.rs:819`, `saturating_add`, y `span_len`): no hay prueba
  directa de la saturación ni de `span_len`; el desbordamiento es inalcanzable con entradas
  reales (offsets `u64`). Abierta a propósito. **Cuándo:** si se toca el lector.
- **5.1-CR2-S3** (`crates/xsd/src/reader.rs`: `check_xml_decl(data, first, fin)`,
  `problema`, `bad`…): identificadores mezclados de inglés y español, frente al inglés de
  `crates/lexer` y `crates/schema-parser`. Cosmético. **Cuándo:** la próxima tarea que toque
  `reader.rs`.
