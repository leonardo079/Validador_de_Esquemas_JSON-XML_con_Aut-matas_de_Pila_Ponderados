# Fase 4 — `pattern`: el subconjunto regular y su autómata

`pattern` (JSON Schema, ECMA-262 con la bandera `u`) y `xs:pattern` (XSD 1.0 Parte 2, apéndice F) se aceptan solo en un **subconjunto regular** (F4-U14, D46 (3)), y el validador los recorre con un AFD sin retroceso (F4.4b). Este documento describe el analizador [`ir::regex`](../../crates/ir/src/regex.rs) (F4.4a) y la construcción del AFD dentro de M (F4.4b). Decisiones: D61 (F4.4a) y D62 (F4.4b) del plan.

## 1. Gramática del subconjunto

```
regex  ::= alt ( '|' alt )*
alt    ::= ('^')? item* ('$')?          — ECMA: ^ y $ solo aquí y en el primer nivel
item   ::= atom quant?
quant  ::= '*' | '+' | '?' | '{' n '}' | '{' n ',' '}' | '{' n ',' n '}'
atom   ::= char | '.' | escape | class | '(' regex ')' | '(?:' regex ')'   — (?: solo en ECMA
class  ::= '[' '^'? ( catom ( '-' catom )? )* ']'
```

Los caracteres son **puntos de código** (semántica `u`): un astral cuenta como uno. `n` va en decimal; un `{m,n}` con n < m está mal formado.

| | ECMA (JSON Schema) | XSD |
|---|---|---|
| Anclaje | Búsqueda sin anclar: basta una subcadena. `^`/`$` anclan su alternativa de primer nivel (`^a\|b$` = «empieza por a» o «acaba en b») | Anclado implícito: el texto entero. `^` y `$` son literales |
| `.` | Todo salvo `\n \r U+2028 U+2029` | Todo salvo `\n \r` |
| `\d` | `[0-9]` | `[0-9]`: **desviación** conservadora (XSD es `\p{Nd}`; §3.4 de `00-alcance.md`) |
| `\s` / `\S` | La lista de ECMA (WhiteSpace y LineTerminator) / su complemento | `[ \t\n\r]` / su complemento |
| `\w` / `\W`, `\D` | `[A-Za-z0-9_]` / complemento, `[^0-9]` | Rechazados (proponen `[0-9]` o una clase) |
| Otros escapes | `\t \n \r \v \f \0`, `\xHH`, `\uHHHH` (con pares sustitutos), los de los caracteres de sintaxis | `\n \r \t` y los de los caracteres de sintaxis |

## 2. Lo que se rechaza y por qué

Un patrón rechazado es un error de la Fase 3 en la posición de la cadena del esquema, con el constructo citado (recortado a 32 caracteres) y el motivo (F4-B19). La clase del error:

| Clase | Código JSON / XSD | Ejemplos |
|---|---|---|
| Mal formado | `meta-schema` / `value` | `(a`, `a)`, `[z-a]`, `a{3,2}`, `*a`, `a**`, `\q`, `(?:a)` en XSD |
| Fuera del subconjunto | `unsupported` / `unsupported` | `(a)\1`, `\k<n>`, `(?=a)`, `(?!a)`, `(?<=a)`, `(?<!a)`, `\b`, `a*?`, `(?<n>a)`, `\p{L}`, `a^b`, `\cA`, `\u{…}`; en XSD `\w \i \p{Nd} \D` y `[a-z-[aeiou]]` |
| Demasiado grande | `limit` / `limit` | Más de 4 096 posiciones tras expandir `{m,n}` (`a{999999999}`, `(a{100}){100}`) o más de 256 grupos anidados (F4-U16) |

**Por qué.** Las retrorreferencias no son regulares (`(a*)b\1` describe aⁿbaⁿ), y el *lookaround* y `\b` piden mirar fuera del carácter actual. Ninguno de los dos cabe en un AFD que lee cada carácter una vez. Los cuantificadores perezosos y posesivos solo cambian **qué** subcadena captura un motor de retroceso, no **si** hay coincidencia, pero se rechazan para no aceptar una sintaxis cuyo significado no se respeta. `\p{…}` exigiría las tablas de Unicode. Es la mitigación de **ReDoS** que pide el enunciado: con un AFD, `^(a+)+$` sobre 10⁵ `a` y un `!` cuesta un paso por carácter, mientras que un motor de retroceso prueba 2ⁿ particiones.

En XSD, cada `xs:pattern` se analiza **por separado** antes de unirlas con `|` (5.6-P1, CR-S2), así que `(a` y `b)` en el mismo paso dan dos errores y no un patrón `((a)|(b))` aparentemente válido.

## 3. El analizador

[`ir::regex::parse(src, dialect)`](../../crates/ir/src/regex.rs) (F4-U15: en `crates/ir`, sin crate nuevo, D21) construye un AST en arena con los hijos antes que el padre (sin `Box`) y **sin recursión**: la única pila es la de los grupos abiertos, cortada en 256 (D7: 10⁵ `(` en un hilo de 256 KiB dan `limit` en el 257.º). Las posiciones se cuentan con multiplicación saturada **antes** de expandir (`a{m,n}` cuesta n copias de `a`; `a{m,}`, max(m, 1)), así que `a{999999999}` se rechaza sin reservar nada.

`Regex::glushkov()` da, por cada alternativa de primer nivel, el **autómata de Glushkov** (de posiciones, sin ε): `{m,n}` se expande en `a^m (a (a …)?)?` y `{m,}` en `a^(m−1) a+`, con una posición por carácter de cada copia; luego se calculan `nullable`, `first`, `last` y `follow` en un recorrido por índice. `Regex::matches(s)` simula el conjunto de posiciones activas, sin retroceso: O(|s| · m²/64). Es el **oráculo** de la construcción del AFD (F4.4b) y de la comprobación de las enumeraciones (D29, recortada; §5).

## 4. Pruebas

`crates/ir/tests/regex.rs`: una tabla de 48 patrones con la clase y el carácter del error esperados (todos los de los criterios de F4.4a), otra de 45 coincidencias (anclaje por dialecto, `.` frente a fines de línea y astrales, clases negadas, `{m,n}`, `\d` y `\s` por dialecto), los tamaños tras expandir y el tope de 4 096, el anidamiento con poca pila, y dos proptest: sin pánico con patrones y cadenas arbitrarios, y `matches` frente a un oráculo de retroceso en patrones pequeños. Los esquemas `tests/schemas/semantic-error/pattern-*.json` y `tests/schemas/xsd/semantic-error/pattern-*.xsd` fijan el error de cada clase con línea y columna.

## 5. Lo que queda

- **D29** (recorte 1 del plan): que cada `xs:enumeration`, `default` y `fixed` cumpla las `pattern` de su cadena de bases. Con `Regex::matches` ya es una llamada por valor, pero cambia la omisión escrita en `00-alcance.md` §3.4, que es interfaz compartida. Queda en [pendientes.md](pendientes.md) §65.

## 6. El AFD dentro de M (F4.4b)

**Patrón → AFD** ([`automaton::pattern`](../../crates/automaton/src/pattern.rs), F4-B18). Las posiciones de Glushkov de todas las alternativas de primer nivel van en una numeración común. Sus conjuntos de caracteres definen una **partición local** de [0, 0x10FFFF]: los intervalos con la misma pertenencia forman una clase, así que `[a-z]`, `\d` o `.` cuestan una clase cada uno y no un estado por carácter. La determinización por subconjuntos trabaja sobre esas clases. Un estado es (posiciones activas, ¿primer paso?, ¿encontrado?). Las alternativas sin `^` reinyectan `first` en cada paso, y las que lo tienen, solo en el primero. Al alcanzar el `last` de una alternativa sin `$`, el estado pasa a «encontrado», absorbente y aceptador. Una alternativa con `$` acepta si el estado contiene su `last`. Las anulables se deciden en el estado inicial. Los estados se cuentan **antes** de crearlos: el 4 097.º es `limit` en la ubicación del `pattern` (F4-U16), y el trabajo de la determinización se acota además en 2²⁶ palabras (`MAX_PATTERN_WORK`, como `MAX_CLOSURE_WORK` de los cierres). Por último, Hopcroft (F4.3). **Tope del esquema (E3B-2, E3B-3):** el trabajo de cada patrón (filas de follow, determinización y claves de los estados, `PatternDfa::work`) se suma en todo el esquema, y pasar de `BuildOptions::max_pattern_work` (2²³ palabras, `MAX_SCHEMA_PATTERN_WORK`) es `limit` «trabajo de los `pattern` del esquema» en la ubicación del que lo pasa. Sin él, 200 patrones `^a{0,4000}$` en un `allOf` de 5,6 KB tardaban 39 s, y una cadena XSD de 200 restricciones, 48 s; ahora dan `limit` en 1,2-1,5 s (`mitigaciones.rs::trabajo_de_pattern_por_esquema`).

Glushkov con conjuntos de bits (F4.4b): `first`, `last` y `follow` son bits (2 MiB con 4 096 posiciones), y una concatenación de n hijos se recorre de derecha a izquierda con el `first` del sufijo acumulado. `(a?){4096}` cuesta O(n·m/64), no O(n³).

**Tamaños.** `^(a{p})*$` queda en p estados más el muerto. `^(a+)+$` (el ReDoS clásico) queda en 3, y en M 10⁵ `a` seguidas de `!` cuestan un paso por carácter. `(a|b)*a(a|b){k}` necesita 2^(k+1) estados (las k + 1 últimas letras): con k = 8 son 513 (con el muerto, anclado) y con k = 12 da `limit` antes de crear el estado 4 097. Es el peor caso 2^m de la determinización, que el informe usa para «explosión al determinizar».

**En M (JSON).** El subautómata de una cadena es el **producto** del trie de sus literales (`enum`/`const`) y del AFD de cada `pattern` de los componentes del marco (`allOf`, `$ref` con hermanas), sobre una partición de la cadena que refina la de cada patrón. Se construye contando los estados al crearlos (`ProductLimit` en el 4 097.º) y se minimiza. Cada estado lleva la palabra leída y los bits de los `pattern` que se cumplen. El cierre de la cadena añade **al compilar** un `Fails` por cada `pattern` no cumplido en ese estado: 1 violación por `Pattern`, en la comilla de cierre (desde F4.4b, toda violación del cierre de una cadena se informa ahí). El núcleo no cambia: es δ más comprobaciones. `pattern` sobre una instancia que no es cadena no aplica.

**En M (XSD).** El AFD del texto (espacio léxico con *collapse*, o el trie de los literales en `xs:string`) por el de cada `xs:pattern` de la cadena de restricciones: todas deben cumplirse, y las de un mismo paso ya van unidas con `|`. Si el tipo no es `xs:string`, el patrón lleva delante el mismo transductor de *collapse* (`dfa::collapse`), así que `[0-9]{3}` sobre `xs:int` acepta ` 123 `. Un texto fuera del espacio léxico cuenta 1 y no evalúa el `pattern` (§36 de [pendientes.md](pendientes.md)). Valen el mismo tope y la misma minimización.

**Clases globales** (F4-B3). La partición global de Σ refina los caracteres sueltos de los nombres y literales (cada uno en su clase, como antes) y los conjuntos de todos los `pattern` (`pattern::Partition::refine`, un barrido por los extremos de los intervalos). Una clase puede ser un rango (`[a-z]`) o varios. Sin `pattern`, las clases son las de antes, así que la dorada no cambia.

**Pruebas** (`crates/automaton/tests/pattern.rs`): en 23 patrones, el AFD y `Regex::matches` coinciden en cadenas aleatorias con los caracteres del patrón, `\n`, `é` y un astral (proptest). Además: los tamaños de arriba, el `limit` en M, el ReDoS lineal, JSON sin anclar y solo en cadenas, `pattern` junto a `enum`, longitudes y otro `pattern`, clases con rangos junto a claves, y XSD anclado, con *collapse*, en dos pasos y en un atributo. El corpus añade `invalid-pattern-01` (1 violación en la comilla de cierre), y `non-dpda-pattern-01` da 0.
