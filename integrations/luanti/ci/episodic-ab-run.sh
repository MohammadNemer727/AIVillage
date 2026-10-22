#!/usr/bin/env sh
# One untouched real VoxeLibre arm of episodic-policy-live-ab-v1.
# Usage: episodic-ab-run.sh WORLD ARM SEED BRIDGE_PORT SERVER_PORT
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
world=${1:?world required}
arm=${2:?off or policy required}
seed=${3:?seed required}
bridge_port=${4:?bridge port required}
server_port=${5:?server port required}
case "$arm" in off|policy) ;; *) echo "arm must be off or policy" >&2; exit 2;; esac

games=${LUANTI_GAMES:-"$HOME/.minetest/games"}
[ -d "$games/mineclone2" ] || { echo "VoxeLibre not found in $games" >&2; exit 77; }
rm -rf "$world"
mkdir -p "$world/worldmods" "$world/home"
cp -R "$root/games/ai_village/mods/ai_bridge" "$world/worldmods/ai_bridge"
cp -R "$root/ci/episodic_ab" "$world/worldmods/ai_bridge_episodic_ab"
cat >"$world/world.mt" <<EOF
gameid = mineclone2
backend = sqlite3
seed = 424242
EOF
cat >"$world/minetest.conf" <<EOF
mg_name = flat
fixed_map_seed = 424242
port = $server_port
bind_address = 127.0.0.1
enable_damage = true
creative_mode = false
default_privs = interact, shout
mobs_spawn = false
dedicated_server_step = 0.05
secure.http_mods = ai_bridge
ai_bridge_world = open
ai_bridge_play = true
ai_bridge_mode = observe
ai_bridge_observer = nobody
ai_bridge_founder = founder
ai_bridge_founder_count = 1
ai_bridge_endpoint = http://127.0.0.1:$bridge_port/v1/transition
ai_bridge_step_seconds = 0.1
ai_bridge_test_hooks = true
episodic_ab_arm = $arm
episodic_ab_seed = $seed
episodic_ab_trials = 16
EOF
cat >"$world/founder-client.conf" <<'EOF'
video_driver = opengl
enable_sound = false
screen_w = 64
screen_h = 64
fps_max = 10
fps_max_unfocused = 10
viewing_range = 32
enable_shaders = false
enable_minimap = false
EOF

cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
bridge="$repo/target/debug/sim-luanti"
bridge_pid=
server_pid=
client_pid=
cleanup() {
  kill $client_pid $server_pid $bridge_pid 2>/dev/null || true
  wait $client_pid $server_pid $bridge_pid 2>/dev/null || true
}
trap cleanup EXIT INT TERM

SIM_LUANTI_SEED="$seed" SIM_LUANTI_AGENT_ID=1 SIM_LUANTI_PORT="$bridge_port" \
  SIM_LUANTI_FOUNDER_COUNT=1 SIM_LUANTI_POPULATION_CAPACITY=1 \
  SIM_LUANTI_REPRODUCTIVE_MATURITY=1000000 \
  SIM_LUANTI_MODE=baseline SIM_LUANTI_EPISODIC="$arm" \
  SIM_LUANTI_TRACE="$world/bridge.ndjson" SIM_LUANTI_STATE="$world/founder-brain.json" \
  "$bridge" >"$world/bridge.log" 2>&1 &
bridge_pid=$!
for _ in $(seq 1 120); do
  grep -q 'sim-luanti listening' "$world/bridge.log" 2>/dev/null && break
  kill -0 "$bridge_pid" 2>/dev/null || { cat "$world/bridge.log"; exit 1; }
  sleep 0.25
done
grep -q 'sim-luanti listening' "$world/bridge.log"

HOME="$world/home" LUANTI_GAME_PATH="$games" luantiserver --gameid mineclone2 --world "$world" \
  --config "$world/minetest.conf" >"$world/server.log" 2>&1 &
server_pid=$!
for _ in $(seq 1 360); do
  grep -q 'Server for gameid' "$world/server.log" 2>/dev/null && break
  kill -0 "$server_pid" 2>/dev/null || { cat "$world/server.log"; exit 1; }
  sleep 0.5
done
grep -q 'Server for gameid' "$world/server.log"

HOME="$world/home" LUANTI_GAME_PATH="$games" SDL_VIDEODRIVER=offscreen \
  luanti --config "$world/founder-client.conf" --go --address 127.0.0.1 \
  --port "$server_port" --name founder >"$world/founder-client.log" 2>&1 &
client_pid=$!
for _ in $(seq 1 240); do
  [ -f "$world/episodic-ab-result.json" ] && break
  kill -0 "$bridge_pid" 2>/dev/null && kill -0 "$server_pid" 2>/dev/null \
    && kill -0 "$client_pid" 2>/dev/null || {
      cat "$world/server.log"; cat "$world/bridge.log"; exit 1;
    }
  sleep 0.5
done
[ -f "$world/episodic-ab-result.json" ] || {
  echo "episodic A/B arm timed out" >&2; exit 1;
}
cleanup
trap - EXIT INT TERM
SIM_LUANTI_SEED="$seed" SIM_LUANTI_AGENT_ID=1 \
  "$bridge" replay "$world/bridge.ndjson" >"$world/replay.log" 2>&1
