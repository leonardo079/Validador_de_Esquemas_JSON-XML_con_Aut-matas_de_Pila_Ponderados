# Pendientes de la Fase 3 (análisis semántico del esquema)

Lo cerrado está archivado en [historial.md](historial.md) (3 nov 2026).

Observaciones que dejaron abiertas las rondas de auditoría de `crates/semantic`
y que se resuelven en otra fase. Las decisiones de diseño viven en el
«Registro de decisiones» del plan; aquí solo se remite a ellas.

## Vigilar más adelante

### 1. Truncar la lista de errores mostrada

- **Estado:** la Fase 3 informa todos sus errores, sin tope, ordenados por posición
  ([ciclos.md](ciclos.md) §5, «Número de errores: sin tope»). Lo guardado es O(n), pero
  el texto que se muestra es varias veces la entrada. Esta sección acota ese texto para
  **todas** las clases de error de `schemac check`.
- **Etapas.** `schemac check` devuelve los errores de una sola etapa: la primera que
  falla.
  - El lexer y el parser (Fase 2) se detienen en el primer error: **un solo mensaje**,
    de tamaño acotado. Lleva un texto fijo de ≤ 93 bytes de pista, el `expected` y el
    `found` recortados a 32 caracteres escapados (`short`, ≤ 10 bytes por carácter) y el
    prefijo de posición: ≲ 750 bytes más la ruta, sea cual sea la entrada (el Tester de
    4.1 midió 725 bytes sin ruta con dos cadenas de 40 × U+10FFFF).
  - La Fase 3 (`semantic::analyze`) devuelve **todos** los errores de su primera etapa
    que falla: meta-esquema, luego tabla de símbolos, luego ciclos. Nunca se suman dos
    etapas.
  - Los errores de `Ir::verify` (la IR que sale de la bajada, [ir.md](ir.md) §4) **no se
    listan al usuario**: son fallos del frontal, no del esquema, y `schemac` y el servidor
    los colapsan en **un único error interno** de tamaño fijo. Por eso no tienen fila en
    la tabla de abajo; la lista completa solo va al registro y a las pruebas.

  Por tanto la salida es ≤ ρ(r)·n + K, con n los bytes de la entrada, K unos cientos de
  bytes (el error de la Fase 2, o los bordes de un arreglo) y ρ(r) el mayor cociente de
  la tabla de abajo.
- **Cociente por construcción.** Cada error sale de una construcción de la entrada, y
  las construcciones de errores distintos no comparten bytes. El cociente de una
  construcción es (M + e·(r + 1)) / b, donde:
  - b son los bytes que la construcción **añade** a la entrada (las llaves de un objeto
    que comparte con otras no se cuentan, lo que sobrestima el cociente);
  - e son los errores que provoca;
  - M son los bytes de esos mensajes sin la ruta, tomando el mayor mensaje de la clase y
    posiciones de 7 cifras (entradas de menos de 10 MB; cada cifra más suma 1 o 2 bytes
    por error);
  - r es la longitud de la ruta que `schemac` pone delante de cada línea (`t.json: ` son
    8 bytes); cada error añade además un salto de línea.

  Para la Fase 3 entera:

  **ρ(r) = (376 + 2·(r + 1)) / 3 ≈ 126 + 0,67·r**, unas 131 veces con una ruta de 8
  bytes. Lo fija el elemento `""` de `type`. Es una cota **holgada**: M = 376 toma dos
  veces el mayor de los dos mensajes; los dos reales suman ≤ 351 bytes, lo que daría
  ≈ 118 + 0,67·r.

  Los valores de b, e y M salen del formato real de los mensajes
  (`crates/semantic/src/error.rs`, `meta.rs`, `symbols.rs`, `cycles.rs`), medidos con
  `schemac check` en release sobre documentos de m = 2 000 repeticiones:

  | Clase de error | Construcción mínima | b | e | M | Cociente, r = 0 | Cociente, r = 8 |
  |---|---|---|---|---|---|---|
  | meta-esquema: nombre de `type` en un arreglo | elemento `""` (nombre inválido **y** repetido) | 3 | 2 | 376 | **126,0** | **131,3** |
  | meta-esquema: repetido en `dependentRequired` | elemento `""` | 3 | 1 | 169 | 56,7 | 59,3 |
  | meta-esquema: repetido en `required` | elemento `""` | 3 | 1 | 160 | 53,7 | 56,3 |
  | meta-esquema: palabra clave rechazada (D6) | `"if":0,` | 7 | 1 | 292 | 41,9 | 43,0 |
  | ídem, desconocida | `"a":0,` | 6 | 1 | 201 | 33,7 | 35,0 |
  | meta-esquema: `$id` fuera de la raíz y con fragmento | `"$id":"##",` | 11 | 2 | 364 | 33,3 | 34,7 |
  | meta-esquema: `$ref` no local | `"$ref":"x",` | 11 | 1 | 296 | 27,0 | 27,7 |
  | meta-esquema: nombre de `type` (cadena) | `"type":"",` | 10 | 1 | 188 | 18,9 | 19,7 |
  | meta-esquema: `$schema` fuera de la raíz | `"$schema":"",` | 13 | 1 | 213 | 16,5 | 17,1 |
  | meta-esquema: `$anchor` inválido | `"$anchor":"",` | 13 | 1 | 209 | 16,2 | 16,8 |
  | meta-esquema: arreglo vacío | `"type":[],` / `"allOf":[],` | 10 / 11 | 1 | 155 / 149 | 15,6 / 13,6 | 16,4 / 14,4 |
  | meta-esquema: número fuera de rango | `"minItems":-1,` / `"multipleOf":0,` | 14 / 15 | 1 | 141 / 143 | 10,1 / 9,6 | 10,7 / 10,1 |
  | tabla de símbolos: `$ref` sin resolver | `{"$ref":"#%"},` (el peor de los 7 motivos) | 14 | 1 | 154 | 11,1 | 11,6 |
  | tabla de símbolos: ancla repetida | `{"$anchor":"a"},` | 16 | 1 | 94 | 5,9 | 6,4 |
  | ciclos no productivos | 8 aristas como elemento de `allOf` (abajo) | ≥ 84 | 1 | ≤ 974 | ≲ 11,6 | ≲ 12 |
  | Fase 2 (lexer y parser) | cualquiera | — | 1 | ≲ 750 | constante | constante |

  Por qué no hay construcciones más baratas:
  - un elemento de arreglo de cadenas no baja de 3 bytes (`""` y la coma);
  - un elemento no provoca más de 2 errores (nombre inválido y repetido, solo en `type`);
  - una palabra clave no puede repetirse en un objeto (D2), así que cada aparición paga
    su nombre;
  - los textos del usuario van recortados a 32 caracteres, así que alargarlos sube b más
    que M;
  - los escapes de `short` dan como mucho **6** bytes de salida por byte de entrada (un
    DEL, 0x7F, válido sin escapar en una cadena JSON, sale como `\u{7f}`), muy por
    debajo del cociente base: con elementos `"\x7f"` en `type` el Tester de 4.1 midió
    91,6 veces, menos que con `""`.
- **Medido** con la ruta `t.json: ` (8 bytes), en una sola línea:
  - el peor caso, `{"type":["","",…]}` con 10⁵ elementos: 0,30 MB → 35,4 MB, **118,1
    veces**;
  - `required`: 53,9 veces;
  - `dependentRequired`: 57,2 veces (lo midió el Tester de 4.1);
  - palabras clave rechazadas: con 52 claves de una letra más `if`, `then` y `else` por
    objeto, 34,6 veces; con la ruta `kw.json: ` (9 bytes), un error de `if` ocupa
    292 bytes por 7 de entrada (41,7 veces).
- **Ciclos, en detalle.**
  - **Formato del mensaje.** Un error de ciclo ocupa a lo sumo:
    - 137 bytes de texto fijo;
    - el prefijo `línea L, columna C: `, de 21 a 39 bytes;
    - 9 × 80 bytes de fragmentos;
    - 8 etiquetas, cada una de 8 bytes de separadores más la palabra clave;
    - ` …` y ` (N aristas)`.

    Eso da 974 bytes con etiquetas `$ref`/`not` y columnas de 7 cifras, y unos
    1 115 bytes en el extremo de 8 etiquetas `dependentSchemas`.
  - **Entrada mínima.** El ciclo más barato que llena los 9 fragmentos es un elemento
    de un `allOf`, sin clave: `{"$anchor":"a","not":{"not":…{"$ref":"#a"}…}}` con 7
    `not`, de 82 + 2k bytes (k es la longitud del ancla, así que ≥ 84). Las etiquetas
    largas cuestan en la entrada más de lo que añaden al mensaje. La autoarista más
    barata, `{"$anchor":"a","$ref":"#a"}`, cuesta ≥ 28 bytes para ≤ 337 de mensaje.
    Llegar a 12 exige anclas de una letra, que son como mucho 53, y con tan pocas
    dominan el prefijo compartido y la posición: medido, 5,2 y 8,0 veces con m = 50.
  - **Medido**, con ciclos como elementos de un `allOf` bajo 240 `not` de prefijo
    compartido: ciclos de 8 aristas, 11,35 veces con m = 3 000 y 11,2 con m = 2·10⁴;
    autoaristas, 11,3 y 10,9 veces.
  - **Variantes anteriores**, menos densas: la del auditor con `$defs`, 9,8 veces; el
    escenario H de [ciclos.md](ciclos.md) §5, 7,2 veces; `t16k.json`, ≈ 6 veces. El
    «≈ 9 veces» de la variante de 2,2 MB del escenario H es una **estimación**
    (1 015 ÷ 110), no una medida.
- **Frontal XSD (tarea 5.4).** Desde la 5.4 `schemac check` e `ir` aceptan XSD (D22,
  [bajada.md](bajada.md) §5), así que sus errores también salen por aquí. Igual que en JSON,
  el lector y el parser XSD (Fase 2) se detienen en el primer error, y `xsd::analyze`
  devuelve todos los de su primera etapa que falla (componentes y nombres; modelos de
  contenido; [xsd.md](xsd.md) §3). Medido con `schemac check` en release
  (`target/sondas-5.4/rho/medir.py`): cada construcción repetida m = 2 000 veces en su
  contexto, en una sola línea, con la ruta `t.xsd: ` (r = 7; la columna «medido» es bytes de
  stderr entre bytes de la entrada). b, e y M como arriba (M con las posiciones llevadas a
  7 cifras). Cuando la construcción repetida es además un nombre repetido, e cuenta los dos
  errores (por ejemplo «y repetido»). Los esquemas usan el espacio de XML Schema como
  espacio por defecto (`<element/>` en lugar de `<xs:element/>`, lo más barato) salvo
  cuando hace falta nombrar un tipo o un elemento del usuario sin espacio de nombres
  (prefijo `x:`).

  | Clase de error XSD | Construcción mínima | b | e | M | Cociente, r = 0 | Cociente, r = 8 | Medido, r = 7 |
  |---|---|---|---|---|---|---|---|
  | `value` (NCName) y `unsupported` (D14) a la vez | `<element name="␡"/>` (␡ = DEL, 0x7F, que `short` escribe `\u{7f}`) | 19 | 2 | 392 | **20,7** | **21,6** | 20,5 |
  | ídem con un nombre ASCII | `<element name="1"/>` | 19 | 2 | 382 | 20,2 | 21,1 | 20,0 |
  | `unsupported`: D14 (elemento sin tipo) | `<element name="a"/>` | 19 | 1 | 274 | 14,5 | 14,9 | 14,4 |
  | `syntax`: `xs:element` local sin `name` ni `ref` | `<element/>` | 10 | 1 | 70 | 7,1 | 7,9 | 6,9 |
  | `unresolved`: `ref` | `<element ref="a"/>` (y con un prefijo de URI largo, recortado: 6,7 / 7,0) | 18 | 1 | 127 | 7,1 | 7,6 | 7,0 |
  | `unresolved`: tipo complejo en un atributo (y repetido) | `<x:attribute name="a" type="T"/>` | 32 | 2 | 215 | 6,8 | 7,3 | 6,4 |
  | `unsupported`: `xs:anyType` | `<element name="a" type="anyType"/>` | 34 | 1 | 215 | 6,4 | 6,6 | 6,3 |
  | `duplicate-name`: atributo global | `<attribute name="a"/>` | 21 | 1 | 119 | 5,7 | 6,1 | 5,1 |
  | `content-model`: EDC (I12) | `<x:element name="a" type="T"/>` tras un `a` de otro tipo | 30 | 1 | 168 | 5,6 | 5,9 | 5,2 |
  | `duplicate-name`: atributo de un tipo (I10) | `<attribute name="a"/>` | 21 | 1 | 112 | 5,4 | 5,8 | 4,8 |
  | `syntax`: `default` y `fixed` (I13; y repetido) | `<attribute name="a" default="" fixed=""/>` | 41 | 2 | 209 | 5,1 | 5,5 | 4,8 |
  | `syntax`: requerido con `default` (I13; y repetido) | `<attribute name="a" use="required" default=""/>` | 47 | 2 | 224 | 4,8 | 5,1 | 4,5 |
  | `duplicate-name`: tipo global | `<complexType name="a"/>` | 23 | 1 | 108 | 4,7 | 5,1 | 4,2 |
  | `unresolved`: `type` | `<element name="a" type="b"/>` | 28 | 1 | 116 | 4,2 | 4,5 | 4,1 |
  | `unsupported`: predefinido fuera de §3.2 | `<element name="a" type="date"/>` | 31 | 1 | 125 | 4,1 | 4,3 | 4,0 |
  | `duplicate-name`: elemento global | `<element name="a" type="int"/>` | 30 | 1 | 119 | 4,0 | 4,3 | 3,6 |
  | `unsupported`: D27 (base compleja; y repetido) | `<x:complexType name="a"><x:simpleContent><x:extension base="T"/>…` | 98 | 2 | 360 | 3,7 | 3,9 | 3,6 |
  | `syntax`: `ref` con `type` | `<x:element ref="a" type="x:int"/>` | 33 | 1 | 113 | 3,5 | 3,7 | 3,4 |
  | `unsupported`: `xs:simpleType` pendiente (5.6; y repetido) | `<attribute name="a"><simpleType><restriction base="int"/></simpleType></attribute>` | 82 | 2 | 259 | 3,2 | 3,4 | 3,0 |
  | `content-model`: *all-group limited* (I12) | `<x:element ref="a" maxOccurs="2"/>` en `xs:all` | 34 | 1 | 103 | 3,1 | 3,3 | 3,0 |
  | `value`: `minOccurs` inválido | `<x:element ref="a" minOccurs="x"/>` | 34 | 1 | 104 | 3,1 | 3,3 | 3,0 |
  | `syntax`: `name` y `ref` | `<x:element name="a" ref="a"/>` | 29 | 1 | 87 | 3,0 | 3,3 | 3,0 |
  | `content-model`: `minOccurs` > `maxOccurs` (I11) | `<x:element ref="a" maxOccurs="0"/>` | 34 | 1 | 86 | 2,6 | 2,8 | 2,5 |
  | `syntax`: `type` y tipo anónimo | `<element name="a" type="int"><complexType/></element>` | 53 | 1 | 102 | 1,9 | 2,1 | 1,9 |
  | Fase 2 XSD (lector y parser) | cualquiera | — | 1 | ≲ 280 medidos (un valor de 40 DEL, recortado a 32) | constante | constante | — |

  **No supera el ρ vigente.** El peor cociente XSD es ≈ 21,6 con r = 8, unas seis veces
  menos que ρ(8) ≈ 131 de JSON Schema, así que **ρ(r) no cambia** y sigue acotando la
  salida de `schemac check` con los dos formatos. Por qué no hay construcciones más baratas
  (corregido en el cierre de la 5.4, 5.4-CR1-I1: una versión anterior decía, por error, que
  una etiqueta da a lo sumo 2 errores):
  - todo error del análisis sale de una etiqueta de XSD, que no baja de 10 bytes
    (`<element/>` con el espacio de XML Schema por defecto; el lector ya rechaza un prefijo
    sin declarar o una etiqueta sin nombre);
  - una etiqueta da **a lo sumo un error por valor de atributo** (nombre que no es NCName,
    `minOccurs`/`maxOccurs` inválidos, `type`/`ref` sin resolver o fuera de §3.2, `default`
    con `fixed`…; un atributo repetido en una etiqueta es `duplicate-attribute`, de la
    Fase 2, así que cada atributo pesa una vez), **más uno por la etiqueta** (D14, «sin
    `name` ni `ref`» o el nombre repetido);
  - cada error de atributo ocupa ≤ ≈ 120 bytes de mensaje con un valor de un carácter y su
    atributo añade ≥ 13 bytes a la entrada (` minOccurs="␡"`; el más corto, ` name="␡"`,
    9 bytes, ya cuenta en la fila de arriba): menos de 10 veces, por debajo del peor
    cociente (con un valor de 32 caracteres escapados el mensaje llega a ≈ 300 bytes, pero
    el atributo pasa de 44); el error de la etiqueta ocupa ≤ 274 bytes sobre ≥ 10 a 19 bytes (D14
    necesita `name`, así que no baja de `<element name="a"/>`). Añadir atributos con error
    **baja** el cociente; la peor combinación sigue siendo `name="␡"` sin tipo (NCName y
    D14, 21,6 con r = 8);
  - los textos del usuario van recortados a 32 caracteres (`XsdError::new`), así que
    alargarlos sube b más que M, y los escapes de `short` dan como mucho 6 bytes por byte
    (DEL), que en un nombre de un carácter es la fila de arriba.

  Lo confirman las sondas del Reviewer de la 5.4 (`target/sondas-5.4/reviewer1/p2.py`, en
  release, ruta `t.xsd`, r = 7): `<element name="␡" maxOccurs="␡"/>`, 3 errores por etiqueta
  (NCName, `maxOccurs`, D14), cociente medido 15,5; `<element name="␡" minOccurs="␡"
  maxOccurs="␡"/>`, 4 errores (NCName, `minOccurs`, `maxOccurs`, D14), 13,2; y la segunda a
  16 MiB (`reviewer1/multi.xsd`), stderr 18,7 veces la entrada y memoria 46 veces (por
  debajo de las cifras de JSON Schema, abajo). El camino de éxito XSD también queda por
  debajo del de JSON (abajo).
- **Camino de éxito de `schemac ir` (SA-L2 de la revisión de la 4.2).** La tabla de
  arriba solo acota los errores. Con un esquema válido, `schemac ir` escribe la IR entera,
  que es más grande que la entrada, y la construye en memoria antes de escribirla.
  Medido en release con `{"allOf":[{},{},…]}`, la entrada más densa en nodos (un
  subesquema `{}` cada 3 bytes). **Remedido el 28 oct tras la tarea 4.3**, que añade la
  clase y la estrategia de cada nodo y regla (formato JSON v2); entre paréntesis, la
  medida del 27 oct con el formato v1 (después del cambio de los literales a listas de
  tokens):

  | Entrada | Salida JSON | Salida DOT | Memoria máx. `ir` | Memoria `ir --dot` | Memoria `check` |
  |---|---|---|---|---|---|
  | 0,30 MB | 27,0 MB (**90×**; v1: 75×) | 9,4 MB (31×; v1: 30×) | 54 MiB (v1: 50) | 37 MiB | 27 MiB |
  | 8,1 MB | 752 MB (**93×**; v1: 78×) | 278 MB (**34×**; v1: 33×) | 1 221 MiB (**≈ 158×**; v1: 143×) | 856 MiB (111×) | 645 MiB (83×) |

  Unos 278 bytes de JSON por nodo `{}` de 3 bytes (230 en v1: `class` y `strategies`
  suman ≈ 48 por nodo). La memoria de la entrada pequeña está dominada por el tamaño
  base del proceso; la cifra que escala es la de 8 MB. La **salida** (≈ 93×) queda por
  debajo de ρ(r) ≈ 131, pero la **memoria** (≈ 158× la entrada, frente a ≈ 83× de
  `check`) **supera** ρ(r): la dominan la IR, el AST y el `String` con la salida entera
  (la clasificación solo añade un vector de un par de bytes por nodo y el CSR de E₌).
  **XSD (tarea 5.4).** Medido igual, en release, con las entradas XSD más densas en nodos y
  en aristas: 2·10⁵ tipos anónimos `<element name="aN"><complexType/></element>` (9,5 MB):
  JSON 9,1×, DOT 2,0×, memoria ≈ 28× en `ir` y ≈ 23× en `check`; 2·10⁵ partículas
  `<x:element name="a" type="T"/>` (6,0 MB): JSON 4,9×, DOT 0,9×, memoria ≈ 22× (≈ 19× en
  `check`). Con entradas de **16 MiB** (el tope; `target/sondas-5.4/rho/memoria.py`, que
  incluye los peores casos del lector y del AST de [../fase2/pendientes.md](../fase2/pendientes.md)
  §12), memoria de pico de `check` / `ir` y salida de `ir`:

  | Entrada de 16 MiB | Código | Memoria `check` | Memoria `ir` | Salida `ir` |
  |---|---|---|---|---|
  | `<choice/>` repetido en una `xs:sequence` | 0 | 829 MiB (51,8×) | **945 MiB (59,0×)** | **15,5×** |
  | `<element/>` repetido (un error por cada uno) | 1 | 863 MiB (53,9×) | 864 MiB (54,0×) | 8,4× (stderr) |
  | `xmlns:pK="u"` en `xs:schema` | 0 | 722 MiB (45,1×) | 724 MiB (45,2×) | ≈ 0 |
  | una etiqueta con ≈ 1,9 M atributos desconocidos | 1 | 508 MiB (31,8×) | 508 MiB (31,8×) | ≈ 0 |
  | tipos anónimos `<element name="aNNNNNNN"><complexType/></element>` | 0 | 363 MiB (22,7×) | 429 MiB (26,8×) | 8,9× |
  | elementos globales `<element name="aNNNNNNN" type="int"/>` | 0 | 236 MiB (14,8×) | 281 MiB (17,6×) | 4,2× |

  El peor XSD (≈ 59× de memoria y ≈ 15,5× de salida) queda por debajo de las cifras de JSON
  Schema (≈ 158× y ≈ 93×), así que las de esta sección siguen siendo las que dimensionan el
  servidor (requisito 4 de §2).
  Por eso el requisito 4 de §2 dimensiona `RLIMIT_AS` con la memoria medida (≈ 158×),
  **no con ρ**, que solo acota el texto de salida. Con el tope de 16 MiB de `schemac`
  (`00-alcance.md` §4) una compilación puede necesitar ≈ 2,7 GB y escribir ≈ 1,6 GB.
  **Consecuencia para el servidor:** el tope de subida y el `RLIMIT_AS` del requisito 4
  de §2 se fijan juntos (memoria ≈ 165 × tope de subida, más el proceso base); por
  ejemplo, 1 MiB de subida necesita ≈ 175 MiB. **Opción para la
  Fase 7:** escribir `write_json`/`write_dot` directamente en la respuesta con un
  adaptador `fmt::Write` → `io::Write`, sin el `String` intermedio (quita la salida de la
  memoria), decidiendo qué se hace si hay un pánico a medio escribir (la respuesta ya
  empezó).
- **Cuándo actuar:** en el servidor de la aplicación web (Fase 7; está en la tarea del
  backend de la semana 4 del plan). La Fase 3 sigue devolviéndolos todos, igual que
  `schemac`, y el formato de los mensajes no cambia: quien presenta la lista la trunca o
  la agrupa (por ejemplo, el motivo una sola vez por clase). La tarea del backend debe
  tener un **criterio de aceptación explícito** que cubra las tres cosas juntas, porque
  sin ellas una subida pequeña produce una respuesta de hasta ρ(r) ≈ 126 + 0,67·r veces
  su tamaño (r es lo que el servidor ponga delante de cada mensaje):
  - límite de tamaño de subida (`docs/fase2/pendientes.md` §6), que con el factor de
    arriba acota también el tamaño de la respuesta;
  - truncar la lista a N errores, con «… y K más»;
  - tope de CPU (tiempo) por compilación.
  Opcional: una variante `check_cycles_limited(table, n)` que deje de construir errores
  tras los primeros n, para no pagar su costo cuando no se van a mostrar.

### 2. `debug_assert!` en código que llamará el servidor

- **Estado:** `path_in` (`crates/semantic/src/cycles.rs`) afirma con `debug_assert!` dos
  invariantes de la búsqueda del camino de un ciclo. En release no hacen nada: la
  salvaguarda devuelve un camino vacío y el error de la componente se informa igual (lo
  fijan `camino_a_otra_componente_es_vacio_en_release` y
  `cadena_de_pred_rota_es_vacia_en_release`, que solo corren con
  `cargo test -p semantic --release`; `make test` compila en debug y corre las gemelas
  `#[should_panic]`). En debug, si un cambio futuro rompiera el invariante, harían pánico.
- **Cuándo actuar:** al integrar la Fase 3 en el servidor (Fase 7). La tarea del backend
  debe cumplir los **cuatro requisitos a la vez** (ninguno sustituye a otro):
  1. cada compilación de esquema corre en su propio hilo con `stack_size` explícito (D7)
     y el servidor la espera con `join()` (o `std::panic::catch_unwind`), convirtiendo un
     pánico en un error 500 de **esa** petición, como ya hace `schemac`
     (`crates/cli/src/main.rs`, hilo `schemac-check` y la rama `Ok(Err(_))` del `join`);
  2. el perfil con que se compila el servidor nunca usa `panic = "abort"`: con él un
     pánico tumba el proceso aunque esté en otro hilo (lo recuerda un comentario en el
     `Cargo.toml` del workspace);
  3. compilar en release es recomendable por rendimiento, pero **no es una medida de
     seguridad**: quita los `debug_assert!`, no los pánicos de `assert!`, índices o
     `unwrap`. Y un desbordamiento de pila aborta el proceso aunque ocurra en un hilo
     aparte (no se puede capturar con `join` ni `catch_unwind`), por eso el tope de
     profundidad del parser y la pila explícita de las fases siguientes (D7) son
     obligatorios.
  4. **tope de memoria por compilación** (L-2 de la revisión final de 4.1): cada
     compilación corre en un **proceso hijo** con `RLIMIT_AS` o en un cgroup con límite
     de memoria. Un fallo de asignación aborta el proceso entero, igual que un
     desbordamiento de pila: ni `join` ni `catch_unwind` lo capturan, así que el aislamiento
     por hilo de los requisitos 1 y 2 no basta. La cifra: la compilación completa hasta la
     IR exportada usa ≈ 158 veces el tamaño del esquema (≈ 83 veces solo hasta la Fase 3;
     medido en §1, «Camino de éxito de `schemac ir`», con el formato v2 de la tarea 4.3),
     así que `RLIMIT_AS` ≈ 165 × tope de subida + el proceso base. Se dimensiona con esa
     memoria y no con ρ(r) ≈ 131 de §1, que acota el texto de salida y queda por debajo
     de ella. **Además de `RLIMIT_AS`**, el servidor debe **consumir la salida en
     streaming** (escribir la IR directamente en la respuesta, la opción de §1) **o fijar
     un límite de bytes de salida**: con el tope de 16 MiB de `schemac`, el peor caso
     medido es ≈ 1,5 GiB de salida JSON y ≈ 2,5 GiB de memoria por compilación (≈ 1,6 GB y
     ≈ 2,7 GB, §1), que ningún cliente web necesita recibir.

### 3. Obligaciones del frontal XSD (tareas 5.3, 5.5, 5.6 y 5.7 de la semana 2) — parte de la 5.3 RESUELTA

- **Resuelto en la tarea 5.3** ([xsd.md](xsd.md) §3; corpus `tests/schemas/xsd/semantic-error`,
  manifiesto `tests/schemas/errores-semanticos-xsd.json`, prueba `corpus_entero_sin_error_interno`):
  I10 (atributos repetidos en un tipo o en su `xs:extension`), I11 (`minOccurs` > `maxOccurs`), I12
  (*all-group limited* y *Element Declarations Consistent*), la parte de I13 que no depende de los
  tipos simples (`default` con `fixed`, requerido con `default`), D14, D27 (U7) y S-5 primera mitad
  (B3). Todo con su posición y antes de bajar; `xsd::lower` llama siempre a `verify` (D15) y
  ningún esquema del corpus da el error interno. **Sigue abierto:** la parte de I13 de los tipos
  simples y facetas, la derivación circular y que `default`/`fixed` sean del tipo (5.6), la forma
  léxica de los valores (5.5), UPA y la ampliación de la lista de [ir.md](ir.md) §4 (5.7). Mientras
  no esté la 5.6, todo `xs:simpleType` es el error «pendiente (tarea 5.6)». **UPA y `xs:all`
  (5.3-CR1-S4):** un `xs:all` con dos hijos del mismo nombre y tipo pasa EDC y `verify`, pero viola
  UPA; la máscara de `xs:all` de la Fase 4 (un bit por nombre, I12) depende de que la 5.7 lo
  rechace (§33).
- **Para la 5.6 (lo que deja la 5.3):** al bajar los `xs:simpleType` del usuario hay que meterlos en
  el preorden de declaraciones de B3 (hoy `Analysis::types` solo tiene tipos complejos), crear el
  envoltorio `#/simpleContent/T` del usuario y los orígenes de los anónimos de [ir.md](ir.md) §5.2,
  y quitar el error «pendiente»: `TypeRef::UserSimple` y la arista colgante de
  `crates/xsd/src/lower.rs` (`text_type`, `element_type`) son el punto de entrada.

Observaciones de la revisión final de 4.1 (L-3, I-1, I-2, S-5). `verify` es defensa en
profundidad ([ir.md](ir.md) §4): todo lo que un XSD del **usuario** pueda romper lo tiene
que rechazar antes el frontal, **con su posición**, no como el error interno de `verify`
(código 3 de `schemac ir`, [bajada.md](bajada.md) §4).

- **Lista de [ir.md](ir.md) §4, «Obligaciones del frontal XSD»** (L-3): derivación
  circular de tipos simples (iterativa, D7), I12, la parte de I13 que viene del usuario,
  *Unique Particle Attribution* y que `default`/`fixed` sean valores del tipo.
- **La lista se queda corta** (I-1): I4, I10 e I11 también los rompe un XSD del usuario:
  atributos repetidos (I10), `minOccurs` > `maxOccurs` (I11), una faceta repetida en una
  misma restricción o `length` junto a `minLength`/`maxLength` (I4, porque `length` baja
  a las dos reglas), y además `min*`/`max*` inclusivo y exclusivo a la vez. **Regla
  general:** el frontal rechaza con su posición toda *Schema Component Constraint* y toda
  restricción `src-*` de XSD 1.0 que caiga en el subconjunto; ampliar la lista de
  [ir.md](ir.md) §4 con ella.
  - **Reparto entre las subtareas de `tasks/todo.md` (3 nov):**
    - **Tarea 5.3** (invariantes estructurales): I10 (dos `xs:attribute` con el mismo nombre
      en un tipo), I11 (`minOccurs` > `maxOccurs`), I12 y la parte de I13 que viene del
      usuario.
    - **Tarea 5.6** (tipos simples): I4 (faceta repetida, `length` con `minLength`/
      `maxLength`), derivación circular de `base` (iterativa, D7), `min*`/`max*` inclusivo y
      exclusivo a la vez, y que `default`/`fixed` sean valores del tipo.
    - **Tarea 5.7:** *Unique Particle Attribution* (conservadora, D25) y la ampliación de la
      lista de [ir.md](ir.md) §4. **Aplazada** a después del hito de la semana 3 por D28
      (aplicado el 3 nov); mientras, el Glushkov de la Fase 4 es la defensa.
  - *Avance de la tarea 5.1 (base de I10):* **resuelto** que un mismo atributo XML no se
    repita en una etiqueta del documento de esquema, por nombre escrito y por nombre
    expandido (`duplicate-attribute`, [../fase2/lector-xsd.md](../fase2/lector-xsd.md) §2).
    **Sigue pendiente** I10 en sí: dos `xs:attribute` con el mismo nombre en un tipo
    complejo (`duplicate-name`, tareas 5.3-5.6).
- **`xs:float`/`xs:double`** (I-2; frontal XSD y Fase 4): se comparan en IEEE
  binary32/binary64 (XSD 1.0 Parte 2 §3.2.4–3.2.5), no en decimal exacto como dicen hoy
  [ir.md](ir.md) §5.1 y §7 (fila Q). Hay que corregir §5.2, §7 y el pendiente 3 de §11, o
  documentar la desviación en `00-alcance.md` §3.2. No se corrigió en la 4.2.
  **Tarea 5.5.** **DECIDIDO (D24, 29 oct):** `INF`, `-INF`, `NaN` y los valores que
  desbordan al redondear a binary32/64 (`1e39` en `xs:float`) se rechazan en facetas y
  enumeraciones de `xs:float`/`xs:double` con un mensaje que lo diga; no se amplían `Num` ni
  I8.
- **Orígenes sin fijar** (S-5):
  - Primera mitad, el origen de un tipo anónimo dentro de un `xs:element` **global** —
    **RESUELTO (tarea 5.3)**: padre el documento y paso `Entry("element", nombre)` (B3;
    [xsd.md](xsd.md) §4, prueba `esquemas_de_los_casos_bajan`).
  - Segunda mitad, `xs:simpleContent` + `xs:extension` sobre un tipo complejo de contenido
    simple: **DECIDIDO (D27, 29 oct)**, se rechaza con un mensaje claro (derivar un tipo
    complejo de otro queda fuera del alcance, como `xs:complexContent`). **RESUELTO (tarea
    5.3)**: `unsupported` en el valor de `base` (`semantic-error/simple-content-base-compleja.xsd`).

### 4. Ubicación recortada para los mensajes del validador (Fases 4 y 5)

- **Estado:** `Ir::location` construye la ruta entera, sin tope ([ir.md](ir.md) §11,
  pendiente 2). La 4.2 no la usa en ningún mensaje: `schemac ir` escribe el padre y el
  paso de cada nodo, no su ruta ([bajada.md](bajada.md) §5.1).
- **Cuándo actuar:** al escribir los mensajes de error del payload (Fase 4) y el codegen
  (Fase 5): usar `location_clipped(id, max)`, análogo a `SymbolTable::fragment_clipped`,
  en O(profundidad + max), con `max` fijado en `00-alcance.md` §4; y añadir su fila a la
  tabla ρ de §1.

### 5. Construcción del DPDA (Fase 4)

Observaciones de la revisión final de 4.1 sobre [ir.md](ir.md) §6–§7:

- **Tope de estados para todo producto** (L-1): no solo `anyOf`/`oneOf`; también `allOf`,
  `Ref` con reglas hermanas y `not` sobre un subesquema (c). Contar estados y transiciones
  **antes** de reservar. Caso de prueba: un `allOf` de `pattern` `^(a{p})*$` con los 15
  primeros primos (el producto tiene ∏ p estados).
- **Registros de la fila Γ** (S-1): a la máscara le faltan los bits de `DependentRequired`
  (D5); falta la tabla hash acotada de claves no declaradas (D2,
  `max_claves_adicionales`); y decir que los registros salen «de las reglas (b)» no es
  exacto.
- **`not` y los límites** (S-2): un error de límite atraviesa `not` sin invertirse, y la
  inversión exige un veredicto exacto (con `uniqueItems` por hash no lo es).
- **Componentes del producto** (S-3): son el conjunto de `NodeId` **distintos** alcanzados
  por E₌, no los caminos (la cadena de `cadena_de_referencias_tamano_lineal`,
  [bajada.md](bajada.md) §7, tiene 2^(10⁴ − 1) caminos y 10⁴ nodos); los combinadores se
  evalúan como un circuito, con multiplicidad en `oneOf`.

### 6. CI — SIN EFECTO (D32, 3 nov) → [historial.md](historial.md) §6

### 7. Abiertas a propósito en la tarea 4.0

- `pointer::encode_char` (`crates/semantic/src/pointer.rs`) devuelve `&[u8]`; podría
  devolver `&str` y ahorrar la conversión byte a byte de `emit_head`/`emit_tail`.
- `Seg::Num` reserva un `String` por índice (`i.to_string()`) cada vez que
  `emit_head` y `emit_tail` (`crates/semantic/src/symbols.rs`, `fragment_clipped`)
  recorren un paso `Index`.

### 8. Código de salida de un pánico en `schemac check` (tarea 4.2) — RESUELTO (tarea 5.4, D33)

- **Resuelto (tarea 5.4):** el usuario lo decidió el 3 nov (D33): un pánico en el hilo de
  `schemac check` da **3**, igual que en `ir`. `en_hilo` (`crates/cli/src/main.rs`) ya no
  recibe el código del pánico: siempre es `SALIDA_INTERNA`; `check` pasa por
  `ejecutar_check`, que la prueba `panico_en_el_hilo_da_codigo_3` recorre con un análisis
  que hace pánico (3, el mensaje en stderr, nada en stdout). [bajada.md](bajada.md) §4 y la
  tabla de §5 y el texto de uso lo dicen.
- **Estado (antes de la 5.4):** `schemac ir` da **3** para un error interno, sea un fallo de `verify` o un
  pánico en su hilo ([bajada.md](bajada.md) §4). `schemac check` conserva el **2** que ya
  daba a un pánico («no se pudo leer o analizar»). Unificarlo (pánico = 3 en los dos) cambia
  la interfaz de `check`; queda propuesto para cuando lo decida el usuario.

### 9. Aviso para «`allOf` + `additionalProperties: false`» (Fase 7, o cuando haya avisos)

- **Estado:** el pendiente 5 de [ir.md](ir.md) §11 propone un **aviso** (no un error) para
  ese patrón, que casi nunca hace lo que se quiere ([ir.md](ir.md) §6.2). La 4.2 no lo
  implementa: `schemac` no tiene todavía un canal de avisos (solo errores, con su código de
  salida) y añadirlo cambia la interfaz.
- **Cuándo actuar:** cuando exista ese canal, a más tardar en la aplicación web (Fase 7),
  que muestra el resultado de la compilación. La comprobación es local a un nodo de la IR
  (`AllOf` y `Properties { additional: Some(n) }` con `n` = `Reject`), O(1) por nodo.

### 10. Formato JSON de `schemac ir` en JavaScript (Fase 7)

- **Estado:** los contadores (`minLength`, `maxItems`…) salen como enteros JSON y pueden
  valer hasta 18446744073709551615 (`u64::MAX`, saturado; [bajada.md](bajada.md) §5.1),
  más que `Number.MAX_SAFE_INTEGER`. `JSON.parse` los redondea.
- **Cuándo actuar:** en la web (Fase 7), al leer la IR: tratarlos con `BigInt` (o como «sin
  límite» si valen `u64::MAX`), o cambiar el formato a cadenas subiendo `irVersion`.

### 11. Ordenación de las claves de los literales (Fase 6)

- **Estado:** la bajada es O(n) salvo la ordenación de las claves de cada objeto literal de
  `enum`/`const` (I9), O(k log k) por objeto de k miembros, O(n log n) en el peor caso
  ([bajada.md](bajada.md) §6). Es inherente a I9 con comparaciones.
- **Cuándo actuar:** si el benchmark del crate `bench` (Fase 6) muestra que pesa en esquemas
  con literales grandes, cambiarla por una ordenación por radix sobre los bytes de las
  claves, lineal en su longitud total.

### 12. Etiquetas DOT de la IR sin escapes de RFC 6901 (cosmético)

- **Estado:** el DOT de `schemac ir --dot` etiqueta cada nodo con su paso desde el padre,
  por ejemplo `properties/a/b~c`, sin los escapes `~0`/`~1` que sí pone `Ir::location`
  (`#/properties/a~1b~0c`). La etiqueta es un texto para leer, no un puntero: los nombres
  van recortados con `short` y no se pueden resolver ([bajada.md](bajada.md) §5.2). Por eso
  una clave con `/` se lee ambigua en el diagrama.
- **Cuándo actuar:** si un diagrama lo necesita, escapar el nombre con RFC 6901 antes de
  `short`. No afecta al JSON de `schemac ir`, que separa `keyword` y `entry`.

### 13. Tipo `AnalyzedTable` para la entrada de la bajada (Fase 4)

- **Estado:** `semantic::lower` exige una tabla que ya pasó `analyze`, pero acepta
  cualquier `&SymbolTable`, también la de `SymbolTable::build` sola (CR-S-2 de la revisión
  de la 4.2). Con ella, un error del esquema del usuario (un ciclo no productivo, que
  rompe I14) saldría como error interno (código 3) en lugar de como error del esquema con
  su posición. Hoy solo lo documenta `lower` ([bajada.md](bajada.md) §1).
- **Cuándo actuar:** antes de que otro llamador use `lower` (el constructor del DPDA de la
  Fase 4, o el servidor de la Fase 7): que `analyze` devuelva un tipo `AnalyzedTable` que
  solo se pueda obtener de ella y que `lower` lo exija, igual que `VerifiedIr` para la
  Fase 4.

### 14. El «registro» de D15 es stderr (Fase 7)

- **Estado:** D15 manda la lista de errores de `verify` «solo al registro». `schemac` no
  tiene registro aparte: el error único y, debajo, el registro acotado (el recuento y los
  8 primeros, con el prefijo `schemac: registro:`) salen los dos por stderr
  ([bajada.md](bajada.md) §4).
- **Cuándo actuar:** en el servidor (Fase 7): el usuario recibe solo el error único y el
  registro va al log del servidor. Si `schemac` llega a tener un modo de registro (un
  archivo, `--log`), las líneas `schemac: registro:` se mueven allí.

### 15. Texto del usuario en bruto en el JSON de la IR (Fase 7, frontal web)

- **Identificadores:** SA-I1 (revisión de la ronda 2), **4.2-I2** (revisión final de la
  4.2) y **4.3-I1** (revisión de seguridad de la 4.3): la misma observación.
- **Estado:** el JSON de `schemac ir` es fiel al esquema: claves, nombres y cadenas van
  enteros, sin recortar, con solo los escapes de RFC 8259 (SA-I1 de la revisión de la
  4.2). Pueden llevar U+202E (texto bidireccional), U+2028, U+0085, DEL o marcado HTML.
- **Cuándo actuar:** en la tarea del frontal de la Fase 7: la web que muestre la IR inserta
  ese texto con `textContent` (o el equivalente que no interpreta marcado), **nunca con
  `innerHTML`**, y conviene marcar los caracteres de control y bidireccionales al
  mostrarlos.
- **4.3-I1 (añadido en la 4.3):** el JSON deja además `<`, `>`, U+2028 y U+2029 sin escapar
  (es JSON válido: RFC 8259 no los obliga a escapar). Si la web lo incrusta en un
  `<script>` (U+2028/U+2029 terminaban literales de cadena en JavaScript anterior a
  ES2019, y `</script>` cerraría el bloque) o lo inserta con `innerHTML`, debe tratarlo
  como texto no confiable: cargarlo con `fetch` + `JSON.parse`, o escapar `<`, `>`, `&`,
  U+2028 y U+2029 como `\u…` al incrustarlo.

### Observaciones abiertas de la revisión final de la 4.2

La 4.2 quedó aprobada (0 Critical, 0 High, 0 Important, 0 Medium). Estas observaciones
quedaron abiertas; 4.2-I2 es §15. La tarea 4.3, que toca la CLI y `crates/ir`, resolvió
§16-§21 (cada una conserva su texto original y una línea **Resuelto**).

### 16. 4.2-S1: argumentos con `--` según su posición — RESUELTO (tarea 4.3) → [historial.md](historial.md) §16

### 17. 4.2-S2: la prueba `fallo_interno_da_codigo_3` escribe el DOT en la salida de `cargo test` — RESUELTO (tarea 4.3) → [historial.md](historial.md) §17

### 18. 4.2-S3: el corpus de `tests/cases` se reserializa con `serde_json` — RESUELTO (tarea 4.3) → [historial.md](historial.md) §18

### 19. 4.2-L1: abrir una FIFO sin escritor bloquea `schemac` — RESUELTO (tarea 4.3; el requisito de la Fase 7 sigue)

- **Resuelto:** `leer` comprueba con `fs::metadata` que la ruta es un archivo regular
  **antes** de abrirla, y otra vez sobre el descriptor; el comentario de `MAX_SCHEMA_BYTES`
  y [bajada.md](bajada.md) §5 lo dicen, y documentan que las rutas de la CLI son de
  confianza (entre las dos comprobaciones queda una carrera si alguien cambia la ruta por
  una FIFO). `fifo_sin_escritor_no_bloquea` (`crates/cli/tests/es.rs`, solo Unix, FIFO
  creada con `mkfifo` y una guarda de 60 s que mata el proceso) da 2 en `check` e `ir`; sin
  el arreglo se cuelga y falla. **Sigue en pie** el requisito para el servidor de la Fase 7:
  recibe bytes, nunca rutas de los usuarios. La carrera que queda entre las dos
  comprobaciones es **4.3-L1** (§23).

- **Dónde:** `crates/cli/src/main.rs:124` (`leer`: `File::open` antes de `is_file()`).
- **Estado:** `File::open` sobre una FIFO sin escritor se bloquea antes de llegar a la
  comprobación `is_file()`: `mkfifo f; timeout 5 schemac ir f` termina con 124 (colgado).
  «Una FIFO nunca se carga» es cierto en memoria, no en tiempo. La frase ya está corregida
  en [bajada.md](bajada.md) §5; la del comentario de `MAX_SCHEMA_BYTES`
  (`crates/cli/src/main.rs:44-46`) hay que corregirla con el arreglo, porque este cierre solo
  toca `docs/`. Solo importa si un proceso de confianza pasa a `schemac` rutas que eligen
  los usuarios.
- **Arreglo propuesto:** abrir con `O_NONBLOCK` (`OpenOptionsExt::custom_flags` en Unix), o
  llamar a `std::fs::metadata(path)` antes de `open` y rechazar lo que no sea un archivo
  regular, manteniendo `is_file` sobre el descriptor. Documentar además que las rutas de
  la CLI son de confianza.
- **Cuándo:** la próxima tarea que toque la CLI. Requisito para el servidor de la Fase 7:
  recibe **bytes**, nunca rutas de usuarios.

### 20. 4.2-S4: orden de los `use` en `verify.rs` — RESUELTO (tarea 4.3) → [historial.md](historial.md) §20

### 21. 4.2-I1: `eprintln!` puede hacer pánico si falla stderr — RESUELTO (tarea 4.3) → [historial.md](historial.md) §21

### Abiertas por la tarea 4.3 (clasificación)

### 22. Clasificación: `$ref` con hermanas y XSD (Fase 4 y frontal XSD)

- **`$ref` con reglas hermanas (Fase 4; CR1-S6).** Es (a) por el alcance (`00-alcance.md` §2.1),
  pero la Fase 4 lo compila como un producto con sus hermanas ([ir.md](ir.md) §7), igual
  que un `allOf` de dos ramas, que es (c). Una cadena de `$ref` con hermanas crece como un
  producto; el tope de estados de §5 (L-1) ya lo cubre. Si la Fase 4 mide que se comporta
  como el producto de `allOf`, proponer al usuario cambiar la fila de `$ref` en el alcance
  ([clasificacion.md](clasificacion.md) §8, pendiente 1). Mientras, manda el alcance.
- **Frontal XSD (tarea 5.4) — RESUELTO (tarea 5.4).** La tabla ya clasifica las reglas de XSD y
  `tabla_coincide_con_el_alcance` cubre sus filas de §3.1, pero ningún XSD real pasa todavía
  por `classify`. Lo hace la **tarea 5.4** (criterio de `tasks/todo.md`): el equivalente de
  `corpus_clasificado` en `crates/xsd/tests/corpus.rs`, con los esquemas XSD del corpus y el
  caso `valid-xml-pedido-01`.
  - **Resuelto:** `corpus_clasificado` de `crates/xsd/tests/corpus.rs` clasifica todo XSD de
    `tests/schemas/xsd/parse-ok` que pasa el análisis (7 hoy; los otros 3 usan
    `xs:simpleType`, pendiente de la 5.6) y los dos casos XSD de `tests/cases` con el texto de
    su campo `schema` (una cadena JSON cuyo contenido es el XSD, byte a byte igual a su copia
    de `parse-ok`), comprueba la definición nodo a nodo y que no hay nodos (c);
    `valid-xml-pedido-01` tiene un solo nodo (b), subido solo por `Attributes`, y
    `malformed-xml-billion-laughs-01`, ninguno ([clasificacion.md](clasificacion.md) §7).
    **Queda para la 5.6:** cuando bajen los `xs:simpleType`, los tres XSD de `parse-ok` que
    hoy dan «pendiente» entrarán solos en el recorrido.
- **Regenerar la tabla escrita (CR1-S7, en parte).** El bloque de [clasificacion.md](clasificacion.md) §3 no
  lo regenera `make docs`: lo comprueba la prueba `tabla_escrita_es_la_del_codigo`, que al
  fallar imprime la tabla que hay que copiar. Si se quiere en `make docs`/`make docs-check`,
  añadir un subcomando (por ejemplo `schemac tabla-clasificacion`) cuando haya otro motivo
  para tocar la CLI.
- **Un solo caso `valid` JSON sin palabras clave (c) (T-4; corpus de la Fase 8 o la próxima
  tarea que añada casos).**
  - **Dónde:** `tests/cases/valid/` (`valid-recursive-tree-01` es el único caso `valid` en
    JSON); `corpus_clasificado` (`crates/semantic/tests/corpus.rs`).
  - **Estado:** el criterio «`valid` sin palabras clave (c) → ningún nodo (c)» se comprueba
    sobre ese único caso; desde la ronda 1 de la 4.3 también sobre los esquemas de
    `tests/schemas/parse-ok` (al menos 5), así que ya no depende de uno solo.
  - **Arreglo propuesto:** añadir casos `valid` con objetos, arreglos, números y cadenas
    sin combinadores cuando se amplíe el corpus.

### Abiertas por la revisión final de la 4.3

### 23. 4.3-L1: carrera entre `fs::metadata` y `File::open` en `leer` — RESUELTO (tarea 5.4)

- **Resuelto (tarea 5.4):** `leer` abre el esquema con `abrir`, que en Unix usa
  `OpenOptionsExt::custom_flags(libc::O_NONBLOCK)` (`libc` como dependencia de `cli` solo en
  Unix, por la constante; sin `unsafe`) y decide con `is_file()` sobre el descriptor. Se
  **mantiene** la comprobación previa con `fs::metadata`, para no abrir siquiera un
  dispositivo o una FIFO en el caso normal (abrir un dispositivo puede tener efectos); ya no
  es la que decide. Fuera de Unix sigue `File::open`. Prueba
  `abrir_una_fifo_sin_escritor_no_bloquea` (`crates/cli/src/main.rs`, solo Unix): `abrir`
  sobre una FIFO sin escritor, saltándose la comprobación previa como en la carrera, devuelve
  «no es un archivo regular»; sin `O_NONBLOCK` se queda en el `open` y la guarda de 60 s la
  hace fallar (comprobado). [bajada.md](bajada.md) §5 lo dice.

- **Dónde:** `crates/cli/src/main.rs:152-164` (`leer`).
- **Estado:** el arreglo de §19 comprueba que la ruta es un archivo regular con
  `fs::metadata(path)` y después la abre con `File::open(path)`. Entre las dos llamadas otro
  proceso puede cambiar la ruta por una FIFO sin escritor, y el `open` se bloquea para
  siempre: la comprobación `is_file()` sobre el descriptor llega tarde. Es solo una
  denegación de servicio de esa invocación; las rutas de la CLI son de confianza
  ([bajada.md](bajada.md) §5) y el servidor de la Fase 7 pasa bytes, no rutas. Seguridad,
  Low.
- **Arreglo propuesto:** en Unix, abrir con `OpenOptionsExt::custom_flags(O_NONBLOCK)` (el
  `open` de una FIFO no espera al escritor) y decidir solo con `is_file()` sobre el
  descriptor, sin la primera comprobación; en Windows basta lo de hoy. `O_NONBLOCK` vale
  distinto en Linux y macOS: tomarlo de `libc` o del sistema, sin `unsafe`.
- **Cuándo:** la próxima tarea que toque la CLI (tarea 5.4).

### 24. 4.3-I2: directorios temporales con nombre predecible en las pruebas de la CLI (opcional) — CERRADO: aceptado (tarea 5.4)

- **Cerrado (tarea 5.4), aceptado y escrito:** las pruebas de la CLI siguen con nombres
  `schemac-…-{pid}` en `temp_dir()` (también las nuevas de la 5.4), sin `tempfile`: solo
  corren en local, en la máquina de quien las lanza (D32, sin CI), y un enlace de otro
  usuario en un `/tmp` compartido solo haría fallar o escribir una prueba, no `schemac`. Lo
  dice el comentario de `dir_temporal` (`crates/cli/tests/es.rs`). Si algún día las pruebas
  corren en una máquina compartida, el arreglo de abajo sigue valiendo.

- **Dónde:** `crates/cli/tests/es.rs:30` (`dir_temporal`: `temp_dir()` +
  `schemac-es-{nombre}-{pid}` + `create_dir_all`), `crates/cli/tests/ir.rs:139`, `:155` y
  `:181`, `crates/cli/tests/check.rs:67`.
- **Estado:** los nombres son predecibles, así que en un `/tmp` compartido otro usuario
  podría adelantarse con un enlace simbólico. Solo afecta a pruebas locales. Seguridad,
  Info.
- **Arreglo propuesto:** usar `tempfile` (directorio con nombre aleatorio y permisos 0700)
  como dev-dependency, o aceptarlo y dejarlo escrito.
- **Cuándo:** opcional; la próxima tarea que toque las pruebas de la CLI (tarea 5.4).

### 25. 4.3-I3: `schemac ir /proc/self/mem` da un error de E/S, no «no es un archivo regular» (cosmético) — RESUELTO (tarea 5.4)

- **Resuelto (tarea 5.4):** se nombra el caso en [bajada.md](bajada.md) §5 («Entrada»):
  procfs declara sus archivos regulares, pasan las dos comprobaciones y la lectura falla
  con `Input/output error`, código 2 (comprobado tras el cambio a `O_NONBLOCK`, que no lo
  altera). El código no cambia.

- **Dónde:** `crates/cli/src/main.rs:152-170` (`leer`).
- **Estado:** procfs declara sus archivos como regulares, así que pasan las dos
  comprobaciones de `is_file()`; la lectura falla con un error de E/S y `schemac` termina
  con código 2 y ese mensaje. El código es el correcto; solo cambia el texto. Seguridad,
  Info.
- **Arreglo propuesto:** ninguno obligatorio; si se quiere, nombrar el caso en
  [bajada.md](bajada.md) §5.
- **Cuándo:** cosmético; cuando se toque `leer` (tarea 5.4).

### 26. 4.3-S1: la fila `minOccurs`/`maxOccurs` del alcance, columna «Soporte» (frontal XSD o punto de control B)

- **Dónde:** `docs/00-alcance.md:129` (§3.1).
- **Estado:** la celda «Cat.» ya dice cuándo la ocurrencia es (b), pero la columna
  «Soporte» sigue diciendo «Contador acotado en el marco de pila» sin matiz, y la celda
  «Cat.» solo nombra `?`, `*` y `+` como (a), no «una vez» ni `maxOccurs="0"`, que también
  lo son ([clasificacion.md](clasificacion.md) §3.2, decisión 1).
- **Arreglo propuesto:** «Soporte»: «Contador acotado en el marco de pila cuando hace
  falta (no se expande el modelo); el resto, estados del Glushkov»; y en «Cat.», añadir
  «una vez y `maxOccurs="0"`» a la lista de (a). Es un cambio del alcance (fuente de verdad
  3): lo revisa el usuario.
- **Cuándo:** **aprobado por el usuario en D29 (29 oct).** Lo aplica la **tarea 5.7** al
  tocar `00-alcance.md` (aplazada a después del hito de la semana 3 por D28). Como ya está
  aprobado, cualquier tarea anterior que lo necesite puede aplicarlo.

### 27. 4.3-S2: «en un solo marco» en `clasificacion.md` §4.2 supone que E₌ se expande (Fase 4)

- **Dónde:** [clasificacion.md](clasificacion.md) §4.2, primera frase.
- **Estado:** dice que la instancia de *v* la lee «en un solo marco, el producto» de sus
  reglas y de los destinos de E₌. Eso es así si la Fase 4 expande E₌; si compila `$ref` como
  llamada ε ([ir.md](ir.md) §7 deja abiertas las dos opciones), los registros del destino
  viven en el marco que apila la llamada. La clase y las estrategias del nodo no cambian.
- **Arreglo propuesto:** matizar: «(si E₌ se expande; con llamada ε, el marco del destino
  aporta los mismos registros y estrategias)».
- **Cuándo:** la Fase 4, al elegir entre expansión y llamada ε.

### 28. 4.3-S3: la prueba `ir_senala_el_nodo_c_y_su_estrategia` reserializa el esquema del caso — RESUELTO (tarea 5.4)

- **Resuelto (tarea 5.4):** la prueba recorta el texto original del valor `schema` con
  `texto_del_esquema` (`crates/cli/tests/ir.rs`), copia de la de
  `crates/semantic/tests/common/mod.rs` que usa `schema_parser`, dependencia normal de `cli`
  (las pruebas de integración la ven sin añadir nada); comprueba además que ese texto vuelve
  a dar el mismo valor que lee `serde_json`.

- **Dónde:** `crates/cli/tests/ir.rs:179` (`caso["schema"].to_string()`).
- **Estado:** lee el caso con `serde_json` y lo reescribe, que es lo que §18 quiso evitar
  (claves ordenadas, números por `f64`). No falla porque sus aserciones no dependen del
  orden de las claves ni de lexemas.
- **Arreglo propuesto:** recortar el texto original del valor `schema` como hace
  `crates/semantic/tests/common/mod.rs` (`texto_del_esquema`), añadiendo `schema-parser`
  como dev-dependency de `cli` o moviendo esa ayuda a un sitio común.
- **Cuándo:** la próxima tarea que toque la CLI (tarea 5.4).

### 29. 4.3-S4: `menciona_palabra_c` hace `unwrap` sin nombre de archivo — RESUELTO (tarea 5.4)

- **Resuelto (tarea 5.4):** `menciona_palabra_c(nombre, texto)` usa
  `unwrap_or_else(|e| panic!("{nombre}: {e}"))` (`crates/semantic/tests/corpus.rs`); la
  tarea añade el `corpus_clasificado` de XSD junto a ese corpus.

- **Dónde:** `crates/semantic/tests/corpus.rs:193` (`parse(texto).unwrap()`).
- **Estado:** hoy no puede fallar (los esquemas ya pasaron la Fase 3 antes de llegar
  aquí), pero si fallara el mensaje no diría qué archivo.
- **Arreglo propuesto:** pasar el nombre y usar `unwrap_or_else(|e| panic!("{nombre}: {e}"))`.
- **Cuándo:** la próxima tarea que toque el corpus de la Fase 3.

### 30. 4.3-S5: `write_json` y `write_dot` clasifican cada uno la IR (Fase 7)

- **Dónde:** `crates/ir/src/export.rs` (`write_json` y `write_dot` llaman a `classify`).
- **Estado:** cada exportación calcula la clasificación, O(n). Hoy `schemac ir` solo
  escribe un formato por invocación, así que no cuesta nada.
- **Arreglo propuesto:** si la Fase 7 quiere los dos formatos de una misma IR, recibir la
  `Classification` como parámetro (o una variante `write_*_with`).
- **Cuándo:** la Fase 7 (servidor).

### 31. 4.3-S6: `estrategias_de_la_tabla` repite el `match` del código (abierta a propósito)

- **Dónde:** `crates/ir/tests/classify.rs:215-235` (`estrategias_de_la_tabla`).
- **Estado:** la prueba repite casi literalmente la asignación de estrategias de
  `RuleKind::classification`: es en parte tautológica. Se deja porque el alcance no tiene
  columna de estrategia contra la que comparar, y la prueba protege de cambios
  accidentales en la tabla (la estrategia de cada regla sale del texto del alcance y de
  D5).
- **Arreglo propuesto:** ninguno; si el alcance añade una columna de estrategia, leerla
  como hace `tabla_coincide_con_el_alcance` con «Cat.».
- **Cuándo:** abierta a propósito.

### 32. 4.3-S7: cosméticos de `classify.rs` y `export.rs` (próxima tarea que toque `crates/ir`)

- **Dónde:** `crates/ir/src/classify.rs:256-265` (`NodeClass::join`), `Classification::node`
  y `crates/ir/src/export.rs:111` (`Rule::assignment` por regla).
- **Estado:** `join` compara a mano en lugar de `self.class.max(o.class)`;
  `Classification::node` hace pánico si se usa con otra IR (documentado en `# Panics`);
  la exportación recalcula `Rule::assignment` de cada regla, que `classify` ya calculó
  (O(n) en total, sin costo apreciable).
- **Arreglo propuesto:** usar `Ord::max`; si `Classification` se pasa entre fases, ligarla
  a su IR (por ejemplo, devolverla junto a la `VerifiedIr`); guardar las asignaciones por
  regla si algún día pesan.
- **Cuándo:** la próxima tarea que toque `crates/ir`.

### Cierre de la 5.3: observaciones de la revisión (4 nov)

### 33. Observaciones abiertas de la revisión de la 5.3 (frontal XSD)

Revisión de la tarea 5.3 (APROBADO CON OBSERVACIONES, D38: ninguna abre ronda). El
comportamiento es correcto salvo donde se dice; lo que falta es cobertura, una restricción de XSD o
pulido.

- **5.3-CR1-I2 (Important, cobertura; incluye 5.3-T1-2 del Tester 1).**
  - **Dónde:** `crates/xsd/src/check.rs:910-928` (EDC), corpus `tests/schemas/xsd/semantic-error`.
  - **Estado:** el único caso de EDC es `semantic-error/declaraciones-inconsistentes.xsd` (dos
    locales); no hay prueba de EDC a través de `ref` ni con tipos anónimos (dos anónimos son tipos
    distintos aunque sean iguales).
  - **Arreglo:** `semantic-error/edc-ref-y-local.xsd` y `semantic-error/edc-anonimos.xsd` en
    `errores-semanticos-xsd.json`, y un caso aceptado con el mismo `ref` repetido en
    `correspondencia_de_cada_construccion` (`crates/xsd/tests/lower.rs`).
  - **Destino:** tarea 5.7 (criterio en `tasks/todo.md`).
- **5.3-CR1-S1 (`ref` con `form`/`nillable` sin efecto).**
  - **Dónde:** `crates/xsd/src/parser.rs:480-533` (D39 los quita en el parser) y
    `crates/xsd/src/check.rs:783-812` (`particle_element`, rama de `ref`).
  - **Estado:** `<xs:element ref="g" nillable="false"/>` o con el `form` vigente se acepta, aunque
    XSD 1.0 src-element 2.2 prohíbe esos atributos con `ref`; antes de la 5.3 se rechazaban (como
    `unsupported`).
  - **Arreglo:** que el parser deje constancia de su presencia en un `xs:element` local y el
    análisis los rechace con `ref` (`syntax`, «con ref no admite …»).
  - **Destino:** tarea 5.7 (criterio en `tasks/todo.md`).
- **5.3-CR1-S2 (ocurrencias saturadas).**
  - **Dónde:** `crates/xsd/src/check.rs:555-574` (`occurs_value`) y `:845` (`check_model`).
  - **Estado:** `minOccurs` > `maxOccurs` no se detecta si los dos pasan de 2⁶⁴ (ambos saturan a
    `u64::MAX`; por ejemplo `minOccurs='18446744073709551617'`, `maxOccurs='18446744073709551616'`).
    Inocuo: un mínimo saturado ya es insatisfacible ([ir.md](ir.md) §5.1); documentado en
    [xsd.md](xsd.md) §3.
  - **Arreglo:** cuando los dos saturan, comparar los lexemas tras quitar el signo y los ceros
    iniciales (primero por longitud, después lexicográficamente).
  - **Destino:** la próxima tarea que toque `check.rs` (p. ej. 5.6).
- **5.3-CR1-S3 (`Debug` derivado de `Analysis`).**
  - **Dónde:** `crates/xsd/src/check.rs:42` (`Analysis`), `:68` (`TypeInfo`), `:86` (`Part`).
  - **Estado:** `#[derive(Debug)]` imprime el `&ComplexType` de cada tipo con sus anónimos: O(n·d)
    y recursivo (D36: solo en un hilo con `stack_size`). Hoy solo se usa en el `expect_err` de
    `crates/xsd/tests/lower.rs:69`, y solo si la prueba falla.
  - **Arreglo:** `Debug` manual con recuentos (tipos, globales, partículas) o quitar el derive.
  - **Destino:** la próxima tarea que toque `check.rs` (5.6) o la Fase 7 (API pública).
- **5.3-CR1-S4 (UPA en `xs:all`).** Ver §3: dos hijos de `xs:all` con el mismo nombre y tipo pasan
  EDC y `verify` y violan UPA; la máscara de `xs:all` de la Fase 4 depende de que se rechace.
  **Destino:** tarea 5.7 (criterio en `tasks/todo.md`).
- **5.3-CR1-S5 (legibilidad).**
  - **Dónde:** `crates/xsd/src/symbols.rs:89` (`Builtin::ALL[..12]` depende de que `AnySimpleType`
    sea el último) y `crates/xsd/src/check.rs:125-131` (`Errors::push`/`Errors::model` no dicen la
    etapa).
  - **Arreglo:** `const NAMED: [Builtin; 12]` (o un filtro); renombrar a `component`/`model` o
    `stage1`/`stage2`.
  - **Destino:** la próxima tarea que toque esos archivos.
- **5.3-CR1-S6 — RESUELTO (cierre de la 5.3):** [xsd.md](xsd.md) §8 ya no remite al reporte de la
  tarea, sino a D40 (y D39) del plan general, con su resumen.
- **5.3-T1-3 (Tester 1; copia de `pedido_xsd`).**
  - **Dónde:** `crates/xsd/tests/lower.rs` (`pedido_xsd`, copia de `crates/ir/tests/forma.rs`;
    lo declara [xsd.md](xsd.md) §7).
  - **Estado:** las dos copias pueden desincronizarse sin que falle ninguna prueba.
  - **Arreglo:** un módulo de prueba compartido (por ejemplo `tests/common/pedido.rs` incluido con
    `#[path]`) o una prueba que compare ambas.
  - **Destino:** la próxima tarea que toque `crates/ir/tests/forma.rs`, o la 5.7.

### Abiertas por la tarea 5.4 (`schemac` con XSD)

### 34. Ayudas de prueba duplicadas (opcional; la próxima tarea que toque esas pruebas)

- **Dónde:** `texto_del_esquema` está en `crates/cli/tests/ir.rs` (5.4, para §28) y en
  `crates/semantic/tests/common/mod.rs`; `casos_xml` está en `crates/xsd/tests/corpus.rs`
  (5.4) y en `crates/xsd/tests/lower.rs` (5.3), con la misma lectura del campo `schema`.
- **Estado:** son copias pequeñas y deliberadas: la 5.4 no reorganiza las pruebas de la 5.3
  ni crea un crate de ayudas de prueba solo para ellas. Pueden desincronizarse sin que falle
  ninguna prueba (como la copia de `pedido_xsd`, §33, 5.3-T1-3).
- **Arreglo propuesto:** un módulo `tests/common` en `crates/xsd` (incluido por `lower.rs` y
  `corpus.rs`) y, para `texto_del_esquema`, un módulo compartido con `#[path]` o una función
  de prueba en `schema-parser`.
- **Destino:** opcional; la próxima tarea que toque esas pruebas (la 5.6 tocará el corpus XSD).

### Cierre de la 5.4: observaciones de la revisión (4 nov)

### 35. Observaciones abiertas de la revisión de la 5.4 (`schemac` con XSD)

Revisión de la tarea 5.4 (APROBADO CON OBSERVACIONES, D38: ninguna abre ronda; decisiones de
la tarea, D41). 5.4-CR1-I1 (argumento de ρ en §1) se corrigió en el cierre de documentación.

- **5.4-CR1-S1 (cobertura; incluye 5.4-T1-3 del Tester).**
  - **Dónde:** `crates/xsd/tests/corpus.rs:33` (`ir_de`) y `:132-146` (`corpus_clasificado`,
    umbral `clasificados >= 7`).
  - **Estado:** `ir_de` devuelve `None` ante cualquier error del usuario, así que
    `corpus_clasificado` salta en silencio un XSD de `parse-ok` que deje de pasar el
    análisis; el umbral `>= 7` dejará de vigilarlo en cuanto se añadan archivos.
  - **Arreglo:** comprobar que los omitidos son exactamente `anotaciones`,
    `atributos-ajenos` y `tipos-simples`, y que su error es «pendiente (tarea 5.6)».
  - **Destino:** tarea 5.6 (ya en su «Arrastra» de `tasks/todo.md`).
- **5.4-CR1-S2 (cobertura).**
  - **Dónde:** `crates/cli/src/main.rs:567` (`fallo_interno_xsd_da_codigo_3`), con `ir_rota`
    (`:451`, `Format::Json`) y la rama `xsd::lower(&analysis).map_err(Fallo::Interno)` (`:412`).
  - **Estado:** `bajar_rota` ignora los bytes y devuelve la IR rota de `Format::Json`, así
    que la prueba no ejercita la rama de XSD de `bajar`; solo comprueba que un
    `Fallo::Interno` sobre un archivo XSD da 3.
  - **Arreglo:** una IR rota con `Format::Xml`, o comprobar que el `Fallo::Interno` de la
    rama XSD llega a `informe_interno` (`:431`).
  - **Destino:** la próxima tarea que toque la CLI.
- **5.4-CR1-S3 (seguridad menor).**
  - **Dónde:** `crates/cli/src/main.rs:215-226` (`abrir`, `custom_flags(libc::O_NONBLOCK)`).
  - **Estado:** en la carrera de §23 (la ruta se cambia por un dispositivo después de
    `fs::metadata`), abrir una tty sin `O_NOCTTY` puede convertirla en la terminal de control
    del proceso si no tiene una. Las rutas de la CLI son de confianza y `fs::metadata` filtra
    el caso normal ([bajada.md](bajada.md) §5).
  - **Arreglo:** `custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)` y decirlo en
    [bajada.md](bajada.md) §5.
  - **Destino:** la próxima tarea que toque la CLI, o la Fase 7 si el servidor reutiliza
    `abrir`.
- **5.4-CR1-S6 (`Makefile`).**
  - **Dónde:** `Makefile:107` (objetivo `doradas`, bucle `for f in tests/ir/xsd/*.json`).
  - **Estado:** si `tests/ir/xsd/` queda vacío, el glob se expande literal y `schemac` falla
    con `tests/schemas/xsd/parse-ok/*.xsd`; el bucle JSON tiene el mismo patrón.
  - **Arreglo:** `[ -e "$$f" ] || continue` al principio de los dos bucles.
  - **Destino:** la próxima tarea que toque el `Makefile`.
- **5.4-CR1-N1..N3 (nits de la CLI).**
  - **Dónde:** `crates/cli/src/main.rs:324` (N1), `:266-268` (N2), `:14-16` (N3).
  - **Estado:** N1, «1 tipos complejos» sin concordancia (igual que «1 subesquemas» y
    «1 entradas de $defs» de JSON); N2, en `ejecutar_check` el parámetro `analizar` tapa la
    función del mismo nombre; N3, tres líneas del texto de uso miden 81-83 columnas
    (preexistente).
  - **Arreglo:** singular/plural en los resúmenes de `check`; renombrar el parámetro (`f` o
    `analisis`); partir las líneas del uso a 80 columnas.
  - **Destino:** la próxima tarea que toque la CLI.
- **5.4-T1-2 (Tester; cosmético).**
  - **Dónde:** `crates/cli/src/main.rs` (`formato`) y [bajada.md](bajada.md) §5.
  - **Estado:** un archivo vacío, de solo espacios o de solo un BOM llamado `.xsd` da el
    error de JSON («se esperaba un esquema…»): es lo correcto por D22 (sin `<` no es XSD),
    pero ninguna prueba de la CLI fija ese mensaje.
  - **Arreglo:** una prueba de `check` con esos tres archivos que fije el código 1 y el
    mensaje de JSON.
  - **Destino:** la próxima tarea que toque la CLI (opcional).
