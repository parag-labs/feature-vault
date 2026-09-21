/**
 * A minimal feature store with point-in-time-correct (as-of) joins.
 *
 * The hard, valuable part of any feature store is avoiding data leakage: when
 * you build a training set, each row must see only feature values that existed
 * at or before that row's event time -- never a value recorded afterwards.
 * Getting this wrong silently inflates offline metrics and wrecks the model in
 * production.
 *
 * Timestamps are integers (e.g. epoch seconds) to keep the logic portable.
 */

/** A single observed feature value for an entity at a timestamp. */
export interface FeatureValue {
  entity: string;
  timestamp: number;
  value: number;
}

/**
 * One row of a training spine: an entity paired with the event time whose
 * point-in-time feature value is wanted.
 */
export interface SpineRow {
  entity: string;
  eventTime: number;
}

/** Stores time-ordered feature values per entity and serves as-of joins. */
export class FeatureStore {
  private readonly ts = new Map<string, number[]>();
  private readonly vals = new Map<string, number[]>();
  private readonly dirty = new Set<string>();

  /**
   * Records a feature value. Values may arrive in any timestamp order; each
   * entity's observations are sorted lazily on the first read.
   */
  ingest(fv: FeatureValue): void {
    const times = this.ts.get(fv.entity);
    if (times === undefined) {
      this.ts.set(fv.entity, [fv.timestamp]);
      this.vals.set(fv.entity, [fv.value]);
    } else {
      times.push(fv.timestamp);
      this.vals.get(fv.entity)!.push(fv.value);
    }
    this.dirty.add(fv.entity);
  }

  /**
   * Returns the latest feature value with `timestamp <= eventTime` (a backward
   * as-of join), or `null` if no such value exists.
   *
   * If `maxStaleness` is non-null and the newest eligible value is older than
   * it, the feature is treated as expired and `null` is returned.
   */
  getPointInTime(entity: string, eventTime: number, maxStaleness: number | null = null): number | null {
    if (!this.ts.has(entity)) {
      return null;
    }
    this.ensureSorted(entity);
    const times = this.ts.get(entity)!;
    const idx = upperBound(times, eventTime) - 1;
    if (idx < 0) {
      return null;
    }
    if (maxStaleness !== null && eventTime - times[idx] > maxStaleness) {
      return null;
    }
    return this.vals.get(entity)![idx];
  }

  /**
   * Returns a point-in-time-correct feature column, one entry per spine row
   * (`null` where no value is available).
   */
  getTrainingSet(spine: SpineRow[], maxStaleness: number | null = null): (number | null)[] {
    return spine.map((r) => this.getPointInTime(r.entity, r.eventTime, maxStaleness));
  }

  /**
   * Returns the latest known value for an entity (the serving path), or `null`
   * if the entity is unknown.
   */
  getOnline(entity: string): number | null {
    if (!this.ts.has(entity)) {
      return null;
    }
    this.ensureSorted(entity);
    const values = this.vals.get(entity)!;
    return values.length === 0 ? null : values[values.length - 1];
  }

  /**
   * Orders an entity's observations by `(timestamp, value)` so that among
   * values sharing a timestamp the largest value sorts last. This makes the
   * as-of tie-break deterministic and independent of ingest order.
   */
  private ensureSorted(entity: string): void {
    if (!this.dirty.has(entity)) {
      return;
    }
    const times = this.ts.get(entity)!;
    const values = this.vals.get(entity)!;
    const paired = times.map((t, i) => ({ t, v: values[i] }));
    paired.sort((a, b) => a.t - b.t || a.v - b.v);
    this.ts.set(entity, paired.map((p) => p.t));
    this.vals.set(entity, paired.map((p) => p.v));
    this.dirty.delete(entity);
  }
}

/** Returns the first index `i` where `ts[i] > target` (std::upper_bound). */
function upperBound(ts: number[], target: number): number {
  let lo = 0;
  let hi = ts.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (ts[mid] <= target) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  return lo;
}
