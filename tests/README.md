# End-to-end tests and fixtures

Cross-crate deterministic replay scenarios and frozen V1 golden hashes live here. Unit tests remain
next to the simulation primitives they exercise.

## V1 golden replay

File: [`fixtures/v1_golden_replay.toml`](fixtures/v1_golden_replay.toml)

Each case pins `seed`, world geometry, and expected `state_hash` values at selected ticks under
`sim-format` schema version 1 and `sim_core::STATE_HASH_VERSION`.

Reproduce / verify with the headless CLI:

```sh
cargo run -p sim-cli --locked -- golden
cargo run -p sim-cli --locked -- golden-print   # regenerate hash lines after intentional hash-schema bumps
```

The same fixture is also asserted by `crates/sim-core/tests/v1_golden_replay.rs`.

## One-agent runtime golden trace

File: [`fixtures/agent_runtime_golden.ndjson`](fixtures/agent_runtime_golden.ndjson)

One `EnvironmentTransition` per line: a seeded `FounderSpec` driven by the deterministic
`FakeEnvironment`, with every observation, primitive action, and post-decision brain fingerprint.
`crates/sim-format/tests/agent_runtime_golden.rs` asserts that a live run reproduces the file byte
for byte, that replaying the recorded observations reproduces every decision, and that resuming from
a TOML `AgentCheckpoint` midway reproduces the rest.

Regenerate only after an intentional behaviour change:

```sh
UPDATE_AGENT_GOLDEN=1 cargo test -p sim-format --test agent_runtime_golden
```

## Luanti headless bridge

The original minimal game and loopback-only Lua adapter live in
[`../integrations/luanti`](../integrations/luanti). Its generated colour lookup must be current:

```sh
python3 integrations/luanti/tools/generate_palette.py --check
```

See [`../integrations/luanti/FEDORA_PACKAGES.md`](../integrations/luanti/FEDORA_PACKAGES.md) for
the Fedora package/version lock and user-terminal-only install commands. The smoke script starts
the bridge itself, runs the server, asserts the live behaviour, and replays the recorded trace:

```sh
sh integrations/luanti/ci/headless-smoke.sh                 # host luantiserver
docker build -t ai-village-luanti integrations/luanti        # or the pinned image, as CI does
LUANTI_SERVER_IMAGE=ai-village-luanti sh integrations/luanti/ci/headless-smoke.sh
```

With rootless Podman, build with `podman` and set `LUANTI_CONTAINER_RUNTIME=podman`. Set
`SIM_LUANTI_TRACE=trace.ndjson` when running `cargo run -p sim-luanti` by hand to record accepted
post-action transitions, and replay them with `cargo run -p sim-luanti -- replay trace.ndjson`;
live Luanti physics itself is not claimed to be bit-identical.
Acceleration is intentionally a bounded signed receptor: horizontal finite differences are scaled
at 4 m/s² per full-scale unit and clamped to `[-255, 255]`. This can saturate during collisions or
gravity-adjacent motion; changing the fixed receptor mapping requires a versioned V2 contract
decision rather than a bridge-local adjustment.

## Luanti graphical observer

Run the three local processes against one isolated world with:

```sh
sh integrations/luanti/observe.sh
```

The script starts `sim-luanti`, `luantiserver` (falling back to `minetestserver` on Fedora),
and the `luanti` graphical client as the `observer` player at `127.0.0.1:30000`. It uses
`LUANTI_GAME_PATH`, starts in `observe` mode, and writes logs under
`integrations/luanti/.observe-world`. The observer begins inside the room, aimed at the founder, and
has normal movement, jumping, gravity, and mouse look for exploration; it remains non-privileged, so
it cannot interact with the world. Server-side ceiling fixtures illuminate the room even on a fresh
singlenode world. The original CC0 blocky humanoid founder has an amber head and hands, green
torso/arms, dark legs, and dark face cues; it is labelled
`FOUNDER — Rust-controlled humanoid`. Its applied motor values and Rust decision tick appear in
the observer HUD. Stop the graphical client to cleanly
stop the local server and bridge.

For a rendering and actuator check when the live founder happens to be still, change
`ai_bridge_mode = observe` to `ai_bridge_mode = demo-motion` in the generated world config, then
restart the script. This mode deliberately replaces the Rust action with a deterministic Lua
actuator fixture after each successful Rust bridge response; it demonstrates rendering and physics,
not learned Rust motor output. `smoke` is reserved for the automated harness; only that mode
enables the injected stall and timed server shutdown.

The observer-only HUD labels the decision tick, server step, held command (including strafe,
pitch, jump, and hand pulse), grounded state, generic hand contact, request latency and
transport status; it also separates receptors used by the current V2 compression (green and
brightness means plus interoception), live-but-unused channels (red, blue, direction), and dead
channels (chemicals, vibration). Rust returns these values only in a top-level `diagnostics`
response object: Lua never includes them in a later observation, and detailed failures remain in
the Rust/server logs.

Manual graphical checklist:

1. In `observe`, confirm the labelled humanoid is visible from the initial framing: it is
   approximately 1.85 room nodes tall, with readable head, face cues, torso, arms/hands, and
   legs. The nearby grey interaction cube is visibly smaller (0.4 nodes) and begins to the
   founder's right. The HUD decision tick and `Rust action applied` fields update.
2. Move and jump: the observer explores normally, while the founder remains a separately visible
   Rust-bridge-controlled entity.
3. In `demo-motion`, confirm the founder turns/moves with its face leading and its legs/arms
   swing from movement. The HUD separates baseline from intrinsic action.
4. In `smoke`, the deterministic fixture sends one generic hand pulse into the sole nearby grey
   physical cube; verify the hand-contact indication. This does not identify the object to Rust
   or enable inventory, pickup, use, mining, placement, crafting, rewards, or steering.
5. Stop `sim-luanti` and confirm the founder halts within 0.5 seconds.

If the untrained founder holds a forward action into a wall, the configured `observer` may enter
`/founder_reset`. This is explicitly an observer-only scene reset: it returns the body to the
arena centre and restores its initial heading, while preserving the held Rust action, runtime,
sequence, observations, and learning state. The HUD counts these resets and labels them as not
steering or retraining; the founder can therefore drive back into a wall immediately afterward.

The headless smoke test additionally asserts applied-action/pose consistency and this
bridge-stop safe-zero behavior. Creature artwork provenance and license are in
[`../integrations/luanti/games/ai_village/mods/ai_bridge/ASSETS.md`](../integrations/luanti/games/ai_village/mods/ai_bridge/ASSETS.md).

## Luanti learning run

The infant mobile paradigm in the real game, with the affordance founder: baseline without the
test cube, a connected phase with it in front of the founder, then extinction without it. In the
`inert` control the cube is present but a hand pulse neither moves it nor reports contact. Each
run replays its trace through the runtime; Luanti physics itself is not bit-exact.

```sh
SIM_LUANTI_SEED=1 LEARN_OBJECT=free sh integrations/luanti/ci/learning-run.sh /tmp/learn/free-seed1
SIM_LUANTI_SEED=1 LEARN_OBJECT=inert sh integrations/luanti/ci/learning-run.sh /tmp/learn/inert-seed1
python3 integrations/luanti/tools/analyze_learning.py docs/luanti-learning-run.md /tmp/learn/*
```

In learning runs the cube hangs at the founder's eye height. `LEARN_FACING=turned` starts each
trial facing one of eight directions instead of the cube (the reaching test), and
`SIM_LUANTI_AFFORDANCE='{"context":"SenseBits"}' LEARN_LABEL=context` runs the context-conditioned
learner. The synthetic counterpart is `cargo run -p sim-luanti -- reaching`
([`../docs/reaching-test.md`](../docs/reaching-test.md)).

Set distinct `SIM_LUANTI_PORT` and `LUANTI_SMOKE_PORT` values to run several at once. Current
results are in [`../docs/luanti-learning-run.md`](../docs/luanti-learning-run.md).
