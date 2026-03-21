//! Behavioral threat scoring engine.
//!
//! Computes four signals from per-session state and combines them into a
//! hybrid threat score that determines the security tier.
//!
//! # Signal weights (configurable in `config.toml`)
//!
//! | Signal | Default weight | Detects |
//! |--------|----------------|---------|
//! | V-Score (velocity) | 0.25 | High-frequency automated queries |
//! | D-Score (divergence) | 0.35 | Systematic probing with minimal input variation |
//! | E-Score (entropy) | 0.15 | Template-based extraction scripts |
//! | A-Score (anomaly) | 0.25 | Statistically abnormal sessions |
//!
//! # Tier classification
//!
//! ```text
//! Tier 1 (Clean)      : hybrid < 0.55  AND  duration <  2 min
//! Tier 2 (Suspicious) : hybrid ≥ 0.55  OR   duration ∈ [2, 10) min
//! Tier 3 (Malicious)  : hybrid ≥ 0.90  AND  duration > 10 min
//! ```

pub mod a_score;
pub mod d_score;
pub mod e_score;
pub mod v_score;

use crate::config::ScoringConfig;

/// All four signal scores for a single request.
#[derive(Debug, Clone, Default)]
pub struct Scores {
    /// Velocity score in [0.0, 1.0].
    pub v: f32,
    /// Divergence score in [0.0, 1.0].
    pub d: f32,
    /// Entropy score in [0.0, 1.0]. Higher = more suspicious (low entropy).
    pub e: f32,
    /// Anomaly score in [0.0, 1.0]. Higher = more anomalous.
    pub a: f32,
    /// Whether the intent classifier forced a Tier 2 escalation.
    pub intent_flagged: bool,
    /// The final weighted combination.
    pub hybrid: f32,
}

/// Compute the hybrid score from individual signals.
///
/// Formula: `H = w_v * V + w_d * D + w_e * E + w_a * A`
///
/// Weights are taken from `cfg` and validated to sum to 1.0 at startup.
pub fn compute_hybrid(v: f32, d: f32, e: f32, a: f32, cfg: &ScoringConfig) -> f32 {
    let score = cfg.weight_velocity   * v
              + cfg.weight_divergence * d
              + cfg.weight_entropy    * e
              + cfg.weight_anomaly    * a;
    score.clamp(0.0, 1.0)
}

/// Classify a session into a security tier based on hybrid score and duration.
///
/// # Arguments
/// * `hybrid` — the combined threat score in [0.0, 1.0]
/// * `duration_mins` — minutes since the session's `first_seen_at` was set
/// * `intent_flagged` — whether the NLI classifier flagged this request
/// * `cfg` — scoring thresholds from config
///
/// # Returns
/// Security tier: 1 (clean), 2 (suspicious), or 3 (malicious).
pub fn classify_tier(
    hybrid: f32,
    duration_mins: f32,
    intent_flagged: bool,
    cfg: &ScoringConfig,
) -> u8 {
    // Intent classification is an override: a semantically malicious prompt
    // escalates immediately to Tier 2 regardless of the numerical score.
    // This catches slow attackers who keep their RPM and entropy in range.
    if intent_flagged {
        // Check whether duration also qualifies for Tier 3.
        if hybrid >= cfg.tier3_score_threshold
            && duration_mins > cfg.tier3_min_duration_mins
        {
            return 3;
        }
        return 2;
    }

    if hybrid >= cfg.tier3_score_threshold
        && duration_mins > cfg.tier3_min_duration_mins
    {
        3
    } else if hybrid >= cfg.tier2_score_threshold
        || duration_mins >= cfg.tier2_min_duration_mins
    {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScoringConfig;

    fn default_cfg() -> ScoringConfig {
        ScoringConfig {
            weight_velocity:   0.25,
            weight_divergence: 0.35,
            weight_entropy:    0.15,
            weight_anomaly:    0.25,
            tier2_score_threshold:   0.55,
            tier3_score_threshold:   0.90,
            tier2_min_duration_mins: 2.0,
            tier3_min_duration_mins: 10.0,
            max_rpm:             30.0,
            rpm_window_mins:      5.0,
            entropy_window_size:  20,
            isolation_forest_n_trees:    100,
            isolation_forest_sample_size: 256,
            max_timestamps_per_session:   50,
        }
    }

    #[test]
    fn tier1_low_score_short_session() {
        let cfg = default_cfg();
        assert_eq!(classify_tier(0.30, 0.5, false, &cfg), 1);
    }

    #[test]
    fn tier2_score_threshold() {
        let cfg = default_cfg();
        // At exactly the threshold → Tier 2.
        assert_eq!(classify_tier(0.55, 0.0, false, &cfg), 2);
    }

    #[test]
    fn tier2_duration_threshold() {
        let cfg = default_cfg();
        // Score below threshold but duration ≥ 2 min → Tier 2.
        assert_eq!(classify_tier(0.40, 2.0, false, &cfg), 2);
    }

    #[test]
    fn tier3_requires_both_score_and_duration() {
        let cfg = default_cfg();
        // High score but short duration → only Tier 2.
        assert_eq!(classify_tier(0.95, 5.0, false, &cfg), 2);
        // High score AND long duration → Tier 3.
        assert_eq!(classify_tier(0.95, 11.0, false, &cfg), 3);
    }

    #[test]
    fn intent_flag_forces_tier2_minimum() {
        let cfg = default_cfg();
        // Low score but intent flagged → at least Tier 2.
        assert_eq!(classify_tier(0.10, 0.1, true, &cfg), 2);
    }

    #[test]
    fn hybrid_weights_sum_to_one() {
        let cfg = default_cfg();
        let sum = cfg.weight_velocity
            + cfg.weight_divergence
            + cfg.weight_entropy
            + cfg.weight_anomaly;
        assert!((sum - 1.0).abs() < 0.001, "weights sum to {sum}");
    }

    #[test]
    fn hybrid_score_clamped_to_unit_range() {
        let cfg = default_cfg();
        // All signals at maximum → hybrid = 1.0 (not >1.0).
        let h = compute_hybrid(1.0, 1.0, 1.0, 1.0, &cfg);
        assert_eq!(h, 1.0);
        // All signals at zero → hybrid = 0.0.
        let h = compute_hybrid(0.0, 0.0, 0.0, 0.0, &cfg);
        assert_eq!(h, 0.0);
    }
}