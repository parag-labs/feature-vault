//! FeatureVault: a minimal feature store with point-in-time-correct (as-of) joins.
//!
//! The hard, valuable part of any feature store is avoiding data leakage: when
//! you build a training set, each row must see only feature values that existed
//! at or before that row's event time -- never a value recorded afterwards.
//! Getting this wrong silently inflates offline metrics and wrecks the model in
//! production.
//!
//! Timestamps are integers (e.g. epoch seconds) to keep the logic portable.

pub mod store;

pub use store::{FeatureStore, FeatureValue, SpineRow};
