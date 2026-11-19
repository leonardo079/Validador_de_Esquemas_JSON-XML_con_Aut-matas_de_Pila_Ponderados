# Equivalente de `make test-baseline-xml` para Windows sin make (F6.7b;
# `docs/fase6/linea-base.md` §7.3). Necesita Docker Desktop arrancado.
#
# 1. Construye la imagen (Rust 1.100.0 fijada por digest + libxml2, pkg-config
#    y libclang de Debian 13).
# 2. Descarga las dependencias en un volumen de caché (con red).
# 3. Corre `cargo test` y `cargo clippy` con la feature `libxml2` SIN red
#    (`--network none`), sin capacidades (`--cap-drop ALL`) ni escalada de
#    privilegios, con el repositorio montado en solo lectura y el `target` en
#    un volumen aparte (nunca el `target/` de Windows). Mantener a la par con
#    el objetivo `test-baseline-xml` del Makefile, flag por flag.
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$imagen = 'validador-baseline-xml:1'
$montajes = @(
    '-v', "${repo}:/src:ro",
    '-v', 'validador-cargo-registry:/usr/local/cargo/registry',
    '-v', 'validador-baseline-xml-target:/target',
    '-w', '/src'
)

docker build -t $imagen (Join-Path $repo 'crates/baseline/docker')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

docker run --rm @montajes $imagen cargo fetch --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

docker run --rm --network none --cap-drop ALL --security-opt no-new-privileges @montajes $imagen sh scripts/test-baseline-xml.sh
exit $LASTEXITCODE
