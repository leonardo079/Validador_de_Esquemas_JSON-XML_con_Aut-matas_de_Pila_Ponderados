//! Lector XML de documentos de esquema: tokens bajo demanda con espacios de
//! nombres sobre el tokenizador XML de la Fase 1.
//!
//! El lexer empuja eventos a un [`Sink`]; el parser LL(1) del esquema (tarea
//! 5.2) necesita *pedir* el siguiente token, igual que el de JSON Schema
//! (`schema-parser/src/token.rs`). [`Reader`] adapta uno al otro: entrega al
//! lexer **un byte cada vez** hasta que el ensamblador completa al menos un
//! token, y se detiene ahí. Los terminales son los de
//! `docs/fase2/gramatica-xml.md` §1 (`STAG`, `ATTR`, `>`, `/>`, `ETAG`,
//! `TEXT`, `PI`).
//!
//! Además comprueba lo que esa gramática deja fuera (§2, «Restricciones que
//! no expresa la gramática», y §3) y lo que añade Namespaces in XML 1.0:
//!
//! - el nombre de cierre es el de apertura, con una pila explícita de
//!   elementos abiertos (sin recursión, D7);
//! - un solo elemento raíz, y fuera de él solo texto de espacios;
//! - atributos únicos por etiqueta, por nombre escrito y por nombre expandido;
//! - `<?xml …?>` solo como lo primero del documento, bien formada y con
//!   `encoding` UTF-8 si lo trae;
//! - ámbito de las declaraciones `xmlns`/`xmlns:p` (otra pila) y resolución
//!   de cada nombre a (URI, nombre local), también la de los QName que
//!   aparecen como **valor** de un atributo ([`Attr::as_qname`]), que se
//!   resuelven al cerrar la etiqueta, con su ámbito.
//!
//! Una etiqueta de apertura es una unidad: sus declaraciones `xmlns` pueden
//! ir después del nombre que las usa, así que `STAG`, sus `ATTR` y su `>` o
//! `/>` se entregan juntos al leer el `>`, ya resueltos.
//!
//! Es solo de tiempo de compilación: reserva memoria (cola de tokens, textos,
//! pilas). Teoría, costo y formato de los errores en
//! `docs/fase2/lector-xsd.md`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use lexer::xml::{Event, XmlLexer};
use lexer::{Error, Pos, Sink, Span};

use crate::error::{clip, ErrorKind, XsdError};

/// Espacio de nombres de XML Schema 1.0.
pub const XS_NS: &str = "http://www.w3.org/2001/XMLSchema";
/// Espacio de nombres del prefijo `xml`, declarado siempre.
pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
/// Espacio de nombres de los atributos `xmlns` y `xmlns:p` (Namespaces in
/// XML 1.0, convención de DOM): así las declaraciones llegan como `ATTR`
/// y el parser las reconoce por su URI.
pub const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

/// Identidad de un espacio de nombres **internado** por un [`Reader`]: el
/// lector guarda un único `Arc<str>` por URI distinto, así que dos `NsId`
/// del mismo lector son iguales si y solo si lo son sus URI. Comparar y
/// calcular el *hash* cuesta O(1) (la dirección del `Arc`), sea cual sea la
/// longitud del URI: es la clave barata para las tablas de símbolos.
///
/// Solo es comparable con identidades del **mismo** lector (las de sus
/// nombres y las de [`Reader::intern`]); las de dos lectores distintos son
/// siempre distintas. Un `NsId` mantiene vivo su `Arc`, así que su dirección
/// no se reutiliza mientras exista.
#[derive(Clone, Debug)]
pub struct NsId(Arc<str>);

impl NsId {
    /// El URI.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for NsId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for NsId {}

impl Hash for NsId {
    fn hash<H: Hasher>(&self, h: &mut H) {
        Arc::as_ptr(&self.0).cast::<u8>().hash(h);
    }
}

/// Mismo espacio de nombres **por contenido** (para comparar tokens de
/// lectores distintos, como hacen las pruebas de tamaño de bloque).
fn same_ns(a: &Option<NsId>, b: &Option<NsId>) -> bool {
    a.as_ref().map(NsId::as_str) == b.as_ref().map(NsId::as_str)
}

/// Nombre resuelto: tal como está escrito y expandido (URI, nombre local).
///
/// La igualdad (`==`) compara el texto escrito y el URI **por contenido**,
/// así que vale entre lectores distintos; para claves de tablas, usar
/// [`Name::ns_id`].
#[derive(Clone, Debug)]
pub struct Name {
    /// El QName escrito (`xs:element`). Privado junto con `local_start`:
    /// los dos forman un invariante (`local_start` es un límite de carácter
    /// de `raw`, justo después del `:`).
    raw: String,
    /// Espacio de nombres internado; `None` si no tiene. Privado: solo el
    /// lector crea identidades internadas.
    ns: Option<NsId>,
    /// Inicio del nombre local en `raw` (0 si no hay prefijo).
    local_start: usize,
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
            && self.local_start == other.local_start
            && same_ns(&self.ns, &other.ns)
    }
}

impl Eq for Name {}

impl Name {
    /// El QName tal como está escrito (`xs:element`).
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// URI del espacio de nombres, si tiene.
    pub fn ns(&self) -> Option<&str> {
        self.ns.as_ref().map(NsId::as_str)
    }

    /// Identidad internada del espacio de nombres (ver [`NsId`]): con
    /// `(ns_id, local)` como clave, buscar un nombre cuesta O(|local|) y no
    /// vuelve a leer el URI.
    pub fn ns_id(&self) -> Option<&NsId> {
        self.ns.as_ref()
    }

    /// Nombre expandido (URI, nombre local) como texto. Para mostrarlo o
    /// compararlo una vez; **no** como clave de una tabla que se consulte
    /// por cada referencia (cada consulta leería el URI entero: usar
    /// [`Name::ns_id`]).
    pub fn expanded(&self) -> (Option<&str>, &str) {
        (self.ns(), self.local())
    }

    /// Nombre local (`element`).
    pub fn local(&self) -> &str {
        &self.raw[self.local_start..]
    }

    /// Prefijo escrito (`xs`), si lo hay.
    pub fn prefix(&self) -> Option<&str> {
        (self.local_start > 0).then(|| &self.raw[..self.local_start - 1])
    }

    /// ¿Es el nombre expandido (`ns`, `local`)?
    pub fn is(&self, ns: &str, local: &str) -> bool {
        self.ns() == Some(ns) && self.local() == local
    }
}

/// Atributo de una etiqueta de apertura.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// Nombre resuelto. Sin prefijo no tiene espacio de nombres (el espacio
    /// por defecto no se aplica a los atributos); `xmlns` y `xmlns:p` están
    /// en [`XMLNS_NS`].
    pub name: Name,
    /// Valor con las referencias resueltas y la normalización de XML 1.0
    /// (§2.11 y §3.3.3, atributos CDATA porque no hay DTD): cada fin de
    /// línea y cada tabulador o salto **literal** pasa a un espacio; los de
    /// una referencia (`&#9;`, `&#10;`, `&#13;`) se conservan. La
    /// normalización según el tipo XSD del atributo (`whiteSpace`
    /// *replace*/*collapse*) es de las tareas siguientes. Privado (se lee
    /// con [`Attr::value`]): `qname` guarda rangos dentro de él.
    value: String,
    /// Rango del valor con sus comillas.
    pub value_span: Span,
    /// El valor resuelto como QName contra el ámbito de su etiqueta.
    qname: QValue,
}

/// Resolución de un valor de atributo como QName, hecha al cerrar su
/// etiqueta (cuando el ámbito es exactamente el suyo). Guarda rangos de
/// `value`, no copias: O(1) de memoria además del valor.
#[derive(Clone, Debug)]
enum QValue {
    /// `value[start..end]` es el QName sin los espacios de los extremos y
    /// `value[local..end]`, su nombre local.
    Resolved {
        ns: Option<NsId>,
        start: usize,
        local: usize,
        end: usize,
    },
    /// No es un QName.
    Invalid,
    /// `value[start..end]` es un prefijo sin declarar.
    Undeclared { start: usize, end: usize },
    /// El prefijo es `xmlns`, que está ligado a [`XMLNS_NS`] y no puede
    /// usarse en un QName (Namespaces in XML 1.0, *Reserved Prefixes*).
    XmlnsPrefix,
}

impl PartialEq for QValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                QValue::Resolved {
                    ns,
                    start,
                    local,
                    end,
                },
                QValue::Resolved {
                    ns: n2,
                    start: s2,
                    local: l2,
                    end: e2,
                },
            ) => same_ns(ns, n2) && (start, local, end) == (s2, l2, e2),
            (QValue::Invalid, QValue::Invalid) | (QValue::XmlnsPrefix, QValue::XmlnsPrefix) => true,
            (QValue::Undeclared { start, end }, QValue::Undeclared { start: s2, end: e2 }) => {
                (start, end) == (s2, e2)
            }
            _ => false,
        }
    }
}

impl Eq for QValue {}

impl Attr {
    /// ¿Es una declaración de espacio de nombres (`xmlns`, `xmlns:p`)?
    pub fn is_namespace_decl(&self) -> bool {
        self.name.ns() == Some(XMLNS_NS)
    }

    /// Valor del atributo (ver el campo). Para normalizarlo (*collapse*),
    /// trabajar sobre una copia o con [`Attr::into_value`]: el `Attr` no se
    /// puede modificar, porque [`Attr::as_qname`] guarda rangos del valor.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// El valor, consumiendo el atributo.
    pub fn into_value(self) -> String {
        self.value
    }

    /// El valor como QName (`type`, `base`, `ref`), resuelto contra el
    /// ámbito de **su** etiqueta: el lector lo resolvió al cerrarla, así que
    /// el resultado no depende de cuándo se llame ni de cuántos tokens haya
    /// leído ya el parser. Sin prefijo toma el espacio por defecto (XSD 1.0,
    /// *QName resolution (Schema Document)*); los espacios de los extremos
    /// no cuentan (`xs:QName` tiene `whiteSpace` *collapse*). Un valor que no
    /// es QName (`InvalidQName`) o con un prefijo sin declarar
    /// (`UndeclaredPrefix`) es un error en el primer carácter del valor (el
    /// que sigue a la comilla). El prefijo `xmlns` da `ReservedNamespace`.
    /// Solo es error si el parser lo pide: el lector resuelve todos los
    /// valores porque no sabe cuáles son QName. El texto del valor que lleva
    /// el error se recorta (ver [`ErrorKind`]): pedirlo muchas veces no
    /// copia un valor largo.
    pub fn as_qname(&self) -> Result<Name, XsdError> {
        let mut at = self.value_span.start;
        at.advance(b'"');
        match &self.qname {
            QValue::Resolved {
                ns,
                start,
                local,
                end,
            } => Ok(Name {
                raw: self.value[*start..*end].to_owned(),
                ns: ns.clone(),
                local_start: local - start,
            }),
            QValue::Invalid => err(ErrorKind::InvalidQName(clip(&self.value)), at),
            QValue::Undeclared { start, end } => err(
                ErrorKind::UndeclaredPrefix(clip(&self.value[*start..*end])),
                at,
            ),
            QValue::XmlnsPrefix => err(
                ErrorKind::ReservedNamespace("el prefijo xmlns no puede usarse en un QName"),
                at,
            ),
        }
    }
}

/// Terminal de la gramática XML (`docs/fase2/gramatica-xml.md` §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// `<nombre`.
    STag(Name),
    /// `nombre="valor"`.
    Attr(Attr),
    /// `>` de una etiqueta de apertura.
    Gt,
    /// `/>`.
    EmptyClose,
    /// `</nombre>`, con el nombre resuelto de su etiqueta de apertura.
    ETag(Name),
    /// Texto máximo entre marcas (comentarios descartados, CDATA y
    /// referencias resueltas).
    Text(String),
    /// Instrucción de procesamiento, incluida la declaración `<?xml …?>`.
    /// `data` es el contenido tras el espacio que sigue al nombre.
    Pi { target: String, data: String },
    /// Fin de la entrada (`$`).
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    /// `STAG`: de `<` al fin del nombre; `ATTR`: del nombre a la comilla de
    /// cierre; `>` y `/>`: esos bytes; `ETAG` y `PI`: la marca entera;
    /// `TEXT`: del primer al último carácter.
    pub span: Span,
}

// ------------------------------------------------------------- ámbito ---

/// Declaraciones de espacio de nombres en vigor. Cada prefijo (la cadena
/// vacía es el espacio por defecto) tiene una pila de URI; `undo` guarda los
/// prefijos ligados en orden, y una marca (`undo.len()`) por elemento
/// abierto permite deshacer las suyas al cerrarlo. Buscar un prefijo cuesta
/// O(|prefijo|) esperado, sin importar cuántas declaraciones haya.
///
/// Los URI se **internan** (`uris`): dos declaraciones con el mismo URI
/// comparten el mismo `Arc`, así que su identidad ([`NsId`]) identifica el
/// URI. Internar cuesta O(|URI|) y se cobra una vez a su declaración;
/// después, comparar o usar como clave un `NsId` cuesta O(1) sea cual sea
/// la longitud del URI.
#[derive(Debug)]
struct Scope {
    map: HashMap<String, Vec<Option<NsId>>>,
    undo: Vec<String>,
    uris: HashSet<Arc<str>>,
}

impl Scope {
    fn new() -> Self {
        let mut s = Self {
            map: HashMap::new(),
            undo: Vec::new(),
            uris: HashSet::new(),
        };
        let xml = s.intern(XML_NS);
        s.map.insert("xml".to_owned(), vec![Some(xml)]);
        s
    }

    /// La identidad única de un URI: O(|uri|) esperado.
    fn intern(&mut self, uri: &str) -> NsId {
        if let Some(a) = self.uris.get(uri) {
            return NsId(a.clone());
        }
        let a: Arc<str> = Arc::from(uri);
        self.uris.insert(a.clone());
        NsId(a)
    }

    fn mark(&self) -> usize {
        self.undo.len()
    }

    /// `uri = None`: `xmlns=""` (sin espacio por defecto).
    fn bind(&mut self, prefix: &str, uri: Option<NsId>) {
        self.map.entry(prefix.to_owned()).or_default().push(uri);
        self.undo.push(prefix.to_owned());
    }

    fn pop_to(&mut self, mark: usize) {
        while self.undo.len() > mark {
            let Some(p) = self.undo.pop() else { break };
            if let Some(v) = self.map.get_mut(&p) {
                v.pop();
                if v.is_empty() {
                    self.map.remove(&p);
                }
            }
        }
    }

    /// `None`: prefijo sin declarar; `Some(None)`: sin espacio de nombres.
    fn lookup(&self, prefix: &str) -> Option<Option<NsId>> {
        self.map.get(prefix).and_then(|v| v.last()).cloned()
    }

    /// Espacio de nombres de un QName con el prefijo dado. Sin prefijo, el
    /// espacio por defecto si `default` (elementos y valores), ninguno si no
    /// (atributos).
    fn resolve(&self, prefix: Option<&str>, default: bool) -> Result<Option<NsId>, ()> {
        match prefix {
            None if default => Ok(self.lookup("").flatten()),
            None => Ok(None),
            // `xmlns` nunca está ligado: sus usos válidos (declaraciones) se
            // tratan aparte.
            Some("xmlns") => Err(()),
            Some(p) => self.lookup(p).ok_or(()),
        }
    }
}

impl Scope {
    /// Un valor de atributo como QName (ver [`Attr::as_qname`]): O(|valor|)
    /// para recortar y separar, más la búsqueda del prefijo.
    fn resolve_value(&self, value: &str) -> QValue {
        let t = value.trim_matches(is_xml_space);
        let start = value.len() - value.trim_start_matches(is_xml_space).len();
        let Some(ls) = split_qname(t) else {
            return QValue::Invalid;
        };
        let prefix = (ls > 0).then(|| &t[..ls - 1]);
        if prefix == Some("xmlns") {
            return QValue::XmlnsPrefix;
        }
        match self.resolve(prefix, true) {
            Ok(ns) => QValue::Resolved {
                ns,
                start,
                local: start + ls,
                end: start + t.len(),
            },
            Err(()) => QValue::Undeclared {
                start,
                end: start + ls.saturating_sub(1),
            },
        }
    }
}

/// Longitud en bytes de un rango (1 para un carácter literal ASCII; más para
/// una referencia `&#…;` o un carácter UTF-8 de varios bytes).
fn span_len(span: Span) -> u64 {
    span.end.saturating_sub(span.start.offset)
}

/// ¿Es un NCName? Con la misma simplificación que el lexer
/// (`lexer/src/xml.rs`): cualquier carácter no ASCII vale como letra.
fn is_ncname(s: &str) -> bool {
    let mut cs = s.chars();
    let Some(c0) = cs.next() else { return false };
    (c0.is_ascii_alphabetic() || c0 == '_' || !c0.is_ascii())
        && cs.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') || !c.is_ascii())
}

/// Si `s` es un QName (`NCName` o `NCName:NCName`), el inicio del nombre local.
fn split_qname(s: &str) -> Option<usize> {
    match s.find(':') {
        None => is_ncname(s).then_some(0),
        Some(i) => (is_ncname(&s[..i]) && is_ncname(&s[i + 1..])).then_some(i + 1),
    }
}

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

// -------------------------------------------------------- ensamblador ---

/// Dónde van los `NameChar` y `AttrValueChar` que llegan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum In {
    Nothing,
    STagName,
    AttrName,
    ETagName,
    PiTarget,
}

/// Atributo tal como se leyó, antes de cerrar la etiqueta.
#[derive(Debug)]
struct RawAttr {
    name: String,
    at: Pos,
    value: String,
    value_span: Span,
}

/// Elemento abierto: su nombre resuelto y la marca de su ámbito.
#[derive(Debug)]
struct Open {
    name: Name,
    mark: usize,
}

/// Arma tokens completos con los eventos del lexer y comprueba la buena
/// formación y los espacios de nombres. Devuelve el primer error como error
/// del [`Sink`], lo que detiene el lexer.
#[derive(Debug)]
struct Assembler {
    queue: VecDeque<Token>,
    scope: Scope,
    /// Pila de elementos abiertos.
    open: Vec<Open>,
    root_seen: bool,
    /// Posición del byte que se está entregando y del anterior.
    here: Pos,
    prev: Pos,
    /// [`XMLNS_NS`], internado en `scope`.
    xmlns_ns: NsId,

    text: String,
    text_start: Pos,
    text_end: u64,
    /// Offset siguiente a un `\r` literal: un `\n` literal ahí es el resto
    /// de un fin de línea `\r\n` (XML 1.0 §2.11).
    cr_end: Option<u64>,

    into: In,
    tag_lt: Pos,
    tag_name: String,
    tag_name_at: Pos,
    tag_name_end: u64,
    attrs: Vec<RawAttr>,

    etag_name: String,
    etag_name_at: Pos,

    /// `<` de la instrucción en curso.
    pi_lt: Pos,
    pi_target: String,
    /// El nombre ya se comprobó (al llegar el primer carácter del contenido
    /// o, si no tiene, al cerrar).
    pi_target_done: bool,
    pi_data: String,
    /// Posición del primer carácter de `pi_data`: con ella y los bytes de
    /// `pi_data` (sin normalizar en `<?xml …?>`) se reconstruye la de
    /// cualquier otro, sin guardar una posición por carácter.
    pi_first: Option<Pos>,
}

/// Conserva el error de menor offset (el primero en el orden de la entrada).
fn keep(first: &mut Option<XsdError>, kind: ErrorKind, at: Pos) {
    if first.as_ref().is_none_or(|e| at.offset < e.at.offset) {
        *first = Some(XsdError {
            kind: kind.clipped(),
            at,
        });
    }
}

/// Todo error del lector pasa por aquí o por [`keep`]: el texto del usuario
/// que lleva se recorta ([`ErrorKind::clipped`]).
fn err<T>(kind: ErrorKind, at: Pos) -> Result<T, XsdError> {
    Err(XsdError {
        kind: kind.clipped(),
        at,
    })
}

impl Assembler {
    fn new() -> Self {
        let mut scope = Scope::new();
        let xmlns_ns = scope.intern(XMLNS_NS);
        Self {
            queue: VecDeque::new(),
            scope,
            open: Vec::new(),
            root_seen: false,
            here: Pos::START,
            prev: Pos::START,
            xmlns_ns,
            text: String::new(),
            text_start: Pos::START,
            text_end: 0,
            cr_end: None,
            into: In::Nothing,
            tag_lt: Pos::START,
            tag_name: String::new(),
            tag_name_at: Pos::START,
            tag_name_end: 0,
            attrs: Vec::new(),
            etag_name: String::new(),
            etag_name_at: Pos::START,
            pi_lt: Pos::START,
            pi_target: String::new(),
            pi_target_done: false,
            pi_data: String::new(),
            pi_first: None,
        }
    }

    fn push(&mut self, tok: Tok, start: Pos, end: u64) {
        self.queue.push_back(Token {
            tok,
            span: Span { start, end },
        });
    }

    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            let t = std::mem::take(&mut self.text);
            self.push(Tok::Text(t), self.text_start, self.text_end);
        }
    }

    fn text_char(&mut self, c: char, span: Span) -> Result<(), XsdError> {
        // Fuera de la raíz solo espacios literales: una referencia (`&#32;`)
        // ocupa más de un byte y no es `S` (XML 1.0, producción `Misc`).
        if self.open.is_empty() && (!is_xml_space(c) || span_len(span) != 1) {
            return err(ErrorKind::TextOutsideRoot, span.start);
        }
        if self.text.is_empty() {
            self.text_start = span.start;
        }
        if let Some(c) = self.line_end(c, span) {
            self.text.push(c);
        }
        self.text_end = span.end;
        Ok(())
    }

    /// Fines de línea de XML 1.0 §2.11: `\r\n` y `\r` literales pasan a
    /// `\n`; un carácter de una referencia (su rango mide más de un byte) se
    /// conserva. `None`: el `\n` de un `\r\n` ya convertido.
    fn line_end(&mut self, c: char, span: Span) -> Option<char> {
        let literal = span_len(span) == 1;
        match c {
            '\n' if literal && self.cr_end == Some(span.start.offset) => None,
            '\r' if literal => {
                self.cr_end = Some(span.end);
                Some('\n')
            }
            _ => Some(c),
        }
    }

    /// Carácter de un valor de atributo: fines de línea, y después §3.3.3
    /// (cada espacio literal `\t` o `\n` pasa a ` `).
    fn value_char(&mut self, c: char, span: Span) {
        let literal = span_len(span) == 1;
        let Some(c) = self.line_end(c, span) else {
            return;
        };
        let c = if literal && matches!(c, '\t' | '\n') {
            ' '
        } else {
            c
        };
        if let Some(a) = self.attrs.last_mut() {
            a.value.push(c);
        }
    }

    fn name_char(&mut self, c: char, span: Span) {
        let (buf, at) = match self.into {
            In::STagName => {
                self.tag_name_end = span.end;
                (&mut self.tag_name, &mut self.tag_name_at)
            }
            In::AttrName => match self.attrs.last_mut() {
                Some(a) => {
                    a.name.push(c);
                    return;
                }
                None => return,
            },
            In::ETagName => (&mut self.etag_name, &mut self.etag_name_at),
            In::PiTarget => {
                self.pi_target.push(c);
                return;
            }
            In::Nothing => return,
        };
        if buf.is_empty() {
            *at = span.start;
        }
        buf.push(c);
    }

    /// `>` o `/>`: comprueba la etiqueta entera, abre su ámbito y entrega sus
    /// tokens. Si hay varios errores, el de menor offset.
    fn close_start_tag(&mut self, empty: bool) -> Result<(), XsdError> {
        let mut first = None;
        let attrs = std::mem::take(&mut self.attrs);

        // 1. Sintaxis de QName del elemento y de los atributos, y nombres
        //    escritos repetidos.
        let el_split = split_qname(&self.tag_name);
        match el_split {
            None => keep(
                &mut first,
                ErrorKind::InvalidQName(self.tag_name.clone()),
                self.tag_name_at,
            ),
            Some(ls) if ls > 0 && &self.tag_name[..ls - 1] == "xmlns" => keep(
                &mut first,
                ErrorKind::ReservedNamespace("el prefijo xmlns no puede usarse en un elemento"),
                self.tag_name_at,
            ),
            Some(_) => {}
        }
        let splits: Vec<Option<usize>> = attrs.iter().map(|a| split_qname(&a.name)).collect();
        let mut seen = HashSet::with_capacity(attrs.len());
        for (a, s) in attrs.iter().zip(&splits) {
            if s.is_none() {
                keep(&mut first, ErrorKind::InvalidQName(a.name.clone()), a.at);
            }
            if !seen.insert(a.name.as_str()) {
                keep(
                    &mut first,
                    ErrorKind::DuplicateAttribute(a.name.clone()),
                    a.at,
                );
            }
        }

        // 2. Declaraciones de espacio de nombres (se ligan aunque sean
        //    inválidas: el error ya está anotado y así no se informa además
        //    un prefijo «sin declarar»).
        let mark = self.scope.mark();
        for (a, s) in attrs.iter().zip(&splits) {
            let prefix = match (a.name.as_str(), s) {
                ("xmlns", _) => "",
                (n, Some(ls)) if *ls > 0 && &n[..ls - 1] == "xmlns" => &n[*ls..],
                _ => continue,
            };
            let v = a.value.as_str();
            let problema = if prefix == "xmlns" {
                Some("el prefijo xmlns no se puede declarar")
            } else if prefix == "xml" && v != XML_NS {
                Some("el prefijo xml solo puede ligarse a http://www.w3.org/XML/1998/namespace")
            } else if prefix != "xml" && v == XML_NS {
                Some("solo el prefijo xml puede ligarse a http://www.w3.org/XML/1998/namespace")
            } else if v == XMLNS_NS {
                Some("ningún prefijo puede ligarse a http://www.w3.org/2000/xmlns/")
            } else if !prefix.is_empty() && v.is_empty() {
                Some("xmlns:p=\"\" no está permitido en Namespaces in XML 1.0")
            } else {
                None
            };
            if let Some(m) = problema {
                keep(&mut first, ErrorKind::ReservedNamespace(m), a.at);
            }
            let uri = (!v.is_empty() || !prefix.is_empty()).then(|| self.scope.intern(v));
            self.scope.bind(prefix, uri);
        }

        // 3. Resolución del elemento y de los atributos; nombres expandidos
        //    repetidos.
        let raw = std::mem::take(&mut self.tag_name);
        let el_ls = el_split.unwrap_or(0);
        let el_prefix = (el_ls > 0).then(|| &raw[..el_ls - 1]);
        let el_ns = match self.scope.resolve(el_prefix, true) {
            Ok(ns) => ns,
            Err(()) => {
                if el_split.is_some() && el_prefix != Some("xmlns") {
                    let p = el_prefix.unwrap_or_default().to_owned();
                    keep(&mut first, ErrorKind::UndeclaredPrefix(p), self.tag_name_at);
                }
                None
            }
        };
        let mut resolved = Vec::with_capacity(attrs.len());
        // Clave (identidad del URI internado, nombre local): O(|local|) por
        // atributo, sin volver a leer el URI (que puede ser largo).
        let mut expanded: HashSet<(NsId, &str)> = HashSet::new();
        for (a, s) in attrs.iter().zip(&splits) {
            let ls = s.unwrap_or(0);
            let prefix = (ls > 0).then(|| &a.name[..ls - 1]);
            let ns = if a.name == "xmlns" || prefix == Some("xmlns") {
                Some(self.xmlns_ns.clone())
            } else {
                match self.scope.resolve(prefix, false) {
                    Ok(ns) => ns,
                    Err(()) => {
                        if s.is_some() {
                            let p = prefix.unwrap_or_default().to_owned();
                            keep(&mut first, ErrorKind::UndeclaredPrefix(p), a.at);
                        }
                        None
                    }
                }
            };
            if let Some(uri) = &ns {
                if prefix.is_some() && !expanded.insert((uri.clone(), &a.name[ls..])) {
                    keep(
                        &mut first,
                        ErrorKind::DuplicateAttribute(a.name.clone()),
                        a.at,
                    );
                }
            }
            resolved.push(ns);
        }
        if let Some(e) = first {
            return Err(e);
        }

        // 4. Tokens y pilas. Los valores se resuelven como QName ahora, con
        //    el ámbito de esta etiqueta ya completo (`Attr::as_qname`).
        let name = Name {
            raw,
            ns: el_ns,
            local_start: el_ls,
        };
        self.push(Tok::STag(name.clone()), self.tag_lt, self.tag_name_end);
        for ((a, s), ns) in attrs.into_iter().zip(splits).zip(resolved) {
            let start = a.at;
            let end = a.value_span.end;
            let qname = self.scope.resolve_value(&a.value);
            let attr = Attr {
                name: Name {
                    raw: a.name,
                    ns,
                    local_start: s.unwrap_or(0),
                },
                value: a.value,
                value_span: a.value_span,
                qname,
            };
            self.push(Tok::Attr(attr), start, end);
        }
        let end = self.here.offset.saturating_add(1);
        if empty {
            self.push(Tok::EmptyClose, self.prev, end);
            self.scope.pop_to(mark);
        } else {
            self.push(Tok::Gt, self.here, end);
            self.open.push(Open { name, mark });
        }
        self.root_seen = true;
        Ok(())
    }

    fn close_end_tag(&mut self, span: Span) -> Result<(), XsdError> {
        let close = std::mem::take(&mut self.etag_name);
        let Some(o) = self.open.pop() else {
            return err(ErrorKind::UnopenedEndTag(close), self.etag_name_at);
        };
        if o.name.raw != close {
            return err(
                ErrorKind::MismatchedEndTag {
                    open: o.name.raw,
                    close,
                },
                self.etag_name_at,
            );
        }
        self.scope.pop_to(o.mark);
        self.push(Tok::ETag(o.name), span.start, span.end);
        Ok(())
    }

    /// El nombre de la instrucción está completo: `xml` solo en la línea 1,
    /// columna 1 (offset 0, o 3 tras un BOM, que no ocupa columna; nada
    /// delante), y ningún otro nombre `[Xx][Mm][Ll]`. Se comprueba en cuanto
    /// se sabe, sin esperar al `?>`.
    fn check_pi_target(&mut self) -> Result<(), XsdError> {
        self.pi_target_done = true;
        let t = &self.pi_target;
        if t == "xml" {
            if (self.pi_lt.line, self.pi_lt.column) != (1, 1) {
                return err(ErrorKind::XmlDeclNotFirst, self.pi_lt);
            }
        } else if t.eq_ignore_ascii_case("xml") {
            return err(ErrorKind::ReservedPiTarget(t.clone()), self.pi_lt);
        }
        Ok(())
    }

    fn pi_char(&mut self, c: char, span: Span) -> Result<(), XsdError> {
        if !self.pi_target_done {
            self.check_pi_target()?;
        }
        if self.pi_first.is_none() {
            self.pi_first = Some(span.start);
        }
        if self.pi_target == "xml" {
            // Sin normalizar: los bytes de `pi_data` son los de la entrada
            // (en una PI no hay referencias) y dan la posición de cada error.
            self.pi_data.push(c);
        } else if let Some(c) = self.line_end(c, span) {
            self.pi_data.push(c);
        }
        Ok(())
    }

    fn close_pi(&mut self, span: Span) -> Result<(), XsdError> {
        if !self.pi_target_done {
            self.check_pi_target()?;
        }
        let target = std::mem::take(&mut self.pi_target);
        let data = std::mem::take(&mut self.pi_data);
        if target == "xml" {
            check_xml_decl(&data, self.pi_first, span.start)?;
        }
        self.push(Tok::Pi { target, data }, span.start, span.end);
        Ok(())
    }

    /// Fin de la entrada (el lexer ya aceptó): un único elemento raíz cerrado.
    fn finish(&mut self, at: Pos) -> Result<(), XsdError> {
        self.flush_text();
        if let Some(o) = self.open.last() {
            return err(ErrorKind::UnclosedElement(o.name.raw.clone()), at);
        }
        if !self.root_seen {
            return err(ErrorKind::NoRootElement, at);
        }
        Ok(())
    }
}

impl Sink<Event> for Assembler {
    type Error = XsdError;

    fn event(&mut self, ev: Event, span: Span) -> Result<(), XsdError> {
        match ev {
            Event::TextChar(c) => return self.text_char(c, span),
            Event::NameChar(c) => self.name_char(c, span),
            Event::StartTagOpen => {
                self.flush_text();
                if self.open.is_empty() && self.root_seen {
                    return err(ErrorKind::MultipleRootElements, span.start);
                }
                self.into = In::STagName;
                self.tag_lt = span.start;
                self.tag_name.clear();
            }
            Event::AttrBegin => {
                self.into = In::AttrName;
                self.attrs.push(RawAttr {
                    name: String::new(),
                    at: span.start,
                    value: String::new(),
                    value_span: span,
                });
            }
            Event::AttrValueBegin => self.into = In::Nothing,
            Event::AttrValueChar(c) => self.value_char(c, span),
            Event::AttrValueEnd => {
                if let Some(a) = self.attrs.last_mut() {
                    a.value_span = span;
                }
            }
            Event::StartTagClose => {
                self.into = In::Nothing;
                return self.close_start_tag(false);
            }
            Event::EmptyTagClose => {
                self.into = In::Nothing;
                return self.close_start_tag(true);
            }
            Event::EndTagOpen => {
                self.flush_text();
                self.into = In::ETagName;
            }
            Event::EndTagClose => {
                self.into = In::Nothing;
                return self.close_end_tag(span);
            }
            Event::PiOpen => {
                self.flush_text();
                self.into = In::PiTarget;
                self.pi_lt = span.start;
                self.pi_target.clear();
                self.pi_target_done = false;
                self.pi_data.clear();
                self.pi_first = None;
            }
            Event::PiChar(c) => {
                self.into = In::Nothing;
                return self.pi_char(c, span);
            }
            Event::PiClose => {
                self.into = In::Nothing;
                return self.close_pi(span);
            }
        }
        Ok(())
    }
}

/// Declaración XML 1.0 (producciones 23-32): `version`, `encoding` y
/// `standalone`, en ese orden, separados por espacios; `version` es `1.`
/// seguido de dígitos y `encoding`, si está, UTF-8 (sin distinguir
/// mayúsculas). `data` es el contenido tras el primer espacio, con los bytes
/// de la entrada (la sintaxis es ASCII, así que se recorre por bytes);
/// `first`, la posición de su primer carácter; `fin`, la de la declaración
/// (para lo que falta al final). La posición de un error se reconstruye
/// avanzando `first` sobre los bytes anteriores, una sola vez.
fn check_xml_decl(data: &str, first: Option<Pos>, fin: Pos) -> Result<(), XsdError> {
    let b = data.as_bytes();
    let at = |i: usize| match first {
        Some(mut p) if i < b.len() => {
            b[..i].iter().for_each(|&x| p.advance(x));
            p
        }
        _ => fin,
    };
    let bad = |i: usize, m: &'static str| err(ErrorKind::BadXmlDecl(m), at(i));
    let is_ws = |x: u8| matches!(x, b' ' | b'\t' | b'\n' | b'\r');
    let skip_ws = |mut i: usize| {
        while i < b.len() && is_ws(b[i]) {
            i += 1;
        }
        i
    };
    let mut i = 0;
    let mut seen = 0; // 1 version, 2 encoding, 3 standalone
    loop {
        let ws = i;
        i = skip_ws(i);
        if i == b.len() {
            break;
        }
        // El espacio que separa `xml` del primer pseudoatributo ya lo
        // consumió el lexer.
        if seen > 0 && i == ws {
            return bad(i, "falta un espacio entre los pseudoatributos");
        }
        let n0 = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        let order = match &b[n0..i] {
            b"version" => 1,
            b"encoding" => 2,
            b"standalone" => 3,
            _ => return bad(n0, "se admiten version, encoding y standalone"),
        };
        if seen == 0 && order != 1 {
            return bad(n0, "falta version");
        }
        if order <= seen {
            return bad(n0, "pseudoatributo repetido o fuera de orden");
        }
        i = skip_ws(i);
        if b.get(i) != Some(&b'=') {
            return bad(i, "se esperaba =");
        }
        i = skip_ws(i + 1);
        let q = match b.get(i) {
            Some(&q @ (b'"' | b'\'')) => q,
            _ => return bad(i, "se esperaba una comilla"),
        };
        let v0 = i + 1;
        let Some(len) = b[v0..].iter().position(|&c| c == q) else {
            return bad(i, "comilla sin cerrar");
        };
        // Límites en comillas ASCII: `data[v0..v0 + len]` es un `str` válido.
        let value = &data[v0..v0 + len];
        match order {
            1 => {
                let ok = value
                    .strip_prefix("1.")
                    .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
                if !ok {
                    return bad(v0, "version debe ser 1.x");
                }
            }
            2 => {
                if !value.eq_ignore_ascii_case("UTF-8") {
                    return err(ErrorKind::UnsupportedEncoding(value.to_owned()), at(v0));
                }
            }
            _ => {
                if value != "yes" && value != "no" {
                    return bad(v0, "standalone debe ser yes o no");
                }
            }
        }
        seen = order;
        i = v0 + len + 1;
    }
    if seen == 0 {
        return bad(b.len(), "falta version");
    }
    Ok(())
}

// --------------------------------------------------------------- lector ---

/// Flujo de tokens bajo demanda sobre una entrada en bloques.
///
/// Entrega al lexer un byte cada vez y se detiene en cuanto hay un token, así
/// que el resultado no depende de cómo se parte la entrada. Todo lo que
/// depende del ámbito (nombres, valores QName) se resuelve al cerrar cada
/// etiqueta, así que tampoco depende de cuántos tokens pida por adelantado
/// el parser.
pub struct Reader<I: Iterator>
where
    I::Item: AsRef<[u8]>,
{
    chunks: I,
    cur: Option<I::Item>,
    /// Índice del próximo byte de `cur`.
    idx: usize,
    lexer: XmlLexer,
    sink: Assembler,
    finished: bool,
    /// Primer error: se entrega después de los tokens ya armados y se repite
    /// si se insiste.
    pending: Option<XsdError>,
}

impl<I: Iterator> fmt::Debug for Reader<I>
where
    I::Item: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reader")
            .field("position", &self.lexer.position())
            .field("queued", &self.sink.queue.len())
            .field("depth", &self.sink.open.len())
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl<'a> Reader<std::iter::Once<&'a [u8]>> {
    /// Lector sobre una entrada en memoria (un solo bloque: el lector la
    /// recorre byte a byte de todos modos).
    pub fn new(input: &'a [u8]) -> Self {
        Self::from_chunks(std::iter::once(input))
    }
}

impl<I: Iterator> Reader<I>
where
    I::Item: AsRef<[u8]>,
{
    /// Lector sobre una sucesión de bloques de cualquier tamaño (también
    /// vacíos).
    pub fn from_chunks(chunks: I) -> Self {
        Self {
            chunks,
            cur: None,
            idx: 0,
            lexer: XmlLexer::new(),
            sink: Assembler::new(),
            finished: false,
            pending: None,
        }
    }

    /// Interna un URI en la misma tabla que los del documento (por ejemplo
    /// el `targetNamespace` de `xs:schema`, que llega como valor): su
    /// [`NsId`] es igual al de todo nombre del documento con ese URI.
    /// O(|uri|).
    pub fn intern(&mut self, uri: &str) -> NsId {
        self.sink.scope.intern(uri)
    }

    fn next_byte(&mut self) -> Option<u8> {
        loop {
            if let Some(&b) = self.cur.as_ref().and_then(|c| c.as_ref().get(self.idx)) {
                self.idx += 1;
                return Some(b);
            }
            self.cur = Some(self.chunks.next()?);
            self.idx = 0;
        }
    }

    /// Siguiente token. Tras `Eof` devuelve siempre `Eof`; tras un error,
    /// siempre el mismo error.
    pub fn next_token(&mut self) -> Result<Token, XsdError> {
        loop {
            if let Some(t) = self.sink.queue.pop_front() {
                return Ok(t);
            }
            if let Some(e) = &self.pending {
                return Err(e.clone());
            }
            if self.finished {
                let at = self.lexer.position();
                return Ok(Token {
                    tok: Tok::Eof,
                    span: Span {
                        start: at,
                        end: at.offset,
                    },
                });
            }
            let res = match self.next_byte() {
                Some(b) => {
                    self.sink.prev = self.sink.here;
                    self.sink.here = self.lexer.position();
                    self.lexer.feed(&[b], &mut self.sink)
                }
                None => {
                    self.finished = true;
                    let at = self.lexer.position();
                    self.lexer
                        .finish(&mut self.sink)
                        .and_then(|()| self.sink.finish(at).map_err(Error::Sink))
                }
            };
            if let Err(e) = res {
                self.pending = Some(match e {
                    Error::Lex(e) => XsdError {
                        kind: ErrorKind::Lex(e.kind),
                        at: e.at,
                    },
                    Error::Sink(e) => e,
                });
            }
        }
    }
}
