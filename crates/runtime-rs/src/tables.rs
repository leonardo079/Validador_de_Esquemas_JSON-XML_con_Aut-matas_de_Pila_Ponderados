//! Formato de las tablas de M que lee el núcleo ([`crate::Core`]).
//!
//! Las construye y presta `automaton` (`Automaton::tables`) y, en la Fase 5,
//! las emite el generador como `const`. Son *slices* planos de registros de
//! tamaño fijo, sin punteros entre sí (los enlaces son índices `u32`), para
//! que el mismo formato sirva en Rust y en C. Teoría en
//! `docs/fase4/automata.md` §3-§4.
//!
//! Combinadores (F4.5a, `docs/fase4/automata.md` §18): cada comprobación
//! cuenta a un destino (`Check::slot`: 0 el peso global, k ≥ 1 la salida k
//! de la instancia; con [`SLOT_CHILD`], la de la instancia que abre la
//! transición), cada tipo de marco tiene sus salidas (`Frame::slots`) y sus
//! puertas (`Frame::gates`), que se evalúan al cerrar la instancia, y la
//! transición que abre una instancia con salidas lleva el mapa que las suma
//! a las de la instancia padre (`Trans::outs`; [`OUT_IDENTITY`] si no tiene
//! salidas). Las instancias escalares usan la zona global de
//! `Tables::scalar_slots` palabras al principio de la memoria.

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

/// Clases fijas de Σ en XML (F4.1c): los eventos del lexer XML que no son
/// caracteres. Los caracteres (de nombres, valores de atributo, texto e
/// instrucciones de procesamiento) van, como en JSON, a [`class::CHAR`] +
/// la clase del punto de código; el estado distingue el contexto. Las
/// clases 10-13 no se usan en XML.
pub mod xclass {
    /// `<` de una etiqueta de apertura (`StartTagOpen`).
    pub const START_TAG: u32 = 0;
    /// `</` (`EndTagOpen`).
    pub const END_TAG: u32 = 1;
    /// Comienzo del nombre de un atributo (`AttrBegin`).
    pub const ATTR: u32 = 2;
    /// Comilla de apertura del valor (`AttrValueBegin`).
    pub const VALUE_BEGIN: u32 = 3;
    /// Comilla de cierre del valor (`AttrValueEnd`).
    pub const VALUE_END: u32 = 4;
    /// `>` de una etiqueta de apertura (`StartTagClose`).
    pub const TAG_CLOSE: u32 = 5;
    /// `/>` (`EmptyTagClose`).
    pub const EMPTY_CLOSE: u32 = 6;
    /// `>` de una etiqueta de cierre (`EndTagClose`).
    pub const END_CLOSE: u32 = 7;
    /// `<?` (`PiOpen`).
    pub const PI_OPEN: u32 = 8;
    /// `?>` (`PiClose`).
    pub const PI_CLOSE: u32 = 9;
    /// Comienzo del contenido de una instrucción: el núcleo la pone delante
    /// del primer carácter tras el nombre, que el lexer da con la misma
    /// clase que los del nombre (`<?xmla?>` frente a `<?xml a?>`).
    pub const PI_BODY: u32 = 10;

    /// Nombres de las clases fijas en XML (DOT y mensajes).
    pub const NAMES: [&str; super::class::CHAR as usize] = [
        "<", "</", "@", "=\"", "\"$", ">", "/>", "</…>", "<?", "?>", "<?… ", "—", "—", "—",
    ];

    /// Las clases que terminan el nombre de una etiqueta de apertura.
    pub const NAME_ENDS: [u32; 3] = [ATTR, TAG_CLOSE, EMPTY_CLOSE];
}

/// Sin bit de máscara que fijar ([`Trans::bit`], [`Trans::mark`]).
pub const NO_BIT: u32 = u32::MAX;

/// La instancia que abre la transición no tiene salidas ([`Trans::outs`]).
pub const OUT_IDENTITY: u32 = u32::MAX;

/// [`Check::slot`]: la comprobación cuenta en la instancia que abre la
/// transición (las de entrada de un valor), no en la actual.
pub const SLOT_CHILD: u32 = 1 << 31;

/// Qué combina una [`Gate`] al cerrar la instancia.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GateKind {
    /// `anyOf`: +1 al destino si todas las entradas son > 0.
    Any,
    /// `oneOf`: +1 si no hay exactamente una entrada en 0.
    One,
    /// `not`: +1 si la entrada es 0.
    Not,
    /// `dependentSchemas`: si el bit `Gate::bit` de la máscara está, suma
    /// la entrada al destino.
    Implies,
}

/// Una puerta: combina las salidas `gate_inputs[inputs .. inputs + n_inputs]`
/// de la instancia y suma a `target` (0: el peso global).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Gate {
    pub kind: GateKind,
    /// Regla a la que se atribuye (índice en [`Tables::rules`]).
    pub rule: u32,
    pub target: u32,
    pub inputs: u32,
    pub n_inputs: u32,
    /// [`GateKind::Implies`]: bit de la máscara; [`NO_BIT`] si nunca salta.
    pub bit: u32,
}

/// Un mapa de salidas: la salida j + 1 de la instancia que se cierra se
/// suma a la `out_targets[off + j]` de la instancia padre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutMap {
    pub off: u32,
    pub len: u32,
}

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
    /// la clave (o el atributo XML) está repetida ([`Fault::Duplicate`]).
    /// [`NO_BIT`] si no. Se fija antes de las comprobaciones.
    pub bit: u32,
    /// Bit de la máscara de la cima que se fija **sin** error si ya estaba
    /// (hijos de `xs:all`: la repetición la cuenta [`Test::Unset`]); se fija
    /// después de las comprobaciones. [`NO_BIT`] si no.
    pub mark: u32,
    /// Operaciones de contador `counter_ops[counts .. counts + n_counts]`
    /// sobre el marco de la cima (F4.1b-CR-S5: k contadores por marco), en
    /// orden, **después** de las comprobaciones y antes de apilar nada.
    pub counts: u32,
    pub n_counts: u32,
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
    /// El contador [`Check::counter`] del marco de la cima es ≥
    /// [`Check::bound`] (`minProperties`, `minItems`, el mínimo de una
    /// partícula XSD al salir de ella).
    CountMin,
    /// … ≤ [`Check::bound`] (`maxProperties`, `maxItems`, el máximo de una
    /// partícula XSD al volver a empezarla).
    CountMax,
    /// El bit `arg` de la máscara de la cima no está (un hijo de `xs:all`
    /// que no se repite).
    Unset,
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
    /// Los elementos del arreglo que se cierra son distintos por valor
    /// (`uniqueItems`, F4.5b): sus hashes canónicos, ordenados, sin dos
    /// iguales seguidos.
    Unique,
    /// … está en uno de los intervalos `intervals[arg .. arg + len]`,
    /// ordenados y disjuntos (`enum`/`fixed` de `xs:float`/`xs:double`: el
    /// intervalo de redondeo de cada valor, F4.1c).
    NumIn,
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
    /// Destino (F4.5a): 0 el peso global, k la salida k; con
    /// [`SLOT_CHILD`], de la instancia que abre la transición.
    pub slot: u32,
    /// Contador del marco que leen [`Test::CountMin`] y [`Test::CountMax`]
    /// (F4.1b-CR-S5); 0 en las demás.
    pub counter: u32,
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
/// de registros son `[máscara (mask_words)] [contadores (k)] [salidas
/// (slots)]`: la máscara de nombres va primero, el contador i en la palabra
/// `mask_words + i` y la salida j (desde 1) en `words − slots + j − 1`
/// (F4.1a-CR-S1: [`Test::Mask`] y [`Test::Dependent`] solo leen las
/// `mask_words` primeras; k = `words − mask_words − slots`). Un tipo de
/// marco con `words` = 0 y salidas es el de una instancia escalar (no se
/// apila; usa la zona global).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Frame {
    /// Palabras `u64` de registros (máscara y contadores).
    pub words: u32,
    /// Palabras de la máscara (`words − mask_words` son los contadores).
    pub mask_words: u32,
    /// Salidas (F4.5a).
    pub slots: u32,
    /// Puertas `gates[gates .. gates + n_gates]`, en orden de evaluación.
    pub gates: u32,
    pub n_gates: u32,
    /// Arreglo con `uniqueItems` (F4.5b): el hash canónico de cada elemento
    /// va a la reserva y [`Test::Unique`] la comprueba al cerrarlo.
    pub unique: bool,
}

/// Qué hace una [`CounterOp`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CountOp {
    /// Suma 1, saturado.
    Add,
    /// Reinicio: el contador pasa a 1 (empieza la primera vuelta de una
    /// partícula XSD).
    Restart,
}

/// Una operación sobre el contador `counter` del marco de la cima.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CounterOp {
    pub counter: u32,
    pub op: CountOp,
}

/// Un intervalo de lexemas `[nums[lo], nums[hi]]` ([`Test::NumIn`]),
/// cerrado si `closed` y abierto si no (los dos extremos a la vez: el
/// intervalo de redondeo de un flotante de mantisa par es cerrado).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Interval {
    pub lo: u32,
    pub hi: u32,
    pub closed: bool,
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
    /// Operaciones de contador de las transiciones ([`Trans::counts`]).
    pub counter_ops: &'a [CounterOp],
    /// Intervalos de [`Test::NumIn`].
    pub intervals: &'a [Interval],
    /// Puertas de los tipos de marco y sus entradas (F4.5a).
    pub gates: &'a [Gate],
    pub gate_inputs: &'a [u32],
    /// Mapas de salidas ([`Trans::outs`]) y sus destinos.
    pub outmaps: &'a [OutMap],
    pub out_targets: &'a [u32],
    /// Palabras de la zona global de salidas de la instancia escalar
    /// abierta, al principio de la memoria del llamador (F4.5a). La pila de
    /// marcos empieza detrás.
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

    /// Palabras `u64` de memoria que el llamador presta al núcleo para
    /// validar con `limits` (F4-U18): la zona del escalar, `max_depth` marcos
    /// del tipo más grande (registros y [`crate::FRAME_OVERHEAD`]) y la
    /// reserva de hashes de la capa de buena formación y de `uniqueItems`.
    /// Saturado: con unos
    /// límites absurdos, `usize::MAX` (el llamador no podrá reservarla).
    pub fn state_words(&self, limits: &crate::Limits) -> usize {
        let frame = self
            .frames
            .iter()
            .map(|f| f.words as usize)
            .max()
            .unwrap_or(0)
            .saturating_add(crate::FRAME_OVERHEAD);
        (self.scalar_slots as usize)
            .saturating_add(limits.max_depth.saturating_mul(frame))
            .saturating_add(limits.key_pool)
            .saturating_add(limits.max_unique)
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
