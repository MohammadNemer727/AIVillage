#!/usr/bin/env sh
# Regression test for the founder body's turns: command the same turn every decision in the real
# game and require that at least 98 percent of the commanded rotation is actually carried out.
# Before the closed-loop heading fix only 47.5 percent was.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
world=${1:-"$root/.turn-diag-world"}
rm -rf "$world"
export EPISODIC_CHOICE_MODE=baseline EPISODIC_CHOICE_FIXTURE=turn_diag
export EPISODIC_CHOICE_RESULT=turn-result.json EPISODIC_CHOICE_PROTOCOL=turn-diag
export FIXTURE_CONFIG="turn_diag_decisions = 300
turn_diag_turn = 256
turn_diag_arm = regression"
sh "$root/ci/episodic-choice-run.sh" "$world" off 7003 8504 30504
python3 - "$world/turn-result.json" <<'PY'
import json, math, sys
data = json.load(open(sys.argv[1]))
step = 2 * math.pi / 256
yaw = data["yaw_radians"]
carried = sum(((b - a + math.pi) % (2 * math.pi) - math.pi) / step for a, b in zip(yaw, yaw[1:]))
expected = data["expected_steps_per_decision"] * (len(yaw) - 1)
fraction = carried / expected
print(f"turn fidelity {fraction:.3f} ({carried:.0f} of {expected} steps carried out)")
sys.exit(0 if fraction >= 0.98 else 1)
PY
