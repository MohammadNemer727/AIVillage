#!/usr/bin/env sh
# Headless check of the founder's player body in VoxeLibre: the server, the founder's brain (the
# Rust bridge) and the founder's own windowless client run for real, and body_test/ drives the
# founder through digging, picking up, placing, opening a gate, changing the held item and
# eating. The checks below then confirm that its brain felt hunger and pain, and that a restarted
# brain continues from its saved state.
#
# By default the host's Luanti and VoxeLibre are used (exit 77, skipped, without VoxeLibre). With
# BODY_TEST_IMAGE set (built from Dockerfile.body-test) the server and the founder's client run
# from that pinned image through BODY_TEST_RUNTIME (default: docker), as in CI. The brain always
# runs on the host. All logs stay in the world folder: server.log, founder-client.log,
# bridge.log and the bridge trace bridge.ndjson.
#
# Usage: body-test.sh [world-folder]   (BODY_TEST_PORT, BODY_TEST_BRIDGE_PORT pick free ports)
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
world=${1:-"$root/.body-test-world"}
port=${BODY_TEST_PORT:-30060}
bridge_port=${BODY_TEST_BRIDGE_PORT:-8495}
image=${BODY_TEST_IMAGE:-}
runtime=${BODY_TEST_RUNTIME:-docker}
# Bounds, in half seconds: server start, founder joining, and the abilities themselves.
server_wait=180
join_wait=240
test_wait=360

if [ -z "$image" ]; then
  games=${LUANTI_GAMES:-"$HOME/.minetest/games"}
  [ -d "$games/mineclone2" ] || { echo "VoxeLibre not found in $games; body test skipped" >&2; exit 77; }
fi

rm -rf "$world"
mkdir -p "$world/worldmods" "$world/home"
world=$(CDPATH= cd -- "$world" && pwd)
printf 'gameid = mineclone2\nbackend = sqlite3\n' >"$world/world.mt"
# Copies, not links, so a container needs only the world folder.
cp -R "$root/games/ai_village/mods/ai_bridge" "$world/worldmods/ai_bridge"
cp -R "$root/ci/body_test" "$world/worldmods/ai_bridge_body_test"
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
# Fixed, tiny, software-friendly graphics for the founder's client.
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
state="$world/founder-brain.json"
server_pid=
client_pid=
bridge_pid=
containers=

# Runs a Luanti program (server or client) on the host or in the pinned image. The container
# shares the host's loopback network, writes the world as the invoking user, and keeps its home
# inside the world folder.
luanti_run() {
  name=$1
  shift
  # exec: the program replaces this background shell, so stopping its process stops the program.
  if [ -z "$image" ]; then
    HOME="$world/home" LUANTI_GAME_PATH="$games" exec "$@"
  fi
  userns=
  [ "$runtime" = podman ] && userns=--userns=keep-id
  exec "$runtime" run --rm --name "$name" --network host $userns --user "$(id -u):$(id -g)" \
    -e HOME="$world/home" -e LUANTI_GAME_PATH=/opt/luanti/games -v "$world:$world:z" \
    "$image" "$@"
}

cleanup() {
  for container in $containers; do "$runtime" rm -f "$container" >/dev/null 2>&1 || true; done
  kill $client_pid $server_pid $bridge_pid 2>/dev/null || true
  wait $client_pid $server_pid $bridge_pid 2>/dev/null || true
}
trap cleanup EXIT INT TERM

fail() {
  echo "body test FAILED: $1" >&2
  for log in server.log founder-client.log bridge.log; do
    [ -f "$world/$log" ] || continue
    echo "--- last lines of $log ---" >&2
    tail -n 40 "$world/$log" >&2
  done
  echo "(all logs are in $world)" >&2
  exit 1
}

# Waits up to $1 half seconds for $2 to appear in $3, failing early if a process has died.
wait_for() {
  limit=$1 text=$2 log=$3 what=$4
  for _ in $(seq 1 "$limit"); do
    grep -q "$text" "$log" 2>/dev/null && return 0
    for pid in $bridge_pid $server_pid $client_pid; do
      kill -0 "$pid" 2>/dev/null || fail "a process stopped while waiting for $what"
    done
    sleep 0.5
  done
  fail "timed out waiting for $what"
}

start_bridge() {
  SIM_LUANTI_PORT=$bridge_port SIM_LUANTI_MODE=affordance SIM_LUANTI_STATE="$state" \
    SIM_LUANTI_TRACE="$world/bridge.ndjson" "$bridge" >>"$world/bridge.log" 2>&1 &
  bridge_pid=$!
  wait_for 60 'sim-luanti listening' "$world/bridge.log" "the founder's brain to start"
}

suffix=$$
# Recorded here, not in luanti_run: it runs in the background, so its variables stay there.
[ -n "$image" ] && containers="body-test-server-$suffix body-test-founder-$suffix"
start_bridge
luanti_run "body-test-server-$suffix" luantiserver --gameid mineclone2 --world "$world" \
  --config "$world/minetest.conf" >"$world/server.log" 2>&1 &
server_pid=$!
wait_for "$server_wait" 'Server for gameid' "$world/server.log" "the server to start"
SDL_VIDEODRIVER=offscreen luanti_run "body-test-founder-$suffix" luanti \
  --config "$world/founder-client.conf" --go --address 127.0.0.1 --port "$port" --name founder \
  >"$world/founder-client.log" 2>&1 &
client_pid=$!
wait_for "$join_wait" 'AI_BRIDGE_FOUNDER_PLAYER joined' "$world/server.log" "the founder to join"
wait_for "$test_wait" 'BODYTEST DONE' "$world/server.log" "the body abilities to finish"
# Let the brain save what it has learned, then stop everything.
sleep 5
cleanup
containers=
server_pid=
client_pid=
grep -E 'BODYTEST (PASS|FAIL)' "$world/server.log" | sed 's/.*BODYTEST /  /'

# A restarted brain continues the saved founder.
: >"$world/bridge.log"
start_bridge
kill "$bridge_pid"
wait "$bridge_pid" 2>/dev/null || true
bridge_pid=

python3 - "$world" <<'PY' || fail "see the checks above"
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
    print("body test checks failed:\n  " + "\n  ".join(failures))
    sys.exit(1)
print(f"body test passed: every ability worked, {len(felt)} felt observations, saved brain resumed")
PY
