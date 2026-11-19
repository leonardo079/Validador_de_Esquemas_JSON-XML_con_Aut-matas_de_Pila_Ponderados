//! Línea base XML: libxml2 construye el árbol (DOM) y lo valida contra un
//! XSD 1.0 (`docs/fase6/linea-base.md` §7). Solo con la feature `libxml2`.
//!
//! libxml2 se configura para entrada no confiable (§7.2):
//!
//! - opciones del parser escritas campo a campo (`opciones`): `recover`
//!   apagado (un documento mal formado no da árbol), `no_net` encendido, sin
//!   `huge`, y sin `NOENT`, `DTDLOAD` ni `DTDVALID` (el crate `libxml` ni
//!   siquiera los expone); `no_error` y `no_warning` para que libxml2 no
//!   escriba en stderr: los diagnósticos se recogen como `StructuredError`
//!   (B8);
//! - **solo UTF-8** (`00-alcance.md` §1.2): libxml2 lee siempre UTF-8
//!   (`encoding: Some("UTF-8")`, `ignore_enc`) y no llega a cargar iconv; una
//!   entrada que empieza como UTF-16, UTF-32 o EBCDIC, o que declara un
//!   `encoding` distinto de UTF-8, es [`Kind::Malformed`] antes de llegar a
//!   libxml2 (`politica_de_entrada`). Así lo que ve libxml2 es compatible con
//!   ASCII y los recorridos de bytes propios (`doctype_en_el_prologo`,
//!   `profundidad_excedida`) son completos;
//! - **política D8 en dos capas (U6):** antes de dar los bytes a libxml2 se
//!   recorre el prólogo y un `<!DOCTYPE` es `Malformed` en la `D`, como el
//!   lexer del proyecto, así que libxml2 no ve la DTD ni declara entidades;
//!   después, si el árbol tiene aun así un nodo DTD (`tiene_dtd`), es
//!   `Malformed` en el offset 0. La segunda capa es defensa en profundidad
//!   probada (`leer_con_libxml2` sin la primera capa, en las pruebas de este
//!   módulo): era la única defensa ante UTF-16 o UTF-7 antes de la política
//!   de codificación;
//! - el XSD se lee con las mismas opciones y la misma política, y se rechaza
//!   `xs:import`, `xs:include`, `xs:redefine` y `xs:override` antes de
//!   compilarlo: libxml2 los cargaría de la red o del disco, y con
//!   `XML_PARSE_NOENT` (`SCHEMAS_PARSE_OPTIONS` de `xmlschemas.c`). Por eso
//!   tampoco se usa `SchemaParserContext::from_buffer`, que lee el XSD con
//!   esas opciones;
//! - el uso de libxml2 se serializa con un cerrojo (`LIBXML2`), también al
//!   liberar el árbol y el esquema.
//!
//! Profundidad: libxml2 sin `huge` acepta 257 elementos anidados y corta en
//! el 258; un recorrido propio de los bytes (`profundidad_excedida`,
//! iterativo) aplica [`Limits::max_depth`] (256) y sitúa el error en el `<`
//! del elemento que lo pasa.

use std::cell::RefCell;
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};

use libxml::error::{StructuredError, XmlErrorLevel};
use libxml::parser::{Parser, ParserOptions, XmlParseFailure, MAX_DIAGNOSTICS};
use libxml::readonly::RoNode;
use libxml::schemas::{SchemaParserContext, SchemaValidationContext};
use libxml::tree::{Document, NodeType};

use crate::json::acotado;
use crate::{CompileError, Kind, Limits, Malformed, Outcome, Stack, Violation};

/// Marca de orden de bytes UTF-8. libxml2 la reconoce sola; aquí solo
/// cuenta para las posiciones.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Espacio de nombres de XML Schema 1.0.
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema";

/// Elementos de XSD que hacen que libxml2 lea otro documento (de la red o
/// del disco). Están fuera del alcance del proyecto (`00-alcance.md` §3.3) y
/// la línea base no los compila, estén donde estén (también dentro de
/// `xs:appinfo`, lo que es más estricto que libxml2).
const CARGAN_OTRO_DOCUMENTO: [&str; 4] = ["import", "include", "redefine", "override"];

/// Comienzos de entrada que no son compatibles con ASCII (XML 1.0, apéndice
/// F): BOM de UTF-16 y UTF-32 y `<` o `<?xm` en UTF-16, UTF-32 y EBCDIC.
/// El proyecto solo acepta UTF-8 (`00-alcance.md` §1.2).
const NO_UTF8: [(&[u8], &str); 6] = [
    (b"\x00\x00\xFE\xFF", "UTF-32"),
    (b"\xFF\xFE", "UTF-16 o UTF-32"),
    (b"\xFE\xFF", "UTF-16"),
    (b"\x00\x3C", "UTF-16 o UTF-32"),
    (b"\x3C\x00", "UTF-16 o UTF-32"),
    (b"\x4C\x6F\xA7\x94", "EBCDIC"),
];

/// Serializa el uso de libxml2: su inicialización, los manejadores de error
/// y los tipos predefinidos de XSD son estado global. Lo toman `compile`,
/// `parse`, `validate_dom` y los `Drop` de [`XmlBaseline`] y [`XmlDom`]
/// (B8, B30). La libxml2 de Debian está compilada con hilos y el crate
/// `libxml` ya protege la inicialización; el cerrojo hace que la línea base
/// tampoco dependa de eso. En un servidor serializaría todas las peticiones
/// (`docs/fase6/pendientes.md` §14).
static LIBXML2: Mutex<()> = Mutex::new(());

fn cerrojo() -> MutexGuard<'static, ()> {
    // Un pánico con el cerrojo tomado no deja libxml2 a medias: cada
    // operación crea y libera su contexto.
    LIBXML2.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Opciones del parser para entrada no confiable, sin depender de
/// `ParserOptions::default()` (que trae `recover: true` y `no_net: false`).
/// `encoding` e `ignore_enc` fijan UTF-8 aunque el documento declare otra
/// codificación (SEC-M1): libxml2 no convierte nada.
fn opciones() -> ParserOptions<'static> {
    ParserOptions {
        recover: false,
        no_def_dtd: false,
        no_error: true,
        no_warning: true,
        pedantic: false,
        no_blanks: false,
        no_net: true,
        no_implied: false,
        huge: false,
        compact: false,
        ignore_enc: true,
        encoding: Some("UTF-8"),
    }
}

/// Un XSD compilado por libxml2 y los límites con que se leen los payloads.
///
/// No es `Send` ni `Sync` (los contextos de libxml2 son punteros de C): se
/// crea, se usa y se libera dentro del mismo hilo de [`crate::with_stack`],
/// y por eso [`compile`](Self::compile) pide el testigo [`Stack`] (D7).
///
/// ```compile_fail,E0277
/// fn send<T: Send>() {}
/// send::<baseline::XmlBaseline>();
/// ```
pub struct XmlBaseline {
    // `None` solo dentro de `drop`, que lo libera con el cerrojo tomado.
    esquema: Option<Esquema>,
    limits: Limits,
}

/// Lo que libera libxml2 al soltar un [`XmlBaseline`].
struct Esquema {
    // Se libera antes que el XSD del que salió (orden de los campos).
    contexto: RefCell<SchemaValidationContext>,
    _xsd: Document,
}

impl Drop for XmlBaseline {
    fn drop(&mut self) {
        let _libxml2 = cerrojo();
        self.esquema = None;
    }
}

impl fmt::Debug for XmlBaseline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XmlBaseline")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

/// El árbol de un payload XML ya leído (la primera de las dos pasadas).
///
/// Como [`crate::JsonDom`], no es `Send` ni `Sync`: solo existe en el hilo de
/// [`crate::with_stack`] que lo leyó. No da acceso al documento de libxml2,
/// para que nadie lo use sin el cerrojo (CR-S3).
///
/// ```compile_fail,E0277
/// fn send<T: Send>() {}
/// send::<baseline::XmlDom>();
/// ```
///
/// ```compile_fail,E0277
/// fn sync<T: Sync>() {}
/// sync::<baseline::XmlDom>();
/// ```
pub struct XmlDom {
    // `None` solo dentro de `drop`, que lo libera con el cerrojo tomado.
    doc: Option<Document>,
}

impl Drop for XmlDom {
    fn drop(&mut self) {
        let _libxml2 = cerrojo();
        self.doc = None;
    }
}

impl fmt::Debug for XmlDom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("XmlDom { .. }")
    }
}

impl XmlBaseline {
    /// Compila el XSD `xsd` (UTF-8). Antes de dárselo al compilador de
    /// esquemas de libxml2:
    ///
    /// - lo lee con las opciones seguras, la política de codificación y la
    ///   política D8 (un `<!DOCTYPE` es error de compilación), y con
    ///   [`crate::MAX_DEPTH_LIMIT`] de profundidad;
    /// - rechaza `xs:import`, `xs:include`, `xs:redefine` y `xs:override`,
    ///   sin abrir red ni archivos.
    ///
    /// libxml2 implementa XSD 1.0 completo, que es más que el subconjunto de
    /// `00-alcance.md` §3 (`docs/fase6/linea-base.md` §7.5). Su costo no está
    /// acotado ante un XSD hostil (`docs/fase6/pendientes.md` §14).
    pub fn compile(_stack: &Stack, xsd: &[u8]) -> Result<Self, CompileError> {
        let _libxml2 = cerrojo();
        let doc = leer(xsd, crate::MAX_DEPTH_LIMIT).map_err(|o| {
            let que = if o.kind == Some(Kind::Limit) {
                "el XSD supera un límite"
            } else {
                "el XSD no es XML bien formado"
            };
            CompileError {
                message: o.malformed.map_or_else(String::new, |m| {
                    acotado(format_args!(
                        "{que}: {} en la línea {}, columna {} (offset {:?})",
                        m.message, m.line, m.column, m.offset
                    ))
                }),
            }
        })?;
        if let Some(nombre) = carga_otro_documento(&doc) {
            return Err(CompileError {
                message: format!(
                    "xs:{nombre} no está permitido en la línea base: leería otro \
                     documento de la red o del disco"
                ),
            });
        }
        let mut parser = SchemaParserContext::from_document(&doc);
        let contexto =
            SchemaValidationContext::from_parser(&mut parser).map_err(|e| CompileError {
                message: primer_error(&e).map_or_else(
                    || "libxml2 no compiló el XSD y no dio un motivo".to_owned(),
                    mensaje,
                ),
            })?;
        Ok(XmlBaseline {
            esquema: Some(Esquema {
                contexto: RefCell::new(contexto),
                _xsd: doc,
            }),
            limits: Limits::default(),
        })
    }

    /// Sustituye los límites de los payloads. La profundidad se recorta a
    /// [`crate::MAX_DEPTH_LIMIT`].
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Primera pasada: lee `payload` entero a un árbol. `Err` es el
    /// resultado negativo ([`Kind::Malformed`] o [`Kind::Limit`]) con su
    /// posición.
    pub fn parse(&self, _stack: &Stack, payload: &[u8]) -> Result<XmlDom, Outcome> {
        let _libxml2 = cerrojo();
        leer(payload, self.limits.effective_max_depth()).map(|doc| XmlDom { doc: Some(doc) })
    }

    /// Segunda pasada: valida el árbol contra el XSD. Cada error de libxml2
    /// es una [`Violation`] (U7), como mucho
    /// [`Limits::effective_max_violations`]; si había más,
    /// [`Outcome::truncated`]. libxml2 no da la ruta del nodo: `pointer`
    /// queda vacío, `keyword` es `libxml2:<código>` y el mensaje empieza por
    /// la línea (`docs/fase6/linea-base.md` §7.4).
    ///
    /// Un error interno de libxml2 (código de retorno negativo, sin
    /// diagnóstico) también sale como [`Kind::Schema`], con la violación que
    /// añade el crate `libxml` («Schema validation failed with status …»,
    /// `keyword` `libxml2:1818`, `XML_SCHEMAV_INTERNAL`); no se ha visto con
    /// el corpus ni con las pruebas (CR-S6).
    pub fn validate_dom(&self, _stack: &Stack, dom: &XmlDom) -> Outcome {
        let _libxml2 = cerrojo();
        let (Some(esquema), Some(doc)) = (&self.esquema, &dom.doc) else {
            unreachable!("solo son `None` dentro de `drop`");
        };
        let mut contexto = esquema.contexto.borrow_mut();
        let resultado = contexto.validate_document(doc);
        // Los avisos de una validación correcta se quedan en el registro del
        // contexto: se vacía para que no crezca de una llamada a otra.
        let _ = contexto.drain_errors();
        let errores = match resultado {
            Ok(()) => return Outcome::conforme(),
            Err(e) => e,
        };
        let graves: Vec<&StructuredError> = errores.iter().filter(|e| es_error(e)).collect();
        let lista = if graves.is_empty() {
            errores.iter().collect()
        } else {
            graves
        };
        let max = self.limits.effective_max_violations();
        Outcome {
            valid: false,
            kind: Some(Kind::Schema),
            truncated: lista.len() > max,
            violations: lista
                .into_iter()
                .take(max)
                .map(|e| Violation {
                    pointer: String::new(),
                    keyword: format!("libxml2:{}", e.code),
                    message: mensaje(e),
                })
                .collect(),
            malformed: None,
        }
    }

    /// Las dos pasadas seguidas: [`parse`](Self::parse) y
    /// [`validate_dom`](Self::validate_dom).
    pub fn validate(&self, stack: &Stack, payload: &[u8]) -> Outcome {
        match self.parse(stack, payload) {
            Ok(dom) => self.validate_dom(stack, &dom),
            Err(o) => o,
        }
    }
}

/// Lee `bytes` a un documento: la política de entrada (UTF-8 y D8, primera
/// capa) y después libxml2. Se llama con el cerrojo tomado.
fn leer(bytes: &[u8], max_depth: u32) -> Result<Document, Outcome> {
    politica_de_entrada(bytes)?;
    leer_con_libxml2(bytes, max_depth)
}

/// Lo que se rechaza antes de dar los bytes a libxml2, en el orden en que
/// aparece en la entrada: una codificación que no es UTF-8 (`00-alcance.md`
/// §1.2) y un `<!DOCTYPE` en el prólogo (D8, primera capa).
fn politica_de_entrada(bytes: &[u8]) -> Result<(), Outcome> {
    if let Some((_, cual)) = NO_UTF8.iter().find(|(p, _)| bytes.starts_with(p)) {
        return Err(negativo(
            Kind::Malformed,
            bytes,
            0,
            format!("la entrada parece {cual}: solo se acepta UTF-8"),
        ));
    }
    if let Some((inicio, valor)) = encoding_declarado(bytes) {
        if !valor.eq_ignore_ascii_case(b"UTF-8") {
            let nombre: String = String::from_utf8_lossy(valor).chars().take(32).collect();
            return Err(negativo(
                Kind::Malformed,
                bytes,
                inicio,
                format!("encoding \"{nombre}\" no permitido: solo se acepta UTF-8"),
            ));
        }
    }
    if let Some(d) = doctype_en_el_prologo(bytes) {
        return Err(negativo(
            Kind::Malformed,
            bytes,
            d,
            "<!DOCTYPE no está permitido (D8: sin DTD ni entidades declaradas)".into(),
        ));
    }
    Ok(())
}

/// libxml2 con las opciones seguras, la segunda capa de D8 y el tope de
/// profundidad, sin la política de entrada (las pruebas de este módulo la
/// llaman directamente para ejercitar la segunda capa).
fn leer_con_libxml2(bytes: &[u8], max_depth: u32) -> Result<Document, Outcome> {
    let (doc, diagnosticos) =
        match Parser::default().parse_string_with_diagnostics(bytes, opciones()) {
            Ok((doc, d)) => (Some(doc), d),
            Err(XmlParseFailure::ParseFailed(d)) => (None, d),
            Err(e) => {
                // Entrada de más de `i32::MAX` bytes o sin memoria para el contexto.
                return Err(negativo(Kind::Limit, bytes, 0, acotado(&e)));
            }
        };
    // D8, segunda capa, antes que nada: un árbol con DTD no se mira más.
    if doc.as_ref().is_some_and(tiene_dtd) {
        return Err(negativo(
            Kind::Malformed,
            bytes,
            0,
            "el documento tiene una DTD (D8)".into(),
        ));
    }
    let error = if doc.is_none() {
        primer_error(&diagnosticos)
    } else {
        // Con `recover` apagado, libxml2 devuelve el árbol aunque haya errores
        // de espacios de nombres (prefijo sin declarar…): también son
        // malformados. Los avisos no cuentan.
        diagnosticos.iter().find(|e| es_error(e))
    };
    if doc.is_some() && error.is_none() && diagnosticos.len() >= MAX_DIAGNOSTICS {
        // SEC-L1: el crate guarda como mucho MAX_DIAGNOSTICS por lectura; si
        // se llenó con avisos, un error posterior quedaría fuera. Con
        // `no_warning` libxml2 no entrega los avisos y esto no se ha visto.
        return Err(negativo(
            Kind::Malformed,
            bytes,
            0,
            format!("libxml2 dio {MAX_DIAGNOSTICS} diagnósticos o más: un error pudo quedar fuera"),
        ));
    }
    let en_error = error.map(|e| offset_de(bytes, e));
    // Profundidad: solo hasta el error de libxml2, que es hasta donde los
    // bytes son XML bien formado.
    let hasta = en_error.unwrap_or(bytes.len());
    if let Some(p) = profundidad_excedida(bytes, hasta, max_depth) {
        return Err(negativo(
            Kind::Limit,
            bytes,
            p,
            format!("se supera la profundidad máxima de {max_depth} niveles"),
        ));
    }
    if let (Some(e), Some(off)) = (error, en_error) {
        let texto = texto(e);
        let kind = if es_limite(&texto) {
            Kind::Limit
        } else {
            Kind::Malformed
        };
        let sat = |n: Option<i32>| n.and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
        return Err(Outcome {
            valid: false,
            kind: Some(kind),
            violations: Vec::new(),
            truncated: false,
            malformed: Some(Malformed {
                line: sat(e.line),
                column: sat(e.col),
                offset: Some(off as u64),
                message: acotado(&texto),
            }),
        });
    }
    doc.ok_or_else(|| {
        // libxml2 no dio documento ni diagnóstico (no debería pasar).
        negativo(
            Kind::Malformed,
            bytes,
            bytes.len(),
            "libxml2 no leyó el documento y no dio un motivo".into(),
        )
    })
}

/// El offset del valor y el valor del pseudoatributo `encoding` de la
/// declaración `<?xml …?>`, que solo puede ir al principio (tras un BOM).
/// `None` si no hay declaración, no declara `encoding` o no se entiende (en
/// ese caso libxml2 da el error).
fn encoding_declarado(b: &[u8]) -> Option<(usize, &[u8])> {
    let mut i = if b.starts_with(BOM) { BOM.len() } else { 0 };
    let espacio = |c: &u8| matches!(c, b' ' | b'\t' | b'\r' | b'\n');
    if !b.get(i..)?.starts_with(b"<?xml") || !b.get(i + 5).is_some_and(espacio) {
        return None;
    }
    i += 5;
    loop {
        while b.get(i).is_some_and(espacio) {
            i += 1;
        }
        let inicio_nombre = i;
        while b.get(i).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
        }
        if i == inicio_nombre {
            return None; // `?>` u otra cosa
        }
        let nombre = &b[inicio_nombre..i];
        while b.get(i).is_some_and(espacio) {
            i += 1;
        }
        if b.get(i) != Some(&b'=') {
            return None;
        }
        i += 1;
        while b.get(i).is_some_and(espacio) {
            i += 1;
        }
        let comilla = *b.get(i).filter(|c| matches!(c, b'"' | b'\''))?;
        let inicio = i + 1;
        let fin = inicio + b.get(inicio..)?.iter().position(|c| *c == comilla)?;
        if nombre == b"encoding" {
            return Some((inicio, &b[inicio..fin]));
        }
        i = fin + 1;
    }
}

fn es_error(e: &StructuredError) -> bool {
    matches!(e.level, XmlErrorLevel::Error | XmlErrorLevel::Fatal)
}

/// El primer error (o fatal) de la lista; si solo hay avisos, el primero.
fn primer_error(d: &[StructuredError]) -> Option<&StructuredError> {
    d.iter().find(|e| es_error(e)).or_else(|| d.first())
}

/// El mensaje de libxml2 sin el salto de línea final.
fn texto(e: &StructuredError) -> String {
    e.message.as_deref().unwrap_or("").trim_end().to_owned()
}

/// El mensaje con la línea delante (libxml2 no da la ruta), recortado.
fn mensaje(e: &StructuredError) -> String {
    match e.line {
        Some(l) => acotado(format_args!("línea {l}: {}", texto(e))),
        None => acotado(texto(e)),
    }
}

/// Los límites propios de libxml2 2.9.14 sin `huge` son [`Kind::Limit`]: la
/// profundidad («Excessive depth»), un comentario, una sección CDATA o una
/// instrucción de procesamiento de más de 10 MB («… too big found»), un
/// valor de atributo de más de 10 MB («AttValue length too long») y un
/// nombre de más de 50 000 bytes («Name too long»). Se reconocen por el
/// mensaje, porque libxml2 no les da un código propio. Un nodo de texto de
/// 10 MB o más también es un límite, con uno de dos mensajes: «Huge input
/// lookup» (ASCII de una tirada o en líneas: 10 000 000 bytes de lookup) o
/// «xmlSAX2Characters: huge text node» (multibyte, referencias intercaladas o
/// dos CDATA seguidos). Un texto hecho solo de referencias (`&amp;` × 2 M) o
/// texto + CDATA no llega y es válido (`docs/fase6/pendientes.md` §24).
fn es_limite(texto: &str) -> bool {
    [
        "Excessive depth",
        "too big",
        "too long",
        "huge text node",
        "Huge input lookup",
    ]
    .iter()
    .any(|m| texto.contains(m))
}

fn negativo(kind: Kind, bytes: &[u8], offset: usize, message: String) -> Outcome {
    let (line, column) = linea_y_columna(bytes, offset);
    Outcome {
        valid: false,
        kind: Some(kind),
        violations: Vec::new(),
        truncated: false,
        malformed: Some(Malformed {
            line,
            column,
            offset: Some(offset as u64),
            message,
        }),
    }
}

/// ¿Hay un nodo DTD entre los hijos del documento?
fn tiene_dtd(doc: &Document) -> bool {
    let Some(raiz) = doc.get_root_readonly() else {
        return false;
    };
    let es_dtd = |n: RoNode| {
        matches!(
            n.get_type(),
            Some(NodeType::DTDNode | NodeType::DocumentTypeNode)
        )
    };
    let mut n = Some(raiz);
    while let Some(x) = n {
        if es_dtd(x) {
            return true;
        }
        n = x.get_prev_sibling();
    }
    let mut n = raiz.get_next_sibling();
    while let Some(x) = n {
        if es_dtd(x) {
            return true;
        }
        n = x.get_next_sibling();
    }
    false
}

/// El nombre local del primer `xs:import`, `xs:include`, `xs:redefine` o
/// `xs:override` del XSD, con un recorrido iterativo (D7).
fn carga_otro_documento(doc: &Document) -> Option<String> {
    let mut pendientes: Vec<RoNode> = doc.get_root_readonly().into_iter().collect();
    while let Some(n) = pendientes.pop() {
        if !n.is_element_node() {
            continue;
        }
        let nombre = n.get_name();
        let de_xsd = n.get_namespace().is_some_and(|ns| ns.get_href() == XSD_NS);
        if de_xsd && CARGAN_OTRO_DOCUMENTO.contains(&nombre.as_str()) {
            return Some(nombre);
        }
        pendientes.extend(n.get_child_nodes());
    }
    None
}

/// Índice del byte siguiente al primer `cierre` en `b[desde..]`.
fn tras(b: &[u8], desde: usize, cierre: &[u8]) -> Option<usize> {
    b.get(desde..)?
        .windows(cierre.len())
        .position(|w| w == cierre)
        .map(|i| desde + i + cierre.len())
}

/// Offset de la `D` de un `<!DOCTYPE` (en mayúsculas o minúsculas, como el
/// lexer del proyecto, D8) en el prólogo: tras un BOM, espacios,
/// instrucciones de procesamiento (la declaración `<?xml …?>` incluida) y
/// comentarios. Cualquier otra cosa termina el prólogo: un `<!DOCTYPE`
/// después del elemento raíz ya es un error de libxml2. Recorre bytes ASCII:
/// es completo porque `politica_de_entrada` ya rechazó toda entrada que no
/// sea UTF-8 y libxml2 lee siempre UTF-8 (`opciones`).
fn doctype_en_el_prologo(b: &[u8]) -> Option<usize> {
    let mut i = if b.starts_with(BOM) { BOM.len() } else { 0 };
    loop {
        while b
            .get(i)
            .is_some_and(|c| matches!(c, b' ' | b'\t' | b'\r' | b'\n'))
        {
            i += 1;
        }
        let resto = &b[i.min(b.len())..];
        if resto.starts_with(b"<?") {
            i = tras(b, i + 2, b"?>")?;
        } else if resto.starts_with(b"<!--") {
            i = tras(b, i + 4, b"-->")?;
        } else if resto.len() >= 9
            && resto.starts_with(b"<!")
            && resto[2..9].eq_ignore_ascii_case(b"DOCTYPE")
        {
            return Some(i + 2);
        } else {
            return None;
        }
    }
}

/// Offset del `<` del primer elemento que pasa `max_depth` elementos
/// abiertos a la vez (la raíz cuenta como 1), recorriendo `b[..hasta]` sin
/// recursión. Supone que esos bytes son XML bien formado (lo ha comprobado
/// libxml2): un `<` solo abre marcado, y dentro de una etiqueta un `>` solo
/// puede estar entre comillas. Como libxml2 lee siempre UTF-8 (`opciones`),
/// esos bytes son UTF-8 y el recorrido es completo; con UTF-16 contaría mal
/// los cierres (CR-I1), por eso `politica_de_entrada` lo rechaza antes.
fn profundidad_excedida(b: &[u8], hasta: usize, max_depth: u32) -> Option<usize> {
    let b = &b[..hasta.min(b.len())];
    let mut i = 0;
    let mut abiertos: u32 = 0;
    while let Some(p) = b.get(i..)?.iter().position(|c| *c == b'<').map(|p| i + p) {
        let resto = &b[p..];
        i = if resto.starts_with(b"<!--") {
            tras(b, p + 4, b"-->")?
        } else if resto.starts_with(b"<![CDATA[") {
            tras(b, p + 9, b"]]>")?
        } else if resto.starts_with(b"<?") {
            tras(b, p + 2, b"?>")?
        } else if resto.starts_with(b"<!") {
            tras(b, p + 2, b">")?
        } else if resto.starts_with(b"</") {
            abiertos = abiertos.saturating_sub(1);
            tras(b, p + 2, b">")?
        } else {
            abiertos += 1;
            if abiertos > max_depth {
                return Some(p);
            }
            // Fin de la etiqueta, saltando los valores de atributo.
            let mut j = p + 1;
            let mut comilla = None;
            loop {
                let c = *b.get(j)?;
                match comilla {
                    Some(q) if c == q => comilla = None,
                    Some(_) => {}
                    None if c == b'"' || c == b'\'' => comilla = Some(c),
                    None if c == b'>' => break,
                    None => {}
                }
                j += 1;
            }
            if b[j - 1] == b'/' {
                abiertos -= 1; // elemento vacío
            }
            j + 1
        };
    }
    None
}

/// Bytes que ocupa el carácter UTF-8 que empieza con `c` (1 si no es un
/// byte inicial válido).
fn ancho(c: u8) -> usize {
    match c {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

/// Offset del error de libxml2 a partir de su línea y su columna (desde 1;
/// libxml2 cuenta la columna en caracteres, no en bytes). En la línea 1 la
/// columna empieza después del BOM. Sin posición, 0.
fn offset_de(b: &[u8], e: &StructuredError) -> usize {
    let (Some(linea), col) = (e.line, e.col.unwrap_or(1)) else {
        return 0;
    };
    let linea = usize::try_from(linea).unwrap_or(1);
    let mut i = if linea <= 1 {
        if b.starts_with(BOM) {
            BOM.len()
        } else {
            0
        }
    } else {
        b.iter()
            .enumerate()
            .filter(|(_, c)| **c == b'\n')
            .nth(linea - 2)
            .map_or(b.len(), |(i, _)| i + 1)
    };
    for _ in 1..col.max(1) {
        match b.get(i) {
            None | Some(b'\n') => break,
            Some(c) => i += ancho(*c),
        }
    }
    i.min(b.len())
}

/// Línea y columna (desde 1; columna en caracteres, como libxml2) del byte
/// en `offset`. En la línea 1 el BOM no cuenta. Saturantes (D9).
fn linea_y_columna(b: &[u8], offset: usize) -> (u32, u32) {
    let antes = &b[..offset.min(b.len())];
    let linea = 1 + antes.iter().filter(|c| **c == b'\n').count();
    let inicio = match antes.iter().rposition(|c| *c == b'\n') {
        Some(i) => i + 1,
        None if antes.starts_with(BOM) => BOM.len(),
        None => 0,
    };
    let caracteres = antes[inicio.min(antes.len())..]
        .iter()
        .filter(|c| !(0x80..=0xBF).contains(*c))
        .count();
    let sat = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    (sat(linea), sat(caracteres + 1))
}

#[cfg(test)]
mod tests {
    //! Lo que las pruebas de `tests/xml.rs` no alcanzan desde la API: la
    //! segunda capa de D8 sin la primera, los textos reales de los límites de
    //! libxml2 2.9.14 y el lector de la declaración XML.
    use super::*;

    const SECRETO: &str = "file:///etc/passwd";

    /// La segunda capa de D8 (CR-I3): sin la política de entrada, libxml2 lee
    /// la DTD, y `tiene_dtd` la corta antes de mirar nada más, en el offset 0
    /// y sin el contenido del archivo.
    #[test]
    fn la_segunda_capa_de_d8_rechaza_un_arbol_con_dtd() {
        let muchos: String = "<b/>".repeat(300);
        let payloads = [
            format!("<!DOCTYPE a [<!ENTITY x SYSTEM \"{SECRETO}\">]><a>&x;</a>"),
            "<!DOCTYPE a [<!ENTITY x \"y\">]><a>&x;</a>".to_owned(),
            format!("<!DOCTYPE a SYSTEM \"{SECRETO}\"><a/>"),
            // Con más de 256 etiquetas: la DTD gana a la profundidad.
            format!("<!DOCTYPE a [<!ENTITY x \"y\">]><a>{muchos}</a>"),
        ];
        crate::with_stack(|_| {
            let _libxml2 = cerrojo();
            for p in &payloads {
                assert!(politica_de_entrada(p.as_bytes()).is_err(), "{p}");
                let o = leer_con_libxml2(p.as_bytes(), 1)
                    .err()
                    .unwrap_or_else(|| panic!("{p}: la segunda capa no lo cortó"));
                assert_eq!(o.kind, Some(Kind::Malformed), "{p}: {o:?}");
                let m = o.malformed.as_ref().unwrap();
                assert_eq!(m.offset, Some(0), "{p}: {m:?}");
                assert!(m.message.contains("DTD (D8)"), "{p}: {m:?}");
                assert!(!format!("{o:?}").contains("root:"), "{p}: {o:?}");
            }
            let doc = leer_con_libxml2(b"<!-- x --><a/>", 256).unwrap();
            assert!(!tiene_dtd(&doc));
        });
    }

    /// CR-I2: los textos reales de libxml2 2.9.14 (`parser.c`) que son
    /// límites, y algunos que no.
    #[test]
    fn es_limite_con_los_mensajes_de_libxml2() {
        for limite in [
            "Excessive depth in document: 256 use XML_PARSE_HUGE option",
            "Comment too big found",
            "CData section too big found",
            "PI p too big found",
            "AttValue length too long",
            "Name too long use XML_PARSE_HUGE option",
            "xmlSAX2Characters: huge text node",
            "Huge input lookup",
        ] {
            assert!(es_limite(limite), "{limite}");
        }
        for no in [
            "Opening and ending tag mismatch: b line 1 and a",
            "Premature end of data in tag a line 1",
            "Entity 'lol' not defined",
            "Document is empty",
        ] {
            assert!(!es_limite(no), "{no}");
        }
    }

    /// (entrada, offset y valor de `encoding` esperados).
    type CasoEncoding = (&'static [u8], Option<(usize, &'static [u8])>);

    #[test]
    fn encoding_declarado_lee_el_pseudoatributo() {
        let casos: [CasoEncoding; 7] = [
            (
                b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a/>",
                Some((30, b"UTF-8")),
            ),
            (
                b"<?xml version='1.0' encoding = 'latin1' ?>",
                Some((32, b"latin1")),
            ),
            (
                b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"x\"?>",
                Some((33, b"x")),
            ),
            (b"<?xml version=\"1.0\"?><a/>", None),
            (b"<?xml-stylesheet encoding=\"x\"?><a/>", None),
            (b"<a encoding=\"x\"/>", None),
            (b"<?xml version=\"1.0\" encoding=\"x", None),
        ];
        for (b, esperado) in casos {
            assert_eq!(
                encoding_declarado(b),
                esperado,
                "{}",
                String::from_utf8_lossy(b)
            );
        }
    }
}
