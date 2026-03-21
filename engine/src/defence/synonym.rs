//! Synonym substitution perturbation.
//!
//! Replaces a fraction of content words (nouns, verbs, adjectives) with
//! synonyms from a curated substitution table. The table covers the
//! vocabulary most commonly found in ML API responses about confidence
//! scores, predictions, and model outputs.
//!
//! Using a curated table rather than a full WordNet lookup keeps the
//! engine dependency-free from Python and produces higher-quality
//! substitutions than a naive random synonym pick.

use rand::{rngs::StdRng, seq::SliceRandom, Rng};
use std::collections::HashMap;

/// Apply synonym substitution to `text`.
///
/// Replaces approximately `ratio` (e.g. 0.45) of substitutable words
/// with synonyms chosen from the built-in table.
///
/// # Arguments
/// * `text`  — the text to perturb
/// * `ratio` — fraction of substitutable words to replace, in (0.0, 1.0]
/// * `rng`   — seeded RNG (caller owns the seed for determinism)
pub fn substitute(text: &str, ratio: f32, rng: &mut StdRng) -> String {
    let table = substitution_table();
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return text.to_string();
    }

    // Identify substitutable positions.
    let substitutable: Vec<usize> = words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let key = w.trim_matches(|c: char| !c.is_alphabetic()).to_lowercase();
            if table.contains_key(key.as_str()) {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if substitutable.is_empty() {
        return text.to_string();
    }

    // Select `ratio` fraction to actually replace.
    let n_replace = ((substitutable.len() as f32 * ratio).ceil() as usize)
        .max(1)
        .min(substitutable.len());

    let mut chosen = substitutable.clone();
    chosen.shuffle(rng);
    let chosen: std::collections::HashSet<usize> =
        chosen.into_iter().take(n_replace).collect();

    // Rebuild the token list with replacements.
    let result: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, &word)| {
            if !chosen.contains(&i) {
                return word.to_string();
            }

            // Extract any surrounding punctuation.
            let leading: String = word.chars().take_while(|c| !c.is_alphabetic()).collect();
            let trailing: String = word.chars().rev().take_while(|c| !c.is_alphabetic()).collect();
            let trailing: String = trailing.chars().rev().collect();
            let core = word
                .trim_matches(|c: char| !c.is_alphabetic())
                .to_lowercase();

            if let Some(synonyms) = table.get(core.as_str()) {
                let synonym = synonyms.choose(rng).unwrap();
                // Preserve original capitalisation pattern.
                let replaced = if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    let mut s = synonym.to_string();
                    if let Some(r) = s.get_mut(0..1) {
                        r.make_ascii_uppercase();
                    }
                    s
                } else {
                    synonym.to_string()
                };
                format!("{leading}{replaced}{trailing}")
            } else {
                word.to_string()
            }
        })
        .collect();

    result.join(" ")
}

/// The substitution table.
/// Keys are lowercase source words; values are candidate synonyms.
/// Designed specifically for the vocabulary of ML API responses.
fn substitution_table() -> HashMap<&'static str, &'static [&'static str]> {
    let mut t: HashMap<&'static str, &'static [&'static str]> = HashMap::new();

    // Confidence / probability vocabulary
    t.insert("confidence",   &["certainty", "probability", "likelihood", "assurance"]);
    t.insert("probability",  &["likelihood", "chance", "odds", "confidence"]);
    t.insert("prediction",   &["forecast", "estimate", "projection", "output"]);
    t.insert("score",        &["rating", "value", "measure", "metric"]);
    t.insert("result",       &["outcome", "output", "response", "finding"]);
    t.insert("output",       &["result", "response", "answer", "finding"]);
    t.insert("response",     &["answer", "reply", "output", "result"]);

    // Model / system vocabulary
    t.insert("model",        &["system", "framework", "classifier", "algorithm"]);
    t.insert("classifier",   &["model", "detector", "estimator", "predictor"]);
    t.insert("algorithm",    &["method", "approach", "technique", "procedure"]);
    t.insert("feature",      &["attribute", "property", "characteristic", "variable"]);
    t.insert("data",         &["information", "input", "records", "samples"]);
    t.insert("input",        &["data", "query", "prompt", "request"]);

    // Action vocabulary
    t.insert("compute",      &["calculate", "determine", "evaluate", "assess"]);
    t.insert("calculate",    &["compute", "determine", "derive", "assess"]);
    t.insert("generate",     &["produce", "create", "yield", "output"]);
    t.insert("produce",      &["generate", "create", "yield", "deliver"]);
    t.insert("detect",       &["identify", "recognise", "find", "locate"]);
    t.insert("identify",     &["detect", "recognise", "classify", "determine"]);
    t.insert("provide",      &["offer", "deliver", "supply", "give"]);
    t.insert("return",       &["provide", "deliver", "yield", "output"]);

    // Quality / accuracy vocabulary
    t.insert("accurate",     &["precise", "correct", "reliable", "exact"]);
    t.insert("precise",      &["accurate", "exact", "correct", "reliable"]);
    t.insert("high",         &["elevated", "significant", "substantial", "strong"]);
    t.insert("low",          &["minimal", "slight", "reduced", "small"]);
    t.insert("significant",  &["notable", "substantial", "considerable", "meaningful"]);

    // Common nouns
    t.insert("class",        &["category", "type", "label", "group"]);
    t.insert("category",     &["class", "type", "group", "classification"]);
    t.insert("label",        &["tag", "class", "category", "annotation"]);
    t.insert("value",        &["measure", "quantity", "amount", "reading"]);
    t.insert("error",        &["mistake", "inaccuracy", "discrepancy", "deviation"]);

    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn rng() -> StdRng { StdRng::seed_from_u64(42) }

    #[test]
    fn substitutes_known_words() {
        let mut rng = rng();
        let text = "The model prediction has high confidence.";
        let result = substitute(text, 1.0, &mut rng);
        // With ratio=1.0 all known words should be replaced.
        assert_ne!(result, text);
    }

    #[test]
    fn preserves_text_without_known_words() {
        let mut rng = rng();
        let text = "Xyzzy plugh twisty maze passages.";
        let result = substitute(text, 1.0, &mut rng);
        assert_eq!(result, text); // no matches → unchanged
    }

    #[test]
    fn deterministic_with_same_rng_seed() {
        let text = "The model generates a prediction with high accuracy.";
        let r1 = substitute(text, 0.5, &mut StdRng::seed_from_u64(1));
        let r2 = substitute(text, 0.5, &mut StdRng::seed_from_u64(1));
        assert_eq!(r1, r2);
    }

    #[test]
    fn preserves_capitalisation() {
        let mut rng = rng();
        let text = "Prediction is high.";
        let result = substitute(text, 1.0, &mut rng);
        let first_word = result.split_whitespace().next().unwrap();
        assert!(
            first_word.chars().next().unwrap().is_uppercase(),
            "first word should remain capitalised: {result}"
        );
    }

    #[test]
    fn empty_input() {
        let mut rng = rng();
        assert_eq!(substitute("", 0.5, &mut rng), "");
    }
}