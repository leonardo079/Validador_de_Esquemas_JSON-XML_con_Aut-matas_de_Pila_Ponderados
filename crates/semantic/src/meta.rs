//! Validación del esquema contra el meta-esquema 2020-12 restringido al
//! subconjunto de `docs/00-alcance.md`.
//!
//! El parser LL(1) ya fijó la **forma** de cada valor (que `minimum` sea un
//! número, `required` un arreglo de cadenas…). Aquí se comprueba lo que la
//! gramática no expresa porque depende del valor y no del tipo de token:
//! rangos (`minLength` entero ≥ 0, `multipleOf` > 0), nombres de `type`,
//! unicidad dentro de `required`, arreglos no vacíos, la sintaxis de
//! `$anchor`, las formas que el subconjunto excluye (`$ref` remoto, otro
//! `$schema`, `$id` fuera de la raíz) y, sobre todo, el rechazo de las
//! palabras clave fuera del subconjunto (D6). La tabla completa, con la regla
//! del meta-esquema oficial de la que sale cada comprobación, está en
//! `docs/fase3/meta-esquema.md`.
//!
//! El recorrido del AST es **iterativo**, con una pila explícita en el
//! montón: su consumo de pila del hilo no depende de la profundidad del
//! esquema (D7). Como las comprobaciones son locales a cada palabra clave
//! (solo se arrastra si el esquema es la raíz), el orden de visita no
//! importa: los errores se ordenan por posición al final.

use std::collections::HashSet;

use schema_parser::error::short;
use schema_parser::{Keyword, Kw, Num, Schema, Str, Value};

use crate::error::{ErrorKind, SemanticError};
use crate::number::classify;
use crate::unsupported;

/// Único dialecto aceptado en `$schema` (`docs/00-alcance.md` §2.1), con o
/// sin un `#` final vacío.
pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Nombres de tipo de 2020-12 (`$defs/simpleTypes` del meta-esquema).
pub const TYPE_NAMES: [&str; 7] = [
    "null", "boolean", "object", "array", "number", "integer", "string",
];

const EXPECT_TYPE: &str =
    "un nombre de tipo (null, boolean, object, array, number, integer o string)";
const EXPECT_NON_NEG_INT: &str = "un entero no negativo";

/// Valida el esquema contra el meta-esquema restringido. Devuelve **todos**
/// los errores, ordenados por posición (offset en la entrada); la lista nunca
/// está vacía si es `Err`.
///
/// Costo: O(n) en el tamaño del AST (cada palabra clave se visita una vez y
/// la unicidad usa una tabla hash), más O(e log e) para ordenar e errores.
/// No recursivo: no puede desbordar la pila del hilo, sea cual sea la
/// profundidad del AST.
pub fn check_meta_schema(schema: &Schema) -> Result<(), Vec<SemanticError>> {
    let mut errors = Vec::new();
    // (esquema, es la raíz del documento)
    let mut stack: Vec<(&Schema, bool)> = vec![(schema, true)];
    while let Some((s, root)) = stack.pop() {
        for kw in s.keywords() {
            check_keyword(kw, root, &mut errors);
            match &kw.value {
                Value::Schema(sub) => stack.push((sub, false)),
                Value::SchemaMap(m) => stack.extend(m.iter().map(|(_, sub)| (sub, false))),
                Value::SchemaList(l) => stack.extend(l.iter().map(|sub| (sub, false))),
                _ => {}
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        // Estable: dos errores en el mismo offset conservan el orden de visita.
        errors.sort_by_key(|e: &SemanticError| e.at.offset);
        Err(errors)
    }
}

/// Comprobaciones de una palabra clave. Las que no aparecen aquí no tienen
/// nada que comprobar más allá de la forma, que ya verificó el parser.
fn check_keyword(kw: &Keyword, root: bool, errors: &mut Vec<SemanticError>) {
    let name = kw.kw.name().unwrap_or("");
    match (kw.kw, &kw.value) {
        (Kw::Unsupported, _) => {
            let (category, reason, suggestion) = unsupported::classify(&kw.name.value);
            errors.push(SemanticError {
                kind: ErrorKind::UnsupportedKeyword {
                    name: kw.name.value.clone(),
                    category,
                    reason,
                    suggestion,
                },
                at: kw.name.span.start,
            });
        }
        (Kw::DollarSchema, Value::Str(v)) => {
            if !root {
                errors.push(not_at_root(
                    kw,
                    "`$schema` solo puede ir en la raíz del documento: el subconjunto no \
                     admite recursos embebidos (`$id` solo en la raíz)",
                ));
            } else if !is_dialect(&v.value) {
                errors.push(form(
                    kw,
                    v,
                    "solo se admite el dialecto https://json-schema.org/draft/2020-12/schema \
                     (con o sin `#` final)",
                ));
            }
        }
        (Kw::Id, Value::Str(v)) => {
            if !root {
                errors.push(not_at_root(
                    kw,
                    "`$id` solo se admite en la raíz, como URI base; el subconjunto no admite \
                     recursos embebidos",
                ));
            }
            // Meta-esquema core: "pattern": "^[^#]*#?$".
            let ok = match v.value.find('#') {
                Some(i) => i + 1 == v.value.len(),
                None => true,
            };
            if !ok {
                errors.push(invalid(
                    name,
                    "una URI sin fragmento (a lo sumo un `#` final vacío)",
                    v,
                ));
            }
        }
        (Kw::Anchor, Value::Str(v)) if !is_anchor(&v.value) => errors.push(invalid(
            name,
            "un nombre que empiece por una letra ASCII o `_`, seguido de letras, dígitos, \
             `-`, `.` o `_`",
            v,
        )),
        // `""` es una referencia al mismo documento (RFC 3986 §4.4), equivalente
        // a `#`: es local y se acepta (D11). Lo demás sin `#` inicial nombra
        // otro recurso.
        (Kw::Ref, Value::Str(v)) if !is_local_ref(&v.value) => errors.push(form(
            kw,
            v,
            "solo se admiten referencias locales: un fragmento que empieza por `#` (`#`, \
             `#/$defs/x`, `#ancla`) o la cadena vacía; no se descargan recursos remotos",
        )),
        (Kw::Type, Value::Str(v)) => check_type_name(v, errors),
        (Kw::Type, Value::Strings(l)) => {
            if l.is_empty() {
                errors.push(empty(kw, "un arreglo no vacío de nombres de tipo"));
            }
            for v in l {
                check_type_name(v, errors);
            }
            unique(name, "un arreglo sin nombres de tipo repetidos", l, errors);
        }
        (Kw::MultipleOf, Value::Num(n)) => {
            let i = classify(&n.lexeme);
            if i.zero || i.negative {
                errors.push(invalid_num(name, "un número mayor que 0", n));
            }
        }
        (
            Kw::MinLength
            | Kw::MaxLength
            | Kw::MinItems
            | Kw::MaxItems
            | Kw::MinProperties
            | Kw::MaxProperties,
            Value::Num(n),
        ) => {
            let i = classify(&n.lexeme);
            if i.negative || !i.integer {
                errors.push(invalid_num(name, EXPECT_NON_NEG_INT, n));
            }
        }
        (Kw::Required, Value::Strings(l)) => {
            unique(name, "un arreglo de cadenas sin repetir", l, errors)
        }
        (Kw::DependentRequired, Value::StringMap(m)) => {
            for (_, l) in m {
                unique(name, "un arreglo de cadenas sin repetir", l, errors);
            }
        }
        (Kw::PrefixItems | Kw::AllOf | Kw::AnyOf | Kw::OneOf, Value::SchemaList(l))
            if l.is_empty() =>
        {
            errors.push(empty(kw, "un arreglo no vacío de esquemas"));
        }
        _ => {}
    }
}

/// `$schema` de 2020-12. Se admite también con un `#` final: un fragmento
/// vacío no cambia el recurso (RFC 3986 §6.2.3) y es la forma que se copia de
/// Draft 7 (`docs/00-alcance.md` §2.1).
fn is_dialect(s: &str) -> bool {
    s.strip_suffix('#').unwrap_or(s) == DIALECT
}

/// `$ref` local: `""` (el mismo documento, RFC 3986 §4.4) o `#…`.
pub(crate) fn is_local_ref(s: &str) -> bool {
    s.is_empty() || s.starts_with('#')
}

/// `$anchor`: `^[A-Za-z_][-A-Za-z0-9._]*$` (meta-esquema core,
/// `$defs/anchorString`). También decide si el fragmento de un `$ref` es un
/// ancla.
pub(crate) fn is_anchor(s: &str) -> bool {
    let mut bytes = s.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_alphabetic() || b == b'_' => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
}

fn check_type_name(v: &Str, errors: &mut Vec<SemanticError>) {
    if !TYPE_NAMES.contains(&v.value.as_str()) {
        errors.push(invalid("type", EXPECT_TYPE, v));
    }
}

/// Cadenas sin repetir (`uniqueItems: true` en el meta-esquema). Reporta la
/// segunda aparición de cada repetida, como las claves repetidas del parser.
fn unique(
    keyword: &'static str,
    expected: &'static str,
    l: &[Str],
    errors: &mut Vec<SemanticError>,
) {
    let mut seen = HashSet::new();
    for s in l {
        if !seen.insert(s.value.as_str()) {
            errors.push(SemanticError {
                kind: ErrorKind::InvalidValue {
                    keyword,
                    expected,
                    found: format!("\"{}\" repetida", short(&s.value)),
                },
                at: s.span.start,
            });
        }
    }
}

#[cold]
fn invalid(keyword: &'static str, expected: &'static str, v: &Str) -> SemanticError {
    SemanticError {
        kind: ErrorKind::InvalidValue {
            keyword,
            expected,
            found: format!("\"{}\"", short(&v.value)),
        },
        at: v.span.start,
    }
}

#[cold]
fn invalid_num(keyword: &'static str, expected: &'static str, n: &Num) -> SemanticError {
    SemanticError {
        kind: ErrorKind::InvalidValue {
            keyword,
            expected,
            found: format!("`{}`", short(&n.lexeme)),
        },
        at: n.span.start,
    }
}

/// Arreglo vacío donde el meta-esquema exige `minItems: 1`; se señala el `[`.
#[cold]
fn empty(kw: &Keyword, expected: &'static str) -> SemanticError {
    SemanticError {
        kind: ErrorKind::InvalidValue {
            keyword: kw.kw.name().unwrap_or(""),
            expected,
            found: "`[]`".to_owned(),
        },
        at: kw.value_span.start,
    }
}

/// Valor de una forma excluida; se señala el valor.
#[cold]
fn form(kw: &Keyword, v: &Str, reason: &'static str) -> SemanticError {
    SemanticError {
        kind: ErrorKind::UnsupportedForm {
            keyword: kw.kw.name().unwrap_or(""),
            found: format!("\"{}\"", short(&v.value)),
            reason,
        },
        at: v.span.start,
    }
}

/// Palabra clave que solo vale en la raíz; se señala su nombre.
#[cold]
fn not_at_root(kw: &Keyword, reason: &'static str) -> SemanticError {
    SemanticError {
        kind: ErrorKind::UnsupportedForm {
            keyword: kw.kw.name().unwrap_or(""),
            found: "en un subesquema".to_owned(),
            reason,
        },
        at: kw.name.span.start,
    }
}
