# Sandbox para compilar y ejecutar el código generado (F7.3, D68)

El enunciado exige que la compilación y la ejecución del código generado ocurran solo dentro de un contenedor
aislado: sin red, con límites de memoria, CPU y procesos, con tope de tiempo, sin root y con el sistema de
archivos de solo lectura. F7.3 construye ese sandbox. F7.5 lo usará con el código que emitan F5.1 (Rust) y F5.2
(C).

| Pieza | Archivo |
| --- | --- |
| Imagen | `sandbox/Dockerfile` (`validador-sandbox:1`) |
| Punto de entrada (órdenes fijas) | `sandbox/sandbox-run.sh` |
| Ejecutor en el servidor | `server/src/sandbox.rs` (`server::sandbox`) |
| Pruebas | `server/tests/sandbox.rs`; con Docker: `scripts/test-sandbox.ps1` o `make test-sandbox` |

## 1. Cómo se usa

```rust
use server::sandbox::{ejecutar, Lenguaje, Resultado, Sandbox, Trabajo};

let r = ejecutar(&Sandbox::default(), &Trabajo {
    lenguaje: Lenguaje::C,
    archivos: &[("validador.c", fuente_c), ("validador.h", cabecera)],
    payload: b"{\"a\": 1}",
});
match r {
    Resultado::Terminado { codigo, stdout, stderr } => { /* el veredicto del binario */ }
    Resultado::ErrorCompilacion { diagnostico } => { /* … */ }
    Resultado::TiempoAgotado { compilando } => { /* … */ }
    Resultado::Matado { .. } => { /* memoria, normalmente */ }
    Resultado::Rechazado(motivo) => { /* entrada no válida o Docker no disponible */ }
}
```

`ejecutar` bloquea hasta terminar; desde el servidor (tokio) se llama con `spawn_blocking`. Construir la imagen
una vez:

```text
docker build -t validador-sandbox:1 sandbox
```

## 2. Qué pasa en cada trabajo

1. **Comprobación** (`comprobar`), antes de lanzar nada:
   - nombres `[a-z0-9_]+` con la extensión del lenguaje (`.c`/`.h` o `.rs`), sin rutas ni repetidos;
   - en Rust, `main.rs` es obligatorio;
   - la entrada (fuentes más payload) no pasa de 16 MiB.
2. **Entrada por tar.** Los fuentes y el archivo `payload` se empaquetan en un tar ustar mínimo (`tar`, sin
   crates) y entran por la entrada estándar de `docker run -i`. **No hay montajes de volúmenes.** Así no se expone
   ninguna ruta del anfitrión, y funciona igual cuando el servidor corre en otro contenedor y habla con Docker por
   su socket (F7.7).
3. **Contenedor nuevo por trabajo** (`--rm`, nombre `sv-<pid>-<n>`), con estas restricciones:

   | Restricción | Bandera | Valor por defecto |
   | --- | --- | --- |
   | Sin red | `--network none` | — |
   | Sin root | `--user 10001:10001` | usuario `sandbox` de la imagen |
   | Raíz de solo lectura | `--read-only` | — |
   | Única zona escribible | `--tmpfs /tmp:rw,exec,nosuid,nodev,size=…` | 256 MiB |
   | Memoria, sin swap | `--memory` = `--memory-swap` | 512 MiB |
   | CPU | `--cpus` | 1 núcleo |
   | Procesos e hilos | `--pids-limit` | 64 (corta una bomba de `fork`) |
   | Archivos abiertos, tamaño de archivo, *core* | `--ulimit nofile`, `fsize`, `core=0` | 256, tamaño del tmpfs, 0 |
   | Sin capacidades ni escalada | `--cap-drop ALL`, `--security-opt no-new-privileges` | — |
   | Sin IPC compartido | `--ipc none` | — |

4. **Órdenes fijas** (`sandbox-run <c|rust> <s compilar> <s ejecutar>`). Desde fuera solo se elige el lenguaje;
   nunca la orden.
   - C: `cc -std=c11 -O2 -Wall -Wextra -o /tmp/out/validador *.c`.
   - Rust: `rustc --edition 2021 -O main.rs`.
   - Después, `timeout -k 1 <s> /tmp/out/validador < payload`.
5. **Tiempo, en dos capas:**
   - dentro del contenedor, `timeout` para la compilación (60 s) y para la ejecución (10 s);
   - fuera, un plazo de pared (compilación + ejecución + 15 s de arranque). Si se pasa, el servidor hace
     `docker kill`, que también corta un contenedor colgado en el arranque.

   Este proceso aparte, al que sí se le puede mandar una señal, es lo que pedía `pendientes.md` §12.
6. **Salida acotada:** se guardan como mucho 64 KiB de `stdout` y otros tantos de `stderr`; el resto se lee y se
   descarta, para que el contenedor no se bloquee.

## 3. Resultados y códigos de salida

| Código de `sandbox-run` | Resultado |
| --- | --- |
| 100 | `ErrorCompilacion` (diagnóstico recortado a 64 KiB) |
| 101, 102 | `Rechazado` (tar ilegible, argumentos) |
| 103 | `TiempoAgotado { compilando: true }` |
| 124, o el plazo de pared | `TiempoAgotado { compilando: false }` |
| 137, o terminado por señal | `Matado` (límite de memoria del cgroup, normalmente) |
| 125-127 | `Rechazado` (falló el propio `docker run`: imagen o demonio) |
| cualquier otro | `Terminado { codigo, stdout, stderr }` |

El validador generado (F5.1/F5.2) no debe usar los códigos 100-103, 124 ni 125-127 para su veredicto; con 0, 1 y
2 basta.

## 4. Pruebas

- **Sin Docker** (corren en `cargo test`):
  - los argumentos de `docker run` incluyen todas las restricciones y ningún montaje;
  - `comprobar` rechaza rutas, mayúsculas, extensiones ajenas, repetidos, la falta de `main.rs` y el tamaño;
  - el tar es ustar válido, con las sumas de control de cabecera, y hace ida y vuelta.
- **Con Docker** (`#[ignore]`; las corre `scripts/test-sandbox.ps1` o `make test-sandbox`):
  - C y Rust compilan y leen el payload;
  - un error de compilación da su diagnóstico;
  - un bucle infinito da `TiempoAgotado`;
  - dentro del contenedor no hay red (`connect` a 1.1.1.1 falla), el uid es 10001, `/usr` no se puede escribir y
    `/tmp` sí;
  - pedir 1 GiB con 128 MiB de límite lo mata;
  - una bomba de `fork` se queda por debajo de 64 procesos;
  - la salida se recorta.

  Ninguna mide tiempo (D1).

## 5. Riesgos y límites

- **El socket de Docker.** En el despliegue (F7.7) el servidor lanza contenedores hermanos con
  `/var/run/docker.sock`, y quien controla ese socket controla el anfitrión. Lo mitiga que las órdenes y las
  banderas sean fijas en el código y que de la petición solo lleguen el lenguaje, los nombres validados y los
  bytes. Alternativa más estricta si sobra tiempo: Docker *rootless* en la EC2, o gVisor (`--runtime runsc`).
- **Costo por trabajo:** arrancar un contenedor y compilar cuesta entre ~1 s (C) y varios segundos (Rust con
  `-O`). F7.5 debería guardar el binario compilado por `id` de esquema y ejecutar solo la validación en las
  peticiones siguientes; queda como F7.3-P1 (`pendientes.md` §13).
- **El código generado debe ser autocontenido** (un `main.rs`, o `.c`/`.h`), sin dependencias de Cargo: la
  compilación corre sin red. Si F5.1 emite un crate que depende de `runtime-rs`, F7.5 tendrá que copiar sus
  fuentes en la imagen; queda como F7.3-P2 (`pendientes.md` §14).
- **Compilación de esquemas** (`POST /api/schemas`, `pendientes.md` §1): sigue en un hilo del servidor. El tope
  de memoria del proceso lo pone `mem_limit` de Compose (F7.7). Llevar también esa compilación al sandbox es
  posible con esta misma pieza, pero no compensa el arranque de un contenedor por petición.
