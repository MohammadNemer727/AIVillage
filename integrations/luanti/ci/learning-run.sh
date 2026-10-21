#!/usr/bin/env sh
# Runs one real Luanti learning run of the affordance founder in the infant mobile paradigm:
# baseline without the test object, a connected phase with it, then extinction without it.
#
#   sh integrations/luanti/ci/learning-run.sh WORLD_DIR
#
# Environment:
#   SIM_LUANTI_SEED          founder seed (default 1)
#   LEARN_OBJECT             free (pulses push it and report contact) or inert (default free)
#   LEARN_PHASES             baseline,connected,extinction decisions (default 200,400,200)
#   LEARN_FACING             toward (each trial starts facing the cube) or turned (each trial
#                            starts facing one of eight directions in turn; default toward)
#   SIM_LUANTI_PORT          bridge port (default 8481)
#   LUANTI_SMOKE_PORT        server port (default 30000)
#   SIM_LUANTI_AFFORDANCE    optional learner settings JSON
#   LEARN_LABEL              name for those settings in the analysis (default "default")
#
# The run writes bridge.ndjson (every transition) and server.log (phase and trial events) into
# WORLD_DIR, then replays the trace through the runtime to confirm the Rust side exactly.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
world=${1:?usage: learning-run.sh WORLD_DIR}
bridge_port=${SIM_LUANTI_PORT:-8481}
server_port=${LUANTI_SMOKE_PORT:-30000}
object=${LEARN_OBJECT:-free}
phases=${LEARN_PHASES:-200,400,200}
facing=${LEARN_FACING:-toward}
case "$object" in free|inert) ;; *) echo "LEARN_OBJECT must be free or inert" >&2; exit 2 ;; esac

rm -rf "$world"
mkdir -p "$world"
cat >"$world/world.mt" <<'EOF'
gameid = ai_village
backend = sqlite3
seed = 424242
time_speed = 0
EOF
config="$world/minetest.conf"
cat "$root/ci/minetest.conf" >"$config"
printf 'ai_bridge_mode = learn\nai_bridge_learn_object = %s\nai_bridge_learn_phases = %s\nai_bridge_learn_facing = %s\n' \
  "$object" "$phases" "$facing" >>"$config"
printf 'ai_bridge_endpoint = http://127.0.0.1:%s/v1/transition\nport = %s\n' "$bridge_port" "$server_port" >>"$config"

learner=${LEARN_LABEL:-default}
printf '{"seed": %s, "object": "%s", "phases": "%s", "learner": "%s", "facing": "%s"}\n' \
  "${SIM_LUANTI_SEED:-1}" "$object" "$phases" "$learner" "$facing" >"$world/run.json"
trace="$world/bridge.ndjson"
bridge_log="$world/bridge.log"
server_log="$world/server.log"
cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
bridge="$repo/target/debug/sim-luanti"
SIM_LUANTI_MODE=affordance SIM_LUANTI_TRACE="$trace" "$bridge" >"$bridge_log" 2>&1 &
bridge_pid=$!
cleanup() {
  kill "$bridge_pid" 2>/dev/null || true
  wait "$bridge_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM
ready=0
for _ in $(seq 1 120); do
  if grep -q 'sim-luanti listening' "$bridge_log"; then ready=1; break; fi
  if ! kill -0 "$bridge_pid" 2>/dev/null; then cat "$bridge_log"; exit 1; fi
  sleep 0.25
done
[ "$ready" -eq 1 ] || { cat "$bridge_log"; exit 1; }

# The server shuts itself down once every phase is complete. Allow a generous margin over the
# nominal 0.1 s per decision.
decisions=$(printf '%s' "$phases" | tr ',' '\n' | awk '{ total += $1 } END { print total }')
limit=$((decisions / 4 + 60))
server_bin=$(command -v luantiserver || command -v minetestserver)
LUANTI_GAME_PATH="$root/games" timeout "${limit}s" "$server_bin" --gameid ai_village --world "$world" \
  --config "$config" >"$server_log" 2>&1 || true
grep -q '"phase":"done"' "$server_log" || { echo "learning run did not complete: $world" >&2; exit 1; }
cleanup
trap - EXIT INT TERM
"$bridge" replay "$trace" >"$world/replay.log" 2>&1 || { cat "$world/replay.log"; exit 1; }
echo "learning run complete: $world ($object, phases $phases, seed ${SIM_LUANTI_SEED:-1})"
