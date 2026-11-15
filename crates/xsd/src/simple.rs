//! Tipos simples del usuario (tarea 5.6, `docs/fase3/xsd.md` §10): la
//! etapa 2 del análisis (B2).
//!
//! Con las bases ya resueltas en la etapa 1, para cada `xs:simpleType`:
//!
//! 1. **derivación circular**: la cadena de bases de cada tipo (grado de
//!    salida ≤ 1) se sigue con una marca de tres estados y una pila
//!    explícita, **antes** de usarla (D7, D15); cada ciclo da un error;
//! 2. en **orden topológico** de la cadena (la base antes que el derivado):
//!    el primitivo (`Lexical`) heredado, las facetas propias (I13: que se
//!    apliquen al primitivo; I4: sin repetir y sin `length` con
//!    `minLength`/`maxLength`; ni `min*`/`max*` inclusivo y exclusivo a la
//!    vez), sus valores con `ir::num` (5.5-P1), *facet valid restriction*
//!    respecto de las facetas efectivas de la base (también los rangos de
//!    los predefinidos acotados, 5.5-P2) y las enumeraciones, que deben ser
//!    valores de la base;
//! 3. `default`/`fixed` de cada atributo como valores de su tipo (salvo
//!    `pattern`, D29).
//!
//! Un tipo con algún error (o en un ciclo) queda **envenenado**: sus
//! derivados y los atributos que lo usan no se comprueban (serían
//! consecuencias del mismo error). Las facetas efectivas de un tipo no se
//! copian por la cadena: cada cota apunta al tipo que la declara. Y cada
//! cota se **analiza una sola vez** (`ir::num::Decimal`, 5.6-CR-I1): cada
//! comparación con ella cuesta O(min(|valor|, |cota|)), así que k valores
//! cortos contra una cota larga cuestan O(k + |cota|), no O(k · |cota|).

use std::collections::HashSet;

use ir::num::{compare_parsed, parse_boolean, xsd_number, Decimal, NumError, NumErrorKind};
use ir::{Lexical, Num};
use lexer::{Pos, Span};

use crate::ast::FacetKind;
use crate::check::{occurs_value, Attr, Errors, SimpleInfo, SimpleOrigin, TypeInfo};
use crate::error::{clip, ErrorKind, ValueWhy};
use crate::symbols::{primitive_name, Builtin, TypeRef};

/// Una cota propia (`min*` o `max*`) de un paso de restricción.
pub(crate) struct Bound {
    pub(crate) num: Num,
    /// `num` ya analizado, para compararlo sin volver a recorrerlo.
    dec: Decimal,
    pub(crate) exclusive: bool,
    /// La faceta entera (rango de la regla de la IR).
    pub(crate) span: Span,
}

/// Facetas propias de un paso de restricción, ya comprobadas: lo que baja
/// a la IR (`docs/fase3/ir.md` §5.2).
pub(crate) struct Own<'a> {
    /// El primitivo heredado de la base.
    pub(crate) lexical: Lexical,
    pub(crate) lower: Option<Bound>,
    pub(crate) upper: Option<Bound>,
    pub(crate) length: Option<(u64, Span)>,
    pub(crate) min_len: Option<(u64, Span)>,
    pub(crate) max_len: Option<(u64, Span)>,
    /// Valores de `xs:enumeration` normalizados: la cadena tal cual en
    /// `xs:string` (*whiteSpace* `preserve`), el lexema de `ir::num` en los
    /// numéricos.
    pub(crate) enums: Vec<String>,
    pub(crate) enum_span: Option<Span>,
    pub(crate) patterns: Vec<&'a str>,
    pub(crate) pattern_span: Option<Span>,
}

impl Default for Own<'_> {
    fn default() -> Self {
        Own {
            lexical: Lexical::String,
            lower: None,
            upper: None,
            length: None,
            min_len: None,
            max_len: None,
            enums: Vec::new(),
            enum_span: None,
            patterns: Vec::new(),
            pattern_span: None,
        }
    }
}

/// Quién declara una cota efectiva.
#[derive(Clone, Copy)]
enum Src {
    Builtin(Builtin),
    Simple(usize),
}

/// Facetas **efectivas** de un tipo (las suyas y las heredadas), de tamaño
/// fijo: las cotas y la enumeración apuntan a quien las declara.
#[derive(Clone, Copy)]
struct Eff {
    lexical: Lexical,
    lower: Option<Src>,
    upper: Option<Src>,
    length: Option<u64>,
    min_len: Option<u64>,
    max_len: Option<u64>,
    /// El tipo cuya `xs:enumeration` rige.
    enums: Option<usize>,
}

/// Relación exigida entre dos valores.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rel {
    Ge,
    Gt,
    Le,
    Lt,
}

impl Rel {
    fn holds(self, o: core::cmp::Ordering) -> bool {
        use core::cmp::Ordering::{Greater, Less};
        match self {
            Rel::Ge => o != Less,
            Rel::Gt => o == Greater,
            Rel::Le => o != Greater,
            Rel::Lt => o == Less,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Rel::Ge => "debe ser ≥",
            Rel::Gt => "debe ser >",
            Rel::Le => "debe ser ≤",
            Rel::Lt => "debe ser <",
        }
    }
}

/// La faceta de una cota.
fn bound_facet(upper: bool, exclusive: bool) -> FacetKind {
    match (upper, exclusive) {
        (false, false) => FacetKind::MinInclusive,
        (false, true) => FacetKind::MinExclusive,
        (true, false) => FacetKind::MaxInclusive,
        (true, true) => FacetKind::MaxExclusive,
    }
}

/// Por qué una faceta no se aplica al primitivo (I13; XSD 1.0 Parte 2
/// §4.1.5: facetas aplicables de cada primitivo). `None`: se aplica.
fn not_applicable(kind: FacetKind, lx: Lexical) -> Option<&'static str> {
    use FacetKind as F;
    match (kind, lx) {
        (F::Pattern, _) => None,
        (_, Lexical::Boolean) => Some("xs:boolean solo admite la faceta pattern"),
        (F::Length | F::MinLength | F::MaxLength, l) if l != Lexical::String => {
            Some("las facetas de longitud solo se aplican a xs:string")
        }
        (
            F::MinInclusive | F::MaxInclusive | F::MinExclusive | F::MaxExclusive,
            Lexical::String,
        ) => Some("las facetas de rango solo se aplican a tipos numéricos"),
        _ => None,
    }
}

/// Facetas que no pueden ir con `kind` en un mismo paso.
fn partners(kind: FacetKind) -> &'static [FacetKind] {
    use FacetKind as F;
    match kind {
        F::Length => &[F::MinLength, F::MaxLength],
        F::MinLength | F::MaxLength => &[F::Length],
        F::MinInclusive => &[F::MinExclusive],
        F::MinExclusive => &[F::MinInclusive],
        F::MaxInclusive => &[F::MaxExclusive],
        F::MaxExclusive => &[F::MaxInclusive],
        F::Enumeration | F::Pattern => &[],
    }
}

/// `collapse` de un valor de primitivo numérico: lo que comparan `ir::num`
/// y los especiales de `xs:float`/`xs:double`.
fn trim(v: &str) -> &str {
    v.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
}

struct Ctx<'s, 'a> {
    simples: &'s [SimpleInfo<'a>],
    /// Cotas de los predefinidos acotados ya analizadas: `[mínimo, máximo]`
    /// por `Builtin`.
    builtin: Vec<[Option<Decimal>; 2]>,
    owns: Vec<Own<'a>>,
    /// `None`: envenenado, en un ciclo o aún sin calcular.
    effs: Vec<Option<Eff>>,
    /// Valores de la enumeración propia de cada tipo (vacío si no tiene).
    sets: Vec<HashSet<String>>,
}

impl Ctx<'_, '_> {
    /// Lexema (para los mensajes), forma analizada y exclusividad de la
    /// cota `src` (inferior o superior). `None` no ocurre: una `Src` solo
    /// apunta a quien tiene esa cota.
    fn bound(&self, src: Src, upper: bool) -> Option<(&str, &Decimal, bool)> {
        match src {
            Src::Builtin(b) => {
                let (_, min, max) = b.rules();
                let lexeme = if upper { max } else { min }?;
                let dec = self.builtin[b as usize][usize::from(upper)].as_ref()?;
                Some((lexeme, dec, false))
            }
            Src::Simple(i) => {
                let own = &self.owns[i];
                let b = if upper { &own.upper } else { &own.lower };
                b.as_ref()
                    .map(|b| (b.num.lexeme.as_str(), &b.dec, b.exclusive))
            }
        }
    }

    /// Facetas efectivas de un tipo usado como base o como tipo de un
    /// atributo; `None` si no se comprueba (envenenado, `anySimpleType`).
    fn eff_of(&self, t: TypeRef) -> Option<Eff> {
        match t {
            TypeRef::Builtin(Builtin::AnySimpleType) => None,
            TypeRef::Builtin(b) => {
                let (lexical, min, max) = b.rules();
                Some(Eff {
                    lexical,
                    lower: min.map(|_| Src::Builtin(b)),
                    upper: max.map(|_| Src::Builtin(b)),
                    length: None,
                    min_len: None,
                    max_len: None,
                    enums: None,
                })
            }
            TypeRef::Simple(j) => self.effs[j],
            TypeRef::Complex(_) | TypeRef::Invalid => None,
        }
    }

    /// Si `raw` es un valor del tipo de facetas `eff` (salvo `pattern`,
    /// D29): forma léxica del primitivo (`ir::num`), cotas, longitudes y
    /// enumeración. Devuelve el valor normalizado. Con `special`, `INF`,
    /// `-INF` y `NaN` de `xs:float`/`xs:double` son valores (en
    /// `default`/`fixed`; en enumeraciones los rechaza D24): NaN queda
    /// fuera de cualquier cota y `±INF` fuera de la del lado contrario.
    fn in_space(&self, eff: &Eff, raw: &str, special: bool) -> Result<String, ValueWhy> {
        let norm = match eff.lexical {
            Lexical::String => {
                let len = raw.chars().count() as u64;
                let checks = [
                    (
                        eff.length,
                        FacetKind::Length,
                        len != eff.length.unwrap_or(0),
                    ),
                    (
                        eff.min_len,
                        FacetKind::MinLength,
                        len < eff.min_len.unwrap_or(0),
                    ),
                    (
                        eff.max_len,
                        FacetKind::MaxLength,
                        len > eff.max_len.unwrap_or(0),
                    ),
                ];
                for (limit, facet, bad) in checks {
                    if let (Some(limit), true) = (limit, bad) {
                        return Err(ValueWhy::Length { facet, limit, len });
                    }
                }
                raw.to_owned()
            }
            Lexical::Boolean => {
                parse_boolean(raw).map_err(ValueWhy::Lexical)?;
                return Ok(String::new());
            }
            lx => match xsd_number(lx, raw) {
                Ok(num) => {
                    let dec = Decimal::new(&num.lexeme);
                    for (src, upper) in [(eff.lower, false), (eff.upper, true)] {
                        let Some((b, bdec, exclusive)) = src.and_then(|s| self.bound(s, upper))
                        else {
                            continue;
                        };
                        let o = compare_parsed(&dec, bdec);
                        let rel = match (upper, exclusive) {
                            (false, false) => Rel::Ge,
                            (false, true) => Rel::Gt,
                            (true, false) => Rel::Le,
                            (true, true) => Rel::Lt,
                        };
                        if !rel.holds(o) {
                            return Err(ValueWhy::Bound {
                                facet: bound_facet(upper, exclusive),
                                bound: clip(b),
                            });
                        }
                    }
                    num.lexeme
                }
                Err(e) if special && e.kind == NumErrorKind::Special => {
                    let t = trim(raw);
                    if t == "+INF" {
                        // De XSD 1.1, no del espacio léxico de XSD 1.0.
                        return Err(ValueWhy::Lexical(NumError {
                            lexical: lx,
                            kind: NumErrorKind::Syntax,
                        }));
                    }
                    if eff.enums.is_some() {
                        return Err(ValueWhy::NotEnumerated);
                    }
                    for (src, upper, out) in [
                        (eff.lower, false, t != "INF"),
                        (eff.upper, true, t != "-INF"),
                    ] {
                        let bound = src.and_then(|s| self.bound(s, upper));
                        if let (Some((b, _, exclusive)), true) = (bound, out) {
                            return Err(ValueWhy::Bound {
                                facet: bound_facet(upper, exclusive),
                                bound: clip(b),
                            });
                        }
                    }
                    return Ok(t.to_owned());
                }
                Err(e) => return Err(ValueWhy::Lexical(e)),
            },
        };
        match eff.enums {
            Some(o) if !self.sets[o].contains(&norm) => Err(ValueWhy::NotEnumerated),
            _ => Ok(norm),
        }
    }
}

/// Nombre de un tipo para los mensajes: `xs:T`, el nombre de un
/// `xs:simpleType` o «anónimo».
fn type_name(simples: &[SimpleInfo<'_>], t: TypeRef) -> String {
    match t {
        TypeRef::Builtin(b) => format!("xs:{}", b.local()),
        TypeRef::Simple(j) => match simples[j].origin {
            SimpleOrigin::Named(n) => clip(n),
            _ => "anónimo".to_owned(),
        },
        TypeRef::Complex(_) | TypeRef::Invalid => String::new(),
    }
}

/// La base de un tipo simple si es otro tipo simple del usuario.
fn base_simple(s: &SimpleInfo<'_>) -> Option<usize> {
    match s.base {
        TypeRef::Simple(j) => Some(j),
        _ => None,
    }
}

/// Etapa 2 (ver el módulo). Deja en cada `SimpleInfo::own` sus facetas
/// comprobadas y en `errors` los de la etapa.
pub(crate) fn check(
    simples: &mut [SimpleInfo<'_>],
    types: &[TypeInfo<'_>],
    global_attrs: &[Attr<'_>],
    errors: &mut Errors,
) {
    let n = simples.len();
    let in_cycle = cycles(simples, errors);
    let parse = |l: Option<&str>| l.map(Decimal::new);
    let mut ctx = Ctx {
        builtin: Builtin::ALL
            .iter()
            .map(|b| {
                let (_, min, max) = b.rules();
                [parse(min), parse(max)]
            })
            .collect(),
        simples,
        owns: (0..n).map(|_| Own::default()).collect(),
        effs: vec![None; n],
        sets: vec![HashSet::new(); n],
    };

    // Orden topológico de la cadena de bases: de cada tipo sin calcular se
    // sube por su cadena hasta uno calculado (o el final, o un ciclo) y se
    // calculan del revés. Cada tipo entra una vez en la pila: O(n).
    let mut done = vec![false; n];
    let mut chain = Vec::new();
    for s in 0..n {
        let mut cur = s;
        while !done[cur] {
            chain.push(cur);
            done[cur] = true;
            match base_simple(&ctx.simples[cur]) {
                Some(j) if !in_cycle[cur] => cur = j,
                _ => break,
            }
        }
        while let Some(c) = chain.pop() {
            if !in_cycle[c] {
                restriction(&mut ctx, c, errors);
            }
        }
    }

    // `default`/`fixed` como valores del tipo del atributo.
    let attrs = types.iter().flat_map(|t| &t.attributes).chain(global_attrs);
    for at in attrs {
        let Some(eff) = ctx.eff_of(at.ty) else {
            continue;
        };
        for (what, v) in [("default", &at.a.default), ("fixed", &at.a.fixed)] {
            let Some(v) = v else { continue };
            if let Err(why) = ctx.in_space(&eff, &v.value, true) {
                errors.simple(
                    ErrorKind::InvalidTypedValue {
                        what,
                        value: clip(&v.value),
                        ty: type_name(ctx.simples, at.ty),
                        why: Box::new(why),
                    },
                    v.at(),
                );
            }
        }
    }

    let owns = ctx.owns;
    for (s, own) in simples.iter_mut().zip(owns) {
        s.own = own;
    }
}

/// Derivación circular (XSD 1.0 Parte 1, *st-props-correct* 2): marca de
/// tres estados (sin ver, en el camino, hecho) y el camino como pila
/// explícita; como cada tipo tiene a lo sumo una base, cada uno se visita
/// una vez, O(n), sin recursión (D7). Un ciclo da **un** error, en el
/// primer valor de `base` (por posición) de sus miembros: todo ciclo tiene
/// uno, porque las bases anónimas forman un árbol. Devuelve los miembros.
fn cycles(simples: &[SimpleInfo<'_>], errors: &mut Errors) -> Vec<bool> {
    let n = simples.len();
    // 0: sin ver; 1: en el camino; 2: hecho.
    let mut state = vec![0u8; n];
    let mut pos = vec![0usize; n];
    let mut in_cycle = vec![false; n];
    let mut path: Vec<usize> = Vec::new();
    for s in 0..n {
        let mut cur = s;
        while state[cur] == 0 {
            state[cur] = 1;
            pos[cur] = path.len();
            path.push(cur);
            match base_simple(&simples[cur]) {
                Some(j) if state[j] == 0 => cur = j,
                Some(j) if state[j] == 1 => {
                    let members = &path[pos[j]..];
                    for &m in members {
                        in_cycle[m] = true;
                    }
                    let first = members
                        .iter()
                        .filter_map(|&m| simples[m].st.restriction.base.as_ref())
                        .min_by_key(|b| b.span.start.offset);
                    if let Some(b) = first {
                        errors.simple(
                            ErrorKind::CircularBase {
                                name: clip(b.value.as_str()),
                                length: members.len(),
                            },
                            b.at(),
                        );
                    }
                    break;
                }
                _ => break,
            }
        }
        for &p in &path {
            state[p] = 2;
        }
        path.clear();
    }
    in_cycle
}

/// Un paso de restricción: las facetas propias del tipo `c` sobre las
/// efectivas de su base. Deja `effs[c]` si no hay errores.
fn restriction(ctx: &mut Ctx<'_, '_>, c: usize, errors: &mut Errors) {
    let s = &ctx.simples[c];
    let Some(be) = ctx.eff_of(s.base) else {
        return;
    };
    let base_name = type_name(ctx.simples, s.base);
    let lx = be.lexical;
    let mut own = Own {
        lexical: lx,
        ..Own::default()
    };
    let before = errors.simples.len();
    // Primera aparición de cada faceta en el paso.
    let mut seen: [Option<Pos>; FacetKind::ALL.len()] = [None; FacetKind::ALL.len()];
    // Posición del valor de cada faceta de un solo valor (para los errores
    // de *valid restriction*).
    let mut value_at: [Pos; FacetKind::ALL.len()] = [Pos::START; FacetKind::ALL.len()];
    let extend = |span: &mut Option<Span>, s: Span| {
        *span = Some(span.map_or(s, |a| Span {
            start: a.start,
            end: s.end,
        }));
    };
    for f in &s.st.restriction.facets {
        let k = f.kind;
        let at = f.span.start;
        if let Some(reason) = not_applicable(k, lx) {
            errors.simple(
                ErrorKind::FacetNotApplicable {
                    facet: k.local(),
                    primitive: primitive_name(lx),
                    reason,
                },
                at,
            );
            continue;
        }
        if !matches!(k, FacetKind::Enumeration | FacetKind::Pattern) {
            if let Some(first) = seen[k as usize] {
                errors.simple(
                    ErrorKind::RepeatedFacet {
                        facet: k.local(),
                        first,
                    },
                    at,
                );
                continue;
            }
        }
        seen[k as usize].get_or_insert(at);
        if let Some(&o) = partners(k).iter().find(|o| seen[**o as usize].is_some()) {
            errors.simple(
                ErrorKind::FacetConflict {
                    facet: k.local(),
                    other: o.local(),
                    first: seen[o as usize].unwrap_or(at),
                },
                at,
            );
            continue;
        }
        let v = &f.value;
        value_at[k as usize] = v.at();
        let bad_value = |why, errors: &mut Errors| {
            errors.simple(
                ErrorKind::InvalidTypedValue {
                    what: if k == FacetKind::Enumeration {
                        "xs:enumeration"
                    } else {
                        k.local()
                    },
                    value: clip(&v.value),
                    ty: base_name.clone(),
                    why: Box::new(why),
                },
                v.at(),
            );
        };
        match k {
            FacetKind::Length | FacetKind::MinLength | FacetKind::MaxLength => {
                match occurs_value(&v.value, false) {
                    Ok(Some(n)) => {
                        let slot = match k {
                            FacetKind::Length => &mut own.length,
                            FacetKind::MinLength => &mut own.min_len,
                            _ => &mut own.max_len,
                        };
                        *slot = Some((n, f.span));
                    }
                    _ => errors.simple(
                        ErrorKind::InvalidValue {
                            attr: k.local(),
                            expected: "un entero no negativo",
                            found: clip(&v.value),
                        },
                        v.at(),
                    ),
                }
            }
            FacetKind::MinInclusive
            | FacetKind::MinExclusive
            | FacetKind::MaxInclusive
            | FacetKind::MaxExclusive => match xsd_number(lx, &v.value) {
                Ok(num) => {
                    let b = Bound {
                        dec: Decimal::new(&num.lexeme),
                        num,
                        exclusive: matches!(k, FacetKind::MinExclusive | FacetKind::MaxExclusive),
                        span: f.span,
                    };
                    if matches!(k, FacetKind::MinInclusive | FacetKind::MinExclusive) {
                        own.lower = Some(b);
                    } else {
                        own.upper = Some(b);
                    }
                }
                Err(e) => bad_value(ValueWhy::Lexical(e), errors),
            },
            FacetKind::Enumeration => match ctx.in_space(&be, &v.value, false) {
                Ok(norm) => {
                    own.enums.push(norm);
                    extend(&mut own.enum_span, f.span);
                }
                Err(why) => bad_value(why, errors),
            },
            FacetKind::Pattern => {
                own.patterns.push(&v.value);
                extend(&mut own.pattern_span, f.span);
            }
        }
    }
    if errors.simples.len() == before {
        valid_restriction(ctx, &be, &own, &value_at, errors);
    }
    if errors.simples.len() != before {
        return;
    }
    let mut eff = be;
    if own.lower.is_some() {
        eff.lower = Some(Src::Simple(c));
    }
    if own.upper.is_some() {
        eff.upper = Some(Src::Simple(c));
    }
    eff.length = own.length.map(|l| l.0).or(be.length);
    eff.min_len = own.min_len.map(|l| l.0).or(be.min_len);
    eff.max_len = own.max_len.map(|l| l.0).or(be.max_len);
    if !own.enums.is_empty() {
        eff.enums = Some(c);
        ctx.sets[c] = own.enums.iter().cloned().collect();
    }
    ctx.effs[c] = Some(eff);
    ctx.owns[c] = own;
}

/// *Facet valid restriction* (XSD 1.0 Parte 2, §4.3.1-§4.3.10) de las
/// facetas propias `own` frente a las efectivas `be` de la base, y la
/// coherencia de las del mismo paso (`minInclusive` ≤ `maxInclusive`,
/// `minLength` ≤ `maxLength`…). Error en el valor de la faceta nueva.
fn valid_restriction(
    ctx: &Ctx<'_, '_>,
    be: &Eff,
    own: &Own<'_>,
    value_at: &[Pos; FacetKind::ALL.len()],
    errors: &mut Errors,
) {
    // Cotas: cada una nueva frente a las dos de la base. Una inclusiva debe
    // caer en el espacio de valores de la base; una exclusiva, no salir de
    // él por su lado y quedar estrictamente dentro por el otro (reglas
    // 1-4 de cada faceta; con `minExclusive` frente a `maxExclusive`, `<`,
    // la lectura conservadora).
    for (b, upper) in [(&own.lower, false), (&own.upper, true)] {
        let Some(b) = b else { continue };
        let kind = bound_facet(upper, b.exclusive);
        for (src, base_upper) in [(be.lower, false), (be.upper, true)] {
            let Some((bv, bdec, bexc)) = src.and_then(|s| ctx.bound(s, base_upper)) else {
                continue;
            };
            let rel = match (upper, base_upper) {
                // Mismo lado: no aflojar la de la base.
                (false, false) if !b.exclusive && bexc => Rel::Gt,
                (false, false) => Rel::Ge,
                (true, true) if !b.exclusive && bexc => Rel::Lt,
                (true, true) => Rel::Le,
                // Lado contrario: dentro del espacio de la base.
                (false, true) if b.exclusive || bexc => Rel::Lt,
                (false, true) => Rel::Le,
                (true, false) if b.exclusive || bexc => Rel::Gt,
                (true, false) => Rel::Ge,
            };
            if !rel.holds(compare_parsed(&b.dec, bdec)) {
                errors.simple(
                    ErrorKind::InvalidRestriction {
                        facet: kind,
                        value: clip(&b.num.lexeme),
                        relation: rel.text(),
                        other: bound_facet(base_upper, bexc),
                        other_value: clip(bv),
                        inherited: true,
                    },
                    value_at[kind as usize],
                );
            }
        }
    }
    // Las dos cotas del mismo paso: `min` ≤ `max` si las dos son inclusivas;
    // si no, `<` (lectura conservadora de *minInclusive <= maxInclusive*,
    // *minExclusive <= maxExclusive*, *minInclusive < maxExclusive*).
    if let (Some(l), Some(u)) = (&own.lower, &own.upper) {
        let (lk, uk) = (
            bound_facet(false, l.exclusive),
            bound_facet(true, u.exclusive),
        );
        let rel = if l.exclusive || u.exclusive {
            Rel::Lt
        } else {
            Rel::Le
        };
        if !rel.holds(compare_parsed(&l.dec, &u.dec)) {
            let (at_l, at_u) = (value_at[lk as usize], value_at[uk as usize]);
            errors.simple(
                ErrorKind::InvalidRestriction {
                    facet: lk,
                    value: clip(&l.num.lexeme),
                    relation: rel.text(),
                    other: uk,
                    other_value: clip(&u.num.lexeme),
                    inherited: false,
                },
                if at_l.offset > at_u.offset {
                    at_l
                } else {
                    at_u
                },
            );
        }
    }

    // Longitudes.
    let mut fail = |kind: FacetKind,
                    n: u64,
                    relation: &'static str,
                    other: FacetKind,
                    m: u64,
                    inherited: bool,
                    at: Pos| {
        errors.simple(
            ErrorKind::InvalidRestriction {
                facet: kind,
                value: n.to_string(),
                relation,
                other,
                other_value: m.to_string(),
                inherited,
            },
            at,
        );
    };
    let at = |k: FacetKind| value_at[k as usize];
    if let Some((n, _)) = own.length {
        let k = FacetKind::Length;
        if let Some(m) = be.length.filter(|&m| m != n) {
            fail(k, n, "debe ser igual a", FacetKind::Length, m, true, at(k));
        }
        if let Some(m) = be.min_len.filter(|&m| n < m) {
            fail(k, n, Rel::Ge.text(), FacetKind::MinLength, m, true, at(k));
        }
        if let Some(m) = be.max_len.filter(|&m| n > m) {
            fail(k, n, Rel::Le.text(), FacetKind::MaxLength, m, true, at(k));
        }
    }
    for (own_len, base_len, k) in [
        (own.min_len, be.min_len, FacetKind::MinLength),
        (own.max_len, be.max_len, FacetKind::MaxLength),
    ] {
        let Some((n, _)) = own_len else { continue };
        if let Some(m) = be.length {
            // *length and minLength or maxLength* (2.ª ed., cláusulas 1.2
            // para `minLength` y 2.2 para `maxLength`): con `length` en la
            // base, solo se puede repetir el valor efectivo que ya tenía.
            if base_len != Some(n) {
                fail(
                    k,
                    n,
                    "no se admite junto al",
                    FacetKind::Length,
                    m,
                    true,
                    at(k),
                );
            }
            continue;
        }
        if let Some(m) = be.min_len.filter(|&m| n < m) {
            fail(k, n, Rel::Ge.text(), FacetKind::MinLength, m, true, at(k));
        }
        if let Some(m) = be.max_len.filter(|&m| n > m) {
            fail(k, n, Rel::Le.text(), FacetKind::MaxLength, m, true, at(k));
        }
    }
    if let (Some((a, _)), Some((b, _))) = (own.min_len, own.max_len) {
        if a > b {
            let (ka, kb) = (FacetKind::MinLength, FacetKind::MaxLength);
            let p = if at(ka).offset > at(kb).offset {
                at(ka)
            } else {
                at(kb)
            };
            fail(ka, a, Rel::Le.text(), kb, b, false, p);
        }
    }
}
