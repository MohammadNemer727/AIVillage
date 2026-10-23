#!/usr/bin/env sh
# Runs preregistered Experiment 005E part P (docs/experiment-005e-perception-preregistration.json)
# exactly once, from a frozen snapshot of the binary, the bridge mod, the fixture and the analysis.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/experiment-005e-perception-runs/$(date -u +%Y-%m-%d)"}
case "$output" in /*) ;; *) output="$(pwd)/$output";; esac
[ ! -e "$output" ] || { echo "$output exists; the perception test runs exactly once" >&2; exit 2; }
[ -z "$(git -C "$repo" status --porcelain --untracked-files=no)" ] || {
  echo "commit the tree first: the run must use exactly the frozen revision" >&2; exit 2;
}

mkdir -p "$output/frozen"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
cp "$repo/target/debug/sim-luanti" "$output/frozen/sim-luanti"
cp -R "$root/games/ai_village/mods/ai_bridge" "$output/frozen/ai_bridge"
cp -R "$root/ci/perception" "$output/frozen/perception"
cp "$root/tools/analyze_experiment_005e_perception.py" "$output/frozen/"
git -C "$repo" rev-parse HEAD >"$output/frozen/git-revision"

python3 "$root/tools/run_experiment_005e_perception.py" "$repo" "$output"
python3 "$output/frozen/analyze_experiment_005e_perception.py" "$repo" "$output" \
  >"$output/experiment-005e-perception-result.json"
cat "$output/experiment-005e-perception-result.json"
