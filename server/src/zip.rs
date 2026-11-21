//! F7.5 — Un ZIP mínimo para descargar el código generado (D81): archivos
//! planos sin comprimir (método 0, *stored*) con su CRC-32, directorio
//! central y fin de directorio (APPNOTE 6.3.x, §4.3). Sin crates: el código
//! generado ocupa unos cientos de KiB y no hace falta comprimir.

/// CRC-32 (IEEE 802.3, polinomio reflejado 0xEDB88320), por tabla.
pub fn crc32(datos: &[u8]) -> u32 {
    const fn tabla() -> [u32; 256] {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    }
    const T: [u32; 256] = tabla();
    let mut c = !0u32;
    for &b in datos {
        c = T[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

fn u16le(o: &mut Vec<u8>, v: u16) {
    o.extend_from_slice(&v.to_le_bytes());
}

fn u32le(o: &mut Vec<u8>, v: u32) {
    o.extend_from_slice(&v.to_le_bytes());
}

/// Fecha y hora fijas (1 ene 2026, 00:00) en formato MS-DOS: el ZIP es
/// reproducible (mismo esquema, mismos bytes).
const HORA: u16 = 0;
const FECHA: u16 = ((2026 - 1980) << 9) | (1 << 5) | 1;

/// El ZIP de `archivos` (`(nombre, contenido)`). Los nombres deben ser
/// ASCII sin `/` inicial ni `..` (los del emisor lo son); cada archivo y el
/// total, menos de 4 GiB (sin ZIP64).
pub fn zip(archivos: &[(&str, &[u8])]) -> Vec<u8> {
    let mut o = Vec::new();
    let mut central = Vec::new();
    for (nombre, datos) in archivos {
        let crc = crc32(datos);
        let tam = datos.len() as u32;
        let desplazamiento = o.len() as u32;
        // Cabecera local.
        u32le(&mut o, 0x0403_4B50);
        u16le(&mut o, 20); // versión necesaria (2.0)
        u16le(&mut o, 0x0800); // bit 11: nombres en UTF-8
        u16le(&mut o, 0); // sin comprimir
        u16le(&mut o, HORA);
        u16le(&mut o, FECHA);
        u32le(&mut o, crc);
        u32le(&mut o, tam);
        u32le(&mut o, tam);
        u16le(&mut o, nombre.len() as u16);
        u16le(&mut o, 0);
        o.extend_from_slice(nombre.as_bytes());
        o.extend_from_slice(datos);
        // Entrada del directorio central.
        u32le(&mut central, 0x0201_4B50);
        u16le(&mut central, 0x0314); // hecho por: Unix, 2.0
        u16le(&mut central, 20);
        u16le(&mut central, 0x0800);
        u16le(&mut central, 0);
        u16le(&mut central, HORA);
        u16le(&mut central, FECHA);
        u32le(&mut central, crc);
        u32le(&mut central, tam);
        u32le(&mut central, tam);
        u16le(&mut central, nombre.len() as u16);
        u16le(&mut central, 0); // extra
        u16le(&mut central, 0); // comentario
        u16le(&mut central, 0); // disco
        u16le(&mut central, 0); // atributos internos
        u32le(&mut central, 0o100_644 << 16); // archivo regular rw-r--r--
        u32le(&mut central, desplazamiento);
        central.extend_from_slice(nombre.as_bytes());
    }
    let inicio_central = o.len() as u32;
    let tam_central = central.len() as u32;
    o.extend_from_slice(&central);
    // Fin del directorio central.
    u32le(&mut o, 0x0605_4B50);
    u16le(&mut o, 0);
    u16le(&mut o, 0);
    u16le(&mut o, archivos.len() as u16);
    u16le(&mut o, archivos.len() as u16);
    u32le(&mut o, tam_central);
    u32le(&mut o, inicio_central);
    u16le(&mut o, 0);
    o
}
