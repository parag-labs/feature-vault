package com.featurevault;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Minimal feature store with point-in-time-correct (as-of) joins.
 *
 * <p>The whole point: never leak a feature value recorded after a training row's
 * event time. Timestamps are longs (e.g. epoch seconds).
 */
public final class FeatureStore {

    public record FeatureValue(String entity, long timestamp, double value) {
    }

    public record SpineRow(String entity, long eventTime) {
    }

    private final Map<String, List<Long>> ts = new HashMap<>();
    private final Map<String, List<Double>> vals = new HashMap<>();
    private final Set<String> dirty = new HashSet<>();

    public void ingest(FeatureValue fv) {
        ts.computeIfAbsent(fv.entity(), k -> new ArrayList<>()).add(fv.timestamp());
        vals.computeIfAbsent(fv.entity(), k -> new ArrayList<>()).add(fv.value());
        dirty.add(fv.entity());
    }

    /** Latest value with timestamp &lt;= eventTime, or null. Honors max staleness. */
    public Double getPointInTime(String entity, long eventTime, Long maxStaleness) {
        if (!ts.containsKey(entity)) {
            return null;
        }
        ensureSorted(entity);
        List<Long> times = ts.get(entity);
        int idx = upperBound(times, eventTime) - 1;
        if (idx < 0) {
            return null;
        }
        if (maxStaleness != null && eventTime - times.get(idx) > maxStaleness) {
            return null;
        }
        return vals.get(entity).get(idx);
    }

    public List<Double> getTrainingSet(List<SpineRow> spine, Long maxStaleness) {
        List<Double> out = new ArrayList<>(spine.size());
        for (SpineRow r : spine) {
            out.add(getPointInTime(r.entity(), r.eventTime(), maxStaleness));
        }
        return out;
    }

    public Double getOnline(String entity) {
        if (!ts.containsKey(entity)) {
            return null;
        }
        ensureSorted(entity);
        List<Double> v = vals.get(entity);
        return v.isEmpty() ? null : v.get(v.size() - 1);
    }

    private void ensureSorted(String entity) {
        if (!dirty.contains(entity)) {
            return;
        }
        List<Long> times = ts.get(entity);
        List<Double> values = vals.get(entity);
        Integer[] order = new Integer[times.size()];
        for (int i = 0; i < order.length; i++) {
            order[i] = i;
        }
        java.util.Arrays.sort(order, (x, y) -> Long.compare(times.get(x), times.get(y)));
        List<Long> newTs = new ArrayList<>(times.size());
        List<Double> newVals = new ArrayList<>(values.size());
        for (int i : order) {
            newTs.add(times.get(i));
            newVals.add(values.get(i));
        }
        ts.put(entity, newTs);
        vals.put(entity, newVals);
        dirty.remove(entity);
    }

    // First index with times[i] > target (upper_bound).
    private static int upperBound(List<Long> times, long target) {
        int lo = 0;
        int hi = times.size();
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            if (times.get(mid) <= target) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        return lo;
    }
}
