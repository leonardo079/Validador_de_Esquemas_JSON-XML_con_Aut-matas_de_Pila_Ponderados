//! Detección del formato de un esquema por su contenido (D22 enmendada por
//! D86, enmiendas E1 y E2; `docs/00-alcance.md` §3bis).
//!
//! Es la **única** implementación en Rust: la usan `schemac` y el servidor.
//! La web tiene su copia en TypeScript (`formatoProbable`, `web/src/api.ts`),
//! solo para la etiqueta «Formato detectado»; las dos se prueban con la
//! misma tabla, `tests/schemas/formato.json`.

/// Formato de un esquema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formato {
    /// JSON Schema (subconjunto de 2020-12).
    Json,
    /// XSD (subconjunto de 1.0).
    Xsd,
    /// DTD externa (subconjunto de XML 1.0 §3).
    Dtd,
}

const BOM: &[u8] = b"\xEF\xBB\xBF";

fn espacio(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn saltar_espacios(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|&&b| espacio(b)).count();
    &s[n..]
}

/// Lo que hay después de la primera aparición de `fin` en `s`, o `None` si
/// no aparece.
fn tras<'a>(s: &'a [u8], fin: &[u8]) -> Option<&'a [u8]> {
    s.windows(fin.len())
        .position(|w| w == fin)
        .map(|i| &s[i + fin.len()..])
}

/// Formato de `bytes`. Tras un BOM UTF-8 (uno, y solo al principio), los
/// espacios, una declaración `<?xml …?>` inicial y los comentarios `<!-- … -->`:
///
/// - `<!DOCTYPE`, en cualquier caja, es [`Formato::Xsd`]: así el frontal XSD
///   lo rechaza como hasta ahora (D8);
/// - cualquier otro `<!` es [`Formato::Dtd`];
/// - otro `<` es [`Formato::Xsd`];
/// - lo demás, o nada, es [`Formato::Json`].
///
/// Si la `<?xml` o un comentario no se cierran, es XSD, y el error lo da el
/// lexer XML en su posición. Si el esquema empezaba por `<` (había prólogo),
/// nunca es JSON. El BOM no se quita de la entrada: los lexers lo saltan.
///
/// Coste O(prólogo), sin recursión ni reservas.
pub fn formato(bytes: &[u8]) -> Formato {
    let inicio = saltar_espacios(bytes.strip_prefix(BOM).unwrap_or(bytes));
    if inicio.first() != Some(&b'<') {
        return Formato::Json;
    }
    let mut s = inicio;
    // `<?xml` seguido de espacio o de `?`: no `<?xml-stylesheet`.
    if s.starts_with(b"<?xml") && s.get(5).is_some_and(|&b| espacio(b) || b == b'?') {
        match tras(s, b"?>") {
            Some(r) => s = saltar_espacios(r),
            None => return Formato::Xsd,
        }
    }
    while s.starts_with(b"<!--") {
        match tras(&s[4..], b"-->") {
            Some(r) => s = saltar_espacios(r),
            None => return Formato::Xsd,
        }
    }
    match s {
        [b'<', b'!', resto @ ..]
            if resto.len() >= 7 && resto[..7].eq_ignore_ascii_case(b"DOCTYPE") =>
        {
            Formato::Xsd
        }
        [b'<', b'!', ..] => Formato::Dtd,
        // Otro `<`, o el prólogo seguido de algo que no es `<`: XSD (el
        // lexer XML da el error).
        _ => Formato::Xsd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casos_del_prologo() {
        // Un comentario con un guion suelto se salta hasta su `-->`.
        assert_eq!(formato(b"<!-- a - b --><!ELEMENT a EMPTY>"), Formato::Dtd);
        assert_eq!(formato(b"<?xml version='1.0'?>"), Formato::Xsd);
        assert_eq!(
            formato(b"<?xml-stylesheet href='x'?><!ELEMENT a EMPTY>"),
            Formato::Xsd
        );
        assert_eq!(formato(b"<!DOCTYP"), Formato::Dtd);
    }
}
