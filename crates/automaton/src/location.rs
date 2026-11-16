//! Ubicación recortada de un nodo para los mensajes del validador y el
//! codegen (F4-B8; cierra `docs/fase3/pendientes.md` §4).

use ir::{NodeId, Step, VerifiedIr};

/// Tope de [`location_clipped`] en las tablas y en los mensajes (F4-U6).
pub const LOCATION_MAX: usize = 256;

/// Lo que se pone en el hueco de una ruta recortada.
const GAP: &str = "…";

/// [`VerifiedIr::location`] recortada a como mucho `max` bytes (mínimo
/// 16): si la ruta entera no cabe, su principio, `…` y su final. No la
/// construye entera: sube por los padres (O(profundidad)) y escribe solo los
/// caracteres que caben, así que cuesta O(profundidad + max) aunque las
/// claves midan megabytes. Nunca parte un carácter ni un escape de la RFC
/// 6901 (`~0`, `~1`). Un `id` fuera de rango da `"#"`, como `location`.
pub fn location_clipped(ir: &VerifiedIr, id: NodeId, max: usize) -> String {
    let max = max.max(16);
    let nodes = ir.nodes();
    let mut steps: Vec<&Step> = Vec::new();
    let mut cur = id.index();
    while let Some(node) = nodes.get(cur) {
        steps.push(&node.origin.step);
        match node.origin.parent {
            Some(p) if p.index() < cur => cur = p.index(),
            _ => break,
        }
    }
    steps.reverse();
    let mut segs: Vec<Seg<'_>> = Vec::with_capacity(2 * steps.len());
    for step in steps {
        match step {
            Step::Root => {}
            Step::Keyword(k) => segs.push(Seg::Str(k)),
            Step::Entry(k, e) => segs.extend([Seg::Str(k), Seg::Str(e)]),
            Step::Index(k, i) => segs.extend([Seg::Str(k), Seg::Num(*i)]),
        }
    }
    let head = (max - GAP.len()) / 2;
    let tail = max - GAP.len() - head;
    let mut out = String::from("#");
    let mut cut = out.len();
    let mut cabe = true;
    'head: for seg in &segs {
        for chunk in seg.chunks() {
            if out.len() + chunk.len() > max {
                cabe = false;
                break 'head;
            }
            out.push_str(chunk.as_str());
            if out.len() <= head {
                cut = out.len();
            }
        }
    }
    if cabe {
        return out;
    }
    out.truncate(cut);
    out.push_str(GAP);
    // El final, de atrás hacia delante, trozo a trozo.
    let mut rev: Vec<Chunk> = Vec::new();
    let mut len = 0;
    'tail: for seg in segs.iter().rev() {
        for chunk in seg.rchunks() {
            if len + chunk.len() > tail {
                break 'tail;
            }
            len += chunk.len();
            rev.push(chunk);
        }
    }
    for chunk in rev.iter().rev() {
        out.push_str(chunk.as_str());
    }
    out
}

/// Segmento de la ruta, sin codificar.
#[derive(Clone, Copy)]
enum Seg<'a> {
    Str(&'a str),
    Num(usize),
}

/// Un trozo indivisible de la ruta escrita: `/`, un carácter o su escape.
#[derive(Clone, Copy)]
struct Chunk {
    buf: [u8; 4],
    len: u8,
}

impl Chunk {
    fn of(s: &str) -> Chunk {
        let mut buf = [0; 4];
        buf[..s.len()].copy_from_slice(s.as_bytes());
        Chunk {
            buf,
            len: s.len() as u8,
        }
    }

    fn len(&self) -> usize {
        usize::from(self.len)
    }

    fn as_str(&self) -> &str {
        // Siempre se construye desde un `&str` entero.
        std::str::from_utf8(&self.buf[..self.len()]).unwrap_or("")
    }
}

impl Seg<'_> {
    /// `/` y los caracteres del segmento con los escapes de la RFC 6901, en
    /// orden. Perezoso: un segmento largo no se recorre más allá de lo que
    /// se pide (un índice tiene pocas cifras).
    fn chunks(&self) -> impl Iterator<Item = Chunk> + '_ {
        let digits: Vec<Chunk> = match *self {
            Seg::Num(i) => i.to_string().chars().map(escape).collect(),
            Seg::Str(_) => Vec::new(),
        };
        let chars = match *self {
            Seg::Str(s) => Some(s.chars().map(escape)),
            Seg::Num(_) => None,
        };
        std::iter::once(Chunk::of("/"))
            .chain(chars.into_iter().flatten())
            .chain(digits)
    }

    /// Los mismos trozos que [`Seg::chunks`], del último al primero.
    fn rchunks(&self) -> impl Iterator<Item = Chunk> + '_ {
        let chars: Vec<Chunk> = match *self {
            Seg::Num(i) => i.to_string().chars().rev().map(escape).collect(),
            Seg::Str(_) => Vec::new(),
        };
        let borrowed = match *self {
            Seg::Str(s) => Some(s.chars().rev().map(escape)),
            Seg::Num(_) => None,
        };
        borrowed
            .into_iter()
            .flatten()
            .chain(chars)
            .chain(std::iter::once(Chunk::of("/")))
    }
}

/// Un carácter con los escapes de la RFC 6901 (`~` → `~0`, `/` → `~1`).
fn escape(c: char) -> Chunk {
    match c {
        '~' => Chunk::of("~0"),
        '/' => Chunk::of("~1"),
        c => {
            let mut b = [0; 4];
            Chunk::of(c.encode_utf8(&mut b))
        }
    }
}
