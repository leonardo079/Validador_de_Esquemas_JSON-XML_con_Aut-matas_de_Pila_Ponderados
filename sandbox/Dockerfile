# Sandbox para compilar y ejecutar el código generado (F7.3, D68;
# `docs/fase7/sandbox.md`). La usa `server::sandbox`, que lanza un contenedor
# por trabajo con:
#
#   --network none --read-only --tmpfs /tmp:exec --user 10001:10001
#   --memory --memory-swap --cpus --pids-limit --ulimit
#   --cap-drop ALL --security-opt no-new-privileges
#
# y le pasa los fuentes y el payload como un tar por la entrada estándar (sin
# montajes: funciona igual si el servidor también corre en un contenedor).
#
# - Misma imagen de Rust que la línea base XML (`crates/baseline/docker`),
#   fijada por versión y digest: comparten capas. Trae `rustc` y `gcc`/`cc`
#   (C11), `tar` y `timeout`.
# - `RUSTUP_TOOLCHAIN` fija la cadena: sin red, rustup no puede descargar otra.
# - El usuario `sandbox` (10001) no tiene shell de login ni permisos de
#   escritura fuera de `/tmp`, que en ejecución es un tmpfs.
FROM rust:1.100.0-slim-trixie@sha256:e9d95b74296f7126a83ae3d041aeaba1ff4c478f13abc5cc034fcf7c131c3d4a

RUN useradd --uid 10001 --no-create-home --home-dir /tmp --shell /usr/sbin/nologin sandbox

COPY sandbox-run.sh /usr/local/bin/sandbox-run
RUN chmod 0555 /usr/local/bin/sandbox-run

ENV RUSTUP_TOOLCHAIN=1.100.0 \
    HOME=/tmp \
    TMPDIR=/tmp

USER 10001:10001
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/sandbox-run"]
