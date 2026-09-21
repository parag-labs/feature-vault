// Package featurevault is a minimal feature store with point-in-time-correct
// (as-of) joins.
//
// The hard, valuable part of any feature store is avoiding data leakage: when
// you build a training set, each row must see only feature values that existed
// at or before that row's event time -- never a value recorded afterwards.
// Getting this wrong silently inflates offline metrics and wrecks the model in
// production.
//
// Timestamps are integers (e.g. epoch seconds) to keep the logic portable.
package featurevault

import "sort"

// FeatureValue is a single observed feature value for an entity at a timestamp.
type FeatureValue struct {
	Entity    string
	Timestamp int64
	Value     float64
}

// SpineRow is one row of a training spine: an entity paired with the event time
// whose point-in-time feature value we want.
type SpineRow struct {
	Entity    string
	EventTime int64
}

// FeatureStore stores time-ordered feature values per entity and serves as-of
// joins. The zero value is not usable; construct one with NewFeatureStore.
type FeatureStore struct {
	ts    map[string][]int64
	vals  map[string][]float64
	dirty map[string]bool
}

// NewFeatureStore returns an empty FeatureStore ready to ingest values.
func NewFeatureStore() *FeatureStore {
	return &FeatureStore{
		ts:    make(map[string][]int64),
		vals:  make(map[string][]float64),
		dirty: make(map[string]bool),
	}
}

// Ingest records a feature value. Values may arrive in any timestamp order;
// each entity's observations are sorted lazily on the first read.
func (s *FeatureStore) Ingest(fv FeatureValue) {
	s.ts[fv.Entity] = append(s.ts[fv.Entity], fv.Timestamp)
	s.vals[fv.Entity] = append(s.vals[fv.Entity], fv.Value)
	s.dirty[fv.Entity] = true
}

// GetPointInTime returns the latest feature value with timestamp <= eventTime
// (a backward as-of join), or nil if no such value exists.
//
// If maxStaleness is non-nil and the newest eligible value is older than it,
// the feature is treated as expired and nil is returned.
func (s *FeatureStore) GetPointInTime(entity string, eventTime int64, maxStaleness *int64) *float64 {
	if _, ok := s.ts[entity]; !ok {
		return nil
	}
	s.ensureSorted(entity)
	ts := s.ts[entity]
	idx := upperBound(ts, eventTime) - 1
	if idx < 0 {
		return nil
	}
	if maxStaleness != nil && eventTime-ts[idx] > *maxStaleness {
		return nil
	}
	v := s.vals[entity][idx]
	return &v
}

// GetTrainingSet returns a point-in-time-correct feature column, one entry per
// spine row (nil where no value is available).
func (s *FeatureStore) GetTrainingSet(spine []SpineRow, maxStaleness *int64) []*float64 {
	out := make([]*float64, len(spine))
	for i, r := range spine {
		out[i] = s.GetPointInTime(r.Entity, r.EventTime, maxStaleness)
	}
	return out
}

// GetOnline returns the latest known value for an entity (the serving path), or
// nil if the entity is unknown.
func (s *FeatureStore) GetOnline(entity string) *float64 {
	if _, ok := s.ts[entity]; !ok {
		return nil
	}
	s.ensureSorted(entity)
	vals := s.vals[entity]
	if len(vals) == 0 {
		return nil
	}
	v := vals[len(vals)-1]
	return &v
}

type observation struct {
	t int64
	v float64
}

// ensureSorted orders an entity's observations by (timestamp, value) so that
// among values sharing a timestamp the largest value sorts last. This makes the
// as-of tie-break deterministic and independent of ingest order.
func (s *FeatureStore) ensureSorted(entity string) {
	if !s.dirty[entity] {
		return
	}
	ts := s.ts[entity]
	vals := s.vals[entity]
	paired := make([]observation, len(ts))
	for i := range ts {
		paired[i] = observation{ts[i], vals[i]}
	}
	sort.Slice(paired, func(a, b int) bool {
		if paired[a].t != paired[b].t {
			return paired[a].t < paired[b].t
		}
		return paired[a].v < paired[b].v
	})
	newTs := make([]int64, len(paired))
	newVals := make([]float64, len(paired))
	for i, p := range paired {
		newTs[i] = p.t
		newVals[i] = p.v
	}
	s.ts[entity] = newTs
	s.vals[entity] = newVals
	delete(s.dirty, entity)
}

// upperBound returns the first index i where ts[i] > target (std::upper_bound).
func upperBound(ts []int64, target int64) int {
	lo, hi := 0, len(ts)
	for lo < hi {
		mid := (lo + hi) / 2
		if ts[mid] <= target {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	return lo
}
