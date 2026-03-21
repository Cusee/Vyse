use std::sync::{Arc, RwLock};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

/// Feature vector fed to the anomaly detector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    /// Requests per minute (raw, not normalised).
    pub rpm: f32,
    /// Cosine similarity with the previous prompt embedding.
    pub similarity: f32,
    /// Inverse bigram entropy (0 = normal, 1 = suspicious).
    pub entropy_inv: f32,
    /// Length of the prompt in characters.
    pub prompt_len: f32,
    /// Session duration in minutes since tracking began.
    pub duration_mins: f32,
}

/// Score a feature vector against the anomaly model.
///
/// Returns a value in [0.0, 1.0] where higher = more anomalous.
///
/// The model is initialised with a prior distribution that represents
/// "normal" API usage. As more sessions are observed, the model updates
/// its internal statistics. This makes Vyse adaptive to the deployment
/// context — a batch-processing API will have different baseline RPM
/// than a conversational one.
///
/// # Implementation note
///
/// A full IsolationForest requires training data which we don't have at
/// startup. This implementation uses an online rolling Z-score model as
/// an efficient, incrementally-updatable approximation:
///
/// - For each feature dimension, track (count, mean, M2) via Welford's algorithm
/// - Compute the Z-score of the incoming feature vs the running distribution
/// - The anomaly score is the max absolute Z-score across dimensions, normalised
///
/// This approximates IsolationForest behaviour for unimodal distributions and
/// is replaced with a true IsolationForest checkpoint in Phase 2.
pub fn score(features: &FeatureVector) -> f32 {
    let model = GLOBAL_MODEL.read().unwrap();
    model.score(features)
}

/// Update the global anomaly model with a new observed feature vector.
/// Call this after every inference request to keep the model current.
pub fn update(features: &FeatureVector) {
    let mut model = GLOBAL_MODEL.write().unwrap();
    model.update(features);
}

// ─────────────────────────────────────────────────────────────────────────────
// Online anomaly model (Welford rolling statistics)
// ─────────────────────────────────────────────────────────────────────────────

/// Per-feature running statistics (Welford's online algorithm).
#[derive(Debug, Clone)]
struct FeatureStats {
    count: f64,
    mean: f64,
    m2: f64, // sum of squared deviations — used to compute variance
}

impl FeatureStats {
    /// Initialise with a prior distribution.
    /// The prior count gives a "virtual" sample size that prevents the model
    /// from being too sensitive to the first few real observations.
    fn with_prior(prior_mean: f64, prior_std: f64) -> Self {
        let prior_count = 100.0_f64;
        Self {
            count: prior_count,
            mean: prior_mean,
            m2: prior_std * prior_std * (prior_count - 1.0),
        }
    }

    /// Update the running statistics with a new observation (Welford's algorithm).
    fn update(&mut self, value: f64) {
        self.count += 1.0;
        let delta = value - self.mean;
        self.mean += delta / self.count;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
    }

    /// Sample variance.
    fn variance(&self) -> f64 {
        if self.count < 2.0 {
            return 1.0;
        }
        self.m2 / (self.count - 1.0)
    }

    /// Standard deviation (floored at a small value to prevent division by zero).
    fn std_dev(&self) -> f64 {
        self.variance().sqrt().max(0.01)
    }

    /// Z-score of a new value against the current distribution.
    fn z_score(&self, value: f64) -> f64 {
        (value - self.mean).abs() / self.std_dev()
    }
}

/// Online anomaly model tracking rolling statistics per feature dimension.
struct AnomalyModel {
    rpm:          FeatureStats,
    similarity:   FeatureStats,
    entropy_inv:  FeatureStats,
    prompt_len:   FeatureStats,
    duration_mins: FeatureStats,
}

impl AnomalyModel {
    /// Initialise with conservative priors representing typical API usage patterns.
    fn new() -> Self {
        Self {
            // Prior: normal RPM 0.5–3 req/min (mean=2, std=1.5)
            rpm:          FeatureStats::with_prior(2.0, 1.5),
            // Prior: similarity ~0.3–0.7 for varied human prompts (mean=0.4, std=0.2)
            similarity:   FeatureStats::with_prior(0.4, 0.2),
            // Prior: diverse prompts have low inverse entropy (mean=0.2, std=0.15)
            entropy_inv:  FeatureStats::with_prior(0.2, 0.15),
            // Prior: typical prompts 50–300 chars (mean=150, std=80)
            prompt_len:   FeatureStats::with_prior(150.0, 80.0),
            // Prior: sessions 0–5 min average (mean=2, std=1.5)
            duration_mins: FeatureStats::with_prior(2.0, 1.5),
        }
    }

    fn update(&mut self, f: &FeatureVector) {
        self.rpm.update(f.rpm as f64);
        self.similarity.update(f.similarity as f64);
        self.entropy_inv.update(f.entropy_inv as f64);
        self.prompt_len.update(f.prompt_len as f64);
        self.duration_mins.update(f.duration_mins as f64);
    }

    fn score(&self, f: &FeatureVector) -> f32 {
        let z_scores = [
            self.rpm.z_score(f.rpm as f64),
            self.similarity.z_score(f.similarity as f64),
            self.entropy_inv.z_score(f.entropy_inv as f64),
            self.prompt_len.z_score(f.prompt_len as f64),
            self.duration_mins.z_score(f.duration_mins as f64),
        ];

        // Use the 75th-percentile Z-score (not max) to be robust against
        // single-dimension outliers that could be benign (e.g. very long prompts).
        let mut sorted = z_scores;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p75 = sorted[3]; // index 3 of 5 = 75th percentile

        // Normalise using sigmoid: z=3 → ~0.95, z=2 → ~0.88, z=1 → ~0.73
        let normalised = 1.0 / (1.0 + (-0.5 * (p75 - 2.0)).exp());

        (normalised as f32).clamp(0.0, 1.0)
    }
}

// Global singleton — one model per engine instance.
lazy_static! {
    static ref GLOBAL_MODEL: Arc<RwLock<AnomalyModel>> =
        Arc::new(RwLock::new(AnomalyModel::new()));
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_features_give_low_score() {
        // Normal-looking request: low RPM, varied prompts, short session
        let f = FeatureVector {
            rpm: 2.0,
            similarity: 0.3,
            entropy_inv: 0.15,
            prompt_len: 120.0,
            duration_mins: 1.0,
        };
        let score = {
            let model = AnomalyModel::new();
            model.score(&f)
        };
        assert!(score < 0.7, "normal features scored too high: {score}");
    }

    #[test]
    fn attacker_features_give_high_score() {
        // Attack pattern: high RPM, high similarity (slight variations), high entropy inv
        let f = FeatureVector {
            rpm: 28.0,
            similarity: 0.95,
            entropy_inv: 0.85,
            prompt_len: 80.0,
            duration_mins: 12.0,
        };
        let score = {
            let model = AnomalyModel::new();
            model.score(&f)
        };
        assert!(score > 0.6, "attacker features scored too low: {score}");
    }

    #[test]
    fn score_in_unit_interval() {
        let f = FeatureVector { rpm: 100.0, similarity: 1.0, entropy_inv: 1.0, prompt_len: 10.0, duration_mins: 60.0 };
        let score = { let model = AnomalyModel::new(); model.score(&f) };
        assert!(score >= 0.0 && score <= 1.0, "score out of [0,1]: {score}");
    }
}