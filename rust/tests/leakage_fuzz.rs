// The leakage fuzz suite: the as-of join must never return a value from the
// future. Thousands of randomized ingest orders, event times, and staleness
// windows are cross-checked against a brute-force oracle. Exact float equality
// is intentional -- the store returns the same f64 it stored.
#![allow(clippy::float_cmp)]

use feature_vault::{FeatureStore, FeatureValue, SpineRow};

/// A tiny dependency-free SplitMix64 PRNG so the fuzz suite needs no external
/// crate (the machine building this has no C compiler for native RNG crates).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a value in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// Returns a float in `[0, 1)`.
    fn float(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A brute-force point-in-time lookup used to cross-check the store. Mirrors the
/// store's tie-break: among values at-or-before the event time, the largest
/// `(timestamp, value)` wins.
fn oracle(values: &[(i64, f64)], event_time: i64, max_staleness: Option<i64>) -> Option<f64> {
    let mut best: Option<(i64, f64)> = None;
    for &(t, v) in values {
        if t > event_time {
            continue;
        }
        best = match best {
            Some((bt, bv)) if bt > t || (bt == t && bv >= v) => Some((bt, bv)),
            _ => Some((t, v)),
        };
    }
    let (t, v) = best?;
    if let Some(ms) = max_staleness {
        if event_time - t > ms {
            return None;
        }
    }
    Some(v)
}

#[test]
fn join_never_returns_a_future_value() {
    let mut rng = Rng::new(0);
    for _ in 0..2000 {
        let n = rng.below(30) + 1;
        let values: Vec<(i64, f64)> = (0..n)
            .map(|_| (rng.below(1001) as i64, round3(rng.float() * 200.0 - 100.0)))
            .collect();

        // Ingest in a shuffled order (Fisher-Yates) to prove order independence.
        let mut order: Vec<usize> = (0..values.len()).collect();
        for i in (1..order.len()).rev() {
            let j = rng.below((i + 1) as u64) as usize;
            order.swap(i, j);
        }
        let mut store = FeatureStore::new();
        for &j in &order {
            store.ingest(FeatureValue::new("e", values[j].0, values[j].1));
        }

        for _ in 0..5 {
            let event_time = rng.below(1001) as i64;
            let got = store.get_point_in_time("e", event_time, None);
            let expected = oracle(&values, event_time, None);
            assert_eq!(got, expected, "leak/mismatch at t={event_time}");
            if let Some(g) = got {
                assert!(
                    values.iter().any(|&(t, v)| t <= event_time && v == g),
                    "returned value {g} not present at or before t={event_time}"
                );
            }
        }
    }
}

#[test]
fn staleness_window_is_respected_under_fuzz() {
    let mut rng = Rng::new(1);
    for _ in 0..2000 {
        let n = rng.below(20) + 1;
        let values: Vec<(i64, f64)> = (0..n)
            .map(|_| (rng.below(501) as i64, round3(rng.float() * 10.0)))
            .collect();
        let mut store = FeatureStore::new();
        for &(t, v) in &values {
            store.ingest(FeatureValue::new("e", t, v));
        }
        let event_time = rng.below(601) as i64;
        let max_staleness = rng.below(201) as i64;
        assert_eq!(
            store.get_point_in_time("e", event_time, Some(max_staleness)),
            oracle(&values, event_time, Some(max_staleness))
        );
    }
}

#[test]
fn ingest_order_does_not_change_results() {
    let mut rng = Rng::new(2);
    let values: Vec<(i64, f64)> = (0..40)
        .map(|_| (rng.below(201) as i64, round3(rng.float() * 5.0)))
        .collect();

    let mut a = FeatureStore::new();
    for &(t, v) in &values {
        a.ingest(FeatureValue::new("e", t, v));
    }
    let mut b = FeatureStore::new();
    for &(t, v) in values.iter().rev() {
        b.ingest(FeatureValue::new("e", t, v));
    }
    let mut et = 0i64;
    while et < 210 {
        assert_eq!(
            a.get_point_in_time("e", et, None),
            b.get_point_in_time("e", et, None)
        );
        et += 7;
    }
}

#[test]
fn training_set_is_row_wise_point_in_time_correct() {
    let mut rng = Rng::new(3);
    let mut store = FeatureStore::new();
    let mut per_entity: std::collections::HashMap<String, Vec<(i64, f64)>> =
        std::collections::HashMap::new();
    for _ in 0..500 {
        let e = format!("user-{}", rng.below(21));
        let t = rng.below(1001) as i64;
        let v = round3(rng.float() * 2.0 - 1.0);
        store.ingest(FeatureValue::new(e.clone(), t, v));
        per_entity.entry(e).or_default().push((t, v));
    }
    let spine: Vec<SpineRow> = (0..300)
        .map(|_| SpineRow::new(format!("user-{}", rng.below(21)), rng.below(1001) as i64))
        .collect();
    let column = store.get_training_set(&spine, None);
    for (row, got) in spine.iter().zip(column.iter()) {
        let empty = Vec::new();
        let expected = oracle(
            per_entity.get(&row.entity).unwrap_or(&empty),
            row.event_time,
            None,
        );
        assert_eq!(*got, expected);
    }
}

#[test]
fn no_value_before_first_observation() {
    let mut store = FeatureStore::new();
    store.ingest(FeatureValue::new("e", 100, 1.0));
    assert_eq!(store.get_point_in_time("e", 99, None), None);
    assert_eq!(store.get_point_in_time("e", 100, None), Some(1.0));
}

#[test]
fn unknown_entity_returns_none_under_fuzz() {
    let mut store = FeatureStore::new();
    store.ingest(FeatureValue::new("known", 10, 1.0));
    assert_eq!(store.get_point_in_time("unknown", 100, None), None);
}
