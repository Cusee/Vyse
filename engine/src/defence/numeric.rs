use once_cell::sync::Lazy;
use regex::Regex;

/// Regex that matches integers and decimals, including negatives.
/// Excludes numbers embedded inside words (e.g. "3D", "mp3", "IPv4").
static NUMBER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?<![a-zA-Z])(-?\d+(?:\.\d+)?)(?![a-zA-Z%])").unwrap()
});

/// Perturb all numeric values in `text` by at most ±`fraction` of their value.
///
/// Returns a new string with every detected number shifted by a deterministic
/// amount derived from the session `seed` and the number's character position.
///
/// # Why perturb numbers
///
/// Model extraction attackers collect many (prompt, response) pairs to
/// reconstruct the model's behaviour. When responses contain confidence scores,
/// probabilities, or measurements, consistent numeric perturbation means the
/// attacker's collected data is systematically skewed — their reconstructed
/// model is off by a consistent margin they cannot detect or correct for.
///
/// # Determinism guarantee
///
/// Given the same `text` and `seed`, this function always produces the same
/// output. This is critical for forensic replay: an auditor with the session
/// seed and the clean response can reproduce exactly what was served.
///
/// # Arguments
///
/// * `text`     — the clean LLM response to perturb
/// * `fraction` — maximum perturbation as a fraction of the original value
///                (e.g. 0.05 = ±5%). Typical range: 0.03–0.08.
/// * `seed`     — the session's deterministic noise seed (hex string)
///
/// # Example
///
/// ```
/// let result = perturb("Accuracy is 0.923 on 1000 samples", 0.05, "abc123");
/// // "Accuracy is 0.967 on 1047 samples"  (exact values depend on seed)
/// ```
pub fn perturb(text: &str, fraction: f32, seed: &str) -> String {
    if fraction <= 0.0 || text.is_empty() {
        return text.to_string();
    }

    let seed_int = seed_to_u64(seed);

    NUMBER_RE
        .replace_all(text, |caps: &regex::Captures| {
            let matched = &caps[1];
            let char_pos = caps.get(1).unwrap().start() as u64;

            // Each number at a different position gets a different perturbation.
            // XOR with position so the same number at two positions shifts differently.
            let factor = perturbation_factor(seed_int, char_pos, fraction);

            if matched.contains('.') {
                // Float — preserve original decimal place count.
                if let Ok(val) = matched.parse::<f64>() {
                    let decimals = matched
                        .find('.')
                        .map(|i| matched.len() - i - 1)
                        .unwrap_or(2)
                        .min(6);
                    return format!("{:.prec$}", val * factor, prec = decimals);
                }
            } else {
                // Integer — round to nearest whole number.
                if let Ok(val) = matched.parse::<i64>() {
                    let perturbed = (val as f64 * factor).round() as i64;
                    return perturbed.to_string();
                }
            }

            // Fallback: leave unchanged if parsing fails.
            matched.to_string()
        })
        .into_owned()
}

/// Compute a perturbation factor in [1 - fraction, 1 + fraction] deterministically.
///
/// Uses a two-step LCG: first mix the seed with the character position,
/// then advance one more step. Maps the high bits of the result to [0.0, 1.0]
/// and scales into the perturbation range.
fn perturbation_factor(seed: u64, position: u64, fraction: f32) -> f64 {
    // Mix seed and position so each number gets an independent perturbation.
    let mixed = seed
        .wrapping_mul(0x9e3779b97f4a7c15)
        .wrapping_add(position.wrapping_mul(0x6c62272e07bb0142));

    // LCG step (Knuth multiplicative).
    let lcg = mixed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);

    // Map high 32 bits to [0.0, 1.0].
    let t = (lcg >> 32) as f64 / u32::MAX as f64;

    // Map to [1 - fraction, 1 + fraction].
    1.0 + (2.0 * t - 1.0) * fraction as f64
}

/// Convert the leading 8 bytes of a seed string to a u64.
fn seed_to_u64(seed: &str) -> u64 {
    seed.as_bytes()
        .iter()
        .take(8)
        .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_output() {
        let text = "Score is 0.923, samples: 1000, threshold 0.5";
        let a = perturb(text, 0.05, "fixed-seed-xyz");
        let b = perturb(text, 0.05, "fixed-seed-xyz");
        assert_eq!(a, b, "same seed must produce identical output");
    }

    #[test]
    fn different_seeds_different_output() {
        let text = "Accuracy 0.88 on 500 examples.";
        let a = perturb(text, 0.05, "seed-aaa");
        let b = perturb(text, 0.05, "seed-bbb");
        assert_ne!(a, b, "different seeds should produce different output");
    }

    #[test]
    fn zero_fraction_passthrough() {
        let text = "Value is 42 and rate is 0.75.";
        assert_eq!(perturb(text, 0.0, "any-seed"), text);
    }

    #[test]
    fn empty_text_passthrough() {
        assert_eq!(perturb("", 0.05, "seed"), "");
    }

    #[test]
    fn non_numeric_text_unchanged() {
        let text = "This response contains no numbers whatsoever.";
        assert_eq!(perturb(text, 0.10, "seed"), text);
    }

    #[test]
    fn integer_stays_integer() {
        let text = "There are 100 items.";
        let result = perturb(text, 0.05, "seed-abc");
        // Should not contain a decimal point for the number.
        assert!(
            !result.contains("100."),
            "integer should remain an integer: {result}"
        );
    }

    #[test]
    fn float_preserves_decimal_places() {
        let text = "Confidence: 0.923.";
        let result = perturb(text, 0.05, "seed-abc");
        // Should preserve 3 decimal places.
        let re = regex::Regex::new(r"(\d+\.\d+)").unwrap();
        if let Some(cap) = re.captures(&result) {
            let num = &cap[1];
            let decimals = num.find('.').map(|i| num.len() - i - 1).unwrap_or(0);
            assert_eq!(decimals, 3, "expected 3 decimal places, got {decimals} in {result}");
        }
    }

    #[test]
    fn perturbation_within_bounds() {
        // Run 50 different seeds and verify all perturbed values stay within ±10%.
        let original = 1000i64;
        let text = format!("value is {original}");
        let re = regex::Regex::new(r"value is (-?\d+)").unwrap();

        for i in 0..50 {
            let seed = format!("test-seed-{i}");
            let result = perturb(&text, 0.10, &seed);
            if let Some(cap) = re.captures(&result) {
                let perturbed: i64 = cap[1].parse().unwrap();
                let max_allowed = (original as f64 * 1.10).ceil() as i64;
                let min_allowed = (original as f64 * 0.90).floor() as i64;
                assert!(
                    perturbed >= min_allowed && perturbed <= max_allowed,
                    "seed {seed}: {perturbed} outside [{min_allowed}, {max_allowed}]"
                );
            }
        }
    }

    #[test]
    fn negative_numbers_perturbed_correctly() {
        let text = "Temperature delta is -3.5 degrees.";
        let result = perturb(text, 0.05, "neg-seed");
        // Result should still contain a negative number.
        assert!(result.contains('-'), "negative sign should be preserved: {result}");
    }

    #[test]
    fn numbers_in_words_not_matched() {
        // "mp3", "IPv4", "3D" — numbers embedded in identifiers should not change.
        let text = "Play the mp3 file and check IPv4 connectivity in 3D.";
        assert_eq!(perturb(text, 0.10, "seed"), text);
    }
}