# Imagen para probar la línea base XML (feature `libxml2` de `crates/baseline`,
# F6.7b; `docs/fase6/linea-base.md` §7). La usan `make test-baseline-xml` y
# `scripts/test-baseline-xml.ps1`.
#
# - Imagen de Rust fijada por versión y por digest (B9): la misma 1.100.0 que
#   fija `rust-toolchain.toml` en las máquinas del equipo (canal `stable` el
#   15 nov 2026), Debian 13 (trixie). El crate `libxml` 0.3.23 pide ≥ 1.88.
# - libxml2, su `pkg-config` y libclang (para el bindgen del `build.rs` de
#   `libxml`). Las versiones de Debian no se fijan (los parches de seguridad
#   retiran las viejas del espejo); el script de pruebas imprime la de
#   libxml2 en cada ejecución.
# - `RUSTUP_TOOLCHAIN` evita que `rust-toolchain.toml` (canal `stable`, con
#   `llvm-tools-preview`) haga que rustup descargue otra cadena: las pruebas
#   corren sin red (`--network none`).
FROM rust:1.100.0-slim-trixie@sha256:e9d95b74296f7126a83ae3d041aeaba1ff4c478f13abc5cc034fcf7c131c3d4a

RUN apt-get update \
    && apt-get install -y --no-install-recommends libxml2-dev libclang-dev pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add clippy

ENV RUSTUP_TOOLCHAIN=1.100.0 \
    CARGO_TARGET_DIR=/target \
    CARGO_BUILD_JOBS=4
