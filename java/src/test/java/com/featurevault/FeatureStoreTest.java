package com.featurevault;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import java.util.Arrays;
import java.util.List;

import org.junit.jupiter.api.Test;

import com.featurevault.FeatureStore.FeatureValue;
import com.featurevault.FeatureStore.SpineRow;

class FeatureStoreTest {

    private static FeatureStore store() {
        FeatureStore s = new FeatureStore();
        s.ingest(new FeatureValue("user-1", 100, 1.0));
        s.ingest(new FeatureValue("user-1", 200, 2.0));
        s.ingest(new FeatureValue("user-1", 300, 3.0));
        return s;
    }

    @Test
    void asOfJoinPicksLatestBeforeEvent() {
        FeatureStore s = store();
        assertEquals(2.0, s.getPointInTime("user-1", 250, null));
        assertEquals(3.0, s.getPointInTime("user-1", 300, null));
        assertEquals(1.0, s.getPointInTime("user-1", 100, null));
    }

    @Test
    void noLeakageBeforeFirstValue() {
        assertNull(store().getPointInTime("user-1", 50, null));
    }

    @Test
    void noFutureLeak() {
        assertEquals(1.0, store().getPointInTime("user-1", 150, null));
    }

    @Test
    void maxStalenessExpiresOldFeatures() {
        FeatureStore s = store();
        assertNull(s.getPointInTime("user-1", 1000, 100L));
        assertEquals(3.0, s.getPointInTime("user-1", 350, 100L));
    }

    @Test
    void unknownEntityReturnsNull() {
        FeatureStore s = store();
        assertNull(s.getPointInTime("ghost", 200, null));
        assertNull(s.getOnline("ghost"));
    }

    @Test
    void trainingSetIsPointInTimeCorrect() {
        FeatureStore s = store();
        List<SpineRow> spine = List.of(
                new SpineRow("user-1", 120),
                new SpineRow("user-1", 220),
                new SpineRow("user-1", 320));
        assertEquals(Arrays.asList(1.0, 2.0, 3.0), s.getTrainingSet(spine, null));
    }

    @Test
    void onlineReturnsLatest() {
        assertEquals(3.0, store().getOnline("user-1"));
    }

    @Test
    void outOfOrderIngestIsHandled() {
        FeatureStore s = new FeatureStore();
        s.ingest(new FeatureValue("e", 300, 3.0));
        s.ingest(new FeatureValue("e", 100, 1.0));
        s.ingest(new FeatureValue("e", 200, 2.0));
        assertEquals(2.0, s.getPointInTime("e", 250, null));
        assertEquals(3.0, s.getOnline("e"));
    }
}
