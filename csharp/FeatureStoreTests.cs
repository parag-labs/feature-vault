using Xunit;

namespace FeatureVault.Tests;

public class FeatureStoreTests
{
    private static FeatureStore Store()
    {
        var s = new FeatureStore();
        foreach (var (ts, val) in new[] { (100L, 1.0), (200L, 2.0), (300L, 3.0) })
        {
            s.Ingest(new FeatureValue("user-1", ts, val));
        }

        return s;
    }

    [Fact]
    public void AsOfJoinPicksLatestBeforeEvent()
    {
        var s = Store();
        Assert.Equal(2.0, s.GetPointInTime("user-1", 250));
        Assert.Equal(3.0, s.GetPointInTime("user-1", 300));  // inclusive
        Assert.Equal(1.0, s.GetPointInTime("user-1", 100));
    }

    [Fact]
    public void NoLeakageBeforeFirstValue()
    {
        Assert.Null(Store().GetPointInTime("user-1", 50));
    }

    [Fact]
    public void NoFutureLeak()
    {
        // At event 150 only ts=100 existed; ts=200/300 are the future.
        Assert.Equal(1.0, Store().GetPointInTime("user-1", 150));
    }

    [Fact]
    public void MaxStalenessExpiresOldFeatures()
    {
        var s = Store();
        Assert.Null(s.GetPointInTime("user-1", 1000, maxStaleness: 100));
        Assert.Equal(3.0, s.GetPointInTime("user-1", 350, maxStaleness: 100));
    }

    [Fact]
    public void UnknownEntityReturnsNull()
    {
        var s = Store();
        Assert.Null(s.GetPointInTime("ghost", 200));
        Assert.Null(s.GetOnline("ghost"));
    }

    [Fact]
    public void TrainingSetIsPointInTimeCorrect()
    {
        var s = Store();
        var spine = new[]
        {
            new SpineRow("user-1", 120),
            new SpineRow("user-1", 220),
            new SpineRow("user-1", 320),
        };
        Assert.Equal(new double?[] { 1.0, 2.0, 3.0 }, s.GetTrainingSet(spine));
    }

    [Fact]
    public void OnlineReturnsLatest()
    {
        Assert.Equal(3.0, Store().GetOnline("user-1"));
    }

    [Fact]
    public void OutOfOrderIngestIsHandled()
    {
        var s = new FeatureStore();
        foreach (var (ts, val) in new[] { (300L, 3.0), (100L, 1.0), (200L, 2.0) })
        {
            s.Ingest(new FeatureValue("e", ts, val));
        }

        Assert.Equal(2.0, s.GetPointInTime("e", 250));
        Assert.Equal(3.0, s.GetOnline("e"));
    }
}
