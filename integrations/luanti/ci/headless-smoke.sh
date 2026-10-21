#!/usr/bin/env sh
# Runs the loopback bridge and a clean, isolated Fedora Luanti server.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo=$(CDPATH= cd -- "$root/../.." && pwd)
python3 "$root/tools/generate_palette.py" --check
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
ai_bridge_smoke_shutdown = true
ai_bridge_smoke = true
EOF

trace="$world/bridge.ndjson"
bridge_log="$world/bridge.log"
server_log="$world/server.log"
SIM_LUANTI_TRACE="$trace" SIM_LUANTI_STALL_SEQUENCE=12 SIM_LUANTI_STALL_MS=750 \
  cargo run --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml" >"$bridge_log" 2>&1 &
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
server_bin=$(command -v luantiserver || command -v minetestserver)
LUANTI_GAME_PATH="$root/games" timeout 20s "$server_bin" --gameid ai_village --world "$world" --config "$config" >"$server_log" 2>&1
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
ray0 = [event["ray0"] for event in events if event["event"] == "observation"]
assert ray0[0]["occlusion"] > 0 and tuple(ray0[0][key] for key in ("red", "green", "blue")) == (190, 82, 58)
assert any(tuple(ray[key] for key in ("red", "green", "blue")) == (56, 150, 104) for ray in ray0), \
    "positive turn did not move ray 0 from +X wall toward +Z wall"
assert any(record["request"]["observation"]["senses"]["touch"] > 0 for record in records), \
    "forced horizontal wall collision produced no touch"
transports = [record["request"]["observation"]["transport"] for record in records]
timeout = transports.index("Timeout")
assert "Healthy" in transports[timeout + 1:], "bridge did not recover after its injected stall"
assert any(event == {"event": "safe_zero", "status": "timeout", "thrust": 0, "turn": 0} for event in events), \
    "timeout did not install the explicit safe zero action"
spawn = next(event for event in events if event["event"] == "spawn")
gravity = next(event for event in events if event["event"] == "gravity")
assert (spawn["x"], spawn["y"], spawn["z"]) == (0, 1, 0), f"unexpected spawn: {spawn}"
assert math.isclose(spawn["yaw"], -math.pi / 2, abs_tol=0.01), f"unexpected spawn yaw: {spawn}"
assert gravity["vy"] < 0, f"gravity was not applied: {gravity}"
print(f"smoke accepted {len(records)} bounded transitions with rays, collision, timeout recovery, spawn, and gravity")
PY
# The live trace must reproduce every decision and brain fingerprint in the Rust runtime.
cargo run --quiet -p sim-luanti --manifest-path "$repo/Cargo.toml" -- replay "$trace"
