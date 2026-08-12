namespace FeatureVault;

public readonly record struct FeatureValue(string Entity, long Timestamp, double Value);

public readonly record struct SpineRow(string Entity, long EventTime);

/// <summary>
/// Minimal feature store with point-in-time-correct (as-of) joins.
/// The point of the whole thing: never leak a feature value recorded after a
/// training row's event time.
/// </summary>
public sealed class FeatureStore
{
    private readonly Dictionary<string, List<long>> _ts = new();
    private readonly Dictionary<string, List<double>> _vals = new();
    private readonly HashSet<string> _dirty = [];

    public void Ingest(FeatureValue fv)
    {
        if (!_ts.TryGetValue(fv.Entity, out var tsList))
        {
            tsList = [];
            _ts[fv.Entity] = tsList;
            _vals[fv.Entity] = [];
        }

        tsList.Add(fv.Timestamp);
        _vals[fv.Entity].Add(fv.Value);
        _dirty.Add(fv.Entity);
    }

    public double? GetPointInTime(string entity, long eventTime, long? maxStaleness = null)
    {
        if (!_ts.ContainsKey(entity))
        {
            return null;
        }

        EnsureSorted(entity);
        var ts = _ts[entity];
        var idx = UpperBound(ts, eventTime) - 1;
        if (idx < 0)
        {
            return null;
        }

        if (maxStaleness is not null && eventTime - ts[idx] > maxStaleness)
        {
            return null;
        }

        return _vals[entity][idx];
    }

    public List<double?> GetTrainingSet(IEnumerable<SpineRow> spine, long? maxStaleness = null)
        => spine.Select(r => GetPointInTime(r.Entity, r.EventTime, maxStaleness)).ToList();

    public double? GetOnline(string entity)
    {
        if (!_ts.ContainsKey(entity))
        {
            return null;
        }

        EnsureSorted(entity);
        var vals = _vals[entity];
        return vals.Count == 0 ? null : vals[^1];
    }

    private void EnsureSorted(string entity)
    {
        if (!_dirty.Contains(entity))
        {
            return;
        }

        var paired = _ts[entity].Zip(_vals[entity]).OrderBy(p => p.First).ToList();
        _ts[entity] = paired.Select(p => p.First).ToList();
        _vals[entity] = paired.Select(p => p.Second).ToList();
        _dirty.Remove(entity);
    }

    // First index with ts[i] > target (std::upper_bound).
    private static int UpperBound(List<long> ts, long target)
    {
        int lo = 0, hi = ts.Count;
        while (lo < hi)
        {
            var mid = (lo + hi) / 2;
            if (ts[mid] <= target)
            {
                lo = mid + 1;
            }
            else
            {
                hi = mid;
            }
        }

        return lo;
    }
}
