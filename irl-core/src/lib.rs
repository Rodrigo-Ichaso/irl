// irl-core/src/lib.rs
// IRL — Intent Record Language
// Core types, risk engine, and policy evaluator.
//
// Copyright (c) 2026 Rodrigo Ichaso <https://linkedin.com/in/ichasorodrigo>
// Licensed under the Apache License, Version 2.0
//
// "MCP solved transport. IRL solves trust."
// https://github.com/Rodrigo-Ichaso/irl

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── ENUMS ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Low,
    Medium,
    High,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    Read,
    Write,
    Delete,
    Execute,
    Network,
    Auth,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Local,
    Staging,
    Production,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DataLossRisk {
    None,
    Partial,
    Total,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RiskLevel::Low      => write!(f, "LOW"),
            RiskLevel::Medium   => write!(f, "MEDIUM"),
            RiskLevel::High     => write!(f, "HIGH"),
            RiskLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Allow,
    LogAllow,
    Gate,
    Deny,
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Decision::Allow    => write!(f, "ALLOW"),
            Decision::LogAllow => write!(f, "LOG+ALLOW"),
            Decision::Gate     => write!(f, "GATE"),
            Decision::Deny     => write!(f, "DENY"),
        }
    }
}

// ── INTENT RECORD ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub model: Option<String>,
    pub trust_level: TrustLevel,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    #[serde(rename = "type")]
    pub op_type: OperationType,
    pub target_resource: String,
    pub target_environment: Environment,
    pub estimated_rows_affected: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rationale {
    pub stated_goal: String,
    pub verified: bool,
    #[serde(default)]
    pub alternatives_considered: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consequences {
    pub reversible: bool,
    #[serde(default)]
    pub data_loss_risk: DataLossRisk,
    #[serde(default)]
    pub affects_backups: bool,
    #[serde(default)]
    pub rollback_plan: bool,
    #[serde(default)]
    pub downstream_services: Vec<String>,
}

impl Default for DataLossRisk {
    fn default() -> Self { DataLossRisk::None }
}

/// The core IRL structure. An agent MUST submit this before any action executes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRecord {
    pub irl_version: String,
    pub agent: AgentInfo,
    pub operation: Operation,
    pub rationale: Rationale,
    pub consequences: Consequences,
}

// ── RISK ENGINE ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub score: u8,          // 0-100
    pub level: RiskLevel,
    pub reasons: Vec<String>,
}

/// Deterministic risk scoring. No LLM. No probabilities.
/// Every point added here is a policy decision, not a guess.
pub fn compute_risk(ir: &IntentRecord) -> RiskAssessment {
    let mut score: u32 = 0;
    let mut reasons: Vec<String> = Vec::new();

    // Operation type base score
    let (op_score, op_reason) = match ir.operation.op_type {
        OperationType::Read    => (0,  None),
        OperationType::Write   => (20, Some("write operation")),
        OperationType::Execute => (30, Some("execute operation")),
        OperationType::Network => (25, Some("network call to external service")),
        OperationType::Auth    => (35, Some("auth/credential operation")),
        OperationType::Delete  => (50, Some("delete operation")),
    };
    score += op_score;
    if let Some(r) = op_reason { reasons.push(r.into()); }

    // Environment multiplier
    if ir.operation.target_environment == Environment::Production {
        score += 30;
        reasons.push("production environment".into());
    }

    // Irreversibility
    if !ir.consequences.reversible {
        score += 25;
        reasons.push("irreversible operation".into());
    }

    // Unverified assumption — this is what killed PocketOS
    if !ir.rationale.verified {
        score += 20;
        reasons.push("assumption not verified".into());
    }

    // No alternatives considered
    if ir.rationale.alternatives_considered.is_empty() {
        score += 15;
        reasons.push("no alternatives considered".into());
    }

    // Backup impact — cascade risk
    if ir.consequences.affects_backups {
        score += 30;
        reasons.push("affects backup systems".into());
    }

    // Delete without rollback plan
    if ir.operation.op_type == OperationType::Delete && !ir.consequences.rollback_plan {
        score += 20;
        reasons.push("delete without rollback plan".into());
    }

    // Total data loss
    if ir.consequences.data_loss_risk == DataLossRisk::Total {
        score += 25;
        reasons.push("total data loss risk".into());
    }

    // Trust level discount
    let discount: u32 = match ir.agent.trust_level {
        TrustLevel::Verified => 20,
        TrustLevel::High     => 10,
        TrustLevel::Medium   => 0,
        TrustLevel::Low      => 0,  // no discount for low trust
    };
    score = score.saturating_sub(discount);

    let score = score.min(100) as u8;

    let level = match score {
        0..=24  => RiskLevel::Low,
        25..=49 => RiskLevel::Medium,
        50..=74 => RiskLevel::High,
        _       => RiskLevel::Critical,
    };

    RiskAssessment { score, level, reasons }
}

// ── POLICY ENGINE ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub decision: Decision,
    pub policy: String,
    pub reason: String,
    pub requires_human: bool,
    pub evaluated_at: DateTime<Utc>,
    pub verdict_id: Uuid,
}

/// Evaluate policy against risk assessment.
/// Returns a deterministic verdict. The agent cannot influence this.
pub fn evaluate_policy(ir: &IntentRecord, risk: &RiskAssessment) -> Verdict {
    let (decision, policy, reason) = match risk.level {
        RiskLevel::Critical => (
            Decision::Deny,
            "POL-003",
            format!("Critical risk auto-denied: {}", risk.reasons.join(", ")),
        ),
        RiskLevel::High => (
            Decision::Gate,
            "POL-002",
            format!("High risk requires human approval: {}", risk.reasons.join(", ")),
        ),
        RiskLevel::Medium => (
            Decision::LogAllow,
            "POL-001",
            "Medium risk: logged and allowed with monitoring".into(),
        ),
        RiskLevel::Low => (
            Decision::Allow,
            "POL-000",
            "Low risk: auto-allowed".into(),
        ),
    };

    // Override: production delete is ALWAYS at minimum GATE regardless of trust
    let (decision, policy, reason) = if ir.operation.op_type == OperationType::Delete
        && ir.operation.target_environment == Environment::Production
        && decision == Decision::Allow
    {
        (
            Decision::Gate,
            "POL-004",
            "Production delete always requires human gate regardless of risk score".into(),
        )
    } else {
        (decision, policy, reason)
    };

    let requires_human = matches!(decision, Decision::Gate);

    Verdict {
        decision,
        policy: policy.into(),
        reason,
        requires_human,
        evaluated_at: Utc::now(),
        verdict_id: Uuid::new_v4(),
    }
}

// ── FULL EVALUATION ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub intent: IntentRecord,
    pub risk: RiskAssessment,
    pub verdict: Verdict,
}

pub fn evaluate(ir: IntentRecord) -> EvaluationResult {
    let risk = compute_risk(&ir);
    let verdict = evaluate_policy(&ir, &risk);
    EvaluationResult { intent: ir, risk, verdict }
}

// ── WASM ENTRY POINT ─────────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

/// Entry point for the browser demo.
/// Input: IntentRecord JSON string.
/// Output: { decision, policy, reason, risk: { score, level, reasons } }
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn evaluate_json(input: &str) -> String {
    #[derive(serde::Serialize)]
    struct WasmResult<'a> {
        decision: String,
        policy: String,
        reason: String,
        risk: WasmRisk<'a>,
    }
    #[derive(serde::Serialize)]
    struct WasmRisk<'a> {
        score: u8,
        level: String,
        reasons: &'a [String],
    }

    match serde_json::from_str::<IntentRecord>(input) {
        Ok(ir) => {
            let risk = compute_risk(&ir);
            let verdict = evaluate_policy(&ir, &risk);
            let out = WasmResult {
                decision: verdict.decision.to_string(),
                policy: verdict.policy,
                reason: verdict.reason,
                risk: WasmRisk {
                    score: risk.score,
                    level: risk.level.to_string(),
                    reasons: &risk.reasons,
                },
            };
            serde_json::to_string(&out).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
        }
        Err(e) => format!("{{\"error\":\"Parse error: {e}\"}}"),
    }
}

// ── TESTS ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(trust: TrustLevel) -> AgentInfo {
        AgentInfo {
            id: "test-agent".into(),
            model: Some("claude-opus-4-6".into()),
            trust_level: trust,
            session_id: None,
        }
    }

    /// Reproduce the PocketOS/Railway incident.
    /// This MUST be CRITICAL + DENY.
    #[test]
    fn test_railway_incident_is_denied() {
        let ir = IntentRecord {
            irl_version: "0.1".into(),
            agent: make_agent(TrustLevel::Medium),
            operation: Operation {
                op_type: OperationType::Delete,
                target_resource: "volume:prod-db-main".into(),
                target_environment: Environment::Production,
                estimated_rows_affected: None,
            },
            rationale: Rationale {
                stated_goal: "Fix credential mismatch in staging".into(),
                verified: false,
                alternatives_considered: vec![],
            },
            consequences: Consequences {
                reversible: false,
                data_loss_risk: DataLossRisk::Total,
                affects_backups: true,
                rollback_plan: false,
                downstream_services: vec!["billing".into(), "api".into()],
            },
        };

        let result = evaluate(ir);
        assert_eq!(result.risk.level, RiskLevel::Critical);
        assert_eq!(result.verdict.decision, Decision::Deny);
        println!("✓ Railway incident correctly DENIED (score: {})", result.risk.score);
    }

    /// A simple read in staging should be auto-allowed.
    #[test]
    fn test_safe_read_is_allowed() {
        let ir = IntentRecord {
            irl_version: "0.1".into(),
            agent: make_agent(TrustLevel::High),
            operation: Operation {
                op_type: OperationType::Read,
                target_resource: "table:users".into(),
                target_environment: Environment::Staging,
                estimated_rows_affected: Some(10),
            },
            rationale: Rationale {
                stated_goal: "Check user count for test".into(),
                verified: true,
                alternatives_considered: vec!["query metrics endpoint".into()],
            },
            consequences: Consequences {
                reversible: true,
                data_loss_risk: DataLossRisk::None,
                affects_backups: false,
                rollback_plan: true,
                downstream_services: vec![],
            },
        };

        let result = evaluate(ir);
        assert_eq!(result.verdict.decision, Decision::Allow);
        println!("✓ Safe read correctly ALLOWED (score: {})", result.risk.score);
    }

    /// Production delete, even with high trust, must be GATE.
    #[test]
    fn test_production_delete_always_gates() {
        let ir = IntentRecord {
            irl_version: "0.1".into(),
            agent: make_agent(TrustLevel::Verified),
            operation: Operation {
                op_type: OperationType::Delete,
                target_resource: "record:user:42".into(),
                target_environment: Environment::Production,
                estimated_rows_affected: Some(1),
            },
            rationale: Rationale {
                stated_goal: "Remove test user from production".into(),
                verified: true,
                alternatives_considered: vec!["soft delete".into(), "flag inactive".into()],
            },
            consequences: Consequences {
                reversible: true,
                data_loss_risk: DataLossRisk::Partial,
                affects_backups: false,
                rollback_plan: true,
                downstream_services: vec![],
            },
        };

        let result = evaluate(ir);
        // Even verified agent + reversible — production delete must be gated
        assert!(matches!(
            result.verdict.decision,
            Decision::Gate | Decision::Deny
        ));
        println!("✓ Production delete correctly GATED (score: {})", result.risk.score);
    }
}
