// irl-server/src/main.rs
// IRL Firewall — HTTP server
// Axum + Tokio + SQLite audit log + Telegram human gate
//
// cargo run --bin irl-firewall
// TELEGRAM_TOKEN=xxx TELEGRAM_CHAT_ID=yyy cargo run

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use irl_core::{evaluate, Decision, EvaluationResult, IntentRecord};
use serde_json::{json, Value};
use std::{env, sync::Arc};
use tokio::sync::Mutex;
use tokio_rusqlite::Connection;
use tracing::{error, info, warn};
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    telegram_token: Option<String>,
    telegram_chat_id: Option<String>,
}

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
        ")?;
        Ok(())
    }).await.expect("Failed to initialize DB");
}

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

async fn send_telegram_gate(state: &AppState, result: &EvaluationResult) {
    let (Some(token), Some(chat_id)) = (&state.telegram_token, &state.telegram_chat_id) else {
        warn!("Telegram not configured — set TELEGRAM_TOKEN and TELEGRAM_CHAT_ID");
        return;
    };

    let ir   = &result.intent;
    let risk = &result.risk;

    let msg = format!(
        "🔴 *IRL FIREWALL — HUMAN GATE*\n\n\
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
}

async fn health() -> Json<Value> {
    Json(json!({
        "status":  "ok",
        "service": "IRL Firewall",
        "version": "0.1.0",
        "spec":    "https://github.com/alciom-cognitive/irl-firewall"
    }))
}

async fn evaluate_handler(
    State(state): State<AppState>,
    Json(ir): Json<IntentRecord>,
) -> (StatusCode, Json<Value>) {

    info!("Evaluating intent from agent: {} ({:?})", ir.agent.id, ir.operation.op_type);

    let result = evaluate(ir);
    log_evaluation(&state.db, &result).await;

    if result.verdict.requires_human {
        warn!("GATE: {} — {}/100", result.intent.agent.id, result.risk.score);
        send_telegram_gate(&state, &result).await;
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("irl_server=debug,info")
        .init();

    let db_path = env::var("IRL_DB_PATH").unwrap_or("irl_audit.db".into());
    let conn    = Connection::open(&db_path).await.expect("Cannot open SQLite DB");
    init_db(&conn).await;

    let state = AppState {
        db:               Arc::new(Mutex::new(conn)),
        telegram_token:   env::var("TELEGRAM_TOKEN").ok(),
        telegram_chat_id: env::var("TELEGRAM_CHAT_ID").ok(),
    };

    let app = Router::new()
        .route("/health",   get(health))
        .route("/evaluate", post(evaluate_handler))
        .route("/audit",    get(get_audit_log))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port = env::var("IRL_PORT").unwrap_or("8800".into());
    let addr = format!("0.0.0.0:{}", port);

    info!("IRL Firewall listening on {}", addr);
    info!("POST /evaluate  — evaluate an intent record");
    info!("GET  /audit     — recent audit log (last 50)");
    info!("GET  /health    — health check");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
