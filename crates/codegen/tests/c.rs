//! Emisor C11 (F5.2, D75; criterios en `tasks/todo.md` «Tarea F5.2»).
//!
//! Compila de verdad lo emitido con el compilador de C del sistema (`CC` o
//! `cc`) en `CARGO_TARGET_TMPDIR`:
//!
//! - **El `main.c` real** de cada esquema del corpus con la orden exacta del
//!   sandbox (`cc -std=c11 -O2 -Wall -Wextra -o validador *.c`, D68), sin
//!   avisos; cada payload por stdin da la misma línea JSON que el intérprete y
//!   el código de salida, igual que el binario Rust (F5.1).
//! - **Estricto:** con gcc y clang (los que haya), `-Werror -pedantic
//!   -Wshadow -Wconversion` en todos los esquemas.
//! - **Sin reservas por construcción:** los objetos del camino de validación
//!   (`validador.c`, `tablas.c`, `sv_nucleo.c`, `sv_lexico.c`) no tienen más
//!   símbolos externos que `memcmp`, `memcpy`, `memmove`, `memset` y `strlen`
//!   (`nm -u`): ni `malloc` ni ningún otro asignador. `main.c` queda fuera:
//!   lee stdin y, una vez antes de `sv_init`, `/dev/urandom` (§31 de
//!   `docs/fase6/pendientes.md`), que no son el camino de validación.
//! - Salida reproducible y núcleo copiado sin cambios.
//!
//! La equivalencia en bloques de 1, 7 y 4 096 bytes y en los dos modos, con
//! sanitizadores, es la prueba diferencial de F5.3 (`tests/diferencial.rs`).
//! Ninguna prueba mide tiempo (D1).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use automaton::build;
use codegen::c::{constantes, emit, NUCLEO};
use codegen::Archivo;

mod comun;
use comun::*;

/// Los `.c` de `archivos`, en orden alfabético (el `*.c` del sandbox).
fn fuentes_c(archivos: &[Archivo]) -> Vec<String> {
    let mut v: Vec<String> = archivos
        .iter()
        .filter(|a| a.nombre.ends_with(".c"))
        .map(|a| a.nombre.clone())
        .collect();
    v.sort();
    v
}

/// Si `orden` existe (`orden --version` termina bien).
fn existe(orden: &str) -> bool {
    Command::new(orden)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Criterio 1: la orden del sandbox en todo el corpus, sin avisos.
#[test]
fn main_c_real_en_todo_el_corpus() {
    let grupos = grupos();
    let cc = cc();
    let binarios = en_paralelo(&grupos, |k, g| {
        let archivos = emit(&g.a);
        nombres_del_sandbox(&archivos, &[".c", ".h"], "main.c");
        let dir = directorio(&format!("c-main{k}"));
        escribir(&dir, &archivos);
        let mut args = vec!["-std=c11", "-O2", "-Wall", "-Wextra", "-o", "validador"];
        let fuentes = fuentes_c(&archivos);
        args.extend(fuentes.iter().map(String::as_str));
        let avisos = compilar_con(&dir, &cc, &args);
        assert!(avisos.is_empty(), "esquema {k}: {avisos}");
        dir.join("validador")
    });
    corpus_por_stdin(&grupos, &binarios);
}

/// Criterio 2: sin avisos con banderas estrictas, en gcc y clang.
#[test]
fn c_estricto_en_gcc_y_clang() {
    let grupos = grupos();
    let compiladores: Vec<&str> = ["gcc", "clang"].into_iter().filter(|c| existe(c)).collect();
    assert!(!compiladores.is_empty(), "ni gcc ni clang");
    en_paralelo(&grupos, |k, g| {
        let archivos = emit(&g.a);
        let dir = directorio(&format!("c-estricto{k}"));
        escribir(&dir, &archivos);
        for cc in &compiladores {
            let mut args = vec![
                "-std=c11",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-pedantic",
                "-Wshadow",
                "-Wconversion",
                "-Wno-sign-conversion",
                "-c",
            ];
            let fuentes = fuentes_c(&archivos);
            args.extend(fuentes.iter().map(String::as_str));
            let avisos = compilar_con(&dir, cc, &args);
            assert!(avisos.is_empty(), "{cc}, esquema {k}: {avisos}");
        }
    });
}

/// Los símbolos sin definir de los objetos `objs` que ninguno de ellos
/// define (`nm`).
fn externos(dir: &Path, objs: &[&str]) -> BTreeSet<String> {
    let nm = |args: &[&str]| -> BTreeSet<String> {
        let out = Command::new("nm")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter_map(|l| l.split_whitespace().last())
            .filter(|s| !s.ends_with(':') && !s.ends_with(".o"))
            .map(str::to_owned)
            .collect()
    };
    let mut u = vec!["-u"];
    u.extend_from_slice(objs);
    let mut d = vec!["--defined-only"];
    d.extend_from_slice(objs);
    let definidos = nm(&d);
    nm(&u).difference(&definidos).cloned().collect()
}

/// Criterio 3: el camino de validación no tiene a quién pedir memoria. El
/// testigo negativo (un objeto con `malloc`) muestra que `nm` lo vería.
#[test]
fn camino_de_validacion_sin_malloc() {
    let grupos = grupos();
    let permitidos: BTreeSet<String> = ["memcmp", "memcpy", "memmove", "memset", "strlen"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let objs = ["validador.o", "tablas.o", "sv_nucleo.o", "sv_lexico.o"];
    for g in [
        grupos.iter().find(|g| !g.xml).unwrap(),
        grupos.iter().find(|g| g.xml).unwrap(),
    ] {
        let dir = directorio(if g.xml { "c-nm-xml" } else { "c-nm-json" });
        escribir(&dir, &emit(&g.a));
        compilar_con(
            &dir,
            &cc(),
            &[
                "-std=c11",
                "-O2",
                "-c",
                "validador.c",
                "tablas.c",
                "sv_nucleo.c",
                "sv_lexico.c",
            ],
        );
        let ext = externos(&dir, &objs);
        assert!(ext.is_subset(&permitidos), "símbolos externos: {ext:?}");
        std::fs::write(
            dir.join("testigo.c"),
            "#include <stdlib.h>\nvoid* f(void) { return malloc(8); }\n",
        )
        .unwrap();
        compilar_con(&dir, &cc(), &["-std=c11", "-O2", "-c", "testigo.c"]);
        assert!(externos(&dir, &["testigo.o"]).contains("malloc"));
    }
}

/// Criterio 4: misma M, mismos bytes (también reconstruyendo M).
#[test]
fn salida_c_reproducible() {
    for g in grupos() {
        let otra = build(&ir_de(&g.texto, g.xml)).unwrap();
        assert_eq!(emit(&g.a), emit(&otra));
        assert_eq!(emit(&g.a), emit(&g.a));
    }
}

/// Criterio 5: el núcleo C se copia sin cambios de `crates/runtime-c`, y
/// cada constante del lexer que usa (`SV_U8_*`, `SV_JS_*`, `SV_XL_*`) está
/// en `sv_lexico.h`, con el valor de la fuente de Rust.
#[test]
fn nucleo_c_sincronizado() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime-c");
    for (nombre, fuente) in NUCLEO {
        let disco = std::fs::read_to_string(raiz.join(nombre)).unwrap();
        assert_eq!(fuente, disco, "{nombre}");
    }
    let g = grupos();
    let emitido = emit(&g[0].a);
    let h = &emitido
        .iter()
        .find(|a| a.nombre == "sv_lexico.h")
        .unwrap()
        .contenido;
    let definidas: BTreeSet<&str> = h
        .lines()
        .filter_map(|l| l.strip_prefix("#define "))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let nucleo = NUCLEO[1].1;
    let mut usadas = BTreeSet::new();
    for pre in ["SV_U8_", "SV_JS_", "SV_XL_"] {
        let mut resto = nucleo;
        while let Some(i) = resto.find(pre) {
            let fin = resto[i..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(resto.len(), |f| i + f);
            usadas.insert(&resto[i..fin]);
            resto = &resto[fin..];
        }
    }
    assert!(usadas.len() > 60, "{usadas:?}");
    for u in &usadas {
        assert!(definidas.contains(u), "{u} no está en sv_lexico.h");
    }
    // Los valores son los de Rust: una muestra con nombre.
    assert!(constantes("json").contains(&("S_VALUE", 0)));
    assert!(constantes("xml").contains(&("S_CONTENT", 0)));
    assert!(constantes("utf8").contains(&("REJECT", 1)));
    assert_eq!(
        constantes("json")
            .iter()
            .find(|(n, _)| *n == "NUM_STATES")
            .map(|p| p.1),
        Some(lexer::json::NUM_STATES as u32)
    );
}

/// §31 de `docs/fase6/pendientes.md`: los dos `main` emitidos validan con una
/// clave de SipHash aleatoria (de `/dev/urandom` en C, de `RandomState` en
/// Rust), no con la pública por defecto. Que los reportes no cambian lo
/// comprueban `limites.rs` (los binarios con claves repetidas, frente al
/// intérprete) y la diferencial.
#[test]
fn main_con_clave_aleatoria() {
    let g = grupos();
    let a = &g[0].a;
    let buscar = |archivos: &[Archivo], nombre: &str| {
        archivos
            .iter()
            .find(|f| f.nombre == nombre)
            .unwrap()
            .contenido
            .clone()
    };
    let c = buscar(&emit(a), "main.c");
    assert!(c.contains("fopen(\"/dev/urandom\", \"rb\")"), "{c}");
    assert!(c.contains("clave_aleatoria(lim.hash_key);\n    sv_init(&estado, &lim);"));
    assert!(!c.contains("sv_init(&estado, NULL)"));
    let r = buscar(&codegen::rust::emit(a), "main.rs");
    assert!(r.contains("Validador::con_clave(&mut mem, Mode::Total, clave_aleatoria())"));
    assert!(r.contains("RandomState::new()"));
}
