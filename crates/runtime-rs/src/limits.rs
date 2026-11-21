//! Límites del ejecutor (F4.6; `docs/00-alcance.md` §4, F4-U16).
//!
//! Pasar cualquiera es un error `limit` de peso ∞ en el evento que lo pasa,
//! nunca un pánico ni una reserva. [`Limits::default`] da los valores del
//! alcance; [`crate::Tables::state_words`] da la memoria que el llamador debe
//! prestar para ellos. F6.2 los expone con nombre en `schemac compile
//! --limit` y en `sv_limits_t` (`docs/fase6/limites.md`); el tiempo máximo
//! no es de la biblioteca (`docs/00-alcance.md` §4): lo vigila quien la
//! alimenta, con [`crate::JsonValidator::abort_limit`].

/// Límites al validar un payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Profundidad de anidamiento: objetos y arreglos en JSON, elementos con
    /// contenido en XML (256).
    pub max_depth: usize,
    /// Bytes UTF-8 (ya sin escapes ni referencias) de una cadena JSON, un
    /// nombre, un valor de atributo o un tramo de texto XML (1 MiB).
    pub max_string: u64,
    /// Bytes del lexema de un número que leen las comprobaciones numéricas
    /// (rangos, `multipleOf`, `enum`/`const` numéricos, `uniqueItems`); más
    /// largo es `limit` (1 024 = [`crate::LEXEME_MAX`], que es también el
    /// tope: un valor mayor cuenta como [`crate::LEXEME_MAX`]).
    pub max_lexeme: usize,
    /// Claves no declaradas de un objeto (o atributos sin bit de una
    /// etiqueta) que se guardan para detectar repetidas (1 024).
    pub max_keys: usize,
    /// Reserva común de esas claves entre todos los objetos abiertos (4 096).
    pub key_pool: usize,
    /// Elementos de los arreglos con `uniqueItems` abiertos a la vez cuyos
    /// hashes se guardan (4 096, F4-U16).
    pub max_unique: usize,
    /// Referencias de entidad o de carácter de un documento XML (100 000).
    pub max_xml_refs: u64,
    /// Bytes del payload (`u64::MAX`: sin límite, *streaming*).
    pub max_payload: u64,
    /// Clave de SipHash-1-3 de la capa de buena formación ([`crate::hash`]).
    pub hash_key: [u64; 2],
}

impl Limits {
    pub const DEFAULT: Limits = Limits {
        max_depth: 256,
        max_string: 1 << 20,
        max_lexeme: 1024,
        max_keys: 1024,
        key_pool: 4096,
        max_unique: 4096,
        max_xml_refs: 100_000,
        max_payload: u64::MAX,
        hash_key: [0x5356_5f4b_4559_5f30, 0x7369_7068_6173_6831],
    };
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}
