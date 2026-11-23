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
3. **Atlas:** en Network Access, añadir esa IP. Sin ella, `/api/tests` da 502. `desplegar.ps1` copia el `.env` (`MONGODB_URI`) a la EC2 (`atlas.md` §5).
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

**Desplegado el 22 nov en https://54-157-96-49.sslip.io** (Elastic IP 54.157.96.49). El tar de las dos imágenes
pesa 55 MB, y el certificado salió en el primer intento. Comprobado desde fuera:

| Prueba | Resultado |
| --- | --- |
| Web | 200 con CSP, HSTS, `nosniff` y `X-Frame-Options`; `http://` → 308 a `https://` |
| `GET /api/health` | `{"database":true,"sandbox":false,"status":"ok"}` |
| `POST /api/tests/run` | 86 casos de Atlas, 86 pasan |
| `POST /api/automaton`, `POST /api/code` | 200; el ZIP en C ocupa 136 KB |
| `WS /ws/validate` | `ready`, dos `progress` y el `report` con el error en `/1`, a través de Caddy |
| `POST /api/validate` | 503 `no-sandbox` en el primer despliegue; con `-Sandbox` (D85, §2.1), el código compilado en el sandbox |

### 2.1 Con el sandbox del código generado (D85)

Para que `POST /api/validate` compile y ejecute el código generado en la EC2, la instancia pasa a `t3.small`
(2 GiB; con 1 GiB, 600 MiB del servidor más 512 MiB del sandbox no caben). En la consola: Stop → Actions →
Instance settings → Change instance type → `t3.small` → Start. La Elastic IP y el certificado se conservan.

```powershell
powershell -NoProfile -File infra/desplegar.ps1 -Ip 54.157.96.49 -Llave $HOME\.ssh\validador-ec2.pem -Sandbox
```

Con `-Sandbox`, el script:

- construye `validador-sandbox:1` en la EC2 (`docker build sandbox`; la base de Rust se descarga allí);
- toma el GID del grupo `docker` de la EC2 (`DOCKER_GID`);
- levanta los servicios con `infra/docker-compose.sandbox.yml` encima del Compose base. Ese archivo monta
  `/var/run/docker.sock` en `server`, le añade el grupo, pone `SANDBOX=docker` y un tmpfs en `/tmp` para la
  configuración del cliente.

La imagen del servidor lleva el cliente estático de Docker 27.5.1 (`COPY --from=docker:27.5.1-cli`). Cada trabajo es
un contenedor hermano con las restricciones de `sandbox.md`: sin red, usuario 10001, solo lectura, 512 MiB,
un núcleo, 64 procesos, entrada por tar y sin montajes. El semáforo de un trabajo a la vez es el de D83.

**Desplegado el 22 nov en la `t3.small`:** `/api/health` da `sandbox: true`. `/api/validate` con C y Rust, para un
válido y un inválido con 3 violaciones, da `status: ok` y `agree: true`, con el sandbox en 1,7-2,0 s. Los 86 casos
de Atlas siguen pasando, y en la EC2 no quedan contenedores del sandbox.

Antes se probó en local, con el servidor en su contenedor como en la EC2. `/api/health` dio `sandbox: true`. Para
un objeto válido y otro con 3 violaciones, C (4-6 s) y Rust (2-6 s) dieron `status: ok` y `agree: true`, y no
quedaron contenedores sueltos.

Para bajar los servicios con el sandbox puesto basta el Compose base (`docker compose -f infra/docker-compose.yml
down`). Con los dos archivos, Compose exige `DOCKER_GID`.

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
- aislar el socket de Docker. Con `-Sandbox` (D85, §2.1), `server` tiene `docker.sock`, y eso equivale a root en
  la EC2 (`sandbox.md` §5). **Riesgo aceptado** para la demo: el servidor solo ejecuta `docker run` con argumentos
  fijos (`server::sandbox::argumentos`; el usuario solo elige `c` o `rust`, los nombres de archivo los valida
  `comprobar`), y la EC2 no guarda nada más que el `.env`. Lo correcto a largo plazo sería un proxy del socket que
  solo permita crear contenedores con esa imagen, o rootless Docker. Sin `-Sandbox`, el socket no se monta y
  `/api/validate` responde 503 `no-sandbox`.

## 4. Costo

El Free plan de AWS cubre la `t3.micro` y una Elastic IP **asociada**. Una Elastic IP sin asociar se cobra:
suéltala si borras la instancia. Los budgets `cero-gasto` y `mensual-5usd` avisan de cualquier gasto.
