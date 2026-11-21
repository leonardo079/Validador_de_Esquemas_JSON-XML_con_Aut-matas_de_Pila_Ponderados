#!/bin/sh
# Prepara una EC2 Ubuntu 24.04 recién creada (F7.7, D71; `docs/fase7/despliegue.md`).
# Se corre UNA vez, en la instancia, como el usuario `ubuntu`:
#
#   curl -fsSL https://raw.githubusercontent.com/leonardo079/Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados/main/infra/ec2-preparar.sh | sh
#
# Instala Docker Engine y el plugin de Compose (repositorio oficial de Docker),
# crea 2 GiB de swap (la t3.micro tiene 1 GiB de RAM) y clona el repo en
# ~/validador. No abre puertos: eso lo hace el security group `validador-web`.
set -eu

sudo apt-get update
sudo apt-get install -y ca-certificates curl git
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc
. /etc/os-release
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu ${VERSION_CODENAME} stable" |
    sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo usermod -aG docker "$USER"

# Swap: margen para picos (y por si hay que construir aquí, que es lento).
if ! swapon --show | grep -q /swapfile; then
    sudo fallocate -l 2G /swapfile
    sudo chmod 600 /swapfile
    sudo mkswap /swapfile
    sudo swapon /swapfile
    echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab >/dev/null
fi

if [ ! -d "$HOME/validador" ]; then
    git clone https://github.com/leonardo079/Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados.git "$HOME/validador"
fi

echo "listo: cierra la sesión SSH y vuelve a entrar para usar docker sin sudo"
