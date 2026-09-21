package featurevault

import "testing"

// staleness returns a pointer to a max-staleness bound for the optional argument.
func staleness(n int64) *int64 { return &n }

func newStore() *FeatureStore {
	s := NewFeatureStore()
	for _, o := range []observation{{100, 1.0}, {200, 2.0}, {300, 3.0}} {
		s.Ingest(FeatureValue{"user-1", o.t, o.v})
	}
	return s
}

func mustValue(t *testing.T, got *float64, want float64) {
	t.Helper()
	if got == nil {
		t.Fatalf("expected %v, got nil", want)
	}
	if *got != want {
		t.Fatalf("expected %v, got %v", want, *got)
	}
}

func mustNil(t *testing.T, got *float64) {
	t.Helper()
	if got != nil {
		t.Fatalf("expected nil, got %v", *got)
	}
}

func TestAsOfJoinPicksLatestBeforeEvent(t *testing.T) {
	s := newStore()
	mustValue(t, s.GetPointInTime("user-1", 250, nil), 2.0) // latest <= 250 is ts=200
	mustValue(t, s.GetPointInTime("user-1", 300, nil), 3.0) // inclusive of event time
	mustValue(t, s.GetPointInTime("user-1", 100, nil), 1.0)
}

func TestNoLeakageBeforeFirstValue(t *testing.T) {
	mustNil(t, newStore().GetPointInTime("user-1", 50, nil))
}

func TestNoFutureLeak(t *testing.T) {
	// At event time 150 only the ts=100 value existed; ts=200/300 are the future.
	mustValue(t, newStore().GetPointInTime("user-1", 150, nil), 1.0)
}

func TestMaxStalenessExpiresOldFeatures(t *testing.T) {
	s := newStore()
	// Newest value at event 1000 is ts=300 -> 700 old; with staleness 100 it expires.
	mustNil(t, s.GetPointInTime("user-1", 1000, staleness(100)))
	mustValue(t, s.GetPointInTime("user-1", 350, staleness(100)), 3.0)
}

func TestMaxStalenessBoundaryIsInclusive(t *testing.T) {
	s := newStore()
	// event 400, ts=300 -> exactly 100 old; staleness 100 is not "> 100", so kept.
	mustValue(t, s.GetPointInTime("user-1", 400, staleness(100)), 3.0)
	// One second older tips it over the boundary and expires it.
	mustNil(t, s.GetPointInTime("user-1", 401, staleness(100)))
}

func TestUnknownEntityReturnsNone(t *testing.T) {
	s := newStore()
	mustNil(t, s.GetPointInTime("ghost", 200, nil))
	mustNil(t, s.GetOnline("ghost"))
}

func TestTrainingSetIsPointInTimeCorrect(t *testing.T) {
	s := newStore()
	spine := []SpineRow{{"user-1", 120}, {"user-1", 220}, {"user-1", 320}}
	col := s.GetTrainingSet(spine, nil)
	want := []float64{1.0, 2.0, 3.0}
	if len(col) != len(want) {
		t.Fatalf("length mismatch: got %d want %d", len(col), len(want))
	}
	for i, w := range want {
		mustValue(t, col[i], w)
	}
}

func TestTrainingSetLeavesGapsAsNil(t *testing.T) {
	s := newStore()
	// First row precedes any value; unknown entity has no value either.
	spine := []SpineRow{{"user-1", 50}, {"user-1", 250}, {"ghost", 999}}
	col := s.GetTrainingSet(spine, nil)
	mustNil(t, col[0])
	mustValue(t, col[1], 2.0)
	mustNil(t, col[2])
}

func TestOnlineReturnsLatest(t *testing.T) {
	mustValue(t, newStore().GetOnline("user-1"), 3.0)
}

func TestOutOfOrderIngestIsHandled(t *testing.T) {
	s := NewFeatureStore()
	for _, o := range []observation{{300, 3.0}, {100, 1.0}, {200, 2.0}} {
		s.Ingest(FeatureValue{"e", o.t, o.v})
	}
	mustValue(t, s.GetPointInTime("e", 250, nil), 2.0)
	mustValue(t, s.GetOnline("e"), 3.0)
}

func TestDuplicateTimestampLargestValueWins(t *testing.T) {
	// The documented tie-break: values are ordered by (timestamp, value), so
	// among equal timestamps the largest value is served.
	s := NewFeatureStore()
	for _, o := range []observation{{200, 5.0}, {200, 2.0}, {200, 9.0}, {100, 1.0}} {
		s.Ingest(FeatureValue{"e", o.t, o.v})
	}
	mustValue(t, s.GetPointInTime("e", 250, nil), 9.0)
	mustValue(t, s.GetOnline("e"), 9.0)
	// A pick at ts=100 is unaffected by the later duplicates.
	mustValue(t, s.GetPointInTime("e", 150, nil), 1.0)
}

func TestEmptyStoreReturnsNil(t *testing.T) {
	s := NewFeatureStore()
	mustNil(t, s.GetPointInTime("nobody", 10, nil))
	mustNil(t, s.GetOnline("nobody"))
	if col := s.GetTrainingSet(nil, nil); len(col) != 0 {
		t.Fatalf("expected empty column, got %v", col)
	}
}

func TestSingleObservationBoundaries(t *testing.T) {
	s := NewFeatureStore()
	s.Ingest(FeatureValue{"e", 100, 1.0})
	mustNil(t, s.GetPointInTime("e", 99, nil)) // strictly before -> nothing
	mustValue(t, s.GetPointInTime("e", 100, nil), 1.0)
	mustValue(t, s.GetPointInTime("e", 101, nil), 1.0)
}

func TestNegativeAndZeroValuesAreServed(t *testing.T) {
	s := NewFeatureStore()
	s.Ingest(FeatureValue{"e", 10, 0.0})
	s.Ingest(FeatureValue{"e", 20, -3.5})
	mustValue(t, s.GetPointInTime("e", 15, nil), 0.0)
	mustValue(t, s.GetPointInTime("e", 25, nil), -3.5)
	mustValue(t, s.GetOnline("e"), -3.5)
}
