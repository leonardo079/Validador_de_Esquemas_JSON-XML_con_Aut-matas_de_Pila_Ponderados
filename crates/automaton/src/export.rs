//! Salidas de `schemac automaton` (F4-U7): el resumen de tamaños en JSON y
//! M completo en DOT (un clúster por grupo de estados, F4-B10). Las dos son
//! deterministas: mismo esquema, mismos bytes.

use std::fmt::Write;

use runtime_rs::tables::{Op, Test};

use crate::model::{Automaton, ClassId, StateId, TransId, ERROR};

/// Versión del resumen JSON.
pub const SUMMARY_VERSION: u32 = 1;

impl Automaton {
    /// Resumen de tamaños en JSON (una línea por campo).
    pub fn write_summary_json(&self, w: &mut impl Write) -> std::fmt::Result {
        let s = self.summary();
        writeln!(w, "{{")?;
        writeln!(w, "  \"automatonVersion\": {SUMMARY_VERSION},")?;
        writeln!(w, "  \"format\": \"json\",")?;
        writeln!(w, "  \"states\": {},", s.states)?;
        writeln!(w, "  \"transitions\": {},", s.transitions)?;
        writeln!(w, "  \"frames\": {},", s.frames)?;
        writeln!(w, "  \"classes\": {},", s.classes)?;
        writeln!(w, "  \"registerBytes\": {},", s.register_bytes)?;
        writeln!(w, "  \"maxFrameBytes\": {},", s.max_frame_bytes)?;
        writeln!(w, "  \"products\": {},", s.products)?;
        writeln!(w, "  \"deferred\": {}", s.deferred)?;
        writeln!(w, "}}")
    }

    /// M en DOT: un clúster por grupo; las aristas de un estado a otro con
    /// la misma operación se juntan en una, con sus clases en la etiqueta.
    /// Las entradas de error no se dibujan. `Pop` y `Leave` van a nodos
    /// «↩» (el destino lo da la pila o el registro del escalar).
    pub fn write_dot(&self, w: &mut impl Write) -> std::fmt::Result {
        writeln!(w, "digraph automata {{")?;
        writeln!(w, "  rankdir=LR;")?;
        writeln!(w, "  node [shape=circle, fontsize=10];")?;
        writeln!(w, "  edge [fontsize=9];")?;
        writeln!(w, "  inicio [shape=point];")?;
        writeln!(w, "  desapila [shape=plaintext, label=\"↩ marco\"];")?;
        writeln!(w, "  escalar [shape=plaintext, label=\"↩ escalar\"];")?;
        for (g, name) in self.groups().iter().enumerate() {
            writeln!(w, "  subgraph cluster_{g} {{")?;
            writeln!(w, "    label=\"{}\";", esc(name))?;
            for q in 0..self.n_states() as u32 {
                let st = self.state(StateId(q));
                if st.group as usize != g {
                    continue;
                }
                let shape = if q == self.accept {
                    ", shape=doublecircle"
                } else {
                    ""
                };
                writeln!(w, "    q{q} [label=\"q{q}\\n{}\"{shape}];", esc(&st.name))?;
            }
            writeln!(w, "  }}")?;
        }
        writeln!(w, "  inicio -> q{};", self.start)?;
        for q in 0..self.n_states() as u32 {
            // (destino, sufijo) → clases, en orden de primera aparición.
            let mut edges: Vec<(String, String, Vec<String>)> = Vec::new();
            for a in 0..self.n_classes() as u16 {
                let t = self.delta(StateId(q), ClassId(a));
                if t == ERROR {
                    continue;
                }
                let (to, suffix) = self.edge(t);
                let class = self.class_name(ClassId(a)).to_owned();
                match edges.iter_mut().find(|e| e.0 == to && e.1 == suffix) {
                    Some(e) => e.2.push(class),
                    None => edges.push((to, suffix, vec![class])),
                }
            }
            for (to, suffix, classes) in edges {
                let label = format!("{}{suffix}", classes.join(" "));
                writeln!(w, "  q{q} -> {to} [label=\"{}\"];", esc(&label))?;
            }
        }
        writeln!(w, "}}")
    }

    /// Destino en el DOT y sufijo de la etiqueta de la transición `t`.
    fn edge(&self, t: TransId) -> (String, String) {
        let tr = self.transition(t);
        let mut suffix = match tr.op {
            Op::Move => String::new(),
            Op::Push => format!(
                " / apila {} ↩q{}",
                self.frame_names[tr.frame as usize], tr.ret
            ),
            Op::Pop => " / desapila".to_owned(),
            Op::Enter => format!(" / abre ↩q{}", tr.ret),
            Op::Leave => " / cierra".to_owned(),
            Op::Fail(_) => " / error".to_owned(),
        };
        if tr.bit != runtime_rs::tables::NO_BIT {
            let _ = write!(suffix, " / bit {}", tr.bit);
        }
        if tr.count {
            suffix.push_str(" / +1");
        }
        let checks: Vec<String> = self
            .checks_of(t)
            .iter()
            .map(|c| {
                let (_, kind) = self.rule(c.rule);
                let test = match c.test {
                    Test::Fails => "✗".to_owned(),
                    Test::Integer => "entero".to_owned(),
                    Test::Mask => "máscara".to_owned(),
                    Test::Dependent => "dependencias".to_owned(),
                    Test::CountMin => format!("n ≥ {}", c.bound()),
                    Test::CountMax => format!("n ≤ {}", c.bound()),
                    Test::LengthMin => format!("|s| ≥ {}", c.bound()),
                    Test::LengthMax => format!("|s| ≤ {}", c.bound()),
                    Test::NumMin => format!("≥ {}", self.num_text(c.arg)),
                    Test::NumExMin => format!("> {}", self.num_text(c.arg)),
                    Test::NumMax => format!("≤ {}", self.num_text(c.arg)),
                    Test::NumExMax => format!("< {}", self.num_text(c.arg)),
                    Test::NumEnum => format!("∈ {} valores", c.len),
                    Test::MultipleOf => match self.divisors.get(c.arg as usize) {
                        Some(d) => format!("múltiplo de {}", self.num_text(d.lexeme)),
                        None => "múltiplo".to_owned(),
                    },
                };
                format!("{} {test} c={}", kind.key(), c.cost)
            })
            .collect();
        if !checks.is_empty() {
            let _ = write!(suffix, " [{}]", checks.join("; "));
        }
        let to = match tr.op {
            Op::Pop => "desapila".to_owned(),
            Op::Leave => "escalar".to_owned(),
            _ => format!("q{}", tr.next),
        };
        (to, suffix)
    }
}

impl Automaton {
    /// Un lexema del esquema para el DOT, recortado a 24 bytes.
    fn num_text(&self, i: u32) -> String {
        let l = self.tables().num(i).unwrap_or(&[]);
        let s = String::from_utf8_lossy(l);
        if s.len() > 24 {
            format!("{}…", &s[..24])
        } else {
            s.into_owned()
        }
    }
}

/// Escapa una etiqueta DOT entre comillas: `"` y `\`, y los controles como
/// `\u{…}`.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
