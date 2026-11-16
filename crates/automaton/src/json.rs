//! Construcción composicional de M sobre una IR de JSON Schema
//! (`docs/fase4/automata.md` §6).
//!
//! Etapas, todas iterativas (D7, F4-B9):
//!
//! 1. Lo no implementado se rechaza antes de construir nada
//!    ([`BuildError::Pending`], F4-U5), recorriendo todo lo alcanzable desde
//!    la raíz.
//! 2. `$ref` sin hermanas es un alias (F4-U2): cada nodo se resuelve a su
//!    destino final, memorizado, o a «cualquiera» si no tiene reglas.
//! 3. Nodos alcanzables por las aristas que consumen entrada (`properties`,
//!    `items`); sus nombres de claves dan las clases de carácter (F4-B3).
//! 4. Se cuentan los estados (cota superior) y se comprueba el presupuesto
//!    **antes** de reservar δ.
//! 5. Se reservan los estados de cada subautómata y después se rellenan sus
//!    filas: los estados de un hijo ya existen cuando el padre los apila,
//!    también con recursión (`$ref` a un antepasado).

use std::collections::{BTreeSet, HashMap};

use ir::{JsonType, Node, NodeId, Rule, RuleKind, TypeSet, VerifiedIr};
use runtime_rs::tables::{
    class, CharRange, Check, Dep, Fault, Frame, Op, Rule as RuleRef, Test, Trans, NO_BIT,
    OUT_IDENTITY,
};

use crate::dfa::Dfa;
use crate::location::{location_clipped, LOCATION_MAX};
use crate::model::{Automaton, StateInfo};
use crate::weight::cost;
use crate::{BuildError, Task};

/// Presupuesto de entradas de δ (|Q| · |Σ|) por esquema (F4-U6): 2²⁰.
pub const MAX_DELTA: usize = 1 << 20;

/// Nombres con bit de máscara por objeto (`properties` ∪ `required` ∪
/// `dependentRequired`; F4-U6): 4 096, una máscara de 512 B.
pub const MAX_NAMES: usize = 4096;

/// Estados fijos: raíz (2), objeto «cualquiera» (6), arreglo «cualquiera»
/// (3), cadena y número «cualquiera» (2).
const FIXED_STATES: usize = 13;

pub(crate) fn build(ir: &VerifiedIr) -> Result<Automaton, BuildError> {
    if let ir::Format::Xml { .. } = ir.format() {
        return Err(pending(ir, NodeId::ROOT, RuleKind::Content, Task::Xml));
    }
    check_pending(ir)?;
    let resolved = resolve_aliases(ir);
    let reach = reachable(ir, &resolved);
    let mut b = Builder::new(ir, resolved, &reach)?;
    b.fill(&reach);
    Ok(b.finish())
}

// ------------------------------------------------------- 1. pendientes ---

/// La tarea que implementará la regla, o `None` si F4.1a ya la compila.
fn task_of(node: &Node, rule: &Rule) -> Option<Task> {
    match rule {
        Rule::Reject
        | Rule::Type(_)
        | Rule::Properties { .. }
        | Rule::Items { .. }
        | Rule::Required(_)
        | Rule::DependentRequired(_) => None,
        // Alias si es la única regla; con hermanas es un producto (F4.1b).
        Rule::Ref(_) if node.rules.len() == 1 => None,
        Rule::Ref(_)
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
        | Rule::MaxProperties(_) => Some(Task::Registers),
        Rule::Enum(lits) if lits.iter().all(scalar) => Some(Task::Registers),
        Rule::Const(lit) if scalar(lit) => Some(Task::Registers),
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
fn scalar(lit: &ir::Literal) -> bool {
    !matches!(
        lit.0.first(),
        Some(ir::LitToken::Array(_) | ir::LitToken::Object(_))
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
            if let Some(task) = task_of(node, &c.rule) {
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

// ----------------------------------------------------------- 2. alias ---

/// `None` = «cualquiera» (sin reglas).
type Target = Option<NodeId>;

/// Destino final de cada nodo: un nodo cuya única regla es `$ref` es un
/// alias de su destino; uno sin reglas es «cualquiera». Las cadenas de
/// `$ref` terminan (I14) y cada nodo se resuelve una vez: O(n).
fn resolve_aliases(ir: &VerifiedIr) -> Vec<Target> {
    let n = ir.nodes().len();
    let mut out: Vec<Option<Target>> = vec![None; n];
    let mut path = Vec::new();
    for v in 0..n {
        let mut cur = v;
        let fin = loop {
            if let Some(r) = out[cur] {
                break r;
            }
            let node = &ir.nodes()[cur];
            match node.rules.as_slice() {
                [] => break None,
                [c] => match c.rule {
                    Rule::Ref(t) => {
                        path.push(cur);
                        cur = t.index();
                    }
                    _ => break Some(NodeId::new(cur)),
                },
                _ => break Some(NodeId::new(cur)),
            }
        };
        out[cur] = Some(fin);
        for p in path.drain(..) {
            out[p] = Some(fin);
        }
        out[v] = Some(fin);
    }
    out.into_iter().map(Option::flatten).collect()
}

// ------------------------------------------------------- 3. alcanzables ---

/// Nodos (ya resueltos) alcanzables desde la raíz por `properties` e
/// `items`, en orden de descubrimiento.
fn reachable(ir: &VerifiedIr, resolved: &[Target]) -> Vec<NodeId> {
    let mut seen = vec![false; ir.nodes().len()];
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = resolved[0].into_iter().collect();
    for r in &stack {
        seen[r.index()] = true;
    }
    let mut buf = Vec::new();
    while let Some(id) = stack.pop() {
        out.push(id);
        for c in &ir.node(id).rules {
            if matches!(c.rule, Rule::Properties { .. } | Rule::Items { .. }) {
                buf.clear();
                c.rule.targets(&mut buf);
                for t in buf.iter().rev() {
                    if let Some(r) = resolved[t.index()] {
                        if !seen[r.index()] {
                            seen[r.index()] = true;
                            stack.push(r);
                        }
                    }
                }
            }
        }
    }
    out
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
}

fn object_rules(node: &Node) -> Option<ObjectRules<'_>> {
    let mut o = ObjectRules {
        declared: &[],
        additional: None,
        required: None,
        dependent: None,
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
                any = true;
            }
            Rule::Required(r) => {
                o.required = Some((i, r));
                any = true;
            }
            Rule::DependentRequired(d) => {
                o.dependent = Some((i, d));
                any = true;
            }
            _ => {}
        }
    }
    any.then_some(o)
}

impl ObjectRules<'_> {
    /// Nombres con bit, sin repetir: los declarados, los requeridos y los de
    /// `dependentRequired`, en ese orden.
    fn names(&self) -> Vec<&str> {
        let mut seen = std::collections::HashSet::new();
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
        declared
            .chain(required)
            .chain(dependent)
            .filter(|n| seen.insert(*n))
            .collect()
    }
}

fn items_rule(node: &Node) -> Option<(&[NodeId], Option<NodeId>)> {
    node.rules.iter().find_map(|c| match &c.rule {
        Rule::Items { prefix, rest } => Some((prefix.as_slice(), *rest)),
        _ => None,
    })
}

// ------------------------------------------------------------ builder ---

/// Estados de un nodo (o de «cualquiera»).
#[derive(Clone, Copy)]
struct NodeStates {
    obj_first: u32,
    obj_frame: u32,
    arr_first: u32,
    arr_frame: u32,
    number: u32,
}

/// Estados de un subautómata de objeto.
struct ObjectStates {
    first: u32,
    next: u32,
    /// Estado de cada estado del trie (el muerto es `unknown`).
    trie: Vec<u32>,
    unknown: u32,
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

struct Builder<'a> {
    ir: &'a VerifiedIr,
    resolved: Vec<Target>,
    n_classes: u32,
    /// Punto de código → clase de carácter (1..; 0 es «otro»).
    char_class: HashMap<char, u32>,
    a: Automaton,
    trans_ids: HashMap<Trans, u32>,
    rule_ids: HashMap<(NodeId, usize), u32>,
    loc_ids: HashMap<NodeId, (u32, u32)>,
    entry_ids: HashMap<(Target, u32, u32), u32>,
    any: NodeStates,
    nodes: HashMap<NodeId, NodeStates>,
    objects: HashMap<NodeId, (ObjectStates, Dfa<Option<u32>>)>,
    arrays: HashMap<NodeId, ArrayStates>,
    any_object: ObjectStatesAny,
    any_array: ArrayStates,
    any_string: u32,
    any_number: u32,
    root_start: u32,
    root_done: u32,
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

impl<'a> Builder<'a> {
    fn new(
        ir: &'a VerifiedIr,
        resolved: Vec<Target>,
        reach: &[NodeId],
    ) -> Result<Self, BuildError> {
        // Clases de carácter: un punto de código distinto por carácter de
        // algún nombre con bit (F4-B3); los demás, la clase 0.
        let mut chars = BTreeSet::new();
        let mut upper = FIXED_STATES;
        for &id in reach {
            let node = ir.node(id);
            if let Some(o) = object_rules(node) {
                let names = o.names();
                if names.len() > MAX_NAMES {
                    return Err(BuildError::Limit {
                        what: "nombres por objeto",
                        limit: MAX_NAMES,
                        found: names.len(),
                        location: location_clipped(ir, id, LOCATION_MAX),
                    });
                }
                let len: usize = names.iter().map(|n| n.chars().count()).sum();
                chars.extend(names.iter().flat_map(|n| n.chars()));
                // primero, siguiente, trie (≤ prefijos + muerto), 2 por
                // nombre, 2 de la clave desconocida, después.
                upper += 2 + (len + 2) + 2 * names.len() + 2 + 1;
            }
            if let Some((prefix, _)) = items_rule(node) {
                upper += 2 * prefix.len() + 3;
            }
            upper += 1; // estado de número propio
        }
        let n_classes = class::CHAR as usize + 1 + chars.len();
        if upper.saturating_mul(n_classes) > MAX_DELTA {
            return Err(BuildError::Limit {
                what: "entradas de δ (estados × clases)",
                limit: MAX_DELTA,
                found: upper.saturating_mul(n_classes),
                location: location_clipped(ir, NodeId::ROOT, LOCATION_MAX),
            });
        }
        let char_class: HashMap<char, u32> = chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u32 + 1))
            .collect();
        let mut class_names: Vec<String> = class::NAMES.iter().map(|s| (*s).to_owned()).collect();
        class_names.push("otro".to_owned());
        class_names.extend(chars.iter().map(|c| format!("'{c}'")));
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
        let a = Automaton {
            n_classes: n_classes as u32,
            start: 0,
            accept: 0,
            delta: Vec::with_capacity(upper * n_classes),
            trans: Vec::new(),
            checks: Vec::new(),
            frames: Vec::new(),
            masks: Vec::new(),
            deps: Vec::new(),
            ascii,
            ranges,
            rules: Vec::new(),
            text: Vec::new(),
            states: Vec::new(),
            groups: Vec::new(),
            class_names,
            frame_names: Vec::new(),
        };
        let mut b = Builder {
            ir,
            resolved,
            n_classes: n_classes as u32,
            char_class,
            a,
            trans_ids: HashMap::new(),
            rule_ids: HashMap::new(),
            loc_ids: HashMap::new(),
            entry_ids: HashMap::new(),
            any: NodeStates {
                obj_first: 0,
                obj_frame: 0,
                arr_first: 0,
                arr_frame: 0,
                number: 0,
            },
            nodes: HashMap::new(),
            objects: HashMap::new(),
            arrays: HashMap::new(),
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
        b.trans(Trans {
            op: Op::Fail(Fault::Syntax),
            next: 0,
            ret: 0,
            frame: 0,
            bit: NO_BIT,
            checks: 0,
            n_checks: 0,
            outs: OUT_IDENTITY,
        });
        b.reserve(reach);
        Ok(b)
    }

    // ----------------------------------------------------- reservas ---

    fn group(&mut self, name: String) -> u32 {
        self.a.groups.push(name);
        self.a.groups.len() as u32 - 1
    }

    fn state(&mut self, group: u32, name: impl Into<String>) -> u32 {
        self.a.states.push(StateInfo {
            group,
            name: name.into(),
        });
        self.a
            .delta
            .extend(std::iter::repeat_n(0, self.n_classes as usize));
        self.a.states.len() as u32 - 1
    }

    fn frame(&mut self, words: u32, name: String) -> u32 {
        self.a.frames.push(Frame {
            words,
            slots: 0,
            gates: 0,
            n_gates: 0,
        });
        self.a.frame_names.push(name);
        self.a.frames.len() as u32 - 1
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

    /// Reserva todos los estados y marcos (sin rellenar δ).
    fn reserve(&mut self, reach: &[NodeId]) {
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
        let any_obj_frame = self.frame(0, "cualquiera: objeto".to_owned());
        let g = self.group("cualquiera: arreglo".to_owned());
        self.any_array = self.array_states(g, 0);
        let any_arr_frame = self.frame(0, "cualquiera: arreglo".to_owned());
        let g = self.group("cualquiera: escalares".to_owned());
        self.any_string = self.state(g, "cadena");
        self.any_number = self.state(g, "número");
        self.any = NodeStates {
            obj_first: self.any_object.first,
            obj_frame: any_obj_frame,
            arr_first: self.any_array.first,
            arr_frame: any_arr_frame,
            number: self.any_number,
        };

        for &id in reach {
            let node = self.ir.node(id);
            let loc = location_clipped(self.ir, id, LOCATION_MAX);
            let mut st = self.any;
            if let Some(o) = object_rules(node) {
                let names = o.names();
                let words: Vec<Vec<usize>> = names
                    .iter()
                    .map(|n| n.chars().map(|c| self.char_class[&c] as usize).collect())
                    .collect();
                let trie = Dfa::trie(self.n_classes as usize - class::CHAR as usize, &words);
                let g = self.group(format!("objeto {loc}"));
                let first = self.state(g, "{ primero");
                let next = self.state(g, "{ tras ,");
                let prefixes = prefix_names(&trie, &names, &words);
                let mut trie_states = Vec::with_capacity(trie.n_states());
                let mut unknown = u32::MAX;
                for s in 0..trie.n_states() as u32 {
                    if trie.is_dead(s) {
                        unknown = self.state(g, "clave desconocida");
                        trie_states.push(unknown);
                    } else {
                        let prefix = &prefixes[s as usize];
                        trie_states.push(self.state(g, format!("clave «{prefix}»")));
                    }
                }
                let after_key = names
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
                let words_n = names.len().div_ceil(64) as u32;
                st.obj_first = first;
                st.obj_frame = self.frame(words_n, format!("objeto {loc}"));
                self.objects.insert(
                    id,
                    (
                        ObjectStates {
                            first,
                            next,
                            trie: trie_states,
                            unknown,
                            after_key,
                            unk_key,
                            after,
                        },
                        trie,
                    ),
                );
            }
            if let Some((prefix, _)) = items_rule(node) {
                let g = self.group(format!("arreglo {loc}"));
                let arr = self.array_states(g, prefix.len());
                st.arr_first = arr.first;
                st.arr_frame = self.frame(0, format!("arreglo {loc}"));
                self.arrays.insert(id, arr);
            }
            if integer_only(node) {
                let g = self.group(format!("número {loc}"));
                st.number = self.state(g, "entero");
            }
            self.nodes.insert(id, st);
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
        let id = self.a.trans.len() as u32 - 1;
        self.trans_ids.insert(t, id);
        id
    }

    fn simple(&mut self, op: Op, next: u32) -> u32 {
        self.trans(Trans {
            op,
            next,
            ret: 0,
            frame: 0,
            bit: NO_BIT,
            checks: 0,
            n_checks: 0,
            outs: OUT_IDENTITY,
        })
    }

    fn mv(&mut self, next: u32) -> u32 {
        self.simple(Op::Move, next)
    }

    /// Índice de la regla `i` del nodo en la tabla de reglas.
    fn rule(&mut self, node: NodeId, i: usize) -> u32 {
        if let Some(&r) = self.rule_ids.get(&(node, i)) {
            return r;
        }
        let (loc, loc_len) = match self.loc_ids.get(&node) {
            Some(&l) => l,
            None => {
                let s = location_clipped(self.ir, node, LOCATION_MAX);
                let l = (self.a.text.len() as u32, s.len() as u32);
                self.a.text.extend_from_slice(s.as_bytes());
                self.loc_ids.insert(node, l);
                l
            }
        };
        let kind = self.ir.node(node).rules[i].rule.kind();
        self.a.rules.push(RuleRef {
            node: node.index() as u32,
            kind: kind as u32,
            loc,
            loc_len,
        });
        let r = self.a.rules.len() as u32 - 1;
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

    /// Guarda las comprobaciones ordenadas por costo (estable: a igualdad,
    /// el orden de las reglas) y devuelve (inicio, número).
    fn checks(&mut self, mut list: Vec<Check>) -> (u32, u32) {
        if list.is_empty() {
            return (0, 0);
        }
        list.sort_by_key(|c| c.cost);
        let ini = self.a.checks.len() as u32;
        let n = list.len() as u32;
        self.a.checks.extend(list);
        (ini, n)
    }

    /// Transición compuesta que empieza un valor de `target` con la clase
    /// `a` y vuelve a `ret` al terminarlo (`automata.md` §6.1): las
    /// comprobaciones del primer evento (`type`, `false`) y la operación
    /// que abre la instancia (apilar un marco o abrir un escalar).
    fn entry(&mut self, target: Target, a: u32, ret: u32) -> u32 {
        if let Some(&t) = self.entry_ids.get(&(target, a, ret)) {
            return t;
        }
        let st = target.map_or(self.any, |id| self.nodes[&id]);
        let mut list = Vec::new();
        if let Some(id) = target {
            let node = self.ir.node(id);
            for (i, c) in node.rules.iter().enumerate() {
                let falla = match &c.rule {
                    Rule::Reject => true,
                    Rule::Type(set) => !admits(*set, a),
                    _ => false,
                };
                if falla {
                    list.push(self.check(id, i, Test::Fails, 0, 0));
                }
            }
        }
        let (checks, n_checks) = self.checks(list);
        let (op, next, frame) = match a {
            class::BEGIN_OBJECT => (Op::Push, st.obj_first, st.obj_frame),
            class::BEGIN_ARRAY => (Op::Push, st.arr_first, st.arr_frame),
            class::STRING_BEGIN => (Op::Enter, self.any_string, 0),
            class::NUMBER_BEGIN => (Op::Enter, st.number, 0),
            _ => (Op::Move, ret, 0),
        };
        let t = self.trans(Trans {
            op,
            next,
            ret: if op == Op::Move { 0 } else { ret },
            frame,
            bit: NO_BIT,
            checks,
            n_checks,
            outs: OUT_IDENTITY,
        });
        self.entry_ids.insert((target, a, ret), t);
        t
    }

    /// Las siete clases que empiezan un valor de `target`, en `q`.
    fn value(&mut self, q: u32, target: Target, ret: u32) {
        for a in class::VALUE_STARTS {
            let t = self.entry(target, a, ret);
            self.set(q, a, t);
        }
    }

    fn chars(&mut self, q: u32, to: u32) {
        let t = self.mv(to);
        for a in class::CHAR..self.n_classes {
            self.set(q, a, t);
        }
    }

    fn resolve(&self, t: Option<NodeId>) -> Target {
        t.and_then(|t| self.resolved[t.index()])
    }

    fn fill(&mut self, reach: &[NodeId]) {
        let root = self.resolved[0];
        let (s, d) = (self.root_start, self.root_done);
        self.value(s, root, d);

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
        self.value(value, None, after);
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
        self.fill_array(&arr, &[], None);
        self.any_array = arr;
        let (s, n) = (self.any_string, self.any_number);
        self.chars(s, s);
        let leave = self.simple(Op::Leave, 0);
        self.set(s, class::STRING_END, leave);
        let t = self.mv(n);
        self.set(n, class::NUMBER_BYTE, t);
        self.set(n, class::NUMBER_END, leave);

        for &id in reach {
            self.fill_node(id);
        }
    }

    fn fill_array(&mut self, arr: &ArrayStates, prefix: &[NodeId], rest: Option<NodeId>) {
        let p = prefix.len();
        let pop = self.simple(Op::Pop, 0);
        let target = |b: &Self, i: usize| {
            if i < p {
                b.resolve(Some(prefix[i]))
            } else {
                b.resolve(rest)
            }
        };
        self.set(arr.first, class::END_ARRAY, pop);
        let t0 = target(self, 0);
        self.value(arr.first, t0, arr.after[0]);
        for i in 0..=p {
            let n = arr.next[(i + 1).min(p)];
            let t = self.mv(n);
            self.set(arr.after[i], class::COMMA, t);
            self.set(arr.after[i], class::END_ARRAY, pop);
        }
        for i in (if p == 0 { 0 } else { 1 })..=p {
            let ti = target(self, i);
            self.value(arr.next[i], ti, arr.after[i]);
        }
    }

    fn fill_node(&mut self, id: NodeId) {
        let node = self.ir.node(id);
        if let Some((prefix, rest)) = items_rule(node) {
            let arr = self.arrays.remove(&id).expect("reservado");
            self.fill_array(&arr, prefix, rest);
            self.arrays.insert(id, arr);
        }
        if integer_only(node) {
            let q = self.nodes[&id].number;
            let i = node
                .rules
                .iter()
                .position(|c| c.rule.kind() == RuleKind::Type)
                .unwrap_or(0);
            let c = self.check(id, i, Test::Integer, 0, 0);
            let (checks, n_checks) = self.checks(vec![c]);
            let leave = self.trans(Trans {
                op: Op::Leave,
                next: 0,
                ret: 0,
                frame: 0,
                bit: NO_BIT,
                checks,
                n_checks,
                outs: OUT_IDENTITY,
            });
            let t = self.mv(q);
            self.set(q, class::NUMBER_BYTE, t);
            self.set(q, class::NUMBER_END, leave);
        }
        if let Some(o) = object_rules(node) {
            self.fill_object(id, &o);
        }
    }

    fn fill_object(&mut self, id: NodeId, o: &ObjectRules<'_>) {
        let (st, trie) = self.objects.remove(&id).expect("reservado");
        let names = o.names();
        let words = names.len().div_ceil(64);
        // Búsquedas en O(1): con 4 096 nombres, `position` sería cuadrático.
        let bits: HashMap<&str, u32> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (*n, i as u32))
            .collect();
        let declared: HashMap<&str, NodeId> =
            o.declared.iter().map(|(n, t)| (n.as_str(), *t)).collect();
        let bit = |n: &str| bits.get(n).copied().unwrap_or(0);
        // Máscaras de `required` y de cada lista de `dependentRequired`.
        let mask_of = |b: &mut Self, list: &[&str]| -> u32 {
            let ini = b.a.masks.len();
            b.a.masks.extend(std::iter::repeat_n(0, words));
            for n in list {
                let k = bit(n) as usize;
                b.a.masks[ini + k / 64] |= 1 << (k % 64);
            }
            ini as u32
        };
        let mut pops = Vec::new();
        if let Some((i, req)) = o.required {
            if !req.is_empty() {
                let list: Vec<&str> = req.iter().map(String::as_str).collect();
                let m = mask_of(self, &list);
                pops.push(self.check(id, i, Test::Mask, m, 0));
            }
        }
        if let Some((i, dep)) = o.dependent {
            let ini = self.a.deps.len() as u32;
            for (k, l) in dep {
                if l.is_empty() {
                    continue;
                }
                let list: Vec<&str> = l.iter().map(String::as_str).collect();
                let m = mask_of(self, &list);
                self.a.deps.push(Dep {
                    bit: bit(k),
                    mask: m,
                });
            }
            let n = self.a.deps.len() as u32 - ini;
            if n > 0 {
                pops.push(self.check(id, i, Test::Dependent, ini, n));
            }
        }
        let (checks, n_checks) = self.checks(pops);
        let pop = self.trans(Trans {
            op: Op::Pop,
            next: 0,
            ret: 0,
            frame: 0,
            bit: NO_BIT,
            checks,
            n_checks,
            outs: OUT_IDENTITY,
        });
        let start = st.trie[trie.start as usize];
        let to_key = self.mv(start);
        self.set(st.first, class::STRING_BEGIN, to_key);
        self.set(st.next, class::STRING_BEGIN, to_key);
        self.set(st.first, class::END_OBJECT, pop);
        let k = trie.n_classes;
        for s in 0..trie.n_states() as u32 {
            let q = st.trie[s as usize];
            for a in 0..k {
                let t = self.mv(st.trie[trie.next(s, a) as usize]);
                self.set(q, class::CHAR + a as u32, t);
            }
            let end = match trie.out[s as usize] {
                Some(j) => self.trans(Trans {
                    op: Op::Move,
                    next: st.after_key[j as usize].0,
                    ret: 0,
                    frame: 0,
                    bit: j,
                    checks: 0,
                    n_checks: 0,
                    outs: OUT_IDENTITY,
                }),
                None => self.mv(st.unk_key.0),
            };
            self.set(q, class::STRING_END, end);
        }
        let unknown_target = self.resolve(o.additional);
        for (j, n) in names.iter().enumerate() {
            let (k_state, v_state) = st.after_key[j];
            let t = self.mv(v_state);
            self.set(k_state, class::COLON, t);
            let target = match declared.get(n) {
                Some(t) => self.resolve(Some(*t)),
                None => unknown_target,
            };
            self.value(v_state, target, st.after);
        }
        let t = self.mv(st.unk_key.1);
        self.set(st.unk_key.0, class::COLON, t);
        self.value(st.unk_key.1, unknown_target, st.after);
        let t = self.mv(st.next);
        self.set(st.after, class::COMMA, t);
        self.set(st.after, class::END_OBJECT, pop);
        debug_assert!(st.unknown != u32::MAX);
        self.objects.insert(id, (st, trie));
    }

    fn finish(self) -> Automaton {
        self.a
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

/// `type` admite `integer` pero no `number`: el número se comprueba al
/// cerrarlo con los registros del número.
fn integer_only(node: &Node) -> bool {
    node.rules.iter().any(|c| {
        matches!(c.rule, Rule::Type(set) if set.contains(JsonType::Integer) && !set.contains(JsonType::Number))
    })
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
