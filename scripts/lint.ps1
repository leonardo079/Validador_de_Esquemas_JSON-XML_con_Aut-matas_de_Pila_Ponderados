# Equivalente de `make lint` para Windows sin make.
$ErrorActionPreference = 'Stop'

cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$cFiles = Get-ChildItem -Recurse -Path crates/runtime-c -Include *.c, *.h | ForEach-Object { $_.FullName }
if ($cFiles) {
    if (Get-Command clang-format -ErrorAction SilentlyContinue) {
        clang-format --dry-run --Werror $cFiles
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } else {
        Write-Warning 'clang-format no está instalado; se omite el formato C.'
    }
}

Write-Host 'lint OK'
