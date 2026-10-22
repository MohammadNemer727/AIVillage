#!/usr/bin/env sh
# Runs the preregistered v4 six-seed real VoxeLibre delayed-choice A/B exactly once.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/episodic-ab-runs/2026-10-22-choice-v4"}
runner="$root/ci/episodic-choice-run.sh"
mkdir -p "$output"

index=0
for seed in 379 383 389 397 401 409; do
  for arm in off policy; do
    bridge_port=$((8620 + index * 2))
    server_port=$((31220 + index * 2))
    sh "$runner" "$output/seed-$seed-$arm" "$arm" "$seed" "$bridge_port" "$server_port"
    index=$((index + 1))
  done
done

python3 - "$repo" "$output" <<'PY'
import hashlib, json, pathlib, subprocess, sys

repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
seeds = [379, 383, 389, 397, 401, 409]

def mediation(trace_path):
    records = [json.loads(line) for line in trace_path.read_text().splitlines() if line]
    rows = [record["response"]["diagnostics"]["episodic_mediation"] for record in records]
    writes = [row["write"] for row in rows]
    retrievals = [row["retrieval"] for row in rows]
    changed = [index for index, row in enumerate(rows) if row["retrieval"]["action_changed"]]
    return {
        "transitions": len(rows),
        "episode_insertions": sum(row["inserted"] for row in writes),
        "episode_reinforcements": sum(row["reinforced"] for row in writes),
        "positive_outcome_writes": sum(row["outcome_utility"] > 0 and
                                       (row["inserted"] or row["reinforced"]) for row in writes),
        "negative_outcome_writes": sum(row["outcome_utility"] < 0 and
                                       (row["inserted"] or row["reinforced"]) for row in writes),
        "retrievals_with_body_evidence": sum(row["retrieved"] > 0 for row in retrievals),
        "retrievals_with_positive_delta": sum(row["score_delta"] > 0 for row in retrievals),
        "action_changes": len(changed),
        "changed_actions_followed_by_positive_outcome": sum(
            index + 1 < len(writes) and writes[index + 1]["outcome_utility"] > 0
            for index in changed
        ),
        "eligibility_delays": sorted(set(
            row["eligibility_delay"] for row in writes
            if row["inserted"] or row["reinforced"]
        )),
        "nearest_distance": {
            "min": min((row["nearest_distance"] for row in retrievals
                        if row["nearest_distance"] is not None), default=None),
            "max": max((row["nearest_distance"] for row in retrievals
                        if row["nearest_distance"] is not None), default=None),
        },
        "baseline_policy_score": {
            "min": min(row["baseline_policy_score"] for row in rows),
            "max": max(row["baseline_policy_score"] for row in rows),
        },
        "episodic_score_delta": {
            "min": min(row["retrieval"]["score_delta"] for row in rows),
            "max": max(row["retrieval"]["score_delta"] for row in rows),
        },
    }

pairs = []
for seed in seeds:
    arms = {}
    for arm in ("off", "policy"):
        directory = root / f"seed-{seed}-{arm}"
        result = json.loads((directory / "episodic-choice-result.json").read_text())
        trace = directory / "bridge.ndjson"
        result["trace_sha256"] = hashlib.sha256(trace.read_bytes()).hexdigest()
        result["mediation"] = mediation(trace)
        arms[arm] = result
    pairs.append({
        "seed": seed,
        "A": arms["off"],
        "B": arms["policy"],
        "resource_loss_delta_B_minus_A":
            arms["policy"]["resource_losses"] - arms["off"]["resource_losses"],
        "resource_return_delta_B_minus_A":
            arms["policy"]["resource_returns"] - arms["off"]["resource_returns"],
    })

deltas = [pair["resource_return_delta_B_minus_A"] for pair in pairs]
a_losses = sum(pair["A"]["resource_losses"] for pair in pairs)
b_losses = sum(pair["B"]["resource_losses"] for pair in pairs)
a_returns = sum(pair["A"]["resource_returns"] for pair in pairs)
b_returns = sum(pair["B"]["resource_returns"] for pair in pairs)
fixture_valid = all(
    pair["A"]["baseline_exposure_valid"]
    and pair["A"]["observed_fullness_increases"] > 0
    and pair["A"]["observed_fullness_decreases"] > 0
    and pair["A"]["scored_eligible_choices"] == 40
    and pair["B"]["scored_eligible_choices"] == 40
    for pair in pairs
)
mediation_valid = (
    sum(pair["B"]["mediation"]["positive_outcome_writes"] for pair in pairs) > 0
    and sum(pair["B"]["mediation"]["retrievals_with_body_evidence"] for pair in pairs) > 0
    and sum(pair["B"]["mediation"]["retrievals_with_positive_delta"] for pair in pairs) > 0
    and sum(pair["B"]["mediation"]["action_changes"] for pair in pairs) > 0
    and sum(pair["B"]["mediation"]["changed_actions_followed_by_positive_outcome"]
            for pair in pairs) > 0
)
success = (
    fixture_valid and mediation_valid
    and b_losses < a_losses and b_returns > a_returns
    and sum(delta > 0 for delta in deltas) >= 5
    and all(delta >= 0 for delta in deltas)
)
report = {
    "schema_version": 1,
    "protocol": "episodic-policy-live-choice-v4",
    "preregistration": "docs/episodic-live-choice-v4-preregistration.json",
    "git_revision": subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip(),
    "pairs": pairs,
    "aggregate": {
        "A_resource_losses": a_losses,
        "B_resource_losses": b_losses,
        "A_resource_returns": a_returns,
        "B_resource_returns": b_returns,
        "positive_pairs": sum(delta > 0 for delta in deltas),
        "negative_pairs": sum(delta < 0 for delta in deltas),
    },
    "fixture_valid": fixture_valid,
    "mediation_valid": mediation_valid,
    "behavioral_success": success,
}
(root / "episodic-live-choice-result.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report["aggregate"], indent=2))
print(f"fixture_valid={str(fixture_valid).lower()}")
print(f"mediation_valid={str(mediation_valid).lower()}")
print(f"behavioral_success={str(success).lower()}")
PY
