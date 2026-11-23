//! F5.4-P1 — Generador de documentos XML desde la IR de un XSD, válidos,
//! inválidos y malformados, con semilla.
//!
//! **Válidos** ([`generar_xml`]). El documento elige uno de los elementos
//! globales (el modelo de contenido de la raíz). Cada elemento escribe sus
//! atributos (los requeridos siempre, los opcionales a veces, el valor `fixed`
//! si lo hay) y después su contenido: el modelo de solo elementos
//! (`xs:sequence` en orden, `xs:choice` con una rama, `xs:all` en un orden al
//! azar, cada partícula entre `minOccurs` y como mucho `minOccurs` + 2), el
//! texto de su `xs:simpleContent` o nada. Los textos salen del tipo
//! predefinido (`xs:boolean`, `xs:integer`, `xs:decimal`, `xs:float`/`double`,
//! `xs:string`) con sus facetas, con las mismas rutinas de números y cadenas
//! que el generador JSON ([`crate::generar`]) y `pattern` en el dialecto XSD.
//!
//! **Inválidos** ([`generar_xml_invalido`]). La misma generación con un
//! sabotaje en una de sus oportunidades, elegida al azar: omitir un hijo
//! obligatorio, omitir un atributo requerido o escribir un texto fuera de su
//! tipo. Queda bien formado.
//!
//! **Malformados** ([`malformar`]). Una mutación de texto de un documento
//! válido: truncarlo, cambiar el nombre de una etiqueta de cierre, repetir un
//! atributo, una entidad no declarada o un `<!DOCTYPE`.
//!
//! El generador sigue la semántica del XSD, no el autómata, así que el DPDA
//! es un oráculo independiente: [`generar_xml_valido`] solo acepta lo que el
//! DPDA da por válido, y las pruebas exigen que lo acepte casi siempre (una
//! discrepancia es un fallo de uno de los dos). La línea base XML (libxml2)
//! solo corre en Docker (D54), así que no entra aquí.
//!
//! La recursión de [`GenXml::elemento`] y [`GenXml::particula`] está acotada
//! por [`PROFUNDIDAD_MAXIMA`] elementos y [`ANIDAMIENTO_MAXIMO`] grupos, y el
//! documento por [`MAX_ELEMENTOS`]: nunca crece con la entrada (D7).

use automaton::{Automaton, ErrorKind, Mode};
use ir::{ContentModel, Lexical, LitToken, NodeId, Rule, Sort, Term, VerifiedIr};

use crate::generar::{Azar, Gen, PROFUNDIDAD};

/// Elementos anidados como mucho: pasado [`PROFUNDIDAD`] se eligen las
/// ocurrencias mínimas, y pasado este tope el intento falla (un tipo
/// recursivo con un hijo obligatorio no tiene documento finito).
pub const PROFUNDIDAD_MAXIMA: usize = 3 * PROFUNDIDAD;

/// Grupos anidados dentro de un modelo de contenido, como mucho.
pub const ANIDAMIENTO_MAXIMO: usize = 64;

/// Elementos por documento, como mucho.
pub const MAX_ELEMENTOS: usize = 400;

/// Qué se rompe en un documento inválido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sabotaje {
    /// Una partícula con `minOccurs` ≥ 1 aparece una vez menos.
    FaltaHijo,
    /// Falta un atributo con `use="required"`.
    FaltaAtributo,
    /// Un texto fuera de su tipo: no numérico, fuera del `enum`, más largo
    /// que su `maxLength`…
    TextoFueraDeTipo,
}

impl Sabotaje {
    pub fn nombre(self) -> &'static str {
        match self {
            Sabotaje::FaltaHijo => "falta-hijo",
            Sabotaje::FaltaAtributo => "falta-atributo",
            Sabotaje::TextoFueraDeTipo => "texto-fuera-de-tipo",
        }
    }
}

struct GenXml<'a> {
    g: Gen<'a>,
    /// La oportunidad que se sabotea (1, 2, …), o ninguna.
    sabotear: Option<usize>,
    oportunidades: usize,
    hecho: Option<Sabotaje>,
    elementos: usize,
}

impl<'a> GenXml<'a> {
    fn new(ir: &'a VerifiedIr, semilla: u64, sabotear: Option<usize>) -> GenXml<'a> {
        GenXml {
            g: Gen {
                ir,
                azar: Azar::new(semilla),
                dialecto: ir::regex::Dialect::Xsd,
            },
            sabotear,
            oportunidades: 0,
            hecho: None,
            elementos: 0,
        }
    }

    /// Cuenta una oportunidad de sabotaje; `true` si es la elegida.
    fn oportunidad(&mut self, s: Sabotaje) -> bool {
        self.oportunidades += 1;
        if self.hecho.is_none() && self.sabotear == Some(self.oportunidades) {
            self.hecho = Some(s);
            return true;
        }
        false
    }

    fn documento(&mut self) -> Option<String> {
        let ir = self.g.ir;
        let raiz = ir.node(NodeId::ROOT);
        let modelo = raiz.rules.iter().find_map(|r| match &r.rule {
            Rule::Content(m) => Some(m),
            _ => None,
        })?;
        let cuerpo = self.particula(modelo, 0, 0, 0)?;
        if cuerpo.is_empty() {
            return None;
        }
        let decl = if self.g.azar.moneda() {
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
        } else {
            ""
        };
        Some(format!("{decl}{}", cuerpo.trim_start()))
    }

    fn elemento(&mut self, nombre: &str, ty: NodeId, prof: usize) -> Option<String> {
        if prof > PROFUNDIDAD_MAXIMA || self.elementos >= MAX_ELEMENTOS {
            return None;
        }
        self.elementos += 1;
        let ir = self.g.ir;
        let n = ir.node(ty);
        let sangria = format!("\n{}", "  ".repeat(prof.saturating_sub(1)));
        let mut atributos = String::new();
        let mut contenido: Option<String> = None;
        for r in &n.rules {
            match &r.rule {
                Rule::Attributes(lista) => {
                    for a in lista {
                        if a.required {
                            if self.oportunidad(Sabotaje::FaltaAtributo) {
                                continue;
                            }
                        } else if !self.g.azar.moneda() {
                            continue;
                        }
                        let valor = match (&a.fixed, a.required) {
                            (Some(f), _) => f.clone(),
                            (None, true) => self.texto(a.ty)?,
                            // Uno opcional que no se sabe generar se omite.
                            (None, false) => match self.texto(a.ty) {
                                Some(v) => v,
                                None => continue,
                            },
                        };
                        atributos.push_str(&format!(" {}=\"{}\"", a.name, escapar(&valor, true)));
                    }
                }
                Rule::Content(m) => {
                    let hijos = self.particula(m, 0, prof, 0)?;
                    contenido = Some(if hijos.is_empty() {
                        String::new()
                    } else {
                        format!("{hijos}{sangria}")
                    });
                }
                Rule::SimpleContent(t) => contenido = Some(escapar(&self.texto(*t)?, false)),
                _ => {}
            }
        }
        Some(match contenido {
            Some(c) if !c.is_empty() => format!("{sangria}<{nombre}{atributos}>{c}</{nombre}>"),
            _ => format!("{sangria}<{nombre}{atributos}/>"),
        })
    }

    /// Las ocurrencias de la partícula `i` del modelo.
    fn particula(
        &mut self,
        m: &ContentModel,
        i: usize,
        prof: usize,
        nivel: usize,
    ) -> Option<String> {
        if nivel > ANIDAMIENTO_MAXIMO {
            return None;
        }
        let p = m.particles.get(i)?;
        let min = p.occurs.min;
        let max = p.occurs.max.unwrap_or(min + 2).min(min + 2).max(min);
        // Un `minOccurs` enorme (el XSD es entrada del usuario) no se
        // recorre: cada vuelta puede no crear ningún elemento (un grupo de
        // partículas opcionales), así que el tope de elementos no lo corta.
        if min > MAX_ELEMENTOS as u64 {
            return None;
        }
        let mut veces = if prof >= PROFUNDIDAD {
            min
        } else {
            self.g.azar.rango(min, max)
        };
        // La partícula raíz del documento no es una oportunidad: sin ella el
        // documento queda vacío, que no es un inválido sino nada.
        let raiz = prof == 0 && nivel == 0;
        if min >= 1 && !raiz && self.oportunidad(Sabotaje::FaltaHijo) {
            veces = min - 1;
        }
        let mut out = String::new();
        for _ in 0..veces {
            match &p.term {
                Term::Element { name, ty } => out.push_str(&self.elemento(name, *ty, prof + 1)?),
                Term::Sequence(hijos) => {
                    for &h in hijos {
                        out.push_str(&self.particula(m, h, prof, nivel + 1)?);
                    }
                }
                Term::Choice(hijos) => {
                    // Las ramas en un orden al azar: si una no da documento
                    // (un tipo recursivo pasado el tope), la siguiente.
                    let mut orden = hijos.clone();
                    self.barajar(&mut orden);
                    let mut hecho = None;
                    for h in orden {
                        // Lo que haga una rama que falla se deshace: si el
                        // sabotaje cayó en ella, el documento no lo llevaría.
                        let antes = (self.oportunidades, self.hecho, self.elementos);
                        if let Some(s) = self.particula(m, h, prof, nivel + 1) {
                            hecho = Some(s);
                            break;
                        }
                        (self.oportunidades, self.hecho, self.elementos) = antes;
                    }
                    out.push_str(&hecho?);
                }
                Term::All(hijos) => {
                    let mut orden = hijos.clone();
                    self.barajar(&mut orden);
                    for h in orden {
                        out.push_str(&self.particula(m, h, prof, nivel + 1)?);
                    }
                }
            }
        }
        Some(out)
    }

    fn barajar(&mut self, v: &mut [usize]) {
        for i in (1..v.len()).rev() {
            let j = self.g.azar.rango(0, i as u64) as usize;
            v.swap(i, j);
        }
    }

    /// Un texto del nodo [`Sort::Text`] `t` (sin escapar).
    fn texto(&mut self, t: NodeId) -> Option<String> {
        let c = self.g.conj(&[t]);
        if c.rechaza {
            return None;
        }
        let reglas: Vec<&Rule> = Gen::reglas(&c).collect();
        let lexico = reglas
            .iter()
            .find_map(|r| match r {
                Rule::Lexical(l) => Some(*l),
                _ => None,
            })
            .unwrap_or(Lexical::String);
        // Las enumeraciones que cumplen las demás facetas de la cadena de
        // restricciones (una derivada puede acotar las de su base:
        // `PocoPorcentaje` de `tipos-simples.xsd`).
        // Si varias restricciones de la cadena enumeran, vale la intersección.
        let listas: Vec<Vec<String>> = reglas
            .iter()
            .filter_map(|r| match r {
                Rule::Enum(ls) => Some(ls.iter().filter_map(|l| texto_literal(&l.0)).collect()),
                _ => None,
            })
            .collect();
        let igual = |a: &str, b: &str| {
            if lexico.is_numeric() {
                ir::num::compare(a, b) == std::cmp::Ordering::Equal
            } else {
                a == b
            }
        };
        let enumeracion: Vec<String> = listas
            .first()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|v| listas[1..].iter().all(|l| l.iter().any(|w| igual(v, w))))
            .filter(|v| cumple_facetas(v, lexico, &reglas))
            .collect();
        let con_enum = reglas.iter().any(|r| matches!(r, Rule::Enum(_)));
        if con_enum && enumeracion.is_empty() {
            return None;
        }
        if let Some(malo) = fuera_de_tipo(lexico, &reglas, con_enum) {
            if self.oportunidad(Sabotaje::TextoFueraDeTipo) {
                return Some(malo);
            }
        }
        if let Some(v) = self.g.azar.elegir(&enumeracion) {
            return Some(v.clone());
        }
        match lexico {
            Lexical::String => self.g.cadena(&c),
            Lexical::Boolean => {
                let v = ["true", "false", "1", "0"];
                self.g.azar.elegir(&v).map(|s| (*s).to_owned())
            }
            Lexical::Integer => self.g.numero(&c, true),
            Lexical::Decimal | Lexical::Float | Lexical::Double => {
                let entero = self.g.azar.moneda();
                self.g.numero(&c, entero)
            }
        }
    }
}

/// El texto de un literal de `xs:enumeration` (un solo token).
fn texto_literal(t: &[LitToken]) -> Option<String> {
    match t {
        [LitToken::String(s)] => Some(s.clone()),
        [LitToken::Number(n)] => Some(n.clone()),
        [LitToken::Bool(b)] => Some(b.to_string()),
        _ => None,
    }
}

/// Si el valor `v` de una enumeración cumple las facetas numéricas, de
/// longitud y de `pattern` de `reglas`.
fn cumple_facetas(v: &str, lexico: Lexical, reglas: &[&Rule]) -> bool {
    let x: Option<f64> = if lexico.is_numeric() {
        v.trim().parse().ok()
    } else {
        None
    };
    let n = v.chars().count() as u64;
    let cota = |l: &str| l.parse::<f64>().unwrap_or(f64::NAN);
    reglas.iter().all(|r| match (r, x) {
        (Rule::Minimum(c), Some(x)) => x >= cota(&c.lexeme),
        (Rule::Maximum(c), Some(x)) => x <= cota(&c.lexeme),
        (Rule::ExclusiveMinimum(c), Some(x)) => x > cota(&c.lexeme),
        (Rule::ExclusiveMaximum(c), Some(x)) => x < cota(&c.lexeme),
        (Rule::MinLength(m), _) => n >= *m,
        (Rule::MaxLength(m), _) => n <= *m,
        (Rule::Pattern(p), _) => ir::regex::parse(p, ir::regex::Dialect::Xsd)
            .map(|re| re.matches(v))
            .unwrap_or(false),
        _ => true,
    })
}

/// Un texto que su tipo no admite, si se sabe escribir uno.
fn fuera_de_tipo(lexico: Lexical, reglas: &[&Rule], con_enum: bool) -> Option<String> {
    if lexico != Lexical::String {
        return Some("no-es-un-valor".to_owned());
    }
    if con_enum {
        return Some("zz-fuera-de-la-enumeracion".to_owned());
    }
    let max = reglas.iter().find_map(|r| match r {
        Rule::MaxLength(n) => Some(*n),
        _ => None,
    });
    match max {
        Some(n) if n < 256 => Some("a".repeat(n as usize + 1)),
        _ => None,
    }
}

/// Escapa `&`, `<`, `>` y, en un valor de atributo, `"`.
fn escapar(s: &str, atributo: bool) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' if atributo => o.push_str("&quot;"),
            // Un atributo normaliza los blancos a espacio y un texto `\r` a
            // `\n`: como referencias de carácter llegan tal cual.
            '\t' if atributo => o.push_str("&#9;"),
            '\n' if atributo => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            c => o.push(c),
        }
    }
    o
}

/// Un documento generado con `semilla` (sin comprobar su validez). `None` si
/// la IR no es de un XSD o si el generador no supo construir uno.
pub fn generar_xml(ir: &VerifiedIr, semilla: u64) -> Option<String> {
    if ir.node(NodeId::ROOT).sort != Sort::Element {
        return None;
    }
    GenXml::new(ir, semilla, None).documento()
}

/// Un documento **válido**: prueba las semillas `semilla`, `semilla + 1`, …
/// (como mucho `intentos`) y devuelve el primero que el DPDA acepta.
pub fn generar_xml_valido(
    ir: &VerifiedIr,
    m: &Automaton,
    semilla: u64,
    intentos: u64,
) -> Option<(String, u64)> {
    (semilla..semilla.saturating_add(intentos)).find_map(|s| {
        let p = generar_xml(ir, s)?;
        crate::validar_dpda(m, p.as_bytes())
            .valido
            .then_some((p, s))
    })
}

/// Un documento **inválido** con `semilla`: el de [`generar_xml`] con un
/// sabotaje en una de sus oportunidades, elegida con la misma semilla.
/// `None` si no hay ninguna oportunidad o el generador falla.
pub fn generar_xml_invalido(ir: &VerifiedIr, semilla: u64) -> Option<(String, Sabotaje)> {
    if ir.node(NodeId::ROOT).sort != Sort::Element {
        return None;
    }
    // Primera pasada: cuántas oportunidades hay con esta semilla.
    let mut cuenta = GenXml::new(ir, semilla, None);
    cuenta.documento()?;
    if cuenta.oportunidades == 0 {
        return None;
    }
    let objetivo = Azar::new(semilla ^ 0x5AB0).rango(1, cuenta.oportunidades as u64) as usize;
    // Segunda: las mismas decisiones hasta la oportunidad elegida.
    let mut g = GenXml::new(ir, semilla, Some(objetivo));
    let doc = g.documento()?;
    Some((doc, g.hecho?))
}

/// Una mutación que deja el documento mal formado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Malformacion {
    /// Cortado antes del final.
    Truncado,
    /// La última etiqueta de cierre con otro nombre.
    CierreDistinto,
    /// Un atributo repetido en la misma etiqueta.
    AtributoRepetido,
    /// Una referencia a una entidad no declarada.
    EntidadDesconocida,
    /// Un `<!DOCTYPE` delante.
    Doctype,
}

impl Malformacion {
    pub const TODAS: [Malformacion; 5] = [
        Malformacion::Truncado,
        Malformacion::CierreDistinto,
        Malformacion::AtributoRepetido,
        Malformacion::EntidadDesconocida,
        Malformacion::Doctype,
    ];

    pub fn nombre(self) -> &'static str {
        match self {
            Malformacion::Truncado => "truncado",
            Malformacion::CierreDistinto => "cierre-distinto",
            Malformacion::AtributoRepetido => "atributo-repetido",
            Malformacion::EntidadDesconocida => "entidad-desconocida",
            Malformacion::Doctype => "doctype",
        }
    }

    /// Las clases de error que puede dar el validador: un truncado a mitad
    /// de un nombre, de un valor o de la declaración deja un token sin
    /// terminar (léxico); entre tokens, la sintaxis queda incompleta.
    pub fn clases(self) -> &'static [ErrorKind] {
        match self {
            Malformacion::EntidadDesconocida | Malformacion::Doctype => &[ErrorKind::Lexical],
            Malformacion::Truncado => &[ErrorKind::Lexical, ErrorKind::Syntax],
            _ => &[ErrorKind::Syntax],
        }
    }
}

/// Aplica `mal` a un documento bien formado. `None` si no se puede (sin
/// atributos, sin texto…).
pub fn malformar(doc: &str, mal: Malformacion, azar: &mut Azar) -> Option<String> {
    match mal {
        Malformacion::Truncado => {
            let fin = doc.rfind('>')?;
            if fin < 2 {
                return None;
            }
            let mut corte = azar.rango(1, fin as u64 - 1) as usize;
            while !doc.is_char_boundary(corte) {
                corte -= 1;
            }
            Some(doc[..corte].to_owned())
        }
        Malformacion::CierreDistinto => {
            let i = doc.rfind("</")?;
            let fin = i + doc[i..].find('>')?;
            Some(format!("{}x{}", &doc[..fin], &doc[fin..]))
        }
        Malformacion::AtributoRepetido => {
            // El primer atributo de la primera etiqueta que tenga uno.
            let mut desde = 0;
            while let Some(i) = doc[desde..].find('<').map(|k| desde + k) {
                let fin = i + doc[i..].find('>')?;
                let etiqueta = &doc[i..fin];
                if !etiqueta.starts_with("<?") && !etiqueta.starts_with("</") {
                    if let Some(e) = etiqueta.find(' ') {
                        let resto = &etiqueta[e..];
                        let cierre = resto.find("\" ").or_else(|| resto.rfind('"'))?;
                        let atributo = &resto[..=cierre];
                        let pos = i + e + atributo.len();
                        return Some(format!("{}{atributo}{}", &doc[..pos], &doc[pos..]));
                    }
                }
                desde = fin + 1;
            }
            None
        }
        Malformacion::EntidadDesconocida => {
            // Dentro del contenido de un elemento: tras el `>` de la primera
            // etiqueta de apertura que no se cierre sola.
            let mut desde = doc.find("?>").map_or(0, |k| k + 2);
            loop {
                let i = desde + doc[desde..].find('<')?;
                let fin = i + doc[i..].find('>')?;
                let etiqueta = &doc[i..=fin];
                if !etiqueta.starts_with("</") && !etiqueta.ends_with("/>") {
                    let j = fin + 1;
                    return Some(format!("{}&desconocida;{}", &doc[..j], &doc[j..]));
                }
                desde = fin + 1;
            }
        }
        Malformacion::Doctype => {
            // Después de la declaración, si la hay: delante de ella, la
            // declaración fuera de sitio ya lo haría malformado.
            let j = doc.find("?>").map_or(0, |k| k + 2);
            Some(format!("{}<!DOCTYPE r>{}", &doc[..j], &doc[j..]))
        }
    }
}

/// La clase de error del DPDA con `payload` (`None` si es válido).
pub fn clase_dpda(m: &Automaton, payload: &[u8]) -> Option<ErrorKind> {
    let r = automaton::validate(m, payload, Mode::Total, 4096);
    if r.valid {
        None
    } else {
        r.error_kind
    }
}
