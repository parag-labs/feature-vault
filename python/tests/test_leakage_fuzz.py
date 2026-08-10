"""Leakage fuzz: the point-in-time join must NEVER return a value from the future.

This is the one property a feature store cannot get wrong. If an as-of join ever
returns a feature value whose timestamp is after the row's event time, offline metrics
inflate and the model breaks in production. This suite throws thousands of randomized
ingest orders, event times, and staleness windows at the store and cross-checks every
answer against a brute-force oracle - the value the join returns must be exactly the
newest value at-or-before the event time, or nothing.
"""

from __future__ import annotations

import random

from store import FeatureStore, FeatureValue, SpineRow


def _oracle(values: list[tuple[int, float]], event_time: int, max_staleness: int | None) -> float | None:
    """Brute-force point-in-time lookup to check the store against.

    Mirrors the store's deterministic tie-break: it stores values sorted by
    (timestamp, value) and returns the last one at-or-before the event time, so among
    equal timestamps the largest value wins. We replicate that exactly here - the
    point of the check is "never a *future* value," and for same-timestamp ties both
    sides must agree on which present value is returned.
    """
    eligible = sorted((t, v) for t, v in values if t <= event_time)
    if not eligible:
        return None
    t, v = eligible[-1]
    if max_staleness is not None and event_time - t > max_staleness:
        return None
    return v


def test_join_never_returns_a_future_value():
    rng = random.Random(0)
    for _ in range(2000):
        n = rng.randint(1, 30)
        # Random (timestamp, value) pairs, ingested in random order.
        values = [(rng.randint(0, 1000), round(rng.uniform(-100, 100), 3)) for _ in range(n)]
        store = FeatureStore()
        for t, v in sorted(values, key=lambda _p: rng.random()):  # shuffle ingest order
            store.ingest(FeatureValue("e", t, v))

        for _ in range(5):
            event_time = rng.randint(0, 1000)
            got = store.get_point_in_time("e", event_time)
            expected = _oracle(values, event_time, None)
            assert got == expected, f"leak/mismatch at t={event_time}: got {got}, want {expected}"
            # The strong invariant: whatever value came back must correspond to a
            # timestamp at or before the event time.
            if got is not None:
                assert any(t <= event_time and v == got for t, v in values)


def test_staleness_window_is_respected_under_fuzz():
    rng = random.Random(1)
    for _ in range(2000):
        n = rng.randint(1, 20)
        values = [(rng.randint(0, 500), round(rng.uniform(0, 10), 3)) for _ in range(n)]
        store = FeatureStore()
        for t, v in values:
            store.ingest(FeatureValue("e", t, v))

        event_time = rng.randint(0, 600)
        max_staleness = rng.randint(0, 200)
        got = store.get_point_in_time("e", event_time, max_staleness)
        expected = _oracle(values, event_time, max_staleness)
        assert got == expected


def test_ingest_order_does_not_change_results():
    rng = random.Random(2)
    values = [(rng.randint(0, 200), round(rng.uniform(0, 5), 3)) for _ in range(40)]

    a = FeatureStore()
    for t, v in values:
        a.ingest(FeatureValue("e", t, v))

    b = FeatureStore()
    for t, v in sorted(values, reverse=True):
        b.ingest(FeatureValue("e", t, v))

    for et in range(0, 210, 7):
        assert a.get_point_in_time("e", et) == b.get_point_in_time("e", et)


def test_training_set_is_row_wise_point_in_time_correct():
    rng = random.Random(3)
    store = FeatureStore()
    per_entity: dict[str, list[tuple[int, float]]] = {}
    for _ in range(500):
        e = f"user-{rng.randint(0, 20)}"
        t = rng.randint(0, 1000)
        v = round(rng.uniform(-1, 1), 4)
        store.ingest(FeatureValue(e, t, v))
        per_entity.setdefault(e, []).append((t, v))

    spine = [SpineRow(f"user-{rng.randint(0, 20)}", rng.randint(0, 1000)) for _ in range(300)]
    column = store.get_training_set(spine)
    for row, got in zip(spine, column):
        expected = _oracle(per_entity.get(row.entity, []), row.event_time, None)
        assert got == expected


def test_no_value_before_the_first_observation():
    store = FeatureStore()
    store.ingest(FeatureValue("e", 100, 1.0))
    # An event strictly before the first observation must see nothing - not the
    # future value at t=100.
    assert store.get_point_in_time("e", 99) is None
    assert store.get_point_in_time("e", 100) == 1.0


def test_unknown_entity_returns_none():
    store = FeatureStore()
    store.ingest(FeatureValue("known", 10, 1.0))
    assert store.get_point_in_time("unknown", 100) is None
