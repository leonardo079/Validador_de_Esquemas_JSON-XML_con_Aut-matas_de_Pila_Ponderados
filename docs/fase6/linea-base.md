# Fase 6 — Línea base DOM

Tarea F6.7 (pista B), en dos partes. La línea base **JSON** (F6.7a, §1-§6): `serde_json` construye el árbol del payload y `jsonschema` lo valida. La línea base **XML** (F6.7b, §7): libxml2 construye el árbol y lo valida contra el XSD, detrás de la feature `libxml2` del mismo crate, apagada por defecto y probada en Docker.

Implementación: [`crates/baseline`](../../crates/baseline) (`src/lib.rs`, `src/json.rs`, `src/corpus.rs` y, con la feature, `src/xml.rs`). Pruebas: `crates/baseline/tests/json.rs` y `tests/xml.rs` (casos unitarios), `tests/corpus_json.rs` y `tests/corpus_xml.rs` (el corpus). Las de XML solo se compilan con la feature (§7.3). Pendientes: [pendientes.md](pendientes.md).

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

// Solo con la feature `libxml2` (F6.7b, §7). Ninguno de los dos es `Send`: se crean y se usan en el hilo de `with_stack`.
impl XmlBaseline {
    pub fn compile(stack: &Stack, xsd: &[u8]) -> Result<Self, CompileError>;      // pide el testigo (D7)
    pub fn with_limits(self, limits: Limits) -> Self;
    pub fn parse(&self, stack: &Stack, payload: &[u8]) -> Result<XmlDom, Outcome>;
    pub fn validate_dom(&self, stack: &Stack, dom: &XmlDom) -> Outcome;
    pub fn validate(&self, stack: &Stack, payload: &[u8]) -> Outcome;
}
pub struct XmlDom { .. }                                // sin acceso al documento de libxml2 (B34, §7.1)

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

Hoy la tabla `DIVERGENCIAS` está vacía: los 8 casos JSON del corpus dan exactamente `expected`. Las pruebas de la línea base XML están en §7.6.

## 6. Pendientes

Los abiertos están en [pendientes.md](pendientes.md) («Vigilar más adelante»), con la tarea que los cierra: divergencias de escapes y clases de `pattern` (F4.6), memoria de libxml2 (F6.5), tope de tamaño del payload (F6.5, E1), formato de `limits` en los casos, offsets de duplicadas y profundidad frente al DPDA, números más allá de `f64` y esquemas que solo acepta la línea base (F4.6). Los de la línea base XML, en §7.7.

## 7. Línea base XML (libxml2, F6.7b)

libxml2 construye el árbol del payload (DOM) y lo valida contra un XSD 1.0, en dos pasadas, como la línea base JSON. Vive en `crates/baseline/src/xml.rs` y solo se compila con la feature **`libxml2`**, apagada por defecto (U4): el `build.rs` del crate `libxml` exige libxml2, `pkg-config` y libclang (genera los *bindings* con bindgen) y hace `panic!` si no los encuentra. Sin la feature, `cargo build --workspace --all-targets` y `cargo test --workspace` no piden nada de eso (comprobado en la máquina Windows, sin libxml2), y `run_case` devuelve `Err(Unavailable::XmlDisabled)` para los casos XML (`sin_libxml2_un_caso_xml_no_esta_disponible`).

### 7.1 API

La de §2 (`XmlBaseline`, `XmlDom`), con una diferencia respecto a `JsonBaseline`: **`compile` pide el testigo `Stack`**. Los contextos de libxml2 son punteros de C, así que `XmlBaseline` y `XmlDom` no son `Send` ni `Sync` (U2: «se crea dentro del hilo»), y `compile` no puede compilar en un hilo propio y devolver el resultado, como hace `JsonBaseline::compile`. Pedir el testigo hace que el tipo obligue a compilar, validar y liberar dentro del hilo de `with_stack` (D7), igual que `parse` y `validate_dom` (B27). Los doc-tests `compile_fail` de `xml.rs` comprueban que pedir `Send` de `XmlBaseline` o de `XmlDom`, o `Sync` de `XmlDom`, no compila. `run_case` compila el XSD del caso con el testigo que recibe.

`XmlDom` **no da acceso** al documento de libxml2: la primera versión tenía `XmlDom::document()`, que exponía un tipo del crate `libxml` en la API y permitía llamar a libxml2 sin el cerrojo de §7.2 (CR-S3). Nada lo usaba y se quitó (B34). B27 y esta forma de `XmlDom` entran en la confirmación de U2 con Leonardo (S1).

Dependencia: `libxml = { version = "=0.3.23", optional = true }` (MSRV 1.88). Versión exacta, porque el crate escribe `unsafe` sobre libxml2 y un cambio de versión merece revisarlo (B28). `Cargo.lock` gana los paquetes de `libxml` y de su `build.rs` (`bindgen`, `clang-sys`, `pkg-config`, `vcpkg`…), que solo se compilan con la feature. `unsafe_code = "deny"` es por crate: no afecta al `unsafe` interno de `libxml`, y `xml.rs` no tiene `unsafe`.

**Unificación de features (CR-S2, para F4.6).** Las features de Cargo se unifican en todo el workspace: si un crate del workspace pone `baseline = { workspace = true, features = ["libxml2"] }` en sus dependencias (también en `dev-dependencies`), `cargo test --workspace` compila `libxml` en todas las máquinas y se rompe en Windows. F4.6 debe activarla con una feature de paso, apagada por defecto, en `runtime-rs`: `[features] baseline-xml = ["baseline/libxml2"]`, y correr la comparación XML con `cargo test -p runtime-rs --features baseline-xml` dentro de la imagen de §7.3.

### 7.2 Configuración segura

El XSD y el payload son entrada no confiable.

- **Opciones del parser** (`opciones` en `xml.rs`), escritas campo a campo en lugar de partir de `ParserOptions::default()`, que trae `recover: true` y `no_net: false`: `recover: false` (un documento mal formado no da árbol), `no_net: true`, `huge: false` (libxml2 conserva sus límites, §7.4: entre ellos, un nodo de texto de 10 MB o más, con un umbral de 10 000 000 bytes de lookup que 9 999 999 bytes de texto ya alcanzan; un texto hecho solo de referencias no lo alcanza; TR1-O2), `no_error`/`no_warning: true`, y `encoding: Some("UTF-8")` con `ignore_enc: true` (siguiente punto). `NOENT`, `DTDLOAD` y `DTDVALID` no se activan (el crate `libxml` ni siquiera los expone). Se lee con `Parser::parse_string_with_diagnostics`, que recoge los diagnósticos como `StructuredError` con un manejador por hilo (como mucho 100 por lectura) y no escribe en stderr (B8). En la ejecución de §7.3 el registro no contiene ninguna línea de libxml2 («parser error», «Schemas validity error»): libxml2 escribe directamente en el descriptor 2, que la captura de `cargo test` no tapa, así que se vería.
- **Solo UTF-8 (SEC-M1, CR-I1, O1; B35).** Como `00-alcance.md` §1.2: libxml2 lee siempre UTF-8 (`encoding: Some("UTF-8")`, `ignore_enc: true`), así que no convierte nada ni llama a iconv (UTF-7, ISO-2022, EBCDIC… son superficie innecesaria). Antes de llamarlo, `politica_de_entrada` da `Kind::Malformed`:
  - en el **offset 0**, si la entrada empieza como UTF-16 o UTF-32 (BOM `FF FE`, `FE FF`, `00 00 FE FF`) o con `<` en UTF-16/UTF-32 (`3C 00`, `00 3C`) o `<?xm` en EBCDIC (`4C 6F A7 94`) (XML 1.0, apéndice F). Lo que la tabla no reconoce lo corta libxml2, que lee UTF-8, también como `Malformed` pero en el offset donde lo detecta: UTF-16LE sin BOM con un espacio delante, en el 1; UTF-32BE sin BOM (`00 00 00 3C`), en el 0, con «Document is empty» (CR1-S3; [pendientes.md](pendientes.md) §27);
  - en el **offset del valor**, si la declaración `<?xml … encoding="X"?>` del principio dice algo distinto de `UTF-8` (sin distinguir mayúsculas: `utf-8` vale). Sin esa comprobación, con `ignore_enc` un `encoding="ISO-8859-1"` se leería como UTF-8 en lugar de rechazarse.

  Bytes de otra codificación sin declarar (Latin-1) los rechaza libxml2 como UTF-8 inválido. Con esto, todo lo que ve libxml2 es UTF-8 y los recorridos de bytes propios (`doctype_en_el_prologo`, `profundidad_excedida`) son completos. Antes no lo eran: en UTF-16 con BOM, `<a>` + 255 `<b/>` + `</a>` daba un `Limit` falso, porque cada `<` contaba como un nivel abierto. Lo prueban `entradas_que_no_son_utf8_son_malformado` (UTF-16LE con y sin BOM, UTF-16BE, UTF-32, EBCDIC, UTF-7, ISO-8859-1 y `utf-16` declarados, con 300 elementos y con una DTD), `utf8_declarado_en_cualquier_caja_se_acepta`, `latin1_sin_declarar_es_malformado` y `xsd_que_no_es_utf8_no_compila`.
- **Política D8 en dos capas (U6).** Primera capa: antes de dar los bytes a libxml2, `doctype_en_el_prologo` recorre el prólogo (BOM, espacios, `<?…?>` incluida la declaración XML, comentarios) y un `<!DOCTYPE`, en mayúsculas o minúsculas, es `Kind::Malformed` en el offset de la `D`, como el error `DoctypeForbidden` del lexer del proyecto. Así libxml2 no ve la DTD: ni interna, ni externa (`SYSTEM`, `PUBLIC`), ni entidades generales o de parámetro. El *billion laughs* del corpus da `Malformed` en el offset 24, igual que `expected`. Un `<!DOCTYPE` dentro de un comentario no cuenta, y uno después de la raíz ya es un error de libxml2. Solo se aceptan las cinco entidades predefinidas y las de carácter; `&lol;` sin DTD es un error de buena formación de libxml2.

  **Segunda capa (CR-I3, O2):** si el árbol que devuelve libxml2 tiene un nodo DTD (`tiene_dtd`), es `Malformed` en el **offset 0**, antes de mirar errores, profundidad o el esquema. No es inalcanzable: hasta la política UTF-8, un payload o un XSD en UTF-16 o UTF-7 con DOCTYPE esquivaba la primera capa, y la segunda era lo único que impedía validar un árbol con una DTD y sus entidades (los auditores lo midieron, sin fuga de archivos). Hoy la política de codificación corta antes esos casos, así que la segunda capa queda como defensa en profundidad **probada**: `la_segunda_capa_de_d8_rechaza_un_arbol_con_dtd` (prueba unitaria de `xml.rs`) llama a `leer_con_libxml2` sin la primera capa con una entidad externa a `file:///etc/passwd`, una DTD interna, una externa y una DTD seguida de 300 elementos con un tope de profundidad de 1, y exige `Malformed` en el offset 0 con «DTD (D8)» y sin `root:` en la salida.
- **Ningún archivo local puede aparecer en la salida.** Con la entidad externa `SYSTEM "file:///etc/passwd"` el resultado es `Malformed` en la `D`, y ni el `Outcome` ni sus mensajes contienen `root:` (`entidad_externa_es_malformado_sin_leer_el_archivo`, `dtd_externa_y_parametros_son_malformado`; en UTF-16, UTF-32 y UTF-7, `entradas_que_no_son_utf8_son_malformado`). Un `xsi:noNamespaceSchemaLocation` que apunta a `file:///etc/passwd` en el payload no se carga: el resultado es válido y sin rastro del archivo (`xsi_schema_location_del_payload_se_ignora`; libxml2 solo usa esas indicaciones cuando valida sin esquema).
- **El XSD.** Se lee con las mismas opciones y la misma política (codificación y D8: un `<!DOCTYPE` en el esquema es error de compilación, `xsd_con_doctype_no_compila`). Después se rechazan **`xs:import`, `xs:include`, `xs:redefine` y `xs:override`**, con cualquier prefijo o sin prefijo y estén donde estén, antes de llamar al compilador de esquemas (`carga_otro_documento`, iterativo): libxml2 los leería de la red o del disco, y además con `XML_PARSE_NOENT` (`SCHEMAS_PARSE_OPTIONS` de `xmlschemas.c`). Un `xs:import` dentro de `xs:appinfo` también se rechaza, aunque libxml2 no lo cargaría: es más estricto de lo necesario. Por lo mismo no se usa `SchemaParserContext::from_buffer`, que lee el XSD con esas opciones, sino `from_document` sobre el árbol ya leído con las seguras (B29). El proyecto también los rechaza (`00-alcance.md` §3.3). Lo prueban `xs_import_e_include_no_compilan_y_no_usan_la_red_ni_el_disco` (`http://127.0.0.1:9/…`, `file:///etc/passwd`, `xs:override`, `xs:appinfo`) y `xsd_sin_prefijo_compila_y_su_import_se_rechaza`. Un XSD de más de 256 niveles es un error de compilación que dice «el XSD supera un límite», no «no es XML bien formado» (CR-S7, `xsd_demasiado_profundo_es_un_limite_no_un_mal_formado`).
- **Sin red, comprobado:** las pruebas corren en un contenedor con `--network none` (§7.3), así que una carga por red fallaría en lugar de pasar inadvertida.
- **Estado global serializado (B30, CR-S4, SEC-L2).** libxml2 tiene estado global (inicialización, manejadores de error, tipos predefinidos de XSD). Un cerrojo estático (`LIBXML2`) lo toman `compile`, `parse`, `validate_dom` y, desde la ronda 1, también los `Drop` de `XmlBaseline` y `XmlDom`, que liberan el esquema y el árbol (`xmlSchemaFree*`, `xmlFreeDoc`) con el cerrojo tomado (B36: el recurso va en un `Option` que `drop` vacía bajo el cerrojo, sin `unsafe`). Así las llamadas desde varios hilos (pruebas en paralelo, F4.6, F6.5) no se cruzan. La libxml2 de Debian está compilada con hilos y el crate `libxml` ya protege la inicialización y los tipos de XSD (`OnceLock`), así que el cerrojo es una precaución, no la corrección de un fallo medido. Cumple B8 en la propia biblioteca: las pruebas no necesitan un cerrojo propio. En un servidor, un cerrojo global serializaría todas las peticiones ([pendientes.md](pendientes.md) §14).
- **Topes de la salida**, como en JSON. Cada mensaje se recorta a `MAX_MESSAGE_BYTES` = 1 KiB: libxml2 ya acorta los valores que copia, pero copia entera la lista de una enumeración, y con 2 000 valores el mensaje se corta (`el_mensaje_de_una_violacion_tiene_tope`). La lista de violaciones se corta en `Limits::max_violations`, con `truncated` (`la_lista_de_violaciones_se_corta_en_max_violations`, con 1 000 errores). El crate `libxml` reúne **todos** los errores de validación en un vector antes de que la línea base los corte ([pendientes.md](pendientes.md) §21).
- **Tope de diagnósticos del parser (SEC-L1).** El crate guarda como mucho 100 diagnósticos por lectura. Si libxml2 devolviera un árbol con 100 avisos antes de un error de espacios de nombres, el error quedaría fuera y el documento se aceptaría. No pasa: con `no_warning`, libxml2 no entrega los avisos (150 espacios de nombres con URI relativa no dan ninguno), así que el tope solo lo llenan errores y el primero siempre se ve (`muchos_avisos_no_ocultan_un_error_de_espacios_de_nombres`). Por si cambiara, un árbol con 100 diagnósticos o más y ningún error es `Malformed` (B37).
- **Pila (D7).** Todo corre en el hilo de `with_stack`. La profundidad del payload y la del XSD están acotadas a 256 (§7.4), y los recorridos propios (`profundidad_excedida`, `carga_otro_documento`, `tiene_dtd`) son iterativos: 10⁵ niveles de anidamiento dan `Limit` sin caída.

### 7.3 Cómo ejecutarlo

```sh
make test-baseline-xml                                                   # Linux (Leonardo)
powershell -ExecutionPolicy Bypass -File scripts/test-baseline-xml.ps1   # Windows sin make (Bayron)
```

Los dos hacen lo mismo, con Docker. Hay que mantenerlos a la par, flag por flag (CR-S11):

1. **Imagen** `validador-baseline-xml:1`, de `crates/baseline/docker/Dockerfile` (B9, B31): `rust:1.100.0-slim-trixie` fijada por versión y por digest (`sha256:e9d95b74296f7126a83ae3d041aeaba1ff4c478f13abc5cc034fcf7c131c3d4a`; la 1.100.0 es la `stable` que fija `rust-toolchain.toml` el 15 nov), más `libxml2-dev`, `libclang-dev` y `pkg-config` de Debian 13 y el componente `clippy`. `RUSTUP_TOOLCHAIN=1.100.0` evita que `rust-toolchain.toml` haga a rustup descargar otra cadena; `CARGO_BUILD_JOBS=4`, porque la máquina virtual de Docker Desktop tiene unos 3,7 GiB.
2. **`cargo fetch --locked`** con red, en el volumen de caché `validador-cargo-registry`.
3. **`scripts/test-baseline-xml.sh`** dentro del contenedor, **sin red** (`--network none`), **sin capacidades** (`--cap-drop ALL`) y **sin escalada de privilegios** (`--security-opt no-new-privileges`) (SEC-L3, B38): imprime la versión de libxml2 y de `rustc` y corre `cargo test -p baseline --features libxml2 --locked --offline` y `cargo clippy -p baseline --all-targets --features libxml2 --locked --offline -- -D warnings`.

El repositorio se monta **en solo lectura** en `/src`, y el `target` va al volumen `validador-baseline-xml-target` (`CARGO_TARGET_DIR=/target`), nunca al `target/` de la máquina. La primera vez tarda unos minutos (imagen y compilación de las dependencias); después, segundos (o un par de minutos si cambió el código de `baseline`).

**Por qué no `--user`:** el contenedor corre como root. `CARGO_HOME` (`/usr/local/cargo`) es de root en la imagen, y los volúmenes con nombre los crea Docker como root la primera vez; con un usuario sin privilegios cargo no podría escribir el `target` ni el bloqueo del registro, salvo cambiando de dueño los volúmenes y `CARGO_HOME` en la imagen. Con root sin capacidades, sin red, con el repositorio en solo lectura y sin escalada, el `build.rs` de `libxml`/`bindgen` (código de terceros que se ejecuta al compilar) solo puede escribir en los dos volúmenes de caché.

**SELinux (Fedora, RHEL; S11):** el montaje del repositorio necesita la etiqueta `z`: `-v "$(CURDIR)":/src:ro,z` en el Makefile. No se puso por defecto porque en Docker Desktop y en Ubuntu no hace falta.

Equivalente a mano desde Git Bash. `MSYS_NO_PATHCONV=1` evita que MSYS reescriba las rutas, `pwd -W` da `C:/…` y las comillas protegen los espacios de la ruta del repositorio:

```sh
docker build -t validador-baseline-xml:1 crates/baseline/docker
export MSYS_NO_PATHCONV=1
M="-v validador-cargo-registry:/usr/local/cargo/registry -v validador-baseline-xml-target:/target -w /src"
docker run --rm -v "$(pwd -W):/src:ro" $M validador-baseline-xml:1 cargo fetch --locked
docker run --rm --network none --cap-drop ALL --security-opt no-new-privileges \
    -v "$(pwd -W):/src:ro" $M validador-baseline-xml:1 sh scripts/test-baseline-xml.sh
```

Salida de `scripts/test-baseline-xml.ps1` en la máquina de Bayron (ronda 1, 16 nov 2026, Docker Desktop, motor 29.9.2), sin las líneas de compilación ni la lista de pruebas:

```text
libxml2 2.9.14 (paquete libxml2-dev 2.12.7+dfsg+really2.9.14-2.1+deb13u3)
rustc 1.100.0 (0ef1bc386 2026-11-09)
     Running unittests src/lib.rs
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/corpus_json.rs
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running tests/corpus_xml.rs
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
     Running tests/json.rs
test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.24s
     Running tests/xml.rs
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.76s
   Doc-tests baseline
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.05s
test-baseline-xml: pruebas y clippy de la línea base XML en verde

```

Con la feature, `corpus_json.rs` tiene 7 pruebas y no 8: `sin_libxml2_un_caso_xml_no_esta_disponible` solo se compila sin ella. La libxml2 de Debian 13 es la 2.9.14 (el paquete se llama 2.12.7, pero lleva la 2.9.14: «really2.9.14»), así que `libxml` usa sus *bindings* para versiones anteriores a la 2.12. Las versiones de Debian no se fijan, porque los parches de seguridad retiran las viejas del espejo; el script las imprime en cada ejecución ([pendientes.md](pendientes.md) §25).

### 7.4 Posiciones y forma del resultado

- **`Malformed`** lleva `line` y `column` de libxml2 (desde 1; **columna en caracteres**, como el proyecto, no en bytes como la línea base JSON) y `offset` en bytes (la referencia, D9), calculado a partir de ellas: inicio de la línea más el ancho UTF-8 de `column − 1` caracteres. En la línea 1 la columna empieza después del BOM, que libxml2 salta solo (`bom_inicial_se_ignora_y_los_offsets_cuentan_sus_3_bytes`). En un fin de entrada prematuro, el offset es la longitud (`fin_de_entrada_prematuro_es_malformado_en_la_longitud`). libxml2 sitúa cada error donde lo detecta, que no siempre es el byte del proyecto: una etiqueta de cierre que no casa se informa tras leer su nombre (`etiqueta_sin_cerrar_es_malformado_con_linea_columna_y_offset`; [pendientes.md](pendientes.md) §23). Los que sitúa la propia línea base: la codificación (offset 0, o el valor de `encoding`), el `<!DOCTYPE` (en la `D`; en la segunda capa, offset 0) y el exceso de profundidad (en el `<`). `malformados_variados_tienen_posicion` comprueba que la entrada vacía, solo espacios, UTF-8 inválido en texto, nombre y atributo, `&#0;`, `&#xD800;`, un atributo duplicado y un BOM incompleto son `Malformed` con posición (CR-S10, O3).
- **Errores de buena formación con árbol.** Con `recover` apagado, libxml2 devuelve el árbol aunque haya errores de espacios de nombres (un prefijo sin declarar). La línea base los trata como `Malformed`: cualquier diagnóstico de nivel error o fatal lo es; los avisos no cuentan (`prefijo_sin_declarar_es_malformado`).
- **Límites (CR-I2).** Los de libxml2 2.9.14 sin `huge` son `Kind::Limit` y se reconocen por el texto del mensaje, porque libxml2 no les da un código propio (`es_limite`; [pendientes.md](pendientes.md) §24): la profundidad («Excessive depth»), un comentario, una sección CDATA o una instrucción de procesamiento de más de 10 MB («Comment too big found», «CData section too big found», «PI … too big found»), un valor de atributo de más de 10 MB («AttValue length too long») y un nombre de más de 50 000 bytes («Name too long»). Un **nodo de texto** de 10 MB o más también es `Limit`, con uno de dos mensajes (TR1-O1): «internal error: Huge input lookup» (`XML_MAX_LOOKUP_LIMIT`, 10 000 000 bytes de búsqueda) para texto ASCII de una tirada o en líneas (`texto_de_mas_de_10_mb_es_limite`), y «xmlSAX2Characters: huge text node» (`XML_MAX_TEXT_LENGTH`, nodo acumulado) con texto multibyte (`ñ` × 5 000 001), con referencias intercaladas o con dos CDATA seguidos. Un texto hecho solo de referencias (`&amp;` × 2 M) o texto + CDATA no llega a ninguno y es válido. La tabla medida está en [pendientes.md](pendientes.md) §24, con el criterio pendiente de una prueba que ejercite «huge text node» contra libxml2 real (F6.5). La primera versión olvidaba «too big» y daba `Malformed` para los comentarios, CDATA e instrucciones grandes; lo prueban `comentario_cdata_e_instruccion_de_mas_de_10_mb_son_limite`, `atributo_de_mas_de_10_mb_y_nombre_de_mas_de_50_000_bytes_son_limite` y, con los textos literales de `parser.c`, la prueba unitaria `es_limite_con_los_mensajes_de_libxml2`. Una entrada de más de `i32::MAX` bytes también es `Limit`.
- **Profundidad.** libxml2 2.9.14 sin `huge` **acepta 257 elementos anidados** y corta en el 258 («Excessive depth in document: 256», medido con `xmllint` en la misma imagen). La línea base aplica además su propio tope, `Limits::max_depth` (256 por defecto, solo configurable a la baja; cuenta elementos, CR-S8), con `profundidad_excedida` (B32). Recorre los bytes ya comprobados por libxml2 (hasta su error, si lo hubo), salta comentarios, CDATA, instrucciones y los `>` entre comillas, y da `Kind::Limit` en el `<` del elemento número 257, como el proyecto (`00-alcance.md` §4). 256 niveles son válidos; 257, 300 y 10⁵ son `Limit` en el offset 768 (`profundidad_256_es_valida`, `profundidad_257_es_limite_en_el_menor_que_que_la_pasa`, `anidamiento_de_300_y_de_cien_mil_niveles_es_limite_sin_caida`, `tope_de_profundidad_configurable_a_la_baja`).
- **`Violation`.** Una por cada error de validación de libxml2 (U7). libxml2 no da la ruta del nodo (el crate `libxml` no expone `xmlError.node`), así que **`pointer` queda vacío**; `keyword` es `libxml2:<código>` (el número de `xmlParserErrors`, p. ej. `libxml2:1866` para un atributo no permitido) y `message` empieza por la línea (`línea 1: Element 'pedido': The attribute 'id' is required but missing.`) (B33). La prueba del corpus XML no compara `pointer` ([pendientes.md](pendientes.md) §22).
- **Error interno de libxml2 (CR-S6).** Si `xmlSchemaValidateDoc` devuelve un código negativo sin diagnóstico, el crate `libxml` añade una violación sintética («Schema validation failed with status …», código 1818, `XML_SCHEMAV_INTERNAL`) y la línea base la da como `Kind::Schema` con `keyword` `libxml2:1818`. No se ha visto con el corpus ni con las pruebas; quien compare con la línea base (F4.6) debe tratar esa `keyword` como «sin veredicto», no como una violación.

### 7.5 Divergencias con el DPDA y el alcance

| # | Tema | Proyecto (`00-alcance.md`, DPDA) | Línea base XML | Motivo / consecuencia |
|---|---|---|---|---|
| 1 | `<!DOCTYPE`, DTD, entidades declaradas | error léxico en la `D` (D8, §1.2) | **coincide**: `Malformed` en la `D`, también en minúsculas | la primera capa de D8 (§7.2); el *billion laughs* del corpus da el offset 24 de `expected`. Si alguna vez actuara la segunda capa (`tiene_dtd`), el offset sería 0, no la `D` |
| 2 | Codificación distinta de UTF-8 (declarada, UTF-16, UTF-32, EBCDIC, UTF-7) | error (§1.2) | **coincide**: `Malformed`, en el offset 0 (inicio no compatible con ASCII) o en el valor de `encoding` (declaración); `encoding="utf-8"` en cualquier caja se acepta | política UTF-8 de §7.2 (ronda 1: antes libxml2 convertía, y UTF-16 daba hasta un `Limit` falso). El offset del proyecto puede ser otro byte de la declaración ([pendientes.md](pendientes.md) §1) |
| 3 | Offset del primer error | byte donde lo detecta el DPDA | donde lo detecta libxml2 (de su línea y su columna), salvo codificación, DOCTYPE y profundidad | §7.4; la prueba del corpus compara el offset de los casos `lexical`/`syntax` (hoy, el *billion laughs*) ([pendientes.md](pendientes.md) §23) |
| 4 | Columnas | puntos de código | **coincide**: caracteres (las de libxml2) | distinto de la línea base JSON, que cuenta bytes. Al calcular el offset, un `\r` suelto no cambia de línea |
| 5 | `pointer` | XPath simple del primer error (`tests/README.md`) | vacío; la línea va en el mensaje | §7.4 ([pendientes.md](pendientes.md) §22) |
| 6 | Recuento de violaciones | peso del recorrido (D4; F4-U3: 1 por `Constraint` violada por instancia) | un error por cada informe de libxml2 | medido: atributo requerido que falta, 1; `abc` como `xs:decimal`, 1; elemento inesperado, 1; raíz no declarada, 1; 1 000 `<n>x</n>` con `xs:integer`, 1 000; un `<a x='>'>` con texto en un modelo de solo elementos, 3 (atributo no permitido, texto, elemento inesperado). Queda por comparar con el intérprete, y por medir si libxml2 sigue validando un modelo de contenido tras el primer elemento inesperado ([pendientes.md](pendientes.md) §5) |
| 7 | Subconjunto XSD | §3: sin `xs:any`, `mixed`, `xs:list`, `xs:union`, `xs:complexContent`, restricciones de identidad, fechas, `totalDigits`… (§3.3) | XSD 1.0 completo: todo eso compila y se aplica | `xsd_con_construcciones_fuera_del_subconjunto_compila`; como la fila 14 de §4, F4.6 y F6.5 solo comparan esquemas que pasan `schemac check` ([pendientes.md](pendientes.md) §10) |
| 8 | `xs:import`, `xs:include`, `xs:redefine` (y `xs:override`) | fuera de alcance (§3.3) | **coincide**: error de compilación, sin red ni disco, también dentro de `xs:appinfo` | §7.2 |
| 9 | Nombres de elemento | nombre local dentro del único `targetNamespace` (§1.2) | nombre expandido (URI + local), como XSD 1.0 | `<pedido xmlns="urn:otro">` contra un XSD sin espacio de nombres es un error de esquema en libxml2; el proyecto compara por nombre local. Comparar en F4.6 |
| 10 | Profundidad | 256 (§4) | **coincide**: 257 es `Limit` en el `<` | libxml2 solo cortaría en 258; la línea base aplica su propio tope (§7.4) |
| 11 | Otros límites de §4 (100 000 referencias de entidad y de carácter por documento; cadena de 1 MiB) | `limit` | no se aplican; son `Limit` los de libxml2: nodo de texto de unos 10 MB o más (umbral de 10 000 000 bytes de lookup: 9 999 999 bytes de texto ya lo alcanzan; un texto solo de referencias no lo alcanza y es válido; TR1-O2), comentario, CDATA o instrucción de más de 10 MB, atributo de más de 10 MB, nombre de más de 50 000 bytes | como §4, filas 20-22 ([pendientes.md](pendientes.md) §13, §24) |
| 12 | `lexical` frente a `syntax` | dos clases | una: `Kind::Malformed` | como §4, fila 10 |
| 13 | Entidad no predefinida (`&lol;`) | error | **coincide**: `Malformed` | `entidad_no_predefinida_es_malformado` |
| 14 | Prefijo sin declarar | error de espacios de nombres (D31) | **coincide**: `Malformed` | `prefijo_sin_declarar_es_malformado` |
| 15 | `xsi:schemaLocation`, `xsi:noNamespaceSchemaLocation` en el payload | sin especificar | no carga nada y valida el resto | `xsi_schema_location_del_payload_se_ignora`; comparar en F4.6 |
| 16 | Tamaño del payload | sin límite (streaming) | como mucho `i32::MAX` bytes (`Limit`), y todo en memoria | [pendientes.md](pendientes.md) §3 |

### 7.6 Pruebas

`make test-baseline-xml` (Docker, §7.3); sin la feature no se compilan. Las pruebas de bloques de 1, 7 y 4 096 bytes del proyecto **no aplican** a la línea base: es DOM y recibe el payload entero (`&[u8]`), no por bloques.

- **`tests/corpus_xml.rs`**: `la_linea_base_recorre_el_corpus_xml` compara cada caso XML con bytes (`valid`, clase de `errorKind`, `violations` y el offset de los errores léxicos y sintácticos), exige ≥ 2 casos y no tiene tabla de divergencias, porque hoy no diverge ninguno. `pedido_valido_y_billion_laughs_malformado`: el pedido es válido; el *billion laughs*, `Malformed` en la línea 2, columna 3, offset 24.
- **`tests/xml.rs`** (41), todas dentro de `with_stack` y sin medir tiempo (D1):
  - resultado y etapas: `pedido_valido`, `las_etapas_se_llaman_por_separado` (el mismo árbol validado dos veces, soltado después del esquema; un árbol validado con otro esquema), `un_mismo_xsd_valida_muchos_payloads` (el registro de errores del contexto se vacía tras cada validación);
  - malformados: etiqueta sin cerrar (línea, columna y offset), fin de entrada prematuro, basura tras la raíz, columna en caracteres con multibyte, prefijo sin declarar, entidad no predefinida, `malformados_variados_tienen_posicion` (vacío, espacios, UTF-8 inválido en texto, nombre y atributo, `&#0;`, `&#xD800;`, atributo duplicado, BOM incompleto), 150 avisos seguidos de un error de espacios de nombres; las cinco entidades predefinidas y las de carácter se aceptan;
  - codificación: `entradas_que_no_son_utf8_son_malformado`, `utf8_declarado_en_cualquier_caja_se_acepta`, `latin1_sin_declarar_es_malformado`, `xsd_que_no_es_utf8_no_compila`, y el BOM UTF-8;
  - D8: entidad externa `file:///etc/passwd` (sin el contenido en la salida), DTD externa, entidad de parámetro y `PUBLIC` con `http://`, DOCTYPE tras BOM, comentario e instrucción y en minúsculas, DOCTYPE dentro de un comentario, `xsi:noNamespaceSchemaLocation`;
  - esquema: falta `id` (`Schema`, 1 violación, `pointer` vacío), `<total>abc</total>` (`Schema`), elemento inesperado y raíz desconocida; topes de la lista de violaciones y de los mensajes;
  - profundidad: 256 válida; 257, 300 y 10⁵ son `Limit` en el offset 768; tope configurable a la baja con comentarios, CDATA, instrucciones, `>` entre comillas y elementos vacíos; un tope mayor que 256 se recorta;
  - límites de libxml2: comentario, CDATA e instrucción de más de 10 MB, atributo de más de 10 MB y nombre de más de 50 000 bytes son `Limit`; un texto de más de 10 MB también («Huge input lookup»);
  - compilación: `xs:import`, `xs:include`, `xs:redefine`, `xs:override` e `import` dentro de `xs:appinfo` (también con otro prefijo y sin prefijo), XSD con DOCTYPE, XSD de más de 256 niveles, XSD mal formado o inválido, construcciones fuera del subconjunto (compilan), tope del mensaje de `CompileError`.
- **Pruebas unitarias de `xml.rs`** (3): `la_segunda_capa_de_d8_rechaza_un_arbol_con_dtd` (§7.2), `es_limite_con_los_mensajes_de_libxml2` y `encoding_declarado_lee_el_pseudoatributo`.
- **Doc-tests** `compile_fail` de `xml.rs`: `XmlBaseline` no es `Send`; `XmlDom` no es `Send` ni `Sync`. Como en JSON, rustdoc estable no comprueba el código `E0277`.

### 7.7 Pendientes

En [pendientes.md](pendientes.md): §1 (codificación: resuelto en XML con la política UTF-8), §2 (memoria de libxml2, para F6.5), §6 (resuelto), §3, §13, §14, §16, §18 y §19 (ampliados a XML; §14 con las cifras de los XSD hostiles) y los nuevos §21-§27 (la lista incluye §26 y §27 desde el cierre, F6.7b CR1-S1): lista de errores de validación sin tope en el crate `libxml` (con la medición de memoria), `pointer` vacío, offsets de libxml2 frente al DPDA, límites de libxml2 reconocidos por el mensaje (con la tabla del nodo de texto de 10 MB), mantenimiento de la imagen Docker y de libxml2 (con los límites de recursos y los volúmenes compartidos del contenedor), la confirmación con Leonardo de la API XML y de `make test-baseline-xml` en Linux (§26), y los dos órdenes de UCS-4 que faltan en `NO_UTF8` (§27).
