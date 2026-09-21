import { describe, expect, it } from "vitest";

import { FeatureStore } from "./store.ts";

const MASK = (1n << 64n) - 1n;

/**
 * A tiny dependency-free SplitMix64 PRNG so the fuzz suite is deterministic and
 * needs no external package. BigInt keeps the u64 arithmetic exact.
 */
class Rng {
  private state: bigint;

  constructor(seed: bigint) {
    this.state = seed & MASK;
  }

  nextU64(): bigint {
    this.state = (this.state + 0x9e3779b97f4a7c15n) & MASK;
    let z = this.state;
    z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK;
    z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK;
    return (z ^ (z >> 31n)) & MASK;
  }

  /** Returns an integer in `0..n`. */
  below(n: number): number {
    return Number(this.nextU64() % BigInt(n));
  }

  /** Returns a float in `[0, 1)`. */
  float(): number {
    return Number(this.nextU64() >> 11n) / 2 ** 53;
  }
}

// Round to 3 decimals and add 0 to normalise a possible -0 to +0.
function round3(x: number): number {
  return Math.round(x * 1000) / 1000 + 0;
}

/**
 * A brute-force point-in-time lookup used to cross-check the store. Mirrors the
 * store's tie-break: among values at-or-before the event time, the largest
 * `(timestamp, value)` wins.
 */
function oracle(values: Array<[number, number]>, eventTime: number, maxStaleness: number | null): number | null {
  let best: [number, number] | null = null;
  for (const [t, v] of values) {
    if (t > eventTime) {
      continue;
    }
    if (best === null || t > best[0] || (t === best[0] && v > best[1])) {
      best = [t, v];
    }
  }
  if (best === null) {
    return null;
  }
  if (maxStaleness !== null && eventTime - best[0] > maxStaleness) {
    return null;
  }
  return best[1];
}

describe("leakage fuzz", () => {
  it("never returns a value from the future", () => {
    const rng = new Rng(0n);
    for (let iter = 0; iter < 2000; iter++) {
      const n = rng.below(30) + 1;
      const values: Array<[number, number]> = [];
      for (let i = 0; i < n; i++) {
        values.push([rng.below(1001), round3(rng.float() * 200 - 100)]);
      }

      // Ingest in a shuffled order (Fisher-Yates) to prove order independence.
      const order = [...values.keys()];
      for (let i = order.length - 1; i > 0; i--) {
        const j = rng.below(i + 1);
        [order[i], order[j]] = [order[j], order[i]];
      }
      const s = new FeatureStore();
      for (const j of order) {
        s.ingest({ entity: "e", timestamp: values[j][0], value: values[j][1] });
      }

      for (let q = 0; q < 5; q++) {
        const eventTime = rng.below(1001);
        const got = s.getPointInTime("e", eventTime);
        expect(got).toBe(oracle(values, eventTime, null));
        if (got !== null) {
          expect(values.some(([t, v]) => t <= eventTime && v === got)).toBe(true);
        }
      }
    }
  });

  it("respects the staleness window under fuzz", () => {
    const rng = new Rng(1n);
    for (let iter = 0; iter < 2000; iter++) {
      const n = rng.below(20) + 1;
      const values: Array<[number, number]> = [];
      for (let i = 0; i < n; i++) {
        values.push([rng.below(501), round3(rng.float() * 10)]);
      }
      const s = new FeatureStore();
      for (const [t, v] of values) {
        s.ingest({ entity: "e", timestamp: t, value: v });
      }
      const eventTime = rng.below(601);
      const maxStaleness = rng.below(201);
      expect(s.getPointInTime("e", eventTime, maxStaleness)).toBe(oracle(values, eventTime, maxStaleness));
    }
  });

  it("is independent of ingest order", () => {
    const rng = new Rng(2n);
    const values: Array<[number, number]> = [];
    for (let i = 0; i < 40; i++) {
      values.push([rng.below(201), round3(rng.float() * 5)]);
    }

    const a = new FeatureStore();
    for (const [t, v] of values) {
      a.ingest({ entity: "e", timestamp: t, value: v });
    }
    const b = new FeatureStore();
    for (const [t, v] of [...values].reverse()) {
      b.ingest({ entity: "e", timestamp: t, value: v });
    }
    for (let et = 0; et < 210; et += 7) {
      expect(a.getPointInTime("e", et)).toBe(b.getPointInTime("e", et));
    }
  });

  it("builds a row-wise point-in-time-correct training set", () => {
    const rng = new Rng(3n);
    const s = new FeatureStore();
    const perEntity = new Map<string, Array<[number, number]>>();
    for (let i = 0; i < 500; i++) {
      const e = `user-${rng.below(21)}`;
      const t = rng.below(1001);
      const v = round3(rng.float() * 2 - 1);
      s.ingest({ entity: e, timestamp: t, value: v });
      const list = perEntity.get(e);
      if (list === undefined) {
        perEntity.set(e, [[t, v]]);
      } else {
        list.push([t, v]);
      }
    }
    const spine = Array.from({ length: 300 }, () => ({
      entity: `user-${rng.below(21)}`,
      eventTime: rng.below(1001),
    }));
    const column = s.getTrainingSet(spine);
    for (let i = 0; i < spine.length; i++) {
      expect(column[i]).toBe(oracle(perEntity.get(spine[i].entity) ?? [], spine[i].eventTime, null));
    }
  });

  it("returns nothing before the first observation", () => {
    const s = new FeatureStore();
    s.ingest({ entity: "e", timestamp: 100, value: 1.0 });
    expect(s.getPointInTime("e", 99)).toBeNull();
    expect(s.getPointInTime("e", 100)).toBe(1.0);
  });

  it("returns null for an unknown entity", () => {
    const s = new FeatureStore();
    s.ingest({ entity: "known", timestamp: 10, value: 1.0 });
    expect(s.getPointInTime("unknown", 100)).toBeNull();
  });
});
