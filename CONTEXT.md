# IRL Firewall — Contexto completo para Claude en code-server

> Este archivo es para Claude. Rodrigo lo abrirá en su code-server
> y continuará desde aquí. Leer completo antes de responder.

---

## Quién es Rodrigo

IT Systems Manager y AI Automation Specialist. 15+ años de experiencia.
Sale de AmCham Bolivia en mayo 2026 para consultoría independiente full-time
bajo la marca **Alciom Cognitive**. Upwork como canal principal, $55/hr actual,
objetivo $200-500/hr con los proyectos correctos.

Infraestructura propia:
- HP ProDesk (Debian 12, Docker, Tailscale, **code-server**) — servidor 24/7
- Proxmox con VMs (inventario exacto pendiente de confirmar)
- MikroTik para red
- Tailscale para acceso seguro

Paga en USDC vía Takenos. La Paz, Bolivia → clientes americanos.

---

## Qué es este proyecto

**IRL Firewall** — Intent Record Language.

Un protocolo y servidor de seguridad para agentes de IA. La capa que falta
entre un agente de IA y la infraestructura que puede destruir.

Motivado por el incidente real del 24 de abril de 2026:
- Cursor (Claude Opus 4.6) borró la DB de producción de PocketOS en 9 segundos
- El agente encontró un token sin scope en disco
- Llamó un endpoint legacy de Railway sin confirmación
- Borró la DB + todos los backups en cascada
- El agente confesó: "I guessed instead of verifying"

**La solución:**
```
ANTES:  AGENT ──────────────────────→ INFRASTRUCTURE
AHORA:  AGENT → [INTENT RECORD] → [IRL FIREWALL] → INFRASTRUCTURE
```

Antes de ejecutar cualquier acción, el agente debe declarar:
- WHO: quién actúa (id, trust level)
- WHAT: qué quiere hacer (operación, recurso, ambiente)
- WHY: por qué cree que es seguro (rationale, verificación)
- CONSEQUENCES: qué pasa si sale mal (reversible, rollback, backups)

El firewall evalúa determinísticamente y retorna:
- **ALLOW** (200) — riesgo bajo, auto-aprobado
- **LOG+ALLOW** (200) — riesgo medio, logueado
- **GATE** (202) — riesgo alto, pausa para aprobación humana vía Telegram
- **DENY** (403) — riesgo crítico, bloqueado automáticamente

---

## Estructura del proyecto

```
irl-firewall/
├── Cargo.toml              ← workspace Rust (2 crates)
├── irl-core/
│   ├── Cargo.toml
│   └── src/lib.rs          ← tipos, risk engine, policy engine, tests
├── irl-server/
│   ├── Cargo.toml
│   └── src/main.rs         ← Axum HTTP server, SQLite audit, Telegram gate
├── examples/
│   ├── railway-incident.json  ← DEBE retornar DENY (100/100 CRITICAL)
│   └── safe-read.json         ← DEBE retornar ALLOW (0/100 LOW)
├── demo.html               ← demo visual lado a lado sin/con IRL
├── spec.html               ← spec completa del protocolo
├── LEEME.md                ← guía paso a paso en español
└── README.md               ← para GitHub en inglés
```

---

## Estado actual

- [x] `irl-core` compila y pasa 3 tests (verificado en ambiente Claude)
- [x] Risk engine determinístico implementado
- [x] Policy engine implementado
- [x] `irl-server` escrito (Axum + SQLite + Telegram)
- [x] Demo visual HTML funcional
- [x] Spec HTML completa
- [x] README para GitHub
- [ ] `irl-server` pendiente de compilar en el ambiente de Rodrigo
- [ ] Deploy en Proxmox pendiente
- [ ] GitHub repo pendiente de crear y pushear

---

## Lo que Claude en code-server debe hacer primero

1. Leer este archivo completo
2. Verificar que el proyecto compila:
   ```bash
   cargo test -p irl-core
   ```
3. Si hay errores de compilación en `irl-server`, ayudar a resolverlos
4. Preguntar a Rodrigo qué VM tiene disponible en Proxmox:
   - OS (Debian/Ubuntu/Alpine)
   - RAM disponible
   - Tiene Docker instalado?
   - IP o hostname dentro de Tailscale
5. Según la respuesta, generar el plan de deploy exacto

---

## Decisiones de arquitectura tomadas

**Lenguaje:** Rust
- Razón: firewall de seguridad no puede tener GC pauses
- Memory safety por compilador, no por disciplina
- Binario único sin dependencias — fácil deploy en Proxmox
- Plan: MVP en Rust, SDK en TypeScript después

**Licencia planeada:**
- `irl-core` → Apache 2.0 (máxima adopción del protocolo)
- `irl-server` → BSL 1.1 (gratis personal, comercial paga)

**Stack del server:**
- Axum (HTTP async)
- Tokio (runtime)
- Serde (JSON)
- SQLite via tokio-rusqlite (audit log)
- reqwest (Telegram)
- tower-http (CORS)

**Telegram gate:** cuando riesgo es HIGH, el firewall pausa y envía
mensaje al operador. Silencio por 5 minutos = auto-DENY.

---

## Risk scoring (determinístico)

| Factor | Puntos |
|--------|--------|
| Operación: delete | +50 |
| Ambiente: production | +30 |
| Irreversible | +25 |
| Assumption not verified | +20 |
| No alternatives considered | +15 |
| Affects backups | +30 |
| Delete sin rollback plan | +20 |
| Total data loss risk | +25 |
| Trust: verified agent | -20 |
| Trust: high agent | -10 |

Scores: LOW 0-24 / MEDIUM 25-49 / HIGH 50-74 / CRITICAL 75-100

Override: production delete es siempre mínimo GATE sin importar score.

---

## Próximos pasos en orden

1. **Compilar y testear localmente en code-server**
2. **Resolver dependencias** si `irl-server` tiene problemas
   (las versiones en `irl-core/Cargo.toml` están pinadas para Cargo 1.75)
3. **Evaluar Proxmox disponible** — preguntar a Rodrigo
4. **Elegir deployment:**
   - Opción A: binario directo en VM (systemd service)
   - Opción B: Docker container
   - Opción C: Docker Compose con reverse proxy
5. **Crear GitHub repo** — `alciom-cognitive/irl-firewall`
6. **Publicar demo** — GitHub Pages con `demo.html`
7. **Siguiente feature:** MCP server wrapper

---

## Variables de entorno del server

```bash
IRL_PORT=8800              # default 8800
IRL_DB_PATH=irl_audit.db   # default en directorio actual
TELEGRAM_TOKEN=xxx          # opcional, habilita gate
TELEGRAM_CHAT_ID=yyy        # opcional, habilita gate
```

---

## Comandos de prueba rápida

```bash
# Tests del core
cargo test -p irl-core

# Levantar server
cargo run --bin irl-firewall

# Caso Railway (en otra terminal) — debe retornar 403
curl -s -X POST http://localhost:8800/evaluate \
  -H "Content-Type: application/json" \
  -d @examples/railway-incident.json | python3 -m json.tool

# Caso seguro — debe retornar 200
curl -s -X POST http://localhost:8800/evaluate \
  -H "Content-Type: application/json" \
  -d @examples/safe-read.json | python3 -m json.tool

# Audit log
curl -s http://localhost:8800/audit | python3 -m json.tool
```

---

## Visión del producto (contexto estratégico)

IRL no es solo un proyecto de Rodrigo. Es un protocolo abierto.

- **irl-core**: spec + tipos, Apache 2.0, cualquiera lo implementa
- **Alciom Shield**: producto comercial encima del protocolo
  - $2,500 setup + $400/mes monitoreo
  - Target: empresas que usan agentes de IA en producción
  - Timing: incidente Railway tiene 6.8M vistas en X, mercado asustado HOY

El path: GitHub público → Hacker News → primeros clientes en Upwork →
crecer como "el estándar de seguridad para agentes de IA en infra propia".

MCP resolvió el transporte. IRL resuelve la confianza.

---

## Nota para Claude en code-server

Rodrigo habla español. Responde en español.
Es técnico, va directo. No expliques lo obvio.
Si algo no compila, muestra el fix directo sin preámbulo.
El objetivo de esta sesión: que el server levante y los tests pasen.
Después, deploy en Proxmox.
