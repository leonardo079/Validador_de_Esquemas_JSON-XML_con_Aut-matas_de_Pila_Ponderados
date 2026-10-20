//! Palabras clave fuera del subconjunto: categoría y motivo del rechazo.
//!
//! El parser deja pasar cualquier clave que no está en su tabla como
//! `Kw::Unsupported` (terminal `K_OTHER`). Aquí se decide el mensaje: las de
//! `docs/00-alcance.md` §2.8 y las de borradores anteriores tienen una
//! entrada propia con su categoría y su motivo; cualquier otra es
//! desconocida, con una sugerencia si se parece a una soportada.

use schema_parser::Kw;

use crate::error::Category;

/// Palabra clave sin soporte conocida: nombre, categoría y motivo.
type Entry = (&'static str, Category, &'static str);

const IF: &str = "aplicación condicional: el resultado de `if` decide si el valor debe \
                  cumplir `then` o `else`, así que hay que evaluar los tres subesquemas a la \
                  vez (producto de autómatas)";
const CONTAINS: &str = "cuantificador existencial sobre los elementos: cada elemento entra a la \
                        vez en el subautómata de `contains` y en el de `items`, con un contador \
                        de coincidencias (producto); candidato a añadirse";
const CONTAINS_COUNT: &str = "cuenta las coincidencias de `contains`, que está fuera del \
                              subconjunto";
const UNEVALUATED: &str = "depende de las anotaciones que dejan todos los demás aplicadores \
                           (`properties`, `items`, `allOf`, `anyOf`…), incluidas las ramas que \
                           fallan; no se traduce a transiciones locales del autómata";
const DYNAMIC: &str = "se resuelve según el ámbito dinámico de la evaluación, no al compilar; \
                       el autómata necesita todas las referencias resueltas en tiempo de \
                       compilación";
const RECURSIVE: &str = "Draft 2019-09; 2020-12 lo reemplazó por `$dynamicRef` y \
                         `$dynamicAnchor`, que también están fuera del subconjunto";
const CONTENT: &str = "describe el contenido codificado dentro de una cadena, que el validador \
                       no decodifica; se rechaza para que no parezca que ese contenido se valida";

/// `docs/00-alcance.md` §2.8 y palabras clave de borradores anteriores.
const TABLE: [Entry; 20] = [
    ("if", Category::C, IF),
    ("then", Category::C, IF),
    ("else", Category::C, IF),
    ("contains", Category::C, CONTAINS),
    ("minContains", Category::B, CONTAINS_COUNT),
    ("maxContains", Category::B, CONTAINS_COUNT),
    (
        "patternProperties",
        Category::C,
        "una clave puede coincidir con varias expresiones a la vez y debe cumplir todos sus \
         esquemas (producto de AFD sobre las claves); candidato a añadirse",
    ),
    (
        "propertyNames",
        Category::C,
        "aplica un subesquema a cada nombre de clave (producto del trie de claves con su \
         subautómata); candidato a añadirse",
    ),
    ("unevaluatedProperties", Category::C, UNEVALUATED),
    ("unevaluatedItems", Category::C, UNEVALUATED),
    ("$dynamicRef", Category::Core, DYNAMIC),
    ("$dynamicAnchor", Category::Core, DYNAMIC),
    (
        "$vocabulary",
        Category::Core,
        "solo tiene sentido en un meta-esquema propio; el subconjunto usa el vocabulario fijo \
         de 2020-12",
    ),
    ("contentEncoding", Category::Annotation, CONTENT),
    ("contentMediaType", Category::Annotation, CONTENT),
    ("contentSchema", Category::Annotation, CONTENT),
    (
        "additionalItems",
        Category::Legacy,
        "Draft 2019-09 y anteriores; en 2020-12 los elementos que siguen a `prefixItems` se \
         validan con `items`",
    ),
    (
        "dependencies",
        Category::Legacy,
        "Draft 7 y anteriores; en 2020-12 se divide en `dependentRequired` y \
         `dependentSchemas`",
    ),
    ("$recursiveRef", Category::Legacy, RECURSIVE),
    ("$recursiveAnchor", Category::Legacy, RECURSIVE),
];

const UNKNOWN: &str = "no es una palabra clave de JSON Schema 2020-12 ni del subconjunto; las \
                       extensiones y las claves desconocidas no se ignoran";

/// Categoría, motivo y sugerencia para la palabra clave `name`.
pub(crate) fn classify(name: &str) -> (Category, &'static str, Option<&'static str>) {
    match TABLE.iter().find(|(n, _, _)| *n == name) {
        Some(&(_, cat, reason)) => (cat, reason, None),
        None => (Category::Unknown, UNKNOWN, suggest(name)),
    }
}

/// Palabra clave soportada a distancia de edición ≤ 2 (sin distinguir
/// mayúsculas). Si hay empate gana la que comparte el prefijo más largo con
/// el nombre (`$def` → `$defs`, no `$ref`: el principio de una palabra suele
/// estar bien escrito), y después la primera de la tabla. Los nombres largos
/// no se comparan: así el costo por palabra clave es O(1) y un esquema hostil
/// con claves enormes no lo vuelve cuadrático.
fn suggest(name: &str) -> Option<&'static str> {
    const MAX_LEN: usize = 32;
    if name.is_empty() || name.chars().nth(MAX_LEN).is_some() {
        return None;
    }
    let name: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    // Clave a minimizar: (distancia, −prefijo común).
    let mut best: Option<((usize, isize), &'static str)> = None;
    for kw in Kw::names() {
        let cand: Vec<char> = kw.chars().flat_map(char::to_lowercase).collect();
        let d = distance(&name, &cand);
        let prefix = name.iter().zip(&cand).take_while(|(a, b)| a == b).count();
        let key = (d, -(prefix as isize));
        if d <= 2 && best.is_none_or(|(b, _)| key < b) {
            best = Some((key, kw));
        }
    }
    best.map(|(_, kw)| kw)
}

/// Distancia de Levenshtein con dos filas.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_tabla_no_choca_con_las_soportadas() {
        for (n, _, _) in TABLE {
            assert_eq!(Kw::from_name(n), Kw::Unsupported, "{n}");
        }
    }

    #[test]
    fn sugerencias() {
        assert_eq!(suggest("minLenght"), Some("minLength"));
        assert_eq!(suggest("MinLength"), Some("minLength"));
        assert_eq!(suggest("requried"), Some("required"));
        assert_eq!(suggest("$deff"), Some("$defs"));
        // Empate a distancia 1 con `$ref`: gana el prefijo común más largo.
        assert_eq!(suggest("$def"), Some("$defs"));
        assert_eq!(suggest("$DEF"), Some("$defs"));
        assert_eq!(suggest("minimun"), Some("minimum"));
        assert_eq!(suggest("definiton"), Some("definitions"));
        assert_eq!(suggest("x-extension"), None);
        assert_eq!(suggest(""), None);
        assert_eq!(suggest(&"a".repeat(1000)), None);
    }

    #[test]
    fn distancia_de_edicion() {
        let d = |a: &str, b: &str| {
            distance(
                &a.chars().collect::<Vec<_>>(),
                &b.chars().collect::<Vec<_>>(),
            )
        };
        assert_eq!(d("", "abc"), 3);
        assert_eq!(d("kitten", "sitting"), 3);
        assert_eq!(d("ñandú", "nandu"), 2);
        assert_eq!(d("igual", "igual"), 0);
    }
}
