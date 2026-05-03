# IRL — Intent Record Language

**The trust layer between AI agents and your infrastructure.**

> *"MCP solved transport. IRL makes agents accountable."*

**→ [Live Demo](https://rodrigo-ichaso.github.io/irl/demo.html) · [Spec](https://rodrigo-ichaso.github.io/irl/spec.html) · [GitHub](https://github.com/Rodrigo-Ichaso/irl)**

---

## The Problem

On April 24, 2026, an AI coding agent (Cursor + Claude Opus 4.6) deleted an entire production database in **9 seconds**. It then confessed: *"I guessed instead of verifying."*

The root cause wasn't bad AI reasoning. It was that **infrastructure let bad reasoning execute.**

Current stack:

```
AGENT ──────────────────────────────→ INFRASTRUCTURE
(probabilistic reasoning)              (deterministic execution)
```

There is nothing in between. Prompts are not contracts. Comments are not enforcement. MCP is transport, not trust.

---

## The Solution

```
AGENT → [INTENT RECORD] → [IRL FIREWALL] → INFRASTRUCTURE
```

Before any action executes, the agent must submit a structured **Intent Record** declaring:
- **WHO** is acting (agent id, trust level)
- **WHAT** they want to do (operation type, target, environment)
- **WHY** they think it's safe (rationale, verification status)
- **WHAT HAPPENS** if it goes wrong (reversibility, rollback plan)

The firewall evaluates this deterministically — no LLM, no probabilities — and returns one of four verdicts:

| Verdict | Meaning |
|---------|---------|
| `ALLOW` | Auto-approved, low risk |
| `LOG+ALLOW` | Approved with monitoring |
| `GATE` | Paused — human approval required (webhook / Telegram) |
| `DENY` | Blocked — critical risk auto-denied |

---

## Build Artifacts

The `target/` directory (Rust build cache, ~1.7GB) is excluded from git and can be safely deleted to free space. Regenerate with:

```bash
cargo build          # debug build
cargo build --release  # production binary
cargo test           # run tests (also builds)
```

---

## Quick Start

```bash
# Clone
git clone https://github.com/Rodrigo-Ichaso/irl
cd irl

# Run tests
cargo test

# Start server (port 8800)
cargo run --bin irl-server

# Test with Railway incident scenario
curl -X POST http://localhost:8800/evaluate \
  -H "Content-Type: application/json" \
  -d @examples/railway-incident.json
# → 403 FORBIDDEN — DENY (risk: CRITICAL 100/100)

# Test with safe read
curl -X POST http://localhost:8800/evaluate \
  -H "Content-Type: application/json" \
  -d @examples/safe-read.json
# → 200 OK — ALLOW (risk: LOW 0/100)

# View audit log
curl http://localhost:8800/audit
```

---

## MCP Integration

Connect any MCP-compatible agent (Claude Code, Cursor, n8n) so it evaluates intent through IRL before acting.

```bash
# Build the MCP server
cd irl-mcp && npm install && npm run build
```

Add to your agent's MCP config (`.mcp.json` for Claude Code):

```json
{
  "mcpServers": {
    "irl": {
      "command": "node",
      "args": ["/path/to/irl/irl-mcp/dist/index.js"],
      "env": {
        "IRL_URL": "http://your-server:8800",
        "IRL_AGENT_ID": "my-agent-01"
      }
    }
  }
}
```

The agent now has access to `evaluate_intent`. Add this to your system prompt or `CLAUDE.md`:

```
Before any risky action (delete, production write, external network call, auth change):
call evaluate_intent and wait for the verdict.
DENY → stop. GATE → wait for human. ALLOW → proceed.
```

The agent fills in what it wants to do. IRL generates the risk score deterministically — the agent cannot influence the verdict.

---

## Trust Registry

Agents cannot self-declare their trust level. The server maintains a registry — unregistered agents always get `low` trust regardless of what they claim.

```bash
# Register an agent (requires IRL_ADMIN_KEY)
curl -X POST http://localhost:8800/agents \
  -H "x-admin-key: your-admin-key" \
  -H "Content-Type: application/json" \
  -d '{"agent_id": "cursor-agent-01", "trust_level": "medium", "note": "dev workstation"}'

# List registered agents
curl http://localhost:8800/agents \
  -H "x-admin-key: your-admin-key"

# Remove an agent (e.g. on offboarding)
curl -X DELETE http://localhost:8800/agents/cursor-agent-01 \
  -H "x-admin-key: your-admin-key"
```

An agent that submits `"trust_level": "verified"` in the intent record gets overridden to `low` if it's not in the registry. The trust override is logged.

---

## Resource Registry

Map known resources to their authoritative environment. If an agent claims `"volume:prod-db-main"` is `staging`, IRL overrides it to `production` — the agent cannot spoof environments.

```bash
# Register a resource
curl -X POST http://localhost:8800/resources \
  -H "x-admin-key: your-admin-key" \
  -H "Content-Type: application/json" \
  -d '{"resource_id": "volume:prod-db-main", "environment": "production", "note": "main prod DB"}'

# List registered resources
curl http://localhost:8800/resources \
  -H "x-admin-key: your-admin-key"
```

Unknown resources are not blocked — the agent's declared environment is used. Only register resources where environment spoofing would be critical (production DBs, backup volumes, billing services).

```bash
# Start server with admin key
IRL_ADMIN_KEY=your-secret-key cargo run --bin irl-server
```

---

## Human Gate

When a HIGH risk operation is submitted, IRL pauses and notifies your team.

**Option A — Generic webhook** (Slack, Discord, n8n, PagerDuty, anything):

```bash
GATE_WEBHOOK_URL=https://your-endpoint/hook \
cargo run --bin irl-server
```

IRL POSTs this payload to your URL:

```json
{
  "event": "irl.gate",
  "verdict_id": "abc-123",
  "agent": { "id": "cursor-agent-01", "trust_level": "Medium" },
  "action": { "type": "Delete", "resource": "volume:prod-db-main", "environment": "Production" },
  "risk": { "score": 68, "level": "HIGH", "reasons": ["production environment", "irreversible operation"] },
  "goal": "Fix credential mismatch",
  "timestamp": "2026-04-24T09:00:00Z"
}
```

**Option B — Telegram fallback**:

```bash
TELEGRAM_TOKEN=your_bot_token \
TELEGRAM_CHAT_ID=your_chat_id \
cargo run --bin irl-server
```

---

## Intent Record Schema (v0.1)

```json
{
  "irl_version": "0.1",
  "agent": {
    "id": "my-agent",
    "trust_level": "medium"
  },
  "operation": {
    "type": "delete",
    "target_resource": "volume:prod-db-main",
    "target_environment": "production"
  },
  "rationale": {
    "stated_goal": "Fix credential mismatch",
    "verified": false,
    "alternatives_considered": []
  },
  "consequences": {
    "reversible": false,
    "data_loss_risk": "total",
    "affects_backups": true,
    "rollback_plan": false
  }
}
```

---

## Risk Engine

Deterministic scoring — not probabilistic. Every point is a policy decision.

| Factor | Points |
|--------|--------|
| Operation: read | +0 |
| Operation: write | +20 |
| Operation: delete | +50 |
| Environment: production | +30 |
| Irreversible | +25 |
| Assumption not verified | +20 |
| No alternatives considered | +15 |
| Affects backups | +30 |
| Delete without rollback | +20 |
| Total data loss risk | +25 |
| Trust: verified agent | -20 |
| Trust: high agent | -10 |

| Score | Level | Verdict |
|-------|-------|---------|
| 0–24 | LOW | ALLOW |
| 25–49 | MEDIUM | LOG+ALLOW |
| 50–74 | HIGH | GATE |
| 75–100 | CRITICAL | DENY |

**Override:** Production deletes are always minimum GATE regardless of score.

---

## Deploy on Proxmox

```bash
# Build release binary — single file, no runtime deps
cargo build --release

# Copy to VM
scp target/release/irl-server user@proxmox-vm:/usr/local/bin/

# Run as systemd service
cat > /etc/systemd/system/irl-server.service << EOF
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

[Install]
WantedBy=multi-user.target
EOF

systemctl enable --now irl-server
```

---

## Architecture

```
┌─────────────────────────────────────────┐
│              IRL               │
│                                         │
│  POST /evaluate                         │
│       ↓                                 │
│  [Risk Engine]   ← deterministic        │
│       ↓                                 │
│  [Policy Engine] ← configurable rules   │
│       ↓                                 │
│  ALLOW / LOG / GATE / DENY              │
│       ↓              ↓                  │
│  [Audit Log]    [Telegram Gate]         │
│  (SQLite)       (human approval)        │
└─────────────────────────────────────────┘
```

---

## Roadmap

- [x] v0.1 — Core types, risk engine, policy engine, HTTP server, audit log, Telegram gate
- [x] v0.2 — MCP server wrapper + trust registry (agents cannot self-declare trust level)
- [ ] v0.3 — Policy config file (define rules without recompiling)
- [ ] v0.4 — Rollback snapshots API
- [ ] v1.0 — Agent identity (DID) + signed intent records

---

## Why Rust?

A policy enforcement layer that pauses for GC in a critical moment is unacceptable. Rust gives us:

- **Memory safety** — guaranteed by the compiler, not by discipline
- **No GC pauses** — predictable latency always
- **Single binary** — copy to Proxmox VM and run, no runtime
- **Fearless concurrency** — safe multi-agent handling by construction

---

## License

`irl-core` and the IRL spec — Apache 2.0

Copyright (c) 2026 Rodrigo Ichaso. Attribution required on all distributions and derivative works.

`irl-server` (IRL Shield) — BSL 1.1 (free for personal use, commercial license required for production business use)

---

**Built by [Rodrigo Ichaso](https://linkedin.com/in/ichasorodrigo) — La Paz, Bolivia**

*Incident reference: PocketOS/Railway, April 24, 2026*
