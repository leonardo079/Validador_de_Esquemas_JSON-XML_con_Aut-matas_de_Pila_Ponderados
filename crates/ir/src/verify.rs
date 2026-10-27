//! Verificador de los invariantes de la IR (`docs/fase3/ir.md` §4).
//!
//! Todo es iterativo (D7): un bucle por nodo y por regla, una pila explícita
//! para los literales y un DFS con pila en el montón para los ciclos. El
//! costo es O(n + m log m), con n el tamaño de la IR (nodos, reglas, aristas,
//! tokens de los literales y partículas) y m el número de errores, que se
//! ordenan por nodo al final; con tablas hash para los nombres únicos. Sobre
//! una IR válida, m = 0 y el costo es O(n).

use core::fmt;
use std::collections::{HashMap, HashSet};

use crate::{
    target_sort, Constraint, ContentModel, Format, Ir, Lexical, LitToken, Literal, Node, NodeId,
    Num, Pos, Rule, RuleKind, Sort, Term,
};

/// Un invariante roto. El verificador no se detiene en el primero: devuelve
/// todos, en orden de nodo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    /// Nodo donde se detecta.
    pub node: NodeId,
    /// Inicio de la regla culpable o, si el problema es del nodo, de su
    /// origen.
    pub at: Pos,
    pub kind: VerifyErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyErrorKind {
    /// I1: la IR no tiene nodos, o el origen de un nodo no forma un árbol
    /// con raíz en el nodo 0.
    Origin { problem: &'static str },
    /// I2: arista a un nodo que no existe.
    Dangling { kind: RuleKind, target: NodeId },
    /// I3: regla que no admite el tipo del nodo.
    NotAllowed { kind: RuleKind, sort: Sort },
    /// I4: dos reglas de la misma clase en un nodo, o contenido simple y de
    /// elementos a la vez.
    Duplicate { kind: RuleKind },
    /// I5: el destino de una arista no es del tipo que exige la regla.
    TargetSort {
        kind: RuleKind,
        target: NodeId,
        expected: Sort,
        found: Sort,
    },
    /// I6: tipos de nodo que no corresponden al formato de la IR.
    Format { problem: &'static str },
    /// I7: lista vacía donde la regla exige al menos un elemento.
    Empty { kind: RuleKind },
    /// I8: número que no es un lexema de RFC 8259, o `multipleOf` ≤ 0.
    Number {
        kind: RuleKind,
        lexeme: String,
        problem: &'static str,
    },
    /// I9: literal mal formado de `enum` (`index`: su posición) o de
    /// `const` (`index` 0).
    Literal {
        kind: RuleKind,
        index: usize,
        problem: &'static str,
    },
    /// I10: nombre repetido donde la regla los exige únicos.
    RepeatedName { kind: RuleKind, name: String },
    /// I11: el modelo de contenido no es un árbol en preorden, o una
    /// partícula tiene `minOccurs` > `maxOccurs`.
    Content { problem: &'static str },
    /// I12: `xs:all` fuera de las restricciones de XSD 1.0 (*all-group
    /// limited*). Dos declaraciones del mismo elemento con tipos distintos
    /// son [`VerifyErrorKind::InconsistentElement`].
    All { problem: &'static str },
    /// I12: «Element Declarations Consistent» de XSD.
    InconsistentElement { name: String },
    /// I13: nodo de texto sin espacio léxico único, faceta que no aplica a
    /// su espacio léxico, o literal de otro tipo.
    Text { problem: &'static str },
    /// I13: atributo con `default` y `fixed`, o requerido con `default`.
    Attribute { name: String, problem: &'static str },
    /// I14: ciclo de aristas sobre la misma instancia (`$ref`, `allOf`…).
    Cycle { kind: RuleKind, target: NodeId },
}

impl VerifyError {
    /// Nombre corto de la clase de error, estable para las pruebas.
    pub fn code(&self) -> &'static str {
        match self.kind {
            VerifyErrorKind::Origin { .. } => "origin",
            VerifyErrorKind::Dangling { .. } => "dangling",
            VerifyErrorKind::NotAllowed { .. } => "rule-sort",
            VerifyErrorKind::Duplicate { .. } => "duplicate-rule",
            VerifyErrorKind::TargetSort { .. } => "target-sort",
            VerifyErrorKind::Format { .. } => "format",
            VerifyErrorKind::Empty { .. } => "empty",
            VerifyErrorKind::Number { .. } => "number",
            VerifyErrorKind::Literal { .. } => "literal",
            VerifyErrorKind::RepeatedName { .. } => "duplicate-name",
            VerifyErrorKind::Content { .. } => "content-model",
            VerifyErrorKind::All { .. } => "all",
            VerifyErrorKind::InconsistentElement { .. } => "element-consistency",
            VerifyErrorKind::Text { .. } => "text",
            VerifyErrorKind::Attribute { .. } => "attribute",
            VerifyErrorKind::Cycle { .. } => "cycle",
        }
    }
}

fn sort_name(s: Sort) -> &'static str {
    match s {
        Sort::Json => "valor JSON",
        Sort::Element => "elemento XML",
        Sort::Text => "texto XML",
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IR inválida: nodo {} (línea {}, columna {}): ",
            self.node.index(),
            self.at.line,
            self.at.column
        )?;
        match &self.kind {
            VerifyErrorKind::Origin { problem } => write!(f, "origen: {problem}"),
            VerifyErrorKind::Dangling { kind, target } => write!(
                f,
                "\"{}\" apunta al nodo {}, que no existe",
                kind.name(),
                target.index()
            ),
            VerifyErrorKind::NotAllowed { kind, sort } => write!(
                f,
                "\"{}\" no se aplica a un nodo de tipo {}",
                kind.name(),
                sort_name(*sort)
            ),
            VerifyErrorKind::Duplicate { kind } => {
                write!(f, "\"{}\" repetida en el mismo nodo", kind.name())
            }
            VerifyErrorKind::TargetSort {
                kind,
                target,
                expected,
                found,
            } => write!(
                f,
                "\"{}\" apunta al nodo {}, de tipo {}, pero exige un nodo de tipo {}",
                kind.name(),
                target.index(),
                sort_name(*found),
                sort_name(*expected)
            ),
            VerifyErrorKind::Format { problem } => write!(f, "formato: {problem}"),
            VerifyErrorKind::Empty { kind } => {
                write!(f, "\"{}\" necesita al menos un elemento", kind.name())
            }
            VerifyErrorKind::Number {
                kind,
                lexeme,
                problem,
            } => write!(f, "\"{}\": número \"{lexeme}\" {problem}", kind.name()),
            VerifyErrorKind::Literal {
                kind,
                index,
                problem,
            } => write!(f, "literal {index} de \"{}\": {problem}", kind.name()),
            VerifyErrorKind::RepeatedName { kind, name } => {
                write!(f, "\"{}\": nombre \"{name}\" repetido", kind.name())
            }
            VerifyErrorKind::Content { problem } => write!(f, "modelo de contenido: {problem}"),
            VerifyErrorKind::All { problem } => write!(f, "xs:all: {problem}"),
            VerifyErrorKind::InconsistentElement { name } => write!(
                f,
                "el elemento \"{name}\" aparece con dos tipos distintos en el mismo modelo \
                 de contenido"
            ),
            VerifyErrorKind::Text { problem } => write!(f, "texto: {problem}"),
            VerifyErrorKind::Attribute { name, problem } => {
                write!(f, "atributo \"{name}\": {problem}")
            }
            VerifyErrorKind::Cycle { kind, target } => write!(
                f,
                "ciclo de aristas sobre la misma instancia: \"{}\" vuelve al nodo {} sin \
                 consumir entrada",
                kind.name(),
                target.index()
            ),
        }
    }
}

impl std::error::Error for VerifyError {}

impl Ir {
    /// Comprueba los invariantes I1–I14 (`docs/fase3/ir.md` §4) y devuelve
    /// todos los que se rompen, en orden de nodo. Dentro de un nodo: primero
    /// los del nodo (origen, formato, texto sin espacio léxico), después los
    /// de cada regla en el orden de las reglas, y al final los de ciclo
    /// (I14), que salen de un recorrido de todo el grafo. O(n + m log m), con
    /// n el tamaño de la IR y m el número de errores (O(n) si es válida); sin
    /// recursión (D7).
    pub fn verify(&self) -> Result<(), Vec<VerifyError>> {
        if self.nodes.is_empty() {
            return Err(vec![VerifyError {
                node: NodeId::ROOT,
                at: Pos::START,
                kind: VerifyErrorKind::Origin {
                    problem: "la IR no tiene nodos (falta la raíz)",
                },
            }]);
        }
        let lexical = self
            .nodes
            .iter()
            .map(|n| {
                n.rules.iter().find_map(|c| match c.rule {
                    Rule::Lexical(l) => Some(l),
                    _ => None,
                })
            })
            .collect();
        let mut v = Verifier {
            ir: self,
            errors: Vec::new(),
            lexical,
        };
        let mut targets = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            v.node(NodeId(i), node, &mut targets);
        }
        v.cycles();
        // Los errores de ciclo se añaden al final; el orden es estable, así
        // que dentro de cada nodo se conserva el orden de las reglas.
        v.errors.sort_by_key(|e| e.node);
        if v.errors.is_empty() {
            Ok(())
        } else {
            Err(v.errors)
        }
    }
}

struct Verifier<'a> {
    ir: &'a Ir,
    errors: Vec<VerifyError>,
    /// Primer `Lexical` de cada nodo, calculado en una sola pasada: así
    /// comprobar un `Ref` entre textos es O(1) aunque la base tenga muchas
    /// reglas o rompa I4.
    lexical: Vec<Option<Lexical>>,
}

impl<'a> Verifier<'a> {
    fn push(&mut self, node: NodeId, at: Pos, kind: VerifyErrorKind) {
        self.errors.push(VerifyError { node, at, kind });
    }

    fn node(&mut self, id: NodeId, node: &'a Node, targets: &mut Vec<NodeId>) {
        let at = node.origin.span.start;
        // I1: el origen forma un árbol con raíz en 0 (padre siempre anterior).
        let origin = match (id.0, node.origin.parent, &node.origin.step) {
            (0, None, crate::Step::Root) => None,
            (0, Some(_), _) => Some("la raíz no puede tener padre"),
            (0, None, _) => Some("el paso de la raíz debe ser `Root`"),
            (_, None, _) => Some("solo la raíz puede no tener padre"),
            (i, Some(p), _) if p.0 >= i => Some("el padre debe ser un nodo anterior"),
            (_, Some(_), crate::Step::Root) => Some("solo la raíz tiene el paso `Root`"),
            _ => None,
        };
        if let Some(problem) = origin {
            self.push(id, at, VerifyErrorKind::Origin { problem });
        }
        self.format(id, node);
        // I13, la parte del nodo: un texto tiene espacio léxico. Lo demás de
        // I13 se comprueba regla a regla, en el bucle de abajo.
        if node.sort == Sort::Text && self.lexical[id.0].is_none() {
            self.push(
                id,
                at,
                VerifyErrorKind::Text {
                    problem: "falta el tipo predefinido (espacio léxico)",
                },
            );
        }

        let mut seen = [false; RuleKind::ALL.len()];
        for c in &node.rules {
            let kind = c.rule.kind();
            let at = c.span.start;
            // I4: una regla de cada clase; contenido simple o de elementos.
            let exclusive = match kind {
                RuleKind::Content => seen[RuleKind::SimpleContent as usize],
                RuleKind::SimpleContent => seen[RuleKind::Content as usize],
                _ => false,
            };
            if seen[kind as usize] || exclusive {
                self.push(id, at, VerifyErrorKind::Duplicate { kind });
            }
            seen[kind as usize] = true;
            // I3: la regla se aplica a este tipo de nodo.
            if !kind.allowed_in(node.sort) {
                self.push(
                    id,
                    at,
                    VerifyErrorKind::NotAllowed {
                        kind,
                        sort: node.sort,
                    },
                );
                continue;
            }
            // I2 e I5: destinos en rango y del tipo que exige la regla.
            targets.clear();
            c.rule.targets(targets);
            let expected = target_sort(node.sort, kind);
            for &t in targets.iter() {
                match self.ir.nodes.get(t.0) {
                    None => self.push(id, at, VerifyErrorKind::Dangling { kind, target: t }),
                    Some(tn) if tn.sort != expected => self.push(
                        id,
                        at,
                        VerifyErrorKind::TargetSort {
                            kind,
                            target: t,
                            expected,
                            found: tn.sort,
                        },
                    ),
                    Some(_) => {}
                }
            }
            self.rule(id, at, &c.rule);
            if let (Sort::Text, Some(lx)) = (node.sort, self.lexical[id.0]) {
                self.text_rule(id, lx, c);
            }
        }
    }

    /// I6: los tipos de nodo corresponden al formato; en XML la raíz es el
    /// documento.
    fn format(&mut self, id: NodeId, node: &Node) {
        let at = node.origin.span.start;
        let problem = match (&self.ir.format, node.sort) {
            (Format::Json, Sort::Json) | (Format::Xml { .. }, Sort::Element | Sort::Text) => None,
            (Format::Json, _) => Some("nodo XML en una IR de JSON"),
            (Format::Xml { .. }, Sort::Json) => Some("nodo JSON en una IR de XML"),
        };
        if let Some(problem) = problem {
            self.push(id, at, VerifyErrorKind::Format { problem });
            return;
        }
        if id != NodeId::ROOT || !matches!(self.ir.format, Format::Xml { .. }) {
            return;
        }
        let has = |k: RuleKind| node.rules.iter().any(|c| c.rule.kind() == k);
        let problem = if node.sort != Sort::Element {
            Some("la raíz de una IR de XML es el documento: un nodo de tipo elemento")
        } else if !has(RuleKind::Content) {
            Some("el documento necesita un modelo de contenido (los elementos globales)")
        } else if has(RuleKind::Attributes) || has(RuleKind::SimpleContent) {
            Some("el documento no tiene atributos ni contenido simple")
        } else {
            None
        };
        if let Some(problem) = problem {
            self.push(id, at, VerifyErrorKind::Format { problem });
        }
    }

    /// Comprobaciones propias de cada regla: I7–I12.
    fn rule(&mut self, id: NodeId, at: Pos, rule: &Rule) {
        let kind = rule.kind();
        let empty = match rule {
            Rule::AllOf(v) | Rule::AnyOf(v) | Rule::OneOf(v) => v.is_empty(),
            Rule::Type(t) => t.is_empty(),
            Rule::Items { prefix, rest } => prefix.is_empty() && rest.is_none(),
            _ => false,
        };
        if empty {
            self.push(id, at, VerifyErrorKind::Empty { kind });
        }
        match rule {
            Rule::Minimum(n)
            | Rule::Maximum(n)
            | Rule::ExclusiveMinimum(n)
            | Rule::ExclusiveMaximum(n)
            | Rule::MultipleOf(n) => self.number(id, at, kind, n),
            Rule::Enum(values) => {
                for (index, lit) in values.iter().enumerate() {
                    if let Some(problem) = literal_problem(lit) {
                        self.push(
                            id,
                            at,
                            VerifyErrorKind::Literal {
                                kind,
                                index,
                                problem,
                            },
                        );
                    }
                }
            }
            Rule::Const(lit) => {
                if let Some(problem) = literal_problem(lit) {
                    self.push(
                        id,
                        at,
                        VerifyErrorKind::Literal {
                            kind,
                            index: 0,
                            problem,
                        },
                    );
                }
            }
            Rule::Properties { declared, .. } => {
                self.unique(id, at, kind, declared.iter().map(|(n, _)| n.as_str()))
            }
            Rule::Required(names) => self.unique(id, at, kind, names.iter().map(String::as_str)),
            Rule::DependentRequired(map) => {
                self.unique(id, at, kind, map.iter().map(|(n, _)| n.as_str()));
                for (_, names) in map {
                    self.unique(id, at, kind, names.iter().map(String::as_str));
                }
            }
            Rule::DependentSchemas(map) => {
                self.unique(id, at, kind, map.iter().map(|(n, _)| n.as_str()))
            }
            Rule::Attributes(attrs) => {
                self.unique(id, at, kind, attrs.iter().map(|a| a.name.as_str()));
                for a in attrs {
                    let problem = if a.default.is_some() && a.fixed.is_some() {
                        Some("no puede tener `default` y `fixed` a la vez")
                    } else if a.required && a.default.is_some() {
                        Some("un atributo requerido no puede tener `default`")
                    } else {
                        None
                    };
                    if let Some(problem) = problem {
                        self.push(
                            id,
                            a.span.start,
                            VerifyErrorKind::Attribute {
                                name: a.name.clone(),
                                problem,
                            },
                        );
                    }
                }
            }
            Rule::Content(model) => self.content(id, at, model),
            _ => {}
        }
    }

    /// I8: lexema de RFC 8259; `multipleOf` > 0.
    fn number(&mut self, id: NodeId, at: Pos, kind: RuleKind, n: &Num) {
        let problem = if !is_rfc8259_number(&n.lexeme) {
            Some("no tiene la sintaxis de RFC 8259")
        } else if kind == RuleKind::MultipleOf && !is_positive(&n.lexeme) {
            Some("debe ser mayor que 0")
        } else {
            None
        };
        if let Some(problem) = problem {
            self.push(
                id,
                at,
                VerifyErrorKind::Number {
                    kind,
                    lexeme: n.lexeme.clone(),
                    problem,
                },
            );
        }
    }

    /// I10: nombres sin repetir (un error por repetición).
    fn unique<'n>(
        &mut self,
        id: NodeId,
        at: Pos,
        kind: RuleKind,
        names: impl Iterator<Item = &'n str>,
    ) {
        let mut set = HashSet::new();
        for name in names {
            if !set.insert(name) {
                self.push(
                    id,
                    at,
                    VerifyErrorKind::RepeatedName {
                        kind,
                        name: name.to_owned(),
                    },
                );
            }
        }
    }

    /// I11 (árbol en preorden, ocurrencias) e I12 (`xs:all`, declaraciones
    /// consistentes).
    fn content(&mut self, id: NodeId, at: Pos, model: &ContentModel) {
        let ps = &model.particles;
        if let Some(problem) = tree_problem(model) {
            self.push(id, at, VerifyErrorKind::Content { problem });
            return;
        }
        for p in ps {
            if p.occurs.max.is_some_and(|max| p.occurs.min > max) {
                self.push(
                    id,
                    p.span.start,
                    VerifyErrorKind::Content {
                        problem: "una partícula tiene minOccurs mayor que maxOccurs",
                    },
                );
            }
        }
        for (i, p) in ps.iter().enumerate() {
            let Term::All(children) = &p.term else {
                continue;
            };
            let problem = if i != 0 {
                Some("solo puede ser la partícula raíz del modelo de contenido")
            } else if p.occurs.min > 1 || p.occurs.max != Some(1) {
                Some("su minOccurs debe ser 0 o 1 y su maxOccurs 1")
            } else if children.iter().any(|&c| {
                !matches!(ps[c].term, Term::Element { .. })
                    || ps[c].occurs.max.is_none_or(|max| max > 1)
            }) {
                Some("sus hijos deben ser elementos con maxOccurs 0 o 1")
            } else {
                None
            };
            if let Some(problem) = problem {
                self.push(id, p.span.start, VerifyErrorKind::All { problem });
            }
        }
        for p in ps {
            if matches!(p.term, Term::Element { ty, .. } if ty == NodeId::ROOT) {
                self.push(
                    id,
                    p.span.start,
                    VerifyErrorKind::Format {
                        problem: "un elemento no puede tener como tipo el documento (nodo 0)",
                    },
                );
            }
        }
        let mut types: HashMap<&str, NodeId> = HashMap::new();
        for p in ps {
            if let Term::Element { name, ty } = &p.term {
                if *types.entry(name).or_insert(*ty) != *ty {
                    self.push(
                        id,
                        p.span.start,
                        VerifyErrorKind::InconsistentElement { name: name.clone() },
                    );
                }
            }
        }
    }

    /// I13 para una regla de un nodo de texto con espacio léxico `lx`: sus
    /// facetas, enumeraciones y restricciones (`Ref`) son de ese espacio.
    fn text_rule(&mut self, id: NodeId, lx: Lexical, c: &Constraint) {
        let problem = match &c.rule {
            Rule::MinLength(_) | Rule::MaxLength(_) if lx != Lexical::String => {
                Some("las facetas de longitud solo se aplican a xs:string")
            }
            Rule::Minimum(_)
            | Rule::Maximum(_)
            | Rule::ExclusiveMinimum(_)
            | Rule::ExclusiveMaximum(_)
                if !lx.is_numeric() =>
            {
                Some("las facetas de rango solo se aplican a tipos numéricos")
            }
            // XSD 1.0 Parte 2, §3.2.2.1: `boolean` solo admite `pattern` y
            // `whiteSpace`.
            Rule::Enum(_) if lx == Lexical::Boolean => {
                Some("xs:boolean no admite xs:enumeration (solo pattern y whiteSpace)")
            }
            Rule::Enum(values) if values.iter().any(|v| !scalar_of(v, lx)) => {
                Some("cada xs:enumeration debe ser un valor escalar del tipo del texto")
            }
            Rule::Ref(t) => match (self.ir.nodes.get(t.0), self.lexical.get(t.0)) {
                (Some(base), Some(Some(b))) if base.sort == Sort::Text && *b != lx => {
                    Some("una restricción debe conservar el espacio léxico de su base")
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(problem) = problem {
            self.push(id, c.span.start, VerifyErrorKind::Text { problem });
        }
    }

    /// I14: el subgrafo de las aristas sobre la misma instancia es acíclico.
    /// DFS iterativo con tres colores; cada arista de retroceso es un ciclo
    /// y da un error en su origen. O(V + E).
    fn cycles(&mut self) {
        let n = self.ir.nodes.len();
        // CSR de las aristas sobre la misma instancia con destino en rango.
        let mut offsets = Vec::with_capacity(n + 1);
        let mut edges: Vec<(usize, RuleKind)> = Vec::new();
        let mut buf = Vec::new();
        for node in &self.ir.nodes {
            offsets.push(edges.len());
            for c in &node.rules {
                let kind = c.rule.kind();
                if !kind.same_instance() || !kind.allowed_in(node.sort) {
                    continue;
                }
                buf.clear();
                c.rule.targets(&mut buf);
                edges.extend(buf.iter().filter(|t| t.0 < n).map(|t| (t.0, kind)));
            }
        }
        offsets.push(edges.len());

        const WHITE: u8 = 0;
        const GRAY: u8 = 1;
        const BLACK: u8 = 2;
        let mut color = vec![WHITE; n];
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for s in 0..n {
            if color[s] != WHITE {
                continue;
            }
            color[s] = GRAY;
            stack.push((s, offsets[s]));
            while let Some(top) = stack.last_mut() {
                let (v, next) = *top;
                if next == offsets[v + 1] {
                    color[v] = BLACK;
                    stack.pop();
                    continue;
                }
                top.1 += 1;
                let (w, kind) = edges[next];
                match color[w] {
                    WHITE => {
                        color[w] = GRAY;
                        stack.push((w, offsets[w]));
                    }
                    GRAY => {
                        let at = self.ir.nodes[v].origin.span.start;
                        self.push(
                            NodeId(v),
                            at,
                            VerifyErrorKind::Cycle {
                                kind,
                                target: NodeId(w),
                            },
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

/// I11: las partículas forman un árbol en preorden con raíz en 0: cada hijo
/// de un grupo está en rango, es posterior al grupo y tiene un solo padre, y
/// toda partícula salvo la raíz es hija de algún grupo.
fn tree_problem(model: &ContentModel) -> Option<&'static str> {
    let ps = &model.particles;
    if ps.is_empty() {
        return Some("no tiene partículas");
    }
    let mut has_parent = vec![false; ps.len()];
    for (i, p) in ps.iter().enumerate() {
        let children = match &p.term {
            Term::Sequence(c) | Term::Choice(c) | Term::All(c) => c,
            Term::Element { .. } => continue,
        };
        for &c in children {
            if c >= ps.len() {
                return Some("un grupo tiene un hijo fuera de rango");
            }
            if c <= i {
                return Some("un grupo tiene un hijo que no es posterior a él (no es preorden)");
            }
            if has_parent[c] {
                return Some("una partícula es hija de dos grupos o dos veces del mismo");
            }
            has_parent[c] = true;
        }
    }
    if has_parent.iter().skip(1).any(|&p| !p) {
        return Some("una partícula no es hija de ningún grupo");
    }
    // Ya es un árbol con raíz en 0 cuyos hijos son posteriores a su grupo
    // (un orden topológico). Es el preorden si, además, el primer hijo de
    // cada grupo es la partícula siguiente y cada hijo empieza justo donde
    // acaba el subárbol del anterior. Los tamaños de subárbol se calculan de
    // atrás hacia delante: los hijos tienen índice mayor, así que ya están.
    // O(n), sin recursión (D7).
    let mut size = vec![1usize; ps.len()];
    for i in (0..ps.len()).rev() {
        if let Term::Sequence(c) | Term::Choice(c) | Term::All(c) = &ps[i].term {
            size[i] += c.iter().map(|&c| size[c]).sum::<usize>();
        }
    }
    for (i, p) in ps.iter().enumerate() {
        if let Term::Sequence(c) | Term::Choice(c) | Term::All(c) = &p.term {
            let mut next = i + 1;
            for &c in c {
                if c != next {
                    return Some(
                        "las partículas no están en preorden (un hijo no empieza donde acaba \
                         el subárbol del anterior)",
                    );
                }
                next += size[c];
            }
        }
    }
    None
}

/// I9: el literal es exactamente un valor JSON en preorden, con las claves
/// de cada objeto en orden estricto (ordenadas y sin repetir) y números de
/// RFC 8259. Pila explícita de marcos (D7).
fn literal_problem(lit: &Literal) -> Option<&'static str> {
    struct Frame<'a> {
        /// Valores (o miembros) que faltan.
        remaining: usize,
        object: bool,
        /// En un objeto: lo siguiente es una clave.
        expect_key: bool,
        last_key: Option<&'a str>,
    }
    let mut stack = vec![Frame {
        remaining: 1,
        object: false,
        expect_key: false,
        last_key: None,
    }];
    for tok in &lit.0 {
        let Some(f) = stack.last_mut() else {
            return Some("sobran tokens después del valor");
        };
        if f.object && f.expect_key {
            let LitToken::Key(k) = tok else {
                return Some("falta la clave de un miembro de objeto");
            };
            if f.last_key.is_some_and(|prev| prev >= k.as_str()) {
                return Some("las claves de un objeto deben ir ordenadas y sin repetir");
            }
            f.last_key = Some(k);
            f.expect_key = false;
            continue;
        }
        f.remaining -= 1;
        f.expect_key = f.object;
        match tok {
            LitToken::Key(_) => return Some("clave fuera de un objeto"),
            LitToken::Number(l) if !is_rfc8259_number(l) => {
                return Some("número que no tiene la sintaxis de RFC 8259")
            }
            LitToken::Array(n) => stack.push(Frame {
                remaining: *n,
                object: false,
                expect_key: false,
                last_key: None,
            }),
            LitToken::Object(n) => stack.push(Frame {
                remaining: *n,
                object: true,
                expect_key: true,
                last_key: None,
            }),
            _ => {}
        }
        while stack.last().is_some_and(|f| f.remaining == 0) {
            stack.pop();
        }
    }
    if stack.is_empty() {
        None
    } else {
        Some("faltan tokens: el valor está incompleto")
    }
}

/// Un literal de un solo token del tipo que corresponde al espacio léxico.
fn scalar_of(lit: &Literal, lx: Lexical) -> bool {
    match lit.0.as_slice() {
        [LitToken::String(_)] => lx == Lexical::String,
        // Ningún texto admite un booleano en una enumeración (`xs:boolean` no
        // tiene la faceta, ver `text`).
        [LitToken::Bool(_)] => false,
        [LitToken::Number(l)] => match lx {
            Lexical::Integer => is_integer(l),
            _ => lx.is_numeric(),
        },
        _ => false,
    }
}

/// `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?` (RFC 8259 §6).
fn is_rfc8259_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i > start
    };
    if b.first() == Some(&b'-') {
        i += 1;
    }
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            digits(&mut i);
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if !digits(&mut i) {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if !digits(&mut i) {
            return false;
        }
    }
    i == b.len()
}

/// Si un lexema de RFC 8259 es un entero (parte fraccionaria cero, como en
/// 2020-12 y en `semantic::number`): el valor es D × 10ˢ con D las cifras de
/// la mantisa sin ceros a la derecha; es entero si D es cero o s ≥ 0. El
/// exponente se satura en `i64` (con más de 18 cifras el signo de s no
/// cambia). Un lexema que no es de RFC 8259 no es entero.
fn is_integer(s: &str) -> bool {
    if !is_rfc8259_number(s) {
        return false;
    }
    let (mantissa, exp) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, "0"),
    };
    let (neg, digits) = match exp.as_bytes().first() {
        Some(b'-') => (true, &exp[1..]),
        Some(b'+') => (false, &exp[1..]),
        _ => (false, exp),
    };
    let mut e: i64 = 0;
    for d in digits.bytes() {
        e = e.saturating_mul(10).saturating_add(i64::from(d - b'0'));
    }
    if neg {
        e = -e;
    }
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let all: Vec<u8> = int
        .bytes()
        .filter(u8::is_ascii_digit)
        .chain(frac.bytes())
        .collect();
    let trailing = all.iter().rev().take_while(|&&c| c == b'0').count();
    if trailing == all.len() {
        return true; // cero
    }
    // s = exponente − cifras decimales + ceros quitados a la derecha.
    let frac_len = i64::try_from(frac.len()).unwrap_or(i64::MAX);
    let trailing = i64::try_from(trailing).unwrap_or(i64::MAX);
    e.saturating_sub(frac_len).saturating_add(trailing) >= 0
}

/// Un lexema de RFC 8259 es > 0 si no tiene signo y su mantisa tiene alguna
/// cifra distinta de cero.
fn is_positive(s: &str) -> bool {
    !s.starts_with('-')
        && s.bytes()
            .take_while(|&c| c != b'e' && c != b'E')
            .any(|c| (b'1'..=b'9').contains(&c))
}
