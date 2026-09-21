import { describe, expect, it } from "vitest";

import { FeatureStore } from "./store.ts";

function store(): FeatureStore {
  const s = new FeatureStore();
  for (const [t, v] of [
    [100, 1.0],
    [200, 2.0],
    [300, 3.0],
  ] as const) {
    s.ingest({ entity: "user-1", timestamp: t, value: v });
  }
  return s;
}

describe("FeatureStore as-of join", () => {
  it("picks the latest value at or before the event time", () => {
    const s = store();
    expect(s.getPointInTime("user-1", 250)).toBe(2.0); // latest <= 250 is ts=200
    expect(s.getPointInTime("user-1", 300)).toBe(3.0); // inclusive of event time
    expect(s.getPointInTime("user-1", 100)).toBe(1.0);
  });

  it("returns null before the first value exists", () => {
    expect(store().getPointInTime("user-1", 50)).toBeNull();
  });

  it("never leaks a value recorded after the event", () => {
    // At event 150 only the ts=100 value existed; ts=200/300 are the future.
    expect(store().getPointInTime("user-1", 150)).toBe(1.0);
  });

  it("expires features older than max staleness", () => {
    const s = store();
    expect(s.getPointInTime("user-1", 1000, 100)).toBeNull();
    expect(s.getPointInTime("user-1", 350, 100)).toBe(3.0);
  });

  it("treats the max-staleness boundary as inclusive", () => {
    const s = store();
    // event 400, ts=300 -> exactly 100 old; 100 is not "> 100", so kept.
    expect(s.getPointInTime("user-1", 400, 100)).toBe(3.0);
    // One unit older tips past the boundary and expires it.
    expect(s.getPointInTime("user-1", 401, 100)).toBeNull();
  });

  it("returns null for an unknown entity", () => {
    const s = store();
    expect(s.getPointInTime("ghost", 200)).toBeNull();
    expect(s.getOnline("ghost")).toBeNull();
  });
});

describe("FeatureStore training set", () => {
  it("is point-in-time correct across a spine", () => {
    const s = store();
    const spine = [
      { entity: "user-1", eventTime: 120 },
      { entity: "user-1", eventTime: 220 },
      { entity: "user-1", eventTime: 320 },
    ];
    expect(s.getTrainingSet(spine)).toEqual([1.0, 2.0, 3.0]);
  });

  it("leaves rows with no available value as null", () => {
    const s = store();
    const spine = [
      { entity: "user-1", eventTime: 50 },
      { entity: "user-1", eventTime: 250 },
      { entity: "ghost", eventTime: 999 },
    ];
    expect(s.getTrainingSet(spine)).toEqual([null, 2.0, null]);
  });

  it("returns an empty column for an empty spine", () => {
    expect(store().getTrainingSet([])).toEqual([]);
  });
});

describe("FeatureStore online lookup", () => {
  it("returns the latest value for an entity", () => {
    expect(store().getOnline("user-1")).toBe(3.0);
  });
});

describe("FeatureStore ordering", () => {
  it("handles out-of-order ingest", () => {
    const s = new FeatureStore();
    for (const [t, v] of [
      [300, 3.0],
      [100, 1.0],
      [200, 2.0],
    ] as const) {
      s.ingest({ entity: "e", timestamp: t, value: v });
    }
    expect(s.getPointInTime("e", 250)).toBe(2.0);
    expect(s.getOnline("e")).toBe(3.0);
  });

  it("serves the largest value when timestamps tie", () => {
    // The documented tie-break: values are ordered by (timestamp, value), so
    // among equal timestamps the largest value is served.
    const s = new FeatureStore();
    for (const [t, v] of [
      [200, 5.0],
      [200, 2.0],
      [200, 9.0],
      [100, 1.0],
    ] as const) {
      s.ingest({ entity: "e", timestamp: t, value: v });
    }
    expect(s.getPointInTime("e", 250)).toBe(9.0);
    expect(s.getOnline("e")).toBe(9.0);
    expect(s.getPointInTime("e", 150)).toBe(1.0);
  });
});

describe("FeatureStore edge cases", () => {
  it("returns null everywhere on an empty store", () => {
    const s = new FeatureStore();
    expect(s.getPointInTime("nobody", 10)).toBeNull();
    expect(s.getOnline("nobody")).toBeNull();
  });

  it("respects single-observation boundaries", () => {
    const s = new FeatureStore();
    s.ingest({ entity: "e", timestamp: 100, value: 1.0 });
    expect(s.getPointInTime("e", 99)).toBeNull(); // strictly before -> nothing
    expect(s.getPointInTime("e", 100)).toBe(1.0);
    expect(s.getPointInTime("e", 101)).toBe(1.0);
  });

  it("serves negative and zero values", () => {
    const s = new FeatureStore();
    s.ingest({ entity: "e", timestamp: 10, value: 0.0 });
    s.ingest({ entity: "e", timestamp: 20, value: -3.5 });
    expect(s.getPointInTime("e", 15)).toBe(0.0);
    expect(s.getPointInTime("e", 25)).toBe(-3.5);
    expect(s.getOnline("e")).toBe(-3.5);
  });
});
