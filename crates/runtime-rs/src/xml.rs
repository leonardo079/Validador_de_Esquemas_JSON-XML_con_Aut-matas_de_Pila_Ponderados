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
}

impl<'t, 'm> XmlValidator<'t, 'm> {
    pub fn new(t: Tables<'t>, opts: &Options, mem: &'m mut [u64]) -> XmlValidator<'t, 'm> {
        XmlValidator {
            lexer: XmlLexer::new(),
            core: Core::new(t, opts, mem),
        }
    }

    /// Consume un bloque de la entrada (de cualquier tamaño).
    pub fn feed(&mut self, chunk: &[u8]) -> Flow {
        if self.core.is_stopped() {
            return Flow::Stop;
        }
        match self.lexer.feed(chunk, &mut self.core) {
            Ok(()) => Flow::Continue,
            Err(e) => {
                self.record(e);
                Flow::Stop
            }
        }
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
