//! Las fuentes de `lexer` y `runtime-rs` que el validador generado lleva
//! copiadas (F5.1, D74; F4-U9: un solo ejecutor, que el código generado
//! reutiliza sin reescribirlo a mano).
//!
//! Se incrustan con `include_str!` al compilar `codegen`, así que la copia
//! es siempre la del árbol actual. Para que compilen como módulos de un solo
//! crate (`rustc main.rs`, sin Cargo; `docs/fase7/sandbox.md` §5) se
//! reescriben solo tres cosas, de forma mecánica:
//!
//! 1. el `#![no_std]` de cada `lib.rs` se quita (es un atributo de crate;
//!    la parte de validación sigue sin usar `std` ni `alloc`, y una prueba lo
//!    comprueba compilándola como `#![no_std]`);
//! 2. cada `mod x;` o `pub mod x;` de un `lib.rs` lleva delante
//!    `#[path = "<prefijo>_x.rs"]`: los archivos son planos (el sandbox
//!    solo admite nombres `[a-z0-9_]+.rs`, sin directorios);
//! 3. las rutas `crate::…` pasan a `crate::<módulo>::…` y, en `runtime-rs`,
//!    `lexer::…` pasa a `crate::lexer::…`.
//!
//! La prueba `copia_sincronizada` (en `tests/rust.rs`) vigila que la lista
//! de archivos incrustados sea exactamente la de los dos `src/`.

/// Un crate copiado: nombre del módulo en el validador generado y sus
/// archivos (`lib` es la raíz del módulo).
#[derive(Clone, Copy, Debug)]
pub struct Copiado {
    /// Módulo en el crate generado (`lexer`, `runtime_rs`) y prefijo de los
    /// archivos de sus submódulos.
    pub modulo: &'static str,
    /// Directorio original, relativo a la raíz del repositorio.
    pub origen: &'static str,
    /// `(nombre sin .rs, contenido)`, en orden alfabético.
    pub archivos: &'static [(&'static str, &'static str)],
}

/// `crates/lexer/src`.
pub const LEXER: Copiado = Copiado {
    modulo: "lexer",
    origen: "crates/lexer/src",
    archivos: &[
        ("dot", include_str!("../../lexer/src/dot.rs")),
        ("json", include_str!("../../lexer/src/json.rs")),
        ("lib", include_str!("../../lexer/src/lib.rs")),
        ("short", include_str!("../../lexer/src/short.rs")),
        ("table", include_str!("../../lexer/src/table.rs")),
        ("utf8", include_str!("../../lexer/src/utf8.rs")),
        ("xml", include_str!("../../lexer/src/xml.rs")),
    ],
};

/// `crates/runtime-rs/src`.
pub const RUNTIME: Copiado = Copiado {
    modulo: "runtime_rs",
    origen: "crates/runtime-rs/src",
    archivos: &[
        ("core", include_str!("../../runtime-rs/src/core.rs")),
        ("hash", include_str!("../../runtime-rs/src/hash.rs")),
        ("json", include_str!("../../runtime-rs/src/json.rs")),
        ("lib", include_str!("../../runtime-rs/src/lib.rs")),
        ("limits", include_str!("../../runtime-rs/src/limits.rs")),
        ("num", include_str!("../../runtime-rs/src/num.rs")),
        ("tables", include_str!("../../runtime-rs/src/tables.rs")),
        ("weight", include_str!("../../runtime-rs/src/weight.rs")),
        ("xml", include_str!("../../runtime-rs/src/xml.rs")),
    ],
};

/// Los dos crates, en el orden en que se emiten.
pub const COPIADOS: [Copiado; 2] = [LEXER, RUNTIME];

impl Copiado {
    /// Nombre del archivo emitido para `archivo` (`lexer.rs`,
    /// `lexer_json.rs`).
    pub fn nombre(&self, archivo: &str) -> String {
        if archivo == "lib" {
            format!("{}.rs", self.modulo)
        } else {
            format!("{}_{archivo}.rs", self.modulo)
        }
    }

    /// El contenido de `archivo` reescrito para el crate generado.
    pub fn reescribir(&self, archivo: &str, fuente: &str) -> String {
        let mut s = rutas(fuente, "crate::", &format!("crate::{}::", self.modulo));
        if self.modulo == RUNTIME.modulo {
            s = rutas(&s, "lexer::", "crate::lexer::");
        }
        if archivo == "lib" {
            s = raiz(&s, self.modulo);
        }
        s
    }
}

/// Sustituye `de` por `a` donde empieza una ruta: el carácter anterior no
/// es de identificador, ni `:` (ya es parte de una ruta más larga) ni `$`.
fn rutas(fuente: &str, de: &str, a: &str) -> String {
    let mut out = String::with_capacity(fuente.len() + fuente.len() / 16);
    let mut resto = fuente;
    let mut previo: Option<char> = None;
    while let Some(i) = resto.find(de) {
        let (antes, despues) = resto.split_at(i);
        let anterior = antes.chars().next_back().or(previo);
        out.push_str(antes);
        let empieza =
            !anterior.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '$'));
        out.push_str(if empieza { a } else { de });
        previo = de.chars().next_back();
        resto = &despues[de.len()..];
    }
    out.push_str(resto);
    out
}

/// Quita `#![no_std]` y pone `#[path]` delante de cada declaración de
/// submódulo de la raíz (una línea `mod x;` o `pub mod x;`).
fn raiz(fuente: &str, modulo: &str) -> String {
    let mut out = String::with_capacity(fuente.len() + 512);
    for linea in fuente.split_inclusive('\n') {
        let l = linea.trim_end();
        if l == "#![no_std]" {
            continue;
        }
        if let Some(nombre) = submodulo(l) {
            out.push_str(&format!("#[path = \"{modulo}_{nombre}.rs\"]\n"));
        }
        out.push_str(linea);
    }
    out
}

/// El nombre `x` de una línea `mod x;` o `pub mod x;`.
pub fn submodulo(linea: &str) -> Option<&str> {
    let l = linea.strip_prefix("pub ").unwrap_or(linea);
    let n = l.strip_prefix("mod ")?.strip_suffix(';')?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rutas_solo_al_empezar() {
        let s =
            "use crate::a; x::crate::b; pub(crate) fn f() { $crate::c; ::crate::d; (crate::e) }";
        assert_eq!(
            rutas(s, "crate::", "crate::m::"),
            "use crate::m::a; x::crate::b; pub(crate) fn f() { $crate::c; ::crate::d; (crate::m::e) }"
        );
        // Lo ya reescrito no se vuelve a tocar.
        let t = rutas(
            "use lexer::json; crate::lexer::x; milexer::y",
            "lexer::",
            "crate::lexer::",
        );
        assert_eq!(t, "use crate::lexer::json; crate::lexer::x; milexer::y");
    }

    #[test]
    fn raiz_con_path_y_sin_no_std() {
        let s = "//! doc\n#![no_std]\n\nmod core;\npub mod hash;\nmod tests {\n}\n";
        assert_eq!(
            raiz(s, "runtime_rs"),
            "//! doc\n\n#[path = \"runtime_rs_core.rs\"]\nmod core;\n\
             #[path = \"runtime_rs_hash.rs\"]\npub mod hash;\nmod tests {\n}\n"
        );
    }
}
