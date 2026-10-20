//! Tabla de símbolos del esquema y resolución de `$ref` (Fase 3).
//!
//! Cada subesquema del documento (todo valor que la gramática lee como
//! `schema`: la raíz, el valor de `items`/`not`/`additionalProperties`, cada
//! entrada de `properties`/`$defs`/`definitions`/`dependentSchemas` y cada
//! elemento de `prefixItems`/`allOf`/`anyOf`/`oneOf`) recibe un
//! [`SchemaId`]: su número en **preorden en orden de documento**, que es el
//! orden de su primer byte en la entrada. Así los ids son estables: solo
//! dependen del texto, no de semillas de hash ni de la versión del
//! compilador.
//!
//! La tabla registra para cada subesquema su padre y el paso desde él (de
//! donde sale su puntero JSON), las anclas (`$anchor`), las entradas de
//! `$defs`/`definitions`, y el destino de cada `$ref`. La resolución recorre
//! el puntero sobre el documento con un índice hash, sin seguir otros
//! `$ref`: una referencia circular (un árbol recursivo, `A → B → A`) se
//! resuelve igual que cualquier otra y no hay bucle. Decidir si un ciclo es
//! productivo es la tarea siguiente, que usa [`SymbolTable::refs`].
//!
//! Teoría: `docs/fase3/referencias.md`.
//!
//! Nada es recursivo sobre el AST (D7): el preorden usa una pila explícita y
//! la resolución es un bucle sobre los segmentos del puntero.

use std::collections::HashMap;

use schema_parser::error::short;
use schema_parser::{Kw, Pos, Schema, Str, Value};

use crate::error::{ErrorKind, RefProblem, SemanticError};
use crate::pointer::{self, DecodeError, Fragment};

/// Identificador de un subesquema: su número en preorden en orden de
/// documento (la raíz es 0). Solo tiene sentido en la tabla que lo creó.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaId(usize);

impl SchemaId {
    /// La raíz del documento.
    pub const ROOT: SchemaId = SchemaId(0);

    /// Posición en [`SymbolTable::nodes`].
    pub fn index(self) -> usize {
        self.0
    }
}

/// Paso del padre a un subesquema: un segmento del puntero JSON (palabra
/// clave) o dos (palabra clave y nombre o índice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    /// `items`, `additionalProperties`, `not`.
    Keyword(&'a str),
    /// Entrada de `properties`, `$defs`, `definitions` o `dependentSchemas`.
    Entry(&'a str, &'a str),
    /// Elemento de `prefixItems`, `allOf`, `anyOf` u `oneOf`.
    Index(&'a str, usize),
}

/// Un subesquema registrado.
#[derive(Clone, Debug)]
pub struct Node<'a> {
    pub schema: &'a Schema,
    /// Padre y paso desde él; `None` en la raíz.
    pub parent: Option<(SchemaId, Step<'a>)>,
    /// Destino de su `$ref`, si tiene (a lo sumo uno: el parser rechaza las
    /// claves repetidas).
    pub ref_target: Option<SchemaId>,
}

/// Un `$ref` resuelto: arista del grafo de referencias.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ref {
    /// Subesquema que contiene el `$ref`.
    pub from: SchemaId,
    /// Subesquema destino.
    pub target: SchemaId,
    /// Posición del valor del `$ref`.
    pub at: Pos,
}

/// Tabla de símbolos de un esquema JSON. Toma prestado el AST.
#[derive(Clone, Debug)]
pub struct SymbolTable<'a> {
    nodes: Vec<Node<'a>>,
    anchors: HashMap<&'a str, SchemaId>,
    defs: Vec<SchemaId>,
    refs: Vec<Ref>,
}

/// Índice para recorrer punteros en O(1) esperado por segmento. Solo vive
/// durante la construcción.
#[derive(Debug, Default)]
struct Index<'a> {
    /// (esquema, nombre) → posición de la palabra clave en el esquema.
    keyword: HashMap<(usize, &'a str), usize>,
    /// (esquema, posición de la palabra clave, nombre) → posición de la
    /// entrada en el mapa.
    entry: HashMap<(usize, usize, &'a str), usize>,
    /// (esquema, posición de la palabra clave, posición) → subesquema.
    child: HashMap<(usize, usize, usize), SchemaId>,
}

/// Dónde está el recorrido de un puntero: en un esquema o en el objeto o
/// arreglo de esquemas de una de sus palabras clave.
#[derive(Clone, Copy)]
enum At<'a> {
    Schema(SchemaId),
    Map(SchemaId, usize, &'a str),
    List(SchemaId, usize, &'a str),
}

/// Padre en la pila del preorden: id, posición de la palabra clave,
/// posición dentro de su valor y paso.
type Parent<'a> = (SchemaId, usize, usize, Step<'a>);

impl<'a> SymbolTable<'a> {
    /// Construye la tabla y resuelve todos los `$ref`. Devuelve todos los
    /// errores (anclas repetidas, referencias sin resolver), ordenados por
    /// posición.
    ///
    /// Se espera un esquema que ya pasó [`crate::check_meta_schema`] (así lo
    /// hace [`crate::analyze`]); sobre otro no falla, pero puede informar
    /// como referencia sin resolver algo que el meta-esquema rechaza mejor.
    ///
    /// Costo: O(n) esperado en el tamaño del AST más O(r) en la longitud
    /// total de los `$ref`, más O(e log e) para ordenar e anclas y errores.
    pub fn build(root: &'a Schema) -> Result<Self, Vec<SemanticError>> {
        let mut nodes: Vec<Node<'a>> = Vec::new();
        let mut idx = Index::default();
        let mut defs = Vec::new();
        let mut anchor_strs: Vec<(&'a Str, SchemaId)> = Vec::new();
        let mut ref_strs: Vec<(&'a Str, SchemaId)> = Vec::new();

        // Preorden en orden de documento con una pila explícita: los hijos de
        // cada esquema se apilan y luego se invierten, para sacarlos en el
        // orden en que aparecen.
        let mut stack: Vec<(&'a Schema, Option<Parent<'a>>)> = vec![(root, None)];
        while let Some((schema, parent)) = stack.pop() {
            let id = SchemaId(nodes.len());
            if let Some((p, kw_pos, i, step)) = parent {
                idx.child.insert((p.0, kw_pos, i), id);
                if matches!(step, Step::Entry("$defs" | "definitions", _)) {
                    defs.push(id);
                }
            }
            nodes.push(Node {
                schema,
                parent: parent.map(|(p, _, _, step)| (p, step)),
                ref_target: None,
            });
            let mark = stack.len();
            for (pos, kw) in schema.keywords().iter().enumerate() {
                let name = kw.name.value.as_str();
                idx.keyword.insert((id.0, name), pos);
                match (kw.kw, &kw.value) {
                    (Kw::Anchor, Value::Str(s)) => anchor_strs.push((s, id)),
                    (Kw::Ref, Value::Str(s)) => ref_strs.push((s, id)),
                    (_, Value::Schema(sub)) => {
                        stack.push((sub, Some((id, pos, 0, Step::Keyword(name)))));
                    }
                    (_, Value::SchemaMap(m)) => {
                        for (i, (k, sub)) in m.iter().enumerate() {
                            idx.entry.insert((id.0, pos, k.value.as_str()), i);
                            stack.push((sub, Some((id, pos, i, Step::Entry(name, &k.value)))));
                        }
                    }
                    (_, Value::SchemaList(l)) => {
                        for (i, sub) in l.iter().enumerate() {
                            stack.push((sub, Some((id, pos, i, Step::Index(name, i)))));
                        }
                    }
                    _ => {}
                }
            }
            stack[mark..].reverse();
        }

        let mut errors = Vec::new();

        // Anclas en orden de aparición del `$anchor`: el preorden visita un
        // esquema antes que sus hijos aunque su `$anchor` esté escrito
        // después de ellos, y la repetida debe ser la segunda en el texto.
        anchor_strs.sort_by_key(|(s, _)| s.span.start.offset);
        let mut anchors: HashMap<&'a str, SchemaId> = HashMap::new();
        let mut first_pos: HashMap<&'a str, Pos> = HashMap::new();
        for (s, id) in anchor_strs {
            match first_pos.get(s.value.as_str()) {
                Some(&first) => errors.push(SemanticError {
                    kind: ErrorKind::DuplicateAnchor {
                        name: s.value.clone(),
                        first,
                    },
                    at: s.span.start,
                }),
                None => {
                    first_pos.insert(&s.value, s.span.start);
                    anchors.insert(&s.value, id);
                }
            }
        }

        let mut table = SymbolTable {
            nodes,
            anchors,
            defs,
            refs: Vec::new(),
        };
        for (s, from) in ref_strs {
            match table.resolve(&idx, &s.value) {
                Ok(target) => {
                    table.nodes[from.0].ref_target = Some(target);
                    table.refs.push(Ref {
                        from,
                        target,
                        at: s.span.start,
                    });
                }
                Err(problem) => errors.push(SemanticError {
                    kind: ErrorKind::UnresolvedRef {
                        reference: short(&s.value),
                        problem,
                    },
                    at: s.span.start,
                }),
            }
        }
        table.refs.sort_by_key(|r| r.at.offset);

        if errors.is_empty() {
            Ok(table)
        } else {
            errors.sort_by_key(|e: &SemanticError| e.at.offset);
            Err(errors)
        }
    }

    /// Destino de un `$ref` local.
    fn resolve(&self, idx: &Index<'a>, reference: &str) -> Result<SchemaId, RefProblem> {
        let segments = match pointer::decode(reference) {
            Ok(Fragment::Pointer(s)) => s,
            Ok(Fragment::Anchor(a)) => {
                return self
                    .anchors
                    .get(a.as_str())
                    .copied()
                    .ok_or(RefProblem::UnknownAnchor)
            }
            Err(e) => {
                return Err(match e {
                    DecodeError::NotLocal => RefProblem::NotLocal,
                    DecodeError::PercentEncoding => RefProblem::PercentEncoding,
                    DecodeError::PointerEscape => RefProblem::PointerEscape,
                    DecodeError::InvalidAnchor => RefProblem::InvalidAnchor,
                })
            }
        };
        let mut at = At::Schema(SchemaId::ROOT);
        for (n, seg) in segments.iter().enumerate() {
            let not_found = || RefProblem::NotFound {
                segment: seg.clone(),
                index: n + 1,
            };
            let child = |id: SchemaId, pos: usize, i: usize| {
                idx.child
                    .get(&(id.0, pos, i))
                    .copied()
                    .ok_or_else(not_found)
            };
            at = match at {
                At::Schema(id) => {
                    let pos = *idx
                        .keyword
                        .get(&(id.0, seg.as_str()))
                        .ok_or_else(not_found)?;
                    let kw = &self.nodes[id.0].schema.keywords()[pos];
                    let name = kw.name.value.as_str();
                    match &kw.value {
                        Value::Schema(_) => At::Schema(child(id, pos, 0)?),
                        Value::SchemaMap(_) => At::Map(id, pos, name),
                        Value::SchemaList(_) => At::List(id, pos, name),
                        Value::Json(_) | Value::JsonList(_) => {
                            return Err(RefProblem::IntoLiteral {
                                keyword: name.to_owned(),
                            })
                        }
                        _ => {
                            return Err(RefProblem::NotASchema {
                                keyword: name.to_owned(),
                            })
                        }
                    }
                }
                At::Map(id, pos, _) => {
                    let i = *idx
                        .entry
                        .get(&(id.0, pos, seg.as_str()))
                        .ok_or_else(not_found)?;
                    At::Schema(child(id, pos, i)?)
                }
                At::List(id, pos, _) => {
                    // Un índice fuera del arreglo no está en `idx.child`:
                    // `child` ya da `NotFound`, no hace falta la longitud.
                    let i = array_index(seg).ok_or_else(not_found)?;
                    At::Schema(child(id, pos, i)?)
                }
            };
        }
        match at {
            At::Schema(id) => Ok(id),
            At::Map(.., name) | At::List(.., name) => Err(RefProblem::NotASchema {
                keyword: name.to_owned(),
            }),
        }
    }

    /// Número de subesquemas.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Siempre `false`: la raíz es un subesquema.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Subesquemas en preorden en orden de documento (`nodes()[i]` es el de
    /// id `i`).
    pub fn nodes(&self) -> &[Node<'a>] {
        &self.nodes
    }

    /// Ids de todos los subesquemas, en preorden (`0..len()`).
    pub fn ids(&self) -> impl Iterator<Item = SchemaId> {
        (0..self.nodes.len()).map(SchemaId)
    }

    /// Id del subesquema en la posición `i` del preorden, si existe.
    pub fn id(&self, i: usize) -> Option<SchemaId> {
        (i < self.nodes.len()).then_some(SchemaId(i))
    }

    pub fn node(&self, id: SchemaId) -> &Node<'a> {
        &self.nodes[id.0]
    }

    /// Subesquema de un `$anchor`.
    pub fn anchor(&self, name: &str) -> Option<SchemaId> {
        self.anchors.get(name).copied()
    }

    /// Entradas de `$defs` y `definitions` (en cualquier nivel), en orden de
    /// documento. Su puntero lo da [`SymbolTable::pointer`].
    pub fn defs(&self) -> &[SchemaId] {
        &self.defs
    }

    /// Todos los `$ref` resueltos, en orden de documento: las aristas del
    /// grafo de referencias.
    pub fn refs(&self) -> &[Ref] {
        &self.refs
    }

    /// Puntero JSON (RFC 6901) del subesquema: `""` para la raíz,
    /// `/$defs/a~1b` para la entrada `a/b` de `$defs`. Costo O(profundidad +
    /// longitud del puntero), que incluye la de todas las claves del camino.
    pub fn pointer(&self, id: SchemaId) -> String {
        let mut steps = Vec::new();
        let mut cur = id;
        while let Some((parent, step)) = self.nodes[cur.0].parent {
            steps.push(step);
            cur = parent;
        }
        let mut out = String::new();
        for step in steps.iter().rev() {
            match *step {
                Step::Keyword(k) => out.push_str(&format!("/{}", pointer::escape(k))),
                Step::Entry(k, e) => {
                    out.push_str(&format!("/{}/{}", pointer::escape(k), pointer::escape(e)))
                }
                Step::Index(k, i) => out.push_str(&format!("/{}/{i}", pointer::escape(k))),
            }
        }
        out
    }

    /// El puntero como fragmento de URI, listo para un `$ref` (`#/$defs/x`,
    /// con `%XX` donde la RFC 3986 lo exige).
    ///
    /// Su longitud es la del camino entero (claves incluidas): para mensajes
    /// de error, donde puede haber un nodo profundo con claves largas por
    /// cada error, use [`SymbolTable::fragment_clipped`].
    pub fn fragment(&self, id: SchemaId) -> String {
        pointer::to_fragment(&self.pointer(id))
    }

    /// El fragmento de [`SymbolTable::fragment`] recortado a como mucho
    /// `max` bytes (mínimo 16): si no cabe, su principio, `…` y su final.
    /// No construye el fragmento entero: recorre los padres (O(profundidad))
    /// y codifica solo los caracteres que caben, así que cuesta
    /// O(profundidad + max) y no depende de la longitud de las claves.
    pub fn fragment_clipped(&self, id: SchemaId, max: usize) -> String {
        const GAP: &str = "…";
        let max = max.max(16);
        let mut steps = Vec::new();
        let mut cur = id;
        while let Some((parent, step)) = self.nodes[cur.0].parent {
            steps.push(step);
            cur = parent;
        }
        steps.reverse();
        let mut segs: Vec<Seg<'_>> = Vec::with_capacity(2 * steps.len());
        for step in &steps {
            match *step {
                Step::Keyword(k) => segs.push(Seg::Str(k)),
                Step::Entry(k, e) => segs.extend([Seg::Str(k), Seg::Str(e)]),
                Step::Index(k, i) => segs.extend([Seg::Str(k), Seg::Num(i)]),
            }
        }
        // Todo, si cabe en `max`.
        let mut out = String::from("#");
        if emit(&segs, &mut out, max, false) {
            return out;
        }
        // Principio y final.
        let head = (max - GAP.len()) / 2;
        let tail = max - GAP.len() - head;
        let mut first = String::from("#");
        emit(&segs, &mut first, head, false);
        let mut last = String::new();
        emit(&segs, &mut last, tail, true);
        first.push_str(GAP);
        first.push_str(&last);
        first
    }
}

/// Segmento de un puntero, sin codificar.
#[derive(Clone, Copy)]
enum Seg<'a> {
    Str(&'a str),
    Num(usize),
}

/// Añade a `out` los trozos codificados (`/` y un carácter de segmento a la
/// vez) de los segmentos, del principio (o del final, con `rev`), mientras
/// `out` no pase de `max` bytes. Con `rev` los trozos se añaden en orden
/// inverso y se dan la vuelta al terminar. Devuelve si cupieron todos. Solo
/// mira los caracteres que caben más uno.
fn emit(segs: &[Seg<'_>], out: &mut String, max: usize, rev: bool) -> bool {
    let mut chunks: Vec<String> = Vec::new();
    let mut len = out.len();
    let mut push = |chunk: String, chunks: &mut Vec<String>| -> bool {
        if len + chunk.len() > max {
            return false;
        }
        len += chunk.len();
        chunks.push(chunk);
        true
    };
    let mut all = true;
    let order: Box<dyn Iterator<Item = &Seg<'_>>> = if rev {
        Box::new(segs.iter().rev())
    } else {
        Box::new(segs.iter())
    };
    'outer: for seg in order {
        let num;
        let text: &str = match seg {
            Seg::Str(s) => s,
            Seg::Num(i) => {
                num = i.to_string();
                &num
            }
        };
        if !rev && !push("/".to_owned(), &mut chunks) {
            all = false;
            break;
        }
        let chars: Box<dyn Iterator<Item = char>> = if rev {
            Box::new(text.chars().rev())
        } else {
            Box::new(text.chars())
        };
        for c in chars {
            let mut enc = String::new();
            pointer::encode_char(c, &mut enc);
            if !push(enc, &mut chunks) {
                all = false;
                break 'outer;
            }
        }
        if rev && !push("/".to_owned(), &mut chunks) {
            all = false;
            break;
        }
    }
    if rev {
        chunks.reverse();
    }
    for c in chunks {
        out.push_str(&c);
    }
    all
}

/// Índice de arreglo de la RFC 6901: `0` o un decimal sin ceros a la
/// izquierda. `-` (posición tras el último) nunca designa un esquema.
fn array_index(seg: &str) -> Option<usize> {
    let ok = seg == "0"
        || (!seg.is_empty() && !seg.starts_with('0') && seg.bytes().all(|b| b.is_ascii_digit()));
    if ok {
        seg.parse().ok()
    } else {
        None
    }
}
