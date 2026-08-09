# FeatureVault

**A mini feature store that refuses to leak the future.**

The hardest, most valuable part of any ML feature store is the **point-in-time-correct join**: when you build a training set, every row must see only feature values that existed *at or before* that row's event time - never a value recorded afterward. Get it wrong and your offline metrics lie to you. FeatureVault implements that join correctly, in **Python, C#, and Java**.

## The problem: train/serve skew from data leakage

Say you're predicting churn. A training row for "user-1 on Jan 3" must use the user's feature values *as of Jan 3* - not the values that got written on Jan 10. Naive joins grab the latest value and leak future information, inflating offline accuracy and then failing in production. This bug is subtle, common, and expensive.

## What it does

- **As-of join** (`get_point_in_time`) - latest value with `timestamp <= event_time`, via binary search.
- **Training-set builder** - a point-in-time-correct feature column for a spine of `(entity, event_time)` rows.
- **Max staleness** - treat a feature as expired if the newest value is too old.
- **Online lookup** - latest value per entity for the serving path.

## The key guarantee (a real test)

```python
# ingested at t=100,200,300. At event t=150, only t=100 existed:
store.get_point_in_time("user-1", 150)  # -> 1.0, never 2.0 or 3.0
```

There's also a CLI that runs a point-in-time join over two JSONL files:

```bash
cd python
python src/cli.py sample-features.jsonl sample-spine.jsonl --max-staleness 200
```

## Three languages, one behavior

| Language | Tests | Run |
|----------|:-----:|-----|
| Python | 8 | `cd python && pytest -q` |
| C# (.NET 10) | 8 | `cd csharp && dotnet test` |
| Java (17+) | 8 | `cd java && mvn test` |

All three use the same binary-search as-of join, so they leak-check identically.

## Known limitations / next

- In-memory only; a Parquet/DuckDB offline store + Redis online store is the natural production split.
- Single feature per store instance - a multi-feature schema + feature-group joins would come next.
- Timestamps are integer epochs to stay language-portable.

## Design notes

- **[DESIGN.md](DESIGN.md)** - the one invariant (never leak the future) and how the
  sorted-timestamp + binary-search design makes it exact, the lazy-sort and tie-break
  decisions, and the non-goals. A leakage-fuzz suite cross-checks thousands of random
  scenarios against a brute-force oracle.

Part of [parag-labs](https://github.com/parag-labs) - small, focused tools for building AI systems you can trust.
