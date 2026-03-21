use chrono::{DateTime, Utc};

/// Compute the V-Score (velocity signal) from a session's request timestamps.
///
/// Returns a value in [0.0, 1.0] where:
/// - 0.0 = idle or very slow (safe)
/// - 1.0 = at or above the configured RPM ceiling (maximally suspicious)
///
/// # How it works
///
/// 1. Filters timestamps to only those within the last `window_mins` minutes.
/// 2. Computes RPM across that window using exponential decay weighting —
///    recent requests contribute more than older ones.
/// 3. Normalises against `max_rpm` and clamps to [0.0, 1.0].
///
/// Exponential decay prevents a burst from 4 minutes ago from equally
/// weighting a calm period right now, making the score more responsive
/// to the attacker's current behaviour rather than their historical peak.
pub fn compute(timestamps: &[DateTime<Utc>], window_mins: f32, max_rpm: f32) -> f32 {
    let rpm = weighted_rpm(timestamps, window_mins);
    (rpm / max_rpm).clamp(0.0, 1.0)
}

/// Compute exponentially-decayed RPM over the sliding window.
///
/// Each timestamp is weighted by `e^(-λ * age_mins)` where λ = 1.0.
/// Older timestamps decay toward zero contribution; a request 5 minutes
/// old contributes ~0.7% of what a request right now would.
///
/// Returns 0.0 if fewer than 2 timestamps fall within the window.
pub fn weighted_rpm(timestamps: &[DateTime<Utc>], window_mins: f32) -> f32 {
    if timestamps.len() < 2 {
        return 0.0;
    }

    let now = Utc::now();
    let cutoff = now - chrono::Duration::seconds((window_mins * 60.0) as i64);

    // Collect (age_in_minutes) for each timestamp inside the window.
    let ages: Vec<f32> = timestamps
        .iter()
        .filter(|&&ts| ts >= cutoff)
        .map(|&ts| (now - ts).num_seconds() as f32 / 60.0)
        .collect();

    if ages.len() < 2 {
        return 0.0;
    }

    // Decay constant: λ = 1.0 (half-life ≈ 0.69 minutes).
    const LAMBDA: f32 = 1.0;

    // Weighted count: sum of e^(-λ * age) for each request.
    let weighted_count: f32 = ages.iter().map(|&age| (-LAMBDA * age).exp()).sum();

    // Effective window: the time span covered by the in-window timestamps.
    // Floor at 1/60 minute (1 second) to avoid division by near-zero.
    let span_mins = (ages.first().unwrap() - ages.last().unwrap()).max(1.0 / 60.0);

    weighted_count / span_mins
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn ts_ago(secs: i64) -> DateTime<Utc> {
        Utc::now() - Duration::seconds(secs)
    }

    #[test]
    fn empty_returns_zero() {
        assert_eq!(compute(&[], 5.0, 30.0), 0.0);
    }

    #[test]
    fn single_timestamp_returns_zero() {
        assert_eq!(compute(&[ts_ago(10)], 5.0, 30.0), 0.0);
    }

    #[test]
    fn score_clamped_to_one() {
        // 120 requests in the last 2 minutes — far above any threshold.
        let ts: Vec<_> = (0..120).map(|i| ts_ago(i)).collect();
        assert_eq!(compute(&ts, 5.0, 30.0), 1.0);
    }

    #[test]
    fn timestamps_outside_window_ignored() {
        // One request 10 minutes ago (outside 5-min window) + one recent.
        let ts = vec![ts_ago(600), ts_ago(5)];
        // Only one in-window timestamp → RPM = 0.
        assert_eq!(weighted_rpm(&ts, 5.0), 0.0);
    }

    #[test]
    fn low_rate_gives_low_score() {
        // 3 requests spread across 3 minutes ≈ 1 RPM → well below 30 RPM max.
        let ts = vec![ts_ago(180), ts_ago(90), ts_ago(0)];
        let score = compute(&ts, 5.0, 30.0);
        assert!(score < 0.2, "expected low score, got {score}");
    }

    #[test]
    fn recent_burst_scores_higher_than_old_burst() {
        // Same number of requests, but one set is recent and one is old.
        let recent: Vec<_> = (0..10).map(|i| ts_ago(i * 6)).collect();
        let old: Vec<_> = (0..10).map(|i| ts_ago(240 + i * 6)).collect();

        let score_recent = compute(&recent, 5.0, 30.0);
        let score_old = compute(&old, 5.0, 30.0);

        assert!(
            score_recent > score_old,
            "recent burst ({score_recent}) should score higher than old burst ({score_old})"
        );
    }
}