//! F6.3 — Payloads inválidos y malformados por mutación de válidos (D67).
//!
//! Una mutación cambia un tramo de un payload JSON válido. Las **semánticas**
//! lo dejan bien formado (cambian un tipo, salen de un rango, borran o añaden
//! un miembro…) y buscan casos `invalid`; las **sintácticas** lo rompen
//! (truncan, dejan una coma colgante, un cero a la izquierda, un escape o un
//! byte UTF-8 inválidos, una clave repetida…) y buscan casos `malformed`.
//!
//! El `expected` de cada caso sale del reporte del DPDA, y el caso solo se
//! acepta si la línea base DOM (D48) da lo mismo: la misma clase de error y,
//! si es `schema`, el mismo número de violaciones, o, si es malformado, el
//! mismo offset ([`clasificar`]). Todo es determinista con la semilla.

use automaton::{Automaton, ErrorKind, Mode};

use crate::generar::Azar;

/// Clase de un tramo de valor JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tipo {
    Objeto,
    Arreglo,
    Cadena,
    Numero,
    Literal,
}

/// Un valor del payload: bytes `ini..fin`, su clase y, si es el valor de un
/// miembro, dónde empieza su clave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tramo {
    pub ini: usize,
    pub fin: usize,
    pub tipo: Tipo,
    pub clave: Option<usize>,
    pub en_arreglo: bool,
}

fn fin_cadena(p: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < p.len() {
        match p[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    p.len()
}

/// Los tramos de todos los valores de un JSON bien formado, en preorden.
/// Iterativo (D7).
pub fn tramos(p: &[u8]) -> Vec<Tramo> {
    // (índice del tramo, es objeto, espera clave)
    let mut pila: Vec<(usize, bool, bool)> = Vec::new();
    let mut out: Vec<Tramo> = Vec::new();
    let mut clave: Option<usize> = None;
    let mut i = 0;
    while i < p.len() {
        let c = p[i];
        let tope = pila.last().copied();
        let nuevo = |ini: usize, fin: usize, tipo: Tipo, clave: &mut Option<usize>| Tramo {
            ini,
            fin,
            tipo,
            clave: if tope.is_some_and(|t| t.1) {
                clave.take()
            } else {
                None
            },
            en_arreglo: tope.is_some_and(|t| !t.1),
        };
        match c {
            b' ' | b'\t' | b'\r' | b'\n' | b':' => i += 1,
            b',' => {
                if let Some(t) = pila.last_mut() {
                    t.2 = t.1;
                }
                i += 1;
            }
            b'}' | b']' => {
                if let Some((k, _, _)) = pila.pop() {
                    out[k].fin = i + 1;
                }
                i += 1;
            }
            b'"' => {
                let f = fin_cadena(p, i);
                if let Some(t) = pila.last_mut().filter(|t| t.1 && t.2) {
                    t.2 = false;
                    clave = Some(i);
                } else {
                    out.push(nuevo(i, f, Tipo::Cadena, &mut clave));
                }
                i = f;
            }
            b'{' | b'[' => {
                let tipo = if c == b'{' {
                    Tipo::Objeto
                } else {
                    Tipo::Arreglo
                };
                out.push(nuevo(i, i + 1, tipo, &mut clave));
                pila.push((out.len() - 1, c == b'{', c == b'{'));
                i += 1;
            }
            _ => {
                let mut f = i;
                while f < p.len() && !b",]} \t\r\n".contains(&p[f]) {
                    f += 1;
                }
                let tipo = if c == b'-' || c.is_ascii_digit() {
                    Tipo::Numero
                } else {
                    Tipo::Literal
                };
                out.push(nuevo(i, f, tipo, &mut clave));
                i = f.max(i + 1);
            }
        }
    }
    out
}

/// Qué cambia una mutación. Las primeras seis son semánticas y las demás
/// sintácticas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mutacion {
    CambiarTipo,
    FueraDeRango,
    CadenaRara,
    BorrarMiembro,
    MiembroExtra,
    Elementos,
    Truncar,
    ComaColgante,
    CeroInicial,
    EscapeInvalido,
    LiteralRoto,
    ClaveDuplicada,
    Utf8Invalido,
}

pub const SEMANTICAS: &[Mutacion] = &[
    Mutacion::CambiarTipo,
    Mutacion::FueraDeRango,
    Mutacion::CadenaRara,
    Mutacion::BorrarMiembro,
    Mutacion::MiembroExtra,
    Mutacion::Elementos,
];

pub const SINTACTICAS: &[Mutacion] = &[
    Mutacion::Truncar,
    Mutacion::ComaColgante,
    Mutacion::CeroInicial,
    Mutacion::EscapeInvalido,
    Mutacion::LiteralRoto,
    Mutacion::ClaveDuplicada,
    Mutacion::Utf8Invalido,
];

impl Mutacion {
    /// Nombre corto para el `_id` y la descripción.
    pub fn nombre(self) -> &'static str {
        match self {
            Mutacion::CambiarTipo => "tipo",
            Mutacion::FueraDeRango => "rango",
            Mutacion::CadenaRara => "cadena",
            Mutacion::BorrarMiembro => "sin-miembro",
            Mutacion::MiembroExtra => "miembro-extra",
            Mutacion::Elementos => "elementos",
            Mutacion::Truncar => "truncado",
            Mutacion::ComaColgante => "coma",
            Mutacion::CeroInicial => "cero-inicial",
            Mutacion::EscapeInvalido => "escape",
            Mutacion::LiteralRoto => "literal",
            Mutacion::ClaveDuplicada => "clave-duplicada",
            Mutacion::Utf8Invalido => "utf8",
        }
    }
}

fn reemplazar(p: &[u8], ini: usize, fin: usize, nuevo: &[u8]) -> Vec<u8> {
    [&p[..ini], nuevo, &p[fin..]].concat()
}

/// Quita el miembro (clave y valor) con su coma.
fn sin_miembro(p: &[u8], k: usize, fin: usize) -> Vec<u8> {
    let mut f = fin;
    while f < p.len() && p[f].is_ascii_whitespace() {
        f += 1;
    }
    if f < p.len() && p[f] == b',' {
        return reemplazar(p, k, f + 1, b"");
    }
    let mut i = k;
    while i > 0 && p[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i > 0 && p[i - 1] == b',' {
        i -= 1;
    }
    reemplazar(p, i, fin, b"")
}

/// Aplica `m` a `p` en un tramo elegido con `azar`; `None` si `p` no tiene
/// dónde aplicarla.
pub fn mutar(p: &[u8], m: Mutacion, azar: &mut Azar) -> Option<Vec<u8>> {
    let ts = tramos(p);
    let de = |f: &dyn Fn(&Tramo) -> bool| -> Vec<Tramo> {
        ts.iter().copied().filter(|t| f(t)).collect()
    };
    let uno = |azar: &mut Azar, v: Vec<Tramo>| -> Option<Tramo> { azar.elegir(&v).copied() };
    let r = match m {
        Mutacion::CambiarTipo => {
            let t = uno(azar, ts.clone())?;
            let otros: &[&[u8]] = match t.tipo {
                Tipo::Objeto => &[b"[]", b"\"x\"", b"7", b"null"],
                Tipo::Arreglo => &[b"{}", b"\"x\"", b"7", b"true"],
                Tipo::Cadena => &[b"7", b"true", b"null", b"[]"],
                Tipo::Numero => &[b"\"7\"", b"true", b"null", b"{}"],
                Tipo::Literal => &[b"\"x\"", b"7", b"[]", b"{}"],
            };
            let n = *azar.elegir(otros)?;
            reemplazar(p, t.ini, t.fin, n)
        }
        Mutacion::FueraDeRango => {
            let t = uno(azar, de(&|t| t.tipo == Tipo::Numero))?;
            let n = *azar.elegir(&[&b"-1000000"[..], b"1000000", b"0.5", b"7.001", b"-1"])?;
            reemplazar(p, t.ini, t.fin, n)
        }
        Mutacion::CadenaRara => {
            let t = uno(azar, de(&|t| t.tipo == Tipo::Cadena))?;
            let largo = format!("\"{}\"", "z".repeat(64));
            let n = *azar.elegir(&[&b"\"\""[..], largo.as_bytes(), b"\"!?\"", b"\"ab12\""])?;
            reemplazar(p, t.ini, t.fin, n)
        }
        Mutacion::BorrarMiembro => {
            let t = uno(azar, de(&|t| t.clave.is_some()))?;
            sin_miembro(p, t.clave?, t.fin)
        }
        Mutacion::MiembroExtra => {
            let t = uno(azar, de(&|t| t.tipo == Tipo::Objeto))?;
            let vacio = p[t.ini + 1..t.fin - 1].iter().all(u8::is_ascii_whitespace);
            let extra: &[u8] = if vacio {
                b"\"zz_extra\":1"
            } else {
                b"\"zz_extra\":1,"
            };
            reemplazar(p, t.ini + 1, t.ini + 1, extra)
        }
        Mutacion::Elementos => {
            let t = uno(azar, de(&|t| t.en_arreglo))?;
            if azar.moneda() {
                // Duplicar el elemento (uniqueItems, maxItems, items: false…).
                let copia = [b",", &p[t.ini..t.fin]].concat();
                reemplazar(p, t.fin, t.fin, &copia)
            } else {
                // Quitarlo (minItems, prefixItems…).
                let mut f = t.fin;
                while f < p.len() && p[f].is_ascii_whitespace() {
                    f += 1;
                }
                if f < p.len() && p[f] == b',' {
                    reemplazar(p, t.ini, f + 1, b"")
                } else {
                    sin_miembro(p, t.ini, t.fin)
                }
            }
        }
        Mutacion::Truncar => {
            if p.len() < 2 {
                return None;
            }
            let n = azar.rango(1, p.len() as u64 - 1) as usize;
            p[..n].to_vec()
        }
        Mutacion::ComaColgante => {
            let t = uno(
                azar,
                de(&|t| matches!(t.tipo, Tipo::Objeto | Tipo::Arreglo) && t.fin - t.ini > 2),
            )?;
            reemplazar(p, t.fin - 1, t.fin - 1, b",")
        }
        Mutacion::CeroInicial => {
            let t = uno(azar, de(&|t| t.tipo == Tipo::Numero))?;
            reemplazar(p, t.ini, t.fin, b"012")
        }
        Mutacion::EscapeInvalido => {
            let t = uno(azar, de(&|t| t.tipo == Tipo::Cadena))?;
            reemplazar(p, t.ini + 1, t.ini + 1, b"\\q")
        }
        Mutacion::LiteralRoto => {
            let t = uno(
                azar,
                de(&|t| t.tipo != Tipo::Objeto && t.tipo != Tipo::Arreglo),
            )?;
            let n = *azar.elegir(&[&b"ture"[..], b"nul", b"fals", b"+1", b"1."])?;
            reemplazar(p, t.ini, t.fin, n)
        }
        Mutacion::ClaveDuplicada => {
            let t = uno(azar, de(&|t| t.clave.is_some()))?;
            let copia = [b",", &p[t.clave?..t.fin]].concat();
            reemplazar(p, t.fin, t.fin, &copia)
        }
        Mutacion::Utf8Invalido => {
            let t = uno(azar, de(&|t| t.tipo == Tipo::Cadena))?;
            reemplazar(p, t.ini + 1, t.ini + 1, &[0xFF])
        }
    };
    (r != p).then_some(r)
}

/// Lo que esperan el DPDA y la línea base de un payload mutado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Esperado {
    /// Bien formado con `violaciones` > 0 (`errorKind: schema`).
    Invalido {
        violaciones: u64,
        offset: u64,
        palabras: Vec<String>,
    },
    /// Malformado: `lexical` o `syntax` en `offset`.
    Malformado { lexico: bool, offset: u64 },
}

/// El veredicto común del DPDA y la línea base; `None` si el payload es
/// válido, pasa un límite o los dos validadores no coinciden.
///
/// También exige lo que pide `crates/automaton/tests/corpus.rs` a todo caso:
/// el mismo reporte en bloques de 1 y 7 bytes, y en fallo temprano la misma
/// clase de error (con `schema`, peso 1 en la misma posición). Así se
/// descartan los payloads en los que el fallo temprano se detiene en una
/// violación antes de ver un error léxico posterior (`"1."` contra
/// `{"type": "string"}`; `docs/fase6/pendientes.md` §28).
pub fn clasificar(m: &Automaton, lb: &baseline::JsonBaseline, p: &[u8]) -> Option<Esperado> {
    let r = automaton::validate(m, p, Mode::Total, 4096);
    if [1, 7]
        .iter()
        .any(|&b| automaton::validate(m, p, Mode::Total, b) != r)
    {
        return None;
    }
    let e = automaton::validate(m, p, Mode::EarlyFail, 7);
    let temprano_igual = match r.error_kind {
        Some(ErrorKind::Schema) => {
            e.error_kind == r.error_kind
                && e.violations == runtime_rs::Weight::ONE
                && e.first == r.first
        }
        _ => e == r,
    };
    if !temprano_igual {
        return None;
    }
    let o = baseline::with_stack(|st| lb.validate(st, p));
    if r.valid || o.valid || o.truncated {
        return None;
    }
    let first = r.first?;
    match (r.error_kind?, o.kind?) {
        (ErrorKind::Schema, baseline::Kind::Schema) => {
            let n = r.violations.get()?;
            (n == o.violation_count() as u64).then(|| Esperado::Invalido {
                violaciones: n,
                offset: first.offset,
                palabras: o.violations.iter().map(|v| v.keyword.clone()).collect(),
            })
        }
        (k @ (ErrorKind::Lexical | ErrorKind::Syntax), baseline::Kind::Malformed) => {
            (o.malformed.as_ref()?.offset? == first.offset).then_some(Esperado::Malformado {
                lexico: k == ErrorKind::Lexical,
                offset: first.offset,
            })
        }
        _ => None,
    }
}

/// Base64 estándar con relleno (RFC 4648), para `payloadBase64`.
pub fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for (k, s) in [18, 12, 6, 0].into_iter().enumerate() {
            o.push(if k <= c.len() {
                A[(n >> s) as usize & 63] as char
            } else {
                '='
            });
        }
    }
    o
}
