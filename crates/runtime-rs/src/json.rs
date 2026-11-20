//! El lexer JSON (`no_std`, Fase 1) delante del núcleo: la composición del
//! transductor de eventos con M en una sola pasada (`docs/fase4/automata.md`
//! §2). Cada byte entra una vez al lexer y cada evento una vez a δ; el
//! resultado no depende de cómo se parta la entrada en bloques.

use lexer::json::JsonLexer;
use lexer::{Error, LexErrorKind};

use crate::core::{Core, Options, Report};
use crate::tables::Tables;

/// Si conviene seguir alimentando la entrada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    /// Error de peso ∞ o fallo temprano: el resto de la entrada no cambia
    /// el resultado.
    Stop,
}

/// Validador JSON en streaming: lexer + núcleo, sin reservas.
#[derive(Debug)]
pub struct JsonValidator<'t, 'm> {
    lexer: JsonLexer,
    core: Core<'t, 'm>,
    /// Bytes recibidos y su tope ([`crate::Limits::max_payload`]).
    fed: u64,
    max_payload: u64,
}

impl<'t, 'm> JsonValidator<'t, 'm> {
    pub fn new(t: Tables<'t>, opts: &Options, mem: &'m mut [u64]) -> JsonValidator<'t, 'm> {
        JsonValidator {
            lexer: JsonLexer::new(),
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

    /// Fin de la entrada: cierra el número pendiente y da el reporte.
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
        // `Sink`: el núcleo ya guardó su error; `Aborted` es el lexer
        // detenido después de él.
        if let Error::Lex(le) = e {
            if le.kind != LexErrorKind::Aborted {
                self.core.lex_error(le);
            }
        }
    }
}
