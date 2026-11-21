//! Construcción composicional de M sobre una IR de XSD (F4.1c;
//! `docs/fase4/automata.md` §10-§15).
//!
//! Etapas, todas iterativas (D7), como en JSON ([`crate::json`]):
//!
//! 1. Lo no implementado (`pattern`, F4.4b) se rechaza antes de construir.
//! 2. Análisis con lista de trabajo desde el documento: un **marco por
//!    (tipo de elemento, nombre local)**; de cada tipo, sus atributos (con el
//!    texto de cada uno), su contenido (vacío, simple con su texto, o el
//!    autómata de Glushkov con contadores de su modelo, o la máscara de
//!    `xs:all`) y los hijos que apila. Los textos se memorizan por (conjunto
//!    canónico de sus nodos por `Ref`, `fixed`), los modelos por nodo y los
//!    `enum` por (nodo, regla). El presupuesto de F4-U6 (el mismo que en
//!    JSON, §22.5 de `docs/fase4/pendientes.md`) y el de pasos se comprueban
//!    aquí, **antes** de reservar.
//! 3. Se reservan los estados de cada marco, texto y parte fija (documento,
//!    elemento saltado, instrucciones de procesamiento) y después se
//!    rellenan sus filas.

/// Si XML ya compila `xs:pattern` (F4.4b); mientras no, `Pending`.
pub(crate) const PATTERNS: bool = true;

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use ir::{Lexical, LitToken, Literal, NodeId, Rule, RuleKind, Term, VerifiedIr};
use runtime_rs::tables::{class, xclass, Check, CounterOp, Interval, Op, Test, Trans, NO_BIT};

use crate::conj::{Closures, SetId, MAX_CLOSURE_WORK};
use crate::dfa::Dfa;
use crate::emit::{
    budget, clip, delta_limit, plain, table_index, trie_size, Bound, Emitter, TrieStates,
};
use crate::glushkov::{self, Close, Guard, Work};
use crate::location::{location_clipped, LOCATION_MAX};
use crate::model::{Automaton, PayloadFormat};
use crate::text::{
    decl_dfa, lexical_chars, lexical_size, overflow, rounding, text_dfa, Rounding, TextOut,
    DECL_CHARS, NO_WORD, WS,
};
use crate::{BuildError, BuildOptions};

pub(crate) fn build(ir: &VerifiedIr, opts: &BuildOptions) -> Result<Automaton, BuildError> {
    crate::json::check_pending(ir)?;
    let an = Analysis::run(ir, opts)?;
    let mut b = Builder::new(ir, an, opts)?;
    b.fill();
    let mut a = b.t.finish()?;
    a.format = PayloadFormat::Xml;
    Ok(a)
}

// ----------------------------------------------------------- análisis ---

/// Guarda de una transición entre hijos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XGuard {
    Count(Guard),
    /// El bit de un hijo de `xs:all` no está (no se repite).
    Unset(u32),
}

/// Una transición entre hijos: el hijo `name` de tipo `ty` lleva al estado
/// `to`, con sus guardas y sus operaciones de contador (y el bit de `xs:all`).
#[derive(Clone, Debug)]
struct KEdge<'a> {
    name: &'a str,
    ty: NodeId,
    to: u32,
    guards: Vec<XGuard>,
    ops: Vec<CounterOp>,
    mark: Option<u32>,
}

/// Qué pasa al cerrar el elemento en un estado de su contenido.
#[derive(Clone, Debug)]
enum KClose {
    /// No es final: una violación del modelo.
    No,
    /// Final con las guardas de los mínimos.
    Guards(Vec<Guard>),
    /// `xs:all`: los bits de los hijos requeridos.
    Mask(Vec<u32>),
}

#[derive(Clone, Debug)]
struct KState<'a> {
    edges: Vec<KEdge<'a>>,
    close: KClose,
}

/// El contenido de un tipo con `Content`: estados entre hijos (el 0 es el
/// inicial).
#[derive(Debug)]
struct ModelSpec<'a> {
    rule: usize,
    states: Vec<KState<'a>>,
    counters: u32,
    /// Estados del trie de los nombres de cada estado entre hijos, contados
    /// una vez por modelo (ronda extra, I6: `elem` y `frame_cost` los leen
    /// de aquí); sus bytes ya se cargaron al presupuesto de pasos.
    tries: Vec<usize>,
}

/// Estados del trie de los nombres de las aristas de cada estado.
fn state_tries(states: &[KState<'_>]) -> Vec<usize> {
    states
        .iter()
        .map(|k| {
            let ns: Vec<&str> = k.edges.iter().map(|e| e.name).collect();
            trie_size(&ns)
        })
        .collect()
}

#[derive(Debug)]
enum Content<'a> {
    /// Sin `Content` ni `SimpleContent`: sin hijos ni texto.
    Empty,
    /// `SimpleContent`: el texto `text`.
    Simple {
        rule: usize,
        text: u32,
    },
    Model(ModelSpec<'a>),
}

/// Un tipo de elemento.
#[derive(Debug)]
struct ElemSpec<'a> {
    node: NodeId,
    attr_rule: Option<usize>,
    /// Atributos declarados (el índice es su bit) con el texto de su valor.
    attrs: Vec<(&'a str, u32)>,
    required: Vec<u32>,
    content: Content<'a>,
    mask_words: u32,
    counters: u32,
    /// Estados del contenido en cada marco del tipo (vacío 3; simple, el AFD
    /// del texto + 2; modelo, Σ por estado entre hijos de 2 + el trie de sus
    /// nombres), calculado una vez por tipo.
    content_states: usize,
}

/// Un `fixed` en el espacio de valores del tipo (5.6-P3).
#[derive(Clone, Debug)]
enum Fixed<'a> {
    Str(&'a str),
    Bool(bool),
    /// `xs:integer`/`xs:decimal`: el lexema reescrito.
    Num(String),
    /// `xs:float`/`xs:double` finito: su intervalo de redondeo.
    Float(Rounding),
    /// `INF` (false) o `-INF` (true), también un desborde (`1e39`).
    Inf(bool),
    NaN,
}

/// Un texto (valor de atributo o contenido simple): el cierre de su nodo
/// por `Ref` (una restricción es la conjunción con su base) y, en un
/// atributo, su `fixed`.
#[derive(Debug)]
struct TextSpec<'a> {
    lexical: Lexical,
    /// La regla del espacio léxico a la que se atribuye un texto que no es
    /// del tipo: una sola violación aunque la repitan todos los nodos de la
    /// cadena de restricciones (I13 obliga a que sea la misma).
    lexical_rule: (NodeId, usize),
    /// Las palabras del trie (`xs:string`: los literales de `enum` y
    /// `fixed`): índice en `Analysis::wordsets`, compartido por todos los
    /// textos con la misma lista de reglas `enum` y el mismo `fixed` (F4D-S1).
    words: u32,
    /// `enum` de cadenas: (nodo, regla, índice en `strsets`).
    lits: Vec<(NodeId, usize, u32)>,
    lens: Vec<Bound>,
    /// Cotas: (nodo, regla, clase, lexema).
    ranges: Vec<(NodeId, usize, RuleKind, &'a str)>,
    /// `enum` numéricos: (nodo, regla, índice en `enums` o `fenums`).
    nenums: Vec<(NodeId, usize, u32)>,
    /// (dueño, regla `Attributes`, valor).
    fixed: Option<(NodeId, usize, Fixed<'a>)>,
    size: usize,
    /// Comprobaciones que emite el cierre del texto en un estado (como
    /// mucho): las del presupuesto (§15 de `automata.md`).
    per_state: usize,
    /// `xs:pattern` de la cadena de restricciones (F4.4b): (nodo, regla);
    /// todas deben cumplirse (las de un mismo paso ya van unidas con `|`).
    pats: Vec<(NodeId, usize)>,
}

struct Analysis<'a> {
    cl: Closures<'a>,
    /// El presupuesto de F4-U6 (`BuildOptions::max_delta`).
    max_delta: usize,
    /// El tope de estados de un producto o de un AFD (F4-U16).
    max_product: usize,
    /// El AFD de cada `xs:pattern`, una vez por regla (F4.4b).
    patterns: HashMap<(NodeId, usize), crate::pattern::PatternDfa>,
    /// Trabajo de todos los `xs:pattern` construidos (E3B-2) y su tope.
    pattern_work: usize,
    max_pattern_work: usize,
    /// Los conjuntos de caracteres de los `pattern` (partición global).
    sets: Vec<ir::regex::CharSet>,
    chars: BTreeSet<char>,
    upper: usize,
    extra: usize,
    work: Work,
    texts: Vec<TextSpec<'a>>,
    text_ids: HashMap<TextKey, u32>,
    /// Textos que leen valores de atributo (tienen su grupo global).
    attr_texts: Vec<u32>,
    attr_seen: HashSet<u32>,
    elems: Vec<ElemSpec<'a>>,
    elem_ids: HashMap<NodeId, u32>,
    /// Marcos: (tipo, índice en `elems`, nombre local).
    frames: Vec<(u32, &'a str)>,
    frame_ids: HashMap<(NodeId, &'a str), u32>,
    /// El modelo del documento (los elementos globales).
    doc: ModelSpec<'a>,
    /// `enum` de `xs:integer`/`xs:decimal` ordenados por valor, una vez por
    /// regla.
    enums: Vec<Vec<&'a str>>,
    enum_ids: HashMap<(NodeId, usize), u32>,
    /// `enum` de `xs:float`/`xs:double` como intervalos ordenados.
    fenums: Vec<Vec<Rounding>>,
    fenum_ids: HashMap<(NodeId, usize), u32>,
    strsets: Vec<HashSet<&'a str>>,
    str_ids: HashMap<(NodeId, usize), u32>,
    /// Palabras de los textos de `xs:string` (la unión ordenada de sus
    /// `enum` y su `fixed`) con el tamaño de su trie, una vez por (lista de
    /// reglas `enum`, `fixed`); la 0 es la vacía.
    wordsets: Vec<WordSet<'a>>,
    word_ids: HashMap<WordKey<'a>, u32>,
    /// Intervalo de redondeo de cada cota de `xs:float`/`xs:double`, una vez
    /// por (nodo, regla) (I4): el `Builder` saca de aquí los umbrales.
    float_bounds: HashMap<(NodeId, usize), Rounding>,
    /// Pares de follow de los Glushkov ya construidos (F4D-M2): el tope se
    /// aplica a la suma de todos los modelos, no a cada uno.
    pairs: usize,
    /// Comprobaciones de un marco de cada tipo (`frame_cost`), una vez.
    frame_costs: Vec<Option<usize>>,
}

/// Palabras de un texto de `xs:string` y los estados de su trie.
#[derive(Debug)]
struct WordSet<'a> {
    words: Vec<&'a str>,
    size: usize,
}

/// Clave de un [`WordSet`]: las reglas `enum` (nodo, regla), en el orden del
/// cierre, y el `fixed` de cadena.
type WordKey<'a> = (Vec<(NodeId, usize)>, Option<&'a str>);

/// Clave de un texto: el cierre de su nodo y, si tiene `fixed`, el
/// atributo (dueño, regla `Attributes`, índice).
type TextKey = (SetId, Option<(NodeId, usize, usize)>);

/// Pasos que se cargan al presupuesto de construcción por cada intervalo de
/// redondeo IEEE (`text::rounding`: dos decimales exactos de a lo sumo ≈ 770
/// cifras, cada uno un producto de enteros grandes de ≈ 80 palabras por
/// 5^1075).
const ROUNDING_STEPS: usize = 2048;

/// Pasos del presupuesto de los cierres que pesa cada regla de un componente
/// del cierre de un texto (F4D-M1): cada una puede dar una entrada de
/// `TextSpec` (cota, longitud o `enum`: 16-40 B) frente a los 4 B de un
/// `NodeId` del cierre, que es lo que mide ese presupuesto. Con 2²⁴ pasos,
/// como mucho 2²² reglas recorridas en todos los textos.
const FACET_WEIGHT: usize = 4;

/// El nombre que no es atributo: las declaraciones de espacio de nombres.
const XMLNS: &str = "xmlns";

/// Estados fijos: documento (3), elemento saltado (5), texto cualquiera
/// (1) e instrucciones de procesamiento (6 + 4).
const FIXED_STATES: usize = 19;

fn internal(m: String) -> BuildError {
    BuildError::Internal(m)
}

impl<'a> Analysis<'a> {
    fn run(ir: &'a VerifiedIr, opts: &BuildOptions) -> Result<Analysis<'a>, BuildError> {
        let mut chars: BTreeSet<char> = WS.iter().copied().collect();
        chars.insert(':');
        chars.extend(DECL_CHARS.chars());
        chars.extend(XMLNS.chars());
        let decl_size = {
            let v: Vec<char> = chars.iter().copied().collect();
            let class_of = |c: char| v.iter().position(|&x| x == c).map_or(0, |i| i + 1);
            decl_dfa(v.len() + 1, &class_of).n_states()
        };
        let mut an = Analysis {
            cl: Closures::new(ir.nodes())?,
            max_delta: opts.max_delta,
            max_product: opts.max_product_states,
            patterns: HashMap::new(),
            pattern_work: 0,
            max_pattern_work: opts.max_pattern_work,
            sets: Vec::new(),
            chars,
            upper: FIXED_STATES + decl_size,
            extra: 0,
            work: Work {
                used: 0,
                limit: MAX_CLOSURE_WORK,
            },
            texts: Vec::new(),
            text_ids: HashMap::new(),
            attr_texts: Vec::new(),
            attr_seen: HashSet::new(),
            elems: Vec::new(),
            elem_ids: HashMap::new(),
            frames: Vec::new(),
            frame_ids: HashMap::new(),
            doc: ModelSpec {
                rule: 0,
                states: Vec::new(),
                counters: 0,
                tries: Vec::new(),
            },
            enums: Vec::new(),
            enum_ids: HashMap::new(),
            fenums: Vec::new(),
            fenum_ids: HashMap::new(),
            strsets: Vec::new(),
            str_ids: HashMap::new(),
            wordsets: vec![WordSet {
                words: Vec::new(),
                size: 1,
            }],
            word_ids: HashMap::new(),
            float_bounds: HashMap::new(),
            pairs: 0,
            frame_costs: Vec::new(),
        };
        // El documento: `Content` con la elección de los globales (D16).
        let root = ir.node(NodeId::ROOT);
        let Some((rule, model)) = root
            .rules
            .iter()
            .enumerate()
            .find_map(|(i, c)| match &c.rule {
                Rule::Content(m) => Some((i, m)),
                _ => None,
            })
        else {
            return Err(internal(
                "el documento no tiene modelo de contenido (I6)".to_owned(),
            ));
        };
        let doc = an.model(ir, NodeId::ROOT, rule, model, 0, opts)?;
        if doc.counters > 0
            || doc.states.iter().skip(1).any(|k| !k.edges.is_empty())
            || doc
                .states
                .iter()
                .any(|k| k.edges.iter().any(|e| e.mark.is_some()))
        {
            return Err(internal(
                "el documento admite más de un elemento raíz (no es una elección de globales, D16)"
                    .to_owned(),
            ));
        }
        let names: Vec<&str> = doc.states[0].edges.iter().map(|e| e.name).collect();
        let doc_trie = doc.tries[0];
        an.upper = an.upper.saturating_add(doc_trie);
        // I5 (ronda 3): cada estado del trie de los globales emite una
        // comprobación, el `Fails` del elemento raíz desconocido
        // (`Builder::name_ends` → `skip_entry`) o las de la arista.
        an.extra = an.extra.saturating_add(doc_trie);
        an.chars.extend(names.iter().flat_map(|n| n.chars()));
        an.check_budget(ir, NodeId::ROOT)?;
        let mut stack: Vec<(NodeId, &'a str)> = doc.states[0]
            .edges
            .iter()
            .rev()
            .map(|e| (e.ty, e.name))
            .collect();
        // Por arista del documento: lo que emite `child_entry` (requeridos y
        // comprobaciones del hijo vacío, ×3 transiciones como mucho).
        let tys: Vec<NodeId> = doc.states[0].edges.iter().map(|e| e.ty).collect();
        an.doc = doc;
        // C2 (ronda 3): comprobado tras cada global, con la granularidad de
        // JSON (un conjunto, `json.rs`).
        for ty in tys {
            let c = an.elem(ir, ty, opts)?;
            let entry = an.entry_cost(c);
            an.extra = an.extra.saturating_add(entry);
            an.check_budget(ir, ty)?;
        }
        while let Some((ty, name)) = stack.pop() {
            if an.frame_ids.contains_key(&(ty, name)) {
                continue;
            }
            let e = an.elem(ir, ty, opts)?;
            let id = table_index(an.frames.len())?;
            an.frames.push((e, name));
            an.frame_ids.insert((ty, name), id);
            let spec = &an.elems[e as usize];
            // tag, nombres de atributo (con `xmlns`), `xmlns:…`, contenido y
            // el nombre de cierre.
            let mut attr_names: Vec<&str> = spec.attrs.iter().map(|a| a.0).collect();
            attr_names.push(XMLNS);
            let states = trie_size(&attr_names)
                .saturating_add(spec.content_states)
                .saturating_add(name.chars().count())
                .saturating_add(4);
            an.upper = an.upper.saturating_add(states);
            an.chars.extend(name.chars());
            if let Content::Model(m) = &spec.content {
                for k in m.states.iter().rev() {
                    for e in k.edges.iter().rev() {
                        stack.push((e.ty, e.name));
                    }
                }
            }
            // C1 (ronda 2): las comprobaciones que emite el marco, contadas
            // antes de reservar con la regla de F4-U6 (D53 (5), §22.5).
            let cost = an.frame_cost(ir, e, opts)?;
            an.extra = an.extra.saturating_add(cost);
            an.check_budget(ir, ty)?;
        }
        Ok(an)
    }

    /// El presupuesto de F4-U6 con las clases vistas hasta ahora (|Σ| solo
    /// crece, así que es una cota inferior de la cuenta final).
    fn check_budget(&self, ir: &VerifiedIr, at: NodeId) -> Result<(), BuildError> {
        let k = class::CHAR as usize
            + 1
            + self.chars.len()
            + self.sets.iter().map(Vec::len).sum::<usize>();
        if budget(self.upper, k, self.extra) > self.max_delta {
            return Err(delta_limit(
                self.upper,
                k,
                self.extra,
                self.max_delta,
                location_clipped(ir, at, LOCATION_MAX),
            ));
        }
        Ok(())
    }

    /// Comprobaciones del elemento `e` vacío (`<e/>` o `<e></e>`): los
    /// requeridos, el modelo en su estado inicial o el texto vacío.
    fn empty_cost(&self, e: u32) -> usize {
        let spec = &self.elems[e as usize];
        usize::from(!spec.required.is_empty()).saturating_add(match &spec.content {
            Content::Empty => 0,
            Content::Simple { text, .. } => self.texts[*text as usize].per_state,
            Content::Model(m) => usize::from(matches!(m.states[0].close, KClose::No)),
        })
    }

    /// Lo que emite una arista hacia el hijo `e` además de sus guardas: los
    /// requeridos en `>` y en `/>`, y las comprobaciones del hijo vacío en
    /// `/>` (`Builder::child_entry`).
    fn entry_cost(&self, e: u32) -> usize {
        usize::from(!self.elems[e as usize].required.is_empty()).saturating_add(self.empty_cost(e))
    }

    /// Las comprobaciones (y palabras de máscaras) que emite un marco del
    /// tipo `e` (`Builder::fill_frame`): la etiqueta, el contenido simple
    /// en cada estado de su AFD o, por estado entre hijos, el cierre, los
    /// nombres inesperados y cada arista (3 · guardas + lo de
    /// [`Analysis::entry_cost`]). Los tipos de los hijos se analizan aquí
    /// (una vez por nodo), con el presupuesto comprobado tras cada uno (C2).
    /// El resultado se memoriza por tipo: los marcos (tipo, nombre) del mismo
    /// tipo emiten lo mismo.
    fn frame_cost(
        &mut self,
        ir: &'a VerifiedIr,
        e: u32,
        opts: &BuildOptions,
    ) -> Result<usize, BuildError> {
        if let Some(Some(c)) = self.frame_costs.get(e as usize) {
            return Ok(*c);
        }
        let spec = &self.elems[e as usize];
        let req = usize::from(!spec.required.is_empty());
        let mw = spec.mask_words as usize;
        let mut cost = (2 * req)
            .saturating_mul(1usize.saturating_add(mw))
            .saturating_add(self.empty_cost(e))
            .saturating_add(1);
        let mut edges: Vec<(NodeId, usize)> = Vec::new();
        match &spec.content {
            Content::Empty => cost = cost.saturating_add(2),
            Content::Simple { text, .. } => {
                let t = &self.texts[*text as usize];
                cost = cost
                    .saturating_add(t.size.saturating_mul(t.per_state))
                    .saturating_add(1);
            }
            Content::Model(m) => {
                for (k, &trie) in m.states.iter().zip(&m.tries) {
                    // El texto no permitido y un `Fails` por estado del trie
                    // de los nombres sin arista (las aristas, abajo).
                    cost = cost.saturating_add(1).saturating_add(trie);
                    cost = cost.saturating_add(match &k.close {
                        KClose::No => 1,
                        KClose::Guards(g) => g.len(),
                        KClose::Mask(_) => mw.saturating_add(1),
                    });
                    edges.extend(k.edges.iter().map(|x| (x.ty, x.guards.len())));
                }
            }
        }
        for (ty, guards) in edges {
            let c = self.elem(ir, ty, opts)?;
            cost = cost
                .saturating_add(guards.saturating_mul(3))
                .saturating_add(self.entry_cost(c));
            // C2: lo que añade cada hijo nuevo se comprueba antes del
            // siguiente.
            self.check_budget(ir, ty)?;
        }
        if self.frame_costs.len() <= e as usize {
            self.frame_costs.resize(e as usize + 1, None);
        }
        self.frame_costs[e as usize] = Some(cost);
        Ok(cost)
    }

    /// El tipo de elemento `node`, una vez por nodo.
    fn elem(
        &mut self,
        ir: &'a VerifiedIr,
        node: NodeId,
        opts: &BuildOptions,
    ) -> Result<u32, BuildError> {
        if let Some(&e) = self.elem_ids.get(&node) {
            return Ok(e);
        }
        let n = ir.node(node);
        let mut attr_rule = None;
        let mut attrs = Vec::new();
        let mut required = Vec::new();
        let mut content = Content::Empty;
        let mut model = None;
        for (i, c) in n.rules.iter().enumerate() {
            match &c.rule {
                Rule::Attributes(list) => {
                    attr_rule = Some(i);
                    for (j, a) in list.iter().enumerate() {
                        let fixed = a.fixed.as_ref().map(|f| (node, i, j, f.as_str()));
                        // Sus comprobaciones (estados × c(t)) las cuenta
                        // `text` al crearlo (C2 de la ronda 3); aquí, los
                        // estados de su grupo global, una vez.
                        let t = self.text(ir, a.ty, fixed)?;
                        if self.attr_seen.insert(t) {
                            self.attr_texts.push(t);
                            let size = self.texts[t as usize].size;
                            self.upper = self.upper.saturating_add(size);
                        }
                        if a.required {
                            required.push(j as u32);
                        }
                        attrs.push((a.name.as_str(), t));
                        self.chars.extend(a.name.chars());
                        self.check_budget(ir, node)?;
                    }
                }
                Rule::SimpleContent(t) => {
                    let text = self.text(ir, *t, None)?;
                    content = Content::Simple { rule: i, text };
                }
                Rule::Content(m) => model = Some((i, m)),
                _ => {
                    return Err(internal(format!(
                        "regla {} en un elemento (I3)",
                        c.rule.kind().name()
                    )))
                }
            }
        }
        let n_attrs = attrs.len() as u32;
        let mut all_bits = 0u32;
        if let Some((i, m)) = model {
            let spec = self.model(ir, node, i, m, n_attrs, opts)?;
            self.check_budget(ir, node)?;
            all_bits = spec
                .states
                .iter()
                .flat_map(|k| k.edges.iter().filter_map(|e| e.mark))
                .map(|b| b + 1 - n_attrs)
                .max()
                .unwrap_or(0);
            content = Content::Model(spec);
        }
        let mask_words = (n_attrs + all_bits).div_ceil(64);
        let counters = match &content {
            Content::Model(m) => m.counters,
            _ => 0,
        };
        // Comprobaciones: máscara de requeridos (y de `xs:all`), una por
        // atributo desconocido, las de cada transición entre hijos (y sus
        // operaciones de contador, que se guardan una vez por lista).
        let mut extra = (2 * mask_words as usize)
            .saturating_add(4)
            .saturating_add(attrs.len());
        let content_states = match &content {
            Content::Empty => 3,
            Content::Simple { text, .. } => self.texts[*text as usize].size.saturating_add(2),
            Content::Model(m) => {
                let mut states = 0usize;
                for (k, &trie) in m.states.iter().zip(&m.tries) {
                    extra = extra.saturating_add(2);
                    for e in &k.edges {
                        extra = extra
                            .saturating_add(e.guards.len())
                            .saturating_add(e.ops.len())
                            .saturating_add(4);
                    }
                    if let KClose::Guards(g) = &k.close {
                        extra = extra.saturating_add(g.len());
                    }
                    states = states.saturating_add(2).saturating_add(trie);
                }
                states
            }
        };
        self.extra = self.extra.saturating_add(extra);
        let id = table_index(self.elems.len())?;
        self.elems.push(ElemSpec {
            node,
            attr_rule,
            attrs,
            required,
            content,
            mask_words,
            counters,
            content_states,
        });
        self.elem_ids.insert(node, id);
        // C2: el modelo y los textos de este tipo, comprobados antes de
        // seguir con el siguiente.
        self.check_budget(ir, node)?;
        Ok(id)
    }

    /// Los estados entre hijos de un modelo: el autómata de Glushkov con
    /// contadores, o la máscara de `xs:all` (bits desde `bit0`).
    fn model(
        &mut self,
        ir: &'a VerifiedIr,
        node: NodeId,
        rule: usize,
        m: &'a ir::ContentModel,
        bit0: u32,
        opts: &BuildOptions,
    ) -> Result<ModelSpec<'a>, BuildError> {
        let ctx = || location_clipped(ir, node, LOCATION_MAX);
        if let Some(Term::All(children)) = m.particles.first().map(|p| &p.term) {
            // `xs:all` (I12: raíz, hijos elementos con `maxOccurs` ≤ 1): un
            // bit por hijo; «aún ninguno» (0) y «alguno» (1).
            let root = m.particles[0].occurs;
            let mut edges = Vec::new();
            let mut req = Vec::new();
            for (b, &c) in children.iter().enumerate() {
                let p = &m.particles[c];
                let Term::Element { name, ty } = &p.term else {
                    return Err(internal(format!(
                        "hijo de xs:all que no es elemento en {}",
                        ctx()
                    )));
                };
                if p.occurs.max == Some(0) || root.max == Some(0) {
                    continue;
                }
                let bit = bit0 + b as u32;
                if p.occurs.min >= 1 {
                    req.push(bit);
                }
                edges.push(KEdge {
                    name,
                    ty: *ty,
                    to: 1,
                    guards: vec![XGuard::Unset(bit)],
                    ops: Vec::new(),
                    mark: Some(bit),
                });
                self.chars.extend(name.chars());
            }
            // Los nombres se ordenan aquí y se cuentan en el trie de los dos
            // estados: sus bytes, al presupuesto de pasos antes (ronda extra).
            let bytes = edges
                .iter()
                .fold(0usize, |n, e| n.saturating_add(e.name.len()));
            self.work.spend(bytes.saturating_mul(3), &ctx)?;
            let mut names: Vec<&str> = edges.iter().map(|e| e.name).collect();
            names.sort_unstable();
            if names.windows(2).any(|w| w[0] == w[1]) {
                return Err(internal(format!(
                    "hijos repetidos en xs:all de {} (UPA)",
                    ctx()
                )));
            }
            let first = if root.min == 0 || req.is_empty() {
                KClose::Guards(Vec::new())
            } else {
                KClose::No
            };
            let rest = if req.is_empty() {
                KClose::Guards(Vec::new())
            } else {
                KClose::Mask(req)
            };
            let states = vec![
                KState {
                    edges: edges.clone(),
                    close: first,
                },
                KState { edges, close: rest },
            ];
            let tries = state_tries(&states);
            return Ok(ModelSpec {
                rule,
                states,
                counters: 0,
                tries,
            });
        }
        let max_pairs = opts.max_delta / (class::CHAR as usize + 1);
        // Cada transición del autómata de posiciones da al menos un estado
        // de trie (sus nombres son distintos por la UPA), de |Σ| > `CHAR`
        // entradas: si los pares pasan de `max_pairs`, el presupuesto de
        // F4-U6 está agotado, y se dice en sus términos. F4D-M2 (ronda 3):
        // la cuenta es la de **todos** los modelos, así que a este le queda
        // lo que no gastaron los anteriores, comprobado dentro de
        // `glushkov::build` antes de materializar sus pares (y aquí, antes de
        // sus `KEdge`).
        let before = self.pairs;
        let rest = max_pairs.saturating_sub(before);
        let g = glushkov::build(m, &mut self.work, rest, &ctx).map_err(|e| match e {
            BuildError::Limit { what, found, .. } if what == glushkov::PAIRS_WHAT => delta_limit(
                before.saturating_add(found),
                class::CHAR as usize + 1,
                0,
                opts.max_delta,
                ctx(),
            ),
            e => e,
        })?;
        self.pairs = before.saturating_add(g.pairs);
        let mut states = Vec::with_capacity(g.edges.len());
        for (k, list) in g.edges.iter().enumerate() {
            let edges = list
                .iter()
                .map(|e| {
                    let p = g.positions[e.to as usize];
                    KEdge {
                        name: p.name,
                        ty: p.ty,
                        to: e.to + 1,
                        guards: e.guards.iter().map(|&g| XGuard::Count(g)).collect(),
                        ops: e.ops.clone(),
                        mark: None,
                    }
                })
                .collect();
            let close = match &g.close[k] {
                Close::No => KClose::No,
                Close::Guards(v) => KClose::Guards(v.clone()),
            };
            states.push(KState { edges, close });
        }
        for p in &g.positions {
            self.chars.extend(p.name.chars());
        }
        let tries = state_tries(&states);
        Ok(ModelSpec {
            rule,
            states,
            counters: g.counters,
            tries,
        })
    }

    /// El texto del nodo `node` (con el `fixed` del atributo `(dueño, regla,
    /// índice, valor)`), una vez por (cierre, atributo con `fixed`).
    ///
    /// Ronda 3 (C2, F4D-S1, F4D-M1, I4): todo lo proporcional al esquema se
    /// carga antes de hacerlo: cada regla de cada componente del cierre al
    /// presupuesto de los cierres (`FACET_WEIGHT`), cada umbral IEEE a los
    /// pasos (una vez por regla), las palabras de los `enum` de cadena a los
    /// pasos y a `extra` antes de unirlas (una vez por lista de reglas), y al
    /// final las comprobaciones que emite el texto (estados × c(t)), con el
    /// presupuesto comprobado antes de volver.
    /// El AFD de `xs:pattern` de la regla `i` de `id`, una vez por regla
    /// (F4.4b; dialecto XSD, anclado): más de `max_product` estados es
    /// `limit` en su ubicación, contado antes de crearlos.
    fn pattern(
        &mut self,
        ir: &VerifiedIr,
        id: NodeId,
        i: usize,
        src: &str,
    ) -> Result<(), BuildError> {
        if self.patterns.contains_key(&(id, i)) {
            return Ok(());
        }
        match crate::pattern::compile(src, ir::regex::Dialect::Xsd, self.max_product) {
            Ok(p) => {
                self.pattern_work = self.pattern_work.saturating_add(p.work);
                if self.pattern_work > self.max_pattern_work {
                    return Err(BuildError::Limit {
                        what: "trabajo de los `pattern` del esquema",
                        limit: self.max_pattern_work,
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
                limit: self.max_product,
                found: n,
                location: location_clipped(ir, id, LOCATION_MAX),
            }),
            Err(crate::pattern::PatternError::Invalid(e)) => Err(internal(format!(
                "`xs:pattern` que el frontal aceptó y `ir::regex` no: {e}"
            ))),
        }
    }

    fn text(
        &mut self,
        ir: &'a VerifiedIr,
        node: NodeId,
        fixed: Option<(NodeId, usize, usize, &'a str)>,
    ) -> Result<u32, BuildError> {
        let set = self.cl.of(node)?;
        let key = (set, fixed.map(|(o, r, j, _)| (o, r, j)));
        if let Some(&t) = self.text_ids.get(&key) {
            return Ok(t);
        }
        let ctx = || location_clipped(ir, node, LOCATION_MAX);
        let mut lexical = None;
        let mut spec = TextSpec {
            lexical: Lexical::String,
            lexical_rule: (node, 0),
            words: 0,
            lits: Vec::new(),
            lens: Vec::new(),
            ranges: Vec::new(),
            nenums: Vec::new(),
            fixed: None,
            size: 0,
            per_state: 0,
            pats: Vec::new(),
        };
        for k in 0..self.cl.sets.get(set).len() {
            let id = self.cl.sets.get(set)[k];
            let rules = &ir.node(id).rules;
            // F4D-M1: cada regla puede dar una entrada del texto; se carga
            // antes de recorrerlas.
            self.cl
                .spend(FACET_WEIGHT.saturating_mul(rules.len().max(1)))
                .map_err(|e| match e {
                    BuildError::Limit {
                        what, limit, found, ..
                    } => BuildError::Limit {
                        what,
                        limit,
                        found,
                        location: ctx(),
                    },
                    e => e,
                })?;
            for (i, c) in rules.iter().enumerate() {
                match &c.rule {
                    Rule::Lexical(l) => match lexical {
                        None => {
                            lexical = Some(*l);
                            spec.lexical_rule = (id, i);
                        }
                        Some(x) if x == *l => {}
                        Some(_) => {
                            return Err(internal(format!(
                                "espacios léxicos distintos en la restricción de {} (I13)",
                                ctx()
                            )))
                        }
                    },
                    Rule::MinLength(n) => spec.lens.push((id, i, Test::LengthMin, *n)),
                    Rule::MaxLength(n) => spec.lens.push((id, i, Test::LengthMax, *n)),
                    Rule::Minimum(n)
                    | Rule::Maximum(n)
                    | Rule::ExclusiveMinimum(n)
                    | Rule::ExclusiveMaximum(n) => {
                        spec.ranges.push((id, i, c.rule.kind(), n.lexeme.as_str()));
                    }
                    Rule::Enum(_) => spec.lits.push((id, i, 0)),
                    Rule::Pattern(src) => {
                        self.pattern(ir, id, i, src)?;
                        spec.pats.push((id, i));
                    }
                    Rule::Ref(_) => {}
                    _ => {
                        return Err(internal(format!(
                            "regla {} en un texto (I3)",
                            c.rule.kind().name()
                        )))
                    }
                }
            }
        }
        let Some(lexical) = lexical else {
            return Err(internal(format!(
                "texto sin espacio léxico en {} (I13)",
                ctx()
            )));
        };
        spec.lexical = lexical;
        // I4: el umbral IEEE de cada cota de `xs:float`/`xs:double`, una vez
        // por (nodo, regla) y cargado a los pasos antes de calcularlo; el
        // `Builder` lo lee de `float_bounds`.
        if matches!(lexical, Lexical::Float | Lexical::Double) {
            let double = lexical == Lexical::Double;
            for &(id, i, _, lex) in &spec.ranges {
                if self.float_bounds.contains_key(&(id, i)) {
                    continue;
                }
                self.work
                    .spend(ROUNDING_STEPS, &|| location_clipped(ir, id, LOCATION_MAX))?;
                let r = rounding(lex, double).ok_or_else(|| {
                    internal(format!(
                        "cota {lex:?} no finita en xs:float/xs:double (D24)"
                    ))
                })?;
                self.float_bounds.insert((id, i), r);
            }
        }
        // Los `enum`, por tipo: cadenas al trie, números ordenados por valor.
        let enums = std::mem::take(&mut spec.lits);
        for (id, i, _) in enums {
            let Rule::Enum(lits) = &ir.node(id).rules[i].rule else {
                continue;
            };
            match lexical {
                Lexical::String => {
                    let k = self.strings(id, i, lits)?;
                    spec.lits.push((id, i, k));
                }
                Lexical::Integer | Lexical::Decimal => {
                    let k = self.numbers(ir, id, i, lits)?;
                    spec.nenums.push((id, i, k));
                }
                Lexical::Float | Lexical::Double => {
                    let k = self.floats(ir, id, i, lits, lexical == Lexical::Double)?;
                    spec.nenums.push((id, i, k));
                }
                Lexical::Boolean => {
                    return Err(internal("xs:enumeration en xs:boolean (I13)".to_owned()));
                }
            }
        }
        if let Some((owner, r, _, value)) = fixed {
            if matches!(lexical, Lexical::Float | Lexical::Double) {
                // El intervalo de redondeo del `fixed` (F4C-S1).
                self.work.spend(ROUNDING_STEPS, &|| {
                    location_clipped(ir, owner, LOCATION_MAX)
                })?;
            }
            let f = fixed_value(lexical, value).ok_or_else(|| {
                internal(format!(
                    "fixed {value:?} fuera del tipo en {} (lo comprueba el frontal, 5.6)",
                    location_clipped(ir, owner, LOCATION_MAX)
                ))
            })?;
            spec.fixed = Some((owner, r, f));
        }
        if lexical == Lexical::String {
            spec.words = self.words(ir, node, &spec)?;
            spec.size = self.wordsets[spec.words as usize].size;
        } else {
            self.chars.extend(lexical_chars(lexical).chars());
            spec.size = lexical_size(lexical);
        }
        // Con `pattern`, el AFD del texto es el producto con el de cada uno
        // (con *collapse* delante, ≤ 2·|p| + 1, si el tipo no es
        // `xs:string`), y el constructor lo corta en `max_product`: esa es
        // la cota (F4.4b).
        if spec.pats.len() > 64 {
            return Err(BuildError::Limit {
                what: "`xs:pattern` sobre el mismo texto",
                limit: 64,
                found: spec.pats.len(),
                location: ctx(),
            });
        }
        if !spec.pats.is_empty() {
            let mut size = spec.size;
            for k in &spec.pats {
                let p = self.patterns[k].dfa.n_states();
                let p = if lexical == Lexical::String {
                    p
                } else {
                    2 * p + 1
                };
                size = size.saturating_mul(p);
            }
            spec.size = size.min(self.max_product);
        }
        let per_state = spec.lits.len()
            + spec.lens.len()
            + spec.ranges.len()
            + spec.nenums.len()
            + spec.pats.len()
            + usize::from(spec.fixed.is_some())
            + 1;
        spec.per_state = per_state;
        // Los lexemas de las cotas y del `fixed` (una vez por texto) y las
        // comprobaciones del cierre del texto en cada estado de su AFD
        // (C2, ronda 3): las de un texto de atributo se emiten una vez, en
        // su grupo global; las de un contenido simple, además, en cada marco
        // (`frame_cost`), así que ahí la cuenta es conservadora.
        self.extra = self
            .extra
            .saturating_add(spec.ranges.len().saturating_mul(2))
            .saturating_add(2)
            .saturating_add(spec.size.saturating_mul(per_state));
        let t = table_index(self.texts.len())?;
        self.texts.push(spec);
        self.text_ids.insert(key, t);
        self.check_budget(ir, node)?;
        Ok(t)
    }

    /// Las palabras del trie de un texto de `xs:string` (la unión de sus
    /// `enum` en orden y su `fixed`), una vez por (lista de reglas `enum`,
    /// `fixed`): F4D-S1 de la ronda 3. Antes de unirlas, las palabras que se
    /// van a recorrer se cargan a los pasos y a `extra` (cada una da al
    /// menos un estado del trie con una comprobación por literal) y se
    /// comprueba el presupuesto.
    fn words(
        &mut self,
        ir: &'a VerifiedIr,
        node: NodeId,
        spec: &TextSpec<'a>,
    ) -> Result<u32, BuildError> {
        let fixed = match &spec.fixed {
            Some((_, _, Fixed::Str(w))) => Some(*w),
            _ => None,
        };
        let key: WordKey<'a> = (spec.lits.iter().map(|&(id, i, _)| (id, i)).collect(), fixed);
        if let Some(&k) = self.word_ids.get(&key) {
            return Ok(k);
        }
        if key.0.is_empty() && fixed.is_none() {
            return Ok(0);
        }
        let n = spec
            .lits
            .iter()
            .fold(usize::from(fixed.is_some()), |n, &(_, _, k)| {
                n.saturating_add(self.strsets[k as usize].len())
            });
        self.work
            .spend(n, &|| location_clipped(ir, node, LOCATION_MAX))?;
        self.extra = self.extra.saturating_add(n);
        self.check_budget(ir, node)?;
        let mut words: Vec<&'a str> = Vec::new();
        let mut seen = HashSet::new();
        for &(_, _, k) in &spec.lits {
            let mut v: Vec<&'a str> = self.strsets[k as usize].iter().copied().collect();
            v.sort_unstable();
            for w in v {
                #[cfg(test)]
                tests::PALABRAS.with(|c| c.set(c.get() + 1));
                if seen.insert(w) {
                    words.push(w);
                }
            }
        }
        if let Some(w) = fixed {
            if seen.insert(w) {
                words.push(w);
            }
        }
        self.chars.extend(words.iter().flat_map(|w| w.chars()));
        let size = if words.is_empty() {
            1
        } else {
            trie_size(&words)
        };
        let k = table_index(self.wordsets.len())?;
        self.wordsets.push(WordSet { words, size });
        self.word_ids.insert(key, k);
        Ok(k)
    }

    /// Los números de un `enum` de `xs:integer`/`xs:decimal`, ordenados por
    /// valor y sin repetir (5.6-CR-S6: los repetidos por valor se absorben),
    /// una vez por regla.
    fn numbers(
        &mut self,
        ir: &VerifiedIr,
        id: NodeId,
        i: usize,
        lits: &'a [Literal],
    ) -> Result<u32, BuildError> {
        if let Some(&k) = self.enum_ids.get(&(id, i)) {
            return Ok(k);
        }
        // Un lexema por valor en las tablas, contado antes de ordenar.
        self.extra = self.extra.saturating_add(lits.len());
        self.check_budget(ir, id)?;
        let mut v: Vec<&'a str> = lits
            .iter()
            .filter_map(|l| match l.0.first() {
                Some(LitToken::Number(n)) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        v.sort_by(|a, b| ir::num::compare(a, b));
        v.dedup_by(|a, b| ir::num::compare(a, b) == Ordering::Equal);
        self.enums.push(v);
        let k = table_index(self.enums.len() - 1)?;
        self.enum_ids.insert((id, i), k);
        Ok(k)
    }

    /// Los intervalos de redondeo de un `enum` de `xs:float`/`xs:double`,
    /// ordenados y sin repetir, una vez por regla.
    fn floats(
        &mut self,
        ir: &VerifiedIr,
        id: NodeId,
        i: usize,
        lits: &'a [Literal],
        double: bool,
    ) -> Result<u32, BuildError> {
        if let Some(&k) = self.fenum_ids.get(&(id, i)) {
            return Ok(k);
        }
        // F4C-S1 (ronda 2): cada intervalo son dos decimales exactos de hasta
        // ≈ 770 cifras (un producto de enteros grandes por valor): se cuentan
        // antes del bucle, en las tablas (un intervalo y dos lexemas por
        // valor) y en el presupuesto de pasos.
        self.extra = self.extra.saturating_add(lits.len().saturating_mul(3));
        self.check_budget(ir, id)?;
        self.work
            .spend(ROUNDING_STEPS.saturating_mul(lits.len()), &|| {
                location_clipped(ir, id, LOCATION_MAX)
            })?;
        let mut v: Vec<Rounding> = Vec::new();
        for l in lits {
            let Some(LitToken::Number(n)) = l.0.first() else {
                continue;
            };
            v.push(rounding(n, double).ok_or_else(|| {
                internal(format!(
                    "enumeración {n:?} no finita en xs:float/xs:double (D24)"
                ))
            })?);
        }
        v.sort_by(|a, b| ir::num::compare(&a.lo, &b.lo));
        v.dedup();
        self.fenums.push(v);
        let k = table_index(self.fenums.len() - 1)?;
        self.fenum_ids.insert((id, i), k);
        Ok(k)
    }

    fn strings(&mut self, id: NodeId, i: usize, lits: &'a [Literal]) -> Result<u32, BuildError> {
        if let Some(&k) = self.str_ids.get(&(id, i)) {
            return Ok(k);
        }
        let v: HashSet<&'a str> = lits
            .iter()
            .filter_map(|l| match l.0.first() {
                Some(LitToken::String(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        self.strsets.push(v);
        let k = table_index(self.strsets.len() - 1)?;
        self.str_ids.insert((id, i), k);
        Ok(k)
    }
}

/// El `fixed` de un atributo en el espacio de valores de su tipo (el frontal
/// ya comprobó que es un valor del tipo, 5.6; aquí se lee con `collapse` en
/// los tipos que no son `xs:string`).
fn fixed_value(l: Lexical, v: &str) -> Option<Fixed<'_>> {
    let trimmed = v.trim_matches(|c| WS.contains(&c));
    Some(match l {
        Lexical::String => Fixed::Str(v),
        Lexical::Boolean => Fixed::Bool(ir::num::parse_boolean(v).ok()?),
        Lexical::Integer | Lexical::Decimal => Fixed::Num(ir::num::xsd_number(l, v).ok()?.lexeme),
        Lexical::Float | Lexical::Double => match trimmed {
            "INF" => Fixed::Inf(false),
            "-INF" => Fixed::Inf(true),
            "NaN" => Fixed::NaN,
            _ => match ir::num::xsd_number(l, v) {
                Ok(n) => Fixed::Float(rounding(&n.lexeme, l == Lexical::Double)?),
                Err(e) if e.kind == ir::num::NumErrorKind::Overflow => {
                    Fixed::Inf(trimmed.starts_with('-'))
                }
                Err(_) => return None,
            },
        },
    })
}

// ------------------------------------------------------------ builder ---

/// Los estados de un marco (tipo, nombre).
struct FrameStates {
    tag: u32,
    attr: TrieStates,
    /// Tras `xmlns:` (una declaración de espacio de nombres con prefijo).
    xmlns: u32,
    content: ContentStates,
    end: TrieStates,
}

enum ContentStates {
    /// (sin texto, con texto, nombre de un hijo inesperado).
    Empty { c: u32, t: u32, u: u32 },
    /// Los estados del AFD del texto, el nombre de un hijo inesperado y
    /// «texto tras un hijo inesperado».
    Simple { states: Vec<u32>, u: u32, z: u32 },
    /// Por estado entre hijos: (sin texto, texto no permitido, trie de los
    /// nombres de sus hijos).
    Model(Vec<(u32, u32, TrieStates)>),
}

impl ContentStates {
    fn start(&self) -> u32 {
        match self {
            ContentStates::Empty { c, .. } => *c,
            ContentStates::Simple { states, .. } => states[0],
            ContentStates::Model(v) => v[0].0,
        }
    }
}

struct Builder<'a> {
    ir: &'a VerifiedIr,
    an: Analysis<'a>,
    t: Emitter<'a>,
    /// AFD de cada texto (ya con las clases de carácter).
    dfas: Vec<Dfa<TextOut>>,
    /// Por texto y estado de su AFD, los `pattern` que se cumplen (bit j =
    /// el j-ésimo de `TextSpec::pats`; F4.4b).
    pbits: Vec<Vec<u64>>,
    /// Grupo global de los textos de atributo: estado de cada estado del AFD.
    attr_states: HashMap<u32, Vec<u32>>,
    frames: Vec<FrameStates>,
    /// Por marco: (tipo de marco, inicio de los atributos, inicio del
    /// contenido), lo que necesita el padre para apilarlo.
    entries: Vec<(u32, u32, u32)>,
    enum_ranges: HashMap<u32, u32>,
    fenum_ranges: HashMap<u32, u32>,
    fixed_ids: HashMap<u32, u32>,
    /// Comprobaciones del cierre de cada texto por salida del AFD, una vez.
    text_memo: HashMap<(u32, TextOut, u64), Vec<Check>>,
    overflow: [Option<u32>; 4],
    // Partes fijas.
    d0: u32,
    d1: u32,
    fin: u32,
    doc_trie: TrieStates,
    skip_frame: u32,
    sk_tag: u32,
    sk_attr: u32,
    sk_content: u32,
    sk_child: u32,
    sk_end: u32,
    any_text: u32,
    /// Instrucciones: p0 (inicio del nombre), p1-p3 (`x`, `xm`, `xml` en
    /// cualquier caja), nombre cualquiera, contenido.
    pi: [u32; 6],
    /// Desde el comienzo del documento: q0-q3 (`x`, `xm`, `xml` en
    /// minúsculas) y el AFD de la declaración.
    decl: [u32; 4],
    decl_states: Vec<u32>,
    decl_dfa: Dfa<TextOut>,
}

impl<'a> Builder<'a> {
    fn new(ir: &'a VerifiedIr, an: Analysis<'a>, opts: &BuildOptions) -> Result<Self, BuildError> {
        // |Σ| exacto con los conjuntos de los `xs:pattern` (F4.4b).
        let n_classes = {
            let singles: Vec<u32> = an.chars.iter().map(|&c| u32::from(c)).collect();
            let refs: Vec<&ir::regex::CharSet> = an.sets.iter().collect();
            class::CHAR as usize + crate::pattern::Partition::refine(&singles, &refs).n_classes
        };
        if budget(an.upper, n_classes, an.extra) > opts.max_delta {
            return Err(delta_limit(
                an.upper,
                n_classes,
                an.extra,
                opts.max_delta,
                location_clipped(ir, NodeId::ROOT, LOCATION_MAX),
            ));
        }
        let mut t = Emitter::new(ir, &an.chars, &an.sets, &xclass::NAMES, an.upper, 0);
        let n = t.n_chars();
        let class_of = |c: char| t.class_of(c);
        let crudos: Vec<Dfa<TextOut>> = an
            .texts
            .iter()
            .map(|s| {
                let words: Vec<Vec<usize>> = an.wordsets[s.words as usize]
                    .words
                    .iter()
                    .map(|w| t.classes_of(w))
                    .collect();
                text_dfa(s.lexical, &words, n, &class_of)
            })
            .collect();
        let decl = decl_dfa(n, &class_of);
        // F4.3: cada parte finita se minimiza antes de volcarla a δ (el
        // presupuesto ya se contó con los tamaños sin minimizar, que lo
        // acotan).
        let mut dfas = Vec::with_capacity(crudos.len());
        let mut pbits = Vec::with_capacity(crudos.len());
        for (tx, d) in crudos.into_iter().enumerate() {
            let spec = &an.texts[tx];
            if spec.pats.is_empty() {
                pbits.push(vec![u64::MAX; 0]);
                dfas.push(t.minimized(d));
                continue;
            }
            let (d, bits) = with_patterns(&mut t, &an, spec, d, opts.max_product_states)?;
            pbits.push(bits);
            dfas.push(d);
        }
        let decl = t.minimized(decl);
        let empty: TrieStates = (Vec::new(), Dfa::trie(1, &[]));
        let mut b = Builder {
            ir,
            an,
            t,
            dfas,
            pbits,
            attr_states: HashMap::new(),
            frames: Vec::new(),
            entries: Vec::new(),
            enum_ranges: HashMap::new(),
            fenum_ranges: HashMap::new(),
            fixed_ids: HashMap::new(),
            text_memo: HashMap::new(),
            overflow: [None; 4],
            d0: 0,
            d1: 0,
            fin: 0,
            doc_trie: empty,
            skip_frame: 0,
            sk_tag: 0,
            sk_attr: 0,
            sk_content: 0,
            sk_child: 0,
            sk_end: 0,
            any_text: 0,
            pi: [0; 6],
            decl: [0; 4],
            decl_states: Vec::new(),
            decl_dfa: decl,
        };
        b.reserve();
        Ok(b)
    }

    // ----------------------------------------------------- reservas ---

    fn reserve(&mut self) {
        let g = self.t.group("documento (Z₀)".to_owned());
        self.d0 = self.t.state(g, "inicio");
        self.d1 = self.t.state(g, "prólogo");
        self.fin = self.t.state(g, "fin");
        self.t.a.start = self.d0;
        self.t.a.accept = self.fin;
        let names: Vec<&str> = self.an.doc.states[0].edges.iter().map(|e| e.name).collect();
        self.doc_trie = self.t.trie_states(g, &names, "<", "< desconocido");

        let g = self.t.group("elemento saltado".to_owned());
        self.sk_tag = self.t.state(g, "en la etiqueta");
        self.sk_attr = self.t.state(g, "atributo");
        self.sk_content = self.t.state(g, "contenido");
        self.sk_child = self.t.state(g, "< hijo");
        self.sk_end = self.t.state(g, "</");
        self.skip_frame = self.t.frame(0, 0, "elemento saltado".to_owned());
        let g = self.t.group("texto cualquiera".to_owned());
        self.any_text = self.t.state(g, "texto");

        let g = self.t.group("instrucción de procesamiento".to_owned());
        let names = ["<?", "<?x", "<?xm", "<?xml", "<?nombre", "<?… contenido"];
        for (k, n) in names.iter().enumerate() {
            self.pi[k] = self.t.state(g, *n);
        }
        let g = self.t.group("declaración <?xml …?>".to_owned());
        for (k, n) in ["<?", "<?x", "<?xm", "<?xml"].iter().enumerate() {
            self.decl[k] = self.t.state(g, format!("inicio {n}"));
        }
        let size = self.decl_dfa.n_states();
        self.decl_states = (0..size)
            .map(|s| self.t.state(g, format!("decl {s}")))
            .collect();

        // Textos de atributo, en un grupo global cada uno.
        for k in 0..self.an.attr_texts.len() {
            let tx = self.an.attr_texts[k];
            let g = self.t.group(format!("texto {}", self.text_label(tx)));
            let n = self.dfas[tx as usize].n_states();
            let states = (0..n).map(|s| self.t.state(g, format!("t{s}"))).collect();
            self.attr_states.insert(tx, states);
        }

        for f in 0..self.an.frames.len() {
            let (e, name) = self.an.frames[f];
            let spec = &self.an.elems[e as usize];
            let node = spec.node;
            let words = spec.mask_words + spec.counters;
            let mask_words = spec.mask_words;
            let mut attr_names: Vec<&str> = spec.attrs.iter().map(|a| a.0).collect();
            attr_names.push(XMLNS);
            let label = format!("<{}> {}", clip(name), location_clipped(self.ir, node, 64));
            let g = self.t.group(label.clone());
            let frame = self.t.frame(words, mask_words, label);
            let tag = self.t.state(g, "en la etiqueta");
            let attr = self.t.trie_states(g, &attr_names, "@", "@ desconocido");
            let xmlns = self.t.state(g, "@xmlns:");
            let content = match &self.an.elems[e as usize].content {
                Content::Empty => ContentStates::Empty {
                    c: self.t.state(g, "vacío"),
                    t: self.t.state(g, "texto no permitido"),
                    u: self.t.state(g, "< inesperado"),
                },
                Content::Simple { text, .. } => {
                    let n = self.dfas[*text as usize].n_states();
                    ContentStates::Simple {
                        states: (0..n)
                            .map(|s| self.t.state(g, format!("texto t{s}")))
                            .collect(),
                        u: self.t.state(g, "< inesperado"),
                        z: self.t.state(g, "texto tras un hijo inesperado"),
                    }
                }
                Content::Model(m) => {
                    let lists: Vec<Vec<&str>> = m
                        .states
                        .iter()
                        .map(|k| k.edges.iter().map(|e| e.name).collect())
                        .collect();
                    let mut v = Vec::with_capacity(lists.len());
                    for (k, ns) in lists.iter().enumerate() {
                        let c = self.t.state(g, format!("hijos k{k}"));
                        let tx = self.t.state(g, format!("hijos k{k} texto"));
                        let trie = self.t.trie_states(
                            g,
                            ns,
                            &format!("k{k} <"),
                            &format!("k{k} < inesperado"),
                        );
                        v.push((c, tx, trie));
                    }
                    ContentStates::Model(v)
                }
            };
            let end = self.t.trie_states(g, &[name], "</", "</ distinto");
            self.entries
                .push((frame, attr.0[attr.1.start as usize], content.start()));
            self.frames.push(FrameStates {
                tag,
                attr,
                xmlns,
                content,
                end,
            });
        }
    }

    fn text_label(&self, tx: u32) -> String {
        let (id, _) = self.an.texts[tx as usize].lexical_rule;
        location_clipped(self.ir, id, 64)
    }

    // --------------------------------------------------- rellenado ---

    fn ws_classes(&self) -> Vec<u32> {
        WS.iter()
            .map(|&c| class::CHAR + self.t.class_of(c) as u32)
            .collect()
    }

    /// Todas las clases de carácter de `q` vuelven a `q`.
    fn loop_chars(&mut self, q: u32) {
        let t = self.t.mv(q);
        self.chars(q, t);
    }

    /// Todas las clases de carácter de `q` con la transición `t`.
    fn chars(&mut self, q: u32, t: u32) {
        for a in class::CHAR..self.t.n_classes {
            self.t.set(q, a, t);
        }
    }

    /// Las filas de un trie de nombres; `colon`: `:` vuelve al inicio (nombre
    /// local, `docs/00-alcance.md` §1.2).
    fn trie(&mut self, (states, trie): &TrieStates, colon: bool) {
        for ts in 0..trie.n_states() as u32 {
            let q = states[ts as usize];
            for a in 0..trie.n_classes {
                let t = self.t.mv(states[trie.next(ts, a) as usize]);
                self.t.set(q, class::CHAR + a as u32, t);
            }
            if colon {
                let t = self.t.mv(states[trie.start as usize]);
                let c = class::CHAR + self.t.class_of(':') as u32;
                self.t.set(q, c, t);
            }
        }
    }

    /// `<?` desde `q`: abre la instrucción y vuelve a `q`.
    fn pi_from(&mut self, q: u32) {
        let t = self.t.trans(Trans {
            ret: q,
            ..plain(Op::Enter, self.pi[0])
        });
        self.t.set(q, xclass::PI_OPEN, t);
    }

    /// Un `Fails` de la regla `i` del nodo, o de su regla implícita `kind`.
    fn fails(&mut self, node: NodeId, i: Option<usize>, kind: RuleKind) -> Check {
        self.t
            .check_of(node, i.unwrap_or(usize::MAX), kind, Test::Fails, 0, 0)
    }

    fn fill(&mut self) {
        self.fill_fixed();
        for k in 0..self.an.attr_texts.len() {
            let tx = self.an.attr_texts[k];
            let states = self.attr_states[&tx].clone();
            let dfa = self.dfas[tx as usize].clone();
            for s in 0..dfa.n_states() as u32 {
                let q = states[s as usize];
                for a in 0..dfa.n_classes {
                    let t = self.t.mv(states[dfa.next(s, a) as usize]);
                    self.t.set(q, class::CHAR + a as u32, t);
                }
                let list = self.text_checks(tx, s);
                let leave = self.t.with_checks(Op::Leave, 0, list);
                self.t.set(q, xclass::VALUE_END, leave);
            }
        }
        for f in 0..self.frames.len() {
            self.fill_frame(f);
        }
    }

    fn fill_fixed(&mut self) {
        let ws = self.ws_classes();
        // Documento: prólogo (espacios, instrucciones, la declaración solo al
        // principio), el elemento raíz y el epílogo; otro texto o un segundo
        // elemento raíz son error de sintaxis (∞).
        let (d0, d1, fin) = (self.d0, self.d1, self.fin);
        let to_d1 = self.t.mv(d1);
        let to_fin = self.t.mv(fin);
        for &a in &ws {
            self.t.set(d0, a, to_d1);
            self.t.set(d1, a, to_d1);
            self.t.set(fin, a, to_fin);
        }
        let decl = self.t.trans(Trans {
            ret: d1,
            ..plain(Op::Enter, self.decl[0])
        });
        self.t.set(d0, xclass::PI_OPEN, decl);
        self.pi_from(d1);
        self.pi_from(fin);
        let trie = std::mem::replace(&mut self.doc_trie, (Vec::new(), Dfa::trie(1, &[])));
        let start = self.t.mv(trie.0[trie.1.start as usize]);
        self.t.set(d0, xclass::START_TAG, start);
        self.t.set(d1, xclass::START_TAG, start);
        self.trie(&trie, true);
        let edges = self.an.doc.states[0].edges.clone();
        let rule = self.an.doc.rule;
        self.name_ends(
            &trie,
            &edges,
            fin,
            |b| b.t.check(NodeId::ROOT, rule, Test::Fails, 0, 0),
            None,
            None,
        );
        self.doc_trie = trie;

        // Elemento saltado (F4-U3: un elemento inesperado suma 1 y se salta
        // su subárbol): cualquier atributo, texto e hijo.
        let (tag, attr, content, child, end) = (
            self.sk_tag,
            self.sk_attr,
            self.sk_content,
            self.sk_child,
            self.sk_end,
        );
        let t = self.t.mv(attr);
        self.t.set(tag, xclass::ATTR, t);
        let t = self.t.mv(content);
        self.t.set(tag, xclass::TAG_CLOSE, t);
        let pop = self.t.simple(Op::Pop, 0);
        self.t.set(tag, xclass::EMPTY_CLOSE, pop);
        self.loop_chars(attr);
        let t = self.t.trans(Trans {
            ret: tag,
            ..plain(Op::Enter, self.any_text)
        });
        self.t.set(attr, xclass::VALUE_BEGIN, t);
        self.loop_chars(content);
        let t = self.t.mv(child);
        self.t.set(content, xclass::START_TAG, t);
        let t = self.t.mv(end);
        self.t.set(content, xclass::END_TAG, t);
        self.pi_from(content);
        self.loop_chars(child);
        let push = |b: &mut Self, next: u32| {
            b.t.trans(Trans {
                ret: content,
                frame: b.skip_frame,
                ..plain(Op::Push, next)
            })
        };
        let t = push(self, attr);
        self.t.set(child, xclass::ATTR, t);
        let t = push(self, content);
        self.t.set(child, xclass::TAG_CLOSE, t);
        let t = self.t.mv(content);
        self.t.set(child, xclass::EMPTY_CLOSE, t);
        self.loop_chars(end);
        self.t.set(end, xclass::END_CLOSE, pop);
        let any = self.any_text;
        self.loop_chars(any);
        let leave = self.t.simple(Op::Leave, 0);
        self.t.set(any, xclass::VALUE_END, leave);

        // Instrucciones de procesamiento: el nombre `xml` (en cualquier
        // caja) está reservado salvo en la declaración, al principio.
        let [p0, p1, p2, p3, pok, pcont] = self.pi;
        let ok = self.t.mv(pok);
        for q in [p0, p1, p2, p3, pok] {
            self.chars(q, ok);
        }
        let cl = |b: &Self, s: &str| -> Vec<u32> {
            s.chars()
                .map(|c| class::CHAR + b.t.class_of(c) as u32)
                .collect()
        };
        for (from, to, letters) in [(p0, p1, "xX"), (p1, p2, "mM"), (p2, p3, "lL")] {
            let t = self.t.mv(to);
            for a in cl(self, letters) {
                self.t.set(from, a, t);
            }
        }
        let body = self.t.mv(pcont);
        for q in [p1, p2, pok] {
            self.t.set(q, xclass::PI_BODY, body);
            self.t.set(q, xclass::PI_CLOSE, leave);
        }
        self.loop_chars(pcont);
        self.t.set(pcont, xclass::PI_CLOSE, leave);
        // La declaración: `xml` en minúsculas; otra caja, al reservado.
        let [q0, q1, q2, q3] = self.decl;
        for q in [q0, q1, q2, q3] {
            self.chars(q, ok);
        }
        for (from, low, high, up) in [(q0, q1, p1, "x"), (q1, q2, p2, "m"), (q2, q3, p3, "l")] {
            let t = self.t.mv(low);
            for a in cl(self, up) {
                self.t.set(from, a, t);
            }
            let t = self.t.mv(high);
            for a in cl(self, &up.to_uppercase()) {
                self.t.set(from, a, t);
            }
        }
        for q in [q1, q2] {
            self.t.set(q, xclass::PI_BODY, body);
            self.t.set(q, xclass::PI_CLOSE, leave);
        }
        let dfa = self.decl_dfa.clone();
        let states = self.decl_states.clone();
        let t = self.t.mv(states[dfa.start as usize]);
        self.t.set(q3, xclass::PI_BODY, t);
        for s in 0..dfa.n_states() as u32 {
            let q = states[s as usize];
            for a in 0..dfa.n_classes {
                let t = self.t.mv(states[dfa.next(s, a) as usize]);
                self.t.set(q, class::CHAR + a as u32, t);
            }
            if dfa.out[s as usize] == TextOut::Decl {
                self.t.set(q, xclass::PI_CLOSE, leave);
            }
        }
    }

    /// Las transiciones que terminan el nombre de un hijo (atributo, `>` o
    /// `/>`) desde cada estado del trie: el hijo de la arista con ese nombre,
    /// o un elemento inesperado (`unexpected`, y se salta) que deja el
    /// contenido en `stay`. Se vuelve al estado entre hijos destino de la
    /// arista (`targets`) o a `stay` (el documento); `parent` es el
    /// elemento (y su regla `Content`) dueño de las guardas.
    fn name_ends(
        &mut self,
        (states, trie): &TrieStates,
        edges: &[KEdge<'a>],
        stay: u32,
        unexpected: impl Fn(&mut Self) -> Check,
        targets: Option<&[(u32, u32, TrieStates)]>,
        parent: Option<(NodeId, usize)>,
    ) {
        let names: Vec<&str> = edges.iter().map(|e| e.name).collect();
        // El trie se hizo con estas palabras en este orden: la salida j es la
        // arista j.
        for ts in 0..trie.n_states() as u32 {
            let q = states[ts as usize];
            match trie.out[ts as usize] {
                Some(j) => {
                    let e = edges[j as usize].clone();
                    debug_assert_eq!(names[j as usize], e.name);
                    let ret = match targets {
                        Some(t) => t[e.to as usize].0,
                        None => stay,
                    };
                    self.child_entry(q, &e, ret, parent);
                }
                None => {
                    let c = unexpected(self);
                    self.skip_entry(q, c, stay);
                }
            }
        }
    }

    /// Un hijo inesperado desde `q`: suma 1 (la comprobación `c`), salta su
    /// subárbol y vuelve a `ret` sin mover el contenido.
    fn skip_entry(&mut self, q: u32, c: Check, ret: u32) {
        let (checks, n_checks) = self.t.checks(vec![c]);
        let push = |b: &mut Self, next: u32| {
            b.t.trans(Trans {
                ret,
                frame: b.skip_frame,
                checks,
                n_checks,
                ..plain(Op::Push, next)
            })
        };
        let t = push(self, self.sk_attr);
        self.t.set(q, xclass::ATTR, t);
        let t = push(self, self.sk_content);
        self.t.set(q, xclass::TAG_CLOSE, t);
        let t = self.t.trans(Trans {
            checks,
            n_checks,
            ..plain(Op::Move, ret)
        });
        self.t.set(q, xclass::EMPTY_CLOSE, t);
    }

    /// La comprobación de una guarda de contador del elemento `node`.
    fn guard(&mut self, node: NodeId, rule: usize, g: XGuard) -> Check {
        match g {
            XGuard::Count(Guard::Min(c, b)) | XGuard::Count(Guard::Max(c, b)) => {
                let test = if matches!(g, XGuard::Count(Guard::Min(..))) {
                    Test::CountMin
                } else {
                    Test::CountMax
                };
                let (arg, len) = Check::split(b);
                Check {
                    counter: c,
                    ..self.t.check(node, rule, test, arg, len)
                }
            }
            XGuard::Unset(bit) => self.t.check(node, rule, Test::Unset, bit, 0),
        }
    }

    /// El hijo `e` desde el estado `q` del trie (el padre en la cima): las
    /// guardas y contadores del padre y, según el evento, apilar el marco
    /// del hijo (atributo, `>`) o, con `/>`, sus comprobaciones de vacío sin
    /// apilar.
    fn child_entry(&mut self, q: u32, e: &KEdge<'a>, ret: u32, parent: Option<(NodeId, usize)>) {
        let mut base = Vec::new();
        if let Some((node, rule)) = parent {
            for &g in &e.guards {
                base.push(self.guard(node, rule, g));
            }
        }
        let (counts, n_counts) = self.t.counter_ops(&e.ops);
        let mark = e.mark.unwrap_or(NO_BIT);
        let f = self.an.frame_ids[&(e.ty, e.name)] as usize;
        let el = self.an.frames[f].0 as usize;
        let (frame, attr_start, start) = self.entries[f];
        let req = self.required_fails(el);
        let empty = self.empty_checks(el);
        let mk = |b: &mut Self, op: Op, next: u32, frame: u32, list: Vec<Check>| {
            let (checks, n_checks) = b.t.checks(list);
            b.t.trans(Trans {
                ret: if op == Op::Push { ret } else { 0 },
                frame,
                mark,
                counts,
                n_counts,
                checks,
                n_checks,
                ..plain(op, next)
            })
        };
        // `@`: el marco en la etiqueta (el primer carácter del nombre del
        // atributo llega después).
        let t = mk(self, Op::Push, attr_start, frame, base.clone());
        self.t.set(q, xclass::ATTR, t);
        let mut l = base.clone();
        l.extend(req.iter().copied());
        let t = mk(self, Op::Push, start, frame, l.clone());
        self.t.set(q, xclass::TAG_CLOSE, t);
        l.extend(empty);
        let t = mk(self, Op::Move, ret, 0, l);
        self.t.set(q, xclass::EMPTY_CLOSE, t);
    }

    /// `Fails` de los atributos requeridos de un elemento que se cierra sin
    /// atributos.
    fn required_fails(&mut self, el: usize) -> Vec<Check> {
        let spec = &self.an.elems[el];
        if spec.required.is_empty() {
            return Vec::new();
        }
        let (node, rule) = (spec.node, spec.attr_rule);
        vec![self.fails(node, rule, RuleKind::Attributes)]
    }

    /// Las comprobaciones de un elemento sin hijos ni texto (`<e/>` o
    /// `<e></e>`): el contenido en su estado inicial.
    fn empty_checks(&mut self, el: usize) -> Vec<Check> {
        let node = self.an.elems[el].node;
        match &self.an.elems[el].content {
            Content::Empty => Vec::new(),
            Content::Simple { text, .. } => {
                let tx = *text;
                let s = self.dfas[tx as usize].start;
                self.text_checks(tx, s)
            }
            Content::Model(m) => match m.states[0].close {
                KClose::No => {
                    let r = m.rule;
                    vec![self.t.check(node, r, Test::Fails, 0, 0)]
                }
                _ => Vec::new(),
            },
        }
    }

    /// Las comprobaciones del cierre del texto `tx` en un estado de salida
    /// `o` (decididas al compilar salvo las que leen el lexema o la
    /// longitud).
    fn text_checks(&mut self, tx: u32, s: u32) -> Vec<Check> {
        let o = self.dfas[tx as usize].out[s as usize];
        let bits = self.pbits[tx as usize]
            .get(s as usize)
            .copied()
            .unwrap_or(0);
        if let Some(v) = self.text_memo.get(&(tx, o, bits)) {
            return v.clone();
        }
        let mut v = self.text_checks_new(tx, o);
        // F4.4b: 1 violación por `pattern` que el estado no cumple (decidido
        // al compilar). Un texto fuera del espacio léxico ya cuenta 1 y no
        // evalúa facetas (§36 de `pendientes.md`).
        if o != TextOut::Reject {
            let pats = self.an.texts[tx as usize].pats.clone();
            for (j, (id, i)) in pats.into_iter().enumerate() {
                if bits & (1 << j) == 0 {
                    v.push(self.t.check(id, i, Test::Fails, 0, 0));
                }
            }
        }
        self.text_memo.insert((tx, o, bits), v.clone());
        v
    }

    fn text_checks_new(&mut self, tx: u32, o: TextOut) -> Vec<Check> {
        let mut out = Vec::new();
        let spec = &self.an.texts[tx as usize];
        let (lexical, (ln, li)) = (spec.lexical, spec.lexical_rule);
        let double = lexical == Lexical::Double;
        let lits = spec.lits.clone();
        let lens = spec.lens.clone();
        let ranges = spec.ranges.clone();
        let nenums = spec.nenums.clone();
        let fixed = spec.fixed.clone();
        // Las palabras, por índice (ronda 3: sin copiarlas por texto).
        let ws = spec.words as usize;
        if o == TextOut::Reject {
            return vec![self.t.check(ln, li, Test::Fails, 0, 0)];
        }
        // Cadenas: literales y longitudes.
        if let TextOut::Str(w) = o {
            let word = (w != NO_WORD).then(|| self.an.wordsets[ws].words[w as usize]);
            for (id, i, k) in lits {
                if !word.is_some_and(|w| self.an.strsets[k as usize].contains(w)) {
                    out.push(self.t.check(id, i, Test::Fails, 0, 0));
                }
            }
            for b in lens {
                out.push(self.t.bound(b));
            }
        }
        // Rangos: exactos en `xs:integer`/`xs:decimal`; en `xs:float`/
        // `xs:double`, el umbral decimal exacto (5.5-CR-S1) y, en NaN e INF,
        // decididos aquí (5.5-P3).
        for (id, i, kind, lex) in ranges {
            let is_min = matches!(kind, RuleKind::Minimum | RuleKind::ExclusiveMinimum);
            match o {
                TextOut::Num if matches!(lexical, Lexical::Integer | Lexical::Decimal) => {
                    let test = match kind {
                        RuleKind::Minimum => Test::NumMin,
                        RuleKind::ExclusiveMinimum => Test::NumExMin,
                        RuleKind::Maximum => Test::NumMax,
                        _ => Test::NumExMax,
                    };
                    let arg = self.t.rule_lexeme(id, i, lex);
                    out.push(self.t.check(id, i, test, arg, 0));
                }
                TextOut::Num => {
                    // I4 (ronda 3): calculado una vez por regla en el análisis.
                    let Some(r) = self.an.float_bounds.get(&(id, i)) else {
                        self.t
                            .internal(format!("cota {lex:?} sin umbral IEEE calculado (D24)"));
                        continue;
                    };
                    let (test, thr) = match (kind, r.closed) {
                        (RuleKind::Minimum, true) => (Test::NumMin, &r.lo),
                        (RuleKind::Minimum, false) => (Test::NumExMin, &r.lo),
                        (RuleKind::Maximum, true) => (Test::NumMax, &r.hi),
                        (RuleKind::Maximum, false) => (Test::NumExMax, &r.hi),
                        (RuleKind::ExclusiveMinimum, true) => (Test::NumExMin, &r.hi),
                        (RuleKind::ExclusiveMinimum, false) => (Test::NumMin, &r.hi),
                        (_, true) => (Test::NumExMax, &r.lo),
                        (_, false) => (Test::NumMax, &r.lo),
                    };
                    let arg = self.t.rule_lexeme(id, i, thr);
                    out.push(self.t.check(id, i, test, arg, 0));
                }
                TextOut::Inf(neg) if neg != is_min => {}
                _ => out.push(self.t.check(id, i, Test::Fails, 0, 0)),
            }
        }
        // `enum` numéricos.
        for (id, i, k) in nenums {
            match (o, lexical) {
                (TextOut::Num, Lexical::Integer | Lexical::Decimal) => {
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
                    out.push(self.t.check(id, i, Test::NumEnum, arg, len));
                }
                (TextOut::Num, _) => {
                    let n = self.an.fenums[k as usize].len();
                    let arg = match self.fenum_ranges.get(&k) {
                        Some(&a) => a,
                        None => {
                            let list = self.an.fenums[k as usize].clone();
                            let a = self.intervals(&list);
                            self.fenum_ranges.insert(k, a);
                            a
                        }
                    };
                    let len = self.t.idx(n);
                    out.push(self.t.check(id, i, Test::NumIn, arg, len));
                }
                _ => out.push(self.t.check(id, i, Test::Fails, 0, 0)),
            }
        }
        // `fixed`, en el espacio de valores (5.6-P3).
        if let Some((owner, r, f)) = fixed {
            let ok = match (&f, o) {
                (Fixed::Str(v), TextOut::Str(w)) => {
                    w != NO_WORD && self.an.wordsets[ws].words[w as usize] == *v
                }
                (Fixed::Bool(v), TextOut::Bool(x)) => *v == x,
                (Fixed::Inf(n), TextOut::Inf(x)) => *n == x,
                (Fixed::NaN, TextOut::NaN) => true,
                (Fixed::Num(v), TextOut::Num) => {
                    let arg = match self.fixed_ids.get(&tx) {
                        Some(&a) => a,
                        None => {
                            let a = self.t.lexeme(v);
                            self.fixed_ids.insert(tx, a);
                            a
                        }
                    };
                    out.push(self.t.check(owner, r, Test::NumEnum, arg, 1));
                    true
                }
                (Fixed::Float(v), TextOut::Num) => {
                    let arg = match self.fixed_ids.get(&tx) {
                        Some(&a) => a,
                        None => {
                            let a = self.intervals(std::slice::from_ref(v));
                            self.fixed_ids.insert(tx, a);
                            a
                        }
                    };
                    out.push(self.t.check(owner, r, Test::NumIn, arg, 1));
                    true
                }
                (Fixed::Inf(neg), TextOut::Num) => {
                    let k = usize::from(*neg) + 2 * usize::from(double);
                    let arg = match self.overflow[k] {
                        Some(a) => a,
                        None => {
                            let l = overflow(double);
                            let l = if *neg { format!("-{l}") } else { l };
                            let a = self.t.lexeme(&l);
                            self.overflow[k] = Some(a);
                            a
                        }
                    };
                    let test = if *neg { Test::NumMax } else { Test::NumMin };
                    out.push(self.t.check(owner, r, test, arg, 0));
                    true
                }
                _ => false,
            };
            if !ok {
                out.push(self.t.check(owner, r, Test::Fails, 0, 0));
            }
        }
        out
    }

    /// Guarda intervalos de redondeo en las tablas y devuelve el primero.
    fn intervals(&mut self, list: &[Rounding]) -> u32 {
        let a = self.t.idx(self.t.a.intervals.len());
        for r in list {
            let lo = self.t.lexeme(&r.lo);
            let hi = self.t.lexeme(&r.hi);
            self.t.a.intervals.push(Interval {
                lo,
                hi,
                closed: r.closed,
            });
        }
        a
    }

    fn fill_frame(&mut self, f: usize) {
        let el = self.an.frames[f].0 as usize;
        let ws = self.ws_classes();
        let node = self.an.elems[el].node;
        let attr_rule = self.an.elems[el].attr_rule;
        let attrs = self.an.elems[el].attrs.clone();
        let required = self.an.elems[el].required.clone();
        let mask_words = self.an.elems[el].mask_words as usize;
        let fs = std::mem::replace(
            &mut self.frames[f],
            FrameStates {
                tag: 0,
                attr: (Vec::new(), Dfa::trie(1, &[])),
                xmlns: 0,
                content: ContentStates::Empty { c: 0, t: 0, u: 0 },
                end: (Vec::new(), Dfa::trie(1, &[])),
            },
        );
        let tag = fs.tag;
        let start = fs.content.start();
        let end_start = fs.end.0[fs.end.1.start as usize];

        // La etiqueta: atributos y su cierre (`>` comprueba los requeridos;
        // `/>`, también el contenido vacío, y desapila).
        let t = self.t.mv(fs.attr.0[fs.attr.1.start as usize]);
        self.t.set(tag, xclass::ATTR, t);
        let mut req = Vec::new();
        if !required.is_empty() {
            let ini = self.t.a.masks.len();
            self.t.a.masks.extend(std::iter::repeat_n(0, mask_words));
            for &b in &required {
                self.t.a.masks[ini + b as usize / 64] |= 1 << (b % 64);
            }
            let m = self.t.idx(ini);
            let r = attr_rule.unwrap_or(usize::MAX);
            req.push(
                self.t
                    .check_of(node, r, RuleKind::Attributes, Test::Mask, m, 0),
            );
        }
        let t = self.t.with_checks(Op::Move, start, req.clone());
        self.t.set(tag, xclass::TAG_CLOSE, t);
        let mut l = req;
        l.extend(self.empty_checks(el));
        let pop = self.t.with_checks(Op::Pop, 0, l);
        self.t.set(tag, xclass::EMPTY_CLOSE, pop);
        // Nombres de atributo: el declarado j fija su bit (repetido: ∞) y
        // abre su texto; `xmlns` y `xmlns:…` se ignoran; otro suma 1.
        self.trie(&fs.attr, false);
        let colon = class::CHAR + self.t.class_of(':') as u32;
        let ign = self.t.trans(Trans {
            ret: tag,
            ..plain(Op::Enter, self.any_text)
        });
        let unknown = {
            let c = self.fails(node, attr_rule, RuleKind::Attributes);
            let (checks, n_checks) = self.t.checks(vec![c]);
            self.t.trans(Trans {
                ret: tag,
                checks,
                n_checks,
                ..plain(Op::Enter, self.any_text)
            })
        };
        let (states, trie) = &fs.attr;
        for ts in 0..trie.n_states() as u32 {
            let q = states[ts as usize];
            let t = match trie.out[ts as usize] {
                Some(j) if (j as usize) < attrs.len() => {
                    let tx = attrs[j as usize].1;
                    let next = self.attr_states[&tx][self.dfas[tx as usize].start as usize];
                    self.t.trans(Trans {
                        ret: tag,
                        bit: j,
                        ..plain(Op::Enter, next)
                    })
                }
                Some(_) => {
                    let t = self.t.mv(fs.xmlns);
                    self.t.set(q, colon, t);
                    ign
                }
                None => unknown,
            };
            self.t.set(q, xclass::VALUE_BEGIN, t);
        }
        self.loop_chars(fs.xmlns);
        self.t.set(fs.xmlns, xclass::VALUE_BEGIN, ign);

        // El contenido.
        match &fs.content {
            ContentStates::Empty { c, t, u } => {
                let (c, tt, u) = (*c, *t, *u);
                let chk = self.fails(node, None, RuleKind::Content);
                let bad = self.t.with_checks(Op::Move, tt, vec![chk]);
                self.chars(c, bad);
                let x = self.t.mv(tt);
                self.chars(tt, x);
                for q in [c, tt] {
                    let t = self.t.mv(u);
                    self.t.set(q, xclass::START_TAG, t);
                    let t = self.t.mv(end_start);
                    self.t.set(q, xclass::END_TAG, t);
                    self.pi_from(q);
                }
                let x = self.t.mv(u);
                self.chars(u, x);
                let chk = self.fails(node, None, RuleKind::Content);
                self.skip_entry(u, chk, c);
            }
            ContentStates::Simple { states, u, z } => {
                let (u, z) = (*u, *z);
                let Content::Simple { rule, text } = &self.an.elems[el].content else {
                    return;
                };
                let (rule, tx) = (*rule, *text);
                let dfa = self.dfas[tx as usize].clone();
                for s in 0..dfa.n_states() as u32 {
                    let q = states[s as usize];
                    for a in 0..dfa.n_classes {
                        let t = self.t.mv(states[dfa.next(s, a) as usize]);
                        self.t.set(q, class::CHAR + a as u32, t);
                    }
                    let list = self.text_checks(tx, s);
                    let t = self.t.with_checks(Op::Move, end_start, list);
                    self.t.set(q, xclass::END_TAG, t);
                    let t = self.t.mv(u);
                    self.t.set(q, xclass::START_TAG, t);
                    self.pi_from(q);
                }
                let x = self.t.mv(u);
                self.chars(u, x);
                let chk = self.t.check(node, rule, Test::Fails, 0, 0);
                self.skip_entry(u, chk, z);
                let x = self.t.mv(z);
                self.chars(z, x);
                let t = self.t.mv(u);
                self.t.set(z, xclass::START_TAG, t);
                let t = self.t.mv(end_start);
                self.t.set(z, xclass::END_TAG, t);
                self.pi_from(z);
            }
            ContentStates::Model(ks) => {
                let Content::Model(m) = &self.an.elems[el].content else {
                    return;
                };
                let rule = m.rule;
                let mstates = m.states.clone();
                for (k, (c, tt, trie)) in ks.iter().enumerate() {
                    let (c, tt) = (*c, *tt);
                    // Solo espacios entre hijos (contenido de solo elementos);
                    // otro texto suma 1 una vez por tramo.
                    let chk = self.t.check(node, rule, Test::Fails, 0, 0);
                    let bad = self.t.with_checks(Op::Move, tt, vec![chk]);
                    self.chars(c, bad);
                    let stay = self.t.mv(c);
                    for &a in &ws {
                        self.t.set(c, a, stay);
                    }
                    let x = self.t.mv(tt);
                    self.chars(tt, x);
                    let close = match &mstates[k].close {
                        KClose::No => vec![self.t.check(node, rule, Test::Fails, 0, 0)],
                        KClose::Guards(g) => g
                            .iter()
                            .map(|&g| self.guard(node, rule, XGuard::Count(g)))
                            .collect(),
                        KClose::Mask(bits) => {
                            let ini = self.t.a.masks.len();
                            self.t.a.masks.extend(std::iter::repeat_n(0, mask_words));
                            for &b in bits {
                                self.t.a.masks[ini + b as usize / 64] |= 1 << (b % 64);
                            }
                            let m = self.t.idx(ini);
                            vec![self.t.check(node, rule, Test::Mask, m, 0)]
                        }
                    };
                    let to_end = self.t.with_checks(Op::Move, end_start, close);
                    let to_trie = self.t.mv(trie.0[trie.1.start as usize]);
                    for q in [c, tt] {
                        self.t.set(q, xclass::END_TAG, to_end);
                        self.t.set(q, xclass::START_TAG, to_trie);
                        self.pi_from(q);
                    }
                    self.trie(trie, true);
                    let edges = mstates[k].edges.clone();
                    self.name_ends(
                        trie,
                        &edges,
                        c,
                        |b| b.t.check(node, rule, Test::Fails, 0, 0),
                        Some(ks),
                        Some((node, rule)),
                    );
                }
            }
        }

        // El nombre de cierre: el mismo nombre local que la apertura (si no,
        // el error de sintaxis por defecto).
        self.trie(&fs.end, true);
        let (es, et) = &fs.end;
        for ts in 0..et.n_states() as u32 {
            if et.out[ts as usize].is_some() {
                let pop = self.t.simple(Op::Pop, 0);
                self.t.set(es[ts as usize], xclass::END_CLOSE, pop);
            }
        }
        self.frames[f] = fs;
    }
}

/// El AFD del texto `d` (sobre las clases globales) por el de cada
/// `xs:pattern` del texto (F4.4b): cada patrón se pasa a las clases globales
/// (la partición global refina la suya) y, si el tipo no es `xs:string`, se
/// le pone delante el transductor de *collapse* (el patrón ve el valor
/// normalizado, como el espacio léxico). Producto contado antes de crear
/// cada estado (`cap`), minimizado; devuelve el AFD con la salida del texto
/// y, por estado, los bits de los `pattern` que se cumplen.
fn with_patterns(
    t: &mut Emitter<'_>,
    an: &Analysis<'_>,
    spec: &TextSpec<'_>,
    d: Dfa<TextOut>,
    cap: usize,
) -> Result<(Dfa<TextOut>, Vec<u64>), BuildError> {
    let n = d.n_classes;
    let ws: Vec<usize> = WS.iter().map(|&c| t.class_of(c)).collect();
    let space = t.class_of(' ');
    let mut ps: Vec<Dfa<bool>> = Vec::new();
    for k in &spec.pats {
        let p = &an.patterns[k];
        let local: Vec<usize> = (0..n)
            .map(|a| p.part.class(t.class_rep(a)) as usize)
            .collect();
        let mut trans = Vec::with_capacity(p.dfa.n_states() * n);
        for q in 0..p.dfa.n_states() as u32 {
            for &l in &local {
                trans.push(p.dfa.next(q, l));
            }
        }
        let g = Dfa {
            n_classes: n,
            start: p.dfa.start,
            trans,
            out: p.dfa.out.clone(),
        };
        ps.push(if spec.lexical == Lexical::String {
            g
        } else {
            crate::dfa::collapse(&g, &ws, space)
        });
    }
    type Key = (u32, Vec<u32>);
    let start: Key = (d.start, ps.iter().map(|p| p.start).collect());
    let mut ids: HashMap<Key, u32> = HashMap::from([(start.clone(), 0)]);
    let mut keys = vec![start];
    let mut trans = Vec::new();
    let mut i = 0;
    while i < keys.len() {
        let (q, qs) = keys[i].clone();
        for a in 0..n {
            let key: Key = (
                d.next(q, a),
                qs.iter().zip(&ps).map(|(&x, p)| p.next(x, a)).collect(),
            );
            let id = match ids.get(&key) {
                Some(&id) => id,
                None => {
                    if keys.len() >= cap {
                        return Err(BuildError::ProductLimit {
                            states: keys.len() + 1,
                            limit: cap,
                            scalar: true,
                            location: location_clipped(t.ir, spec.pats[0].0, LOCATION_MAX),
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
    let out: Vec<(TextOut, u64)> = keys
        .iter()
        .map(|(q, qs)| {
            let bits = qs
                .iter()
                .zip(&ps)
                .enumerate()
                .filter(|(_, (&x, p))| p.out[x as usize])
                .fold(0u64, |b, (j, _)| b | (1 << j));
            (d.out[*q as usize], bits)
        })
        .collect();
    let raw = Dfa {
        n_classes: n,
        start: 0,
        trans,
        out,
    };
    let m = t.minimized(raw);
    let bits = m.out.iter().map(|o| o.1).collect();
    let out = m.out.iter().map(|o| o.0).collect();
    Ok((
        Dfa {
            n_classes: m.n_classes,
            start: m.start,
            trans: m.trans,
            out,
        },
        bits,
    ))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    std::thread_local! {
        /// Palabras de `enum` de cadena que ha copiado el análisis en este
        /// hilo (al unir los literales de un texto).
        pub(super) static PALABRAS: Cell<u64> = const { Cell::new(0) };
    }

    fn xsd_de(src: String) -> ir::VerifiedIr {
        std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                let s = xsd::parse(src.as_bytes()).unwrap();
                xsd::lower(&xsd::analyze(&s).unwrap()).unwrap()
            })
            .unwrap()
            .join()
            .unwrap()
    }

    /// Ronda extra de F4.1c (I6 = F4E-M1): un `xs:choice*` de 260 hijos cuyos
    /// nombres comparten un prefijo de `l` caracteres tiene 67 600 pares (bajo
    /// el tope) pero Σₓ Σ_{y ∈ follow(x)} |nombre(y)| ≈ 67 600 · `l` bytes. Se
    /// cargan al presupuesto de pasos **antes** de la UPA y de los tries por
    /// estado: `limit` sin recorrer los nombres (antes, ≈ 2,7·10⁸ bytes en
    /// `trie_size` con `l` = 2 000). Sin reloj (D1).
    #[test]
    fn nombres_largos_de_follow_se_cobran_antes() {
        let l = 2000;
        let prefijo = "p".repeat(l);
        let hijos: String = (0..260)
            .map(|i| format!("<xs:element name='{prefijo}{i}' type='xs:string'/>"))
            .collect();
        let src = format!(
            "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'><xs:element name='r'>\
             <xs:complexType><xs:choice maxOccurs='unbounded'>{hijos}</xs:choice>\
             </xs:complexType></xs:element></xs:schema>"
        );
        let ir = xsd_de(src);
        crate::emit::CARACTERES.with(|c| c.set(0));
        let r = crate::build(&ir);
        let bytes = crate::emit::CARACTERES.with(Cell::get);
        assert!(
            matches!(r, Err(crate::BuildError::Limit { .. })),
            "{:?}",
            r.map(|a| a.summary())
        );
        assert!(bytes < 1 << 20, "{bytes} bytes de nombres recorridos");
    }

    /// Ronda 3 de F4.1c (F4D-S1): `d` tipos derivados (cada uno con su
    /// `maxLength`) de una base con un `enum` de `e` cadenas. Las palabras
    /// del texto se memorizan por la lista de reglas `enum`, así que se
    /// copian una vez (≤ `e` visitas) y no una vez por cierre (antes,
    /// `d` · `e`): sin reloj (D1).
    #[test]
    fn enum_derivado_se_copia_una_vez() {
        let (e, d) = (500, 20);
        let lits: String = (0..e)
            .map(|i| format!("<xs:enumeration value='w{i:05}'/>"))
            .collect();
        let mut src = format!(
            "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema'>\
             <xs:simpleType name='S'><xs:restriction base='xs:string'>{lits}</xs:restriction></xs:simpleType>"
        );
        for k in 0..d {
            src.push_str(&format!(
                "<xs:simpleType name='D{k}'><xs:restriction base='S'><xs:maxLength value='{}'/></xs:restriction></xs:simpleType>",
                10 + k
            ));
        }
        let hijos: String = (0..d)
            .map(|k| format!("<xs:element name='e{k}' type='D{k}'/>"))
            .collect();
        src.push_str(&format!(
            "<xs:element name='r'><xs:complexType><xs:sequence>{hijos}</xs:sequence></xs:complexType></xs:element></xs:schema>"
        ));
        let ir = xsd_de(src);
        PALABRAS.with(|c| c.set(0));
        let a = crate::build(&ir).unwrap();
        let visitas = PALABRAS.with(Cell::get);
        assert!(visitas <= e as u64, "{visitas} visitas");
        // Y M sigue aplicando el `enum` y el `maxLength` de cada hijo.
        let mut ok = String::from("<r>");
        for k in 0..d {
            ok.push_str(&format!("<e{k}>w{k:05}</e{k}>"));
        }
        ok.push_str("</r>");
        let r = crate::validate(&a, ok.as_bytes(), runtime_rs::Mode::Total, 7);
        assert_eq!(r.violations.get(), Some(0));
        let mal = ok.replacen("<e0>w00000</e0>", "<e0>x</e0>", 1);
        let r = crate::validate(&a, mal.as_bytes(), runtime_rs::Mode::Total, 7);
        assert_eq!(r.violations.get(), Some(1));
    }
}
