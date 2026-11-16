//! Formato de las tablas de M que lee el núcleo ([`crate::Core`]).
//!
//! Las construye y presta `automaton` (`Automaton::tables`) y, en la Fase 5,
//! las emite el generador como `const`. Son *slices* planos de registros de
//! tamaño fijo, sin punteros entre sí (los enlaces son índices `u32`), para
//! que el mismo formato sirva en Rust y en C. Teoría en
//! `docs/fase4/automata.md` §3-§4.
//!
//! Campos reservados para los combinadores (F4.5a): `Check::slot` (siempre
//! 0), `Frame::slots`/`Frame::gates` (vacíos) y `Trans::outs` (siempre
//! [`OUT_IDENTITY`] en las transiciones que apilan). El núcleo de F4.1a no
//! los lee.

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
        "{", "}", "[", "]", ":", ",", "true", "false", "null", "\"", "\"$", "num", "dígito", "num$",
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

/// Tipo de marco: disposición de sus registros en la pila.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Frame {
    /// Palabras `u64` de registros (la máscara).
    pub words: u32,
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
