//! Parser LL(1) de DTD (DTD.2; `docs/fase2/gramatica-dtd.md`).
//!
//! Sigue la tabla de `docs/fase2/ll1-dtd.md`: cada función es un no terminal y
//! cada decisión mira **un** token, ya clasificado por su posición (palabras
//! clave contextuales, §2). La única recursión de la gramática,
//! `cp → LPAR cp children_rest RPAR occ`, se recorre con una **pila explícita**
//! de grupos abiertos (D7): ningún esquema puede desbordar la pila del hilo, y
//! pasar de [`MAX_DEPTH`] paréntesis anidados es un error `Limit`.
//!
//! Lo que es sintaxis de XML pero está fuera del subconjunto (§3bis.2) se
//! reconoce y se rechaza con su motivo (`OutOfScope`), no como error
//! sintáctico. Se para en el primer error.

use core::fmt;

use lexer::{Pos, Span};

use crate::ast::{
    AttDef, AttType, AttlistDecl, ContentSpec, DefaultDecl, Dtd, ElementDecl, Literal, Name,
    Occurs, Particle, ParticleKind,
};
use crate::lexer::{self as lx, DtdLexer, Event, Sink};

/// Paréntesis anidados como máximo en un modelo de contenido (`docs/00-alcance.md` §4, D7).
pub const MAX_DEPTH: usize = 256;

/// Clase de un error del frontal DTD (Fases 1-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Error léxico (Fase 1), con su clase.
    Lexical(lx::ErrorKind),
    /// La secuencia de tokens no está en la gramática.
    Syntax,
    /// Sintaxis válida de XML, pero fuera del subconjunto (§3bis.2).
    OutOfScope,
    /// Se pasó un límite (§4).
    Limit,
    /// Fase 3: un elemento, un atributo o un valor de enumeración repetido.
    Duplicate,
    /// Fase 3: un nombre sin su `<!ELEMENT>` (§3bis.4, desviación 1).
    Unresolved,
    /// Fase 3: modelo de contenido no determinista (XML 1.0, apéndice E).
    NonDeterministic,
    /// Fase 3: valor por defecto o `#FIXED` fuera de su enumeración.
    InvalidValue,
    /// Fase 3: la DTD no declara ningún elemento.
    NoElements,
}

impl ErrorKind {
    /// Código estable para la CLI y el servidor.
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::Lexical(_) => "lexical",
            ErrorKind::Syntax => "syntax",
            ErrorKind::OutOfScope => "unsupported",
            ErrorKind::Limit => "limit",
            ErrorKind::Duplicate => "duplicate-name",
            ErrorKind::Unresolved => "unresolved",
            ErrorKind::NonDeterministic => "content-model",
            ErrorKind::InvalidValue => "value",
            ErrorKind::NoElements => "empty-schema",
        }
    }
}

/// Error con su posición y un mensaje para el usuario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtdError {
    pub kind: ErrorKind,
    pub at: Pos,
    pub message: String,
}

impl DtdError {
    /// Código estable de la clase (`ErrorKind::code`).
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
    pub fn line(&self) -> u32 {
        self.at.line
    }
    pub fn column(&self) -> u32 {
        self.at.column
    }
}

/// Con el mismo prefijo que los errores de JSON Schema y XSD («línea L,
/// columna C: »), que el servidor quita del mensaje.
impl fmt::Display for DtdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "línea {}, columna {}: {}",
            self.at.line, self.at.column, self.message
        )
    }
}

impl std::error::Error for DtdError {}

/// Analiza una DTD completa.
pub fn parse(input: &[u8]) -> Result<Dtd, DtdError> {
    let (toks, fin) = tokens(input)?;
    Parser { toks, i: 0, fin }.dtd()
}

// ---------------------------------------------------------------- tokens ---

/// Token de la gramática (`docs/fase2/dtd.ll1`), antes de clasificar los nombres.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    /// `<!` y su palabra clave.
    Decl(String),
    Name(String),
    /// `#` y su nombre.
    Hash(String),
    LParen,
    RParen,
    Pipe,
    Comma,
    Opt,
    Star,
    Plus,
    Literal(String),
    /// `>` (su `Span` es solo el `>`).
    Close,
    Pi {
        target: String,
        content: String,
    },
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    span: Span,
}

/// Agrupa los eventos del lexer en tokens.
#[derive(Default)]
struct Tokens {
    v: Vec<Token>,
    /// Tras `<!`: el `NameBegin` siguiente es el de la palabra clave.
    tras_apertura: bool,
}

impl Tokens {
    fn push(&mut self, tok: Tok, span: Span) {
        self.v.push(Token { tok, span });
    }
}

impl Sink for Tokens {
    type Error = core::convert::Infallible;

    fn event(&mut self, ev: Event, span: Span) -> Result<(), Self::Error> {
        match ev {
            Event::DeclOpen => {
                self.push(Tok::Decl(String::new()), span);
                self.tras_apertura = true;
            }
            Event::NameBegin if self.tras_apertura => self.tras_apertura = false,
            Event::NameBegin => self.push(Tok::Name(String::new()), span),
            Event::HashBegin => self.push(Tok::Hash(String::new()), span),
            Event::NameChar(c) => {
                if let Some(t) = self.v.last_mut() {
                    match &mut t.tok {
                        Tok::Decl(s) | Tok::Name(s) | Tok::Hash(s) => s.push(c),
                        Tok::Pi { target, .. } => target.push(c),
                        _ => {}
                    }
                    t.span.end = span.end;
                }
            }
            Event::LParen => self.push(Tok::LParen, span),
            Event::RParen => self.push(Tok::RParen, span),
            Event::Pipe => self.push(Tok::Pipe, span),
            Event::Comma => self.push(Tok::Comma, span),
            Event::Opt => self.push(Tok::Opt, span),
            Event::Star => self.push(Tok::Star, span),
            Event::Plus => self.push(Tok::Plus, span),
            Event::LiteralBegin => self.push(Tok::Literal(String::new()), span),
            Event::LiteralChar(c) => {
                if let Some(Token {
                    tok: Tok::Literal(s),
                    ..
                }) = self.v.last_mut()
                {
                    s.push(c);
                }
            }
            Event::DeclClose => self.push(Tok::Close, span),
            Event::PiOpen => self.push(
                Tok::Pi {
                    target: String::new(),
                    content: String::new(),
                },
                span,
            ),
            Event::PiChar(c) => {
                if let Some(Token {
                    tok: Tok::Pi { content, .. },
                    ..
                }) = self.v.last_mut()
                {
                    content.push(c);
                }
            }
            // El rango del evento de cierre cubre el token completo.
            Event::LiteralEnd | Event::PiClose => {
                if let Some(t) = self.v.last_mut() {
                    t.span = span;
                }
            }
        }
        Ok(())
    }
}

/// Tokeniza toda la entrada; devuelve también la posición del final.
fn tokens(input: &[u8]) -> Result<(Vec<Token>, Pos), DtdError> {
    let mut lx = DtdLexer::new();
    let mut t = Tokens::default();
    let r = lx.feed(input, &mut t).and_then(|()| lx.finish(&mut t));
    match r {
        Ok(()) => Ok((t.v, lx.position())),
        Err(lx::Error::Lex(e)) => Err(DtdError {
            kind: ErrorKind::Lexical(e.kind),
            at: e.at,
            message: e.kind.message().to_owned(),
        }),
        Err(lx::Error::Sink(nunca)) => match nunca {},
    }
}

// ---------------------------------------------------------------- parser ---

struct Parser {
    toks: Vec<Token>,
    i: usize,
    /// Posición del fin de la entrada (para «se esperaba … y terminó»).
    fin: Pos,
}

/// Separador de un grupo, fijado por el primero que aparece.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sep {
    Seq,
    Choice,
}

/// Un grupo `(` abierto del modelo de contenido, en la pila explícita.
struct Grupo {
    abre: Span,
    items: Vec<Particle>,
    sep: Option<Sep>,
}

impl Grupo {
    fn new(abre: Span) -> Self {
        Grupo {
            abre,
            items: Vec::new(),
            sep: None,
        }
    }
}

/// Recorta un texto del usuario para los mensajes.
fn corto(s: &str) -> String {
    const MAX: usize = 40;
    if s.chars().count() <= MAX {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(MAX).collect::<String>())
    }
}

fn describir(t: Option<&Token>) -> String {
    let Some(t) = t else {
        return "el fin de la entrada".into();
    };
    match &t.tok {
        Tok::Decl(k) => format!("«<!{}»", corto(k)),
        Tok::Name(n) => format!("el nombre «{}»", corto(n)),
        Tok::Hash(h) => format!("«#{}»", corto(h)),
        Tok::LParen => "«(»".into(),
        Tok::RParen => "«)»".into(),
        Tok::Pipe => "«|»".into(),
        Tok::Comma => "«,»".into(),
        Tok::Opt => "«?»".into(),
        Tok::Star => "«*»".into(),
        Tok::Plus => "«+»".into(),
        Tok::Literal(_) => "un literal".into(),
        Tok::Close => "«>»".into(),
        Tok::Pi { .. } => "una instrucción de procesamiento".into(),
    }
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.i)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.toks.get(self.i).cloned();
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn pos(&self, t: Option<&Token>) -> Pos {
        t.map_or(self.fin, |t| t.span.start)
    }

    fn error(&self, kind: ErrorKind, at: Pos, message: String) -> DtdError {
        DtdError { kind, at, message }
    }

    /// Error sintáctico en el token `t` (o en el fin de la entrada).
    fn esperaba(&self, que: &str, t: Option<&Token>) -> DtdError {
        self.error(
            ErrorKind::Syntax,
            self.pos(t),
            format!("se esperaba {que}, pero se encontró {}", describir(t)),
        )
    }

    // dtd -> item dtd | ε ;  item -> PI | elementdecl | attlistdecl
    fn dtd(mut self) -> Result<Dtd, DtdError> {
        let mut dtd = Dtd::default();
        let mut primero = true;
        while let Some(t) = self.next() {
            match &t.tok {
                Tok::Pi { target, content } => {
                    if target == "xml" {
                        if !primero {
                            return Err(self.error(
                                ErrorKind::Syntax,
                                t.span.start,
                                "la declaración <?xml …?> solo puede ir al inicio de la DTD".into(),
                            ));
                        }
                        self.declaracion_xml(content, t.span)?;
                    }
                    // Las demás instrucciones se ignoran (§3bis.1).
                }
                Tok::Decl(k) if k == "ELEMENT" => dtd.elements.push(self.elementdecl(t.span)?),
                Tok::Decl(k) if k == "ATTLIST" => dtd.attlists.push(self.attlistdecl(t.span)?),
                Tok::Decl(k) if k == "NOTATION" => {
                    return Err(self.error(
                        ErrorKind::OutOfScope,
                        t.span.start,
                        "<!NOTATION está fuera del subconjunto DTD (docs/00-alcance.md §3bis.2)"
                            .into(),
                    ))
                }
                Tok::Decl(k) => {
                    return Err(self.error(
                        ErrorKind::Syntax,
                        t.span.start,
                        format!(
                            "declaración desconocida «<!{}»: se esperaba <!ELEMENT o <!ATTLIST",
                            corto(k)
                        ),
                    ))
                }
                _ => return Err(self.esperaba("una declaración <!ELEMENT o <!ATTLIST", Some(&t))),
            }
            primero = false;
        }
        Ok(dtd)
    }

    /// `<?xml … encoding="…"?>`: solo UTF-8 (`docs/00-alcance.md` §1.2).
    fn declaracion_xml(&self, content: &str, span: Span) -> Result<(), DtdError> {
        let Some(k) = content.find("encoding") else {
            return Ok(());
        };
        let resto = content[k + "encoding".len()..].trim_start();
        let valor = resto.strip_prefix('=').map(str::trim_start).and_then(|r| {
            let q = r.chars().next().filter(|&c| c == '"' || c == '\'')?;
            r[1..].split(q).next()
        });
        match valor {
            Some(v) if v.eq_ignore_ascii_case("UTF-8") => Ok(()),
            _ => Err(self.error(
                ErrorKind::OutOfScope,
                span.start,
                "la DTD tiene que estar en UTF-8: solo se admite encoding=\"UTF-8\"".into(),
            )),
        }
    }

    /// Un `Name` en posición de nombre de elemento o de atributo.
    fn nombre(&mut self, que: &str) -> Result<Name, DtdError> {
        let t = self.next();
        let Some(Token {
            tok: Tok::Name(text),
            span,
        }) = t
        else {
            return Err(self.esperaba(que, t.as_ref()));
        };
        self.validar_nombre(&text, span)?;
        Ok(Name { text, span })
    }

    /// `Name` de XML: no empieza por dígito, `-` ni `.` (eso es un *nmtoken*),
    /// y sin `:` (espacios de nombres, §3bis.2).
    fn validar_nombre(&self, text: &str, span: Span) -> Result<(), DtdError> {
        if text.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.') {
            return Err(self.error(
                ErrorKind::Syntax,
                span.start,
                format!(
                    "«{}» no es un nombre válido: no puede empezar por dígito, «-» ni «.»",
                    corto(text)
                ),
            ));
        }
        if text.contains(':') {
            return Err(self.error(
                ErrorKind::OutOfScope,
                span.start,
                format!(
                    "«{}»: los nombres con «:» (espacios de nombres) están fuera del subconjunto DTD",
                    corto(text)
                ),
            ));
        }
        Ok(())
    }

    /// Consume el `>` de la declaración; su rango cubre la declaración entera.
    fn cierre(&mut self, que: &str) -> Result<Span, DtdError> {
        let t = self.next();
        match t {
            Some(Token {
                tok: Tok::Close,
                span,
            }) => Ok(span),
            _ => Err(self.esperaba(que, t.as_ref())),
        }
    }

    // elementdecl -> DECL_ELEMENT NAME contentspec GT
    fn elementdecl(&mut self, abre: Span) -> Result<ElementDecl, DtdError> {
        let name = self.nombre("el nombre del elemento")?;
        let content = self.contentspec()?;
        let cierre = self.cierre("«>» al final de <!ELEMENT")?;
        Ok(ElementDecl {
            name,
            content,
            span: Span {
                start: abre.start,
                end: cierre.end,
            },
        })
    }

    // contentspec -> K_EMPTY | K_ANY | LPAR group_body
    fn contentspec(&mut self) -> Result<ContentSpec, DtdError> {
        let t = self.next();
        match t.as_ref().map(|t| &t.tok) {
            Some(Tok::Name(n)) if n == "EMPTY" => Ok(ContentSpec::Empty),
            Some(Tok::Name(n)) if n == "ANY" => Err(self.error(
                ErrorKind::OutOfScope,
                self.pos(t.as_ref()),
                "ANY está fuera del subconjunto: equivale a xs:any con contenido mixto (docs/00-alcance.md §3bis.2)"
                    .into(),
            )),
            Some(Tok::LParen) => {
                let abre = t.map(|t| t.span).unwrap_or(Span {
                    start: self.fin,
                    end: self.fin.offset,
                });
                self.group_body(abre)
            }
            _ => Err(self.esperaba("EMPTY, ANY o «(» tras el nombre del elemento", t.as_ref())),
        }
    }

    // group_body -> K_PCDATA mixed | cp children_rest RPAR occ
    fn group_body(&mut self, abre: Span) -> Result<ContentSpec, DtdError> {
        let Some(Token {
            tok: Tok::Hash(h),
            span,
        }) = self.peek().cloned()
        else {
            return self.children(abre).map(ContentSpec::Children);
        };
        if h != "PCDATA" {
            return Err(self.error(
                ErrorKind::Syntax,
                span.start,
                format!(
                    "«#{}» no puede ir aquí: se esperaba #PCDATA, un nombre o «(»",
                    corto(&h)
                ),
            ));
        }
        self.i += 1;
        // mixed -> RPAR mixed_star | PIPE NAME mixed_names
        let t = self.next();
        match t.as_ref().map(|t| &t.tok) {
            Some(Tok::RParen) => {
                if matches!(self.peek().map(|t| &t.tok), Some(Tok::Star)) {
                    self.i += 1;
                }
                Ok(ContentSpec::Pcdata)
            }
            Some(Tok::Pipe) => Err(self.error(
                ErrorKind::OutOfScope,
                abre.start,
                "el contenido mixto (#PCDATA|…)* está fuera del subconjunto (docs/00-alcance.md §1.2 y §3bis.2)"
                    .into(),
            )),
            _ => Err(self.esperaba("«)» tras #PCDATA", t.as_ref())),
        }
    }

    // occ -> OPT | STAR | PLUS | ε
    fn occ(&mut self) -> Occurs {
        let o = match self.peek().map(|t| &t.tok) {
            Some(Tok::Opt) => Occurs::Opt,
            Some(Tok::Star) => Occurs::Star,
            Some(Tok::Plus) => Occurs::Plus,
            _ => return Occurs::One,
        };
        self.i += 1;
        o
    }

    /// `cp children_rest RPAR occ` tras el primer `(`, con la recursión de
    /// `cp -> LPAR cp children_rest RPAR occ` en una pila explícita.
    fn children(&mut self, abre: Span) -> Result<Particle, DtdError> {
        let mut pila = vec![Grupo::new(abre)];
        loop {
            // Se espera un cp.
            match self.next() {
                Some(Token {
                    tok: Tok::Name(text),
                    span,
                }) => {
                    self.validar_nombre(&text, span)?;
                    let occurs = self.occ();
                    let p = Particle {
                        kind: ParticleKind::Name(Name { text, span }),
                        occurs,
                        span,
                    };
                    pila.last_mut()
                        .expect("la pila nunca está vacía aquí")
                        .items
                        .push(p);
                }
                Some(Token {
                    tok: Tok::LParen,
                    span,
                }) => {
                    if pila.len() == MAX_DEPTH {
                        return Err(self.error(
                            ErrorKind::Limit,
                            span.start,
                            format!(
                                "más de {MAX_DEPTH} paréntesis anidados en el modelo de contenido"
                            ),
                        ));
                    }
                    pila.push(Grupo::new(span));
                    continue;
                }
                Some(Token {
                    tok: Tok::Hash(h),
                    span,
                }) if h == "PCDATA" => {
                    return Err(self.error(
                        ErrorKind::Syntax,
                        span.start,
                        "#PCDATA solo puede ir el primero del grupo exterior".into(),
                    ))
                }
                otro => return Err(self.esperaba("un nombre o «(»", otro.as_ref())),
            }
            // Tras un cp: `,` o `|` (sin mezclar) o `)`, que puede cerrar varios.
            loop {
                let t = self.next();
                let sep = match t.as_ref().map(|t| &t.tok) {
                    Some(Tok::Comma) => Sep::Seq,
                    Some(Tok::Pipe) => Sep::Choice,
                    Some(Tok::RParen) => {
                        let g = pila.pop().expect("la pila nunca está vacía aquí");
                        let cierra = t.map(|t| t.span).expect("hay token");
                        let occurs = self.occ();
                        let p = Particle {
                            kind: match g.sep {
                                Some(Sep::Choice) => ParticleKind::Choice(g.items),
                                _ => ParticleKind::Seq(g.items),
                            },
                            occurs,
                            span: Span {
                                start: g.abre.start,
                                end: cierra.end,
                            },
                        };
                        match pila.last_mut() {
                            Some(padre) => {
                                padre.items.push(p);
                                continue;
                            }
                            None => return Ok(p),
                        }
                    }
                    _ => return Err(self.esperaba("«,», «|» o «)»", t.as_ref())),
                };
                let g = pila.last_mut().expect("la pila nunca está vacía aquí");
                match g.sep {
                    None => g.sep = Some(sep),
                    Some(s) if s == sep => {}
                    Some(_) => {
                        return Err(self.error(
                            ErrorKind::Syntax,
                            self.pos(t.as_ref()),
                            "no se pueden mezclar «,» y «|» en el mismo grupo: usa paréntesis"
                                .into(),
                        ))
                    }
                }
                break;
            }
        }
    }

    // attlistdecl -> DECL_ATTLIST NAME attdefs GT ;  attdefs -> attdef attdefs | ε
    fn attlistdecl(&mut self, abre: Span) -> Result<AttlistDecl, DtdError> {
        let element = self.nombre("el nombre del elemento de <!ATTLIST")?;
        let mut defs = Vec::new();
        loop {
            let t = self.next();
            match t.as_ref().map(|t| &t.tok) {
                Some(Tok::Close) => {
                    let cierre = t.map(|t| t.span).expect("hay token");
                    return Ok(AttlistDecl {
                        element,
                        defs,
                        span: Span {
                            start: abre.start,
                            end: cierre.end,
                        },
                    });
                }
                Some(Tok::Name(n)) => {
                    let span = t.as_ref().map(|t| t.span).expect("hay token");
                    self.validar_nombre(n, span)?;
                    let name = Name {
                        text: n.clone(),
                        span,
                    };
                    let ty = self.atttype()?;
                    let default = self.defaultdecl()?;
                    defs.push(AttDef { name, ty, default });
                }
                _ => return Err(self.esperaba("el nombre de un atributo o «>»", t.as_ref())),
            }
        }
    }

    // atttype -> T_CDATA | T_TOKENIZED | K_NOTATION LPAR NAME enum_rest | LPAR NAME enum_rest
    fn atttype(&mut self) -> Result<AttType, DtdError> {
        let t = self.next();
        let fuera = |motivo: &str| {
            Err(self.error(
                ErrorKind::OutOfScope,
                self.pos(t.as_ref()),
                format!("{motivo} (docs/00-alcance.md §3bis.2)"),
            ))
        };
        match t.as_ref().map(|t| &t.tok) {
            Some(Tok::Name(n)) => match n.as_str() {
                "CDATA" => Ok(AttType::Cdata),
                "ID" | "IDREF" | "IDREFS" => fuera(&format!(
                    "el tipo {n} está fuera del subconjunto: la unicidad y las referencias globales no caben en un DPDA con registros acotados"
                )),
                "ENTITY" | "ENTITIES" => fuera(&format!("el tipo {n} está fuera del subconjunto: las entidades están prohibidas")),
                "NMTOKEN" | "NMTOKENS" => fuera(&format!("el tipo {n} está fuera de la primera versión del subconjunto")),
                "NOTATION" => fuera("el tipo NOTATION está fuera del subconjunto"),
                _ => Err(self.esperaba("CDATA o una enumeración «(…)» como tipo del atributo", t.as_ref())),
            },
            Some(Tok::LParen) => {
                // enum_rest -> PIPE NAME enum_rest | RPAR
                let mut valores = Vec::new();
                loop {
                    let v = self.next();
                    match v {
                        Some(Token {
                            tok: Tok::Name(text),
                            span,
                        }) => valores.push(Name { text, span }),
                        _ => return Err(self.esperaba("un valor de la enumeración", v.as_ref())),
                    }
                    let s = self.next();
                    match s.as_ref().map(|t| &t.tok) {
                        Some(Tok::Pipe) => {}
                        Some(Tok::RParen) => return Ok(AttType::Enumeration(valores)),
                        _ => return Err(self.esperaba("«|» o «)» en la enumeración", s.as_ref())),
                    }
                }
            }
            _ => Err(self.esperaba("CDATA o una enumeración «(…)» como tipo del atributo", t.as_ref())),
        }
    }

    // defaultdecl -> K_REQUIRED | K_IMPLIED | K_FIXED LITERAL | LITERAL
    fn defaultdecl(&mut self) -> Result<DefaultDecl, DtdError> {
        let t = self.next();
        match t.as_ref().map(|t| &t.tok) {
            Some(Tok::Hash(h)) if h == "REQUIRED" => Ok(DefaultDecl::Required),
            Some(Tok::Hash(h)) if h == "IMPLIED" => Ok(DefaultDecl::Implied),
            Some(Tok::Hash(h)) if h == "FIXED" => {
                let l = self.next();
                match l {
                    Some(Token {
                        tok: Tok::Literal(value),
                        span,
                    }) => Ok(DefaultDecl::Fixed(Literal { value, span })),
                    _ => Err(self.esperaba("el valor entre comillas de #FIXED", l.as_ref())),
                }
            }
            Some(Tok::Literal(value)) => Ok(DefaultDecl::Default(Literal {
                value: value.clone(),
                span: t.as_ref().map(|t| t.span).expect("hay token"),
            })),
            _ => Err(self.esperaba(
                "#REQUIRED, #IMPLIED, #FIXED \"valor\" o un valor por defecto",
                t.as_ref(),
            )),
        }
    }
}
