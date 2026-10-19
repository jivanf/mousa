#!/usr/bin/env python3
"""Summarize Mousa's bounded local playback logs; uses only the Python standard library."""
import argparse
import collections
import json
import math
import os
from pathlib import Path
import statistics


def main():
    default = Path(os.environ.get("MOUSA_DATA_DIR") or
                   Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share")) / "mousa")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=default)
    args = parser.parse_args()
    events = []
    for name in ("playback.jsonl.1", "playback.jsonl"):
        path = args.directory / name
        if not path.exists():
            continue
        for line in path.read_text().splitlines():
            try:
                value = json.loads(line)
                if isinstance(value, dict):
                    events.append(value)
            except json.JSONDecodeError:
                pass  # A running writer may have an incomplete final line.
    groups = collections.defaultdict(list)
    for event in events:
        if (event.get("event") == "playback_started" and event.get("origin") == "automatic"
                and not event.get("interrupted")):
            groups[event["preparation"]].append(event)
    if not groups:
        print("No uninterrupted automatic transitions recorded yet. Let a few tracks finish normally.")
        return
    print("Automatic transitions (milliseconds; excludes user pauses and seeks during startup):")
    for status, rows in sorted(groups.items()):
        values = sorted(row["first_progress_ms"] for row in rows)
        p95 = values[math.ceil(len(values) * .95) - 1]
        buffering = sum(row.get("startup_buffering_ms", 0) > 0 for row in rows)
        print(f"  {status}: n={len(rows)}, median={statistics.median(values):.0f}, "
              f"p95={p95:.0f}, startup cache waits={buffering}")
    preparation = collections.Counter(event.get("outcome") for event in events
                                      if event.get("event") == "prepare_finished")
    print("URL preparation outcomes:", dict(preparation))
    summaries = [event for event in events if event.get("event") == "playback_summary"]
    print(f"Playback stalls in {len(summaries)} completed playback attempts: "
          f"{sum(e.get('stall_count', 0) for e in summaries)}; "
          f"{sum(e.get('stall_ms', 0) for e in summaries)} ms total")
    print("Timing approximates audible gaps. Compare ready transitions with other outcomes; "
          "persistent delays plus cache waits support testing audio prebuffering.")
    print("Small samples and different tracks/network conditions do not establish causation.")


if __name__ == "__main__":
    main()
