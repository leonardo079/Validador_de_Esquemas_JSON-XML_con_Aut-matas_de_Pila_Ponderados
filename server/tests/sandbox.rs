//! Pruebas del sandbox (F7.3, D68).
//!
//! Las primeras no necesitan Docker: argumentos de `docker run`, validación del
//! trabajo y formato del tar. Las marcadas `#[ignore]` lanzan contenedores de
//! verdad y corren con `scripts/test-sandbox.ps1` (o `make test-sandbox`),
//! que antes construye la imagen: `cargo test -p server --test sandbox --
//! --ignored --test-threads 1`. Ninguna mide tiempo (D1): los topes se
//! comprueban por el resultado (`TiempoAgotado`), no por cuánto tardó.

use std::time::Duration;

use server::sandbox::{
    argumentos, comprobar, ejecutar, tar, Lenguaje, Limites, Resultado, Sandbox, Trabajo,
};

fn trabajo<'a>(l: Lenguaje, archivos: &'a [(&'a str, &'a [u8])], payload: &'a [u8]) -> Trabajo<'a> {
    Trabajo {
        lenguaje: l,
        archivos,
        payload,
    }
}

#[test]
fn argumentos_con_todas_las_restricciones() {
    let a = argumentos(&Sandbox::default(), Lenguaje::C, "sv-1");
    let tiene = |par: &[&str]| {
        a.windows(par.len())
            .any(|w| w.iter().zip(par).all(|(x, y)| x == y))
    };
    for par in [
        &["--network", "none"][..],
        &["--read-only"],
        &["--cap-drop", "ALL"],
        &["--security-opt", "no-new-privileges"],
        &["--user", "10001:10001"],
        &["--memory", "512m"],
        &["--memory-swap", "512m"],
        &["--cpus", "1.000"],
        &["--pids-limit", "64"],
        &["--name", "sv-1"],
        &["--rm"],
        &["--ipc", "none"],
    ] {
        assert!(tiene(par), "falta {par:?} en {a:?}");
    }
    assert!(a
        .iter()
        .any(|x| x.starts_with("/tmp:rw,exec,nosuid,nodev,size=256m")));
    // Sin montajes ni privilegios.
    assert!(!a
        .iter()
        .any(|x| x == "-v" || x == "--volume" || x == "--mount" || x == "--privileged"));
    // La imagen y después los argumentos fijos del punto de entrada.
    assert_eq!(&a[a.len() - 4..], ["validador-sandbox:1", "c", "60", "10"]);
}

#[test]
fn comprobar_rechaza_nombres_y_tamanos() {
    let lim = Limites::default();
    let ok: &[(&str, &[u8])] = &[("validador.c", b""), ("validador.h", b"")];
    assert_eq!(comprobar(&trabajo(Lenguaje::C, ok, b""), &lim), Ok(()));
    for malo in [
        "../x.c", "a/b.c", "X.c", "a.rs", ".c", "a b.c", "a.c\0", "main",
    ] {
        let v: &[(&str, &[u8])] = &[(malo, b"")];
        assert!(
            comprobar(&trabajo(Lenguaje::C, v, b""), &lim).is_err(),
            "{malo:?}"
        );
    }
    let rep: &[(&str, &[u8])] = &[("a.c", b""), ("a.c", b"")];
    assert!(comprobar(&trabajo(Lenguaje::C, rep, b""), &lim).is_err());
    let sin_main: &[(&str, &[u8])] = &[("lib.rs", b"")];
    assert!(comprobar(&trabajo(Lenguaje::Rust, sin_main, b""), &lim).is_err());
    assert!(comprobar(&trabajo(Lenguaje::C, &[], b""), &lim).is_err());
    assert_eq!(
        comprobar(&trabajo(Lenguaje::C, ok, &vec![0u8; lim.max_entrada]), &lim),
        Ok(())
    );
    let grande = vec![0u8; lim.max_entrada + 1];
    assert!(comprobar(&trabajo(Lenguaje::C, ok, &grande), &lim).is_err());
    // Rechazado sin lanzar Docker (el programa no existe).
    let s = Sandbox {
        docker: "no-existe-docker".into(),
        ..Sandbox::default()
    };
    let v: &[(&str, &[u8])] = &[("../x.c", b"")];
    assert!(matches!(
        ejecutar(&s, &trabajo(Lenguaje::C, v, b"")),
        Resultado::Rechazado(_)
    ));
}

/// Lee un tar ustar con archivos regulares; comprueba la suma de cada cabecera.
fn leer_tar(t: &[u8]) -> Vec<(String, Vec<u8>)> {
    assert_eq!(t.len() % 512, 0);
    let mut v = Vec::new();
    let mut i = 0;
    while i + 512 <= t.len() && t[i..i + 512].iter().any(|&b| b != 0) {
        let h = &t[i..i + 512];
        let campo = |a: usize, b: usize| {
            let s = std::str::from_utf8(&h[a..b]).unwrap();
            u64::from_str_radix(s.trim_matches(|c: char| c == '\0' || c == ' '), 8).unwrap()
        };
        let mut sin_suma = h.to_vec();
        sin_suma[148..156].copy_from_slice(b"        ");
        assert_eq!(
            campo(148, 156),
            sin_suma.iter().map(|&b| u64::from(b)).sum::<u64>()
        );
        assert_eq!(&h[257..263], b"ustar\0");
        assert_eq!(h[156], b'0');
        let nombre =
            String::from_utf8(h[..100].iter().copied().take_while(|&b| b != 0).collect()).unwrap();
        let n = campo(124, 136) as usize;
        v.push((nombre, t[i + 512..i + 512 + n].to_vec()));
        i += 512 + n.div_ceil(512) * 512;
    }
    assert!(t[i..].iter().all(|&b| b == 0) && t.len() - i >= 1024);
    v
}

#[test]
fn tar_ustar_ida_y_vuelta() {
    let grande = vec![7u8; 1500];
    let archivos: &[(&str, &[u8])] = &[
        ("main.rs", b"fn main() {}\n"),
        ("vacio.h", b""),
        ("payload", &grande),
    ];
    let t = tar(archivos);
    let leido = leer_tar(&t);
    assert_eq!(leido.len(), 3);
    for ((n, d), (m, e)) in archivos.iter().zip(&leido) {
        assert_eq!(n, m);
        assert_eq!(*d, e.as_slice());
    }
}

// ---------------------------------------------------------------------------
// Con Docker (`scripts/test-sandbox.ps1`).

fn sandbox() -> Sandbox {
    Sandbox {
        limites: Limites {
            memoria_mib: 128,
            compilar: Duration::from_secs(60),
            ejecutar: Duration::from_secs(3),
            ..Limites::default()
        },
        ..Sandbox::default()
    }
}

fn c(fuente: &str, payload: &[u8]) -> Resultado {
    ejecutar(
        &sandbox(),
        &trabajo(Lenguaje::C, &[("validador.c", fuente.as_bytes())], payload),
    )
}

fn salida(r: &Resultado) -> (i32, String) {
    match r {
        Resultado::Terminado { codigo, stdout, .. } => {
            (*codigo, String::from_utf8_lossy(stdout).into_owned())
        }
        otro => panic!("esperaba Terminado, obtuve {otro:?}"),
    }
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn c_compila_y_lee_el_payload() {
    let r = c(
        r#"#include <stdio.h>
int main(void) { long n = 0; int ch; while ((ch = getchar()) != EOF) n += ch == '{'; printf("%ld\n", n); return n == 2 ? 0 : 1; }"#,
        br#"{"a": {"b": 1}}"#,
    );
    assert_eq!(salida(&r), (0, "2\n".into()));
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn rust_compila_y_lee_el_payload() {
    let fuente = b"use std::io::Read; fn main() { let mut s = String::new(); std::io::stdin().read_to_string(&mut s).unwrap(); println!(\"{}\", s.len()); std::process::exit(if s.len() == 4 { 0 } else { 3 }); }";
    let r = ejecutar(
        &sandbox(),
        &trabajo(Lenguaje::Rust, &[("main.rs", fuente)], b"[1] "),
    );
    assert_eq!(salida(&r), (0, "4\n".into()));
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn error_de_compilacion() {
    match c("int main(void) { return nope; }", b"") {
        Resultado::ErrorCompilacion { diagnostico } => {
            assert!(
                String::from_utf8_lossy(&diagnostico).contains("nope"),
                "{diagnostico:?}"
            )
        }
        otro => panic!("{otro:?}"),
    }
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn bucle_infinito_da_tiempo_agotado() {
    let r = c("int main(void) { volatile int x = 0; for (;;) x++; }", b"");
    assert_eq!(r, Resultado::TiempoAgotado { compilando: false });
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn sin_red_sin_root_y_solo_lectura() {
    let r = c(
        r#"#include <stdio.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
int main(void) {
    int s = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in a = {0}; a.sin_family = AF_INET; a.sin_port = htons(53);
    inet_pton(AF_INET, "1.1.1.1", &a.sin_addr);
    int red = s >= 0 && connect(s, (struct sockaddr *)&a, sizeof a) == 0;
    FILE *f = fopen("/usr/escrito", "w");
    FILE *g = fopen("/tmp/escrito", "w");
    printf("red=%d uid=%d raiz=%d tmp=%d\n", red, (int)getuid(), f != NULL, g != NULL);
    return 0;
}"#,
        b"",
    );
    assert_eq!(salida(&r), (0, "red=0 uid=10001 raiz=0 tmp=1\n".into()));
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn memoria_acotada() {
    let r = c(
        r#"#include <stdlib.h>
#include <string.h>
int main(void) { for (int i = 0; i < 64; i++) { char *p = malloc(16 << 20); if (!p) return 2; memset(p, 1, 16 << 20); } return 0; }"#,
        b"",
    );
    // 1 GiB pedido con 128 MiB de límite: lo mata el núcleo (o `malloc` falla).
    assert!(
        matches!(
            r,
            Resultado::Matado { .. } | Resultado::Terminado { codigo: 2, .. }
        ),
        "{r:?}"
    );
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn bomba_de_fork_acotada() {
    let r = c(
        r#"#include <stdio.h>
#include <unistd.h>
#include <sys/wait.h>
int main(void) {
    int n = 0;
    for (int i = 0; i < 1000; i++) { pid_t p = fork(); if (p < 0) break; if (p == 0) { pause(); _exit(0); } n++; }
    printf("%d\n", n);
    return 0;
}"#,
        b"",
    );
    let (codigo, n) = salida(&r);
    assert_eq!(codigo, 0);
    let n: u32 = n.trim().parse().unwrap();
    assert!(n < 64, "{n} procesos con --pids-limit 64");
}

#[test]
#[ignore = "necesita Docker y la imagen validador-sandbox:1"]
fn salida_recortada() {
    let s = Sandbox {
        limites: Limites {
            max_salida: 1000,
            ..sandbox().limites
        },
        ..sandbox()
    };
    let fuente = b"#include <stdio.h>\nint main(void) { for (int i = 0; i < 100000; i++) putchar('x'); return 0; }";
    let r = ejecutar(&s, &trabajo(Lenguaje::C, &[("validador.c", fuente)], b""));
    match r {
        Resultado::Terminado {
            codigo: 0, stdout, ..
        } => assert_eq!(stdout.len(), 1000),
        otro => panic!("{otro:?}"),
    }
}
