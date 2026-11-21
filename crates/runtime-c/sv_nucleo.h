/*
 * Núcleo C11 del validador generado (F5.2, D75; docs/fase5/codegen-c.md).
 *
 * Es el port del ejecutor de runtime-rs (crates/runtime-rs/src/core.rs, json.rs,
 * xml.rs, num.rs, hash.rs) y de los lexers de la Fase 1 (crates/lexer/src): las
 * mismas tablas de M (tablas.c, emitidas por codegen), las mismas tablas de los
 * AFD léxicos (sv_lexico.c, volcadas desde el crate lexer) y la misma lógica,
 * función por función. Sin malloc, sin recursión: toda la memoria es la del
 * llamador (sv_state_t en validador.h). La prueba diferencial (F5.3) compara el
 * reporte con el de Rust en todo el corpus.
 */
#ifndef SV_NUCLEO_H
#define SV_NUCLEO_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "sv_lexico.h"

/* ------------------------------------------------------------- tablas de M --- */

/* Clases fijas de Σ (runtime_rs::tables::class y xclass). */
#define SV_CLASS_BEGIN_OBJECT 0u
#define SV_CLASS_END_OBJECT 1u
#define SV_CLASS_BEGIN_ARRAY 2u
#define SV_CLASS_END_ARRAY 3u
#define SV_CLASS_COLON 4u
#define SV_CLASS_COMMA 5u
#define SV_CLASS_TRUE 6u
#define SV_CLASS_FALSE 7u
#define SV_CLASS_NULL 8u
#define SV_CLASS_STRING_BEGIN 9u
#define SV_CLASS_STRING_END 10u
#define SV_CLASS_NUMBER_BEGIN 11u
#define SV_CLASS_NUMBER_BYTE 12u
#define SV_CLASS_NUMBER_END 13u
#define SV_CLASS_CHAR 14u

#define SV_XCLASS_START_TAG 0u
#define SV_XCLASS_END_TAG 1u
#define SV_XCLASS_ATTR 2u
#define SV_XCLASS_VALUE_BEGIN 3u
#define SV_XCLASS_VALUE_END 4u
#define SV_XCLASS_TAG_CLOSE 5u
#define SV_XCLASS_EMPTY_CLOSE 6u
#define SV_XCLASS_END_CLOSE 7u
#define SV_XCLASS_PI_OPEN 8u
#define SV_XCLASS_PI_CLOSE 9u
#define SV_XCLASS_PI_BODY 10u

#define SV_NO_BIT UINT32_MAX
#define SV_OUT_IDENTITY UINT32_MAX
#define SV_SLOT_CHILD (1u << 31)

/* Op (con Fail(Fault) separado en op + fault). */
enum { SV_OP_MOVE, SV_OP_PUSH, SV_OP_POP, SV_OP_ENTER, SV_OP_LEAVE, SV_OP_FAIL };
enum { SV_FAULT_SYNTAX, SV_FAULT_DUPLICATE, SV_FAULT_LIMIT, SV_FAULT_INTERNAL };
enum {
    SV_TEST_FAILS,
    SV_TEST_INTEGER,
    SV_TEST_MASK,
    SV_TEST_DEPENDENT,
    SV_TEST_COUNT_MIN,
    SV_TEST_COUNT_MAX,
    SV_TEST_UNSET,
    SV_TEST_LENGTH_MIN,
    SV_TEST_LENGTH_MAX,
    SV_TEST_NUM_MIN,
    SV_TEST_NUM_EX_MIN,
    SV_TEST_NUM_MAX,
    SV_TEST_NUM_EX_MAX,
    SV_TEST_NUM_ENUM,
    SV_TEST_MULTIPLE_OF,
    SV_TEST_UNIQUE,
    SV_TEST_NUM_IN
};
enum { SV_GATE_ANY, SV_GATE_ONE, SV_GATE_NOT, SV_GATE_IMPLIES };
enum { SV_COUNT_ADD, SV_COUNT_RESTART };

typedef struct {
    uint8_t op;
    uint8_t fault;
    uint32_t next, ret, frame, bit, mark, counts, n_counts, checks, n_checks, outs;
} sv_trans_t;

typedef struct {
    uint8_t test;
    uint32_t rule, cost, slot, counter, arg, len;
} sv_check_t;

typedef struct {
    uint32_t words, mask_words, slots, gates, n_gates;
    bool unique;
} sv_frame_t;

typedef struct {
    uint8_t kind;
    uint32_t rule, target, inputs, n_inputs, bit;
} sv_gate_t;

typedef struct {
    uint32_t off, len;
} sv_outmap_t;

typedef struct {
    uint32_t counter;
    uint8_t op;
} sv_counter_op_t;

typedef struct {
    uint32_t lo, hi;
    bool closed;
} sv_interval_t;

typedef struct {
    uint32_t bit, mask;
} sv_dep_t;

typedef struct {
    uint32_t off, len;
} sv_lexeme_t;

typedef struct {
    uint32_t lexeme;
    uint64_t coprime;
    uint32_t twos, fives;
} sv_divisor_t;

typedef struct {
    uint32_t lo, hi, class_;
} sv_char_range_t;

typedef struct {
    uint32_t node, kind, loc, loc_len;
} sv_rule_t;

/* Las tablas de M: cada tabla es puntero + longitud (los índices se comprueban
 * como los `get` de Rust; un índice fuera de rango es un error interno). */
typedef struct {
    uint32_t n_classes, start, accept;
    const uint32_t* delta;
    size_t n_delta;
    const sv_trans_t* trans;
    size_t n_trans;
    const sv_check_t* checks;
    size_t n_checks;
    const sv_frame_t* frames;
    size_t n_frames;
    const uint64_t* masks;
    size_t n_masks;
    const sv_dep_t* deps;
    size_t n_deps;
    const sv_lexeme_t* nums;
    size_t n_nums;
    const uint8_t* lexemes;
    size_t n_lexemes;
    const sv_divisor_t* divisors;
    size_t n_divisors;
    const sv_counter_op_t* counter_ops;
    size_t n_counter_ops;
    const sv_interval_t* intervals;
    size_t n_intervals;
    const sv_gate_t* gates;
    size_t n_gates;
    const uint32_t* gate_inputs;
    size_t n_gate_inputs;
    const sv_outmap_t* outmaps;
    size_t n_outmaps;
    const uint32_t* out_targets;
    size_t n_out_targets;
    uint32_t scalar_slots;
    const uint32_t* ascii;
    size_t n_ascii;
    const sv_char_range_t* ranges;
    size_t n_ranges;
    const sv_rule_t* rules;
    size_t n_rules;
    const uint8_t* text;
    size_t n_text;
} sv_tables_t;

/* ------------------------------------------------------- opciones y reporte --- */

/* Modo de ejecución (runtime_rs::Mode). */
#define SV_MODE_TOTAL 0
#define SV_MODE_EARLY_FAIL 1

/* Límites y modo (runtime_rs::Options { mode, limits }). */
typedef struct {
    int mode;
    size_t max_depth;
    uint64_t max_string;
    size_t max_keys;
    size_t key_pool;
    size_t max_unique;
    uint64_t max_xml_refs;
    uint64_t max_payload;
    uint64_t hash_key[2];
} sv_limits_t;

/* Los de docs/00-alcance.md §4 (runtime_rs::Limits::DEFAULT), en conformidad
 * total. */
void sv_limits_default(sv_limits_t* lim);

/* Clase del error (runtime_rs::ErrorKind; 0 = ninguno). */
#define SV_KIND_NONE 0
#define SV_KIND_LEXICAL 1
#define SV_KIND_SYNTAX 2
#define SV_KIND_SCHEMA 3
#define SV_KIND_LIMIT 4
#define SV_KIND_INTERNAL 5

/* Peso ∞ (runtime_rs::Weight::INFINITY). */
#define SV_INFINITY UINT64_MAX

/* El resultado (runtime_rs::Report). */
typedef struct {
    bool valid;
    uint64_t violations;
    int error_kind;
    bool has_first;
    uint64_t offset;
    uint32_t line, column;
    bool has_rule;
    uint32_t rule;
} sv_report_t;

/* ------------------------------------------------------------ piezas internas --- */

typedef struct {
    uint64_t offset;
    uint32_t line, column;
} sv_pos_t;

typedef struct {
    sv_pos_t start;
    uint64_t end;
} sv_span_t;

typedef struct {
    uint8_t state;
    uint32_t cp;
} sv_utf8_t;

typedef struct {
    uint64_t v0, v1, v2, v3, tail;
    uint32_t ntail;
    uint64_t len;
} sv_sip_t;

typedef struct {
    bool nonzero;
    uint64_t zeros, frac, exp;
    bool exp_neg;
} sv_number_t;

#define SV_LEXEME_MAX 1024
#define SV_FRAME_OVERHEAD 4

typedef struct {
    const sv_tables_t* t;
    uint64_t* mem;
    size_t len;
    size_t base, sp;
    sv_limits_t limits;
    size_t pool_top;
    uint64_t next_wf;
    bool xml, key_next, in_key;
    int name;
    sv_sip_t hasher;
    uint32_t colons;
    bool first_colon, last_colon;
    uint64_t name_hash;
    sv_pos_t dup_at;
    size_t tag_region;
    uint64_t refs;
    bool prev_cr;
    uint64_t run;
    uint32_t hash_depth;
    size_t unique_count;
    sv_sip_t vhash;
    bool scalar_open;
    uint32_t scalar_frame, scalar_outs;
    size_t depth, max_depth;
    uint32_t state, scalar_ret;
    sv_number_t num;
    uint8_t lex[SV_LEXEME_MAX];
    size_t lex_len;
    uint64_t chars;
    bool pi_target;
    int mode;
    uint64_t weight;
    bool has_first;
    uint64_t first_offset;
    uint32_t first_line, first_column;
    bool first_has_rule;
    uint32_t first_rule;
    int fault;
    bool stopped;
} sv_core_t;

typedef struct {
    uint8_t state;
    sv_pos_t pos, tok_start, char_start;
    uint32_t hex, high;
    sv_utf8_t utf8;
    uint8_t bom;
    bool failed, aborted;
    sv_pos_t failed_at;
} sv_json_lexer_t;

typedef struct {
    uint8_t state;
    sv_pos_t pos, tok_start, char_start, val_start;
    sv_utf8_t utf8;
    uint8_t ref_ret;
    uint8_t ref_name[4];
    uint8_t ref_len;
    uint32_t ref_val;
    bool failed, aborted;
    sv_pos_t failed_at;
} sv_xml_lexer_t;

/* Validador en streaming: lexer + núcleo (runtime_rs::JsonValidator o
 * XmlValidator según `xml`). */
typedef struct {
    bool xml;
    sv_json_lexer_t json;
    sv_xml_lexer_t xmll;
    sv_core_t core;
    uint64_t fed, max_payload;
} sv_validator_t;

/* Palabras de memoria del núcleo para `lim` (Tables::state_words). */
size_t sv_state_words(const sv_tables_t* t, const sv_limits_t* lim);

/* new: `mem` de `len` palabras; `xml` elige el lexer. */
void sv_validator_init(sv_validator_t* v, const sv_tables_t* t, bool xml, const sv_limits_t* lim,
                       uint64_t* mem, size_t len);
/* feed: 0 = sigue, -1 = se detuvo (error de peso ∞ o fallo temprano). */
int sv_validator_feed(sv_validator_t* v, const uint8_t* chunk, size_t len);
/* finish: el reporte. */
void sv_validator_finish(sv_validator_t* v, sv_report_t* out);

/* Ubicación recortada de la regla `rule` en el esquema (Tables::location). */
const uint8_t* sv_rule_location(const sv_tables_t* t, uint32_t rule, size_t* len);

#endif /* SV_NUCLEO_H */
