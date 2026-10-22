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
# runs on the host. All logs stay in the world folder, with per-founder client, bridge, state,
# and trace files.
#
# Usage: body-test.sh [world-folder]   (BODY_TEST_PORT, BODY_TEST_BRIDGE_PORT pick free ports)
# BODY_TEST_FOUNDER_COUNT=4 starts founder-00..founder-03; founder-00 runs the ability fixture.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
world=${1:-"$root/.body-test-world"}
port=${BODY_TEST_PORT:-30060}
bridge_port=${BODY_TEST_BRIDGE_PORT:-8495}
founder_count=${BODY_TEST_FOUNDER_COUNT:-1}
case "$founder_count" in *[!0-9]*|'') echo "BODY_TEST_FOUNDER_COUNT must be a positive integer" >&2; exit 2;; esac
[ "$founder_count" -ge 1 ] || { echo "BODY_TEST_FOUNDER_COUNT must be at least 1" >&2; exit 2; }
if [ "$founder_count" -eq 1 ]; then test_founder=founder; else test_founder=founder-00; fi
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
ai_bridge_founder_count = $founder_count
ai_bridge_body_test_founder = $test_founder
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
server_pid=
client_pids=
bridge_pids=
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
  kill $client_pids $server_pid $bridge_pids 2>/dev/null || true
  wait $client_pids $server_pid $bridge_pids 2>/dev/null || true
}
trap cleanup EXIT INT TERM

fail() {
  echo "body test FAILED: $1" >&2
  if [ "$founder_count" -eq 1 ]; then logs="server.log founder-client.log bridge.log"
  else logs="server.log $test_founder-client.log bridge.log"; fi
  for log in $logs; do
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
    for pid in $bridge_pids $server_pid $client_pids; do
      kill -0 "$pid" 2>/dev/null || fail "a process stopped while waiting for $what"
    done
    sleep 0.5
  done
  fail "timed out waiting for $what"
}

start_bridge() {
  bridge_pids=
  if [ "$founder_count" -eq 1 ]; then
    SIM_LUANTI_PORT="$bridge_port" SIM_LUANTI_AGENT_ID=1 \
      SIM_LUANTI_FOUNDER_COUNT=1 SIM_LUANTI_POPULATION_CAPACITY=1 \
      SIM_LUANTI_MODE=affordance SIM_LUANTI_TRACE="$world/bridge.ndjson" \
      SIM_LUANTI_STATE="$world/founder-brain.json" \
      "$bridge" >>"$world/bridge.log" 2>&1 &
  else
    SIM_LUANTI_PORT="$bridge_port" SIM_LUANTI_AGENT_ID=1 \
      SIM_LUANTI_FOUNDER_COUNT="$founder_count" SIM_LUANTI_POPULATION_CAPACITY="$founder_count" \
      SIM_LUANTI_MODE=affordance SIM_LUANTI_TRACE="$world/bridge.ndjson" \
      SIM_LUANTI_STALL_SEQUENCE=5 SIM_LUANTI_STALL_MS=750 \
      SIM_LUANTI_POPULATION="$world/population" \
      "$bridge" >>"$world/bridge.log" 2>&1 &
  fi
  bridge_pids=$!
  wait_for 60 'sim-luanti listening' "$world/bridge.log" "the population bridge to start"
}

suffix=$$
# Recorded here, not in luanti_run: it runs in the background, so its variables stay there.
[ -n "$image" ] && containers="body-test-server-$suffix"
start_bridge
luanti_run "body-test-server-$suffix" luantiserver --gameid mineclone2 --world "$world" \
  --config "$world/minetest.conf" >"$world/server.log" 2>&1 &
server_pid=$!
wait_for "$server_wait" 'Server for gameid' "$world/server.log" "the server to start"
slot=0
while [ "$slot" -lt "$founder_count" ]; do
  if [ "$founder_count" -eq 1 ]; then
    label=founder; client_log="$world/founder-client.log"
  else
    label=$(printf 'founder-%02d' "$slot"); client_log="$world/$label-client.log"
  fi
  container_name="body-test-$label-$suffix"
  [ -n "$image" ] && containers="$containers $container_name"
  SDL_VIDEODRIVER=offscreen luanti_run "$container_name" luanti \
    --config "$world/founder-client.conf" --go --address 127.0.0.1 --port "$port" --name "$label" \
    >"$client_log" 2>&1 &
  client_pids="$client_pids $!"
  slot=$((slot + 1))
done
wait_for "$join_wait" "AI_BRIDGE_FOUNDER_PLAYER joined name=$test_founder " "$world/server.log" "the test founder to join"
wait_for "$test_wait" 'BODYTEST DONE' "$world/server.log" "the body abilities to finish"
# Let the brain save what it has learned, then stop everything.
sleep 5
cleanup
containers=
server_pid=
client_pids=
grep -E 'BODYTEST (PASS|FAIL)' "$world/server.log" | sed 's/.*BODYTEST /  /'

# A restarted brain continues the saved founder.
: >"$world/bridge.log"
start_bridge
# "listening" is emitted before the checkpoint is loaded. Wait for the later load confirmation so
# killing a fast restart cannot race the assertion below.
if [ "$founder_count" -eq 1 ]; then
  restart_text='continuing the saved founder'
else
  restart_text="continuing $founder_count independently saved individuals"
fi
wait_for 60 "$restart_text" "$world/bridge.log" "the saved brain state to resume"
kill $bridge_pids
wait $bridge_pids 2>/dev/null || true
bridge_pids=

python3 - "$world" "$founder_count" <<'PY' || fail "see the checks above"
import json, pathlib, re, sys
world = pathlib.Path(sys.argv[1])
founder_count = int(sys.argv[2])
log = (world / "server.log").read_text(errors="replace")
failures = []
if "BODYTEST DONE" not in log:
    failures.append("the body test did not finish")
if re.search(r"ERROR\[Main\].*(Runtime error|ServerError)", log):
    failures.append("the server reported a Lua error")
failures += [f"body ability failed: {name}" for name in re.findall(r"BODYTEST FAIL (.+)", log)]
felt = []
founder = "founder" if founder_count == 1 else "founder-00"
lines = (world / "bridge.ndjson").read_text().splitlines()
all_records = []
for number, line in enumerate(lines):
    try:
        record = json.loads(line)
    except json.JSONDecodeError:
        # Stopping the bridge may cut off the line it was writing; nothing else may be broken.
        if number == len(lines) - 1:
            continue
        raise
    all_records.append(record)
    observation = record["request"]["observation"]
    if observation.get("interoception"):
        felt.append(observation["interoception"])
for slot in range(founder_count):
    label = "founder" if founder_count == 1 else f"founder-{slot:02d}"
    records = [
        record for record in all_records
        if record["request"]["agent_id"] == slot + 1
    ]
    if not records:
        failures.append(f"{label} produced no transitions")
        continue
    agent_id = slot + 1
    if any(record["request"]["agent_id"] != agent_id for record in records):
        failures.append(f"{label} did not consistently use agent_id {agent_id}")
    if any(record["request"].get("body_slot") != slot for record in records):
        failures.append(f"{label} did not consistently use body_slot {slot}")
    if any(record["response"]["agent_id"] != agent_id
           or record["response"]["body_slot"] != slot for record in records):
        failures.append(f"{label} received a response for another identity or body slot")
    sequences = [record["request"]["observation"]["sequence"] for record in records]
    if sequences != list(range(len(sequences))):
        failures.append(f"{label} sequence was not an independent contiguous stream")
    if any(record["request"].get("protocol") != 5 or record["response"].get("protocol") != 5
           for record in records):
        failures.append(f"{label} did not use bridge protocol 5")
    if any(record["request"]["observation"]["capabilities"].get("body_contract_version") != 7
           for record in records):
        failures.append(f"{label} did not use player body contract 7")
test_records = [
    record for record in all_records
    if record["request"]["agent_id"] == 1
]
held = [
    record["request"]["observation"].get("held_item")
    for record in test_records
    if record["request"]["observation"].get("held_item") is not None
]
allowed_held_keys = {
    "occupied", "red", "green", "blue", "amount", "capacity", "durability"
}
if not held:
    failures.append("Rust received no held-item observations")
elif any(set(item) != allowed_held_keys or any(isinstance(value, str) for value in item.values())
         for item in held):
    failures.append("held-item observations contained a name, label, or unexpected field")
elif len({tuple(item[key] for key in sorted(allowed_held_keys)) for item in held}) < 3:
    failures.append("held-item observations did not change with inventory selection")
action_fields = [
    ("thrust", 256), ("thrust", -256), ("turn", 256), ("turn", -256),
    ("strafe", 256), ("strafe", -256), ("look_pitch", 256), ("look_pitch", -256),
    ("jump", 256), ("hand_pulse", 256), ("jaw", 256), ("acoustic", 256),
    ("use_pulse", 256), ("next_slot", 256),
]
applied = [record["request"]["trace"]["applied_action"] for record in test_records]
for field, value in action_fields:
    if not any(action.get(field, 0) == value for action in applied):
        failures.append(f"the {field}={value} primitive did not traverse the live actuator path")
if founder_count > 1:
    timed_out = {
        record["request"]["agent_id"]
        for record in all_records
        if record["request"]["observation"]["transport"] == "Timeout"
    }
    if len(timed_out) != 1:
        failures.append(
            "the injected slow request did not remain isolated to exactly one body: "
            + repr(sorted(timed_out))
        )
if not felt:
    failures.append("the brain received no felt body state")
else:
    if not any(f["pain"] > 0 for f in felt):
        failures.append("the brain never felt pain")
    if not any(f["fullness"] <= 80 for f in felt):
        failures.append("the brain never felt hunger 6 of 20 (fullness 77)")
    if not any(f["health"] < 255 for f in felt):
        failures.append("the brain never felt lost health")
target_log = world / "bridge.log"
resume_text = "continuing the saved founder" if founder_count == 1 else \
    f"continuing {founder_count} independently saved individuals"
if resume_text not in target_log.read_text():
    failures.append("a restarted bridge did not continue its saved population state")
if failures:
    print("body test checks failed:\n  " + "\n  ".join(failures))
    sys.exit(1)
print(f"body test passed: every ability worked, {len(felt)} felt observations, saved brain resumed")
PY
