#!/usr/bin/env sh
# Runs preregistered 8C (docs/spatial-8c-preregistration.json) exactly once: 52 real VoxeLibre
# arms from a frozen snapshot.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
output=${1:-"$root/spatial-8c-runs/$(date -u +%Y-%m-%d)"}
case "$output" in /*) ;; *) output="$(pwd)/$output";; esac
runner="$root/ci/episodic-choice-run.sh"
open_seeds="9201 9203 9207 9209 9213 9217 9219 9223 9229 9231 9233 9239"
wall_seeds="9241 9243 9247 9249 9251 9253 9257 9259"
[ ! -e "$output" ] || { echo "$output exists; 8C seeds are consumed" >&2; exit 2; }
[ -z "$(git -C "$repo" status --porcelain --untracked-files=no)" ] || {
  echo "commit the tree first: the run must use exactly the frozen revision" >&2; exit 2;
}

mkdir -p "$output/frozen"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
cp "$repo/target/debug/sim-luanti" "$output/frozen/sim-luanti"
cp "$root/tools/analyze_spatial_8c.py" "$output/frozen/"
git -C "$repo" rev-parse HEAD >"$output/frozen/git-revision"
export SIM_LUANTI_BIN="$output/frozen/sim-luanti"
export EPISODIC_CHOICE_MODE=wander EPISODIC_CHOICE_FIXTURE=spatial_nav
export EPISODIC_CHOICE_RESULT=spatial-result.json EPISODIC_CHOICE_PROTOCOL=spatial-8c-navigation

index=0
run_arm() {
  variant=$1 seed=$2 spatial=$3
  world="$output/$variant-seed-$seed-$spatial"
  bridge_port=$((9200 + index * 2))
  server_port=$((32000 + index * 2))
  index=$((index + 1))
  export SIM_LUANTI_SPATIAL=$spatial
  export FIXTURE_CONFIG="spatial_seed = $seed
spatial_decisions = 600
spatial_variant = $variant
spatial_arm = $spatial
spatial_protocol = spatial-8c-navigation"
  if ! sh "$runner" "$world" off "$seed" "$bridge_port" "$server_port"; then
    if [ -f "$world/$EPISODIC_CHOICE_RESULT" ]; then
      echo "integrity failure in $world (result written, replay failed)" >&2
      echo "$variant-seed-$seed-$spatial" >>"$output/integrity-failures.txt"
    else
      echo "infrastructure failure in $world; rerunning once" >&2
      echo "$variant-seed-$seed-$spatial" >>"$output/infrastructure-reruns.txt"
      mv "$world" "$world.failed"
      sh "$runner" "$world" off "$seed" "$bridge_port" "$server_port" || {
        echo "second infrastructure failure in $world; 8C is incomplete" >&2
        exit 1
      }
    fi
  fi
  # Keep the trace and result; the map database is large and not needed.
  rm -rf "$world/map.sqlite" "$world/home" "$world.failed"
}

for seed in $open_seeds; do
  for spatial in off on scrambled; do run_arm open "$seed" "$spatial"; done
done
for seed in $wall_seeds; do
  for spatial in off on; do run_arm wall "$seed" "$spatial"; done
done

python3 "$output/frozen/analyze_spatial_8c.py" "$repo" "$output" \
  "$(echo $open_seeds | tr ' ' ',')" "$(echo $wall_seeds | tr ' ' ',')"
