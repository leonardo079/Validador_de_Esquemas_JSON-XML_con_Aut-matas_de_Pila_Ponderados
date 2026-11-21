# Despliegue en EC2 con Docker Compose y Caddy (F7.7, D71)

La aplicación se despliega con **dos contenedores** en una EC2 `t3.micro` (Ubuntu 24.04, us-east-1):

| Servicio | Imagen | Qué hace |
| --- | --- | --- |
| `server` | `validador-server:1` (`infra/Dockerfile.server`, 132 MB) | La API de `server/` en `:8080`, solo dentro de la red de Compose. |
| `web` | `validador-web:1` (`infra/Dockerfile.web`, 76 MB) | Caddy 2.8.4: sirve `web/dist`, reenvía `/api/*` a `server`, pone HTTPS (Let's Encrypt) y las cabeceras de seguridad. |

Archivos: `infra/docker-compose.yml`, `infra/Caddyfile`, `infra/ec2-preparar.sh`, `infra/desplegar.ps1` y
`.dockerignore`.

## 1. Probar en local (Windows con Docker Desktop)

```powershell
docker compose -f infra/docker-compose.yml build
$env:DOMINIO="http://localhost"; $env:HTTP_PORT="8088"; $env:HTTPS_PORT="8443"
docker compose -f infra/docker-compose.yml up -d --no-build
# http://localhost:8088
docker compose -f infra/docker-compose.yml down
```

Comprobado el 20 nov contra Caddy (no contra el servidor directo):

- `/api/health` da 200;
- la web y sus rutas de SPA dan 200;
- `POST /api/schemas` da 200 y 422, y `/api/compare` responde;
- las cabeceras CSP, HSTS, `X-Frame-Options`, `nosniff` y `Referrer-Policy` están presentes;
- cuerpos de 1,5 MB y 3 MB, con `Content-Length` y en `chunked`, dan 413 JSON en milisegundos;
- un cuerpo de exactamente 1 MiB pasa.

## 2. El sábado: desplegar en la EC2

Requisitos de S0, ya listos: la cuenta AWS, el key pair `validador-ec2` y el security group `validador-web`, que
tiene SSH (22) solo desde tu IP y HTTP (80) y HTTPS (443) abiertos. El 80 es imprescindible para que
Let's Encrypt valide el dominio.

1. **Crear la instancia** (EC2 → Launch instance):
   - Ubuntu Server 24.04 LTS, `t3.micro`, key pair `validador-ec2`, security group `validador-web`;
   - 20 GB gp3.
2. **Elastic IP:** EC2 → Elastic IPs → Allocate → Associate a la instancia. Esa es la IP fija; anótala, por
   ejemplo `3.4.5.6`.
3. **Atlas:** en Network Access, añadir esa IP. Hace falta desde F7.2; hoy el servidor aún no usa Mongo.
4. **Preparar la máquina**, una vez:
   ```powershell
   ssh -i $HOME\.ssh\validador-ec2.pem ubuntu@3.4.5.6
   ```
   ```sh
   curl -fsSL https://raw.githubusercontent.com/leonardo079/Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados/main/infra/ec2-preparar.sh | sh
   exit
   ```
   Instala Docker con el plugin de Compose, crea 2 GiB de swap y clona el repo en `~/validador`.
5. **Desplegar desde tu PC:**
   ```powershell
   powershell -NoProfile -File infra/desplegar.ps1 -Ip 3.4.5.6 -Llave $HOME\.ssh\validador-ec2.pem
   ```
   El script hace todo esto:
   - construye las imágenes aquí, porque compilar Rust en release en la `t3.micro` tarda mucho;
   - las copia y las carga en la EC2 (`docker save` → `scp` → `docker load`);
   - hace `git pull` y `docker compose up -d --no-build` con `DOMINIO=3-4-5-6.sslip.io`;
   - espera a que `https://3-4-5-6.sslip.io/api/health` responda.

   Caddy pide el certificado solo; `sslip.io` resuelve el nombre a la IP sin comprar dominio.
6. **URL pública:** `https://3-4-5-6.sslip.io`. Para actualizar tras nuevos commits, repetir el paso 5.

Si algo falla, en la EC2:

```sh
cd ~/validador
docker compose -f infra/docker-compose.yml ps
docker compose -f infra/docker-compose.yml logs --tail 50 web
docker compose -f infra/docker-compose.yml logs --tail 50 server
```

- Si el certificado no sale: revisa que el 80 esté abierto en el security group y que el `DOMINIO` corresponda a
  la Elastic IP. Let's Encrypt limita los intentos, así que no lo repitas en bucle.
- Plan B, construir en la EC2: `docker compose -f infra/docker-compose.yml build`. Con la swap funciona, pero
  tarda.

## 3. Endurecimiento (lo que F7.7 heredó de `pendientes.md`)

| Pendiente | Cómo queda |
| --- | --- |
| §1 (SEC-M2): tope de memoria del proceso | `mem_limit`/`memswap_limit: 600m` y `restart: unless-stopped` en `server`. Un fallo de asignación aborta el proceso y Compose lo reinicia. El pico medido es ≈ 380 MiB. |
| §8: Caddy delante | `timeouts` `read_header 10s`, `read_body 20s`, `write 60s`, `idle 2m`; `max_header_size 32KB`; `request_body max_size 1MiB`, `request_buffers 1MiB` y el rechazo por `Content-Length` antes de leer. |
| §11: registro sin tope | `logging` `json-file` con `max-size 10m` y `max-file 3` en los dos servicios. |
| HSTS y CSP (`servidor.md` §2) | En el `Caddyfile`. La CSP es `default-src 'self'`, más los estilos en línea y los workers `blob:` que necesita Monaco. |
| Contenedores | `server` corre como usuario 10002, con `read_only`, `cap_drop: ALL`, `no-new-privileges` y `pids_limit 128`. `web` también lleva `cap_drop: ALL` (salvo `NET_BIND_SERVICE`) y `pids_limit`. |
| Secretos | `.dockerignore` deja fuera `.env`, `*.pem`, `tasks/contexto/` y `.git`. Ninguna imagen lleva credenciales. |

**Sin hacer:**

- el límite de conexiones por IP (Caddy no lo trae sin un plugin; lo acota el semáforo del servidor);
- `cargo audit`;
- montar el socket de Docker para el sandbox (F7.3). Ningún endpoint lo usa hasta F7.5; cuando lo haga, se añade
  el volumen `/var/run/docker.sock` a `server`, con el riesgo de `sandbox.md` §5.

## 4. Costo

El Free plan de AWS cubre la `t3.micro` y una Elastic IP **asociada**. Una Elastic IP sin asociar se cobra:
suéltala si borras la instancia. Los budgets `cero-gasto` y `mensual-5usd` avisan de cualquier gasto.
