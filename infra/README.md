# infra

Despliegue (F7.7, D71): Docker Compose con dos servicios, `server` (la API) y `web` (Caddy con
la web ya compilada, proxy de `/api` y HTTPS). Paso a paso en
[docs/fase7/despliegue.md](../docs/fase7/despliegue.md).

| Archivo | Para qué |
| --- | --- |
| `docker-compose.yml` | Los dos servicios con sus límites (memoria, procesos, solo lectura, logs). |
| `Dockerfile.server` | Compila `server` en release y lo deja sobre Debian slim, sin root. |
| `Dockerfile.web` | Compila `web/` con Vite y lo sirve con Caddy. |
| `Caddyfile` | HTTPS por `DOMINIO`, cabeceras de seguridad, topes de cuerpo y plazos. |
| `ec2-preparar.sh` | Una vez en la EC2: Docker, swap y clon del repo. |
| `desplegar.ps1` | Desde Windows: construye, sube las imágenes y levanta todo en la EC2. |
