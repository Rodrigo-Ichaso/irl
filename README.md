# IRL — Intent Record Language

**The trust layer between AI agents and your infrastructure.**

> *"MCP solved transport. IRL solves trust."*

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
- [ ] v0.2 — MCP server wrapper (any MCP-compatible agent goes through IRL automatically)
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
