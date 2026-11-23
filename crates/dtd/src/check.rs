//! Fase 3 de DTD (DTD.3; `docs/fase3/dtd.md`): tabla de símbolos, reglas
//! semánticas y determinismo de los modelos de contenido.
//!
//! Entrada: el AST de [`crate::parse`]. Salida: un [`Analysis`] que la bajada
//! ([`crate::lower`]) convierte en IR, o **todos** los errores encontrados,
//! ordenados por posición.
//!
//! Sin recursión (D7): cada modelo de contenido se aplana a una arena en
//! preorden con una pila explícita, y el determinismo (Glushkov) se calcula
//! recorriendo esa arena de atrás hacia adelante.

use std::collections::HashMap;

use lexer::{Pos, Span};

use crate::ast::{
    AttDef, AttType, ContentSpec, DefaultDecl, Dtd, Name, Occurs, Particle, ParticleKind,
};
use crate::parser::{DtdError, ErrorKind};

/// Partículas por modelo de contenido, la raíz incluida (`docs/00-alcance.md`
/// §4, D26, como en XSD).
pub const MAX_PARTICLES: usize = 4096;

/// Partícula de un modelo aplanado en preorden: los hijos de un grupo son
/// índices posteriores, como exige la IR (invariante I11).
#[derive(Clone, Debug)]
pub struct Flat<'a> {
    pub kind: FlatKind<'a>,
    pub occurs: Occurs,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum FlatKind<'a> {
    Name(&'a Name),
    Seq(Vec<usize>),
    Choice(Vec<usize>),
}

/// Resultado del análisis sin errores.
#[derive(Debug)]
pub struct Analysis<'a> {
    pub dtd: &'a Dtd,
    /// Nombre de elemento → posición en `dtd.elements`.
    pub index: HashMap<&'a str, usize>,
    /// Por elemento: su modelo aplanado (vacío si no es `Children`).
    pub models: Vec<Vec<Flat<'a>>>,
    /// Por elemento: sus atributos, de todas sus `<!ATTLIST>` en orden.
    pub attributes: Vec<Vec<&'a AttDef>>,
}

fn err(kind: ErrorKind, at: Pos, message: String) -> DtdError {
    DtdError { kind, at, message }
}

fn donde(p: Pos) -> String {
    format!("{}:{}", p.line, p.column)
}

/// Analiza la DTD.
///
/// # Errors
///
/// Todos los errores semánticos encontrados, ordenados por posición.
pub fn analyze(dtd: &Dtd) -> Result<Analysis<'_>, Vec<DtdError>> {
    let mut errores = Vec::new();

    if dtd.elements.is_empty() {
        errores.push(err(
            ErrorKind::NoElements,
            Pos::START,
            "la DTD no declara ningún elemento: un documento necesita al menos un <!ELEMENT>"
                .into(),
        ));
    }

    // Tabla de símbolos de los elementos (la primera declaración manda).
    let mut index: HashMap<&str, usize> = HashMap::new();
    for (i, e) in dtd.elements.iter().enumerate() {
        if let Some(&j) = index.get(e.name.text.as_str()) {
            errores.push(err(
                ErrorKind::Duplicate,
                e.name.span.start,
                format!(
                    "el elemento «{}» ya se declaró en {}",
                    e.name.text,
                    donde(dtd.elements[j].name.span.start)
                ),
            ));
        } else {
            index.insert(&e.name.text, i);
        }
    }

    // Atributos: se unen las `<!ATTLIST>` del mismo elemento.
    let mut attributes: Vec<Vec<&AttDef>> = vec![Vec::new(); dtd.elements.len()];
    for a in &dtd.attlists {
        let Some(&i) = index.get(a.element.text.as_str()) else {
            errores.push(err(
                ErrorKind::Unresolved,
                a.element.span.start,
                format!(
                    "<!ATTLIST> de «{}», que no tiene <!ELEMENT>",
                    a.element.text
                ),
            ));
            continue;
        };
        for def in &a.defs {
            if let Some(previo) = attributes[i].iter().find(|d| d.name.text == def.name.text) {
                errores.push(err(
                    ErrorKind::Duplicate,
                    def.name.span.start,
                    format!(
                        "el atributo «{}» de «{}» ya se declaró en {}",
                        def.name.text,
                        a.element.text,
                        donde(previo.name.span.start)
                    ),
                ));
                continue;
            }
            comprobar_valores(def, &mut errores);
            attributes[i].push(def);
        }
    }

    // Modelos de contenido: nombres resueltos, tope de partículas y determinismo.
    let mut models = Vec::with_capacity(dtd.elements.len());
    for e in &dtd.elements {
        let ContentSpec::Children(raiz) = &e.content else {
            models.push(Vec::new());
            continue;
        };
        let flat = match aplanar(raiz) {
            Ok(f) => f,
            Err(e) => {
                errores.push(e);
                models.push(Vec::new());
                continue;
            }
        };
        let mut resueltos = true;
        for p in &flat {
            if let FlatKind::Name(n) = p.kind {
                if !index.contains_key(n.text.as_str()) {
                    resueltos = false;
                    errores.push(err(
                        ErrorKind::Unresolved,
                        n.span.start,
                        format!(
                            "«{}» se usa en el modelo de «{}», pero no tiene <!ELEMENT>",
                            n.text, e.name.text
                        ),
                    ));
                }
            }
        }
        if resueltos {
            if let Some(e) = determinismo(&e.name.text, &flat) {
                errores.push(e);
            }
        }
        models.push(flat);
    }

    if errores.is_empty() {
        Ok(Analysis {
            dtd,
            index,
            models,
            attributes,
        })
    } else {
        errores.sort_by_key(|e| e.at.offset);
        Err(errores)
    }
}

/// Enumeración sin valores repetidos («No Duplicate Tokens» de XML) y valor
/// por defecto o `#FIXED` dentro de ella.
fn comprobar_valores(def: &AttDef, errores: &mut Vec<DtdError>) {
    let AttType::Enumeration(valores) = &def.ty else {
        return;
    };
    for (k, v) in valores.iter().enumerate() {
        if let Some(previo) = valores[..k].iter().find(|p| p.text == v.text) {
            errores.push(err(
                ErrorKind::Duplicate,
                v.span.start,
                format!(
                    "el valor «{}» se repite en la enumeración de «{}» (ya está en {})",
                    v.text,
                    def.name.text,
                    donde(previo.span.start)
                ),
            ));
        }
    }
    let literal = match &def.default {
        DefaultDecl::Fixed(l) | DefaultDecl::Default(l) => l,
        DefaultDecl::Required | DefaultDecl::Implied => return,
    };
    // Sin normalizar espacios (§3bis.4, desviación 3).
    if !valores.iter().any(|v| v.text == literal.value) {
        errores.push(err(
            ErrorKind::InvalidValue,
            literal.span.start,
            format!(
                "«{}» no es uno de los valores de «{}»: {}",
                literal.value,
                def.name.text,
                valores
                    .iter()
                    .map(|v| v.text.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
}

/// Aplana el modelo a una arena en preorden con una pila explícita. Pasar de
/// [`MAX_PARTICLES`] es `Limit` en la partícula siguiente.
fn aplanar(raiz: &Particle) -> Result<Vec<Flat<'_>>, DtdError> {
    let mut flat: Vec<Flat<'_>> = Vec::new();
    // (partícula, índice de su padre en `flat`).
    let mut pila: Vec<(&Particle, Option<usize>)> = vec![(raiz, None)];
    while let Some((p, padre)) = pila.pop() {
        if flat.len() == MAX_PARTICLES {
            return Err(err(
                ErrorKind::Limit,
                p.span.start,
                format!("el modelo de contenido tiene más de {MAX_PARTICLES} partículas"),
            ));
        }
        let i = flat.len();
        let kind = match &p.kind {
            ParticleKind::Name(n) => FlatKind::Name(n),
            ParticleKind::Seq(_) => FlatKind::Seq(Vec::new()),
            ParticleKind::Choice(_) => FlatKind::Choice(Vec::new()),
        };
        flat.push(Flat {
            kind,
            occurs: p.occurs,
            span: p.span,
        });
        if let Some(padre) = padre {
            if let FlatKind::Seq(hijos) | FlatKind::Choice(hijos) = &mut flat[padre].kind {
                hijos.push(i);
            }
        }
        // Al revés, para sacarlos en orden: así cada hijo empieza donde
        // acaba el subárbol del anterior (preorden).
        if let ParticleKind::Seq(xs) | ParticleKind::Choice(xs) = &p.kind {
            pila.extend(xs.iter().rev().map(|x| (x, Some(i))));
        }
    }
    Ok(flat)
}

/// Conjunto de posiciones como mapa de bits.
#[derive(Clone)]
struct Bits(Vec<u64>);

impl Bits {
    fn new(n: usize) -> Self {
        Bits(vec![0; n.div_ceil(64)])
    }
    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    fn union(&mut self, o: &Bits) {
        for (a, b) in self.0.iter_mut().zip(&o.0) {
            *a |= b;
        }
    }
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(w, &x)| {
            (0..64)
                .filter(move |b| x & (1 << b) != 0)
                .map(move |b| w * 64 + b)
        })
    }
}

/// Determinismo del modelo (XML 1.0, apéndice E): en el autómata de Glushkov,
/// ni el conjunto inicial ni el *follow* de ninguna posición tienen dos
/// posiciones con el mismo nombre. Devuelve el primer conflicto, con las dos
/// partículas que compiten.
fn determinismo(elemento: &str, flat: &[Flat<'_>]) -> Option<DtdError> {
    let n = flat.len();
    let mut nullable = vec![false; n];
    let mut first: Vec<Bits> = vec![Bits::new(n); n];
    let mut last: Vec<Bits> = vec![Bits::new(n); n];
    let mut follow: Vec<Bits> = vec![Bits::new(n); n];
    let repite = |o: Occurs| matches!(o, Occurs::Star | Occurs::Plus);
    let opcional = |o: Occurs| matches!(o, Occurs::Opt | Occurs::Star);

    // En preorden los hijos van después del padre: de atrás hacia adelante,
    // cada nodo se calcula con sus hijos ya calculados.
    for i in (0..n).rev() {
        let (null, f, l) = match &flat[i].kind {
            FlatKind::Name(_) => {
                let mut b = Bits::new(n);
                b.set(i);
                (false, b.clone(), b)
            }
            FlatKind::Choice(hs) => {
                let (mut f, mut l) = (Bits::new(n), Bits::new(n));
                for &h in hs {
                    f.union(&first[h]);
                    l.union(&last[h]);
                }
                (hs.iter().any(|&h| nullable[h]), f, l)
            }
            FlatKind::Seq(hs) => {
                let mut f = Bits::new(n);
                for &h in hs {
                    f.union(&first[h]);
                    if !nullable[h] {
                        break;
                    }
                }
                let mut l = Bits::new(n);
                for &h in hs.iter().rev() {
                    l.union(&last[h]);
                    if !nullable[h] {
                        break;
                    }
                }
                // follow(last(prefijo)) ⊇ first(siguiente).
                let mut cola = Bits::new(n);
                for (k, &h) in hs.iter().enumerate() {
                    if k > 0 {
                        for p in cola.iter() {
                            follow[p].union(&first[h]);
                        }
                    }
                    if !nullable[h] {
                        cola = Bits::new(n);
                    }
                    cola.union(&last[h]);
                }
                (hs.iter().all(|&h| nullable[h]), f, l)
            }
        };
        if repite(flat[i].occurs) {
            for p in l.iter() {
                follow[p].union(&f);
            }
        }
        nullable[i] = null || opcional(flat[i].occurs);
        first[i] = f;
        last[i] = l;
    }

    let nombre = |p: usize| match flat[p].kind {
        FlatKind::Name(nm) => nm,
        _ => unreachable!("solo los nombres son posiciones"),
    };
    let choque = |conjunto: &Bits| -> Option<(usize, usize)> {
        let mut vistos: HashMap<&str, usize> = HashMap::new();
        for p in conjunto.iter() {
            if let Some(&q) = vistos.get(nombre(p).text.as_str()) {
                return Some((q, p));
            }
            vistos.insert(&nombre(p).text, p);
        }
        None
    };
    let conflicto = choque(&first[0]).or_else(|| (0..n).find_map(|p| choque(&follow[p])));
    conflicto.map(|(a, b)| {
        let (a, b) = (nombre(a), nombre(b));
        err(
            ErrorKind::NonDeterministic,
            b.span.start,
            format!(
                "el modelo de contenido de «{elemento}» no es determinista: «{}» en {} y «{}» en {} compiten por el \
                 mismo hijo (XML 1.0, apéndice E)",
                a.text,
                donde(a.span.start),
                b.text,
                donde(b.span.start)
            ),
        )
    })
}
