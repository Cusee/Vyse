//! Defence pipeline — applies perturbation to model responses for Tier 2/3 sessions.
//!
//! # Design principles
//!
//! **Seed-locked determinism**: every step is seeded from a per-session HMAC.
//! Given the same seed and clean text, the pipeline produces identical output.
//! This makes all perturbations forensically replayable from the audit log.
//!
//! **Tier-scaled intensity**:
//! - Tier 1 → no-op, clean response returned unchanged
//! - Tier 2 → synonym substitution + numeric perturbation
//! - Tier 3 → synonym + numeric + sentence reordering

pub mod numeric;
pub mod synonym;

use crate::config::DefenceConfig;
use hmac::{Hmac, Mac};
use rand::{rngs::StdRng, Rng, SeedableRng};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// The result of running the defence pipeline.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// The response to serve to the client.
    pub served_response: String,
    /// Names of the perturbation steps applied, in order. Empty for Tier 1.
    pub steps_applied: Vec<String>,
    /// Hex seed used — stored in the audit log for forensic replay.
    pub noise_seed_hex: String,
}

/// Run the defence pipeline for the given tier.
pub fn run(
    clean_response: &str,
    tier: u8,
    session_id_hash: &str,
    session_start_unix: i64,
    cfg: &DefenceConfig,
) -> PipelineResult {
    if tier == 1 {
        return PipelineResult {
            served_response: clean_response.to_string(),
            steps_applied:   Vec::new(),
            noise_seed_hex:  String::new(),
        };
    }

    let seed_bytes = derive_session_seed(session_id_hash, session_start_unix);
    let noise_seed_hex = hex::encode(&seed_bytes[..8]);
    let seed_u64 = u64::from_le_bytes(
        seed_bytes[..8].try_into().unwrap_or([0u8; 8]),
    );
    let mut rng = StdRng::seed_from_u64(seed_u64);

    let mut text = clean_response.to_string();
    let mut steps: Vec<String> = Vec::new();

    // Step 1 — synonym substitution (Tier 2 + 3).
    let ratio = if tier == 3 { cfg.tier3_synonym_ratio } else { cfg.tier2_synonym_ratio };
    text = synonym::substitute(&text, ratio, &mut rng);
    steps.push(format!("synonym_substitution(ratio={ratio:.2})"));

    // Step 2 — numeric perturbation (Tier 2 + 3).
    let (perturbed, changed) = numeric::perturb(&text, cfg.numeric_perturb_pct, &mut rng);
    if changed {
        text = perturbed;
        steps.push(format!("numeric_perturb(pct={:.2})", cfg.numeric_perturb_pct));
    }

    // Step 3 — sentence reordering (Tier 3 only).
    if tier == 3 && cfg.reorder_sentences {
        let (reordered, changed) = reorder_sentences(&text, &mut rng);
        if changed {
            text = reordered;
            steps.push("sentence_reorder".into());
        }
    }

    PipelineResult {
        served_response: text,
        steps_applied:   steps,
        noise_seed_hex,
    }
}

/// Derive a 32-byte session seed: HMAC-SHA256(key=session_id_hash, msg=start_unix).
fn derive_session_seed(session_id_hash: &str, session_start_unix: i64) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(session_id_hash.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(session_start_unix.to_string().as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Shuffle independent sentences using a seeded RNG (Fisher-Yates).
/// Only applied when there are 3+ sentences — a 2-sentence shuffle is obvious.
fn reorder_sentences(text: &str, rng: &mut StdRng) -> (String, bool) {
    let sentences: Vec<&str> = text
        .split(". ")
        .filter(|s| !s.trim().is_empty())
        .collect();

    if sentences.len() < 3 {
        return (text.to_string(), false);
    }

    let mut indices: Vec<usize> = (0..sentences.len()).collect();
    for i in (1..sentences.len()).rev() {
        let j = rng.gen_range(0..=i);
        indices.swap(i, j);
    }

    let reordered = indices
        .iter()
        .map(|&i| sentences[i])
        .collect::<Vec<_>>()
        .join(". ");

    (reordered, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DefenceConfig;

    fn cfg() -> DefenceConfig {
        DefenceConfig {
            tier2_synonym_ratio: 0.45,
            tier3_synonym_ratio: 0.70,
            numeric_perturb_pct: 0.05,
            reorder_sentences:   true,
        }
    }

    #[test]
    fn tier1_passthrough() {
        let r = run("Hello world.", 1, "abc", 0, &cfg());
        assert_eq!(r.served_response, "Hello world.");
        assert!(r.steps_applied.is_empty());
    }

    #[test]
    fn tier2_applies_steps() {
        let r = run("The model returns confidence values for each class.", 2, "abc", 0, &cfg());
        assert!(!r.steps_applied.is_empty());
    }

    #[test]
    fn deterministic_with_same_seed() {
        let text = "The quick brown fox jumps over the lazy dog.";
        let r1 = run(text, 2, "sess", 1000, &cfg());
        let r2 = run(text, 2, "sess", 1000, &cfg());
        assert_eq!(r1.served_response, r2.served_response);
    }
}