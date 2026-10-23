#!/usr/bin/env sh
# Runs preregistered Experiment 003 (docs/experiment-003-preregistration.json, seeds from
# docs/experiment-003-amendment-1.json) exactly once: a four-way episodic ablation in real
# VoxeLibre plus a frozen-replay prediction ablation.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/experiment-003-runs/$(date -u +%Y-%m-%d)"}
runner="$root/ci/episodic-choice-run.sh"
seeds="653 659 661 673 677 683 691 701"
export EPISODIC_CHOICE_PROTOCOL=experiment-003-episodic-integration
[ ! -e "$output" ] || { echo "$output exists; Experiment 003 seeds are consumed" >&2; exit 2; }
[ -z "$(git -C "$repo" status --porcelain --untracked-files=no)" ] || {
  echo "commit the tree first: the run must use exactly the frozen revision" >&2; exit 2;
}

# Snapshot the frozen build and analysis once; later source edits cannot reach this run.
mkdir -p "$output/frozen"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
cp "$repo/target/debug/sim-luanti" "$output/frozen/sim-luanti"
cp "$root/tools/analyze_experiment_003.py" "$output/frozen/"
git -C "$repo" rev-parse HEAD >"$output/frozen/git-revision"
export SIM_LUANTI_BIN="$output/frozen/sim-luanti"
bridge=$SIM_LUANTI_BIN

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

python3 "$output/frozen/analyze_experiment_003.py" "$repo" "$output" $seeds
