//! El lexer XML (`no_std`, Fase 1) delante del núcleo (F4.1c): la misma
//! composición en una pasada que en JSON (`docs/fase4/automata.md` §2). El
//! lexer ya rechaza `<!DOCTYPE` (D8), las entidades no predefinidas y el
//! XML mal formado a nivel léxico; el emparejamiento de etiquetas, el
//! documento y la validación son de M.

use lexer::xml::XmlLexer;
use lexer::{Error, LexErrorKind};

use crate::core::{Core, Options, Report};
use crate::json::Flow;
use crate::tables::Tables;

/// Validador XML en streaming: lexer + núcleo, sin reservas.
#[derive(Debug)]
pub struct XmlValidator<'t, 'm> {
    lexer: XmlLexer,
    core: Core<'t, 'm>,
    /// Bytes recibidos y su tope ([`crate::Limits::max_payload`]).
    fed: u64,
    max_payload: u64,
}

impl<'t, 'm> XmlValidator<'t, 'm> {
    pub fn new(t: Tables<'t>, opts: &Options, mem: &'m mut [u64]) -> XmlValidator<'t, 'm> {
        XmlValidator {
            lexer: XmlLexer::new(),
            core: Core::new(t, opts, mem),
            fed: 0,
            max_payload: opts.limits.max_payload,
        }
    }

    /// Consume un bloque de la entrada (de cualquier tamaño). Lo que pase
    /// de [`crate::Limits::max_payload`] no se lee: `limit` en el primer
    /// byte de más.
    pub fn feed(&mut self, chunk: &[u8]) -> Flow {
        if self.core.is_stopped() {
            return Flow::Stop;
        }
        let room = self.max_payload.saturating_sub(self.fed);
        let take = usize::try_from(room).map_or(chunk.len(), |r| r.min(chunk.len()));
        let (cabe, sobra) = chunk.split_at(take);
        self.fed = self.fed.saturating_add(take as u64);
        if let Err(e) = self.lexer.feed(cabe, &mut self.core) {
            self.record(e);
            return Flow::Stop;
        }
        if !sobra.is_empty() {
            self.core.limit(self.lexer.position());
            return Flow::Stop;
        }
        Flow::Continue
    }

    /// Detiene la validación con un error `limit` en la posición actual,
    /// si no se había detenido ya (F6.2): el tiempo máximo, u otro tope que
    /// vigile el llamador (`docs/fase6/limites.md`).
    pub fn abort_limit(&mut self) {
        self.core.limit(self.lexer.position());
    }

    /// Fin de la entrada: el reporte.
    pub fn finish(mut self) -> Report {
        if !self.core.is_stopped() {
            if let Err(e) = self.lexer.finish(&mut self.core) {
                self.record(e);
            }
        }
        let end = self.lexer.position();
        self.core.finish(end)
    }

    fn record<E>(&mut self, e: Error<E>) {
        if let Error::Lex(le) = e {
            if le.kind != LexErrorKind::Aborted {
                self.core.lex_error(le);
            }
        }
    }
}
