"""Command line demo for FeatureVault's point-in-time join.

Feed it a file of feature values and a file of spine rows (the things you want to
train on), and it prints the feature value each row should see -- the one that
existed at that row's event time, never a later one.

Both files are JSONL:
  features.jsonl : {"entity": "...", "timestamp": 123, "value": 1.0}
  spine.jsonl    : {"entity": "...", "event_time": 150}

    python -m src.cli features.jsonl spine.jsonl --max-staleness 100
"""

from __future__ import annotations

import argparse
import json
import sys

from store import FeatureStore, FeatureValue, SpineRow


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="featurevault")
    parser.add_argument("features", help="feature values, JSONL")
    parser.add_argument("spine", help="spine rows, JSONL")
    parser.add_argument("--max-staleness", type=int, default=None)
    args = parser.parse_args(argv)

    store = FeatureStore()
    with open(args.features, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                d = json.loads(line)
                store.ingest(FeatureValue(d["entity"], int(d["timestamp"]), float(d["value"])))

    spine = []
    with open(args.spine, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                d = json.loads(line)
                spine.append(SpineRow(d["entity"], int(d["event_time"])))

    values = store.get_training_set(spine, args.max_staleness)
    print("entity           event_time   feature")
    for row, val in zip(spine, values):
        shown = "None" if val is None else f"{val}"
        print(f"{row.entity:<16} {row.event_time:<12} {shown}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
