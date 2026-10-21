#!/usr/bin/env sh
# Plays the Minecraft-like world: VoxeLibre (a faithful Minecraft clone for Luanti) in survival,
# on a local server shared with the founder. The founder is a real player too: his own
# windowless game client connects as "founder", and his brain (the Rust bridge) drives that
# player. Type /founder in chat to observe through his eyes, and /founder again to return to
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
rm -f "$world/worldmods/ai_bridge"
if [ "$founder" = 1 ]; then
  mkdir -p "$world/worldmods"
  ln -s "$root/games/ai_village/mods/ai_bridge" "$world/worldmods/ai_bridge"
  cat >>"$world/minetest.conf" <<EOF
secure.http_mods = ai_bridge
ai_bridge_world = open
ai_bridge_play = true
ai_bridge_mode = observe
ai_bridge_observer = $name
ai_bridge_founder = founder
ai_bridge_endpoint = http://127.0.0.1:$bridge_port/v1/transition
ai_bridge_step_seconds = 0.25
max_forceloaded_blocks = 64
EOF
fi

server_bin=$(command -v luantiserver || command -v minetestserver)
client_bin=$(command -v luanti || command -v minetest)
server_log="$world/server.log"
bridge_pid=
if [ "$founder" = 1 ]; then
  bridge_log="$world/bridge.log"
  cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
  SIM_LUANTI_PORT=$bridge_port SIM_LUANTI_MODE=affordance \
    "$repo/target/debug/sim-luanti" >"$bridge_log" 2>&1 &
  bridge_pid=$!
  for _ in $(seq 1 240); do
    if grep -q 'sim-luanti listening' "$bridge_log"; then break; fi
    if ! kill -0 "$bridge_pid" 2>/dev/null; then cat "$bridge_log" >&2; exit 1; fi
    sleep 0.5
  done
fi
LUANTI_GAME_PATH="$games" "$server_bin" --gameid mineclone2 --world "$world" \
  --config "$world/minetest.conf" >"$server_log" 2>&1 &
server_pid=$!
cleanup() {
  kill $founder_client_pid "$server_pid" $bridge_pid 2>/dev/null || true
  wait $founder_client_pid "$server_pid" $bridge_pid 2>/dev/null || true
}
trap cleanup EXIT INT TERM
ready=0
for _ in $(seq 1 120); do
  if grep -q 'Server for gameid' "$server_log"; then ready=1; break; fi
  if ! kill -0 "$server_pid" 2>/dev/null; then cat "$server_log" >&2; exit 1; fi
  sleep 0.5
done
[ "$ready" -eq 1 ] || { cat "$server_log" >&2; exit 1; }
founder_client_pid=
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
  SDL_VIDEODRIVER=offscreen "$client_bin" --config "$world/founder-client.conf" --go \
    --address 127.0.0.1 --port "$port" --name founder >"$world/founder-client.log" 2>&1 &
  founder_client_pid=$!
fi
printf '%s\n' "VoxeLibre world: $world (server 127.0.0.1:$port, player $name)"
"$client_bin" --go --address 127.0.0.1 --port "$port" --name "$name"
