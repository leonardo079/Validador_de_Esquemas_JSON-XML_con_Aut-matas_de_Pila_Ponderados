//! F5.4 — Generador de payloads JSON válidos desde la IR, con semilla (D66).
//!
//! Recorre la IR desde la raíz. En cada instancia junta las reglas que se le
//! aplican a la vez: las del nodo, las de sus `$ref` y `allOf`, y las de **una**
//! rama elegida al azar de cada `anyOf`/`oneOf`. Con ellas produce un valor:
//! `const`/`enum` (un literal), `type`, rangos y `multipleOf`, longitudes y
//! `pattern` (por prueba con `ir::regex`), `properties`/`required`/
//! `additionalProperties`/`dependentRequired`, `items`/`prefixItems`,
//! `minItems`/`maxItems`/`uniqueItems` y `min`/`maxProperties`. `not`,
//! `dependentSchemas` y las combinaciones difíciles no se resuelven: el
//! generador **no garantiza** la validez por sí solo; la garantiza
//! [`generar_valido`], que solo acepta un payload que el DPDA **y** la línea
//! base dan por válido, y si no prueba con la semilla siguiente.
//!
//! La profundidad del valor está acotada ([`PROFUNDIDAD`]): pasado el tope se
//! eligen arreglos y objetos mínimos, así que la recursión de [`Gen::valor`]
//! es como mucho de ese orden, nunca proporcional a la entrada (D7). Solo
//! JSON: la IR de XSD (documento, contenido, atributos) queda fuera.

use std::collections::BTreeMap;

use ir::{JsonType, LitToken, Literal, Node, NodeId, Rule, Sort, TypeSet, VerifiedIr};

/// Profundidad a partir de la cual el generador elige valores mínimos.
pub const PROFUNDIDAD: usize = 6;

/// splitmix64: determinista y sin dependencias.
#[derive(Clone, Debug)]
pub struct Azar(u64);

impl Azar {
    pub fn new(semilla: u64) -> Azar {
        Azar(semilla ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Entero en `[lo, hi]` (inclusive).
    pub fn rango(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            lo
        } else {
            lo + self.u64() % (hi - lo + 1)
        }
    }

    pub fn moneda(&mut self) -> bool {
        self.u64() & 1 == 1
    }

    pub fn elegir<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        if v.is_empty() {
            None
        } else {
            Some(&v[(self.u64() % v.len() as u64) as usize])
        }
    }
}

/// Las reglas que se aplican a una instancia (la conjunción).
#[derive(Default)]
struct Conj<'a> {
    nodos: Vec<&'a Node>,
    rechaza: bool,
}

struct Gen<'a> {
    ir: &'a VerifiedIr,
    azar: Azar,
}

impl<'a> Gen<'a> {
    /// Junta los nodos que se aplican a la misma instancia, sin recursión:
    /// `$ref` y `allOf` enteros, una rama de `anyOf`/`oneOf`.
    fn conj(&mut self, raices: &[NodeId]) -> Conj<'a> {
        let mut c = Conj::default();
        let mut pila: Vec<NodeId> = raices.to_vec();
        let mut vistos = std::collections::HashSet::new();
        while let Some(id) = pila.pop() {
            if !vistos.insert(id) {
                continue;
            }
            let n = self.ir.node(id);
            for r in &n.rules {
                match &r.rule {
                    Rule::Reject => c.rechaza = true,
                    Rule::Ref(d) => pila.push(*d),
                    Rule::AllOf(ds) => pila.extend(ds.iter().copied()),
                    Rule::AnyOf(ds) | Rule::OneOf(ds) => {
                        if let Some(d) = self.azar.elegir(ds) {
                            pila.push(*d);
                        }
                    }
                    _ => {}
                }
            }
            c.nodos.push(n);
        }
        c
    }

    fn reglas<'b>(c: &'b Conj<'a>) -> impl Iterator<Item = &'a Rule> + 'b {
        c.nodos.iter().flat_map(|n| n.rules.iter().map(|r| &r.rule))
    }

    /// Un valor JSON que (con suerte) cumple todos `raices` a la vez.
    fn valor(&mut self, raices: &[NodeId], prof: usize) -> Option<String> {
        let c = self.conj(raices);
        if c.rechaza {
            return None;
        }
        // `const` y `enum`: un literal.
        if let Some(l) = Self::reglas(&c).find_map(|r| match r {
            Rule::Const(l) => Some(l),
            _ => None,
        }) {
            return Some(literal_json(l));
        }
        let enums: Vec<&Literal> = Self::reglas(&c)
            .find_map(|r| match r {
                Rule::Enum(ls) => Some(ls.iter().collect()),
                _ => None,
            })
            .unwrap_or_default();
        if let Some(l) = self.azar.elegir(&enums) {
            return Some(literal_json(l));
        }
        match self.tipo(&c, prof)? {
            JsonType::Null => Some("null".into()),
            JsonType::Boolean => Some(if self.azar.moneda() { "true" } else { "false" }.into()),
            JsonType::Integer => self.numero(&c, true),
            JsonType::Number => {
                let entero = self.azar.moneda();
                self.numero(&c, entero)
            }
            JsonType::String => self.cadena(&c).map(|s| cadena_json(&s)),
            JsonType::Array => self.arreglo(&c, prof),
            JsonType::Object => self.objeto(&c, prof),
        }
    }

    /// El tipo: la intersección de los `type`, preferiendo el que piden las
    /// demás reglas; pasado [`PROFUNDIDAD`], los escalares.
    fn tipo(&mut self, c: &Conj<'a>, prof: usize) -> Option<JsonType> {
        let permitidos: Vec<JsonType> = JsonType::ALL
            .into_iter()
            .filter(|t| {
                Self::reglas(c).all(|r| match r {
                    Rule::Type(ts) => contiene(*ts, *t),
                    _ => true,
                })
            })
            .collect();
        let mut pide = Vec::new();
        for r in Self::reglas(c) {
            let t = match r {
                Rule::Properties { .. }
                | Rule::Required(_)
                | Rule::MinProperties(_)
                | Rule::MaxProperties(_)
                | Rule::DependentRequired(_)
                | Rule::DependentSchemas(_) => JsonType::Object,
                Rule::Items { .. } | Rule::MinItems(_) | Rule::MaxItems(_) | Rule::UniqueItems => {
                    JsonType::Array
                }
                Rule::MinLength(_) | Rule::MaxLength(_) | Rule::Pattern(_) => JsonType::String,
                Rule::Minimum(_)
                | Rule::Maximum(_)
                | Rule::ExclusiveMinimum(_)
                | Rule::ExclusiveMaximum(_)
                | Rule::MultipleOf(_) => JsonType::Number,
                _ => continue,
            };
            let t = if t == JsonType::Number && !permitidos.contains(&JsonType::Number) {
                JsonType::Integer
            } else {
                t
            };
            if permitidos.contains(&t) && !pide.contains(&t) {
                pide.push(t);
            }
        }
        if let Some(t) = self.azar.elegir(&pide) {
            return Some(*t);
        }
        let escalares: Vec<JsonType> = permitidos
            .iter()
            .copied()
            .filter(|t| !matches!(t, JsonType::Array | JsonType::Object))
            .collect();
        if prof >= PROFUNDIDAD && !escalares.is_empty() {
            return self.azar.elegir(&escalares).copied();
        }
        self.azar.elegir(&permitidos).copied()
    }

    fn numero(&mut self, c: &Conj<'a>, entero: bool) -> Option<String> {
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        let (mut lo_ex, mut hi_ex) = (false, false);
        let mut multiplo: Option<(f64, usize)> = None;
        for r in Self::reglas(c) {
            match r {
                Rule::Minimum(n) => (lo, lo_ex) = max_cota(lo, lo_ex, num(&n.lexeme), false),
                Rule::ExclusiveMinimum(n) => {
                    (lo, lo_ex) = max_cota(lo, lo_ex, num(&n.lexeme), true)
                }
                Rule::Maximum(n) => (hi, hi_ex) = min_cota(hi, hi_ex, num(&n.lexeme), false),
                Rule::ExclusiveMaximum(n) => {
                    (hi, hi_ex) = min_cota(hi, hi_ex, num(&n.lexeme), true)
                }
                Rule::MultipleOf(n) => multiplo = Some((num(&n.lexeme), decimales(&n.lexeme))),
                _ => {}
            }
        }
        // Intervalo de búsqueda finito alrededor de las cotas.
        let base_lo = if lo.is_finite() {
            lo
        } else if hi.is_finite() {
            hi - 100.0
        } else {
            -10.0
        };
        let base_hi = if hi.is_finite() { hi } else { base_lo + 100.0 };
        let dentro = |x: f64| {
            (if lo_ex { x > lo } else { x >= lo }) && (if hi_ex { x < hi } else { x <= hi })
        };
        for _ in 0..64 {
            let x = match multiplo {
                Some((m, dec)) if m > 0.0 => {
                    let k_lo = (base_lo / m).ceil() as i64;
                    let k_hi = (base_hi / m).floor() as i64;
                    if k_hi < k_lo {
                        return None;
                    }
                    let k = k_lo + (self.azar.rango(0, (k_hi - k_lo).min(1000) as u64) as i64);
                    let v = k as f64 * m;
                    if dentro(v) {
                        return Some(if dec == 0 {
                            format!("{}", v as i64)
                        } else {
                            format!("{v:.dec$}")
                        });
                    }
                    continue;
                }
                _ => {
                    let a = base_lo.ceil() as i64;
                    let b = base_hi.floor() as i64;
                    if entero || b - a >= 1 {
                        if b < a {
                            return None;
                        }
                        (a + self.azar.rango(0, (b - a).min(1_000_000) as u64) as i64) as f64
                    } else {
                        base_lo + (base_hi - base_lo) * (self.azar.rango(1, 99) as f64 / 100.0)
                    }
                }
            };
            if dentro(x) {
                return Some(if x.fract() == 0.0 && x.abs() < 1e15 {
                    format!("{}", x as i64)
                } else {
                    format!("{x}")
                });
            }
        }
        None
    }

    fn cadena(&mut self, c: &Conj<'a>) -> Option<String> {
        let mut min = 0u64;
        let mut max: Option<u64> = None;
        let mut patrones = Vec::new();
        for r in Self::reglas(c) {
            match r {
                Rule::MinLength(n) => min = min.max(*n),
                Rule::MaxLength(n) => max = Some(max.map_or(*n, |m| m.min(*n))),
                Rule::Pattern(p) => {
                    patrones.push(ir::regex::parse(p, ir::regex::Dialect::Ecma).ok()?)
                }
                _ => {}
            }
        }
        let max = max.unwrap_or(min + 8).min(min + 64);
        if max < min {
            return None;
        }
        const LETRAS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
        let glushkov: Vec<ir::regex::Glushkov> =
            patrones.first().map(|r| r.glushkov()).unwrap_or_default();
        let intentos = if patrones.is_empty() { 1 } else { 400 };
        for _ in 0..intentos {
            // Con `pattern`, un paseo por su autómata de Glushkov (casa por
            // construcción); si no, letras al azar.
            let s = match self.azar.elegir(&glushkov) {
                Some(g) => match self.paseo(g, min, max) {
                    Some(s) => s,
                    None => continue,
                },
                None => {
                    let largo = self.azar.rango(min, max.min(min + 12)) as usize;
                    (0..largo)
                        .map(|_| *self.azar.elegir(LETRAS).unwrap_or(&b'a') as char)
                        .collect()
                }
            };
            let n = s.chars().count() as u64;
            if n >= min && n <= max && patrones.iter().all(|r| r.matches(&s)) {
                return Some(s);
            }
        }
        None
    }

    /// Una palabra del lenguaje de `g`: desde una posición de `first`, por
    /// `follow`, hasta una de `last` (iterativo, como mucho `max` + 8 pasos).
    fn paseo(&mut self, g: &ir::regex::Glushkov, min: u64, max: u64) -> Option<String> {
        let m = g.sets.len();
        let en = |b: &[u64]| -> Vec<usize> { (0..m).filter(|&p| ir::regex::bit(b, p)).collect() };
        if g.nullable && min == 0 && self.azar.rango(0, 3) == 0 {
            return Some(String::new());
        }
        let objetivo = self.azar.rango(min.max(1), max.max(1).min(min + 12));
        let mut s = String::new();
        let mut p = *self.azar.elegir(&en(&g.first))?;
        for paso in 1..=(max + 8) {
            s.push(self.caracter(&g.sets[p])?);
            let es_ultima = ir::regex::bit(&g.last, p);
            let siguientes = en(g.follow_of(p));
            if es_ultima && (paso >= objetivo || siguientes.is_empty()) {
                return Some(s);
            }
            p = *self.azar.elegir(&siguientes)?;
        }
        None
    }

    /// Un carácter del conjunto, preferentemente ASCII imprimible.
    fn caracter(&mut self, set: &ir::regex::CharSet) -> Option<char> {
        let ascii: Vec<char> = (0x21u32..0x7f)
            .filter(|&c| {
                ir::regex::contains(set, c) && c != u32::from(b'"') && c != u32::from(b'\\')
            })
            .filter_map(char::from_u32)
            .collect();
        if let Some(c) = self.azar.elegir(&ascii) {
            return Some(*c);
        }
        set.iter().find_map(|&(lo, _)| char::from_u32(lo))
    }

    fn arreglo(&mut self, c: &Conj<'a>, prof: usize) -> Option<String> {
        let mut prefijos: Vec<Vec<NodeId>> = Vec::new();
        let mut resto: Vec<NodeId> = Vec::new();
        let (mut min, mut max) = (0u64, None::<u64>);
        let mut unicos = false;
        for r in Self::reglas(c) {
            match r {
                Rule::Items { prefix, rest } => {
                    for (i, p) in prefix.iter().enumerate() {
                        if prefijos.len() <= i {
                            prefijos.push(Vec::new());
                        }
                        prefijos[i].push(*p);
                    }
                    resto.extend(rest.iter().copied());
                }
                Rule::MinItems(n) => min = min.max(*n),
                Rule::MaxItems(n) => max = Some(max.map_or(*n, |m| m.min(*n))),
                Rule::UniqueItems => unicos = true,
                _ => {}
            }
        }
        let tope = if prof >= PROFUNDIDAD { min } else { min + 3 };
        let largo = self
            .azar
            .rango(min, max.map_or(tope, |m| m.min(tope.max(min))));
        if max.is_some_and(|m| largo > m) {
            return None;
        }
        let mut elems: Vec<String> = Vec::new();
        for i in 0..largo as usize {
            let mut nodos = prefijos.get(i).cloned().unwrap_or_default();
            if i >= prefijos.len() {
                nodos.extend(resto.iter().copied());
            }
            let mut v = None;
            for _ in 0..if unicos { 10 } else { 1 } {
                let x = if nodos.is_empty() {
                    self.azar.rango(0, 1000).to_string()
                } else {
                    self.valor(&nodos, prof + 1)?
                };
                if !unicos || !elems.contains(&x) {
                    v = Some(x);
                    break;
                }
            }
            elems.push(v?);
        }
        Some(format!("[{}]", elems.join(",")))
    }

    fn objeto(&mut self, c: &Conj<'a>, prof: usize) -> Option<String> {
        let mut declaradas: BTreeMap<&str, Vec<NodeId>> = BTreeMap::new();
        let mut adicionales: Vec<NodeId> = Vec::new();
        let mut cerrado = false;
        let mut requeridas: Vec<&str> = Vec::new();
        let mut dependencias: Vec<(&str, &[String])> = Vec::new();
        let (mut min, mut max) = (0u64, None::<u64>);
        for r in Self::reglas(c) {
            match r {
                Rule::Properties {
                    declared,
                    additional,
                } => {
                    for (k, n) in declared {
                        declaradas.entry(k.as_str()).or_default().push(*n);
                    }
                    if let Some(a) = additional {
                        adicionales.push(*a);
                        cerrado |= matches!(
                            self.ir.node(*a).rules.first().map(|r| &r.rule),
                            Some(Rule::Reject)
                        );
                    }
                }
                Rule::Required(ks) => requeridas.extend(ks.iter().map(String::as_str)),
                Rule::DependentRequired(ds) => {
                    dependencias.extend(ds.iter().map(|(k, v)| (k.as_str(), v.as_slice())))
                }
                Rule::MinProperties(n) => min = min.max(*n),
                Rule::MaxProperties(n) => max = Some(max.map_or(*n, |m| m.min(*n))),
                _ => {}
            }
        }
        let mut claves: Vec<String> = Vec::new();
        let poner = |claves: &mut Vec<String>, k: &str| {
            if !claves.iter().any(|x| x == k) {
                claves.push(k.to_owned());
            }
        };
        for k in &requeridas {
            poner(&mut claves, k);
        }
        if prof < PROFUNDIDAD {
            for k in declaradas.keys() {
                if self.azar.moneda() {
                    poner(&mut claves, k);
                }
            }
        }
        // Cierre de `dependentRequired` (iterativo: cada vuelta añade algo).
        loop {
            let antes = claves.len();
            for (k, exige) in &dependencias {
                if claves.iter().any(|x| x == k) {
                    for e in exige.iter() {
                        poner(&mut claves, e);
                    }
                }
            }
            if claves.len() == antes {
                break;
            }
        }
        // `minProperties`: primero las declaradas, luego claves nuevas si se puede.
        let mut extra = 0;
        while (claves.len() as u64) < min {
            if let Some(k) = declaradas.keys().find(|k| !claves.iter().any(|x| x == *k)) {
                poner(&mut claves, k);
            } else if !cerrado {
                extra += 1;
                poner(&mut claves, &format!("extra{extra}"));
            } else {
                return None;
            }
        }
        if max.is_some_and(|m| claves.len() as u64 > m) {
            return None;
        }
        let mut miembros = Vec::new();
        for k in &claves {
            let nodos = declaradas
                .get(k.as_str())
                .cloned()
                .unwrap_or_else(|| adicionales.clone());
            let v = if nodos.is_empty() {
                self.azar.rango(0, 1000).to_string()
            } else {
                self.valor(&nodos, prof + 1)?
            };
            miembros.push(format!("{}:{v}", cadena_json(k)));
        }
        Some(format!("{{{}}}", miembros.join(",")))
    }
}

fn contiene(ts: TypeSet, t: JsonType) -> bool {
    ts.contains(t) || (t == JsonType::Integer && ts.contains(JsonType::Number))
}

fn num(lexema: &str) -> f64 {
    lexema.parse().unwrap_or(0.0)
}

fn decimales(lexema: &str) -> usize {
    let mantisa = lexema.split(['e', 'E']).next().unwrap_or("");
    mantisa.split('.').nth(1).map_or(0, str::len)
}

fn max_cota(c: f64, ex: bool, v: f64, v_ex: bool) -> (f64, bool) {
    if v > c || (v == c && v_ex) {
        (v, v_ex)
    } else {
        (c, ex)
    }
}

fn min_cota(c: f64, ex: bool, v: f64, v_ex: bool) -> (f64, bool) {
    if v < c || (v == c && v_ex) {
        (v, v_ex)
    } else {
        (c, ex)
    }
}

/// Una cadena como literal JSON (RFC 8259).
pub fn cadena_json(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Un literal de la IR (tokens en preorden) como texto JSON, sin recursión.
pub fn literal_json(l: &Literal) -> String {
    /// Un contenedor abierto: su cierre, cuántos elementos le faltan, si aún
    /// no se escribió ninguno y si es un objeto (la coma va antes de la clave).
    struct Abierto {
        cierre: char,
        faltan: usize,
        primero: bool,
        objeto: bool,
    }
    let mut o = String::new();
    let mut pila: Vec<Abierto> = Vec::new();
    for t in &l.0 {
        if let LitToken::Key(k) = t {
            if let Some(top) = pila.last_mut() {
                if !top.primero {
                    o.push(',');
                }
                top.primero = false;
            }
            o.push_str(&cadena_json(k));
            o.push(':');
            continue;
        }
        if let Some(top) = pila.last_mut() {
            if !top.objeto {
                if !top.primero {
                    o.push(',');
                }
                top.primero = false;
            }
        }
        match t {
            LitToken::Null => o.push_str("null"),
            LitToken::Bool(b) => o.push_str(if *b { "true" } else { "false" }),
            LitToken::Number(n) => o.push_str(n),
            LitToken::String(s) => o.push_str(&cadena_json(s)),
            LitToken::Array(n) | LitToken::Object(n) => {
                let objeto = matches!(t, LitToken::Object(_));
                let (abre, cierre) = if objeto { ('{', '}') } else { ('[', ']') };
                o.push(abre);
                if *n > 0 {
                    pila.push(Abierto {
                        cierre,
                        faltan: *n,
                        primero: true,
                        objeto,
                    });
                    continue;
                }
                o.push(cierre);
            }
            LitToken::Key(_) => {}
        }
        // Valor completo: descontarlo y cerrar los contenedores llenos.
        while let Some(top) = pila.last_mut() {
            top.faltan -= 1;
            if top.faltan > 0 {
                break;
            }
            o.push(top.cierre);
            pila.pop();
        }
    }
    o
}

/// Un payload generado con `semilla` (sin comprobar su validez). `None` si
/// el generador no supo construir uno (rango vacío, `pattern` sin acierto…).
pub fn generar(ir: &VerifiedIr, semilla: u64) -> Option<String> {
    if ir.node(NodeId::ROOT).sort != Sort::Json {
        return None;
    }
    let mut g = Gen {
        ir,
        azar: Azar::new(semilla),
    };
    g.valor(&[NodeId::ROOT], 0)
}

/// Un payload **válido**: prueba las semillas `semilla`, `semilla + 1`, … (como
/// mucho `intentos`) y devuelve el primero que el DPDA y `valido` (la línea
/// base, por ejemplo) aceptan, con la semilla que lo produjo.
pub fn generar_valido(
    ir: &VerifiedIr,
    m: &automaton::Automaton,
    semilla: u64,
    intentos: u64,
    mut valido: impl FnMut(&[u8]) -> bool,
) -> Option<(String, u64)> {
    (semilla..semilla.saturating_add(intentos)).find_map(|s| {
        let p = generar(ir, s)?;
        (crate::validar_dpda(m, p.as_bytes()).valido && valido(p.as_bytes())).then_some((p, s))
    })
}
