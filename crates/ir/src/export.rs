//! Exportación de una IR verificada: JSON estable (`schemac ir`, y la web de
//! la Fase 7) y DOT (Graphviz, `schemac ir --dot` y `make docs`).
//!
//! Las dos salidas son **deterministas**: solo dependen de la IR (nodos en
//! orden de `NodeId`, reglas y destinos en su orden), nunca de tablas hash.
//! Las dos son O(tamaño de la IR): no se escribe la ruta de cada nodo
//! (`Ir::location`), que sumaría las longitudes de todas las rutas
//! (cuadrático en el peor caso, `docs/fase3/ir.md` §3), sino su padre y su
//! paso. Sin recursión (D7): los literales se escriben como su lista plana
//! de tokens. Formato: `docs/fase3/bajada.md` §5.
//!
//! Desde la versión 2 del JSON, cada nodo y cada regla llevan su clase
//! (a)/(b)/(c) y su estrategia ([`classify`], `docs/fase3/clasificacion.md`
//! §5), que no se guardan en la IR sino que se derivan al exportar.

use core::fmt::{self, Write};

use lexer::short;

use crate::{
    classify, Attribute, Class, ContentModel, Format, JsonType, Lexical, LitToken, Literal, Node,
    NodeClass, NodeId, Rule, Sort, Span, Step, StrategySet, Term, VerifiedIr,
};

/// Versión del formato JSON; cambia si cambia la forma de la salida. La 2
/// (tarea 4.3) añade la clase y la estrategia de cada nodo y de cada regla
/// y el recuento por clase de la cabecera.
pub const IR_JSON_VERSION: u32 = 2;

/// Escribe la IR en el JSON de `schemac ir`: un objeto con la cabecera
/// (versión, formato, recuentos) y un objeto por nodo con una regla por
/// línea.
pub fn write_json(ir: &VerifiedIr, w: &mut impl Write) -> fmt::Result {
    let classes = classify(ir);
    writeln!(w, "{{")?;
    writeln!(w, "  \"irVersion\": {IR_JSON_VERSION},")?;
    match ir.format() {
        Format::Json => {
            writeln!(w, "  \"format\": \"json\",")?;
            writeln!(w, "  \"targetNamespace\": null,")?;
        }
        Format::Xml { target_namespace } => {
            writeln!(w, "  \"format\": \"xml\",")?;
            write!(w, "  \"targetNamespace\": ")?;
            opt_str(w, target_namespace.as_deref())?;
            writeln!(w, ",")?;
        }
    }
    writeln!(w, "  \"nodeCount\": {},", ir.nodes().len())?;
    writeln!(w, "  \"edgeCount\": {},", ir.edge_count())?;
    writeln!(
        w,
        "  \"classCount\": {{\"a\": {}, \"b\": {}, \"c\": {}}},",
        classes.count(Class::A),
        classes.count(Class::B),
        classes.count(Class::C)
    )?;
    write!(w, "  \"nodes\": [")?;
    for (i, node) in ir.nodes().iter().enumerate() {
        w.write_str(if i == 0 { "\n" } else { ",\n" })?;
        json_node(w, i, node, classes.nodes()[i])?;
    }
    writeln!(w, "\n  ]")?;
    writeln!(w, "}}")
}

fn json_node(w: &mut impl Write, i: usize, node: &Node, class: NodeClass) -> fmt::Result {
    writeln!(w, "    {{")?;
    writeln!(w, "      \"id\": {i},")?;
    writeln!(w, "      \"sort\": \"{}\",", sort_key(node.sort))?;
    write!(w, "      \"span\": ")?;
    span(w, node.origin.span)?;
    writeln!(w, ",")?;
    write!(w, "      \"parent\": ")?;
    opt_id(w, node.origin.parent)?;
    writeln!(w, ",")?;
    write!(w, "      \"step\": ")?;
    match &node.origin.step {
        Step::Root => w.write_str("null")?,
        Step::Keyword(k) => {
            w.write_str("{\"keyword\": ")?;
            string(w, k)?;
            w.write_str("}")?;
        }
        Step::Entry(k, e) => {
            w.write_str("{\"keyword\": ")?;
            string(w, k)?;
            w.write_str(", \"entry\": ")?;
            string(w, e)?;
            w.write_str("}")?;
        }
        Step::Index(k, n) => {
            w.write_str("{\"keyword\": ")?;
            string(w, k)?;
            write!(w, ", \"index\": {n}}}")?;
        }
    }
    writeln!(w, ",")?;
    writeln!(w, "      \"class\": \"{}\",", class.class.key())?;
    write!(w, "      \"strategies\": ")?;
    strategies(w, class.strategies)?;
    writeln!(w, ",")?;
    if node.rules.is_empty() {
        writeln!(w, "      \"rules\": []")?;
    } else {
        write!(w, "      \"rules\": [")?;
        for (j, c) in node.rules.iter().enumerate() {
            w.write_str(if j == 0 { "\n        " } else { ",\n        " })?;
            write!(w, "{{\"rule\": \"{}\", \"span\": ", c.rule.kind().key())?;
            span(w, c.span)?;
            let a = c.rule.assignment();
            write!(
                w,
                ", \"class\": \"{}\", \"strategy\": \"{}\"",
                a.class.key(),
                a.strategy.key()
            )?;
            json_rule(w, &c.rule)?;
            w.write_str("}")?;
        }
        writeln!(w, "\n      ]")?;
    }
    write!(w, "    }}")
}

/// Los campos propios de cada regla, tras `"rule"` y `"span"`.
fn json_rule(w: &mut impl Write, rule: &Rule) -> fmt::Result {
    match rule {
        Rule::Reject | Rule::UniqueItems => Ok(()),
        Rule::Ref(t) | Rule::Not(t) | Rule::SimpleContent(t) => {
            write!(w, ", \"target\": {}", t.index())
        }
        Rule::Type(set) => {
            w.write_str(", \"types\": [")?;
            let names = JsonType::ALL.iter().filter(|&&t| set.contains(t));
            for (i, &t) in names.enumerate() {
                if i > 0 {
                    w.write_str(", ")?;
                }
                write!(w, "\"{}\"", type_key(t))?;
            }
            w.write_str("]")
        }
        Rule::Enum(lits) => {
            w.write_str(", \"values\": [")?;
            for (i, l) in lits.iter().enumerate() {
                if i > 0 {
                    w.write_str(", ")?;
                }
                literal(w, l)?;
            }
            w.write_str("]")
        }
        Rule::Const(l) => {
            w.write_str(", \"value\": ")?;
            literal(w, l)
        }
        // El lexema exacto, como cadena: un lector JSON lo convertiría a
        // `f64` si fuera un número.
        Rule::Minimum(n)
        | Rule::Maximum(n)
        | Rule::ExclusiveMinimum(n)
        | Rule::ExclusiveMaximum(n)
        | Rule::MultipleOf(n) => {
            w.write_str(", \"value\": ")?;
            string(w, &n.lexeme)
        }
        Rule::MinLength(n)
        | Rule::MaxLength(n)
        | Rule::MinItems(n)
        | Rule::MaxItems(n)
        | Rule::MinProperties(n)
        | Rule::MaxProperties(n) => write!(w, ", \"value\": {n}"),
        Rule::Pattern(p) => {
            w.write_str(", \"source\": ")?;
            string(w, p)
        }
        Rule::Items { prefix, rest } => {
            w.write_str(", \"prefix\": ")?;
            ids(w, prefix)?;
            w.write_str(", \"rest\": ")?;
            opt_id(w, *rest)
        }
        Rule::Properties {
            declared,
            additional,
        } => {
            w.write_str(", \"declared\": ")?;
            named(w, declared)?;
            w.write_str(", \"additional\": ")?;
            opt_id(w, *additional)
        }
        Rule::Required(names) => {
            w.write_str(", \"names\": ")?;
            strings(w, names)
        }
        Rule::DependentRequired(m) => {
            w.write_str(", \"entries\": [")?;
            for (i, (k, l)) in m.iter().enumerate() {
                if i > 0 {
                    w.write_str(", ")?;
                }
                w.write_str("{\"name\": ")?;
                string(w, k)?;
                w.write_str(", \"requires\": ")?;
                strings(w, l)?;
                w.write_str("}")?;
            }
            w.write_str("]")
        }
        Rule::DependentSchemas(m) => {
            w.write_str(", \"entries\": ")?;
            named(w, m)
        }
        Rule::AllOf(v) | Rule::AnyOf(v) | Rule::OneOf(v) => {
            w.write_str(", \"targets\": ")?;
            ids(w, v)
        }
        Rule::Attributes(attrs) => {
            w.write_str(", \"attributes\": [")?;
            for (i, a) in attrs.iter().enumerate() {
                if i > 0 {
                    w.write_str(", ")?;
                }
                attribute(w, a)?;
            }
            w.write_str("]")
        }
        Rule::Content(m) => content(w, m),
        Rule::Lexical(l) => write!(w, ", \"lexical\": \"{}\"", lexical_key(*l)),
    }
}

fn attribute(w: &mut impl Write, a: &Attribute) -> fmt::Result {
    w.write_str("{\"name\": ")?;
    string(w, &a.name)?;
    write!(
        w,
        ", \"required\": {}, \"type\": {}",
        a.required,
        a.ty.index()
    )?;
    w.write_str(", \"default\": ")?;
    opt_str(w, a.default.as_deref())?;
    w.write_str(", \"fixed\": ")?;
    opt_str(w, a.fixed.as_deref())?;
    w.write_str(", \"span\": ")?;
    span(w, a.span)?;
    w.write_str("}")
}

fn content(w: &mut impl Write, m: &ContentModel) -> fmt::Result {
    w.write_str(", \"particles\": [")?;
    for (i, p) in m.particles.iter().enumerate() {
        if i > 0 {
            w.write_str(", ")?;
        }
        match &p.term {
            Term::Element { name, ty } => {
                w.write_str("{\"term\": \"element\", \"name\": ")?;
                string(w, name)?;
                write!(w, ", \"type\": {}", ty.index())?;
            }
            Term::Sequence(c) | Term::Choice(c) | Term::All(c) => {
                let term = match p.term {
                    Term::Sequence(_) => "sequence",
                    Term::Choice(_) => "choice",
                    _ => "all",
                };
                write!(w, "{{\"term\": \"{term}\", \"children\": [")?;
                for (j, k) in c.iter().enumerate() {
                    if j > 0 {
                        w.write_str(", ")?;
                    }
                    write!(w, "{k}")?;
                }
                w.write_str("]")?;
            }
        }
        write!(w, ", \"min\": {}, \"max\": ", p.occurs.min)?;
        match p.occurs.max {
            Some(n) => write!(w, "{n}")?,
            None => w.write_str("null")?,
        }
        w.write_str(", \"span\": ")?;
        span(w, p.span)?;
        w.write_str("}")?;
    }
    w.write_str("]")
}

/// Un literal como su **lista de tokens en preorden** (la forma de la IR,
/// I9): un arreglo JSON de tokens, cada uno un arreglo con su etiqueta
/// (`null`, `bool`, `number`, `string`, `array`, `object`, `key`) y su dato.
/// Los números van como **lexema en una cadena** (`["number", "1e400"]`),
/// así que ningún lector JSON los convierte a `f64` ni los pierde; y la
/// lista es plana, así que un lector no necesita recursión aunque el valor
/// esté muy anidado. `bajada.md` §5.1.
fn literal(w: &mut impl Write, l: &Literal) -> fmt::Result {
    w.write_str("[")?;
    for (i, tok) in l.0.iter().enumerate() {
        if i > 0 {
            w.write_str(", ")?;
        }
        match tok {
            LitToken::Null => w.write_str("[\"null\"]")?,
            LitToken::Bool(b) => write!(w, "[\"bool\", {b}]")?,
            LitToken::Number(n) => {
                w.write_str("[\"number\", ")?;
                string(w, n)?;
                w.write_str("]")?;
            }
            LitToken::String(t) => {
                w.write_str("[\"string\", ")?;
                string(w, t)?;
                w.write_str("]")?;
            }
            LitToken::Array(n) => write!(w, "[\"array\", {n}]")?,
            LitToken::Object(n) => write!(w, "[\"object\", {n}]")?,
            LitToken::Key(k) => {
                w.write_str("[\"key\", ")?;
                string(w, k)?;
                w.write_str("]")?;
            }
        }
    }
    w.write_str("]")
}

fn strategies(w: &mut impl Write, set: StrategySet) -> fmt::Result {
    w.write_str("[")?;
    for (i, s) in set.iter().enumerate() {
        if i > 0 {
            w.write_str(", ")?;
        }
        write!(w, "\"{}\"", s.key())?;
    }
    w.write_str("]")
}

fn span(w: &mut impl Write, s: Span) -> fmt::Result {
    write!(
        w,
        "{{\"line\": {}, \"column\": {}, \"start\": {}, \"end\": {}}}",
        s.start.line, s.start.column, s.start.offset, s.end
    )
}

fn ids(w: &mut impl Write, v: &[NodeId]) -> fmt::Result {
    w.write_str("[")?;
    for (i, t) in v.iter().enumerate() {
        if i > 0 {
            w.write_str(", ")?;
        }
        write!(w, "{}", t.index())?;
    }
    w.write_str("]")
}

fn named(w: &mut impl Write, v: &[(String, NodeId)]) -> fmt::Result {
    w.write_str("[")?;
    for (i, (k, t)) in v.iter().enumerate() {
        if i > 0 {
            w.write_str(", ")?;
        }
        w.write_str("{\"name\": ")?;
        string(w, k)?;
        write!(w, ", \"target\": {}}}", t.index())?;
    }
    w.write_str("]")
}

fn strings(w: &mut impl Write, v: &[String]) -> fmt::Result {
    w.write_str("[")?;
    for (i, s) in v.iter().enumerate() {
        if i > 0 {
            w.write_str(", ")?;
        }
        string(w, s)?;
    }
    w.write_str("]")
}

fn opt_id(w: &mut impl Write, t: Option<NodeId>) -> fmt::Result {
    match t {
        Some(t) => write!(w, "{}", t.index()),
        None => w.write_str("null"),
    }
}

fn opt_str(w: &mut impl Write, s: Option<&str>) -> fmt::Result {
    match s {
        Some(s) => string(w, s),
        None => w.write_str("null"),
    }
}

/// Cadena JSON (RFC 8259 §7): comillas, barra inversa y controles
/// escapados; lo demás, tal cual en UTF-8.
fn string(w: &mut impl Write, s: &str) -> fmt::Result {
    w.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => w.write_str("\\\"")?,
            '\\' => w.write_str("\\\\")?,
            '\n' => w.write_str("\\n")?,
            '\r' => w.write_str("\\r")?,
            '\t' => w.write_str("\\t")?,
            c if u32::from(c) < 0x20 => write!(w, "\\u{:04x}", u32::from(c))?,
            c => w.write_char(c)?,
        }
    }
    w.write_char('"')
}

fn sort_key(s: Sort) -> &'static str {
    match s {
        Sort::Json => "json",
        Sort::Element => "element",
        Sort::Text => "text",
    }
}

fn type_key(t: JsonType) -> &'static str {
    match t {
        JsonType::Null => "null",
        JsonType::Boolean => "boolean",
        JsonType::Object => "object",
        JsonType::Array => "array",
        JsonType::Number => "number",
        JsonType::Integer => "integer",
        JsonType::String => "string",
    }
}

fn lexical_key(l: Lexical) -> &'static str {
    match l {
        Lexical::String => "string",
        Lexical::Boolean => "boolean",
        Lexical::Decimal => "decimal",
        Lexical::Integer => "integer",
        Lexical::Float => "float",
        Lexical::Double => "double",
    }
}

// --- DOT -------------------------------------------------------------------

/// Escribe la IR como grafo DOT: un nodo por nodo de la IR (su número, su
/// paso desde el padre y las claves de sus reglas; la raíz con doble borde)
/// y una arista por destino de cada regla, etiquetada con la palabra clave y
/// el nombre o índice. Las aristas de E₌ (misma instancia, I14) van
/// discontinuas y las que consumen entrada, continuas: la recursión
/// productiva es un ciclo con al menos una arista continua. La última línea
/// de cada nodo es su clase y sus estrategias ([`classify`]); los nodos (c)
/// llevan el borde grueso. El texto del usuario va recortado y escapado con
/// [`lexer::short`].
pub fn write_dot(ir: &VerifiedIr, w: &mut impl Write) -> fmt::Result {
    let classes = classify(ir);
    let format = match ir.format() {
        Format::Json => "json",
        Format::Xml { .. } => "xml",
    };
    writeln!(w, "digraph ir {{")?;
    writeln!(w, "  rankdir=LR;")?;
    writeln!(
        w,
        "  label=\"IR ({format}): {} nodos, {} aristas; clases: {} (a), {} (b), {} (c)\\n\
         (continuas: consumen entrada; discontinuas: misma instancia)\";",
        ir.nodes().len(),
        ir.edge_count(),
        classes.count(Class::A),
        classes.count(Class::B),
        classes.count(Class::C)
    )?;
    writeln!(w, "  labelloc=t;")?;
    writeln!(w, "  node [shape=box, fontname=\"Helvetica\"];")?;
    writeln!(w, "  edge [fontname=\"Helvetica\", fontsize=10];")?;
    for (i, node) in ir.nodes().iter().enumerate() {
        write!(w, "  n{i} [label=\"{i} ")?;
        match &node.origin.step {
            Step::Root => w.write_str("(raíz)")?,
            Step::Keyword(k) => dot_text(w, k)?,
            Step::Entry(k, e) => {
                dot_text(w, k)?;
                w.write_str("/")?;
                dot_text(w, e)?;
            }
            Step::Index(k, n) => {
                dot_text(w, k)?;
                write!(w, "/{n}")?;
            }
        }
        if node.sort != Sort::Json {
            write!(w, " [{}]", sort_key(node.sort))?;
        }
        if !node.rules.is_empty() {
            w.write_str("\\n")?;
            for (j, c) in node.rules.iter().enumerate() {
                if j > 0 {
                    w.write_str(", ")?;
                }
                dot_text(w, c.rule.kind().key())?;
            }
        }
        let class = classes.nodes()[i];
        dot_class(w, class)?;
        w.write_str("\"")?;
        if i == 0 {
            w.write_str(", peripheries=2")?;
        }
        if class.class == Class::C {
            w.write_str(", style=bold")?;
        }
        writeln!(w, "];")?;
    }
    for (i, node) in ir.nodes().iter().enumerate() {
        for c in &node.rules {
            dot_edges(w, i, &c.rule)?;
        }
    }
    writeln!(w, "}}")
}

/// La línea de la clase en la etiqueta de un nodo: `\n(c) AFD, producto
/// acotado`, o solo `\n(a)` si no tiene estrategias.
fn dot_class(w: &mut impl Write, c: NodeClass) -> fmt::Result {
    write!(w, "\\n{}", c.class.label())?;
    for (i, s) in c.strategies.iter().enumerate() {
        w.write_str(if i == 0 { " " } else { ", " })?;
        w.write_str(s.name())?;
    }
    Ok(())
}

/// Las aristas de una regla, en el orden de [`Rule::targets`].
fn dot_edges(w: &mut impl Write, from: usize, rule: &Rule) -> fmt::Result {
    let kind = rule.kind();
    let mut edge = |to: NodeId, label: &dyn Fn(&mut dyn Write) -> fmt::Result| -> fmt::Result {
        write!(w, "  n{from} -> n{} [label=\"", to.index())?;
        label(w)?;
        w.write_str("\"")?;
        if kind.same_instance() {
            w.write_str(", style=dashed")?;
        }
        writeln!(w, "];")
    };
    let key = kind.key();
    match rule {
        Rule::Ref(t) | Rule::Not(t) | Rule::SimpleContent(t) => edge(*t, &|w| w.write_str(key)),
        Rule::Items { prefix, rest } => {
            for (i, t) in prefix.iter().enumerate() {
                edge(*t, &|w| write!(w, "prefixItems/{i}"))?;
            }
            match rest {
                Some(t) => edge(*t, &|w| w.write_str("items")),
                None => Ok(()),
            }
        }
        Rule::Properties {
            declared,
            additional,
        } => {
            for (k, t) in declared {
                edge(*t, &|w| {
                    w.write_str("properties/")?;
                    dot_text(w, k)
                })?;
            }
            match additional {
                Some(t) => edge(*t, &|w| w.write_str("additionalProperties")),
                None => Ok(()),
            }
        }
        Rule::DependentSchemas(m) => {
            for (k, t) in m {
                edge(*t, &|w| {
                    w.write_str("dependentSchemas/")?;
                    dot_text(w, k)
                })?;
            }
            Ok(())
        }
        Rule::AllOf(v) | Rule::AnyOf(v) | Rule::OneOf(v) => {
            for (i, t) in v.iter().enumerate() {
                edge(*t, &|w| write!(w, "{key}/{i}"))?;
            }
            Ok(())
        }
        Rule::Attributes(attrs) => {
            for a in attrs {
                edge(a.ty, &|w| {
                    w.write_str("@")?;
                    dot_text(w, &a.name)
                })?;
            }
            Ok(())
        }
        Rule::Content(m) => {
            for p in &m.particles {
                if let Term::Element { name, ty } = &p.term {
                    edge(*ty, &|w| {
                        w.write_str("<")?;
                        dot_text(w, name)?;
                        w.write_str(">")
                    })?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Texto en una etiqueta DOT entre comillas: recortado y escapado con
/// `short` (una línea, ≤ 32 caracteres) y después con los escapes de DOT
/// (`\` y `"`).
fn dot_text(w: &mut (impl Write + ?Sized), s: &str) -> fmt::Result {
    let mut buf = String::new();
    write!(buf, "{}", short(s))?;
    for c in buf.chars() {
        match c {
            '\\' => w.write_str("\\\\")?,
            '"' => w.write_str("\\\"")?,
            c => w.write_char(c)?,
        }
    }
    Ok(())
}
