//! SipHash-1-3 en *streaming*, `no_std` y sin reservas (F4-B21).
//!
//! Lo usa la capa de buena formación del ejecutor (F4.6) para detectar las
//! claves JSON no declaradas y los atributos XML repetidos y para comparar
//! el nombre de cierre de un elemento con el de apertura, y lo reutiliza
//! `uniqueItems` (F4.5b). La clave va en [`crate::Limits::hash_key`]: con una
//! clave que el atacante no conoce, la probabilidad de que dos nombres
//! distintos de un mismo objeto choquen es ≤ n²/2⁶⁵ (n ≤ 1 024 claves por
//! objeto: < 2⁻⁴⁵), y un choque solo puede dar un falso «repetido», nunca
//! aceptar un payload mal formado (`docs/fase4/runtime.md` §5).
//!
//! Es el algoritmo de Aumasson y Bernstein con 1 ronda de compresión y 3 de
//! finalización (el de `std::collections::HashMap`). La prueba
//! `vectores_de_referencia` fija cuatro vectores con la clave 00 … 0F,
//! comprobados al escribirla contra `DefaultHasher` de `std` (SipHash-1-3
//! con clave 0) para mensajes de 0 a 11 bytes.

/// Estado de SipHash-1-3: cuatro palabras, los bytes pendientes (< 8) y la
/// longitud total (módulo 2⁸, lo que usa el algoritmo).
#[derive(Clone, Copy, Debug)]
pub struct SipHasher13 {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    tail: u64,
    ntail: u32,
    len: u64,
}

impl SipHasher13 {
    pub const fn new(key: [u64; 2]) -> SipHasher13 {
        SipHasher13 {
            v0: key[0] ^ 0x736f_6d65_7073_6575,
            v1: key[1] ^ 0x646f_7261_6e64_6f6d,
            v2: key[0] ^ 0x6c79_6765_6e65_7261,
            v3: key[1] ^ 0x7465_6462_7974_6573,
            tail: 0,
            ntail: 0,
            len: 0,
        }
    }

    #[inline]
    fn round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(13);
        self.v1 ^= self.v0;
        self.v0 = self.v0.rotate_left(32);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(16);
        self.v3 ^= self.v2;
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(21);
        self.v3 ^= self.v0;
        self.v2 = self.v2.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(17);
        self.v1 ^= self.v2;
        self.v2 = self.v2.rotate_left(32);
    }

    #[inline]
    fn compress(&mut self, m: u64) {
        self.v3 ^= m;
        self.round();
        self.v0 ^= m;
    }

    /// Añade un byte.
    #[inline]
    pub fn write_u8(&mut self, b: u8) {
        self.tail |= u64::from(b) << (8 * self.ntail);
        self.ntail += 1;
        self.len = self.len.wrapping_add(1);
        if self.ntail == 8 {
            let m = self.tail;
            self.compress(m);
            self.tail = 0;
            self.ntail = 0;
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u8(b);
        }
    }

    /// Añade un punto de código en UTF-8.
    pub fn write_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.write(c.encode_utf8(&mut buf).as_bytes());
    }

    /// El hash de lo escrito (no consume el estado).
    pub fn finish(&self) -> u64 {
        let mut s = *self;
        let m = s.tail | ((s.len & 0xFF) << 56);
        s.compress(m);
        s.v2 ^= 0xFF;
        s.round();
        s.round();
        s.round();
        s.v0 ^ s.v1 ^ s.v2 ^ s.v3
    }
}

#[cfg(test)]
mod tests {
    use super::SipHasher13;

    /// Vectores de SipHash-1-3 con la clave 00 01 … 0F y el mensaje
    /// 00 01 … (n−1); el primero (mensaje vacío) es el de la implementación
    /// de referencia.
    #[test]
    fn vectores_de_referencia() {
        let k0 = u64::from_le_bytes([0, 1, 2, 3, 4, 5, 6, 7]);
        let k1 = u64::from_le_bytes([8, 9, 10, 11, 12, 13, 14, 15]);
        let esperados: [u64; 4] = [
            u64::from_le_bytes([0xdc, 0xc4, 0x0f, 0x05, 0x58, 0x01, 0xac, 0xab]),
            u64::from_le_bytes([0x93, 0xca, 0x57, 0x7d, 0xf3, 0x9b, 0xf4, 0xc9]),
            u64::from_le_bytes([0x4d, 0xd4, 0xc7, 0x4d, 0x02, 0x9b, 0xcb, 0x82]),
            u64::from_le_bytes([0xfb, 0xf7, 0xdd, 0xe7, 0xb8, 0x0a, 0xf8, 0x8b]),
        ];
        for (n, &e) in esperados.iter().enumerate() {
            let mut h = SipHasher13::new([k0, k1]);
            for b in 0..n as u8 {
                h.write_u8(b);
            }
            assert_eq!(h.finish(), e, "longitud {n}");
        }
    }

    #[test]
    fn la_particion_no_cambia_el_hash() {
        let mut a = SipHasher13::new([1, 2]);
        a.write(b"una clave larga de prueba");
        let mut b = SipHasher13::new([1, 2]);
        for c in "una clave larga de prueba".chars() {
            b.write_char(c);
        }
        assert_eq!(a.finish(), b.finish());
        let mut c = SipHasher13::new([1, 3]);
        c.write(b"una clave larga de prueba");
        assert_ne!(a.finish(), c.finish());
    }
}
