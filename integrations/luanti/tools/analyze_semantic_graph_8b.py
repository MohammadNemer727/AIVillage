"""Applies the frozen 8B decision rules to a completed run directory.

Usage: analyze_semantic_graph_8b.py <repo> <run-directory> <seed>...

Thresholds are copied from docs/semantic-graph-8b-preregistration.json and must not change
after outcomes exist.
"""

import hashlib
import json
import pathlib
import subprocess
import sys

ARMS = {"A": "off", "B": "policy", "G": "graph"}
SCORED = {"reversal": 80, "stationary": 40}
PRIMARY_MARGIN = 16
MIN_POSITIVE_PAIRS = 6
MAX_NEGATIVE_PAIRS = 1
STATIONARY_TOLERANCE = 8
STATIONARY_PAIR_TOLERANCE = 4


def trace_summary(path):
    records = [json.loads(line) for line in path.read_text().splitlines() if line]
    rows = [record["response"]["diagnostics"]["episodic_mediation"] for record in records]
    graph = [row["write"].get("graph", {}) for row in rows]
    return {
        "transitions": len(rows),
        "action_changes": sum(row["retrieval"]["action_changed"] for row in rows),
        "retrievals_with_body_evidence": sum(row["retrieval"]["retrieved"] > 0 for row in rows),
        "graph_nodes_max": max((g.get("nodes", 0) for g in graph), default=0),
        "graph_created": sum(bool(g.get("created")) for g in graph),
        "graph_evicted": sum(bool(g.get("evicted")) for g in graph),
        "graph_merged": sum(bool(g.get("merged")) for g in graph),
        "graph_contradictions": sum(g.get("contradicted", 0) for g in graph),
    }


def flips(choices):
    thrusts = [choice["thrust"] for choice in choices]
    return sum(left != right for left, right in zip(thrusts, thrusts[1:]))


def load(root, variant, seed):
    pair = {"seed": seed}
    replay_exact = True
    for name, arm in ARMS.items():
        directory = root / f"{variant}-seed-{seed}-{arm}"
        result = json.loads((directory / "episodic-choice-result.json").read_text())
        replay_log = directory / "replay.log"
        result["replayed_exactly"] = replay_log.exists() and "exactly" in replay_log.read_text()
        replay_exact &= result["replayed_exactly"]
        result["trace_sha256"] = hashlib.sha256((directory / "bridge.ndjson").read_bytes()).hexdigest()
        result["trace"] = trace_summary(directory / "bridge.ndjson")
        result["policy_flips"] = flips(result["choices"])
        result["online_prediction"] = json.loads(
            (directory / "online-prediction.json").read_text())["prediction"]
        del result["choices"]
        pair[name] = result
    return pair, replay_exact


def fixture_valid(pairs, variant):
    return all(
        pair["A"]["baseline_exposure_valid"]
        and pair["A"]["observed_fullness_increases"] > 0
        and pair["A"]["observed_fullness_decreases"] > 0
        and all(pair[name]["scored_eligible_choices"] == SCORED[variant] for name in ARMS)
        for pair in pairs
    )


def main():
    repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    seeds = [int(seed) for seed in sys.argv[3:]]
    reversal, replay_reversal = zip(*(load(root, "reversal", seed) for seed in seeds))
    stationary, replay_stationary = zip(*(load(root, "stationary", seed) for seed in seeds))
    reversal, stationary = list(reversal), list(stationary)
    replay_exact = all(replay_reversal) and all(replay_stationary)
    valid = fixture_valid(reversal, "reversal") and fixture_valid(stationary, "stationary") \
        and replay_exact

    post_returns = {n: sum(p[n]["post_reversal_returns"] for p in reversal) for n in ARMS}
    post_losses = {n: sum(p[n]["post_reversal_losses"] for p in reversal) for n in ARMS}
    post_deltas = [p["G"]["post_reversal_returns"] - p["B"]["post_reversal_returns"]
                   for p in reversal]
    mediation = {
        "graph_contradictions": sum(p["G"]["trace"]["graph_contradictions"] for p in reversal),
        "action_changes": sum(p["G"]["trace"]["action_changes"] for p in reversal),
    }
    primary = {
        "post_reversal_returns": post_returns,
        "post_reversal_losses": post_losses,
        "return_deltas_G_minus_B": post_deltas,
        "positive_pairs": sum(d > 0 for d in post_deltas),
        "negative_pairs": sum(d < 0 for d in post_deltas),
        "mediation": mediation,
    }
    primary["passes"] = (
        post_returns["G"] - post_returns["B"] >= PRIMARY_MARGIN
        and post_losses["B"] - post_losses["G"] >= PRIMARY_MARGIN
        and primary["positive_pairs"] >= MIN_POSITIVE_PAIRS
        and primary["negative_pairs"] <= MAX_NEGATIVE_PAIRS
        and mediation["graph_contradictions"] > 0
        and mediation["action_changes"] > 0
    )

    returns = {n: sum(p[n]["resource_returns"] for p in stationary) for n in ARMS}
    losses = {n: sum(p[n]["resource_losses"] for p in stationary) for n in ARMS}
    pair_gaps = [p["B"]["resource_returns"] - p["G"]["resource_returns"] for p in stationary]
    non_inferiority = {
        "resource_returns": returns,
        "resource_losses": losses,
        "worst_pair_shortfall_G_vs_B": max(pair_gaps),
        "policy_flips": {n: sum(p[n]["policy_flips"] for p in stationary) for n in ARMS},
    }
    non_inferiority["passes"] = (
        returns["G"] >= returns["B"] - STATIONARY_TOLERANCE
        and losses["G"] <= losses["B"] + STATIONARY_TOLERANCE
        and max(pair_gaps) <= STATIONARY_PAIR_TOLERANCE
        and returns["G"] - returns["A"] >= PRIMARY_MARGIN
    )

    accepted = valid and primary["passes"] and non_inferiority["passes"]
    preregistration = repo / "docs/semantic-graph-8b-preregistration.json"
    graph_arms = [p["G"]["trace"] for p in reversal + stationary]
    report = {
        "schema_version": 1,
        "protocol": "semantic-graph-8b-consolidation",
        "preregistration_sha256": hashlib.sha256(preregistration.read_bytes()).hexdigest(),
        "git_revision_of_snapshot": (root / "frozen/git-revision").read_text().strip(),
        "seeds": seeds,
        "infrastructure_reruns": (root / "infrastructure-reruns.txt").read_text().split()
        if (root / "infrastructure-reruns.txt").exists() else [],
        "validity": {
            "fixture_valid_reversal": fixture_valid(reversal, "reversal"),
            "fixture_valid_stationary": fixture_valid(stationary, "stationary"),
            "all_traces_replay_exactly": replay_exact,
            "valid": valid,
        },
        "primary_reversal_G_vs_B": primary,
        "non_inferiority_stationary_G_vs_B": non_inferiority,
        "graph_secondary": {
            "nodes_max": max(g["graph_nodes_max"] for g in graph_arms),
            "nodes_created_total": sum(g["graph_created"] for g in graph_arms),
            "nodes_evicted_total": sum(g["graph_evicted"] for g in graph_arms),
        },
        "decision": "accepted" if accepted else "rejected",
        "reversal": reversal,
        "stationary": stationary,
    }
    (root / "semantic-graph-8b-result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: report[key] for key in (
        "validity", "primary_reversal_G_vs_B", "non_inferiority_stationary_G_vs_B",
        "graph_secondary", "decision")}, indent=2))


if __name__ == "__main__":
    main()
