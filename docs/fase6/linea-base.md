# Fase 6 — Línea base DOM

Tarea F6.7 (pista B). Esta primera parte es la línea base **JSON** (F6.7a): `serde_json` construye el árbol del payload y `jsonschema` lo valida. La parte XML (libxml2, F6.7b) se añadirá como §7 detrás de la feature `libxml2` del mismo crate.

Implementación: [`crates/baseline`](../../crates/baseline) (`src/lib.rs`, `src/json.rs`, `src/corpus.rs`). Pruebas: `crates/baseline/tests/json.rs` (casos unitarios) y `crates/baseline/tests/corpus_json.rs` (el corpus). Pendientes: [pendientes.md](pendientes.md).

La línea base sirve para dos cosas:

- **Oráculo.** El intérprete de referencia del DPDA (F4.6) debe aceptar y rechazar el corpus igual que la línea base («Terminado cuando» de la Fase 4). La prueba del corpus garantiza que, en cada caso JSON con bytes, la línea base da `expected` (U8: el corpus es un oráculo certificado), salvo las divergencias de recuento declaradas.
- **Comparación de costo.** El arnés de métricas (F6.5) y el experimento de bombeo (E1) miden DPDA frente a línea base: MB/s, pico de memoria, asignaciones. Por eso compilar el esquema, leer el payload y validar el árbol se pueden llamar por separado (§2).

Vive en un crate propio y no en `bench` (U1): F4.6 la usa desde `runtime-rs` como dev-dependencia, y F6.5 hará que `bench` dependa de `runtime-rs`; en `bench` saldría el ciclo `runtime-rs` → `bench` → `runtime-rs`. Además, los ~65 crates de `jsonschema` solo entran en los crates que la usan.

## 1. DOM frente a streaming

| | DPDA del proyecto | Línea base DOM |
|---|---|---|
| Pasadas sobre el payload | una, en bloques de cualquier tamaño | dos: leer el árbol entero y después validarlo |
| Memoria | O(d) en pila (d = profundidad), marco de tamaño fijo; sin asignación dinámica en el camino de validación | el payload entero (`&[u8]`) más el árbol: O(n) con una constante grande |
| Tiempo de lectura | O(n) en bytes | O(n) (`serde_json`) |
| Tiempo de validación | O(n) con la misma pasada | depende del esquema: cada nodo del árbol se visita una vez por cada subesquema que se le aplica (`allOf`, `anyOf`, `oneOf` evalúan todas sus ramas), y `uniqueItems` compara los elementos entre sí sin el tope de 4 096 del proyecto |
| Primer error | en el byte donde se detecta, sin haber leído el resto | después de leer el payload entero (un error de esquema al principio no ahorra la lectura) |
| Entrada infinita o mayor que la RAM | se valida | no termina o agota la memoria |

**Costo en memoria del árbol.** Un `serde_json::Value` ocupa 32 bytes en 64 bits (lo fija `tamano_de_un_nodo_del_dom`), más lo que reserva fuera: el búfer de cada cadena, el de cada arreglo (con capacidad que crece al doble) y los nodos del `BTreeMap` de cada objeto (sin `preserve_order`). Estimación por tamaños, sin la cabecera del asignador (F6.5 lo medirá):

- `[0,0,0,…]`: 2 bytes de entrada por elemento → 32 bytes de nodo, hasta 64 con la capacidad sobrante del vector: **16× a 32×**.
- `[{"a":0},{"a":0},…]`: 8 bytes por elemento → 32 del nodo del objeto, una hoja de `BTreeMap<String, Value>` (11 claves de 24 bytes y 11 valores de 32: unos 630 bytes) y el búfer de la clave: **más de 80×**.

Por eso la línea base necesita un tope de tamaño en el arnés ([pendientes.md](pendientes.md) §3), y en E1 su memoria debe crecer con k mientras la del DPDA queda constante.

## 2. API (para F4.6 y F6.5)

API aprobada en U2 (`crates/baseline/src/lib.rs`, con documentación de cada elemento):

```rust
pub enum Kind { Malformed, Schema, Limit, Unsupported }
pub struct Violation { pub pointer: String, pub keyword: String, pub message: String }
pub struct Malformed { pub line: u32, pub column: u32, pub offset: Option<u64>, pub message: String }
pub struct Outcome { pub valid: bool, pub kind: Option<Kind>, pub violations: Vec<Violation>,
                     pub truncated: bool, pub malformed: Option<Malformed> }   // `truncated`: ronda 1
impl Outcome { pub fn violation_count(&self) -> usize }
pub struct Limits { pub max_depth: u32,             // 256 por defecto; se recorta a MAX_DEPTH_LIMIT
                    pub max_violations: u32 }       // 100 por defecto (ronda 1, §3.6)
// Construir con `Limits { max_depth: 16, ..Limits::default() }`.
pub const STACK_SIZE: usize = 8 << 20;  pub const MAX_DEPTH_LIMIT: u32 = 256;  pub const MAX_MESSAGE_BYTES: usize = 1024;

impl JsonBaseline {
    pub fn compile(schema: &serde_json::Value) -> Result<Self, CompileError>;
    pub fn compile_text(text: &[u8]) -> Result<Self, CompileError>;
    pub fn with_limits(self, limits: Limits) -> Self;
    pub fn parse(&self, stack: &Stack, payload: &[u8]) -> Result<JsonDom, Outcome>;   // 1.ª pasada
    pub fn validate_dom(&self, stack: &Stack, dom: &JsonDom) -> Outcome;               // 2.ª pasada
    pub fn validate(&self, stack: &Stack, payload: &[u8]) -> Outcome;                  // las dos
}
pub fn with_stack<R: Send>(f: impl FnOnce(&Stack) -> R + Send) -> R;

impl JsonDom { pub fn value(&self) -> &serde_json::Value }                            // !Send, !Sync

pub mod corpus {
    pub const CASES_DIR: &str;                           // tests/cases del repositorio
    pub fn casos() -> Result<Vec<Case>, CorpusError>;   // casos_en(CASES_DIR)
    pub fn casos_en(raiz: &Path) -> Result<Vec<Case>, CorpusError>;   // por ruta
    pub struct Case { id, path, category, format, schema_text, schema_value, payload, expected, strategy, limits }
    pub enum Format { Json, Xml }
    pub enum Payload { Bytes(Vec<u8>), Generator { name, params } }
    pub struct Expected { valid, violations, error_kind, pointer, offset }
    pub enum ErrorKind { Lexical, Syntax, Schema, Limit }   // ErrorKind::kind(self) -> Kind
    pub struct CorpusError { pub path: PathBuf, pub message: String }
    pub fn run_case(case: &Case, stack: &Stack) -> Result<Outcome, Unavailable>;      // también baseline::run_case
}
#[non_exhaustive]
pub enum Unavailable { Generator(String), XmlDisabled, CaseLimits, Compile(CompileError) }
```

Uso típico (F4.6):

```rust
let casos = baseline::corpus::casos()?;
baseline::with_stack(|s| {
    for c in &casos {
        match baseline::run_case(c, s) {
            Ok(base) => { /* comparar el intérprete con `base` */ }
            Err(baseline::Unavailable::Generator(_)) => { /* omitir */ }
            Err(e) => panic!("{}: {e}", c.id),
        }
    }
});
```

Detalles que conviene saber al usarla:

- **El testigo `Stack` (D7).** Leer el árbol y validarlo es recursivo. `parse`, `validate_dom`, `validate` y `run_case` piden un `&Stack`, que solo da `with_stack`: un hilo con `STACK_SIZE` = 8 MiB (como `schemac`). `Stack` no es `Send` ni `Sync`, así que no sale de ese hilo. Dentro de un solo `with_stack` se validan tantos payloads como se quiera: el tipo hace cumplir D7 sin un hilo por llamada.
- **La compilación crea su propio hilo.** `compile` y `compile_text` no reciben el testigo (U2), y compilar también es recursivo; así que cada una corre en un hilo con `STACK_SIZE`. Es un hilo por esquema, no por payload; F6.5 debe tenerlo en cuenta al medir la etapa de compilación ([pendientes.md](pendientes.md) §11).
- **`run_case`** compila el esquema desde su **texto original** (`schema_text`, recortado del archivo del caso con `schema_parser` por el rango del valor, D20), no desde `schema_value`. Devuelve `Err` si no puede dar un resultado: payload de un generador, caso XML sin la feature `libxml2`, caso con `limits` (su formato no está fijado, [pendientes.md](pendientes.md) §7) o esquema que no compila.
- **Mapeo con `expected`:** `ErrorKind::kind()` traduce `errorKind`: `lexical` y `syntax` son `Kind::Malformed` (la línea base no los distingue), `schema` es `Kind::Schema` y `limit` es `Kind::Limit`. `Kind::Unsupported` no tiene equivalente en el corpus: la línea base no sabe juzgar el payload, y F4.6 excluye esos casos (U9).
- `JsonBaseline` es `Send + Sync`: se compila una vez y se usa desde cualquier hilo de `with_stack`.
- **`JsonDom` no es `Send` ni `Sync` (CR-I2).** Su `Drop`, `Clone`, `PartialEq` y `Debug` (derivados de `serde_json::Value`) son recursivos. Con 256 objetos `{"a":…}` anidados, medido en `dev` (Windows, 13 nov) en hilos de pila fija: `drop` desborda con 256 KiB y pasa con 320 KiB; `==` y `{:?}` desbordan con 320 KiB y pasan con 384 KiB; `clone` desborda con 512 KiB y pasa con 768 KiB. Con arreglos se necesita bastante menos, así que la cifra de la primera versión («cabe en 256 KiB») solo valía para arreglos. Para que el tipo haga cumplir D7, `JsonDom` lleva un `PhantomData<*const ()>` como `Stack`: no puede salir del hilo de `with_stack` que lo leyó (`with_stack` exige `R: Send`), y todo recorrido suyo ocurre con 8 MiB de pila. Lo comprueban dos doc-tests `compile_fail` en `crates/baseline/src/json.rs` (pedir `Send` o `Sync` de `JsonDom` no compila) y `recorrer_un_dom_de_256_objetos_dentro_de_with_stack`. F6.5 mide el DOM dentro de `with_stack`, que es donde existe. Quien copie el `serde_json::Value` de `JsonDom::value()` y lo saque del hilo debe recorrerlo en otro hilo con pila holgada.
- **`Unavailable` es `#[non_exhaustive]`** (CR-S2): F6.7b o E1 pueden añadir motivos, así que un `match` fuera del crate necesita un brazo `_`, como el del ejemplo. `Limits` se construye con `..Limits::default()`; no se marcó `#[non_exhaustive]` porque eso prohibiría también esa forma fuera del crate.

## 3. Configuración segura

### 3.1 Dependencias

`serde`, `serde_json = "1.0.154"`, `jsonschema = { version = "0.61.2", default-features = false }`, `base64 = "0.23.1"` y `schema-parser` (path). Sin las features por defecto de `jsonschema` no entran `resolve-http` (reqwest, rustls, aws-lc), `resolve-file` ni `tls-*`: `cargo tree -p baseline` no contiene `reqwest`, `rustls`, `aws-lc`, `ring`, `tokio` ni `cc`, y todo el árbol es Rust (sin compilar C).

**Features de `serde_json` (U3).** Las features se unifican en todo el workspace (D20), así que afectan a todas las pruebas. `cargo tree -p baseline -e features -i serde_json` muestra solo `default`, `std`, `float_roundtrip` (la impone `jsonschema`: lectura de decimales con redondeo exacto) y `unbounded_depth` (solo añade `Deserializer::disable_recursion_limit`; la profundidad la acota el visitante, §3.3). **No** se activa `arbitrary_precision`: cambiaría la representación de los números para todo el workspace.

### 3.2 Compilación del esquema

`json.rs`, `construir`:

- **Draft 2020-12 fijo** (`jsonschema::draft202012::options()`), aunque `$schema` diga otro: es el único que acepta el proyecto (`00-alcance.md` §2.1).
- **Sin red ni archivos (B5).** Además de compilar sin `resolve-http` ni `resolve-file`, se instala un recuperador propio (`SinRed`) que falla siempre sin abrir nada: un `$ref` fuera del documento (`http://`, `https://`, `file://`) es un error de compilación con el mensaje «la línea base no recupera recursos externos (sin red ni archivos)». Defensa doble: aunque alguien active por error las features de red, la línea base sigue sin red. Lo prueba `ref_remoto_no_compila_y_no_usa_la_red`.
- **`format` como anotación** (`should_validate_formats(false)`), como el proyecto (`00-alcance.md` §2.4).
- **`pattern` sin retroceso.** `PatternOptions::regex()`: el crate `regex`, de tiempo lineal en la entrada, con su `size_limit` por defecto. El motor por defecto de `jsonschema` es `fancy-regex`, que admite *lookaround* y retrorreferencias con retroceso (ReDoS). Con `regex`, esos patrones no compilan, igual que en el proyecto. `pattern_con_lookahead_no_compila` lo comprueba con un control: el mismo esquema compila con el motor por defecto. `patron_con_retroceso_exponencial_termina` valida `^(a+)+$` contra 4 096 `a` y un `!`.
- **Profundidad del esquema.** `compile` comprueba con una pila explícita que el esquema no pasa de 256 contenedores anidados antes de dárselo a `jsonschema` (cuya compilación es recursiva); `compile_text` lo lee con el mismo visitante que los payloads, así que una clave duplicada en el esquema también es un error (D2), y un BOM inicial se ignora.

### 3.3 Lectura del payload: el visitante (U5)

`serde_json::from_slice::<Value>` no basta por dos motivos: acepta claves duplicadas (gana la última), que el proyecto rechaza como malformado (D2, `00-alcance.md` §1.1), y su tope de recursión es 128, no 256. `json.rs` construye el `Value` con un `DeserializeSeed` propio (`Semilla`) que:

- antes de leer el valor de cada miembro, comprueba si la clave ya está en el objeto; si está, falla con `Error::custom` («clave duplicada»). La misma clave en objetos distintos es válida;
- cuenta los contenedores abiertos (la raíz cuenta como 1): con el tope por defecto, 256 `[` anidados se aceptan y el `[` número 257 es `Kind::Limit`. El tope propio de `serde_json` se apaga (`disable_recursion_limit`), así que la única cota es la del visitante, y 10⁵ `[` se cortan en el 257 sin desbordar la pila;
- anota en una `Cell` por qué falló (`Motivo`), porque `serde_json` solo ve un `Error::custom`; así se distingue `Limit` de `Malformed`.

Al terminar el valor, `Deserializer::end` exige que no quede nada salvo espacios (`{} x` es malformado).

Números: sin `arbitrary_precision`, `serde_json` da `u64`, `i64` o `f64`. Un número que no cabe en `f64` (`1e400`, `-1e400`) es su error «number out of range», que la línea base clasifica como `Kind::Unsupported` (U9), no como `Malformed`: el payload es JSON válido (`00-alcance.md` §1.1) y es una carencia de la línea base.

### 3.4 Posiciones

`Malformed` lleva `offset` (bytes desde 0, la referencia, D9), `line` y `column` (desde 1; **columna en bytes**, no en puntos de código como el proyecto). Se calculan así:

- `serde_json` informa línea y columna del último byte consumido (B3): offset = inicio de la línea + columna − 1. Así coinciden los dos casos léxicos del corpus (`malformed-json-invalid-utf8-01`, offset 7, y `malformed-json-leading-zero-01`, offset 10).
- En un fin de entrada prematuro (`{"a":`, `[1, 2` o la entrada vacía), el offset es la longitud de la entrada (`fin_de_entrada_prematuro_tiene_el_offset_de_la_longitud`).
- Los errores del visitante salen donde `serde_json` termina de cerrar el contenedor que falló (`end_map`/`end_seq`), que puede estar unos bytes después: tras el byte culpable consume espacios, como mucho un `]`, `}` o `,`, y otra vez espacios. La línea base (`corregir`) deshace exactamente eso y acepta el resultado solo si es el byte esperado: la **comilla que cierra la clave repetida** (`{"a":1,"a":2}`, `{"a":1,"a"}` y `{"a":1,"a"   }` → 9) o el **`[`/`{` que pasa el tope** (257 `[` seguidos de `[`, `]`, `,1]` o ` , ]` → 256; 257 objetos `{"a":` → 1 280). Si no lo encuentra, deja la posición de `serde_json`. La primera versión solo retrocedía sobre espacios y un cierre, y fallaba con `{"a":1,"a"}` (10) y con `,` (257, 259) (CR-I1). Lo prueban:
  - `clave_duplicada_en_la_raiz_es_malformado_en_la_comilla_que_la_cierra`
  - `clave_duplicada_con_espacios_antes_de_los_dos_puntos`
  - `clave_duplicada_seguida_del_cierre_del_objeto`
  - `clave_duplicada_anidada`
  - `clave_duplicada_dentro_de_un_arreglo`
  - `clave_duplicada_escrita_con_escape`
  - `profundidad_257_es_limite_en_el_corchete_que_la_pasa`
  - `limite_de_profundidad_seguido_de_una_coma`
  - `profundidad_con_objetos_y_con_mezcla`
  - `cien_mil_corchetes_son_limite_sin_desbordar_la_pila`
- Línea: solo `\n` cambia de línea, como en `serde_json`; `\r\n` cuenta como un salto y un `\r` suelto no (`crlf_y_cr_suelto_en_linea_y_columna`).
- El BOM se quita antes de leer y sus 3 bytes se suman al offset (B4); línea y columna se cuentan sobre la entrada original, BOM incluido.
- Línea y columna son saturantes (`u32::MAX` = «`u32::MAX` o más», D9).

### 3.5 Pila (D7)

La lectura de `serde_json` con el visitante, la validación de `jsonschema` y su compilación corren en un hilo con `STACK_SIZE` = 8 MiB: `with_stack` o el hilo interno de `compile`. **Una excepción (CR1-I1):** el `Drop` de `JsonBaseline`. El validador de `jsonschema` es un árbol con la forma del esquema y se libera de forma recursiva, y como `JsonBaseline` es `Send + Sync` (U2), se libera en el hilo de quien lo tenga, a menudo el principal. Está acotado por `MAX_DEPTH_LIMIT`. Medido por el code-reviewer en `dev` con 256 contenedores anidados: `{"not": …}` y `unevaluatedItems` desbordan con 256 KiB y pasan con 384 KiB; el resto de palabras clave medidas pasan con 256 KiB (`items`, `additionalProperties`, `if`, `else`, `contains`, `propertyNames`; `allOf` y `properties` con 128 niveles). Cabe en el hilo principal de Windows (1 MiB) con un margen de unas 3 veces; si hiciera falta más, la solución está en [pendientes.md](pendientes.md) §19. La profundidad está acotada a 256 en payload y esquema (`MAX_DEPTH_LIMIT`; `Limits::max_depth` solo se puede bajar, como `MAX_DEPTH_LIMIT` del parser de esquemas, porque la recursión solo está medida hasta ahí). **B2:** con 8 MiB, un `$ref` recursivo a profundidad 256 valida y produce un error en la hoja en `dev` y en `release` (`ref_recursivo_a_profundidad_maxima`). Medido en `dev` (Windows, 13 nov) bajando `STACK_SIZE` a mano: las pruebas de profundidad 256 (ese `$ref` recursivo, 257 y 10⁵ `[`, el esquema de 300 `not`) pasan con 1 MiB y desbordan con 512 KiB, así que 8 MiB deja un margen de más de 8 veces. Un ciclo de `$ref` no productivo (que el proyecto rechaza, D12) tampoco desborda: `jsonschema` lo trata como `true` (`ref_ciclico_no_productivo_termina`).

### 3.6 Esquemas y payloads no confiables: lo que se acota y lo que no

La línea base se escribió para el corpus, que es de confianza. La auditoría de seguridad de la ronda 0 (SEC-M1, SEC-M2, SEC-L1) mostró tres formas de que un esquema o un payload hostil multiplique el costo. Desde la ronda 1 se acotan las dos que dependen de la línea base. La tercera no se puede acotar desde aquí.

- **La lista de violaciones (SEC-M2).** `items: {"type": "string"}` contra un arreglo de 10⁶ números daba 10⁶ `Violation`, cada una con tres `String`. Ahora `validate_dom` guarda como mucho `Limits::max_violations` (100 por defecto; `0` cuenta como 1) y marca `Outcome::truncated` si había más (`la_lista_de_violaciones_se_corta_en_max_violations`, con 10⁵ elementos). Con `truncated`, `violation_count()` es una cota inferior, y la prueba del corpus lo trata como fallo, porque el recuento ya no es comparable.
- **Los mensajes (SEC-M1, escenario 2, y SEC-L1).** Los mensajes de `jsonschema` copian la instancia, y los de compilación copian el esquema. Antes, `allOf` de 2 000 `{"not": {}}` sobre un payload de 220 KB daba 440 MB de mensajes, y un `pattern` inválido de 10 MB, un mensaje de 10 MB. Ahora cada `Violation::message` y cada `CompileError::message` se escriben en un búfer que se corta a `MAX_MESSAGE_BYTES` = 1 KiB y termina en `…` (`acotado` en `json.rs`; `el_mensaje_de_una_violacion_tiene_tope` con 200 KB, `el_mensaje_de_compile_error_tiene_tope` con un patrón de 10⁶ bytes). Si el `Display` de `jsonschema` escribe por partes, el corte también ahorra el resto del formateo; si construye antes el texto entero, solo ahorra la copia guardada.
- **El trabajo de `jsonschema`: no se acota (SEC-M1, escenario 1).** `iter_errors` reúne **todos** los errores en un vector interno antes de entregar el primero, así que `take(max_violations)` acota lo que se guarda, no lo que se calcula. Un esquema de ~2 KB con `$ref` en abanico obliga a evaluar 2^k ramas. Por ejemplo, `$defs` `d0…d24`, cada `dI` = `{"allOf": [{"$ref": "#/$defs/dI+1"}, {"$ref": "#/$defs/dI+1"}]}`, contra el payload `1`: según el auditor, 42 s y 2²⁴ errores internos; con k = 30, minutos y decenas de GB. La profundidad (256) no lo impide, porque el abanico es ancho, no hondo. El proyecto no tiene este problema: la Fase 3 acota el producto de autómatas antes de construirlo ([../fase3/pendientes.md](../fase3/pendientes.md) §5). **Consecuencia (bloqueante para F7):** `JsonBaseline` no debe ejecutar esquemas ni payloads de usuario sin un límite **externo** de CPU y memoria (proceso aparte o sandbox con tiempo máximo). Si el servidor llega a exponer la línea base, conviene `Validator::is_valid`, que no reúne errores y corta cada `allOf` en su primera rama falsa (un `anyOf` en abanico, en cambio, sigue siendo exponencial) ([pendientes.md](pendientes.md) §14).

## 4. Divergencias con el DPDA y el alcance

Lo que la línea base hace distinto del proyecto y por qué. «Coincide» indica dónde se puso trabajo para que **no** diverja. Cada fila nombra la prueba de `crates/baseline/tests/json.rs` que la fija, salvo la 7, que fijan las pruebas de posiciones de §3.4.

| # | Tema | Proyecto (`00-alcance.md`, DPDA) | Línea base | Motivo / consecuencia |
|---|---|---|---|---|
| 1 | Claves duplicadas | malformado (D2, §1.1) | **coincide**: `Malformed` en la comilla que cierra la clave repetida | `serde_json` solo se queda con la última; lo arregla el visitante (U5; `clave_duplicada_*`). El offset del DPDA puede ser otro byte: [pendientes.md](pendientes.md) §8 |
| 2 | Número fuera del rango de `f64` (`1e400`) | válido; se compara el lexema; > 1 024 bytes de lexema es `limit` (§4) | `Kind::Unsupported` | sin `arbitrary_precision` (U3) no se representa; F4.6 excluye el caso (U9; `numero_que_no_cabe_en_f64_no_esta_soportado`). En un **esquema** (`{"maximum": 1e400}`) es `CompileError`, y `run_case` da `Unavailable::Compile`, aunque el proyecto lo acepte ([pendientes.md](pendientes.md) §9) |
| 3 | Enteros mayores que 2⁶⁴ (o menores que −2⁶³) | comparación exacta del lexema | pasan por `f64`: `18446744073709551617` ≤ `maximum: 18446744073709551616` es válido | hasta `u64::MAX` la comparación de `jsonschema` es exacta (`numeros_mas_alla_de_f64`); por encima, divergencia declarada en F4.6 ([pendientes.md](pendientes.md) §9) |
| 4 | Decimales en `minimum`, `maximum`, `multipleOf` | aritmética decimal exacta | `f64` con `float_roundtrip`; `0.3` es múltiplo de `0.1` y `1.0` es entero, como en el proyecto | diverge con decimales de más de 17 cifras significativas: `0.10000000000000000001` ≤ `maximum: 0.1` es válido (`decimal_largo_se_redondea_a_f64`) |
| 5 | BOM inicial | se ignora (§1.1) | **coincide**: se quita y los offsets cuentan sus 3 bytes | `serde_json` lo rechaza; se quita antes (B4; `bom_inicial_se_ignora_y_los_offsets_cuentan_sus_3_bytes`). Un prefijo incompleto (`EF`, `EF BB`, `EF BB {}`) no es BOM: `Malformed` en el offset 0, como D10 (`bom_incompleto_es_malformado_en_el_offset_0`) |
| 6 | Profundidad | 256 por defecto (§4) | **coincide**: 257 contenedores es `Kind::Limit`, en el `[`/`{` que pasa el tope | `serde_json` corta en 128; se apaga y cuenta el visitante (U3; `profundidad_*`, `limite_de_profundidad_seguido_de_una_coma`) |
| 7 | Columnas | puntos de código | bytes | `serde_json` cuenta bytes; el `offset` (la referencia, D9) sí es comparable |
| 8 | Recuento de violaciones (`violations`) | peso del recorrido en el semianillo tropical (D4; lo fija F4.2) | número de errores de `iter_errors` (U7): `required`, uno por clave que falta; `additionalProperties`, uno por palabra clave aunque sobren varias; `type` + `enum`, dos; `allOf` suma los de sus ramas; `anyOf`, `oneOf` y `not`, uno; `items`, uno por elemento | coincide en todo el corpus de hoy (`DIVERGENCIAS` vacía); `recuento_de_violaciones_de_jsonschema` lo fija y [pendientes.md](pendientes.md) §5 lo revisa cuando F4.2 fije los pesos |
| 9 | Orden de los errores | `pointer` del **primer** error por posición | en el orden en que `jsonschema` evalúa el esquema (B10) | con `properties` `b` y `a` y el payload `{"b": 1, "a": 2}` da `/a` antes que `/b` (`orden_de_los_errores_es_el_del_esquema`); la prueba del corpus exige que `pointer` esté **entre** los punteros de la línea base, no que sea el primero |
| 10 | `lexical` frente a `syntax` | dos clases | una: `Kind::Malformed` | `serde_json` no distingue léxico de sintáctico; el offset sí se compara (`lexico_y_sintactico_son_la_misma_clase`) |
| 11 | `format` | anotación (§2.4) | **coincide**: no se valida | `should_validate_formats(false)` (`format_email_no_se_valida`) |
| 12 | Escape `\ud800` sin su pareja | sin especificar (§1.1 solo habla de sustitutos codificados en UTF-8) | `Malformed` en la comilla que sigue | comparar con el intérprete en F4.6 ([pendientes.md](pendientes.md) §1; `escape_de_sustituto_suelto_es_malformado`) |
| 13 | Clases de `pattern` (`\d`, `\w`, `\s`, `.`) | subconjunto de ECMA-262 compilado a AFD; `\d` ASCII | `jsonschema` traduce ECMA-262 al crate `regex`: `\d` es ASCII (`d_en_pattern_es_ascii`) | `\w`, `\s` y `.` sobre caracteres no ASCII quedan por comparar ([pendientes.md](pendientes.md) §1) |
| 14 | Palabras clave fuera del subconjunto (`if`/`then`/`else`, `unevaluatedProperties`, `$dynamicRef`…) | error de la Fase 3 (D6) | se compilan y se aplican | la línea base da un veredicto sobre esquemas que `schemac` no compila; F4.6 y F6.5 solo comparan esquemas que pasan `schemac check` ([pendientes.md](pendientes.md) §10; `palabras_clave_fuera_del_subconjunto_se_aceptan`) |
| 15 | Palabras clave desconocidas (`"foo": 1`) | error de la Fase 3 (D6) | se ignoran | ídem (misma prueba) |
| 16 | Ciclos de `$ref` no productivos | error al compilar (D12) | el ciclo vale `true` | ídem; no desborda (`ref_ciclico_no_productivo_termina`) |
| 17 | `$schema` de otro draft | error (§2.1) | se fuerza 2020-12 | ídem; el control de `palabras_clave_fuera_del_subconjunto_se_aceptan` muestra que sin forzarlo `prefixItems` se ignoraría |
| 18 | `$ref` remoto (`http://`, `file://`) | error (§2.1) | **coincide**: error de compilación, sin red | B5 (`ref_remoto_no_compila_y_no_usa_la_red`). **Excepción** (CR-S5): un `$ref` a un meta-esquema oficial (`{"$ref": "https://json-schema.org/draft/2020-12/schema"}`) **compila**, también sin red, porque `jsonschema` trae los meta-esquemas incluidos; el proyecto lo rechaza (§2.1). Va con las filas 14-17 ([pendientes.md](pendientes.md) §10) |
| 19 | *Lookahead*, retrorreferencias | rechazados en `pattern` (§2.4) | **coincide**: no compilan | motor `regex` (`pattern_con_lookahead_no_compila`) |
| 20 | Cadena de más de 1 MiB | `limit` (§4) | válida (una cadena de 2 MiB) | la línea base no aplica el tope (CR-I3; `limites_del_alcance_que_la_linea_base_no_aplica`, también las filas 21-22). Un caso `limit` de esta clase haría fallar la prueba del corpus, que compara siempre la clase (U7): [pendientes.md](pendientes.md) §13 |
| 21 | Lexema numérico de más de 1 024 bytes | `limit` (§4) | válido si cabe en `f64` (`0.000…01` de 2 003 bytes); `Unsupported` si no (2 000 nueves) | `serde_json` sin `arbitrary_precision` no deja ver el lexema; ídem §13 |
| 22 | Más de 1 024 claves no declaradas en un objeto | `limit` (§4, detección de duplicados) | válido (2 000 claves); detecta duplicados en cualquier objeto sin tope, con un `BTreeMap` | ídem §13 |
| 23 | Tiempo máximo de validación | 10 s en la aplicación web (§4) | sin tope | la línea base no se expone en la web (§3.6, [pendientes.md](pendientes.md) §14) |

## 5. Pruebas

`cargo test -p baseline` (dentro de `make test`):

- **`tests/corpus_json.rs`**, `la_linea_base_recorre_el_corpus_json`: para cada caso JSON con bytes de `tests/cases`, `valid` igual; la clase de `errorKind` igual (`lexical`/`syntax` → `Malformed`, `schema` → `Schema`, `limit` → `Limit`); `violations` igual salvo las entradas de la tabla `DIVERGENCIAS` (id, campo, motivo), y una entrada que ya no diverge también falla (U7; lo prueba `una_divergencia_que_ya_no_diverge_es_un_fallo`); el `pointer` esperado entre los de la línea base; el `offset` de los errores léxicos y sintácticos exacto. Exige que hayan corrido al menos 8 casos e informa y omite los de `generator` (B6). Los mensajes de fallo dan archivo, campo, esperado y obtenido (B11). Un resultado con `truncated` también es un fallo. Además: `sin_libxml2_un_caso_xml_no_esta_disponible` y `el_esquema_de_un_caso_es_su_texto_original` (D20). Las rutas de error de `corpus.rs` se prueban con `casos_en` sobre una carpeta temporal (`corpus_temporal`):
  - `payload_base64_mal_formado_es_un_error_del_corpus`
  - `rutas_de_archivo_fuera_de_la_carpeta_del_caso_se_rechazan`: `..`, absolutas, `C:`, `./`, vacía, `:` y `\`, en `schemaFile` y en `payloadFile`
  - `rutas_de_archivo_en_una_subcarpeta_se_leen`
  - `run_case_sin_resultado_por_generador_limites_o_esquema`: `Generator` con sus `params`, `CaseLimits` y `Compile`
- **`tests/json.rs`**, todas dentro de `with_stack` (D7) y sin medir tiempo (D1):
  - claves duplicadas: raíz, anidada, en un arreglo, con espacios, seguida del `}` y escrita con escape; la misma clave en objetos distintos es válida;
  - profundidad: 256 válido; 257 es `Limit` con arreglos, objetos, mezcla y seguido de `,`; 10⁵ `[` sin desbordar; tope configurable solo a la baja; `$ref` recursivo a profundidad 256;
  - `JsonDom` de 256 objetos recorrido dentro de `with_stack`;
  - BOM completo e incompleto; `1e400`; basura al final; fin de entrada prematuro; UTF-8 inválido en valor y en clave; línea y columna en bytes, con `\r\n` y `\r`;
  - `$ref` remoto y al meta-esquema; *lookahead*; ReDoS; `format`; esquemas demasiado profundos;
  - los topes de violaciones y de mensajes (§3.6);
  - las etapas por separado y las filas de la tabla de divergencias.

Los doc-tests de `crates/baseline/src/json.rs` comprueban que `JsonDom` no es `Send` ni `Sync`. Rustdoc estable no verifica el código `E0277`, pero el doc-test falla si el tipo pasa a ser `Send` (comprobado a mano).

Hoy la tabla `DIVERGENCIAS` está vacía: los 8 casos JSON del corpus dan exactamente `expected`.

## 6. Pendientes

Los abiertos están en [pendientes.md](pendientes.md) («Vigilar más adelante»), con la tarea que los cierra: divergencias de escapes y clases de `pattern` (F4.6), memoria de libxml2 (F6.5), tope de tamaño del payload (F6.5, E1), formato de `limits` en los casos, offsets de duplicadas y profundidad frente al DPDA, números más allá de `f64` y esquemas que solo acepta la línea base (F4.6).
