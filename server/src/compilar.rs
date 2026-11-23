//! La compilación de un esquema que hace `POST /api/schemas`: las Fases 2 y 3
//! del formato que dice el contenido (D22), la bajada a la IR verificada, su
//! JSON v2 (D17) y el resumen del autómata (F4-U7). Es la tubería de
//! `schemac check`, `ir` y `automaton` (`crates/cli/src/main.rs`) sobre los
//! bytes de la petición; `crates/cli` no se toca (U2) y unificar las dos en
//! un crate `pipeline` queda pendiente (`docs/fase7/pendientes.md` §6).
//!
//! [`compilar`] corre en el hilo de la petición (`api.rs`, 8 MiB de pila,
//! D7): el AST, la tabla de símbolos o el `Analysis` y la IR se crean y se
//! liberan dentro de él (los `Drop` del AST XSD son recursivos, D36), y solo
//! sale un [`Compilado`] hecho de texto.

use std::fmt;

use ir::{InternalError, VerifiedIr};

/// Errores del esquema que se devuelven; del resto solo se da el total
/// (`docs/fase3/pendientes.md` §1: la Fase 3 los informa todos, y su texto
/// puede ser ≈ 126 veces la subida).
pub const MAX_ERRORES: usize = 100;

/// Errores internos que van al registro; el resto solo se cuenta (D15, como
/// `MAX_DETALLE` de `schemac`).
const MAX_DETALLE: usize = 8;

/// Formato del esquema, detectado por su contenido (D22).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formato {
    Json,
    Xsd,
    /// DTD externa (D86).
    Dtd,
}

impl Formato {
    /// El valor de `format` en las respuestas (el de la IR: `"json"` o
    /// `"xml"`). Una DTD es `"xml"`, como un XSD: la API no cambia (D86, E2).
    pub fn nombre(self) -> &'static str {
        match self {
            Formato::Json => "json",
            Formato::Xsd | Formato::Dtd => "xml",
        }
    }
}

/// D22 enmendada por D86 (E1, E2): la regla vive en **una sola** función,
/// `dtd::formato`, la misma que usa `schemac` (antes era una copia literal de
/// la de la CLI). Tras un BOM, espacios, una `<?xml …?>` y comentarios,
/// `<!DOCTYPE` es XSD, otro `<!` es DTD, otro `<` es XSD y lo demás, JSON.
pub fn formato(bytes: &[u8]) -> Formato {
    match dtd::formato(bytes) {
        dtd::Formato::Json => Formato::Json,
        dtd::Formato::Xsd => Formato::Xsd,
        dtd::Formato::Dtd => Formato::Dtd,
    }
}

/// Un error del esquema tal como se devuelve: el `code` estable de su tipo,
/// la posición y el mensaje sin el prefijo «línea L, columna C: ».
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorEsquema {
    pub code: &'static str,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// Lo que se sabe del autómata de un esquema válido (U6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Automata {
    /// El resumen JSON de `write_summary_json`, tal cual.
    Ok(String),
    /// Una regla que aún no se compila (F4-U5; todo XSD hasta F4.1c), con
    /// el mensaje de `schemac automaton`.
    Pendiente(String),
    /// Un límite de `docs/00-alcance.md` §4 del constructor.
    Limite(String),
}

/// El resultado de una compilación: solo texto y estructuras propias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compilado {
    /// Esquema válido (200).
    Valido {
        formato: Formato,
        /// El resumen de `schemac check`.
        resumen: String,
        /// El JSON v2 de `ir::write_json`, o `None` si pasaba del tope (U8).
        ir: Option<String>,
        automata: Automata,
    },
    /// Errores del esquema (422): los [`MAX_ERRORES`] primeros de la
    /// primera etapa que falla y el total.
    Esquema {
        formato: Formato,
        errores: Vec<ErrorEsquema>,
        total: usize,
    },
    /// Error interno del compilador (500): las líneas para el registro; el
    /// usuario solo ve un mensaje fijo (D15).
    Interno(Vec<String>),
}

/// Un error del esquema de cualquiera de los frontales: los tres tienen
/// `code`, `line` y `column`, y su `Display` empieza por la posición.
trait ConPosicion: fmt::Display {
    fn posicion(&self) -> (&'static str, u32, u32);
}

impl ConPosicion for schema_parser::ParseError {
    fn posicion(&self) -> (&'static str, u32, u32) {
        (self.code(), self.line(), self.column())
    }
}

impl ConPosicion for semantic::SemanticError {
    fn posicion(&self) -> (&'static str, u32, u32) {
        (self.code(), self.line(), self.column())
    }
}

impl ConPosicion for xsd::XsdError {
    fn posicion(&self) -> (&'static str, u32, u32) {
        (self.code(), self.line(), self.column())
    }
}

impl ConPosicion for dtd::DtdError {
    fn posicion(&self) -> (&'static str, u32, u32) {
        (self.code(), self.line(), self.column())
    }
}

/// Los [`MAX_ERRORES`] primeros de `errores` (solo esos se pasan a texto) y
/// el total.
fn errores<E: ConPosicion>(formato: Formato, errores: &[E]) -> Compilado {
    let lista = errores
        .iter()
        .take(MAX_ERRORES)
        .map(|e| {
            let (code, line, column) = e.posicion();
            let texto = e.to_string();
            let prefijo = format!("línea {line}, columna {column}: ");
            let message = match texto.strip_prefix(&prefijo) {
                Some(resto) => resto.to_owned(),
                None => texto,
            };
            ErrorEsquema {
                code,
                line,
                column,
                message,
            }
        })
        .collect();
    Compilado::Esquema {
        formato,
        errores: lista,
        total: errores.len(),
    }
}

/// Las líneas del registro de un error interno de la IR: el mensaje fijo, el
/// recuento y los [`MAX_DETALLE`] primeros invariantes rotos (tamaño acotado).
pub fn interno(e: &InternalError) -> Compilado {
    Compilado::Interno(
        std::iter::once(e.to_string())
            .chain(e.detail(MAX_DETALLE))
            .collect(),
    )
}

/// Un `fmt::Write` que falla en cuanto el texto pasaría de `max` bytes
/// (U8, B7): la IR se deja de escribir ahí, sin construirla entera.
#[derive(Debug)]
pub struct Tope {
    pub texto: String,
    max: usize,
}

impl Tope {
    pub fn new(max: usize) -> Tope {
        Tope {
            texto: String::new(),
            max,
        }
    }
}

impl fmt::Write for Tope {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // `texto.len() ≤ max` siempre: la resta no se desborda.
        if s.len() > self.max - self.texto.len() {
            return Err(fmt::Error);
        }
        self.texto.push_str(s);
        Ok(())
    }
}

/// Compila `bytes` con un tope de `max_ir` bytes para el JSON de la IR.
pub fn compilar(bytes: &[u8], max_ir: usize) -> Compilado {
    match compilar_o_fallar(bytes, max_ir) {
        Ok(c) | Err(c) => c,
    }
}

fn compilar_o_fallar(bytes: &[u8], max_ir: usize) -> Result<Compilado, Compilado> {
    let (f, resumen, ir) = ir_de(bytes)?;
    let mut tope = Tope::new(max_ir);
    let texto_ir = ir::write_json(&ir, &mut tope).ok().map(|()| tope.texto);
    Ok(Compilado::Valido {
        formato: f,
        resumen,
        ir: texto_ir,
        automata: automata(&ir)?,
    })
}

/// Las Fases 2 y 3 y la bajada (F7.6 la reutiliza para el grafo y la
/// comparación): el formato, el resumen de `schemac check` y la IR
/// verificada, o el [`Compilado`] de error (422 o 500).
pub fn ir_de(bytes: &[u8]) -> Result<(Formato, String, VerifiedIr), Compilado> {
    let f = formato(bytes);
    let (resumen, ir) = match f {
        Formato::Json => {
            let schema =
                schema_parser::parse(bytes).map_err(|e| errores(f, std::slice::from_ref(&e)))?;
            // `lower` exige una tabla que ya pasó `analyze`
            // (`docs/fase3/pendientes.md` §13): el servidor, segundo
            // llamador de `lower`, la obtiene siempre de `analyze`.
            let t = semantic::analyze(&schema).map_err(|e| errores(f, &e))?;
            let resumen = format!(
                "esquema válido (subconjunto de JSON Schema 2020-12); \
                 {} subesquemas, {} entradas de $defs, {} referencias resueltas",
                t.len(),
                t.defs().len(),
                t.refs().len()
            );
            (resumen, semantic::lower(&t).map_err(|e| interno(&e))?)
        }
        Formato::Xsd => {
            let schema = xsd::parse(bytes).map_err(|e| errores(f, std::slice::from_ref(&e)))?;
            let a = xsd::analyze(&schema).map_err(|e| errores(f, &e))?;
            let resumen = format!(
                "esquema válido (subconjunto de XSD 1.0); {} tipos complejos",
                a.type_count()
            );
            (resumen, xsd::lower(&a).map_err(|e| interno(&e))?)
        }
        Formato::Dtd => {
            let d = dtd::parse(bytes).map_err(|e| errores(f, std::slice::from_ref(&e)))?;
            let a = dtd::analyze(&d).map_err(|e| errores(f, &e))?;
            let resumen = format!(
                "esquema válido (subconjunto de DTD); {} elementos, {} atributos",
                d.elements.len(),
                a.attributes.iter().map(Vec::len).sum::<usize>()
            );
            (resumen, dtd::lower(&a).map_err(|e| interno(&e))?)
        }
    };
    Ok((f, resumen, ir))
}

/// El autómata de la IR (U6): su resumen, o por qué no se construye. Un
/// error interno del constructor es un error interno (500, D15).
fn automata(ir: &VerifiedIr) -> Result<Automata, Compilado> {
    use automaton::BuildError;
    match automaton::build(ir) {
        Ok(a) => {
            let mut s = String::new();
            a.write_summary_json(&mut s)
                .map_err(|_| Compilado::Interno(vec!["no se pudo escribir el resumen".into()]))?;
            Ok(Automata::Ok(s))
        }
        Err(e @ BuildError::Pending { .. }) => Ok(Automata::Pendiente(e.to_string())),
        Err(e @ (BuildError::Limit { .. } | BuildError::ProductLimit { .. })) => {
            Ok(Automata::Limite(e.to_string()))
        }
        Err(e @ BuildError::Internal(_)) => Err(Compilado::Interno(vec![e.to_string()])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{Constraint, Format, Ir, Node, NodeId, Origin, Pos, Rule, Sort, Span, Step};

    /// D22 enmendada por D86: el servidor delega en `dtd::formato`, la misma
    /// función que `schemac` (la tabla común completa la prueba
    /// `crates/dtd/tests/formato.rs`).
    #[test]
    fn formato_por_el_primer_byte_significativo() {
        for (bytes, esperado) in [
            (&b"<xs:schema/>"[..], Formato::Xsd),
            (b" \t\r\n<x/>", Formato::Xsd),
            (b"\xEF\xBB\xBF<x/>", Formato::Xsd),
            (b"\xEF\xBB\xBF \n<x/>", Formato::Xsd),
            (b"{\"type\": \"object\"}", Formato::Json),
            (b"\xEF\xBB\xBF{}", Formato::Json),
            (b"  true", Formato::Json),
            (b"", Formato::Json),
            (b" \n ", Formato::Json),
            // Solo se salta un BOM, y solo al principio.
            (b"\xEF\xBB\xBF\xEF\xBB\xBF<x/>", Formato::Json),
            (b" \xEF\xBB\xBF<x/>", Formato::Json),
            // Otros espacios de Unicode no cuentan.
            (b"\x0c<x/>", Formato::Json),
            (b"\xEF\xBB<x/>", Formato::Json),
            (b"<!ELEMENT a EMPTY>", Formato::Dtd),
            (
                b"<?xml version='1.0'?><!DOCTYPE x><xs:schema/>",
                Formato::Xsd,
            ),
        ] {
            assert_eq!(formato(bytes), esperado, "{bytes:?}");
        }
    }

    /// El tope deja escribir hasta `max` bytes exactos y falla en el
    /// siguiente, sin guardar lo que no cabe.
    #[test]
    fn tope_falla_al_pasar_el_maximo() {
        use std::fmt::Write;
        let mut t = Tope::new(5);
        assert!(t.write_str("abc").is_ok());
        assert!(t.write_str("de").is_ok());
        assert!(t.write_str("f").is_err());
        assert_eq!(t.texto, "abcde");
    }

    /// Un error interno de la IR da el mensaje fijo de D15 y un registro
    /// acotado: el recuento y los 8 primeros, sea cual sea su número. IR
    /// rota como la de `crates/cli` (100 `$ref` colgantes: 199 invariantes).
    #[test]
    fn error_interno_con_registro_acotado() {
        let sp = Span {
            start: Pos::START,
            end: 0,
        };
        let rules = (0..100)
            .map(|_| Constraint {
                rule: Rule::Ref(NodeId::new(usize::MAX)),
                span: sp,
            })
            .collect();
        let rota = Ir::new(
            Format::Json,
            Node {
                sort: Sort::Json,
                rules,
                origin: Origin {
                    span: sp,
                    parent: None,
                    step: Step::Root,
                },
            },
        );
        let (_, errores) = rota.verify().unwrap_err();
        let Compilado::Interno(lineas) = interno(&InternalError::new(errores)) else {
            panic!("no es interno");
        };
        assert_eq!(lineas[0], InternalError::MESSAGE);
        assert_eq!(
            lineas[1],
            "199 invariantes rotos; se muestran los 8 primeros"
        );
        assert_eq!(lineas.len(), 2 + MAX_DETALLE);
    }

    /// CR-S1: hasta que exista el crate `pipeline` (U2,
    /// `docs/fase7/pendientes.md` §6), la paridad con `schemac` se vigila
    /// sobre el **código fuente** de `crates/cli`. Desde D86 (E2) la
    /// detección de formato ya no es una copia: los dos llaman a
    /// `dtd::formato`, y la prueba comprueba que ninguno tenga su propia
    /// regla. Las cadenas de los resúmenes siguen siendo copias y tienen que
    /// coincidir. De los dos fuentes solo se mira la parte anterior a las
    /// pruebas. El de `crates/cli` se lee al correr la prueba, no con
    /// `include_str!` (CR2-S1): si se mueve, falla esta prueba y no la
    /// compilación de todas las de `server`.
    #[test]
    fn misma_tuberia_que_schemac() {
        let cli = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/cli/src/main.rs"
        ))
        .expect(
            "se movió `formato` de `schemac`: actualizar esta prueba o crear \
             `pipeline` (docs/fase7/pendientes.md §6)",
        );
        fn antes_de_las_pruebas(s: &str) -> &str {
            s.split("#[cfg(test)]").next().unwrap_or(s)
        }
        let cli = antes_de_las_pruebas(&cli);
        let servidor = antes_de_las_pruebas(include_str!("compilar.rs"));
        // Una sola regla de formato (D86, E2): la de `dtd::formato`.
        assert!(
            cli.contains("use dtd::{formato, Formato};"),
            "schemac no usa dtd::formato"
        );
        assert!(
            servidor.contains("match dtd::formato(bytes) {"),
            "el servidor no usa dtd::formato"
        );
        for (quien, fuente) in [("schemac", cli), ("servidor", servidor)] {
            assert!(
                !fuente.contains("strip_prefix(b\"\\xEF\\xBB\\xBF\")"),
                "{quien} vuelve a tener su propia detección de formato"
            );
        }
        fn resumenes(fuente: &str) -> Vec<&str> {
            fuente
                .lines()
                .map(str::trim)
                .filter(|l| {
                    (l.starts_with('"') && l.contains("esquema válido"))
                        || l.starts_with("{} subesquemas")
                })
                .collect()
        }
        let en_cli = resumenes(cli);
        // JSON (dos líneas), XSD y DTD.
        assert_eq!(en_cli.len(), 4, "{en_cli:?}");
        assert_eq!(resumenes(servidor), en_cli);
    }

    /// El mensaje se devuelve sin el prefijo de posición, que ya va en
    /// `line` y `column`.
    #[test]
    fn mensaje_sin_prefijo_de_posicion() {
        let Compilado::Esquema { errores, total, .. } = compilar(b"{\"type\": 1}", 1 << 20) else {
            panic!("debía ser un error del esquema");
        };
        assert_eq!(total, 1);
        assert!(!errores[0].message.starts_with("línea"), "{errores:?}");
        assert_eq!((errores[0].line, errores[0].column), (1, 10));
    }
}
