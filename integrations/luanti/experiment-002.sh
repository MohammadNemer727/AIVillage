#!/usr/bin/env sh
# Runs preregistered Experiment 002 (docs/experiment-002-preregistration.json) exactly once:
# eight rotated evolution replicates, then a four-block common garden of newborn founders.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/experiment-002-runs/$(date -u +%Y-%m-%d)"}
[ ! -e "$output" ] || { echo "$output exists; Experiment 002 runs exactly once" >&2; exit 2; }
[ -z "$(git -C "$repo" status --porcelain --untracked-files=no)" ] || {
  echo "commit the tree first: the run must use exactly the frozen revision" >&2; exit 2;
}

mkdir -p "$output/frozen"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
cp "$repo/target/debug/sim-luanti" "$output/frozen/sim-luanti"
cp -R "$root/games/ai_village/mods/ai_bridge" "$output/frozen/ai_bridge"
cp "$root/tools/analyze_experiment_002.py" "$output/frozen/"
git -C "$repo" rev-parse HEAD >"$output/frozen/git-revision"

export PLAY_BRIDGE_BIN="$output/frozen/sim-luanti"
export PLAY_AI_BRIDGE_MOD="$output/frozen/ai_bridge"
export PLAY_FOUNDER_COUNT=4 PLAY_POPULATION_CAPACITY=4 PLAY_UNATTENDED=1
export PLAY_PORT=30201 PLAY_BRIDGE_PORT=8682
export SIM_LUANTI_SEED=103 SIM_LUANTI_MODE=baseline
export SIM_LUANTI_EXPERIMENT_NAME=002-heritable-behavioral-variation
export SIM_LUANTI_MINIMUM_REPRODUCTIVE_RESERVE=160 SIM_LUANTI_REPRODUCTIVE_COST=80
export SIM_LUANTI_EXPERIMENT_001_BIRTHS=100 SIM_LUANTI_EXPERIMENT_001_DEADLINE_SECONDS=1800

# One population run; a run that fails before writing its report is rerun once.
run() {
  name=$1 rotation=$2 mutation=$3 maturity=$4
  world="$output/$name"
  attempt=1
  while :; do
    mkdir -p "$world"
    SIM_LUANTI_FOUNDER_GENOME_ROTATION=$rotation SIM_LUANTI_MUTATION=$mutation \
      SIM_LUANTI_REPRODUCTIVE_MATURITY=$maturity \
      SIM_LUANTI_EXPERIMENT_001_REPORT="$world/population-report.json" \
      SIM_LUANTI_TRACE="$world/bridge-trace.ndjson" \
      sh "$root/play.sh" "$world" >"$world/launcher.log" 2>&1 || true
    [ -f "$world/population-report.json" ] && return 0
    [ "$attempt" -eq 1 ] || {
      echo "second infrastructure failure in $name; Experiment 002 is incomplete" >&2; exit 1;
    }
    echo "infrastructure failure in $name; rerunning once" >&2
    echo "$name" >>"$output/infrastructure-reruns.txt"
    mv "$world" "$world.failed"
    attempt=2
  done
}

for replicate in 0 1 2 3 4 5 6 7; do
  run "evolution-$replicate" $((replicate % 4)) enabled 100
done
for block in 0 1 2 3; do
  run "garden-$block" "$block" disabled 1000000000000
done

python3 "$output/frozen/analyze_experiment_002.py" "$repo" "$output"
