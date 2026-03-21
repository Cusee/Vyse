//! Configuration for the Vyse defence engine.
//!
//! Values are loaded from `config.toml` first; environment variables with
//! the `VYSE_ENGINE_` prefix override any file value at runtime.
//!
//! **Secrets** (`redis_url`, `database_url`, `groq_api_key`, `rekor_signing_key`)
//! must be provided via environment variables — never put them in config.toml.

use serde::Deserialize;
use thiserror::Error;

/// Top-level engine configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server:  ServerConfig,
    pub scoring: ScoringConfig,
    pub defence: DefenceConfig,
    pub store:   StoreConfig,
    pub llm:     LlmConfig,
    pub rekor:   RekorConfig,
    pub models:  ModelsConfig,
    pub logging: LoggingConfig,
}

/// gRPC server settings.
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Address the gRPC server binds to. Default: `[::]:50051`
    pub grpc_addr: String,
    /// Maximum gRPC message size in bytes. Default: 4 MiB.
    pub max_message_size_bytes: usize,
    /// Keepalive interval in seconds.
    pub keepalive_secs: u64,
}

/// Scoring signal weights and tier thresholds.
/// All weights must sum to 1.0 — validated on startup.
#[derive(Debug, Deserialize, Clone)]
pub struct ScoringConfig {
    // ── Signal weights ──────────────────────────────────────────────────────
    /// Weight of the V-Score (velocity) signal. Default: 0.25
    pub weight_velocity:   f32,
    /// Weight of the D-Score (divergence) signal. Default: 0.35
    pub weight_divergence: f32,
    /// Weight of the E-Score (entropy) signal. Default: 0.15
    pub weight_entropy:    f32,
    /// Weight of the A-Score (anomaly) signal. Default: 0.25
    pub weight_anomaly:    f32,

    // ── Tier thresholds ─────────────────────────────────────────────────────
    /// Hybrid score at or above which a session is classified Tier 2.
    pub tier2_score_threshold:    f32,
    /// Hybrid score at or above which a session is classified Tier 3
    /// (also requires duration > tier3_min_duration_mins).
    pub tier3_score_threshold:    f32,
    /// Session duration in minutes before Tier 2 is applied (even if score < threshold).
    pub tier2_min_duration_mins:  f32,
    /// Session duration in minutes required for Tier 3 (combined with score threshold).
    pub tier3_min_duration_mins:  f32,

    // ── Velocity (V-Score) ──────────────────────────────────────────────────
    /// Requests-per-minute considered maximum before V-Score saturates at 1.0.
    pub max_rpm: f32,
    /// Sliding window for RPM calculation in minutes. Default: 5.
    pub rpm_window_mins: f32,

    // ── Entropy (E-Score) ───────────────────────────────────────────────────
    /// Number of recent prompts to include in bigram entropy calculation.
    pub entropy_window_size: usize,

    // ── Anomaly (A-Score) ───────────────────────────────────────────────────
    /// Number of trees in the IsolationForest. Default: 100.
    pub isolation_forest_n_trees: usize,
    /// Sub-sample size per tree. Default: 256.
    pub isolation_forest_sample_size: usize,

    // ── Timestamps ──────────────────────────────────────────────────────────
    /// Maximum number of per-session timestamps retained in Redis.
    pub max_timestamps_per_session: usize,
}

/// Defence pipeline settings.
#[derive(Debug, Deserialize, Clone)]
pub struct DefenceConfig {
    /// Fraction of replaceable words substituted with synonyms in Tier 2.
    /// Range: (0.0, 1.0]. Default: 0.45
    pub tier2_synonym_ratio: f32,
    /// Fraction of replaceable words substituted in Tier 3. Default: 0.70
    pub tier3_synonym_ratio: f32,
    /// Numeric values in responses are shifted by ±this fraction. Default: 0.05
    pub numeric_perturb_pct: f32,
    /// Whether to reorder sentences (Tier 3 only). Default: true
    pub reorder_sentences: bool,
}

/// Storage settings for Redis and PostgreSQL.
#[derive(Debug, Deserialize, Clone)]
pub struct StoreConfig {
    /// Redis connection URL. Must be set via VYSE_ENGINE_STORE_REDIS_URL.
    /// Example: redis://localhost:6379
    pub redis_url: String,
    /// Redis key TTL for session state in seconds. Default: 86400 (24h).
    pub session_ttl_secs: u64,
    /// PostgreSQL connection URL. Must be set via VYSE_ENGINE_STORE_DATABASE_URL.
    /// Example: postgres://vyse:password@localhost:5432/vyse
    pub database_url: String,
    /// Maximum PostgreSQL connection pool size. Default: 10.
    pub pg_pool_max: u32,
}

/// LLM provider settings.
#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    /// Which provider to use. Values: "groq" | "openai" | "ollama"
    pub provider: String,
    /// Model name/ID. Example: "llama-3.1-8b-instant"
    pub model: String,
    /// Maximum tokens to generate per response.
    pub max_tokens: u32,
    /// Temperature. Range: [0.0, 2.0]. Default: 0.7
    pub temperature: f32,
    /// Provider API key. Set via VYSE_ENGINE_LLM_API_KEY. Never in config.toml.
    pub api_key: String,
    /// For Ollama only: base URL. Example: http://localhost:11434
    #[serde(default)]
    pub ollama_base_url: String,
}

/// Rekor transparency log settings.
#[derive(Debug, Deserialize, Clone)]
pub struct RekorConfig {
    /// Rekor server URL.
    /// Default: https://rekor.sigstore.dev (public Sigstore instance).
    /// For self-hosted: http://localhost:3002
    pub server_url: String,
    /// Whether Rekor submission is enabled. Set false to disable for testing.
    pub enabled: bool,
    /// Submission timeout in seconds. Default: 10.
    pub timeout_secs: u64,
    /// Path to the Ed25519 private key used to sign Rekor entries.
    /// If empty, a key is generated on startup and stored in the data dir.
    #[serde(default)]
    pub signing_key_path: String,
}

/// ML model file paths and settings.
#[derive(Debug, Deserialize, Clone)]
pub struct ModelsConfig {
    /// Directory containing ONNX model files.
    /// Default: ./models/
    pub model_dir: String,
    /// ONNX file for the sentence embedding model (D-Score).
    /// Default: all-MiniLM-L6-v2.onnx
    pub embedding_model: String,
    /// ONNX file for the NLI intent classifier.
    /// Default: nli-MiniLM2-L6-H768.onnx
    pub intent_model: String,
    /// Number of ONNX Runtime inter-op threads. Default: 2.
    pub ort_inter_op_threads: usize,
    /// Number of ONNX Runtime intra-op threads. Default: 2.
    pub ort_intra_op_threads: usize,
}

/// Logging configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    /// Minimum log level: "trace" | "debug" | "info" | "warn" | "error"
    pub level: String,
    /// Output format: "json" for production, "pretty" for development.
    pub format: String,
}

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config load error: {0}")]
    Load(#[from] config::ConfigError),
    #[error("config validation error:\n{0}")]
    Validation(String),
}

// ── Loader ────────────────────────────────────────────────────────────────────

impl Config {
    /// Load configuration from `config.toml` (if present) and apply
    /// `VYSE_ENGINE_*` environment variable overrides.
    ///
    /// Environment variable mapping:
    /// - Replace `.` with `_` and prepend `VYSE_ENGINE_`
    /// - Example: `store.redis_url` → `VYSE_ENGINE_STORE_REDIS_URL`
    pub fn load() -> Result<Self, ConfigError> {
        let cfg = config::Config::builder()
            // Defaults — every field must have a sensible default here.
            .set_default("server.grpc_addr", "[::]:50051")?
            .set_default("server.max_message_size_bytes", 4 * 1024 * 1024)?
            .set_default("server.keepalive_secs", 30u64)?
            .set_default("scoring.weight_velocity",   0.25f64)?
            .set_default("scoring.weight_divergence", 0.35f64)?
            .set_default("scoring.weight_entropy",    0.15f64)?
            .set_default("scoring.weight_anomaly",    0.25f64)?
            .set_default("scoring.tier2_score_threshold",   0.55f64)?
            .set_default("scoring.tier3_score_threshold",   0.90f64)?
            .set_default("scoring.tier2_min_duration_mins", 2.0f64)?
            .set_default("scoring.tier3_min_duration_mins", 10.0f64)?
            .set_default("scoring.max_rpm",             30.0f64)?
            .set_default("scoring.rpm_window_mins",      5.0f64)?
            .set_default("scoring.entropy_window_size",  20usize)?
            .set_default("scoring.isolation_forest_n_trees",    100usize)?
            .set_default("scoring.isolation_forest_sample_size", 256usize)?
            .set_default("scoring.max_timestamps_per_session",   50usize)?
            .set_default("defence.tier2_synonym_ratio", 0.45f64)?
            .set_default("defence.tier3_synonym_ratio", 0.70f64)?
            .set_default("defence.numeric_perturb_pct", 0.05f64)?
            .set_default("defence.reorder_sentences",   true)?
            .set_default("store.session_ttl_secs", 86400u64)?
            .set_default("store.pg_pool_max",      10u32)?
            .set_default("llm.provider",    "groq")?
            .set_default("llm.model",       "llama-3.1-8b-instant")?
            .set_default("llm.max_tokens",  512u32)?
            .set_default("llm.temperature", 0.7f64)?
            .set_default("llm.ollama_base_url", "http://localhost:11434")?
            .set_default("rekor.server_url", "https://rekor.sigstore.dev")?
            .set_default("rekor.enabled",     true)?
            .set_default("rekor.timeout_secs", 10u64)?
            .set_default("rekor.signing_key_path", "")?
            .set_default("models.model_dir",          "./models")?
            .set_default("models.embedding_model",    "all-MiniLM-L6-v2.onnx")?
            .set_default("models.intent_model",       "nli-MiniLM2-L6-H768.onnx")?
            .set_default("models.ort_inter_op_threads", 2usize)?
            .set_default("models.ort_intra_op_threads", 2usize)?
            .set_default("logging.level",  "info")?
            .set_default("logging.format", "json")?
            // Config file (optional — env vars are sufficient).
            .add_source(
                config::File::with_name("config")
                    .format(config::FileFormat::Toml)
                    .required(false),
            )
            // Environment variable overrides.
            .add_source(
                config::Environment::with_prefix("VYSE_ENGINE")
                    .separator("_")
                    .try_parsing(true),
            )
            .build()?;

        let parsed: Config = cfg.try_deserialize()?;
        parsed.validate()?;
        Ok(parsed)
    }

    /// Validate all fields that have invariants not expressible in the type system.
    fn validate(&self) -> Result<(), ConfigError> {
        let mut errors: Vec<String> = Vec::new();

        // Weights must sum to 1.0 (within float tolerance).
        let weight_sum = self.scoring.weight_velocity
            + self.scoring.weight_divergence
            + self.scoring.weight_entropy
            + self.scoring.weight_anomaly;
        if (weight_sum - 1.0f32).abs() > 0.01 {
            errors.push(format!(
                "scoring weights must sum to 1.0, got {weight_sum:.4}"
            ));
        }

        // Required secrets.
        if self.store.redis_url.is_empty() {
            errors.push("store.redis_url must be set via VYSE_ENGINE_STORE_REDIS_URL".into());
        }
        if self.store.database_url.is_empty() {
            errors.push("store.database_url must be set via VYSE_ENGINE_STORE_DATABASE_URL".into());
        }
        if self.llm.api_key.is_empty() && self.llm.provider != "ollama" {
            errors.push("llm.api_key must be set via VYSE_ENGINE_LLM_API_KEY (not required for ollama)".into());
        }

        // Synonym ratios.
        if self.defence.tier2_synonym_ratio <= 0.0 || self.defence.tier2_synonym_ratio > 1.0 {
            errors.push("defence.tier2_synonym_ratio must be in (0.0, 1.0]".into());
        }
        if self.defence.tier3_synonym_ratio <= 0.0 || self.defence.tier3_synonym_ratio > 1.0 {
            errors.push("defence.tier3_synonym_ratio must be in (0.0, 1.0]".into());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Validation(errors.join("\n  - ")))
        }
    }
}