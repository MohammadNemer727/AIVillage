#!/usr/bin/env sh
# Runs preregistered Experiment 004 (docs/experiment-004-preregistration.json) exactly once, from
# a frozen snapshot of the binary and the analysis. The probes are deterministic and need no game.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/experiment-004-runs/$(date -u +%Y-%m-%d)"}
case "$output" in /*) ;; *) output="$(pwd)/$output";; esac
[ ! -e "$output" ] || { echo "$output exists; Experiment 004 runs exactly once" >&2; exit 2; }
[ -z "$(git -C "$repo" status --porcelain --untracked-files=no)" ] || {
  echo "commit the tree first: the run must use exactly the frozen revision" >&2; exit 2;
}

mkdir -p "$output/frozen"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
cp "$repo/target/debug/sim-luanti" "$output/frozen/sim-luanti"
cp "$root/tools/analyze_experiment_004.py" "$output/frozen/"
git -C "$repo" rev-parse HEAD >"$output/frozen/git-revision"
bridge="$output/frozen/sim-luanti"

"$bridge" genome-audit 4000000 legacy >"$output/audit-legacy.json"
"$bridge" genome-audit 4000000 v2 >"$output/audit-v2.json"
"$bridge" locus-probe >"$output/locus-probe.json"
"$bridge" mutation-effect >"$output/mutation-effect.json"
python3 "$output/frozen/analyze_experiment_004.py" "$repo" "$output/locus-probe.json" \
  "$output/mutation-effect.json" "$output/audit-v2.json" "$output/audit-legacy.json" \
  >"$output/experiment-004-result.json"
cat "$output/experiment-004-result.json"
