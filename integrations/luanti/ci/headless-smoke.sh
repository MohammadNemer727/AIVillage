#!/usr/bin/env sh
# Runs the loopback bridge and a clean, isolated Luanti server: the host's `luantiserver` by
# default, or the image named by LUANTI_SERVER_IMAGE (built from integrations/luanti/Dockerfile)
# through LUANTI_CONTAINER_RUNTIME (default: docker).
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
python3 "$root/tools/generate_palette.py" --check
bridge_port=${SIM_LUANTI_PORT:-8481}
server_port=${LUANTI_SMOKE_PORT:-30000}
world=${1:-"$root/.smoke-world"}
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
cat >>"$config" <<'EOF'
ai_bridge_mode = smoke
EOF
printf 'ai_bridge_endpoint = http://127.0.0.1:%s/v1/transition\nport = %s\n' "$bridge_port" "$server_port" >>"$config"

trace="$world/bridge.ndjson"
bridge_log="$world/bridge.log"
server_log="$world/server.log"
SIM_LUANTI_TRACE="$trace" SIM_LUANTI_STALL_SEQUENCE=12 SIM_LUANTI_STALL_MS=750 \
  cargo run --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml" >"$bridge_log" 2>&1 &
bridge_pid=$!
container=ai-village-smoke-$$
cleanup() {
  if [ -n "${LUANTI_SERVER_IMAGE:-}" ]; then
    "${LUANTI_CONTAINER_RUNTIME:-docker}" rm -f "$container" >/dev/null 2>&1 || true
  fi
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
# Terminate the bridge after the extended maximum-movement fixture. Lua must replace the held command with zero
# within its request timeout; this is distinct from the bridge's injected delayed reply.
( sleep 16; kill "$bridge_pid" 2>/dev/null || true ) &
if [ -n "${LUANTI_SERVER_IMAGE:-}" ]; then
  runtime=${LUANTI_CONTAINER_RUNTIME:-docker}
  # Host networking reaches the loopback-only bridge; the world is mounted at the same path and
  # written as the invoking user. Rootless podman needs keep-id for that user mapping.
  userns=
  [ "$runtime" = podman ] && userns=--userns=keep-id
  timeout 30s "$runtime" run --rm --name "$container" --network host $userns \
    --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$world:$world:Z" \
    "$LUANTI_SERVER_IMAGE" --world "$world" --config "$config" >"$server_log" 2>&1
else
  server_bin=$(command -v luantiserver || command -v minetestserver)
  LUANTI_GAME_PATH="$root/games" timeout 30s "$server_bin" --gameid ai_village --world "$world" \
    --config "$config" >"$server_log" 2>&1
fi
python3 - "$trace" "$server_log" <<'PY'
import json, math, pathlib, sys
records = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
events = [
    json.loads(line.split("AI_BRIDGE_SMOKE ", 1)[1])
    for line in pathlib.Path(sys.argv[2]).read_text().splitlines()
    if "AI_BRIDGE_SMOKE " in line
]
assert records, "bridge accepted no post-action observations"
for index, record in enumerate(records):
    request, response = record["request"], record["response"]
    assert request["observation"]["sequence"] == index
    assert response["generated_sequence"] == index
    assert response["expected_sequence"] == index + 1
    assert request["observation"]["transport"] in {"Healthy", "Timeout", "Rejected", "Disconnected"}
    for sample in request["observation"]["senses"]["retinal"]:
        assert all(0 <= sample[key] <= 255 for key in ("red", "green", "blue", "brightness", "occlusion"))
        assert -255 <= sample["motion"] <= 255
    assert -255 <= request["observation"]["senses"]["acceleration"] <= 255
assert any(
    sample["occlusion"] > 0 and (sample["red"], sample["green"], sample["blue"]) != (120, 170, 255)
    for record in records for sample in record["request"]["observation"]["senses"]["retinal"]
), "no ray hit a visible wall"
room = next(event for event in events if event["event"] == "room")
assert room["floor"] == "ai_bridge:floor" and room["wall"] == "ai_bridge:wall_x", \
    f"unexpected generated room nodes: {room}"
assert room["floor_light"] >= 10 and room["wall_light"] >= 10, \
    f"room surfaces were not lit after fix_light: {room}"
ray0 = [event["ray0"] for event in events if event["event"] == "observation"]
assert ray0[0]["occlusion"] > 0 and tuple(ray0[0][key] for key in ("red", "green", "blue")) == (190, 82, 58)
assert any(tuple(ray[key] for key in ("red", "green", "blue")) == (56, 150, 104) for ray in ray0), \
    "positive turn did not move ray 0 from +X wall toward +Z wall"
assert any(record["request"]["observation"]["senses"]["touch"] > 0 for record in records), \
    "forced horizontal wall collision produced no touch"
assert any(record["request"]["observation"]["body"]["hand_contact"] > 0 for record in records), \
    "scripted generic hand pulse produced no bounded hand-contact feedback"
assert any(event["event"] == "hand_contact" for event in events), \
    "generic physical target received no hand impulse"
transports = [record["request"]["observation"]["transport"] for record in records]
timeout = transports.index("Timeout")
assert "Healthy" in transports[timeout + 1:], "bridge did not recover after its injected stall"
assert any(event == {"event": "safe_zero", "status": "timeout", "thrust": 0, "turn": 0} for event in events), \
    "timeout did not install the explicit safe zero action"
spawn = next(event for event in events if event["event"] == "spawn")
gravity = next(event for event in events if event["event"] == "gravity")
assert (spawn["x"], spawn["y"], spawn["z"]) == (0, 0.5, 0), f"unexpected spawn: {spawn}"
assert math.isclose(spawn["yaw"], -math.pi / 2, abs_tol=0.01), f"unexpected spawn yaw: {spawn}"
assert gravity["vy"] <= 0, f"unexpected upward startup velocity: {gravity}"
generic = next(event for event in events if event["event"] == "generic_spawn")
assert generic == {"event": "generic_spawn", "x": 1.2, "y": 0.7, "z": 0,
                   "visual_size": 0.4, "collision_height": 0.4}, \
    f"unexpected generic fixture placement or scale: {generic}"
overrides = [event for event in events if event["event"] == "override_action"]
assert any(abs(event["thrust"]) == 255 and event["jump"] == 255 for event in overrides), \
    "fixture never applied maximum forward/jump action"
assert any(abs(event["strafe"]) == 255 and event["jump"] == 255 for event in overrides), \
    "fixture never applied maximum strafe/jump action"
animations = [event for event in events if event["event"] == "animation"]
assert any(event["name"] == "walk" and (event["thrust"] or event["strafe"]) for event in animations), \
    "locomotion never selected the native walk animation"
assert all(event["hand_pulse"] > 0 and not (event["thrust"] or event["strafe"])
           for event in animations if event["name"] == "hand"), \
    "hand animation occurred without an isolated nonzero hand pulse"
assert not any(event["event"] == "safety_reset" for event in events), \
    "physical containment failed and required a safety reset"
def heading(pose):
    return pose["yaw_milliradian"] / 1000 + math.pi / 2
traces = [record["request"]["trace"] for record in records]
assert len(traces) >= 140, f"extended fixture ended too early: {len(traces)} transitions"
assert all(abs(trace["pose"]["x_millinode"]) <= 3700
           and abs(trace["pose"]["z_millinode"]) <= 3700
           and 500 <= trace["pose"]["y_millinode"] <= 2300 for trace in traces), \
    "founder escaped the physical inner arena or fell outside its floor/roof"
turns = [(a, b) for a, b in zip(traces, traces[1:])
         if a["applied_action"]["turn"] > 0 and b["applied_action"]["turn"] > 0]
assert turns and any(heading(b["pose"]) > heading(a["pose"]) for a, b in turns), \
    "positive applied turn did not increase recorded V2 heading"
drives = [(a, b) for a, b in zip(traces, traces[1:])
          if a["applied_action"]["thrust"] > 0 and b["applied_action"]["thrust"] > 0]
assert drives and any(
    math.hypot(b["pose"]["x_millinode"] - a["pose"]["x_millinode"],
               b["pose"]["z_millinode"] - a["pose"]["z_millinode"]) > 0
    or record["request"]["observation"]["senses"]["touch"] > 0
    for (a, b), record in zip(drives, records[1:])
), "positive applied thrust neither moved the recorded body nor reported touch"
assert any(event == {"event": "safe_zero", "status": "disconnected", "thrust": 0, "turn": 0} for event in events), \
    "bridge stop did not install the explicit zero action"
print(f"smoke accepted {len(records)} bounded transitions with pose evidence and bridge-stop safety")
PY
# The live trace must reproduce every decision and brain fingerprint in the Rust runtime.
cargo run --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml" -- replay "$trace"
