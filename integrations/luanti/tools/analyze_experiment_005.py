"""Applies the frozen Experiment 005 decision rules to a completed run directory.

Usage: analyze_experiment_005.py <repo> <run-directory>

Thresholds are copied from docs/experiment-005-preregistration.json and must not change after the
results exist.
"""

import hashlib
import json
import math
import pathlib
import re
import statistics
import sys

B_SEEDS = [7201, 7203, 7207, 7209, 7213, 7217, 7219, 7223]
C_SEEDS = [7301, 7303, 7307, 7309, 7313, 7317, 7319, 7323]
D_SEEDS = [7401, 7403]
GENOTYPES = ("still", "tone", "flight", "runner")
B_MIN_HIGH_PATH = 30.0
B_MIN_RATIO = 5.0
B_MIN_PAIRS = 7
C_MIN_GAIN = 0.25
C_MIN_SEEDS = 6
WINDOW_DECISIONS = 100


def arena(root, name):
    return json.loads((root / name / "arena-result.json").read_text())


def path_length(result):
    path = result["path_milli"]
    return sum(math.hypot(b[0] - a[0], b[1] - a[1]) for a, b in zip(path, path[1:])) / 1000


def survived(result):
    return result.get("died_at_decision") or result["decisions"]


def replayed(root, name):
    log = root / name / "replay.log"
    return log.exists() and "exactly" in log.read_text()


def part_b(root):
    pairs = []
    for seed in B_SEEDS:
        low = path_length(arena(root, f"B-{seed}-low"))
        high = path_length(arena(root, f"B-{seed}-high"))
        pairs.append({"seed": seed, "low": low, "high": high})
    median_high = statistics.median(pair["high"] for pair in pairs)
    median_low = statistics.median(pair["low"] for pair in pairs)
    farther = sum(pair["high"] > pair["low"] for pair in pairs)
    ratio = median_high / median_low if median_low else math.inf
    return {
        "pairs": pairs, "median_path_low": median_low, "median_path_high": median_high,
        "ratio": ratio, "pairs_high_farther": farther,
        "passes": median_high >= B_MIN_HIGH_PATH and ratio >= B_MIN_RATIO
        and farther >= B_MIN_PAIRS,
    }


def part_c(root):
    rows = {}
    for seed in C_SEEDS:
        rows[seed] = {}
        for genotype in GENOTYPES:
            result = arena(root, f"C-{seed}-{genotype}")
            rows[seed][genotype] = {
                "survived_decisions": survived(result),
                "alive_at_end": result.get("died_at_decision") is None,
                "damage": 20 - min(result["health"]),
            }
    summary = {}
    baseline = statistics.mean(rows[s]["still"]["survived_decisions"] for s in C_SEEDS)
    for genotype in GENOTYPES:
        mean = statistics.mean(rows[s][genotype]["survived_decisions"] for s in C_SEEDS)
        longer = sum(rows[s][genotype]["survived_decisions"] > rows[s]["still"]["survived_decisions"]
                     for s in C_SEEDS)
        summary[genotype] = {
            "mean_survived_decisions": mean,
            "alive_at_end": sum(rows[s][genotype]["alive_at_end"] for s in C_SEEDS),
            "seeds_surviving_longer_than_still": longer,
            "gain_over_still": (mean - baseline) / baseline if baseline else math.inf,
            "mean_damage": statistics.mean(rows[s][genotype]["damage"] for s in C_SEEDS),
        }
    helping = [g for g in GENOTYPES if g != "still"
               and summary[g]["gain_over_still"] >= C_MIN_GAIN
               and summary[g]["seeds_surviving_longer_than_still"] >= C_MIN_SEEDS]
    return {"by_seed": rows, "summary": summary, "genotypes_that_help": helping,
            "passes": bool(helping)}


def opportunity_windows(lifecycle):
    blocked = {event["event_id"] for event in lifecycle
               if event["event"] == "reproduction_blocked"
               and event["reason"] == "biological_eligibility"}
    windows = {}
    for event in lifecycle:
        if event["event"] == "reproduction_attempt" and event["event_id"] not in blocked:
            windows.setdefault(event["parent_id"], set()).add(
                event["decision_tick"] // WINDOW_DECISIONS)
    return {individual: len(found) for individual, found in windows.items()}


def part_d(root):
    populations = []
    for seed in D_SEEDS:
        for genotype in ("still", "runner"):
            name = f"D-{seed}-{genotype}"
            report = json.loads((root / name / "population-report.json").read_text())
            lifecycle = report["manifest"]["lifecycle"]
            windows = opportunity_windows(lifecycle)
            individuals = sorted({e["individual_id"] for e in lifecycle if e["event"] == "birth"}
                                 | {1, 2, 3, 4})
            counts = [windows.get(individual, 0) for individual in individuals]
            log = (root / name / "server.log").read_text(errors="replace")
            deaths = re.findall(r"^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d): ACTION\[Server\]: Player "
                                r"founder-\d+ died", log, re.MULTILINE)
            first_line = re.search(r"^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d):", log, re.MULTILINE)
            populations.append({
                "name": name, "seed": seed, "genotype": genotype,
                "stop_reason": report["stop_reason"], "elapsed_seconds": report["elapsed_seconds"],
                "births": report["births"], "alive_at_end": report["live_individuals"],
                "first_death": deaths[0] if deaths else None,
                "server_started": first_line.group(1) if first_line else None,
                "deaths": len(deaths),
                "opportunity_windows_per_individual": counts,
                "median_opportunity_windows": statistics.median(counts) if counts else 0,
                "max_opportunity_windows": max(counts, default=0),
                "individuals_with_two_or_more": sum(count >= 2 for count in counts),
                "suitable": report["live_individuals"] >= 1
                and sum(count >= 2 for count in counts) >= 2,
            })
    by_genotype = {}
    for genotype in ("still", "runner"):
        rows = [p for p in populations if p["genotype"] == genotype]
        by_genotype[genotype] = {"suitable_replicates": sum(p["suitable"] for p in rows),
                                 "suitable_for_selection": any(p["suitable"] for p in rows)}
    return {"populations": populations, "by_genotype": by_genotype}


def main():
    repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    names = ([f"B-{s}-{n}" for s in B_SEEDS for n in ("low", "high")]
             + [f"C-{s}-{g}" for s in C_SEEDS for g in GENOTYPES])
    replay_exact = all(replayed(root, name) for name in names)
    report = {
        "schema_version": 1,
        "protocol": "experiment-005-embodied-survival-reliability",
        "preregistration_sha256": hashlib.sha256(
            (repo / "docs/experiment-005-preregistration.json").read_bytes()).hexdigest(),
        "git_revision_of_snapshot": (root / "frozen/git-revision").read_text().strip(),
        "infrastructure_reruns": (root / "infrastructure-reruns.txt").read_text().split()
        if (root / "infrastructure-reruns.txt").exists() else [],
        "all_arena_traces_replay_exactly": replay_exact,
        "part_B_movement_initiation": part_b(root),
        "part_C_hostile_encounter": part_c(root),
        "part_D_first_night_populations": part_d(root),
    }
    report["conclusions"] = {
        "movement_without_a_driver": report["part_B_movement_initiation"]["passes"],
        "survival_depends_on_inheritance": report["part_C_hostile_encounter"]["passes"],
        "runner_populations_suitable_for_selection":
            report["part_D_first_night_populations"]["by_genotype"]["runner"][
                "suitable_for_selection"],
    }
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
