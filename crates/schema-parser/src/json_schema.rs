//! Parser LL(1) descendente recursivo del metalenguaje JSON Schema.
//!
//! Una función por no terminal de `docs/fase2/esquema.ll1`; cada una decide
//! su producción mirando un solo token (`self.look`), exactamente como indica
//! la tabla de `docs/fase2/ll1-esquema.md`. Las listas `X → item X_tail | ε`,
//! `X_tail → , item X_tail | ε` se recorren con un bucle en
//! [`Parser::delimited`] en lugar de con recursión por la cola.
//!
//! El token `STRING` en posición de clave de un objeto-esquema se refina en
//! una clase `K_*` según su valor ([`Kw::from_name`]); en cualquier otra
//! posición es un `STRING` sin más.
//!
//! La recursión del parser sigue el anidamiento de `{`/`[` del esquema, por
//! eso está acotada por `max_depth`: superarla es un error reportado, no un
//! desbordamiento de la pila del proceso. Cada nivel cuesta ~1 KiB de pila
//! optimizado y ~5,5 KiB sin optimizar (medido; ver
//! `docs/fase2/gramatica-esquema.md` §6): con [`DEFAULT_MAX_DEPTH`] el peor
//! caso cabe en los 2 MiB que Rust y tokio dan por defecto a cada hilo.

use std::collections::HashSet;
use std::mem;

use lexer::Span;

use crate::ast::{Class, Json, JsonKind, Keyword, Kw, Num, Schema, SchemaKind, Str, Value};
use crate::error::{short, ErrorKind, ParseError};
use crate::token::{Tok, Token, Tokens};

/// Profundidad máxima de anidamiento de `{`/`[` por defecto, la misma de los
/// payloads (`docs/00-alcance.md` §4).
pub const DEFAULT_MAX_DEPTH: u32 = 256;

/// Analiza un esquema JSON completo. Devuelve el AST o el primer error, con
/// su línea y columna.
pub fn parse(input: &[u8]) -> Result<Schema, ParseError> {
    parse_with_max_depth(input, DEFAULT_MAX_DEPTH)
}

/// Como [`parse`], con otra profundidad máxima de anidamiento de `{`/`[`.
pub fn parse_with_max_depth(input: &[u8], max_depth: u32) -> Result<Schema, ParseError> {
    parse_boxed(input, max_depth).map_err(|e| *e)
}

fn parse_boxed(input: &[u8], max_depth: u32) -> Res<Schema> {
    let mut toks = Tokens::new(input);
    let look = toks.next_token().map_err(Box::new)?;
    let mut p = Parser {
        toks,
        look,
        last_end: 0,
        depth: 0,
        max_depth,
    };
    let schema = p.schema()?;
    if p.look.tok != Tok::Eof {
        return Err(p.unexpected("el fin de la entrada", None));
    }
    Ok(schema)
}

/// Dentro del parser el error va en una caja: así cada `Result` intermedio
/// ocupa poco y los marcos de la recursión son pequeños (ver
/// [`DEFAULT_MAX_DEPTH`]).
type Res<T> = Result<T, Box<ParseError>>;

/// Claves ya vistas en el objeto en curso. Con una tabla hash la detección de
/// claves repetidas cuesta O(1) esperado por clave, así que el análisis sigue
/// siendo lineal en el tamaño del esquema. El hash por defecto de `std`
/// (SipHash con semilla aleatoria) evita que un esquema hostil fuerce
/// colisiones.
type Keys = HashSet<String>;

#[derive(Debug)]
struct Parser<'a> {
    toks: Tokens<'a>,
    /// Token de anticipación (el único que mira LL(1)).
    look: Token,
    /// Desplazamiento final del último token consumido.
    last_end: u64,
    depth: u32,
    max_depth: u32,
}

// FIRST de los no terminales que encabezan una lista.
fn is_str(t: &Tok) -> bool {
    matches!(t, Tok::Str(_))
}

fn is_schema_first(t: &Tok) -> bool {
    matches!(t, Tok::LBrace | Tok::True | Tok::False)
}

fn is_value_first(t: &Tok) -> bool {
    !matches!(
        t,
        Tok::RBrace | Tok::RBrack | Tok::Colon | Tok::Comma | Tok::Eof
    )
}

const EXPECT_SCHEMA: &str = "un esquema (objeto `{…}`, `true` o `false`)";

/// Pista para formas de versiones anteriores del estándar que el subconjunto
/// excluye a propósito (`docs/00-alcance.md` §2.8).
fn hint(kw: Kw, found: &Tok) -> Option<&'static str> {
    match (kw, found) {
        (Kw::Items, Tok::LBrack) => Some(
            "la forma de arreglo de `items` (Draft 2019-09) está fuera del subconjunto; \
             use `prefixItems`",
        ),
        (Kw::ExclusiveMinimum | Kw::ExclusiveMaximum, Tok::True | Tok::False) => Some(
            "la forma booleana de Draft 4 está fuera del subconjunto; \
             use el límite numérico",
        ),
        _ => None,
    }
}

impl<'a> Parser<'a> {
    /// Consume el token de anticipación y lee el siguiente.
    fn bump(&mut self) -> Res<Token> {
        let next = self.toks.next_token().map_err(Box::new)?;
        let t = mem::replace(&mut self.look, next);
        self.last_end = t.span.end;
        Ok(t)
    }

    /// Error en el token de anticipación: celda vacía de la tabla LL(1).
    #[cold]
    #[inline(never)]
    fn unexpected(
        &self,
        expected: impl Into<String>,
        hint: Option<&'static str>,
    ) -> Box<ParseError> {
        Box::new(ParseError {
            kind: ErrorKind::Unexpected {
                expected: expected.into(),
                found: self.look.tok.describe(),
                hint,
            },
            at: self.look.span.start,
        })
    }

    fn expect(&mut self, tok: Tok, expected: &str) -> Res<Token> {
        if self.look.tok == tok {
            self.bump()
        } else {
            Err(self.unexpected(expected, None))
        }
    }

    /// Consume un `STRING`; el llamador ya comprobó que lo es.
    fn string(&mut self, expected: &str) -> Res<Str> {
        if !is_str(&self.look.tok) {
            return Err(self.unexpected(expected, None));
        }
        let t = self.bump()?;
        let value = match t.tok {
            Tok::Str(s) => s,
            _ => String::new(),
        };
        Ok(Str {
            value,
            span: t.span,
        })
    }

    /// Rango desde `start` hasta el final del último token consumido.
    fn span_from(&self, start: Span) -> Span {
        Span {
            start: start.start,
            end: self.last_end,
        }
    }

    /// `open items close` con `items → item tail | ε` y
    /// `tail → , item tail | ε`. `self.look` es el delimitador de apertura.
    /// `item` recibe el conjunto de claves ya vistas en esta lista (vacío y sin
    /// reservar memoria en las listas que no tienen claves).
    fn delimited<T>(
        &mut self,
        close: Tok,
        first: fn(&Tok) -> bool,
        item: fn(&mut Self, &mut Keys) -> Res<T>,
        what: &str,
    ) -> Res<(Vec<T>, Span)> {
        let open = self.look.span;
        if self.depth >= self.max_depth {
            return Err(Box::new(ParseError {
                kind: ErrorKind::TooDeep {
                    max: self.max_depth,
                },
                at: open.start,
            }));
        }
        self.depth += 1;
        self.bump()?;
        let close_s = close.describe();
        let mut items = Vec::new();
        let mut keys = Keys::new();
        if self.look.tok != close {
            if !first(&self.look.tok) {
                return Err(self.unexpected(format!("{what} o {close_s}"), None));
            }
            loop {
                let it = item(self, &mut keys)?;
                items.push(it);
                if self.look.tok == Tok::Comma {
                    self.bump()?;
                    if !first(&self.look.tok) {
                        return Err(self.unexpected(what, None));
                    }
                } else if self.look.tok == close {
                    break;
                } else {
                    return Err(self.unexpected(format!("`,` o {close_s}"), None));
                }
            }
        }
        self.bump()?;
        self.depth -= 1;
        Ok((items, self.span_from(open)))
    }

    // ------------------------------------------------------- esquemas ---

    /// `schema → { keywords } | true | false`
    fn schema(&mut self) -> Res<Schema> {
        match self.look.tok {
            Tok::True | Tok::False => {
                let t = self.bump()?;
                Ok(Schema {
                    kind: SchemaKind::Bool(t.tok == Tok::True),
                    span: t.span,
                })
            }
            Tok::LBrace => {
                let (kws, span) =
                    self.delimited(Tok::RBrace, is_str, Self::keyword, "una palabra clave")?;
                Ok(Schema {
                    kind: SchemaKind::Object(kws),
                    span,
                })
            }
            _ => Err(self.unexpected(EXPECT_SCHEMA, None)),
        }
    }

    /// `keyword → K_* : …` (una alternativa por clase).
    fn keyword(&mut self, keys: &mut Keys) -> Res<Keyword> {
        let name = self.string("una palabra clave")?;
        if !keys.insert(name.value.clone()) {
            return Err(duplicate(&name));
        }
        let kw = Kw::from_name(&name.value);
        if self.look.tok != Tok::Colon {
            return Err(self.missing_colon(&name.value));
        }
        self.bump()?;
        let start = self.look.span;
        let value = self.keyword_value(kw, &name.value)?;
        Ok(Keyword {
            name,
            kw,
            value,
            value_span: self.span_from(start),
        })
    }

    /// Parte derecha de `keyword` según la clase `K_*` de la palabra clave.
    /// Cada alternativa está en su propia función para que el marco de esta,
    /// que está en el ciclo de la recursión, sea pequeño.
    fn keyword_value(&mut self, kw: Kw, name: &str) -> Res<Value> {
        let t = &self.look.tok;
        match kw.class() {
            Class::String if is_str(t) => self.string("una cadena").map(Value::Str),
            Class::Number if matches!(t, Tok::Num(_)) => self.number().map(Value::Num),
            Class::Bool if matches!(t, Tok::True | Tok::False) => {
                Ok(Value::Bool(self.bump()?.tok == Tok::True))
            }
            Class::Schema if is_schema_first(t) => self.schema_value(),
            Class::SchemaMap if *t == Tok::LBrace => self.schema_map(),
            Class::SchemaList if *t == Tok::LBrack => self.schema_list(),
            Class::Values if *t == Tok::LBrack => self.json_list(),
            Class::Type if is_str(t) => self.string("una cadena").map(Value::Str),
            Class::Type | Class::Required if *t == Tok::LBrack => {
                self.string_array().map(Value::Strings)
            }
            Class::DependentRequired if *t == Tok::LBrace => self.dep_map(),
            Class::Value | Class::Other if is_value_first(t) => self.json().map(Value::Json),
            _ => Err(self.bad_value(kw, name)),
        }
    }

    /// `K_SCHEMA : schema`
    fn schema_value(&mut self) -> Res<Value> {
        Ok(Value::Schema(Box::new(self.schema()?)))
    }

    /// `schema_map → { entries }`
    fn schema_map(&mut self) -> Res<Value> {
        let (m, _) = self.delimited(
            Tok::RBrace,
            is_str,
            Self::schema_entry,
            "un nombre (cadena)",
        )?;
        Ok(Value::SchemaMap(m))
    }

    /// `K_LIST : [ schemas ]`
    fn schema_list(&mut self) -> Res<Value> {
        let (l, _) = self.delimited(
            Tok::RBrack,
            is_schema_first,
            |p, _| p.schema(),
            EXPECT_SCHEMA,
        )?;
        Ok(Value::SchemaList(l))
    }

    /// `K_VALUES : array`
    fn json_list(&mut self) -> Res<Value> {
        Ok(Value::JsonList(self.json_array()?.0))
    }

    /// `dep_map → { deps }`
    fn dep_map(&mut self) -> Res<Value> {
        let (m, _) = self.delimited(Tok::RBrace, is_str, Self::dep_entry, "un nombre (cadena)")?;
        Ok(Value::StringMap(m))
    }

    /// Celda vacía en la fila de la alternativa `K_* : …` de `kw`.
    #[cold]
    #[inline(never)]
    fn bad_value(&self, kw: Kw, name: &str) -> Box<ParseError> {
        let expected = match kw.class() {
            Class::String => "una cadena",
            Class::Number => "un número",
            Class::Bool => "`true` o `false`",
            Class::Schema => EXPECT_SCHEMA,
            Class::SchemaMap => "un objeto de esquemas",
            Class::SchemaList => "un arreglo de esquemas",
            Class::Values => "un arreglo",
            Class::Type => "un nombre de tipo o un arreglo de nombres",
            Class::Required => "un arreglo de cadenas",
            Class::DependentRequired => "un objeto de arreglos de cadenas",
            Class::Value | Class::Other => "un valor JSON",
        };
        let expected = format!("{expected} como valor de \"{}\"", short(name));
        self.unexpected(expected, hint(kw, &self.look.tok))
    }

    #[cold]
    #[inline(never)]
    fn missing_colon(&self, name: &str) -> Box<ParseError> {
        self.unexpected(format!("`:` después de \"{}\"", short(name)), None)
    }

    fn number(&mut self) -> Res<Num> {
        let t = self.bump()?;
        let lexeme = match t.tok {
            Tok::Num(n) => n,
            _ => String::new(),
        };
        Ok(Num {
            lexeme,
            span: t.span,
        })
    }

    /// `entries → STRING : schema entries_tail | ε` (un elemento).
    fn schema_entry(&mut self, keys: &mut Keys) -> Res<(Str, Schema)> {
        let name = self.key(keys)?;
        Ok((name, self.schema()?))
    }

    /// `deps → STRING : string_array deps_tail | ε` (un elemento).
    fn dep_entry(&mut self, keys: &mut Keys) -> Res<(Str, Vec<Str>)> {
        let name = self.key(keys)?;
        if self.look.tok != Tok::LBrack {
            return Err(self.unexpected("un arreglo de cadenas", None));
        }
        Ok((name, self.string_array()?))
    }

    /// `STRING :` de una entrada de mapa, sin repetir una clave anterior.
    fn key(&mut self, keys: &mut Keys) -> Res<Str> {
        let name = self.string("un nombre (cadena)")?;
        if !keys.insert(name.value.clone()) {
            return Err(duplicate(&name));
        }
        self.expect(Tok::Colon, "`:`")?;
        Ok(name)
    }

    /// `string_array → [ strings ]`
    fn string_array(&mut self) -> Res<Vec<Str>> {
        Ok(self
            .delimited(
                Tok::RBrack,
                is_str,
                |p, _| p.string("una cadena"),
                "una cadena",
            )?
            .0)
    }

    // ----------------------------------------------------- JSON genérico ---

    /// `value → object | array | STRING | NUMBER | true | false | null`
    fn json(&mut self) -> Res<Json> {
        let (kind, span) = match self.look.tok {
            Tok::LBrace => {
                let (members, span) =
                    self.delimited(Tok::RBrace, is_str, Self::pair, "un nombre (cadena)")?;
                (JsonKind::Object(members), span)
            }
            Tok::LBrack => {
                let (items, span) = self.json_array()?;
                (JsonKind::Array(items), span)
            }
            Tok::Str(_) | Tok::Num(_) | Tok::True | Tok::False | Tok::Null => {
                let t = self.bump()?;
                let kind = match t.tok {
                    Tok::Str(s) => JsonKind::String(s),
                    Tok::Num(n) => JsonKind::Number(n),
                    Tok::True => JsonKind::Bool(true),
                    Tok::False => JsonKind::Bool(false),
                    _ => JsonKind::Null,
                };
                (kind, t.span)
            }
            _ => return Err(self.unexpected("un valor JSON", None)),
        };
        Ok(Json { kind, span })
    }

    /// `array → [ elements ]`
    fn json_array(&mut self) -> Res<(Vec<Json>, Span)> {
        self.delimited(
            Tok::RBrack,
            is_value_first,
            |p, _| p.json(),
            "un valor JSON",
        )
    }

    /// `pair → STRING : value`
    fn pair(&mut self, keys: &mut Keys) -> Res<(Str, Json)> {
        let name = self.key(keys)?;
        if !is_value_first(&self.look.tok) {
            return Err(self.unexpected("un valor JSON", None));
        }
        Ok((name, self.json()?))
    }
}

#[cold]
#[inline(never)]
fn duplicate(name: &Str) -> Box<ParseError> {
    Box::new(ParseError {
        kind: ErrorKind::DuplicateKey(name.value.clone()),
        at: name.span.start,
    })
}
