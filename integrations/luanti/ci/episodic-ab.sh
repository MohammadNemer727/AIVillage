#!/usr/bin/env sh
# Runs the preregistered four-seed real VoxeLibre episodic policy A/B once.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/episodic-ab-runs/2026-10-22"}
runner="$root/ci/episodic-ab-run.sh"
mkdir -p "$output"

index=0
for seed in 211 223 227 229; do
  for arm in off policy; do
    bridge_port=$((8520 + index * 2))
    server_port=$((30220 + index * 2))
    sh "$runner" "$output/seed-$seed-$arm" "$arm" "$seed" "$bridge_port" "$server_port"
    index=$((index + 1))
  done
done

python3 - "$repo" "$output" <<'PY'
import hashlib, json, pathlib, subprocess, sys

repo, root = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
seeds = [211, 223, 227, 229]
pairs = []
for seed in seeds:
    arms = {}
    for arm in ("off", "policy"):
        directory = root / f"seed-{seed}-{arm}"
        result = json.loads((directory / "episodic-ab-result.json").read_text())
        trace = directory / "bridge.ndjson"
        result["trace_sha256"] = hashlib.sha256(trace.read_bytes()).hexdigest()
        result["trace_transitions"] = sum(1 for line in trace.read_text().splitlines() if line)
        arms[arm] = result
    pairs.append({
        "seed": seed,
        "A": arms["off"],
        "B": arms["policy"],
        "injury_delta_B_minus_A": arms["policy"]["injuries"] - arms["off"]["injuries"],
        "resource_return_delta_B_minus_A":
            arms["policy"]["resource_returns"] - arms["off"]["resource_returns"],
    })
deltas = [pair["resource_return_delta_B_minus_A"] for pair in pairs]
a_injuries = sum(pair["A"]["injuries"] for pair in pairs)
b_injuries = sum(pair["B"]["injuries"] for pair in pairs)
a_returns = sum(pair["A"]["resource_returns"] for pair in pairs)
b_returns = sum(pair["B"]["resource_returns"] for pair in pairs)
success = (
    b_injuries < a_injuries
    and b_returns > a_returns
    and sum(delta > 0 for delta in deltas) >= 3
    and all(delta >= 0 for delta in deltas)
)
revision = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
report = {
    "schema_version": 1,
    "protocol": "episodic-policy-live-ab-v1",
    "preregistration": "docs/episodic-live-ab-preregistration.json",
    "git_revision": revision,
    "pairs": pairs,
    "aggregate": {
        "A_injuries": a_injuries,
        "B_injuries": b_injuries,
        "A_resource_returns": a_returns,
        "B_resource_returns": b_returns,
        "positive_pairs": sum(delta > 0 for delta in deltas),
        "negative_pairs": sum(delta < 0 for delta in deltas),
    },
    "behavioral_success": success,
}
(root / "episodic-live-ab-result.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report["aggregate"], indent=2))
print(f"behavioral_success={str(success).lower()}")
PY
