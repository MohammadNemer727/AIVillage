#!/usr/bin/env sh
# Runs preregistered 8B (docs/semantic-graph-8b-preregistration.json) exactly once: 48 real
# VoxeLibre arms (8 seeds x reversal/stationary x off/policy/graph) from a frozen snapshot.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/semantic-graph-8b-runs/$(date -u +%Y-%m-%d)"}
runner="$root/ci/episodic-choice-run.sh"
seeds="709 719 727 733 739 743 751 757"
export EPISODIC_CHOICE_PROTOCOL=semantic-graph-8b-consolidation
case "$output" in /*) ;; *) output="$(pwd)/$output";; esac
[ ! -e "$output" ] || { echo "$output exists; 8B seeds are consumed" >&2; exit 2; }
[ -z "$(git -C "$repo" status --porcelain --untracked-files=no)" ] || {
  echo "commit the tree first: the run must use exactly the frozen revision" >&2; exit 2;
}

mkdir -p "$output/frozen"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
cp "$repo/target/debug/sim-luanti" "$output/frozen/sim-luanti"
cp "$root/tools/analyze_semantic_graph_8b.py" "$output/frozen/"
git -C "$repo" rev-parse HEAD >"$output/frozen/git-revision"
export SIM_LUANTI_BIN="$output/frozen/sim-luanti"
bridge=$SIM_LUANTI_BIN

index=0
for seed in $seeds; do
  for variant in reversal stationary; do
    if [ "$variant" = reversal ]; then
      export EPISODIC_CHOICE_SCORED=80 EPISODIC_CHOICE_REVERSE_AFTER=40
    else
      export EPISODIC_CHOICE_SCORED=40 EPISODIC_CHOICE_REVERSE_AFTER=0
    fi
    for arm in off policy graph; do
      world="$output/$variant-seed-$seed-$arm"
      bridge_port=$((8920 + index * 2))
      server_port=$((31520 + index * 2))
      if ! sh "$runner" "$world" "$arm" "$seed" "$bridge_port" "$server_port"; then
        if [ -f "$world/episodic-choice-result.json" ]; then
          echo "integrity failure in $world (result written, replay failed)" >&2
          echo "$variant-seed-$seed-$arm" >>"$output/integrity-failures.txt"
        else
          echo "infrastructure failure in $world; rerunning once" >&2
          echo "$variant-seed-$seed-$arm" >>"$output/infrastructure-reruns.txt"
          mv "$world" "$world.failed"
          sh "$runner" "$world" "$arm" "$seed" "$bridge_port" "$server_port" || {
            echo "second infrastructure failure in $world; 8B is incomplete" >&2
            exit 1
          }
        fi
      fi
      SIM_LUANTI_SEED="$seed" SIM_LUANTI_AGENT_ID=1 "$bridge" online-prediction "$arm" \
        "$world/bridge.ndjson" "$world/online-prediction.json"
      index=$((index + 1))
    done
  done
done

python3 "$output/frozen/analyze_semantic_graph_8b.py" "$repo" "$output" $seeds
