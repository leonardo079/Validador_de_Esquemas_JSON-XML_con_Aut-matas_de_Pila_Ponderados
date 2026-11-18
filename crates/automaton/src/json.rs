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
    class, CharRange, Check, Dep, DivisorRef, Fault, Frame, Lexeme, Op, Rule as RuleRef, Test,
    Trans, NO_BIT, OUT_IDENTITY,
};

use crate::conj::{Closures, SetId, ANY, CLOSURE_WHAT};
use crate::dfa::Dfa;
use crate::location::{location_clipped, LOCATION_MAX};
use crate::model::{Automaton, StateInfo};
use crate::weight::cost;
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
    if let ir::Format::Xml { .. } = ir.format() {
        return Err(pending(ir, NodeId::ROOT, RuleKind::Content, Task::Xml));
    }
    check_pending(ir)?;
    let an = Analysis::run(ir, opts)?;
    let mut b = Builder::new(ir, an)?;
    b.fill();
    b.finish()
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
        Rule::Enum(_) | Rule::Const(_) | Rule::UniqueItems => Some(Task::Literals),
        Rule::Pattern(_) => Some(Task::Pattern),
        Rule::AnyOf(_) | Rule::OneOf(_) | Rule::Not(_) | Rule::DependentSchemas(_) => {
            Some(Task::Combinators)
        }
        Rule::Attributes(_) | Rule::Content(_) | Rule::SimpleContent(_) | Rule::Lexical(_) => {
            Some(Task::Xml)
        }
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
fn check_pending(ir: &VerifiedIr) -> Result<(), BuildError> {
    let n = ir.nodes().len();
    let mut seen = vec![false; n];
    let mut queue = std::collections::VecDeque::from([NodeId::ROOT]);
    seen[0] = true;
    let mut worst: Option<(Task, NodeId, RuleKind)> = None;
    let mut buf = Vec::new();
    while let Some(id) = queue.pop_front() {
        let node = ir.node(id);
        for c in &node.rules {
            if let Some(task) = task_of(&c.rule) {
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

/// Una comprobación de contador o longitud: (nodo, índice de la regla,
/// prueba, cota).
type Bound = (NodeId, usize, Test, u64);

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
    /// Hijo de cada nombre.
    children: Vec<SetId>,
    /// Hijo de una clave desconocida (unión de los `additionalProperties`).
    unknown: SetId,
    required: Vec<(NodeId, usize, &'a [String])>,
    dependent: Vec<(NodeId, usize, &'a [Dependencia])>,
    counts: Vec<Bound>,
}

/// El subautómata de arreglo de un conjunto: el índice saturado en el mayor
/// `prefixItems`; el hijo del índice i es la unión de los de cada
/// componente. `slots[p]` es el de los elementos siguientes.
struct ArrSpec {
    slots: Vec<SetId>,
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
}

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
}

/// Estados del trie de `words` (uno por prefijo distinto, más el muerto),
/// contados sin construirlo: con las palabras ordenadas, cada una añade sus
/// caracteres menos el prefijo común con la anterior.
fn trie_size(words: &[&str]) -> usize {
    let mut v: Vec<&str> = words.to_vec();
    v.sort_unstable();
    v.dedup();
    let mut n = 2;
    for (i, w) in v.iter().enumerate() {
        let lcp = if i == 0 {
            0
        } else {
            w.chars()
                .zip(v[i - 1].chars())
                .take_while(|(a, b)| a == b)
                .count()
        };
        n += w.chars().count() - lcp;
    }
    n
}

/// Nombre de un conjunto para el DOT y los errores: las ubicaciones de sus
/// componentes (hasta tres) unidas con «∧».
fn label(ir: &VerifiedIr, comps: &[NodeId]) -> String {
    let mut parts: Vec<String> = comps
        .iter()
        .take(3)
        .map(|&id| location_clipped(ir, id, 64))
        .collect();
    if comps.len() > 3 {
        parts.push("…".to_owned());
    }
    if parts.is_empty() {
        "#".to_owned()
    } else {
        parts.join(" ∧ ")
    }
}

impl<'a> Analysis<'a> {
    fn run(ir: &'a VerifiedIr, opts: &BuildOptions) -> Result<Analysis<'a>, BuildError> {
        let mut cl = Closures::new(ir.nodes())?;
        let root = cl.of(NodeId::ROOT)?;
        let mut an = Analysis {
            cl,
            infos: Vec::new(),
            order: Vec::new(),
            root,
            chars: BTreeSet::new(),
            upper: FIXED_STATES,
            extra: 0,
            products: 0,
            enums: Vec::new(),
            enum_ids: HashMap::new(),
            strsets: Vec::new(),
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
            let k = class::CHAR as usize + 1 + an.chars.len();
            if budget(an.upper, k, an.extra) > MAX_DELTA {
                return Err(delta_limit(
                    an.upper,
                    k,
                    an.extra,
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

    /// Analiza el conjunto `s` y devuelve sus subautómatas y sus hijos.
    fn set(
        &mut self,
        ir: &'a VerifiedIr,
        s: SetId,
        opts: &BuildOptions,
    ) -> Result<(SetInfo<'a>, Vec<SetId>), BuildError> {
        let comps: Vec<NodeId> = self.cl.sets.get(s).to_vec();
        let mut info = SetInfo::default();
        let mut children = Vec::new();
        let product = |b: &mut Self, n: usize, states: usize, scalar: bool| {
            if n < 2 {
                return Ok(());
            }
            b.products += 1;
            if states > opts.max_product_states {
                return Err(BuildError::ProductLimit {
                    states,
                    limit: opts.max_product_states,
                    scalar,
                    location: label(ir, &comps),
                });
            }
            Ok(())
        };

        // Comprobaciones de las 7 transiciones de entrada de un valor del
        // conjunto (una lista por clase, compartida por todos los retornos).
        for &id in &comps {
            for (i, c) in ir.node(id).rules.iter().enumerate() {
                self.extra += self.entry_mask(id, i, &c.rule).count_ones() as usize;
            }
        }

        // Objeto.
        let objs: Vec<(NodeId, ObjectRules<'a>)> = comps
            .iter()
            .filter_map(|&id| object_rules(id, ir.node(id)).map(|o| (id, o)))
            .collect();
        if !objs.is_empty() {
            let mut names: Vec<&'a str> = Vec::new();
            let mut idx: HashMap<&'a str, usize> = HashMap::new();
            for (_, o) in &objs {
                for n in o.names() {
                    if !idx.contains_key(n) {
                        idx.insert(n, names.len());
                        names.push(n);
                    }
                }
            }
            if names.len() > MAX_NAMES {
                return Err(BuildError::Limit {
                    what: "nombres por objeto",
                    limit: MAX_NAMES,
                    found: names.len(),
                    location: label(ir, &comps),
                });
            }
            // El tamaño del producto solo depende de los nombres y del número
            // de componentes: el tope y el trabajo (nombres × componentes) se
            // comprueban **antes** de reservar nada proporcional al producto
            // (L-1). Estados: primero, siguiente, trie, 2 por nombre, 2 de la
            // clave desconocida, después.
            let states = 2 + trie_size(&names) + 2 * names.len() + 2 + 1;
            product(self, objs.len(), states, false)?;
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
                unknown: ANY,
                required: Vec::new(),
                dependent: Vec::new(),
                counts: Vec::new(),
            };
            // Por componente: su `additionalProperties` y sus declarados (lo
            // que ya ocupa la IR).
            let mut comp: Vec<(SetId, HashMap<&str, NodeId>)> = Vec::with_capacity(objs.len());
            for &(id, ref o) in &objs {
                let add = match o.additional {
                    Some(t) => self.cl.of(t)?,
                    None => ANY,
                };
                comp.push((
                    add,
                    o.declared.iter().map(|(n, t)| (n.as_str(), *t)).collect(),
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
                for (add, declared) in &comp {
                    parts.push(match declared.get(n) {
                        Some(&t) => self.cl.of(t)?,
                        None => *add,
                    });
                }
                spec.children.push(self.cl.union(&parts)?);
            }
            let unknown: Vec<SetId> = comp.iter().map(|(add, _)| *add).collect();
            spec.unknown = self.cl.union(&unknown)?;
            self.upper += states;
            self.chars.extend(names.iter().flat_map(|n| n.chars()));
            children.extend(&spec.children);
            children.push(spec.unknown);
            spec.names = names;
            info.obj = Some(spec);
        }

        // Arreglo.
        let mut items: Vec<(&'a [NodeId], Option<NodeId>)> = Vec::new();
        let mut counts = Vec::new();
        let mut n_arr = 0;
        for &id in &comps {
            let mut here = false;
            for (i, c) in ir.node(id).rules.iter().enumerate() {
                match &c.rule {
                    Rule::Items { prefix, rest } => items.push((prefix, *rest)),
                    Rule::MinItems(n) => counts.push((id, i, Test::CountMin, *n)),
                    Rule::MaxItems(n) => counts.push((id, i, Test::CountMax, *n)),
                    _ => continue,
                }
                here = true;
            }
            n_arr += usize::from(here);
        }
        if n_arr > 0 {
            let p = items.iter().map(|(pre, _)| pre.len()).max().unwrap_or(0);
            // primero, siguientes (al menos uno), uno tras cada valor: el tope
            // y el trabajo se comprueban antes de calcular los hijos.
            let states = 1 + p.max(1) + p + 1;
            product(self, n_arr, states, false)?;
            self.cl.spend((p + 1).saturating_mul(items.len()))?;
            self.extra += counts.len();
            let mut slots = Vec::with_capacity(p + 1);
            let mut parts = Vec::with_capacity(items.len());
            for i in 0..=p {
                parts.clear();
                for (pre, rest) in &items {
                    let t = if i < pre.len() { Some(pre[i]) } else { *rest };
                    parts.push(match t {
                        Some(t) => self.cl.of(t)?,
                        None => ANY,
                    });
                }
                slots.push(self.cl.union(&parts)?);
            }
            self.upper += states;
            children.extend(&slots);
            info.arr = Some(ArrSpec { slots, counts });
        }

        // Número.
        let mut num = Vec::new();
        let mut n_num = 0;
        for &id in &comps {
            let node = ir.node(id);
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
                    Rule::MultipleOf(n) => match self.divisors.get(&(id, i)) {
                        Some(&d) => NumTest::Multiple(&n.lexeme, d),
                        None => match Divisor::of(n.lexeme.as_bytes()) {
                            Ok(d) => {
                                self.divisors.insert((id, i), d);
                                NumTest::Multiple(&n.lexeme, d)
                            }
                            Err(e) => return Err(divisor_limit(ir, id, e)),
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
        if !num.is_empty() {
            product(self, n_num, 1, true)?;
            self.upper += 1;
            self.extra += num.len();
            info.num = Some(NumSpec { checks: num });
        }

        // Cadena.
        let mut lits = Vec::new();
        let mut lens = Vec::new();
        let mut n_str = 0;
        for &id in &comps {
            let antes = lits.len() + lens.len();
            for (i, c) in ir.node(id).rules.iter().enumerate() {
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
                    _ => {}
                }
            }
            n_str += usize::from(lits.len() + lens.len() > antes);
        }
        if !lits.is_empty() || !lens.is_empty() {
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
            let states = trie_size(&words);
            product(self, n_str, states, true)?;
            self.upper += states;
            // El cierre de cada estado del trie lleva una comprobación por
            // literal que no contiene su palabra y una por longitud: como
            // mucho estados × (literales + longitudes).
            self.extra = self
                .extra
                .saturating_add(states.saturating_mul(lits.len() + lens.len()));
            self.chars.extend(words.iter().flat_map(|w| w.chars()));
            info.string = Some(StrSpec { lits, lens, words });
        }
        children.retain(|&c| c != ANY);
        Ok((info, children))
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

/// `n` como índice `u32` de una tabla del análisis, o `limit`.
fn table_index(n: usize) -> Result<u32, BuildError> {
    u32::try_from(n).map_err(|_| BuildError::Limit {
        what: "entradas de una tabla de M (índices de 32 bits)",
        limit: u32::MAX as usize,
        found: n,
        location: "#".to_owned(),
    })
}

/// Entradas del presupuesto de F4-U6 (precisado en F4.1b): las de δ
/// (|Q| · |Σ|) más las de las tablas que crecen con los productos.
fn budget(upper: usize, n_classes: usize, extra: usize) -> usize {
    upper.saturating_mul(n_classes).saturating_add(extra)
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

/// El presupuesto de F4-U6 agotado, con la ubicación del conjunto que lo
/// agotó (o la raíz, en la comprobación final).
fn delta_limit(upper: usize, n_classes: usize, extra: usize, location: String) -> BuildError {
    BuildError::Limit {
        what: "entradas de las tablas como mucho (cota de estados × clases de δ, más comprobaciones, máscaras y lexemas)",
        limit: MAX_DELTA,
        found: budget(upper, n_classes, extra),
        location,
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

/// Un trie incrustado: el estado de M de cada estado del trie, y el trie.
type TrieStates = (Vec<u32>, Dfa<Option<u32>>);

/// Estados de entrada de un conjunto (o de «cualquiera»).
#[derive(Clone, Copy)]
struct SetStates {
    obj_first: u32,
    obj_frame: u32,
    arr_first: u32,
    arr_frame: u32,
    number: u32,
    string: u32,
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
    n_classes: u32,
    /// Punto de código → clase de carácter (1..; 0 es «otro»).
    char_class: HashMap<char, u32>,
    a: Automaton,
    trans_ids: HashMap<Trans, u32>,
    rule_ids: HashMap<(NodeId, usize), u32>,
    loc_ids: HashMap<NodeId, (u32, u32)>,
    entry_ids: HashMap<(SetId, u32, u32, bool), u32>,
    /// Comprobaciones de la entrada de un valor de cada conjunto con cada
    /// clase, compartidas por todas las transiciones de entrada con
    /// retornos distintos (ronda 2 de F4.1b, I1).
    entry_checks: HashMap<(SetId, u32), (u32, u32)>,
    /// Lexema de cada cota, divisor de cada `multipleOf` y rango de cada
    /// `enum` numérico en las tablas: una vez por regla.
    lexeme_ids: HashMap<(NodeId, usize), u32>,
    divisor_ids: HashMap<(NodeId, usize), u32>,
    enum_ranges: HashMap<u32, u32>,
    /// El primer error al rellenar (un índice que no cabe en `u32`, un nombre
    /// fuera de la unión): `finish` lo devuelve.
    error: Option<BuildError>,
    any: SetStates,
    sets: Vec<Option<SetStates>>,
    objects: Vec<Option<(ObjectStates, Dfa<Option<u32>>)>>,
    arrays: Vec<Option<ArrayStates>>,
    strings: Vec<Option<TrieStates>>,
    any_object: ObjectStatesAny,
    any_array: ArrayStates,
    any_string: u32,
    any_number: u32,
    root_start: u32,
    root_done: u32,
}

impl<'a> Builder<'a> {
    fn new(ir: &'a VerifiedIr, an: Analysis<'a>) -> Result<Self, BuildError> {
        // Clases de carácter: un punto de código distinto por carácter de
        // algún nombre con bit o de alguna cadena de un `enum`/`const`
        // (F4-B3, partición común de claves y valores); los demás, la 0.
        let n_classes = class::CHAR as usize + 1 + an.chars.len();
        if budget(an.upper, n_classes, an.extra) > MAX_DELTA {
            return Err(delta_limit(
                an.upper,
                n_classes,
                an.extra,
                location_clipped(ir, NodeId::ROOT, LOCATION_MAX),
            ));
        }
        // Desde aquí |Q|, |Σ|, los estados de cada trie y las clases son
        // ≤ 2²⁰, así que sus conversiones a `u32` no truncan; las longitudes
        // de las tablas pasan por `idx`.
        let char_class: HashMap<char, u32> = an
            .chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u32 + 1))
            .collect();
        let mut class_names: Vec<String> = class::NAMES.iter().map(|s| (*s).to_owned()).collect();
        class_names.push("otro".to_owned());
        class_names.extend(an.chars.iter().map(|c| format!("'{c}'")));
        let mut ascii = vec![0u32; 128];
        let mut ranges = Vec::new();
        for (&c, &k) in &char_class {
            if (c as u32) < 128 {
                ascii[c as usize] = k;
            } else {
                ranges.push(CharRange {
                    lo: c as u32,
                    hi: c as u32,
                    class: k,
                });
            }
        }
        ranges.sort_by_key(|r| r.lo);
        let n_sets = an.cl.sets.len();
        let a = Automaton {
            n_classes: n_classes as u32,
            start: 0,
            accept: 0,
            delta: Vec::with_capacity(an.upper * n_classes),
            trans: Vec::new(),
            checks: Vec::new(),
            frames: Vec::new(),
            masks: Vec::new(),
            deps: Vec::new(),
            nums: Vec::new(),
            lexemes: Vec::new(),
            divisors: Vec::new(),
            ascii,
            ranges,
            rules: Vec::new(),
            text: Vec::new(),
            states: Vec::new(),
            groups: Vec::new(),
            class_names,
            frame_names: Vec::new(),
            products: an.products,
        };
        let empty = SetStates {
            obj_first: 0,
            obj_frame: 0,
            arr_first: 0,
            arr_frame: 0,
            number: 0,
            string: 0,
        };
        let mut b = Builder {
            ir,
            an,
            n_classes: n_classes as u32,
            char_class,
            a,
            trans_ids: HashMap::new(),
            rule_ids: HashMap::new(),
            loc_ids: HashMap::new(),
            entry_ids: HashMap::new(),
            entry_checks: HashMap::new(),
            lexeme_ids: HashMap::new(),
            divisor_ids: HashMap::new(),
            enum_ranges: HashMap::new(),
            error: None,
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
        // La transición 0 es el error de sintaxis (model::ERROR).
        b.simple(Op::Fail(Fault::Syntax), 0);
        b.reserve();
        Ok(b)
    }

    // ----------------------------------------------------- reservas ---

    /// `n` como índice `u32` de las tablas; si no cabe, guarda un `limit`
    /// (que devuelve `finish`) en vez de truncar: un índice truncado
    /// señalaría otra entrada válida y daría un veredicto incorrecto.
    fn idx(&mut self, n: usize) -> u32 {
        match u32::try_from(n) {
            Ok(v) => v,
            Err(_) => {
                if self.error.is_none() {
                    self.error = Some(BuildError::Limit {
                        what: "entradas de una tabla de M (índices de 32 bits)",
                        limit: u32::MAX as usize,
                        found: n,
                        location: "#".to_owned(),
                    });
                }
                u32::MAX
            }
        }
    }

    /// El último índice de una tabla de `len` entradas.
    fn last(&mut self, len: usize) -> u32 {
        self.idx(len.saturating_sub(1))
    }

    fn internal(&mut self, m: String) {
        if self.error.is_none() {
            self.error = Some(BuildError::Internal(m));
        }
    }

    fn group(&mut self, name: String) -> u32 {
        self.a.groups.push(name);
        self.last(self.a.groups.len())
    }

    fn state(&mut self, group: u32, name: impl Into<String>) -> u32 {
        self.a.states.push(StateInfo {
            group,
            name: name.into(),
        });
        self.a
            .delta
            .extend(std::iter::repeat_n(0, self.n_classes as usize));
        self.last(self.a.states.len())
    }

    fn frame(&mut self, words: u32, mask_words: u32, name: String) -> u32 {
        self.a.frames.push(Frame {
            words,
            mask_words,
            slots: 0,
            gates: 0,
            n_gates: 0,
        });
        self.a.frame_names.push(name);
        self.last(self.a.frames.len())
    }

    fn array_states(&mut self, g: u32, p: usize) -> ArrayStates {
        let first = self.state(g, "[ primero");
        let next = if p == 0 {
            vec![self.state(g, "[ tras ,")]
        } else {
            std::iter::once(u32::MAX)
                .chain((1..=p).map(|i| self.state(g, format!("[ valor {i}"))))
                .collect()
        };
        let after = (0..=p)
            .map(|i| self.state(g, format!("[ tras valor {i}")))
            .collect();
        ArrayStates { first, next, after }
    }

    fn classes_of(&self, w: &str) -> Vec<usize> {
        w.chars()
            .map(|c| self.char_class.get(&c).copied().unwrap_or(0) as usize)
            .collect()
    }

    /// Un trie como grupo de estados: (estado de cada estado del trie, trie).
    fn trie_states(&mut self, g: u32, words: &[&str], pre: &str, dead: &str) -> TrieStates {
        let ws: Vec<Vec<usize>> = words.iter().map(|w| self.classes_of(w)).collect();
        let trie = Dfa::trie(self.n_classes as usize - class::CHAR as usize, &ws);
        let prefixes = prefix_names(&trie, words, &ws);
        let mut states = Vec::with_capacity(trie.n_states());
        for s in 0..trie.n_states() as u32 {
            let name = if trie.is_dead(s) {
                dead.to_owned()
            } else {
                format!("{pre}«{}»", prefixes[s as usize])
            };
            states.push(self.state(g, name));
        }
        (states, trie)
    }

    /// Reserva todos los estados y marcos (sin rellenar δ).
    fn reserve(&mut self) {
        let g = self.group("raíz (Z₀)".to_owned());
        self.root_start = self.state(g, "q₀");
        self.root_done = self.state(g, "fin");
        self.a.start = self.root_start;
        self.a.accept = self.root_done;

        let g = self.group("cualquiera: objeto".to_owned());
        self.any_object = ObjectStatesAny {
            first: self.state(g, "{ primero"),
            next: self.state(g, "{ tras ,"),
            key: self.state(g, "clave"),
            colon: self.state(g, "tras clave"),
            value: self.state(g, "tras :"),
            after: self.state(g, "tras valor"),
        };
        let any_obj_frame = self.frame(0, 0, "cualquiera: objeto".to_owned());
        let g = self.group("cualquiera: arreglo".to_owned());
        self.any_array = self.array_states(g, 0);
        let any_arr_frame = self.frame(0, 0, "cualquiera: arreglo".to_owned());
        let g = self.group("cualquiera: escalares".to_owned());
        self.any_string = self.state(g, "cadena");
        self.any_number = self.state(g, "número");
        self.any = SetStates {
            obj_first: self.any_object.first,
            obj_frame: any_obj_frame,
            arr_first: self.any_array.first,
            arr_frame: any_arr_frame,
            number: self.any_number,
            string: self.any_string,
        };

        for k in 0..self.an.order.len() {
            let s = self.an.order[k];
            let Some(info) = self.an.infos[s as usize].take() else {
                continue;
            };
            let loc = label(self.ir, self.an.cl.sets.get(s));
            let mut st = self.any;
            if let Some(o) = &info.obj {
                let g = self.group(format!("objeto {loc}"));
                let first = self.state(g, "{ primero");
                let next = self.state(g, "{ tras ,");
                let (trie_states, trie) =
                    self.trie_states(g, &o.names, "clave ", "clave desconocida");
                let after_key = o
                    .names
                    .iter()
                    .map(|n| {
                        (
                            self.state(g, format!("tras «{}»", clip(n))),
                            self.state(g, format!("valor «{}»", clip(n))),
                        )
                    })
                    .collect();
                let unk_key = (
                    self.state(g, "tras clave desconocida"),
                    self.state(g, "valor desconocido"),
                );
                let after = self.state(g, "tras valor");
                let mask = o.names.len().div_ceil(64) as u32;
                let words = mask + u32::from(!o.counts.is_empty());
                st.obj_first = first;
                st.obj_frame = self.frame(words, mask, format!("objeto {loc}"));
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
                let g = self.group(format!("arreglo {loc}"));
                let states = self.array_states(g, arr.slots.len() - 1);
                st.arr_first = states.first;
                let words = u32::from(!arr.counts.is_empty());
                st.arr_frame = self.frame(words, 0, format!("arreglo {loc}"));
                self.arrays[s as usize] = Some(states);
            }
            if info.num.is_some() {
                let g = self.group(format!("número {loc}"));
                let name = if info
                    .num
                    .as_ref()
                    .is_some_and(|n| n.checks.iter().all(|c| matches!(c.2, NumTest::Integer)))
                {
                    "entero"
                } else {
                    "número"
                };
                st.number = self.state(g, name);
            }
            if let Some(sp) = &info.string {
                let g = self.group(format!("cadena {loc}"));
                let (states, trie) = self.trie_states(g, &sp.words, "", "fuera de los literales");
                st.string = states[trie.start as usize];
                self.strings[s as usize] = Some((states, trie));
            }
            self.sets[s as usize] = Some(st);
            self.an.infos[s as usize] = Some(info);
        }
    }

    // --------------------------------------------------- rellenado ---

    fn set(&mut self, q: u32, a: u32, t: u32) {
        self.a.delta[q as usize * self.n_classes as usize + a as usize] = t;
    }

    fn trans(&mut self, t: Trans) -> u32 {
        if let Some(&id) = self.trans_ids.get(&t) {
            return id;
        }
        self.a.trans.push(t);
        let id = self.last(self.a.trans.len());
        self.trans_ids.insert(t, id);
        id
    }

    fn with_checks(&mut self, op: Op, next: u32, list: Vec<Check>) -> u32 {
        let (checks, n_checks) = self.checks(list);
        self.trans(Trans {
            op,
            next,
            ret: 0,
            frame: 0,
            bit: NO_BIT,
            count: false,
            checks,
            n_checks,
            outs: OUT_IDENTITY,
        })
    }

    fn simple(&mut self, op: Op, next: u32) -> u32 {
        self.with_checks(op, next, Vec::new())
    }

    fn mv(&mut self, next: u32) -> u32 {
        self.simple(Op::Move, next)
    }

    /// Índice de la regla `i` del nodo en la tabla de reglas: la
    /// comprobación se **atribuye a su componente** (su nodo y su regla).
    fn rule(&mut self, node: NodeId, i: usize) -> u32 {
        if let Some(&r) = self.rule_ids.get(&(node, i)) {
            return r;
        }
        let (loc, loc_len) = match self.loc_ids.get(&node) {
            Some(&l) => l,
            None => {
                let s = location_clipped(self.ir, node, LOCATION_MAX);
                let l = (self.idx(self.a.text.len()), self.idx(s.len()));
                self.a.text.extend_from_slice(s.as_bytes());
                self.loc_ids.insert(node, l);
                l
            }
        };
        let kind = self.ir.node(node).rules[i].rule.kind();
        let node_ix = self.idx(node.index());
        self.a.rules.push(RuleRef {
            node: node_ix,
            kind: kind as u32,
            loc,
            loc_len,
        });
        let r = self.last(self.a.rules.len());
        self.rule_ids.insert((node, i), r);
        r
    }

    fn check(&mut self, node: NodeId, i: usize, test: Test, arg: u32, len: u32) -> Check {
        let kind = self.ir.node(node).rules[i].rule.kind();
        Check {
            test,
            rule: self.rule(node, i),
            cost: cost(kind),
            slot: 0,
            arg,
            len,
        }
    }

    fn bound(&mut self, (id, i, test, n): Bound) -> Check {
        let (arg, len) = Check::split(n);
        self.check(id, i, test, arg, len)
    }

    /// Guarda las comprobaciones ordenadas por costo (estable: a igualdad,
    /// el orden de las reglas) y devuelve (inicio, número).
    fn checks(&mut self, mut list: Vec<Check>) -> (u32, u32) {
        if list.is_empty() {
            return (0, 0);
        }
        list.sort_by_key(|c| c.cost);
        let ini = self.idx(self.a.checks.len());
        let n = self.idx(list.len());
        self.a.checks.extend(list);
        (ini, n)
    }

    /// El lexema de la regla `i` de `id` (una cota o un divisor), guardado
    /// una sola vez aunque lo lean muchos conjuntos.
    fn rule_lexeme(&mut self, id: NodeId, i: usize, l: &str) -> u32 {
        if let Some(&k) = self.lexeme_ids.get(&(id, i)) {
            return k;
        }
        let k = self.lexeme(l);
        self.lexeme_ids.insert((id, i), k);
        k
    }

    /// Guarda un lexema del esquema y devuelve su índice en `nums`.
    fn lexeme(&mut self, l: &str) -> u32 {
        let off = self.idx(self.a.lexemes.len());
        let len = self.idx(l.len());
        self.a.nums.push(Lexeme { off, len });
        self.a.lexemes.extend_from_slice(l.as_bytes());
        self.last(self.a.nums.len())
    }

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
    fn entry(&mut self, s: SetId, a: u32, ret: u32, count: bool) -> u32 {
        if let Some(&t) = self.entry_ids.get(&(s, a, ret, count)) {
            return t;
        }
        let st = self.states_of(s);
        let (checks, n_checks) = match self.entry_checks.get(&(s, a)) {
            Some(&c) => c,
            None => {
                let comps = self.an.cl.sets.get(s).to_vec();
                let mut list = Vec::new();
                let bit = class::VALUE_STARTS
                    .iter()
                    .position(|&x| x == a)
                    .unwrap_or(0);
                for id in comps {
                    let node = self.ir.node(id);
                    for (i, c) in node.rules.iter().enumerate() {
                        // La máscara ya calculada en el análisis: O(reglas)
                        // por conjunto, sin volver a recorrer los literales.
                        if self.an.entry_mask(id, i, &c.rule) & (1 << bit) != 0 {
                            list.push(self.check(id, i, Test::Fails, 0, 0));
                        }
                    }
                }
                let c = self.checks(list);
                self.entry_checks.insert((s, a), c);
                c
            }
        };
        let (op, next, frame) = match a {
            class::BEGIN_OBJECT => (Op::Push, st.obj_first, st.obj_frame),
            class::BEGIN_ARRAY => (Op::Push, st.arr_first, st.arr_frame),
            class::STRING_BEGIN => (Op::Enter, st.string, 0),
            class::NUMBER_BEGIN => (Op::Enter, st.number, 0),
            _ => (Op::Move, ret, 0),
        };
        let t = self.trans(Trans {
            op,
            next,
            ret: if op == Op::Move { 0 } else { ret },
            frame,
            bit: NO_BIT,
            count,
            checks,
            n_checks,
            outs: OUT_IDENTITY,
        });
        self.entry_ids.insert((s, a, ret, count), t);
        t
    }

    /// Las siete clases que empiezan un valor de `s`, en `q`.
    fn value(&mut self, q: u32, s: SetId, ret: u32, count: bool) {
        for a in class::VALUE_STARTS {
            let t = self.entry(s, a, ret, count);
            self.set(q, a, t);
        }
    }

    fn chars(&mut self, q: u32, to: u32) {
        let t = self.mv(to);
        for a in class::CHAR..self.n_classes {
            self.set(q, a, t);
        }
    }

    fn fill(&mut self) {
        let (s, d) = (self.root_start, self.root_done);
        self.value(s, self.an.root, d, false);

        // Objeto «cualquiera».
        let o = &self.any_object;
        let (first, next, key, colon, value, after) =
            (o.first, o.next, o.key, o.colon, o.value, o.after);
        let pop = self.simple(Op::Pop, 0);
        let to_key = self.mv(key);
        for q in [first, next] {
            self.set(q, class::STRING_BEGIN, to_key);
        }
        self.set(first, class::END_OBJECT, pop);
        self.chars(key, key);
        let t = self.mv(colon);
        self.set(key, class::STRING_END, t);
        let t = self.mv(value);
        self.set(colon, class::COLON, t);
        self.value(value, ANY, after, false);
        let t = self.mv(next);
        self.set(after, class::COMMA, t);
        self.set(after, class::END_OBJECT, pop);

        // Arreglo «cualquiera», cadena y número.
        let arr = std::mem::replace(
            &mut self.any_array,
            ArrayStates {
                first: 0,
                next: Vec::new(),
                after: Vec::new(),
            },
        );
        self.fill_array(&arr, &[ANY], &[]);
        self.any_array = arr;
        let (s, n) = (self.any_string, self.any_number);
        self.chars(s, s);
        let leave = self.simple(Op::Leave, 0);
        self.set(s, class::STRING_END, leave);
        let t = self.mv(n);
        self.set(n, class::NUMBER_BYTE, t);
        self.set(n, class::NUMBER_END, leave);

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
                    self.fill_array(&states, &arr.slots, &arr.counts);
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

    fn fill_array(&mut self, arr: &ArrayStates, slots: &[SetId], counts: &[Bound]) {
        let p = slots.len() - 1;
        let list: Vec<Check> = counts.iter().map(|&b| self.bound(b)).collect();
        let count = !counts.is_empty();
        let pop = self.with_checks(Op::Pop, 0, list);
        self.set(arr.first, class::END_ARRAY, pop);
        self.value(arr.first, slots[0], arr.after[0], count);
        for i in 0..=p {
            let n = arr.next[(i + 1).min(p)];
            let t = self.mv(n);
            self.set(arr.after[i], class::COMMA, t);
            self.set(arr.after[i], class::END_ARRAY, pop);
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
                NumTest::Integer => self.check(id, i, Test::Integer, 0, 0),
                NumTest::Cmp(test, l) => {
                    let arg = self.rule_lexeme(id, i, l);
                    self.check(id, i, test, arg, 0)
                }
                NumTest::Multiple(l, div) => {
                    let arg = match self.divisor_ids.get(&(id, i)) {
                        Some(&d) => d,
                        None => {
                            let lexeme = self.rule_lexeme(id, i, l);
                            self.a.divisors.push(DivisorRef { lexeme, div });
                            let d = self.last(self.a.divisors.len());
                            self.divisor_ids.insert((id, i), d);
                            d
                        }
                    };
                    self.check(id, i, Test::MultipleOf, arg, 0)
                }
                NumTest::Enum(k) => {
                    let n = self.an.enums[k as usize].len();
                    let arg = match self.enum_ranges.get(&k) {
                        Some(&a) => a,
                        None => {
                            let a = self.idx(self.a.nums.len());
                            for j in 0..n {
                                let l = self.an.enums[k as usize][j];
                                self.lexeme(l);
                            }
                            self.enum_ranges.insert(k, a);
                            a
                        }
                    };
                    let len = self.idx(n);
                    self.check(id, i, Test::NumEnum, arg, len)
                }
            };
            list.push(c);
        }
        let leave = self.with_checks(Op::Leave, 0, list);
        let t = self.mv(q);
        self.set(q, class::NUMBER_BYTE, t);
        self.set(q, class::NUMBER_END, leave);
    }

    fn fill_string(&mut self, s: SetId, spec: &StrSpec<'_>) {
        let Some((states, trie)) = self.strings[s as usize].take() else {
            return;
        };
        let k = trie.n_classes;
        for ts in 0..trie.n_states() as u32 {
            let q = states[ts as usize];
            for a in 0..k {
                let t = self.mv(states[trie.next(ts, a) as usize]);
                self.set(q, class::CHAR + a as u32, t);
            }
            let word = trie.out[ts as usize].map(|w| spec.words[w as usize]);
            let mut list = Vec::new();
            for &(id, i, k) in &spec.lits {
                let set = &self.an.strsets[k as usize];
                if !word.is_some_and(|w| set.contains(w)) {
                    list.push(self.check(id, i, Test::Fails, 0, 0));
                }
            }
            for &b in &spec.lens {
                list.push(self.bound(b));
            }
            let leave = self.with_checks(Op::Leave, 0, list);
            self.set(q, class::STRING_END, leave);
        }
        self.strings[s as usize] = Some((states, trie));
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
                b.internal(format!("el nombre {n:?} no está en la unión del objeto"));
                0
            }
        };
        // Máscaras de `required` y de cada lista de `dependentRequired`.
        let mask_of = |b: &mut Self, list: &[String]| -> u32 {
            let ini = b.a.masks.len();
            b.a.masks.extend(std::iter::repeat_n(0, words));
            for n in list {
                let k = bit(b, n) as usize;
                b.a.masks[ini + k / 64] |= 1 << (k % 64);
            }
            b.idx(ini)
        };
        let mut pops = Vec::new();
        for &(id, i, req) in &o.required {
            if !req.is_empty() {
                let m = mask_of(self, req);
                pops.push(self.check(id, i, Test::Mask, m, 0));
            }
        }
        for &(id, i, dep) in &o.dependent {
            let ini = self.idx(self.a.deps.len());
            for (k, l) in dep {
                if l.is_empty() {
                    continue;
                }
                let m = mask_of(self, l);
                let b = bit(self, k);
                self.a.deps.push(Dep { bit: b, mask: m });
            }
            let n = self.idx(self.a.deps.len()).saturating_sub(ini);
            if n > 0 {
                pops.push(self.check(id, i, Test::Dependent, ini, n));
            }
        }
        for &b in &o.counts {
            pops.push(self.bound(b));
        }
        let pop = self.with_checks(Op::Pop, 0, pops);
        let start = st.trie[trie.start as usize];
        let to_key = self.trans(Trans {
            op: Op::Move,
            next: start,
            ret: 0,
            frame: 0,
            bit: NO_BIT,
            count: !o.counts.is_empty(),
            checks: 0,
            n_checks: 0,
            outs: OUT_IDENTITY,
        });
        self.set(st.first, class::STRING_BEGIN, to_key);
        self.set(st.next, class::STRING_BEGIN, to_key);
        self.set(st.first, class::END_OBJECT, pop);
        let k = trie.n_classes;
        for ts in 0..trie.n_states() as u32 {
            let q = st.trie[ts as usize];
            for a in 0..k {
                let t = self.mv(st.trie[trie.next(ts, a) as usize]);
                self.set(q, class::CHAR + a as u32, t);
            }
            let end = match trie.out[ts as usize] {
                Some(j) => self.trans(Trans {
                    op: Op::Move,
                    next: st.after_key[j as usize].0,
                    ret: 0,
                    frame: 0,
                    bit: j,
                    count: false,
                    checks: 0,
                    n_checks: 0,
                    outs: OUT_IDENTITY,
                }),
                None => self.mv(st.unk_key.0),
            };
            self.set(q, class::STRING_END, end);
        }
        for (j, &child) in o.children.iter().enumerate() {
            let (k_state, v_state) = st.after_key[j];
            let t = self.mv(v_state);
            self.set(k_state, class::COLON, t);
            self.value(v_state, child, st.after, false);
        }
        let t = self.mv(st.unk_key.1);
        self.set(st.unk_key.0, class::COLON, t);
        self.value(st.unk_key.1, o.unknown, st.after, false);
        let t = self.mv(st.next);
        self.set(st.after, class::COMMA, t);
        self.set(st.after, class::END_OBJECT, pop);
        self.objects[s as usize] = Some((st, trie));
    }

    fn finish(self) -> Result<Automaton, BuildError> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(self.a),
        }
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

/// `s` recortado a 32 caracteres para los nombres de estados.
fn clip(s: &str) -> String {
    let mut out: String = s.chars().take(32).collect();
    if s.chars().nth(32).is_some() {
        out.push('…');
    }
    out
}

/// Prefijo que lee cada estado del trie (para el nombre del estado), de la
/// primera palabra que pasa por él y recortado a 32 caracteres: O(Σ |w|).
fn prefix_names(trie: &Dfa<Option<u32>>, names: &[&str], words: &[Vec<usize>]) -> Vec<String> {
    const MAX: usize = 32;
    let mut out: Vec<Option<String>> = vec![None; trie.n_states()];
    out[trie.start as usize] = Some(String::new());
    for (name, w) in names.iter().zip(words) {
        let mut q = trie.start;
        for (i, &a) in w.iter().enumerate() {
            q = trie.next(q, a);
            if out[q as usize].is_none() {
                let mut p: String = name.chars().take((i + 1).min(MAX)).collect();
                if i + 1 > MAX {
                    p.push('…');
                }
                out[q as usize] = Some(p);
            }
        }
    }
    out.into_iter().map(Option::unwrap_or_default).collect()
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
