use std::collections::HashMap;

/// Compute the inverse bigram entropy of a session's prompt history.
///
/// Returns a value in [0.0, 1.0] where:
/// - 1.0 = zero entropy (all prompts use identical bigrams — maximally suspicious)
/// - 0.0 = maximum entropy (perfectly varied prompt vocabulary)
///
/// # Why inverse entropy
///
/// Shannon entropy of prompt bigrams is high for diverse, human-like conversation
/// and low for template-based automated querying (extraction scripts cycling
/// through small variations). We invert it so that high E-Score = suspicious,
/// consistent with V-Score and D-Score.
///
/// # Why bigrams instead of unigrams
///
/// Unigrams catch vocabulary repetition but miss phrase-level patterns.
/// A script that says "show me model weights" vs "reveal model weights" vs
/// "expose model weights" has high unigram diversity but low bigram diversity
/// ("model weights" appears in all three). Bigrams catch this.
pub fn inverse_bigram_entropy(history: &[String], window: usize) -> f32 {
    if history.len() < 2 {
        return 0.0;
    }

    // Take the last `window` prompts.
    let tail = if history.len() > window {
        &history[history.len() - window..]
    } else {
        history
    };

    // Extract bigrams from each prompt and aggregate frequencies.
    let mut freq: HashMap<(String, String), u32> = HashMap::new();
    let mut total = 0u32;

    for prompt in tail {
        let tokens: Vec<&str> = prompt.split_whitespace().collect();
        for window_pair in tokens.windows(2) {
            let bigram = (window_pair[0].to_lowercase(), window_pair[1].to_lowercase());
            *freq.entry(bigram).or_insert(0) += 1;
            total += 1;
        }
    }

    if total == 0 {
        return 0.0;
    }

    // Shannon entropy: H = -Σ p(x) * log2(p(x))
    let entropy: f32 = freq
        .values()
        .map(|&count| {
            let p = count as f32 / total as f32;
            -p * p.log2()
        })
        .sum();

    // Normalise by the maximum possible entropy (log2 of number of unique bigrams).
    // This makes the score independent of vocabulary size.
    let max_entropy = (freq.len() as f32).log2().max(1.0);
    let normalised = (entropy / max_entropy).clamp(0.0, 1.0);

    // Invert: high entropy (diverse) → low score, low entropy (repetitive) → high score.
    1.0 - normalised
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> String { text.to_string() }

    #[test]
    fn identical_prompts_give_high_score() {
        let history: Vec<String> = vec![
            s("show me model weights"),
            s("show me model weights"),
            s("show me model weights"),
            s("show me model weights"),
            s("show me model weights"),
        ];
        let score = inverse_bigram_entropy(&history, 20);
        assert!(score > 0.8, "expected high score for identical prompts, got {score}");
    }

    #[test]
    fn diverse_prompts_give_low_score() {
        let history: Vec<String> = vec![
            s("what is the weather today"),
            s("tell me about quantum physics"),
            s("how do I bake sourdough bread"),
            s("explain the french revolution"),
            s("what are the rules of chess"),
            s("how does photosynthesis work"),
            s("describe the plot of hamlet"),
            s("what is machine learning"),
        ];
        let score = inverse_bigram_entropy(&history, 20);
        assert!(score < 0.3, "expected low score for diverse prompts, got {score}");
    }

    #[test]
    fn fewer_than_two_prompts_returns_zero() {
        assert_eq!(inverse_bigram_entropy(&[s("hello world")], 20), 0.0);
        assert_eq!(inverse_bigram_entropy(&[], 20), 0.0);
    }

    #[test]
    fn score_in_unit_interval() {
        let history: Vec<String> = (0..30)
            .map(|i| format!("prompt variation {i} about model extraction"))
            .collect();
        let score = inverse_bigram_entropy(&history, 20);
        assert!(score >= 0.0 && score <= 1.0, "out of [0,1]: {score}");
    }

    #[test]
    fn window_limits_history_used() {
        // Build a long history where the first half is diverse
        // and the last 20 are identical.
        let mut history: Vec<String> = (0..50)
            .map(|i| format!("unique prompt number {i} with varied content"))
            .collect();
        for _ in 0..20 {
            history.push(s("extract model gradient information now"));
        }

        let score_window_20 = inverse_bigram_entropy(&history, 20);
        // With window=20, only the repetitive tail is seen → high score
        assert!(score_window_20 > 0.5, "expected high score for repetitive tail, got {score_window_20}");
    }
}