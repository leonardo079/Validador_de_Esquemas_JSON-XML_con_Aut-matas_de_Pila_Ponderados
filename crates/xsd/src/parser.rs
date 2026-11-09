//! Parser LL(1) descendente recursivo del subconjunto XSD → AST (tarea 5.2).
//!
//! Una función por construcción de `docs/fase2/xsd.ll1`; cada una decide su
//! producción mirando un solo token (`self.look`), como indica la tabla de
//! `docs/fase2/ll1-xsd.md`. Las listas (`top_items`, `particles`, `facets`,
//! `attr_decls`…) se recorren con un bucle en lugar de con recursión por la
//! cola. Los terminales son los tokens del lector de la 5.1 ([`Reader`]), con
//! el `STAG` del espacio de nombres de XSD refinado en una clase `S_*` por su
//! nombre local ([`classify`]), como las clases `K_*` de `esquema.ll1`.
//!
//! Antes de la tabla se descartan las instrucciones de procesamiento y los
//! `TEXT` de solo espacios (igual que el lexer descarta los comentarios); un
//! `TEXT` con otro contenido solo está en la fila de `any` (el contenido de
//! `xs:documentation`/`xs:appinfo`), así que en cualquier otro sitio es una
//! celda vacía: texto en un contenido de solo elementos.
//!
//! **Profundidad (D7).** El lector corta en el `<` del elemento que pasa el
//! tope ([`Reader::with_max_depth`]), sin leer más. La recursión del parser
//! sigue el anidamiento de los elementos (un marco de construcción por
//! nivel), así que está acotada por el mismo tope; el contenido de
//! `xs:documentation`/`xs:appinfo` se salta con un contador, sin recursión.
//! El costo de pila por nivel está medido en `docs/fase2/gramatica-xsd.md` §8.

use std::mem;

use lexer::{Pos, Span};

use crate::ast::{
    AnonymousType, Attribute, ComplexContent, ComplexType, Compositor, Element, Facet, FacetKind,
    FormDefault, Group, Particle, Restriction, Schema, SimpleContent, SimpleType, Spanned,
    TopLevel, Use,
};
use crate::error::{clip, ErrorKind, Found, XsdError};
use crate::reader::{is_xml_space, value_start};
use crate::subset::{
    classify, default_only, unsupported_attribute, Class, Ctx, Xs, SIMPLE_CONTENT_RESTRICTION,
};
use crate::{Attr, Name, NsId, Reader, Tok, Token, XS_NS};

/// Profundidad máxima de anidamiento de elementos por defecto, la misma de
/// los payloads y del parser de JSON Schema (`docs/00-alcance.md` §4).
pub const DEFAULT_MAX_DEPTH: u32 = 256;

/// Tope de la profundidad configurable con [`parse_with_max_depth`] (D7).
///
/// Es la profundidad cuyo peor caso se midió y cabe en un hilo de 2 MiB de
/// pila también sin optimizar (`docs/fase2/gramatica-xsd.md` §8). Igual que
/// en `schema-parser`, la garantía vale en hilos con **al menos 2 MiB de
/// pila**: `schemac` y el servidor llaman al parser en un hilo creado con
/// `std::thread::Builder::stack_size`, nunca en el hilo principal.
pub const MAX_DEPTH_LIMIT: u32 = DEFAULT_MAX_DEPTH;

/// Analiza un documento de esquema XSD. Devuelve el AST o el primer error,
/// con su línea y columna.
pub fn parse(input: &[u8]) -> Result<Schema, XsdError> {
    parse_with_max_depth(input, DEFAULT_MAX_DEPTH)
}

/// Como [`parse`], con otra profundidad máxima de anidamiento de elementos.
/// `max_depth` se recorta a [`MAX_DEPTH_LIMIT`] (como en `schema-parser`):
/// si la entrada lo supera, el error `limit` informa el máximo efectivo.
/// `xs:schema` cuenta como el primer nivel, así que con `max_depth = 0`
/// todo esquema es el error `limit` en el `<` de `xs:schema`.
pub fn parse_with_max_depth(input: &[u8], max_depth: u32) -> Result<Schema, XsdError> {
    parse_chunks(std::iter::once(input), max_depth)
}

/// Como [`parse_with_max_depth`], sobre una entrada en bloques de cualquier
/// tamaño: el resultado no depende de la partición (el lector la recorre
/// byte a byte) y el análisis se detiene en el primer error sin pedir más
/// bloques.
pub fn parse_chunks<I>(chunks: I, max_depth: u32) -> Result<Schema, XsdError>
where
    I: Iterator,
    I::Item: AsRef<[u8]>,
{
    let reader = Reader::from_chunks(chunks).with_max_depth(max_depth.min(MAX_DEPTH_LIMIT));
    let mut p = Parser {
        r: reader,
        look: Token {
            tok: Tok::Eof,
            span: Span {
                start: Pos::START,
                end: 0,
            },
        },
        last_end: 0,
        element_form: FormDefault::Unqualified,
    };
    p.document().map_err(|e| *e)
}

/// Dentro del parser el error va en una caja, para que los marcos de la
/// recursión sean pequeños (como en `schema-parser`).
type Res<T> = Result<T, Box<XsdError>>;

#[cold]
#[inline(never)]
fn fail<T>(kind: ErrorKind, at: Pos) -> Res<T> {
    Err(Box::new(XsdError::new(kind, at)))
}

/// *Collapse* de XSD (`whiteSpace`): sin espacios en los extremos y cada
/// secuencia interior reducida a un espacio. Trabaja sobre una copia: el
/// `Attr` no se modifica (`Attr::as_qname` guarda rangos de su valor).
fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for w in s.split(is_xml_space).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    out
}

/// Valor de un atributo admitido, convertido **al llegar** (en el orden del
/// documento, CR-I2): así el primer error de una etiqueta es el del primer
/// atributo que lo tiene, sea cual sea el orden en que el código pide los
/// valores después.
#[derive(Debug)]
enum Val {
    /// *Collapse* sobre una copia (`name`, `minOccurs`, `maxOccurs`, facetas
    /// de rango y de longitud).
    Text(Spanned<String>),
    /// Tal como lo entrega el lector (`default`, `fixed`, `xs:enumeration`,
    /// `xs:pattern`: su *whiteSpace* depende del tipo).
    Raw(Spanned<String>),
    /// QName resuelto por el lector (`type`, `base`, `ref`): una sola
    /// llamada a `as_qname` por atributo.
    QName(Spanned<Name>),
    Use(Spanned<Use>),
    Form(Spanned<FormDefault>),
    /// `targetNamespace` recortado e internado.
    Ns(Spanned<NsId>),
    /// `mixed="false"`: no deja nada en el AST.
    Nothing,
}

/// Atributos admitidos de una etiqueta, por su nombre en la lista blanca, ya
/// convertidos.
#[derive(Debug, Default)]
struct Attrs(Vec<(&'static str, Val)>);

impl Attrs {
    fn take(&mut self, name: &str) -> Option<Val> {
        let i = self.0.iter().position(|(n, _)| *n == name)?;
        Some(self.0.swap_remove(i).1)
    }

    fn text(&mut self, name: &str) -> Option<Spanned<String>> {
        match self.take(name)? {
            Val::Text(v) | Val::Raw(v) => Some(v),
            _ => None,
        }
    }

    fn qname(&mut self, name: &str) -> Option<Spanned<Name>> {
        match self.take(name)? {
            Val::QName(v) => Some(v),
            _ => None,
        }
    }

    fn use_(&mut self) -> Option<Spanned<Use>> {
        match self.take("use")? {
            Val::Use(v) => Some(v),
            _ => None,
        }
    }

    fn form(&mut self) -> Option<Spanned<FormDefault>> {
        match self.take("elementFormDefault")? {
            Val::Form(v) => Some(v),
            _ => None,
        }
    }

    fn namespace(&mut self) -> Option<Spanned<NsId>> {
        match self.take("targetNamespace")? {
            Val::Ns(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Parser<I: Iterator>
where
    I::Item: AsRef<[u8]>,
{
    r: Reader<I>,
    /// Token de anticipación (el único que mira LL(1)).
    look: Token,
    /// Desplazamiento final del último token consumido.
    last_end: u64,
    /// `elementFormDefault` del esquema (por defecto `unqualified`): el
    /// `form` que rige en los `xs:element` locales (D39).
    element_form: FormDefault,
}

impl<I: Iterator> Parser<I>
where
    I::Item: AsRef<[u8]>,
{
    /// Siguiente token de la tabla: descarta las instrucciones de
    /// procesamiento y los `TEXT` de solo espacios.
    fn next_significant(&mut self) -> Res<Token> {
        loop {
            let t = self.r.next_token().map_err(Box::new)?;
            match &t.tok {
                Tok::Pi { .. } => {}
                Tok::Text(s) if s.chars().all(is_xml_space) => {}
                _ => return Ok(t),
            }
        }
    }

    /// Consume el token de anticipación y lee el siguiente.
    fn bump(&mut self) -> Res<Token> {
        let next = self.next_significant()?;
        let t = mem::replace(&mut self.look, next);
        self.last_end = t.span.end;
        Ok(t)
    }

    /// Clase `S_*` del token de anticipación, si es un `STAG` de una
    /// construcción del subconjunto.
    fn look_xs(&self) -> Option<Xs> {
        match &self.look.tok {
            Tok::STag(n) => match classify(n) {
                Class::Xs(x) => Some(x),
                _ => None,
            },
            _ => None,
        }
    }

    /// Celda vacía de la tabla en la fila cuyo contenido es el de `parent`:
    /// el mensaje dice qué admitía (`expected`) o, si el token es un
    /// elemento fuera del subconjunto o ajeno a XSD, o texto, lo dice.
    #[cold]
    #[inline(never)]
    fn unexpected(&self, expected: &'static str, parent: Ctx) -> Box<XsdError> {
        let at = self.look.span.start;
        let kind = match &self.look.tok {
            Tok::Text(_) => ErrorKind::TextNotAllowed {
                parent: parent.describe(),
            },
            Tok::STag(n) => match classify(n) {
                Class::Foreign => ErrorKind::ForeignElement(clip(n.as_str())),
                Class::Unknown => ErrorKind::UnknownElement(clip(n.as_str())),
                Class::Unsupported(category, reason) => ErrorKind::UnsupportedElement {
                    name: clip(n.as_str()),
                    category,
                    reason,
                },
                Class::Xs(_) => ErrorKind::Unexpected {
                    expected,
                    found: Found::Tag(clip(n.as_str())),
                },
            },
            Tok::ETag(n) => ErrorKind::Unexpected {
                expected,
                found: Found::EndTag(clip(n.as_str())),
            },
            Tok::EmptyClose => ErrorKind::Unexpected {
                expected,
                found: Found::EmptyClose,
            },
            // `ATTR` y `>` solo aparecen dentro de una etiqueta (los consume
            // `open`/`tag_end`), `PI` se descarta y `Eof` solo puede llegar
            // tras cerrar la raíz (el lector garantiza la buena formación).
            Tok::Attr(_) | Tok::Gt | Tok::Pi { .. } | Tok::Eof => ErrorKind::Unexpected {
                expected,
                found: Found::Eof,
            },
        };
        Box::new(XsdError::new(kind, at))
    }

    // ------------------------------------------------------- etiquetas ---

    /// `S_X attrs`: consume el `STAG` (el llamador ya comprobó su clase) y
    /// sus atributos, aplicando la lista blanca de `ctx` y convirtiendo cada
    /// valor al llegar. No consume el `>` o `/>`: así todo error de la
    /// etiqueta sale antes que cualquiera de lo que la sigue.
    ///
    /// Un atributo obligatorio que falta solo se sabe al llegar al `>` o
    /// `/>`, y es ahí donde se informa (`MissingAttribute`): así el primer
    /// error es siempre el primero en el orden del documento.
    fn open(&mut self, ctx: Ctx) -> Res<(Pos, Attrs)> {
        let start = self.bump()?.span.start;
        let mut attrs = Attrs::default();
        while let Tok::Attr(_) = self.look.tok {
            let t = self.bump()?;
            if let Tok::Attr(a) = t.tok {
                self.attr(ctx, a, t.span.start, &mut attrs)?;
            }
        }
        for name in ctx.required() {
            if !attrs.0.iter().any(|(n, _)| n == name) {
                return fail(
                    ErrorKind::MissingAttribute {
                        element: ctx.describe(),
                        name,
                    },
                    self.look.span.start,
                );
            }
        }
        Ok((start, attrs))
    }

    /// Un atributo contra la lista blanca de `ctx` (`00-alcance.md` §3.1) y,
    /// si es admitido, su valor convertido.
    fn attr(&mut self, ctx: Ctx, a: Attr, at: Pos, attrs: &mut Attrs) -> Res<()> {
        if a.is_namespace_decl() {
            return Ok(());
        }
        match a.name.ns() {
            None => {}
            // El esquema de esquemas admite atributos de otros espacios de
            // nombres (`anyAttribute namespace="##other"`), no del de XSD:
            // `xs:type="…"` no es un atributo de XSD (mismo error que uno
            // desconocido).
            Some(XS_NS) => {
                return fail(
                    ErrorKind::UnknownAttribute {
                        element: ctx.describe(),
                        name: clip(a.name.as_str()),
                        allowed: ctx.allowed(),
                    },
                    at,
                )
            }
            // D34: de otro espacio de nombres (`xml:lang`, `xsi:…`,
            // extensiones): se acepta y se ignora, como las anotaciones.
            Some(_) => return Ok(()),
        }
        let local = a.name.local();
        match ctx.allowed().iter().find(|n| **n == local) {
            // D23: `id` y `source` se aceptan y se ignoran.
            Some(&"id" | &"source") => Ok(()),
            Some(n) => {
                let v = self.convert(ctx, n, a)?;
                attrs.0.push((n, v));
                Ok(())
            }
            None => match unsupported_attribute(ctx, local) {
                Some((category, reason)) => fail(
                    ErrorKind::UnsupportedAttribute {
                        element: ctx.describe(),
                        name: clip(local),
                        category,
                        reason,
                    },
                    at,
                ),
                None => fail(
                    ErrorKind::UnknownAttribute {
                        element: ctx.describe(),
                        name: clip(local),
                        allowed: ctx.allowed(),
                    },
                    at,
                ),
            },
        }
    }

    /// Valor de un atributo admitido. Cada nombre tiene el mismo tipo en
    /// todas las construcciones que lo admiten, salvo `value`, que en
    /// `xs:enumeration` y `xs:pattern` se guarda sin *collapse*.
    fn convert(&mut self, ctx: Ctx, name: &'static str, a: Attr) -> Res<Val> {
        let span = a.value_span;
        let at = value_start(a.value_span);
        let word = || collapse(a.value());
        let v = match name {
            "type" | "base" | "ref" => Val::QName(Spanned {
                value: a.as_qname().map_err(Box::new)?,
                span,
            }),
            "use" => {
                let w = word();
                let value = match w.as_str() {
                    "required" => Use::Required,
                    "optional" => Use::Optional,
                    "prohibited" => {
                        return fail(
                            ErrorKind::UnsupportedValue {
                                attr: "use",
                                found: w,
                                category: "derivación de tipos complejos",
                                reason: "solo tiene efecto al restringir un tipo complejo \
                                         (xs:complexContent), fuera del subconjunto",
                            },
                            at,
                        )
                    }
                    _ => {
                        return fail(
                            ErrorKind::InvalidValue {
                                attr: "use",
                                expected: "required u optional",
                                found: clip(a.value()),
                            },
                            at,
                        )
                    }
                };
                Val::Use(Spanned { value, span })
            }
            "mixed" => {
                let w = word();
                match w.as_str() {
                    "false" | "0" => Val::Nothing,
                    "true" | "1" => {
                        return fail(
                            ErrorKind::UnsupportedValue {
                                attr: "mixed",
                                found: w,
                                category: "contenido mixto",
                                reason: "texto entre los elementos hijos: el autómata de \
                                         contenido de solo elementos no lo admite \
                                         (00-alcance.md §3.3)",
                            },
                            at,
                        )
                    }
                    _ => {
                        return fail(
                            ErrorKind::InvalidValue {
                                attr: "mixed",
                                expected: "true o false",
                                found: clip(a.value()),
                            },
                            at,
                        )
                    }
                }
            }
            "elementFormDefault" => {
                let w = word();
                let value = match w.as_str() {
                    "qualified" => FormDefault::Qualified,
                    "unqualified" => FormDefault::Unqualified,
                    _ => {
                        return fail(
                            ErrorKind::InvalidValue {
                                attr: "elementFormDefault",
                                expected: "qualified o unqualified",
                                found: clip(a.value()),
                            },
                            at,
                        )
                    }
                };
                Val::Form(Spanned { value, span })
            }
            "targetNamespace" => {
                let uri = word();
                if uri.is_empty() {
                    return fail(
                        ErrorKind::InvalidValue {
                            attr: "targetNamespace",
                            expected: "un URI no vacío (sin espacio de nombres, se omite)",
                            found: clip(a.value()),
                        },
                        at,
                    );
                }
                // 5.1-CR3-I1/SEC3-I2: el único valor que se interna, ya
                // recortado, mientras el lector existe.
                Val::Ns(Spanned {
                    value: self.r.intern(&uri),
                    span,
                })
            }
            // D39: solo el valor sin efecto, que no deja nada en el AST.
            "attributeFormDefault" | "form" | "nillable" | "abstract" => {
                let w = word();
                let form = |w: &str| match w {
                    "qualified" => Some(FormDefault::Qualified),
                    "unqualified" => Some(FormDefault::Unqualified),
                    _ => None,
                };
                let (no_effect, expected) = match name {
                    // `attributeFormDefault` y el `form` de un atributo local:
                    // rige `unqualified`; en un elemento local, el
                    // `elementFormDefault` del esquema.
                    "attributeFormDefault" | "form" => {
                        let vigente = if ctx == Ctx::LocalElement {
                            self.element_form
                        } else {
                            FormDefault::Unqualified
                        };
                        (form(&w).map(|f| f == vigente), "qualified o unqualified")
                    }
                    _ => (
                        match w.as_str() {
                            "false" | "0" => Some(true),
                            "true" | "1" => Some(false),
                            _ => None,
                        },
                        "true o false",
                    ),
                };
                match (no_effect, default_only(name)) {
                    (Some(true), _) => Val::Nothing,
                    (Some(false), Some((category, reason))) => {
                        return fail(
                            ErrorKind::UnsupportedValue {
                                attr: name,
                                found: w,
                                category,
                                reason,
                            },
                            at,
                        )
                    }
                    _ => {
                        return fail(
                            ErrorKind::InvalidValue {
                                attr: name,
                                expected,
                                found: clip(a.value()),
                            },
                            at,
                        )
                    }
                }
            }
            "default" | "fixed" => Val::Raw(Spanned {
                value: a.into_value(),
                span,
            }),
            "value" if matches!(ctx, Ctx::Facet(k) if k.keeps_whitespace()) => Val::Raw(Spanned {
                value: a.into_value(),
                span,
            }),
            // `name`, `minOccurs`, `maxOccurs` y el `value` de las demás
            // facetas.
            _ => Val::Text(Spanned {
                value: word(),
                span,
            }),
        };
        Ok(v)
    }

    /// `X_rest → /> | > …`: consume el cierre de la etiqueta de apertura y
    /// dice si sigue contenido. `needs`: lo que exige el contenido en las
    /// construcciones sin la alternativa `/>` (`xs:simpleType`,
    /// `xs:simpleContent`).
    fn tag_end(&mut self, ctx: Ctx, needs: Option<&'static str>) -> Res<bool> {
        let content = self.look.tok == Tok::Gt;
        if let (false, Some(expected)) = (content, needs) {
            return Err(self.unexpected(expected, ctx));
        }
        self.bump()?;
        Ok(content)
    }

    /// `ETAG` que cierra la construcción en curso; si no está, celda vacía
    /// de la fila (`expected`). El lector ya garantiza que el cierre
    /// corresponde a la apertura.
    fn close(&mut self, expected: &'static str, ctx: Ctx) -> Res<()> {
        if !matches!(self.look.tok, Tok::ETag(_)) {
            return Err(self.unexpected(expected, ctx));
        }
        self.bump()?;
        Ok(())
    }

    fn span_from(&self, start: Pos) -> Span {
        Span {
            start,
            end: self.last_end,
        }
    }

    // ------------------------------------------------------- documento ---

    /// `document → S_SCHEMA attrs schema_rest` (las PI y los espacios de
    /// alrededor ya se descartaron).
    fn document(&mut self) -> Res<Schema> {
        self.look = self.next_significant()?;
        if self.look_xs() != Some(Xs::Schema) {
            return Err(self.unexpected("<xs:schema> como elemento raíz", Ctx::Schema));
        }
        self.schema()
        // El lector garantiza que tras la raíz solo hay `Eof` (y PI o
        // espacios, ya descartados).
    }

    /// `schema_rest → /> | > top_items ETAG`
    fn schema(&mut self) -> Res<Schema> {
        let (start, mut attrs) = self.open(Ctx::Schema)?;
        let target_namespace = attrs.namespace();
        let element_form_default = attrs.form();
        if let Some(f) = &element_form_default {
            self.element_form = f.value;
        }
        let mut items = Vec::new();
        if self.tag_end(Ctx::Schema, None)? {
            loop {
                let item = match self.look_xs() {
                    Some(Xs::Annotation) => {
                        self.annotation()?;
                        continue;
                    }
                    Some(Xs::Element) => TopLevel::Element(self.element(true)?),
                    Some(Xs::ComplexType) => TopLevel::ComplexType(self.complex_type(true)?),
                    Some(Xs::SimpleType) => TopLevel::SimpleType(self.simple_type(true)?),
                    Some(Xs::Attribute) => TopLevel::Attribute(self.attribute(true)?),
                    _ => break,
                };
                items.push(item);
            }
            self.close(
                "xs:element, xs:complexType, xs:simpleType, xs:attribute, xs:annotation o \
                 </xs:schema>",
                Ctx::Schema,
            )?;
        }
        Ok(Schema {
            target_namespace,
            element_form_default,
            items,
            span: self.span_from(start),
        })
    }

    // ---------------------------------------------------- anotaciones ---

    /// `annot_opt → annotation | ε`
    fn annotation_opt(&mut self) -> Res<()> {
        if self.look_xs() == Some(Xs::Annotation) {
            self.annotation()?;
        }
        Ok(())
    }

    /// `annotation → S_ANNOTATION attrs annot_rest`,
    /// `annot_rest → /> | > annot_items ETAG` (D23: no deja nada en el AST).
    fn annotation(&mut self) -> Res<()> {
        self.open(Ctx::Annotation)?;
        if self.tag_end(Ctx::Annotation, None)? {
            loop {
                match self.look_xs() {
                    Some(Xs::Documentation) => self.any_content(Ctx::Documentation)?,
                    Some(Xs::Appinfo) => self.any_content(Ctx::Appinfo)?,
                    _ => break,
                }
            }
            self.close(
                "xs:documentation, xs:appinfo o </xs:annotation>",
                Ctx::Annotation,
            )?;
        }
        Ok(())
    }

    /// `S_DOCUMENTATION attrs any_rest` (o `S_APPINFO`), con
    /// `any_rest → /> | > any ETAG` y `any → STAG attrs any_rest any | TEXT
    /// any | ε`: contenido XML bien formado cualquiera (D23), que se salta
    /// con un contador de anidamiento en lugar de con recursión. Dentro, los
    /// `STAG` no se refinan.
    fn any_content(&mut self, ctx: Ctx) -> Res<()> {
        self.open(ctx)?;
        if !self.tag_end(ctx, None)? {
            return Ok(());
        }
        let mut depth = 0usize;
        loop {
            match self.look.tok {
                Tok::ETag(_) if depth == 0 => break,
                Tok::STag(_) => depth += 1,
                Tok::EmptyClose | Tok::ETag(_) => depth = depth.saturating_sub(1),
                Tok::Eof => return Err(self.unexpected("un cierre", ctx)),
                _ => {}
            }
            self.bump()?;
        }
        self.bump()?;
        Ok(())
    }

    // ------------------------------------------------------- elementos ---

    /// `element → S_ELEMENT attrs element_rest`,
    /// `element_rest → /> | > annot_opt element_type ETAG`,
    /// `element_type → complex_type | simple_type | ε`.
    fn element(&mut self, global: bool) -> Res<Element> {
        let ctx = if global {
            Ctx::GlobalElement
        } else {
            Ctx::LocalElement
        };
        let (start, mut attrs) = self.open(ctx)?;
        let name = attrs.text("name");
        let ty = attrs.qname("type");
        let reference = attrs.qname("ref");
        let min_occurs = attrs.text("minOccurs");
        let max_occurs = attrs.text("maxOccurs");
        let mut anonymous = None;
        if self.tag_end(ctx, None)? {
            self.annotation_opt()?;
            anonymous = match self.look_xs() {
                Some(Xs::ComplexType) => {
                    Some(AnonymousType::Complex(Box::new(self.complex_type(false)?)))
                }
                Some(Xs::SimpleType) => {
                    Some(AnonymousType::Simple(Box::new(self.simple_type(false)?)))
                }
                _ => None,
            };
            self.close(
                if anonymous.is_some() {
                    "</xs:element>"
                } else {
                    "xs:complexType, xs:simpleType o </xs:element>"
                },
                ctx,
            )?;
        }
        Ok(Element {
            name,
            ty,
            reference,
            min_occurs,
            max_occurs,
            anonymous,
            span: self.span_from(start),
        })
    }

    /// `complex_type → S_COMPLEXTYPE attrs ct_rest`,
    /// `ct_rest → /> | > annot_opt ct_content ETAG`,
    /// `ct_content → simple_content | model_opt attr_decls`.
    fn complex_type(&mut self, global: bool) -> Res<ComplexType> {
        let ctx = if global {
            Ctx::GlobalComplexType
        } else {
            Ctx::LocalComplexType
        };
        let (start, mut attrs) = self.open(ctx)?;
        let name = attrs.text("name");
        // `mixed` ya se comprobó al llegar: solo `false` deja seguir.
        attrs.take("mixed");
        let mut content = ComplexContent::Elements {
            model: None,
            attributes: Vec::new(),
        };
        if self.tag_end(ctx, None)? {
            self.annotation_opt()?;
            if self.look_xs() == Some(Xs::SimpleContent) {
                content = ComplexContent::Simple(self.simple_content()?);
                self.close("</xs:complexType>", ctx)?;
            } else {
                let model = match self.look_xs() {
                    Some(Xs::Sequence) => Some(self.group(Compositor::Sequence)?),
                    Some(Xs::Choice) => Some(self.group(Compositor::Choice)?),
                    Some(Xs::All) => Some(self.group(Compositor::All)?),
                    _ => None,
                };
                let attributes = self.attr_decls()?;
                self.close(
                    if model.is_some() || !attributes.is_empty() {
                        "xs:attribute o </xs:complexType>"
                    } else {
                        "xs:simpleContent, xs:sequence, xs:choice, xs:all, xs:attribute o \
                         </xs:complexType>"
                    },
                    ctx,
                )?;
                content = ComplexContent::Elements { model, attributes };
            }
        }
        Ok(ComplexType {
            name,
            content,
            span: self.span_from(start),
        })
    }

    /// `attr_decls → attribute attr_decls | ε`
    fn attr_decls(&mut self) -> Res<Vec<Attribute>> {
        let mut v = Vec::new();
        while self.look_xs() == Some(Xs::Attribute) {
            v.push(self.attribute(false)?);
        }
        Ok(v)
    }

    /// `sequence → S_SEQUENCE attrs group_rest` (y `choice`),
    /// `group_rest → /> | > annot_opt particles ETAG`,
    /// `particle → element | sequence | choice`; y
    /// `all → S_ALL attrs all_rest`, `all_items → element all_items | ε`.
    fn group(&mut self, compositor: Compositor) -> Res<Group> {
        let (ctx, expected) = match compositor {
            Compositor::Sequence => (
                Ctx::Sequence,
                "xs:element, xs:sequence, xs:choice o </xs:sequence>",
            ),
            Compositor::Choice => (
                Ctx::Choice,
                "xs:element, xs:sequence, xs:choice o </xs:choice>",
            ),
            Compositor::All => (Ctx::All, "xs:element o </xs:all>"),
        };
        let (start, mut attrs) = self.open(ctx)?;
        let min_occurs = attrs.text("minOccurs");
        let max_occurs = attrs.text("maxOccurs");
        let mut particles = Vec::new();
        if self.tag_end(ctx, None)? {
            self.annotation_opt()?;
            loop {
                let p = match (self.look_xs(), compositor) {
                    (Some(Xs::Element), _) => Particle::Element(self.element(false)?),
                    (Some(Xs::Sequence), Compositor::Sequence | Compositor::Choice) => {
                        Particle::Group(self.group(Compositor::Sequence)?)
                    }
                    (Some(Xs::Choice), Compositor::Sequence | Compositor::Choice) => {
                        Particle::Group(self.group(Compositor::Choice)?)
                    }
                    _ => break,
                };
                particles.push(p);
            }
            self.close(expected, ctx)?;
        }
        Ok(Group {
            compositor,
            min_occurs,
            max_occurs,
            particles,
            span: self.span_from(start),
        })
    }

    // ------------------------------------------------------- atributos ---

    /// `attribute → S_ATTRIBUTE attrs attr_rest`,
    /// `attr_rest → /> | > annot_opt simple_opt ETAG`.
    fn attribute(&mut self, global: bool) -> Res<Attribute> {
        let ctx = if global {
            Ctx::GlobalAttribute
        } else {
            Ctx::LocalAttribute
        };
        let (start, mut attrs) = self.open(ctx)?;
        let Some(name) = attrs.text("name") else {
            // `open` ya comprobó que `name` es obligatorio aquí.
            return fail(
                ErrorKind::MissingAttribute {
                    element: ctx.describe(),
                    name: "name",
                },
                start,
            );
        };
        let ty = attrs.qname("type");
        let use_ = attrs.use_();
        let default = attrs.text("default");
        let fixed = attrs.text("fixed");
        let mut simple_type = None;
        if self.tag_end(ctx, None)? {
            self.annotation_opt()?;
            if self.look_xs() == Some(Xs::SimpleType) {
                simple_type = Some(Box::new(self.simple_type(false)?));
            }
            self.close(
                if simple_type.is_some() {
                    "</xs:attribute>"
                } else {
                    "xs:simpleType o </xs:attribute>"
                },
                ctx,
            )?;
        }
        Ok(Attribute {
            name,
            ty,
            use_,
            default,
            fixed,
            simple_type,
            span: self.span_from(start),
        })
    }

    /// `simple_content → S_SIMPLECONTENT attrs > annot_opt extension ETAG`
    /// (sin la alternativa `/>`: XSD exige la derivación).
    fn simple_content(&mut self) -> Res<SimpleContent> {
        let ctx = Ctx::SimpleContent;
        let (start, _) = self.open(ctx)?;
        self.tag_end(ctx, Some("un hijo xs:extension en xs:simpleContent"))?;
        self.annotation_opt()?;
        match self.look_xs() {
            Some(Xs::Extension) => {}
            Some(Xs::Restriction) => {
                let (category, reason) = SIMPLE_CONTENT_RESTRICTION;
                let name = match &self.look.tok {
                    Tok::STag(n) => clip(n.as_str()),
                    _ => String::new(),
                };
                return fail(
                    ErrorKind::UnsupportedElement {
                        name,
                        category,
                        reason,
                    },
                    self.look.span.start,
                );
            }
            _ => return Err(self.unexpected("xs:extension", ctx)),
        }
        // `extension → S_EXTENSION attrs ext_rest`,
        // `ext_rest → /> | > annot_opt attr_decls ETAG`.
        let ext = Ctx::Extension;
        let (ext_start, mut attrs) = self.open(ext)?;
        let base = attrs.qname("base");
        let mut attributes = Vec::new();
        if self.tag_end(ext, None)? {
            self.annotation_opt()?;
            attributes = self.attr_decls()?;
            self.close("xs:attribute o </xs:extension>", ext)?;
        }
        let extension_span = self.span_from(ext_start);
        self.close("</xs:simpleContent>", ctx)?;
        let Some(base) = base else {
            // `open` ya comprobó que `base` es obligatorio.
            return fail(
                ErrorKind::MissingAttribute {
                    element: ext.describe(),
                    name: "base",
                },
                ext_start,
            );
        };
        Ok(SimpleContent {
            base,
            attributes,
            span: self.span_from(start),
            extension_span,
        })
    }

    // -------------------------------------------------- tipos simples ---

    /// `simple_type → S_SIMPLETYPE attrs > annot_opt restriction ETAG` (sin
    /// `/>`: XSD exige la derivación; `xs:list` y `xs:union` están fuera).
    fn simple_type(&mut self, global: bool) -> Res<SimpleType> {
        let ctx = if global {
            Ctx::GlobalSimpleType
        } else {
            Ctx::LocalSimpleType
        };
        let (start, mut attrs) = self.open(ctx)?;
        let name = attrs.text("name");
        self.tag_end(ctx, Some("un hijo xs:restriction en xs:simpleType"))?;
        self.annotation_opt()?;
        if self.look_xs() != Some(Xs::Restriction) {
            return Err(self.unexpected("xs:restriction", ctx));
        }
        let restriction = self.restriction()?;
        self.close("</xs:simpleType>", ctx)?;
        Ok(SimpleType {
            name,
            restriction,
            span: self.span_from(start),
        })
    }

    /// `restriction → S_RESTRICTION attrs restr_rest`,
    /// `restr_rest → /> | > annot_opt simple_opt facets ETAG`.
    fn restriction(&mut self) -> Res<Restriction> {
        let ctx = Ctx::Restriction;
        let (start, mut attrs) = self.open(ctx)?;
        let base = attrs.qname("base");
        let mut simple_type = None;
        let mut facets = Vec::new();
        if self.tag_end(ctx, None)? {
            self.annotation_opt()?;
            if self.look_xs() == Some(Xs::SimpleType) {
                simple_type = Some(Box::new(self.simple_type(false)?));
            }
            while let Some(Xs::Facet(kind)) = self.look_xs() {
                facets.push(self.facet(kind)?);
            }
            self.close(
                if simple_type.is_some() || !facets.is_empty() {
                    "una faceta o </xs:restriction>"
                } else {
                    "xs:simpleType, una faceta o </xs:restriction>"
                },
                ctx,
            )?;
        }
        Ok(Restriction {
            base,
            simple_type,
            facets,
            span: self.span_from(start),
        })
    }

    /// `facet → S_<FACETA> attrs facet_rest`,
    /// `facet_rest → /> | > annot_opt ETAG`.
    fn facet(&mut self, kind: FacetKind) -> Res<Facet> {
        let ctx = Ctx::Facet(kind);
        let (start, mut attrs) = self.open(ctx)?;
        let value = attrs.text("value");
        if self.tag_end(ctx, None)? {
            self.annotation_opt()?;
            self.close("xs:annotation o el cierre de la faceta", ctx)?;
        }
        let Some(value) = value else {
            // `open` ya comprobó que `value` es obligatorio.
            return fail(
                ErrorKind::MissingAttribute {
                    element: ctx.describe(),
                    name: "value",
                },
                start,
            );
        };
        Ok(Facet {
            kind,
            value,
            span: self.span_from(start),
        })
    }
}
