#!/usr/bin/env sh
# Runs preregistered Experiment 003 (docs/experiment-003-preregistration.json) exactly once:
# a four-way episodic ablation in real VoxeLibre plus a frozen-replay prediction ablation.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/experiment-003-runs/$(date -u +%Y-%m-%d)"}
runner="$root/ci/episodic-choice-run.sh"
seeds="601 607 613 617 631 641 643 647"
export EPISODIC_CHOICE_PROTOCOL=experiment-003-episodic-integration
[ ! -e "$output/experiment-003-result.json" ] || {
  echo "$output already holds a result; Experiment 003 seeds are consumed" >&2; exit 2;
}
mkdir -p "$output"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
bridge="$repo/target/debug/sim-luanti"

# An arm that crashed or timed out before writing a result is rerun once; anything else that
# fails (replay mismatch) is an integrity failure and is never rerun.
index=0
for seed in $seeds; do
  for arm in off policy prediction both; do
    world="$output/seed-$seed-$arm"
    bridge_port=$((8820 + index * 2))
    server_port=$((31420 + index * 2))
    if ! sh "$runner" "$world" "$arm" "$seed" "$bridge_port" "$server_port"; then
      if [ -f "$world/episodic-choice-result.json" ]; then
        echo "integrity failure in $world (result written, replay failed)" >&2
        echo "seed-$seed-$arm" >>"$output/integrity-failures.txt"
      else
        echo "infrastructure failure in $world; rerunning once" >&2
        echo "seed-$seed-$arm" >>"$output/infrastructure-reruns.txt"
        mv "$world" "$world.failed"
        sh "$runner" "$world" "$arm" "$seed" "$bridge_port" "$server_port" || {
          echo "second infrastructure failure in $world; Experiment 003 is incomplete" >&2
          exit 1
        }
      fi
    fi
    SIM_LUANTI_SEED="$seed" SIM_LUANTI_AGENT_ID=1 "$bridge" online-prediction "$arm" \
      "$world/bridge.ndjson" "$world/online-prediction.json"
    index=$((index + 1))
  done
  SIM_LUANTI_SEED="$seed" SIM_LUANTI_AGENT_ID=1 "$bridge" prediction-ablation \
    "$output/seed-$seed-off/bridge.ndjson" "$output/seed-$seed-frozen-replay.json"
done

python3 "$root/tools/analyze_experiment_003.py" "$repo" "$output" $seeds
