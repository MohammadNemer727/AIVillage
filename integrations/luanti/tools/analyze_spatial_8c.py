"""Applies the frozen 8C decision rules to a completed run directory.

Usage: analyze_spatial_8c.py <repo> <run-directory> <open-seeds-comma> <wall-seeds-comma>

Thresholds are copied from docs/spatial-8c-preregistration.json and must not change after
outcomes exist.
"""

import hashlib
import json
import math
import pathlib
import statistics
import sys

ARMS = {"B": "off", "S": "on", "X": "scrambled"}
DECISIONS = 900
FED_MARGIN_FRACTION = 0.10
MIN_POSITIVE_PAIRS_FRACTION = 0.75
SHAM_MARGIN_FRACTION = 0.05
MAX_MEDIAN_DEAD_RECKONING_ERROR_BLOCKS = 3.0
MIN_ENTRIES_MARGIN = 6


def load_arm(root, variant, seed, arm):
    directory = root / f"{variant}-seed-{seed}-{arm}"
    result = json.loads((directory / "spatial-result.json").read_text())
    replay_log = directory / "replay.log"
    result["replayed_exactly"] = replay_log.exists() and "exactly" in replay_log.read_text()
    path = result["path_milli"]
    start = result["start_milli"]
    rows = [json.loads(line) for line in (directory / "bridge.ndjson").read_text().splitlines()
            if line]
    errors, steering, places = [], 0, 0
    for index, row in enumerate(rows[: len(path)]):
        spatial = row["response"]["diagnostics"].get("spatial", {})
        errors.append(math.hypot(spatial.get("x", 0) - (path[index][0] - start["x"]),
                                 spatial.get("y", 0) - (path[index][1] - start["z"])))
        steering += bool(spatial.get("steering"))
        places = max(places, spatial.get("places", 0))
    result["dead_reckoning_error_blocks_median"] = (
        statistics.median(errors) / 1000 if errors else None)
    result["steering_decisions"] = steering
    result["max_places"] = places
    result["max_distance_from_start_blocks"] = max(
        (math.hypot(x - start["x"], z - start["z"]) for x, z in path), default=0) / 1000
    result["trace_sha256"] = hashlib.sha256((directory / "bridge.ndjson").read_bytes()).hexdigest()
    del result["path_milli"]
    return result


def load(root, variant, seeds, arms):
    pairs = []
    for seed in seeds:
        pairs.append({"seed": seed, **{name: load_arm(root, variant, seed, ARMS[name])
                                       for name in arms}})
    return pairs


def total(pairs, arm, key):
    return sum(pair[arm][key] for pair in pairs)


def main():
    repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    open_seeds = [int(seed) for seed in sys.argv[3].split(",")]
    wall_seeds = [int(seed) for seed in sys.argv[4].split(",")]
    open_pairs = load(root, "open", open_seeds, "BSX")
    wall_pairs = load(root, "wall", wall_seeds, "BS")

    replay_exact = all(pair[arm]["replayed_exactly"]
                       for pairs in (open_pairs, wall_pairs) for pair in pairs
                       for arm in pair if arm != "seed")
    complete = all(pair[arm]["decisions"] == DECISIONS
                   for pairs in (open_pairs, wall_pairs) for pair in pairs
                   for arm in pair if arm != "seed")
    navigation_errors = [pair["S"]["dead_reckoning_error_blocks_median"] for pair in open_pairs]
    median_error = statistics.median(navigation_errors)
    valid = replay_exact and complete and median_error <= MAX_MEDIAN_DEAD_RECKONING_ERROR_BLOCKS

    all_decisions = DECISIONS * len(open_pairs)
    fed = {arm: total(open_pairs, arm, "fed_decisions") for arm in "BSX"}
    entries = {arm: total(open_pairs, arm, "relief_entries") for arm in "BSX"}
    harm = {arm: total(open_pairs, arm, "harm_entries") for arm in "BSX"}
    deltas_sb = [pair["S"]["fed_decisions"] - pair["B"]["fed_decisions"] for pair in open_pairs]
    deltas_sx = [pair["S"]["fed_decisions"] - pair["X"]["fed_decisions"] for pair in open_pairs]
    needed_pairs = math.ceil(MIN_POSITIVE_PAIRS_FRACTION * len(open_pairs))
    primary = {
        "fed_decisions": fed,
        "relief_entries": entries,
        "harm_entries": harm,
        "fed_deltas_S_minus_B": deltas_sb,
        "positive_pairs": sum(d > 0 for d in deltas_sb),
        "negative_pairs": sum(d < 0 for d in deltas_sb),
        "median_dead_reckoning_error_blocks": median_error,
        "steering_decisions": total(open_pairs, "S", "steering_decisions"),
    }
    primary["passes"] = (
        fed["S"] - fed["B"] >= FED_MARGIN_FRACTION * all_decisions
        and entries["S"] - entries["B"] >= MIN_ENTRIES_MARGIN
        and primary["positive_pairs"] >= needed_pairs
        and primary["negative_pairs"] == 0
        and harm["S"] <= harm["B"]
        and primary["steering_decisions"] > 0
    )
    sham = {
        "fed_deltas_S_minus_X": deltas_sx,
        "positive_pairs": sum(d > 0 for d in deltas_sx),
        "negative_pairs": sum(d < 0 for d in deltas_sx),
    }
    sham["passes"] = (
        fed["S"] - fed["X"] >= SHAM_MARGIN_FRACTION * all_decisions
        and sham["positive_pairs"] >= needed_pairs
        and sham["negative_pairs"] == 0
    )

    wall = {
        "fed_decisions": {arm: total(wall_pairs, arm, "fed_decisions") for arm in "BS"},
        "relief_entries": {arm: total(wall_pairs, arm, "relief_entries") for arm in "BS"},
        "fed_deltas_S_minus_B": [pair["S"]["fed_decisions"] - pair["B"]["fed_decisions"]
                                 for pair in wall_pairs],
    }

    accepted = valid and primary["passes"] and sham["passes"]
    preregistration = repo / "docs/spatial-8c-preregistration.json"
    report = {
        "schema_version": 1,
        "protocol": "spatial-8c-navigation",
        "preregistration_sha256": hashlib.sha256(preregistration.read_bytes()).hexdigest(),
        "git_revision_of_snapshot": (root / "frozen/git-revision").read_text().strip(),
        "open_seeds": open_seeds,
        "wall_seeds": wall_seeds,
        "infrastructure_reruns": (root / "infrastructure-reruns.txt").read_text().split()
        if (root / "infrastructure-reruns.txt").exists() else [],
        "validity": {"all_traces_replay_exactly": replay_exact, "all_runs_complete": complete,
                     "median_dead_reckoning_error_blocks": median_error, "valid": valid},
        "primary_S_vs_B": primary,
        "sham_S_vs_X": sham,
        "wall_report_only": wall,
        "bounded": {"max_places": max(pair["S"]["max_places"] for pair in open_pairs + wall_pairs)},
        "decision": "accepted" if accepted else "rejected",
        "open": open_pairs,
        "wall": wall_pairs,
    }
    (root / "spatial-8c-result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in (
        "validity", "primary_S_vs_B", "sham_S_vs_X", "wall_report_only", "bounded", "decision")},
        indent=2))


if __name__ == "__main__":
    main()
