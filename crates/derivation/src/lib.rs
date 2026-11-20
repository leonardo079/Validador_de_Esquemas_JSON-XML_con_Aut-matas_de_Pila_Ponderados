//! F5.5 — Árbol de derivación de un payload JSON o XML (D69;
//! `docs/fase5/arbol-derivacion.md`).
//!
//! Un parser predictivo LL(1) con **pila explícita** (sin recursión, D7) sobre
//! las gramáticas de `docs/fase2/json.ll1` y `docs/fase2/xml.ll1`, alimentado
//! por los tokens del lexer de la Fase 1 en streaming. Cada vez que saca un no
//! terminal de la pila elige la producción con el token de adelanto y crea su
//! nodo. Así el árbol queda en el orden de la derivación más a la izquierda, sin
//! retroceso.
//!
//! Salidas: [`a_json`], JSON jerárquico (`name`, `children`…) para
//! d3-hierarchy (F7.6), y [`a_dot`], DOT para la documentación.
//!
//! Esto no es el validador: el DPDA no construye árbol (invariante del
//! proyecto). El árbol sirve para mostrar y explicar, y por eso tiene un tope de
//! nodos ([`MAX_NODOS`]).

use lexer::json::{Event as JEv, JsonLexer};
use lexer::xml::{Event as XEv, XmlLexer};
use lexer::{Error as LexErr, Sink, Span};

/// Tope de nodos por defecto: un árbol más grande no se puede leer en
/// pantalla.
pub const MAX_NODOS: usize = 5000;

/// Formato del payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formato {
    Json,
    Xml,
}

/// Decide el formato como `schemac` (D22): XML si el primer byte
/// significativo (tras un BOM y espacios) es `<`.
pub fn formato(p: &[u8]) -> Formato {
    let p = p.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(p);
    match p.iter().find(|b| !b" \t\r\n".contains(b)) {
        Some(b'<') => Formato::Xml,
        _ => Formato::Json,
    }
}

/// Clase de un nodo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clase {
    /// No terminal de la gramática.
    NoTerminal,
    /// Terminal (token), con su lexema.
    Terminal,
    /// Producción vacía.
    Epsilon,
}

/// Un nodo del árbol. Los hijos están en `Arbol::nodos` por índice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nodo {
    pub simbolo: &'static str,
    pub clase: Clase,
    /// Texto del token (terminales): los bytes del payload, recortados a
    /// [`MAX_LEXEMA`] caracteres.
    pub lexema: String,
    /// Bytes `[inicio, fin)` del payload que cubre (terminales; en los no
    /// terminales, desde el primer terminal hasta el último).
    pub inicio: u64,
    pub fin: u64,
    /// JSON Pointer del valor (solo en los nodos `value` de JSON).
    pub puntero: Option<String>,
    pub hijos: Vec<usize>,
}

/// Lexema máximo que se guarda por terminal (en caracteres).
pub const MAX_LEXEMA: usize = 48;

/// El árbol: `nodos[0]` es la raíz (`value` o `document`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arbol {
    pub formato: Formato,
    pub nodos: Vec<Nodo>,
}

/// Por qué no hay árbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Error léxico de la Fase 1 en `offset`.
    Lexico { offset: u64, mensaje: String },
    /// Token inesperado para la gramática en `offset`.
    Sintactico { offset: u64, mensaje: String },
    /// El árbol pasaría de `max` nodos.
    Grande { max: usize },
    /// Pasa la profundidad máxima del proyecto ([`MAX_PROFUNDIDAD`]) en
    /// `offset`. Sin este tope, los punteros JSON sumarían O(n²) bytes.
    Profundo { offset: u64, max: usize },
}

/// Profundidad máxima de anidamiento (`docs/00-alcance.md` §4).
pub const MAX_PROFUNDIDAD: usize = 256;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Lexico { offset, mensaje } => {
                write!(f, "error léxico en el byte {offset}: {mensaje}")
            }
            Error::Sintactico { offset, mensaje } => {
                write!(f, "error sintáctico en el byte {offset}: {mensaje}")
            }
            Error::Grande { max } => write!(f, "el árbol pasa de {max} nodos"),
            Error::Profundo { offset, max } => {
                write!(
                    f,
                    "en el byte {offset} el anidamiento pasa de {max} niveles"
                )
            }
        }
    }
}

// ------------------------------------------------------------- tokens ---

/// Terminal de las gramáticas (los de JSON y los agrupados de XML).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum T {
    LLave,
    RLlave,
    LCorchete,
    RCorchete,
    DosPuntos,
    Coma,
    Cadena,
    Numero,
    True,
    False,
    Null,
    STag,
    Attr,
    Cierra,
    Vacia,
    ETag,
    Texto,
    Pi,
    Fin,
}

impl T {
    fn nombre(self) -> &'static str {
        match self {
            T::LLave => "{",
            T::RLlave => "}",
            T::LCorchete => "[",
            T::RCorchete => "]",
            T::DosPuntos => ":",
            T::Coma => ",",
            T::Cadena => "STRING",
            T::Numero => "NUMBER",
            T::True => "true",
            T::False => "false",
            T::Null => "null",
            T::STag => "STAG",
            T::Attr => "ATTR",
            T::Cierra => ">",
            T::Vacia => "/>",
            T::ETag => "ETAG",
            T::Texto => "TEXT",
            T::Pi => "PI",
            T::Fin => "fin de la entrada",
        }
    }
}

#[derive(Clone, Debug)]
struct Tok {
    t: T,
    inicio: u64,
    fin: u64,
    /// Nombre de la etiqueta (STAG, ETAG) o la clave sin escapes (STRING).
    nombre: String,
}

/// Agrupa los eventos del lexer JSON en tokens de la gramática.
#[derive(Default)]
struct TokJson {
    toks: Vec<Tok>,
    texto: String,
}

impl Sink<JEv> for TokJson {
    type Error = ();
    fn event(&mut self, ev: JEv, s: Span) -> Result<(), ()> {
        let simple = |t| Tok {
            t,
            inicio: s.start.offset,
            fin: s.end,
            nombre: String::new(),
        };
        let tok = match ev {
            JEv::BeginObject => simple(T::LLave),
            JEv::EndObject => simple(T::RLlave),
            JEv::BeginArray => simple(T::LCorchete),
            JEv::EndArray => simple(T::RCorchete),
            JEv::Colon => simple(T::DosPuntos),
            JEv::Comma => simple(T::Coma),
            JEv::True => simple(T::True),
            JEv::False => simple(T::False),
            JEv::Null => simple(T::Null),
            JEv::StringBegin => {
                self.texto.clear();
                return Ok(());
            }
            JEv::StringChar(c) => {
                self.texto.push(c);
                return Ok(());
            }
            JEv::StringEnd => Tok {
                t: T::Cadena,
                inicio: s.start.offset,
                fin: s.end,
                nombre: std::mem::take(&mut self.texto),
            },
            JEv::NumberBegin | JEv::Number(..) => return Ok(()),
            JEv::NumberEnd => Tok {
                t: T::Numero,
                inicio: s.start.offset,
                fin: s.end,
                nombre: String::new(),
            },
        };
        self.toks.push(tok);
        Ok(())
    }
}

/// Agrupa los eventos del lexer XML en los terminales de `xml.ll1`.
#[derive(Default)]
struct TokXml {
    toks: Vec<Tok>,
    /// Token en construcción (STAG, ETAG, ATTR, TEXT o PI).
    abierto: Option<Tok>,
}

impl TokXml {
    fn cerrar(&mut self) {
        if let Some(t) = self.abierto.take() {
            self.toks.push(t);
        }
    }
    fn abrir(&mut self, t: T, inicio: u64) {
        self.cerrar();
        self.abierto = Some(Tok {
            t,
            inicio,
            fin: inicio,
            nombre: String::new(),
        });
    }
}

impl Sink<XEv> for TokXml {
    type Error = ();
    fn event(&mut self, ev: XEv, s: Span) -> Result<(), ()> {
        let (ini, fin) = (s.start.offset, s.end);
        match ev {
            XEv::StartTagOpen => self.abrir(T::STag, ini),
            XEv::EndTagOpen => self.abrir(T::ETag, ini),
            XEv::AttrBegin => self.abrir(T::Attr, ini),
            XEv::PiOpen => self.abrir(T::Pi, ini),
            XEv::NameChar(c) => {
                if let Some(t) = self.abierto.as_mut() {
                    if matches!(t.t, T::STag | T::ETag | T::Attr | T::Pi) {
                        t.nombre.push(c);
                    }
                    t.fin = fin;
                }
            }
            XEv::AttrValueBegin | XEv::AttrValueChar(_) | XEv::PiChar(_) => {
                if let Some(t) = self.abierto.as_mut() {
                    t.fin = fin;
                }
            }
            XEv::AttrValueEnd | XEv::PiClose => {
                if let Some(t) = self.abierto.as_mut() {
                    t.fin = fin;
                }
                self.cerrar();
            }
            XEv::StartTagClose | XEv::EmptyTagClose => {
                self.cerrar();
                let t = if ev == XEv::StartTagClose {
                    T::Cierra
                } else {
                    T::Vacia
                };
                // El `Span` cubre la etiqueta entera: el token es el final.
                let largo = if t == T::Cierra { 1 } else { 2 };
                self.toks.push(Tok {
                    t,
                    inicio: fin.saturating_sub(largo),
                    fin,
                    nombre: String::new(),
                });
            }
            XEv::EndTagClose => {
                if let Some(t) = self.abierto.as_mut() {
                    t.fin = fin;
                }
                self.cerrar();
            }
            XEv::TextChar(_) => match self.abierto.as_mut() {
                Some(t) if t.t == T::Texto => t.fin = fin,
                _ => {
                    self.abrir(T::Texto, ini);
                    if let Some(t) = self.abierto.as_mut() {
                        t.fin = fin;
                    }
                }
            },
        }
        Ok(())
    }
}

fn lexico(e: LexErr<()>) -> Error {
    match e {
        LexErr::Lex(l) => Error::Lexico {
            offset: l.at.offset,
            mensaje: format!("{:?}", l.kind),
        },
        LexErr::Sink(()) => Error::Lexico {
            offset: 0,
            mensaje: "interno".into(),
        },
    }
}

fn tokens(p: &[u8], f: Formato) -> Result<Vec<Tok>, Error> {
    let mut toks = match f {
        Formato::Json => {
            let mut lx = JsonLexer::new();
            let mut s = TokJson::default();
            lx.feed(p, &mut s).map_err(lexico)?;
            lx.finish(&mut s).map_err(lexico)?;
            s.toks
        }
        Formato::Xml => {
            let mut lx = XmlLexer::default();
            let mut s = TokXml::default();
            lx.feed(p, &mut s).map_err(lexico)?;
            lx.finish(&mut s).map_err(lexico)?;
            s.cerrar();
            s.toks
        }
    };
    let n = p.len() as u64;
    toks.push(Tok {
        t: T::Fin,
        inicio: n,
        fin: n,
        nombre: String::new(),
    });
    Ok(toks)
}

// ------------------------------------------------------------ gramáticas ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    N(&'static str),
    T(T),
}

/// La producción de `nt` con el adelanto `t` (tablas LL(1) de
/// `docs/fase2/ll1-json.md` y `ll1-xml.md`); `None` si no hay.
fn produccion(nt: &str, t: T) -> Option<&'static [S]> {
    use S::{N, T as Tm};
    Some(match (nt, t) {
        ("value", T::LLave) => &[N("object")],
        ("value", T::LCorchete) => &[N("array")],
        ("value", T::Cadena) => &[Tm(T::Cadena)],
        ("value", T::Numero) => &[Tm(T::Numero)],
        ("value", T::True) => &[Tm(T::True)],
        ("value", T::False) => &[Tm(T::False)],
        ("value", T::Null) => &[Tm(T::Null)],
        ("object", T::LLave) => &[Tm(T::LLave), N("members"), Tm(T::RLlave)],
        ("members", T::Cadena) => &[N("pair"), N("members_tail")],
        ("members", T::RLlave) => &[],
        ("members_tail", T::Coma) => &[Tm(T::Coma), N("pair"), N("members_tail")],
        ("members_tail", T::RLlave) => &[],
        ("pair", T::Cadena) => &[Tm(T::Cadena), Tm(T::DosPuntos), N("value")],
        ("array", T::LCorchete) => &[Tm(T::LCorchete), N("elements"), Tm(T::RCorchete)],
        (
            "elements",
            T::LLave | T::LCorchete | T::Cadena | T::Numero | T::True | T::False | T::Null,
        ) => &[N("value"), N("elements_tail")],
        ("elements", T::RCorchete) => &[],
        ("elements_tail", T::Coma) => &[Tm(T::Coma), N("value"), N("elements_tail")],
        ("elements_tail", T::RCorchete) => &[],
        ("document", T::Texto | T::Pi | T::STag) => &[N("misc"), N("element"), N("misc")],
        ("misc", T::Texto) => &[Tm(T::Texto), N("misc")],
        ("misc", T::Pi) => &[Tm(T::Pi), N("misc")],
        ("misc", T::STag | T::Fin) => &[],
        ("element", T::STag) => &[Tm(T::STag), N("attrs"), N("element_rest")],
        ("element_rest", T::Vacia) => &[Tm(T::Vacia)],
        ("element_rest", T::Cierra) => &[Tm(T::Cierra), N("content"), Tm(T::ETag)],
        ("attrs", T::Attr) => &[Tm(T::Attr), N("attrs")],
        ("attrs", T::Cierra | T::Vacia) => &[],
        ("content", T::Texto) => &[Tm(T::Texto), N("content")],
        ("content", T::Pi) => &[Tm(T::Pi), N("content")],
        ("content", T::STag) => &[N("element"), N("content")],
        ("content", T::ETag) => &[],
        _ => return None,
    })
}

/// Terminales que esperaba `nt` (para el mensaje de error).
fn esperados(nt: &str) -> String {
    const TODOS: [T; 19] = [
        T::LLave,
        T::RLlave,
        T::LCorchete,
        T::RCorchete,
        T::DosPuntos,
        T::Coma,
        T::Cadena,
        T::Numero,
        T::True,
        T::False,
        T::Null,
        T::STag,
        T::Attr,
        T::Cierra,
        T::Vacia,
        T::ETag,
        T::Texto,
        T::Pi,
        T::Fin,
    ];
    let v: Vec<&str> = TODOS
        .iter()
        .filter(|&&t| produccion(nt, t).is_some())
        .map(|t| t.nombre())
        .collect();
    v.join(", ")
}

fn escapar_puntero(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

/// Recorta a `MAX_LEXEMA` caracteres.
fn recortar(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    if s.chars().count() <= MAX_LEXEMA {
        return s.into_owned();
    }
    let mut r: String = s.chars().take(MAX_LEXEMA - 1).collect();
    r.push('…');
    r
}

/// Contexto de un contenedor JSON abierto, para los punteros.
enum Ctx {
    Objeto { puntero: String, clave: String },
    Arreglo { puntero: String, i: usize },
}

/// El árbol de derivación de `p` con como mucho `max_nodos` nodos.
pub fn derivar(p: &[u8], max_nodos: usize) -> Result<Arbol, Error> {
    let f = formato(p);
    let toks = tokens(p, f)?;
    let raiz = if f == Formato::Json {
        "value"
    } else {
        "document"
    };
    let mut nodos: Vec<Nodo> = Vec::new();
    let nuevo =
        |nodos: &mut Vec<Nodo>, simbolo, clase, padre: Option<usize>| -> Result<usize, Error> {
            if nodos.len() >= max_nodos {
                return Err(Error::Grande { max: max_nodos });
            }
            nodos.push(Nodo {
                simbolo,
                clase,
                lexema: String::new(),
                inicio: u64::MAX,
                fin: 0,
                puntero: None,
                hijos: Vec::new(),
            });
            let n = nodos.len() - 1;
            if let Some(pa) = padre {
                nodos[pa].hijos.push(n);
            }
            Ok(n)
        };
    // Pila de (símbolo, padre). Padres de cada nodo, para propagar rangos.
    let mut pila: Vec<(S, Option<usize>)> = vec![(S::N(raiz), None)];
    let mut padres: Vec<Option<usize>> = Vec::new();
    let mut ctx: Vec<Ctx> = Vec::new();
    let mut etiquetas: Vec<String> = Vec::new();
    let mut k = 0usize;
    while let Some((s, padre)) = pila.pop() {
        let tok = &toks[k];
        match s {
            S::N(nt) => {
                let n = nuevo(&mut nodos, nt, Clase::NoTerminal, padre)?;
                padres.push(padre);
                if nt == "value" {
                    let pt = match ctx.last_mut() {
                        None => String::new(),
                        Some(Ctx::Objeto { puntero, clave }) => {
                            format!("{puntero}/{}", escapar_puntero(clave))
                        }
                        Some(Ctx::Arreglo { puntero, i }) => {
                            *i += 1;
                            format!("{puntero}/{}", *i - 1)
                        }
                    };
                    nodos[n].puntero = Some(pt);
                }
                let Some(rhs) = produccion(nt, tok.t) else {
                    return Err(Error::Sintactico {
                        offset: tok.inicio,
                        mensaje: format!(
                            "en {nt} se esperaba {}; llegó {}",
                            esperados(nt),
                            tok.t.nombre()
                        ),
                    });
                };
                if rhs.is_empty() {
                    nuevo(&mut nodos, "ε", Clase::Epsilon, Some(n))?;
                    padres.push(Some(n));
                }
                for &x in rhs.iter().rev() {
                    pila.push((x, Some(n)));
                }
            }
            S::T(t) => {
                if tok.t != t {
                    return Err(Error::Sintactico {
                        offset: tok.inicio,
                        mensaje: format!("se esperaba {}; llegó {}", t.nombre(), tok.t.nombre()),
                    });
                }
                let n = nuevo(&mut nodos, t.nombre(), Clase::Terminal, padre)?;
                padres.push(padre);
                nodos[n].lexema = recortar(&p[tok.inicio as usize..tok.fin as usize]);
                nodos[n].inicio = tok.inicio;
                nodos[n].fin = tok.fin;
                // Punteros JSON y nombres de etiqueta XML.
                let abuelo = padre.and_then(|pa| padres[pa]);
                match t {
                    T::LLave | T::LCorchete if ctx.len() >= MAX_PROFUNDIDAD => {
                        return Err(Error::Profundo {
                            offset: tok.inicio,
                            max: MAX_PROFUNDIDAD,
                        });
                    }
                    T::STag if etiquetas.len() >= MAX_PROFUNDIDAD => {
                        return Err(Error::Profundo {
                            offset: tok.inicio,
                            max: MAX_PROFUNDIDAD,
                        });
                    }
                    T::LLave | T::LCorchete => {
                        let puntero = abuelo
                            .and_then(|a| nodos[a].puntero.clone())
                            .unwrap_or_default();
                        ctx.push(if t == T::LLave {
                            Ctx::Objeto {
                                puntero,
                                clave: String::new(),
                            }
                        } else {
                            Ctx::Arreglo { puntero, i: 0 }
                        });
                    }
                    T::RLlave | T::RCorchete => {
                        ctx.pop();
                    }
                    T::Cadena if padre.is_some_and(|pa| nodos[pa].simbolo == "pair") => {
                        if let Some(Ctx::Objeto { clave, .. }) = ctx.last_mut() {
                            clave.clone_from(&tok.nombre);
                        }
                    }
                    T::STag => etiquetas.push(tok.nombre.clone()),
                    T::Vacia => {
                        etiquetas.pop();
                    }
                    T::ETag => {
                        let abierta = etiquetas.pop().unwrap_or_default();
                        if abierta != tok.nombre {
                            return Err(Error::Sintactico {
                                offset: tok.inicio,
                                mensaje: format!("</{}> cierra <{abierta}>", tok.nombre),
                            });
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
        }
    }
    if toks[k].t != T::Fin {
        return Err(Error::Sintactico {
            offset: toks[k].inicio,
            mensaje: format!("sobra {} después del valor", toks[k].t.nombre()),
        });
    }
    // Rango de cada no terminal: de su primer a su último terminal. Los hijos
    // tienen siempre índice mayor que el padre, así que basta recorrer al revés.
    for i in (0..nodos.len()).rev() {
        if let Some(pa) = padres[i] {
            let (ini, fin) = (nodos[i].inicio, nodos[i].fin);
            nodos[pa].inicio = nodos[pa].inicio.min(ini);
            nodos[pa].fin = nodos[pa].fin.max(fin);
        }
    }
    for n in &mut nodos {
        if n.inicio == u64::MAX {
            n.inicio = n.fin;
        }
    }
    Ok(Arbol { formato: f, nodos })
}

// -------------------------------------------------------------- salidas ---

fn cadena_json(s: &str, o: &mut String) {
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
}

/// JSON jerárquico para `d3.hierarchy` (iterativo): cada nodo es
/// `{"name", "kind": "nt"|"t"|"eps", "start", "end", ["lexeme"], ["pointer"],
/// ["children"]}`.
pub fn a_json(a: &Arbol) -> String {
    let mut o = String::new();
    // (nodo, siguiente hijo que falta escribir)
    let mut pila: Vec<(usize, usize)> = vec![(0, 0)];
    while let Some(&mut (n, ref mut h)) = pila.last_mut() {
        let nodo = &a.nodos[n];
        if *h == 0 {
            o.push_str("{\"name\":");
            cadena_json(nodo.simbolo, &mut o);
            let kind = match nodo.clase {
                Clase::NoTerminal => "nt",
                Clase::Terminal => "t",
                Clase::Epsilon => "eps",
            };
            o.push_str(&format!(
                ",\"kind\":\"{kind}\",\"start\":{},\"end\":{}",
                nodo.inicio, nodo.fin
            ));
            if nodo.clase == Clase::Terminal {
                o.push_str(",\"lexeme\":");
                cadena_json(&nodo.lexema, &mut o);
            }
            if let Some(pt) = &nodo.puntero {
                o.push_str(",\"pointer\":");
                cadena_json(pt, &mut o);
            }
            if !nodo.hijos.is_empty() {
                o.push_str(",\"children\":[");
            }
        }
        if *h < nodo.hijos.len() {
            if *h > 0 {
                o.push(',');
            }
            let hijo = nodo.hijos[*h];
            *h += 1;
            pila.push((hijo, 0));
        } else {
            if !nodo.hijos.is_empty() {
                o.push(']');
            }
            o.push('}');
            pila.pop();
        }
    }
    o
}

fn etiqueta_dot(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\\\n"),
            '\r' => o.push_str("\\\\r"),
            '\t' => o.push_str("\\\\t"),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o
}

/// DOT del árbol: no terminales en elipse, terminales en caja con su lexema y
/// ε en gris; hijos en orden (`ordering=out`).
pub fn a_dot(a: &Arbol, nombre: &str) -> String {
    let mut o = format!(
        "// Árbol de derivación de «{}» ({}, F5.5). Generado: `make docs`.\ndigraph derivacion {{\n  ordering=out;\n  node [fontname=\"Helvetica\", fontsize=11];\n",
        etiqueta_dot(nombre),
        if a.formato == Formato::Json { "docs/fase2/json.ll1" } else { "docs/fase2/xml.ll1" }
    );
    for (i, n) in a.nodos.iter().enumerate() {
        let linea = match n.clase {
            Clase::NoTerminal => format!("  n{i} [label=\"{}\"];\n", etiqueta_dot(n.simbolo)),
            Clase::Terminal if n.simbolo == n.lexema => {
                format!(
                    "  n{i} [shape=box, style=filled, fillcolor=\"#e8f0fe\", label=\"{}\"];\n",
                    etiqueta_dot(n.simbolo)
                )
            }
            Clase::Terminal => format!(
                "  n{i} [shape=box, style=filled, fillcolor=\"#e8f0fe\", label=\"{}\\n{}\"];\n",
                etiqueta_dot(n.simbolo),
                etiqueta_dot(&n.lexema)
            ),
            Clase::Epsilon => {
                format!("  n{i} [shape=plaintext, fontcolor=\"#888888\", label=\"ε\"];\n")
            }
        };
        o.push_str(&linea);
    }
    for (i, n) in a.nodos.iter().enumerate() {
        for h in &n.hijos {
            o.push_str(&format!("  n{i} -> n{h};\n"));
        }
    }
    o.push_str("}\n");
    o
}
