package featurevault

import (
	"math"
	"math/rand"
	"testing"
)

// oracle is a brute-force point-in-time lookup used to cross-check the store.
//
// It mirrors the store's deterministic tie-break: observations are ordered by
// (timestamp, value) and the last one at-or-before the event time wins, so among
// equal timestamps the largest value is returned.
func oracle(values []observation, eventTime int64, maxStaleness *int64) (float64, bool) {
	best := observation{}
	found := false
	for _, o := range values {
		if o.t > eventTime {
			continue
		}
		if !found || o.t > best.t || (o.t == best.t && o.v > best.v) {
			best = o
			found = true
		}
	}
	if !found {
		return 0, false
	}
	if maxStaleness != nil && eventTime-best.t > *maxStaleness {
		return 0, false
	}
	return best.v, true
}

func round3(x float64) float64 { return math.Round(x*1000) / 1000 }

func checkAgrees(t *testing.T, got *float64, wantVal float64, wantOK bool, ctx string) {
	t.Helper()
	if wantOK {
		if got == nil {
			t.Fatalf("%s: expected %v, got nil", ctx, wantVal)
		}
		if *got != wantVal {
			t.Fatalf("%s: expected %v, got %v", ctx, wantVal, *got)
		}
	} else if got != nil {
		t.Fatalf("%s: expected nil, got %v", ctx, *got)
	}
}

func TestJoinNeverReturnsAFutureValue(t *testing.T) {
	rng := rand.New(rand.NewSource(0))
	for iter := 0; iter < 2000; iter++ {
		n := rng.Intn(30) + 1
		values := make([]observation, n)
		for i := range values {
			values[i] = observation{int64(rng.Intn(1001)), round3(rng.Float64()*200 - 100)}
		}
		store := NewFeatureStore()
		order := rng.Perm(n)
		for _, j := range order {
			store.Ingest(FeatureValue{"e", values[j].t, values[j].v})
		}
		for q := 0; q < 5; q++ {
			eventTime := int64(rng.Intn(1001))
			got := store.GetPointInTime("e", eventTime, nil)
			wv, wok := oracle(values, eventTime, nil)
			checkAgrees(t, got, wv, wok, "join")
			if got != nil {
				// The returned value must correspond to a timestamp at or before the event.
				matched := false
				for _, o := range values {
					if o.t <= eventTime && o.v == *got {
						matched = true
						break
					}
				}
				if !matched {
					t.Fatalf("leaked value %v not present at or before t=%d", *got, eventTime)
				}
			}
		}
	}
}

func TestStalenessWindowIsRespectedUnderFuzz(t *testing.T) {
	rng := rand.New(rand.NewSource(1))
	for iter := 0; iter < 2000; iter++ {
		n := rng.Intn(20) + 1
		values := make([]observation, n)
		for i := range values {
			values[i] = observation{int64(rng.Intn(501)), round3(rng.Float64() * 10)}
		}
		store := NewFeatureStore()
		for _, o := range values {
			store.Ingest(FeatureValue{"e", o.t, o.v})
		}
		eventTime := int64(rng.Intn(601))
		ms := int64(rng.Intn(201))
		got := store.GetPointInTime("e", eventTime, &ms)
		wv, wok := oracle(values, eventTime, &ms)
		checkAgrees(t, got, wv, wok, "staleness")
	}
}

func TestIngestOrderDoesNotChangeResults(t *testing.T) {
	rng := rand.New(rand.NewSource(2))
	n := 40
	values := make([]observation, n)
	for i := range values {
		values[i] = observation{int64(rng.Intn(201)), round3(rng.Float64() * 5)}
	}

	a := NewFeatureStore()
	for _, o := range values {
		a.Ingest(FeatureValue{"e", o.t, o.v})
	}
	b := NewFeatureStore()
	for i := n - 1; i >= 0; i-- {
		b.Ingest(FeatureValue{"e", values[i].t, values[i].v})
	}
	for et := int64(0); et < 210; et += 7 {
		av := a.GetPointInTime("e", et, nil)
		bv := b.GetPointInTime("e", et, nil)
		switch {
		case av == nil && bv == nil:
		case av != nil && bv != nil && *av == *bv:
		default:
			t.Fatalf("ingest order changed result at t=%d: %v vs %v", et, av, bv)
		}
	}
}

func TestTrainingSetIsRowWisePointInTimeCorrect(t *testing.T) {
	rng := rand.New(rand.NewSource(3))
	store := NewFeatureStore()
	perEntity := map[string][]observation{}
	for i := 0; i < 500; i++ {
		e := "user-" + itoa(rng.Intn(21))
		o := observation{int64(rng.Intn(1001)), round3(rng.Float64()*2 - 1)}
		store.Ingest(FeatureValue{e, o.t, o.v})
		perEntity[e] = append(perEntity[e], o)
	}
	spine := make([]SpineRow, 300)
	for i := range spine {
		spine[i] = SpineRow{"user-" + itoa(rng.Intn(21)), int64(rng.Intn(1001))}
	}
	col := store.GetTrainingSet(spine, nil)
	for i, r := range spine {
		wv, wok := oracle(perEntity[r.Entity], r.EventTime, nil)
		checkAgrees(t, col[i], wv, wok, "training-row")
	}
}

func TestNoValueBeforeFirstObservation(t *testing.T) {
	store := NewFeatureStore()
	store.Ingest(FeatureValue{"e", 100, 1.0})
	mustNil(t, store.GetPointInTime("e", 99, nil))
	mustValue(t, store.GetPointInTime("e", 100, nil), 1.0)
}

func TestUnknownEntityReturnsNoneFuzz(t *testing.T) {
	store := NewFeatureStore()
	store.Ingest(FeatureValue{"known", 10, 1.0})
	mustNil(t, store.GetPointInTime("unknown", 100, nil))
}

// itoa formats a small non-negative int without pulling in strconv at call sites.
func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var buf [12]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	return string(buf[i:])
}
