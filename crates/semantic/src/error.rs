//! Errores del análisis semántico del esquema, con línea y columna.
//!
//! Siguen el formato de los de la Fase 2 (`schema_parser::ParseError`):
//! posición del token que provoca el error, `code()` estable para las pruebas
//! y la web, y un mensaje de una sola línea que empieza por
//! `línea L, columna C: `.

use core::fmt;

use schema_parser::error::short;
use schema_parser::Pos;

/// Un error semántico. A diferencia del parser, la Fase 3 no se detiene en el
/// primero: [`crate::check_meta_schema`], [`crate::SymbolTable::build`] y
/// [`crate::check_cycles`] devuelven todos, en orden de posición.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticError {
    pub kind: ErrorKind,
    /// Posición del nombre de la palabra clave o del valor que provoca el error.
    pub at: Pos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Palabra clave fuera del subconjunto (`docs/00-alcance.md` §2.8, D6).
    /// Se rechaza siempre: no hay modo que la ignore.
    UnsupportedKeyword {
        /// Nombre tal como aparece en el esquema.
        name: String,
        category: Category,
        /// Por qué no se soporta.
        reason: &'static str,
        /// Palabra clave soportada más parecida, si el nombre parece una errata.
        suggestion: Option<&'static str>,
    },
    /// Palabra clave soportada usada en una forma que el subconjunto excluye:
    /// `$ref` remoto, `$schema` de otro borrador, `$id` o `$schema` fuera de
    /// la raíz.
    UnsupportedForm {
        keyword: &'static str,
        /// Qué forma se encontró (texto del usuario ya recortado y escapado).
        found: String,
        /// Por qué no se soporta.
        reason: &'static str,
    },
    /// Valor que no cumple el meta-esquema 2020-12 restringido al subconjunto
    /// (`docs/fase3/meta-esquema.md` §3).
    InvalidValue {
        keyword: &'static str,
        /// Qué exige el meta-esquema.
        expected: &'static str,
        /// Qué se encontró (texto del usuario ya recortado y escapado).
        found: String,
    },
    /// `$ref` que no se puede resolver (`docs/fase3/referencias.md`). Se
    /// señala el inicio del valor del `$ref`.
    UnresolvedRef {
        /// El `$ref` tal como aparece (ya recortado y escapado).
        reference: String,
        problem: RefProblem,
    },
    /// `$anchor` repetido en el documento. Se señala la segunda aparición.
    DuplicateAnchor {
        name: String,
        /// Posición del valor de la primera aparición.
        first: Pos,
    },
    /// Ciclo de aristas que no consumen entrada (`$ref` y aplicadores sobre
    /// la misma instancia; `docs/fase3/ciclos.md`). Se señala el primer
    /// `$ref` del ciclo en el texto.
    NonProductiveCycle {
        /// Fragmento (`#…`) del esquema que contiene ese `$ref`.
        start: String,
        /// Aristas del ciclo desde `start`: (palabra clave, fragmento del
        /// destino). La primera es el `$ref` y la última vuelve a `start`. Si
        /// el ciclo es largo solo se guardan las primeras y la última.
        edges: Vec<(String, String)>,
        /// Número total de aristas del ciclo mostrado.
        total: usize,
    },
}

/// Por qué no se resuelve un `$ref`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefProblem {
    /// No es `""` ni empieza por `#` (la Fase 3 ya lo rechaza antes, en el
    /// meta-esquema; aquí solo por si se llama sin esa validación).
    NotLocal,
    /// `%` sin dos dígitos hexadecimales, o el fragmento no es UTF-8 tras
    /// decodificarlo (RFC 3986).
    PercentEncoding,
    /// `~` no seguido de `0` o `1` en el puntero (RFC 6901).
    PointerEscape,
    /// El fragmento no es un puntero (no empieza por `/`) ni un nombre de
    /// ancla válido.
    InvalidAnchor,
    /// No hay ningún `$anchor` con ese nombre.
    UnknownAnchor,
    /// El segmento `index` (desde 1) del puntero no existe en el documento.
    NotFound { segment: String, index: usize },
    /// El puntero entra en el valor literal de una palabra clave (`const`,
    /// `enum`, `default`, `examples` o una fuera del subconjunto).
    IntoLiteral { keyword: String },
    /// El puntero termina (o entra) en algo que no es un esquema: el valor
    /// de una palabra clave que no lleva esquemas (`minimum`, `required`…)
    /// o el objeto/arreglo que contiene los esquemas (`properties`, `allOf`…).
    NotASchema { keyword: String },
}

impl fmt::Display for RefProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefProblem::NotLocal => {
                write!(f, "no es una referencia local (`#…` o la cadena vacía)")
            }
            RefProblem::PercentEncoding => write!(
                f,
                "el fragmento tiene un escape `%` mal formado o no es UTF-8 al decodificarlo \
                 (RFC 3986)"
            ),
            RefProblem::PointerEscape => write!(
                f,
                "en un puntero JSON `~` debe ir seguido de `0` o `1` (RFC 6901)"
            ),
            RefProblem::InvalidAnchor => write!(
                f,
                "el fragmento no es un puntero JSON (empieza por `/`) ni un nombre de ancla \
                 válido"
            ),
            RefProblem::UnknownAnchor => write!(f, "no hay ningún `$anchor` con ese nombre"),
            RefProblem::NotFound { segment, index } => write!(
                f,
                "el destino no existe: no se encuentra el segmento {index} del puntero, \"{}\"",
                short(segment)
            ),
            RefProblem::IntoLiteral { keyword } => write!(
                f,
                "el puntero entra en el valor literal de \"{}\", que no es un esquema",
                short(keyword)
            ),
            RefProblem::NotASchema { keyword } => write!(
                f,
                "el destino no es un esquema sino el valor de \"{}\"",
                short(keyword)
            ),
        }
    }
}

/// Categoría de una palabra clave **rechazada**, la de la columna «Cat.» de
/// `docs/00-alcance.md`, ampliada con las que no tienen categoría de
/// reconocibilidad. El mensaje de rechazo la nombra (D6). No hay variante
/// para la (a): ninguna palabra clave fuera del subconjunto lo es. La
/// clasificación de las soportadas es otra tarea de la Fase 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// (b) con registros o contadores acotados.
    B,
    /// (c) no determinista o no libre de contexto.
    C,
    /// Palabra clave del núcleo (identificadores y referencias, «—» en el
    /// alcance): no valida por sí misma.
    Core,
    /// Anotación: en 2020-12 no cambia el resultado de la validación.
    Annotation,
    /// Palabra clave de un borrador anterior que 2020-12 reemplazó.
    Legacy,
    /// No es una palabra clave de JSON Schema 2020-12.
    Unknown,
}

impl Category {
    /// Texto de la categoría en los mensajes.
    pub fn describe(self) -> &'static str {
        match self {
            Category::B => "clase (b), con registros o contadores",
            Category::C => "clase (c), no determinista o no libre de contexto",
            Category::Core => "núcleo de JSON Schema, sin clase de reconocibilidad",
            Category::Annotation => "anotación",
            Category::Legacy => "de un borrador anterior a 2020-12",
            Category::Unknown => "desconocida",
        }
    }
}

impl SemanticError {
    pub fn line(&self) -> u32 {
        self.at.line
    }

    pub fn column(&self) -> u32 {
        self.at.column
    }

    /// Nombre corto de la clase de error, estable para las pruebas y la web.
    pub fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::UnsupportedKeyword { .. } | ErrorKind::UnsupportedForm { .. } => {
                "unsupported"
            }
            ErrorKind::InvalidValue { .. } => "meta-schema",
            ErrorKind::UnresolvedRef { .. } => "reference",
            ErrorKind::DuplicateAnchor { .. } => "duplicate-anchor",
            ErrorKind::NonProductiveCycle { .. } => "cycle",
        }
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "línea {}, columna {}: ", self.at.line, self.at.column)?;
        match &self.kind {
            ErrorKind::UnsupportedKeyword {
                name,
                category,
                reason,
                suggestion,
            } => {
                write!(
                    f,
                    "palabra clave \"{}\" no soportada ({}): {reason}",
                    short(name),
                    category.describe()
                )?;
                match suggestion {
                    Some(s) => write!(f, " (¿quiso decir \"{s}\"?)"),
                    None => Ok(()),
                }
            }
            ErrorKind::UnsupportedForm {
                keyword,
                found,
                reason,
            } => write!(
                f,
                "forma de \"{keyword}\" fuera del subconjunto ({found}): {reason}"
            ),
            ErrorKind::InvalidValue {
                keyword,
                expected,
                found,
            } => write!(
                f,
                "valor inválido para \"{keyword}\" según el meta-esquema: \
                 se esperaba {expected}, se encontró {found}"
            ),
            ErrorKind::UnresolvedRef { reference, problem } => {
                write!(
                    f,
                    "no se puede resolver \"$ref\": \"{reference}\": {problem}"
                )
            }
            ErrorKind::DuplicateAnchor { name, first } => write!(
                f,
                "ancla \"{}\" repetida: ya se definió en la línea {}, columna {}",
                short(name),
                first.line,
                first.column
            ),
            ErrorKind::NonProductiveCycle {
                start,
                edges,
                total,
            } => {
                write!(
                    f,
                    "ciclo de referencias que no consume entrada (validar el valor exigiría \
                     volver a validarlo con el mismo esquema sin leer ningún token): {}",
                    clip(start)
                )?;
                let gap = *total > edges.len();
                for (i, (via, to)) in edges.iter().enumerate() {
                    if gap && i + 1 == edges.len() {
                        write!(f, " …")?;
                    }
                    write!(f, " —{}→ {}", clip(via), clip(to))?;
                }
                if gap {
                    write!(f, " ({total} aristas)")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for SemanticError {}

/// Fragmento o palabra clave en un mensaje: recortado a 80 caracteres. Los
/// fragmentos ya están codificados (sin caracteres de control); las palabras
/// clave son las de la tabla.
fn clip(s: &str) -> String {
    const MAX: usize = 80;
    match s.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_owned(),
    }
}
