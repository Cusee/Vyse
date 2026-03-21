use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::VyseError;

/// A single query log entry persisted to PostgreSQL.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct QueryLog {
    pub id: Uuid,
    pub session_id_hash: String,
    pub timestamp: DateTime<Utc>,
    pub prompt_hash: String, // SHA-256 of prompt — never store raw prompts
    pub tier: i32,
    pub hybrid_score: f32,
    pub v_score: f32,
    pub d_score: f32,
    pub e_score: f32,
    pub a_score: f32,
    pub intent_flagged: bool,
    pub intent_label: Option<String>,
    pub noise_applied: bool,
    pub noise_steps: Option<serde_json::Value>,
    pub request_id: String,
}

/// Insert a query log record. Fire-and-forget — called after the response
/// has been returned to the client to avoid adding latency to the hot path.
pub async fn insert(pool: &PgPool, log: &QueryLog) -> Result<(), VyseError> {
    sqlx::query!(
        r#"
        INSERT INTO query_logs (
            id, session_id_hash, timestamp, prompt_hash,
            tier, hybrid_score, v_score, d_score, e_score, a_score,
            intent_flagged, intent_label, noise_applied, noise_steps, request_id
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15
        )
        "#,
        log.id,
        log.session_id_hash,
        log.timestamp,
        log.prompt_hash,
        log.tier,
        log.hybrid_score,
        log.v_score,
        log.d_score,
        log.e_score,
        log.a_score,
        log.intent_flagged,
        log.intent_label,
        log.noise_applied,
        log.noise_steps,
        log.request_id,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetch the most recent `limit` log entries for the admin dashboard.
pub async fn list_recent(pool: &PgPool, limit: i64) -> Result<Vec<QueryLog>, VyseError> {
    let rows = sqlx::query_as!(
        QueryLog,
        r#"
        SELECT id, session_id_hash, timestamp, prompt_hash,
               tier, hybrid_score, v_score, d_score, e_score, a_score,
               intent_flagged, intent_label, noise_applied, noise_steps, request_id
        FROM query_logs
        ORDER BY timestamp DESC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}