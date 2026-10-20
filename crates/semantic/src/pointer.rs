//! Decodificación del fragmento de un `$ref` local.
//!
//! Un `$ref` local es `""` o `#` seguido de un fragmento de URI. El fragmento
//! se decodifica en dos niveles, en este orden (RFC 6901 §6):
//!
//! 1. **RFC 3986**: cada `%XX` (dos dígitos hexadecimales) es un byte; el
//!    resultado debe ser UTF-8 válido. Los demás caracteres se toman tal
//!    cual: no se exige que el fragmento use solo los caracteres que la RFC
//!    permite sin codificar, igual que los validadores habituales (el
//!    meta-esquema oficial no lo comprueba: `format: uri-reference` es una
//!    anotación; D11).
//! 2. Si el fragmento decodificado está vacío, designa la raíz; si empieza
//!    por `/`, es un **puntero JSON** (RFC 6901): se parte en segmentos por
//!    `/` y en cada uno `~1` es `/` y `~0` es `~`, de izquierda a derecha
//!    (así `~01` es `~1`, no `/`); cualquier otro `~` es un error. Si no,
//!    es el nombre de un `$anchor`.

use crate::meta::is_anchor;

/// Fragmento de un `$ref` ya decodificado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fragment {
    /// Puntero JSON con sus segmentos decodificados; vacío = la raíz.
    Pointer(Vec<String>),
    /// Nombre de un `$anchor`.
    Anchor(String),
}

/// Por qué un `$ref` no se puede decodificar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    /// No es `""` ni empieza por `#`.
    NotLocal,
    /// `%` sin dos dígitos hexadecimales, o bytes que no son UTF-8.
    PercentEncoding,
    /// `~` no seguido de `0` o `1` en un segmento del puntero.
    PointerEscape,
    /// No es puntero (no empieza por `/`) ni nombre de ancla válido.
    InvalidAnchor,
}

/// Decodifica un `$ref` local. Costo O(longitud del `$ref`).
pub(crate) fn decode(reference: &str) -> Result<Fragment, DecodeError> {
    if reference.is_empty() {
        // El mismo documento (RFC 3986 §4.4), como `#`.
        return Ok(Fragment::Pointer(Vec::new()));
    }
    let raw = reference.strip_prefix('#').ok_or(DecodeError::NotLocal)?;
    let frag = percent_decode(raw)?;
    if frag.is_empty() {
        return Ok(Fragment::Pointer(Vec::new()));
    }
    match frag.strip_prefix('/') {
        Some(rest) => rest
            .split('/')
            .map(unescape)
            .collect::<Result<Vec<_>, _>>()
            .map(Fragment::Pointer),
        None if is_anchor(&frag) => Ok(Fragment::Anchor(frag)),
        None => Err(DecodeError::InvalidAnchor),
    }
}

fn percent_decode(s: &str) -> Result<String, DecodeError> {
    if !s.contains('%') {
        return Ok(s.to_owned());
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = |j: usize| bytes.get(j).and_then(|&b| (b as char).to_digit(16));
            match (hex(i + 1), hex(i + 2)) {
                (Some(h), Some(l)) => {
                    // h, l < 16: el byte cabe.
                    out.push((h * 16 + l) as u8);
                    i += 3;
                }
                _ => return Err(DecodeError::PercentEncoding),
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| DecodeError::PercentEncoding)
}

/// Segmento de un puntero JSON: `~1` → `/`, `~0` → `~`.
fn unescape(seg: &str) -> Result<String, DecodeError> {
    if !seg.contains('~') {
        return Ok(seg.to_owned());
    }
    let mut out = String::with_capacity(seg.len());
    let mut chars = seg.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0') => out.push('~'),
                Some('1') => out.push('/'),
                _ => return Err(DecodeError::PointerEscape),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// Codifica un segmento para un puntero JSON (inversa de `unescape`).
pub(crate) fn escape(seg: &str) -> String {
    seg.replace('~', "~0").replace('/', "~1")
}

/// Codifica un puntero JSON como fragmento de URI: deja tal cual los
/// caracteres que la RFC 3986 admite en un fragmento y codifica con `%XX`
/// los demás bytes (incluidos `%` y los no ASCII).
pub fn to_fragment(pointer: &str) -> String {
    let mut out = String::with_capacity(pointer.len() + 1);
    out.push('#');
    for &b in pointer.as_bytes() {
        let keep = b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/?".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ptr(segs: &[&str]) -> Result<Fragment, DecodeError> {
        Ok(Fragment::Pointer(
            segs.iter().map(|s| s.to_string()).collect(),
        ))
    }

    #[test]
    fn formas_basicas() {
        assert_eq!(decode(""), ptr(&[]));
        assert_eq!(decode("#"), ptr(&[]));
        assert_eq!(decode("#/"), ptr(&[""]));
        assert_eq!(decode("#/$defs/x"), ptr(&["$defs", "x"]));
        assert_eq!(decode("#/a//b"), ptr(&["a", "", "b"]));
        assert_eq!(decode("#nodo"), Ok(Fragment::Anchor("nodo".into())));
        assert_eq!(decode("a.json#/x"), Err(DecodeError::NotLocal));
        assert_eq!(decode("#1abc"), Err(DecodeError::InvalidAnchor));
        assert_eq!(decode("#a b"), Err(DecodeError::InvalidAnchor));
    }

    /// Ejemplos de la RFC 6901 §5 y §6.
    #[test]
    fn ejemplos_rfc_6901() {
        assert_eq!(decode("#/a~1b"), ptr(&["a/b"]));
        assert_eq!(decode("#/m~0n"), ptr(&["m~n"]));
        assert_eq!(decode("#/c%25d"), ptr(&["c%d"]));
        assert_eq!(decode("#/e%5Ef"), ptr(&["e^f"]));
        assert_eq!(decode("#/g%7Ch"), ptr(&["g|h"]));
        assert_eq!(decode("#/i%5Cj"), ptr(&["i\\j"]));
        assert_eq!(decode("#/k%22l"), ptr(&["k\"l"]));
        assert_eq!(decode("#/%20"), ptr(&[" "]));
        // De izquierda a derecha: `~01` es `~1`, no `/`.
        assert_eq!(decode("#/~01"), ptr(&["~1"]));
        // El `%` se decodifica antes: `%7E1` es `~1`, es decir `/`.
        assert_eq!(decode("#/%7E1"), ptr(&["/"]));
        assert_eq!(decode("#%2F$defs%2Fx"), ptr(&["$defs", "x"]));
        assert_eq!(decode("#/%C3%B1"), ptr(&["ñ"]));
        assert_eq!(decode("#/ñ"), ptr(&["ñ"]));
    }

    #[test]
    fn errores() {
        for r in ["#/a~", "#/a~2", "#/~x", "#/%7E"] {
            assert_eq!(decode(r), Err(DecodeError::PointerEscape), "{r}");
        }
        for r in ["#/%", "#/%2", "#/%zz", "#/%FF", "#/%C3", "#%"] {
            assert_eq!(decode(r), Err(DecodeError::PercentEncoding), "{r}");
        }
    }

    proptest! {
        /// Codificar segmentos arbitrarios (con `~`, `/`, `%`, espacios, no
        /// ASCII…) y decodificarlos devuelve los mismos segmentos.
        #[test]
        fn ida_y_vuelta(segs in prop::collection::vec("[a~/%# ñ😀0-9]{0,6}", 0..5)) {
            let pointer: String = segs.iter().map(|s| format!("/{}", escape(s))).collect();
            let frag = to_fragment(&pointer);
            prop_assert_eq!(decode(&frag), Ok(Fragment::Pointer(segs)));
        }

        /// Nunca falla (sin pánico) con texto arbitrario.
        #[test]
        fn no_falla(s in "#?.{0,24}") {
            let _ = decode(&s);
        }
    }
}
