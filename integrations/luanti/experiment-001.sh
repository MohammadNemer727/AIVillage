#!/usr/bin/env sh
# Unattended real VoxeLibre Experiment 001 cohort.
# Usage: experiment-001.sh enabled|disabled [world-folder] [report.json]
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
cohort=${1:-enabled}
case "$cohort" in enabled|disabled) ;; *) echo "cohort must be enabled or disabled" >&2; exit 2;; esac
world=${2:-"$root/.experiment-001-$cohort"}
report=${3:-"$world/experiment-001-report.json"}
mkdir -p "$world"

export PLAY_FOUNDER_COUNT=${EXPERIMENT_001_FOUNDERS:-4}
export PLAY_POPULATION_CAPACITY=${EXPERIMENT_001_CAPACITY:-$PLAY_FOUNDER_COUNT}
export PLAY_UNATTENDED=1
# Affordance mode emits only its fixed actuator experiments, whose blank signals are always zero.
# Use the predictive policy so the preregistered positive first-signal reproduction decision exists.
export SIM_LUANTI_MODE=baseline
export SIM_LUANTI_MUTATION=$cohort
export SIM_LUANTI_REPRODUCTIVE_MATURITY=${EXPERIMENT_001_MATURITY:-100}
export SIM_LUANTI_MINIMUM_REPRODUCTIVE_RESERVE=${EXPERIMENT_001_MINIMUM_RESERVE:-160}
export SIM_LUANTI_REPRODUCTIVE_COST=${EXPERIMENT_001_REPRODUCTIVE_COST:-80}
export SIM_LUANTI_EXPERIMENT_001_BIRTHS=${EXPERIMENT_001_BIRTHS:-100}
export SIM_LUANTI_EXPERIMENT_001_DEADLINE_SECONDS=${EXPERIMENT_001_DEADLINE_SECONDS:-86400}
export SIM_LUANTI_EXPERIMENT_001_REPORT=$report
export SIM_LUANTI_TRACE=${EXPERIMENT_001_TRACE:-"$world/bridge-trace.ndjson"}
export SIM_LUANTI_RUN_PROVENANCE=${EXPERIMENT_001_PROVENANCE:-"$world/run-provenance.json"}

if [ ! -e "$SIM_LUANTI_RUN_PROVENANCE" ]; then
  revision=$(git -C "$repo" rev-parse HEAD)
  source_hash=$(git -C "$repo" ls-files -co --exclude-standard -z |
    python3 -c 'import hashlib, os, sys
root = sys.argv[1]
h = hashlib.sha256()
for raw in sorted(sys.stdin.buffer.read().split(b"\0")):
    if not raw or raw.startswith(b"integrations/luanti/experiment-001-runs/"):
        continue
    path = os.path.join(root, os.fsdecode(raw))
    if not os.path.isfile(path):
        continue
    h.update(raw + b"\0")
    with open(path, "rb") as source:
        for chunk in iter(lambda: source.read(1048576), b""):
            h.update(chunk)
print(h.hexdigest())' "$repo")
  umask 077
  (set -C; cat >"$SIM_LUANTI_RUN_PROVENANCE" <<EOF
{"schema_version":1,"experiment":"001-short-natural-pilot","created_utc":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","git_revision":"$revision","tracked_diff_sha256":"$source_hash","cohort":"$cohort","founders":$PLAY_FOUNDER_COUNT,"capacity":$PLAY_POPULATION_CAPACITY,"mode":"baseline","maturity_decisions":$SIM_LUANTI_REPRODUCTIVE_MATURITY,"minimum_reproductive_reserve":$SIM_LUANTI_MINIMUM_REPRODUCTIVE_RESERVE,"reproductive_cost":$SIM_LUANTI_REPRODUCTIVE_COST,"birth_target":$SIM_LUANTI_EXPERIMENT_001_BIRTHS,"deadline_seconds":$SIM_LUANTI_EXPERIMENT_001_DEADLINE_SECONDS,"forced_signal":false}
EOF
  )
fi

exec "$root/play.sh" "$world"
