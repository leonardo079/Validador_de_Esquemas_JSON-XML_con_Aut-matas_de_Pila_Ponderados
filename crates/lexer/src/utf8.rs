//! AFD de validación y decodificación UTF-8 (RFC 3629, tabla 3-7 de Unicode).
//!
//! Alfabeto: Σ = bytes 0x00–0xFF, agrupados en 12 clases de equivalencia.
//! El autómata acepta exactamente las secuencias UTF-8 bien formadas: rechaza
//! por construcción las formas sobrelargas (`C0 80`, `E0 80 80`, …), los
//! sustitutos U+D800–U+DFFF (`ED A0..BF ..`) y los valores > U+10FFFF
//! (`F4 90..`, `F5..FF`).

/// Estado inicial y único estado de aceptación: frontera entre caracteres.
pub const ACCEPT: usize = 0;
/// Estado sumidero de error.
pub const REJECT: usize = 1;
/// Falta 1 byte de continuación cualquiera (80–BF).
const CONT1: usize = 2;
/// Faltan 2 bytes de continuación.
const CONT2: usize = 3;
/// Faltan 3 bytes de continuación.
const CONT3: usize = 4;
/// Tras `E0`: el segundo byte debe ser A0–BF (evita sobrelargas de 3 bytes).
const E0_2: usize = 5;
/// Tras `ED`: el segundo byte debe ser 80–9F (evita sustitutos).
const ED_2: usize = 6;
/// Tras `F0`: el segundo byte debe ser 90–BF (evita sobrelargas de 4 bytes).
const F0_2: usize = 7;
/// Tras `F4`: el segundo byte debe ser 80–8F (evita > U+10FFFF).
const F4_2: usize = 8;

pub const NUM_STATES: usize = 9;

pub const STATE_NAMES: [&str; NUM_STATES] = [
    "inicio", "error", "falta 1", "falta 2", "falta 3", "E0·", "ED·", "F0·", "F4·",
];

const C_ASCII: usize = 0;
const C_80_8F: usize = 1;
const C_90_9F: usize = 2;
const C_A0_BF: usize = 3;
const C_INVALID: usize = 4;
const C_C2_DF: usize = 5;
const C_E0: usize = 6;
const C_E1_EF: usize = 7;
const C_ED: usize = 8;
const C_F0: usize = 9;
const C_F1_F3: usize = 10;
const C_F4: usize = 11;

pub const NUM_CLASSES: usize = 12;

pub const CLASS_NAMES: [&str; NUM_CLASSES] = [
    "00–7F",
    "80–8F",
    "90–9F",
    "A0–BF",
    "C0 C1 F5–FF",
    "C2–DF",
    "E0",
    "E1–EC EE EF",
    "ED",
    "F0",
    "F1–F3",
    "F4",
];

/// Clase de cada byte.
pub static CLASS: [u8; 256] = build_classes();

/// Función de transición δ: `TABLE[estado][clase]`.
pub static TABLE: [[u8; NUM_CLASSES]; NUM_STATES] = build_table();

const fn build_classes() -> [u8; 256] {
    let mut c = [0u8; 256];
    let mut b = 0;
    while b < 256 {
        c[b] = match b {
            0x00..=0x7F => C_ASCII,
            0x80..=0x8F => C_80_8F,
            0x90..=0x9F => C_90_9F,
            0xA0..=0xBF => C_A0_BF,
            0xC2..=0xDF => C_C2_DF,
            0xE0 => C_E0,
            0xED => C_ED,
            0xE1..=0xEF => C_E1_EF,
            0xF0 => C_F0,
            0xF1..=0xF3 => C_F1_F3,
            0xF4 => C_F4,
            _ => C_INVALID, // C0, C1, F5–FF
        } as u8;
        b += 1;
    }
    c
}

const fn build_table() -> [[u8; NUM_CLASSES]; NUM_STATES] {
    let mut t = [[REJECT as u8; NUM_CLASSES]; NUM_STATES];
    let cont = [C_80_8F, C_90_9F, C_A0_BF];

    t[ACCEPT][C_ASCII] = ACCEPT as u8;
    t[ACCEPT][C_C2_DF] = CONT1 as u8;
    t[ACCEPT][C_E0] = E0_2 as u8;
    t[ACCEPT][C_E1_EF] = CONT2 as u8;
    t[ACCEPT][C_ED] = ED_2 as u8;
    t[ACCEPT][C_F0] = F0_2 as u8;
    t[ACCEPT][C_F1_F3] = CONT3 as u8;
    t[ACCEPT][C_F4] = F4_2 as u8;

    let mut i = 0;
    while i < cont.len() {
        t[CONT1][cont[i]] = ACCEPT as u8;
        t[CONT2][cont[i]] = CONT1 as u8;
        t[CONT3][cont[i]] = CONT2 as u8;
        i += 1;
    }
    t[E0_2][C_A0_BF] = CONT1 as u8;
    t[ED_2][C_80_8F] = CONT1 as u8;
    t[ED_2][C_90_9F] = CONT1 as u8;
    t[F0_2][C_90_9F] = CONT2 as u8;
    t[F0_2][C_A0_BF] = CONT2 as u8;
    t[F4_2][C_80_8F] = CONT2 as u8;
    t
}

/// Resultado de consumir un byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Se completó un punto de código.
    Char(char),
    /// Secuencia en curso: faltan bytes de continuación.
    Pending,
    /// Secuencia inválida.
    Invalid,
}

/// Decodificador incremental: el AFD más un registro con el punto de código
/// parcial. Ocupa 8 bytes y nunca reserva memoria.
#[derive(Clone, Copy, Debug)]
pub struct Utf8Decoder {
    state: u8,
    cp: u32,
}

impl Default for Utf8Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Utf8Decoder {
    pub const fn new() -> Self {
        Self {
            state: ACCEPT as u8,
            cp: 0,
        }
    }

    /// `true` si hay una secuencia multibyte a medias.
    #[inline]
    pub fn is_pending(&self) -> bool {
        self.state != ACCEPT as u8
    }

    #[inline]
    pub fn step(&mut self, b: u8) -> Step {
        let class = CLASS[b as usize] as usize;
        self.cp = if self.state == ACCEPT as u8 {
            // Bits de carga del byte inicial según su longitud.
            u32::from(b)
                & match b {
                    0x00..=0x7F => 0x7F,
                    0x80..=0xDF => 0x1F,
                    0xE0..=0xEF => 0x0F,
                    _ => 0x07,
                }
        } else {
            (self.cp << 6) | u32::from(b & 0x3F)
        };
        self.state = TABLE[self.state as usize][class];
        match self.state as usize {
            ACCEPT => match char::from_u32(self.cp) {
                Some(c) => Step::Char(c),
                // Inalcanzable: el AFD ya excluye sustitutos y > U+10FFFF.
                None => self.reject(),
            },
            REJECT => self.reject(),
            _ => Step::Pending,
        }
    }

    fn reject(&mut self) -> Step {
        *self = Self::new();
        Step::Invalid
    }
}

/// Valida un búfer completo. Devuelve el desplazamiento del primer byte de la
/// secuencia inválida (o de la secuencia truncada al final).
pub fn validate(bytes: &[u8]) -> Result<(), usize> {
    let mut d = Utf8Decoder::new();
    let mut seq_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if !d.is_pending() {
            seq_start = i;
        }
        if d.step(b) == Step::Invalid {
            return Err(seq_start);
        }
    }
    if d.is_pending() {
        Err(seq_start)
    } else {
        Ok(())
    }
}

/// Escribe el AFD en formato DOT (Graphviz).
pub fn write_dot(w: &mut impl core::fmt::Write) -> core::fmt::Result {
    crate::dot::write(
        w,
        &crate::dot::Dfa {
            name: "utf8",
            title: "AFD UTF-8 (RFC 3629)",
            num_states: NUM_STATES,
            num_classes: NUM_CLASSES,
            next: &|s, c| usize::from(TABLE[s][c]),
            state_names: &STATE_NAMES,
            class_names: &CLASS_NAMES,
            start: ACCEPT,
            accepting: &|s| s == ACCEPT,
            error: REJECT,
            guarded: &[],
        },
    )
}
