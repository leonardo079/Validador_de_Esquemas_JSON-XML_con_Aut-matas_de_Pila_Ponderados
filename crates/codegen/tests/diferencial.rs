//! Prueba diferencial intérprete / C / Rust (F5.3, D76; criterios en
//! `tasks/todo.md` «Tarea F5.3»).
//!
//! Tres implementaciones del mismo M sobre las mismas entradas:
//!
//! 1. el intérprete de referencia (`automaton::validate_with`, Fase 4);
//! 2. el validador Rust generado (F5.1): un banco con las tablas y la fachada
//!    de cada esquema sobre una copia del runtime, compilado con `rustc` en
//!    *debug* (desbordamientos comprobados);
//! 3. el validador C generado (F5.2): un banco con las tablas de cada esquema
//!    sobre el núcleo C, compilado con AddressSanitizer y
//!    UndefinedBehaviorSanitizer cuando el compilador los tiene (una lectura
//!    fuera de rango o un desbordamiento con signo abortan el banco).
//!
//! Entradas: cada payload del corpus y, por cada uno, mutaciones
//! deterministas (truncar, borrar, duplicar o cambiar un byte por uno de los
//! que mueven a los lexers: llaves, comillas, `<`, `&`, bytes no ASCII…). Para
//! cada entrada, bloques de 1, 7 y 4 096 bytes y los dos modos: los tres
//! reportes deben ser idénticos campo a campo (válido, peso, clase, primer
//! error con offset, línea, columna y regla). Ninguna prueba mide tiempo
//! (D1).

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use automaton::{build, validate_with, Automaton, ErrorKind, Limits, Mode, Options, Report};
use codegen::Archivo;

mod comun;
use comun::*;

/// El reporte en una línea, igual en las tres implementaciones: válido,
/// peso (`inf` si ∞), clase (0 ninguna, 1 léxico, 2 sintaxis, 3 esquema, 4
/// límite, 5 interno) y el primer error (`-` si no hay) con su puntero
/// en hexadecimal y si está recortado (F6.1).
fn norma(r: &Report) -> String {
    let kind = match r.error_kind {
        None => 0,
        Some(ErrorKind::Lexical) => 1,
        Some(ErrorKind::Syntax) => 2,
        Some(ErrorKind::Schema) => 3,
        Some(ErrorKind::Limit) => 4,
        Some(ErrorKind::Internal) => 5,
    };
    let peso = r
        .violations
        .get()
        .map_or("inf".to_owned(), |n| n.to_string());
    let first = match r.first {
        None => "-".to_owned(),
        Some(f) => format!(
            "{} {} {} {} x{} {}",
            f.offset,
            f.line,
            f.column,
            f.rule.map_or("-".to_owned(), |k| k.to_string()),
            r.pointer
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            u8::from(r.pointer.truncated())
        ),
    };
    format!("{} {peso} {kind} {first}", u8::from(r.valid))
}

/// Generador determinista (xorshift64*).
struct Azar(u64);

impl Azar {
    fn sig(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn hasta(&mut self, n: usize) -> usize {
        (self.sig() % n.max(1) as u64) as usize
    }
}

/// Bytes que cambian el camino de los lexers JSON y XML.
const BYTES: &[u8] = b"{}[]\":,0-9.eE<>/?!&#;=' \n\r\t\\x\x00\x7f\xc3\xa9\xef\xbb\xbf\xff";

/// Mutaciones deterministas de `p` (semilla `s`).
fn mutantes(p: &[u8], s: u64) -> Vec<Vec<u8>> {
    let mut az = Azar(s.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut v = Vec::new();
    let n = p.len();
    if n > 1 {
        v.push(p[..n / 2].to_vec());
        v.push(p[..n - 1].to_vec());
    }
    for _ in 0..8 {
        let mut m = p.to_vec();
        let i = az.hasta(n);
        match az.hasta(4) {
            0 if !m.is_empty() => {
                m.remove(i);
            }
            1 if !m.is_empty() => {
                let b = m[i];
                m.insert(i, b);
            }
            _ if !m.is_empty() => m[i] = BYTES[az.hasta(BYTES.len())],
            _ => m.push(BYTES[az.hasta(BYTES.len())]),
        }
        v.push(m);
    }
    v
}

/// Un esquema de la prueba: su M y si el payload es XML.
struct Esquema<'a> {
    a: &'a Automaton,
    xml: bool,
}

/// Las entradas del corpus: (esquema, payload), con sus mutaciones.
fn entradas(grupos: &[Grupo]) -> Vec<(usize, Vec<u8>)> {
    let mut v = Vec::new();
    for (k, g) in grupos.iter().enumerate() {
        for (j, (_, p)) in g.casos.iter().enumerate() {
            v.push((k, p.clone()));
            for m in mutantes(p, ((k as u64) << 32) | j as u64) {
                v.push((k, m));
            }
        }
    }
    v
}

/// Escribe `p{j}.bin` y `trabajos.txt` (una línea `k j` por entrada).
fn escribir_trabajos(dir: &Path, entradas: &[(usize, Vec<u8>)]) {
    let mut t = String::new();
    for (j, (k, p)) in entradas.iter().enumerate() {
        std::fs::write(dir.join(format!("p{j}.bin")), p).unwrap();
        let _ = writeln!(t, "{k} {j}");
    }
    std::fs::write(dir.join("trabajos.txt"), t).unwrap();
}

/// Las líneas esperadas: `j bloque modo norma`, del intérprete.
fn esperadas(esquemas: &[Esquema<'_>], entradas: &[(usize, Vec<u8>)]) -> Vec<String> {
    let mut v = Vec::new();
    for (j, (k, p)) in entradas.iter().enumerate() {
        for b in BLOQUES {
            for (m, modo) in [(Mode::Total, 0), (Mode::EarlyFail, 1)] {
                let r = validate_with(esquemas[*k].a, p, &Options::new(m), b);
                v.push(format!("{j} {b} {modo} {}", norma(&r)));
            }
        }
    }
    v
}

fn correr(dir: &Path, bin: &str) -> Vec<String> {
    let out = Command::new(dir.join(bin))
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{bin}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Compara línea a línea y da la primera diferencia con su entrada.
fn iguales(quien: &str, obtenidas: &[String], esperadas: &[String]) {
    for (i, (o, e)) in obtenidas.iter().zip(esperadas).enumerate() {
        assert_eq!(o, e, "{quien}, línea {i}");
    }
    assert_eq!(
        obtenidas.len(),
        esperadas.len(),
        "{quien}: número de líneas"
    );
}

/// El banco Rust: tablas y fachada de cada esquema (`e{k}`) sobre una copia
/// del runtime; imprime `j bloque modo norma`.
fn banco_rust(esquemas: &[Esquema<'_>], dir: &Path) {
    let primeros = codegen::rust::emit(esquemas[0].a);
    let runtime: Vec<Archivo> = primeros
        .into_iter()
        .filter(|f| f.nombre.starts_with("lexer") || f.nombre.starts_with("runtime_rs"))
        .collect();
    escribir(dir, &runtime);
    let mut main =
        String::from("#![allow(dead_code, unused_imports)]\nmod lexer;\nmod runtime_rs;\n");
    let mut ramas = String::new();
    for (k, g) in esquemas.iter().enumerate() {
        for f in codegen::rust::emit(g.a) {
            if f.nombre == "tablas.rs" || f.nombre == "validador.rs" {
                std::fs::write(dir.join(format!("e{k}_{}", f.nombre)), &f.contenido).unwrap();
            }
        }
        std::fs::write(
            dir.join(format!("e{k}.rs")),
            format!(
                "#[path = \"e{k}_tablas.rs\"]\npub mod tablas;\n\
                 #[path = \"e{k}_validador.rs\"]\npub mod validador;\n"
            ),
        )
        .unwrap();
        let _ = writeln!(main, "mod e{k};");
        let _ = writeln!(
            ramas,
            "        {k} => e{k}::validador::validar_por_bloques(p, b, m),"
        );
    }
    main.push_str(&BANCO_RUST.replace("{{RAMAS}}", &ramas));
    std::fs::write(dir.join("main.rs"), main).unwrap();
    rustc(dir, &["--edition", "2021", "-o", "banco", "main.rs"]);
}

const BANCO_RUST: &str = r#"
use runtime_rs::{ErrorKind, Mode, Report};

fn uno(k: usize, p: &[u8], b: usize, m: Mode) -> Report {
    match k {
{{RAMAS}}        _ => unreachable!(),
    }
}

fn norma(r: &Report) -> String {
    let kind = match r.error_kind {
        None => 0,
        Some(ErrorKind::Lexical) => 1,
        Some(ErrorKind::Syntax) => 2,
        Some(ErrorKind::Schema) => 3,
        Some(ErrorKind::Limit) => 4,
        Some(ErrorKind::Internal) => 5,
    };
    let peso = r.violations.get().map_or("inf".to_owned(), |n| n.to_string());
    let first = match r.first {
        None => "-".to_owned(),
        Some(f) => format!(
            "{} {} {} {} x{} {}",
            f.offset,
            f.line,
            f.column,
            f.rule.map_or("-".to_owned(), |k| k.to_string()),
            r.pointer.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>(),
            u8::from(r.pointer.truncated())
        ),
    };
    format!("{} {peso} {kind} {first}", u8::from(r.valid))
}

fn correr() {
    let trabajos = std::fs::read_to_string("trabajos.txt").unwrap();
    for l in trabajos.lines() {
        let mut it = l.split(' ');
        let k: usize = it.next().unwrap().parse().unwrap();
        let j: usize = it.next().unwrap().parse().unwrap();
        let p = std::fs::read(format!("p{j}.bin")).unwrap();
        for b in [1usize, 7, 4096] {
            for (m, modo) in [(Mode::Total, 0), (Mode::EarlyFail, 1)] {
                println!("{j} {b} {modo} {}", norma(&uno(k, &p, b, m)));
            }
        }
    }
}

fn main() {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(correr)
        .unwrap()
        .join()
        .unwrap();
}
"#;

/// Las banderas de los sanitizadores si `cc` los admite (se prueba con un
/// programa vacío), o ninguna.
fn sanitizadores(cc: &str, dir: &Path) -> Vec<&'static str> {
    let banderas = vec![
        "-fsanitize=address,undefined",
        "-fno-sanitize-recover=all",
        "-fno-omit-frame-pointer",
    ];
    std::fs::write(dir.join("sonda.c"), "int main(void) { return 0; }\n").unwrap();
    let mut args = banderas.clone();
    args.extend(["-o", "sonda", "sonda.c"]);
    let ok = Command::new(cc)
        .current_dir(dir)
        .args(&args)
        .output()
        .is_ok_and(|o| o.status.success())
        && Command::new(dir.join("sonda"))
            .status()
            .is_ok_and(|s| s.success());
    if ok {
        banderas
    } else {
        eprintln!("diferencial: {cc} sin sanitizadores; el banco C corre sin ellos");
        Vec::new()
    }
}

/// El banco C: las tablas de cada esquema como `sv_tablas_{k}` sobre un solo
/// núcleo; para cada entrada, la memoria justa de `sv_state_words` (como el
/// intérprete) e imprime `j bloque modo norma`.
fn banco_c(esquemas: &[Esquema<'_>], dir: &Path) {
    let primeros = codegen::c::emit(esquemas[0].a);
    let comunes: Vec<Archivo> = primeros
        .into_iter()
        .filter(|f| f.nombre.starts_with("sv_"))
        .collect();
    escribir(dir, &comunes);
    let mut decl = String::new();
    let mut tablas = String::from("static const sv_tables_t* const TABLAS[] = {");
    let mut xml = String::from("static const bool XML[] = {");
    let mut fuentes = vec![
        "banco.c".to_owned(),
        "sv_lexico.c".into(),
        "sv_nucleo.c".into(),
    ];
    let mut max_words = 0;
    for (k, g) in esquemas.iter().enumerate() {
        let t = codegen::c::emit(g.a)
            .into_iter()
            .find(|f| f.nombre == "tablas.c")
            .unwrap()
            .contenido
            .replace("#include \"validador.h\"", "#include \"sv_nucleo.h\"")
            .replace(
                "const sv_tables_t sv_tablas = {",
                &format!("const sv_tables_t sv_tablas_{k} = {{"),
            );
        std::fs::write(dir.join(format!("t{k}.c")), t).unwrap();
        fuentes.push(format!("t{k}.c"));
        let _ = writeln!(decl, "extern const sv_tables_t sv_tablas_{k};");
        let _ = write!(tablas, "&sv_tablas_{k}, ");
        let _ = write!(xml, "{}, ", g.xml);
        max_words = max_words.max(g.a.state_words(&Limits::DEFAULT));
    }
    tablas.push_str("};\n");
    xml.push_str("};\n");
    let banco = BANCO_C
        .replace("{{DECL}}", &decl)
        .replace("{{TABLAS}}", &tablas)
        .replace("{{XML}}", &xml)
        .replace("{{MAX_WORDS}}", &max_words.to_string());
    std::fs::write(dir.join("banco.c"), banco).unwrap();
    let cc = cc();
    let mut args = vec!["-std=c11", "-O1", "-g", "-Wall", "-Wextra", "-Werror"];
    let san = sanitizadores(&cc, dir);
    args.extend(san);
    args.extend(["-o", "banco"]);
    args.extend(fuentes.iter().map(String::as_str));
    compilar_con(dir, &cc, &args);
}

const BANCO_C: &str = r#"#include <stdio.h>
#include <stdlib.h>

#include "sv_nucleo.h"

{{DECL}}
{{TABLAS}}
{{XML}}
static uint64_t mem[{{MAX_WORDS}}];

static uint8_t* leer(unsigned j, size_t* n) {
    char nombre[64];
    snprintf(nombre, sizeof nombre, "p%u.bin", j);
    FILE* f = fopen(nombre, "rb");
    if (f == NULL) {
        exit(2);
    }
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t* p = malloc((size_t)len + 1);
    *n = fread(p, 1, (size_t)len, f);
    fclose(f);
    return p;
}

int main(void) {
    static const size_t bloques[3] = {1, 7, 4096};
    FILE* t = fopen("trabajos.txt", "r");
    unsigned k, j;
    if (t == NULL) {
        return 2;
    }
    while (fscanf(t, "%u %u", &k, &j) == 2) {
        size_t n;
        uint8_t* p = leer(j, &n);
        for (int bi = 0; bi < 3; bi++) {
            for (int modo = 0; modo < 2; modo++) {
                sv_limits_t lim;
                sv_validator_t v;
                sv_report_t r;
                sv_limits_default(&lim);
                lim.mode = modo;
                size_t w = sv_state_words(TABLAS[k], &lim);
                sv_validator_init(&v, TABLAS[k], XML[k], &lim, mem, w);
                for (size_t i = 0; i < n; i += bloques[bi]) {
                    size_t len = n - i < bloques[bi] ? n - i : bloques[bi];
                    if (sv_validator_feed(&v, p + i, len) < 0) {
                        break;
                    }
                }
                sv_validator_finish(&v, &r);
                printf("%u %zu %d %d ", j, bloques[bi], modo, r.valid ? 1 : 0);
                if (r.violations == SV_INFINITY) {
                    printf("inf");
                } else {
                    printf("%llu", (unsigned long long)r.violations);
                }
                printf(" %d ", r.error_kind);
                if (!r.has_first) {
                    printf("-\n");
                } else {
                    if (r.has_rule) {
                        printf("%llu %u %u %u x", (unsigned long long)r.offset, r.line, r.column,
                               r.rule);
                    } else {
                        printf("%llu %u %u - x", (unsigned long long)r.offset, r.line, r.column);
                    }
                    for (size_t q = 0; q < r.pointer_len; q++) {
                        printf("%02x", r.pointer[q]);
                    }
                    printf(" %d\n", r.pointer_truncated ? 1 : 0);
                }
            }
        }
        free(p);
    }
    fclose(t);
    return 0;
}
"#;

/// Esquemas sintéticos para la aritmética de `num` (comparación exacta y
/// `multipleOf` sobre el lexema): el corpus apenas tiene números negativos
/// comparados con cotas negativas, ni lexemas de más de 18 cifras (donde el
/// C usa `int64_t` y Rust `i128`).
const NUMERICOS_JSON: &[&str] = &[
    r#"{"type":"array","items":{"minimum":-10.5,"maximum":1e20}}"#,
    r#"{"type":"array","items":{"exclusiveMinimum":-1e-5,"exclusiveMaximum":123456789012345678901234567890}}"#,
    r#"{"type":"array","items":{"minimum":-1e30,"maximum":-1e-30}}"#,
    r#"{"type":"array","items":{"multipleOf":0.25}}"#,
    r#"{"type":"array","items":{"multipleOf":7}}"#,
    r#"{"type":"array","items":{"multipleOf":1e-3}}"#,
    r#"{"type":"array","items":{"multipleOf":12345678901234567}}"#,
    r#"{"type":"array","items":{"enum":[1,-2.5,3e2,100000000000000000000000,-0.0001,0]}}"#,
    r#"{"type":"array","items":{"type":"integer"}}"#,
    r#"{"type":"array","items":{"minimum":-99999999999999999999.5,"exclusiveMaximum":1e1000000000000000000000}}"#,
    r#"{"type":"array","items":{"multipleOf":5e-20,"maximum":-0}}"#,
    r#"{"type":"array","items":{"exclusiveMinimum":1e1}}"#,
];

const NUMERICO_XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="l"><xs:complexType><xs:sequence>
    <xs:element name="d" minOccurs="0" maxOccurs="unbounded"><xs:simpleType><xs:restriction base="xs:decimal"><xs:minInclusive value="-12.5"/><xs:maxExclusive value="1000000000000000000000"/></xs:restriction></xs:simpleType></xs:element>
    <xs:element name="f" minOccurs="0" maxOccurs="unbounded"><xs:simpleType><xs:restriction base="xs:double"><xs:enumeration value="1.5"/><xs:enumeration value="-0.1"/><xs:enumeration value="1e300"/></xs:restriction></xs:simpleType></xs:element>
    <xs:element name="i" minOccurs="0" maxOccurs="unbounded" type="xs:integer"/>
  </xs:sequence></xs:complexType></xs:element>
</xs:schema>"#;

/// Números cerca de las cotas de los esquemas sintéticos.
const BORDES: &[&str] = &[
    "0",
    "-0",
    "0.0",
    "-0e5",
    "-10.5",
    "-10.50",
    "-1050e-2",
    "-10.4999",
    "-10.5000000000000000001",
    "1e20",
    "100000000000000000000",
    "100000000000000000000.0000000001",
    "1e-5",
    "-0.00001",
    "-1e-5",
    "-1E-5",
    "-0.0000099999",
    "123456789012345678901234567890",
    "123456789012345678901234567890.0000001",
    "123456789012345678901234567889.9999999",
    "0.25",
    "-0.75",
    "0.125",
    "7e0",
    "70e-1",
    "-49",
    "12345678901234567",
    "24691357802469134",
    "24691357802469135",
    "5e-20",
    "1e-19",
    "-1e-19",
    "-99999999999999999999.5",
    "-99999999999999999999.4",
    "-99999999999999999999.6",
    "1e999999999999999999999",
    "-1e-999999999999999999999",
    "1e0000000000000000000000001",
    "-1e+30",
    "-1e-30",
    "-1e-31",
    "-1.0000000000000000000000000000000001e-30",
    "300",
    "3e2",
    "30e1",
    "1e23",
    "1.5",
    "-0.1",
    "1e300",
    "1.5000000000000001",
    "-0.10000000000000001",
    "1.4999999999999999",
    // Exponentes de más de 18 cifras que difieren en poco de la cota
    // `1e1000000000000000000000` y que las cifras compensan (iguales, justo
    // por debajo o por encima): ejercen la resta exacta de `exp_diff`.
    "10e999999999999999999999",
    "0.1e1000000000000000000001",
    "100e999999999999999999998",
    "9.99e999999999999999999999",
    "1.000001e1000000000000000000000",
    "1e1000000000000000000001",
    "1e999999999999999999999",
    // Diferencia de exponentes de exactamente 10^19 con `1e1`: las 18 cifras
    // bajas de la resta son 0 y solo la cifra 19 dice que es enorme.
    "1e10000000000000000001",
    "-1e10000000000000000001",
    "1e-9999999999999999999",
];

/// Un número de RFC 8259 al azar: con signo, cifras (también más de 18 y de
/// 31), fracción y exponente (también de más de 18 cifras).
fn numero(az: &mut Azar) -> String {
    if az.hasta(3) == 0 {
        return BORDES[az.hasta(BORDES.len())].to_owned();
    }
    let mut s = String::new();
    if az.hasta(3) == 0 {
        s.push('-');
    }
    let largos = [1, 2, 3, 17, 18, 19, 20, 31, 32, 40];
    let cifras = |az: &mut Azar, n: usize, s: &mut String| {
        for _ in 0..n {
            s.push(char::from(b'0' + az.hasta(10) as u8));
        }
    };
    if az.hasta(5) == 0 {
        s.push('0');
    } else {
        s.push(char::from(b'1' + az.hasta(9) as u8));
        let n = largos[az.hasta(largos.len())] - 1;
        cifras(az, n, &mut s);
    }
    if az.hasta(2) == 0 {
        s.push('.');
        let n = largos[az.hasta(largos.len())];
        cifras(az, n, &mut s);
    }
    if az.hasta(3) == 0 {
        s.push(if az.hasta(2) == 0 { 'e' } else { 'E' });
        match az.hasta(3) {
            0 => s.push('-'),
            1 => s.push('+'),
            _ => {}
        }
        let n = [1, 2, 5, 19, 22][az.hasta(5)];
        cifras(az, n, &mut s);
    }
    s
}

/// Entradas de los esquemas sintéticos: arreglos (o listas XML) de números;
/// alguno con un lexema de más de 1 024 bytes (`limit`).
fn entradas_numericas(primero: usize, n_json: usize) -> Vec<(usize, Vec<u8>)> {
    let mut az = Azar(0x5eed_f53a_0000_0001);
    let mut v = Vec::new();
    for k in 0..=n_json {
        for i in 0..30 {
            let nums: Vec<String> = (0..20).map(|_| numero(&mut az)).collect();
            let mut nums = nums;
            if i == 29 {
                nums.push(format!("1{}", "0".repeat(1100)));
            }
            let p = if k < n_json {
                format!("[{}]", nums.join(", "))
            } else {
                let mut x = String::from("<l>");
                for (j, n) in nums.iter().enumerate() {
                    let e = ["d", "f", "i"][j * 3 / nums.len()];
                    let _ = write!(x, "<{e}>{n}</{e}>");
                }
                x.push_str("</l>");
                x
            };
            v.push((primero + k, p.into_bytes()));
        }
    }
    v
}

/// Esquemas sintéticos para la capa de buena formación (F4.6), con bordes
/// que el corpus no tiene.
/// (`~` es la barra invertida de un escape JSON, como en [`payloads_json`].)
const BUENA_FORMACION_JSON: &str = r#"{"type":"object","properties":{"ab":{"type":"integer"},"e":{"enum":["a~nb","~u00e9~t","~"~~~/~b~f~r"]}}}"#;

const BUENA_FORMACION_XSD: &str = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="n" minOccurs="0" maxOccurs="unbounded"><xs:simpleType><xs:restriction base="xs:decimal"><xs:minInclusive value="1.5"/></xs:restriction></xs:simpleType></xs:element>
    <xs:element name="s" minOccurs="0" maxOccurs="unbounded"><xs:simpleType><xs:restriction base="xs:string"><xs:maxLength value="3"/></xs:restriction></xs:simpleType></xs:element>
  </xs:sequence>
  <xs:attribute name="a"><xs:simpleType><xs:restriction base="xs:string"><xs:enumeration value="x y"/></xs:restriction></xs:simpleType></xs:attribute>
  <xs:attribute name="b" type="xs:string"/>
  </xs:complexType></xs:element>
</xs:schema>"#;

/// Payloads JSON de buena formación: claves repetidas con escapes (declarada
/// y no declarada), clave vacía repetida, 1 024 y 1 025 claves no
/// declaradas, BOM, y profundidad 256 y 257.
fn payloads_json() -> Vec<Vec<u8>> {
    let claves = |n: usize| {
        let v: Vec<String> = (0..n).map(|i| format!("\"k{i}\": {i}")).collect();
        format!("{{{}}}", v.join(","))
    };
    // `~` es la barra invertida de un escape JSON (`~u0062` = `b`).
    let mut v: Vec<Vec<u8>> = [
        r#"{"a~u0062": 1, "ab": 2}"#,
        r#"{"ab": 1, "a~u0062": 2}"#,
        r#"{"x~u0079": 1, "xy": 2}"#,
        r#"{"xy": 1, "x~u0079": 2}"#,
        r#"{"x~u00e9~u00e9": 1, "xéé": 2}"#,
        r#"{"xéé": 1, "x~u00e9~u00e9": 2}"#,
        r#"{"": 1, "": 2}"#,
        r#"{"~ud83d~ude00": 1, "😀": 2}"#,
        r#"{"😀": 1, "~ud83d~ude00": 2}"#,
        r#"{"a~n~t~"": 1, "a~n~t~"": 2}"#,
        r#"{"ab": 1.5, "c": [1, {"c": 2, "c": 3}]}"#,
        r#"{"e": "a~nb"}"#,
        r#"{"e": "a~rb"}"#,
        r#"{"e": "a~u000ab"}"#,
        r#"{"e": "é~t"}"#,
        r#"{"e": "~u00e9~u0009"}"#,
        r#"{"e": "~"~~/~u0008~f~r"}"#,
        r#"{"e": "~"~~/~b~f~n"}"#,
        "\u{feff}{\"ab\": 1}",
        // F6.1: punteros con `~` y `/` en la clave, índices de dos cifras y
        // errores en miembros.
        r#"{"z": {"a/b~u007ec": [0,0,0,0,0,0,0,0,0,0,1,]}}"#,
        r#"{"z": [[], {"": [0, {"k": tru}]}]}"#,
        r#"{"ab": "x", "e": 1}"#,
        r#"{"c": 1, "e": "~u00e9", "ab": [1]}"#,
    ]
    .iter()
    .map(|s| s.replace('~', "\\").into_bytes())
    .collect();
    // F6.1: un puntero de más de 1 024 bytes (recortado) y, tras salir de
    // él, uno exacto.
    let k = "x".repeat(300);
    v.push(
        format!(r#"{{"z": {{"{k}": {{"{k}": {{"{k}": {{"{k}": [1, tru]}}}}}}}}}}"#).into_bytes(),
    );
    v.push(
        format!(r#"{{"z": {{"{k}": {{"{k}": {{"{k}": {{"{k}": [1]}}}}}}}}, "ab": "x"}}"#)
            .into_bytes(),
    );
    // El índice que deja de caber al crecer (`/z/q…` llega a 1 022 bytes:
    // cabe `/9`, no `/10`) y un error en la coma con segmentos recortados.
    let q = "q".repeat(1019);
    v.push(format!(r#"{{"z": {{"{q}": [0,0,0,0,0,0,0,0,0,0,tru]}}}}"#).into_bytes());
    let q = "q".repeat(1020);
    v.push(format!(r#"{{"z": {{"{q}": [0,,1]}}}}"#).into_bytes());
    v.push(claves(1024).into_bytes());
    v.push(claves(1025).into_bytes());
    for d in [256, 257] {
        v.push(
            format!(
                "{{\"ab\": 1, \"z\": {}{}}}",
                "[".repeat(d - 1),
                "]".repeat(d - 1)
            )
            .into_bytes(),
        );
    }
    v
}

/// Payloads XML de buena formación: BOM, espacios normalizados en un
/// atributo (y no los de una referencia), `\r\n`, QName con dos `:`,
/// referencias en un número y en un texto, nombre de cierre distinto,
/// atributo repetido y 100 000 y 100 001 referencias.
fn payloads_xml() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = [
        "\u{feff}<r a=\"x y\"/>",
        "<r a=\"x\ty\"/>",
        "<r a=\"x\ny\"/>",
        "<r a=\"x\r\ny\"/>",
        "<r a=\"x&#9;y\"/>",
        "<r a=\"x&#32;y\"/>",
        "<r><n>1&#13;</n></r>",
        "<r><n>2&#13;</n><n>1.5&#10;</n><n>&#32;1.4</n></r>",
        "<r><n> 1.5 </n><n>1&#46;5</n><s>a\r\nb</s><s>a\rb</s></r>",
        "<r><s>&lt;&gt;&amp;</s><s>&#x10FFFF;</s></r>",
        "<a:b:c/>",
        "<r><n>1</n></x>",
        "<r a=\"x y\" a=\"x y\"/>",
        "<r b=\"1\" b=\"2\"/>",
        "<?xml version=\"1.0\"?>\r\n<r><s>abc</s></r>\r\n",
        "<r><!-- c --><s>ab</s><![CDATA[x]]></r>",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    for n in [100_000, 100_001] {
        v.push(format!("<r b=\"{}\"/>", "&amp;".repeat(n)).into_bytes());
    }
    v
}

/// Los payloads del directorio de la variable `var` (F6.4; relativo a la
/// raíz del repositorio o absoluto): como mucho 1 000 archivos de hasta
/// 64 KiB, en orden de nombre.
fn extra(var: &str) -> Vec<Vec<u8>> {
    let Some(dir) = std::env::var_os(var) else {
        return Vec::new();
    };
    // Relativo a la raíz del repositorio (`cargo test` corre en el crate).
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(dir);
    let mut rutas: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{var}: {e}"))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .collect();
    rutas.sort();
    let v: Vec<Vec<u8>> = rutas
        .iter()
        .map(|p| std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .filter(|b| b.len() <= 64 << 10)
        .take(1000)
        .collect();
    if v.len() < rutas.len() {
        eprintln!(
            "{var}: se usan {} de {} archivos (≤ 64 KiB, como mucho 1 000)",
            v.len(),
            rutas.len()
        );
    }
    v
}

/// Criterios 1-3 de F5.3: intérprete = Rust = C en todo el corpus y sus
/// mutaciones, y en los esquemas numéricos sintéticos, en 1, 7 y 4 096 bytes
/// y los dos modos.
#[test]
fn interprete_c_y_rust_iguales() {
    let grupos = grupos();
    let mut entradas = entradas(&grupos);
    let corpus = entradas.len();
    assert!(corpus >= 700, "{corpus}");
    let sinteticos: Vec<(Automaton, bool)> = NUMERICOS_JSON
        .iter()
        .map(|t| (build(&ir_de(t.as_bytes(), false)).unwrap(), false))
        .chain(std::iter::once((
            build(&ir_de(NUMERICO_XSD.as_bytes(), true)).unwrap(),
            true,
        )))
        .chain([
            (
                build(&ir_de(
                    BUENA_FORMACION_JSON.replace('~', "\\").as_bytes(),
                    false,
                ))
                .unwrap(),
                false,
            ),
            (
                build(&ir_de(BUENA_FORMACION_XSD.as_bytes(), true)).unwrap(),
                true,
            ),
        ])
        .collect();
    let esquemas: Vec<Esquema<'_>> = grupos
        .iter()
        .map(|g| Esquema {
            a: &g.a,
            xml: g.xml,
        })
        .chain(sinteticos.iter().map(|(a, xml)| Esquema { a, xml: *xml }))
        .collect();
    entradas.extend(entradas_numericas(grupos.len(), NUMERICOS_JSON.len()));
    let numericas_fin = entradas.len();
    let bf = grupos.len() + NUMERICOS_JSON.len() + 1;
    for (k, ps) in [(bf, payloads_json()), (bf + 1, payloads_xml())] {
        for (j, p) in ps.iter().enumerate() {
            entradas.push((k, p.clone()));
            // Los de 100 000 referencias (≈ 500 KB) no se mutan: solo
            // alargarían la prueba.
            if p.len() > 64 << 10 {
                continue;
            }
            for m in mutantes(p, 0xbf00 + ((k as u64) << 16) + j as u64) {
                entradas.push((k, m));
            }
        }
    }
    // F6.4: lo que encontró el fuzzer (`scripts/fuzz.sh`), con cada esquema
    // del corpus de su formato. Sin las variables, nada.
    for (var, xml) in [("SV_EXTRA_JSON", false), ("SV_EXTRA_XML", true)] {
        let extra = extra(var);
        if !extra.is_empty() {
            eprintln!("{var}: {} payloads", extra.len());
        }
        for p in &extra {
            for (k, _) in grupos.iter().enumerate().filter(|(_, g)| g.xml == xml) {
                entradas.push((k, p.clone()));
            }
        }
    }
    let esperadas = esperadas(&esquemas, &entradas);

    let dir_r = directorio("dif-rust");
    let dir_c = directorio("dif-c");
    std::thread::scope(|s| {
        let r = s.spawn(|| banco_rust(&esquemas, &dir_r));
        let c = s.spawn(|| banco_c(&esquemas, &dir_c));
        r.join().unwrap();
        c.join().unwrap();
    });
    escribir_trabajos(&dir_r, &entradas);
    escribir_trabajos(&dir_c, &entradas);
    iguales("Rust", &correr(&dir_r, "banco"), &esperadas);
    iguales("C", &correr(&dir_c, "banco"), &esperadas);

    // Las mutaciones llegan a todas las clases de resultado, y los números
    // sintéticos dan violaciones y aciertos.
    let clases: std::collections::BTreeSet<&str> = esperadas
        .iter()
        .filter_map(|l| l.split(' ').nth(5))
        .collect();
    for k in ["0", "1", "2", "3", "4"] {
        assert!(
            clases.contains(k),
            "ninguna entrada da la clase {k}: {clases:?}"
        );
    }
    let numericas = &esperadas[corpus * 6..numericas_fin * 6];
    let pesos: std::collections::BTreeSet<&str> = numericas
        .iter()
        .filter_map(|l| l.split(' ').nth(4))
        .collect();
    assert!(pesos.len() >= 10, "{pesos:?}");
}
