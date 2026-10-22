#!/usr/bin/env sh
# Headless check of the founder's player body in VoxeLibre: the server, the founder's brain (the
# Rust bridge) and the founder's own windowless client run for real, and body_test/ drives the
# founder through digging, picking up, placing, opening a gate, changing the held item and
# eating. The checks below then confirm that its brain felt hunger and pain, and that a restarted
# brain continues from its saved state. Needs VoxeLibre and OpenGL; exits 77 (skipped) without
# VoxeLibre.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
world=${1:-"$root/.body-test-world"}
port=${BODY_TEST_PORT:-30060}
bridge_port=${BODY_TEST_BRIDGE_PORT:-8495}
games=${LUANTI_GAMES:-"$HOME/.minetest/games"}
[ -d "$games/mineclone2" ] || { echo "VoxeLibre not found in $games; body test skipped" >&2; exit 77; }

rm -rf "$world"
mkdir -p "$world/worldmods"
printf 'gameid = mineclone2\nbackend = sqlite3\n' >"$world/world.mt"
ln -s "$root/games/ai_village/mods/ai_bridge" "$world/worldmods/ai_bridge"
ln -s "$root/ci/body_test" "$world/worldmods/ai_bridge_body_test"
cat >"$world/minetest.conf" <<EOF
mg_name = flat
port = $port
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
ai_bridge_endpoint = http://127.0.0.1:$bridge_port/v1/transition
ai_bridge_step_seconds = 0.1
ai_bridge_test_hooks = true
max_forceloaded_blocks = 64
EOF
cat >"$world/founder-client.conf" <<'EOF'
video_driver = opengl
enable_sound = false
screen_w = 64
screen_h = 64
fps_max = 10
fps_max_unfocused = 10
viewing_range = 32
EOF

cargo build --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml"
bridge="$repo/target/debug/sim-luanti"
state="$world/founder-brain.json"
start_bridge() {
  SIM_LUANTI_PORT=$bridge_port SIM_LUANTI_MODE=affordance SIM_LUANTI_STATE="$state" \
    SIM_LUANTI_TRACE="$world/bridge.ndjson" "$bridge" >>"$world/bridge.log" 2>&1 &
  bridge_pid=$!
  for _ in $(seq 1 60); do
    grep -q 'sim-luanti listening' "$world/bridge.log" && return 0
    sleep 0.25
  done
  cat "$world/bridge.log" >&2
  exit 1
}
server_pid=
client_pid=
cleanup() {
  kill $client_pid $server_pid $bridge_pid 2>/dev/null || true
  wait $client_pid $server_pid $bridge_pid 2>/dev/null || true
}
trap cleanup EXIT INT TERM

start_bridge
LUANTI_GAME_PATH="$games" luantiserver --gameid mineclone2 --world "$world" \
  --config "$world/minetest.conf" >"$world/server.log" 2>&1 &
server_pid=$!
for _ in $(seq 1 120); do
  grep -q 'Server for gameid' "$world/server.log" && break
  sleep 0.5
done
SDL_VIDEODRIVER=offscreen luanti --config "$world/founder-client.conf" --go \
  --address 127.0.0.1 --port "$port" --name founder >"$world/founder-client.log" 2>&1 &
client_pid=$!
for _ in $(seq 1 240); do
  grep -q 'BODYTEST DONE' "$world/server.log" && break
  if ! kill -0 "$server_pid" 2>/dev/null; then break; fi
  sleep 0.5
done
# Let the brain save what it has learned, then stop everything.
sleep 5
cleanup
grep -E 'BODYTEST (PASS|FAIL)' "$world/server.log" | sed 's/.*BODYTEST /  /'

# A restarted brain continues the saved founder.
: >"$world/bridge.log"
start_bridge
sleep 1
kill "$bridge_pid"
wait "$bridge_pid" 2>/dev/null || true

python3 - "$world" <<'PY'
import json, pathlib, re, sys
world = pathlib.Path(sys.argv[1])
log = (world / "server.log").read_text(errors="replace")
failures = []
if "BODYTEST DONE" not in log:
    failures.append("the body test did not finish")
if re.search(r"ERROR\[Main\].*(Runtime error|ServerError)", log):
    failures.append("the server reported a Lua error")
failures += [f"body ability failed: {name}" for name in re.findall(r"BODYTEST FAIL (.+)", log)]
felt = []
lines = (world / "bridge.ndjson").read_text().splitlines()
for number, line in enumerate(lines):
    try:
        observation = json.loads(line)["request"]["observation"]
    except json.JSONDecodeError:
        # Stopping the bridge may cut off the line it was writing; nothing else may be broken.
        if number == len(lines) - 1:
            continue
        raise
    if observation.get("interoception"):
        felt.append(observation["interoception"])
if not felt:
    failures.append("the brain received no felt body state")
else:
    if not any(f["pain"] > 0 for f in felt):
        failures.append("the brain never felt pain")
    if not any(f["fullness"] <= 80 for f in felt):
        failures.append("the brain never felt hunger 6 of 20 (fullness 77)")
    if not any(f["health"] < 255 for f in felt):
        failures.append("the brain never felt lost health")
if "continuing the saved founder" not in (world / "bridge.log").read_text():
    failures.append("a restarted brain did not continue the saved founder")
if failures:
    print("body test FAILED:\n  " + "\n  ".join(failures))
    sys.exit(1)
print(f"body test passed: every ability worked, {len(felt)} felt observations, saved brain resumed")
PY
