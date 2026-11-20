//! Expresiones regulares de `pattern` (JSON Schema, ECMA-262 con `u`) y de
//! `xs:pattern` (XSD 1.0 Parte 2, apéndice F), en el subconjunto regular de
//! F4-U14 (`docs/fase4/pattern.md`).
//!
//! [`parse`] analiza un patrón en un AST en arena (los hijos antes que el
//! padre, sin `Box`) sin recursión (D7: la pila explícita es la de los
//! grupos abiertos, como mucho [`MAX_NESTING`]), y rechaza con el constructo
//! y el motivo lo que no es regular o no está en el subconjunto
//! (retrorreferencias, *lookaround*, cuantificadores perezosos, anclas en
//! medio…; D6, mitigación de ReDoS). Las posiciones se cuentan con
//! multiplicación saturada **antes** de expandir `{m,n}`: más de
//! [`MAX_POSITIONS`] es [`RegexErrorKind::Limit`].
//!
//! [`Regex::glushkov`] da, por cada alternativa de primer nivel, el autómata
//! de posiciones (Glushkov, sin ε) con `{m,n}` expandido: lo usan
//! [`Regex::matches`] (simulación del conjunto de posiciones, O(|s|·m²/64),
//! sin retroceso; el oráculo de F4.4b y de D29) y la construcción del AFD de
//! `automaton` (F4.4b). Los caracteres son puntos de código (semántica `u`).

use std::fmt;

/// Dialecto del patrón.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// ECMA-262 con la bandera `u` (JSON Schema 2020-12): búsqueda sin
    /// anclar, `^`/`$` solo al principio o al final de una alternativa de
    /// primer nivel.
    Ecma,
    /// XSD 1.0: anclado implícito, `^` y `$` son literales.
    Xsd,
}

/// Posiciones de Glushkov como mucho, tras expandir `{m,n}` (F4-U16).
pub const MAX_POSITIONS: usize = 4096;
/// Grupos anidados como mucho (F4-U16).
pub const MAX_NESTING: usize = 256;
/// Bytes del constructo citado en un error.
const CLIP: usize = 32;

/// Clase del error (F4-B19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegexErrorKind {
    /// No es una expresión regular del dialecto (paréntesis sin cerrar,
    /// rango al revés, cuantificador sin nada que repetir…).
    Malformed,
    /// Bien formada pero fuera del subconjunto regular (D6).
    Unsupported,
    /// Demasiado grande: posiciones o anidamiento (F4-U16).
    Limit,
}

/// Un patrón rechazado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegexError {
    pub kind: RegexErrorKind,
    /// Índice (en caracteres, desde 0) del constructo en el patrón.
    pub at: usize,
    /// El constructo, recortado y escapado.
    pub construct: String,
    /// Por qué se rechaza.
    pub reason: &'static str,
}

impl fmt::Display for RegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` (carácter {}): {}",
            self.construct,
            self.at + 1,
            self.reason
        )
    }
}

impl std::error::Error for RegexError {}

/// Conjunto de puntos de código: rangos `[lo, hi]` ordenados, disjuntos y no
/// adyacentes, sin los sustitutos.
pub type CharSet = Vec<(u32, u32)>;

/// Nodo del AST: los índices de los hijos son menores que el del padre.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    /// La palabra vacía (un grupo o una alternativa vacíos).
    Empty,
    /// Un carácter de `sets[i]`.
    Set(u32),
    Concat(Vec<u32>),
    Alt(Vec<u32>),
    Repeat {
        child: u32,
        min: u32,
        /// `None`: sin cota.
        max: Option<u32>,
    },
}

/// Una alternativa de primer nivel con sus anclas (en XSD, las dos).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopAlt {
    node: u32,
    pub start: bool,
    pub end: bool,
}

/// Un patrón analizado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regex {
    dialect: Dialect,
    nodes: Vec<Node>,
    sets: Vec<CharSet>,
    alts: Vec<TopAlt>,
    positions: usize,
}

/// El autómata de posiciones de una alternativa de primer nivel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glushkov {
    /// Anclas de la alternativa.
    pub start: bool,
    pub end: bool,
    /// El conjunto de caracteres de cada posición.
    pub sets: Vec<CharSet>,
    /// Si la alternativa acepta la palabra vacía.
    pub nullable: bool,
    /// Palabras de 64 bits de cada conjunto de posiciones.
    pub words: usize,
    /// first y last como conjuntos de bits.
    pub first: Vec<u64>,
    pub last: Vec<u64>,
    /// follow(p) como conjuntos de bits: la fila p es
    /// `follow[p·words .. (p+1)·words]` (O(m²/64) palabras: 2 MiB con 4 096
    /// posiciones, frente a 64 MiB en listas).
    pub follow: Vec<u64>,
}

/// Si el bit `p` está en `b`.
pub fn bit(b: &[u64], p: usize) -> bool {
    b.get(p / 64).is_some_and(|w| w & (1 << (p % 64)) != 0)
}

fn set_bit(b: &mut [u64], p: usize) {
    b[p / 64] |= 1 << (p % 64);
}

fn or_into(a: &mut [u64], b: &[u64]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x |= y;
    }
}

impl Regex {
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Posiciones tras expandir `{m,n}` (≤ [`MAX_POSITIONS`]).
    pub fn positions(&self) -> usize {
        self.positions
    }

    /// Las alternativas de primer nivel.
    pub fn top_alts(&self) -> &[TopAlt] {
        &self.alts
    }

    /// El autómata de Glushkov de cada alternativa de primer nivel.
    pub fn glushkov(&self) -> Vec<Glushkov> {
        self.alts.iter().map(|a| self.glushkov_of(*a)).collect()
    }

    /// Si `s` cumple el patrón: en ECMA, si alguna subcadena de `s` lo
    /// cumple respetando las anclas de su alternativa; en XSD, si `s`
    /// entero lo cumple. Simulación del conjunto de posiciones, sin
    /// retroceso.
    pub fn matches(&self, s: &str) -> bool {
        self.glushkov().iter().any(|g| g.matches(s))
    }

    fn glushkov_of(&self, a: TopAlt) -> Glushkov {
        let mut e = Expanded::default();
        let root = e.expand(self, a.node);
        let mut g = e.positions(root);
        g.start = a.start;
        g.end = a.end;
        g
    }
}

impl Glushkov {
    /// La fila de follow(p).
    pub fn follow_of(&self, p: usize) -> &[u64] {
        &self.follow[p * self.words..(p + 1) * self.words]
    }

    /// Simulación de [`Regex::matches`] para esta alternativa: un paso es
    /// O(m²/64) (las filas de follow de las posiciones activas).
    pub fn matches(&self, s: &str) -> bool {
        let m = self.sets.len();
        let w = self.words;
        let hits = |active: &[u64]| active.iter().zip(&self.last).any(|(a, l)| a & l != 0);
        if !self.end && self.nullable {
            return true;
        }
        let mut active = vec![0u64; w];
        for (i, c) in s.chars().enumerate() {
            let c = u32::from(c);
            let mut next = vec![0u64; w];
            for p in (0..m).filter(|&p| bit(&active, p)) {
                or_into(&mut next, self.follow_of(p));
            }
            if i == 0 || !self.start {
                or_into(&mut next, &self.first);
            }
            for (p, set) in self.sets.iter().enumerate() {
                if bit(&next, p) && !contains(set, c) {
                    next[p / 64] &= !(1 << (p % 64));
                }
            }
            active = next;
            if !self.end && hits(&active) {
                return true;
            }
        }
        hits(&active) || (self.nullable && (!self.start || s.is_empty()))
    }
}

/// `c` está en `s` (búsqueda binaria).
pub fn contains(s: &CharSet, c: u32) -> bool {
    let i = s.partition_point(|&(_, hi)| hi < c);
    s.get(i).is_some_and(|&(lo, _)| lo <= c)
}

// ------------------------------------------------------------ conjuntos ---

const MAX_CP: u32 = 0x10FFFF;
const SUR: (u32, u32) = (0xD800, 0xDFFF);

/// Normaliza: ordena, junta los solapados y adyacentes y quita los
/// sustitutos.
fn normalize(mut v: CharSet) -> CharSet {
    v.sort_unstable();
    let mut out: CharSet = Vec::with_capacity(v.len());
    for (lo, hi) in v {
        match out.last_mut() {
            Some(last) if lo <= last.1.saturating_add(1) => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    let mut res = Vec::with_capacity(out.len() + 1);
    for (lo, hi) in out {
        if hi < SUR.0 || lo > SUR.1 {
            res.push((lo, hi));
        } else {
            if lo < SUR.0 {
                res.push((lo, SUR.0 - 1));
            }
            if hi > SUR.1 {
                res.push((SUR.1 + 1, hi));
            }
        }
    }
    res
}

/// Complemento en [0, 0x10FFFF] sin sustitutos.
fn complement(s: &CharSet) -> CharSet {
    let mut out = Vec::new();
    let mut next = 0u32;
    for &(lo, hi) in s {
        if lo > next {
            out.push((next, lo - 1));
        }
        next = hi.saturating_add(1);
    }
    if next <= MAX_CP {
        out.push((next, MAX_CP));
    }
    normalize(out)
}

fn one(c: u32) -> CharSet {
    vec![(c, c)]
}

fn digit() -> CharSet {
    vec![(0x30, 0x39)]
}

fn word() -> CharSet {
    normalize(vec![(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)])
}

/// `\s` de ECMA-262 (WhiteSpace y LineTerminator).
fn ecma_space() -> CharSet {
    normalize(vec![
        (0x09, 0x0D),
        (0x20, 0x20),
        (0xA0, 0xA0),
        (0x1680, 0x1680),
        (0x2000, 0x200A),
        (0x2028, 0x2029),
        (0x202F, 0x202F),
        (0x205F, 0x205F),
        (0x3000, 0x3000),
        (0xFEFF, 0xFEFF),
    ])
}

/// `\s` de XSD: los cuatro espacios de XML.
fn xsd_space() -> CharSet {
    normalize(vec![(0x09, 0x0A), (0x0D, 0x0D), (0x20, 0x20)])
}

fn dot(d: Dialect) -> CharSet {
    let mut ex = vec![(0x0A, 0x0A), (0x0D, 0x0D)];
    if d == Dialect::Ecma {
        ex.push((0x2028, 0x2029));
    }
    complement(&normalize(ex))
}

// --------------------------------------------------------------- análisis ---

/// Una secuencia (rama de una alternación) en construcción.
#[derive(Default)]
struct Seq {
    items: Vec<u32>,
    start: bool,
    end: bool,
    /// Índice del `$` (ECMA): nada puede seguirle en la rama.
    end_at: usize,
}

/// Un grupo abierto.
struct Group {
    open_at: usize,
    alts: Vec<Seq>,
    cur: Seq,
}

/// Lo que da un escape.
enum Esc {
    Char(u32),
    Set(CharSet),
}

struct Parser {
    d: Dialect,
    cs: Vec<char>,
    i: usize,
    nodes: Vec<Node>,
    sets: Vec<CharSet>,
}

/// Analiza `src` en el dialecto `d`.
pub fn parse(src: &str, d: Dialect) -> Result<Regex, RegexError> {
    let mut p = Parser {
        d,
        cs: src.chars().collect(),
        i: 0,
        nodes: Vec::new(),
        sets: Vec::new(),
    };
    let top = p.run()?;
    let positions = p.count(&top)?;
    Ok(Regex {
        dialect: d,
        nodes: p.nodes,
        sets: p.sets,
        alts: top,
        positions,
    })
}

impl Parser {
    fn err(&self, kind: RegexErrorKind, at: usize, len: usize, reason: &'static str) -> RegexError {
        let end = (at + len.max(1)).min(self.cs.len());
        let mut c = String::new();
        for &ch in self.cs.get(at.min(end)..end).unwrap_or(&[]) {
            if c.len() >= CLIP {
                c.push('…');
                break;
            }
            c.extend(ch.escape_debug());
        }
        RegexError {
            kind,
            at,
            construct: c,
            reason,
        }
    }

    fn malformed(&self, at: usize, len: usize, reason: &'static str) -> RegexError {
        self.err(RegexErrorKind::Malformed, at, len, reason)
    }

    fn unsupported(&self, at: usize, len: usize, reason: &'static str) -> RegexError {
        self.err(RegexErrorKind::Unsupported, at, len, reason)
    }

    fn peek(&self, k: usize) -> Option<char> {
        self.cs.get(self.i + k).copied()
    }

    fn node(&mut self, n: Node) -> u32 {
        self.nodes.push(n);
        (self.nodes.len() - 1) as u32
    }

    fn set(&mut self, s: CharSet) -> u32 {
        self.sets.push(s);
        let i = (self.sets.len() - 1) as u32;
        self.node(Node::Set(i))
    }

    /// El nodo de una secuencia.
    fn seq_node(&mut self, s: &Seq) -> u32 {
        match s.items.as_slice() {
            [] => self.node(Node::Empty),
            [one] => *one,
            many => {
                let v = many.to_vec();
                self.node(Node::Concat(v))
            }
        }
    }

    /// Cierra un grupo: la alternación de sus ramas.
    fn group_node(&mut self, g: Group) -> u32 {
        let mut alts = g.alts;
        alts.push(g.cur);
        let ns: Vec<u32> = alts.iter().map(|s| self.seq_node(s)).collect();
        if ns.len() == 1 {
            ns[0]
        } else {
            self.node(Node::Alt(ns))
        }
    }

    fn run(&mut self) -> Result<Vec<TopAlt>, RegexError> {
        let ecma = self.d == Dialect::Ecma;
        let mut stack: Vec<Group> = vec![Group {
            open_at: 0,
            alts: Vec::new(),
            cur: Seq::default(),
        }];
        while let Some(c) = self.peek(0) {
            let at = self.i;
            let top_level = stack.len() == 1;
            // Un elemento tras el `$` de la rama (ECMA).
            let cur_end = stack.last().is_some_and(|g| g.cur.end);
            if cur_end && c != '|' && c != ')' {
                let e = stack.last().map_or(at, |g| g.cur.end_at);
                return Err(self.unsupported(
                    e,
                    1,
                    "`$` solo puede ir al final de una alternativa de primer nivel",
                ));
            }
            match c {
                '(' => {
                    if stack.len() > MAX_NESTING {
                        return Err(self.err(
                            RegexErrorKind::Limit,
                            at,
                            1,
                            "más de 256 grupos anidados",
                        ));
                    }
                    self.i += 1;
                    if self.peek(0) == Some('?') {
                        if !ecma {
                            return Err(self.malformed(
                                at,
                                2,
                                "`(?` no existe en las expresiones de XSD",
                            ));
                        }
                        match (self.peek(1), self.peek(2)) {
                            (Some(':'), _) => self.i += 2,
                            (Some('=' | '!'), _) => {
                                return Err(self.unsupported(
                                    at,
                                    3,
                                    "lookahead: no es regular en un recorrido de una pasada (D6)",
                                ))
                            }
                            (Some('<'), Some('=' | '!')) => {
                                return Err(self.unsupported(
                                    at,
                                    4,
                                    "lookbehind: no se admite (D6)",
                                ))
                            }
                            (Some('<'), _) => {
                                return Err(self.unsupported(
                                    at,
                                    3,
                                    "grupos con nombre: no se admiten; use `(?:…)`",
                                ))
                            }
                            _ => return Err(self.malformed(at, 3, "grupo `(?` desconocido")),
                        }
                    }
                    stack.push(Group {
                        open_at: at,
                        alts: Vec::new(),
                        cur: Seq::default(),
                    });
                    continue;
                }
                ')' => {
                    if top_level {
                        return Err(self.malformed(at, 1, "`)` sin `(`"));
                    }
                    self.i += 1;
                    let g = stack.pop().expect("grupo abierto");
                    let n = self.group_node(g);
                    stack.last_mut().expect("grupo").cur.items.push(n);
                    continue;
                }
                '|' => {
                    self.i += 1;
                    let g = stack.last_mut().expect("grupo");
                    let s = std::mem::take(&mut g.cur);
                    g.alts.push(s);
                    continue;
                }
                '*' | '+' | '?' | '{' => {
                    let (min, max, len) = self.quantifier(at)?;
                    let g = stack.last_mut().expect("grupo");
                    let Some(child) = g.cur.items.pop() else {
                        return Err(self.malformed(at, len, "cuantificador sin nada que repetir"));
                    };
                    let n = self.node(Node::Repeat { child, min, max });
                    stack.last_mut().expect("grupo").cur.items.push(n);
                    // Tras el cuantificador.
                    match self.peek(0) {
                        Some('?') if ecma => {
                            return Err(self.unsupported(
                                at,
                                len + 1,
                                "cuantificadores perezosos: no se admiten (D6)",
                            ))
                        }
                        Some('*' | '+' | '?') => {
                            return Err(self.malformed(
                                at,
                                len + 1,
                                "dos cuantificadores seguidos (perezoso o posesivo)",
                            ))
                        }
                        Some('{') if self.quantifier_ahead() => {
                            return Err(self.malformed(at, len + 1, "dos cuantificadores seguidos"))
                        }
                        _ => {}
                    }
                    continue;
                }
                '^' if ecma => {
                    self.i += 1;
                    let g = stack.last_mut().expect("grupo");
                    if !top_level || !g.cur.items.is_empty() || g.cur.start {
                        return Err(self.unsupported(
                            at,
                            1,
                            "`^` solo puede ir al principio de una alternativa de primer nivel",
                        ));
                    }
                    g.cur.start = true;
                    continue;
                }
                '$' if ecma => {
                    self.i += 1;
                    if !top_level {
                        return Err(self.unsupported(
                            at,
                            1,
                            "`$` solo puede ir al final de una alternativa de primer nivel",
                        ));
                    }
                    let g = stack.last_mut().expect("grupo");
                    g.cur.end = true;
                    g.cur.end_at = at;
                    continue;
                }
                '[' => {
                    let s = self.class()?;
                    let n = self.set(s);
                    stack.last_mut().expect("grupo").cur.items.push(n);
                    continue;
                }
                ']' | '}' => {
                    return Err(self.malformed(at, 1, "carácter especial sin escapar"));
                }
                '.' => {
                    self.i += 1;
                    let n = self.set(dot(self.d));
                    stack.last_mut().expect("grupo").cur.items.push(n);
                    continue;
                }
                '\\' => {
                    let s = match self.escape(false)? {
                        Esc::Char(c) => one(c),
                        Esc::Set(s) => s,
                    };
                    let n = self.set(s);
                    stack.last_mut().expect("grupo").cur.items.push(n);
                    continue;
                }
                _ => {
                    self.i += 1;
                    let n = self.set(one(u32::from(c)));
                    stack.last_mut().expect("grupo").cur.items.push(n);
                }
            }
        }
        if stack.len() > 1 {
            let at = stack.last().map_or(0, |g| g.open_at);
            return Err(self.malformed(at, 1, "`(` sin cerrar"));
        }
        let g = stack.pop().expect("grupo de primer nivel");
        let mut alts = g.alts;
        alts.push(g.cur);
        let mut top = Vec::with_capacity(alts.len());
        for s in &alts {
            let node = self.seq_node(s);
            top.push(TopAlt {
                node,
                start: s.start || !ecma,
                end: s.end || !ecma,
            });
        }
        Ok(top)
    }

    /// Si en la posición actual empieza un `{m}`, `{m,}` o `{m,n}`.
    fn quantifier_ahead(&self) -> bool {
        let mut k = 1;
        let mut digits = 0;
        while self.peek(k).is_some_and(|c| c.is_ascii_digit()) {
            k += 1;
            digits += 1;
        }
        digits > 0 && matches!(self.peek(k), Some('}' | ','))
    }

    /// Lee un cuantificador: (mín, máx, longitud).
    fn quantifier(&mut self, at: usize) -> Result<(u32, Option<u32>, usize), RegexError> {
        let c = self.peek(0).unwrap_or(' ');
        self.i += 1;
        match c {
            '*' => return Ok((0, None, 1)),
            '+' => return Ok((1, None, 1)),
            '?' => return Ok((0, Some(1), 1)),
            _ => {}
        }
        let num = |p: &mut Self| -> Option<u32> {
            let ini = p.i;
            let mut n: u32 = 0;
            while let Some(d) = p.peek(0).and_then(|c| c.to_digit(10)) {
                n = n.saturating_mul(10).saturating_add(d);
                p.i += 1;
            }
            (p.i > ini).then_some(n)
        };
        let Some(min) = num(self) else {
            return Err(self.malformed(at, 2, "`{` sin cuantificador (escríbalo `\\{`)"));
        };
        let max = match self.peek(0) {
            Some('}') => Some(min),
            Some(',') => {
                self.i += 1;
                num(self)
            }
            _ => return Err(self.malformed(at, self.i - at + 1, "cuantificador `{` mal formado")),
        };
        if self.peek(0) != Some('}') {
            return Err(self.malformed(at, self.i - at + 1, "cuantificador `{` sin cerrar"));
        }
        self.i += 1;
        let len = self.i - at;
        if max.is_some_and(|m| m < min) {
            return Err(self.malformed(at, len, "cuantificador {m,n} con n < m"));
        }
        Ok((min, max, len))
    }

    /// Un escape (el `\` en la posición actual).
    fn escape(&mut self, in_class: bool) -> Result<Esc, RegexError> {
        let at = self.i;
        self.i += 1;
        let Some(c) = self.peek(0) else {
            return Err(self.malformed(at, 1, "`\\` al final del patrón"));
        };
        self.i += 1;
        let ecma = self.d == Dialect::Ecma;
        let ch = |c: char| Ok(Esc::Char(u32::from(c)));
        match c {
            'd' => Ok(Esc::Set(digit())),
            's' if ecma => Ok(Esc::Set(ecma_space())),
            's' => Ok(Esc::Set(xsd_space())),
            'S' if ecma => Ok(Esc::Set(complement(&ecma_space()))),
            'S' => Ok(Esc::Set(complement(&xsd_space()))),
            'n' => ch('\n'),
            'r' => ch('\r'),
            't' => ch('\t'),
            '\\' | '|' | '.' | '?' | '*' | '+' | '(' | ')' | '{' | '}' | '-' | '[' | ']'
            | '^' => ch(c),
            'D' if ecma => Ok(Esc::Set(complement(&digit()))),
            'w' if ecma => Ok(Esc::Set(word())),
            'W' if ecma => Ok(Esc::Set(complement(&word()))),
            'v' if ecma => ch('\u{b}'),
            'f' if ecma => ch('\u{c}'),
            '$' | '/' if ecma => ch(c),
            '0' if ecma && !self.peek(0).is_some_and(|d| d.is_ascii_digit()) => ch('\0'),
            '1'..='9' if ecma => Err(self.unsupported(at, 2, "retrorreferencias: no son regulares (D6)")),
            'k' if ecma => Err(self.unsupported(at, 2, "retrorreferencias con nombre: no son regulares (D6)")),
            'b' | 'B' if ecma => Err(self.unsupported(
                at,
                2,
                if in_class {
                    "`\\b` dentro de una clase no se admite"
                } else {
                    "límites de palabra (`\\b`, `\\B`): no se admiten"
                },
            )),
            'p' | 'P' if ecma => Err(self.unsupported(at, 2, "propiedades Unicode (`\\p{…}`): no se admiten")),
            'c' if ecma => Err(self.unsupported(at, 2, "`\\cX`: no se admite")),
            'x' if ecma => {
                let v = self.hex(2).ok_or_else(|| self.malformed(at, 4, "`\\x` sin dos cifras hexadecimales"))?;
                ch(char::from_u32(v).unwrap_or('\0'))
            }
            'u' if ecma => {
                if self.peek(0) == Some('{') {
                    return Err(self.unsupported(at, 3, "`\\u{…}`: no se admite; use `\\uHHHH`"));
                }
                let hi = self.hex(4).ok_or_else(|| self.malformed(at, 6, "`\\u` sin cuatro cifras hexadecimales"))?;
                if (0xD800..0xDC00).contains(&hi) && self.peek(0) == Some('\\') && self.peek(1) == Some('u') {
                    let save = self.i;
                    self.i += 2;
                    if let Some(lo) = self.hex(4).filter(|lo| (0xDC00..0xE000).contains(lo)) {
                        let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                        return Ok(Esc::Char(cp));
                    }
                    self.i = save;
                }
                match char::from_u32(hi) {
                    Some(c) => ch(c),
                    None => Err(self.unsupported(at, 6, "sustituto UTF-16 suelto: no es un punto de código")),
                }
            }
            'D' | 'w' | 'W' | 'i' | 'I' | 'c' | 'C' | 'p' | 'P' => Err(self.unsupported(
                at,
                2,
                "escape de XSD fuera del subconjunto (`\\D \\w \\W \\i \\I \\c \\C \\p \\P`); use una clase como `[0-9]`",
            )),
            _ => Err(self.malformed(at, 2, "escape desconocido")),
        }
    }

    fn hex(&mut self, n: usize) -> Option<u32> {
        let mut v = 0u32;
        for k in 0..n {
            v = v * 16 + self.peek(k)?.to_digit(16)?;
        }
        self.i += n;
        Some(v)
    }

    /// Una clase `[…]` (el `[` en la posición actual).
    fn class(&mut self) -> Result<CharSet, RegexError> {
        let at = self.i;
        self.i += 1;
        let neg = self.peek(0) == Some('^');
        if neg {
            self.i += 1;
        }
        let ecma = self.d == Dialect::Ecma;
        let mut set: CharSet = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.peek(0) else {
                return Err(self.malformed(at, 1, "`[` sin cerrar"));
            };
            if c == ']' && !(first && !ecma) {
                self.i += 1;
                break;
            }
            first = false;
            if c == '-' && self.peek(1) == Some('[') {
                return Err(self.unsupported(self.i, 2, "resta de clases (`-[…]`): no se admite"));
            }
            if c == '[' && !ecma {
                return Err(self.malformed(self.i, 1, "`[` sin escapar dentro de una clase"));
            }
            let lo_at = self.i;
            let lo = self.class_atom()?;
            // Un rango `a-b` (el `-` final es literal).
            if self.peek(0) == Some('-') && self.peek(1).is_some_and(|c| c != ']') {
                if self.peek(1) == Some('[') {
                    return Err(self.unsupported(
                        self.i,
                        2,
                        "resta de clases (`-[…]`): no se admite",
                    ));
                }
                self.i += 1;
                let hi = self.class_atom()?;
                match (lo, hi) {
                    (Esc::Char(a), Esc::Char(b)) if a <= b => set.push((a, b)),
                    (Esc::Char(_), Esc::Char(_)) => {
                        return Err(self.malformed(lo_at, self.i - lo_at, "rango al revés"))
                    }
                    _ => {
                        return Err(self.malformed(
                            lo_at,
                            self.i - lo_at,
                            "rango con una clase en un extremo",
                        ))
                    }
                }
            } else {
                match lo {
                    Esc::Char(a) => set.push((a, a)),
                    Esc::Set(s) => set.extend(s),
                }
            }
        }
        let set = normalize(set);
        Ok(if neg { complement(&set) } else { set })
    }

    /// Un carácter o un escape de una clase.
    fn class_atom(&mut self) -> Result<Esc, RegexError> {
        match self.peek(0) {
            Some('\\') => self.escape(true),
            Some(c) => {
                self.i += 1;
                Ok(Esc::Char(u32::from(c)))
            }
            None => Err(self.malformed(self.i, 1, "`[` sin cerrar")),
        }
    }

    /// Posiciones tras expandir, con multiplicación saturada (los hijos
    /// antes que el padre: un recorrido por índice).
    fn count(&self, top: &[TopAlt]) -> Result<usize, RegexError> {
        let mut p = vec![0usize; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            p[i] = match n {
                Node::Empty => 0,
                Node::Set(_) => 1,
                Node::Concat(v) | Node::Alt(v) => v
                    .iter()
                    .fold(0usize, |a, &c| a.saturating_add(p[c as usize])),
                Node::Repeat { child, min, max } => {
                    p[*child as usize].saturating_mul(copies(*min, *max) as usize)
                }
            };
        }
        let total = top
            .iter()
            .fold(0usize, |a, t| a.saturating_add(p[t.node as usize]));
        if total > MAX_POSITIONS {
            return Err(self.err(
                RegexErrorKind::Limit,
                0,
                self.cs.len(),
                "más de 4 096 posiciones al expandir los cuantificadores {m,n}",
            ));
        }
        Ok(total)
    }
}

/// Copias del hijo que da `{min,max}` al expandirlo: `max` si está acotado;
/// si no, `max(min, 1)` (`a{m,}` = a^(m−1)·a+).
fn copies(min: u32, max: Option<u32>) -> u32 {
    match max {
        Some(m) => m,
        None => min.max(1),
    }
}

// -------------------------------------------------------------- Glushkov ---

/// El AST con `{m,n}` expandido: solo quedan `*`, `+` y `?`.
#[derive(Clone, Debug)]
enum ENode {
    Empty,
    Pos(u32),
    Cat(Vec<u32>),
    Or(Vec<u32>),
    Star(u32),
    Plus(u32),
    Opt(u32),
}

#[derive(Default)]
struct Expanded {
    nodes: Vec<ENode>,
    sets: Vec<CharSet>,
}

impl Expanded {
    fn push(&mut self, n: ENode) -> u32 {
        self.nodes.push(n);
        (self.nodes.len() - 1) as u32
    }

    /// Expande el subárbol `root` de `r` (los hijos antes que el padre).
    fn expand(&mut self, r: &Regex, root: u32) -> u32 {
        // Los nodos del subárbol, en orden de índice (hijos primero).
        let mut sub = Vec::new();
        let mut st = vec![root];
        let mut seen = std::collections::HashSet::new();
        while let Some(n) = st.pop() {
            if !seen.insert(n) {
                continue;
            }
            sub.push(n);
            match &r.nodes[n as usize] {
                Node::Concat(v) | Node::Alt(v) => st.extend(v.iter().copied()),
                Node::Repeat { child, .. } => st.push(*child),
                Node::Empty | Node::Set(_) => {}
            }
        }
        sub.sort_unstable();
        let mut map = std::collections::HashMap::new();
        for n in sub {
            let e = match &r.nodes[n as usize] {
                Node::Empty => self.push(ENode::Empty),
                Node::Set(s) => {
                    self.sets.push(r.sets[*s as usize].clone());
                    let p = (self.sets.len() - 1) as u32;
                    self.push(ENode::Pos(p))
                }
                Node::Concat(v) => {
                    let v = v.iter().map(|c| map[c]).collect();
                    self.push(ENode::Cat(v))
                }
                Node::Alt(v) => {
                    let v = v.iter().map(|c| map[c]).collect();
                    self.push(ENode::Or(v))
                }
                Node::Repeat { child, min, max } => self.repeat(map[child], *min, *max),
            };
            map.insert(n, e);
        }
        map[&root]
    }

    /// Una copia nueva del subárbol expandido `root` (posiciones nuevas).
    fn clone_tree(&mut self, root: u32) -> u32 {
        let mut sub = Vec::new();
        let mut st = vec![root];
        let mut seen = std::collections::HashSet::new();
        while let Some(n) = st.pop() {
            if !seen.insert(n) {
                continue;
            }
            sub.push(n);
            match &self.nodes[n as usize] {
                ENode::Cat(v) | ENode::Or(v) => st.extend(v.iter().copied()),
                ENode::Star(c) | ENode::Plus(c) | ENode::Opt(c) => st.push(*c),
                ENode::Empty | ENode::Pos(_) => {}
            }
        }
        sub.sort_unstable();
        let mut map = std::collections::HashMap::new();
        for n in sub {
            let e = match self.nodes[n as usize].clone() {
                ENode::Pos(p) => {
                    let s = self.sets[p as usize].clone();
                    self.sets.push(s);
                    let q = (self.sets.len() - 1) as u32;
                    ENode::Pos(q)
                }
                ENode::Cat(v) => ENode::Cat(v.iter().map(|c| map[c]).collect()),
                ENode::Or(v) => ENode::Or(v.iter().map(|c| map[c]).collect()),
                ENode::Star(c) => ENode::Star(map[&c]),
                ENode::Plus(c) => ENode::Plus(map[&c]),
                ENode::Opt(c) => ENode::Opt(map[&c]),
                ENode::Empty => ENode::Empty,
            };
            let id = self.push(e);
            map.insert(n, id);
        }
        map[&root]
    }

    /// `child{min,max}` con `child` ya expandido (lo usa como primera copia).
    fn repeat(&mut self, child: u32, min: u32, max: Option<u32>) -> u32 {
        match (min, max) {
            (0, None) => return self.push(ENode::Star(child)),
            (1, None) => return self.push(ENode::Plus(child)),
            (0, Some(1)) => return self.push(ENode::Opt(child)),
            (_, Some(0)) => return self.push(ENode::Empty),
            (1, Some(1)) => return child,
            _ => {}
        }
        let n = copies(min, max);
        let mut cs = vec![child];
        for _ in 1..n {
            let c = self.clone_tree(child);
            cs.push(c);
        }
        match max {
            None => {
                // a^(m−1) · a+
                let last = cs.pop().expect("al menos una copia");
                let p = self.push(ENode::Plus(last));
                cs.push(p);
                self.push(ENode::Cat(cs))
            }
            Some(_) => {
                // a^min · (a · (a · …)?)?
                let opt: Vec<u32> = cs.split_off(min as usize);
                let mut tail: Option<u32> = None;
                for c in opt.into_iter().rev() {
                    let inner = match tail {
                        None => c,
                        Some(t) => self.push(ENode::Cat(vec![c, t])),
                    };
                    tail = Some(self.push(ENode::Opt(inner)));
                }
                cs.extend(tail);
                match cs.as_slice() {
                    [one] => *one,
                    _ => self.push(ENode::Cat(cs)),
                }
            }
        }
    }

    /// first, last, nullable y follow de `root` (los hijos antes que el
    /// padre: un recorrido por índice; las posiciones se renumeran por orden
    /// de aparición). Conjuntos de bits: una concatenación de n hijos cuesta
    /// O(n·m/64) con los sufijos de first acumulados de derecha a izquierda,
    /// así que `(a?){4096}` no es cúbico.
    fn positions(&self, root: u32) -> Glushkov {
        // Las posiciones del subárbol, renumeradas en orden de aparición.
        let mut used = vec![u32::MAX; self.sets.len()];
        let mut sets = Vec::new();
        for i in 0..=root as usize {
            if let ENode::Pos(p) = self.nodes[i] {
                used[p as usize] = sets.len() as u32;
                sets.push(self.sets[p as usize].clone());
            }
        }
        let m = sets.len();
        let w = m.div_ceil(64).max(1);
        let n = root as usize + 1;
        let mut nullable = vec![false; n];
        let mut first = vec![0u64; n * w];
        let mut last = vec![0u64; n * w];
        let mut follow = vec![0u64; m * w];
        let row = |v: &[u64], i: usize| v[i * w..(i + 1) * w].to_vec();
        for i in 0..n {
            match &self.nodes[i] {
                ENode::Empty => nullable[i] = true,
                ENode::Pos(p) => {
                    let q = used[*p as usize] as usize;
                    set_bit(&mut first[i * w..(i + 1) * w], q);
                    set_bit(&mut last[i * w..(i + 1) * w], q);
                }
                ENode::Or(v) => {
                    for &c in v {
                        let c = c as usize;
                        let (f, l) = (row(&first, c), row(&last, c));
                        or_into(&mut first[i * w..(i + 1) * w], &f);
                        or_into(&mut last[i * w..(i + 1) * w], &l);
                        nullable[i] |= nullable[c];
                    }
                }
                ENode::Cat(v) => {
                    // De derecha a izquierda: `suf` = first de lo que sigue a
                    // c_j hasta el primer hijo no anulable.
                    let mut suf = vec![0u64; w];
                    let mut tail_nullable = true;
                    let mut l = vec![0u64; w];
                    for &c in v.iter().rev() {
                        let c = c as usize;
                        for q in 0..m {
                            if bit(&last[c * w..(c + 1) * w], q) {
                                or_into(&mut follow[q * w..(q + 1) * w], &suf);
                            }
                        }
                        if tail_nullable {
                            or_into(&mut l, &last[c * w..(c + 1) * w]);
                        }
                        tail_nullable &= nullable[c];
                        if !nullable[c] {
                            suf = row(&first, c);
                        } else {
                            let f = row(&first, c);
                            or_into(&mut suf, &f);
                        }
                    }
                    // first: los hijos hasta el primero no anulable.
                    let mut f = vec![0u64; w];
                    for &c in v {
                        let c = c as usize;
                        or_into(&mut f, &first[c * w..(c + 1) * w]);
                        if !nullable[c] {
                            break;
                        }
                    }
                    nullable[i] = tail_nullable;
                    first[i * w..(i + 1) * w].copy_from_slice(&f);
                    last[i * w..(i + 1) * w].copy_from_slice(&l);
                }
                ENode::Star(c) | ENode::Plus(c) | ENode::Opt(c) => {
                    let c = *c as usize;
                    let (f, l) = (row(&first, c), row(&last, c));
                    first[i * w..(i + 1) * w].copy_from_slice(&f);
                    last[i * w..(i + 1) * w].copy_from_slice(&l);
                    nullable[i] = nullable[c] || !matches!(self.nodes[i], ENode::Plus(_));
                    if !matches!(self.nodes[i], ENode::Opt(_)) {
                        for q in 0..m {
                            if bit(&l, q) {
                                or_into(&mut follow[q * w..(q + 1) * w], &f);
                            }
                        }
                    }
                }
            }
        }
        let r = root as usize;
        Glushkov {
            start: false,
            end: false,
            sets,
            nullable: nullable[r],
            words: w,
            first: row(&first, r),
            last: row(&last, r),
            follow,
        }
    }
}
