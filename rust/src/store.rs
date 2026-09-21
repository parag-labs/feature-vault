//! The feature store: time-ordered values per entity plus as-of lookups.

use std::collections::{HashMap, HashSet};

/// A single observed feature value for an entity at a timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureValue {
    /// The entity the value belongs to.
    pub entity: String,
    /// When the value was observed (e.g. epoch seconds).
    pub timestamp: i64,
    /// The observed value.
    pub value: f64,
}

impl FeatureValue {
    /// Creates a feature value.
    pub fn new(entity: impl Into<String>, timestamp: i64, value: f64) -> Self {
        Self {
            entity: entity.into(),
            timestamp,
            value,
        }
    }
}

/// One row of a training spine: an entity paired with the event time whose
/// point-in-time feature value is wanted.
#[derive(Debug, Clone, PartialEq)]
pub struct SpineRow {
    /// The entity to look up.
    pub entity: String,
    /// The event time to join as-of.
    pub event_time: i64,
}

impl SpineRow {
    /// Creates a spine row.
    pub fn new(entity: impl Into<String>, event_time: i64) -> Self {
        Self {
            entity: entity.into(),
            event_time,
        }
    }
}

/// A minimal feature store that serves point-in-time-correct (as-of) joins.
#[derive(Debug, Default)]
pub struct FeatureStore {
    ts: HashMap<String, Vec<i64>>,
    vals: HashMap<String, Vec<f64>>,
    dirty: HashSet<String>,
}

impl FeatureStore {
    /// Creates an empty feature store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a feature value. Values may arrive in any timestamp order; each
    /// entity's observations are sorted lazily on the first read.
    pub fn ingest(&mut self, fv: FeatureValue) {
        self.ts
            .entry(fv.entity.clone())
            .or_default()
            .push(fv.timestamp);
        self.vals
            .entry(fv.entity.clone())
            .or_default()
            .push(fv.value);
        self.dirty.insert(fv.entity);
    }

    /// Returns the latest feature value with `timestamp <= event_time` (a
    /// backward as-of join), or `None` if no such value exists.
    ///
    /// If `max_staleness` is `Some` and the newest eligible value is older than
    /// it, the feature is treated as expired and `None` is returned.
    pub fn get_point_in_time(
        &mut self,
        entity: &str,
        event_time: i64,
        max_staleness: Option<i64>,
    ) -> Option<f64> {
        if !self.ts.contains_key(entity) {
            return None;
        }
        self.ensure_sorted(entity);
        let ts = &self.ts[entity];
        let count = upper_bound(ts, event_time);
        if count == 0 {
            return None;
        }
        let idx = count - 1;
        if let Some(ms) = max_staleness {
            if event_time - ts[idx] > ms {
                return None;
            }
        }
        Some(self.vals[entity][idx])
    }

    /// Returns a point-in-time-correct feature column, one entry per spine row
    /// (`None` where no value is available).
    pub fn get_training_set(
        &mut self,
        spine: &[SpineRow],
        max_staleness: Option<i64>,
    ) -> Vec<Option<f64>> {
        spine
            .iter()
            .map(|r| self.get_point_in_time(&r.entity, r.event_time, max_staleness))
            .collect()
    }

    /// Returns the latest known value for an entity (the serving path), or
    /// `None` if the entity is unknown.
    pub fn get_online(&mut self, entity: &str) -> Option<f64> {
        if !self.ts.contains_key(entity) {
            return None;
        }
        self.ensure_sorted(entity);
        self.vals[entity].last().copied()
    }

    /// Orders an entity's observations by `(timestamp, value)` so that among
    /// values sharing a timestamp the largest value sorts last. This makes the
    /// as-of tie-break deterministic and independent of ingest order.
    fn ensure_sorted(&mut self, entity: &str) {
        if !self.dirty.contains(entity) {
            return;
        }
        let mut paired: Vec<(i64, f64)> = self.ts[entity]
            .iter()
            .copied()
            .zip(self.vals[entity].iter().copied())
            .collect();
        paired.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
        let new_ts: Vec<i64> = paired.iter().map(|p| p.0).collect();
        let new_vals: Vec<f64> = paired.iter().map(|p| p.1).collect();
        self.ts.insert(entity.to_string(), new_ts);
        self.vals.insert(entity.to_string(), new_vals);
        self.dirty.remove(entity);
    }
}

/// Returns the first index `i` where `ts[i] > target` (std::upper_bound).
fn upper_bound(ts: &[i64], target: i64) -> usize {
    let (mut lo, mut hi) = (0usize, ts.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if ts[mid] <= target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}
