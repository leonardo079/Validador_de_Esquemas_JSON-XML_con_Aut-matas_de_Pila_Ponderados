//! Formato de las tablas de M que lee el núcleo ([`crate::Core`]).
//!
//! Las construye y presta `automaton` (`Automaton::tables`) y, en la Fase 5,
//! las emite el generador como `const`. Son *slices* planos de registros de
//! tamaño fijo, sin punteros entre sí (los enlaces son índices `u32`), para
//! que el mismo formato sirva en Rust y en C. Teoría en
//! `docs/fase4/automata.md` §3-§4.
//!
//! Campos reservados para los combinadores (F4.5a): `Check::slot` (siempre
//! 0), `Frame::slots`/`Frame::gates` (vacíos), `Trans::outs` (siempre
//! [`OUT_IDENTITY`] en las transiciones que apilan o abren un escalar) y
//! `Tables::scalar_slots` (la zona global de salidas de la instancia escalar
//! abierta, 0 palabras; `docs/fase4/automata.md` §3.6). El núcleo de F4.1b
//! no los lee.

/// Clases fijas de Σ: los eventos estructurales del lexer JSON. Las clases
/// de carácter de cadena van a continuación ([`class::CHAR`] + la clase del
/// punto de código, [`Tables::char_class`]).
pub mod class {
    pub const BEGIN_OBJECT: u32 = 0;
    pub const END_OBJECT: u32 = 1;
    pub const BEGIN_ARRAY: u32 = 2;
    pub const END_ARRAY: u32 = 3;
    pub const COLON: u32 = 4;
    pub const COMMA: u32 = 5;
    pub const TRUE: u32 = 6;
    pub const FALSE: u32 = 7;
    pub const NULL: u32 = 8;
    pub const STRING_BEGIN: u32 = 9;
    pub const STRING_END: u32 = 10;
    pub const NUMBER_BEGIN: u32 = 11;
    /// Cualquier byte del lexema de un número (la parte va a los registros
    /// del número, no a δ).
    pub const NUMBER_BYTE: u32 = 12;
    pub const NUMBER_END: u32 = 13;
    /// Primera clase de carácter de cadena: `CHAR + 0` es «cualquier otro
    /// punto de código».
    pub const CHAR: u32 = 14;

    /// Nombres de las clases fijas (DOT y mensajes).
    pub const NAMES: [&str; CHAR as usize] = [
        "{", "}", "[", "]", ":", ",", "true", "false", "null", "\"", "\"$", "num", "byte num",
        "num$",
    ];

    /// Clases que empiezan un valor.
    pub const VALUE_STARTS: [u32; 7] = [
        BEGIN_OBJECT,
        BEGIN_ARRAY,
        STRING_BEGIN,
        NUMBER_BEGIN,
        TRUE,
        FALSE,
        NULL,
    ];
}

/// Sin bit de máscara que fijar ([`Trans::bit`]).
pub const NO_BIT: u32 = u32::MAX;

/// Mapa de salidas identidad ([`Trans::outs`]; F4.5a añadirá otros).
pub const OUT_IDENTITY: u32 = u32::MAX;

/// Error de peso ∞ que da una transición o el propio núcleo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fault {
    /// Evento que la gramática de JSON no admite en ese punto.
    Syntax,
    /// Clave declarada repetida en un objeto (D2): malformado.
    Duplicate,
    /// La memoria del llamador no admite otro marco (profundidad).
    Limit,
    /// Tablas incoherentes (índice fuera de rango): defensa en
    /// profundidad, nunca un pánico.
    Internal,
}

/// Operación de pila de una transición.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// Cambia de estado (`next`).
    Move,
    /// Apila un marco del tipo `frame` con retorno `ret` y va a `next`.
    Push,
    /// Desapila y vuelve al estado de retorno del marco.
    Pop,
    /// Abre un escalar (cadena o número): guarda `ret` en el registro del
    /// escalar abierto y va a `next`.
    Enter,
    /// Cierra el escalar: vuelve al estado guardado por [`Op::Enter`].
    Leave,
    /// Error de peso ∞.
    Fail(Fault),
}

/// Una transición: δ(q, a, X) = (q', operación de pila, registros,
/// comprobaciones).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Trans {
    pub op: Op,
    pub next: u32,
    /// Estado de retorno ([`Op::Push`], [`Op::Enter`]).
    pub ret: u32,
    /// Tipo de marco que apila ([`Op::Push`]).
    pub frame: u32,
    /// Bit de la máscara del marco de la cima que se fija; si ya estaba,
    /// la clave está repetida ([`Fault::Duplicate`]). [`NO_BIT`] si no.
    pub bit: u32,
    /// Suma 1 (saturado) al contador del marco de la cima (miembros de un
    /// objeto o elementos de un arreglo) **antes** de apilar nada.
    pub count: bool,
    /// Comprobaciones `checks[checks .. checks + n_checks]`, ordenadas por
    /// costo.
    pub checks: u32,
    pub n_checks: u32,
    /// Mapa de salidas (F4.5a); [`OUT_IDENTITY`].
    pub outs: u32,
}

/// Qué comprueba una [`Check`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Test {
    /// Falla siempre: decidido al compilar (un `type` que no admite el
    /// evento, `false`).
    Fails,
    /// El número que se cierra es entero.
    Integer,
    /// La máscara del marco de la cima contiene `masks[arg ..]`.
    Mask,
    /// Para cada `deps[arg .. arg + len]` cuyo bit está, la máscara
    /// contiene la suya.
    Dependent,
    /// El contador del marco de la cima es ≥ [`Check::bound`]
    /// (`minProperties`, `minItems`).
    CountMin,
    /// El contador del marco de la cima es ≤ [`Check::bound`]
    /// (`maxProperties`, `maxItems`).
    CountMax,
    /// Los puntos de código de la cadena que se cierra son ≥
    /// [`Check::bound`] (`minLength`).
    LengthMin,
    /// … ≤ [`Check::bound`] (`maxLength`).
    LengthMax,
    /// El número que se cierra es ≥ `nums[arg]` (`minimum`).
    NumMin,
    /// … > `nums[arg]` (`exclusiveMinimum`).
    NumExMin,
    /// … ≤ `nums[arg]` (`maximum`).
    NumMax,
    /// … < `nums[arg]` (`exclusiveMaximum`).
    NumExMax,
    /// … es igual por valor a uno de `nums[arg .. arg + len]`, ordenados
    /// por valor y sin repetir (`enum`/`const` de números).
    NumEnum,
    /// … es múltiplo de `divisors[arg]` (`multipleOf`).
    MultipleOf,
}

/// Una comprobación de una regla del esquema; si falla suma 1 al peso.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Check {
    pub test: Test,
    /// Índice en [`Tables::rules`].
    pub rule: u32,
    /// Costo estático c (`automaton::weight::cost`); ordena las
    /// comprobaciones de la transición.
    pub cost: u32,
    /// Salida a la que cuenta (F4.5a); siempre 0.
    pub slot: u32,
    pub arg: u32,
    pub len: u32,
}

impl Check {
    /// Cota de 64 bits de las pruebas de contador y longitud: `arg` es la
    /// mitad baja y `len` la alta.
    pub const fn bound(&self) -> u64 {
        (self.arg as u64) | ((self.len as u64) << 32)
    }

    /// `(arg, len)` de la cota `n` ([`Check::bound`]).
    pub const fn split(n: u64) -> (u32, u32) {
        (n as u32, (n >> 32) as u32)
    }
}

/// Tipo de marco: disposición de sus registros en la pila. Las palabras
/// de registros son `[máscara (mask_words)] [contador (0 o 1)]`: la máscara
/// de nombres va primero y el contador de miembros o de elementos, si lo
/// hay, en la palabra `mask_words` (F4.1a-CR-S1: [`Test::Mask`] y
/// [`Test::Dependent`] solo leen las `mask_words` primeras).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Frame {
    /// Palabras `u64` de registros (máscara y contador).
    pub words: u32,
    /// Palabras de la máscara (`words − mask_words` ≤ 1 es el contador).
    pub mask_words: u32,
    /// Salidas (F4.5a); 0.
    pub slots: u32,
    /// Puertas `gates[gates .. gates + n_gates]` (F4.5a); vacías.
    pub gates: u32,
    pub n_gates: u32,
}

/// Una entrada de `dependentRequired`: si está `bit`, deben estar los de
/// `masks[mask ..]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dep {
    pub bit: u32,
    pub mask: u32,
}

/// Un lexema de RFC 8259 del esquema (cota, valor de `enum`/`const` o
/// divisor) en `lexemes[off .. off + len]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lexeme {
    pub off: u32,
    pub len: u32,
}

/// Un divisor de `multipleOf`: su lexema (`nums[lexeme]`) y su análisis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DivisorRef {
    pub lexeme: u32,
    pub div: crate::num::Divisor,
}

/// Rango de puntos de código `[lo, hi]` de una clase de carácter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CharRange {
    pub lo: u32,
    pub hi: u32,
    pub class: u32,
}

/// Regla del esquema a la que se atribuye una comprobación.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rule {
    /// `NodeId` de la IR.
    pub node: u32,
    /// Índice en `ir::RuleKind::ALL`.
    pub kind: u32,
    /// Ubicación recortada (`automaton::location_clipped`, ≤ 256 bytes) en
    /// `text[loc .. loc + loc_len]`.
    pub loc: u32,
    pub loc_len: u32,
}

/// Las tablas de M, prestadas.
#[derive(Clone, Copy, Debug)]
pub struct Tables<'a> {
    /// |Σ|: [`class::CHAR`] más las clases de carácter.
    pub n_classes: u32,
    /// q₀.
    pub start: u32,
    /// F = {`accept`}, con la pila vacía.
    pub accept: u32,
    /// δ densa y total: `delta[q * n_classes + a]` es un índice en `trans`.
    pub delta: &'a [u32],
    pub trans: &'a [Trans],
    pub checks: &'a [Check],
    pub frames: &'a [Frame],
    pub masks: &'a [u64],
    pub deps: &'a [Dep],
    /// Lexemas numéricos del esquema, en `lexemes`.
    pub nums: &'a [Lexeme],
    pub lexemes: &'a [u8],
    pub divisors: &'a [DivisorRef],
    /// Palabras de la zona global de salidas de la instancia escalar
    /// abierta, al principio de la memoria del llamador (F4.5a; 0 en
    /// F4.1b). La pila de marcos empieza detrás.
    pub scalar_slots: u32,
    /// Clase de carácter de cada punto de código ASCII (128 entradas).
    pub ascii: &'a [u32],
    /// Clases del resto de puntos de código, ordenadas y disjuntas; lo que
    /// no está en ninguna es la clase 0.
    pub ranges: &'a [CharRange],
    pub rules: &'a [Rule],
    pub text: &'a [u8],
}

impl<'a> Tables<'a> {
    /// Clase de carácter de `c` (sin sumar [`class::CHAR`]): tabla densa
    /// en ASCII y búsqueda binaria en el resto (O(log k), k fijo por
    /// esquema).
    pub fn char_class(&self, c: char) -> u32 {
        let cp = u32::from(c);
        if cp < 128 {
            return self.ascii.get(cp as usize).copied().unwrap_or(0);
        }
        let i = self.ranges.partition_point(|r| r.hi < cp);
        match self.ranges.get(i) {
            Some(r) if r.lo <= cp => r.class,
            _ => 0,
        }
    }

    /// Número de estados.
    pub fn n_states(&self) -> usize {
        match self.n_classes {
            0 => 0,
            n => self.delta.len() / n as usize,
        }
    }

    /// El lexema `nums[i]`, o `None` si las tablas son incoherentes.
    pub fn num(&self, i: u32) -> Option<&'a [u8]> {
        let l = self.nums.get(i as usize)?;
        let ini = l.off as usize;
        self.lexemes.get(ini..ini.checked_add(l.len as usize)?)
    }

    /// Ubicación recortada de la regla `rule`, o `""`.
    pub fn location(&self, rule: u32) -> &'a [u8] {
        let Some(r) = self.rules.get(rule as usize) else {
            return &[];
        };
        let ini = r.loc as usize;
        let fin = ini.saturating_add(r.loc_len as usize);
        self.text.get(ini..fin).unwrap_or(&[])
    }
}
