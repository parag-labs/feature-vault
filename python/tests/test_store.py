"""FeatureVault tests: the point-in-time join must never leak future values."""

from store import FeatureStore, FeatureValue, SpineRow


def _store() -> FeatureStore:
    s = FeatureStore()
    for ts, val in [(100, 1.0), (200, 2.0), (300, 3.0)]:
        s.ingest(FeatureValue("user-1", ts, val))
    return s


def test_as_of_join_picks_latest_before_event():
    s = _store()
    assert s.get_point_in_time("user-1", 250) == 2.0   # latest <= 250 is ts=200
    assert s.get_point_in_time("user-1", 300) == 3.0   # inclusive of event_time
    assert s.get_point_in_time("user-1", 100) == 1.0


def test_no_leakage_before_first_value():
    s = _store()
    assert s.get_point_in_time("user-1", 50) is None   # no value existed yet


def test_no_future_leak():
    """A value recorded AFTER the event must never be used."""
    s = _store()
    # At event_time 150, only the ts=100 value existed. ts=200/300 are the future.
    assert s.get_point_in_time("user-1", 150) == 1.0


def test_max_staleness_expires_old_features():
    s = _store()
    # Newest value at event 1000 is ts=300 -> 700 old. With max_staleness 100, expired.
    assert s.get_point_in_time("user-1", 1000, max_staleness=100) is None
    assert s.get_point_in_time("user-1", 350, max_staleness=100) == 3.0


def test_unknown_entity_returns_none():
    s = _store()
    assert s.get_point_in_time("ghost", 200) is None
    assert s.get_online("ghost") is None


def test_training_set_is_point_in_time_correct():
    s = _store()
    spine = [SpineRow("user-1", 120), SpineRow("user-1", 220), SpineRow("user-1", 320)]
    assert s.get_training_set(spine) == [1.0, 2.0, 3.0]


def test_online_returns_latest():
    s = _store()
    assert s.get_online("user-1") == 3.0


def test_out_of_order_ingest_is_handled():
    s = FeatureStore()
    for ts, val in [(300, 3.0), (100, 1.0), (200, 2.0)]:  # unsorted
        s.ingest(FeatureValue("e", ts, val))
    assert s.get_point_in_time("e", 250) == 2.0
    assert s.get_online("e") == 3.0
