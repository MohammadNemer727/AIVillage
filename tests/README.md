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
