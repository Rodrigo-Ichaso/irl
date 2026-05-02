# IRL — Guía de arranque

## Estructura del proyecto

```
irl-firewall/
├── irl-core/           ← lógica central en Rust (tipos, risk engine, policy)
│   └── src/lib.rs
├── irl-server/         ← servidor HTTP (Axum + SQLite + Telegram)
│   └── src/main.rs
├── examples/
│   ├── railway-incident.json   ← caso real: DEBE retornar DENY
│   └── safe-read.json          ← caso seguro: DEBE retornar ALLOW
├── demo.html           ← demo visual sin/con IRL (abrir en browser)
├── spec.html           ← spec completa del protocolo
├── Cargo.toml          ← workspace
└── README.md           ← para GitHub (en inglés)
```

---

## Paso 1 — Instalar Rust (una sola vez)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
rustc --version   # debe mostrar 1.75+
```

---

## Paso 2 — Correr los tests del core

```bash
cd irl
cargo test -p irl-core
```

Resultado esperado:
```
test tests::test_railway_incident_is_denied ... ok
test tests::test_safe_read_is_allowed ... ok
test tests::test_production_delete_always_gates ... ok

test result: ok. 3 passed; 0 failed
```

---

## Paso 3 — Levantar el servidor

```bash
# Sin Telegram (solo pruebas locales)
cargo run --bin irl-server

# Con Telegram gate (para demo real)
TELEGRAM_TOKEN=tu_token TELEGRAM_CHAT_ID=tu_chat_id cargo run --bin irl-server
```

El server queda en: `http://localhost:8800`

---

## Paso 4 — Probar los casos

**Caso Railway — debe retornar HTTP 403 DENY:**
```bash
curl -s -X POST http://localhost:8800/evaluate \
  -H "Content-Type: application/json" \
  -d @examples/railway-incident.json | python3 -m json.tool
```

**Caso seguro — debe retornar HTTP 200 ALLOW:**
```bash
curl -s -X POST http://localhost:8800/evaluate \
  -H "Content-Type: application/json" \
  -d @examples/safe-read.json | python3 -m json.tool
```

**Ver audit log:**
```bash
curl -s http://localhost:8800/audit | python3 -m json.tool
```

---

## Paso 5 — Subir a GitHub

```bash
git init
git add .
git commit -m "feat: IRL v0.1 — intent record language for AI agent safety"

# Crear repo en github.com primero, luego:
git remote add origin https://github.com/TU_USUARIO/irl
git push -u origin main
```

---

## Deploy en Proxmox (cuando estés listo)

**Opción A — Binario directo en VM Debian/Ubuntu:**
```bash
# En tu máquina local, compilar release
cargo build --release
# Binario queda en: target/release/irl-server (sin extensión, ~5MB)

# Copiar a la VM
scp target/release/irl-server usuario@IP-VM:/usr/local/bin/

# En la VM, crear servicio systemd
sudo nano /etc/systemd/system/irl-server.service
```

Contenido del servicio:
```ini
[Unit]
Description=IRL
After=network.target

[Service]
ExecStart=/usr/local/bin/irl-server
Environment=IRL_PORT=8800
Environment=IRL_DB_PATH=/var/lib/irl/audit.db
Environment=TELEGRAM_TOKEN=xxx
Environment=TELEGRAM_CHAT_ID=yyy
Restart=always
User=nobody

[Install]
WantedBy=multi-user.target
```

```bash
sudo mkdir -p /var/lib/irl
sudo systemctl enable --now irl-server
sudo systemctl status irl-server
```

**Opción B — Docker en Proxmox:**
```bash
# En el proyecto, crear Dockerfile (Claude te lo genera cuando llegues aquí)
docker build -t irl-server .
docker run -d -p 8800:8800 \
  -e TELEGRAM_TOKEN=xxx \
  -e TELEGRAM_CHAT_ID=yyy \
  -v /data/irl:/var/lib/irl \
  irl-server
```

---

## Variables de entorno

| Variable | Default | Descripción |
|----------|---------|-------------|
| `IRL_PORT` | `8800` | Puerto del servidor |
| `IRL_DB_PATH` | `irl_audit.db` | Ruta del audit log SQLite |
| `TELEGRAM_TOKEN` | — | Token del bot de Telegram |
| `TELEGRAM_CHAT_ID` | — | Chat ID donde llegan las alertas |

---

## Endpoints

| Método | Ruta | Descripción |
|--------|------|-------------|
| GET | `/health` | Health check |
| POST | `/evaluate` | Evaluar un intent record |
| GET | `/audit` | Últimas 50 entradas del audit log |

---

## Respuestas HTTP

| Decisión | HTTP | Significado |
|----------|------|-------------|
| ALLOW | 200 OK | Auto-aprobado |
| LOG+ALLOW | 200 OK | Aprobado con log |
| GATE | 202 Accepted | Pendiente aprobación humana |
| DENY | 403 Forbidden | Bloqueado automáticamente |

---

Cualquier duda: Claude tiene todo el contexto del proyecto.
