/// Compute the D-Score (semantic divergence signal) between two prompt embeddings.
///
/// Returns a value in [0.0, 1.0] where:
/// - 0.0 = completely unrelated prompts (diverse, human-like conversation)
/// - 1.0 = identical or near-identical prompts (systematic extraction probing)
///
/// # Why cosine similarity
///
/// Model extraction attackers vary their prompts minimally — changing one word
/// or rephrasing slightly — to map the model's decision boundary while staying
/// under naive rate limits. Cosine similarity between consecutive embeddings
/// catches this: semantically similar prompts produce embedding vectors that
/// point in nearly the same direction regardless of surface-level word changes.
///
/// Example: "show model weights" and "display model parameters" differ in every
/// word but have cosine similarity ≈ 0.91 under all-MiniLM-L6-v2 — correctly
/// flagged as suspicious. A hash-based approach would score these as unrelated.
///
/// # Arguments
///
/// * `current`  — embedding of the current prompt (384-dim f32 vector)
/// * `previous` — embedding of the previous prompt in this session
///
/// Both vectors must be L2-normalised (the ml::embedding module guarantees this).
/// For L2-normalised vectors, cosine similarity reduces to a dot product.
pub fn compute(current: &[f32], previous: &[f32]) -> f32 {
    debug_assert_eq!(
        current.len(),
        previous.len(),
        "embedding dimension mismatch: {} vs {}",
        current.len(),
        previous.len()
    );

    if current.is_empty() || previous.is_empty() {
        return 0.0;
    }

    cosine_similarity(current, previous)
}

/// Cosine similarity between two float slices.
///
/// For L2-normalised inputs this is equivalent to a dot product (no sqrt needed).
/// The result is always clamped to [0.0, 1.0] — all-MiniLM-L6-v2 embeddings
/// use ReLU activations and produce non-negative values, so the natural range
/// is [0, 1]. The clamp handles floating-point rounding at the boundaries.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    let denom = norm_a * norm_b;
    if denom < f32::EPSILON {
        return 0.0;
    }

    (dot / denom).clamp(0.0, 1.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_vectors_return_one() {
        let v = vec![0.3, 0.5, 0.2, 0.8, 0.1];
        let sim = compute(&v, &v);
        assert!((sim - 1.0).abs() < 1e-5, "expected 1.0, got {sim}");
    }

    #[test]
    fn orthogonal_vectors_return_zero() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = compute(&a, &b);
        assert!(sim.abs() < 1e-5, "expected 0.0, got {sim}");
    }

    #[test]
    fn zero_vector_returns_zero() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![0.5, 0.3, 0.8];
        assert_eq!(compute(&a, &b), 0.0);
    }

    #[test]
    fn empty_inputs_return_zero() {
        assert_eq!(compute(&[], &[]), 0.0);
    }

    #[test]
    fn result_within_unit_interval() {
        // Arbitrary non-normalised vectors — result must still be in [0, 1].
        let a = vec![0.1, 0.9, 0.4, 0.2, 0.7];
        let b = vec![0.8, 0.1, 0.5, 0.6, 0.3];
        let sim = compute(&a, &b);
        assert!(sim >= 0.0 && sim <= 1.0, "out of [0,1]: {sim}");
    }

    #[test]
    fn similar_vectors_score_higher_than_dissimilar() {
        // Very similar: slight perturbation.
        let base = vec![0.6, 0.5, 0.4, 0.3, 0.2];
        let similar = vec![0.61, 0.49, 0.41, 0.31, 0.21];
        // Very dissimilar: nearly opposite direction.
        let dissimilar = vec![0.1, 0.1, 0.9, 0.9, 0.05];

        let sim_high = compute(&base, &similar);
        let sim_low = compute(&base, &dissimilar);

        assert!(
            sim_high > sim_low,
            "similar ({sim_high}) should score higher than dissimilar ({sim_low})"
        );
    }
}