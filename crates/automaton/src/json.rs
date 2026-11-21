//! Construcción composicional de M sobre una IR de JSON Schema
//! (`docs/fase4/automata.md` §6).
//!
//! Etapas, todas iterativas (D7, F4-B9):
//!
//! 1. Lo no implementado se rechaza antes de construir nada
//!    ([`BuildError::Pending`], F4-U5), recorriendo todo lo alcanzable desde
//!    la raíz.
//! 2. Cada valor lo lee el marco de un **conjunto canónico de `NodeId`**: el
//!    cierre conjuntivo de su nodo por `$ref` y `allOf` ([`crate::conj`],
//!    F4-U2). Un conjunto de un nodo es el marco de F4.1a; uno de varios es
//!    un **producto** de sus componentes.
//! 3. Análisis de cada conjunto alcanzable (lista de trabajo): sus
//!    subautómatas de objeto, arreglo, número y cadena, sus hijos (uniones
//!    de cierres) y su tamaño exacto. El tope de estados por producto (L-1)
//!    y la cota del presupuesto de δ se comprueban aquí, **antes** de
//!    reservar.
//! 4. Se reservan los estados de cada subautómata y después se rellenan sus
//!    filas: los estados de un hijo ya existen cuando el padre los apila,
//!    también con recursión (`$ref` a un antepasado).

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use ir::{JsonType, LitToken, Literal, Node, NodeId, Rule, RuleKind, TypeSet, VerifiedIr};
use runtime_rs::num::{Divisor, DivisorError};
use runtime_rs::tables::{
    class, Check, CountOp, CounterOp, Dep, DivisorRef, Op, Test, Trans, OUT_IDENTITY, SLOT_CHILD,
};

use crate::conj::{Closures, SetId, ANY, CLOSURE_WHAT};
use crate::dfa::Dfa;
use crate::emit::{
    budget, clip, delta_limit, label, plain, table_index, trie_size, Bound, Emitter,
};
use crate::location::{location_clipped, LOCATION_MAX};
use crate::model::Automaton;
use crate::{BuildError, BuildOptions, Task};

/// Presupuesto de entradas de δ (|Q| · |Σ|) por esquema (F4-U6): 2²⁰.
pub const MAX_DELTA: usize = 1 << 20;

/// Nombres con bit de máscara por objeto (`properties` ∪ `required` ∪
/// `dependentRequired`, de todos los componentes de un producto; F4-U6):
/// 4 096, una máscara de 512 B.
pub const MAX_NAMES: usize = 4096;

/// Tope de estados de un producto, por defecto (L-1, `00-alcance.md` §4):
/// 4 096; configurable con [`BuildOptions`].
pub const MAX_PRODUCT_STATES: usize = 4096;

/// Estados fijos: raíz (2), objeto «cualquiera» (6), arreglo «cualquiera»
/// (3), cadena y número «cualquiera» (2).
const FIXED_STATES: usize = 13;

pub(crate) fn build(ir: &VerifiedIr, opts: &BuildOptions) -> Result<Automaton, BuildError> {
    check_pending(ir)?;
    let an = Analysis::run(ir, opts)?;
    let mut b = Builder::new(ir, an, opts)?;
    b.fill();
    b.t.finish()
}

// ------------------------------------------------------- 1. pendientes ---

/// La tarea que implementará la regla, o `None` si ya se compila.
fn task_of(rule: &Rule) -> Option<Task> {
    match rule {
        Rule::Reject
        | Rule::Type(_)
        | Rule::Properties { .. }
        | Rule::Items { .. }
        | Rule::Required(_)
        | Rule::DependentRequired(_)
        | Rule::Ref(_)
        | Rule::AllOf(_)
        | Rule::Minimum(_)
        | Rule::Maximum(_)
        | Rule::ExclusiveMinimum(_)
        | Rule::ExclusiveMaximum(_)
        | Rule::MultipleOf(_)
        | Rule::MinLength(_)
        | Rule::MaxLength(_)
        | Rule::MinItems(_)
        | Rule::MaxItems(_)
        | Rule::MinProperties(_)
        | Rule::MaxProperties(_) => None,
        Rule::Enum(lits) if lits.iter().all(scalar) => None,
        Rule::Const(lit) if scalar(lit) => None,
        // F4.5b: `uniqueItems` por hash canónico; los literales compuestos,
        // recorte 2 del plan (`docs/fase4/pendientes.md` §72).
        Rule::UniqueItems => None,
        Rule::Enum(_) | Rule::Const(_) => Some(Task::Literals),
        // F4.4b: en JSON ya se compila; en XML, todavía no (`check_pending`).
        Rule::Pattern(_) => None,
        // F4.5a: salidas por rama y puertas (`conj::Closures::of`).
        Rule::AnyOf(_) | Rule::OneOf(_) | Rule::Not(_) | Rule::DependentSchemas(_) => None,
        // XML (F4.1c): el constructor de `crate::xml`.
        Rule::Attributes(_) | Rule::Content(_) | Rule::SimpleContent(_) | Rule::Lexical(_) => None,
    }
}

/// Un literal de un solo token escalar.
fn scalar(lit: &Literal) -> bool {
    !matches!(
        lit.0.first(),
        Some(LitToken::Array(_) | LitToken::Object(_))
    )
}

/// Recorre todo lo alcanzable desde la raíz (por todas las aristas) y
/// rechaza la regla pendiente de la tarea **más tardía** (la que fija cuándo
/// compilará el esquema); a igualdad, la primera en el recorrido.
pub(crate) fn check_pending(ir: &VerifiedIr) -> Result<(), BuildError> {
    let n = ir.nodes().len();
    let mut seen = vec![false; n];
    let mut queue = std::collections::VecDeque::from([NodeId::ROOT]);
    seen[0] = true;
    let mut worst: Option<(Task, NodeId, RuleKind)> = None;
    let mut buf = Vec::new();
    while let Some(id) = queue.pop_front() {
        let node = ir.node(id);
        for c in &node.rules {
            let xml_pattern = matches!(c.rule, Rule::Pattern(_))
                && matches!(ir.format(), ir::Format::Xml { .. })
                && !crate::xml::PATTERNS;
            let task = task_of(&c.rule).or(xml_pattern.then_some(Task::Pattern));
            if let Some(task) = task {
                if worst.is_none_or(|(w, _, _)| task > w) {
                    worst = Some((task, id, c.rule.kind()));
                }
            }
            buf.clear();
            c.rule.targets(&mut buf);
            for &t in &buf {
                if !seen[t.index()] {
                    seen[t.index()] = true;
                    queue.push_back(t);
                }
            }
        }
    }
    match worst {
        Some((task, id, kind)) => Err(pending(ir, id, kind, task)),
        None => Ok(()),
    }
}

fn pending(ir: &VerifiedIr, node: NodeId, rule: RuleKind, task: Task) -> BuildError {
    BuildError::Pending {
        node,
        rule,
        task,
        location: location_clipped(ir, node, LOCATION_MAX),
    }
}

// ------------------------------------------------------- reglas del nodo ---

/// Una entrada de `dependentRequired`: propiedad → las que exige.
type Dependencia = (String, Vec<String>);

/// Las reglas de objeto de un nodo, juntas.
struct ObjectRules<'a> {
    declared: &'a [(String, NodeId)],
    additional: Option<NodeId>,
    /// (índice de la regla en el nodo, nombres).
    required: Option<(usize, &'a [String])>,
    dependent: Option<(usize, &'a [Dependencia])>,
    counts: Vec<Bound>,
}

fn object_rules(id: NodeId, node: &Node) -> Option<ObjectRules<'_>> {
    let mut o = ObjectRules {
        declared: &[],
        additional: None,
        required: None,
        dependent: None,
        counts: Vec::new(),
    };
    let mut any = false;
    for (i, c) in node.rules.iter().enumerate() {
        match &c.rule {
            Rule::Properties {
                declared,
                additional,
            } => {
                o.declared = declared;
                o.additional = *additional;
            }
            Rule::Required(r) => o.required = Some((i, r)),
            Rule::DependentRequired(d) => o.dependent = Some((i, d)),
            Rule::MinProperties(n) => o.counts.push((id, i, Test::CountMin, *n)),
            Rule::MaxProperties(n) => o.counts.push((id, i, Test::CountMax, *n)),
            _ => continue,
        }
        any = true;
    }
    any.then_some(o)
}

impl<'a> ObjectRules<'a> {
    /// Nombres con bit (con repeticiones): los declarados, los requeridos y
    /// los de `dependentRequired`, en ese orden.
    fn names(&self) -> impl Iterator<Item = &'a str> + '_ {
        let declared = self.declared.iter().map(|(n, _)| n.as_str());
        let required = self
            .required
            .map_or(&[][..], |r| r.1)
            .iter()
            .map(String::as_str);
        let dependent = self
            .dependent
            .map_or(&[][..], |d| d.1)
            .iter()
            .flat_map(|(k, l)| std::iter::once(k.as_str()).chain(l.iter().map(String::as_str)));
        declared.chain(required).chain(dependent)
    }
}

/// `type` admite `integer` pero no `number`: el número se comprueba al
/// cerrarlo con los registros del número.
fn integer_only(node: &Node) -> Option<usize> {
    node.rules.iter().position(|c| {
        matches!(c.rule, Rule::Type(set) if set.contains(JsonType::Integer) && !set.contains(JsonType::Number))
    })
}

// ----------------------------------------------------------- análisis ---

/// El subautómata de objeto de un conjunto: el producto de los de sus
/// componentes con reglas de objeto. Los tries de todos se funden en el de
/// la unión de sus nombres: el producto alcanzable de varios tries sobre la
/// misma entrada tiene un estado por prefijo distinto, que es el trie de la
/// unión (`automata.md` §6.4). Cada nombre lleva al hijo que es la unión de
/// los cierres de los destinos de cada componente (§6.2 de `fase3/ir.md`).
struct ObjSpec<'a> {
    names: Vec<&'a str>,
    /// Hijo de cada nombre, con su mapa de salidas (F4.5a).
    children: Vec<(SetId, u32)>,
    /// Hijo de una clave desconocida (unión de los `additionalProperties`).
    unknown: (SetId, u32),
    required: Vec<(NodeId, usize, &'a [String])>,
    dependent: Vec<(NodeId, usize, &'a [Dependencia])>,
    counts: Vec<Bound>,
}

/// El subautómata de arreglo de un conjunto: el índice saturado en el mayor
/// `prefixItems`; el hijo del índice i es la unión de los de cada
/// componente. `slots[p]` es el de los elementos siguientes.
struct ArrSpec {
    slots: Vec<(SetId, u32)>,
    /// Reglas `uniqueItems` de los componentes (F4.5b).
    unique: Vec<(NodeId, usize)>,
    counts: Vec<Bound>,
}

/// Lo que comprueba el cierre de un número.
enum NumTest<'a> {
    Integer,
    /// `NumMin`, `NumExMin`, `NumMax` o `NumExMax` contra el lexema.
    Cmp(Test, &'a str),
    Multiple(&'a str, Divisor),
    /// Valores de `enum`/`const`, ordenados por valor y sin repetir: índice
    /// en `Analysis::enums` (uno por regla, compartido entre conjuntos).
    Enum(u32),
}

struct NumSpec<'a> {
    checks: Vec<(NodeId, usize, NumTest<'a>)>,
}

/// El subautómata de cadena: el trie de la unión de las cadenas de los
/// `enum`/`const` de todos los componentes (con la cadena vacía y prefijos
/// compartidos); al cerrarla, cada literal comprueba si el estado es una de
/// las suyas. Las longitudes son del contador global de puntos de código.
struct StrSpec<'a> {
    /// (nodo, regla, índice en `Analysis::strsets`).
    lits: Vec<(NodeId, usize, u32)>,
    lens: Vec<Bound>,
    words: Vec<&'a str>,
    /// (nodo, regla) de cada `pattern` (F4.4b): el bit j de la salida del
    /// AFD dice si el j-ésimo se cumple.
    pats: Vec<(NodeId, usize)>,
    /// El AFD de la cadena: el producto del trie de `words` y de los AFD de
    /// `pats`, minimizado, sobre la partición `inst`; la salida es (palabra
    /// leída, bits de los `pattern` cumplidos).
    dfa: Dfa<StrOut>,
    inst: crate::pattern::Partition,
    /// Estados del producto antes de minimizar.
    raw_states: usize,
}

/// Salida del AFD de una cadena: la palabra literal leída y los `pattern`
/// que se cumplen (bit j = el j-ésimo de `StrSpec::pats`).
type StrOut = (Option<u32>, u64);

/// `pattern` por cadena como mucho (un bit de la salida por cada uno).
const MAX_PATTERNS: usize = 64;

#[derive(Default)]
struct SetInfo<'a> {
    obj: Option<ObjSpec<'a>>,
    arr: Option<ArrSpec>,
    num: Option<NumSpec<'a>>,
    string: Option<StrSpec<'a>>,
}

struct Analysis<'a> {
    cl: Closures<'a>,
    /// Información de cada conjunto alcanzable, por id.
    infos: Vec<Option<SetInfo<'a>>>,
    /// Conjuntos alcanzables con estados propios o comprobaciones, en orden
    /// de descubrimiento.
    order: Vec<SetId>,
    root: SetId,
    /// Mapa de salidas con que se abre la raíz (F4.5a).
    root_outs: u32,
    chars: BTreeSet<char>,
    /// Cota superior de |Q|.
    upper: usize,
    /// Entradas de las tablas que pueden crecer con los productos
    /// (comprobaciones, palabras de máscaras, dependencias y lexemas),
    /// contadas antes de reservar: el presupuesto de F4-U6 cubre
    /// |Q| · |Σ| + `extra` (§6.6 de `automata.md`).
    extra: usize,
    products: usize,
    /// Números de cada `enum`/`const` ya ordenados, una vez por regla.
    enums: Vec<Vec<&'a str>>,
    enum_ids: HashMap<(NodeId, usize), Option<u32>>,
    /// Cadenas de cada `enum`/`const`, una vez por regla.
    strsets: Vec<HashSet<&'a str>>,
    str_ids: HashMap<(NodeId, usize), Option<u32>>,
    /// Cotas y divisores ya contados (cada lexema del esquema entra una vez).
    lexeme_rules: HashSet<(NodeId, usize)>,
    /// Máscara de 7 bits (sobre `class::VALUE_STARTS`) de las clases en las
    /// que falla cada regla al empezar un valor ([`entry_fails`]), una vez
    /// por (nodo, regla): un `enum` se recorre una sola vez aunque lo lean
    /// muchos conjuntos (ronda 3 de F4.1b).
    entry_masks: HashMap<(NodeId, usize), u8>,
    /// Divisor de cada `multipleOf`, analizado una vez por regla.
    divisors: HashMap<(NodeId, usize), Divisor>,
    /// El AFD de cada `pattern`, una vez por regla (F4.4b).
    patterns: HashMap<(NodeId, usize), crate::pattern::PatternDfa>,
    /// Trabajo de todos los `pattern` construidos (E3B-3).
    pattern_work: usize,
    /// Los conjuntos de caracteres de los `pattern`: refinan la partición
    /// global de clases (F4-B3).
    sets: Vec<ir::regex::CharSet>,
}

impl<'a> Analysis<'a> {
    /// Rangos de los conjuntos de los `pattern` (cota de las clases que
    /// añaden a la partición, para las cuentas parciales).
    fn ranges(&self) -> usize {
        self.sets.iter().map(Vec::len).sum()
    }

    /// Clases de carácter de la partición final (con «otro»).
    fn n_char_classes(&self) -> usize {
        let singles: Vec<u32> = self.chars.iter().map(|&c| u32::from(c)).collect();
        let refs: Vec<&ir::regex::CharSet> = self.sets.iter().collect();
        crate::pattern::Partition::refine(&singles, &refs).n_classes
    }

    fn run(ir: &'a VerifiedIr, opts: &BuildOptions) -> Result<Analysis<'a>, BuildError> {
        let mut cl = Closures::new(ir.nodes())?;
        let root = cl.of(NodeId::ROOT)?;
        let root_outs = cl.root_outs(root);
        let mut an = Analysis {
            cl,
            infos: Vec::new(),
            order: Vec::new(),
            root,
            root_outs,
            chars: BTreeSet::new(),
            upper: FIXED_STATES,
            extra: 0,
            products: 0,
            enums: Vec::new(),
            enum_ids: HashMap::new(),
            strsets: Vec::new(),
            patterns: HashMap::new(),
            pattern_work: 0,
            sets: Vec::new(),
            str_ids: HashMap::new(),
            lexeme_rules: HashSet::new(),
            entry_masks: HashMap::new(),
            divisors: HashMap::new(),
        };
        let mut seen = HashSet::from([ANY]);
        let mut stack = Vec::new();
        if seen.insert(root) {
            stack.push(root);
        }
        while let Some(s) = stack.pop() {
            let (info, children) = an.set(ir, s, opts).map_err(|e| match e {
                // El presupuesto de los cierres no sabe de qué conjunto viene.
                BuildError::Limit {
                    what, limit, found, ..
                } if what == CLOSURE_WHAT => BuildError::Limit {
                    what,
                    limit,
                    found,
                    location: label(ir, an.cl.sets.get(s)),
                },
                e => e,
            })?;
            // Cota parcial con las clases vistas hasta ahora (|Σ| solo
            // crece): un grafo de conjuntos que crece sin parar se corta
            // antes de agotar la memoria.
            let k = class::CHAR as usize + 1 + an.chars.len() + an.ranges();
            if budget(an.upper, k, an.extra) > opts.max_delta {
                return Err(delta_limit(
                    an.upper,
                    k,
                    an.extra,
                    opts.max_delta,
                    label(ir, an.cl.sets.get(s)),
                ));
            }
            if an.infos.len() <= s as usize {
                an.infos.resize_with(s as usize + 1, || None);
            }
            an.infos[s as usize] = Some(info);
            an.order.push(s);
            for &c in children.iter().rev() {
                if seen.insert(c) {
                    stack.push(c);
                }
            }
        }
        Ok(an)
    }

    /// Analiza el conjunto `s` y devuelve sus subautómatas y sus hijos
    /// (F4.1b-CR-S6: un método por subautómata).
    fn set(
        &mut self,
        ir: &'a VerifiedIr,
        s: SetId,
        opts: &BuildOptions,
    ) -> Result<(SetInfo<'a>, Vec<SetId>), BuildError> {
        let comps: Vec<NodeId> = self.cl.sets.get(s).to_vec();
        let comps = &comps[..];
        // Claves de los `dependentSchemas` del conjunto (F4.5a): necesitan su
        // bit en la máscara del objeto.
        let mut implies: Vec<&'a str> = Vec::new();
        for g in self.cl.sets.meta(s).gates.iter() {
            if let (Some(k), Rule::DependentSchemas(v)) =
                (&g.key, &ir.node(g.node).rules[g.rule].rule)
            {
                if let Some((key, _)) = v.iter().find(|(x, _)| x == k) {
                    implies.push(key.as_str());
                }
            }
        }
        let mut children = Vec::new();
        // Comprobaciones de las 7 transiciones de entrada de un valor del
        // conjunto (una lista por clase, compartida por todos los retornos).
        for &id in comps {
            for (i, c) in ir.node(self.cl.real(id)).rules.iter().enumerate() {
                self.extra += self.entry_mask(id, i, &c.rule).count_ones() as usize;
            }
        }
        let info = SetInfo {
            obj: self.object(ir, comps, &implies, opts, &mut children)?,
            arr: self.array(ir, comps, opts, &mut children)?,
            num: self.number(ir, comps, opts)?,
            string: self.string(ir, comps, opts)?,
        };
        children.retain(|&c| c != ANY);
        Ok((info, children))
    }

    /// Cuenta un subautómata producto (de `n` ≥ 2 componentes que aportan
    /// a él) y comprueba su tope de estados antes de reservar (L-1).
    fn product(
        &mut self,
        ir: &VerifiedIr,
        comps: &[NodeId],
        opts: &BuildOptions,
        n: usize,
        states: usize,
        scalar: bool,
    ) -> Result<(), BuildError> {
        if n < 2 {
            return Ok(());
        }
        self.products += 1;
        if states > opts.max_product_states {
            return Err(BuildError::ProductLimit {
                states,
                limit: opts.max_product_states,
                scalar,
                location: label(ir, comps),
            });
        }
        Ok(())
    }

    /// El subautómata de objeto del conjunto de `comps`, si alguno tiene
    /// reglas de objeto; sus hijos van a `children`.
    fn object(
        &mut self,
        ir: &'a VerifiedIr,
        comps: &[NodeId],
        implies: &[&'a str],
        opts: &BuildOptions,
        children: &mut Vec<SetId>,
    ) -> Result<Option<ObjSpec<'a>>, BuildError> {
        let objs: Vec<(NodeId, ObjectRules<'a>)> = comps
            .iter()
            .filter_map(|&id| object_rules(id, ir.node(self.cl.real(id))).map(|o| (id, o)))
            .collect();
        if objs.is_empty() && implies.is_empty() {
            return Ok(None);
        }
        let mut names: Vec<&'a str> = Vec::new();
        let mut idx: HashMap<&'a str, usize> = HashMap::new();
        for n in objs
            .iter()
            .flat_map(|(_, o)| o.names())
            .chain(implies.iter().copied())
        {
            if !idx.contains_key(n) {
                idx.insert(n, names.len());
                names.push(n);
            }
        }
        if names.len() > MAX_NAMES {
            return Err(BuildError::Limit {
                what: "nombres por objeto",
                limit: MAX_NAMES,
                found: names.len(),
                location: label(ir, comps),
            });
        }
        // El tamaño del producto solo depende de los nombres y del número
        // de componentes: el tope y el trabajo (nombres × componentes) se
        // comprueban **antes** de reservar nada proporcional al producto
        // (L-1). Estados: primero, siguiente, trie, 2 por nombre, 2 de la
        // clave desconocida, después.
        let states = 2 + trie_size(&names) + 2 * names.len() + 2 + 1;
        self.product(ir, comps, opts, objs.len(), states, false)?;
        self.cl.spend(names.len().saturating_mul(objs.len()))?;
        // Al cerrar: una comprobación por `required`, `dependentRequired`
        // y contador de cada componente, y sus máscaras sobre la unión.
        let mask_words = names.len().div_ceil(64);
        for (_, o) in &objs {
            let req = o.required.map_or(0, |(_, r)| usize::from(!r.is_empty()));
            let deps = o
                .dependent
                .map_or(0, |(_, d)| d.iter().filter(|(_, l)| !l.is_empty()).count());
            self.extra += req + usize::from(deps > 0) + o.counts.len();
            self.extra += (req + deps).saturating_mul(mask_words) + deps;
        }
        let mut spec = ObjSpec {
            names: Vec::new(),
            children: Vec::new(),
            unknown: (ANY, OUT_IDENTITY),
            required: Vec::new(),
            dependent: Vec::new(),
            counts: Vec::new(),
        };
        // Por componente: su `additionalProperties` y sus declarados (lo
        // que ya ocupa la IR).
        let mut comp: Vec<(SetId, HashMap<&str, NodeId>, u32)> = Vec::with_capacity(objs.len());
        for &(id, ref o) in &objs {
            let add = match o.additional {
                Some(t) => self.cl.of(t)?,
                None => ANY,
            };
            comp.push((
                add,
                o.declared.iter().map(|(n, t)| (n.as_str(), *t)).collect(),
                self.cl.tgt(id),
            ));
            if let Some((i, r)) = o.required {
                spec.required.push((id, i, r));
            }
            if let Some((i, d)) = o.dependent {
                spec.dependent.push((id, i, d));
            }
            spec.counts.extend(o.counts.iter().copied());
        }
        // Hijo de cada nombre: la unión de los cierres de lo que le asigna
        // cada componente, con un solo vector temporal.
        let mut parts = Vec::with_capacity(objs.len());
        for n in &names {
            parts.clear();
            for (add, declared, tgt) in &comp {
                parts.push(match declared.get(n) {
                    Some(&t) => (self.cl.of(t)?, *tgt),
                    None => (*add, *tgt),
                });
            }
            spec.children.push(self.cl.union(&parts)?);
        }
        let unknown: Vec<(SetId, u32)> = comp.iter().map(|(add, _, t)| (*add, *t)).collect();
        spec.unknown = self.cl.union(&unknown)?;
        self.upper += states;
        self.chars.extend(names.iter().flat_map(|n| n.chars()));
        children.extend(spec.children.iter().map(|c| c.0));
        children.push(spec.unknown.0);
        spec.names = names;
        Ok(Some(spec))
    }

    /// El subautómata de arreglo; sus hijos van a `children`.
    fn array(
        &mut self,
        ir: &'a VerifiedIr,
        comps: &[NodeId],
        opts: &BuildOptions,
        children: &mut Vec<SetId>,
    ) -> Result<Option<ArrSpec>, BuildError> {
        let mut items: Vec<(&'a [NodeId], Option<NodeId>, u32)> = Vec::new();
        let mut counts = Vec::new();
        let mut unique = Vec::new();
        let mut n_arr = 0;
        for &id in comps {
            let mut here = false;
            for (i, c) in ir.node(self.cl.real(id)).rules.iter().enumerate() {
                match &c.rule {
                    Rule::Items { prefix, rest } => items.push((prefix, *rest, self.cl.tgt(id))),
                    Rule::MinItems(n) => counts.push((id, i, Test::CountMin, *n)),
                    Rule::MaxItems(n) => counts.push((id, i, Test::CountMax, *n)),
                    Rule::UniqueItems => unique.push((id, i)),
                    _ => continue,
                }
                here = true;
            }
            n_arr += usize::from(here);
        }
        if n_arr == 0 {
            return Ok(None);
        }
        let p = items.iter().map(|(pre, _, _)| pre.len()).max().unwrap_or(0);
        // primero, siguientes (al menos uno), uno tras cada valor: el tope
        // y el trabajo se comprueban antes de calcular los hijos.
        let states = 1 + p.max(1) + p + 1;
        self.product(ir, comps, opts, n_arr, states, false)?;
        self.cl.spend((p + 1).saturating_mul(items.len()))?;
        self.extra += counts.len() + unique.len();
        let mut slots = Vec::with_capacity(p + 1);
        let mut parts = Vec::with_capacity(items.len());
        for i in 0..=p {
            parts.clear();
            for (pre, rest, tgt) in &items {
                let t = if i < pre.len() { Some(pre[i]) } else { *rest };
                parts.push(match t {
                    Some(t) => (self.cl.of(t)?, *tgt),
                    None => (ANY, *tgt),
                });
            }
            slots.push(self.cl.union(&parts)?);
        }
        self.upper += states;
        children.extend(slots.iter().map(|c| c.0));
        Ok(Some(ArrSpec {
            slots,
            counts,
            unique,
        }))
    }

    /// El estado de número propio y sus comprobaciones al cerrarlo.
    fn number(
        &mut self,
        ir: &'a VerifiedIr,
        comps: &[NodeId],
        opts: &BuildOptions,
    ) -> Result<Option<NumSpec<'a>>, BuildError> {
        let mut num = Vec::new();
        let mut n_num = 0;
        for &id in comps {
            let node = ir.node(self.cl.real(id));
            let antes = num.len();
            if let Some(i) = integer_only(node) {
                num.push((id, i, NumTest::Integer));
            }
            for (i, c) in node.rules.iter().enumerate() {
                let t = match &c.rule {
                    Rule::Minimum(n) => NumTest::Cmp(Test::NumMin, &n.lexeme),
                    Rule::ExclusiveMinimum(n) => NumTest::Cmp(Test::NumExMin, &n.lexeme),
                    Rule::Maximum(n) => NumTest::Cmp(Test::NumMax, &n.lexeme),
                    Rule::ExclusiveMaximum(n) => NumTest::Cmp(Test::NumExMax, &n.lexeme),
                    Rule::MultipleOf(n) => match self.divisors.get(&(self.cl.real(id), i)) {
                        Some(&d) => NumTest::Multiple(&n.lexeme, d),
                        None => match Divisor::of(n.lexeme.as_bytes()) {
                            Ok(d) => {
                                self.divisors.insert((self.cl.real(id), i), d);
                                NumTest::Multiple(&n.lexeme, d)
                            }
                            Err(e) => return Err(divisor_limit(ir, self.cl.real(id), e)),
                        },
                    },
                    Rule::Enum(lits) => match self.numbers(id, i, lits)? {
                        Some(k) => NumTest::Enum(k),
                        None => continue,
                    },
                    Rule::Const(lit) => match self.numbers(id, i, std::slice::from_ref(lit))? {
                        Some(k) => NumTest::Enum(k),
                        None => continue,
                    },
                    _ => continue,
                };
                if matches!(t, NumTest::Cmp(..) | NumTest::Multiple(..))
                    && self.lexeme_rules.insert((id, i))
                {
                    self.extra += 1;
                }
                num.push((id, i, t));
            }
            n_num += usize::from(num.len() > antes);
        }
        if num.is_empty() {
            return Ok(None);
        }
        self.product(ir, comps, opts, n_num, 1, true)?;
        self.upper += 1;
        self.extra += num.len();
        Ok(Some(NumSpec { checks: num }))
    }

    /// El subautómata de cadena: el trie de la unión de los literales.
    fn string(
        &mut self,
        ir: &'a VerifiedIr,
        comps: &[NodeId],
        opts: &BuildOptions,
    ) -> Result<Option<StrSpec<'a>>, BuildError> {
        let mut lits = Vec::new();
        let mut lens = Vec::new();
        let mut pats = Vec::new();
        let mut n_str = 0;
        for &id in comps {
            let antes = lits.len() + lens.len();
            for (i, c) in ir.node(self.cl.real(id)).rules.iter().enumerate() {
                match &c.rule {
                    Rule::MinLength(n) => lens.push((id, i, Test::LengthMin, *n)),
                    Rule::MaxLength(n) => lens.push((id, i, Test::LengthMax, *n)),
                    Rule::Enum(l) => {
                        if let Some(k) = self.strings(id, i, l)? {
                            lits.push((id, i, k));
                        }
                    }
                    Rule::Const(l) => {
                        if let Some(k) = self.strings(id, i, std::slice::from_ref(l))? {
                            lits.push((id, i, k));
                        }
                    }
                    Rule::Pattern(src) => {
                        self.pattern(ir, id, i, src, opts)?;
                        pats.push((id, i));
                    }
                    _ => {}
                }
            }
            n_str += usize::from(lits.len() + lens.len() + pats.len() > antes);
        }
        if lits.is_empty() && lens.is_empty() && pats.is_empty() {
            return Ok(None);
        }
        if pats.len() > MAX_PATTERNS {
            return Err(BuildError::Limit {
                what: "`pattern` sobre la misma cadena",
                limit: MAX_PATTERNS,
                found: pats.len(),
                location: label(ir, comps),
            });
        }
        let mut words: Vec<&'a str> = Vec::new();
        let mut seen = HashSet::new();
        for &(_, _, k) in &lits {
            let mut v: Vec<&'a str> = self.strsets[k as usize].iter().copied().collect();
            v.sort_unstable();
            for w in v {
                if seen.insert(w) {
                    words.push(w);
                }
            }
        }
        let (dfa, inst, raw_states) = self.string_dfa(ir, comps, opts, &words, &pats)?;
        let states = dfa.n_states();
        self.product(ir, comps, opts, n_str, states, true)?;
        self.upper += states;
        // El cierre de cada estado lleva una comprobación por literal que no
        // contiene su palabra, una por longitud y una por `pattern` que no
        // se cumple: como mucho estados × (literales + longitudes + patrones).
        self.extra = self
            .extra
            .saturating_add(states.saturating_mul(lits.len() + lens.len() + pats.len()));
        self.chars.extend(words.iter().flat_map(|w| w.chars()));
        Ok(Some(StrSpec {
            lits,
            lens,
            words,
            pats,
            dfa,
            inst,
            raw_states,
        }))
    }

    /// El AFD de `pattern` de la regla `i` de `id`, una vez por regla
    /// (F4.4b): más de `max_product_states` estados es `limit` en su
    /// ubicación, contado antes de crearlos.
    fn pattern(
        &mut self,
        ir: &VerifiedIr,
        id: NodeId,
        i: usize,
        src: &str,
        opts: &BuildOptions,
    ) -> Result<(), BuildError> {
        let id = self.cl.real(id);
        if self.patterns.contains_key(&(id, i)) {
            return Ok(());
        }
        let cap = opts.max_product_states;
        match crate::pattern::compile(src, ir::regex::Dialect::Ecma, cap) {
            Ok(p) => {
                self.pattern_work = self.pattern_work.saturating_add(p.work);
                if self.pattern_work > opts.max_pattern_work {
                    return Err(BuildError::Limit {
                        what: "trabajo de los `pattern` del esquema",
                        limit: opts.max_pattern_work,
                        found: self.pattern_work,
                        location: location_clipped(ir, id, LOCATION_MAX),
                    });
                }
                self.sets.extend(p.sets.iter().cloned());
                self.patterns.insert((id, i), p);
                Ok(())
            }
            Err(crate::pattern::PatternError::Limit(n)) => Err(BuildError::Limit {
                what: "estados del AFD de `pattern`",
                limit: cap,
                found: n,
                location: location_clipped(ir, id, LOCATION_MAX),
            }),
            Err(crate::pattern::PatternError::Invalid(e)) => Err(BuildError::Internal(format!(
                "`pattern` que la Fase 3 aceptó y `ir::regex` no: {e}"
            ))),
        }
    }

    /// El producto del trie de `words` y de los AFD de `pats` sobre la
    /// partición común de la cadena, minimizado: (AFD, partición, estados
    /// antes de minimizar). Sin `pattern`, es el trie (minimizado). Los
    /// estados se cuentan antes de crearlos: más de `max_product_states` es
    /// `ProductLimit` (marco escalar).
    fn string_dfa(
        &self,
        ir: &VerifiedIr,
        comps: &[NodeId],
        opts: &BuildOptions,
        words: &[&str],
        pats: &[(NodeId, usize)],
    ) -> Result<(Dfa<StrOut>, crate::pattern::Partition, usize), BuildError> {
        let singles: Vec<u32> = {
            let mut v: Vec<u32> = words
                .iter()
                .flat_map(|w| w.chars())
                .map(u32::from)
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let ps: Vec<&crate::pattern::PatternDfa> = pats
            .iter()
            .map(|&(id, i)| &self.patterns[&(self.cl.real(id), i)])
            .collect();
        let sets: Vec<&ir::regex::CharSet> = ps.iter().flat_map(|p| p.sets.iter()).collect();
        let inst = crate::pattern::Partition::refine(&singles, &sets);
        let k = inst.n_classes;
        let reps = inst.reps();
        let ws: Vec<Vec<usize>> = words
            .iter()
            .map(|w| {
                w.chars()
                    .map(|c| inst.class(u32::from(c)) as usize)
                    .collect()
            })
            .collect();
        // Cota del producto antes de reservar: estados (≤ el tope) × clases.
        let bound = ps
            .iter()
            .fold(trie_size(words), |b, p| b.saturating_mul(p.dfa.n_states()))
            .min(opts.max_product_states)
            .saturating_mul(k);
        if bound > opts.max_delta {
            return Err(BuildError::Limit {
                what: "entradas del producto de una cadena (estados × clases)",
                limit: opts.max_delta,
                found: bound,
                location: label(ir, comps),
            });
        }
        let trie = Dfa::trie(k, &ws);
        // Clase local de cada `pattern` para cada clase de la cadena.
        let local: Vec<Vec<usize>> = ps
            .iter()
            .map(|p| reps.iter().map(|&c| p.part.class(c) as usize).collect())
            .collect();
        type Key = (u32, Vec<u32>);
        let start: Key = (trie.start, ps.iter().map(|p| p.dfa.start).collect());
        let mut ids: HashMap<Key, u32> = HashMap::from([(start.clone(), 0)]);
        let mut keys = vec![start];
        let mut trans = Vec::new();
        let cap = opts.max_product_states;
        let mut i = 0;
        while i < keys.len() {
            let (t, qs) = keys[i].clone();
            for a in 0..k {
                let key: Key = (
                    trie.next(t, a),
                    qs.iter()
                        .zip(&ps)
                        .zip(&local)
                        .map(|((&q, p), l)| p.dfa.next(q, l[a]))
                        .collect(),
                );
                let id = match ids.get(&key) {
                    Some(&id) => id,
                    None => {
                        if keys.len() >= cap {
                            return Err(BuildError::ProductLimit {
                                states: keys.len() + 1,
                                limit: cap,
                                scalar: true,
                                location: label(ir, comps),
                            });
                        }
                        let id = keys.len() as u32;
                        ids.insert(key.clone(), id);
                        keys.push(key);
                        id
                    }
                };
                trans.push(id);
            }
            i += 1;
        }
        let out = keys
            .iter()
            .map(|(t, qs)| {
                let bits = qs
                    .iter()
                    .zip(&ps)
                    .enumerate()
                    .filter(|(_, (&q, p))| p.dfa.out[q as usize])
                    .fold(0u64, |b, (j, _)| b | (1 << j));
                (trie.out[*t as usize], bits)
            })
            .collect();
        let raw = Dfa {
            n_classes: k,
            start: 0,
            trans,
            out,
        };
        let raw_states = raw.n_states();
        Ok((crate::minimize::hopcroft(&raw), inst, raw_states))
    }

    /// La máscara de [`entry_fails`] de la regla `i` de `id` sobre las 7
    /// clases de `class::VALUE_STARTS` (bit k = falla con la clase k),
    /// calculada una vez por regla. Solo `false`, `type`, `enum` y `const`
    /// pueden fallar al entrar; las demás dan 0 sin guardarse.
    fn entry_mask(&mut self, id: NodeId, i: usize, rule: &Rule) -> u8 {
        if !matches!(
            rule,
            Rule::Reject | Rule::Type(_) | Rule::Enum(_) | Rule::Const(_)
        ) {
            return 0;
        }
        // Memorias por (nodo real, regla): una vez aunque el nodo aparezca
        // con varias salidas (F4.1b-CR3-S2).
        let id = self.cl.real(id);
        if let Some(&m) = self.entry_masks.get(&(id, i)) {
            return m;
        }
        let m = class::VALUE_STARTS
            .iter()
            .enumerate()
            .fold(0u8, |m, (k, &a)| m | (u8::from(entry_fails(rule, a)) << k));
        self.entry_masks.insert((id, i), m);
        m
    }

    /// Los números de la regla `i` de `id`, ordenados una sola vez por regla
    /// (aunque la lean muchos conjuntos): índice en `enums`.
    fn numbers(
        &mut self,
        id: NodeId,
        i: usize,
        lits: &'a [Literal],
    ) -> Result<Option<u32>, BuildError> {
        let id = self.cl.real(id);
        if let Some(&k) = self.enum_ids.get(&(id, i)) {
            return Ok(k);
        }
        let k = match numbers(lits) {
            Some(v) => {
                self.extra = self.extra.saturating_add(v.len());
                self.enums.push(v);
                Some(table_index(self.enums.len() - 1)?)
            }
            None => None,
        };
        self.enum_ids.insert((id, i), k);
        Ok(k)
    }

    /// Las cadenas de la regla `i` de `id`, una sola vez por regla.
    fn strings(
        &mut self,
        id: NodeId,
        i: usize,
        lits: &'a [Literal],
    ) -> Result<Option<u32>, BuildError> {
        let id = self.cl.real(id);
        if let Some(&k) = self.str_ids.get(&(id, i)) {
            return Ok(k);
        }
        let k = match strings(lits) {
            Some(v) => {
                self.strsets.push(v);
                Some(table_index(self.strsets.len() - 1)?)
            }
            None => None,
        };
        self.str_ids.insert((id, i), k);
        Ok(k)
    }
}

/// Si la regla falla al empezar un valor con la clase `a` (decidido al
/// compilar): `false`, `type` y el tipo de los literales de `enum`/`const`.
fn entry_fails(rule: &Rule, a: u32) -> bool {
    match rule {
        Rule::Reject => true,
        Rule::Type(set) => !admits(*set, a),
        Rule::Enum(lits) => !lits.iter().any(|l| {
            #[cfg(test)]
            tests::LITERALES.with(|c| c.set(c.get() + 1));
            literal_admits(l, a)
        }),
        Rule::Const(l) => !literal_admits(l, a),
        _ => false,
    }
}

/// Los lexemas de los números de un `enum`, ordenados por valor y sin
/// repetir (`1`, `1.0` y `1e0` son uno); `None` si no tiene números.
fn numbers(lits: &[Literal]) -> Option<Vec<&str>> {
    let mut v: Vec<&str> = lits
        .iter()
        .filter_map(|l| match l.0.first() {
            Some(LitToken::Number(n)) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| ir::num::compare(a, b));
    v.dedup_by(|a, b| ir::num::compare(a, b) == Ordering::Equal);
    Some(v)
}

/// Las cadenas de un `enum`; `None` si no tiene ninguna.
fn strings(lits: &[Literal]) -> Option<HashSet<&str>> {
    let v: HashSet<&str> = lits
        .iter()
        .filter_map(|l| match l.0.first() {
            Some(LitToken::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    (!v.is_empty()).then_some(v)
}

/// Si el literal admite un valor que empieza con la clase `a` (solo
/// escalares: los compuestos son `Pending` de F4.5b).
fn literal_admits(lit: &Literal, a: u32) -> bool {
    match lit.0.first() {
        Some(LitToken::Null) => a == class::NULL,
        Some(LitToken::Bool(true)) => a == class::TRUE,
        Some(LitToken::Bool(false)) => a == class::FALSE,
        Some(LitToken::Number(_)) => a == class::NUMBER_BEGIN,
        Some(LitToken::String(_)) => a == class::STRING_BEGIN,
        _ => false,
    }
}

/// F4-U6: el divisor de `multipleOf` pasa del tope (`limit` con la
/// ubicación de la regla).
fn divisor_limit(ir: &VerifiedIr, id: NodeId, e: DivisorError) -> BuildError {
    let (what, limit, found) = match e {
        DivisorError::Digits(n) => ("cifras significativas del divisor de multipleOf", 38, n),
        DivisorError::Coprime(b) => (
            "bits de la parte del divisor de multipleOf coprima con 10",
            60,
            b as usize,
        ),
        DivisorError::Twos(a) => ("exponente de 2 del divisor de multipleOf", 63, a as usize),
        DivisorError::Fives(b) => ("exponente de 5 del divisor de multipleOf", 27, b as usize),
    };
    BuildError::Limit {
        what,
        limit,
        found,
        location: location_clipped(ir, id, LOCATION_MAX),
    }
}

// ------------------------------------------------------------ builder ---

/// Estados de entrada de un conjunto (o de «cualquiera»).
#[derive(Clone, Copy)]
struct SetStates {
    obj_first: u32,
    obj_frame: u32,
    arr_first: u32,
    arr_frame: u32,
    number: u32,
    string: u32,
    /// Tipo de marco sin apilar con las salidas y puertas de las instancias
    /// escalares del conjunto (F4.5a).
    scalar_frame: u32,
}

/// Estados de un subautómata de objeto.
struct ObjectStates {
    first: u32,
    next: u32,
    /// Estado de cada estado del trie (el muerto es «clave desconocida»).
    trie: Vec<u32>,
    /// Por nombre: (tras la clave, tras `:`).
    after_key: Vec<(u32, u32)>,
    unk_key: (u32, u32),
    after: u32,
}

struct ArrayStates {
    first: u32,
    /// `next[i]` y `after[i]` para el índice i (saturado en el número de
    /// `prefixItems`).
    next: Vec<u32>,
    after: Vec<u32>,
}

/// Los seis estados del objeto «cualquiera».
struct ObjectStatesAny {
    first: u32,
    next: u32,
    key: u32,
    colon: u32,
    value: u32,
    after: u32,
}

struct Builder<'a> {
    ir: &'a VerifiedIr,
    an: Analysis<'a>,
    /// Las tablas en construcción y sus memorias comunes (F4.1b-CR-S6).
    t: Emitter<'a>,
    entry_ids: HashMap<(SetId, u32, u32, u32, bool), u32>,
    /// Comprobaciones de la entrada de un valor de cada conjunto con cada
    /// clase, compartidas por todas las transiciones de entrada con
    /// retornos distintos (ronda 2 de F4.1b, I1).
    entry_checks: HashMap<(SetId, u32), (u32, u32)>,
    /// Divisor de cada `multipleOf` y rango de cada `enum` numérico en las
    /// tablas: una vez por regla (los lexemas de las cotas, en `t`).
    divisor_ids: HashMap<(NodeId, usize), u32>,
    enum_ranges: HashMap<u32, u32>,
    any: SetStates,
    sets: Vec<Option<SetStates>>,
    objects: Vec<Option<(ObjectStates, Dfa<Option<u32>>)>>,
    arrays: Vec<Option<ArrayStates>>,
    /// Los estados de cada cadena (uno por estado de su AFD).
    strings: Vec<Option<Vec<u32>>>,
    any_object: ObjectStatesAny,
    any_array: ArrayStates,
    any_string: u32,
    any_number: u32,
    root_start: u32,
    root_done: u32,
}

impl<'a> Builder<'a> {
    fn new(ir: &'a VerifiedIr, an: Analysis<'a>, opts: &BuildOptions) -> Result<Self, BuildError> {
        // Clases de carácter: un punto de código distinto por carácter de
        // algún nombre con bit o de alguna cadena de un `enum`/`const`
        // (F4-B3, partición común de claves y valores); los demás, la 0.
        // |Σ| exacto: la partición de los caracteres y de los conjuntos de
        // los `pattern` (F4.4b), la misma que construye `Emitter::new`.
        let n_classes = class::CHAR as usize + an.n_char_classes();
        if budget(an.upper, n_classes, an.extra) > opts.max_delta {
            return Err(delta_limit(
                an.upper,
                n_classes,
                an.extra,
                opts.max_delta,
                location_clipped(ir, NodeId::ROOT, LOCATION_MAX),
            ));
        }
        // Desde aquí |Q|, |Σ|, los estados de cada trie y las clases son
        // ≤ 2²⁰, así que sus conversiones a `u32` no truncan; las longitudes
        // de las tablas pasan por `idx`.
        let t = Emitter::new(
            ir,
            &an.chars,
            &an.sets,
            &class::NAMES,
            an.upper,
            an.products,
        );
        let n_sets = an.cl.sets.len();
        let empty = SetStates {
            obj_first: 0,
            obj_frame: 0,
            arr_first: 0,
            arr_frame: 0,
            number: 0,
            string: 0,
            scalar_frame: 0,
        };
        let mut t = t;
        t.set_virtuals(an.cl.virtuals());
        t.outmaps(&an.cl.outmaps);
        let mut b = Builder {
            ir,
            an,
            t,
            entry_ids: HashMap::new(),
            entry_checks: HashMap::new(),
            divisor_ids: HashMap::new(),
            enum_ranges: HashMap::new(),
            any: empty,
            sets: (0..n_sets).map(|_| None).collect(),
            objects: (0..n_sets).map(|_| None).collect(),
            arrays: (0..n_sets).map(|_| None).collect(),
            strings: (0..n_sets).map(|_| None).collect(),
            any_object: ObjectStatesAny {
                first: 0,
                next: 0,
                key: 0,
                colon: 0,
                value: 0,
                after: 0,
            },
            any_array: ArrayStates {
                first: 0,
                next: Vec::new(),
                after: Vec::new(),
            },
            any_string: 0,
            any_number: 0,
            root_start: 0,
            root_done: 0,
        };
        b.reserve();
        Ok(b)
    }

    // ----------------------------------------------------- reservas ---

    fn array_states(&mut self, g: u32, p: usize) -> ArrayStates {
        let first = self.t.state(g, "[ primero");
        let next = if p == 0 {
            vec![self.t.state(g, "[ tras ,")]
        } else {
            std::iter::once(u32::MAX)
                .chain((1..=p).map(|i| self.t.state(g, format!("[ valor {i}"))))
                .collect()
        };
        let after = (0..=p)
            .map(|i| self.t.state(g, format!("[ tras valor {i}")))
            .collect();
        ArrayStates { first, next, after }
    }

    /// Reserva todos los estados y marcos (sin rellenar δ).
    fn reserve(&mut self) {
        let g = self.t.group("raíz (Z₀)".to_owned());
        self.root_start = self.t.state(g, "q₀");
        self.root_done = self.t.state(g, "fin");
        self.t.a.start = self.root_start;
        self.t.a.accept = self.root_done;

        let g = self.t.group("cualquiera: objeto".to_owned());
        self.any_object = ObjectStatesAny {
            first: self.t.state(g, "{ primero"),
            next: self.t.state(g, "{ tras ,"),
            key: self.t.state(g, "clave"),
            colon: self.t.state(g, "tras clave"),
            value: self.t.state(g, "tras :"),
            after: self.t.state(g, "tras valor"),
        };
        let any_obj_frame = self.t.frame(0, 0, "cualquiera: objeto".to_owned());
        let g = self.t.group("cualquiera: arreglo".to_owned());
        self.any_array = self.array_states(g, 0);
        let any_arr_frame = self.t.frame(0, 0, "cualquiera: arreglo".to_owned());
        let g = self.t.group("cualquiera: escalares".to_owned());
        self.any_string = self.t.state(g, "cadena");
        self.any_number = self.t.state(g, "número");
        self.any = SetStates {
            obj_first: self.any_object.first,
            obj_frame: any_obj_frame,
            arr_first: self.any_array.first,
            arr_frame: any_arr_frame,
            number: self.any_number,
            string: self.any_string,
            scalar_frame: 0,
        };

        for k in 0..self.an.order.len() {
            let s = self.an.order[k];
            let Some(info) = self.an.infos[s as usize].take() else {
                continue;
            };
            let loc = label(self.ir, self.an.cl.sets.get(s));
            let mut st = self.any;
            // F4.5a: las salidas y puertas del conjunto van en sus tipos de
            // marco (también los de «cualquiera» si no tiene reglas de objeto
            // o de arreglo) y en un tipo sin apilar para sus escalares.
            let meta = self.an.cl.sets.meta(s).clone();
            let ns = meta.n_slots;
            let generic = if ns > 0 {
                self.t.gates(&meta.gates, None)
            } else {
                (0, 0)
            };
            if ns > 0 {
                st.obj_frame = self
                    .t
                    .frame_with(0, 0, ns, generic, format!("objeto {loc}"));
                st.arr_frame = self
                    .t
                    .frame_with(0, 0, ns, generic, format!("arreglo {loc}"));
                st.scalar_frame = self
                    .t
                    .frame_with(0, 0, ns, generic, format!("escalar {loc}"));
            }
            if let Some(o) = &info.obj {
                let g = self.t.group(format!("objeto {loc}"));
                let first = self.t.state(g, "{ primero");
                let next = self.t.state(g, "{ tras ,");
                let (trie_states, trie) =
                    self.t
                        .trie_states(g, &o.names, "clave ", "clave desconocida");
                let after_key = o
                    .names
                    .iter()
                    .map(|n| {
                        (
                            self.t.state(g, format!("tras «{}»", clip(n))),
                            self.t.state(g, format!("valor «{}»", clip(n))),
                        )
                    })
                    .collect();
                let unk_key = (
                    self.t.state(g, "tras clave desconocida"),
                    self.t.state(g, "valor desconocido"),
                );
                let after = self.t.state(g, "tras valor");
                let mask = o.names.len().div_ceil(64) as u32;
                let words = mask + u32::from(!o.counts.is_empty());
                st.obj_first = first;
                st.obj_frame = if ns > 0 {
                    let bits: HashMap<&str, u32> = o
                        .names
                        .iter()
                        .enumerate()
                        .map(|(j, n)| (*n, j as u32))
                        .collect();
                    let g = self.t.gates(&meta.gates, Some(&bits));
                    self.t
                        .frame_with(words, mask, ns, g, format!("objeto {loc}"))
                } else {
                    self.t.frame(words, mask, format!("objeto {loc}"))
                };
                self.objects[s as usize] = Some((
                    ObjectStates {
                        first,
                        next,
                        trie: trie_states,
                        after_key,
                        unk_key,
                        after,
                    },
                    trie,
                ));
            }
            if let Some(arr) = &info.arr {
                let g = self.t.group(format!("arreglo {loc}"));
                let states = self.array_states(g, arr.slots.len() - 1);
                st.arr_first = states.first;
                let words = u32::from(!arr.counts.is_empty());
                st.arr_frame = if ns > 0 {
                    self.t
                        .frame_with(words, 0, ns, generic, format!("arreglo {loc}"))
                } else {
                    self.t.frame(words, 0, format!("arreglo {loc}"))
                };
                if !arr.unique.is_empty() {
                    self.t.a.frames[st.arr_frame as usize].unique = true;
                }
                self.arrays[s as usize] = Some(states);
            }
            if info.num.is_some() {
                let g = self.t.group(format!("número {loc}"));
                let name = if info
                    .num
                    .as_ref()
                    .is_some_and(|n| n.checks.iter().all(|c| matches!(c.2, NumTest::Integer)))
                {
                    "entero"
                } else {
                    "número"
                };
                st.number = self.t.state(g, name);
            }
            if let Some(sp) = &info.string {
                let g = self.t.group(format!("cadena {loc}"));
                let d = &sp.dfa;
                let mut states = Vec::with_capacity(d.n_states());
                for q in 0..d.n_states() as u32 {
                    let name = if d.is_dead(q) {
                        "fuera de los literales".to_owned()
                    } else {
                        match d.out[q as usize].0 {
                            Some(w) => format!("«{}»", crate::emit::clip(sp.words[w as usize])),
                            None => format!("c{q}"),
                        }
                    };
                    states.push(self.t.state(g, name));
                }
                st.string = states[d.start as usize];
                self.t.a.dfa_states += sp.raw_states;
                self.t.a.dfa_states_min += d.n_states();
                self.strings[s as usize] = Some(states);
            }
            self.sets[s as usize] = Some(st);
            self.an.infos[s as usize] = Some(info);
        }
    }

    // --------------------------------------------------- rellenado ---

    fn states_of(&self, s: SetId) -> SetStates {
        self.sets
            .get(s as usize)
            .copied()
            .flatten()
            .unwrap_or(self.any)
    }

    /// Transición compuesta que empieza un valor del conjunto `s` con la
    /// clase `a` y vuelve a `ret` al terminarlo (`automata.md` §6.1): las
    /// comprobaciones del primer evento de cada componente (`type`, `false`,
    /// el tipo de los literales de `enum`/`const`) y la operación que abre
    /// la instancia. `count` suma 1 al contador del arreglo que la contiene.
    fn entry(&mut self, s: SetId, outs: u32, a: u32, ret: u32, count: bool) -> u32 {
        if let Some(&t) = self.entry_ids.get(&(s, outs, a, ret, count)) {
            return t;
        }
        let st = self.states_of(s);
        let (checks, n_checks) = match self.entry_checks.get(&(s, a)) {
            Some(&c) => c,
            None => {
                let comps = self.an.cl.sets.get(s).to_vec();
                let mut list = Vec::new();
                // `entry` solo se llama con las clases que empiezan un valor;
                // otra es un error interno, nunca el bit de `{` en silencio
                // (F4.1b-CR3-S3).
                let bit = match class::VALUE_STARTS.iter().position(|&x| x == a) {
                    Some(b) => b,
                    None => {
                        self.t.internal(format!("clase {a} al empezar un valor"));
                        0
                    }
                };
                for id in comps {
                    let node = self.ir.node(self.an.cl.real(id));
                    for (i, c) in node.rules.iter().enumerate() {
                        // La máscara ya calculada en el análisis: O(reglas)
                        // por conjunto, sin volver a recorrer los literales.
                        if self.an.entry_mask(id, i, &c.rule) & (1 << bit) != 0 {
                            let mut c = self.t.check(id, i, Test::Fails, 0, 0);
                            // Cuenta en la instancia que se abre (F4.5a).
                            c.slot |= SLOT_CHILD;
                            list.push(c);
                        }
                    }
                }
                let c = self.t.checks(list);
                self.entry_checks.insert((s, a), c);
                c
            }
        };
        // Con salidas (F4.5a), los escalares y los valores de un evento
        // llevan el tipo de marco (sin apilar) de sus puertas.
        let sf = if outs == OUT_IDENTITY {
            0
        } else {
            st.scalar_frame
        };
        let (op, next, frame) = match a {
            class::BEGIN_OBJECT => (Op::Push, st.obj_first, st.obj_frame),
            class::BEGIN_ARRAY => (Op::Push, st.arr_first, st.arr_frame),
            class::STRING_BEGIN => (Op::Enter, st.string, sf),
            class::NUMBER_BEGIN => (Op::Enter, st.number, sf),
            _ => (Op::Move, ret, sf),
        };
        let (counts, n_counts) = self.count_op(count);
        let t = self.t.trans(Trans {
            ret: if op == Op::Move { 0 } else { ret },
            frame,
            counts,
            n_counts,
            checks,
            n_checks,
            outs,
            ..plain(op, next)
        });
        self.entry_ids.insert((s, outs, a, ret, count), t);
        t
    }

    /// La operación «suma 1 al contador del marco» (el único de un objeto o
    /// un arreglo JSON, el 0), o ninguna.
    fn count_op(&mut self, count: bool) -> (u32, u32) {
        if !count {
            return (0, 0);
        }
        self.t.counter_ops(&[CounterOp {
            counter: 0,
            op: CountOp::Add,
        }])
    }

    /// Las siete clases que empiezan un valor de `s`, en `q`.
    fn value(&mut self, q: u32, (s, outs): (SetId, u32), ret: u32, count: bool) {
        for a in class::VALUE_STARTS {
            let t = self.entry(s, outs, a, ret, count);
            self.t.set(q, a, t);
        }
    }

    fn chars(&mut self, q: u32, to: u32) {
        let t = self.t.mv(to);
        for a in class::CHAR..self.t.n_classes {
            self.t.set(q, a, t);
        }
    }

    fn fill(&mut self) {
        let (s, d) = (self.root_start, self.root_done);
        self.value(s, (self.an.root, self.an.root_outs), d, false);

        // Objeto «cualquiera».
        let o = &self.any_object;
        let (first, next, key, colon, value, after) =
            (o.first, o.next, o.key, o.colon, o.value, o.after);
        let pop = self.t.simple(Op::Pop, 0);
        let to_key = self.t.mv(key);
        for q in [first, next] {
            self.t.set(q, class::STRING_BEGIN, to_key);
        }
        self.t.set(first, class::END_OBJECT, pop);
        self.chars(key, key);
        let t = self.t.mv(colon);
        self.t.set(key, class::STRING_END, t);
        let t = self.t.mv(value);
        self.t.set(colon, class::COLON, t);
        self.value(value, (ANY, OUT_IDENTITY), after, false);
        let t = self.t.mv(next);
        self.t.set(after, class::COMMA, t);
        self.t.set(after, class::END_OBJECT, pop);

        // Arreglo «cualquiera», cadena y número.
        let arr = std::mem::replace(
            &mut self.any_array,
            ArrayStates {
                first: 0,
                next: Vec::new(),
                after: Vec::new(),
            },
        );
        self.fill_array(&arr, &[(ANY, OUT_IDENTITY)], &[]);
        self.any_array = arr;
        let (s, n) = (self.any_string, self.any_number);
        self.chars(s, s);
        let leave = self.t.simple(Op::Leave, 0);
        self.t.set(s, class::STRING_END, leave);
        let t = self.t.mv(n);
        self.t.set(n, class::NUMBER_BYTE, t);
        self.t.set(n, class::NUMBER_END, leave);

        for k in 0..self.an.order.len() {
            let s = self.an.order[k];
            let Some(info) = self.an.infos[s as usize].take() else {
                continue;
            };
            if let Some(o) = &info.obj {
                self.fill_object(s, o);
            }
            if let Some(arr) = &info.arr {
                if let Some(states) = self.arrays[s as usize].take() {
                    let mut checks: Vec<Check> =
                        arr.counts.iter().map(|&b| self.t.bound(b)).collect();
                    for &(id, i) in &arr.unique {
                        checks.push(self.t.check(id, i, Test::Unique, 0, 0));
                    }
                    self.fill_array_with(&states, &arr.slots, checks, !arr.counts.is_empty());
                    self.arrays[s as usize] = Some(states);
                }
            }
            if let Some(n) = &info.num {
                self.fill_number(s, n);
            }
            if let Some(sp) = &info.string {
                self.fill_string(s, sp);
            }
            self.an.infos[s as usize] = Some(info);
        }
    }

    fn fill_array(&mut self, arr: &ArrayStates, slots: &[(SetId, u32)], counts: &[Bound]) {
        let list: Vec<Check> = counts.iter().map(|&b| self.t.bound(b)).collect();
        self.fill_array_with(arr, slots, list, !counts.is_empty());
    }

    /// [`Builder::fill_array`] con las comprobaciones del cierre dadas (los
    /// contadores y `uniqueItems`, F4.5b).
    fn fill_array_with(
        &mut self,
        arr: &ArrayStates,
        slots: &[(SetId, u32)],
        list: Vec<Check>,
        count: bool,
    ) {
        let p = slots.len() - 1;
        let pop = self.t.with_checks(Op::Pop, 0, list);
        self.t.set(arr.first, class::END_ARRAY, pop);
        self.value(arr.first, slots[0], arr.after[0], count);
        for i in 0..=p {
            let n = arr.next[(i + 1).min(p)];
            let t = self.t.mv(n);
            self.t.set(arr.after[i], class::COMMA, t);
            self.t.set(arr.after[i], class::END_ARRAY, pop);
        }
        // Con `prefixItems`, el elemento 0 lo espera «primero» y los demás
        // `next[i]`; sin ellos, `next[0]` espera todos los siguientes.
        for (i, &slot) in slots.iter().enumerate().skip(usize::from(p > 0)) {
            self.value(arr.next[i], slot, arr.after[i], count);
        }
    }

    fn fill_number(&mut self, s: SetId, spec: &NumSpec<'_>) {
        let q = self.states_of(s).number;
        let mut list = Vec::new();
        for &(id, i, ref t) in &spec.checks {
            let c = match *t {
                NumTest::Integer => self.t.check(id, i, Test::Integer, 0, 0),
                NumTest::Cmp(test, l) => {
                    let arg = self.t.rule_lexeme(id, i, l);
                    self.t.check(id, i, test, arg, 0)
                }
                NumTest::Multiple(l, div) => {
                    let arg = match self.divisor_ids.get(&(id, i)) {
                        Some(&d) => d,
                        None => {
                            let lexeme = self.t.rule_lexeme(id, i, l);
                            self.t.a.divisors.push(DivisorRef { lexeme, div });
                            let d = self.t.last(self.t.a.divisors.len());
                            self.divisor_ids.insert((id, i), d);
                            d
                        }
                    };
                    self.t.check(id, i, Test::MultipleOf, arg, 0)
                }
                NumTest::Enum(k) => {
                    let n = self.an.enums[k as usize].len();
                    let arg = match self.enum_ranges.get(&k) {
                        Some(&a) => a,
                        None => {
                            let a = self.t.idx(self.t.a.nums.len());
                            for j in 0..n {
                                let l = self.an.enums[k as usize][j];
                                self.t.lexeme(l);
                            }
                            self.enum_ranges.insert(k, a);
                            a
                        }
                    };
                    let len = self.t.idx(n);
                    self.t.check(id, i, Test::NumEnum, arg, len)
                }
            };
            list.push(c);
        }
        let leave = self.t.with_checks(Op::Leave, 0, list);
        let t = self.t.mv(q);
        self.t.set(q, class::NUMBER_BYTE, t);
        self.t.set(q, class::NUMBER_END, leave);
    }

    fn fill_string(&mut self, s: SetId, spec: &StrSpec<'_>) {
        let Some(states) = self.strings[s as usize].take() else {
            return;
        };
        let d = &spec.dfa;
        // La clase de la cadena de cada clase de carácter global (F4.4b: la
        // partición global refina la de la cadena).
        let local: Vec<usize> = (0..self.t.n_chars())
            .map(|a| spec.inst.class(self.t.class_rep(a)) as usize)
            .collect();
        for ts in 0..d.n_states() as u32 {
            let q = states[ts as usize];
            for (a, &l) in local.iter().enumerate() {
                let t = self.t.mv(states[d.next(ts, l) as usize]);
                self.t.set(q, class::CHAR + a as u32, t);
            }
            let (word, bits) = d.out[ts as usize];
            let word = word.map(|w| spec.words[w as usize]);
            let mut list = Vec::new();
            for &(id, i, k) in &spec.lits {
                let set = &self.an.strsets[k as usize];
                if !word.is_some_and(|w| set.contains(w)) {
                    list.push(self.t.check(id, i, Test::Fails, 0, 0));
                }
            }
            for &b in &spec.lens {
                list.push(self.t.bound(b));
            }
            // El veredicto de cada `pattern` lo decide el estado, al
            // compilar: 1 violación al cerrar la cadena si no se cumple.
            for (j, &(id, i)) in spec.pats.iter().enumerate() {
                if bits & (1 << j) == 0 {
                    list.push(self.t.check(id, i, Test::Fails, 0, 0));
                }
            }
            let leave = self.t.with_checks(Op::Leave, 0, list);
            self.t.set(q, class::STRING_END, leave);
        }
        self.strings[s as usize] = Some(states);
    }

    fn fill_object(&mut self, s: SetId, o: &ObjSpec<'_>) {
        let Some((st, trie)) = self.objects[s as usize].take() else {
            return;
        };
        let words = o.names.len().div_ceil(64);
        // Búsquedas en O(1): con 4 096 nombres, `position` sería cuadrático.
        // (Los nombres son ≤ MAX_NAMES, así que sus bits caben en u32.)
        let bits: HashMap<&str, u32> = o
            .names
            .iter()
            .enumerate()
            .map(|(i, n)| (*n, i as u32))
            .collect();
        // Todo nombre de `required` y `dependentRequired` está en la unión;
        // si no, es un error interno (nunca el bit 0 en silencio).
        let bit = |b: &mut Self, n: &str| match bits.get(n) {
            Some(&k) => k,
            None => {
                b.t.internal(format!("el nombre {n:?} no está en la unión del objeto"));
                0
            }
        };
        // Máscaras de `required` y de cada lista de `dependentRequired`.
        let mask_of = |b: &mut Self, list: &[String]| -> u32 {
            let ini = b.t.a.masks.len();
            b.t.a.masks.extend(std::iter::repeat_n(0, words));
            for n in list {
                let k = bit(b, n) as usize;
                b.t.a.masks[ini + k / 64] |= 1 << (k % 64);
            }
            b.t.idx(ini)
        };
        let mut pops = Vec::new();
        for &(id, i, req) in &o.required {
            if !req.is_empty() {
                let m = mask_of(self, req);
                pops.push(self.t.check(id, i, Test::Mask, m, 0));
            }
        }
        for &(id, i, dep) in &o.dependent {
            let ini = self.t.idx(self.t.a.deps.len());
            for (k, l) in dep {
                if l.is_empty() {
                    continue;
                }
                let m = mask_of(self, l);
                let b = bit(self, k);
                self.t.a.deps.push(Dep { bit: b, mask: m });
            }
            let n = self.t.idx(self.t.a.deps.len()).saturating_sub(ini);
            if n > 0 {
                pops.push(self.t.check(id, i, Test::Dependent, ini, n));
            }
        }
        for &b in &o.counts {
            pops.push(self.t.bound(b));
        }
        let pop = self.t.with_checks(Op::Pop, 0, pops);
        let start = st.trie[trie.start as usize];
        let (counts, n_counts) = self.count_op(!o.counts.is_empty());
        let to_key = self.t.trans(Trans {
            counts,
            n_counts,
            ..plain(Op::Move, start)
        });
        self.t.set(st.first, class::STRING_BEGIN, to_key);
        self.t.set(st.next, class::STRING_BEGIN, to_key);
        self.t.set(st.first, class::END_OBJECT, pop);
        let k = trie.n_classes;
        for ts in 0..trie.n_states() as u32 {
            let q = st.trie[ts as usize];
            for a in 0..k {
                let t = self.t.mv(st.trie[trie.next(ts, a) as usize]);
                self.t.set(q, class::CHAR + a as u32, t);
            }
            let end = match trie.out[ts as usize] {
                Some(j) => self.t.trans(Trans {
                    bit: j,
                    ..plain(Op::Move, st.after_key[j as usize].0)
                }),
                None => self.t.mv(st.unk_key.0),
            };
            self.t.set(q, class::STRING_END, end);
        }
        for (j, &child) in o.children.iter().enumerate() {
            let (k_state, v_state) = st.after_key[j];
            let t = self.t.mv(v_state);
            self.t.set(k_state, class::COLON, t);
            self.value(v_state, child, st.after, false);
        }
        let t = self.t.mv(st.unk_key.1);
        self.t.set(st.unk_key.0, class::COLON, t);
        self.value(st.unk_key.1, o.unknown, st.after, false);
        let t = self.t.mv(st.next);
        self.t.set(st.after, class::COMMA, t);
        self.t.set(st.after, class::END_OBJECT, pop);
        self.objects[s as usize] = Some((st, trie));
    }
}

/// `type` admite el valor que empieza con la clase `a`; un número solo se
/// rechaza aquí si `type` no admite ni `number` ni `integer` (si solo
/// admite `integer`, se comprueba al cerrarlo).
fn admits(set: TypeSet, a: u32) -> bool {
    let t = match a {
        class::BEGIN_OBJECT => JsonType::Object,
        class::BEGIN_ARRAY => JsonType::Array,
        class::STRING_BEGIN => JsonType::String,
        class::TRUE | class::FALSE => JsonType::Boolean,
        class::NULL => JsonType::Null,
        _ => return set.contains(JsonType::Number) || set.contains(JsonType::Integer),
    };
    set.contains(t)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    std::thread_local! {
        /// Literales de `enum` que ha visitado `entry_fails` en este hilo.
        pub(super) static LITERALES: Cell<u64> = const { Cell::new(0) };
    }

    fn ir_de(src: String) -> ir::VerifiedIr {
        std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                let s = schema_parser::parse(src.as_bytes()).unwrap();
                semantic::lower(&semantic::analyze(&s).unwrap()).unwrap()
            })
            .unwrap()
            .join()
            .unwrap()
    }

    /// Ronda 3 de F4.1b: 2 000 conjuntos {E, `minimum: i`} que comparten un
    /// `enum` E de 10⁴ valores. `entry_fails` recorre los literales de E una
    /// sola vez por clase (≤ 7 · 10⁴ visitas), no una vez por conjunto (antes,
    /// 2 000 × 2 × 6 · 10⁴): el tiempo de compilación es O(Σ reglas de los
    /// componentes de cada conjunto), no conjuntos × |`enum`| (sin reloj, D1).
    #[test]
    fn enum_compartido_se_recorre_una_vez() {
        let m = 10_000;
        let vals: Vec<String> = (0..m).map(|i| format!("{i}.5")).collect();
        let items: Vec<String> = (0..2000)
            .map(|i| format!(r##"{{"allOf": [{{"$ref": "#/$defs/e"}}, {{"minimum": {i}}}]}}"##))
            .collect();
        let src = format!(
            r#"{{"prefixItems": [{}], "$defs": {{"e": {{"enum": [{}]}}}}}}"#,
            items.join(","),
            vals.join(",")
        );
        let ir = ir_de(src);
        LITERALES.with(|c| c.set(0));
        let a = crate::build(&ir).unwrap();
        let visitas = LITERALES.with(Cell::get);
        assert!(visitas <= 7 * m as u64, "{visitas} visitas");
        // Y M sigue rechazando lo que no es número en esas posiciones.
        let r = crate::validate(&a, br#"["x", 1.5, 0.5]"#, runtime_rs::Mode::Total, 7);
        assert_eq!(r.violations.get(), Some(2));
    }
}
