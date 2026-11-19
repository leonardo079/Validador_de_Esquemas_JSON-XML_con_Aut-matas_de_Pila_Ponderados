#!/bin/sh
# Pruebas y clippy de la línea base XML (feature `libxml2`, F6.7b). Corre
# DENTRO del contenedor de `crates/baseline/docker/Dockerfile`, sin red
# (`--network none`) y con las dependencias ya descargadas (`cargo fetch`):
# lo lanzan `make test-baseline-xml` y `scripts/test-baseline-xml.ps1`
# (`docs/fase6/linea-base.md` §7.3).
set -eu

echo "libxml2 $(pkg-config --modversion libxml-2.0) (paquete libxml2-dev $(dpkg-query -W -f='${Version}' libxml2-dev))"
rustc -V

cargo test -p baseline --features libxml2 --locked --offline
cargo clippy -p baseline --all-targets --features libxml2 --locked --offline -- -D warnings

echo "test-baseline-xml: pruebas y clippy de la línea base XML en verde"
