#!/usr/bin/env sh
# Plays the Minecraft-like world: VoxeLibre (a faithful Minecraft clone for Luanti) in survival,
# on a local server shared with the founder. The founder is a real player too: his own
# windowless game client connects as "founder", and his brain (the Rust bridge) drives that
# player. Set PLAY_FOUNDER_COUNT=4 for founder-00..founder-03 under one population coordinator.
# Type /founder in chat to observe through his eyes, and /founder again to return to
# yourself; /founder here brings him to you. The world is kept
# between sessions; delete the world folder to start over. Quitting the game stops everything.
# PLAY_FOUNDER=0 plays without the founder.
#
# Usage: play.sh [world-folder]   (PLAY_NAME sets your player name, PLAY_SEED a new world's seed)
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
world=${1:-"$root/.play-world"}
name=${PLAY_NAME:-player}
port=${PLAY_PORT:-30001}
bridge_port=${PLAY_BRIDGE_PORT:-8482}
founder=${PLAY_FOUNDER:-1}
founder_count=${PLAY_FOUNDER_COUNT:-1}
population_capacity=${PLAY_POPULATION_CAPACITY:-$founder_count}
unattended=${PLAY_UNATTENDED:-0}
case "$founder_count" in *[!0-9]*|'') echo "PLAY_FOUNDER_COUNT must be a positive integer" >&2; exit 2;; esac
case "$population_capacity" in *[!0-9]*|'') echo "PLAY_POPULATION_CAPACITY must be a positive integer" >&2; exit 2;; esac
[ "$founder_count" -ge 1 ] || { echo "PLAY_FOUNDER_COUNT must be at least 1" >&2; exit 2; }
[ "$population_capacity" -ge "$founder_count" ] || {
  echo "PLAY_POPULATION_CAPACITY must contain every founder" >&2; exit 2
}
games=${LUANTI_GAMES:-"$HOME/.minetest/games"}
[ -d "$games/mineclone2" ] || {
  echo "VoxeLibre not found in $games; install it from ContentDB (Wuzzy/mineclone2)" >&2
  exit 1
}

if [ ! -f "$world/world.mt" ]; then
  mkdir -p "$world"
  cat >"$world/world.mt" <<'EOF'
gameid = mineclone2
backend = sqlite3
player_backend = sqlite3
auth_backend = sqlite3
mod_storage_backend = sqlite3
EOF
fi
cat >"$world/minetest.conf" <<EOF
mg_name = v7
fixed_map_seed = ${PLAY_SEED:-424242}
port = $port
bind_address = 127.0.0.1
enable_damage = true
creative_mode = false
default_privs = interact, shout, home
server_announce = false
EOF
# The founder's mod joins the world as a world mod; its brain runs in the Rust bridge.
# Protocol drivers pin both to snapshots with PLAY_AI_BRIDGE_MOD and PLAY_BRIDGE_BIN.
rm -f "$world/worldmods/ai_bridge"
if [ "$founder" = 1 ]; then
  mkdir -p "$world/worldmods"
  ln -s "${PLAY_AI_BRIDGE_MOD:-$root/games/ai_village/mods/ai_bridge}" "$world/worldmods/ai_bridge"
  cat >>"$world/minetest.conf" <<EOF
secure.http_mods = ai_bridge
ai_bridge_world = open
ai_bridge_play = true
ai_bridge_mode = observe
ai_bridge_observer = $name
ai_bridge_founder = founder
ai_bridge_founder_count = $founder_count
ai_bridge_population_capacity = $population_capacity
ai_bridge_endpoint = http://127.0.0.1:$bridge_port/v1/transition
ai_bridge_step_seconds = 0.25
max_forceloaded_blocks = 64
EOF
fi

server_bin=$(command -v luantiserver || command -v minetestserver)
client_bin=$(command -v luanti || command -v minetest)
server_log="$world/server.log"
bridge_pids=
founder_client_pids=
if [ "$founder" = 1 ]; then
  bridge_bin=${PLAY_BRIDGE_BIN:-}
  if [ -z "$bridge_bin" ]; then
    cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
    bridge_bin="$repo/target/debug/sim-luanti"
  fi
  bridge_log="$world/bridge.log"
  : >"$bridge_log"
  if [ "$founder_count" -eq 1 ]; then
    SIM_LUANTI_PORT="$bridge_port" SIM_LUANTI_AGENT_ID=1 \
      SIM_LUANTI_MODE="${SIM_LUANTI_MODE:-affordance}" \
      SIM_LUANTI_STATE="$world/founder-brain.json" \
      "$bridge_bin" >"$bridge_log" 2>&1 &
  else
    SIM_LUANTI_PORT="$bridge_port" SIM_LUANTI_AGENT_ID=1 \
      SIM_LUANTI_MODE="${SIM_LUANTI_MODE:-affordance}" \
      SIM_LUANTI_FOUNDER_COUNT="$founder_count" \
      SIM_LUANTI_POPULATION_CAPACITY="$population_capacity" \
      SIM_LUANTI_POPULATION="$world/population" \
      "$bridge_bin" >"$bridge_log" 2>&1 &
  fi
  bridge_pids=$!
  ready=0
  for _ in $(seq 1 240); do
    kill -0 "$bridge_pids" 2>/dev/null || { echo "population bridge stopped" >&2; exit 1; }
    grep -q 'sim-luanti listening' "$bridge_log" && { ready=1; break; }
    sleep 0.5
  done
  [ "$ready" -eq 1 ] || { echo "population bridge did not become ready" >&2; exit 1; }
fi
LUANTI_GAME_PATH="$games" "$server_bin" --gameid mineclone2 --world "$world" \
  --config "$world/minetest.conf" >"$server_log" 2>&1 &
server_pid=$!
cleanup() {
  kill $founder_client_pids "$server_pid" $bridge_pids 2>/dev/null || true
  wait $founder_client_pids "$server_pid" $bridge_pids 2>/dev/null || true
}
trap cleanup EXIT INT TERM
ready=0
for _ in $(seq 1 120); do
  if grep -q 'Server for gameid' "$server_log"; then ready=1; break; fi
  if ! kill -0 "$server_pid" 2>/dev/null; then cat "$server_log" >&2; exit 1; fi
  sleep 0.5
done
[ "$ready" -eq 1 ] || { cat "$server_log" >&2; exit 1; }
if [ "$founder" = 1 ]; then
  # The founder's own client: a real player with no window, drawing offscreen at a tiny size.
  cat >"$world/founder-client.conf" <<EOF
video_driver = opengl
enable_sound = false
screen_w = 64
screen_h = 64
fps_max = 10
fps_max_unfocused = 10
viewing_range = 32
EOF
  slot=0
  while [ "$slot" -lt "$population_capacity" ]; do
    if [ "$population_capacity" -eq 1 ]; then
      label=founder; client_log="$world/founder-client.log"
    else
      label=$(printf 'founder-%02d' "$slot"); client_log="$world/$label-client.log"
    fi
    SDL_VIDEODRIVER=offscreen "$client_bin" --config "$world/founder-client.conf" --go \
      --address 127.0.0.1 --port "$port" --name "$label" >"$client_log" 2>&1 &
    founder_client_pids="$founder_client_pids $!"
    slot=$((slot + 1))
  done
fi
if [ "$unattended" = 1 ]; then
  echo "Experiment 001 running unattended; waiting for birth target, extinction, or deadline"
  wait "$bridge_pids"
  exit $?
fi
printf '%s\n' "VoxeLibre world: $world (server 127.0.0.1:$port, player $name)"
"$client_bin" --go --address 127.0.0.1 --port "$port" --name "$name"
