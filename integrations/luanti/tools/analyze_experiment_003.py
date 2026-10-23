"""Applies the frozen Experiment 003 decision rules to a completed run directory.

Usage: analyze_experiment_003.py <repo> <run-directory> <seed>...

Every threshold here is copied from docs/experiment-003-preregistration.json and must not be
changed after outcomes exist.
"""

import hashlib
import json
import pathlib
import subprocess
import sys

ARMS = {"A": "off", "B": "policy", "C": "prediction", "D": "both"}
MIN_AGGREGATE_CHANGE = 16
MIN_POSITIVE_PAIRS = 6
SCORED_CHOICES = 40


def mediation(trace_path):
    records = [json.loads(line) for line in trace_path.read_text().splitlines() if line]
    rows = [record["response"]["diagnostics"]["episodic_mediation"] for record in records]
    writes = [row["write"] for row in rows]
    retrievals = [row["retrieval"] for row in rows]
    changed = [index for index, row in enumerate(rows) if row["retrieval"]["action_changed"]]
    following = [writes[index + 1]["outcome_utility"] for index in changed
                 if index + 1 < len(writes)]
    health = [record["request"]["observation"]["interoception"]["health"]
              for record in records if record["request"]["observation"].get("interoception")]
    return {
        "transitions": len(rows),
        "episode_insertions": sum(row["inserted"] for row in writes),
        "episode_reinforcements": sum(row["reinforced"] for row in writes),
        "positive_outcome_writes": sum(row["outcome_utility"] > 0
                                       and (row["inserted"] or row["reinforced"])
                                       for row in writes),
        "negative_outcome_writes": sum(row["outcome_utility"] < 0
                                       and (row["inserted"] or row["reinforced"])
                                       for row in writes),
        "retrievals_with_body_evidence": sum(row["retrieved"] > 0 for row in retrievals),
        "retrievals_with_positive_delta": sum(row["score_delta"] > 0 for row in retrievals),
        "action_changes": len(changed),
        "changed_followed_by_nonnegative_outcome": sum(value >= 0 for value in following),
        "changed_followed_by_negative_outcome": sum(value < 0 for value in following),
        "minimum_felt_health": min(health, default=None),
        "deaths": sum(1 for before, after in zip(health, health[1:]) if before > 0 and after == 0),
    }


def policy_criterion(pairs, arm):
    a_losses = sum(pair["A"]["resource_losses"] for pair in pairs)
    a_returns = sum(pair["A"]["resource_returns"] for pair in pairs)
    x_losses = sum(pair[arm]["resource_losses"] for pair in pairs)
    x_returns = sum(pair[arm]["resource_returns"] for pair in pairs)
    deltas = [pair[arm]["resource_returns"] - pair["A"]["resource_returns"] for pair in pairs]
    mediated = {
        key: sum(pair[arm]["mediation"][key] for pair in pairs)
        for key in ("positive_outcome_writes", "retrievals_with_body_evidence",
                    "action_changes", "changed_followed_by_nonnegative_outcome",
                    "changed_followed_by_negative_outcome")
    }
    aggregate = (a_losses - x_losses >= MIN_AGGREGATE_CHANGE
                 and x_returns - a_returns >= MIN_AGGREGATE_CHANGE)
    paired = (sum(delta > 0 for delta in deltas) >= MIN_POSITIVE_PAIRS
              and all(delta >= 0 for delta in deltas))
    mediation_valid = (mediated["positive_outcome_writes"] > 0
                       and mediated["retrievals_with_body_evidence"] > 0
                       and mediated["action_changes"] > 0
                       and mediated["changed_followed_by_nonnegative_outcome"]
                       >= mediated["changed_followed_by_negative_outcome"])
    return {
        "A_resource_losses": a_losses,
        "A_resource_returns": a_returns,
        f"{arm}_resource_losses": x_losses,
        f"{arm}_resource_returns": x_returns,
        "return_deltas": deltas,
        "positive_pairs": sum(delta > 0 for delta in deltas),
        "negative_pairs": sum(delta < 0 for delta in deltas),
        "mediation": mediated,
        "aggregate_valid": aggregate,
        "paired_valid": paired,
        "mediation_valid": mediation_valid,
        "passes": aggregate and paired and mediation_valid,
    }


def prediction_criterion(frozen, treatment, control):
    pooled = {}
    for condition in (treatment, control):
        for modality in ("vision", "body", "held_item"):
            errors = [report["conditions"][condition][modality] for report in frozen]
            total = sum(error["absolute_error_total"] for error in errors)
            samples = sum(error["feature_samples"] for error in errors)
            pooled.setdefault(modality, {})[condition] = {
                "absolute_error_total": total,
                "feature_samples": samples,
                "mae": total / samples if samples else 0.0,
            }
    counts = {}
    for modality in ("vision", "body", "held_item"):
        rows = [next(c for c in report["comparisons"] if c["treatment"] == treatment
                     and c["control"] == control)["modalities"][modality] for report in frozen]
        counts[modality] = {key: sum(row[key] for row in rows)
                            for key in ("treatment_wins", "ties", "treatment_losses")}
    no_regression = all(
        pooled[m][treatment]["absolute_error_total"] * pooled[m][control]["feature_samples"]
        <= pooled[m][control]["absolute_error_total"] * pooled[m][treatment]["feature_samples"]
        for m in ("vision", "body")
    )
    improved = [
        m for m in ("vision", "body")
        if pooled[m][treatment]["mae"] < pooled[m][control]["mae"]
        and counts[m]["treatment_wins"] > counts[m]["treatment_losses"]
    ]
    return {
        "comparison": f"{treatment}_vs_{control}",
        "pooled": pooled,
        "paired_transitions": counts,
        "no_regression": no_regression,
        "improved_modalities": improved,
        "passes": no_regression and bool(improved),
    }


def main():
    repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    seeds = [int(seed) for seed in sys.argv[3:]]
    pairs = []
    replay_exact = True
    for seed in seeds:
        pair = {"seed": seed}
        for condition, arm in ARMS.items():
            directory = root / f"seed-{seed}-{arm}"
            result = json.loads((directory / "episodic-choice-result.json").read_text())
            trace = directory / "bridge.ndjson"
            replay_log = directory / "replay.log"
            result["replayed_exactly"] = (replay_log.exists()
                                          and "exactly" in replay_log.read_text())
            replay_exact &= result["replayed_exactly"]
            result["trace_sha256"] = hashlib.sha256(trace.read_bytes()).hexdigest()
            result["mediation"] = mediation(trace)
            result["online_prediction"] = json.loads(
                (directory / "online-prediction.json").read_text())["prediction"]
            del result["choices"]
            pair[condition] = result
        pairs.append(pair)
    frozen = [json.loads((root / f"seed-{seed}-frozen-replay.json").read_text())
              for seed in seeds]

    fixture_valid = all(
        pair["A"]["baseline_exposure_valid"]
        and pair["A"]["observed_fullness_increases"] > 0
        and pair["A"]["observed_fullness_decreases"] > 0
        and all(pair[c]["scored_eligible_choices"] == SCORED_CHOICES for c in ARMS)
        for pair in pairs
    )
    switch_invariant = all(report["policy_switch_invariant"] for report in frozen)
    valid = fixture_valid and replay_exact and switch_invariant

    policy_b = policy_criterion(pairs, "B")
    policy_c = policy_criterion(pairs, "C")
    policy_d = policy_criterion(pairs, "D")
    prediction_ca = prediction_criterion(frozen, "C", "A")
    prediction_db = prediction_criterion(frozen, "D", "B")
    b_zero_regression = all(
        report["conditions"]["A"] == report["conditions"]["B"] for report in frozen)

    accept_b = valid and policy_b["passes"] and b_zero_regression
    accept_prediction = (valid and prediction_ca["passes"] and policy_d["passes"]
                         and prediction_db["passes"])
    preregistration = repo / "docs/experiment-003-preregistration.json"
    report = {
        "schema_version": 1,
        "protocol": "experiment-003-episodic-integration",
        "preregistration": "docs/experiment-003-preregistration.json",
        "preregistration_sha256": hashlib.sha256(preregistration.read_bytes()).hexdigest(),
        "git_revision": subprocess.check_output(
            ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip(),
        "seeds": seeds,
        "infrastructure_reruns": (root / "infrastructure-reruns.txt").read_text().split()
        if (root / "infrastructure-reruns.txt").exists() else [],
        "validity": {
            "fixture_valid": fixture_valid,
            "all_traces_replay_exactly": replay_exact,
            "frozen_policy_switch_invariant": switch_invariant,
            "valid": valid,
        },
        "live": {"B_vs_A": policy_b, "C_vs_A_diagnostic": policy_c, "D_vs_A": policy_d},
        "frozen_replay": {"C_vs_A": prediction_ca, "D_vs_B": prediction_db,
                          "B_equals_A_exactly": b_zero_regression},
        "decisions": {
            "policy_only_episodic_retrieval": "accepted" if accept_b else "rejected",
            "prediction_integrated_episodic_retrieval":
                "accepted" if accept_prediction else "rejected",
        },
        "pairs": pairs,
        "frozen_by_seed": frozen,
    }
    (root / "experiment-003-result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"validity": report["validity"], "decisions": report["decisions"],
                      "B_vs_A": {k: v for k, v in policy_b.items() if k != "mediation"},
                      "C_vs_A_body_mae": {c: prediction_ca["pooled"]["body"][c]["mae"]
                                          for c in ("A", "C")}}, indent=2))


if __name__ == "__main__":
    main()
