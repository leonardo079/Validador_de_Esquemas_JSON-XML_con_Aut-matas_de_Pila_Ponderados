//! Fase 3 — Representación intermedia (IR) común de JSON Schema y XSD.
//!
//! Es el contrato entre los frontales (la bajada de JSON Schema desde
//! `semantic` y la del futuro AST de XSD) y el constructor del DPDA de la
//! Fase 4 (decisión D3 del plan). No depende de ningún frontal: solo de
//! [`lexer`] por [`Span`].
//!
//! La IR es un **grafo en arena**: [`Ir::nodes`] es un `Vec<Node>` y las
//! aristas son [`NodeId`]. Cada nodo describe lo que se exige a **una
//! instancia** (un valor JSON, un elemento XML o un texto XML) como una lista
//! de reglas tipadas ([`Rule`]), casi una por palabra clave o construcción
//! (`Items` y `Properties` juntan dos palabras clave, y `Reject` es el
//! esquema `false`). Un `$ref`, un `allOf` o el tipo de un `xs:element` son
//! aristas a nodos que ya existen, así que un subesquema referenciado
//! muchas veces está una sola vez y el tamaño de la IR es O(n) en el número
//! de subesquemas.
//!
//! Ningún tipo de la IR es recursivo (no hay `Box<Node>` ni árboles
//! anidados): los literales de `enum` son una lista de tokens en preorden y
//! los modelos de contenido de XSD una arena de partículas. Así ni el
//! verificador ([`Ir::verify`]) ni la liberación de memoria recorren nada
//! recursivamente (D7).
//!
//! Teoría, invariantes y correspondencia con cada palabra clave:
//! `docs/fase3/ir.md`.

mod verify;

pub use lexer::{Pos, Span};
pub use verify::{VerifyError, VerifyErrorKind};

/// Identificador de un nodo: su posición en [`Ir::nodes`]. La raíz es el 0.
/// Solo tiene sentido en la IR que lo creó (el verificador comprueba que
/// todos los de la IR están en rango, invariante I2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(usize);

impl NodeId {
    /// La raíz: el esquema completo en JSON, el documento en XML.
    pub const ROOT: NodeId = NodeId(0);

    pub const fn new(index: usize) -> NodeId {
        NodeId(index)
    }

    /// Posición en [`Ir::nodes`].
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Formato de los payloads que valida la IR. Un payload JSON nunca se valida
/// contra una IR de XSD ni al revés (`docs/fase3/ir.md` §6.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    /// `target_namespace`: el único espacio de nombres admitido
    /// (`docs/00-alcance.md` §1.2); los elementos y atributos de la IR se
    /// nombran por su nombre local.
    Xml {
        target_namespace: Option<String>,
    },
}

/// La IR de un esquema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ir {
    pub format: Format,
    /// Arena de nodos; `nodes[0]` es la raíz.
    pub nodes: Vec<Node>,
}

/// Tipo de instancia a la que se aplica un nodo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sort {
    /// Un valor JSON (cualquiera de los siete tipos).
    Json,
    /// Un elemento XML: sus atributos y su contenido. La raíz de una IR de
    /// XSD es el **documento**, que se trata como un elemento sin atributos
    /// cuyo contenido es la elección de los elementos globales.
    Element,
    /// Un texto XML: el valor de un atributo o el contenido simple de un
    /// elemento (`xs:simpleType`).
    Text,
}

/// Un nodo: lo que se exige a una instancia. Sin reglas acepta cualquier
/// instancia de su tipo (el esquema `true` o `{}`); un elemento sin
/// [`Rule::Content`] ni [`Rule::SimpleContent`] tiene contenido vacío.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub sort: Sort,
    /// Reglas en el orden del esquema; a lo sumo una de cada [`RuleKind`]
    /// (invariante I4).
    pub rules: Vec<Constraint>,
    pub origin: Origin,
}

/// Una regla con su posición en el esquema, para los mensajes de error de
/// las fases siguientes («no cumple `minimum`, línea 4, columna 12»).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    pub rule: Rule,
    /// Rango de la palabra clave (nombre y valor) o de la construcción XSD.
    pub span: Span,
}

/// Ubicación de origen de un nodo. Se guarda el padre en el **árbol del
/// esquema fuente** y el paso desde él, no la ruta entera: guardar cada ruta
/// costaría la suma de sus longitudes, cuadrática en el peor caso
/// (`docs/fase3/referencias.md` §2). [`Ir::location`] la reconstruye.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    /// Rango del subesquema o de la declaración XSD.
    pub span: Span,
    /// Padre en el esquema fuente; `None` solo en la raíz. Siempre es un
    /// nodo anterior (invariante I1), así que la cadena de padres termina.
    pub parent: Option<NodeId>,
    pub step: Step,
}

/// Paso del padre a un nodo en el esquema fuente: uno o dos segmentos de su
/// ruta. En JSON son los del puntero JSON (`docs/fase3/referencias.md` §1);
/// en XSD, la construcción y su nombre (`complexType`/`Pedido`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// La raíz (ruta vacía).
    Root,
    /// `items`, `not`, `additionalProperties`, `simpleContent`…
    Keyword(String),
    /// Entrada con nombre: `properties`/`a`, `$defs`/`x`, `element`/`item`.
    Entry(String, String),
    /// Elemento de un arreglo: `allOf`/`0`, `prefixItems`/`2`.
    Index(String, usize),
}

/// Número como **lexema decimal exacto** con la sintaxis de RFC 8259
/// (invariante I8); nunca se convierte a `f64` (`docs/00-alcance.md` §1.1).
/// El frontal XSD reescribe su forma léxica (`+1`, `.5`, `1.`) a la
/// equivalente de RFC 8259 sin cambiar el valor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Num {
    pub lexeme: String,
}

impl Num {
    pub fn new(lexeme: &str) -> Num {
        Num {
            lexeme: lexeme.to_owned(),
        }
    }
}

/// Los siete tipos de `type` (`docs/00-alcance.md` §2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JsonType {
    Null,
    Boolean,
    Object,
    Array,
    Number,
    /// Número con parte fraccionaria cero (`1.0` es entero, como en 2020-12).
    Integer,
    String,
}

impl JsonType {
    pub const ALL: [JsonType; 7] = [
        JsonType::Null,
        JsonType::Boolean,
        JsonType::Object,
        JsonType::Array,
        JsonType::Number,
        JsonType::Integer,
        JsonType::String,
    ];

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Conjunto de tipos de `type`, como máscara de 7 bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TypeSet(u8);

impl TypeSet {
    pub fn of(types: &[JsonType]) -> TypeSet {
        TypeSet(types.iter().fold(0, |m, t| m | t.bit()))
    }

    pub const fn contains(self, t: JsonType) -> bool {
        self.0 & t.bit() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Espacio léxico de un texto XML: el tipo predefinido primitivo del que
/// deriva un `xs:simpleType` (`docs/00-alcance.md` §3.2). Los tipos enteros
/// acotados (`xs:int`, `xs:byte`, `xs:positiveInteger`…) son `Integer` con
/// [`Rule::Minimum`]/[`Rule::Maximum`] en el mismo nodo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lexical {
    /// `xs:string`: espacios conservados.
    String,
    /// `xs:boolean`: `true`, `false`, `1`, `0`.
    Boolean,
    /// `xs:decimal`.
    Decimal,
    /// `xs:integer` y sus derivados.
    Integer,
    /// `xs:float` (incluye `INF`, `-INF`, `NaN`).
    Float,
    /// `xs:double` (ídem).
    Double,
}

impl Lexical {
    /// Admite las facetas numéricas (`minInclusive`…).
    pub const fn is_numeric(self) -> bool {
        matches!(
            self,
            Lexical::Decimal | Lexical::Integer | Lexical::Float | Lexical::Double
        )
    }
}

/// Un token de un literal de `enum`/`const` o de `xs:enumeration`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LitToken {
    Null,
    Bool(bool),
    /// Lexema exacto (RFC 8259).
    Number(String),
    /// Cadena ya decodificada (sin comillas ni escapes).
    String(String),
    /// Arreglo de n elementos: le siguen sus n valores.
    Array(usize),
    /// Objeto de n miembros: le siguen n pares `Key`, valor.
    Object(usize),
    /// Nombre de un miembro de objeto.
    Key(String),
}

/// Un valor literal JSON como lista de tokens en **preorden**, sin árbol
/// anidado (D7). Las claves de cada objeto van **ordenadas** por punto de
/// código y sin repetir (invariante I9): así dos objetos iguales como valores
/// JSON tienen la misma lista salvo en la forma de sus números, que se
/// comparan por valor (`1` = `1.0`) al construir el trie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal(pub Vec<LitToken>);

/// Ocurrencias de una partícula XSD (`minOccurs`/`maxOccurs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Occurs {
    pub min: u64,
    /// `None` es `unbounded`.
    pub max: Option<u64>,
}

impl Occurs {
    /// `minOccurs="1" maxOccurs="1"`, el valor por defecto de XSD.
    pub const ONCE: Occurs = Occurs {
        min: 1,
        max: Some(1),
    };
}

/// Una partícula de un modelo de contenido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Particle {
    pub term: Term,
    pub occurs: Occurs,
    pub span: Span,
}

/// Término de una partícula. Los hijos de los grupos son índices en
/// [`ContentModel::particles`] en preorden (invariante I11): el primer hijo
/// de un grupo es la partícula siguiente y cada hijo empieza donde acaba el
/// subárbol del anterior.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    /// `xs:element` local o `ref` a uno global: nombre local y tipo (un nodo
    /// [`Sort::Element`]).
    Element { name: String, ty: NodeId },
    /// `xs:sequence`: concatenación en orden.
    Sequence(Vec<usize>),
    /// `xs:choice`: unión.
    Choice(Vec<usize>),
    /// `xs:all`: cada hijo a lo sumo una vez, en cualquier orden.
    All(Vec<usize>),
}

/// Modelo de contenido de solo elementos: una expresión regular sobre los
/// nombres de los elementos hijos, guardada como arena de partículas en
/// preorden; `particles[0]` es la partícula raíz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentModel {
    pub particles: Vec<Particle>,
}

/// Un `xs:attribute`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    /// Nombre local.
    pub name: String,
    /// `use="required"`.
    pub required: bool,
    /// Tipo del valor: un nodo [`Sort::Text`].
    pub ty: NodeId,
    pub default: Option<String>,
    pub fixed: Option<String>,
    pub span: Span,
}

/// Una regla tipada. Las de JSON Schema llevan el nombre de su palabra
/// clave; `docs/fase3/ir.md` §5 da la correspondencia completa con
/// `docs/00-alcance.md` §2 y §3.1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// El esquema `false`: no acepta ninguna instancia.
    Reject,
    /// `$ref`: el nodo destino se aplica a la misma instancia.
    Ref(NodeId),
    /// `type`.
    Type(TypeSet),
    /// `enum`; en XSD, `xs:enumeration`.
    Enum(Vec<Literal>),
    /// `const`. Es su propia regla y no un `Enum` de un valor: un esquema
    /// puede tener `enum` y `const` a la vez (`docs/fase3/ir.md` §5.1).
    Const(Literal),
    /// `minimum`; en XSD, `minInclusive`.
    Minimum(Num),
    /// `maximum`; `maxInclusive`.
    Maximum(Num),
    /// `exclusiveMinimum`; `minExclusive`.
    ExclusiveMinimum(Num),
    /// `exclusiveMaximum`; `maxExclusive`.
    ExclusiveMaximum(Num),
    /// `multipleOf` (> 0).
    MultipleOf(Num),
    /// `minLength` en puntos de código; `minLength` y `length` de XSD.
    MinLength(u64),
    /// `maxLength`; `maxLength` y `length` de XSD.
    MaxLength(u64),
    /// `pattern`. El dialecto lo fija el tipo del nodo: ECMA-262 sin anclar
    /// en [`Sort::Json`], regex de XSD (anclada) en [`Sort::Text`].
    Pattern(String),
    /// `prefixItems` (`prefix`) e `items` (`rest`, los elementos que siguen).
    Items {
        prefix: Vec<NodeId>,
        rest: Option<NodeId>,
    },
    /// `minItems`.
    MinItems(u64),
    /// `maxItems`.
    MaxItems(u64),
    /// `uniqueItems: true` (`false` no produce regla).
    UniqueItems,
    /// `properties` (`declared`, en orden del esquema) y
    /// `additionalProperties` (`additional`; `None` acepta cualquiera) del
    /// **mismo** objeto-esquema (`docs/fase3/ir.md` §6.2).
    Properties {
        declared: Vec<(String, NodeId)>,
        additional: Option<NodeId>,
    },
    /// `required`.
    Required(Vec<String>),
    /// `minProperties`.
    MinProperties(u64),
    /// `maxProperties`.
    MaxProperties(u64),
    /// `dependentRequired`: propiedad → propiedades que exige.
    DependentRequired(Vec<(String, Vec<String>)>),
    /// `dependentSchemas`: propiedad → esquema que valida el objeto entero.
    DependentSchemas(Vec<(String, NodeId)>),
    /// `allOf`.
    AllOf(Vec<NodeId>),
    /// `anyOf`.
    AnyOf(Vec<NodeId>),
    /// `oneOf`.
    OneOf(Vec<NodeId>),
    /// `not`.
    Not(NodeId),
    /// Los `xs:attribute` de un tipo complejo.
    Attributes(Vec<Attribute>),
    /// Contenido de solo elementos (`xs:sequence`, `xs:choice`, `xs:all`).
    Content(ContentModel),
    /// `xs:simpleContent`: el texto del elemento debe cumplir el nodo
    /// [`Sort::Text`] destino.
    SimpleContent(NodeId),
    /// Espacio léxico de un texto: obligatorio y único en cada nodo
    /// [`Sort::Text`].
    Lexical(Lexical),
}

/// Clase de regla, sin datos: decide qué reglas admite cada tipo de nodo,
/// la unicidad por nodo y, en la tarea 4.3, la clase (a)/(b)/(c).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuleKind {
    Reject,
    Ref,
    Type,
    Enum,
    Const,
    Minimum,
    Maximum,
    ExclusiveMinimum,
    ExclusiveMaximum,
    MultipleOf,
    MinLength,
    MaxLength,
    Pattern,
    Items,
    MinItems,
    MaxItems,
    UniqueItems,
    Properties,
    Required,
    MinProperties,
    MaxProperties,
    DependentRequired,
    DependentSchemas,
    AllOf,
    AnyOf,
    OneOf,
    Not,
    Attributes,
    Content,
    SimpleContent,
    Lexical,
}

impl RuleKind {
    /// Todas, en el orden de la declaración (para recorridos exhaustivos).
    pub const ALL: [RuleKind; 31] = [
        RuleKind::Reject,
        RuleKind::Ref,
        RuleKind::Type,
        RuleKind::Enum,
        RuleKind::Const,
        RuleKind::Minimum,
        RuleKind::Maximum,
        RuleKind::ExclusiveMinimum,
        RuleKind::ExclusiveMaximum,
        RuleKind::MultipleOf,
        RuleKind::MinLength,
        RuleKind::MaxLength,
        RuleKind::Pattern,
        RuleKind::Items,
        RuleKind::MinItems,
        RuleKind::MaxItems,
        RuleKind::UniqueItems,
        RuleKind::Properties,
        RuleKind::Required,
        RuleKind::MinProperties,
        RuleKind::MaxProperties,
        RuleKind::DependentRequired,
        RuleKind::DependentSchemas,
        RuleKind::AllOf,
        RuleKind::AnyOf,
        RuleKind::OneOf,
        RuleKind::Not,
        RuleKind::Attributes,
        RuleKind::Content,
        RuleKind::SimpleContent,
        RuleKind::Lexical,
    ];

    /// Nombre en los mensajes: la palabra clave de JSON Schema o la
    /// construcción de XSD.
    pub const fn name(self) -> &'static str {
        match self {
            RuleKind::Reject => "false",
            RuleKind::Ref => "$ref",
            RuleKind::Type => "type",
            RuleKind::Enum => "enum",
            RuleKind::Const => "const",
            RuleKind::Minimum => "minimum",
            RuleKind::Maximum => "maximum",
            RuleKind::ExclusiveMinimum => "exclusiveMinimum",
            RuleKind::ExclusiveMaximum => "exclusiveMaximum",
            RuleKind::MultipleOf => "multipleOf",
            RuleKind::MinLength => "minLength",
            RuleKind::MaxLength => "maxLength",
            RuleKind::Pattern => "pattern",
            RuleKind::Items => "items",
            RuleKind::MinItems => "minItems",
            RuleKind::MaxItems => "maxItems",
            RuleKind::UniqueItems => "uniqueItems",
            RuleKind::Properties => "properties",
            RuleKind::Required => "required",
            RuleKind::MinProperties => "minProperties",
            RuleKind::MaxProperties => "maxProperties",
            RuleKind::DependentRequired => "dependentRequired",
            RuleKind::DependentSchemas => "dependentSchemas",
            RuleKind::AllOf => "allOf",
            RuleKind::AnyOf => "anyOf",
            RuleKind::OneOf => "oneOf",
            RuleKind::Not => "not",
            RuleKind::Attributes => "xs:attribute",
            RuleKind::Content => "modelo de contenido",
            RuleKind::SimpleContent => "xs:simpleContent",
            RuleKind::Lexical => "tipo predefinido",
        }
    }

    /// Si la regla puede aparecer en un nodo de tipo `sort` (invariante I3;
    /// tabla en `docs/fase3/ir.md` §4).
    pub const fn allowed_in(self, sort: Sort) -> bool {
        use RuleKind as K;
        match sort {
            Sort::Json => !matches!(
                self,
                K::Attributes | K::Content | K::SimpleContent | K::Lexical
            ),
            Sort::Element => matches!(self, K::Attributes | K::Content | K::SimpleContent),
            Sort::Text => matches!(
                self,
                K::Lexical
                    | K::Ref
                    | K::Enum
                    | K::Minimum
                    | K::Maximum
                    | K::ExclusiveMinimum
                    | K::ExclusiveMaximum
                    | K::MinLength
                    | K::MaxLength
                    | K::Pattern
            ),
        }
    }

    /// Si sus aristas aplican el destino a la **misma instancia** (no
    /// consumen entrada, `docs/fase3/ciclos.md` §2). El subgrafo de estas
    /// aristas es acíclico (invariante I14).
    pub const fn same_instance(self) -> bool {
        matches!(
            self,
            RuleKind::Ref
                | RuleKind::AllOf
                | RuleKind::AnyOf
                | RuleKind::OneOf
                | RuleKind::Not
                | RuleKind::DependentSchemas
        )
    }
}

impl Rule {
    pub const fn kind(&self) -> RuleKind {
        match self {
            Rule::Reject => RuleKind::Reject,
            Rule::Ref(_) => RuleKind::Ref,
            Rule::Type(_) => RuleKind::Type,
            Rule::Enum(_) => RuleKind::Enum,
            Rule::Const(_) => RuleKind::Const,
            Rule::Minimum(_) => RuleKind::Minimum,
            Rule::Maximum(_) => RuleKind::Maximum,
            Rule::ExclusiveMinimum(_) => RuleKind::ExclusiveMinimum,
            Rule::ExclusiveMaximum(_) => RuleKind::ExclusiveMaximum,
            Rule::MultipleOf(_) => RuleKind::MultipleOf,
            Rule::MinLength(_) => RuleKind::MinLength,
            Rule::MaxLength(_) => RuleKind::MaxLength,
            Rule::Pattern(_) => RuleKind::Pattern,
            Rule::Items { .. } => RuleKind::Items,
            Rule::MinItems(_) => RuleKind::MinItems,
            Rule::MaxItems(_) => RuleKind::MaxItems,
            Rule::UniqueItems => RuleKind::UniqueItems,
            Rule::Properties { .. } => RuleKind::Properties,
            Rule::Required(_) => RuleKind::Required,
            Rule::MinProperties(_) => RuleKind::MinProperties,
            Rule::MaxProperties(_) => RuleKind::MaxProperties,
            Rule::DependentRequired(_) => RuleKind::DependentRequired,
            Rule::DependentSchemas(_) => RuleKind::DependentSchemas,
            Rule::AllOf(_) => RuleKind::AllOf,
            Rule::AnyOf(_) => RuleKind::AnyOf,
            Rule::OneOf(_) => RuleKind::OneOf,
            Rule::Not(_) => RuleKind::Not,
            Rule::Attributes(_) => RuleKind::Attributes,
            Rule::Content(_) => RuleKind::Content,
            Rule::SimpleContent(_) => RuleKind::SimpleContent,
            Rule::Lexical(_) => RuleKind::Lexical,
        }
    }

    /// Añade a `out` los nodos destino de la regla, en orden.
    pub fn targets(&self, out: &mut Vec<NodeId>) {
        match self {
            Rule::Ref(t) | Rule::Not(t) | Rule::SimpleContent(t) => out.push(*t),
            Rule::Items { prefix, rest } => {
                out.extend(prefix);
                out.extend(rest);
            }
            Rule::Properties {
                declared,
                additional,
            } => {
                out.extend(declared.iter().map(|(_, t)| *t));
                out.extend(additional);
            }
            Rule::DependentSchemas(m) => out.extend(m.iter().map(|(_, t)| *t)),
            Rule::AllOf(v) | Rule::AnyOf(v) | Rule::OneOf(v) => out.extend(v),
            Rule::Attributes(attrs) => out.extend(attrs.iter().map(|a| a.ty)),
            Rule::Content(m) => out.extend(m.particles.iter().filter_map(|p| match p.term {
                Term::Element { ty, .. } => Some(ty),
                _ => None,
            })),
            Rule::Reject
            | Rule::Type(_)
            | Rule::Enum(_)
            | Rule::Const(_)
            | Rule::Minimum(_)
            | Rule::Maximum(_)
            | Rule::ExclusiveMinimum(_)
            | Rule::ExclusiveMaximum(_)
            | Rule::MultipleOf(_)
            | Rule::MinLength(_)
            | Rule::MaxLength(_)
            | Rule::Pattern(_)
            | Rule::MinItems(_)
            | Rule::MaxItems(_)
            | Rule::UniqueItems
            | Rule::Required(_)
            | Rule::MinProperties(_)
            | Rule::MaxProperties(_)
            | Rule::DependentRequired(_)
            | Rule::Lexical(_) => {}
        }
    }
}

/// Tipo que debe tener el destino de una arista de una regla `kind` en un
/// nodo de tipo `sort` (invariante I5): en JSON todo es JSON; en XML, el
/// modelo de contenido lleva a elementos y lo demás a textos.
pub const fn target_sort(sort: Sort, kind: RuleKind) -> Sort {
    match (sort, kind) {
        (Sort::Json, _) => Sort::Json,
        (Sort::Element, RuleKind::Content) => Sort::Element,
        _ => Sort::Text,
    }
}

impl Ir {
    /// IR con solo la raíz.
    pub fn new(format: Format, root: Node) -> Ir {
        Ir {
            format,
            nodes: vec![root],
        }
    }

    /// Añade un nodo y devuelve su id.
    pub fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        NodeId(self.nodes.len() - 1)
    }

    /// El nodo `id`.
    ///
    /// # Panics
    ///
    /// Si `id` está fuera de rango, lo que no ocurre con los `NodeId` de una
    /// IR que cumple I2.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    /// Número de aristas (destinos de todas las reglas): con el número de
    /// nodos, el tamaño del grafo.
    pub fn edge_count(&self) -> usize {
        let mut buf = Vec::new();
        let mut n = 0;
        for node in &self.nodes {
            for c in &node.rules {
                buf.clear();
                c.rule.targets(&mut buf);
                n += buf.len();
            }
        }
        n
    }

    /// Ruta de origen del nodo (`#/properties/a`, `#/complexType/Pedido`),
    /// con los segmentos escapados como en RFC 6901 (`~` → `~0`, `/` → `~1`).
    /// Sube por los padres de forma iterativa: O(profundidad + longitud).
    /// Requiere una IR que cumpla I1 y I2; si no, corta en el primer padre
    /// que no sea anterior. Un `id` fuera de rango da `"#"`.
    pub fn location(&self, id: NodeId) -> String {
        let mut steps = Vec::new();
        let mut cur = id.0;
        while let Some(node) = self.nodes.get(cur) {
            steps.push(&node.origin.step);
            match node.origin.parent {
                Some(p) if p.0 < cur => cur = p.0,
                _ => break,
            }
        }
        let mut out = String::from("#");
        for step in steps.iter().rev() {
            match step {
                Step::Root => {}
                Step::Keyword(k) => push_segment(&mut out, k),
                Step::Entry(k, e) => {
                    push_segment(&mut out, k);
                    push_segment(&mut out, e);
                }
                Step::Index(k, i) => {
                    push_segment(&mut out, k);
                    push_segment(&mut out, &i.to_string());
                }
            }
        }
        out
    }
}

fn push_segment(out: &mut String, seg: &str) {
    out.push('/');
    for c in seg.chars() {
        match c {
            '~' => out.push_str("~0"),
            '/' => out.push_str("~1"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `RuleKind::ALL` está en el orden de la declaración y sin huecos: el
    /// verificador indexa por `kind as usize`.
    #[test]
    fn todas_las_clases_en_orden() {
        for (i, k) in RuleKind::ALL.iter().enumerate() {
            assert_eq!(*k as usize, i, "{k:?}");
        }
    }

    /// Las aristas sobre la misma instancia son las de `docs/fase3/ciclos.md`
    /// §2 (`$ref`, `allOf`, `anyOf`, `oneOf`, `not`, `dependentSchemas`); las
    /// de `properties`, `items`, el modelo de contenido y los atributos
    /// consumen entrada.
    #[test]
    fn misma_instancia_como_en_ciclos() {
        let mismas: Vec<&str> = RuleKind::ALL
            .iter()
            .filter(|k| k.same_instance())
            .map(|k| k.name())
            .collect();
        assert_eq!(
            mismas,
            ["$ref", "dependentSchemas", "allOf", "anyOf", "oneOf", "not"]
        );
        // Toda regla con aristas se admite en algún tipo de nodo.
        for k in RuleKind::ALL {
            assert!(
                [Sort::Json, Sort::Element, Sort::Text]
                    .iter()
                    .any(|&s| k.allowed_in(s)),
                "{k:?}"
            );
        }
    }
}
