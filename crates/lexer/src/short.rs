//! Texto del usuario dentro de un mensaje de error, acotado y en una línea.
//!
//! Lo comparten todas las fases que repiten nombres, cadenas o lexemas del
//! esquema en sus mensajes: el parser (Fase 2), el análisis semántico y el
//! verificador de la IR (Fase 3). Vive aquí porque `lexer` es la única
//! dependencia común de todos ellos (`ir` solo depende de `lexer`). Es
//! `no_std` y no reserva memoria: [`Short`] se escribe directamente en el
//! `Formatter`.

use core::fmt;

/// Caracteres del usuario que se muestran como mucho; si hay más, se añade
/// `…`.
pub const SHORT_MAX: usize = 32;

/// Texto del usuario para un mensaje: con los caracteres de control
/// escapados (el mensaje ocupa siempre una línea; `str::escape_debug`) y
/// recortado a [`SHORT_MAX`] caracteres, seguido de `…` si se recortó.
/// Cada carácter escapado ocupa a lo sumo 10 bytes (`\u{10ffff}`), así que
/// el texto mide ≤ 10 · [`SHORT_MAX`] + 3 bytes sea cual sea la entrada.
#[derive(Clone, Copy, Debug)]
pub struct Short<'a>(pub &'a str);

/// Atajo para [`Short`]: `format!("\"{}\"", short(nombre))`.
pub fn short(s: &str) -> Short<'_> {
    Short(s)
}

impl fmt::Display for Short<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.char_indices().nth(SHORT_MAX) {
            Some((cut, _)) => write!(f, "{}…", self.0[..cut].escape_debug()),
            None => write!(f, "{}", self.0.escape_debug()),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use super::*;

    #[test]
    fn corto_sin_cambios() {
        assert_eq!(short("abc").to_string(), "abc");
        assert_eq!(short("").to_string(), "");
    }

    #[test]
    fn recorta_a_32_caracteres() {
        let s: std::string::String = "é".repeat(SHORT_MAX);
        assert_eq!(short(&s).to_string(), s);
        let largo = "é".repeat(SHORT_MAX + 1);
        assert_eq!(short(&largo).to_string(), std::format!("{s}…"));
    }

    #[test]
    fn escapa_controles_y_comillas() {
        assert_eq!(short("a\nb\"c\u{7f}").to_string(), "a\\nb\\\"c\\u{7f}");
    }
}
