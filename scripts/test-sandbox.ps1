# Pruebas del sandbox con Docker (F7.3, D68; `docs/fase7/sandbox.md`).
# Necesita Docker Desktop arrancado. Mantener a la par con `make test-sandbox`.
#
# 1. Construye la imagen `validador-sandbox:1` desde `sandbox/`.
# 2. Corre las pruebas `#[ignore]` de `server/tests/sandbox.rs`, que lanzan
#    un contenedor por caso (de uno en uno: cada uno usa su cuota de CPU y
#    memoria).
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

docker build -t validador-sandbox:1 (Join-Path $repo 'sandbox')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo test -p server --release --test sandbox -- --ignored --test-threads 1
exit $LASTEXITCODE
