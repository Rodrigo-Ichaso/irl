// irl-server/src/main.rs
// IRL — Intent Record Language — HTTP server
// Axum + Tokio + SQLite audit log + trust registry + webhook human gate
//
// Copyright (c) 2026 Rodrigo Ichaso <https://linkedin.com/in/ichasorodrigo>

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{delete, get, post},
    Router,
};
use chrono::Utc;
use irl_core::{evaluate, Decision, EvaluationResult, IntentRecord, TrustLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, sync::Arc};
use tokio::sync::Mutex;
use tokio_rusqlite::Connection;
use tracing::{error, info, warn};
use tokio_rusqlite::OptionalExtension;
use tower_http::cors::CorsLayer;

// ── STATE ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    db:               Arc<Mutex<Connection>>,
    gate_webhook_url: Option<String>,
    telegram_token:   Option<String>,
    telegram_chat_id: Option<String>,
    admin_key:        Option<String>,
}

// ── DB INIT ───────────────────────────────────────────────────────────────────

async fn init_db(conn: &Connection) {
    conn.call(|db| {
        db.execute_batch("
            CREATE TABLE IF NOT EXISTS audit_log (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                verdict_id     TEXT NOT NULL,
                agent_id       TEXT NOT NULL,
                operation      TEXT NOT NULL,
                environment    TEXT NOT NULL,
                risk_score     INTEGER NOT NULL,
                risk_level     TEXT NOT NULL,
                decision       TEXT NOT NULL,
                policy         TEXT NOT NULL,
                reason         TEXT NOT NULL,
                requires_human INTEGER NOT NULL,
                created_at     TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agents (
                agent_id      TEXT PRIMARY KEY,
                trust_level   TEXT NOT NULL DEFAULT 'low',
                note          TEXT,
                registered_at TEXT NOT NULL
            );
        ")?;
        Ok(())
    }).await.expect("Failed to initialize DB");
}

// ── TRUST REGISTRY ────────────────────────────────────────────────────────────

// Look up the agent's trust level from the registry.
// If the agent is not registered, returns Low — maximum scrutiny, no exceptions.
async fn registered_trust(conn: &Arc<Mutex<Connection>>, agent_id: &str) -> TrustLevel {
    let id = agent_id.to_string();
    let db = conn.lock().await;
    let result = db.call(move |db| {
        let mut stmt = db.prepare(
            "SELECT trust_level FROM agents WHERE agent_id = ?1"
        )?;
        let level = stmt.query_row([&id], |row| row.get::<_, String>(0))
            .optional()?;
        Ok(level)
    }).await.unwrap_or(None);

    match result.as_deref() {
        Some("verified") => TrustLevel::Verified,
        Some("high")     => TrustLevel::High,
        Some("medium")   => TrustLevel::Medium,
        _                => TrustLevel::Low,
    }
}

// ── ADMIN AUTH ────────────────────────────────────────────────────────────────

fn is_admin(headers: &HeaderMap, admin_key: &Option<String>) -> bool {
    match admin_key {
        None => false, // no key configured = admin endpoints disabled
        Some(key) => headers
            .get("x-admin-key")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == key)
            .unwrap_or(false),
    }
}

// ── AUDIT LOG ─────────────────────────────────────────────────────────────────

async fn log_evaluation(conn: &Arc<Mutex<Connection>>, result: &EvaluationResult) {
    let verdict_id  = result.verdict.verdict_id.to_string();
    let agent_id    = result.intent.agent.id.clone();
    let operation   = format!("{:?}", result.intent.operation.op_type);
    let environment = format!("{:?}", result.intent.operation.target_environment);
    let risk_score  = result.risk.score as i64;
    let risk_level  = format!("{:?}", result.risk.level);
    let decision    = result.verdict.decision.to_string();
    let policy      = result.verdict.policy.clone();
    let reason      = result.verdict.reason.clone();
    let req_human   = result.verdict.requires_human as i64;
    let created_at  = Utc::now().to_rfc3339();

    let db = conn.lock().await;
    let _ = db.call(move |db| {
        db.execute(
            "INSERT INTO audit_log
             (verdict_id,agent_id,operation,environment,risk_score,risk_level,
              decision,policy,reason,requires_human,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![
                verdict_id, agent_id, operation, environment,
                risk_score, risk_level, decision, policy,
                reason, req_human, created_at
            ],
        )?;
        Ok(())
    }).await;
}

// ── GATE NOTIFICATION ─────────────────────────────────────────────────────────

async fn send_gate_notification(state: &AppState, result: &EvaluationResult) {
    let ir   = &result.intent;
    let risk = &result.risk;

    if let Some(url) = &state.gate_webhook_url {
        let payload = json!({
            "event":      "irl.gate",
            "verdict_id": result.verdict.verdict_id,
            "agent": {
                "id":          ir.agent.id,
                "trust_level": format!("{:?}", ir.agent.trust_level),
            },
            "action": {
                "type":        format!("{:?}", ir.operation.op_type),
                "resource":    ir.operation.target_resource,
                "environment": format!("{:?}", ir.operation.target_environment),
            },
            "risk": {
                "score":   risk.score,
                "level":   risk.level.to_string(),
                "reasons": risk.reasons,
            },
            "goal":      ir.rationale.stated_goal,
            "policy":    result.verdict.policy,
            "reason":    result.verdict.reason,
            "timestamp": Utc::now().to_rfc3339(),
        });

        match reqwest::Client::new().post(url).json(&payload).send().await {
            Ok(r) if r.status().is_success() =>
                info!("Gate webhook sent for {}", result.verdict.verdict_id),
            Ok(r) =>
                error!("Gate webhook error: {}", r.status()),
            Err(e) =>
                error!("Gate webhook failed: {}", e),
        }
        return;
    }

    if let (Some(token), Some(chat_id)) = (&state.telegram_token, &state.telegram_chat_id) {
        let msg = format!(
            "🔴 *IRL — HUMAN GATE*\n\n\
            Agent: `{}`\n\
            Action: `{:?}` → `{}`\n\
            Env: *{:?}*\n\
            Risk: *{} ({}/100)*\n\
            Reasons: `{}`\n\
            Goal: _{}_\n\n\
            Verdict ID: `{}`\n\n\
            _Silence = auto-DENY in 5 minutes_",
            ir.agent.id,
            ir.operation.op_type,
            ir.operation.target_resource,
            ir.operation.target_environment,
            risk.level,
            risk.score,
            risk.reasons.join(", "),
            ir.rationale.stated_goal,
            result.verdict.verdict_id,
        );

        let url  = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let body = json!({ "chat_id": chat_id, "text": msg, "parse_mode": "Markdown" });

        match reqwest::Client::new().post(&url).json(&body).send().await {
            Ok(r) if r.status().is_success() =>
                info!("Telegram gate sent for {}", result.verdict.verdict_id),
            Ok(r) =>
                error!("Telegram error: {}", r.status()),
            Err(e) =>
                error!("Telegram send failed: {}", e),
        }
        return;
    }

    warn!("GATE triggered but no notifier configured — set GATE_WEBHOOK_URL or TELEGRAM_TOKEN+TELEGRAM_CHAT_ID");
}

// ── HANDLERS ──────────────────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({
        "status":  "ok",
        "service": "IRL — Intent Record Language",
        "version": "0.1.0",
        "spec":    "https://github.com/Rodrigo-Ichaso/irl"
    }))
}

async fn evaluate_handler(
    State(state): State<AppState>,
    Json(mut ir): Json<IntentRecord>,
) -> (StatusCode, Json<Value>) {

    // Override self-declared trust level with the registry value.
    // Unregistered agents always get Low — the agent cannot elevate itself.
    let registry_trust = registered_trust(&state.db, &ir.agent.id).await;
    if ir.agent.trust_level != registry_trust {
        info!(
            "Trust override for {}: {:?} → {:?} (registry)",
            ir.agent.id, ir.agent.trust_level, registry_trust
        );
    }
    ir.agent.trust_level = registry_trust;

    info!("Evaluating intent from agent: {} ({:?})", ir.agent.id, ir.operation.op_type);

    let result = evaluate(ir);
    log_evaluation(&state.db, &result).await;

    if result.verdict.requires_human {
        warn!("GATE: {} — {}/100", result.intent.agent.id, result.risk.score);
        send_gate_notification(&state, &result).await;
    }

    let decision_str = result.verdict.decision.to_string();
    let risk_level   = result.risk.level.to_string();

    info!("Verdict: {} ({}/100)", decision_str, result.risk.score);

    let status = match result.verdict.decision {
        Decision::Allow | Decision::LogAllow => StatusCode::OK,
        Decision::Gate                       => StatusCode::ACCEPTED,
        Decision::Deny                       => StatusCode::FORBIDDEN,
    };

    (status, Json(json!({
        "verdict_id":     result.verdict.verdict_id,
        "decision":       decision_str,
        "risk": {
            "score":   result.risk.score,
            "level":   risk_level,
            "reasons": result.risk.reasons,
        },
        "policy":         result.verdict.policy,
        "reason":         result.verdict.reason,
        "requires_human": result.verdict.requires_human,
        "evaluated_at":   result.verdict.evaluated_at,
    })))
}

async fn get_audit_log(State(state): State<AppState>) -> Json<Value> {
    let db = state.db.lock().await;
    let rows = db.call(|db| {
        let mut stmt = db.prepare(
            "SELECT verdict_id,agent_id,operation,environment,
                    risk_score,risk_level,decision,policy,reason,created_at
             FROM audit_log ORDER BY id DESC LIMIT 50"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(json!({
                "verdict_id":  row.get::<_,String>(0)?,
                "agent_id":    row.get::<_,String>(1)?,
                "operation":   row.get::<_,String>(2)?,
                "environment": row.get::<_,String>(3)?,
                "risk_score":  row.get::<_,i64>(4)?,
                "risk_level":  row.get::<_,String>(5)?,
                "decision":    row.get::<_,String>(6)?,
                "policy":      row.get::<_,String>(7)?,
                "reason":      row.get::<_,String>(8)?,
                "created_at":  row.get::<_,String>(9)?,
            }))
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }).await.unwrap_or_default();

    Json(json!({ "count": rows.len(), "entries": rows }))
}

// ── AGENT REGISTRY ENDPOINTS ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct RegisterAgent {
    agent_id:    String,
    trust_level: String,
    note:        Option<String>,
}

#[derive(Serialize)]
struct AgentEntry {
    agent_id:      String,
    trust_level:   String,
    note:          Option<String>,
    registered_at: String,
}

async fn register_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterAgent>,
) -> (StatusCode, Json<Value>) {
    if !is_admin(&headers, &state.admin_key) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "x-admin-key required"})));
    }

    let valid_levels = ["low", "medium", "high", "verified"];
    if !valid_levels.contains(&body.trust_level.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(json!({
            "error": "trust_level must be: low | medium | high | verified"
        })));
    }

    let agent_id    = body.agent_id.clone();
    let trust_level = body.trust_level.clone();
    let note        = body.note.clone();
    let now         = Utc::now().to_rfc3339();

    let db = state.db.lock().await;
    let result = db.call(move |db| {
        db.execute(
            "INSERT INTO agents (agent_id, trust_level, note, registered_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(agent_id) DO UPDATE SET trust_level=excluded.trust_level, note=excluded.note",
            rusqlite::params![agent_id, trust_level, note, now],
        )?;
        Ok(())
    }).await;

    match result {
        Ok(_) => {
            info!("Agent registered: {} → {}", body.agent_id, body.trust_level);
            (StatusCode::OK, Json(json!({
                "agent_id":    body.agent_id,
                "trust_level": body.trust_level,
                "registered":  true
            })))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))),
    }
}

async fn list_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    if !is_admin(&headers, &state.admin_key) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "x-admin-key required"})));
    }

    let db = state.db.lock().await;
    let agents = db.call(|db| {
        let mut stmt = db.prepare(
            "SELECT agent_id, trust_level, note, registered_at FROM agents ORDER BY registered_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AgentEntry {
                agent_id:      row.get(0)?,
                trust_level:   row.get(1)?,
                note:          row.get(2)?,
                registered_at: row.get(3)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }).await.unwrap_or_default();

    (StatusCode::OK, Json(json!({ "count": agents.len(), "agents": agents })))
}

async fn remove_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if !is_admin(&headers, &state.admin_key) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "x-admin-key required"})));
    }

    let id = agent_id.clone();
    let db = state.db.lock().await;
    let _ = db.call(move |db| {
        db.execute("DELETE FROM agents WHERE agent_id = ?1", [&id])?;
        Ok(())
    }).await;

    info!("Agent removed from registry: {}", agent_id);
    (StatusCode::OK, Json(json!({ "agent_id": agent_id, "removed": true })))
}

// ── MAIN ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("irl_server=debug,info")
        .init();

    let db_path = env::var("IRL_DB_PATH").unwrap_or("irl_audit.db".into());
    let conn    = Connection::open(&db_path).await.expect("Cannot open SQLite DB");
    init_db(&conn).await;

    let admin_key = env::var("IRL_ADMIN_KEY").ok();
    if admin_key.is_none() {
        warn!("IRL_ADMIN_KEY not set — agent registry endpoints are disabled");
    }

    let state = AppState {
        db:               Arc::new(Mutex::new(conn)),
        gate_webhook_url: env::var("GATE_WEBHOOK_URL").ok(),
        telegram_token:   env::var("TELEGRAM_TOKEN").ok(),
        telegram_chat_id: env::var("TELEGRAM_CHAT_ID").ok(),
        admin_key,
    };

    let app = Router::new()
        .route("/health",        get(health))
        .route("/evaluate",      post(evaluate_handler))
        .route("/audit",         get(get_audit_log))
        .route("/agents",        post(register_agent))
        .route("/agents",        get(list_agents))
        .route("/agents/:id",    delete(remove_agent))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port = env::var("IRL_PORT").unwrap_or("8800".into());
    let addr = format!("0.0.0.0:{}", port);

    info!("IRL listening on {}", addr);
    info!("POST /evaluate       — evaluate an intent record");
    info!("GET  /audit          — recent audit log (last 50)");
    info!("GET  /health         — health check");
    info!("POST /agents         — register agent trust level  [admin]");
    info!("GET  /agents         — list registered agents      [admin]");
    info!("DELETE /agents/:id   — remove agent from registry  [admin]");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
