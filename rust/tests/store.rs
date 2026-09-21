// Exact expected values are compared with `==` on purpose: the store returns
// the same f64 it was given, so equality is bit-exact, not approximate.
#![allow(clippy::float_cmp)]

use feature_vault::{FeatureStore, FeatureValue, SpineRow};

fn store() -> FeatureStore {
    let mut s = FeatureStore::new();
    for &(t, v) in &[(100i64, 1.0f64), (200, 2.0), (300, 3.0)] {
        s.ingest(FeatureValue::new("user-1", t, v));
    }
    s
}

#[test]
fn as_of_join_picks_latest_before_event() {
    let mut s = store();
    assert_eq!(s.get_point_in_time("user-1", 250, None), Some(2.0)); // latest <= 250 is ts=200
    assert_eq!(s.get_point_in_time("user-1", 300, None), Some(3.0)); // inclusive of event time
    assert_eq!(s.get_point_in_time("user-1", 100, None), Some(1.0));
}

#[test]
fn no_leakage_before_first_value() {
    let mut s = store();
    assert_eq!(s.get_point_in_time("user-1", 50, None), None);
}

#[test]
fn no_future_leak() {
    // At event time 150 only the ts=100 value existed; ts=200/300 are the future.
    let mut s = store();
    assert_eq!(s.get_point_in_time("user-1", 150, None), Some(1.0));
}

#[test]
fn max_staleness_expires_old_features() {
    let mut s = store();
    assert_eq!(s.get_point_in_time("user-1", 1000, Some(100)), None);
    assert_eq!(s.get_point_in_time("user-1", 350, Some(100)), Some(3.0));
}

#[test]
fn max_staleness_boundary_is_inclusive() {
    let mut s = store();
    // event 400, ts=300 -> exactly 100 old; 100 is not "> 100", so kept.
    assert_eq!(s.get_point_in_time("user-1", 400, Some(100)), Some(3.0));
    // One unit older tips past the boundary and expires it.
    assert_eq!(s.get_point_in_time("user-1", 401, Some(100)), None);
}

#[test]
fn unknown_entity_returns_none() {
    let mut s = store();
    assert_eq!(s.get_point_in_time("ghost", 200, None), None);
    assert_eq!(s.get_online("ghost"), None);
}

#[test]
fn training_set_is_point_in_time_correct() {
    let mut s = store();
    let spine = vec![
        SpineRow::new("user-1", 120),
        SpineRow::new("user-1", 220),
        SpineRow::new("user-1", 320),
    ];
    assert_eq!(
        s.get_training_set(&spine, None),
        vec![Some(1.0), Some(2.0), Some(3.0)]
    );
}

#[test]
fn training_set_leaves_gaps_as_none() {
    let mut s = store();
    let spine = vec![
        SpineRow::new("user-1", 50),
        SpineRow::new("user-1", 250),
        SpineRow::new("ghost", 999),
    ];
    assert_eq!(
        s.get_training_set(&spine, None),
        vec![None, Some(2.0), None]
    );
}

#[test]
fn online_returns_latest() {
    let mut s = store();
    assert_eq!(s.get_online("user-1"), Some(3.0));
}

#[test]
fn out_of_order_ingest_is_handled() {
    let mut s = FeatureStore::new();
    for &(t, v) in &[(300i64, 3.0f64), (100, 1.0), (200, 2.0)] {
        s.ingest(FeatureValue::new("e", t, v));
    }
    assert_eq!(s.get_point_in_time("e", 250, None), Some(2.0));
    assert_eq!(s.get_online("e"), Some(3.0));
}

#[test]
fn duplicate_timestamp_largest_value_wins() {
    // The documented tie-break: values are ordered by (timestamp, value), so
    // among equal timestamps the largest value is served.
    let mut s = FeatureStore::new();
    for &(t, v) in &[(200i64, 5.0f64), (200, 2.0), (200, 9.0), (100, 1.0)] {
        s.ingest(FeatureValue::new("e", t, v));
    }
    assert_eq!(s.get_point_in_time("e", 250, None), Some(9.0));
    assert_eq!(s.get_online("e"), Some(9.0));
    assert_eq!(s.get_point_in_time("e", 150, None), Some(1.0));
}

#[test]
fn empty_store_returns_none() {
    let mut s = FeatureStore::new();
    assert_eq!(s.get_point_in_time("nobody", 10, None), None);
    assert_eq!(s.get_online("nobody"), None);
    assert_eq!(s.get_training_set(&[], None), Vec::<Option<f64>>::new());
}

#[test]
fn single_observation_boundaries() {
    let mut s = FeatureStore::new();
    s.ingest(FeatureValue::new("e", 100, 1.0));
    assert_eq!(s.get_point_in_time("e", 99, None), None); // strictly before -> nothing
    assert_eq!(s.get_point_in_time("e", 100, None), Some(1.0));
    assert_eq!(s.get_point_in_time("e", 101, None), Some(1.0));
}

#[test]
fn negative_and_zero_values_are_served() {
    let mut s = FeatureStore::new();
    s.ingest(FeatureValue::new("e", 10, 0.0));
    s.ingest(FeatureValue::new("e", 20, -3.5));
    assert_eq!(s.get_point_in_time("e", 15, None), Some(0.0));
    assert_eq!(s.get_point_in_time("e", 25, None), Some(-3.5));
    assert_eq!(s.get_online("e"), Some(-3.5));
}
