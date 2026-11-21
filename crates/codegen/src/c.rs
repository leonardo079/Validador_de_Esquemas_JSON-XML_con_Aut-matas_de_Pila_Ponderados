//! Emisor C11 (F5.2, D75; `docs/fase5/codegen-c.md`).
//!
//! [`emit`] da los archivos de un validador C autocontenido, que compila con
//! `cc -std=c11 -O2 -Wall -Wextra -o validador *.c` (la orden fija del
//! sandbox, D68):
//!
//! | Archivo | Contenido |
//! | --- | --- |
//! | `main.c` | stdin por bloques → una línea JSON en stdout y el código de salida (los mismos que en Rust) |
//! | `validador.h`, `validador.c` | la API C-ABI: `sv_state_t` de tamaño fijo, `sv_init`, `sv_feed`, `sv_finish`, `sv_report_json` |
//! | `tablas.c` | las tablas de M como arreglos `static const` (densas, D46 (1)) |
//! | `sv_lexico.h`, `sv_lexico.c` | las tablas de los AFD de la Fase 1, volcadas del crate `lexer` |
//! | `sv_nucleo.h`, `sv_nucleo.c` | el núcleo C (`crates/runtime-c`), port del de `runtime-rs` |
//!
//! Salida determinista, como la de Rust.

use std::fmt::Write;

use automaton::{Automaton, Limits, PayloadFormat};
use runtime_rs::tables::{
    CharRange, Check, CountOp, CounterOp, Dep, DivisorRef, Fault, Frame, Gate, GateKind, Interval,
    Lexeme, Op, OutMap, Rule, Test, Trans,
};
use runtime_rs::Tables;

use crate::fuentes::LEXER;
use crate::rust::{salida, BLOQUE_STDIN};
use crate::{Archivo, Opciones};

/// El núcleo C escrito a mano (`crates/runtime-c`), copiado tal cual.
pub const NUCLEO: [(&str, &str); 2] = [
    ("sv_nucleo.h", include_str!("../../runtime-c/sv_nucleo.h")),
    ("sv_nucleo.c", include_str!("../../runtime-c/sv_nucleo.c")),
];

const CABECERA: &str = "/* Generado por schemac (F5.2, docs/fase5/codegen-c.md); no editar. */\n";

/// Los archivos del validador C de `a` con los límites por defecto.
pub fn emit(a: &Automaton) -> Vec<Archivo> {
    emitir(a, &Opciones::default())
}

/// Como [`emit`], con los límites y el tiempo de `op` (F6.2), o el error de
/// [`Opciones::comprobar`] (memoria de más o profundidad 0).
pub fn emit_con(a: &Automaton, op: &Opciones) -> Result<Vec<Archivo>, String> {
    op.comprobar(a)?;
    Ok(emitir(a, op))
}

fn emitir(a: &Automaton, op: &Opciones) -> Vec<Archivo> {
    let mut v = vec![
        archivo("main.c", main_c(op)),
        archivo("validador.h", validador_h(a, op)),
        archivo("validador.c", validador_c(a, op)),
        archivo("tablas.c", tablas_c(a)),
        archivo("sv_lexico.h", lexico_h()),
        archivo("sv_lexico.c", lexico_c()),
    ];
    for (nombre, fuente) in NUCLEO {
        v.push(archivo(
            nombre,
            format!("/* Copia generada por schemac (F5.2) de `crates/runtime-c/{nombre}`; no editar. */\n{fuente}"),
        ));
    }
    v
}

fn archivo(nombre: &str, contenido: String) -> Archivo {
    Archivo {
        nombre: nombre.into(),
        contenido,
    }
}

fn main_c(op: &Opciones) -> String {
    let tiempo = op
        .tiempo_ms
        .map_or("-1".to_owned(), |ms| ms.min(i64::MAX as u64).to_string());
    format!(
        "{CABECERA}{}",
        MAIN.replace("{{BLOQUE}}", &BLOQUE_STDIN.to_string())
            .replace("{{INTERNO}}", &salida::INTERNO.to_string())
            .replace("{{TIEMPO_MS}}", &tiempo)
    )
}

/// `main.c`. stdin sin búfer (`_IONBF`): la biblioteca no reserva el suyo y
/// `fread` lee directo en `bloque`. El estado es estático (en `.bss`).
const MAIN: &str = r#"/*
 * Validador generado: lee el payload de stdin en bloques, escribe el reporte en
 * una línea JSON en stdout y sale con 0 (válido), 1 (inválido), 2 (malformado o
 * límite) o 3 (error interno o de E/S).
 *
 * Compilar: cc -std=c11 -O2 -Wall -Wextra -o validador *.c
 */
#include <stdint.h>
#include <stdio.h>
#include <time.h>

#include "validador.h"

#define BLOQUE {{BLOQUE}}
#define INTERNO {{INTERNO}}
/* Tiempo máximo de validación en milisegundos, -1 sin tiempo (F6.2): se mira
 * antes de entregar cada bloque leído; pasarlo es un error `limit`. */
#define TIEMPO_MS {{TIEMPO_MS}}LL

static sv_state_t estado;
static uint8_t bloque[BLOQUE];
static char linea[16384];

/* Clave de SipHash aleatoria para esta ejecución (docs/fase6/pendientes.md
 * §31): con la de por defecto, que es pública, un payload puede fabricar
 * choques (un falso «repetido», o aceptar un cierre XML que no empareja o un
 * arreglo bajo `not: uniqueItems`). C11 no tiene una fuente de aleatoriedad
 * portable: se lee /dev/urandom si existe y, si no, se mezcla la clave con el
 * reloj, el tiempo de CPU y una dirección (peor, pero no pública). */
static void clave_aleatoria(uint64_t k[2]) {
    uint64_t b[2];
    FILE* f = fopen("/dev/urandom", "rb");
    if (f != NULL) {
        size_t leidos;
        setvbuf(f, NULL, _IONBF, 0);
        leidos = fread(b, sizeof b[0], 2, f);
        fclose(f);
        if (leidos == 2) {
            k[0] = b[0];
            k[1] = b[1];
            return;
        }
    }
    {
        struct timespec t;
        uint64_t x = (uint64_t)(uintptr_t)&t;
        if (timespec_get(&t, TIME_UTC) == TIME_UTC) {
            x ^= (uint64_t)t.tv_sec * 0x9E3779B97F4A7C15u ^ (uint64_t)t.tv_nsec;
        }
        x ^= (uint64_t)clock() << 32;
        /* splitmix64 */
        x += 0x9E3779B97F4A7C15u;
        x = (x ^ (x >> 30)) * 0xBF58476D1CE4E5B9u;
        x = (x ^ (x >> 27)) * 0x94D049BB133111EBu;
        k[0] ^= x ^ (x >> 31);
        x += 0x9E3779B97F4A7C15u;
        x = (x ^ (x >> 30)) * 0xBF58476D1CE4E5B9u;
        x = (x ^ (x >> 27)) * 0x94D049BB133111EBu;
        k[1] ^= x ^ (x >> 31);
    }
}

int main(void) {
    sv_report_t r;
    sv_limits_t lim;
    int codigo, n;
    setvbuf(stdin, NULL, _IONBF, 0);
    sv_limits_emitted(&lim);
    clave_aleatoria(lim.hash_key);
    sv_init(&estado, &lim);
#if TIEMPO_MS >= 0
    struct timespec t0, t;
    int reloj = timespec_get(&t0, TIME_UTC) == TIME_UTC;
#endif
    for (;;) {
        size_t k = fread(bloque, 1, sizeof bloque, stdin);
#if TIEMPO_MS >= 0
        /* Sin reloj, el tiempo se da por agotado. */
        if (k > 0 && (!reloj || timespec_get(&t, TIME_UTC) != TIME_UTC ||
                      (long long)(t.tv_sec - t0.tv_sec) * 1000 +
                              (t.tv_nsec - t0.tv_nsec) / 1000000 >=
                          TIEMPO_MS)) {
            sv_abort_limit(&estado);
            break;
        }
#endif
        if (k > 0 && sv_feed(&estado, bloque, k) < 0) {
            break;
        }
        if (k < sizeof bloque) {
            if (ferror(stdin)) {
                fputs("validador: no se pudo leer la entrada\n", stderr);
                return INTERNO;
            }
            break;
        }
    }
    codigo = sv_finish(&estado, &r);
    n = sv_report_json(&r, linea, sizeof linea - 1);
    if (n < 0) {
        return INTERNO;
    }
    linea[n] = '\n';
    if (fwrite(linea, 1, (size_t)n + 1, stdout) != (size_t)n + 1 || fflush(stdout) != 0) {
        return INTERNO;
    }
    return codigo;
}
"#;

fn validador_h(a: &Automaton, op: &Opciones) -> String {
    let formato = a.format().name();
    format!(
        "{CABECERA}{}",
        VALIDADOR_H
            .replace("{{FORMATO}}", formato)
            .replace("{{STATE_WORDS}}", &a.state_words(&op.limites).to_string())
            .replace("{{SV_MAX_DEPTH}}", &op.limites.max_depth.to_string())
    )
}

/// El cuerpo de `sv_limits_emitted` (exhaustivo: un campo nuevo de `Limits`
/// rompe aquí).
fn limites_c(l: &Limits) -> String {
    let Limits {
        max_depth,
        max_string,
        max_lexeme,
        max_keys,
        key_pool,
        max_unique,
        max_xml_refs,
        max_payload,
        hash_key,
    } = *l;
    format!(
        "    lim->mode = SV_MODE_TOTAL;\n    lim->max_depth = {max_depth}u;\n    \
         lim->max_string = {max_string}ULL;\n    lim->max_lexeme = {max_lexeme}u;\n    \
         lim->max_keys = {max_keys}u;\n    lim->key_pool = {key_pool}u;\n    \
         lim->max_unique = {max_unique}u;\n    lim->max_xml_refs = {max_xml_refs}ULL;\n    \
         lim->max_payload = {max_payload}ULL;\n    lim->hash_key[0] = {:#x}ULL;\n    \
         lim->hash_key[1] = {:#x}ULL;\n",
        hash_key[0], hash_key[1]
    )
}

/// `validador.h`: la API C-ABI del validador generado (plan general,
/// «API C-ABI»; F4-U18).
const VALIDADOR_H: &str = r#"/*
 * API C-ABI del validador generado (F5.2). Sin malloc: el llamador pone
 * sv_state_t (tamaño fijo) en la pila o en memoria estática.
 *
 *   sv_state_t s;
 *   sv_init(&s, NULL);                      // NULL: los límites emitidos
 *   while (hay_bloque) if (sv_feed(&s, p, n) < 0) break;
 *   sv_report_t r;
 *   int codigo = sv_finish(&s, &r);         // 0/1/2/3, como el binario
 */
#ifndef VALIDADOR_H
#define VALIDADOR_H

#include "sv_nucleo.h"

/* Formato del payload: "json" o "xml". */
#define SV_FORMATO "{{FORMATO}}"
/* Profundidad máxima de anidamiento con los límites emitidos. */
#define SV_MAX_DEPTH {{SV_MAX_DEPTH}}
/* Palabras de memoria del núcleo con los límites emitidos. */
#define SV_STATE_WORDS {{STATE_WORDS}}u

typedef struct {
    sv_validator_t v;
    uint64_t mem[SV_STATE_WORDS];
} sv_state_t;

/* Las tablas de M (tablas.c). */
extern const sv_tables_t sv_tablas;

/* Los límites con que se emitió el validador (schemac compile --limit; F6.2),
 * en conformidad total. Sin --limit son los de sv_limits_default. */
void sv_limits_emitted(sv_limits_t* lim);

/* Prepara `s`. `lim` NULL da los límites emitidos (sv_limits_emitted); la
 * memoria está dimensionada para ellos: con límites mayores, lo que no quepa
 * es un error `limit`, nunca una escritura fuera. */
void sv_init(sv_state_t* s, const sv_limits_t* lim);

/* Consume un bloque de cualquier tamaño: 0 = sigue, -1 = se detuvo (malformado,
 * límite o, en fallo temprano, la primera violación). */
int sv_feed(sv_state_t* s, const uint8_t* chunk, size_t len);

/* Error `limit` donde se quedó la entrada, si no se había detenido (F6.2: el
 * tiempo máximo que vigila quien llama). */
void sv_abort_limit(sv_state_t* s);

/* Fin de la entrada: el reporte en `out` y el código de salida del binario
 * (0 válido, 1 inválido, 2 malformado o límite, 3 error interno). */
int sv_finish(sv_state_t* s, sv_report_t* out);

/* El reporte en una línea JSON (sin el salto final), con los campos del binario
 * Rust: valid, violations (null si ∞), errorKind, offset, line, column, pointer,
 * pointerTruncated, rule, keyword. Devuelve su longitud, o -1 si no cabe en `cap` bytes. */
int sv_report_json(const sv_report_t* r, char* buf, size_t cap);

/* Código de salida para `r`. */
int sv_exit_code(const sv_report_t* r);

#endif /* VALIDADOR_H */
"#;

fn validador_c(a: &Automaton, op: &Opciones) -> String {
    let t = a.tables();
    let mut palabras = String::new();
    for k in 0..t.rules.len() {
        let _ = writeln!(palabras, "    \"{}\",", a.rule(k as u32).1.name());
    }
    if t.rules.is_empty() {
        palabras.push_str("    \"\",\n");
    }
    format!(
        "{CABECERA}{}",
        VALIDADOR_C
            .replace("{{XML}}", bool_c(a.format() == PayloadFormat::Xml))
            .replace("{{N_REGLAS}}", &t.rules.len().to_string())
            // Sin reglas, `rule < 0u` sería «siempre falso» (`-Wtype-limits`).
            .replace(
                "{{HAY_PALABRA}}",
                if t.rules.is_empty() {
                    "false"
                } else {
                    "r->rule < N_REGLAS"
                },
            )
            .replace("{{N_ARREGLO}}", &t.rules.len().max(1).to_string())
            .replace("{{PALABRAS}}", &palabras)
            .replace("{{LIMITES}}", &limites_c(&op.limites))
            .replace("{{VALIDO}}", &salida::VALIDO.to_string())
            .replace("{{INVALIDO}}", &salida::INVALIDO.to_string())
            .replace("{{MALFORMADO}}", &salida::MALFORMADO.to_string())
            .replace("{{INTERNO}}", &salida::INTERNO.to_string())
    )
}

fn bool_c(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

const VALIDADOR_C: &str = r#"/* Fachada del validador generado: las tablas de tablas.c delante del núcleo. */
#include "validador.h"

#include <string.h>

/* Palabra clave de cada regla (sv_tablas.rules), para el reporte. */
#define N_REGLAS {{N_REGLAS}}u
static const char* const palabras[{{N_ARREGLO}}] = {
{{PALABRAS}}};

void sv_limits_emitted(sv_limits_t* lim) {
{{LIMITES}}}

void sv_init(sv_state_t* s, const sv_limits_t* lim) {
    sv_limits_t l;
    if (lim != NULL) {
        l = *lim;
    } else {
        sv_limits_emitted(&l);
    }
    sv_validator_init(&s->v, &sv_tablas, {{XML}}, &l, s->mem, SV_STATE_WORDS);
}

int sv_feed(sv_state_t* s, const uint8_t* chunk, size_t len) {
    return sv_validator_feed(&s->v, chunk, len);
}

void sv_abort_limit(sv_state_t* s) {
    sv_validator_abort_limit(&s->v);
}

int sv_exit_code(const sv_report_t* r) {
    if (r->valid) {
        return {{VALIDO}};
    }
    switch (r->error_kind) {
    case SV_KIND_SCHEMA:
        return {{INVALIDO}};
    case SV_KIND_LEXICAL:
    case SV_KIND_SYNTAX:
    case SV_KIND_LIMIT:
        return {{MALFORMADO}};
    default:
        return {{INTERNO}};
    }
}

int sv_finish(sv_state_t* s, sv_report_t* out) {
    sv_validator_finish(&s->v, out);
    return sv_exit_code(out);
}

typedef struct {
    char* p;
    size_t cap, n;
    bool ok;
} escritor_t;

static void put(escritor_t* w, const char* s, size_t n) {
    if (!w->ok || n > w->cap - w->n) {
        w->ok = false;
        return;
    }
    memcpy(w->p + w->n, s, n);
    w->n += n;
}

static void put_str(escritor_t* w, const char* s) {
    put(w, s, strlen(s));
}

static void put_u64(escritor_t* w, uint64_t v) {
    char d[20];
    size_t i = sizeof d;
    do {
        d[--i] = (char)('0' + v % 10);
        v /= 10;
    } while (v != 0);
    put(w, d + i, sizeof d - i);
}

/* Longitud de la secuencia UTF-8 válida que empieza en p[0] (tabla 3-7 de
 * Unicode), o 0 si no es válida. */
static size_t utf8_valido(const uint8_t* p, size_t n) {
    uint8_t b = p[0], lo = 0x80, hi = 0xBF;
    size_t len;
    if (b >= 0xC2 && b <= 0xDF) {
        len = 2;
    } else if (b >= 0xE0 && b <= 0xEF) {
        len = 3;
        if (b == 0xE0) {
            lo = 0xA0;
        } else if (b == 0xED) {
            hi = 0x9F;
        }
    } else if (b >= 0xF0 && b <= 0xF4) {
        len = 4;
        if (b == 0xF0) {
            lo = 0x90;
        } else if (b == 0xF4) {
            hi = 0x8F;
        }
    } else {
        return 0;
    }
    if (n < len || p[1] < lo || p[1] > hi) {
        return 0;
    }
    for (size_t i = 2; i < len; i++) {
        if (p[i] < 0x80 || p[i] > 0xBF) {
            return 0;
        }
    }
    return len;
}

/* Cadena JSON: escapa `"`, `\` y los controles; un byte que no es UTF-8 válido
 * sale como U+FFFD. */
static void put_cadena(escritor_t* w, const uint8_t* s, size_t n) {
    static const char hex[] = "0123456789abcdef";
    put(w, "\"", 1);
    for (size_t i = 0; i < n;) {
        uint8_t b = s[i];
        if (b == '"') {
            put(w, "\\\"", 2);
            i++;
        } else if (b == '\\') {
            put(w, "\\\\", 2);
            i++;
        } else if (b < 0x20) {
            char e[6] = {'\\', 'u', '0', '0', hex[b >> 4], hex[b & 15]};
            put(w, e, 6);
            i++;
        } else if (b < 0x80) {
            put(w, (const char*)s + i, 1);
            i++;
        } else {
            size_t k = utf8_valido(s + i, n - i);
            if (k == 0) {
                put(w, "\\ufffd", 6);
                i++;
            } else {
                put(w, (const char*)s + i, k);
                i += k;
            }
        }
    }
    put(w, "\"", 1);
}

static const char* clase(int k) {
    switch (k) {
    case SV_KIND_LEXICAL:
        return "lexical";
    case SV_KIND_SYNTAX:
        return "syntax";
    case SV_KIND_SCHEMA:
        return "schema";
    case SV_KIND_LIMIT:
        return "limit";
    default:
        return "internal";
    }
}

int sv_report_json(const sv_report_t* r, char* buf, size_t cap) {
    escritor_t w;
    w.p = buf;
    w.cap = cap;
    w.n = 0;
    w.ok = true;
    put_str(&w, r->valid ? "{\"valid\":true,\"violations\":" : "{\"valid\":false,\"violations\":");
    if (r->violations == SV_INFINITY) {
        put_str(&w, "null");
    } else {
        put_u64(&w, r->violations);
    }
    put_str(&w, ",\"errorKind\":");
    if (r->error_kind == SV_KIND_NONE) {
        put_str(&w, "null");
    } else {
        put_str(&w, "\"");
        put_str(&w, clase(r->error_kind));
        put_str(&w, "\"");
    }
    if (r->has_first) {
        put_str(&w, ",\"offset\":");
        put_u64(&w, r->offset);
        put_str(&w, ",\"line\":");
        put_u64(&w, r->line);
        put_str(&w, ",\"column\":");
        put_u64(&w, r->column);
        put_str(&w, ",\"pointer\":");
        put_cadena(&w, r->pointer, r->pointer_len);
        put_str(&w, r->pointer_truncated ? ",\"pointerTruncated\":true"
                                         : ",\"pointerTruncated\":false");
    } else {
        put_str(&w, ",\"offset\":null,\"line\":null,\"column\":null,\"pointer\":null,"
                    "\"pointerTruncated\":false");
    }
    put_str(&w, ",\"rule\":");
    if (r->has_first && r->has_rule) {
        size_t n;
        const uint8_t* loc = sv_rule_location(&sv_tablas, r->rule, &n);
        put_cadena(&w, loc, n);
    } else {
        put_str(&w, "null");
    }
    put_str(&w, ",\"keyword\":");
    if (r->has_first && r->has_rule && {{HAY_PALABRA}}) {
        const char* p = palabras[r->rule];
        put_cadena(&w, (const uint8_t*)p, strlen(p));
    } else {
        put_str(&w, "null");
    }
    put_str(&w, "}");
    if (!w.ok || w.n > 0x7FFFFFFF) {
        return -1;
    }
    return (int)w.n;
}
"#;

/// `tablas.c`: `const sv_tables_t sv_tablas` y un arreglo `static const` por
/// tabla (`NULL` si está vacía: C no admite arreglos de longitud 0).
fn tablas_c(a: &Automaton) -> String {
    let t = a.tables();
    // Exhaustivo: un campo nuevo en `Tables` rompe la compilación aquí.
    let Tables {
        n_classes,
        start,
        accept,
        delta,
        trans,
        checks,
        frames,
        masks,
        deps,
        nums,
        lexemes,
        divisors,
        counter_ops,
        intervals,
        gates,
        gate_inputs,
        outmaps,
        out_targets,
        scalar_slots,
        ascii,
        ranges,
        rules,
        text,
    } = t;
    let s = a.summary();
    let mut o = String::from(CABECERA);
    let _ = write!(
        o,
        "/* Tablas densas de M (D46 (1)) para payloads {}: {} estados, |Σ| = {}, {} transiciones,\n * \
         {} tipos de marco. */\n#include \"validador.h\"\n\n#define X 0xFFFFFFFFu\n\n",
        a.format().name(),
        s.states,
        s.classes,
        s.transitions,
        s.frames
    );
    let mut campos = String::new();
    // `filas`: los elementos, uno por línea (o de 16 en 16 los números);
    // `cuantos`: el número de elementos.
    let mut tabla = |o: &mut String,
                     nombre: &str,
                     campo: &str,
                     tipo: &str,
                     cuantos: usize,
                     filas: Vec<String>| {
        if cuantos == 0 {
            let _ = writeln!(campos, "    .{campo} = NULL,\n    .n_{campo} = 0,");
            return;
        }
        let _ = writeln!(o, "static const {tipo} {nombre}[{cuantos}] = {{");
        for f in &filas {
            let _ = writeln!(o, "    {f},");
        }
        o.push_str("};\n\n");
        let _ = writeln!(
            campos,
            "    .{campo} = {nombre},\n    .n_{campo} = {cuantos},"
        );
    };
    tabla(
        &mut o,
        "DELTA",
        "delta",
        "uint32_t",
        delta.len(),
        filas_u32(delta),
    );
    tabla(
        &mut o,
        "TRANS",
        "trans",
        "sv_trans_t",
        trans.len(),
        trans.iter().map(trans_lit).collect(),
    );
    tabla(
        &mut o,
        "CHECKS",
        "checks",
        "sv_check_t",
        checks.len(),
        checks.iter().map(check_lit).collect(),
    );
    tabla(
        &mut o,
        "FRAMES",
        "frames",
        "sv_frame_t",
        frames.len(),
        frames.iter().map(frame_lit).collect(),
    );
    tabla(
        &mut o,
        "MASKS",
        "masks",
        "uint64_t",
        masks.len(),
        masks.iter().map(|m| format!("{m:#x}ULL")).collect(),
    );
    tabla(
        &mut o,
        "DEPS",
        "deps",
        "sv_dep_t",
        deps.len(),
        deps.iter()
            .map(|d| {
                let Dep { bit, mask } = *d;
                format!("{{{}, {mask}}}", n(bit))
            })
            .collect(),
    );
    tabla(
        &mut o,
        "NUMS",
        "nums",
        "sv_lexeme_t",
        nums.len(),
        nums.iter()
            .map(|l| {
                let Lexeme { off, len } = *l;
                format!("{{{off}, {len}}}")
            })
            .collect(),
    );
    tabla(
        &mut o,
        "LEXEMES",
        "lexemes",
        "uint8_t",
        lexemes.len(),
        filas_u8(lexemes),
    );
    tabla(
        &mut o,
        "DIVISORS",
        "divisors",
        "sv_divisor_t",
        divisors.len(),
        divisors
            .iter()
            .map(|d| {
                let DivisorRef { lexeme, div } = *d;
                let runtime_rs::num::Divisor {
                    coprime,
                    twos,
                    fives,
                } = div;
                format!("{{{lexeme}, {coprime}ULL, {twos}, {fives}}}")
            })
            .collect(),
    );
    tabla(
        &mut o,
        "COUNTER_OPS",
        "counter_ops",
        "sv_counter_op_t",
        counter_ops.len(),
        counter_ops
            .iter()
            .map(|c| {
                let CounterOp { counter, op } = *c;
                let op = match op {
                    CountOp::Add => "SV_COUNT_ADD",
                    CountOp::Restart => "SV_COUNT_RESTART",
                };
                format!("{{{counter}, {op}}}")
            })
            .collect(),
    );
    tabla(
        &mut o,
        "INTERVALS",
        "intervals",
        "sv_interval_t",
        intervals.len(),
        intervals
            .iter()
            .map(|i| {
                let Interval { lo, hi, closed } = *i;
                format!("{{{lo}, {hi}, {}}}", bool_c(closed))
            })
            .collect(),
    );
    tabla(
        &mut o,
        "GATES",
        "gates",
        "sv_gate_t",
        gates.len(),
        gates.iter().map(gate_lit).collect(),
    );
    tabla(
        &mut o,
        "GATE_INPUTS",
        "gate_inputs",
        "uint32_t",
        gate_inputs.len(),
        filas_u32(gate_inputs),
    );
    tabla(
        &mut o,
        "OUTMAPS",
        "outmaps",
        "sv_outmap_t",
        outmaps.len(),
        outmaps
            .iter()
            .map(|m| {
                let OutMap { off, len } = *m;
                format!("{{{off}, {len}}}")
            })
            .collect(),
    );
    tabla(
        &mut o,
        "OUT_TARGETS",
        "out_targets",
        "uint32_t",
        out_targets.len(),
        filas_u32(out_targets),
    );
    tabla(
        &mut o,
        "ASCII",
        "ascii",
        "uint32_t",
        ascii.len(),
        filas_u32(ascii),
    );
    tabla(
        &mut o,
        "RANGES",
        "ranges",
        "sv_char_range_t",
        ranges.len(),
        ranges
            .iter()
            .map(|r| {
                let CharRange { lo, hi, class } = *r;
                format!("{{{lo}, {hi}, {class}}}")
            })
            .collect(),
    );
    tabla(
        &mut o,
        "RULES",
        "rules",
        "sv_rule_t",
        rules.len(),
        rules
            .iter()
            .map(|r| {
                let Rule {
                    node,
                    kind,
                    loc,
                    loc_len,
                } = *r;
                format!("{{{node}, {kind}, {loc}, {loc_len}}}")
            })
            .collect(),
    );
    tabla(
        &mut o,
        "TEXT",
        "text",
        "uint8_t",
        text.len(),
        filas_u8(text),
    );
    let _ = write!(
        o,
        "const sv_tables_t sv_tablas = {{\n    .n_classes = {n_classes},\n    .start = {start},\n    \
         .accept = {accept},\n    .scalar_slots = {scalar_slots},\n{campos}}};\n"
    );
    o
}

/// Un `u32` de las tablas: `X` si es `u32::MAX`.
fn n(v: u32) -> String {
    if v == u32::MAX {
        "X".into()
    } else {
        v.to_string()
    }
}

/// Filas de 16 números (cada una es un «elemento» de [`tablas_c`]).
fn filas_u32(v: &[u32]) -> Vec<String> {
    v.chunks(16)
        .map(|f| f.iter().map(|&x| n(x)).collect::<Vec<_>>().join(", "))
        .collect()
}

fn filas_u8(v: &[u8]) -> Vec<String> {
    v.chunks(16)
        .map(|f| f.iter().map(u8::to_string).collect::<Vec<_>>().join(", "))
        .collect()
}

fn trans_lit(t: &Trans) -> String {
    let Trans {
        op,
        next,
        ret,
        frame,
        bit,
        mark,
        counts,
        n_counts,
        checks,
        n_checks,
        outs,
    } = *t;
    let (op, fault) = match op {
        Op::Move => ("SV_OP_MOVE", "0"),
        Op::Push => ("SV_OP_PUSH", "0"),
        Op::Pop => ("SV_OP_POP", "0"),
        Op::Enter => ("SV_OP_ENTER", "0"),
        Op::Leave => ("SV_OP_LEAVE", "0"),
        Op::Fail(f) => (
            "SV_OP_FAIL",
            match f {
                Fault::Syntax => "SV_FAULT_SYNTAX",
                Fault::Duplicate => "SV_FAULT_DUPLICATE",
                Fault::Limit => "SV_FAULT_LIMIT",
                Fault::Internal => "SV_FAULT_INTERNAL",
            },
        ),
    };
    format!(
        "{{{op}, {fault}, {next}, {ret}, {frame}, {}, {}, {counts}, {n_counts}, {checks}, {n_checks}, {}}}",
        n(bit),
        n(mark),
        n(outs)
    )
}

fn check_lit(c: &Check) -> String {
    let Check {
        test,
        rule,
        cost,
        slot,
        counter,
        arg,
        len,
    } = *c;
    let test = match test {
        Test::Fails => "SV_TEST_FAILS",
        Test::Integer => "SV_TEST_INTEGER",
        Test::Mask => "SV_TEST_MASK",
        Test::Dependent => "SV_TEST_DEPENDENT",
        Test::CountMin => "SV_TEST_COUNT_MIN",
        Test::CountMax => "SV_TEST_COUNT_MAX",
        Test::Unset => "SV_TEST_UNSET",
        Test::LengthMin => "SV_TEST_LENGTH_MIN",
        Test::LengthMax => "SV_TEST_LENGTH_MAX",
        Test::NumMin => "SV_TEST_NUM_MIN",
        Test::NumExMin => "SV_TEST_NUM_EX_MIN",
        Test::NumMax => "SV_TEST_NUM_MAX",
        Test::NumExMax => "SV_TEST_NUM_EX_MAX",
        Test::NumEnum => "SV_TEST_NUM_ENUM",
        Test::MultipleOf => "SV_TEST_MULTIPLE_OF",
        Test::Unique => "SV_TEST_UNIQUE",
        Test::NumIn => "SV_TEST_NUM_IN",
    };
    format!(
        "{{{test}, {rule}, {cost}, {}u, {counter}, {}, {}}}",
        slot,
        n(arg),
        n(len)
    )
}

fn frame_lit(f: &Frame) -> String {
    let Frame {
        words,
        mask_words,
        slots,
        gates,
        n_gates,
        unique,
    } = *f;
    format!(
        "{{{words}, {mask_words}, {slots}, {gates}, {n_gates}, {}}}",
        bool_c(unique)
    )
}

fn gate_lit(g: &Gate) -> String {
    let Gate {
        kind,
        rule,
        target,
        inputs,
        n_inputs,
        bit,
    } = *g;
    let kind = match kind {
        GateKind::Any => "SV_GATE_ANY",
        GateKind::One => "SV_GATE_ONE",
        GateKind::Not => "SV_GATE_NOT",
        GateKind::Implies => "SV_GATE_IMPLIES",
    };
    format!(
        "{{{kind}, {rule}, {target}, {inputs}, {n_inputs}, {}}}",
        n(bit)
    )
}

/// Las constantes `pub const NOMBRE: usize|u8 = N;` de una fuente del lexer
/// (estados `S_*`, acciones `A_*`, clases `C_*`, `NUM_*`, `ACCEPT`,
/// `REJECT`), en su orden. Se leen de la fuente incrustada para que el C use
/// los mismos números que Rust sin copiarlos a mano.
pub fn constantes(archivo: &str) -> Vec<(&'static str, u32)> {
    let fuente = LEXER
        .archivos
        .iter()
        .find(|(n, _)| *n == archivo)
        .map(|(_, f)| *f)
        .expect("archivo del lexer");
    fuente
        .lines()
        .filter_map(|l| {
            let r = l.strip_prefix("pub const ")?;
            let (nombre, resto) = r.split_once(": ")?;
            // El valor va hasta el `;` (detrás puede haber un comentario).
            let valor = resto
                .strip_prefix("usize = ")
                .or_else(|| resto.strip_prefix("u8 = "))?
                .split_once(';')?
                .0;
            let ok = nombre.starts_with("S_")
                || nombre.starts_with("A_")
                || nombre.starts_with("C_")
                || matches!(nombre, "NUM_STATES" | "NUM_CLASSES" | "ACCEPT" | "REJECT");
            ok.then_some((nombre, valor.parse().ok()?))
        })
        .collect()
}

fn lexico_h() -> String {
    let mut o = String::from(CABECERA);
    o.push_str(
        "/* Tablas de los AFD léxicos de la Fase 1 (crates/lexer), volcadas por codegen: el lexer\n * \
         C usa las mismas que el de Rust. */\n#ifndef SV_LEXICO_H\n#define SV_LEXICO_H\n\n\
         #include <stdint.h>\n\ntypedef struct {\n    uint8_t next, action;\n} sv_tr_t;\n\n",
    );
    for (archivo, prefijo) in [("utf8", "SV_U8"), ("json", "SV_JS"), ("xml", "SV_XL")] {
        for (nombre, valor) in constantes(archivo) {
            let _ = writeln!(o, "#define {prefijo}_{nombre} {valor}");
        }
        o.push('\n');
    }
    o.push_str(
        "extern const uint8_t sv_u8_class[256];\n\
         extern const uint8_t sv_u8_table[SV_U8_NUM_STATES][SV_U8_NUM_CLASSES];\n\
         extern const uint8_t sv_js_class[256];\n\
         extern const sv_tr_t sv_js_table[SV_JS_NUM_STATES][SV_JS_NUM_CLASSES];\n\
         extern const uint8_t sv_xl_class[256];\n\
         extern const sv_tr_t sv_xl_table[SV_XL_NUM_STATES][SV_XL_NUM_CLASSES];\n\n\
         #endif /* SV_LEXICO_H */\n",
    );
    o
}

fn lexico_c() -> String {
    let mut o = String::from(CABECERA);
    o.push_str("#include \"sv_lexico.h\"\n\n");
    clases(&mut o, "sv_u8_class", &lexer::utf8::CLASS);
    o.push_str("const uint8_t sv_u8_table[SV_U8_NUM_STATES][SV_U8_NUM_CLASSES] = {\n");
    for fila in &lexer::utf8::TABLE {
        let f: Vec<String> = fila.iter().map(u8::to_string).collect();
        let _ = writeln!(o, "    {{{}}},", f.join(", "));
    }
    o.push_str("};\n\n");
    clases(&mut o, "sv_js_class", &lexer::json::CLASS);
    tabla_tr(
        &mut o,
        "sv_js_table",
        "SV_JS",
        lexer::json::TABLE.iter().map(|f| &f[..]),
    );
    clases(&mut o, "sv_xl_class", &lexer::xml::CLASS);
    tabla_tr(
        &mut o,
        "sv_xl_table",
        "SV_XL",
        lexer::xml::TABLE.iter().map(|f| &f[..]),
    );
    o
}

fn clases(o: &mut String, nombre: &str, c: &[u8; 256]) {
    let _ = writeln!(o, "const uint8_t {nombre}[256] = {{");
    for f in filas_u8(c) {
        let _ = writeln!(o, "    {f},");
    }
    o.push_str("};\n\n");
}

fn tabla_tr<'a>(
    o: &mut String,
    nombre: &str,
    prefijo: &str,
    filas: impl Iterator<Item = &'a [lexer::Tr]>,
) {
    let _ = writeln!(
        o,
        "const sv_tr_t {nombre}[{prefijo}_NUM_STATES][{prefijo}_NUM_CLASSES] = {{"
    );
    for fila in filas {
        let f: Vec<String> = fila
            .iter()
            .map(|t| format!("{{{}, {}}}", t.next, t.action))
            .collect();
        let _ = writeln!(o, "    {{{}}},", f.join(", "));
    }
    o.push_str("};\n\n");
}
