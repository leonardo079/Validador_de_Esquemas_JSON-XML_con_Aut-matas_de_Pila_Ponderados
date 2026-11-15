# Fase 3 — Números de XSD: forma léxica, reescritura, comparación exacta y redondeo IEEE

Tarea 5.5 (semana 3). Funciones puras, sin AST, que usarán el frontal XSD para las facetas de rango, las enumeraciones y `default`/`fixed` de los tipos simples (tarea 5.6) y la Fase 4 para los comparadores de `Minimum`…`ExclusiveMaximum` sobre `Num::lexeme` ([ir.md](ir.md) §7, fila Q). Cierra la observación I-2 de la revisión final de la 4.1 ([pendientes.md](pendientes.md) §3) y aplica D24.

Implementación: [`crates/ir/src/num.rs`](../../crates/ir/src/num.rs) (módulo `ir::num`); pruebas en [`crates/ir/tests/num.rs`](../../crates/ir/tests/num.rs) y en el propio módulo.

## 1. Dónde está y por qué (B4)

En `crates/ir`, como `ir::num`, no en `crates/xsd`: la comparación por valor la necesitan el frontal XSD (5.6, comprobar que una restricción es «válida» respecto de su base y que una enumeración cumple los rangos) **y** la Fase 4 (comparadores del DPDA sobre los lexemas de la IR, de JSON Schema y de XSD), y `automaton` no puede depender de un frontal (D13). `ir` sigue dependiendo solo de `lexer`; el módulo no usa nada de `lexer`. La comprobación de I8 (`is_rfc8259_number`) se mudó de `verify.rs` a este módulo, sin cambiar su comportamiento, para que la reescritura y el verificador compartan la misma definición de lexema.

API:

| Función | Qué hace |
|---|---|
| `parse_boolean(valor) -> Result<bool, NumError>` | Forma léxica de `xs:boolean` |
| `rewrite(lx, valor) -> Result<String, NumError>` | Forma léxica del tipo numérico `lx` y reescritura **exacta** a RFC 8259 |
| `xsd_number(lx, valor) -> Result<Num, NumError>` | El `Num` de una faceta o enumeración: `rewrite` y, en `Float`/`Double`, redondeo IEEE y rechazo de D24 |
| `compare(a, b) -> Ordering` | Orden exacto por valor de dos lexemas de RFC 8259 |
| `Decimal::new(lexema)`, `compare_parsed(&a, &b) -> Ordering` | El mismo orden sobre lexemas **analizados una vez** (signo, cifras significativas D y exponente E): `new` cuesta O(\|lexema\|) y cada comparación O(min(\|a\|, \|b\|)). Añadido en la 5.6 (5.6-CR-I1) para las cotas de las facetas, que se comparan con muchos valores |
| `round_f32(lexema)`, `round_f64(lexema)` | Redondeo IEEE 754 al par más cercano, con exponentes de cualquier longitud |
| `is_rfc8259_number(s)` | Lo que I8 exige a un `Num` |

`NumError { lexical, kind }` no guarda el texto del usuario (no hay nada que recortar); quien da el error con su posición, el frontal, cita el valor recortado como el resto de `XsdError` ([../fase2/gramatica-xsd.md](../fase2/gramatica-xsd.md) §6). Su `Display` es una línea con el tipo y el motivo.

## 2. Espacios léxicos y de valores

Las expresiones son las de XSD 1.0 Parte 2 (2.ª ed.), §3.2.2-§3.2.5 y §3.3.13, sobre el valor ya con *collapse* (§3):

| Tipo (`Lexical`) | Espacio léxico | Espacio de valores | `NumErrorKind` de un valor fuera |
|---|---|---|---|
| `xs:boolean` | `true` \| `false` \| `1` \| `0` | {verdadero, falso} | `Empty`, `Syntax` |
| `xs:integer` | `[+-]?[0-9]+` | ℤ | `Fraction` (`1.0`, `1.`, `.5`), `Exponent` (`1e2`), `Syntax` (`+-1`), `Empty` |
| `xs:decimal` | `[+-]?([0-9]+(\.[0-9]*)?` \| `\.[0-9]+)` | los decimales finitos { m·10⁻ᵏ } | `Exponent`, `Syntax`, `Empty` |
| `xs:float` / `xs:double` | el de `xs:decimal` seguido de `([eE][+-]?[0-9]+)?`, o `INF`, `-INF`, `NaN` | binary32 / binary64 (IEEE 754) | `Special` (D24), `Overflow` (D24), `Syntax`, `Empty` |

Los enteros acotados (`xs:int`, `xs:byte`…) son `xs:integer` más un `Minimum`/`Maximum` en el nodo ([ir.md](ir.md) §5.2): su rango lo comprueba la 5.6 con `compare`, no este módulo. `xs:string` y `xs:boolean` en `rewrite`/`xsd_number` dan `NotNumeric`.

**Fuera del subconjunto, con su motivo (D24).** `INF`, `-INF` y `NaN` no son lexemas de RFC 8259, así que no tienen `Num` (I8) y se rechazan en facetas y enumeraciones (`Special`); `+INF`, que es de XSD 1.1, se rechaza con el mismo motivo. `inf`, `nan`, `Infinity` no son de ningún espacio léxico (`Syntax`). Un valor que redondea a ±∞ (`1e39` en `xs:float`) también se rechaza (`Overflow`): no se amplían `Num` ni I8.

## 3. *whiteSpace* `collapse`

Todos estos tipos tienen `whiteSpace` fijo `collapse` (XSD 1.0 Parte 2 §4.3.6): quitar los espacios XML (#x20, #x9, #xD, #xA) de los extremos y reducir cada secuencia interior a un espacio. Ninguno de los espacios léxicos de §2 contiene un espacio, así que «`collapse` y después el espacio léxico» equivale a «quitar los espacios de los extremos y después el espacio léxico»: si quedaba una secuencia interior, `collapse` la reduce a un espacio y el valor ya no es del espacio léxico; sin ella, `collapse` es solo el recorte. Por eso el módulo recorta los extremos y no copia el valor. Otros espacios de Unicode (U+00A0) no son espacios XML y dan `Syntax`.

Lo que no es de este módulo: el *whiteSpace* `preserve` de `xs:string` y la normalización de `default`, `fixed` y `xs:enumeration` según el tipo base, que aplica la 5.6 llamando a estas funciones ([../fase2/pendientes.md](../fase2/pendientes.md) §9).

## 4. Reescritura a RFC 8259

`rewrite` comprueba la forma léxica con un solo recorrido de izquierda a derecha (`scan`: signo, cifras, punto, cifras, exponente) y construye

```
-? (0 | [1-9][0-9]*) ( . [0-9]*[1-9] )? ( e -? [1-9][0-9]* )?        o bien   0
```

quitando el `+` (de la mantisa y del exponente), los ceros a la izquierda de la parte entera (`007` → `7`; si queda vacía, `0`: `.5` → `0.5`) y del exponente, los ceros finales de la fracción (y el punto si queda vacía: `1.` → `1`, `1.50` → `1.5`), el exponente si vale cero, y el signo si el valor es cero (`-0.0` → `0`).

**Proposición.** Si `rewrite(lx, v) = Ok(r)`, entonces r cumple la gramática de RFC 8259 §6 (por tanto I8) y val(r) = val(v) en ℚ. En `xs:integer` y `xs:decimal`, además, val(v) = val(w) ⇒ `rewrite(v)` = `rewrite(w)` (forma canónica).

*Prueba.* Gramática: la parte entera es `0` o empieza por una cifra no nula; la fracción, si está, tiene al menos una cifra; el exponente, si está, tiene al menos una cifra. Valor: v = ±(I + F·10⁻ᵏ)·10ˣ; quitar ceros a la izquierda de I o de x, ceros a la derecha de F (con su k), un `+` o un exponente cero no cambia ninguno de los factores, y el signo de un cero no cambia el valor en ℚ. Canonicidad: sin exponente, un decimal no nulo tiene una única escritura sin ceros a la izquierda de la parte entera ni a la derecha de la fracción, y el cero se escribe siempre `0`. ∎

En `xs:float`/`xs:double` la reescritura exacta no es canónica (`10` y `1e1`); `xsd_number` guarda en su lugar el decimal más corto del valor redondeado (§6), que sí lo es dentro del espacio de valores IEEE.

**Costo.** Un recorrido de `scan` y una copia: O(|v|).

## 5. Comparación exacta

Un lexema de RFC 8259 se escribe en **forma científica normalizada** (el método de `semantic::number`, [meta-esquema.md](meta-esquema.md)): sea D la sucesión de las cifras de la parte entera y la fracción, sin los ceros de los extremos, p el número de cifras de la parte entera y z el de ceros a la izquierda que se quitaron. Entonces

```
valor = (−1)^s · 0.D · 10^E,     E = exp + p − z,     D vacía ⇔ valor = 0.
```

Con D no vacía, D empieza por una cifra no nula, así que 10^(E−1) ≤ |valor| < 10^E.

**Algoritmo** (`compare(a, b)`): (1) por el signo, con −0 = 0 = +0; si los dos son cero, iguales. (2) Con el mismo signo, por E; (3) con el mismo E, por D en orden lexicográfico, donde un prefijo propio es menor (D no tiene ceros finales). (4) Si el signo es negativo, se invierte (2)-(3).

**Corrección.** (2): si E_a < E_b, |a| < 10^E_a ≤ 10^(E_b−1) ≤ |b|. (3): con el mismo E, |a| < |b| ⇔ 0.D_a < 0.D_b, y dos fracciones decimales sin ceros finales se comparan como cadenas en orden lexicográfico con «prefijo menor» (la primera cifra distinta decide; si una es prefijo de la otra, a la más larga le sobra una cola no nula). Como el resultado es el orden de ℚ sobre los valores, es un **orden total**: reflexivo, antisimétrico y transitivo.

**Exponentes de cualquier longitud, sin saturación.** p y z son longitudes del lexema y caben en `i64`; `exp` no tiene por qué (`1e` seguido de 10⁵ cifras es un lexema válido). E se calcula con un entero con signo de longitud arbitraria (`Big`: magnitud en cifras decimales), con una suma O(|exp|) y una comparación O(|exp|) (primero por longitud, después por cifras). A diferencia de `semantic::number`, que satura el exponente en `i64` porque solo necesita su signo, aquí **no hay saturación**: dos exponentes de 30 cifras que difieren en una unidad se distinguen (`1e99…9` > `1e99…8`), y `0.1e10³⁰` = `1e(10³⁰ − 1)`. La única conversión acotada es la de las longitudes p y z a `i64` (`try_from`, con `i64::MAX` si no cabe), inalcanzable con un lexema en memoria.

**Costo.** Un número constante de recorridos de cada lexema (contar cifras, ceros de los extremos, cifras de la parte entera), la suma y comparación de `Big` y la comparación de D: **O(|a| + |b|)**. Con los lexemas ya analizados (`Decimal`, 5.6) esos recorridos se hacen una vez por lexema y `compare_parsed` solo compara el signo, E (primero por longitud, así que dos exponentes de longitudes distintas se deciden en O(1)) y D en orden lexicográfico, que se detiene en la primera cifra distinta o al acabar la más corta: **O(min(|a|, |b|))**. Es lo que deja al frontal comparar *k* valores cortos con una cota de 1 MiB en O(*k* + 1 MiB) ([xsd.md](xsd.md) §10.1). **Contador de pruebas:** con la *feature* `contadores` (que solo activan las pruebas de `xsd`, como dev-dependency), `ir::num::contador::bytes()` da los bytes que ha recorrido en el hilo el análisis de lexemas (la forma científica de `compare`, `Decimal::new` y el redondeo); sin ella no existe, y con ella ningún resultado cambia. Memoria: la magnitud de E (O(|exp|)); sin `f64`, sin recursión, sin pánico. Ante un texto que no es un lexema de RFC 8259 no falla: los caracteres que no son cifras se ignoran, como en `semantic::number`.

## 6. Redondeo IEEE de `xs:float` y `xs:double` (I-2, D24)

XSD 1.0 Parte 2 §3.2.4-§3.2.5 define el espacio de valores de `xs:float`/`xs:double` como binary32/binary64 y la correspondencia léxica como el redondeo al valor más cercano, con empate al par (el de IEEE 754). Por eso `xs:float` **no** se compara en decimal exacto: `<xs:maxInclusive value="0.1"/>` acepta `0.10000000000000000001`, que redondea al mismo binary32.

**Redondeo con exponentes de cualquier longitud** (`round_f32`, `round_f64`). `str::parse` de `std` redondea bien, pero satura el exponente (deja de acumular sus cifras en cuanto pasa de 65 536, así que `0.0…01e1000000` con 10⁶ − 1 ceros, que vale 1, daría 0) y su coste depende de la longitud de la mantisa. Así que el lexema se reduce antes a `0.D′eE` con |E| ≤ 400 y D′ de a lo sumo 801 cifras, que `std` sí redondea igual que el original:

- **E ≥ 401** ⇒ |valor| ≥ 10⁴⁰⁰ > 2¹⁰²⁴ ⇒ ±∞ en los dos formatos. **E ≤ −401** ⇒ |valor| < 10⁻⁴⁰¹ < 2⁻¹⁰⁷⁵ (la mitad del menor subnormal de binary64, ≈ 2,5·10⁻³²⁴; en binary32 la cota es aún mayor) ⇒ ±0.
- **Corte de D en 800 cifras.** Los puntos donde cambia el redondeo (los puntos medios entre dos flotantes consecutivos, entre 0 y el menor subnormal, y entre el máximo finito y ∞) son racionales diádicos k·2⁻ʲ con a lo sumo 768 cifras decimales significativas en binary64 (el peor es (2⁵⁴ − 1)·2⁻¹⁰⁷⁵ = (2⁵⁴ − 1)·5¹⁰⁷⁵·10⁻¹⁰⁷⁵, con (2⁵⁴ − 1)·5¹⁰⁷⁵ impar y sin ceros finales) y 113 en binary32 ((2²⁵ − 1)·5¹⁵⁰). Si D tiene más de 800 cifras, el valor está en el intervalo **abierto** (P, P + 10^(E−800)), con P el prefijo de 800 cifras (la cola no es nula porque D termina en una cifra no nula); P y P + 10^(E−800) son múltiplos consecutivos de 10^(E−800). Un punto de cambio dentro de (P, P + 10^(E−800)) está en [10^(E−1), 10^E), luego su cifra menos significativa está en una posición ≥ 10^(E−768), luego es múltiplo de 10^(E−800); y ningún múltiplo de 10^(E−800) puede caer estrictamente entre dos múltiplos consecutivos. Así que ningún punto de cambio cae dentro del intervalo, todo él redondea igual, y P seguido de un `1` en la cifra 801 está dentro: mismo resultado. Lo comprueba `redondeo_ieee_casos` con 1 + 2⁻⁵³ (punto medio exacto, empata a 1) seguido de 900 ceros y un 1 (sube al siguiente doble).

Coste: O(|lexema|) para la forma científica y O(1) para `parse` sobre la cadena reducida; nunca NaN.

**Lo que guarda `xsd_number`.** El **decimal más corto** que vuelve al mismo binary32/binary64 (la escritura de `std`, con o sin exponente, la más corta: `0.1`, `1e38`, `3.4028235e38`, `16777217` → `16777216`), que es un lexema de RFC 8259 y por tanto pasa I8 y el JSON v2 de `schemac ir` sin cambios. Alternativa descartada: guardar el lexema exacto y que la Fase 4 redondee los dos lados. Con el decimal más corto, dos facetas con el mismo valor IEEE escritas de forma distinta dan el mismo `Num`, la IR muestra el valor que de verdad se compara, y el redondeo de D24 (que hay que hacer de todos modos para detectar el desborde) se hace una sola vez, en el frontal, con la posición del valor. **Contrato para la Fase 4 (semántica):** en un nodo `Float`/`Double`, el resultado es el de redondear el texto del payload a binary32/binary64 y compararlo con el redondeo de `Num` (que es exacto, porque el lexema guardado vuelve a su valor); en `Integer`/`Decimal` y en JSON, `compare` exacto.

**Implementación recomendada para el validador generado.** `round_f32`/`round_f64` describen la semántica, pero no sirven dentro del validador: `ir::num` asigna memoria (`Big`, `String`), y el validador generado (F5.1, sin asignación dinámica) y el runtime C no pueden llamarlo (en C haría falta un `strtod` correctamente redondeado e independiente del *locale*). La vía recomendada es compilar cada cota IEEE, en tiempo de compilación, a su **umbral decimal exacto**: el punto medio m⁺ (o m⁻) entre el valor guardado F y su vecino del otro lado, con la regla del empate. Por ejemplo, `round(p) ≤ F` ⇔ `p ≤ m⁺` si la mantisa de F es par (el empate en m⁺ redondea a F) y ⇔ `p < m⁺` si es impar; `round(p) ≥ F`, igual con m⁻ y el vecino inferior; `round(p) < F` es la negación de `round(p) ≥ F`. Así el comparador del payload sigue siendo el comparador decimal exacto de clase (a), sin flotantes ni memoria dinámica, y m± (a lo sumo 768 cifras significativas) cabe en el lexema de ≤ 1 024 bytes de `00-alcance.md` §4 ([pendientes.md](pendientes.md) §36, 5.5-CR-S1).

**Monotonía del decimal más corto.** Si x < y en binary32 (o binary64), entonces `compare(num(x), num(y)) = Less`, con num(x) el lexema que guarda `xsd_number`. *Prueba:* num(x) está en el intervalo de redondeo de x (redondea a x) y num(y) en el de y; los intervalos de redondeo de flotantes distintos son disjuntos salvo en sus extremos, los puntos medios, y un punto medio redondea a uno solo de los dos; como x < y, todo el intervalo de x queda por debajo del de y, así que num(x) < num(y) en ℚ. Por eso la 5.6 puede comparar los `Num` de `Float`/`Double` directamente con `compare`, sin redondear (el cero, −0 incluido, se guarda como `0`, el decimal más corto de 0).

**Desborde (D24).** Un valor desborda si redondea a ±∞: |v| ≥ (2 − 2⁻²⁴)·2¹²⁷ = 340282356779733661637539395458142568448 en binary32 (el punto medio entre el máximo finito y 2¹²⁸ empata al par, que es ∞) y |v| ≥ (2 − 2⁻⁵³)·2¹⁰²³ en binary64. Uno más por debajo redondea al máximo finito y se admite (prueba `d24_inf_nan_y_desbordes_se_rechazan_con_motivo`). El subdesborde **no** es un error: `1e-50` en `xs:float` es el valor más cercano, 0.

**Ceros.** El valor cero se guarda como `0`, también `-0` y el que sale de un subdesborde negativo (`-1e-50`). Es **conforme a XSD 1.0** (2.ª ed.), Parte 2 §3.2.4 y §3.2.5: «This datatype differs from that of [IEEE 754-1985] in that there is only one NaN and only one zero» y «Lexical representations for zero may take a positive or negative sign»; el espacio de valores tiene un solo cero (el orden −0 < +0 era de la 1.ª edición).

**NaN e INF en el payload.** En facetas y enumeraciones se rechazan (D24), pero en el payload son valores válidos de un texto `Float`/`Double`. Según la 2.ª ed., «NaN equals itself but is incomparable with … any other value» y «Any value incomparable with the value used for the four bounding facets … will be excluded»: un NaN del payload **viola cualquier faceta de rango**; `INF` y `-INF` se ordenan normalmente (por encima y por debajo de todo valor finito). Es la regla que hereda la Fase 4 ([pendientes.md](pendientes.md) §36, 5.5-P3).

## 7. Pruebas

[`crates/ir/tests/num.rs`](../../crates/ir/tests/num.rs):

- **Forma léxica y reescritura** (tabla): `integer_forma_lexica_y_reescritura` (`1.0`, `1.`, `.5` → `Fraction`; `1e2` → `Exponent`; `+-1`, `1 2`, U+00A0, cifras de ancho completo → `Syntax`; vacío o solo espacios → `Empty`), `decimal_forma_lexica_y_reescritura` (exponentes rechazados; `.5` → `0.5`, `1.` → `1`, `00.0100` → `0.01`), `float_y_double_reescritura_exacta`, `float_y_double_redondeados_al_decimal_mas_corto`, `boolean_forma_lexica`, `tipos_no_numericos`.
- **D24 con su motivo:** `d24_inf_nan_y_desbordes_se_rechazan_con_motivo` (`INF`, `-INF`, `+INF`, `NaN` → `Special`; `1e39` y el umbral exacto en `xs:float`, `1e309` en `xs:double` → `Overflow`; el texto del mensaje nombra el motivo, el tipo, `binary32`/`binary64` y D24, en una línea).
- **Comparación:** `comparacion_casos_conocidos` (ceros con signo, formas distintas del mismo valor, exponentes de 30 cifras que no caben en `i64`, en los dos sentidos) y `redondeo_ieee_casos` (umbrales, subnormales, corte de 800 cifras).
- **10⁵ cifras** (`exponentes_y_fracciones_de_cien_mil_cifras`): exponentes y fracciones de 10⁵ cifras en `compare`, `round_f32`/`round_f64`, `rewrite` y `xsd_number`, con el resultado exacto y sin pánico; comprueba el resultado, no el tiempo (D1).
- **Propiedades** (proptest): `reescritura_valida_y_del_mismo_valor` (los cuatro tipos: el lexema es de RFC 8259, tiene el valor del oráculo racional y, como `Minimum`, `ExclusiveMaximum` y `Enum` de un texto de ese tipo, **pasa `Ir::verify`**: I8 e I13), `reescritura_decimal_canonica`, `comparacion_coincide_con_oraculo` (oráculo de racionales m·10ˢ en `i128`), `comparacion_antisimetrica_y_transitiva` (exponentes de hasta 40 cifras), `redondeo_coincide_con_std`, `decimal_mas_corto_vuelve_al_mismo_valor` (todo `f32`/`f64` finito) y `no_fallan_con_texto_arbitrario`.

En `crates/ir/src/num.rs`: `suma_y_orden_de_enteros_largos`, `forma_cientifica` y `enteros_largos_como_i128` (proptest de `Big` contra `i128`).

## 8. Decisiones y pendientes

Decisiones de esta tarea (candidatas al registro del plan):

1. **B4:** el módulo vive en `crates/ir` (`ir::num`), porque lo usan la 5.6 y la Fase 4; `is_rfc8259_number` se muda allí desde `verify.rs` (§1).
2. En `xs:float`/`xs:double`, `Num` guarda el decimal más corto del valor redondeado al par más cercano; la Fase 4 redondea el texto del payload y compara en IEEE (§6).
3. Reescritura canónica en `xs:integer`/`xs:decimal` (§4); `collapse` como recorte de los extremos (§3).
4. Comparación exacta con exponente de longitud arbitraria, sin saturación (§5).
5. Un solo cero, guardado como `0` (conforme a XSD 1.0 2.ª ed. §3.2.4, sin desviación); el subdesborde da 0 y se admite; `+INF` se rechaza como `INF` (§2, §6).

Abiertos: la integración en el frontal y los rangos de los predefinidos (5.6), el contrato y la implementación de los comparadores de la Fase 4 (umbrales decimales exactos, NaN e INF del payload) y los cosméticos de la revisión, en [pendientes.md](pendientes.md) §36.
