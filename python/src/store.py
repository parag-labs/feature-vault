"""A minimal feature store with point-in-time-correct (as-of) joins.

The hard, valuable part of any feature store is avoiding *data leakage*: when you
build a training set, each row must see only feature values that existed at or
before that row's event time -- never a value recorded afterwards. Getting this
wrong silently inflates offline metrics and wrecks the model in production.

Timestamps are integers (e.g. epoch seconds) to keep the logic portable to C#/Java.
"""

from __future__ import annotations

from bisect import bisect_right
from dataclasses import dataclass


@dataclass(frozen=True)
class FeatureValue:
    entity: str
    timestamp: int
    value: float


@dataclass(frozen=True)
class SpineRow:
    entity: str
    event_time: int


class FeatureStore:
    """Stores time-ordered feature values per entity and serves as-of joins."""

    def __init__(self) -> None:
        # entity -> (sorted timestamps, aligned values)
        self._ts: dict[str, list[int]] = {}
        self._vals: dict[str, list[float]] = {}
        self._dirty: set[str] = set()

    def ingest(self, fv: FeatureValue) -> None:
        self._ts.setdefault(fv.entity, []).append(fv.timestamp)
        self._vals.setdefault(fv.entity, []).append(fv.value)
        self._dirty.add(fv.entity)

    def _ensure_sorted(self, entity: str) -> None:
        if entity in self._dirty:
            paired = sorted(zip(self._ts[entity], self._vals[entity]))
            self._ts[entity] = [t for t, _ in paired]
            self._vals[entity] = [v for _, v in paired]
            self._dirty.discard(entity)

    def get_point_in_time(self, entity: str, event_time: int, max_staleness: int | None = None) -> float | None:
        """Latest feature value with timestamp <= event_time (as-of join, backward).

        Returns None if no such value exists, or if the newest value is older than
        `max_staleness` (feature considered expired).
        """
        if entity not in self._ts:
            return None
        self._ensure_sorted(entity)
        ts = self._ts[entity]
        idx = bisect_right(ts, event_time) - 1
        if idx < 0:
            return None
        if max_staleness is not None and event_time - ts[idx] > max_staleness:
            return None
        return self._vals[entity][idx]

    def get_training_set(self, spine: list[SpineRow], max_staleness: int | None = None) -> list[float | None]:
        """Point-in-time-correct feature column for a set of spine rows."""
        return [self.get_point_in_time(r.entity, r.event_time, max_staleness) for r in spine]

    def get_online(self, entity: str) -> float | None:
        """Latest known value for an entity (serving path)."""
        if entity not in self._ts:
            return None
        self._ensure_sorted(entity)
        return self._vals[entity][-1] if self._vals[entity] else None
