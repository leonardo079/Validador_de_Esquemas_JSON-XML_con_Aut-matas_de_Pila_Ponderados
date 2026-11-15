//! Tipos simples del usuario (tarea 5.6, `docs/fase3/xsd.md` §10):
//! derivación circular, facetas (I4, I13, *valid restriction*),
//! enumeraciones y `default`/`fixed` como valores del tipo, y la bajada de
//! `docs/fase3/ir.md` §5.2 con sus orígenes.
//!
//! Los errores con línea y columna de cada obligación están en el corpus
//! (`tests/schemas/xsd/semantic-error`, manifiesto
//! `errores-semanticos-xsd.json`, prueba `errores_semanticos_con_linea_y_columna`
//! de `lower.rs`); aquí van la IR de `parse-ok/tipos-simples.xsd`, la cadena
//! de 10⁵ restricciones, las tablas de casos aceptados y rechazados y el
//! orden de los nodos (B3).

use std::path::Path;

use ir::{Lexical, LitToken, Literal, NodeId, Num, Rule, Sort, VerifiedIr};
use xsd::{analyze, lower, parse, XsdError};

const TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests");
const XS: &str = "xmlns:xs='http://www.w3.org/2001/XMLSchema'";

/// Todo el frontal; un error interno hace fallar la prueba.
fn compilar(input: &str) -> Result<VerifiedIr, Vec<XsdError>> {
    let schema = parse(input.as_bytes()).map_err(|e| vec![e])?;
    let analysis = analyze(&schema)?;
    Ok(lower(&analysis).unwrap_or_else(|e| {
        panic!(
            "error interno:\n{}\n{input}",
            e.detail(8).collect::<Vec<_>>().join("\n")
        )
    }))
}

fn ir_de(input: &str) -> VerifiedIr {
    compilar(input).unwrap_or_else(|e| {
        panic!(
            "{}\n{input}",
            e.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

/// Códigos de los errores del análisis (los mensajes, de una línea).
fn codigos(input: &str) -> Vec<&'static str> {
    let errs = compilar(input)
        .err()
        .unwrap_or_else(|| panic!("acepta:\n{input}"));
    for e in &errs {
        assert!(!e.to_string().contains('\n'));
    }
    errs.iter().map(XsdError::code).collect()
}

/// Un esquema con los tipos simples `tipos` y un elemento `r` de tipo `R`.
fn esquema(tipos: &str) -> String {
    format!("<xs:schema {XS}>{tipos}<xs:element name='r' type='R'/></xs:schema>")
}

/// `R` restringe `base` con `facetas`.
fn restriccion(base: &str, facetas: &str) -> String {
    format!(
        "<xs:simpleType name='R'><xs:restriction base='{base}'>{facetas}</xs:restriction>\
         </xs:simpleType>"
    )
}

fn ubicacion(v: &VerifiedIr, s: &str) -> NodeId {
    (0..v.nodes().len())
        .map(NodeId::new)
        .find(|&i| v.location(i) == s)
        .unwrap_or_else(|| panic!("no hay {s}"))
}

fn reglas(v: &VerifiedIr, s: &str) -> Vec<Rule> {
    v.node(ubicacion(v, s))
        .rules
        .iter()
        .map(|c| c.rule.clone())
        .collect()
}

fn num(s: &str) -> Num {
    Num::new(s)
}

/// Criterio 1: `parse-ok/tipos-simples.xsd` (con nombre y anónimos, cadena
/// de 3 restricciones, enumeraciones, rangos, longitudes y `pattern`) baja a
/// una IR que pasa `verify`, con las ubicaciones de `ir.md` §5.2, `Lexical`
/// del primitivo, `Ref` a la base que no es primitiva y las facetas
/// reescritas por `ir::num`.
#[test]
fn tipos_simples_baja_con_las_ubicaciones_de_5_2() {
    let texto = std::fs::read_to_string(
        Path::new(TESTS_DIR).join("schemas/xsd/parse-ok/tipos-simples.xsd"),
    )
    .unwrap();
    let v = ir_de(&texto);
    let orden: Vec<String> = (0..v.nodes().len())
        .map(|i| v.location(NodeId::new(i)))
        .collect();
    // B3: documento, declaraciones en preorden (los tipos con nombre, los
    // anónimos detrás de quien los contiene) y sintéticos por primer uso.
    assert_eq!(
        orden,
        [
            "#",
            "#/simpleType/TallaCorta",
            "#/simpleType/Talla",
            "#/simpleType/Porcentaje",
            "#/simpleType/Abierto",
            "#/simpleType/PocoPorcentaje",
            "#/simpleType/PocoPorcentaje/restriction",
            "#/simpleType/Peso",
            "#/element/talla",
            "#/element/talla/simpleType",
            "#/element/prenda",
            "#/element/prenda/attribute/color",
            "#/builtin/int",
            "#/simpleContent/PocoPorcentaje",
            "#/simpleContent/Peso",
            "#/builtin/anySimpleType",
            "#/builtin/boolean",
        ]
    );
    let id = |s| ubicacion(&v, s);
    for (s, sort) in [
        ("#/simpleType/Talla", Sort::Text),
        ("#/element/talla", Sort::Element),
        ("#/element/talla/simpleType", Sort::Text),
        ("#/simpleContent/Peso", Sort::Element),
    ] {
        assert_eq!(v.node(id(s)).sort, sort, "{s}");
    }
    let s = |v: &str| Literal(vec![LitToken::String(v.into())]);
    let n = |v: &str| Literal(vec![LitToken::Number(v.into())]);
    assert_eq!(
        reglas(&v, "#/simpleType/TallaCorta"),
        [
            Rule::Lexical(Lexical::String),
            Rule::MinLength(1),
            Rule::MaxLength(3)
        ]
    );
    // `length` = MinLength + MaxLength; las enumeraciones de xs:string sin
    // tocar (" M ", *preserve*); dos `pattern` unidas con `|`.
    assert_eq!(
        reglas(&v, "#/simpleType/Talla"),
        [
            Rule::Lexical(Lexical::String),
            Rule::Ref(id("#/simpleType/TallaCorta")),
            Rule::Enum(vec![s("S"), s(" M "), s("L")]),
            Rule::MinLength(1),
            Rule::MaxLength(1),
            Rule::Pattern("([SML])|(X+L)".into()),
        ]
    );
    // `100.0` reescrito a `100`; sin `Ref` a xs:decimal, que es primitivo.
    assert_eq!(
        reglas(&v, "#/simpleType/Porcentaje"),
        [
            Rule::Lexical(Lexical::Decimal),
            Rule::Minimum(num("0")),
            Rule::Maximum(num("100"))
        ]
    );
    // xs:int no es primitivo: `Ref` a su predefinido; `+10` → `10`.
    assert_eq!(
        reglas(&v, "#/simpleType/Abierto"),
        [
            Rule::Lexical(Lexical::Integer),
            Rule::Ref(id("#/builtin/int")),
            Rule::ExclusiveMinimum(num("-1")),
            Rule::ExclusiveMaximum(num("10")),
        ]
    );
    // La cadena de 3: PocoPorcentaje → anónimo → Porcentaje.
    assert_eq!(
        reglas(&v, "#/simpleType/PocoPorcentaje"),
        [
            Rule::Lexical(Lexical::Decimal),
            Rule::Ref(id("#/simpleType/PocoPorcentaje/restriction")),
            Rule::Maximum(num("10")),
        ]
    );
    assert_eq!(
        reglas(&v, "#/simpleType/PocoPorcentaje/restriction"),
        [
            Rule::Lexical(Lexical::Decimal),
            Rule::Ref(id("#/simpleType/Porcentaje")),
            Rule::Enum(vec![n("0"), n("5.5"), n("10"), n("50")]),
        ]
    );
    // xs:float: el decimal más corto del valor redondeado (`1e3`).
    assert_eq!(
        reglas(&v, "#/simpleType/Peso"),
        [
            Rule::Lexical(Lexical::Float),
            Rule::ExclusiveMinimum(num("0")),
            Rule::Maximum(num("1e3")),
        ]
    );
    // Tipo anónimo de un elemento: envoltorio (declaración) + texto.
    assert_eq!(
        reglas(&v, "#/element/talla"),
        [Rule::SimpleContent(id("#/element/talla/simpleType"))]
    );
    assert_eq!(
        reglas(&v, "#/element/talla/simpleType"),
        [
            Rule::Lexical(Lexical::String),
            Rule::Ref(id("#/simpleType/Talla"))
        ]
    );
    // Un tipo con nombre usado por dos elementos: un envoltorio compartido.
    assert_eq!(
        reglas(&v, "#/simpleContent/PocoPorcentaje"),
        [Rule::SimpleContent(id("#/simpleType/PocoPorcentaje"))]
    );
    let prenda = reglas(&v, "#/element/prenda");
    let Rule::Content(m) = &prenda[0] else {
        panic!("Content");
    };
    let tys: Vec<NodeId> = m
        .particles
        .iter()
        .filter_map(|p| match &p.term {
            ir::Term::Element { ty, .. } => Some(*ty),
            _ => None,
        })
        .collect();
    let poco = id("#/simpleContent/PocoPorcentaje");
    assert_eq!(tys, [poco, id("#/simpleContent/Peso"), poco]);
    let Rule::Attributes(attrs) = &prenda[1] else {
        panic!("Attributes");
    };
    let at: Vec<(&str, NodeId, Option<&str>, Option<&str>)> = attrs
        .iter()
        .map(|a| {
            (
                a.name.as_str(),
                a.ty,
                a.default.as_deref(),
                a.fixed.as_deref(),
            )
        })
        .collect();
    // `default`/`fixed` llegan tal como los dejó el lector.
    assert_eq!(
        at,
        [
            ("color", id("#/element/prenda/attribute/color"), None, None),
            (
                "nota",
                id("#/builtin/anySimpleType"),
                None,
                Some("  sin cambios  ")
            ),
            ("uso", id("#/builtin/boolean"), Some(" 1 "), None),
            ("abierto", id("#/simpleType/Abierto"), Some("+09"), None),
        ]
    );
    // Rangos: cada texto, el de su xs:simpleType; cada regla, el de su faceta.
    let linea = |s: &str| texto[..texto.find(s).unwrap()].matches('\n').count() as u32 + 1;
    let talla = v.node(id("#/simpleType/Talla"));
    assert_eq!(
        talla.origin.span.start.line,
        linea("<xs:simpleType name=\"Talla\"")
    );
    assert_eq!(
        talla.rules[3].span.start.line,
        linea("<xs:length value=\"1\"/>")
    );
    assert_eq!(
        talla.rules[5].span.start.line,
        linea("<xs:pattern value=\"[SML]\"/>")
    );
    assert_eq!(
        talla.rules[5].span.end,
        texto.find("<xs:pattern value=\"X+L\"/>").unwrap() as u64 + 25
    );
}

/// Criterio (D7, D15): una cadena de 10⁵ restricciones con un ciclo al final
/// se rechaza con **un** error `cycle` en su posición, sin desbordar la
/// pila: el análisis corre en un hilo de 64 KiB (el parser y la liberación
/// del AST, en uno de 8 MiB). La misma cadena sin ciclo baja a una IR
/// verificada con un texto por restricción.
#[test]
fn cadena_de_cien_mil_restricciones_con_ciclo_al_final() {
    const N: usize = 100_000;
    let cadena = |ciclo: bool| {
        let mut doc = format!("<xs:schema {XS}>");
        doc.reserve(N * 90);
        for i in 0..N {
            let base = if i + 1 < N {
                format!("T{}", i + 1)
            } else if ciclo {
                // El último vuelve a T99990: un ciclo de 10 al final.
                format!("T{}", N - 10)
            } else {
                "xs:string".to_owned()
            };
            doc.push_str(&format!(
                "<xs:simpleType name='T{i}'><xs:restriction base='{base}'/></xs:simpleType>\n"
            ));
        }
        doc.push_str("<xs:element name='r' type='T0'/></xs:schema>");
        doc
    };
    let correr = |doc: String| {
        std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                let schema = parse(doc.as_bytes()).unwrap();
                let (schema, r) = std::thread::Builder::new()
                    .stack_size(64 << 10)
                    .spawn(move || {
                        let r = match analyze(&schema) {
                            Err(e) => Err(e
                                .iter()
                                .map(|e| (e.code(), e.line(), e.column()))
                                .collect::<Vec<_>>()),
                            Ok(a) => {
                                let v = lower(&a).unwrap_or_else(|_| panic!("interno"));
                                Ok(v.nodes().len())
                            }
                        };
                        (schema, r)
                    })
                    .unwrap()
                    .join()
                    .unwrap();
                drop(schema);
                r
            })
            .unwrap()
            .join()
            .unwrap()
    };
    // Un solo error, en el primer `base` del ciclo por posición: el de
    // T99990 (línea 1 + 99990), en la columna de su valor.
    let col = "<xs:simpleType name='T99990'><xs:restriction base='".len() as u32 + 1;
    assert_eq!(
        correr(cadena(true)),
        Err(vec![("cycle", (N - 10 + 1) as u32, col)])
    );
    // Documento, N textos y el envoltorio de T0 (sin `builtin/string`: una
    // restricción de un primitivo no lleva `Ref`).
    assert_eq!(correr(cadena(false)), Ok(1 + N + 1));
}

/// Costo O(n) (`xsd.md` §10): una cota de 1 MiB heredada por 10⁴
/// restricciones no se copia por la cadena (las facetas efectivas apuntan a
/// quien las declara); cada derivado se compara una vez con ella.
#[test]
fn cota_larga_heredada_por_una_cadena() {
    const K: usize = 10_000;
    let cota = format!("1{}", "0".repeat(1 << 20));
    let mut tipos = format!(
        "<xs:simpleType name='T0'><xs:restriction base='xs:integer'>\
         <xs:maxInclusive value='{cota}'/></xs:restriction></xs:simpleType>"
    );
    for i in 1..K {
        tipos.push_str(&format!(
            "<xs:simpleType name='T{i}'><xs:restriction base='T{}'/></xs:simpleType>",
            i - 1
        ));
    }
    tipos.push_str(&format!(
        "<xs:simpleType name='R'><xs:restriction base='T{}'>\
         <xs:maxInclusive value='5'/></xs:restriction></xs:simpleType>",
        K - 1
    ));
    let v = ir_de(&esquema(&tipos));
    assert_eq!(v.nodes().len(), 1 + (K + 1) + 1);
    assert_eq!(
        reglas(&v, "#/simpleType/R").last(),
        Some(&Rule::Maximum(num("5")))
    );
}

/// *Facet valid restriction* y coherencia de las facetas (5.5-P2): casos
/// aceptados y rechazados, con su código. Las cotas inclusivas no salen del
/// espacio de valores de la base; las exclusivas no aflojan la de su lado
/// y quedan estrictamente dentro por el otro; las longitudes, igual.
#[test]
fn restriccion_valida_respecto_de_la_base() {
    let base_dec = "<xs:simpleType name='B'><xs:restriction base='xs:decimal'>\
                    <xs:minInclusive value='0'/><xs:maxExclusive value='10'/>\
                    </xs:restriction></xs:simpleType>";
    let base_str = "<xs:simpleType name='B'><xs:restriction base='xs:string'>\
                    <xs:minLength value='2'/><xs:maxLength value='5'/>\
                    </xs:restriction></xs:simpleType>";
    let base_len = "<xs:simpleType name='B'><xs:restriction base='xs:string'>\
                    <xs:length value='3'/></xs:restriction></xs:simpleType>";
    let ok: &[(&str, &str, &str)] = &[
        (base_dec, "B", "<xs:minInclusive value='0'/>"),
        (base_dec, "B", "<xs:minExclusive value='0'/>"),
        (base_dec, "B", "<xs:maxInclusive value='9.99'/>"),
        (base_dec, "B", "<xs:maxExclusive value='10'/>"),
        (
            base_dec,
            "B",
            "<xs:minInclusive value='3'/><xs:maxInclusive value='3'/>",
        ),
        (base_str, "B", "<xs:length value='4'/>"),
        (base_str, "B", "<xs:minLength value='5'/>"),
        (base_str, "B", "<xs:maxLength value='2'/>"),
        (base_len, "B", "<xs:length value='3'/>"),
        ("", "xs:byte", "<xs:minInclusive value='-128'/>"),
        ("", "xs:byte", "<xs:maxExclusive value='127'/>"),
        ("", "xs:positiveInteger", "<xs:minExclusive value='1'/>"),
        ("", "xs:float", "<xs:maxInclusive value='3.4028235e38'/>"),
        ("", "xs:double", "<xs:minInclusive value='-1e308'/>"),
    ];
    for (b, base, f) in ok {
        ir_de(&esquema(&format!("{b}{}", restriccion(base, f))));
    }
    let err: &[(&str, &str, &str)] = &[
        (base_dec, "B", "<xs:minInclusive value='-1'/>"),
        (base_dec, "B", "<xs:minExclusive value='-0.5'/>"),
        (base_dec, "B", "<xs:maxInclusive value='10'/>"),
        (base_dec, "B", "<xs:maxExclusive value='10.5'/>"),
        (base_dec, "B", "<xs:minExclusive value='10'/>"),
        (base_dec, "B", "<xs:maxExclusive value='0'/>"),
        (
            base_dec,
            "B",
            "<xs:minExclusive value='3'/><xs:maxInclusive value='3'/>",
        ),
        (base_str, "B", "<xs:length value='6'/>"),
        (base_str, "B", "<xs:minLength value='1'/>"),
        (base_str, "B", "<xs:maxLength value='6'/>"),
        (base_str, "B", "<xs:minLength value='6'/>"),
        (base_len, "B", "<xs:length value='4'/>"),
        // Con `length` en la base no se añade `minLength`/`maxLength`
        // (XSD 1.0 2.ª ed., *length and minLength or maxLength* 1.2).
        (base_len, "B", "<xs:minLength value='1'/>"),
        ("", "xs:byte", "<xs:minInclusive value='-129'/>"),
        ("", "xs:short", "<xs:maxInclusive value='40000'/>"),
        ("", "xs:nonNegativeInteger", "<xs:maxExclusive value='0'/>"),
        (
            "",
            "xs:string",
            "<xs:minLength value='3'/><xs:maxLength value='2'/>",
        ),
    ];
    for (b, base, f) in err {
        let doc = esquema(&format!("{b}{}", restriccion(base, f)));
        assert_eq!(codigos(&doc), ["facet"], "{base} {f}");
    }
}

/// Enumeraciones (forma léxica del primitivo y espacio de valores de la
/// base, también los rangos de los predefinidos y su enumeración) y
/// `default`/`fixed` como valores del tipo (5.5-P1). `pattern` no se
/// comprueba (D29).
#[test]
fn enumeraciones_y_valores_por_defecto_son_del_tipo() {
    let enumera = |base: &str, valores: &[&str]| {
        let f: String = valores
            .iter()
            .map(|v| format!("<xs:enumeration value='{v}'/>"))
            .collect();
        esquema(&restriccion(base, &f))
    };
    // Aceptadas, con su valor normalizado en la IR.
    for (base, valores, esperado) in [
        ("xs:integer", &["+1", "007", "-0"][..], &["1", "7", "0"][..]),
        ("xs:byte", &["-128", "127"][..], &["-128", "127"][..]),
        ("xs:decimal", &[" .50 ", "1."][..], &["0.5", "1"][..]),
        (
            "xs:float",
            &["0.1", "16777217"][..],
            &["0.1", "16777216"][..],
        ),
        // xs:string conserva los espacios y no se compara con `pattern`.
        ("xs:string", &[" a ", ""][..], &[" a ", ""][..]),
    ] {
        let v = ir_de(&enumera(base, valores));
        let Some(Rule::Enum(lits)) = reglas(&v, "#/simpleType/R")
            .into_iter()
            .find(|r| matches!(r, Rule::Enum(_)))
        else {
            panic!("{base}");
        };
        let got: Vec<String> = lits
            .iter()
            .map(|l| match l.0.as_slice() {
                [LitToken::Number(n) | LitToken::String(n)] => n.clone(),
                t => panic!("{t:?}"),
            })
            .collect();
        assert_eq!(got, esperado, "{base}");
    }
    let con_patron = esquema(&format!(
        "<xs:simpleType name='B'><xs:restriction base='xs:string'><xs:pattern value='[a-z]'/>\
         </xs:restriction></xs:simpleType>{}",
        restriccion("B", "<xs:enumeration value='ZZZ'/>")
    ));
    ir_de(&con_patron);
    // Rechazadas, todas `value`.
    for (base, valores) in [
        ("xs:integer", &["1.0"][..]),
        ("xs:integer", &["1e2"][..]),
        ("xs:byte", &["300"][..]),
        ("xs:positiveInteger", &["0"][..]),
        ("xs:decimal", &["abc"][..]),
        ("xs:double", &["NaN"][..]),
        ("xs:double", &["-INF"][..]),
        ("xs:float", &["1e39"][..]),
    ] {
        assert_eq!(
            codigos(&enumera(base, valores)),
            ["value"],
            "{base} {valores:?}"
        );
    }
    // `default`/`fixed`.
    let atributo = |ty: &str, extra: &str| {
        format!(
            "<xs:schema {XS}><xs:simpleType name='P'><xs:restriction base='xs:decimal'>\
             <xs:minInclusive value='0'/><xs:maxInclusive value='100'/>\
             </xs:restriction></xs:simpleType>\
             <xs:simpleType name='C'><xs:restriction base='xs:string'>\
             <xs:enumeration value='a'/><xs:maxLength value='1'/>\
             </xs:restriction></xs:simpleType>\
             <xs:simpleType name='F'><xs:restriction base='xs:float'>\
             <xs:minInclusive value='0'/></xs:restriction></xs:simpleType>\
             <xs:element name='r'><xs:complexType>\
             <xs:attribute name='a' type='{ty}' {extra}/>\
             </xs:complexType></xs:element></xs:schema>"
        )
    };
    for (ty, extra) in [
        ("P", "default=' 50.0 '"),
        ("P", "fixed='100'"),
        ("C", "fixed='a'"),
        ("xs:boolean", "default='0'"),
        ("xs:int", "default='-2147483648'"),
        ("xs:string", "default='  '"),
        // `INF` es un valor de xs:float (D24 solo rige facetas y
        // enumeraciones) y cumple `minInclusive`.
        ("F", "default='INF'"),
        ("xs:double", "fixed='NaN'"),
    ] {
        ir_de(&atributo(ty, extra));
    }
    for (ty, extra) in [
        ("P", "default='-1'"),
        ("P", "fixed='x'"),
        ("C", "fixed='b'"),
        ("xs:boolean", "default='verdadero'"),
        ("xs:byte", "fixed='128'"),
        ("F", "default='-INF'"),
        ("F", "default='NaN'"),
        ("xs:float", "fixed='+INF'"),
        ("xs:float", "fixed='1e39'"),
    ] {
        assert_eq!(codigos(&atributo(ty, extra)), ["value"], "{ty} {extra}");
    }
}

/// Un tipo con errores no arrastra a sus derivados ni a los atributos que
/// lo usan (serían consecuencias); los errores de tipos independientes
/// salen todos, en orden del documento (B2).
#[test]
fn errores_sin_consecuencias_y_en_orden() {
    let doc = format!(
        "<xs:schema {XS}>\
         <xs:simpleType name='A'><xs:restriction base='xs:int'>\
           <xs:minLength value='1'/></xs:restriction></xs:simpleType>\
         <xs:simpleType name='B'><xs:restriction base='A'>\
           <xs:enumeration value='x'/></xs:restriction></xs:simpleType>\
         <xs:simpleType name='C'><xs:restriction base='xs:string'>\
           <xs:maxInclusive value='1'/></xs:restriction></xs:simpleType>\
         <xs:element name='r'><xs:complexType>\
           <xs:attribute name='a' type='B' default='no'/></xs:complexType></xs:element>\
         </xs:schema>"
    );
    let schema = parse(doc.as_bytes()).unwrap();
    let errs = analyze(&schema).unwrap_err();
    let got: Vec<(&str, u32)> = errs.iter().map(|e| (e.code(), e.column())).collect();
    let col = |s: &str| doc.find(s).unwrap() as u32 + 1;
    assert_eq!(
        got,
        [
            ("facet", col("<xs:minLength")),
            ("facet", col("<xs:maxInclusive"))
        ]
    );
}

/// Los mensajes de tipos simples recortan los valores y nombres del usuario
/// y son de una línea.
#[test]
fn errores_de_tipos_simples_se_recortan() {
    let largo = "9".repeat(1 << 20);
    let nombre = "n".repeat(1 << 20);
    for tipos in [
        restriccion("xs:byte", &format!("<xs:enumeration value='{largo}'/>")),
        restriccion("xs:byte", &format!("<xs:maxInclusive value='{largo}'/>")),
        format!(
            "<xs:simpleType name='{nombre}'><xs:restriction base='{nombre}'/></xs:simpleType>\
             <xs:simpleType name='R'><xs:restriction base='xs:string'/></xs:simpleType>"
        ),
        format!(
            "<xs:simpleType name='{nombre}'><xs:restriction base='xs:string'>\
             <xs:enumeration value='{largo}'/></xs:restriction></xs:simpleType>{}",
            restriccion(&nombre, "<xs:enumeration value='a'/>")
        ),
    ] {
        let doc = esquema(&tipos);
        let schema = parse(doc.as_bytes()).unwrap();
        let errs = analyze(&schema).unwrap_err();
        assert_eq!(errs.len(), 1);
        let msg = errs[0].to_string();
        assert!(msg.len() < 400 && !msg.contains('\n'), "{}", &msg[..200]);
    }
}

/// `xs:simpleContent` + `xs:extension` sobre un tipo simple del usuario, un
/// tipo simple de atributo usado en `xs:extension` y la base anónima de un
/// tipo anónimo (orígenes de `ir.md` §5.2).
#[test]
fn origenes_de_los_tipos_anonimos() {
    let doc = format!(
        "<xs:schema {XS}>\
         <xs:complexType name='Precio'><xs:simpleContent><xs:extension base='Monto'>\
           <xs:attribute name='moneda'><xs:simpleType><xs:restriction base='xs:string'>\
             <xs:length value='3'/></xs:restriction></xs:simpleType></xs:attribute>\
         </xs:extension></xs:simpleContent></xs:complexType>\
         <xs:simpleType name='Monto'><xs:restriction base='xs:decimal'>\
           <xs:minInclusive value='0'/></xs:restriction></xs:simpleType>\
         <xs:element name='pedido'><xs:complexType><xs:sequence>\
           <xs:element name='precio' type='Precio'/>\
           <xs:element name='nota'><xs:simpleType><xs:restriction>\
             <xs:simpleType><xs:restriction base='xs:string'><xs:maxLength value='9'/>\
             </xs:restriction></xs:simpleType></xs:restriction></xs:simpleType></xs:element>\
         </xs:sequence></xs:complexType></xs:element>\
         </xs:schema>"
    );
    let v = ir_de(&doc);
    let orden: Vec<String> = (0..v.nodes().len())
        .map(|i| v.location(NodeId::new(i)))
        .collect();
    assert_eq!(
        orden,
        [
            "#",
            "#/complexType/Precio",
            "#/complexType/Precio/attribute/moneda",
            "#/simpleType/Monto",
            "#/element/pedido",
            "#/element/pedido/element/nota",
            "#/element/pedido/element/nota/simpleType",
            "#/element/pedido/element/nota/simpleType/restriction",
        ]
    );
    let id = |s| ubicacion(&v, s);
    assert_eq!(
        reglas(&v, "#/complexType/Precio")[0],
        Rule::SimpleContent(id("#/simpleType/Monto"))
    );
    assert_eq!(
        reglas(&v, "#/element/pedido/element/nota/simpleType"),
        [
            Rule::Lexical(Lexical::String),
            Rule::Ref(id("#/element/pedido/element/nota/simpleType/restriction")),
        ]
    );
}

/// Un atributo global con tipo anónimo se comprueba (sus facetas y su
/// `default`) pero no se baja, como el resto de los atributos globales.
#[test]
fn tipo_de_atributo_global_se_comprueba_y_no_se_baja() {
    let global = |f: &str| {
        format!(
            "<xs:schema {XS}><xs:attribute name='g' default='abc'><xs:simpleType>\
             <xs:restriction base='xs:string'>{f}</xs:restriction></xs:simpleType>\
             </xs:attribute><xs:element name='r' type='xs:int'/></xs:schema>"
        )
    };
    let v = ir_de(&global("<xs:maxLength value='3'/>"));
    assert!((0..v.nodes().len()).all(|i| !v.location(NodeId::new(i)).contains("attribute")));
    assert_eq!(codigos(&global("<xs:maxLength value='2'/>")), ["value"]);
    assert_eq!(codigos(&global("<xs:minInclusive value='2'/>")), ["facet"]);
}

/// 5.6-CR-I1 (costo lineal, D1 sin reloj): cada cota se **analiza una vez**.
/// Una cota de 10⁶ cifras se compara con k = 1000 valores cortos en las tres
/// formas que la revisión midió como cuadráticas: k enumeraciones de un
/// derivado, k atributos con `default` del tipo y una cadena de k pasos con
/// su propia cota del otro lado. `ir::num::contador` (feature `contadores`,
/// solo en estas pruebas) cuenta los bytes que recorre el análisis de
/// lexemas: tiene que quedar por debajo de unas pocas veces la entrada; la
/// versión cuadrática recorre la cota k veces (≈ 10⁹ bytes). Solo cuenta lo
/// que pasa por `Sci::new`: clonar un `Decimal` en cada comparación no lo
/// vería (hoy `Ctx::bound` devuelve `&Decimal`, sin copia).
#[test]
fn cota_larga_se_analiza_una_sola_vez() {
    const K: usize = 1000;
    let cota = format!("1{}", "0".repeat(1_000_000));
    let t0 = format!(
        "<xs:simpleType name='T0'><xs:restriction base='xs:integer'>\
         <xs:maxInclusive value='{cota}'/></xs:restriction></xs:simpleType>"
    );
    let enumeraciones: String = (0..K)
        .map(|i| format!("<xs:enumeration value='{i}'/>"))
        .collect();
    let atributos: String = (0..K)
        .map(|i| format!("<xs:attribute name='a{i}' type='T0' default='{i}'/>"))
        .collect();
    let cadena: String = (1..=K)
        .map(|i| {
            format!(
                "<xs:simpleType name='C{i}'><xs:restriction base='{}'>\
                 <xs:minInclusive value='{i}'/></xs:restriction></xs:simpleType>",
                if i == 1 {
                    "T0".to_owned()
                } else {
                    format!("C{}", i - 1)
                }
            )
        })
        .collect();
    let mut medidas = Vec::new();
    for (forma, doc) in [
        (
            "enumeraciones",
            esquema(&format!("{t0}{}", restriccion("T0", &enumeraciones))),
        ),
        (
            "default",
            format!(
                "<xs:schema {XS}>{t0}<xs:element name='r'><xs:complexType>{atributos}\
                 </xs:complexType></xs:element></xs:schema>"
            ),
        ),
        (
            "cadena",
            esquema(&format!(
                "{t0}{cadena}{}",
                restriccion(&format!("C{K}"), "")
            )),
        ),
    ] {
        ir::num::contador::reiniciar();
        ir_de(&doc);
        let bytes = ir::num::contador::bytes();
        medidas.push((forma, bytes, doc.len() as u64));
    }
    // Todas las formas a la vez, para que un mutante muestre cuáles rompe.
    let cuadraticas: Vec<_> = medidas.iter().filter(|m| m.1 > 4 * m.2).collect();
    assert!(
        cuadraticas.is_empty(),
        "(forma, bytes analizados, bytes de entrada): {cuadraticas:?}"
    );
}
