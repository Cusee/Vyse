use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::VyseError;

/// The full behavioural profile for one session, stored in Redis.
///
/// Redis key: `vyse:session:{session_id_hash}`
/// TTL: configurable via `redis.session_ttl_secs` (default: 3600s)
///
/// Serialised as JSON — serde handles the encoding/decoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// SHA-256 hex hash of the raw session ID.
    pub session_id_hash: String,
    /// Current security tier: 1, 2, or 3.
    pub tier: i32,
    /// Timestamp of the first request in this session.
    pub first_seen_at: DateTime<Utc>,
    /// Timestamp of the most recent request.
    pub last_active_at: DateTime<Utc>,
    /// Timestamp when suspicious activity was first detected.
    /// None until the session exceeds the tracking_threshold.
    pub tracking_started_at: Option<DateTime<Utc>>,
    /// Total number of requests in this session.
    pub request_count: u64,
    /// Rolling average requests-per-minute.
    pub dynamic_mean_rpm: f32,
    /// Most recently computed hybrid threat score.
    pub current_hybrid_score: f32,
    /// Timestamps of the last N requests (for velocity calculation).
    pub timestamps: Vec<DateTime<Utc>>,
    /// Last N prompt texts (for entropy calculation).
    pub prompt_history: Vec<String>,
    /// The embedding vector of the most recent prompt (float32, flat).
    pub last_embedding: Option<Vec<f32>>,
    /// Whether this session has been manually banned by an admin.
    pub is_banned: bool,
    /// Deterministic noise seed derived from session_id_hash + tracking_started_at.
    /// Used to make perturbations reproducible for forensic replay.
    pub noise_seed: Option<String>,
}

impl Session {
    /// Create a new session with default values.
    pub fn new(session_id_hash: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            session_id_hash: session_id_hash.into(),
            tier: 1,
            first_seen_at: now,
            last_active_at: now,
            tracking_started_at: None,
            request_count: 0,
            dynamic_mean_rpm: 0.0,
            current_hybrid_score: 0.0,
            timestamps: Vec::new(),
            prompt_history: Vec::new(),
            last_embedding: None,
            is_banned: false,
            noise_seed: None,
        }
    }

    /// How long this session has been actively tracked (minutes).
    pub fn tracking_duration_mins(&self) -> f32 {
        match self.tracking_started_at {
            Some(started) => {
                (Utc::now() - started).num_seconds() as f32 / 60.0
            }
            None => 0.0,
        }
    }

    /// Trim the timestamps and prompt_history vecs to `max_len`.
    /// Keeps the most recent entries.
    pub fn trim(&mut self, max_timestamps: usize, max_history: usize) {
        if self.timestamps.len() > max_timestamps {
            let drain = self.timestamps.len() - max_timestamps;
            self.timestamps.drain(..drain);
        }
        if self.prompt_history.len() > max_history {
            let drain = self.prompt_history.len() - max_history;
            self.prompt_history.drain(..drain);
        }
    }
}

/// Redis key prefix for session records.
const SESSION_KEY_PREFIX: &str = "vyse:session:";

fn session_key(hash: &str) -> String {
    format!("{SESSION_KEY_PREFIX}{hash}")
}

/// Load a session from Redis. Returns `None` if the key does not exist.
#[instrument(skip(conn), fields(hash = %hash[..8.min(hash.len())]))]
pub async fn get(
    conn: &mut impl AsyncCommands,
    hash: &str,
) -> Result<Option<Session>, VyseError> {
    let raw: Option<String> = conn.get(session_key(hash)).await?;
    match raw {
        Some(json) => {
            let session: Session = serde_json::from_str(&json)
                .map_err(|e| VyseError::Internal(format!("deserialise session: {e}")))?;
            Ok(Some(session))
        }
        None => Ok(None),
    }
}

/// Upsert a session in Redis, resetting the TTL.
#[instrument(skip(conn, session), fields(
    hash = %session.session_id_hash[..8.min(session.session_id_hash.len())],
    tier = session.tier,
))]
pub async fn set(
    conn: &mut impl AsyncCommands,
    session: &Session,
    ttl_secs: u64,
) -> Result<(), VyseError> {
    let json = serde_json::to_string(session)
        .map_err(|e| VyseError::Internal(format!("serialise session: {e}")))?;

    conn.set_ex(session_key(&session.session_id_hash), json, ttl_secs)
        .await?;

    Ok(())
}

/// Delete a session from Redis. Returns `true` if the key existed.
pub async fn delete(
    conn: &mut impl AsyncCommands,
    hash: &str,
) -> Result<bool, VyseError> {
    let deleted: u64 = conn.del(session_key(hash)).await?;
    Ok(deleted > 0)
}

/// Scan all session keys. Returns the list of session ID hashes.
/// Used by ListSessions — may be slow on large deployments; use with pagination.
pub async fn list_hashes(
    conn: &mut impl AsyncCommands,
) -> Result<Vec<String>, VyseError> {
    let pattern = format!("{SESSION_KEY_PREFIX}*");
    let keys: Vec<String> = conn.keys(pattern).await?;
    let hashes = keys
        .into_iter()
        .map(|k| k.strip_prefix(SESSION_KEY_PREFIX).unwrap_or(&k).to_string())
        .collect();
    Ok(hashes)
}