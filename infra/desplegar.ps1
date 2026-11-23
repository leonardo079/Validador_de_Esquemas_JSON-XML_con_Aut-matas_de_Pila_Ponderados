# Despliega desde Windows a la EC2 (F7.7, D71; `docs/fase7/despliegue.md`).
#
#   powershell -File infra/desplegar.ps1 -Ip 3.4.5.6 -Llave $HOME\.ssh\validador-ec2.pem
#
# 1. Construye las dos imágenes aquí (la t3.micro tarda mucho en compilar Rust).
# 2. Las guarda con `docker save`, las copia por scp y las carga en la EC2.
# 3. Actualiza el repo en la EC2 (Compose y Caddyfile) y levanta los servicios
#    con DOMINIO=<ip-con-guiones>.sslip.io (Caddy saca el certificado solo).
# 4. Comprueba https://<dominio>/api/health.
#
# Con -Sandbox (D85) construye además `validador-sandbox:1` EN la EC2 (la base
# de Rust se descarga allí, más rápido que subirla) y levanta el servidor con
# `docker-compose.sandbox.yml`: socket de Docker y `SANDBOX=docker`.
param(
    [Parameter(Mandatory = $true)][string]$Ip,
    [Parameter(Mandatory = $true)][string]$Llave,
    [string]$Usuario = 'ubuntu',
    [switch]$Sandbox
)
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$compose = Join-Path $repo 'infra/docker-compose.yml'
$dominio = ($Ip -replace '\.', '-') + '.sslip.io'
$destino = "$Usuario@$Ip"
$tar = Join-Path ([System.IO.Path]::GetTempPath()) 'validador-imagenes.tar'
$ssh = @('-i', $Llave, '-o', 'StrictHostKeyChecking=accept-new')

docker compose -f $compose build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

docker save -o $tar validador-server:1 validador-web:1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

scp @ssh $tar "${destino}:/tmp/validador-imagenes.tar"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Remove-Item $tar

# F7.2: el `.env` (MONGODB_URI) va aparte, solo a la EC2, con permisos 600.
$env_local = Join-Path $repo '.env'
if (Test-Path $env_local) {
    # E3C-11: el archivo se crea vacío con permisos 600 ANTES de copiar la
    # contraseña (scp conserva los permisos de un archivo que ya existe).
    ssh @ssh $destino "umask 077; : > ~/validador/.env; chmod 600 ~/validador/.env"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    scp @ssh $env_local "${destino}:validador/.env"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    Write-Host "Sin .env: el servidor arranca sin MongoDB (/api/tests responde 503)."
}

$archivos = '-f infra/docker-compose.yml'
$previo = ''
if ($Sandbox) {
    $archivos += ' -f infra/docker-compose.sandbox.yml'
    $previo = 'docker build -q -t validador-sandbox:1 sandbox; ' +
        'export DOCKER_GID=$(getent group docker | cut -d: -f3); '
}
$remoto = "set -e; docker load -i /tmp/validador-imagenes.tar && rm /tmp/validador-imagenes.tar; " +
    "cd ~/validador && git pull --ff-only; " + $previo +
    "DOMINIO=$dominio docker compose $archivos up -d --no-build; " +
    "docker compose $archivos ps"
ssh @ssh $destino $remoto
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Esperando el certificado de https://$dominio ..."
for ($i = 0; $i -lt 30; $i++) {
    try {
        $r = Invoke-WebRequest -UseBasicParsing -TimeoutSec 5 "https://$dominio/api/health"
        Write-Host "OK: $($r.Content)"
        Write-Host "Aplicación: https://$dominio"
        exit 0
    } catch { Start-Sleep -Seconds 5 }
}
Write-Host "No respondió todavía: revisa 'docker compose logs web' en la EC2 y el security group (80/443)."
exit 1
