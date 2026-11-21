/*
 * Núcleo C11 del validador generado (F5.2, D75; docs/fase5/codegen-c.md).
 *
 * Port función por función de crates/runtime-rs/src/{core,json,xml,num,hash}.rs
 * y de crates/lexer/src/{utf8,json,xml}.rs (los nombres de las funciones siguen
 * a los de Rust). Las tablas de los AFD léxicos son las del crate lexer, volcadas
 * en sv_lexico.c por codegen. Sin malloc ni recursión: la pila de marcos y la
 * reserva de hashes son la memoria del llamador. Lo que en Rust es `get` (un
 * índice que puede no existir) aquí es una comprobación explícita que da un
 * error interno, nunca una lectura fuera de rango.
 *
 * Diferencias con Rust, sin efecto en el resultado (las vigila F5.3):
 *  - num: Rust usa i128/u128; aquí int64_t con exp_diff saturado en ±2^62 y
 *    exacto hasta 18 cifras (Rust: 2^100 y 31). Solo importan su signo y su
 *    valor cuando es pequeño: si el resultado no es exacto, |valor| ≥ 10^18 en
 *    los dos, muy por encima de los desplazamientos que se le suman (< 2^31).
 *    El resto de 5^27 se calcula por duplicaciones para no pasar de 64 bits.
 *  - uniqueItems ordena con heapsort (Rust: sort_unstable); solo se miran
 *    los iguales adyacentes, así que el orden exacto no cambia nada.
 */
#include "sv_nucleo.h"

#include <string.h>

#define STOP (-1)

/* ------------------------------------------------------------- aritmética --- */

static uint64_t sat_add64(uint64_t a, uint64_t b) {
    return a > UINT64_MAX - b ? UINT64_MAX : a + b;
}
static uint64_t sat_sub64(uint64_t a, uint64_t b) {
    return a > b ? a - b : 0;
}
static size_t sat_addz(size_t a, size_t b) {
    return a > SIZE_MAX - b ? SIZE_MAX : a + b;
}
static size_t sat_subz(size_t a, size_t b) {
    return a > b ? a - b : 0;
}
static size_t sat_mulz(size_t a, size_t b) {
    return (a != 0 && b > SIZE_MAX / a) ? SIZE_MAX : a * b;
}
static uint32_t sat_add32(uint32_t a, uint32_t b) {
    return a > UINT32_MAX - b ? UINT32_MAX : a + b;
}
static size_t minz(size_t a, size_t b) {
    return a < b ? a : b;
}

/* Semianillo tropical (weight.rs). */
#define W_MAX_FINITE (UINT64_MAX - 1)
static uint64_t w_finite(uint64_t n) {
    return n > W_MAX_FINITE ? W_MAX_FINITE : n;
}
static uint64_t w_times(uint64_t a, uint64_t b) {
    if (a == SV_INFINITY || b == SV_INFINITY) {
        return SV_INFINITY;
    }
    return w_finite(sat_add64(a, b));
}

/* --------------------------------------------------------------- posiciones --- */

static const sv_pos_t POS_START = {0, 1, 1};

static void pos_advance(sv_pos_t* p, uint8_t b) {
    p->offset += 1;
    if (b == '\n') {
        p->line = sat_add32(p->line, 1);
        p->column = 1;
    } else if ((b & 0xC0) != 0x80) {
        p->column = sat_add32(p->column, 1);
    }
}

/* La posición `bytes` bytes y `cols` columnas después de `p` (core.rs `after`). */
static sv_pos_t after(sv_pos_t p, uint64_t bytes, uint64_t cols) {
    sv_pos_t r;
    uint64_t c = sat_add64(p.column, cols);
    r.offset = sat_add64(p.offset, bytes);
    r.line = p.line;
    r.column = c > UINT32_MAX ? UINT32_MAX : (uint32_t)c;
    return r;
}

/* xml.rs `back`. */
static sv_pos_t back(sv_pos_t p, uint32_t k) {
    sv_pos_t r;
    r.offset = sat_sub64(p.offset, k);
    r.line = p.line;
    r.column = p.column == UINT32_MAX ? UINT32_MAX : (p.column > k ? p.column - k : 0);
    return r;
}

/* ------------------------------------------------------------------ Unicode --- */

static bool is_scalar(uint32_t cp) {
    return cp <= 0x10FFFF && !(cp >= 0xD800 && cp <= 0xDFFF);
}

static uint64_t utf8_len(uint32_t c) {
    return c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
}

static size_t encode_utf8(uint32_t c, uint8_t buf[4]) {
    if (c < 0x80) {
        buf[0] = (uint8_t)c;
        return 1;
    }
    if (c < 0x800) {
        buf[0] = (uint8_t)(0xC0 | (c >> 6));
        buf[1] = (uint8_t)(0x80 | (c & 0x3F));
        return 2;
    }
    if (c < 0x10000) {
        buf[0] = (uint8_t)(0xE0 | (c >> 12));
        buf[1] = (uint8_t)(0x80 | ((c >> 6) & 0x3F));
        buf[2] = (uint8_t)(0x80 | (c & 0x3F));
        return 3;
    }
    buf[0] = (uint8_t)(0xF0 | (c >> 18));
    buf[1] = (uint8_t)(0x80 | ((c >> 12) & 0x3F));
    buf[2] = (uint8_t)(0x80 | ((c >> 6) & 0x3F));
    buf[3] = (uint8_t)(0x80 | (c & 0x3F));
    return 4;
}

/* AFD UTF-8 (utf8.rs). */
enum { U8_CHAR, U8_PENDING, U8_INVALID };

static void u8_new(sv_utf8_t* d) {
    d->state = SV_U8_ACCEPT;
    d->cp = 0;
}

static bool u8_pending(const sv_utf8_t* d) {
    return d->state != SV_U8_ACCEPT;
}

static int u8_step(sv_utf8_t* d, uint8_t b, uint32_t* c) {
    unsigned cls = sv_u8_class[b];
    if (d->state == SV_U8_ACCEPT) {
        uint32_t m = b <= 0x7F ? 0x7F : b <= 0xDF ? 0x1F : b <= 0xEF ? 0x0F : 0x07;
        d->cp = b & m;
    } else {
        d->cp = (d->cp << 6) | (uint32_t)(b & 0x3F);
    }
    d->state = sv_u8_table[d->state][cls];
    if (d->state == SV_U8_ACCEPT) {
        if (is_scalar(d->cp)) {
            *c = d->cp;
            return U8_CHAR;
        }
        u8_new(d);
        return U8_INVALID;
    }
    if (d->state == SV_U8_REJECT) {
        u8_new(d);
        return U8_INVALID;
    }
    return U8_PENDING;
}

static uint32_t hex_value(uint8_t b) {
    if (b >= '0' && b <= '9') {
        return (uint32_t)(b - '0');
    }
    if (b >= 'a' && b <= 'f') {
        return (uint32_t)(b - 'a' + 10);
    }
    return (uint32_t)(b - 'A' + 10);
}

/* ------------------------------------------------------------ SipHash-1-3 --- */

static uint64_t rotl(uint64_t x, int r) {
    return (x << r) | (x >> (64 - r));
}

static void sip_new(sv_sip_t* h, const uint64_t key[2]) {
    h->v0 = key[0] ^ 0x736f6d6570736575ULL;
    h->v1 = key[1] ^ 0x646f72616e646f6dULL;
    h->v2 = key[0] ^ 0x6c7967656e657261ULL;
    h->v3 = key[1] ^ 0x7465646279746573ULL;
    h->tail = 0;
    h->ntail = 0;
    h->len = 0;
}

static void sip_round(sv_sip_t* s) {
    s->v0 += s->v1;
    s->v1 = rotl(s->v1, 13);
    s->v1 ^= s->v0;
    s->v0 = rotl(s->v0, 32);
    s->v2 += s->v3;
    s->v3 = rotl(s->v3, 16);
    s->v3 ^= s->v2;
    s->v0 += s->v3;
    s->v3 = rotl(s->v3, 21);
    s->v3 ^= s->v0;
    s->v2 += s->v1;
    s->v1 = rotl(s->v1, 17);
    s->v1 ^= s->v2;
    s->v2 = rotl(s->v2, 32);
}

static void sip_compress(sv_sip_t* s, uint64_t m) {
    s->v3 ^= m;
    sip_round(s);
    s->v0 ^= m;
}

static void sip_u8(sv_sip_t* s, uint8_t b) {
    s->tail |= (uint64_t)b << (8 * s->ntail);
    s->ntail += 1;
    s->len += 1;
    if (s->ntail == 8) {
        sip_compress(s, s->tail);
        s->tail = 0;
        s->ntail = 0;
    }
}

static void sip_u64le(sv_sip_t* s, uint64_t x) {
    for (int i = 0; i < 8; i++) {
        sip_u8(s, (uint8_t)(x >> (8 * i)));
    }
}

static void sip_char(sv_sip_t* s, uint32_t c) {
    uint8_t buf[4];
    size_t n = encode_utf8(c, buf);
    for (size_t i = 0; i < n; i++) {
        sip_u8(s, buf[i]);
    }
}

static uint64_t sip_finish(const sv_sip_t* h) {
    sv_sip_t s = *h;
    uint64_t m = s.tail | ((s.len & 0xFF) << 56);
    sip_compress(&s, m);
    s.v2 ^= 0xFF;
    sip_round(&s);
    sip_round(&s);
    sip_round(&s);
    return s.v0 ^ s.v1 ^ s.v2 ^ s.v3;
}

/* ---------------------------------------------------------------- números --- */

typedef struct {
    const uint8_t* p;
    size_t n;
} sl_t;

typedef struct {
    bool neg;
    sl_t ip, frac, exp;
    bool exp_neg;
} parts_t;

static sl_t sl(const uint8_t* p, size_t n) {
    sl_t s;
    s.p = p;
    s.n = n;
    return s;
}

static parts_t parts_new(const uint8_t* l, size_t n) {
    parts_t r;
    sl_t rest = sl(l, n), mant, exp;
    size_t i;
    r.neg = false;
    if (n > 0 && l[0] == '-') {
        r.neg = true;
        rest = sl(l + 1, n - 1);
    } else if (n > 0 && l[0] == '+') {
        rest = sl(l + 1, n - 1);
    }
    for (i = 0; i < rest.n && rest.p[i] != 'e' && rest.p[i] != 'E'; i++) {
    }
    if (i < rest.n) {
        mant = sl(rest.p, i);
        exp = sl(rest.p + i + 1, rest.n - i - 1);
    } else {
        mant = rest;
        exp = sl(rest.p + rest.n, 0);
    }
    for (i = 0; i < mant.n && mant.p[i] != '.'; i++) {
    }
    if (i < mant.n) {
        r.ip = sl(mant.p, i);
        r.frac = sl(mant.p + i + 1, mant.n - i - 1);
    } else {
        r.ip = mant;
        r.frac = sl(mant.p + mant.n, 0);
    }
    r.exp_neg = false;
    if (exp.n > 0 && exp.p[0] == '-') {
        r.exp_neg = true;
        exp = sl(exp.p + 1, exp.n - 1);
    } else if (exp.n > 0 && exp.p[0] == '+') {
        exp = sl(exp.p + 1, exp.n - 1);
    }
    r.exp = exp;
    return r;
}

static uint8_t dig(uint8_t c) {
    uint8_t d = (uint8_t)(c - '0');
    return d > 9 ? 9 : d;
}

static size_t parts_total(const parts_t* p) {
    return p->ip.n + p->frac.n;
}

static uint8_t parts_digit(const parts_t* p, size_t i) {
    return i < p->ip.n ? dig(p->ip.p[i]) : dig(p->frac.p[i - p->ip.n]);
}

static size_t parts_lead(const parts_t* p) {
    size_t n = parts_total(p), i = 0;
    while (i < n && parts_digit(p, i) == 0) {
        i++;
    }
    return i;
}

static size_t parts_trailing(const parts_t* p) {
    size_t n = parts_total(p), k = 0;
    while (k < n && parts_digit(p, n - 1 - k) == 0) {
        k++;
    }
    return k;
}

static int parts_sign(const parts_t* p, size_t lead) {
    if (lead == parts_total(p)) {
        return 0;
    }
    return p->neg ? -1 : 1;
}

static sl_t strip(sl_t d) {
    size_t i = 0;
    while (i < d.n && d.p[i] == '0') {
        i++;
    }
    return sl(d.p + i, d.n - i);
}

#define SMALL 18
#define CAP ((int64_t)1 << 62)

static int64_t small_val(sl_t d) {
    int64_t v = 0;
    for (size_t i = 0; i < d.n; i++) {
        v = v * 10 + dig(d.p[i]);
    }
    return v;
}

static int cmp_mag(sl_t a, sl_t b) {
    if (a.n != b.n) {
        return a.n < b.n ? -1 : 1;
    }
    if (a.n == 0) {
        return 0;
    }
    int c = memcmp(a.p, b.p, a.n);
    return c < 0 ? -1 : c > 0 ? 1 : 0;
}

static int64_t sub_clamped(sl_t x, sl_t y) {
    int borrow = 0;
    int64_t val = 0, pow = 1;
    bool big = false;
    for (size_t i = 0; i < x.n; i++) {
        int dx = dig(x.p[x.n - 1 - i]);
        int dy = i < y.n ? dig(y.p[y.n - 1 - i]) : 0;
        int d = dx - dy - borrow;
        borrow = 0;
        if (d < 0) {
            d += 10;
            borrow = 1;
        }
        if (i < SMALL) {
            val += (int64_t)d * pow;
            pow *= 10;
        } else if (d != 0) {
            big = true;
        }
    }
    if (big) {
        return CAP;
    }
    return val < CAP ? val : CAP;
}

static int64_t exp_diff(bool a_neg, sl_t a, bool b_neg, sl_t b) {
    a = strip(a);
    b = strip(b);
    int64_t sa = a.n == 0 ? 0 : (a_neg ? -1 : 1);
    int64_t sb = b.n == 0 ? 0 : (b_neg ? -1 : 1);
    if (a.n <= SMALL && b.n <= SMALL) {
        int64_t v = sa * small_val(a) - sb * small_val(b);
        return v > CAP ? CAP : v < -CAP ? -CAP : v;
    }
    if (sa != sb) {
        return sa != 0 ? sa * CAP : -sb * CAP;
    }
    int c = cmp_mag(a, b);
    if (c == 0) {
        return 0;
    }
    return c > 0 ? sa * sub_clamped(a, b) : -sa * sub_clamped(b, a);
}

static int digits_cmp(const parts_t* x, size_t i, const parts_t* y, size_t j) {
    size_t nx = parts_total(x), ny = parts_total(y);
    for (;;) {
        bool hx = i < nx, hy = j < ny;
        if (!hx && !hy) {
            return 0;
        }
        if (hx && hy) {
            uint8_t a = parts_digit(x, i), b = parts_digit(y, j);
            if (a != b) {
                return a < b ? -1 : 1;
            }
        } else if (hx) {
            if (parts_digit(x, i) != 0) {
                return 1;
            }
        } else if (parts_digit(y, j) != 0) {
            return -1;
        }
        i += hx;
        j += hy;
    }
}

/* num::compare: −1, 0 o 1. */
static int num_compare(const uint8_t* a, size_t an, const uint8_t* b, size_t bn) {
    parts_t x = parts_new(a, an), y = parts_new(b, bn);
    size_t lx = parts_lead(&x), ly = parts_lead(&y);
    int sx = parts_sign(&x, lx), sy = parts_sign(&y, ly);
    if (sx != sy) {
        return sx < sy ? -1 : 1;
    }
    if (sx == 0) {
        return 0;
    }
    int64_t ox = (int64_t)x.ip.n - (int64_t)lx, oy = (int64_t)y.ip.n - (int64_t)ly;
    int64_t e = exp_diff(x.exp_neg, x.exp, y.exp_neg, y.exp) + (ox - oy);
    int mag = e != 0 ? (e > 0 ? 1 : -1) : digits_cmp(&x, lx, &y, ly);
    return x.neg ? -mag : mag;
}

#define P5_27 7450580596923828125ULL

static uint64_t mod_add(uint64_t a, uint64_t b, uint64_t m) {
    /* a, b < m < 2^63: a + b no desborda. */
    uint64_t s = a + b;
    return s >= m ? s - m : s;
}

/* (10·r + v) mod 5^27 sin pasar de 64 bits (r < 5^27 < 2^63). */
static uint64_t mod5_step(uint64_t r, uint64_t v) {
    uint64_t d2 = mod_add(r, r, P5_27);
    uint64_t d4 = mod_add(d2, d2, P5_27);
    uint64_t d8 = mod_add(d4, d4, P5_27);
    uint64_t r10 = mod_add(d8, d2, P5_27);
    return (r10 + v) % P5_27;
}

/* num::is_multiple. */
static bool num_is_multiple(const uint8_t* x, size_t xn, const uint8_t* d, size_t dn,
                            const sv_divisor_t* div) {
    parts_t px = parts_new(x, xn);
    size_t lead = parts_lead(&px), total = parts_total(&px);
    if (lead == total) {
        return true;
    }
    parts_t pd = parts_new(d, dn);
    size_t zx = parts_trailing(&px);
    int64_t ex = (int64_t)zx - (int64_t)px.frac.n;
    int64_t ed = (int64_t)parts_trailing(&pd) - (int64_t)pd.frac.n;
    int64_t k = exp_diff(px.exp_neg, px.exp, pd.exp_neg, pd.exp) + (ex - ed);
    if (k < 0) {
        return false;
    }
    uint64_t m = div->coprime > 1 ? div->coprime : 1;
    uint64_t rm = 0, r2 = 0, r5 = 0;
    for (size_t i = lead; i < total - zx; i++) {
        uint64_t v = parts_digit(&px, i);
        rm = (rm * 10 + v) % m;
        r2 = r2 * 10 + v;
        r5 = mod5_step(r5, v);
    }
    uint32_t kk = k > 64 ? 64 : (uint32_t)k;
    uint32_t need2 = div->twos > kk ? div->twos - kk : 0;
    uint32_t need5 = div->fives > kk ? div->fives - kk : 0;
    if (need2 > 63) {
        need2 = 63;
    }
    if (need5 > 27) {
        need5 = 27;
    }
    uint64_t p5 = 1;
    for (uint32_t i = 0; i < need5; i++) {
        p5 *= 5;
    }
    return rm == 0 && (r2 & ((1ULL << need2) - 1)) == 0 && r5 % p5 == 0;
}

/* Partes del lexema de un número JSON (lexer::json::NumPart). */
enum { NP_MINUS, NP_INT, NP_DOT, NP_FRAC, NP_EXP_MARK, NP_EXP_SIGN, NP_EXP };

#define EXP_CAP ((uint64_t)1 << 62)

static void number_byte(sv_number_t* n, int part, uint8_t b) {
    uint64_t d = (uint8_t)(b - '0');
    switch (part) {
    case NP_INT:
    case NP_FRAC:
        if (part == NP_FRAC) {
            n->frac = sat_add64(n->frac, 1);
        }
        if (d == 0) {
            n->zeros = sat_add64(n->zeros, 1);
        } else {
            n->nonzero = true;
            n->zeros = 0;
        }
        break;
    case NP_EXP_SIGN:
        n->exp_neg = b == '-';
        break;
    case NP_EXP:
        if (n->exp > EXP_CAP / 10) {
            n->exp = EXP_CAP;
        } else {
            uint64_t e = n->exp * 10 + d;
            n->exp = e < EXP_CAP ? e : EXP_CAP;
        }
        break;
    default:
        break;
    }
}

/* e + zeros ≥ frac, con e = ±exp (Number::is_integer, sin i128). */
static bool number_is_integer(const sv_number_t* n) {
    if (!n->nonzero) {
        return true;
    }
    if (n->exp_neg) {
        return n->zeros >= n->frac && n->zeros - n->frac >= n->exp;
    }
    return n->zeros >= n->frac || n->frac - n->zeros <= n->exp;
}

/* ------------------------------------------------------------------ tablas --- */

/* Tables::char_class. */
static uint32_t char_class(const sv_tables_t* t, uint32_t cp) {
    if (cp < 128) {
        return cp < t->n_ascii ? t->ascii[cp] : 0;
    }
    size_t lo = 0, hi = t->n_ranges;
    while (lo < hi) {
        size_t mid = lo + (hi - lo) / 2;
        if (t->ranges[mid].hi < cp) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if (lo < t->n_ranges && t->ranges[lo].lo <= cp) {
        return t->ranges[lo].class_;
    }
    return 0;
}

/* Tables::num. */
static bool table_num(const sv_tables_t* t, uint32_t i, const uint8_t** p, size_t* n) {
    if (i >= t->n_nums) {
        return false;
    }
    uint64_t ini = t->nums[i].off, fin = ini + t->nums[i].len;
    if (fin > t->n_lexemes) {
        return false;
    }
    *p = t->lexemes + ini;
    *n = (size_t)(fin - ini);
    return true;
}

const uint8_t* sv_rule_location(const sv_tables_t* t, uint32_t rule, size_t* len) {
    *len = 0;
    if (rule >= t->n_rules) {
        return t->text;
    }
    size_t ini = t->rules[rule].loc;
    size_t fin = sat_addz(ini, t->rules[rule].loc_len);
    if (fin > t->n_text) {
        return t->text;
    }
    *len = fin - ini;
    return t->text + ini;
}

size_t sv_state_words(const sv_tables_t* t, const sv_limits_t* lim) {
    size_t frame = 0;
    for (size_t i = 0; i < t->n_frames; i++) {
        if (t->frames[i].words > frame) {
            frame = t->frames[i].words;
        }
    }
    frame = sat_addz(frame, SV_FRAME_OVERHEAD);
    size_t w = sat_addz(t->scalar_slots, sat_mulz(lim->max_depth, frame));
    return sat_addz(sat_addz(w, lim->key_pool), lim->max_unique);
}

void sv_limits_default(sv_limits_t* lim) {
    lim->mode = SV_MODE_TOTAL;
    lim->max_depth = 256;
    lim->max_string = (uint64_t)1 << 20;
    lim->max_lexeme = 1024;
    lim->max_keys = 1024;
    lim->key_pool = 4096;
    lim->max_unique = 4096;
    lim->max_xml_refs = 100000;
    lim->max_payload = UINT64_MAX;
    lim->hash_key[0] = 0x53565f4b45595f30ULL;
    lim->hash_key[1] = 0x7369706861736831ULL;
}

/* ------------------------------------------------------------------- núcleo --- */

enum { N_NONE, N_ELEM, N_END, N_ATTR, N_PI };
enum { SP_ZONE, SP_TOP };

#define WF_OBJECT ((uint64_t)1 << 63)
#define TAG_TRUE 0x74727565ULL
#define TAG_FALSE 0x66616c73ULL
#define TAG_NULL 0x6e756c6cULL
#define TAG_OBJECT 0x6f626a65ULL
#define TAG_ARRAY 0x61727261ULL
#define TAG_ZERO 0x7a65726fULL

/* Sin fallo (push). */
#define NO_FAULT (-1)

/* Eventos del lexer JSON (lexer::json::Event). */
enum {
    JE_BEGIN_OBJECT,
    JE_END_OBJECT,
    JE_BEGIN_ARRAY,
    JE_END_ARRAY,
    JE_COLON,
    JE_COMMA,
    JE_TRUE,
    JE_FALSE,
    JE_NULL,
    JE_STRING_BEGIN,
    JE_STRING_CHAR,
    JE_STRING_END,
    JE_NUMBER_BEGIN,
    JE_NUMBER,
    JE_NUMBER_END
};

/* Eventos del lexer XML (lexer::xml::Event). */
enum {
    XE_START_TAG_OPEN,
    XE_END_TAG_OPEN,
    XE_NAME_CHAR,
    XE_ATTR_BEGIN,
    XE_ATTR_VALUE_BEGIN,
    XE_ATTR_VALUE_CHAR,
    XE_ATTR_VALUE_END,
    XE_START_TAG_CLOSE,
    XE_EMPTY_TAG_CLOSE,
    XE_END_TAG_CLOSE,
    XE_TEXT_CHAR,
    XE_PI_OPEN,
    XE_PI_CHAR,
    XE_PI_CLOSE
};

/* ------------------------------------------- puntero del primer error (F6.1) --- */

/* Dónde empieza el último segmento de path[..len] (core.rs `last_slash`). */
static size_t last_slash(const uint8_t* path, size_t len) {
    while (len > 0) {
        len--;
        if (path[len] == '/') {
            return len;
        }
    }
    return 0;
}

static void seg_begin(sv_core_t* s) {
    s->pending = true;
    s->pend_ok = s->pextra == 0 && s->plen < SV_POINTER_MAX;
    s->pend = s->plen;
    if (s->pend_ok) {
        s->path[s->plen] = '/';
        s->pend = s->plen + 1;
    }
}

static void seg_byte(sv_core_t* s, uint8_t b) {
    if (!(s->pending && s->pend_ok)) {
        return;
    }
    if (s->pend < SV_POINTER_MAX) {
        s->path[s->pend] = b;
        s->pend++;
    } else {
        s->pend_ok = false;
    }
}

static void seg_char(sv_core_t* s, uint32_t c, bool escape) {
    if (escape && c == '~') {
        seg_byte(s, '~');
        seg_byte(s, '0');
    } else if (escape && c == '/') {
        seg_byte(s, '~');
        seg_byte(s, '1');
    } else {
        uint8_t buf[4];
        size_t n = encode_utf8(c, buf);
        for (size_t i = 0; i < n; i++) {
            seg_byte(s, buf[i]);
        }
    }
}

static void seg_commit(sv_core_t* s) {
    if (!s->pending) {
        return;
    }
    s->pending = false;
    if (s->pend_ok) {
        s->plen = s->pend;
    } else {
        s->pextra = sat_add32(s->pextra, 1);
    }
}

static void seg_pop(sv_core_t* s) {
    if (s->pextra > 0) {
        s->pextra--;
    } else {
        s->plen = last_slash(s->path, s->plen);
    }
}

static void seg_next(sv_core_t* s) {
    if (s->pextra > 0) {
        return;
    }
    size_t st = last_slash(s->path, s->plen) + 1;
    size_t i = s->plen;
    while (i > st) {
        i--;
        if (s->path[i] == '9') {
            s->path[i] = '0';
        } else {
            s->path[i]++;
            return;
        }
    }
    if (s->plen < SV_POINTER_MAX && st <= s->plen) {
        s->path[st] = '1';
        memset(s->path + st + 1, '0', s->plen - st);
        s->plen++;
    } else {
        s->plen = st > 0 ? st - 1 : 0;
        s->pextra = 1;
    }
}

static void snapshot(sv_core_t* s) {
    size_t len = s->plen;
    uint32_t extra = s->pextra;
    if (s->phide) {
        if (extra > 0) {
            extra--;
        } else {
            len = last_slash(s->path, len);
        }
    }
    memcpy(s->ptr, s->path, len);
    s->ptr_len = len;
    s->ptr_truncated = extra > 0;
}

static void core_stop(sv_core_t* s, int kind, sv_pos_t at) {
    s->weight = SV_INFINITY;
    s->fault = kind;
    s->has_first = true;
    s->first_offset = at.offset;
    s->first_line = at.line;
    s->first_column = at.column;
    s->first_has_rule = false;
    s->first_rule = 0;
    snapshot(s);
    s->stopped = true;
}

static int core_fail(sv_core_t* s, int fault, sv_pos_t at) {
    int kind = fault == SV_FAULT_LIMIT      ? SV_KIND_LIMIT
               : fault == SV_FAULT_INTERNAL ? SV_KIND_INTERNAL
                                            : SV_KIND_SYNTAX;
    core_stop(s, kind, at);
    return STOP;
}

static void core_init(sv_core_t* s, const sv_tables_t* t, const sv_limits_t* lim, uint64_t* mem,
                      size_t len) {
    memset(s, 0, sizeof *s);
    s->t = t;
    s->mem = mem;
    s->len = len;
    s->base = t->scalar_slots;
    s->sp = s->base;
    s->limits = *lim;
    s->name = N_NONE;
    sip_new(&s->hasher, lim->hash_key);
    sip_new(&s->vhash, lim->hash_key);
    s->dup_at = POS_START;
    s->scalar_outs = SV_OUT_IDENTITY;
    s->state = t->start;
    s->mode = lim->mode;
    s->weight = 0;
    s->fault = SV_KIND_NONE;
    if (s->limits.key_pool > UINT32_MAX) {
        s->limits.key_pool = UINT32_MAX;
    }
    if (s->limits.max_lexeme > SV_LEXEME_MAX) {
        s->limits.max_lexeme = SV_LEXEME_MAX;
    }
    if (t->n_classes <= SV_CLASS_CHAR) {
        core_stop(s, SV_KIND_INTERNAL, POS_START);
    }
}

/* Registros del marco de la cima (core.rs `top`). */
static bool top(const sv_core_t* s, size_t* ini, size_t* regs, size_t* mask) {
    if (s->sp <= s->base || s->sp - 1 >= s->len) {
        return false;
    }
    uint64_t fid = s->mem[s->sp - 1] & 0xFFFFFFFFULL;
    if (fid >= s->t->n_frames) {
        return false;
    }
    const sv_frame_t* f = &s->t->frames[fid];
    size_t words = f->words, m = f->mask_words;
    if (f->slots > words) {
        return false;
    }
    size_t r = words - f->slots;
    if (m > r) {
        return false;
    }
    size_t need = words + SV_FRAME_OVERHEAD;
    if (s->sp < need || s->sp - need < s->base) {
        return false;
    }
    *ini = s->sp - need;
    *regs = r;
    *mask = m;
    return true;
}

static bool top_frame(const sv_core_t* s, size_t* ini, const sv_frame_t** f) {
    size_t r, m;
    if (!top(s, ini, &r, &m)) {
        return false;
    }
    *f = &s->t->frames[s->mem[s->sp - 1] & 0xFFFFFFFFULL];
    return true;
}

static uint32_t top_id(const sv_core_t* s) {
    if (s->sp == 0 || s->sp - 1 >= s->len) {
        return UINT32_MAX;
    }
    return (uint32_t)(s->mem[s->sp - 1] & 0xFFFFFFFFULL);
}

static bool top_wf(const sv_core_t* s, uint64_t* w) {
    size_t i, r, m;
    if (!top(s, &i, &r, &m)) {
        return false;
    }
    *w = s->mem[s->sp - 2];
    return true;
}

static bool top_is_object(const sv_core_t* s) {
    uint64_t w;
    return !s->xml && top_wf(s, &w) && (w & WF_OBJECT) != 0;
}

static uint32_t frame_outs(const sv_core_t* s) {
    uint64_t w;
    if (s->xml || !top_wf(s, &w)) {
        return SV_OUT_IDENTITY;
    }
    uint64_t o = (w >> 32) & 0x7FFFFFFFULL;
    return o > 0 ? (uint32_t)(o - 1) : SV_OUT_IDENTITY;
}

static bool counter(const sv_core_t* s, uint32_t k, uint64_t* v) {
    size_t ini, words, mask;
    if (!top(s, &ini, &words, &mask)) {
        return false;
    }
    size_t w = mask + k;
    if (w >= words || ini + w >= s->len) {
        return false;
    }
    *v = s->mem[ini + w];
    return true;
}

static bool counters(sv_core_t* s, const sv_trans_t* tr) {
    size_t ini, words, mask;
    if (!top(s, &ini, &words, &mask)) {
        return false;
    }
    size_t i = tr->counts, n = tr->n_counts;
    if (i + n > s->t->n_counter_ops) {
        return false;
    }
    for (size_t j = i; j < i + n; j++) {
        const sv_counter_op_t* o = &s->t->counter_ops[j];
        size_t w = mask + o->counter;
        if (w >= words || ini + w >= s->len) {
            return false;
        }
        uint64_t* x = &s->mem[ini + w];
        *x = o->op == SV_COUNT_ADD ? sat_add64(*x, 1) : 1;
    }
    return true;
}

static bool bit(const sv_core_t* s, uint32_t b, bool* v) {
    size_t ini, r, words;
    if (!top(s, &ini, &r, &words)) {
        return false;
    }
    size_t w = b / 64;
    if (w >= words) {
        return false;
    }
    *v = (s->mem[ini + w] & ((uint64_t)1 << (b % 64))) != 0;
    return true;
}

static bool set_bit(sv_core_t* s, uint32_t b, bool* nuevo) {
    size_t ini, r, words;
    if (!top(s, &ini, &r, &words)) {
        return false;
    }
    size_t w = b / 64;
    if (w >= words) {
        return false;
    }
    uint64_t m = (uint64_t)1 << (b % 64);
    uint64_t* word = &s->mem[ini + w];
    *nuevo = (*word & m) == 0;
    *word |= m;
    return true;
}

static bool contains(const sv_core_t* s, uint32_t mask, bool* ok) {
    size_t ini, r, words;
    if (!top(s, &ini, &r, &words)) {
        return false;
    }
    size_t m = mask;
    if (m + words > s->t->n_masks || ini + words > s->len) {
        return false;
    }
    *ok = true;
    for (size_t i = 0; i < words; i++) {
        uint64_t w = s->t->masks[m + i];
        if ((s->mem[ini + i] & w) != w) {
            *ok = false;
        }
    }
    return true;
}

static int push(sv_core_t* s, uint32_t frame, uint32_t ret, uint32_t outs) {
    if (frame >= s->t->n_frames) {
        return SV_FAULT_INTERNAL;
    }
    const sv_frame_t* f = &s->t->frames[frame];
    if (s->depth >= s->limits.max_depth) {
        return SV_FAULT_LIMIT;
    }
    size_t size = (size_t)f->words + SV_FRAME_OVERHEAD;
    size_t free_ = sat_subz(s->len, s->pool_top);
    if (s->sp > SIZE_MAX - size || s->sp + size > free_) {
        return SV_FAULT_LIMIT;
    }
    size_t fin = s->sp + size;
    for (size_t i = s->sp; i < fin - 2; i++) {
        s->mem[i] = 0;
    }
    uint64_t wf = (outs == SV_OUT_IDENTITY || s->xml)
                      ? s->next_wf
                      : s->next_wf | ((((uint64_t)outs + 1) & 0x7FFFFFFFULL) << 32);
    s->mem[fin - 2] = wf;
    s->mem[fin - 1] = ((uint64_t)ret << 32) | frame;
    s->sp = fin;
    if (f->unique) {
        s->hash_depth += 1;
    }
    s->depth += 1;
    if (s->depth > s->max_depth) {
        s->max_depth = s->depth;
    }
    return NO_FAULT;
}

static bool pop(sv_core_t* s, uint32_t* ret) {
    size_t ini, r, m;
    if (!top(s, &ini, &r, &m)) {
        return false;
    }
    uint64_t header = s->mem[s->sp - 1];
    if (!s->xml) {
        size_t region = (size_t)(s->mem[s->sp - 2] & 0xFFFFFFFFULL);
        uint64_t fid = header & 0xFFFFFFFFULL;
        if (fid < s->t->n_frames && s->t->frames[fid].unique) {
            s->unique_count -= minz(sat_subz(s->pool_top, region), s->unique_count);
            s->hash_depth = s->hash_depth > 0 ? s->hash_depth - 1 : 0;
        }
        s->pool_top = minz(s->pool_top, region);
    }
    s->sp = ini;
    s->depth -= 1;
    *ret = (uint32_t)(header >> 32);
    return true;
}

static uint64_t h2(const sv_core_t* s, uint64_t a, uint64_t b) {
    sv_sip_t h;
    sip_new(&h, s->limits.hash_key);
    sip_u8(&h, 'h');
    sip_u64le(&h, a);
    sip_u64le(&h, b);
    return sip_finish(&h);
}

static bool container_hash(const sv_core_t* s, uint64_t* out) {
    if (s->sp < 4 || s->sp - 4 >= s->len) {
        return false;
    }
    uint64_t acc = s->mem[s->sp - 4];
    *out = top_is_object(s) ? h2(s, TAG_OBJECT, acc) : h2(s, TAG_ARRAY, acc);
    return true;
}

/* core.rs `number_hash`: false si el lexema no cupo o el exponente es enorme. */
static bool number_hash(const sv_core_t* s, uint64_t* out) {
    if (s->lex_len > s->limits.max_lexeme) {
        return false;
    }
    const uint8_t* lex = s->lex;
    size_t n = s->lex_len;
    bool neg = n > 0 && lex[0] == '-';
    const uint8_t* rest = neg ? lex + 1 : lex;
    size_t rn = neg ? n - 1 : n;
    size_t mant_end = 0;
    while (mant_end < rn && rest[mant_end] != 'e' && rest[mant_end] != 'E') {
        mant_end++;
    }
    const uint8_t* mant = rest;
    size_t mn = mant_end;
    const uint8_t* exp = rest + mant_end;
    size_t en = rn - mant_end;
    size_t int_len = 0;
    while (int_len < mn && mant[int_len] != '.') {
        int_len++;
    }
    bool all_zero = true;
    for (size_t i = 0; i < mn; i++) {
        if (mant[i] != '0' && mant[i] != '.') {
            all_zero = false;
        }
    }
    if (all_zero) {
        *out = h2(s, TAG_ZERO, 0);
        return true;
    }
    int64_t e = 0;
    if (en > 0) {
        const uint8_t* ex = exp + 1;
        size_t xn = en - 1;
        int64_t sign = 1;
        if (xn > 0 && ex[0] == '-') {
            sign = -1;
            ex++;
            xn--;
        } else if (xn > 0 && ex[0] == '+') {
            ex++;
            xn--;
        }
        /* E3A-1: en uint64_t, porque e <= 2^60 antes del paso y
         * e * 10 + 9 < 2^64 no desborda; en int64_t era UB (e * 10 pasa de
         * INT64_MAX) y -O2 dejaba pasar exponentes que Rust da como límite. */
        uint64_t u = 0;
        for (size_t i = 0; i < xn; i++) {
            u = u * 10 + (uint64_t)(uint8_t)(ex[i] - '0');
            if (u > ((uint64_t)1 << 60)) {
                return false;
            }
        }
        e = (int64_t)u;
        e *= sign;
    }
    size_t nd = mn - (int_len < mn ? 1 : 0);
#define DIGIT(i) ((i) < int_len ? mant[(i)] : mant[(i) + 1])
    size_t f = 0;
    while (f < nd && DIGIT(f) == '0') {
        f++;
    }
    if (f == nd) {
        *out = h2(s, TAG_ZERO, 0);
        return true;
    }
    size_t l = nd - 1;
    while (l > f && DIGIT(l) == '0') {
        l--;
    }
    int64_t exp10 = (int64_t)int_len - (int64_t)f + e;
    sv_sip_t h;
    sip_new(&h, s->limits.hash_key);
    sip_u8(&h, 'n');
    sip_u8(&h, neg ? 1 : 0);
    for (size_t i = f; i <= l; i++) {
        sip_u8(&h, DIGIT(i));
    }
#undef DIGIT
    sip_u64le(&h, (uint64_t)exp10);
    *out = sip_finish(&h);
    return true;
}

static int deliver(sv_core_t* s, uint64_t h, sv_pos_t at) {
    size_t ini;
    const sv_frame_t* f;
    if (!top_frame(s, &ini, &f)) {
        return 0;
    }
    size_t acc_i = s->sp - 4, aux_i = s->sp - 3;
    if (top_is_object(s)) {
        uint64_t m = h2(s, s->mem[aux_i], h);
        s->mem[acc_i] += m;
    } else {
        s->mem[acc_i] = h2(s, s->mem[acc_i], h);
        if (f->unique) {
            if (s->unique_count >= s->limits.max_unique || s->len - s->pool_top <= s->sp) {
                return core_fail(s, SV_FAULT_LIMIT, at);
            }
            s->mem[s->len - 1 - s->pool_top] = h;
            s->pool_top += 1;
            s->unique_count += 1;
        }
    }
    return 0;
}

static void sift_down(uint64_t* v, size_t i, size_t n) {
    for (;;) {
        size_t c = 2 * i + 1;
        if (c >= n) {
            return;
        }
        if (c + 1 < n && v[c + 1] > v[c]) {
            c++;
        }
        if (v[i] >= v[c]) {
            return;
        }
        uint64_t x = v[i];
        v[i] = v[c];
        v[c] = x;
        i = c;
    }
}

static void heapsort(uint64_t* v, size_t n) {
    for (size_t i = n / 2; i > 0; i--) {
        sift_down(v, i - 1, n);
    }
    for (size_t k = n; k > 1; k--) {
        uint64_t x = v[0];
        v[0] = v[k - 1];
        v[k - 1] = x;
        sift_down(v, 0, k - 1);
    }
}

/* core.rs `unique_ok`: 1 si son distintos, 0 si hay dos iguales, −1 interno. */
static int unique_ok(sv_core_t* s) {
    uint64_t w;
    if (!top_wf(s, &w)) {
        return -1;
    }
    size_t region = (size_t)(w & 0xFFFFFFFFULL);
    if (s->pool_top > s->len || region > s->len) {
        return -1;
    }
    size_t lo = s->len - s->pool_top, hi = s->len - region;
    if (lo > hi) {
        return -1;
    }
    uint64_t* v = s->mem + lo;
    size_t n = hi - lo;
    heapsort(v, n);
    for (size_t i = 1; i < n; i++) {
        if (v[i - 1] == v[i]) {
            return 0;
        }
    }
    return 1;
}

static int run_char(sv_core_t* s, uint32_t c, sv_pos_t at) {
    s->run = sat_add64(s->run, utf8_len(c));
    if (s->run > s->limits.max_string) {
        return core_fail(s, SV_FAULT_LIMIT, at);
    }
    return 0;
}

static void zone_clear(sv_core_t* s, uint32_t frame) {
    size_t n = frame < s->t->n_frames ? s->t->frames[frame].slots : 0;
    n = minz(minz(n, s->base), s->len);
    for (size_t i = 0; i < n; i++) {
        s->mem[i] = 0;
    }
}

static bool slot_index(const sv_core_t* s, int space, uint32_t k, size_t* out) {
    if (k == 0) {
        return false;
    }
    size_t kk = (size_t)k - 1;
    if (space == SP_ZONE) {
        if (kk >= s->base || kk >= s->len) {
            return false;
        }
        *out = kk;
        return true;
    }
    size_t ini;
    const sv_frame_t* f;
    if (!top_frame(s, &ini, &f) || kk >= f->slots) {
        return false;
    }
    *out = ini + (f->words - f->slots) + kk;
    return true;
}

static int add_to(sv_core_t* s, int space, uint32_t target, uint64_t n, bool has_rule,
                  uint32_t rule, sv_pos_t at) {
    if (n == 0) {
        return 0;
    }
    if (target == 0) {
        s->weight = w_times(s->weight, w_finite(n));
        if (!s->has_first) {
            s->has_first = true;
            s->first_offset = at.offset;
            s->first_line = at.line;
            s->first_column = at.column;
            s->first_has_rule = has_rule;
            s->first_rule = rule;
            snapshot(s);
        }
        if (s->mode == SV_MODE_EARLY_FAIL) {
            s->stopped = true;
            return STOP;
        }
        return 0;
    }
    size_t i;
    if (!slot_index(s, space, target, &i)) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    s->mem[i] = sat_add64(s->mem[i], n);
    return 0;
}

static int core_close(sv_core_t* s, int space, uint32_t frame, sv_pos_t at) {
    const sv_tables_t* t = s->t;
    if (frame >= t->n_frames) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    sv_frame_t f = t->frames[frame];
    size_t ini = f.gates, fin = sat_addz(ini, f.n_gates);
    if (fin > t->n_gates) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    for (size_t gi = ini; gi < fin; gi++) {
        sv_gate_t g = t->gates[gi];
        size_t i0 = g.inputs, i1 = sat_addz(i0, g.n_inputs);
        if (i1 > t->n_gate_inputs) {
            return core_fail(s, SV_FAULT_INTERNAL, at);
        }
        uint32_t ceros = 0;
        uint64_t suma = 0;
        for (size_t j = i0; j < i1; j++) {
            size_t i;
            if (!slot_index(s, space, t->gate_inputs[j], &i)) {
                return core_fail(s, SV_FAULT_INTERNAL, at);
            }
            uint64_t v = s->mem[i];
            ceros += v == 0;
            suma = sat_add64(suma, v);
        }
        uint32_t n = (uint32_t)(i1 - i0);
        uint64_t add = 0;
        switch (g.kind) {
        case SV_GATE_ANY:
            add = n > 0 && ceros == 0;
            break;
        case SV_GATE_ONE:
            add = ceros != 1;
            break;
        case SV_GATE_NOT:
            add = n == 1 && ceros == 1;
            break;
        default: {
            bool on = false;
            if (g.bit != SV_NO_BIT && space == SP_TOP) {
                bool b;
                on = bit(s, g.bit, &b) && b;
            }
            add = on ? suma : 0;
            break;
        }
        }
        if (add_to(s, space, g.target, add, true, g.rule, at) != 0) {
            return STOP;
        }
    }
    return 0;
}

static int propagate(sv_core_t* s, int space, bool has_from, size_t from, uint32_t outs,
                     sv_pos_t at) {
    if (outs == SV_OUT_IDENTITY) {
        return 0;
    }
    const sv_tables_t* t = s->t;
    if (outs >= t->n_outmaps) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    sv_outmap_t m = t->outmaps[outs];
    for (uint32_t j = 0; j < m.len; j++) {
        uint64_t ti = (uint64_t)m.off + j;
        if (ti >= t->n_out_targets) {
            return core_fail(s, SV_FAULT_INTERNAL, at);
        }
        uint32_t target = t->out_targets[ti];
        size_t src;
        bool ok;
        if (has_from) {
            src = from + j;
            ok = true;
        } else {
            ok = slot_index(s, space, j + 1, &src);
        }
        if (!ok || src >= s->len) {
            return core_fail(s, SV_FAULT_INTERNAL, at);
        }
        if (add_to(s, SP_TOP, target, s->mem[src], false, 0, at) != 0) {
            return STOP;
        }
    }
    return 0;
}

/* El lexema del número que se cierra; false = Limit (no cupo). */
static bool lexeme(const sv_core_t* s, const uint8_t** p, size_t* n) {
    if (s->lex_len > s->limits.max_lexeme) {
        return false;
    }
    *p = s->lex;
    *n = s->lex_len;
    return true;
}

/* core.rs `holds`: 1 se cumple, 0 no, o −(fallo + 2). */
#define HOLDS_FAULT(f) (-((f) + 2))

static int holds(const sv_core_t* s, const sv_check_t* c) {
    const sv_tables_t* t = s->t;
    const int internal = HOLDS_FAULT(SV_FAULT_INTERNAL);
    const int limit = HOLDS_FAULT(SV_FAULT_LIMIT);
    uint64_t bound = (uint64_t)c->arg | ((uint64_t)c->len << 32);
    const uint8_t *x, *y;
    size_t xn, yn;
    bool b;
    uint64_t v;
    switch (c->test) {
    case SV_TEST_FAILS:
        return 0;
    case SV_TEST_INTEGER:
        return number_is_integer(&s->num);
    case SV_TEST_MASK:
        return contains(s, c->arg, &b) ? b : internal;
    case SV_TEST_DEPENDENT: {
        size_t ini = c->arg, fin = sat_addz(ini, c->len);
        if (fin > t->n_deps) {
            return internal;
        }
        for (size_t i = ini; i < fin; i++) {
            bool on, ok;
            if (!bit(s, t->deps[i].bit, &on)) {
                return internal;
            }
            if (on) {
                if (!contains(s, t->deps[i].mask, &ok)) {
                    return internal;
                }
                if (!ok) {
                    return 0;
                }
            }
        }
        return 1;
    }
    case SV_TEST_COUNT_MIN:
        return counter(s, c->counter, &v) ? v >= bound : internal;
    case SV_TEST_COUNT_MAX:
        return counter(s, c->counter, &v) ? v <= bound : internal;
    case SV_TEST_UNSET:
        return bit(s, c->arg, &b) ? !b : internal;
    case SV_TEST_LENGTH_MIN:
        return s->chars >= bound;
    case SV_TEST_LENGTH_MAX:
        return s->chars <= bound;
    case SV_TEST_NUM_MIN:
    case SV_TEST_NUM_EX_MIN:
    case SV_TEST_NUM_MAX:
    case SV_TEST_NUM_EX_MAX: {
        if (!lexeme(s, &x, &xn)) {
            return limit;
        }
        if (!table_num(t, c->arg, &y, &yn)) {
            return internal;
        }
        int o = num_compare(x, xn, y, yn);
        switch (c->test) {
        case SV_TEST_NUM_MIN:
            return o >= 0;
        case SV_TEST_NUM_EX_MIN:
            return o > 0;
        case SV_TEST_NUM_MAX:
            return o <= 0;
        default:
            return o < 0;
        }
    }
    case SV_TEST_NUM_ENUM: {
        if (!lexeme(s, &x, &xn)) {
            return limit;
        }
        if (c->arg > UINT32_MAX - c->len) {
            return internal;
        }
        uint32_t lo = c->arg, hi = c->arg + c->len;
        while (lo < hi) {
            uint32_t mid = lo + (hi - lo) / 2;
            if (!table_num(t, mid, &y, &yn)) {
                return internal;
            }
            int o = num_compare(x, xn, y, yn);
            if (o == 0) {
                return 1;
            }
            if (o < 0) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        return 0;
    }
    case SV_TEST_MULTIPLE_OF: {
        if (c->arg >= t->n_divisors) {
            return internal;
        }
        const sv_divisor_t* d = &t->divisors[c->arg];
        if (!table_num(t, d->lexeme, &y, &yn)) {
            return internal;
        }
        if (!lexeme(s, &x, &xn)) {
            return limit;
        }
        return num_is_multiple(x, xn, y, yn, d);
    }
    case SV_TEST_NUM_IN: {
        if (!lexeme(s, &x, &xn)) {
            return limit;
        }
        size_t ini = c->arg;
        if (ini > SIZE_MAX - c->len || ini + c->len > t->n_intervals) {
            return internal;
        }
        const sv_interval_t* list = t->intervals + ini;
        size_t lo = 0, hi = c->len;
        while (lo < hi) {
            size_t mid = lo + (hi - lo) / 2;
            if (!table_num(t, list[mid].lo, &y, &yn)) {
                return internal;
            }
            if (num_compare(y, yn, x, xn) <= 0) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if (lo == 0) {
            return 0;
        }
        sv_interval_t iv = list[lo - 1];
        const uint8_t* z;
        size_t zn;
        if (!table_num(t, iv.lo, &y, &yn)) {
            return internal;
        }
        int a = num_compare(x, xn, y, yn);
        if (!table_num(t, iv.hi, &z, &zn)) {
            return internal;
        }
        int bb = num_compare(x, xn, z, zn);
        return iv.closed ? (a >= 0 && bb <= 0) : (a > 0 && bb < 0);
    }
    default:
        return internal;
    }
}

static int run_checks(sv_core_t* s, const sv_trans_t* tr, sv_pos_t at, bool child, int space) {
    const sv_tables_t* t = s->t;
    size_t ini = tr->checks, fin = sat_addz(ini, tr->n_checks);
    if (fin > t->n_checks) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    for (size_t i = ini; i < fin; i++) {
        const sv_check_t* c = &t->checks[i];
        if (((c->slot & SV_SLOT_CHILD) != 0) != child) {
            continue;
        }
        int r;
        if (c->test == SV_TEST_UNIQUE) {
            r = unique_ok(s);
            if (r < 0) {
                r = HOLDS_FAULT(SV_FAULT_INTERNAL);
            }
        } else {
            r = holds(s, c);
        }
        if (r == 0) {
            if (add_to(s, space, c->slot & ~SV_SLOT_CHILD, 1, true, c->rule, at) != 0) {
                return STOP;
            }
        } else if (r < 0) {
            return core_fail(s, -r - 2, at);
        }
    }
    return 0;
}

static int well_formed(sv_core_t* s, uint32_t a, const sv_trans_t* tr) {
    if (tr->bit != SV_NO_BIT) {
        return 0;
    }
    size_t region;
    if (s->xml) {
        if (a != SV_XCLASS_VALUE_BEGIN || s->name != N_ATTR) {
            return 0;
        }
        region = s->tag_region;
    } else {
        uint64_t w;
        if (a != SV_CLASS_STRING_END || !s->in_key) {
            return 0;
        }
        if (!top_wf(s, &w)) {
            return core_fail(s, SV_FAULT_INTERNAL, s->dup_at);
        }
        region = (size_t)(w & 0xFFFFFFFFULL);
    }
    sv_pos_t at = s->dup_at;
    uint64_t h = s->name_hash;
    size_t len = s->len;
    region = minz(region, s->pool_top);
    for (size_t i = region; i < s->pool_top; i++) {
        if (s->mem[len - 1 - i] == h) {
            return core_fail(s, SV_FAULT_DUPLICATE, at);
        }
    }
    if (s->pool_top - region >= s->limits.max_keys ||
        s->pool_top - s->unique_count >= s->limits.key_pool || len - s->pool_top <= s->sp) {
        return core_fail(s, SV_FAULT_LIMIT, at);
    }
    s->mem[len - 1 - s->pool_top] = h;
    s->pool_top += 1;
    return 0;
}

/* Una transición de M (core.rs `step`). */
static int step(sv_core_t* s, uint32_t a, sv_pos_t at) {
    const sv_tables_t* t = s->t;
    if (a >= t->n_classes) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    uint64_t idx = (uint64_t)s->state * t->n_classes + a;
    if (idx >= t->n_delta || t->delta[idx] >= t->n_trans) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    sv_trans_t tr = t->trans[t->delta[idx]];
    if (tr.op == SV_OP_FAIL) {
        return core_fail(s, tr.fault, at);
    }
    if (well_formed(s, a, &tr) != 0) {
        return STOP;
    }
    if (tr.bit != SV_NO_BIT) {
        bool nuevo;
        if (!set_bit(s, tr.bit, &nuevo)) {
            return core_fail(s, SV_FAULT_INTERNAL, at);
        }
        if (!nuevo) {
            return core_fail(s, SV_FAULT_DUPLICATE, s->dup_at);
        }
    }
    int cur = s->scalar_open ? SP_ZONE : SP_TOP;
    if (run_checks(s, &tr, at, false, cur) != 0) {
        return STOP;
    }
    if (tr.mark != SV_NO_BIT) {
        bool nuevo;
        if (!set_bit(s, tr.mark, &nuevo)) {
            return core_fail(s, SV_FAULT_INTERNAL, at);
        }
    }
    if (tr.n_counts > 0 && !counters(s, &tr)) {
        return core_fail(s, SV_FAULT_INTERNAL, at);
    }
    bool slotted = tr.outs != SV_OUT_IDENTITY;
    switch (tr.op) {
    case SV_OP_MOVE:
        s->state = tr.next;
        if (slotted) {
            zone_clear(s, tr.frame);
            if (run_checks(s, &tr, at, true, SP_ZONE) != 0 ||
                core_close(s, SP_ZONE, tr.frame, at) != 0 ||
                propagate(s, SP_ZONE, false, 0, tr.outs, at) != 0) {
                return STOP;
            }
        } else if (run_checks(s, &tr, at, true, SP_TOP) != 0) {
            return STOP;
        }
        break;
    case SV_OP_PUSH: {
        int f = push(s, tr.frame, tr.ret, tr.outs);
        if (f != NO_FAULT) {
            return core_fail(s, f, at);
        }
        s->state = tr.next;
        if (run_checks(s, &tr, at, true, SP_TOP) != 0) {
            return STOP;
        }
        break;
    }
    case SV_OP_POP: {
        size_t ini;
        const sv_frame_t* f;
        uint32_t ret;
        if (!top_frame(s, &ini, &f)) {
            return core_fail(s, SV_FAULT_INTERNAL, at);
        }
        sv_frame_t fr = *f;
        uint32_t outs = frame_outs(s);
        if (core_close(s, SP_TOP, top_id(s), at) != 0) {
            return STOP;
        }
        if (!pop(s, &ret)) {
            return core_fail(s, SV_FAULT_INTERNAL, at);
        }
        s->state = ret;
        size_t first_slot = ini + (fr.words - fr.slots);
        if (propagate(s, SP_TOP, true, first_slot, outs, at) != 0) {
            return STOP;
        }
        break;
    }
    case SV_OP_ENTER:
        s->scalar_ret = tr.ret;
        s->state = tr.next;
        s->scalar_open = true;
        if (slotted) {
            zone_clear(s, tr.frame);
            s->scalar_frame = tr.frame;
            s->scalar_outs = tr.outs;
        } else {
            s->scalar_outs = SV_OUT_IDENTITY;
        }
        if (run_checks(s, &tr, at, true, SP_ZONE) != 0) {
            return STOP;
        }
        break;
    case SV_OP_LEAVE:
        s->state = s->scalar_ret;
        s->scalar_open = false;
        if (s->scalar_outs != SV_OUT_IDENTITY) {
            uint32_t f = s->scalar_frame, o = s->scalar_outs;
            s->scalar_outs = SV_OUT_IDENTITY;
            if (core_close(s, SP_ZONE, f, at) != 0 || propagate(s, SP_ZONE, false, 0, o, at) != 0) {
                return STOP;
            }
        }
        break;
    default:
        break;
    }
    return 0;
}

/* Procesa un evento del lexer JSON (core.rs `event`). */
static int core_event(sv_core_t* s, int ev, uint32_t c, int part, uint8_t b, sv_span_t span) {
    uint32_t a = 0;
    if (s->stopped) {
        return STOP;
    }
    if (ev != JE_STRING_CHAR) {
        s->run = 0;
    }
    /* F6.1: el cierre y la coma son del contenedor, no de su miembro. */
    bool in_array = !top_is_object(s);
    if ((ev == JE_END_OBJECT || ev == JE_END_ARRAY) && s->popen) {
        seg_pop(s);
    } else if (ev == JE_COMMA && in_array) {
        s->phide = true;
    } else if (ev == JE_COMMA && s->popen) {
        seg_pop(s);
        s->popen = false;
    }
    switch (ev) {
    case JE_BEGIN_OBJECT:
        s->next_wf = WF_OBJECT | (uint64_t)s->pool_top;
        a = SV_CLASS_BEGIN_OBJECT;
        break;
    case JE_END_OBJECT:
        a = SV_CLASS_END_OBJECT;
        break;
    case JE_BEGIN_ARRAY:
        s->next_wf = (uint64_t)s->pool_top;
        a = SV_CLASS_BEGIN_ARRAY;
        break;
    case JE_END_ARRAY:
        a = SV_CLASS_END_ARRAY;
        break;
    case JE_COLON:
        a = SV_CLASS_COLON;
        break;
    case JE_COMMA:
        a = SV_CLASS_COMMA;
        break;
    case JE_TRUE:
        a = SV_CLASS_TRUE;
        break;
    case JE_FALSE:
        a = SV_CLASS_FALSE;
        break;
    case JE_NULL:
        a = SV_CLASS_NULL;
        break;
    case JE_STRING_BEGIN:
        s->chars = 0;
        s->in_key = s->key_next;
        if (s->in_key) {
            seg_begin(s);
            sip_new(&s->hasher, s->limits.hash_key);
        } else if (s->hash_depth > 0) {
            sip_new(&s->vhash, s->limits.hash_key);
            sip_u8(&s->vhash, 's');
        }
        s->dup_at = after(span.start, 1, 1);
        a = SV_CLASS_STRING_BEGIN;
        break;
    case JE_STRING_CHAR: {
        s->chars = sat_add64(s->chars, 1);
        if (run_char(s, c, span.start) != 0) {
            return STOP;
        }
        if (s->in_key) {
            seg_char(s, c, true);
            sip_char(&s->hasher, c);
        } else if (s->hash_depth > 0) {
            sip_char(&s->vhash, c);
        }
        uint64_t len = sat_sub64(span.end, span.start.offset);
        uint64_t cols = len == utf8_len(c) ? 1 : len;
        s->dup_at = after(span.start, len, cols);
        a = sat_add32(SV_CLASS_CHAR, char_class(s->t, c));
        break;
    }
    case JE_STRING_END:
        if (s->in_key) {
            s->name_hash = sip_finish(&s->hasher);
        }
        a = SV_CLASS_STRING_END;
        break;
    case JE_NUMBER_BEGIN:
        memset(&s->num, 0, sizeof s->num);
        s->lex_len = 0;
        a = SV_CLASS_NUMBER_BEGIN;
        break;
    case JE_NUMBER:
        number_byte(&s->num, part, b);
        if (s->lex_len < SV_LEXEME_MAX) {
            s->lex[s->lex_len] = b;
        }
        s->lex_len = sat_addz(s->lex_len, 1);
        a = SV_CLASS_NUMBER_BYTE;
        break;
    default:
        a = SV_CLASS_NUMBER_END;
        break;
    }
    sv_pos_t at = ev == JE_STRING_END ? s->dup_at : span.start;
    bool has_vh = false;
    uint64_t vh = 0;
    if (s->hash_depth > 0) {
        switch (ev) {
        case JE_TRUE:
            vh = h2(s, TAG_TRUE, 0);
            has_vh = true;
            break;
        case JE_FALSE:
            vh = h2(s, TAG_FALSE, 0);
            has_vh = true;
            break;
        case JE_NULL:
            vh = h2(s, TAG_NULL, 0);
            has_vh = true;
            break;
        case JE_STRING_END:
            if (!s->in_key) {
                vh = sip_finish(&s->vhash);
                has_vh = true;
            }
            break;
        case JE_NUMBER_END:
            if (!number_hash(s, &vh)) {
                return core_fail(s, SV_FAULT_LIMIT, span.start);
            }
            has_vh = true;
            break;
        case JE_END_OBJECT:
        case JE_END_ARRAY:
            has_vh = container_hash(s, &vh);
            break;
        default:
            break;
        }
    }
    if (step(s, a, at) != 0) {
        return STOP;
    }
    switch (ev) {
    case JE_BEGIN_OBJECT:
        s->pnest = sat_add32(s->pnest, 1);
        s->popen = false;
        break;
    case JE_BEGIN_ARRAY:
        s->pnest = sat_add32(s->pnest, 1);
        seg_begin(s);
        seg_byte(s, '0');
        seg_commit(s);
        s->popen = true;
        break;
    case JE_END_OBJECT:
    case JE_END_ARRAY:
        s->pnest = s->pnest > 0 ? s->pnest - 1 : 0;
        s->popen = s->pnest > 0;
        break;
    case JE_COLON:
        seg_commit(s);
        s->popen = true;
        break;
    case JE_COMMA:
        if (in_array) {
            s->phide = false;
            seg_next(s);
        }
        break;
    default:
        break;
    }
    if (s->hash_depth > 0) {
        if (has_vh) {
            if (deliver(s, vh, at) != 0) {
                return STOP;
            }
        } else if (ev == JE_STRING_END && s->in_key) {
            if (s->sp >= 3 && s->sp > s->base && s->sp - 3 < s->len) {
                s->mem[s->sp - 3] = s->name_hash;
            }
        }
    }
    switch (ev) {
    case JE_STRING_CHAR:
    case JE_STRING_BEGIN:
        s->key_next = false;
        break;
    case JE_STRING_END:
        s->in_key = false;
        s->key_next = false;
        break;
    case JE_BEGIN_OBJECT:
        s->key_next = true;
        break;
    case JE_COMMA:
        s->key_next = top_is_object(s);
        break;
    default:
        s->key_next = false;
        break;
    }
    return 0;
}

static void begin_name(sv_core_t* s, int n) {
    s->name = n;
    sip_new(&s->hasher, s->limits.hash_key);
    s->colons = 0;
    s->first_colon = false;
    s->last_colon = false;
}

static int end_name(sv_core_t* s, sv_pos_t at) {
    s->name_hash = sip_finish(&s->hasher);
    if (s->colons > 1 || s->first_colon || s->last_colon) {
        return core_fail(s, SV_FAULT_SYNTAX, at);
    }
    return 0;
}

static int end_elem_name(sv_core_t* s, sv_pos_t at) {
    if (s->name == N_ELEM) {
        /* F6.1: lo que se detecta desde aquí es del elemento. */
        seg_commit(s);
        if (end_name(s, at) != 0) {
            return STOP;
        }
        s->next_wf = s->name_hash;
        s->name = N_NONE;
    }
    return 0;
}

/* core.rs `xml_text_char`: 0 con *c listo, 1 si el carácter no llega a M (el \n
 * de un \r\n), STOP si se detuvo. */
static int xml_text_char(sv_core_t* s, uint32_t* c, bool attr, sv_span_t span) {
    uint64_t len = sat_sub64(span.end, span.start.offset);
    bool from_ref = len != utf8_len(*c);
    if (from_ref) {
        s->prev_cr = false;
        s->refs = sat_add64(s->refs, 1);
        if (s->refs > s->limits.max_xml_refs) {
            return core_fail(s, SV_FAULT_LIMIT, span.start);
        }
    } else {
        if (*c == '\n' && s->prev_cr) {
            s->prev_cr = false;
            return 1;
        }
        s->prev_cr = *c == '\r';
        if (*c == '\r') {
            *c = '\n';
        }
        if (attr && (*c == '\t' || *c == '\n')) {
            *c = ' ';
        }
    }
    if (run_char(s, *c, span.start) != 0) {
        return STOP;
    }
    return 0;
}

static void text_begin(sv_core_t* s) {
    s->chars = 0;
    s->lex_len = 0;
}

static void text_char(sv_core_t* s, uint32_t c) {
    s->chars = sat_add64(s->chars, 1);
    if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
        return;
    }
    uint8_t buf[4];
    size_t n = encode_utf8(c, buf);
    for (size_t i = 0; i < n; i++) {
        if (s->lex_len < SV_LEXEME_MAX) {
            s->lex[s->lex_len] = buf[i];
        }
        s->lex_len = sat_addz(s->lex_len, 1);
    }
}

/* Procesa un evento del lexer XML (core.rs `xml_event`). */
static int core_xml_event(sv_core_t* s, int ev, uint32_t c, sv_span_t span) {
    uint32_t a = 0;
    if (s->stopped) {
        return STOP;
    }
    s->xml = true;
    sv_pos_t at = span.start;
    bool is_char =
        ev == XE_NAME_CHAR || ev == XE_ATTR_VALUE_CHAR || ev == XE_TEXT_CHAR || ev == XE_PI_CHAR;
    if (!is_char) {
        s->run = 0;
        s->prev_cr = false;
    }
    switch (ev) {
    case XE_START_TAG_OPEN:
        seg_begin(s);
        begin_name(s, N_ELEM);
        s->tag_region = s->pool_top;
        a = SV_XCLASS_START_TAG;
        break;
    case XE_END_TAG_OPEN:
        begin_name(s, N_END);
        a = SV_XCLASS_END_TAG;
        break;
    case XE_ATTR_BEGIN:
        if (end_elem_name(s, at) != 0) {
            return STOP;
        }
        a = SV_XCLASS_ATTR;
        break;
    case XE_ATTR_VALUE_BEGIN:
        if (s->name == N_ATTR && end_name(s, at) != 0) {
            return STOP;
        }
        text_begin(s);
        a = SV_XCLASS_VALUE_BEGIN;
        break;
    case XE_ATTR_VALUE_END:
        a = SV_XCLASS_VALUE_END;
        break;
    case XE_START_TAG_CLOSE:
    case XE_EMPTY_TAG_CLOSE:
        if (end_elem_name(s, at) != 0) {
            return STOP;
        }
        text_begin(s);
        a = ev == XE_START_TAG_CLOSE ? SV_XCLASS_TAG_CLOSE : SV_XCLASS_EMPTY_CLOSE;
        break;
    case XE_END_TAG_CLOSE:
        if (s->name == N_END) {
            uint64_t w;
            if (end_name(s, at) != 0) {
                return STOP;
            }
            if (top_wf(s, &w) && w != s->name_hash) {
                return core_fail(s, SV_FAULT_SYNTAX, at);
            }
        }
        a = SV_XCLASS_END_CLOSE;
        break;
    case XE_PI_OPEN:
        s->name = N_PI;
        s->pi_target = true;
        a = SV_XCLASS_PI_OPEN;
        break;
    case XE_PI_CLOSE:
        s->pi_target = false;
        a = SV_XCLASS_PI_CLOSE;
        break;
    case XE_PI_CHAR:
        if (run_char(s, c, at) != 0) {
            return STOP;
        }
        if (s->pi_target) {
            s->pi_target = false;
            if (step(s, SV_XCLASS_PI_BODY, at) != 0) {
                return STOP;
            }
        }
        a = sat_add32(SV_CLASS_CHAR, char_class(s->t, c));
        break;
    case XE_NAME_CHAR:
        if (run_char(s, c, at) != 0) {
            return STOP;
        }
        if (s->name == N_ELEM) {
            seg_char(s, c, false);
        }
        if (s->name == N_ELEM || s->name == N_END || s->name == N_ATTR) {
            sip_char(&s->hasher, c);
            s->last_colon = c == ':';
            if (c == ':') {
                if (s->colons == 0 && s->run == 1) {
                    s->first_colon = true;
                }
                s->colons = sat_add32(s->colons, 1);
            }
        }
        a = sat_add32(SV_CLASS_CHAR, char_class(s->t, c));
        break;
    default: {
        /* XE_ATTR_VALUE_CHAR, XE_TEXT_CHAR */
        int r = xml_text_char(s, &c, ev == XE_ATTR_VALUE_CHAR, span);
        if (r == STOP) {
            return STOP;
        }
        if (r == 1) {
            return 0;
        }
        text_char(s, c);
        a = sat_add32(SV_CLASS_CHAR, char_class(s->t, c));
        break;
    }
    }
    if (step(s, a, at) != 0) {
        return STOP;
    }
    switch (ev) {
    case XE_ATTR_BEGIN:
        begin_name(s, N_ATTR);
        s->dup_at = at;
        break;
    case XE_ATTR_VALUE_BEGIN:
    case XE_PI_CLOSE:
        s->name = N_NONE;
        break;
    case XE_END_TAG_CLOSE:
        s->name = N_NONE;
        seg_pop(s);
        break;
    case XE_START_TAG_CLOSE:
    case XE_EMPTY_TAG_CLOSE:
        s->pool_top = minz(s->pool_top, s->tag_region);
        s->name = N_NONE;
        if (ev == XE_EMPTY_TAG_CLOSE) {
            seg_pop(s);
        }
        break;
    default:
        break;
    }
    return 0;
}

static void core_lex_error(sv_core_t* s, sv_pos_t at) {
    if (!s->stopped) {
        core_stop(s, SV_KIND_LEXICAL, at);
    }
}

static void core_limit(sv_core_t* s, sv_pos_t at) {
    if (!s->stopped) {
        core_stop(s, SV_KIND_LIMIT, at);
    }
}

static void core_finish(sv_core_t* s, sv_pos_t end, sv_report_t* r) {
    if (!s->stopped && (s->state != s->t->accept || s->sp != s->base)) {
        core_stop(s, SV_KIND_SYNTAX, end);
    }
    r->valid = s->fault == SV_KIND_NONE && s->weight == 0;
    r->violations = s->weight;
    r->error_kind =
        s->fault != SV_KIND_NONE ? s->fault : (s->weight == 0 ? SV_KIND_NONE : SV_KIND_SCHEMA);
    r->has_first = s->has_first;
    r->offset = s->first_offset;
    r->line = s->first_line;
    r->column = s->first_column;
    r->has_rule = s->first_has_rule;
    r->rule = s->first_rule;
    r->pointer_len = s->has_first ? s->ptr_len : 0;
    r->pointer_truncated = s->has_first && s->ptr_truncated;
    memcpy(r->pointer, s->ptr, r->pointer_len);
}

/* ------------------------------------------------------------- lexer JSON --- */

/* Resultado de un paso de los lexers: bien, error léxico (o detenido antes, con
 * `aborted`) o el núcleo detuvo la entrada. */
enum { LX_OK, LX_ERR, LX_STOP };

static int jl_emit_until(sv_json_lexer_t* l, sv_core_t* c, int ev, uint32_t ch, int part, uint8_t b,
                         sv_pos_t start, uint64_t end) {
    sv_span_t sp;
    sp.start = start;
    sp.end = end;
    if (core_event(c, ev, ch, part, b, sp) != 0) {
        l->failed = true;
        l->aborted = true;
        l->failed_at = l->pos;
        return LX_STOP;
    }
    return LX_OK;
}

static int jl_emit(sv_json_lexer_t* l, sv_core_t* c, int ev, uint32_t ch, int part, uint8_t b,
                   sv_pos_t start) {
    return jl_emit_until(l, c, ev, ch, part, b, start, l->pos.offset + 1);
}

static int jl_emit_char(sv_json_lexer_t* l, sv_core_t* c, uint32_t ch) {
    return jl_emit(l, c, JE_STRING_CHAR, ch, 0, 0, l->char_start);
}

static int jl_fail_at(sv_json_lexer_t* l, sv_pos_t at) {
    l->failed = true;
    l->aborted = false;
    l->failed_at = at;
    return LX_ERR;
}

static int jl_fail(sv_json_lexer_t* l) {
    return jl_fail_at(l, l->pos);
}

static int num_part(unsigned state) {
    switch (state) {
    case SV_JS_S_MINUS:
        return NP_MINUS;
    case SV_JS_S_ZERO:
    case SV_JS_S_INT:
        return NP_INT;
    case SV_JS_S_DOT:
        return NP_DOT;
    case SV_JS_S_FRAC:
        return NP_FRAC;
    case SV_JS_S_EXP:
        return NP_EXP_MARK;
    case SV_JS_S_EXP_SIGN:
        return NP_EXP_SIGN;
    default:
        return NP_EXP;
    }
}

static void jl_init(sv_json_lexer_t* l) {
    memset(l, 0, sizeof *l);
    l->state = SV_JS_S_VALUE;
    l->pos = POS_START;
    l->tok_start = POS_START;
    l->char_start = POS_START;
    u8_new(&l->utf8);
}

static int jl_byte(sv_json_lexer_t* l, sv_core_t* c, uint8_t b) {
    static const uint8_t BOM[3] = {0xEF, 0xBB, 0xBF};
    uint32_t ch;
    if (l->bom < 3) {
        if (b == BOM[l->bom]) {
            l->bom++;
            if (l->bom == 3) {
                l->pos.column = 1;
            }
            return LX_OK;
        }
        if (l->bom > 0) {
            return jl_fail_at(l, POS_START);
        }
        l->bom = 3;
    }
    if (u8_pending(&l->utf8)) {
        switch (u8_step(&l->utf8, b, &ch)) {
        case U8_PENDING:
            return LX_OK;
        case U8_CHAR:
            return jl_emit_char(l, c, ch);
        default:
            return jl_fail_at(l, l->char_start);
        }
    }
    for (;;) {
        unsigned state = l->state;
        sv_tr_t t = sv_js_table[state][sv_js_class[b]];
        sv_pos_t here = l->pos;
        int r = LX_OK;
        l->state = t.next;
        switch (t.action) {
        case SV_JS_A_SKIP:
            break;
        case SV_JS_A_PUNCT: {
            int ev = b == '{'   ? JE_BEGIN_OBJECT
                     : b == '}' ? JE_END_OBJECT
                     : b == '[' ? JE_BEGIN_ARRAY
                     : b == ']' ? JE_END_ARRAY
                     : b == ':' ? JE_COLON
                                : JE_COMMA;
            r = jl_emit(l, c, ev, 0, 0, 0, here);
            break;
        }
        case SV_JS_A_LIT_START:
            l->tok_start = here;
            break;
        case SV_JS_A_LIT_END: {
            int ev = state == SV_JS_S_T3 ? JE_TRUE : state == SV_JS_S_F4 ? JE_FALSE : JE_NULL;
            r = jl_emit(l, c, ev, 0, 0, 0, l->tok_start);
            break;
        }
        case SV_JS_A_NUM_BEGIN:
            l->tok_start = here;
            r = jl_emit(l, c, JE_NUMBER_BEGIN, 0, 0, 0, here);
            if (r == LX_OK) {
                r = jl_emit(l, c, JE_NUMBER, 0, num_part(t.next), b, here);
            }
            break;
        case SV_JS_A_NUM_BYTE:
            r = jl_emit(l, c, JE_NUMBER, 0, num_part(t.next), b, here);
            break;
        case SV_JS_A_NUM_END:
            r = jl_emit_until(l, c, JE_NUMBER_END, 0, 0, 0, l->tok_start, here.offset);
            if (r != LX_OK) {
                return r;
            }
            continue;
        case SV_JS_A_STR_BEGIN:
            l->tok_start = here;
            r = jl_emit(l, c, JE_STRING_BEGIN, 0, 0, 0, here);
            break;
        case SV_JS_A_STR_CHAR:
            r = jl_emit(l, c, JE_STRING_CHAR, b, 0, 0, here);
            break;
        case SV_JS_A_STR_END:
            r = jl_emit(l, c, JE_STRING_END, 0, 0, 0, l->tok_start);
            break;
        case SV_JS_A_ESC_START:
            l->char_start = here;
            break;
        case SV_JS_A_ESC_CHAR:
            ch = b == 'b'   ? 0x8
                 : b == 'f' ? 0xC
                 : b == 'n' ? '\n'
                 : b == 'r' ? '\r'
                 : b == 't' ? '\t'
                            : b;
            r = jl_emit_char(l, c, ch);
            break;
        case SV_JS_A_U_START:
            l->hex = 0;
            break;
        case SV_JS_A_HEX:
            l->hex = (l->hex << 4) | hex_value(b);
            break;
        case SV_JS_A_HEX_LAST: {
            uint32_t cp = (l->hex << 4) | hex_value(b);
            if (cp >= 0xD800 && cp <= 0xDBFF) {
                l->high = cp;
                l->state = SV_JS_S_LOW_BS;
            } else if (cp >= 0xDC00 && cp <= 0xDFFF) {
                return jl_fail_at(l, l->char_start);
            } else {
                r = jl_emit_char(l, c, is_scalar(cp) ? cp : 0xFFFD);
            }
            break;
        }
        case SV_JS_A_LOW_HEX_LAST: {
            uint32_t low = (l->hex << 4) | hex_value(b);
            if (!(low >= 0xDC00 && low <= 0xDFFF)) {
                return jl_fail_at(l, l->char_start);
            }
            uint32_t cp = 0x10000 + ((l->high - 0xD800) << 10) + (low - 0xDC00);
            r = jl_emit_char(l, c, is_scalar(cp) ? cp : 0xFFFD);
            break;
        }
        case SV_JS_A_UTF8:
            l->char_start = here;
            if (u8_step(&l->utf8, b, &ch) == U8_INVALID) {
                return jl_fail(l);
            }
            break;
        default:
            l->state = (uint8_t)state;
            if (state == SV_JS_S_LOW_BS || state == SV_JS_S_LOW_U) {
                return jl_fail_at(l, l->char_start);
            }
            return jl_fail(l);
        }
        return r;
    }
}

static int jl_feed(sv_json_lexer_t* l, sv_core_t* c, const uint8_t* chunk, size_t len) {
    if (l->failed) {
        return LX_ERR;
    }
    for (size_t i = 0; i < len; i++) {
        int r = jl_byte(l, c, chunk[i]);
        if (r != LX_OK) {
            return r;
        }
        pos_advance(&l->pos, chunk[i]);
    }
    return LX_OK;
}

static int jl_finish(sv_json_lexer_t* l, sv_core_t* c) {
    if (l->failed) {
        return LX_ERR;
    }
    if (l->bom >= 1 && l->bom < 3) {
        return jl_fail_at(l, POS_START);
    }
    if (u8_pending(&l->utf8)) {
        return jl_fail_at(l, l->char_start);
    }
    switch (l->state) {
    case SV_JS_S_VALUE:
        return LX_OK;
    case SV_JS_S_ZERO:
    case SV_JS_S_INT:
    case SV_JS_S_FRAC:
    case SV_JS_S_EXP_DIG:
        l->state = SV_JS_S_VALUE;
        return jl_emit_until(l, c, JE_NUMBER_END, 0, 0, 0, l->tok_start, l->pos.offset);
    default:
        return jl_fail(l);
    }
}

/* -------------------------------------------------------------- lexer XML --- */

static bool is_xml_char(uint32_t cp) {
    return cp == 0x9 || cp == 0xA || cp == 0xD || (cp >= 0x20 && cp <= 0xD7FF) ||
           (cp >= 0xE000 && cp <= 0xFFFD) || (cp >= 0x10000 && cp <= 0x10FFFF);
}

static int xl_emit_until(sv_xml_lexer_t* l, sv_core_t* c, int ev, uint32_t ch, sv_pos_t start,
                         uint64_t end) {
    sv_span_t sp;
    sp.start = start;
    sp.end = end;
    if (core_xml_event(c, ev, ch, sp) != 0) {
        l->failed = true;
        l->aborted = true;
        l->failed_at = l->pos;
        return LX_STOP;
    }
    return LX_OK;
}

static int xl_emit(sv_xml_lexer_t* l, sv_core_t* c, int ev, uint32_t ch, sv_pos_t start) {
    return xl_emit_until(l, c, ev, ch, start, l->pos.offset + 1);
}

static int xl_fail_at(sv_xml_lexer_t* l, sv_pos_t at) {
    l->failed = true;
    l->aborted = false;
    l->failed_at = at;
    return LX_ERR;
}

static int xl_fail(sv_xml_lexer_t* l) {
    return xl_fail_at(l, l->pos);
}

static int xl_emit_char(sv_xml_lexer_t* l, sv_core_t* c, uint32_t ch) {
    int ev;
    if (!is_xml_char(ch)) {
        return xl_fail_at(l, l->char_start);
    }
    switch (l->state) {
    case SV_XL_S_STAG_NAME:
    case SV_XL_S_ATTR_NAME:
    case SV_XL_S_ETAG_NAME:
    case SV_XL_S_PI_TARGET:
        ev = XE_NAME_CHAR;
        break;
    case SV_XL_S_VAL_DQ:
    case SV_XL_S_VAL_SQ:
        ev = XE_ATTR_VALUE_CHAR;
        break;
    case SV_XL_S_PI:
    case SV_XL_S_PI_Q:
        ev = XE_PI_CHAR;
        break;
    case SV_XL_S_COMMENT:
    case SV_XL_S_COM_D1:
        return LX_OK;
    default:
        ev = XE_TEXT_CHAR;
        break;
    }
    return xl_emit(l, c, ev, ch, l->char_start);
}

static int xl_char(sv_xml_lexer_t* l, sv_core_t* c, uint8_t b) {
    uint32_t ch;
    l->char_start = l->pos;
    if (b < 0x80) {
        return xl_emit_char(l, c, b);
    }
    if (u8_step(&l->utf8, b, &ch) == U8_INVALID) {
        return xl_fail(l);
    }
    return LX_OK;
}

static void xl_init(sv_xml_lexer_t* l) {
    memset(l, 0, sizeof *l);
    l->state = SV_XL_S_CONTENT;
    l->pos = POS_START;
    l->tok_start = POS_START;
    l->char_start = POS_START;
    l->val_start = POS_START;
    u8_new(&l->utf8);
    l->ref_ret = SV_XL_S_CONTENT;
}

static bool ref_is(const sv_xml_lexer_t* l, const char* name) {
    size_t n = strlen(name);
    return l->ref_len == n && memcmp(l->ref_name, name, n) == 0;
}

static int xl_byte(sv_xml_lexer_t* l, sv_core_t* c, uint8_t b) {
    uint32_t ch;
    int r;
    if (u8_pending(&l->utf8)) {
        switch (u8_step(&l->utf8, b, &ch)) {
        case U8_PENDING:
            return LX_OK;
        case U8_CHAR:
            if (ch == 0xFEFF && l->char_start.offset == 0) {
                l->pos.column = 1;
                return LX_OK;
            }
            return xl_emit_char(l, c, ch);
        default:
            return xl_fail_at(l, l->char_start);
        }
    }
    unsigned state = l->state;
    unsigned cls = sv_xl_class[b];
    sv_tr_t t = sv_xl_table[state][cls];
    sv_pos_t here = l->pos;
    l->state = t.next;
    switch (t.action) {
    case SV_XL_A_SKIP:
        return LX_OK;
    case SV_XL_A_MARK:
        l->tok_start = here;
        return LX_OK;
    case SV_XL_A_CHAR:
        return xl_char(l, c, b);
    case SV_XL_A_STAG_OPEN: {
        sv_pos_t lt = l->tok_start;
        r = xl_emit_until(l, c, XE_START_TAG_OPEN, 0, lt, lt.offset + 1);
        return r != LX_OK ? r : xl_char(l, c, b);
    }
    case SV_XL_A_ETAG_OPEN:
        return xl_emit(l, c, XE_END_TAG_OPEN, 0, l->tok_start);
    case SV_XL_A_ATTR_BEGIN:
        r = xl_emit(l, c, XE_ATTR_BEGIN, 0, here);
        return r != LX_OK ? r : xl_char(l, c, b);
    case SV_XL_A_VAL_BEGIN:
        l->val_start = here;
        return xl_emit(l, c, XE_ATTR_VALUE_BEGIN, 0, here);
    case SV_XL_A_VAL_END:
        return xl_emit(l, c, XE_ATTR_VALUE_END, 0, l->val_start);
    case SV_XL_A_STAG_CLOSE:
        return xl_emit(l, c, XE_START_TAG_CLOSE, 0, l->tok_start);
    case SV_XL_A_EMPTY_CLOSE:
        return xl_emit(l, c, XE_EMPTY_TAG_CLOSE, 0, l->tok_start);
    case SV_XL_A_ETAG_CLOSE:
        return xl_emit(l, c, XE_END_TAG_CLOSE, 0, l->tok_start);
    case SV_XL_A_REF_START:
        l->ref_ret =
            (uint8_t)((state == SV_XL_S_VAL_DQ || state == SV_XL_S_VAL_SQ) ? state
                                                                           : SV_XL_S_CONTENT);
        l->char_start = here;
        l->ref_len = 0;
        l->ref_val = 0;
        return LX_OK;
    case SV_XL_A_REF_NAME:
        if (l->ref_len == sizeof l->ref_name) {
            return xl_fail_at(l, l->char_start);
        }
        l->ref_name[l->ref_len++] = b;
        return LX_OK;
    case SV_XL_A_REF_NAME_END:
        if (ref_is(l, "lt")) {
            ch = '<';
        } else if (ref_is(l, "gt")) {
            ch = '>';
        } else if (ref_is(l, "amp")) {
            ch = '&';
        } else if (ref_is(l, "quot")) {
            ch = '"';
        } else if (ref_is(l, "apos")) {
            ch = '\'';
        } else {
            return xl_fail_at(l, l->char_start);
        }
        l->state = l->ref_ret;
        return xl_emit_char(l, c, ch);
    case SV_XL_A_REF_DEC:
    case SV_XL_A_REF_HEX:
        if (t.action == SV_XL_A_REF_DEC) {
            l->ref_val = l->ref_val * 10 + (uint32_t)(b - '0');
        } else {
            l->ref_val = l->ref_val * 16 + hex_value(b);
        }
        if (l->ref_val > 0x10FFFF) {
            return xl_fail_at(l, l->char_start);
        }
        return LX_OK;
    case SV_XL_A_REF_NUM_END:
        if (!is_scalar(l->ref_val) || !is_xml_char(l->ref_val)) {
            return xl_fail_at(l, l->char_start);
        }
        l->state = l->ref_ret;
        return xl_emit_char(l, c, l->ref_val);
    case SV_XL_A_CD_RB:
        return xl_emit_until(l, c, XE_TEXT_CHAR, ']', back(here, 2), sat_sub64(here.offset, 1));
    case SV_XL_A_CD_FLUSH1:
        r = xl_emit_until(l, c, XE_TEXT_CHAR, ']', back(here, 1), here.offset);
        return r != LX_OK ? r : xl_char(l, c, b);
    case SV_XL_A_PI_OPEN: {
        sv_pos_t lt = l->tok_start;
        r = xl_emit_until(l, c, XE_PI_OPEN, 0, lt, lt.offset + 2);
        return r != LX_OK ? r : xl_char(l, c, b);
    }
    case SV_XL_A_PI_Q:
        return xl_emit_until(l, c, XE_PI_CHAR, '?', back(here, 1), here.offset);
    case SV_XL_A_PI_FLUSH:
        r = xl_emit_until(l, c, XE_PI_CHAR, '?', back(here, 1), here.offset);
        return r != LX_OK ? r : xl_char(l, c, b);
    case SV_XL_A_PI_CLOSE:
        return xl_emit(l, c, XE_PI_CLOSE, 0, l->tok_start);
    case SV_XL_A_CD_FLUSH2:
        r = xl_emit_until(l, c, XE_TEXT_CHAR, ']', back(here, 2), sat_sub64(here.offset, 1));
        if (r != LX_OK) {
            return r;
        }
        r = xl_emit_until(l, c, XE_TEXT_CHAR, ']', back(here, 1), here.offset);
        return r != LX_OK ? r : xl_char(l, c, b);
    default:
        l->state = (uint8_t)state;
        return xl_fail(l);
    }
}

static int xl_feed(sv_xml_lexer_t* l, sv_core_t* c, const uint8_t* chunk, size_t len) {
    if (l->failed) {
        return LX_ERR;
    }
    for (size_t i = 0; i < len; i++) {
        int r = xl_byte(l, c, chunk[i]);
        if (r != LX_OK) {
            return r;
        }
        pos_advance(&l->pos, chunk[i]);
    }
    return LX_OK;
}

static int xl_finish(sv_xml_lexer_t* l) {
    if (l->failed) {
        return LX_ERR;
    }
    if (u8_pending(&l->utf8)) {
        return xl_fail_at(l, l->char_start);
    }
    if (l->state != SV_XL_S_CONTENT && l->state != SV_XL_S_RB1 && l->state != SV_XL_S_RB2) {
        return xl_fail(l);
    }
    return LX_OK;
}

/* -------------------------------------------------------------- validador --- */

void sv_validator_init(sv_validator_t* v, const sv_tables_t* t, bool xml, const sv_limits_t* lim,
                       uint64_t* mem, size_t len) {
    v->xml = xml;
    jl_init(&v->json);
    xl_init(&v->xmll);
    core_init(&v->core, t, lim, mem, len);
    v->fed = 0;
    v->max_payload = lim->max_payload;
}

/* `record`: un error léxico (no el de un lexer ya detenido) es peso ∞. */
static void record(sv_validator_t* v, int r) {
    if (r != LX_ERR) {
        return;
    }
    bool aborted = v->xml ? v->xmll.aborted : v->json.aborted;
    sv_pos_t at = v->xml ? v->xmll.failed_at : v->json.failed_at;
    if (!aborted) {
        core_lex_error(&v->core, at);
    }
}

int sv_validator_feed(sv_validator_t* v, const uint8_t* chunk, size_t len) {
    if (v->core.stopped) {
        return -1;
    }
    uint64_t room = sat_sub64(v->max_payload, v->fed);
    size_t take = room < len ? (size_t)room : len;
    v->fed = sat_add64(v->fed, take);
    int r = v->xml ? xl_feed(&v->xmll, &v->core, chunk, take)
                   : jl_feed(&v->json, &v->core, chunk, take);
    if (r != LX_OK) {
        record(v, r);
        return -1;
    }
    if (take < len) {
        core_limit(&v->core, v->xml ? v->xmll.pos : v->json.pos);
        return -1;
    }
    return 0;
}

void sv_validator_abort_limit(sv_validator_t* v) {
    core_limit(&v->core, v->xml ? v->xmll.pos : v->json.pos);
}

void sv_validator_finish(sv_validator_t* v, sv_report_t* out) {
    if (!v->core.stopped) {
        int r = v->xml ? xl_finish(&v->xmll) : jl_finish(&v->json, &v->core);
        if (r != LX_OK) {
            record(v, r);
        }
    }
    core_finish(&v->core, v->xml ? v->xmll.pos : v->json.pos, out);
}
