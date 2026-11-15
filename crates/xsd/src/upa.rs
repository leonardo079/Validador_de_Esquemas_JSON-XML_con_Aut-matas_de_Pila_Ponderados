//! *Unique Particle Attribution* (UPA) y tope de partículas por modelo
//! (tarea 5.7, `docs/fase3/xsd.md` §11; D25, D26). Etapa 3 del análisis
//! (modelos de contenido), junto con I11, *all-group limited* y EDC.
//!
//! **Abstracción (D25, conservadora).** Cada partícula se lee como una
//! expresión regular sobre los nombres **locales** de los elementos (los
//! mismos que compara EDC y que usa la IR, D40): `maxOccurs` > 1 o
//! `unbounded` es `+` (o `*` si es opcional), `minOccurs="0"` es `?`,
//! `maxOccurs="0"` también es `?` (la partícula sigue ahí) y un `xs:choice`
//! sin hijos es ε. Las tres cosas solo **agrandan** las relaciones *first* y
//! *follow* del Glushkov exacto: si la abstracción es determinista, también
//! lo es el autómata de **posiciones** del modelo exacto (lo que pide XSD
//! 1.0), y lo que se rechaza de más son modelos que XSD 1.0 admite
//! (`a{2,2}, a`; `a{0,0}, a`). `xs:all` es la raíz del modelo (I12): tras
//! cualquiera de sus hijos puede venir cualquier otro.
//!
//! **Vueltas con contador (enmienda de D25, I-1 de la revisión).** Con un
//! contador por partícula en el marco (D16), la Fase 4 necesita además
//! saber **qué contadores** cambia cada transición (determinismo *fuerte*,
//! no solo *débil*; Kilpeläinen y Tuhkanen; Gelade, Gyssens y Martens). Al
//! enumerar *follow*(*x*) cada nodo guarda el nivel de la subida que lo
//! añadió; si vuelve a entrar desde un nivel más alto (la vuelta de un
//! antecesor) y entre los dos niveles, ambos incluidos, hay una partícula
//! con contador (`minOccurs` ≥ 2 o `maxOccurs` acotado > 1), los dos caminos
//! lo actualizan distinto: error en el nodo que vuelve a entrar, con la
//! partícula que se repite y la del contador (la más alta de la subida). Se
//! rechaza `(a{1,2}){2,2}`, que XSD 1.0 admite. Sin contador por medio
//! (`(a*)*`) los dos caminos son la misma transición del Glushkov.
//!
//! **Condición.** El modelo es determinista si ni *first*(raíz) ni
//! *follow*(*x*) de ninguna posición *x* (partícula de elemento) tienen dos
//! posiciones distintas con el mismo nombre. *follow*(*x*) se enumera
//! subiendo desde *x* mientras *x* sea una de las últimas posiciones del
//! subárbol: en cada nodo que se repite, *first* de ese nodo; en una
//! secuencia, *first* de los hermanos siguientes hasta el primero no
//! anulable (si lo hay, se para); en un `xs:all`, *first* de los demás
//! hijos; en un `xs:choice`, nada.
//!
//! **Costo.** El tope de D26 se comprueba **antes** de calcular nada: un
//! modelo de más de [`MAX_PARTICLES`] partículas (elementos y grupos) da
//! `limit` en el `<` de la partícula 4 097 y no se mira más. Con *n* ≤ 4 096
//! partículas y *m* < *n* posiciones, cada uno de los *m* + 1 conjuntos se
//! enumera con una marca por nodo (un nodo cuyo *first* ya entró no se
//! vuelve a recorrer; volver a entrar se comprueba en O(1) con el nivel
//! guardado y, por nivel, el más alto con contador), así que cuesta O(*n*):
//! a lo sumo 2*n* nodos sacados de la pila más *n* pasos de subida. En total **O(*n*·*m*) ≤
//! 3·*n*·(*m* + 1) pasos por modelo** y memoria O(*n*), sin recursión (D7):
//! pilas explícitas y vectores indexados por partícula. Sumado sobre el
//! esquema, ≤ 3·4 096·N con N las partículas de todos los modelos.

use std::collections::HashMap;

use ir::Occurs;
use lexer::Pos;

use crate::ast::Compositor;
use crate::check::{Part, Term};
use crate::error::{clip, ErrorKind};

/// Partículas (elementos y grupos, la raíz incluida) por modelo de
/// contenido (D26, `00-alcance.md` §4).
pub(crate) const MAX_PARTICLES: usize = 4096;

/// Comprueba el tope de D26 y, si cabe, la UPA del modelo aplanado `parts`
/// (la arena en preorden del análisis). Cada error va a `report` con su
/// posición; cada partícula se informa a lo sumo una vez, como la segunda
/// (en orden del documento) de su pareja.
pub(crate) fn check(parts: &[Part<'_>], report: &mut impl FnMut(ErrorKind, Pos)) {
    if parts.len() > MAX_PARTICLES {
        report(
            ErrorKind::TooManyParticles { max: MAX_PARTICLES },
            parts[MAX_PARTICLES].span.start,
        );
        return;
    }
    if parts.is_empty() {
        return;
    }
    Upa::new(parts).run(report);
}

/// Sin efecto fuera de las pruebas: cuenta los pasos de la UPA (nodos
/// sacados de la pila y pasos de subida) para comprobar sin reloj (D1) su
/// cota y que el tope de D26 va antes.
#[inline(always)]
fn step() {
    #[cfg(test)]
    PASOS.with(|p| p.set(p.get() + 1));
}

#[cfg(test)]
std::thread_local! {
    static PASOS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// Sin hijo: la raíz del modelo.
const NONE: usize = usize::MAX;

struct Upa<'p, 'a> {
    parts: &'p [Part<'a>],
    /// Grupo que contiene cada partícula y su posición entre los hijos.
    parent: Vec<usize>,
    slot: Vec<usize>,
    nullable: Vec<bool>,
    /// Si *first* de la partícula tiene alguna posición.
    has_first: Vec<bool>,
    /// Nombre de cada posición, como índice en `seen` (`NONE` en los grupos).
    name: Vec<usize>,
    /// Por nombre: la marca del conjunto en curso y la primera posición con
    /// ese nombre en él.
    seen: Vec<(u32, usize)>,
    /// Por partícula: la marca del conjunto en el que ya entró su *first*.
    visited: Vec<u32>,
    /// Marca del conjunto en curso (0: ninguno).
    mark: u32,
    /// Por partícula: el nivel de la subida de *follow* en el que entró su
    /// *first* en el conjunto en curso (0 en *first* de la raíz).
    level: Vec<u32>,
    /// Nivel de la subida en curso.
    now: u32,
    /// Subida en curso, por nivel: la partícula de ese nivel y el nivel más
    /// alto, hasta él, con una partícula con contador (`NONE` si no hay).
    path: Vec<(usize, usize)>,
    reported: Vec<bool>,
    /// Partículas ya informadas por una vuelta ambigua con contador.
    reported_counter: Vec<bool>,
    stack: Vec<usize>,
}

/// `?` (o `*`) en la abstracción: `minOccurs="0"` o `maxOccurs="0"`.
fn optional(o: Occurs) -> bool {
    o.min == 0 || o.max == Some(0)
}

/// `+` o `*` en la abstracción: `maxOccurs` > 1 o `unbounded` (D25).
fn repeats(o: Occurs) -> bool {
    o.max.is_none_or(|m| m > 1)
}

/// La partícula necesita contador en la Fase 4 (D16, `ir.md` §7, fila Γ):
/// `minOccurs` ≥ 2 o `maxOccurs` acotado > 1. Es la regla de
/// `ir::classify` (`needs_registers`).
fn counted(o: Occurs) -> bool {
    o.min >= 2 || matches!(o.max, Some(m) if m > 1)
}

impl<'p, 'a> Upa<'p, 'a> {
    fn new(parts: &'p [Part<'a>]) -> Self {
        let n = parts.len();
        let mut parent = vec![NONE; n];
        let mut slot = vec![0; n];
        let mut names: HashMap<&str, usize> = HashMap::new();
        let mut name = vec![NONE; n];
        for (i, p) in parts.iter().enumerate() {
            match &p.term {
                Term::Group { children, .. } => {
                    for (k, &c) in children.iter().enumerate() {
                        parent[c] = i;
                        slot[c] = k;
                    }
                }
                Term::Element { name: local, .. } => {
                    let next = names.len();
                    name[i] = *names.entry(local).or_insert(next);
                }
            }
        }
        // Los hijos van después de su grupo (preorden): de atrás hacia
        // delante cada grupo encuentra calculados a sus hijos.
        let mut nullable = vec![false; n];
        for i in (0..n).rev() {
            let p = &parts[i];
            nullable[i] = optional(p.occurs)
                || match &p.term {
                    Term::Element { .. } => false,
                    Term::Group { group, children } => match group.compositor {
                        Compositor::Sequence | Compositor::All => {
                            children.iter().all(|&c| nullable[c])
                        }
                        // Un `xs:choice` vacío como ε (ver el módulo).
                        Compositor::Choice => {
                            children.is_empty() || children.iter().any(|&c| nullable[c])
                        }
                    },
                };
        }
        // Si *first* tiene alguna posición: un grupo sin ninguna (`<xs:sequence/>`)
        // puede volver a entrar sin que cambie nada.
        let mut has_first = vec![false; n];
        for i in (0..n).rev() {
            has_first[i] = match &parts[i].term {
                Term::Element { .. } => true,
                Term::Group { group, children } => match group.compositor {
                    Compositor::Sequence => {
                        let k = children
                            .iter()
                            .position(|&c| !nullable[c])
                            .map_or(children.len(), |k| k + 1);
                        children[..k].iter().any(|&c| has_first[c])
                    }
                    Compositor::Choice | Compositor::All => children.iter().any(|&c| has_first[c]),
                },
            };
        }
        Upa {
            parts,
            parent,
            slot,
            nullable,
            has_first,
            seen: vec![(0, NONE); names.len()],
            name,
            visited: vec![0; n],
            mark: 0,
            level: vec![0; n],
            now: 0,
            path: Vec::new(),
            reported: vec![false; n],
            reported_counter: vec![false; n],
            stack: Vec::new(),
        }
    }

    fn children(&self, i: usize) -> &'p [usize] {
        match &self.parts[i].term {
            Term::Group { children, .. } => children,
            Term::Element { .. } => &[],
        }
    }

    fn compositor(&self, i: usize) -> Option<Compositor> {
        match &self.parts[i].term {
            Term::Group { group, .. } => Some(group.compositor),
            Term::Element { .. } => None,
        }
    }

    /// Empieza un conjunto nuevo (*first* de la raíz o *follow* de una
    /// posición).
    fn begin(&mut self) {
        self.mark += 1;
    }

    /// Añade *first*(`v`) al conjunto en curso.
    fn add_first(&mut self, v: usize, report: &mut impl FnMut(ErrorKind, Pos)) {
        self.stack.push(v);
        while let Some(u) = self.stack.pop() {
            step();
            if self.visited[u] == self.mark {
                self.reentry(u, report);
                continue;
            }
            self.visited[u] = self.mark;
            self.level[u] = self.now;
            // Los hijos se apilan del revés: salen en orden del documento.
            let children = self.children(u);
            match self.compositor(u) {
                None => self.offer(u, report),
                Some(Compositor::Sequence) => {
                    // Hasta el primer hijo no anulable, incluido.
                    let k = children
                        .iter()
                        .position(|&c| !self.nullable[c])
                        .map_or(children.len(), |k| k + 1);
                    self.stack.extend(children[..k].iter().rev());
                }
                Some(Compositor::Choice | Compositor::All) => {
                    self.stack.extend(children.iter().rev());
                }
            }
        }
    }

    /// La posición `y` entra en el conjunto: si ya hay otra con su nombre,
    /// es un conflicto.
    fn offer(&mut self, y: usize, report: &mut impl FnMut(ErrorKind, Pos)) {
        let k = self.name[y];
        let (mark, x) = self.seen[k];
        if mark != self.mark {
            self.seen[k] = (self.mark, y);
            return;
        }
        if x == y {
            return;
        }
        // `seen` guarda la primera en orden del documento: la pareja de cada
        // conflicto es la más temprana que se conoce.
        let (first, second) = (x.min(y), x.max(y));
        self.seen[k].1 = first;
        if !self.reported[second] {
            self.reported[second] = true;
            let name = match &self.parts[second].term {
                Term::Element { name, .. } => clip(name),
                Term::Group { .. } => String::new(),
            };
            report(
                ErrorKind::Ambiguous {
                    name,
                    first: self.parts[first].span.start,
                },
                self.parts[second].span.start,
            );
        }
    }

    /// `u` vuelve a entrar en el conjunto en curso desde el nivel `now`
    /// después de haber entrado desde `level[u]` (siempre uno más bajo: la
    /// subida solo sube). Si entre los dos niveles, ambos incluidos, hay una
    /// partícula con contador, los dos caminos lo actualizan distinto: el
    /// modelo no es determinista con contadores (ver el módulo). O(1).
    ///
    /// Por simplicidad el rango incluye el nivel `level[u]` aunque `u` haya
    /// entrado ahí como hermana (la partícula de ese nivel se sale en los
    /// dos caminos): puede salir un segundo error con la causa mal atribuida,
    /// nunca un rechazo de más (`docs/fase3/pendientes.md` §38, 5.7-CR2-S1).
    fn reentry(&mut self, u: usize, report: &mut impl FnMut(ErrorKind, Pos)) {
        let (from, to) = (self.level[u] as usize, self.now as usize);
        if from >= to || self.reported_counter[u] || !self.has_first[u] {
            return;
        }
        let (_, top) = self.path[to];
        if top == NONE || top < from {
            return;
        }
        self.reported_counter[u] = true;
        let counter = self.parts[self.path[top].0].span.start;
        let repeat = self.parts[self.path[to].0].span.start;
        let (name, group) = match &self.parts[u].term {
            Term::Element { name, .. } => (clip(name), None),
            Term::Group { group, .. } => (
                String::new(),
                Some(match group.compositor {
                    Compositor::Sequence => "xs:sequence",
                    Compositor::Choice => "xs:choice",
                    Compositor::All => "xs:all",
                }),
            ),
        };
        report(
            ErrorKind::AmbiguousCounter {
                name,
                group,
                repeat,
                counter,
            },
            self.parts[u].span.start,
        );
    }

    /// Enumera *follow*(`x`) (ver el módulo).
    fn follow(&mut self, x: usize, report: &mut impl FnMut(ErrorKind, Pos)) {
        let mut cur = x;
        self.path.clear();
        loop {
            step();
            // El nivel `now` de la subida es `cur`.
            self.now = self.path.len() as u32;
            let below = self.path.last().map_or(NONE, |&(_, t)| t);
            let top = if counted(self.parts[cur].occurs) {
                self.path.len()
            } else {
                below
            };
            self.path.push((cur, top));
            if repeats(self.parts[cur].occurs) {
                self.add_first(cur, report);
            }
            let p = self.parent[cur];
            if p == NONE {
                return;
            }
            let siblings = self.children(p);
            match self.compositor(p) {
                Some(Compositor::Sequence) => {
                    for &s in &siblings[self.slot[cur] + 1..] {
                        self.add_first(s, report);
                        if !self.nullable[s] {
                            // `x` no es una de las últimas de `p`.
                            return;
                        }
                    }
                }
                Some(Compositor::All) => {
                    for &s in siblings {
                        if s != cur {
                            self.add_first(s, report);
                        }
                    }
                }
                Some(Compositor::Choice) | None => {}
            }
            cur = p;
        }
    }

    fn run(mut self, report: &mut impl FnMut(ErrorKind, Pos)) {
        self.begin();
        self.now = 0;
        self.add_first(0, report);
        for x in 0..self.parts.len() {
            if self.name[x] != NONE {
                self.begin();
                self.follow(x, report);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PASOS;
    use crate::{analyze, parse};

    const XS: &str = "xmlns:xs='http://www.w3.org/2001/XMLSchema'";

    /// Pasos de la UPA al analizar `doc` y su primer código de error.
    fn pasos(doc: &str) -> (u64, Option<&'static str>) {
        let schema = parse(doc.as_bytes()).unwrap();
        PASOS.with(|p| p.set(0));
        let code = analyze(&schema).err().map(|e| e[0].code());
        (PASOS.with(|p| p.get()), code)
    }

    /// Un tipo con el modelo `xs:choice maxOccurs="unbounded"` de `k`
    /// elementos de nombres distintos: el peor caso de la UPA (cada
    /// *follow* es el modelo entero), con `k` + 1 partículas.
    fn eleccion(k: usize) -> String {
        let hijos: String = (0..k)
            .map(|i| format!("<xs:element name='e{i}' type='xs:int'/>"))
            .collect();
        format!(
            "<xs:schema {XS}><xs:element name='r' type='T'/><xs:complexType name='T'>\
             <xs:choice maxOccurs='unbounded'>{hijos}</xs:choice></xs:complexType></xs:schema>"
        )
    }

    /// Criterio (D26, D1 sin reloj): un modelo en el tope (4 096 partículas,
    /// el peor caso) termina dentro de la cota documentada, 3·n·(m + 1)
    /// pasos, y es cuadrático de verdad (más de n·m); uno por encima se
    /// rechaza con `limit` **sin ningún paso** de la UPA: el tope va antes de
    /// calcular conjuntos.
    #[test]
    fn tope_antes_de_calcular_conjuntos() {
        let (n, m) = (4096u64, 4095u64);
        let (p, code) = pasos(&eleccion(4095));
        assert_eq!(code, None);
        assert!(p > n * m && p <= 3 * n * (m + 1), "{p}");
        let (p, code) = pasos(&eleccion(4096));
        assert_eq!((p, code), (0, Some("limit")));
    }

    /// I-1: la comprobación de las vueltas con contador es O(1) por paso y
    /// no cambia la cota. En el tope, `(e₀* | … | e₄₀₉₄*)*` vuelve a entrar
    /// en cada *follow* por las dos vueltas (sin contador: pasa) y
    /// `(e₀* | … | e₄₀₉₄*){1,2}` da una vuelta ambigua por partícula, con
    /// los mismos pasos.
    #[test]
    fn vueltas_con_contador_sin_costo_extra() {
        let (n, m) = (4096u64, 4095u64);
        let con = |max: &str| {
            eleccion(4095)
                .replace("type='xs:int'/>", "type='xs:int' maxOccurs='unbounded'/>")
                .replace("maxOccurs='unbounded'>", &format!("maxOccurs='{max}'>"))
        };
        let (p, code) = pasos(&con("unbounded"));
        assert_eq!(code, None);
        assert!(p > n * m && p <= 3 * n * (m + 1), "{p}");
        let doc = con("2");
        let schema = parse(doc.as_bytes()).unwrap();
        PASOS.with(|p| p.set(0));
        let errs = analyze(&schema).unwrap_err();
        let p = PASOS.with(|p| p.get());
        assert!(p > n * m && p <= 3 * n * (m + 1), "{p}");
        assert_eq!(errs.len(), 4095);
        assert!(errs
            .iter()
            .all(|e| matches!(e.kind, crate::ErrorKind::AmbiguousCounter { .. })));
    }

    /// Con un error de la etapa 1 la UPA no se calcula (sus errores no se
    /// informarían).
    #[test]
    fn sin_upa_si_falla_una_etapa_anterior() {
        let doc = eleccion(100).replace("name='e7' type='xs:int'", "name='e7' type='Nadie'");
        assert_eq!(pasos(&doc), (0, Some("unresolved")));
    }

    /// Un modelo lineal (secuencia de 4 000 elementos con el mismo nombre)
    /// cuesta O(n): cada *follow* tiene una posición.
    #[test]
    fn secuencia_larga_es_lineal() {
        let e = "<xs:element name='e' type='xs:int'/>".repeat(4000);
        let doc = format!(
            "<xs:schema {XS}><xs:element name='r' type='T'/><xs:complexType name='T'>\
             <xs:sequence>{e}</xs:sequence></xs:complexType></xs:schema>"
        );
        let (p, code) = pasos(&doc);
        assert_eq!(code, None);
        assert!(p <= 10 * 4001, "{p}");
    }
}
