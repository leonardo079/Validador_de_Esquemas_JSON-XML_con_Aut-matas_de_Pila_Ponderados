//! Bajada de JSON Schema a la IR común (Fase 3, tarea 4.2).
//!
//! Entrada: la tabla de símbolos de un esquema que ya pasó [`crate::analyze`]
//! (meta-esquema, `$ref` resueltos, sin ciclos no productivos). Salida: la
//! IR verificada ([`ir::VerifiedIr`]) o, si la IR que sale no cumple sus
//! invariantes, un único [`ir::InternalError`] (`docs/fase3/ir.md` §4,
//! «Contrato»; D15).
//!
//! La bajada es 1:1 con la tabla (`docs/fase3/ir.md` §5.1): cada subesquema
//! es el nodo con su mismo número (`NodeId` = `SchemaId`), también las
//! entradas de `$defs` que nadie referencia (D13), con su rango, su padre y
//! su paso; cada palabra clave que valida es una regla; un `$ref` es la regla
//! `Ref` con el destino que resolvió la tabla, una arista y no una copia.
//!
//! **Sin recursión (D7).** Los nodos se recorren con un bucle en el orden de
//! la tabla. Los hijos de un nodo son los nodos cuyo padre es él, y en el
//! preorden aparecen en el orden de sus palabras clave y entradas; así que se
//! agrupan por padre una vez (conteo, O(n)) y, al bajar un nodo, cada palabra
//! clave con subesquemas toma los siguientes de su grupo. Los literales de
//! `enum`/`const` se aplanan con una pila explícita.
//!
//! **Costo.** O(n) en el tamaño del AST, salvo la ordenación de las claves de
//! los objetos literales (I9), O(k log k) por objeto de k miembros. Teoría:
//! `docs/fase3/bajada.md`.

use ir::{
    Constraint, Format, InternalError, Ir, JsonType, LitToken, Literal, Node, NodeId, Num, Origin,
    Rule, Sort, Span, Step, TypeSet, VerifiedIr,
};
use schema_parser::{Json, JsonKind, Keyword, Kw, SchemaKind, Str, Value};

use crate::number::to_u64_saturating;
use crate::symbols::{self, SymbolTable};

/// `NodeId` imposible: lo usa la bajada si la tabla no le da un destino (un
/// `$ref` sin resolver, un hijo que falta), cosa que no ocurre con una tabla
/// de [`crate::analyze`]. En lugar de hacer pánico deja una arista colgante,
/// que `verify` rechaza (I2) y se presenta como el error interno.
const DANGLING: NodeId = NodeId::new(usize::MAX);

/// Baja el esquema de `table` a la IR y la verifica.
///
/// **Exige una tabla que ya pasó [`crate::analyze`]** (meta-esquema, `$ref`
/// resueltos y sin ciclos no productivos), aunque el tipo no lo imponga: la
/// de [`SymbolTable::build`] sola no basta. Con otra tabla, un error del
/// esquema del usuario (por ejemplo un ciclo `{"$ref": "#"}`, que rompe I14)
/// saldría como [`InternalError`] en lugar de como error del esquema con su
/// posición (`docs/fase3/ir.md` §4). Un tipo `AnalyzedTable` que lo
/// garantice queda como mejora (`docs/fase3/pendientes.md`).
///
/// # Errors
///
/// [`InternalError`] si la IR no cumple los invariantes de
/// `docs/fase3/ir.md` §4: un fallo de la bajada, no del esquema.
pub fn lower(table: &SymbolTable<'_>) -> Result<VerifiedIr, InternalError> {
    build(table)
        .verify()
        .map_err(|(_, errors)| InternalError::new(errors))
}

/// La IR sin verificar.
fn build(table: &SymbolTable<'_>) -> Ir {
    let children = Children::new(table);
    let mut nodes = Vec::with_capacity(table.len());
    for sid in table.ids() {
        let t = table.node(sid);
        let (parent, step) = match t.parent {
            None => (None, Step::Root),
            Some((p, step)) => (Some(NodeId::new(p.index())), step_of(step)),
        };
        let mut kids = children.of(sid.index());
        let rules = match &t.schema.kind {
            SchemaKind::Bool(true) => Vec::new(),
            SchemaKind::Bool(false) => vec![Constraint {
                rule: Rule::Reject,
                span: t.schema.span,
            }],
            SchemaKind::Object(kws) => {
                let ref_target = t.ref_target.map(|r| NodeId::new(r.index()));
                rules_of(table, kws, ref_target, &mut kids)
            }
        };
        nodes.push(Node {
            sort: Sort::Json,
            rules,
            origin: Origin {
                span: t.schema.span,
                parent,
                step,
            },
        });
    }
    Ir {
        format: Format::Json,
        nodes,
    }
}

fn step_of(step: symbols::Step<'_>) -> Step {
    match step {
        symbols::Step::Keyword(k) => Step::Keyword(k.to_owned()),
        symbols::Step::Entry(k, e) => Step::Entry(k.to_owned(), e.to_owned()),
        symbols::Step::Index(k, i) => Step::Index(k.to_owned(), i),
    }
}

/// Hijos de cada nodo en formato CSR: los de `p` son
/// `ids[offsets[p]..offsets[p + 1]]`, en preorden.
struct Children {
    offsets: Vec<usize>,
    ids: Vec<usize>,
}

impl Children {
    fn new(table: &SymbolTable<'_>) -> Children {
        let n = table.len();
        let mut offsets = vec![0; n + 1];
        for t in table.nodes() {
            if let Some((p, _)) = t.parent {
                offsets[p.index() + 1] += 1;
            }
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut fill = offsets.clone();
        let mut ids = vec![0; offsets[n]];
        // En orden de id: cada grupo queda en preorden.
        for (i, t) in table.nodes().iter().enumerate() {
            if let Some((p, _)) = t.parent {
                ids[fill[p.index()]] = i;
                fill[p.index()] += 1;
            }
        }
        Children { offsets, ids }
    }

    fn of(&self, p: usize) -> std::slice::Iter<'_, usize> {
        self.ids[self.offsets[p]..self.offsets[p + 1]].iter()
    }
}

/// Siguiente hijo del nodo, si su paso desde el padre en la tabla es
/// `expected` (la palabra clave que lo toma y, en mapas y arreglos, su
/// entrada o índice). Si no quedan hijos o el siguiente no es el esperado
/// (los grupos estarían desalineados con las palabras clave), devuelve
/// [`DANGLING`]: la arista colgante acaba en el error interno (I2) en lugar
/// de apuntar en silencio a otro subesquema, que I2 e I5 no detectarían.
fn next(
    table: &SymbolTable<'_>,
    kids: &mut std::slice::Iter<'_, usize>,
    expected: symbols::Step<'_>,
) -> NodeId {
    let Some(&i) = kids.next() else {
        return DANGLING;
    };
    let step = table.id(i).and_then(|id| table.node(id).parent);
    match step {
        Some((_, step)) if step == expected => NodeId::new(i),
        _ => DANGLING,
    }
}

/// Rango de la palabra clave entera: desde su nombre hasta el final de su
/// valor.
fn span_of(kw: &Keyword) -> Span {
    Span {
        start: kw.name.span.start,
        end: kw.value_span.end,
    }
}

/// Reglas de un objeto-esquema, en el orden de sus palabras clave.
/// `prefixItems`/`items` y `properties`/`additionalProperties` son una
/// sola regla cada pareja (`ir.md` §5.1), en la posición de la primera
/// que aparece y con su rango.
fn rules_of(
    table: &SymbolTable<'_>,
    kws: &[Keyword],
    ref_target: Option<NodeId>,
    kids: &mut std::slice::Iter<'_, usize>,
) -> Vec<Constraint> {
    let mut rules: Vec<Constraint> = Vec::with_capacity(kws.len());
    // Posición en `rules` de la regla `Items` y de la `Properties`.
    let mut items_at: Option<usize> = None;
    let mut props_at: Option<usize> = None;
    for kw in kws {
        let span = span_of(kw);
        let name = kw.name.value.as_str();
        // Hijos de la palabra clave, en orden: tantos como subesquemas
        // tenga su valor. Se toman siempre, aunque la palabra clave no
        // produzca regla (`$defs`), para no desalinear los siguientes.
        let sub: Vec<NodeId> = match &kw.value {
            Value::Schema(_) => vec![next(table, kids, symbols::Step::Keyword(name))],
            Value::SchemaMap(m) => m
                .iter()
                .map(|(k, _)| next(table, kids, symbols::Step::Entry(name, &k.value)))
                .collect(),
            Value::SchemaList(l) => (0..l.len())
                .map(|i| next(table, kids, symbols::Step::Index(name, i)))
                .collect(),
            _ => Vec::new(),
        };
        let first = || sub.first().copied().unwrap_or(DANGLING);
        let names = |m: &[(Str, schema_parser::Schema)]| -> Vec<(String, NodeId)> {
            m.iter()
                .zip(&sub)
                .map(|((k, _), &t)| (k.value.clone(), t))
                .collect()
        };
        let rule = match (kw.kw, &kw.value) {
            (Kw::Ref, _) => Some(Rule::Ref(ref_target.unwrap_or(DANGLING))),
            (Kw::Type, Value::Str(s)) => Some(Rule::Type(type_set(std::slice::from_ref(s)))),
            (Kw::Type, Value::Strings(l)) => Some(Rule::Type(type_set(l))),
            (Kw::Enum, Value::JsonList(l)) => Some(Rule::Enum(l.iter().map(literal).collect())),
            (Kw::Const, Value::Json(j)) => Some(Rule::Const(literal(j))),
            (Kw::Minimum, Value::Num(n)) => Some(Rule::Minimum(Num::new(&n.lexeme))),
            (Kw::Maximum, Value::Num(n)) => Some(Rule::Maximum(Num::new(&n.lexeme))),
            (Kw::ExclusiveMinimum, Value::Num(n)) => {
                Some(Rule::ExclusiveMinimum(Num::new(&n.lexeme)))
            }
            (Kw::ExclusiveMaximum, Value::Num(n)) => {
                Some(Rule::ExclusiveMaximum(Num::new(&n.lexeme)))
            }
            (Kw::MultipleOf, Value::Num(n)) => Some(Rule::MultipleOf(Num::new(&n.lexeme))),
            (Kw::MinLength, Value::Num(n)) => Some(Rule::MinLength(to_u64_saturating(&n.lexeme))),
            (Kw::MaxLength, Value::Num(n)) => Some(Rule::MaxLength(to_u64_saturating(&n.lexeme))),
            (Kw::Pattern, Value::Str(s)) => Some(Rule::Pattern(s.value.clone())),
            (Kw::MinItems, Value::Num(n)) => Some(Rule::MinItems(to_u64_saturating(&n.lexeme))),
            (Kw::MaxItems, Value::Num(n)) => Some(Rule::MaxItems(to_u64_saturating(&n.lexeme))),
            (Kw::UniqueItems, Value::Bool(true)) => Some(Rule::UniqueItems),
            (Kw::Required, Value::Strings(l)) => {
                Some(Rule::Required(l.iter().map(|s| s.value.clone()).collect()))
            }
            (Kw::MinProperties, Value::Num(n)) => {
                Some(Rule::MinProperties(to_u64_saturating(&n.lexeme)))
            }
            (Kw::MaxProperties, Value::Num(n)) => {
                Some(Rule::MaxProperties(to_u64_saturating(&n.lexeme)))
            }
            (Kw::DependentRequired, Value::StringMap(m)) => Some(Rule::DependentRequired(
                m.iter()
                    .map(|(k, l)| (k.value.clone(), l.iter().map(|s| s.value.clone()).collect()))
                    .collect(),
            )),
            (Kw::DependentSchemas, Value::SchemaMap(m)) => Some(Rule::DependentSchemas(names(m))),
            (Kw::AllOf, _) => Some(Rule::AllOf(sub.clone())),
            (Kw::AnyOf, _) => Some(Rule::AnyOf(sub.clone())),
            (Kw::OneOf, _) => Some(Rule::OneOf(sub.clone())),
            (Kw::Not, _) => Some(Rule::Not(first())),
            // Reglas dobles: se crean con la primera palabra clave de la
            // pareja y la segunda rellena su parte.
            (Kw::Items | Kw::PrefixItems, _) => {
                let at = *items_at.get_or_insert_with(|| {
                    rules.push(Constraint {
                        rule: Rule::Items {
                            prefix: Vec::new(),
                            rest: None,
                        },
                        span,
                    });
                    rules.len() - 1
                });
                if let Rule::Items { prefix, rest } = &mut rules[at].rule {
                    if kw.kw == Kw::Items {
                        *rest = Some(first());
                    } else {
                        prefix.clone_from(&sub);
                    }
                }
                None
            }
            (Kw::Properties | Kw::AdditionalProperties, _) => {
                let at = *props_at.get_or_insert_with(|| {
                    rules.push(Constraint {
                        rule: Rule::Properties {
                            declared: Vec::new(),
                            additional: None,
                        },
                        span,
                    });
                    rules.len() - 1
                });
                if let Rule::Properties {
                    declared,
                    additional,
                } = &mut rules[at].rule
                {
                    match &kw.value {
                        Value::SchemaMap(m) => *declared = names(m),
                        _ => *additional = Some(first()),
                    }
                }
                None
            }
            // `$schema`, `$id`, `$anchor`, `$defs`/`definitions` (sus
            // entradas ya son nodos), anotaciones, `format` y
            // `uniqueItems: false`: nada (`ir.md` §5.1).
            _ => None,
        };
        if let Some(rule) = rule {
            rules.push(Constraint { rule, span });
        }
    }
    rules
}

/// Conjunto de tipos de `type`. Un nombre desconocido (el meta-esquema ya lo
/// rechazó) no suma nada; si ninguno es válido, el conjunto vacío lo
/// rechaza `verify` (I7).
fn type_set(names: &[Str]) -> TypeSet {
    let types: Vec<JsonType> = names
        .iter()
        .filter_map(|s| {
            Some(match s.value.as_str() {
                "null" => JsonType::Null,
                "boolean" => JsonType::Boolean,
                "object" => JsonType::Object,
                "array" => JsonType::Array,
                "number" => JsonType::Number,
                "integer" => JsonType::Integer,
                "string" => JsonType::String,
                _ => return None,
            })
        })
        .collect();
    TypeSet::of(&types)
}

/// Un valor JSON como lista de tokens en preorden con las claves de cada
/// objeto ordenadas por punto de código (I9; el orden de bytes de UTF-8 es
/// el de los puntos de código). Pila explícita (D7). El parser ya rechazó
/// las claves repetidas (D2).
fn literal(value: &Json) -> Literal {
    enum Item<'a> {
        Value(&'a Json),
        Key(&'a str),
    }
    let mut out = Vec::new();
    let mut stack = vec![Item::Value(value)];
    while let Some(item) = stack.pop() {
        let v = match item {
            Item::Key(k) => {
                out.push(LitToken::Key(k.to_owned()));
                continue;
            }
            Item::Value(v) => v,
        };
        match &v.kind {
            JsonKind::Null => out.push(LitToken::Null),
            JsonKind::Bool(b) => out.push(LitToken::Bool(*b)),
            JsonKind::Number(n) => out.push(LitToken::Number(n.clone())),
            JsonKind::String(s) => out.push(LitToken::String(s.clone())),
            JsonKind::Array(items) => {
                out.push(LitToken::Array(items.len()));
                stack.extend(items.iter().rev().map(Item::Value));
            }
            JsonKind::Object(members) => {
                out.push(LitToken::Object(members.len()));
                let mut sorted: Vec<&(Str, Json)> = members.iter().collect();
                sorted.sort_unstable_by(|a, b| a.0.value.cmp(&b.0.value));
                for (k, v) in sorted.into_iter().rev() {
                    stack.push(Item::Value(v));
                    stack.push(Item::Key(&k.value));
                }
            }
        }
    }
    Literal(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CR-S-1: un hijo cuyo paso no es el que espera la palabra clave (los
    /// grupos desalineados) da [`DANGLING`], no el id de otro subesquema.
    /// No hay forma de construir una tabla desalineada sin API nueva, así
    /// que se prueba `next` con expectativas equivocadas sobre una tabla
    /// real.
    #[test]
    fn hijo_con_otro_paso_es_colgante() {
        let schema =
            schema_parser::parse(br#"{"not": {}, "properties": {"a": {}}, "allOf": [{}]}"#)
                .unwrap();
        let table = crate::analyze(&schema).unwrap();
        let children = Children::new(&table);
        use symbols::Step as S;
        // Las expectativas correctas dan los hijos 1, 2 y 3.
        let mut kids = children.of(0);
        assert_eq!(next(&table, &mut kids, S::Keyword("not")), NodeId::new(1));
        assert_eq!(
            next(&table, &mut kids, S::Entry("properties", "a")),
            NodeId::new(2)
        );
        assert_eq!(
            next(&table, &mut kids, S::Index("allOf", 0)),
            NodeId::new(3)
        );
        assert_eq!(next(&table, &mut kids, S::Keyword("not")), DANGLING);
        // Otra palabra clave, otra entrada u otro índice: colgante.
        for malo in [S::Keyword("items"), S::Entry("not", "a")] {
            let mut kids = children.of(0);
            assert_eq!(next(&table, &mut kids, malo), DANGLING, "{malo:?}");
        }
        let mut kids = children.of(0);
        kids.next();
        assert_eq!(
            next(&table, &mut kids, S::Entry("properties", "b")),
            DANGLING
        );
        let mut kids = children.of(0);
        kids.next();
        kids.next();
        assert_eq!(next(&table, &mut kids, S::Index("allOf", 1)), DANGLING);
    }
}
