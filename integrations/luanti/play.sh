#!/usr/bin/env sh
# Plays the Minecraft-like world: VoxeLibre (a faithful Minecraft clone for Luanti) in survival,
# on a local server so others (and later the founder) can share it. The world is kept between
# sessions; delete the world folder to start over. Ctrl-C or quitting the game stops the server.
#
# Usage: play.sh [world-folder]   (PLAY_NAME sets your player name, PLAY_SEED a new world's seed)
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
world=${1:-"$root/.play-world"}
name=${PLAY_NAME:-player}
port=${PLAY_PORT:-30001}
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

server_bin=$(command -v luantiserver || command -v minetestserver)
client_bin=$(command -v luanti || command -v minetest)
server_log="$world/server.log"
LUANTI_GAME_PATH="$games" "$server_bin" --gameid mineclone2 --world "$world" \
  --config "$world/minetest.conf" >"$server_log" 2>&1 &
server_pid=$!
cleanup() {
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM
ready=0
for _ in $(seq 1 120); do
  if grep -q 'Server for gameid' "$server_log"; then ready=1; break; fi
  if ! kill -0 "$server_pid" 2>/dev/null; then cat "$server_log" >&2; exit 1; fi
  sleep 0.5
done
[ "$ready" -eq 1 ] || { cat "$server_log" >&2; exit 1; }
printf '%s\n' "VoxeLibre world: $world (server 127.0.0.1:$port, player $name)"
"$client_bin" --go --address 127.0.0.1 --port "$port" --name "$name"
