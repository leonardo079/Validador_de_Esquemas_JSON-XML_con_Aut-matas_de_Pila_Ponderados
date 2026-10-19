//! Exportación de AFD de tabla a DOT (Graphviz).
//!
//! Los diagramas de la documentación se generan desde las mismas tablas que
//! ejecuta el lexer, así que no pueden desincronizarse del código.

use core::fmt::{self, Write};

/// Vista de un AFD de tabla sobre clases de bytes.
pub struct Dfa<'a> {
    pub name: &'a str,
    pub title: &'a str,
    pub num_states: usize,
    pub num_classes: usize,
    /// δ(estado, clase).
    pub next: &'a dyn Fn(usize, usize) -> usize,
    pub state_names: &'a [&'a str],
    pub class_names: &'a [&'a str],
    pub start: usize,
    pub accepting: &'a dyn Fn(usize) -> bool,
    /// Estado sumidero de error: no se dibuja; sus transiciones se omiten.
    pub error: usize,
    /// Transiciones que no están en la tabla porque dependen de un registro
    /// (`desde`, `hacia`, etiqueta con la guarda). Se dibujan punteadas.
    pub guarded: &'a [(usize, usize, &'a str)],
}

impl fmt::Debug for Dfa<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dfa")
            .field("name", &self.name)
            .field("num_states", &self.num_states)
            .field("num_classes", &self.num_classes)
            .finish_non_exhaustive()
    }
}

/// Escribe el AFD. Las transiciones entre el mismo par de estados se agrupan
/// en una sola arista; si una arista abarca más de la mitad del alfabeto se
/// etiqueta por complemento (`Σ − {…}`).
pub fn write(w: &mut impl Write, dfa: &Dfa<'_>) -> fmt::Result {
    writeln!(w, "digraph {} {{", dfa.name)?;
    writeln!(w, "  rankdir=LR;")?;
    write!(w, "  label=\"")?;
    escape(w, dfa.title)?;
    writeln!(
        w,
        "\\n(las transiciones omitidas van al estado de error)\";"
    )?;
    writeln!(w, "  labelloc=t;")?;
    writeln!(w, "  node [shape=circle, fontname=\"Helvetica\"];")?;
    writeln!(w, "  edge [fontname=\"Helvetica\", fontsize=10];")?;
    writeln!(w, "  __start [shape=point, label=\"\"];")?;

    for s in (0..dfa.num_states).filter(|&s| s != dfa.error) {
        let shape = if (dfa.accepting)(s) {
            "doublecircle"
        } else {
            "circle"
        };
        write!(w, "  s{s} [shape={shape}, label=\"")?;
        escape(w, dfa.state_names[s])?;
        writeln!(w, "\"];")?;
    }
    writeln!(w, "  __start -> s{};", dfa.start)?;

    for from in (0..dfa.num_states).filter(|&s| s != dfa.error) {
        for to in (0..dfa.num_states).filter(|&s| s != dfa.error) {
            let goes = |c: usize| (dfa.next)(from, c) == to;
            let count = (0..dfa.num_classes).filter(|&c| goes(c)).count();
            if count == 0 {
                continue;
            }
            write!(w, "  s{from} -> s{to} [label=\"")?;
            if count * 2 > dfa.num_classes {
                write!(w, "Σ − {{")?;
                list(w, dfa, |c| !goes(c))?;
                write!(w, "}}")?;
            } else {
                list(w, dfa, goes)?;
            }
            writeln!(w, "\"];")?;
        }
    }
    for &(from, to, label) in dfa.guarded {
        write!(w, "  s{from} -> s{to} [style=dashed, label=\"")?;
        escape(w, label)?;
        writeln!(w, "\"];")?;
    }
    writeln!(w, "}}")
}

fn list(w: &mut impl Write, dfa: &Dfa<'_>, keep: impl Fn(usize) -> bool) -> fmt::Result {
    let mut first = true;
    for c in (0..dfa.num_classes).filter(|&c| keep(c)) {
        if !first {
            write!(w, ", ")?;
        }
        first = false;
        escape(w, dfa.class_names[c])?;
    }
    Ok(())
}

fn escape(w: &mut impl Write, s: &str) -> fmt::Result {
    for ch in s.chars() {
        match ch {
            '"' | '\\' => {
                w.write_char('\\')?;
                w.write_char(ch)?;
            }
            _ => w.write_char(ch)?,
        }
    }
    Ok(())
}
